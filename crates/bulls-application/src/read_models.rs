// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use std::time::SystemTime;

use bulls_core::{
    LocationAvailability, LocationId, ObservationCoverage, ObservationFailureKind, Remote,
    RepositoryId, WorktreeGitState, WorktreeId,
};

use crate::WorkspaceRevision;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Knowledge<T> {
    Known(T),
    Unknown,
}

impl<T> Knowledge<T> {
    pub const fn as_ref(&self) -> Knowledge<&T> {
        match self {
            Self::Known(value) => Knowledge::Known(value),
            Self::Unknown => Knowledge::Unknown,
        }
    }

    pub const fn is_known(&self) -> bool {
        matches!(self, Self::Known(_))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObservationAttemptOutcomeProjection {
    Succeeded,
    Failed(ObservationFailureKind),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObservationAttemptProjection {
    observed_at: SystemTime,
    coverage: ObservationCoverage,
    outcome: ObservationAttemptOutcomeProjection,
}

impl ObservationAttemptProjection {
    pub const fn new(
        observed_at: SystemTime,
        coverage: ObservationCoverage,
        outcome: ObservationAttemptOutcomeProjection,
    ) -> Self {
        Self {
            observed_at,
            coverage,
            outcome,
        }
    }

    pub const fn observed_at(self) -> SystemTime {
        self.observed_at
    }

    pub const fn coverage(self) -> ObservationCoverage {
        self.coverage
    }

    pub const fn outcome(self) -> ObservationAttemptOutcomeProjection {
        self.outcome
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObservationSuccessProjection {
    observed_at: SystemTime,
    coverage: ObservationCoverage,
}

impl ObservationSuccessProjection {
    pub const fn new(observed_at: SystemTime, coverage: ObservationCoverage) -> Self {
        Self {
            observed_at,
            coverage,
        }
    }

    pub const fn observed_at(self) -> SystemTime {
        self.observed_at
    }

    pub const fn coverage(self) -> ObservationCoverage {
        self.coverage
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObservationEvidenceState {
    NeverObserved,
    Observed {
        observed_at: SystemTime,
        coverage: ObservationCoverage,
    },
    LatestAttemptFailed {
        attempted_at: SystemTime,
        failure: ObservationFailureKind,
        latest_success: Option<ObservationSuccessProjection>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservationStatusProjection {
    latest_attempt: Option<ObservationAttemptProjection>,
    latest_success: Option<ObservationSuccessProjection>,
}

impl ObservationStatusProjection {
    pub const fn new(
        latest_attempt: Option<ObservationAttemptProjection>,
        latest_success: Option<ObservationSuccessProjection>,
    ) -> Self {
        Self {
            latest_attempt,
            latest_success,
        }
    }

    pub const fn latest_attempt(&self) -> Option<ObservationAttemptProjection> {
        self.latest_attempt
    }

    pub const fn latest_success(&self) -> Option<ObservationSuccessProjection> {
        self.latest_success
    }

    pub const fn evidence_state(&self) -> ObservationEvidenceState {
        let Some(attempt) = self.latest_attempt else {
            return ObservationEvidenceState::NeverObserved;
        };

        match attempt.outcome() {
            ObservationAttemptOutcomeProjection::Succeeded => ObservationEvidenceState::Observed {
                observed_at: attempt.observed_at(),
                coverage: attempt.coverage(),
            },
            ObservationAttemptOutcomeProjection::Failed(failure) => {
                ObservationEvidenceState::LatestAttemptFailed {
                    attempted_at: attempt.observed_at(),
                    failure,
                    latest_success: self.latest_success,
                }
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocationProjection {
    id: LocationId,
    path: PathBuf,
    availability: LocationAvailability,
}

impl LocationProjection {
    pub fn new(id: LocationId, path: PathBuf, availability: LocationAvailability) -> Self {
        Self {
            id,
            path,
            availability,
        }
    }

    pub fn id(&self) -> &LocationId {
        &self.id
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    pub const fn availability(&self) -> LocationAvailability {
        self.availability
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorktreeProjection {
    revision: WorkspaceRevision,
    id: WorktreeId,
    repository_id: RepositoryId,
    location: LocationProjection,
    git_state: Option<WorktreeGitState>,
    observation: ObservationStatusProjection,
}

impl WorktreeProjection {
    pub fn new(
        revision: WorkspaceRevision,
        id: WorktreeId,
        repository_id: RepositoryId,
        location: LocationProjection,
        git_state: Option<WorktreeGitState>,
        observation: ObservationStatusProjection,
    ) -> Self {
        Self {
            revision,
            id,
            repository_id,
            location,
            git_state,
            observation,
        }
    }

    pub const fn revision(&self) -> WorkspaceRevision {
        self.revision
    }

    pub fn id(&self) -> &WorktreeId {
        &self.id
    }

    pub fn repository_id(&self) -> &RepositoryId {
        &self.repository_id
    }

    pub const fn location(&self) -> &LocationProjection {
        &self.location
    }

    pub const fn git_state(&self) -> Option<&WorktreeGitState> {
        self.git_state.as_ref()
    }

    pub const fn observation(&self) -> &ObservationStatusProjection {
        &self.observation
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepositoryListItemProjection {
    id: RepositoryId,
    locations: Vec<LocationProjection>,
    worktree_count: usize,
    has_remotes: Knowledge<bool>,
    has_local_work: Knowledge<bool>,
}

impl RepositoryListItemProjection {
    pub fn new(
        id: RepositoryId,
        locations: Vec<LocationProjection>,
        worktree_count: usize,
        has_remotes: Knowledge<bool>,
        has_local_work: Knowledge<bool>,
    ) -> Self {
        Self {
            id,
            locations,
            worktree_count,
            has_remotes,
            has_local_work,
        }
    }

    pub fn id(&self) -> &RepositoryId {
        &self.id
    }

    pub fn locations(&self) -> &[LocationProjection] {
        &self.locations
    }

    pub fn location_count(&self) -> usize {
        self.locations.len()
    }

    pub fn available_location_count(&self) -> usize {
        self.locations
            .iter()
            .filter(|location| location.availability() == LocationAvailability::Available)
            .count()
    }

    pub const fn worktree_count(&self) -> usize {
        self.worktree_count
    }

    pub const fn has_remotes(&self) -> &Knowledge<bool> {
        &self.has_remotes
    }

    pub const fn has_local_work(&self) -> &Knowledge<bool> {
        &self.has_local_work
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepositoryListProjection {
    revision: WorkspaceRevision,
    total_matching: usize,
    offset: usize,
    items: Vec<RepositoryListItemProjection>,
}

impl RepositoryListProjection {
    pub fn new(
        revision: WorkspaceRevision,
        total_matching: usize,
        offset: usize,
        items: Vec<RepositoryListItemProjection>,
    ) -> Self {
        Self {
            revision,
            total_matching,
            offset,
            items,
        }
    }

    pub const fn revision(&self) -> WorkspaceRevision {
        self.revision
    }

    pub const fn total_matching(&self) -> usize {
        self.total_matching
    }

    pub const fn offset(&self) -> usize {
        self.offset
    }

    pub fn items(&self) -> &[RepositoryListItemProjection] {
        &self.items
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepositoryDetailProjection {
    revision: WorkspaceRevision,
    id: RepositoryId,
    locations: Vec<LocationProjection>,
    remotes: Option<Vec<Remote>>,
    remotes_observation: ObservationStatusProjection,
    worktrees: Vec<WorktreeProjection>,
}

impl RepositoryDetailProjection {
    pub fn new(
        revision: WorkspaceRevision,
        id: RepositoryId,
        locations: Vec<LocationProjection>,
        remotes: Option<Vec<Remote>>,
        remotes_observation: ObservationStatusProjection,
        worktrees: Vec<WorktreeProjection>,
    ) -> Self {
        Self {
            revision,
            id,
            locations,
            remotes,
            remotes_observation,
            worktrees,
        }
    }

    pub const fn revision(&self) -> WorkspaceRevision {
        self.revision
    }

    pub fn id(&self) -> &RepositoryId {
        &self.id
    }

    pub fn locations(&self) -> &[LocationProjection] {
        &self.locations
    }

    pub fn remotes(&self) -> Option<&[Remote]> {
        self.remotes.as_deref()
    }

    pub const fn remotes_observation(&self) -> &ObservationStatusProjection {
        &self.remotes_observation
    }

    pub fn worktrees(&self) -> &[WorktreeProjection] {
        &self.worktrees
    }

    pub fn has_unknown_observation_evidence(&self) -> bool {
        self.remotes.is_none()
            || self
                .worktrees
                .iter()
                .any(|worktree| worktree.git_state().is_none())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OverviewProjection {
    revision: WorkspaceRevision,
    repository_count: usize,
    available_repository_count: usize,
    worktree_count: usize,
    repositories_with_local_work_count: usize,
    repositories_with_unknown_local_work_count: usize,
    repositories_without_remotes_count: usize,
    repositories_with_unknown_remotes_count: usize,
}

impl OverviewProjection {
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        revision: WorkspaceRevision,
        repository_count: usize,
        available_repository_count: usize,
        worktree_count: usize,
        repositories_with_local_work_count: usize,
        repositories_with_unknown_local_work_count: usize,
        repositories_without_remotes_count: usize,
        repositories_with_unknown_remotes_count: usize,
    ) -> Self {
        Self {
            revision,
            repository_count,
            available_repository_count,
            worktree_count,
            repositories_with_local_work_count,
            repositories_with_unknown_local_work_count,
            repositories_without_remotes_count,
            repositories_with_unknown_remotes_count,
        }
    }

    pub const fn revision(&self) -> WorkspaceRevision {
        self.revision
    }

    pub const fn repository_count(&self) -> usize {
        self.repository_count
    }

    pub const fn available_repository_count(&self) -> usize {
        self.available_repository_count
    }

    pub const fn worktree_count(&self) -> usize {
        self.worktree_count
    }

    pub const fn repositories_with_local_work_count(&self) -> usize {
        self.repositories_with_local_work_count
    }

    pub const fn repositories_with_unknown_local_work_count(&self) -> usize {
        self.repositories_with_unknown_local_work_count
    }

    pub const fn repositories_without_remotes_count(&self) -> usize {
        self.repositories_without_remotes_count
    }

    pub const fn repositories_with_unknown_remotes_count(&self) -> usize {
        self.repositories_with_unknown_remotes_count
    }

    pub const fn has_unknown_observation_evidence(&self) -> bool {
        self.repositories_with_unknown_local_work_count > 0
            || self.repositories_with_unknown_remotes_count > 0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceMirror {
    revision: WorkspaceRevision,
    overview: OverviewProjection,
    repositories: Vec<RepositoryDetailProjection>,
}

impl WorkspaceMirror {
    pub fn new(
        revision: WorkspaceRevision,
        overview: OverviewProjection,
        repositories: Vec<RepositoryDetailProjection>,
    ) -> Self {
        Self {
            revision,
            overview,
            repositories,
        }
    }

    pub const fn revision(&self) -> WorkspaceRevision {
        self.revision
    }

    pub const fn overview(&self) -> &OverviewProjection {
        &self.overview
    }

    pub fn repositories(&self) -> &[RepositoryDetailProjection] {
        &self.repositories
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use bulls_core::{ObservationCoverage, ObservationFailureKind};

    use super::{
        ObservationAttemptOutcomeProjection, ObservationAttemptProjection,
        ObservationEvidenceState, ObservationStatusProjection, ObservationSuccessProjection,
    };

    #[test]
    fn observation_evidence_distinguishes_never_observed_state() {
        let status = ObservationStatusProjection::new(None, None);

        assert_eq!(
            status.evidence_state(),
            ObservationEvidenceState::NeverObserved
        );
    }

    #[test]
    fn observation_evidence_preserves_success_coverage_and_time() {
        let observed_at = UNIX_EPOCH + Duration::from_secs(10);
        let status = ObservationStatusProjection::new(
            Some(ObservationAttemptProjection::new(
                observed_at,
                ObservationCoverage::Partial,
                ObservationAttemptOutcomeProjection::Succeeded,
            )),
            Some(ObservationSuccessProjection::new(
                observed_at,
                ObservationCoverage::Partial,
            )),
        );

        assert_eq!(
            status.evidence_state(),
            ObservationEvidenceState::Observed {
                observed_at,
                coverage: ObservationCoverage::Partial,
            }
        );
    }

    #[test]
    fn observation_evidence_keeps_failed_attempt_separate_from_last_success() {
        let succeeded_at = UNIX_EPOCH + Duration::from_secs(10);
        let failed_at = UNIX_EPOCH + Duration::from_secs(20);
        let success =
            ObservationSuccessProjection::new(succeeded_at, ObservationCoverage::Complete);
        let status = ObservationStatusProjection::new(
            Some(ObservationAttemptProjection::new(
                failed_at,
                ObservationCoverage::Partial,
                ObservationAttemptOutcomeProjection::Failed(ObservationFailureKind::TimedOut),
            )),
            Some(success),
        );

        assert_eq!(
            status.evidence_state(),
            ObservationEvidenceState::LatestAttemptFailed {
                attempted_at: failed_at,
                failure: ObservationFailureKind::TimedOut,
                latest_success: Some(success),
            }
        );
    }
}
