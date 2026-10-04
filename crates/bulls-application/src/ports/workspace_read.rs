// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use bulls_core::{
    Location, Observation, ObservationAttempt, Remote, Repository, Worktree, WorktreeGitState,
};

use crate::WorkspaceRevision;

use super::PortResult;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceObservationState<T> {
    latest_attempt: ObservationAttempt<T>,
    latest_observation: Option<Observation<T>>,
}

impl<T> WorkspaceObservationState<T> {
    pub fn new(
        latest_attempt: ObservationAttempt<T>,
        latest_observation: Option<Observation<T>>,
    ) -> Self {
        Self {
            latest_attempt,
            latest_observation,
        }
    }

    pub const fn latest_attempt(&self) -> &ObservationAttempt<T> {
        &self.latest_attempt
    }

    pub const fn latest_observation(&self) -> Option<&Observation<T>> {
        self.latest_observation.as_ref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceReadSnapshot {
    revision: WorkspaceRevision,
    repositories: Vec<Repository>,
    locations: Vec<Location>,
    worktrees: Vec<Worktree>,
    repository_remotes: Vec<WorkspaceObservationState<Vec<Remote>>>,
    worktree_git_states: Vec<WorkspaceObservationState<WorktreeGitState>>,
}

impl WorkspaceReadSnapshot {
    pub fn new(
        revision: WorkspaceRevision,
        repositories: Vec<Repository>,
        locations: Vec<Location>,
        worktrees: Vec<Worktree>,
        repository_remotes: Vec<WorkspaceObservationState<Vec<Remote>>>,
        worktree_git_states: Vec<WorkspaceObservationState<WorktreeGitState>>,
    ) -> Self {
        Self {
            revision,
            repositories,
            locations,
            worktrees,
            repository_remotes,
            worktree_git_states,
        }
    }

    pub const fn revision(&self) -> WorkspaceRevision {
        self.revision
    }

    pub fn repositories(&self) -> &[Repository] {
        &self.repositories
    }

    pub fn locations(&self) -> &[Location] {
        &self.locations
    }

    pub fn worktrees(&self) -> &[Worktree] {
        &self.worktrees
    }

    pub fn repository_remotes(&self) -> &[WorkspaceObservationState<Vec<Remote>>] {
        &self.repository_remotes
    }

    pub fn worktree_git_states(&self) -> &[WorkspaceObservationState<WorktreeGitState>] {
        &self.worktree_git_states
    }
}

pub trait WorkspaceReadPort {
    fn read_workspace(&self) -> PortResult<WorkspaceReadSnapshot>;
}
