// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, UNIX_EPOCH};

use bulls_application::ports::{
    CatalogMissingScope, CatalogReconciliation, CatalogReconciliationCoverage,
    ObservationStorePort, RepositoryCatalogPort, WorkspaceReadPort,
};
use bulls_core::{
    Freshness, Location, LocationAvailability, LocationId, Observation, ObservationAttempt,
    ObservationCoverage, ObservationFailure, ObservationFailureKind, ObservationKey,
    ObservationMetadata, ObservationRunId, Remote, Repository, RepositoryId, Worktree,
    WorktreeGitState, WorktreeId,
};
use bulls_infra::{
    SqliteRepositoryCatalog, SqliteRepositoryRemotesObservationStore, SqliteWorkspaceReadStore,
    SqliteWorktreeObservationStore,
};

static TEST_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let sequence = TEST_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "bulls-workspace-read-test-{}-{}",
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

fn metadata(run_id: &str, key: ObservationKey, observed_at: u64) -> ObservationMetadata {
    ObservationMetadata::new(
        ObservationRunId::from(run_id),
        key,
        Freshness::new(UNIX_EPOCH + Duration::from_secs(observed_at)),
        ObservationCoverage::Complete,
    )
}

#[test]
fn workspace_read_loads_catalog_and_latest_observation_state_as_one_snapshot() {
    let root = TestDirectory::new();
    let data_dir = root.path().join("data");
    let workspace = root.path().join("workspace");
    let repository_a = RepositoryId::from("repository-a");
    let repository_b = RepositoryId::from("repository-b");
    let location_a = LocationId::from("location-a");
    let location_b = LocationId::from("location-b");
    let worktree_a = WorktreeId::from("worktree-a");
    let worktree_b = WorktreeId::from("worktree-b");

    let mut catalog = SqliteRepositoryCatalog::open(&data_dir).expect("catalog storage must open");
    catalog
        .reconcile(
            &CatalogReconciliation::new(
                &workspace,
                CatalogReconciliationCoverage::Partial,
                CatalogMissingScope::None,
            )
            .with_entities(
                vec![
                    Repository::new(repository_b.clone()),
                    Repository::new(repository_a.clone()),
                ],
                vec![
                    Location::new(
                        location_b.clone(),
                        repository_b.clone(),
                        workspace.join("repository-b"),
                        LocationAvailability::Available,
                    ),
                    Location::new(
                        location_a.clone(),
                        repository_a.clone(),
                        workspace.join("repository-a"),
                        LocationAvailability::Available,
                    ),
                ],
                vec![
                    Worktree::new(worktree_b.clone(), repository_b.clone(), location_b.clone()),
                    Worktree::new(worktree_a.clone(), repository_a.clone(), location_a.clone()),
                ],
            ),
        )
        .expect("catalog reconciliation must succeed");

    let mut worktree_store =
        SqliteWorktreeObservationStore::open(&data_dir).expect("worktree store must open");
    worktree_store
        .save_attempt(&ObservationAttempt::Succeeded(Observation::new(
            metadata(
                "worktree-success",
                ObservationKey::worktree_git_state(worktree_a.clone()),
                10,
            ),
            WorktreeGitState::detached(),
        )))
        .expect("worktree success must persist");
    worktree_store
        .save_attempt(&ObservationAttempt::<WorktreeGitState>::Failed(
            ObservationFailure::new(
                metadata(
                    "worktree-failure",
                    ObservationKey::worktree_git_state(worktree_a.clone()),
                    20,
                ),
                ObservationFailureKind::TimedOut,
            ),
        ))
        .expect("worktree failure must persist");

    let mut repository_store = SqliteRepositoryRemotesObservationStore::open(&data_dir)
        .expect("repository observation store must open");
    repository_store
        .save_attempt(&ObservationAttempt::Succeeded(Observation::new(
            metadata(
                "repository-success",
                ObservationKey::repository_remotes(repository_a.clone()),
                30,
            ),
            vec![Remote::new("origin"), Remote::new("backup")],
        )))
        .expect("repository success must persist");
    repository_store
        .save_attempt(&ObservationAttempt::<Vec<Remote>>::Failed(
            ObservationFailure::new(
                metadata(
                    "repository-failure",
                    ObservationKey::repository_remotes(repository_a.clone()),
                    40,
                ),
                ObservationFailureKind::SubjectUnavailable,
            ),
        ))
        .expect("repository failure must persist");

    let reader = SqliteWorkspaceReadStore::open(&data_dir).expect("workspace reader must open");
    let snapshot = reader
        .read_workspace()
        .expect("workspace snapshot must load");

    assert_eq!(snapshot.revision().value(), 5);
    assert_eq!(
        snapshot
            .repositories()
            .iter()
            .map(|repository| repository.id().as_str())
            .collect::<Vec<_>>(),
        ["repository-a", "repository-b"]
    );
    assert_eq!(
        snapshot
            .locations()
            .iter()
            .map(|location| location.id().as_str())
            .collect::<Vec<_>>(),
        ["location-a", "location-b"]
    );
    assert_eq!(
        snapshot
            .worktrees()
            .iter()
            .map(|worktree| worktree.id().as_str())
            .collect::<Vec<_>>(),
        ["worktree-a", "worktree-b"]
    );

    let worktree_state = snapshot
        .worktree_git_states()
        .first()
        .expect("observed worktree state must exist");
    assert!(matches!(
        worktree_state.latest_attempt(),
        ObservationAttempt::Failed(failure)
            if failure.kind() == ObservationFailureKind::TimedOut
    ));
    assert_eq!(
        worktree_state
            .latest_observation()
            .expect("last successful worktree observation must remain readable")
            .metadata()
            .run_id()
            .as_str(),
        "worktree-success"
    );

    let repository_state = snapshot
        .repository_remotes()
        .first()
        .expect("observed repository state must exist");
    assert!(matches!(
        repository_state.latest_attempt(),
        ObservationAttempt::Failed(failure)
            if failure.kind() == ObservationFailureKind::SubjectUnavailable
    ));
    assert_eq!(
        repository_state
            .latest_observation()
            .expect("last successful repository observation must remain readable")
            .value()
            .iter()
            .map(Remote::name)
            .collect::<Vec<_>>(),
        ["origin", "backup"]
    );

    assert_eq!(snapshot.worktree_git_states().len(), 1);
    assert_eq!(snapshot.repository_remotes().len(), 1);
}
