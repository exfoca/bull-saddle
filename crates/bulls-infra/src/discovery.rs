// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeSet, HashSet};
use std::ffi::OsStr;
use std::fs::{self, FileType, Metadata};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bulls_application::ports::{
    BareRepositoryDiscovery, DiscoveryIssue, DiscoveryIssueKind, DiscoveryOutcome,
    DiscoveryRequest, FilesystemBoundary, FilesystemObjectId, PortError, PortErrorKind, PortResult,
    RepositoryCandidate, RepositoryDiscoveryPort, SymlinkTraversal,
};

use crate::filesystem::{filesystem_id, filesystem_object_id};

#[derive(Clone)]
pub struct NativeRepositoryDiscovery {
    cancellation: Arc<dyn Fn() -> bool + Send + Sync>,
}

impl NativeRepositoryDiscovery {
    pub fn new() -> Self {
        Self::with_cancellation(Arc::new(|| false))
    }

    pub fn with_cancellation(cancellation: Arc<dyn Fn() -> bool + Send + Sync>) -> Self {
        Self { cancellation }
    }
}

impl Default for NativeRepositoryDiscovery {
    fn default() -> Self {
        Self::new()
    }
}

impl RepositoryDiscoveryPort for NativeRepositoryDiscovery {
    fn discover(&self, request: &DiscoveryRequest) -> PortResult<DiscoveryOutcome> {
        ensure_not_cancelled(&self.cancellation)?;
        let root = fs::canonicalize(request.root()).map_err(|error| root_port_error(&error))?;
        let root_metadata = fs::metadata(&root).map_err(|error| root_port_error(&error))?;
        if !root_metadata.is_dir() {
            return Err(PortError::new(PortErrorKind::InvalidData));
        }

        let exclusions = normalized_exclusions(&root, request.exclusions());
        let mut traversal = Traversal::new(
            request,
            root.clone(),
            &root_metadata,
            exclusions,
            Arc::clone(&self.cancellation),
        );
        traversal.run(root)?;
        Ok(traversal.finish())
    }
}

struct Traversal<'a> {
    request: &'a DiscoveryRequest,
    root: PathBuf,
    root_filesystem_id: Option<u128>,
    exclusions: Vec<PathBuf>,
    cancellation: Arc<dyn Fn() -> bool + Send + Sync>,
    pending: Vec<PathBuf>,
    visited_paths: HashSet<PathBuf>,
    visited_objects: HashSet<FilesystemObjectId>,
    candidates: BTreeSet<PathBuf>,
    issues: Vec<DiscoveryIssue>,
}

impl<'a> Traversal<'a> {
    fn new(
        request: &'a DiscoveryRequest,
        root: PathBuf,
        root_metadata: &Metadata,
        exclusions: Vec<PathBuf>,
        cancellation: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Self {
        let root_filesystem_id = filesystem_id(&root, root_metadata);

        Self {
            request,
            root,
            root_filesystem_id,
            exclusions,
            cancellation,
            pending: Vec::new(),
            visited_paths: HashSet::new(),
            visited_objects: HashSet::new(),
            candidates: BTreeSet::new(),
            issues: Vec::new(),
        }
    }

    fn run(&mut self, root: PathBuf) -> PortResult<()> {
        self.pending.push(root);

        while let Some(directory) = self.pending.pop() {
            self.ensure_not_cancelled()?;
            self.visit_directory(directory)?;
        }

        Ok(())
    }

    fn finish(self) -> DiscoveryOutcome {
        let candidates = self
            .candidates
            .into_iter()
            .map(RepositoryCandidate::new)
            .collect();

        DiscoveryOutcome::new(self.root, candidates, self.issues)
    }

    fn visit_directory(&mut self, directory: PathBuf) -> PortResult<()> {
        if self.is_excluded(&directory) {
            return Ok(());
        }

        let metadata = match fs::metadata(&directory) {
            Ok(metadata) => metadata,
            Err(error) => {
                self.record_issue(directory, &error);
                return Ok(());
            }
        };

        if !metadata.is_dir() || !self.is_within_filesystem_boundary(&directory, &metadata) {
            return Ok(());
        }

        if !self.mark_visited(&directory, &metadata) {
            return Ok(());
        }

        let directory_read = match read_directory(&directory, &self.cancellation) {
            Ok(directory_read) => directory_read,
            Err(DirectoryReadError::Io(error)) => {
                self.record_issue(directory, &error);
                return Ok(());
            }
            Err(DirectoryReadError::Cancelled) => return Err(cancelled_port_error()),
        };

        for (path, error) in directory_read.issues {
            self.record_issue(path, &error);
        }
        let entries = directory_read.entries;

        if self.request.bare_repositories() == BareRepositoryDiscovery::Enabled
            && !is_dot_git_directory(&directory)
            && looks_like_bare_repository(&entries)
        {
            self.candidates.insert(directory);
            return Ok(());
        }

        if entries.iter().any(DirectoryEntry::is_git_marker) {
            self.candidates.insert(directory.clone());
        }

        for entry in entries.into_iter().rev() {
            self.ensure_not_cancelled()?;
            if entry.name == OsStr::new(".git") || self.is_excluded(&entry.path) {
                continue;
            }

            if entry.file_type.is_dir() {
                self.pending.push(entry.path);
                continue;
            }

            if entry.file_type.is_symlink()
                && self.request.symlink_traversal() == SymlinkTraversal::FollowWithinRoot
            {
                self.follow_symlink(entry.path)?;
            }
        }

        Ok(())
    }

    fn follow_symlink(&mut self, path: PathBuf) -> PortResult<()> {
        self.ensure_not_cancelled()?;
        let target = match fs::canonicalize(&path) {
            Ok(target) => target,
            Err(error) => {
                self.record_issue(path, &error);
                return Ok(());
            }
        };

        if !target.starts_with(&self.root) || self.is_excluded(&target) {
            return Ok(());
        }

        let metadata = match fs::metadata(&target) {
            Ok(metadata) => metadata,
            Err(error) => {
                self.record_issue(target, &error);
                return Ok(());
            }
        };

        if metadata.is_dir() {
            self.pending.push(target);
        }

        Ok(())
    }

    fn ensure_not_cancelled(&self) -> PortResult<()> {
        ensure_not_cancelled(&self.cancellation)
    }

    fn is_excluded(&self, path: &Path) -> bool {
        self.exclusions
            .iter()
            .any(|excluded| path.starts_with(excluded))
    }

    fn is_within_filesystem_boundary(&self, path: &Path, metadata: &Metadata) -> bool {
        if self.request.filesystem_boundary() == FilesystemBoundary::CrossFilesystems {
            return true;
        }

        match (self.root_filesystem_id, filesystem_id(path, metadata)) {
            (Some(root), Some(current)) => root == current,
            _ => true,
        }
    }

    fn mark_visited(&mut self, path: &Path, metadata: &Metadata) -> bool {
        if !self.visited_paths.insert(path.to_path_buf()) {
            return false;
        }

        let Some(object_id) = filesystem_object_id(path, metadata) else {
            return true;
        };

        self.visited_objects.insert(object_id)
    }

    fn record_issue(&mut self, path: PathBuf, error: &io::Error) {
        self.issues
            .push(DiscoveryIssue::new(path, discovery_issue_kind(error)));
    }
}

struct DirectoryEntry {
    name: std::ffi::OsString,
    path: PathBuf,
    file_type: FileType,
}

impl DirectoryEntry {
    fn is_git_marker(&self) -> bool {
        self.name == OsStr::new(".git") && (self.file_type.is_dir() || self.file_type.is_file())
    }
}

struct DirectoryRead {
    entries: Vec<DirectoryEntry>,
    issues: Vec<(PathBuf, io::Error)>,
}

enum DirectoryReadError {
    Io(io::Error),
    Cancelled,
}

fn read_directory(
    path: &Path,
    cancellation: &Arc<dyn Fn() -> bool + Send + Sync>,
) -> Result<DirectoryRead, DirectoryReadError> {
    let mut entries = Vec::new();
    let mut issues = Vec::new();
    let directory = fs::read_dir(path).map_err(DirectoryReadError::Io)?;

    for entry in directory {
        if cancellation() {
            return Err(DirectoryReadError::Cancelled);
        }
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                issues.push((path.to_path_buf(), error));
                continue;
            }
        };
        let entry_path = entry.path();
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(error) => {
                issues.push((entry_path, error));
                continue;
            }
        };
        entries.push(DirectoryEntry {
            name: entry.file_name(),
            path: entry_path,
            file_type,
        });
    }

    Ok(DirectoryRead { entries, issues })
}

fn looks_like_bare_repository(entries: &[DirectoryEntry]) -> bool {
    let mut head = false;
    let mut objects = false;
    let mut refs = false;

    for entry in entries {
        if entry.name == OsStr::new("HEAD") && entry.file_type.is_file() {
            head = true;
        } else if entry.name == OsStr::new("objects") && entry.file_type.is_dir() {
            objects = true;
        } else if entry.name == OsStr::new("refs") && entry.file_type.is_dir() {
            refs = true;
        }
    }

    head && objects && refs
}

fn is_dot_git_directory(path: &Path) -> bool {
    path.file_name() == Some(OsStr::new(".git"))
}

fn normalized_exclusions(root: &Path, exclusions: &[PathBuf]) -> Vec<PathBuf> {
    exclusions
        .iter()
        .map(|exclusion| {
            let path = if exclusion.is_absolute() {
                exclusion.clone()
            } else {
                root.join(exclusion)
            };

            fs::canonicalize(&path).unwrap_or(path)
        })
        .collect()
}

fn ensure_not_cancelled(cancellation: &Arc<dyn Fn() -> bool + Send + Sync>) -> PortResult<()> {
    if cancellation() {
        Err(cancelled_port_error())
    } else {
        Ok(())
    }
}

const fn cancelled_port_error() -> PortError {
    PortError::new(PortErrorKind::Cancelled)
}

fn discovery_issue_kind(error: &io::Error) -> DiscoveryIssueKind {
    match error.kind() {
        io::ErrorKind::PermissionDenied => DiscoveryIssueKind::PermissionDenied,
        io::ErrorKind::NotFound => DiscoveryIssueKind::ResourceUnavailable,
        _ => DiscoveryIssueKind::IoFailure,
    }
}

fn root_port_error(error: &io::Error) -> PortError {
    let kind = match error.kind() {
        io::ErrorKind::PermissionDenied => PortErrorKind::PermissionDenied,
        io::ErrorKind::NotFound => PortErrorKind::ResourceUnavailable,
        io::ErrorKind::InvalidData | io::ErrorKind::NotADirectory => PortErrorKind::InvalidData,
        _ => PortErrorKind::IoFailure,
    };

    PortError::new(kind)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    use bulls_application::ports::{
        BareRepositoryDiscovery, DiscoveryCompleteness, DiscoveryIssueKind, DiscoveryRequest,
        PortErrorKind, RepositoryDiscoveryPort, SymlinkTraversal,
    };

    use super::{NativeRepositoryDiscovery, discovery_issue_kind};

    static TEST_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let sequence = TEST_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "bulls-discovery-test-{}-{}",
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

    fn create_worktree_marker(path: &Path, marker_is_file: bool) {
        fs::create_dir_all(path).expect("repository directory must be created");
        let marker = path.join(".git");
        if marker_is_file {
            fs::write(marker, "gitdir: ../git-dir\n").expect("Git marker file must be created");
        } else {
            fs::create_dir(marker).expect("Git marker directory must be created");
        }
    }

    fn create_bare_signature(path: &Path) {
        fs::create_dir_all(path.join("objects")).expect("objects directory must be created");
        fs::create_dir_all(path.join("refs")).expect("refs directory must be created");
        fs::write(path.join("HEAD"), "ref: refs/heads/main\n")
            .expect("HEAD fixture must be created");
    }

    #[test]
    fn discovery_finds_git_directory_and_git_file_candidates() {
        let root = TestDirectory::new();
        let normal = root.path().join("normal repo");
        let linked = root.path().join("submódulo");
        create_worktree_marker(&normal, false);
        create_worktree_marker(&linked, true);

        fs::create_dir_all(normal.join("nested")).expect("nested directory must be created");
        create_worktree_marker(&normal.join("nested/repository"), false);

        let outcome = NativeRepositoryDiscovery::new()
            .discover(&DiscoveryRequest::new(root.path()))
            .expect("discovery must succeed");
        let paths: Vec<&Path> = outcome
            .candidates()
            .iter()
            .map(|candidate| candidate.path())
            .collect();

        assert_eq!(outcome.completeness(), DiscoveryCompleteness::Complete);
        assert_eq!(
            outcome.root(),
            fs::canonicalize(root.path())
                .expect("discovery root must canonicalize")
                .as_path()
        );
        assert_eq!(paths.len(), 3);
        assert!(paths.contains(&normal.as_path()));
        assert!(paths.contains(&linked.as_path()));
        assert!(paths.contains(&normal.join("nested/repository").as_path()));
    }

    #[test]
    fn discovery_prunes_excluded_subtrees_before_candidate_detection() {
        let root = TestDirectory::new();
        let visible = root.path().join("visible");
        let excluded = root.path().join("vendor/private");
        create_worktree_marker(&visible, false);
        create_worktree_marker(&excluded, false);

        let outcome = NativeRepositoryDiscovery::new()
            .discover(&DiscoveryRequest::new(root.path()).with_exclusion("vendor"))
            .expect("discovery must succeed");

        assert_eq!(outcome.candidates().len(), 1);
        assert_eq!(outcome.candidates()[0].path(), visible.as_path());
    }

    #[test]
    fn bare_repository_detection_is_explicit_and_prunes_git_internals() {
        let root = TestDirectory::new();
        let bare = root.path().join("archive.git");
        create_bare_signature(&bare);

        let disabled = NativeRepositoryDiscovery::new()
            .discover(&DiscoveryRequest::new(root.path()))
            .expect("discovery must succeed");
        assert!(disabled.candidates().is_empty());

        create_worktree_marker(&bare.join("objects/fake-worktree"), false);
        let enabled = NativeRepositoryDiscovery::new()
            .discover(
                &DiscoveryRequest::new(root.path())
                    .with_bare_repositories(BareRepositoryDiscovery::Enabled),
            )
            .expect("discovery must succeed");

        assert_eq!(enabled.candidates().len(), 1);
        assert_eq!(enabled.candidates()[0].path(), bare.as_path());
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_not_followed_by_default_and_following_stays_inside_root() {
        use std::os::unix::fs::symlink;

        let root = TestDirectory::new();
        let target = root.path().join("target/repository");
        create_worktree_marker(&target, false);
        let alias = root.path().join("alias");
        symlink(root.path().join("target"), &alias).expect("symlink fixture must be created");

        let default_outcome = NativeRepositoryDiscovery::new()
            .discover(&DiscoveryRequest::new(&alias))
            .expect("root symlink must normalize to its target");
        assert_eq!(default_outcome.candidates().len(), 1);
        assert_eq!(default_outcome.candidates()[0].path(), target.as_path());

        let nested_root = root.path().join("nested-root");
        fs::create_dir(&nested_root).expect("nested root must be created");
        let nested_alias = nested_root.join("alias");
        symlink(root.path().join("target"), &nested_alias)
            .expect("nested symlink fixture must be created");

        let not_followed = NativeRepositoryDiscovery::new()
            .discover(&DiscoveryRequest::new(&nested_root))
            .expect("discovery must succeed");
        assert!(not_followed.candidates().is_empty());

        let followed = NativeRepositoryDiscovery::new()
            .discover(
                &DiscoveryRequest::new(&nested_root)
                    .with_symlink_traversal(SymlinkTraversal::FollowWithinRoot),
            )
            .expect("discovery must succeed");
        assert!(followed.candidates().is_empty());
    }

    #[test]
    fn discovery_cancellation_aborts_the_operation_instead_of_becoming_a_partial_issue() {
        let root = TestDirectory::new();
        fs::create_dir_all(root.path().join("nested")).expect("nested directory must be created");

        let checks = Arc::new(AtomicU64::new(0));
        let cancellation_checks = Arc::clone(&checks);
        let discovery = NativeRepositoryDiscovery::with_cancellation(Arc::new(move || {
            cancellation_checks.fetch_add(1, Ordering::Relaxed) > 0
        }));

        let error = discovery
            .discover(&DiscoveryRequest::new(root.path()))
            .expect_err("cancelled discovery must abort the operation");

        assert_eq!(error.kind(), PortErrorKind::Cancelled);
        assert!(checks.load(Ordering::Relaxed) >= 2);
    }

    #[test]
    fn missing_root_is_an_operation_failure_not_a_partial_scan() {
        let root = TestDirectory::new();
        let missing = root.path().join("missing");

        let error = NativeRepositoryDiscovery::new()
            .discover(&DiscoveryRequest::new(missing))
            .expect_err("missing root must fail the discovery operation");

        assert_eq!(error.kind(), PortErrorKind::ResourceUnavailable);
    }

    #[test]
    fn discovery_issue_kind_preserves_partial_failure_semantics() {
        assert_eq!(
            discovery_issue_kind(&io::Error::from(io::ErrorKind::PermissionDenied)),
            DiscoveryIssueKind::PermissionDenied
        );
        assert_eq!(
            discovery_issue_kind(&io::Error::from(io::ErrorKind::NotFound)),
            DiscoveryIssueKind::ResourceUnavailable
        );
        assert_eq!(
            discovery_issue_kind(&io::Error::from(io::ErrorKind::Other)),
            DiscoveryIssueKind::IoFailure
        );
    }
}
