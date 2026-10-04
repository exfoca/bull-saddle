// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};

use super::PortResult;
use crate::FailureCause;

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum BareRepositoryDiscovery {
    #[default]
    Disabled,
    Enabled,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum SymlinkTraversal {
    #[default]
    DoNotFollow,
    FollowWithinRoot,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum FilesystemBoundary {
    #[default]
    StayOnRootFilesystem,
    CrossFilesystems,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct DiscoveryRequest {
    root: PathBuf,
    exclusions: Vec<PathBuf>,
    bare_repositories: BareRepositoryDiscovery,
    symlink_traversal: SymlinkTraversal,
    filesystem_boundary: FilesystemBoundary,
}

impl DiscoveryRequest {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            exclusions: Vec::new(),
            bare_repositories: BareRepositoryDiscovery::Disabled,
            symlink_traversal: SymlinkTraversal::DoNotFollow,
            filesystem_boundary: FilesystemBoundary::StayOnRootFilesystem,
        }
    }

    pub fn with_exclusion(mut self, path: impl Into<PathBuf>) -> Self {
        self.exclusions.push(path.into());
        self
    }

    pub fn with_bare_repositories(mut self, value: BareRepositoryDiscovery) -> Self {
        self.bare_repositories = value;
        self
    }

    pub fn with_symlink_traversal(mut self, value: SymlinkTraversal) -> Self {
        self.symlink_traversal = value;
        self
    }

    pub fn with_filesystem_boundary(mut self, value: FilesystemBoundary) -> Self {
        self.filesystem_boundary = value;
        self
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn exclusions(&self) -> &[PathBuf] {
        &self.exclusions
    }

    pub const fn bare_repositories(&self) -> BareRepositoryDiscovery {
        self.bare_repositories
    }

    pub const fn symlink_traversal(&self) -> SymlinkTraversal {
        self.symlink_traversal
    }

    pub const fn filesystem_boundary(&self) -> FilesystemBoundary {
        self.filesystem_boundary
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RepositoryCandidate {
    path: PathBuf,
}

impl RepositoryCandidate {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DiscoveryCompleteness {
    Complete,
    Partial,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DiscoveryIssueKind {
    PermissionDenied,
    ResourceUnavailable,
    IoFailure,
}

impl DiscoveryIssueKind {
    pub const fn failure_cause(self) -> FailureCause {
        match self {
            Self::PermissionDenied => FailureCause::PermissionDenied,
            Self::ResourceUnavailable => FailureCause::ResourceUnavailable,
            Self::IoFailure => FailureCause::IoFailure,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct DiscoveryIssue {
    path: PathBuf,
    kind: DiscoveryIssueKind,
}

impl DiscoveryIssue {
    pub fn new(path: impl Into<PathBuf>, kind: DiscoveryIssueKind) -> Self {
        Self {
            path: path.into(),
            kind,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub const fn kind(&self) -> DiscoveryIssueKind {
        self.kind
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DiscoveryOutcome {
    root: PathBuf,
    candidates: Vec<RepositoryCandidate>,
    issues: Vec<DiscoveryIssue>,
}

impl DiscoveryOutcome {
    pub fn new(
        root: impl Into<PathBuf>,
        candidates: Vec<RepositoryCandidate>,
        issues: Vec<DiscoveryIssue>,
    ) -> Self {
        Self {
            root: root.into(),
            candidates,
            issues,
        }
    }

    pub fn complete(root: impl Into<PathBuf>, candidates: Vec<RepositoryCandidate>) -> Self {
        Self::new(root, candidates, Vec::new())
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn candidates(&self) -> &[RepositoryCandidate] {
        &self.candidates
    }

    pub fn issues(&self) -> &[DiscoveryIssue] {
        &self.issues
    }

    pub fn completeness(&self) -> DiscoveryCompleteness {
        if self.issues.is_empty() {
            DiscoveryCompleteness::Complete
        } else {
            DiscoveryCompleteness::Partial
        }
    }
}

pub trait RepositoryDiscoveryPort {
    fn discover(&self, request: &DiscoveryRequest) -> PortResult<DiscoveryOutcome>;
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{BareRepositoryDiscovery, DiscoveryRequest, FilesystemBoundary, SymlinkTraversal};

    #[test]
    fn discovery_request_defaults_are_conservative() {
        let request = DiscoveryRequest::new("/workspace");

        assert_eq!(request.root(), Path::new("/workspace"));
        assert!(request.exclusions().is_empty());
        assert_eq!(
            request.bare_repositories(),
            BareRepositoryDiscovery::Disabled
        );
        assert_eq!(request.symlink_traversal(), SymlinkTraversal::DoNotFollow);
        assert_eq!(
            request.filesystem_boundary(),
            FilesystemBoundary::StayOnRootFilesystem
        );
    }

    #[test]
    fn discovery_request_preserves_explicit_traversal_policy() {
        let request = DiscoveryRequest::new("/workspace")
            .with_exclusion("vendor")
            .with_bare_repositories(BareRepositoryDiscovery::Enabled)
            .with_symlink_traversal(SymlinkTraversal::FollowWithinRoot)
            .with_filesystem_boundary(FilesystemBoundary::CrossFilesystems);

        assert_eq!(request.exclusions(), &[Path::new("vendor").to_path_buf()]);
        assert_eq!(
            request.bare_repositories(),
            BareRepositoryDiscovery::Enabled
        );
        assert_eq!(
            request.symlink_traversal(),
            SymlinkTraversal::FollowWithinRoot
        );
        assert_eq!(
            request.filesystem_boundary(),
            FilesystemBoundary::CrossFilesystems
        );
    }
}
