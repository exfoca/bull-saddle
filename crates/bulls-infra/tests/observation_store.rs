// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, UNIX_EPOCH};

use bulls_application::ports::{
    CatalogMissingScope, CatalogReconciliation, CatalogReconciliationCoverage,
    ObservationStorePort, RepositoryCatalogPort, WorkspaceRevisionPort,
};
use bulls_core::{
    Branch, Freshness, Location, LocationAvailability, LocationId, Observation, ObservationAttempt,
    ObservationCoverage, ObservationFailure, ObservationFailureKind, ObservationKey,
    ObservationMetadata, ObservationRunId, Remote, Repository, RepositoryId, Upstream,
    UpstreamDivergence, UpstreamState, Worktree, WorktreeChanges, WorktreeGitState, WorktreeId,
};
use bulls_infra::{
    SqliteRepositoryCatalog, SqliteRepositoryRemotesObservationStore, SqliteWorkspaceRevisionStore,
    SqliteWorktreeObservationStore,
};

static TEST_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let sequence = TEST_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "bulls-observation-store-test-{}-{}",
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

struct FixtureIds {
    repository_id: RepositoryId,
    worktree_id: WorktreeId,
}

fn register_subjects(data_dir: &Path) -> FixtureIds {
    let repository_id = RepositoryId::from("repository-1");
    let location_id = LocationId::from("location-1");
    let worktree_id = WorktreeId::from("worktree-1");
    let path = data_dir
        .parent()
        .expect("data directory must have a parent")
        .join("workspace/repository");
    fs::create_dir_all(&path).expect("fixture repository path must be created");

    let reconciliation = CatalogReconciliation::new(
        path.parent().expect("repository path must have a parent"),
        CatalogReconciliationCoverage::Complete,
        CatalogMissingScope::None,
    )
    .with_entities(
        vec![Repository::new(repository_id.clone())],
        vec![Location::new(
            location_id.clone(),
            repository_id.clone(),
            &path,
            LocationAvailability::Available,
        )],
        vec![Worktree::new(
            worktree_id.clone(),
            repository_id.clone(),
            location_id,
        )],
    );
    let mut catalog = SqliteRepositoryCatalog::open(data_dir).expect("catalog storage must open");
    catalog
        .reconcile(&reconciliation)
        .expect("fixture subjects must persist");

    FixtureIds {
        repository_id,
        worktree_id,
    }
}

fn metadata(
    run_id: &str,
    key: ObservationKey,
    seconds: u64,
    coverage: ObservationCoverage,
) -> ObservationMetadata {
    ObservationMetadata::new(
        ObservationRunId::from(run_id),
        key,
        Freshness::new(UNIX_EPOCH + Duration::from_secs(seconds)),
        coverage,
    )
}

#[test]
fn worktree_store_keeps_latest_success_when_a_newer_attempt_fails() {
    let root = TestDirectory::new();
    let data_dir = root.path().join("data");
    let ids = register_subjects(&data_dir);
    let key = ObservationKey::worktree_git_state(ids.worktree_id.clone());
    let upstream = Upstream::new(Remote::new("origin"), Branch::new("main"));
    let state = WorktreeGitState::branch(
        Branch::new("main"),
        UpstreamState::Tracking {
            upstream,
            divergence: UpstreamDivergence::new(4, 2),
        },
    )
    .with_changes(WorktreeChanges::new(true, false, true));
    let success = ObservationAttempt::Succeeded(Observation::new(
        metadata(
            "run-success",
            key.clone(),
            10,
            ObservationCoverage::Complete,
        ),
        state.clone(),
    ));
    let failure = ObservationAttempt::Failed(ObservationFailure::new(
        metadata("run-failure", key.clone(), 20, ObservationCoverage::Partial),
        ObservationFailureKind::TimedOut,
    ));
    let mut store = SqliteWorktreeObservationStore::open(&data_dir)
        .expect("worktree observation store must open");
    let revisions = SqliteWorkspaceRevisionStore::open(&data_dir)
        .expect("workspace revision storage must open");
    assert_eq!(
        revisions
            .workspace_revision()
            .expect("catalog revision must load")
            .value(),
        1
    );

    store
        .save_attempt(&success)
        .expect("successful observation must persist");
    assert_eq!(
        revisions
            .workspace_revision()
            .expect("successful observation revision must load")
            .value(),
        2
    );
    store
        .save_attempt(&failure)
        .expect("failed attempt must persist independently");
    assert_eq!(
        revisions
            .workspace_revision()
            .expect("failed observation revision must load")
            .value(),
        3
    );

    let latest_observation = store
        .latest_observation(&key)
        .expect("latest successful observation must load")
        .expect("a successful observation must exist");
    assert_eq!(latest_observation.value(), &state);
    assert_eq!(
        latest_observation.metadata().run_id(),
        &ObservationRunId::from("run-success")
    );

    let latest_attempt = store
        .latest_attempt(&key)
        .expect("latest attempt must load")
        .expect("an attempt must exist");
    assert!(matches!(
        latest_attempt,
        ObservationAttempt::Failed(ref failure)
            if failure.kind() == ObservationFailureKind::TimedOut
                && failure.metadata().coverage() == ObservationCoverage::Partial
                && failure.metadata().run_id() == &ObservationRunId::from("run-failure")
    ));
}

#[test]
fn failed_observation_write_does_not_advance_workspace_revision() {
    let root = TestDirectory::new();
    let data_dir = root.path().join("data");
    let key = ObservationKey::worktree_git_state(WorktreeId::from("missing-worktree"));
    let attempt: ObservationAttempt<WorktreeGitState> =
        ObservationAttempt::Failed(ObservationFailure::new(
            metadata("run-missing", key, 25, ObservationCoverage::Partial),
            ObservationFailureKind::SubjectUnavailable,
        ));
    let mut store = SqliteWorktreeObservationStore::open(&data_dir)
        .expect("worktree observation store must open");
    let revisions = SqliteWorkspaceRevisionStore::open(&data_dir)
        .expect("workspace revision storage must open");

    store
        .save_attempt(&attempt)
        .expect_err("unknown worktree observation must not persist");

    assert_eq!(
        revisions
            .workspace_revision()
            .expect("failed write revision must load")
            .value(),
        0
    );
}

#[test]
fn repository_remote_store_round_trips_values_and_process_failures() {
    let root = TestDirectory::new();
    let data_dir = root.path().join("data");
    let ids = register_subjects(&data_dir);
    let key = ObservationKey::repository_remotes(ids.repository_id.clone());
    let remotes = vec![Remote::new("origin"), Remote::new("team/upstream")];
    let success = ObservationAttempt::Succeeded(Observation::new(
        metadata(
            "run-remotes",
            key.clone(),
            30,
            ObservationCoverage::Complete,
        ),
        remotes.clone(),
    ));
    let failure = ObservationAttempt::Failed(ObservationFailure::new(
        metadata(
            "run-remotes-failure",
            key.clone(),
            40,
            ObservationCoverage::Partial,
        ),
        ObservationFailureKind::ProcessExited { exit_code: 128 },
    ));
    let mut store = SqliteRepositoryRemotesObservationStore::open(&data_dir)
        .expect("repository observation store must open");

    store
        .save_attempt(&success)
        .expect("remote observation must persist");
    store
        .save_attempt(&failure)
        .expect("remote failure must persist");

    let latest_observation = store
        .latest_observation(&key)
        .expect("latest successful remotes must load")
        .expect("remote observation must exist");
    assert_eq!(latest_observation.value(), &remotes);

    let latest_attempt = store
        .latest_attempt(&key)
        .expect("latest remote attempt must load")
        .expect("remote attempt must exist");
    assert!(matches!(
        latest_attempt,
        ObservationAttempt::Failed(ref failure)
            if failure.kind() == (ObservationFailureKind::ProcessExited { exit_code: 128 })
    ));
}

#[test]
fn typed_stores_reject_observation_keys_owned_by_the_other_subject_kind() {
    let root = TestDirectory::new();
    let data_dir = root.path().join("data");
    let ids = register_subjects(&data_dir);
    let worktree_key = ObservationKey::worktree_git_state(ids.worktree_id);
    let repository_key = ObservationKey::repository_remotes(ids.repository_id);
    let worktree_store = SqliteWorktreeObservationStore::open(&data_dir)
        .expect("worktree observation store must open");
    let repository_store = SqliteRepositoryRemotesObservationStore::open(&data_dir)
        .expect("repository observation store must open");

    let worktree_error = worktree_store
        .latest_attempt(&repository_key)
        .expect_err("repository key must not be accepted by worktree store");
    let repository_error = repository_store
        .latest_attempt(&worktree_key)
        .expect_err("worktree key must not be accepted by repository store");

    assert_eq!(
        worktree_error.kind(),
        bulls_application::ports::PortErrorKind::InvariantViolation
    );
    assert_eq!(
        repository_error.kind(),
        bulls_application::ports::PortErrorKind::InvariantViolation
    );
}
