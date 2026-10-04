// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use bulls_application::DiscoverRepositories;
use bulls_application::ports::{
    CatalogReconciliation, CatalogReconciliationCoverage, DiscoveryOutcome, DiscoveryRequest,
    FilesystemIdentityEvidence, FilesystemIdentityPort, FilesystemObjectId, GitRepositoryLayout,
    GitRepositoryProbePort, IdentityGeneratorPort, PortError, PortErrorKind, PortResult,
    RepositoryCandidate, RepositoryCatalogPort, RepositoryDiscoveryPort,
};
use bulls_core::{
    Location, LocationId, ObservationRunId, Repository, RepositoryId, Worktree, WorktreeId,
};

struct TestDiscovery {
    outcome: DiscoveryOutcome,
}

impl RepositoryDiscoveryPort for TestDiscovery {
    fn discover(&self, _request: &DiscoveryRequest) -> PortResult<DiscoveryOutcome> {
        Ok(self.outcome.clone())
    }
}

struct FailingDiscovery(PortErrorKind);

impl RepositoryDiscoveryPort for FailingDiscovery {
    fn discover(&self, _request: &DiscoveryRequest) -> PortResult<DiscoveryOutcome> {
        Err(PortError::new(self.0))
    }
}

struct TestGitProbe {
    layouts: HashMap<PathBuf, Result<GitRepositoryLayout, PortError>>,
}

impl GitRepositoryProbePort for TestGitProbe {
    fn probe(&self, candidate: &RepositoryCandidate) -> PortResult<GitRepositoryLayout> {
        self.layouts
            .get(candidate.path())
            .expect("test candidate must have a probe result")
            .clone()
    }
}

struct TestFilesystemIdentity {
    identities: HashMap<PathBuf, FilesystemObjectId>,
}

impl FilesystemIdentityPort for TestFilesystemIdentity {
    fn identity(&self, path: &Path) -> PortResult<FilesystemIdentityEvidence> {
        Ok(self
            .identities
            .get(path)
            .copied()
            .map(FilesystemIdentityEvidence::strong)
            .unwrap_or(FilesystemIdentityEvidence::PathOnly))
    }
}

#[derive(Default)]
struct TestIdentityGenerator {
    repository: u64,
    location: u64,
    worktree: u64,
}

impl IdentityGeneratorPort for TestIdentityGenerator {
    fn next_repository_id(&mut self) -> PortResult<RepositoryId> {
        self.repository += 1;
        Ok(RepositoryId::from(format!(
            "repository-new-{}",
            self.repository
        )))
    }

    fn next_location_id(&mut self) -> PortResult<LocationId> {
        self.location += 1;
        Ok(LocationId::from(format!("location-new-{}", self.location)))
    }

    fn next_worktree_id(&mut self) -> PortResult<WorktreeId> {
        self.worktree += 1;
        Ok(WorktreeId::from(format!("worktree-new-{}", self.worktree)))
    }

    fn next_observation_run_id(&mut self) -> PortResult<ObservationRunId> {
        Ok(ObservationRunId::from("observation-run-test"))
    }
}

#[derive(Default)]
struct TestCatalog {
    repositories: HashMap<RepositoryId, Repository>,
    locations: HashMap<LocationId, Location>,
    locations_by_path: HashMap<PathBuf, LocationId>,
    repositories_by_common_dir: HashMap<PathBuf, RepositoryId>,
    repositories_by_object: HashMap<FilesystemObjectId, RepositoryId>,
    locations_by_object: HashMap<FilesystemObjectId, LocationId>,
    worktrees: Vec<Worktree>,
    reconciliation: Option<CatalogReconciliation>,
    offline_roots: Vec<PathBuf>,
}

impl RepositoryCatalogPort for TestCatalog {
    fn repositories(&self) -> PortResult<Vec<Repository>> {
        Ok(self.repositories.values().cloned().collect())
    }

    fn repository(&self, repository_id: &RepositoryId) -> PortResult<Option<Repository>> {
        Ok(self.repositories.get(repository_id).cloned())
    }

    fn location(&self, location_id: &LocationId) -> PortResult<Option<Location>> {
        Ok(self.locations.get(location_id).cloned())
    }

    fn location_by_path(&self, path: &Path) -> PortResult<Option<Location>> {
        Ok(self
            .locations_by_path
            .get(path)
            .and_then(|location_id| self.locations.get(location_id))
            .cloned())
    }

    fn repository_id_by_common_dir(&self, common_dir: &Path) -> PortResult<Option<RepositoryId>> {
        Ok(self.repositories_by_common_dir.get(common_dir).cloned())
    }

    fn repository_id_by_common_dir_object(
        &self,
        object_id: FilesystemObjectId,
    ) -> PortResult<Option<RepositoryId>> {
        Ok(self.repositories_by_object.get(&object_id).cloned())
    }

    fn location_id_by_filesystem_object(
        &self,
        object_id: FilesystemObjectId,
    ) -> PortResult<Option<LocationId>> {
        Ok(self.locations_by_object.get(&object_id).cloned())
    }

    fn locations_for_repository(&self, repository_id: &RepositoryId) -> PortResult<Vec<Location>> {
        Ok(self
            .locations
            .values()
            .filter(|location| location.repository_id() == repository_id)
            .cloned()
            .collect())
    }

    fn worktrees_for_repository(&self, repository_id: &RepositoryId) -> PortResult<Vec<Worktree>> {
        Ok(self
            .worktrees
            .iter()
            .filter(|worktree| worktree.repository_id() == repository_id)
            .cloned()
            .collect())
    }

    fn mark_discovery_root_offline(&mut self, root: &Path) -> PortResult<()> {
        self.offline_roots.push(root.to_path_buf());
        Ok(())
    }

    fn reconcile(&mut self, reconciliation: &CatalogReconciliation) -> PortResult<()> {
        self.reconciliation = Some(reconciliation.clone());
        Ok(())
    }
}

#[test]
fn linked_worktrees_share_repository_identity_but_keep_distinct_locations() {
    let main = PathBuf::from("/workspace/main");
    let linked = PathBuf::from("/workspace/linked");
    let common_dir = PathBuf::from("/workspace/main/.git");
    let discovery = TestDiscovery {
        outcome: DiscoveryOutcome::complete(
            "/workspace",
            vec![
                RepositoryCandidate::new(&main),
                RepositoryCandidate::new(&linked),
            ],
        ),
    };
    let git = TestGitProbe {
        layouts: HashMap::from([
            (
                main.clone(),
                Ok(GitRepositoryLayout::worktree(
                    &main,
                    "/workspace/main/.git",
                    &common_dir,
                )),
            ),
            (
                linked.clone(),
                Ok(GitRepositoryLayout::worktree(
                    &linked,
                    "/workspace/main/.git/worktrees/linked",
                    &common_dir,
                )),
            ),
        ]),
    };
    let filesystem = TestFilesystemIdentity {
        identities: HashMap::from([
            (common_dir, FilesystemObjectId::new(1, 10)),
            (main, FilesystemObjectId::new(1, 20)),
            (linked, FilesystemObjectId::new(1, 30)),
        ]),
    };
    let mut catalog = TestCatalog::default();
    let mut identities = TestIdentityGenerator::default();

    let outcome =
        DiscoverRepositories::new(&discovery, &git, &filesystem, &mut catalog, &mut identities)
            .execute(&DiscoveryRequest::new("/workspace"))
            .expect("discovery reconciliation must succeed");

    assert_eq!(outcome.coverage(), CatalogReconciliationCoverage::Complete);
    assert_eq!(outcome.repository_count(), 1);
    assert_eq!(outcome.location_count(), 2);
    assert_eq!(outcome.worktree_count(), 2);

    let reconciliation = catalog
        .reconciliation
        .expect("catalog must receive the reconciliation");
    assert_eq!(reconciliation.repositories().len(), 1);
    assert_eq!(reconciliation.locations().len(), 2);
    assert_eq!(reconciliation.worktrees().len(), 2);
    assert!(
        reconciliation
            .locations()
            .iter()
            .all(|location| { location.repository_id() == reconciliation.repositories()[0].id() })
    );
}

#[test]
fn strong_filesystem_evidence_does_not_fall_back_to_reused_path() {
    let path = PathBuf::from("/workspace/repository");
    let common_dir = path.join(".git");
    let old_repository_id = RepositoryId::from("repository-old");
    let old_location_id = LocationId::from("location-old");
    let old_repository = Repository::new(old_repository_id.clone());
    let old_location = Location::new(
        old_location_id.clone(),
        old_repository_id,
        &path,
        bulls_core::LocationAvailability::Available,
    );
    let discovery = TestDiscovery {
        outcome: DiscoveryOutcome::complete("/workspace", vec![RepositoryCandidate::new(&path)]),
    };
    let git = TestGitProbe {
        layouts: HashMap::from([(
            path.clone(),
            Ok(GitRepositoryLayout::worktree(
                &path,
                &common_dir,
                &common_dir,
            )),
        )]),
    };
    let filesystem = TestFilesystemIdentity {
        identities: HashMap::from([
            (common_dir.clone(), FilesystemObjectId::new(1, 200)),
            (path.clone(), FilesystemObjectId::new(1, 100)),
        ]),
    };
    let mut catalog = TestCatalog::default();
    catalog
        .repositories
        .insert(old_repository.id().clone(), old_repository);
    catalog
        .locations
        .insert(old_location_id.clone(), old_location.clone());
    catalog
        .locations_by_path
        .insert(path.clone(), old_location_id);
    catalog
        .repositories_by_common_dir
        .insert(common_dir, RepositoryId::from("repository-old"));
    let mut identities = TestIdentityGenerator::default();

    DiscoverRepositories::new(&discovery, &git, &filesystem, &mut catalog, &mut identities)
        .execute(&DiscoveryRequest::new("/workspace"))
        .expect("replacement repository must reconcile independently");

    let reconciliation = catalog
        .reconciliation
        .expect("catalog must receive the reconciliation");
    assert_eq!(reconciliation.repositories().len(), 1);
    assert_eq!(reconciliation.locations().len(), 1);
    assert_ne!(
        reconciliation.repositories()[0].id().as_str(),
        "repository-old"
    );
    assert_ne!(reconciliation.locations()[0].id().as_str(), "location-old");
}

#[test]
fn path_only_evidence_does_not_claim_rename_continuity() {
    let old_path = PathBuf::from("/workspace/old");
    let new_path = PathBuf::from("/workspace/new");
    let old_common_dir = old_path.join(".git");
    let new_common_dir = new_path.join(".git");
    let old_repository_id = RepositoryId::from("repository-old");
    let old_location_id = LocationId::from("location-old");
    let old_repository = Repository::new(old_repository_id.clone());
    let old_location = Location::new(
        old_location_id.clone(),
        old_repository_id,
        &old_path,
        bulls_core::LocationAvailability::Available,
    );
    let discovery = TestDiscovery {
        outcome: DiscoveryOutcome::complete(
            "/workspace",
            vec![RepositoryCandidate::new(&new_path)],
        ),
    };
    let git = TestGitProbe {
        layouts: HashMap::from([(
            new_path.clone(),
            Ok(GitRepositoryLayout::worktree(
                &new_path,
                &new_common_dir,
                &new_common_dir,
            )),
        )]),
    };
    let filesystem = TestFilesystemIdentity {
        identities: HashMap::new(),
    };
    let mut catalog = TestCatalog::default();
    catalog
        .repositories
        .insert(old_repository.id().clone(), old_repository);
    catalog
        .locations
        .insert(old_location_id.clone(), old_location.clone());
    catalog
        .locations_by_path
        .insert(old_path.clone(), old_location_id);
    catalog
        .repositories_by_common_dir
        .insert(old_common_dir, RepositoryId::from("repository-old"));
    let mut identities = TestIdentityGenerator::default();

    DiscoverRepositories::new(&discovery, &git, &filesystem, &mut catalog, &mut identities)
        .execute(&DiscoveryRequest::new("/workspace"))
        .expect("path-only evidence must reconcile without claiming a rename");

    let reconciliation = catalog
        .reconciliation
        .expect("catalog must receive the reconciliation");
    assert_eq!(reconciliation.repositories().len(), 1);
    assert_eq!(reconciliation.locations().len(), 1);
    assert_ne!(
        reconciliation.repositories()[0].id().as_str(),
        "repository-old"
    );
    assert_ne!(reconciliation.locations()[0].id().as_str(), "location-old");
}

#[test]
fn identification_failure_makes_catalog_coverage_partial() {
    let path = PathBuf::from("/workspace/unreadable");
    let discovery = TestDiscovery {
        outcome: DiscoveryOutcome::complete("/workspace", vec![RepositoryCandidate::new(&path)]),
    };
    let git = TestGitProbe {
        layouts: HashMap::from([(
            path.clone(),
            Err(PortError::new(
                bulls_application::ports::PortErrorKind::InvalidData,
            )),
        )]),
    };
    let filesystem = TestFilesystemIdentity {
        identities: HashMap::new(),
    };
    let mut catalog = TestCatalog::default();
    let mut identities = TestIdentityGenerator::default();

    let outcome =
        DiscoverRepositories::new(&discovery, &git, &filesystem, &mut catalog, &mut identities)
            .execute(&DiscoveryRequest::new("/workspace"))
            .expect("candidate failure must remain a partial reconciliation");

    assert_eq!(outcome.coverage(), CatalogReconciliationCoverage::Partial);
    assert_eq!(outcome.identification_issues().len(), 1);
    assert_eq!(outcome.identification_issues()[0].path(), path.as_path());
    assert_eq!(outcome.repository_count(), 0);
    assert_eq!(
        catalog
            .reconciliation
            .expect("catalog must receive a partial reconciliation")
            .coverage(),
        CatalogReconciliationCoverage::Partial
    );
}

#[test]
fn cancellation_during_repository_probe_aborts_without_reconciling_partial_state() {
    let path = PathBuf::from("/workspace/repository");
    let discovery = TestDiscovery {
        outcome: DiscoveryOutcome::complete("/workspace", vec![RepositoryCandidate::new(&path)]),
    };
    let git = TestGitProbe {
        layouts: HashMap::from([(path, Err(PortError::new(PortErrorKind::Cancelled)))]),
    };
    let filesystem = TestFilesystemIdentity {
        identities: HashMap::new(),
    };
    let mut catalog = TestCatalog::default();
    let mut identities = TestIdentityGenerator::default();

    let error =
        DiscoverRepositories::new(&discovery, &git, &filesystem, &mut catalog, &mut identities)
            .execute(&DiscoveryRequest::new("/workspace"))
            .expect_err("cancellation must abort discovery instead of becoming an issue");

    assert_eq!(error.kind(), PortErrorKind::Cancelled);
    assert!(catalog.reconciliation.is_none());
}

#[test]
fn unavailable_discovery_root_is_recorded_as_offline_before_failure_returns() {
    let discovery = FailingDiscovery(PortErrorKind::ResourceUnavailable);
    let git = TestGitProbe {
        layouts: HashMap::new(),
    };
    let filesystem = TestFilesystemIdentity {
        identities: HashMap::new(),
    };
    let mut catalog = TestCatalog::default();
    let mut identities = TestIdentityGenerator::default();

    let error =
        DiscoverRepositories::new(&discovery, &git, &filesystem, &mut catalog, &mut identities)
            .execute(&DiscoveryRequest::new("/workspace"))
            .expect_err("unavailable discovery root must remain an operational failure");

    assert_eq!(error.kind(), PortErrorKind::ResourceUnavailable);
    assert_eq!(catalog.offline_roots, [PathBuf::from("/workspace")]);
    assert!(catalog.reconciliation.is_none());
}
