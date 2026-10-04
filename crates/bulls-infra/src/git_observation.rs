// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use std::ffi::OsString;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use bulls_application::ports::{GitObservationPort, PortError, PortErrorKind, PortResult};
use bulls_core::{
    Branch, Remote, Upstream, UpstreamDivergence, UpstreamState, WorktreeChanges, WorktreeGitState,
};

use crate::error::invalid_data;
use crate::git_process::GitProcessRunner;
use crate::process::ProcessExecutionPolicy;

const DEFAULT_GIT_PROCESS_TIMEOUT: Duration = Duration::from_secs(10);
const GIT_STDOUT_LIMIT: usize = 4 * 1024 * 1024;
const GIT_STDERR_LIMIT: usize = 256 * 1024;

const UPSTREAM_FORMAT: &str = "%(upstream:remotename)%00%(upstream:remoteref)";

#[derive(Clone)]
pub struct GitCliObservation {
    git: GitProcessRunner,
}

impl GitCliObservation {
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

    fn upstream(&self, repository: &Path, branch: &Branch) -> PortResult<Upstream> {
        let reference = format!("refs/heads/{}", branch.name());
        let format_argument = format!("--format={UPSTREAM_FORMAT}");
        let output = preserve_subject_disappearance(
            repository,
            self.git.run(
                repository,
                [
                    OsString::from("for-each-ref"),
                    OsString::from("--count=1"),
                    OsString::from(format_argument),
                    OsString::from(reference),
                ],
            ),
        )?;

        parse_upstream(&output)
    }
}

fn git_process_policy(timeout: Duration) -> ProcessExecutionPolicy {
    ProcessExecutionPolicy::new(timeout, GIT_STDOUT_LIMIT, GIT_STDERR_LIMIT)
}

impl Default for GitCliObservation {
    fn default() -> Self {
        Self::new()
    }
}

impl GitObservationPort for GitCliObservation {
    fn observe_worktree(&self, worktree_path: &Path) -> PortResult<WorktreeGitState> {
        let output = preserve_subject_disappearance(
            worktree_path,
            self.git.run(
                worktree_path,
                [
                    "status",
                    "--porcelain=v2",
                    "--branch",
                    "-z",
                    "--untracked-files=normal",
                ],
            ),
        )?;
        let status = parse_status(&output)?;

        let changes = status.changes;
        match status.head {
            ParsedHead::Detached => {
                if status.upstream_configured || status.divergence.is_some() {
                    return Err(invalid_data());
                }
                Ok(WorktreeGitState::detached().with_changes(changes))
            }
            ParsedHead::Unborn(branch) => {
                let upstream = self.upstream_state(
                    worktree_path,
                    &branch,
                    status.upstream_configured,
                    status.divergence,
                )?;
                Ok(WorktreeGitState::unborn(branch, upstream).with_changes(changes))
            }
            ParsedHead::Branch(branch) => {
                let upstream = self.upstream_state(
                    worktree_path,
                    &branch,
                    status.upstream_configured,
                    status.divergence,
                )?;
                Ok(WorktreeGitState::branch(branch, upstream).with_changes(changes))
            }
        }
    }

    fn observe_repository_remotes(&self, repository_path: &Path) -> PortResult<Vec<Remote>> {
        let output = preserve_subject_disappearance(
            repository_path,
            self.git.run(repository_path, ["remote"]),
        )?;
        parse_remotes(&output)
    }
}

impl GitCliObservation {
    fn upstream_state(
        &self,
        repository: &Path,
        branch: &Branch,
        configured: bool,
        divergence: Option<UpstreamDivergence>,
    ) -> PortResult<UpstreamState> {
        if !configured {
            return if divergence.is_none() {
                Ok(UpstreamState::Unconfigured)
            } else {
                Err(invalid_data())
            };
        }

        let upstream = self.upstream(repository, branch)?;
        Ok(match divergence {
            Some(divergence) => UpstreamState::Tracking {
                upstream,
                divergence,
            },
            None => UpstreamState::Gone(upstream),
        })
    }
}

fn preserve_subject_disappearance(
    repository: &Path,
    result: PortResult<Vec<u8>>,
) -> PortResult<Vec<u8>> {
    match result {
        Err(error)
            if matches!(
                error.kind(),
                PortErrorKind::ProcessExited { .. } | PortErrorKind::IoFailure
            ) && matches!(repository.try_exists(), Ok(false)) =>
        {
            Err(PortError::new(PortErrorKind::ResourceUnavailable))
        }
        other => other,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ParsedHead {
    Unborn(Branch),
    Branch(Branch),
    Detached,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ParsedStatus {
    head: ParsedHead,
    upstream_configured: bool,
    divergence: Option<UpstreamDivergence>,
    changes: WorktreeChanges,
}

fn parse_status(output: &[u8]) -> PortResult<ParsedStatus> {
    let records: Vec<&[u8]> = output.split(|byte| *byte == 0).collect();
    let mut oid_initial = None;
    let mut head = None;
    let mut upstream_configured = false;
    let mut divergence = None;
    let mut staged = false;
    let mut unstaged = false;
    let mut untracked = false;
    let mut index = 0;

    while index < records.len() {
        let record = records[index];
        if record.is_empty() {
            index += 1;
            continue;
        }

        if let Some(value) = record.strip_prefix(b"# branch.oid ") {
            if oid_initial.replace(value == b"(initial)").is_some() || value.is_empty() {
                return Err(invalid_data());
            }
        } else if let Some(value) = record.strip_prefix(b"# branch.head ") {
            if head.is_some() || value.is_empty() {
                return Err(invalid_data());
            }
            head = Some(utf8(value)?.to_owned());
        } else if let Some(value) = record.strip_prefix(b"# branch.upstream ") {
            if upstream_configured || value.is_empty() {
                return Err(invalid_data());
            }
            utf8(value)?;
            upstream_configured = true;
        } else if let Some(value) = record.strip_prefix(b"# branch.ab ") {
            if divergence.is_some() {
                return Err(invalid_data());
            }
            divergence = Some(parse_divergence(value)?);
        } else if record.starts_with(b"1 ") || record.starts_with(b"u ") {
            update_changes(record, &mut staged, &mut unstaged)?;
        } else if record.starts_with(b"2 ") {
            update_changes(record, &mut staged, &mut unstaged)?;
            index += 1;
            if index >= records.len() || records[index].is_empty() {
                return Err(invalid_data());
            }
        } else if record.starts_with(b"? ") {
            untracked = true;
        } else if record.starts_with(b"! ") {
        } else {
            return Err(invalid_data());
        }

        index += 1;
    }

    let oid_initial = oid_initial.ok_or_else(invalid_data)?;
    let head = head.ok_or_else(invalid_data)?;
    let head = if head == "(detached)" {
        if oid_initial {
            return Err(invalid_data());
        }
        ParsedHead::Detached
    } else if oid_initial {
        ParsedHead::Unborn(Branch::new(head))
    } else {
        ParsedHead::Branch(Branch::new(head))
    };

    if divergence.is_some() && !upstream_configured {
        return Err(invalid_data());
    }

    Ok(ParsedStatus {
        head,
        upstream_configured,
        divergence,
        changes: WorktreeChanges::new(staged, unstaged, untracked),
    })
}

fn update_changes(record: &[u8], staged: &mut bool, unstaged: &mut bool) -> PortResult<()> {
    if record.len() < 4 || record[1] != b' ' {
        return Err(invalid_data());
    }

    *staged |= record[2] != b'.';
    *unstaged |= record[3] != b'.';
    Ok(())
}

fn parse_divergence(value: &[u8]) -> PortResult<UpstreamDivergence> {
    let value = utf8(value)?;
    let mut fields = value.split(' ');
    let ahead = fields.next().ok_or_else(invalid_data)?;
    let behind = fields.next().ok_or_else(invalid_data)?;
    if fields.next().is_some() {
        return Err(invalid_data());
    }

    let ahead = ahead
        .strip_prefix('+')
        .ok_or_else(invalid_data)?
        .parse::<u64>()
        .map_err(|_| invalid_data())?;
    let behind = behind
        .strip_prefix('-')
        .ok_or_else(invalid_data)?
        .parse::<u64>()
        .map_err(|_| invalid_data())?;

    Ok(UpstreamDivergence::new(ahead, behind))
}

fn parse_upstream(output: &[u8]) -> PortResult<Upstream> {
    let output = remove_output_terminator(output);
    let separator = output
        .iter()
        .position(|byte| *byte == 0)
        .ok_or_else(invalid_data)?;
    let (remote, branch) = output.split_at(separator);
    let branch = &branch[1..];
    if remote.is_empty() || branch.is_empty() || branch.contains(&0) {
        return Err(invalid_data());
    }

    let branch = utf8(branch)?
        .strip_prefix("refs/heads/")
        .ok_or_else(invalid_data)?;
    if branch.is_empty() {
        return Err(invalid_data());
    }

    Ok(Upstream::new(
        Remote::new(utf8(remote)?),
        Branch::new(branch),
    ))
}

fn parse_remotes(output: &[u8]) -> PortResult<Vec<Remote>> {
    let output = remove_output_terminator(output);
    if output.is_empty() {
        return Ok(Vec::new());
    }

    let mut remotes: Vec<Remote> = output
        .split(|byte| *byte == b'\n')
        .map(|name| {
            if name.is_empty() || name.contains(&0) {
                Err(invalid_data())
            } else {
                Ok(Remote::new(utf8(name)?))
            }
        })
        .collect::<PortResult<_>>()?;
    remotes.sort_by(|left, right| left.name().cmp(right.name()));
    if remotes
        .windows(2)
        .any(|pair| pair[0].name() == pair[1].name())
    {
        return Err(invalid_data());
    }

    Ok(remotes)
}

fn remove_output_terminator(output: &[u8]) -> &[u8] {
    let output = output.strip_suffix(b"\n").unwrap_or(output);
    output.strip_suffix(b"\r").unwrap_or(output)
}

fn utf8(value: &[u8]) -> PortResult<&str> {
    std::str::from_utf8(value).map_err(|_| invalid_data())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

    use bulls_application::ports::{GitObservationPort, PortErrorKind};
    use bulls_core::{Head, UpstreamState, WorktreeChanges};

    use super::{GitCliObservation, ParsedHead, parse_status, parse_upstream};

    static TEST_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let sequence = TEST_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "bulls-git-observation-test-{}-{}",
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

    fn commit(path: &Path, message: &str) {
        git(
            path,
            &[
                "-c",
                "user.name=BullSaddle Test",
                "-c",
                "user.email=bulls@example.invalid",
                "commit",
                "--allow-empty",
                "-q",
                "-m",
                message,
            ],
        );
    }

    #[test]
    fn status_parser_preserves_branch_divergence_and_change_presence() {
        let output = b"# branch.oid abc\0# branch.head main\0# branch.upstream origin/main\0# branch.ab +3 -2\0\
1 MM N... 100644 100644 100644 abc def path\0? untracked\0";

        let parsed = parse_status(output).expect("machine status must parse");

        assert_eq!(
            parsed.head,
            ParsedHead::Branch(bulls_core::Branch::new("main"))
        );
        assert!(parsed.upstream_configured);
        assert_eq!(
            parsed.divergence,
            Some(bulls_core::UpstreamDivergence::new(3, 2))
        );
        assert_eq!(parsed.changes, WorktreeChanges::new(true, true, true));
    }

    #[test]
    fn status_parser_accepts_rename_source_as_the_second_nul_record() {
        let output = b"# branch.oid abc\0# branch.head main\0\
2 R. N... 100644 100644 100644 abc def R100 new-name\0old-name\0";

        let parsed = parse_status(output).expect("rename status must parse");

        assert_eq!(parsed.changes, WorktreeChanges::new(true, false, false));
    }

    #[test]
    fn upstream_parser_preserves_remote_names_containing_slashes() {
        let upstream = parse_upstream(b"team/origin\0refs/heads/topic/work\n")
            .expect("upstream machine output must parse");

        assert_eq!(upstream.remote().name(), "team/origin");
        assert_eq!(upstream.branch().name(), "topic/work");
    }

    #[test]
    fn observes_unborn_and_local_change_state_without_collecting_paths() {
        let root = TestDirectory::new();
        let repository = root.path().join("repository");
        fs::create_dir(&repository).expect("repository directory must be created");
        git(&repository, &["init", "-q"]);
        fs::write(repository.join("tracked"), "staged").expect("test file must be written");
        git(&repository, &["add", "tracked"]);
        fs::write(repository.join("untracked"), "untracked")
            .expect("untracked test file must be written");

        let state = GitCliObservation::new()
            .observe_worktree(&repository)
            .expect("unborn worktree observation must succeed");

        assert!(matches!(state.head(), Head::Unborn(_)));
        assert_eq!(state.upstream(), Some(&UpstreamState::Unconfigured));
        assert!(state.changes().staged());
        assert!(!state.changes().unstaged());
        assert!(state.changes().untracked());
    }

    #[test]
    fn observes_staged_unstaged_untracked_and_detached_states() {
        let root = TestDirectory::new();
        let repository = root.path().join("repository");
        fs::create_dir(&repository).expect("repository directory must be created");
        git(&repository, &["init", "-q"]);
        fs::write(repository.join("tracked"), "base\n").expect("tracked file must be written");
        git(&repository, &["add", "tracked"]);
        commit(&repository, "initial");

        fs::write(repository.join("staged"), "staged\n").expect("staged file must be written");
        git(&repository, &["add", "staged"]);
        fs::write(repository.join("tracked"), "changed\n").expect("tracked file must be modified");
        fs::write(repository.join("untracked"), "untracked\n")
            .expect("untracked file must be written");

        let observer = GitCliObservation::new();
        let branch_state = observer
            .observe_worktree(&repository)
            .expect("dirty worktree observation must succeed");
        assert!(matches!(branch_state.head(), Head::Branch(_)));
        assert_eq!(
            branch_state.changes(),
            WorktreeChanges::new(true, true, true)
        );

        git(&repository, &["reset", "--hard", "-q", "HEAD"]);
        let _ = fs::remove_file(repository.join("untracked"));
        git(&repository, &["checkout", "--detach", "-q", "HEAD"]);
        let detached = observer
            .observe_worktree(&repository)
            .expect("detached worktree observation must succeed");
        assert_eq!(detached.head(), &Head::Detached);
        assert!(detached.upstream().is_none());
    }

    #[test]
    fn observes_tracking_and_gone_upstream_with_exact_remote_identity() {
        let root = TestDirectory::new();
        let repository = root.path().join("repository");
        fs::create_dir(&repository).expect("repository directory must be created");
        git(&repository, &["init", "-q"]);
        commit(&repository, "initial");
        let branch = String::from_utf8(
            Command::new("git")
                .arg("-C")
                .arg(&repository)
                .args(["branch", "--show-current"])
                .output()
                .expect("branch query must start")
                .stdout,
        )
        .expect("branch must be UTF-8")
        .trim()
        .to_owned();
        git(
            &repository,
            &[
                "remote",
                "add",
                "team/origin",
                "https://example.invalid/repository.git",
            ],
        );
        git(
            &repository,
            &["config", &format!("branch.{branch}.remote"), "team/origin"],
        );
        git(
            &repository,
            &[
                "config",
                &format!("branch.{branch}.merge"),
                &format!("refs/heads/{branch}"),
            ],
        );
        git(
            &repository,
            &[
                "update-ref",
                &format!("refs/remotes/team/origin/{branch}"),
                "HEAD",
            ],
        );

        let observer = GitCliObservation::new();
        let tracking = observer
            .observe_worktree(&repository)
            .expect("tracking observation must succeed");
        assert!(matches!(
            tracking.upstream(),
            Some(UpstreamState::Tracking { upstream, divergence })
                if upstream.remote().name() == "team/origin"
                    && upstream.branch().name() == branch
                    && divergence.ahead() == 0
                    && divergence.behind() == 0
        ));

        git(
            &repository,
            &[
                "update-ref",
                "-d",
                &format!("refs/remotes/team/origin/{branch}"),
            ],
        );
        let gone = observer
            .observe_worktree(&repository)
            .expect("gone upstream observation must succeed");
        assert!(matches!(
            gone.upstream(),
            Some(UpstreamState::Gone(upstream))
                if upstream.remote().name() == "team/origin"
                    && upstream.branch().name() == branch
        ));
    }

    #[test]
    fn observes_ahead_behind_and_diverged_from_the_locally_known_upstream() {
        let root = TestDirectory::new();
        let repository = root.path().join("repository");
        fs::create_dir(&repository).expect("repository directory must be created");
        git(&repository, &["init", "-q"]);
        commit(&repository, "initial");
        let branch = String::from_utf8(
            Command::new("git")
                .arg("-C")
                .arg(&repository)
                .args(["branch", "--show-current"])
                .output()
                .expect("branch query must start")
                .stdout,
        )
        .expect("branch must be UTF-8")
        .trim()
        .to_owned();
        git(
            &repository,
            &[
                "remote",
                "add",
                "origin",
                "https://example.invalid/repository.git",
            ],
        );
        git(
            &repository,
            &["config", &format!("branch.{branch}.remote"), "origin"],
        );
        git(
            &repository,
            &[
                "config",
                &format!("branch.{branch}.merge"),
                &format!("refs/heads/{branch}"),
            ],
        );
        git(
            &repository,
            &[
                "update-ref",
                &format!("refs/remotes/origin/{branch}"),
                "HEAD",
            ],
        );
        let base = String::from_utf8(
            Command::new("git")
                .arg("-C")
                .arg(&repository)
                .args(["rev-parse", "HEAD"])
                .output()
                .expect("base revision query must start")
                .stdout,
        )
        .expect("revision must be UTF-8")
        .trim()
        .to_owned();

        commit(&repository, "local");
        let observer = GitCliObservation::new();
        let ahead = observer
            .observe_worktree(&repository)
            .expect("ahead observation must succeed");
        assert!(matches!(
            ahead.upstream(),
            Some(UpstreamState::Tracking { divergence, .. })
                if divergence.ahead() == 1 && divergence.behind() == 0
        ));

        git(&repository, &["branch", "remote-sim", &base]);
        git(&repository, &["checkout", "-q", "remote-sim"]);
        commit(&repository, "remote");
        let remote_revision = String::from_utf8(
            Command::new("git")
                .arg("-C")
                .arg(&repository)
                .args(["rev-parse", "HEAD"])
                .output()
                .expect("remote revision query must start")
                .stdout,
        )
        .expect("revision must be UTF-8")
        .trim()
        .to_owned();
        git(&repository, &["checkout", "-q", &branch]);
        git(
            &repository,
            &[
                "update-ref",
                &format!("refs/remotes/origin/{branch}"),
                &remote_revision,
            ],
        );

        let diverged = observer
            .observe_worktree(&repository)
            .expect("diverged observation must succeed");
        assert!(matches!(
            diverged.upstream(),
            Some(UpstreamState::Tracking { divergence, .. })
                if divergence.ahead() == 1 && divergence.behind() == 1
        ));

        git(&repository, &["reset", "--hard", "-q", &base]);
        let behind = observer
            .observe_worktree(&repository)
            .expect("behind observation must succeed");
        assert!(matches!(
            behind.upstream(),
            Some(UpstreamState::Tracking { divergence, .. })
                if divergence.ahead() == 0 && divergence.behind() == 1
        ));
    }

    #[test]
    fn observes_repository_remotes_as_repository_level_facts() {
        let root = TestDirectory::new();
        let repository = root.path().join("repository");
        fs::create_dir(&repository).expect("repository directory must be created");
        git(&repository, &["init", "-q"]);
        git(
            &repository,
            &[
                "remote",
                "add",
                "origin",
                "https://example.invalid/origin.git",
            ],
        );
        git(
            &repository,
            &[
                "remote",
                "add",
                "backup",
                "https://example.invalid/backup.git",
            ],
        );

        let remotes = GitCliObservation::new()
            .observe_repository_remotes(&repository)
            .expect("repository remote observation must succeed");
        let names: Vec<&str> = remotes.iter().map(|remote| remote.name()).collect();

        assert_eq!(names, vec!["backup", "origin"]);
    }

    #[test]
    fn observation_keeps_git_subprocess_count_bounded_per_target() {
        let root = TestDirectory::new();
        let repository = root.path().join("repository");
        fs::create_dir(&repository).expect("repository directory must be created");
        git(&repository, &["init", "-q"]);
        commit(&repository, "initial");
        let branch = String::from_utf8(
            Command::new("git")
                .arg("-C")
                .arg(&repository)
                .args(["branch", "--show-current"])
                .output()
                .expect("branch query must start")
                .stdout,
        )
        .expect("branch must be UTF-8")
        .trim()
        .to_owned();
        git(
            &repository,
            &[
                "remote",
                "add",
                "origin",
                "https://example.invalid/repository.git",
            ],
        );
        git(
            &repository,
            &["config", &format!("branch.{branch}.remote"), "origin"],
        );
        git(
            &repository,
            &[
                "config",
                &format!("branch.{branch}.merge"),
                &format!("refs/heads/{branch}"),
            ],
        );
        git(
            &repository,
            &[
                "update-ref",
                &format!("refs/remotes/origin/{branch}"),
                "HEAD",
            ],
        );

        let observer = GitCliObservation::new();
        observer
            .observe_worktree(&repository)
            .expect("tracked worktree observation must succeed");
        assert_eq!(observer.git.command_count(), 2);

        observer
            .observe_repository_remotes(&repository)
            .expect("repository remote observation must succeed");
        assert_eq!(observer.git.command_count(), 3);
    }

    #[test]
    fn missing_subject_is_classified_as_resource_unavailable() {
        let root = TestDirectory::new();
        let missing = root.path().join("missing");

        let error = GitCliObservation::new()
            .observe_worktree(&missing)
            .expect_err("missing observation subject must fail");

        assert_eq!(error.kind(), PortErrorKind::ResourceUnavailable);
    }

    #[test]
    fn observation_honors_cancellation_before_starting_git() {
        let root = TestDirectory::new();
        let repository = root.path().join("repository");
        fs::create_dir(&repository).expect("repository directory must be created");
        git(&repository, &["init", "-q"]);

        let cancelled = Arc::new(AtomicBool::new(true));
        let cancellation = Arc::clone(&cancelled);
        let observer = GitCliObservation::with_cancellation(Arc::new(move || {
            cancellation.load(Ordering::Acquire)
        }));
        let error = observer
            .observe_worktree(&repository)
            .expect_err("cancelled observation must not start Git");

        assert_eq!(error.kind(), PortErrorKind::Cancelled);
    }
}
