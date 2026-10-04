// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

#[cfg(unix)]
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use bulls_application::ports::{
    GitRepositoryLayout, GitRepositoryProbePort, PortResult, RepositoryCandidate,
};

use crate::error::{invalid_data, io_port_error};
use crate::git_process::GitProcessRunner;
use crate::process::ProcessExecutionPolicy;

const DEFAULT_GIT_PROCESS_TIMEOUT: Duration = Duration::from_secs(10);
const GIT_STDOUT_LIMIT: usize = 256 * 1024;
const GIT_STDERR_LIMIT: usize = 256 * 1024;

#[derive(Clone)]
pub struct GitCliRepositoryProbe {
    git: GitProcessRunner,
}

impl GitCliRepositoryProbe {
    pub fn new() -> Self {
        Self::with_timeout(DEFAULT_GIT_PROCESS_TIMEOUT)
    }

    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            git: GitProcessRunner::new(git_process_policy(timeout)),
        }
    }

    pub fn with_cancellation(cancellation: Arc<dyn Fn() -> bool + Send + Sync>) -> Self {
        Self::with_timeout_and_cancellation(DEFAULT_GIT_PROCESS_TIMEOUT, cancellation)
    }

    pub fn with_timeout_and_cancellation(
        timeout: Duration,
        cancellation: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Self {
        Self {
            git: GitProcessRunner::with_cancellation(git_process_policy(timeout), cancellation),
        }
    }

    fn git_boolean(&self, repository: &Path, argument: &str) -> PortResult<bool> {
        let output = self.run_rev_parse(repository, argument)?;
        match output.as_slice() {
            b"true" => Ok(true),
            b"false" => Ok(false),
            _ => Err(invalid_data()),
        }
    }

    fn git_path(&self, repository: &Path, argument: &str) -> PortResult<PathBuf> {
        let output = self.run_rev_parse(repository, argument)?;
        if output.is_empty() {
            return Err(invalid_data());
        }

        let path = path_from_git_output(output)?;
        let normalized = fs::canonicalize(path).map_err(|error| io_port_error(&error))?;
        if normalized.is_absolute() {
            Ok(normalized)
        } else {
            Err(invalid_data())
        }
    }

    fn run_rev_parse(&self, repository: &Path, argument: &str) -> PortResult<Vec<u8>> {
        let output = self.git.run(
            repository,
            ["rev-parse", "--path-format=absolute", argument],
        )?;

        Ok(remove_output_terminator(output))
    }
}

fn git_process_policy(timeout: Duration) -> ProcessExecutionPolicy {
    ProcessExecutionPolicy::new(timeout, GIT_STDOUT_LIMIT, GIT_STDERR_LIMIT)
}

impl Default for GitCliRepositoryProbe {
    fn default() -> Self {
        Self::new()
    }
}

impl GitRepositoryProbePort for GitCliRepositoryProbe {
    fn probe(&self, candidate: &RepositoryCandidate) -> PortResult<GitRepositoryLayout> {
        let is_bare = self.git_boolean(candidate.path(), "--is-bare-repository")?;
        let git_dir = self.git_path(candidate.path(), "--git-dir")?;
        let common_dir = self.git_path(candidate.path(), "--git-common-dir")?;

        if is_bare {
            Ok(GitRepositoryLayout::bare(git_dir, common_dir))
        } else {
            let worktree_path = self.git_path(candidate.path(), "--show-toplevel")?;
            Ok(GitRepositoryLayout::worktree(
                worktree_path,
                git_dir,
                common_dir,
            ))
        }
    }
}

fn remove_output_terminator(mut output: Vec<u8>) -> Vec<u8> {
    if output.last() == Some(&b'\n') {
        output.pop();
        if output.last() == Some(&b'\r') {
            output.pop();
        }
    }
    output
}

#[cfg(unix)]
fn path_from_git_output(output: Vec<u8>) -> PortResult<PathBuf> {
    use std::os::unix::ffi::OsStringExt;

    Ok(PathBuf::from(OsString::from_vec(output)))
}

#[cfg(not(unix))]
fn path_from_git_output(output: Vec<u8>) -> PortResult<PathBuf> {
    let path = String::from_utf8(output).map_err(|_| invalid_data())?;
    Ok(PathBuf::from(path))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

    use bulls_application::ports::{GitRepositoryProbePort, PortErrorKind, RepositoryCandidate};

    use super::GitCliRepositoryProbe;

    static TEST_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let sequence = TEST_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "bulls-git-topology-test-{}-{}",
                std::process::id(),
                sequence
            ));
            fs::create_dir_all(&path).expect("test directory must be created");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn git(path: &Path, arguments: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(path)
            .args(arguments)
            .env("GIT_TERMINAL_PROMPT", "0")
            .status()
            .expect("Git fixture command must start");
        assert!(status.success(), "Git fixture command must succeed");
    }

    #[test]
    fn probe_distinguishes_worktree_and_bare_layouts() {
        let root = TestDirectory::new();
        let worktree = root.path().join("normal repository");
        let bare = root.path().join("archive.git");
        fs::create_dir(&worktree).expect("worktree directory must be created");
        fs::create_dir(&bare).expect("bare directory must be created");
        git(&worktree, &["init", "-q"]);
        git(&bare, &["init", "-q", "--bare"]);

        let probe = GitCliRepositoryProbe::new();
        let worktree_layout = probe
            .probe(&RepositoryCandidate::new(&worktree))
            .expect("worktree probe must succeed");
        let bare_layout = probe
            .probe(&RepositoryCandidate::new(&bare))
            .expect("bare probe must succeed");

        let canonical_worktree =
            fs::canonicalize(&worktree).expect("worktree path must canonicalize");
        assert_eq!(
            worktree_layout.worktree_path(),
            Some(canonical_worktree.as_path())
        );
        assert!(!worktree_layout.is_bare());
        assert!(bare_layout.is_bare());
        assert_eq!(bare_layout.worktree_path(), None);
        let canonical_bare =
            fs::canonicalize(&bare).expect("bare repository path must canonicalize");
        assert_eq!(bare_layout.common_dir(), canonical_bare.as_path());
    }

    #[test]
    fn linked_worktree_probe_preserves_shared_common_directory() {
        let root = TestDirectory::new();
        let main = root.path().join("main");
        let linked = root.path().join("linked");
        fs::create_dir(&main).expect("main worktree directory must be created");
        git(&main, &["init", "-q"]);
        git(
            &main,
            &[
                "-c",
                "user.name=BullSaddle Test",
                "-c",
                "user.email=bulls@example.invalid",
                "commit",
                "--allow-empty",
                "-q",
                "-m",
                "initial",
            ],
        );
        git(
            &main,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "linked-test",
                linked.to_str().expect("test path must be UTF-8"),
            ],
        );

        let probe = GitCliRepositoryProbe::new();
        let main_layout = probe
            .probe(&RepositoryCandidate::new(&main))
            .expect("main worktree probe must succeed");
        let linked_layout = probe
            .probe(&RepositoryCandidate::new(&linked))
            .expect("linked worktree probe must succeed");

        assert_eq!(main_layout.common_dir(), linked_layout.common_dir());
        assert_ne!(main_layout.git_dir(), linked_layout.git_dir());
        assert_ne!(main_layout.worktree_path(), linked_layout.worktree_path());
    }

    #[test]
    fn probe_honors_cancellation_before_starting_git() {
        let root = TestDirectory::new();
        let repository = root.path().join("cancelled");
        fs::create_dir(&repository).expect("repository directory must be created");
        git(&repository, &["init", "-q"]);

        let cancelled = Arc::new(AtomicBool::new(true));
        let cancellation = Arc::clone(&cancelled);
        let probe = GitCliRepositoryProbe::with_cancellation(Arc::new(move || {
            cancellation.load(Ordering::Acquire)
        }));
        let error = probe
            .probe(&RepositoryCandidate::new(&repository))
            .expect_err("cancelled Git execution must not start");

        assert_eq!(error.kind(), PortErrorKind::Cancelled);
    }

    #[cfg(unix)]
    #[test]
    fn probe_preserves_non_utf8_repository_paths() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let root = TestDirectory::new();
        let name = OsString::from_vec(vec![b'r', b'e', b'p', b'o', b'-', 0xff]);
        let repository = root.path().join(name);
        fs::create_dir(&repository).expect("repository directory must be created");
        git(&repository, &["init", "-q"]);

        let layout = GitCliRepositoryProbe::new()
            .probe(&RepositoryCandidate::new(&repository))
            .expect("non-UTF-8 repository path must be preserved");

        let canonical_repository =
            fs::canonicalize(repository).expect("repository path must canonicalize");
        assert_eq!(layout.worktree_path(), Some(canonical_repository.as_path()));
    }
}
