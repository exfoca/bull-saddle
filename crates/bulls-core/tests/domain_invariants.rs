// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
use std::time::{Duration, UNIX_EPOCH};

use bulls_core::{
    Branch, Freshness, Head, HeadKind, Location, LocationAvailability, LocationId, Observation,
    ObservationCoverage, ObservationKey, ObservationMetadata, ObservationRunId, Remote, Repository,
    RepositoryId, Upstream, UpstreamDivergence, UpstreamRelation, UpstreamState, UpstreamStateKind,
    Worktree, WorktreeChanges, WorktreeGitState, WorktreeId,
};

#[test]
fn empty_repository_worktree_is_unborn_without_requiring_remote_tracking() {
    let worktree_state = WorktreeGitState::unborn(Branch::new("main"), UpstreamState::Unconfigured);

    assert_eq!(worktree_state.head(), &Head::Unborn(Branch::new("main")));
    assert_eq!(
        worktree_state.upstream(),
        Some(&UpstreamState::Unconfigured)
    );
}

#[test]
fn missing_location_does_not_erase_repository_identity_or_path() {
    let repository = Repository::new(RepositoryId::from("repository-1"));
    let location = Location::new(
        LocationId::from("location-1"),
        repository.id().clone(),
        "/workspace/repository",
        LocationAvailability::Missing,
    );

    assert_eq!(location.repository_id(), repository.id());
    assert_eq!(location.path(), Path::new("/workspace/repository"));
    assert_eq!(location.availability(), LocationAvailability::Missing);
}

#[test]
fn repository_identity_is_not_derived_from_location_path() {
    let first = Location::new(
        LocationId::from("location-1"),
        RepositoryId::from("repository-1"),
        "/workspace/shared-name",
        LocationAvailability::Available,
    );
    let second = Location::new(
        LocationId::from("location-2"),
        RepositoryId::from("repository-2"),
        "/archive/shared-name",
        LocationAvailability::Available,
    );

    assert_ne!(first.id(), second.id());
    assert_ne!(first.repository_id(), second.repository_id());
    assert_ne!(first.path(), second.path());
}

#[test]
fn linked_worktrees_share_repository_identity_but_not_location_identity() {
    let repository_id = RepositoryId::from("repository-1");
    let first = Worktree::new(
        WorktreeId::from("worktree-1"),
        repository_id.clone(),
        LocationId::from("location-1"),
    );
    let second = Worktree::new(
        WorktreeId::from("worktree-2"),
        repository_id.clone(),
        LocationId::from("location-2"),
    );

    assert_eq!(first.repository_id(), &repository_id);
    assert_eq!(second.repository_id(), &repository_id);
    assert_ne!(first.id(), second.id());
    assert_ne!(first.location_id(), second.location_id());
}

#[test]
fn linked_worktrees_can_hold_distinct_git_state_for_the_same_repository() {
    let repository_id = RepositoryId::from("repository-1");
    let main = Worktree::new(
        WorktreeId::from("worktree-main"),
        repository_id.clone(),
        LocationId::from("location-main"),
    );
    let linked = Worktree::new(
        WorktreeId::from("worktree-linked"),
        repository_id,
        LocationId::from("location-linked"),
    );
    let main_state = WorktreeGitState::branch(Branch::new("main"), UpstreamState::Unconfigured)
        .with_changes(WorktreeChanges::new(true, false, false));
    let linked_state = WorktreeGitState::detached();

    assert_eq!(main.repository_id(), linked.repository_id());
    assert_ne!(main_state, linked_state);
    assert_eq!(main_state.head(), &Head::Branch(Branch::new("main")));
    assert_eq!(linked_state.head(), &Head::Detached);
    assert!(main_state.changes().has_local_work());
    assert!(!linked_state.changes().has_local_work());
}

#[test]
fn semantic_discriminants_are_independent_from_name_payloads() {
    let main = Head::Branch(Branch::new("main"));
    let feature = Head::Branch(Branch::new("feature/semantic-boundary"));

    assert_ne!(main, feature);
    assert_eq!(main.kind(), HeadKind::Branch);
    assert_eq!(feature.kind(), HeadKind::Branch);

    let origin = UpstreamState::Gone(Upstream::new(Remote::new("origin"), Branch::new("main")));
    let mirror = UpstreamState::Gone(Upstream::new(Remote::new("mirror"), Branch::new("release")));

    assert_ne!(origin, mirror);
    assert_eq!(origin.kind(), UpstreamStateKind::Gone);
    assert_eq!(mirror.kind(), UpstreamStateKind::Gone);
}

#[test]
fn upstream_divergence_represents_all_canonical_relations() {
    let cases = [
        (0, 0, UpstreamRelation::Synchronized),
        (2, 0, UpstreamRelation::Ahead),
        (0, 3, UpstreamRelation::Behind),
        (2, 3, UpstreamRelation::Diverged),
    ];

    for (ahead, behind, expected) in cases {
        let divergence = UpstreamDivergence::new(ahead, behind);

        assert_eq!(divergence.ahead(), ahead);
        assert_eq!(divergence.behind(), behind);
        assert_eq!(divergence.relation(), expected);
    }
}

#[test]
fn observation_keeps_git_state_bound_to_its_observation_time() {
    let upstream = Upstream::new(Remote::new("origin"), Branch::new("main"));
    let state = WorktreeGitState::branch(
        Branch::new("main"),
        UpstreamState::Tracking {
            upstream,
            divergence: UpstreamDivergence::new(1, 0),
        },
    );
    let observed_at = UNIX_EPOCH + Duration::from_secs(42);
    let observation = Observation::new(
        ObservationMetadata::new(
            ObservationRunId::from("observation-run-1"),
            ObservationKey::worktree_git_state(WorktreeId::from("worktree-1")),
            Freshness::new(observed_at),
            ObservationCoverage::Complete,
        ),
        state,
    );

    assert_eq!(observation.freshness().observed_at(), observed_at);
    assert_eq!(
        observation.value().head(),
        &Head::Branch(Branch::new("main"))
    );
    assert!(matches!(
        observation.value().upstream(),
        Some(UpstreamState::Tracking { divergence, .. })
            if divergence.relation() == UpstreamRelation::Ahead
    ));
}
