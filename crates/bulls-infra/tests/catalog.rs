// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use bulls_application::ports::{
    CatalogMissingScope, CatalogReconciliation, CatalogReconciliationCoverage, FilesystemObjectId,
    LocationIdentityEvidence, RepositoryCatalogPort, RepositoryIdentityEvidence,
    WorkspaceRevisionPort,
};
use bulls_core::{
    Location, LocationAvailability, LocationId, Repository, RepositoryId, Worktree, WorktreeId,
};
use bulls_infra::{SqliteRepositoryCatalog, SqliteWorkspaceRevisionStore};

static TEST_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let sequence = TEST_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "bulls-catalog-test-{}-{}",
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

fn reconciliation(
    root: &Path,
    repository_path: &Path,
    coverage: CatalogReconciliationCoverage,
    missing_scope: CatalogMissingScope,
) -> CatalogReconciliation {
    let repository_id = RepositoryId::from("repository-1");
    let location_id = LocationId::from("location-1");
    CatalogReconciliation::new(root, coverage, missing_scope)
        .with_entities(
            vec![Repository::new(repository_id.clone())],
            vec![Location::new(
                location_id.clone(),
                repository_id.clone(),
                repository_path,
                LocationAvailability::Available,
            )],
            vec![Worktree::new(
                WorktreeId::from("worktree-1"),
                repository_id.clone(),
                location_id.clone(),
            )],
        )
        .with_identity_evidence(
            vec![RepositoryIdentityEvidence::new(
                repository_id,
                repository_path.join(".git"),
                Some(FilesystemObjectId::new(7, 11)),
            )],
            vec![LocationIdentityEvidence::new(
                location_id,
                Some(FilesystemObjectId::new(7, 12)),
            )],
        )
}

#[test]
fn catalog_writes_advance_the_workspace_revision_once_per_committed_operation() {
    let root = TestDirectory::new();
    let workspace = root.path().join("workspace");
    let repository_path = workspace.join("repository");
    let data_dir = root.path().join("data");
    let mut catalog = SqliteRepositoryCatalog::open(&data_dir).expect("catalog storage must open");
    let revisions = SqliteWorkspaceRevisionStore::open(&data_dir)
        .expect("workspace revision storage must open");

    assert_eq!(
        revisions
            .workspace_revision()
            .expect("initial workspace revision must load")
            .value(),
        0
    );

    catalog
        .reconcile(&reconciliation(
            &workspace,
            &repository_path,
            CatalogReconciliationCoverage::Complete,
            CatalogMissingScope::Filesystem(7),
        ))
        .expect("catalog reconciliation must succeed");
    assert_eq!(
        revisions
            .workspace_revision()
            .expect("catalog revision must load")
            .value(),
        1
    );

    catalog
        .reconcile(&reconciliation(
            &workspace,
            &repository_path,
            CatalogReconciliationCoverage::Complete,
            CatalogMissingScope::Filesystem(7),
        ))
        .expect("repeated catalog reconciliation must succeed");
    assert_eq!(
        revisions
            .workspace_revision()
            .expect("reconciled workspace revision must load")
            .value(),
        2
    );

    catalog
        .mark_discovery_root_offline(&workspace)
        .expect("catalog availability update must succeed");
    assert_eq!(
        revisions
            .workspace_revision()
            .expect("availability revision must load")
            .value(),
        3
    );

    catalog
        .mark_discovery_root_offline(&workspace)
        .expect("idempotent availability update must succeed");
    assert_eq!(
        revisions
            .workspace_revision()
            .expect("idempotent availability revision must load")
            .value(),
        3
    );
}

#[test]
fn catalog_persists_identity_and_applies_conservative_availability_transitions() {
    let root = TestDirectory::new();
    let workspace = root.path().join("workspace");
    let repository_path = workspace.join("repository");
    let data_dir = root.path().join("data");
    let mut catalog = SqliteRepositoryCatalog::open(&data_dir).expect("catalog storage must open");

    catalog
        .reconcile(&reconciliation(
            &workspace,
            &repository_path,
            CatalogReconciliationCoverage::Complete,
            CatalogMissingScope::Filesystem(7),
        ))
        .expect("initial reconciliation must succeed");
    catalog
        .reconcile(&CatalogReconciliation::new(
            &workspace,
            CatalogReconciliationCoverage::Partial,
            CatalogMissingScope::None,
        ))
        .expect("partial reconciliation must not infer absence");
    assert_eq!(
        catalog
            .location(&LocationId::from("location-1"))
            .expect("location must load")
            .expect("location must remain known")
            .availability(),
        LocationAvailability::Available
    );

    catalog
        .reconcile(&CatalogReconciliation::new(
            &workspace,
            CatalogReconciliationCoverage::Complete,
            CatalogMissingScope::Filesystem(7),
        ))
        .expect("complete reconciliation must mark absent locations");
    assert_eq!(
        catalog
            .location(&LocationId::from("location-1"))
            .expect("missing location must load")
            .expect("missing location identity must remain known")
            .availability(),
        LocationAvailability::Missing
    );
    catalog
        .mark_discovery_root_offline(&workspace)
        .expect("unavailable root must mark known locations offline");
    drop(catalog);

    let catalog = SqliteRepositoryCatalog::open(&data_dir).expect("catalog storage must reopen");
    let location = catalog
        .location(&LocationId::from("location-1"))
        .expect("location must load after restart")
        .expect("location identity must survive restart");
    assert_eq!(location.path(), repository_path);
    assert_eq!(location.availability(), LocationAvailability::Offline);
    assert_eq!(
        catalog
            .repository_id_by_common_dir_object(FilesystemObjectId::new(7, 11))
            .expect("repository identity lookup must succeed"),
        Some(RepositoryId::from("repository-1"))
    );
}

#[test]
fn failed_catalog_reconciliation_is_atomic() {
    let root = TestDirectory::new();
    let workspace = root.path().join("workspace");
    let repository_path = workspace.join("repository");
    let mut catalog =
        SqliteRepositoryCatalog::open(root.path().join("data")).expect("catalog storage must open");
    catalog
        .reconcile(&reconciliation(
            &workspace,
            &repository_path,
            CatalogReconciliationCoverage::Partial,
            CatalogMissingScope::None,
        ))
        .expect("initial reconciliation must succeed");

    let replacement_id = RepositoryId::from("repository-2");
    let conflicting = CatalogReconciliation::new(
        &workspace,
        CatalogReconciliationCoverage::Partial,
        CatalogMissingScope::None,
    )
    .with_entities(
        vec![Repository::new(replacement_id.clone())],
        vec![Location::new(
            LocationId::from("location-1"),
            replacement_id.clone(),
            &repository_path,
            LocationAvailability::Available,
        )],
        Vec::new(),
    )
    .with_identity_evidence(
        vec![RepositoryIdentityEvidence::new(
            replacement_id.clone(),
            workspace.join("replacement.git"),
            Some(FilesystemObjectId::new(7, 21)),
        )],
        vec![LocationIdentityEvidence::new(
            LocationId::from("location-1"),
            Some(FilesystemObjectId::new(7, 22)),
        )],
    );

    catalog
        .reconcile(&conflicting)
        .expect_err("foreign-key conflict must roll back the transaction");
    assert!(
        catalog
            .repository(&replacement_id)
            .expect("catalog must remain readable")
            .is_none()
    );
    assert_eq!(
        catalog
            .location(&LocationId::from("location-1"))
            .expect("original location must remain readable")
            .expect("original location must remain persisted")
            .repository_id(),
        &RepositoryId::from("repository-1")
    );
}

#[cfg(unix)]
#[test]
fn catalog_preserves_non_utf8_paths_without_loss() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let root = TestDirectory::new();
    let workspace = root.path().join("workspace");
    let repository_path = workspace.join(OsString::from_vec(vec![b'r', b'e', b'p', b'o', 0xff]));
    let mut catalog =
        SqliteRepositoryCatalog::open(root.path().join("data")).expect("catalog storage must open");
    catalog
        .reconcile(&reconciliation(
            &workspace,
            &repository_path,
            CatalogReconciliationCoverage::Partial,
            CatalogMissingScope::None,
        ))
        .expect("non-UTF-8 path must persist");

    assert_eq!(
        catalog
            .location(&LocationId::from("location-1"))
            .expect("location must load")
            .expect("location must exist")
            .path(),
        repository_path
    );
}
