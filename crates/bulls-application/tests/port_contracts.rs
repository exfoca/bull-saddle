// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bulls_application::ports::{
    CatalogMissingScope, CatalogReconciliation, CatalogReconciliationCoverage, ClockPort,
    ConfigurationPort, DiscoveryCompleteness, DiscoveryIssue, DiscoveryIssueKind, DiscoveryOutcome,
    DiscoveryRequest, FilesystemIdentityEvidence, FilesystemIdentityPort, FilesystemObjectId,
    GitRepositoryLayout, GitRepositoryProbePort, IdentityGeneratorPort, LocationIdentityEvidence,
    ObservationStorePort, PlatformPaths, PlatformPathsPort, PortResult, RepositoryCandidate,
    RepositoryCatalogPort, RepositoryDiscoveryPort, RepositoryIdentityEvidence,
};
use bulls_core::{
    Freshness, Location, LocationAvailability, LocationId, Observation, ObservationAttempt,
    ObservationCoverage, ObservationKey, ObservationMetadata, ObservationRunId, Repository,
    RepositoryId, Worktree, WorktreeGitState, WorktreeId,
};

#[derive(Default)]
struct TestCatalog {
    reconciliation: Option<CatalogReconciliation>,
}

impl RepositoryCatalogPort for TestCatalog {
    fn repositories(&self) -> PortResult<Vec<Repository>> {
        Ok(Vec::new())
    }

    fn repository(&self, _repository_id: &RepositoryId) -> PortResult<Option<Repository>> {
        Ok(None)
    }

    fn location(&self, _location_id: &LocationId) -> PortResult<Option<Location>> {
        Ok(None)
    }

    fn location_by_path(&self, _path: &Path) -> PortResult<Option<Location>> {
        Ok(None)
    }

    fn repository_id_by_common_dir(&self, _common_dir: &Path) -> PortResult<Option<RepositoryId>> {
        Ok(None)
    }

    fn repository_id_by_common_dir_object(
        &self,
        _object_id: FilesystemObjectId,
    ) -> PortResult<Option<RepositoryId>> {
        Ok(None)
    }

    fn location_id_by_filesystem_object(
        &self,
        _object_id: FilesystemObjectId,
    ) -> PortResult<Option<LocationId>> {
        Ok(None)
    }

    fn locations_for_repository(&self, _repository_id: &RepositoryId) -> PortResult<Vec<Location>> {
        Ok(Vec::new())
    }

    fn worktrees_for_repository(&self, _repository_id: &RepositoryId) -> PortResult<Vec<Worktree>> {
        Ok(Vec::new())
    }

    fn mark_discovery_root_offline(&mut self, _root: &Path) -> PortResult<()> {
        Ok(())
    }

    fn reconcile(&mut self, reconciliation: &CatalogReconciliation) -> PortResult<()> {
        self.reconciliation = Some(reconciliation.clone());
        Ok(())
    }
}

struct TestConfiguration {
    value: Option<u8>,
}

impl ConfigurationPort for TestConfiguration {
    type Configuration = u8;

    fn load(&self) -> PortResult<Option<Self::Configuration>> {
        Ok(self.value)
    }

    fn save(&mut self, configuration: &Self::Configuration) -> PortResult<()> {
        self.value = Some(*configuration);
        Ok(())
    }
}

struct TestObservationStore {
    latest_observation: Option<Observation<WorktreeGitState>>,
    latest_attempt: Option<ObservationAttempt<WorktreeGitState>>,
}

impl ObservationStorePort for TestObservationStore {
    type Value = WorktreeGitState;

    fn latest_observation(
        &self,
        _key: &ObservationKey,
    ) -> PortResult<Option<Observation<Self::Value>>> {
        Ok(self.latest_observation.clone())
    }

    fn latest_attempt(
        &self,
        _key: &ObservationKey,
    ) -> PortResult<Option<ObservationAttempt<Self::Value>>> {
        Ok(self.latest_attempt.clone())
    }

    fn save_attempt(&mut self, attempt: &ObservationAttempt<Self::Value>) -> PortResult<()> {
        self.latest_attempt = Some(attempt.clone());
        if let ObservationAttempt::Succeeded(observation) = attempt {
            self.latest_observation = Some(observation.clone());
        }
        Ok(())
    }
}

struct TestPaths;

impl PlatformPathsPort for TestPaths {
    fn user_paths(&self) -> PortResult<PlatformPaths> {
        Ok(PlatformPaths::new(
            PathBuf::from("config"),
            PathBuf::from("data"),
            PathBuf::from("state"),
            PathBuf::from("cache"),
        ))
    }
}

struct TestDiscovery {
    outcome: DiscoveryOutcome,
}

impl RepositoryDiscoveryPort for TestDiscovery {
    fn discover(&self, _request: &DiscoveryRequest) -> PortResult<DiscoveryOutcome> {
        Ok(self.outcome.clone())
    }
}

struct TestGitRepositoryProbe {
    layout: GitRepositoryLayout,
}

impl GitRepositoryProbePort for TestGitRepositoryProbe {
    fn probe(&self, _candidate: &RepositoryCandidate) -> PortResult<GitRepositoryLayout> {
        Ok(self.layout.clone())
    }
}

struct TestFilesystemIdentity(FilesystemObjectId);

impl FilesystemIdentityPort for TestFilesystemIdentity {
    fn identity(&self, _path: &Path) -> PortResult<FilesystemIdentityEvidence> {
        Ok(FilesystemIdentityEvidence::strong(self.0))
    }
}

#[derive(Default)]
struct TestIdentityGenerator {
    sequence: u64,
}

impl IdentityGeneratorPort for TestIdentityGenerator {
    fn next_repository_id(&mut self) -> PortResult<RepositoryId> {
        self.sequence += 1;
        Ok(RepositoryId::from(format!("repository-{}", self.sequence)))
    }

    fn next_location_id(&mut self) -> PortResult<LocationId> {
        self.sequence += 1;
        Ok(LocationId::from(format!("location-{}", self.sequence)))
    }

    fn next_worktree_id(&mut self) -> PortResult<WorktreeId> {
        self.sequence += 1;
        Ok(WorktreeId::from(format!("worktree-{}", self.sequence)))
    }

    fn next_observation_run_id(&mut self) -> PortResult<ObservationRunId> {
        self.sequence += 1;
        Ok(ObservationRunId::from(format!(
            "observation-run-{}",
            self.sequence
        )))
    }
}

struct TestClock(SystemTime);

impl ClockPort for TestClock {
    fn now(&self) -> SystemTime {
        self.0
    }
}

#[test]
fn state_and_environment_ports_are_implementation_independent() {
    let repository = Repository::new(RepositoryId::from("repository-1"));
    let location = Location::new(
        LocationId::from("location-1"),
        repository.id().clone(),
        "/workspace/repository",
        LocationAvailability::Available,
    );
    let worktree = Worktree::new(
        WorktreeId::from("worktree-1"),
        repository.id().clone(),
        location.id().clone(),
    );
    let filesystem_object_id = FilesystemObjectId::new(7, 11);
    let repository_identity = RepositoryIdentityEvidence::new(
        repository.id().clone(),
        "/workspace/repository/.git",
        Some(filesystem_object_id),
    );
    let location_identity =
        LocationIdentityEvidence::new(location.id().clone(), Some(filesystem_object_id));
    let reconciliation = CatalogReconciliation::new(
        "/workspace",
        CatalogReconciliationCoverage::Complete,
        CatalogMissingScope::Filesystem(7),
    )
    .with_entities(vec![repository], vec![location], vec![worktree])
    .with_identity_evidence(
        vec![repository_identity.clone()],
        vec![location_identity.clone()],
    );
    let mut catalog = TestCatalog::default();
    catalog
        .reconcile(&reconciliation)
        .expect("catalog reconciliation must succeed atomically");
    assert_eq!(catalog.reconciliation, Some(reconciliation.clone()));
    assert_eq!(reconciliation.discovery_root(), Path::new("/workspace"));
    assert_eq!(
        reconciliation.coverage(),
        CatalogReconciliationCoverage::Complete
    );
    assert_eq!(
        reconciliation.missing_scope(),
        CatalogMissingScope::Filesystem(7)
    );
    assert_eq!(
        reconciliation.repository_identities(),
        &[repository_identity]
    );
    assert_eq!(reconciliation.location_identities(), &[location_identity]);

    let filesystem_identity = TestFilesystemIdentity(filesystem_object_id);
    assert_eq!(
        filesystem_identity
            .identity(Path::new("/workspace/repository"))
            .expect("filesystem identity lookup must succeed")
            .object_id(),
        Some(filesystem_object_id)
    );
    assert_eq!(filesystem_object_id.filesystem_id(), 7);
    assert_eq!(filesystem_object_id.object_id(), 11);

    let mut identity_generator = TestIdentityGenerator::default();
    assert_eq!(
        identity_generator
            .next_repository_id()
            .expect("repository identity generation must succeed")
            .as_str(),
        "repository-1"
    );
    assert_eq!(
        identity_generator
            .next_location_id()
            .expect("location identity generation must succeed")
            .as_str(),
        "location-2"
    );
    assert_eq!(
        identity_generator
            .next_worktree_id()
            .expect("worktree identity generation must succeed")
            .as_str(),
        "worktree-3"
    );

    let mut configuration = TestConfiguration { value: None };
    assert_eq!(
        configuration
            .load()
            .expect("configuration read must succeed"),
        None
    );
    configuration
        .save(&7)
        .expect("configuration write must succeed");
    assert_eq!(
        configuration
            .load()
            .expect("configuration read must succeed"),
        Some(7)
    );

    let observation_key = ObservationKey::worktree_git_state(WorktreeId::from("worktree-1"));
    let observation = Observation::new(
        ObservationMetadata::new(
            ObservationRunId::from("observation-run-1"),
            observation_key.clone(),
            Freshness::new(UNIX_EPOCH + Duration::from_secs(42)),
            ObservationCoverage::Complete,
        ),
        WorktreeGitState::detached(),
    );
    let attempt = ObservationAttempt::Succeeded(observation.clone());
    let mut store = TestObservationStore {
        latest_observation: None,
        latest_attempt: None,
    };
    store
        .save_attempt(&attempt)
        .expect("observation attempt write must succeed");
    assert_eq!(
        store
            .latest_observation(&observation_key)
            .expect("latest observation read must succeed"),
        Some(observation)
    );
    assert_eq!(
        store
            .latest_attempt(&observation_key)
            .expect("latest observation attempt read must succeed"),
        Some(attempt)
    );

    let paths = TestPaths.user_paths().expect("platform paths must resolve");
    assert_eq!(paths.config_dir(), Path::new("config"));
    assert_eq!(paths.data_dir(), Path::new("data"));
    assert_eq!(paths.state_dir(), Path::new("state"));
    assert_eq!(paths.cache_dir(), Path::new("cache"));

    let now = UNIX_EPOCH + Duration::from_secs(99);
    assert_eq!(TestClock(now).now(), now);
}

#[test]
fn discovery_reports_partial_coverage_without_discarding_candidates() {
    let request = DiscoveryRequest::new(PathBuf::from("/workspace"));
    let candidate = RepositoryCandidate::new(PathBuf::from("/workspace/repository"));
    let issue = DiscoveryIssue::new(
        PathBuf::from("/workspace/private"),
        DiscoveryIssueKind::PermissionDenied,
    );
    let discovery = TestDiscovery {
        outcome: DiscoveryOutcome::new("/workspace", vec![candidate.clone()], vec![issue.clone()]),
    };

    let outcome = discovery
        .discover(&request)
        .expect("partial discovery must preserve usable results");

    assert_eq!(request.root(), Path::new("/workspace"));
    assert_eq!(outcome.root(), Path::new("/workspace"));
    assert_eq!(outcome.candidates(), std::slice::from_ref(&candidate));
    assert_eq!(candidate.path(), Path::new("/workspace/repository"));
    assert_eq!(outcome.issues(), std::slice::from_ref(&issue));
    assert_eq!(issue.path(), Path::new("/workspace/private"));
    assert_eq!(issue.kind(), DiscoveryIssueKind::PermissionDenied);
    assert_eq!(outcome.completeness(), DiscoveryCompleteness::Partial);
    assert_eq!(
        DiscoveryOutcome::complete("/workspace", vec![candidate]).completeness(),
        DiscoveryCompleteness::Complete
    );
}

#[test]
fn git_repository_probe_exposes_topology_without_observation_state() {
    let candidate = RepositoryCandidate::new(PathBuf::from("/workspace/linked"));
    let layout = GitRepositoryLayout::worktree(
        "/workspace/linked",
        "/workspace/main/.git/worktrees/linked",
        "/workspace/main/.git",
    );
    let probe = TestGitRepositoryProbe {
        layout: layout.clone(),
    };

    let actual = probe
        .probe(&candidate)
        .expect("Git topology probe must return repository layout evidence");

    assert_eq!(actual, layout);
    assert_eq!(actual.worktree_path(), Some(Path::new("/workspace/linked")));
    assert_eq!(
        actual.git_dir(),
        Path::new("/workspace/main/.git/worktrees/linked")
    );
    assert_eq!(actual.common_dir(), Path::new("/workspace/main/.git"));
    assert!(!actual.is_bare());

    let bare = GitRepositoryLayout::bare("/srv/git/project.git", "/srv/git/project.git");
    assert!(bare.is_bare());
    assert_eq!(bare.worktree_path(), None);
}
