// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use bulls_core::{Remote, WorktreeGitState};

use super::PortResult;

pub trait GitObservationPort: Sync {
    fn observe_worktree(&self, worktree_path: &Path) -> PortResult<WorktreeGitState>;

    fn observe_repository_remotes(&self, repository_path: &Path) -> PortResult<Vec<Remote>>;
}
