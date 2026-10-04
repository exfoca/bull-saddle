// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use std::time::SystemTime;

use crate::{ObservationRunId, RepositoryId, WorktreeId};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Freshness {
    observed_at: SystemTime,
}

impl Freshness {
    pub fn new(observed_at: SystemTime) -> Self {
        Self { observed_at }
    }

    pub fn observed_at(&self) -> SystemTime {
        self.observed_at
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ObservationKind {
    RepositoryRemotes,
    WorktreeGitState,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum ObservationKey {
    RepositoryRemotes(RepositoryId),
    WorktreeGitState(WorktreeId),
}

impl ObservationKey {
    pub fn repository_remotes(repository_id: RepositoryId) -> Self {
        Self::RepositoryRemotes(repository_id)
    }

    pub fn worktree_git_state(worktree_id: WorktreeId) -> Self {
        Self::WorktreeGitState(worktree_id)
    }

    pub const fn kind(&self) -> ObservationKind {
        match self {
            Self::RepositoryRemotes(_) => ObservationKind::RepositoryRemotes,
            Self::WorktreeGitState(_) => ObservationKind::WorktreeGitState,
        }
    }

    pub fn repository_id(&self) -> Option<&RepositoryId> {
        match self {
            Self::RepositoryRemotes(repository_id) => Some(repository_id),
            Self::WorktreeGitState(_) => None,
        }
    }

    pub fn worktree_id(&self) -> Option<&WorktreeId> {
        match self {
            Self::RepositoryRemotes(_) => None,
            Self::WorktreeGitState(worktree_id) => Some(worktree_id),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ObservationCoverage {
    Complete,
    Partial,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservationMetadata {
    run_id: ObservationRunId,
    key: ObservationKey,
    freshness: Freshness,
    coverage: ObservationCoverage,
}

impl ObservationMetadata {
    pub fn new(
        run_id: ObservationRunId,
        key: ObservationKey,
        freshness: Freshness,
        coverage: ObservationCoverage,
    ) -> Self {
        Self {
            run_id,
            key,
            freshness,
            coverage,
        }
    }

    pub fn run_id(&self) -> &ObservationRunId {
        &self.run_id
    }

    pub fn key(&self) -> &ObservationKey {
        &self.key
    }

    pub const fn freshness(&self) -> Freshness {
        self.freshness
    }

    pub const fn coverage(&self) -> ObservationCoverage {
        self.coverage
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Observation<T> {
    metadata: ObservationMetadata,
    value: T,
}

impl<T> Observation<T> {
    pub fn new(metadata: ObservationMetadata, value: T) -> Self {
        Self { metadata, value }
    }

    pub const fn metadata(&self) -> &ObservationMetadata {
        &self.metadata
    }

    pub fn value(&self) -> &T {
        &self.value
    }

    pub const fn freshness(&self) -> Freshness {
        self.metadata.freshness()
    }

    pub fn into_parts(self) -> (ObservationMetadata, T) {
        (self.metadata, self.value)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ObservationFailureKind {
    SubjectUnavailable,
    ProcessExited { exit_code: i32 },
    TimedOut,
    Cancelled,
    OutputLimitExceeded,
    InvalidMachineOutput,
    SubjectDisappeared,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservationFailure {
    metadata: ObservationMetadata,
    kind: ObservationFailureKind,
}

impl ObservationFailure {
    pub fn new(metadata: ObservationMetadata, kind: ObservationFailureKind) -> Self {
        Self { metadata, kind }
    }

    pub const fn metadata(&self) -> &ObservationMetadata {
        &self.metadata
    }

    pub const fn kind(&self) -> ObservationFailureKind {
        self.kind
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ObservationAttempt<T> {
    Succeeded(Observation<T>),
    Failed(ObservationFailure),
}

impl<T> ObservationAttempt<T> {
    pub const fn metadata(&self) -> &ObservationMetadata {
        match self {
            Self::Succeeded(observation) => observation.metadata(),
            Self::Failed(failure) => failure.metadata(),
        }
    }

    pub const fn is_success(&self) -> bool {
        matches!(self, Self::Succeeded(_))
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use super::{
        Freshness, Observation, ObservationAttempt, ObservationCoverage, ObservationFailure,
        ObservationFailureKind, ObservationKey, ObservationKind, ObservationMetadata,
    };
    use crate::{Head, ObservationRunId, RepositoryId, WorktreeGitState, WorktreeId};

    fn worktree_metadata(observed_at_seconds: u64) -> ObservationMetadata {
        ObservationMetadata::new(
            ObservationRunId::from("observation-run-1"),
            ObservationKey::worktree_git_state(WorktreeId::from("worktree-1")),
            Freshness::new(UNIX_EPOCH + Duration::from_secs(observed_at_seconds)),
            ObservationCoverage::Complete,
        )
    }

    #[test]
    fn observation_key_preserves_subject_ownership_and_kind() {
        let repository_id = RepositoryId::from("repository-1");
        let worktree_id = WorktreeId::from("worktree-1");
        let repository_key = ObservationKey::repository_remotes(repository_id.clone());
        let worktree_key = ObservationKey::worktree_git_state(worktree_id.clone());

        assert_eq!(repository_key.kind(), ObservationKind::RepositoryRemotes);
        assert_eq!(repository_key.repository_id(), Some(&repository_id));
        assert_eq!(repository_key.worktree_id(), None);
        assert_eq!(worktree_key.kind(), ObservationKind::WorktreeGitState);
        assert_eq!(worktree_key.repository_id(), None);
        assert_eq!(worktree_key.worktree_id(), Some(&worktree_id));
    }

    #[test]
    fn observation_preserves_run_subject_kind_coverage_and_time() {
        let metadata = worktree_metadata(42);
        let observation = Observation::new(metadata.clone(), WorktreeGitState::detached());

        assert_eq!(observation.metadata(), &metadata);
        assert_eq!(
            observation.metadata().run_id(),
            &ObservationRunId::from("observation-run-1")
        );
        assert_eq!(
            observation.metadata().key().kind(),
            ObservationKind::WorktreeGitState
        );
        assert_eq!(
            observation.metadata().coverage(),
            ObservationCoverage::Complete
        );
        assert_eq!(
            observation.freshness().observed_at(),
            UNIX_EPOCH + Duration::from_secs(42)
        );
        assert_eq!(observation.value().head(), &Head::Detached);
    }

    #[test]
    fn equal_values_from_distinct_runs_are_distinct_observations() {
        let first = Observation::new(worktree_metadata(42), "state");
        let second = Observation::new(
            ObservationMetadata::new(
                ObservationRunId::from("observation-run-2"),
                ObservationKey::worktree_git_state(WorktreeId::from("worktree-1")),
                Freshness::new(UNIX_EPOCH + Duration::from_secs(42)),
                ObservationCoverage::Complete,
            ),
            "state",
        );

        assert_ne!(first, second);
    }

    #[test]
    fn partial_success_remains_distinct_from_complete_success() {
        let partial_metadata = ObservationMetadata::new(
            ObservationRunId::from("observation-run-1"),
            ObservationKey::worktree_git_state(WorktreeId::from("worktree-1")),
            Freshness::new(UNIX_EPOCH),
            ObservationCoverage::Partial,
        );
        let complete_metadata = ObservationMetadata::new(
            ObservationRunId::from("observation-run-1"),
            ObservationKey::worktree_git_state(WorktreeId::from("worktree-1")),
            Freshness::new(UNIX_EPOCH),
            ObservationCoverage::Complete,
        );

        assert_ne!(
            Observation::new(partial_metadata, 7_u64),
            Observation::new(complete_metadata, 7_u64)
        );
    }

    #[test]
    fn failed_attempt_preserves_typed_failure_without_a_value() {
        let metadata = worktree_metadata(42);
        let attempt: ObservationAttempt<WorktreeGitState> = ObservationAttempt::Failed(
            ObservationFailure::new(metadata.clone(), ObservationFailureKind::TimedOut),
        );

        assert!(!attempt.is_success());
        assert_eq!(attempt.metadata(), &metadata);
        assert!(matches!(
            attempt,
            ObservationAttempt::Failed(ref failure)
                if failure.kind() == ObservationFailureKind::TimedOut
        ));
    }

    #[test]
    fn observation_releases_metadata_together_with_value() {
        let metadata = worktree_metadata(42);
        let observation = Observation::new(metadata.clone(), 7_u64);

        assert_eq!(observation.into_parts(), (metadata, 7));
    }
}
