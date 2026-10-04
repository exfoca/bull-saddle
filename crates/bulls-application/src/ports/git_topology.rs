// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};

use super::{PortResult, RepositoryCandidate};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct GitRepositoryLayout {
    worktree_path: Option<PathBuf>,
    git_dir: PathBuf,
    common_dir: PathBuf,
}

impl GitRepositoryLayout {
    pub fn worktree(
        worktree_path: impl Into<PathBuf>,
        git_dir: impl Into<PathBuf>,
        common_dir: impl Into<PathBuf>,
    ) -> Self {
        Self {
            worktree_path: Some(worktree_path.into()),
            git_dir: git_dir.into(),
            common_dir: common_dir.into(),
        }
    }

    pub fn bare(git_dir: impl Into<PathBuf>, common_dir: impl Into<PathBuf>) -> Self {
        Self {
            worktree_path: None,
            git_dir: git_dir.into(),
            common_dir: common_dir.into(),
        }
    }

    pub fn worktree_path(&self) -> Option<&Path> {
        self.worktree_path.as_deref()
    }

    pub fn git_dir(&self) -> &Path {
        &self.git_dir
    }

    pub fn common_dir(&self) -> &Path {
        &self.common_dir
    }

    pub const fn is_bare(&self) -> bool {
        self.worktree_path.is_none()
    }
}

pub trait GitRepositoryProbePort {
    fn probe(&self, candidate: &RepositoryCandidate) -> PortResult<GitRepositoryLayout>;
}
