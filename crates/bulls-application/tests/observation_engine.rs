// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bulls_application::ports::{
    CancellationPort, CatalogReconciliation, ClockPort, FilesystemObjectId, GitObservationPort,
    IdentityGeneratorPort, ObservationStorePort, PortError, PortErrorKind, PortResult,
    RepositoryCatalogPort,
};
use bulls_application::{ObservationExecutionPolicy, ObserveInventory, OperationCompletion};
use bulls_core::{
    Location, LocationAvailability, LocationId, Observation, ObservationAttempt,
    ObservationCoverage, ObservationFailureKind, ObservationKey, ObservationRunId, Remote,
    Repository, RepositoryId, Worktree, WorktreeGitState, WorktreeId,
};

struct TestCatalog {
    repositories: Vec<Repository>,
    locations: Vec<Location>,
    worktrees: Vec<Worktree>,
}

impl RepositoryCatalogPort for TestCatalog {
    fn repositories(&self) -> PortResult<Vec<Repository>> {
        Ok(self.repositories.clone())
    }

    fn repository(&self, repository_id: &RepositoryId) -> PortResult<Option<Repository>> {
        Ok(self
            .repositories
            .iter()
            .find(|repository| repository.id() == repository_id)
            .cloned())
    }

    fn location(&self, location_id: &LocationId) -> PortResult<Option<Location>> {
        Ok(self
            .locations
            .iter()
            .find(|location| location.id() == location_id)
            .cloned())
    }

    fn location_by_path(&self, path: &Path) -> PortResult<Option<Location>> {
        Ok(self
            .locations
            .iter()
            .find(|location| location.path() == path)
            .cloned())
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

    fn locations_for_repository(&self, repository_id: &RepositoryId) -> PortResult<Vec<Location>> {
        Ok(self
            .locations
            .iter()
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

    fn mark_discovery_root_offline(&mut self, _root: &Path) -> PortResult<()> {
        Ok(())
    }

    fn reconcile(&mut self, _reconciliation: &CatalogReconciliation) -> PortResult<()> {
        Ok(())
    }
}

struct TestGitObservation {
    worktree_results: HashMap<PathBuf, PortResult<WorktreeGitState>>,
    repository_result: PortResult<Vec<Remote>>,
    default_delay: Duration,
    worktree_delays: HashMap<PathBuf, Duration>,
    calls: AtomicUsize,
    active: AtomicUsize,
    max_active: AtomicUsize,
}

impl TestGitObservation {
    fn successful(delay: Duration) -> Self {
        Self {
            worktree_results: HashMap::new(),
            repository_result: Ok(vec![Remote::new("origin")]),
            default_delay: delay,
            worktree_delays: HashMap::new(),
            calls: AtomicUsize::new(0),
            active: AtomicUsize::new(0),
            max_active: AtomicUsize::new(0),
        }
    }

    fn with_worktree_result(
        mut self,
        path: impl Into<PathBuf>,
        result: PortResult<WorktreeGitState>,
    ) -> Self {
        self.worktree_results.insert(path.into(), result);
        self
    }

    fn with_worktree_delay(mut self, path: impl Into<PathBuf>, delay: Duration) -> Self {
        self.worktree_delays.insert(path.into(), delay);
        self
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::Acquire)
    }

    fn max_active(&self) -> usize {
        self.max_active.load(Ordering::Acquire)
    }

    fn execute<T>(&self, result: PortResult<T>, delay: Duration) -> PortResult<T> {
        self.calls.fetch_add(1, Ordering::AcqRel);
        let active = self.active.fetch_add(1, Ordering::AcqRel) + 1;
        self.max_active.fetch_max(active, Ordering::AcqRel);
        thread::sleep(delay);
        self.active.fetch_sub(1, Ordering::AcqRel);
        result
    }
}

impl GitObservationPort for TestGitObservation {
    fn observe_worktree(&self, worktree_path: &Path) -> PortResult<WorktreeGitState> {
        let result = self
            .worktree_results
            .get(worktree_path)
            .cloned()
            .unwrap_or_else(|| Ok(WorktreeGitState::detached()));
        let delay = self
            .worktree_delays
            .get(worktree_path)
            .copied()
            .unwrap_or(self.default_delay);
        self.execute(result, delay)
    }

    fn observe_repository_remotes(&self, _repository_path: &Path) -> PortResult<Vec<Remote>> {
        self.execute(self.repository_result.clone(), self.default_delay)
    }
}

struct TestClock(SystemTime);

impl ClockPort for TestClock {
    fn now(&self) -> SystemTime {
        self.0
    }
}

struct TestCancellation(AtomicBool);

impl TestCancellation {
    fn active() -> Self {
        Self(AtomicBool::new(false))
    }

    fn cancelled() -> Self {
        Self(AtomicBool::new(true))
    }
}

impl CancellationPort for TestCancellation {
    fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

#[derive(Default)]
struct TestIdentityGenerator;

impl IdentityGeneratorPort for TestIdentityGenerator {
    fn next_repository_id(&mut self) -> PortResult<RepositoryId> {
        Ok(RepositoryId::from("unused-repository"))
    }

    fn next_location_id(&mut self) -> PortResult<LocationId> {
        Ok(LocationId::from("unused-location"))
    }

    fn next_worktree_id(&mut self) -> PortResult<WorktreeId> {
        Ok(WorktreeId::from("unused-worktree"))
    }

    fn next_observation_run_id(&mut self) -> PortResult<ObservationRunId> {
        Ok(ObservationRunId::from("observation-run-1"))
    }
}

struct TestObservationStore<T> {
    attempts: Vec<ObservationAttempt<T>>,
}

impl<T> Default for TestObservationStore<T> {
    fn default() -> Self {
        Self {
            attempts: Vec::new(),
        }
    }
}

impl<T> ObservationStorePort for TestObservationStore<T>
where
    T: Clone,
{
    type Value = T;

    fn latest_observation(
        &self,
        key: &ObservationKey,
    ) -> PortResult<Option<Observation<Self::Value>>> {
        Ok(self
            .attempts
            .iter()
            .rev()
            .find_map(|attempt| match attempt {
                ObservationAttempt::Succeeded(observation)
                    if observation.metadata().key() == key =>
                {
                    Some(observation.clone())
                }
                _ => None,
            }))
    }

    fn latest_attempt(
        &self,
        key: &ObservationKey,
    ) -> PortResult<Option<ObservationAttempt<Self::Value>>> {
        Ok(self
            .attempts
            .iter()
            .rev()
            .find(|attempt| attempt.metadata().key() == key)
            .cloned())
    }

    fn save_attempt(&mut self, attempt: &ObservationAttempt<Self::Value>) -> PortResult<()> {
        self.attempts.push(attempt.clone());
        Ok(())
    }
}

fn catalog(worktree_count: usize, availability: LocationAvailability) -> TestCatalog {
    let repository_id = RepositoryId::from("repository-1");
    let repository = Repository::new(repository_id.clone());
    let mut locations = Vec::new();
    let mut worktrees = Vec::new();

    for index in 0..worktree_count.max(1) {
        let location_id = LocationId::from(format!("location-{index:02}"));
        locations.push(Location::new(
            location_id.clone(),
            repository_id.clone(),
            format!("/workspace/worktree-{index:02}"),
            availability,
        ));
        if index < worktree_count {
            worktrees.push(Worktree::new(
                WorktreeId::from(format!("worktree-{index:02}")),
                repository_id.clone(),
                location_id,
            ));
        }
    }

    TestCatalog {
        repositories: vec![repository],
        locations,
        worktrees,
    }
}

#[test]
fn observation_policy_rejects_zero_parallelism() {
    assert!(ObservationExecutionPolicy::new(0).is_none());
    assert_eq!(
        ObservationExecutionPolicy::new(3)
            .expect("positive parallelism must be accepted")
            .max_parallelism(),
        3
    );
}

#[test]
fn observation_limits_parallelism_and_persists_every_success() {
    let catalog = catalog(8, LocationAvailability::Available);
    let git = TestGitObservation::successful(Duration::from_millis(10));
    let clock = TestClock(UNIX_EPOCH + Duration::from_secs(42));
    let cancellation = TestCancellation::active();
    let mut identities = TestIdentityGenerator;
    let mut worktree_store = TestObservationStore::default();
    let mut repository_store = TestObservationStore::default();
    let mut observer = ObserveInventory::new(
        &catalog,
        &git,
        &clock,
        &cancellation,
        &mut identities,
        &mut worktree_store,
        &mut repository_store,
    );

    let outcome = observer
        .execute(ObservationExecutionPolicy::new(2).expect("parallelism must be valid"))
        .expect("inventory observation must succeed");

    assert_eq!(outcome.run_id().as_str(), "observation-run-1");
    assert_eq!(outcome.coverage(), ObservationCoverage::Complete);
    assert!(!outcome.is_partial());
    assert_eq!(outcome.target_count(), 9);
    assert_eq!(outcome.succeeded_count(), 9);
    assert_eq!(outcome.failed_count(), 0);
    assert_eq!(outcome.cancelled_count(), 0);
    assert!(outcome.failures().is_empty());
    assert_eq!(outcome.completion(), OperationCompletion::Complete);
    assert!(!outcome.was_cancelled());
    assert_eq!(git.calls(), 9);
    assert!(git.max_active() <= 2);
    assert_eq!(worktree_store.attempts.len(), 8);
    assert_eq!(repository_store.attempts.len(), 1);
    assert!(worktree_store.attempts.iter().all(|attempt| {
        attempt.metadata().run_id().as_str() == "observation-run-1"
            && attempt.metadata().coverage() == ObservationCoverage::Complete
            && attempt.metadata().freshness().observed_at() == UNIX_EPOCH + Duration::from_secs(42)
    }));
}

#[test]
fn target_failures_are_isolated_typed_and_persisted_with_partial_coverage() {
    let catalog = catalog(3, LocationAvailability::Available);
    let git = TestGitObservation::successful(Duration::ZERO)
        .with_worktree_result(
            "/workspace/worktree-01",
            Err(PortError::new(PortErrorKind::TimedOut)),
        )
        .with_worktree_result(
            "/workspace/worktree-02",
            Err(PortError::new(PortErrorKind::ResourceUnavailable)),
        );
    let clock = TestClock(UNIX_EPOCH);
    let cancellation = TestCancellation::active();
    let mut identities = TestIdentityGenerator;
    let mut worktree_store = TestObservationStore::default();
    let mut repository_store = TestObservationStore::default();
    let mut observer = ObserveInventory::new(
        &catalog,
        &git,
        &clock,
        &cancellation,
        &mut identities,
        &mut worktree_store,
        &mut repository_store,
    );

    let outcome = observer
        .execute(ObservationExecutionPolicy::default())
        .expect("individual target failures must not abort the batch");

    assert_eq!(outcome.coverage(), ObservationCoverage::Partial);
    assert!(outcome.is_partial());
    assert_eq!(outcome.target_count(), 4);
    assert_eq!(outcome.succeeded_count(), 2);
    assert_eq!(outcome.failed_count(), 2);
    assert_eq!(outcome.cancelled_count(), 0);
    assert_eq!(outcome.completion(), OperationCompletion::Partial);
    assert!(!outcome.was_cancelled());
    assert_eq!(outcome.failures().len(), 2);
    assert_eq!(
        outcome.failures()[0].key(),
        &ObservationKey::worktree_git_state(WorktreeId::from("worktree-01"))
    );
    assert_eq!(
        outcome.failures()[0].kind(),
        ObservationFailureKind::TimedOut
    );
    assert_eq!(
        outcome.failures()[1].key(),
        &ObservationKey::worktree_git_state(WorktreeId::from("worktree-02"))
    );
    assert_eq!(
        outcome.failures()[1].kind(),
        ObservationFailureKind::SubjectDisappeared
    );
    assert_eq!(worktree_store.attempts.len(), 3);
    assert!(matches!(
        worktree_store
            .latest_attempt(&ObservationKey::worktree_git_state(WorktreeId::from(
                "worktree-01"
            )))
            .expect("latest attempt lookup must succeed"),
        Some(ObservationAttempt::Failed(ref failure))
            if failure.kind() == ObservationFailureKind::TimedOut
                && failure.metadata().coverage() == ObservationCoverage::Partial
    ));
    assert!(matches!(
        worktree_store
            .latest_attempt(&ObservationKey::worktree_git_state(WorktreeId::from(
                "worktree-02"
            )))
            .expect("latest attempt lookup must succeed"),
        Some(ObservationAttempt::Failed(ref failure))
            if failure.kind() == ObservationFailureKind::SubjectDisappeared
    ));
}

#[test]
fn unavailable_catalog_subjects_do_not_start_git_processes() {
    let catalog = catalog(1, LocationAvailability::Offline);
    let git = TestGitObservation::successful(Duration::ZERO);
    let clock = TestClock(UNIX_EPOCH);
    let cancellation = TestCancellation::active();
    let mut identities = TestIdentityGenerator;
    let mut worktree_store = TestObservationStore::default();
    let mut repository_store = TestObservationStore::default();
    let mut observer = ObserveInventory::new(
        &catalog,
        &git,
        &clock,
        &cancellation,
        &mut identities,
        &mut worktree_store,
        &mut repository_store,
    );

    let outcome = observer
        .execute(ObservationExecutionPolicy::default())
        .expect("unavailable subjects must be recorded as observation failures");

    assert_eq!(git.calls(), 0);
    assert_eq!(outcome.target_count(), 2);
    assert_eq!(outcome.failed_count(), 2);
    assert_eq!(outcome.completion(), OperationCompletion::Failed);
    assert_eq!(outcome.failures().len(), 2);
    assert!(matches!(
        &repository_store.attempts[0],
        ObservationAttempt::Failed(failure)
            if failure.kind() == ObservationFailureKind::SubjectUnavailable
    ));
    assert!(matches!(
        &worktree_store.attempts[0],
        ObservationAttempt::Failed(failure)
            if failure.kind() == ObservationFailureKind::SubjectUnavailable
    ));
}

#[test]
fn cancellation_prevents_new_git_observations_and_is_persisted_per_target() {
    let catalog = catalog(3, LocationAvailability::Available);
    let git = TestGitObservation::successful(Duration::ZERO);
    let clock = TestClock(UNIX_EPOCH);
    let cancellation = TestCancellation::cancelled();
    let mut identities = TestIdentityGenerator;
    let mut worktree_store = TestObservationStore::default();
    let mut repository_store = TestObservationStore::default();
    let mut observer = ObserveInventory::new(
        &catalog,
        &git,
        &clock,
        &cancellation,
        &mut identities,
        &mut worktree_store,
        &mut repository_store,
    );

    let outcome = observer
        .execute(ObservationExecutionPolicy::new(2).expect("parallelism must be valid"))
        .expect("cancellation is an observation outcome, not a batch defect");

    assert_eq!(git.calls(), 0);
    assert_eq!(outcome.target_count(), 4);
    assert_eq!(outcome.succeeded_count(), 0);
    assert_eq!(outcome.failed_count(), 4);
    assert_eq!(outcome.cancelled_count(), 4);
    assert_eq!(outcome.completion(), OperationCompletion::Cancelled);
    assert!(outcome.was_cancelled());
    assert_eq!(outcome.failures().len(), 4);
    assert!(
        outcome
            .failures()
            .iter()
            .all(|failure| failure.kind() == ObservationFailureKind::Cancelled)
    );
    assert!(worktree_store.attempts.iter().all(|attempt| matches!(
        attempt,
        ObservationAttempt::Failed(failure)
            if failure.kind() == ObservationFailureKind::Cancelled
    )));
    assert!(matches!(
        &repository_store.attempts[0],
        ObservationAttempt::Failed(failure)
            if failure.kind() == ObservationFailureKind::Cancelled
    ));
}

#[test]
fn observation_scale_guard_tracks_fanout_throughput_and_a_slow_target() {
    const WORKTREE_COUNT: usize = 128;
    const PARALLELISM: usize = 4;

    let catalog = catalog(WORKTREE_COUNT, LocationAvailability::Available);
    let git = TestGitObservation::successful(Duration::from_millis(20))
        .with_worktree_delay("/workspace/worktree-00", Duration::from_millis(500));
    let clock = TestClock(UNIX_EPOCH);
    let cancellation = TestCancellation::active();
    let mut identities = TestIdentityGenerator;
    let mut worktree_store = TestObservationStore::default();
    let mut repository_store = TestObservationStore::default();
    let mut observer = ObserveInventory::new(
        &catalog,
        &git,
        &clock,
        &cancellation,
        &mut identities,
        &mut worktree_store,
        &mut repository_store,
    );

    let started_at = Instant::now();
    let outcome = observer
        .execute(
            ObservationExecutionPolicy::new(PARALLELISM)
                .expect("scale guard parallelism must be valid"),
        )
        .expect("scale guard observation must complete");
    let elapsed = started_at.elapsed();
    let serialized_floor = Duration::from_millis(
        500 + u64::try_from(WORKTREE_COUNT).expect("worktree count must fit into u64") * 20,
    );
    let observed_throughput = outcome.target_count() as f64 / elapsed.as_secs_f64();
    let serialized_throughput = outcome.target_count() as f64 / serialized_floor.as_secs_f64();

    assert_eq!(outcome.target_count(), WORKTREE_COUNT + 1);
    assert_eq!(outcome.succeeded_count(), WORKTREE_COUNT + 1);
    assert_eq!(git.calls(), WORKTREE_COUNT + 1);
    assert_eq!(git.max_active(), PARALLELISM);
    assert!(elapsed < serialized_floor);
    assert!(observed_throughput > serialized_throughput);
}
