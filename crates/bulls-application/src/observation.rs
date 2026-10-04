// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Mutex;
use std::thread;

use bulls_core::{
    Freshness, LocationAvailability, Observation, ObservationAttempt, ObservationCoverage,
    ObservationFailure, ObservationFailureKind, ObservationKey, ObservationMetadata,
    ObservationRunId, Remote, RepositoryId, WorktreeGitState, WorktreeId,
};

use crate::OperationCompletion;
use crate::ports::{
    CancellationPort, ClockPort, GitObservationPort, IdentityGeneratorPort, ObservationStorePort,
    PortError, PortErrorKind, PortResult, RepositoryCatalogPort,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObservationExecutionPolicy {
    max_parallelism: usize,
}

impl ObservationExecutionPolicy {
    pub fn new(max_parallelism: usize) -> Option<Self> {
        (max_parallelism > 0).then_some(Self { max_parallelism })
    }

    pub const fn max_parallelism(self) -> usize {
        self.max_parallelism
    }
}

impl Default for ObservationExecutionPolicy {
    fn default() -> Self {
        Self { max_parallelism: 4 }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservationTargetFailure {
    key: ObservationKey,
    kind: ObservationFailureKind,
}

impl ObservationTargetFailure {
    #[cfg(test)]
    pub(crate) fn new(key: ObservationKey, kind: ObservationFailureKind) -> Self {
        Self { key, kind }
    }

    fn from_failure(failure: &ObservationFailure) -> Self {
        Self {
            key: failure.metadata().key().clone(),
            kind: failure.kind(),
        }
    }

    pub fn key(&self) -> &ObservationKey {
        &self.key
    }

    pub const fn kind(&self) -> ObservationFailureKind {
        self.kind
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObserveInventoryOutcome {
    run_id: ObservationRunId,
    succeeded_count: usize,
    failures: Vec<ObservationTargetFailure>,
}

impl ObserveInventoryOutcome {
    pub(crate) fn new(
        run_id: ObservationRunId,
        succeeded_count: usize,
        failures: Vec<ObservationTargetFailure>,
    ) -> Self {
        Self {
            run_id,
            succeeded_count,
            failures,
        }
    }

    pub fn run_id(&self) -> &ObservationRunId {
        &self.run_id
    }

    pub fn coverage(&self) -> ObservationCoverage {
        if self.failures.is_empty() {
            ObservationCoverage::Complete
        } else {
            ObservationCoverage::Partial
        }
    }

    pub fn target_count(&self) -> usize {
        self.succeeded_count + self.failures.len()
    }

    pub const fn succeeded_count(&self) -> usize {
        self.succeeded_count
    }

    pub fn failed_count(&self) -> usize {
        self.failures.len()
    }

    pub fn cancelled_count(&self) -> usize {
        self.failures
            .iter()
            .filter(|failure| failure.kind() == ObservationFailureKind::Cancelled)
            .count()
    }

    pub fn failures(&self) -> &[ObservationTargetFailure] {
        &self.failures
    }

    pub fn completion(&self) -> OperationCompletion {
        if self.cancelled_count() > 0 {
            OperationCompletion::Cancelled
        } else if self.failures.is_empty() {
            OperationCompletion::Complete
        } else if self.succeeded_count == 0 {
            OperationCompletion::Failed
        } else {
            OperationCompletion::Partial
        }
    }

    pub fn was_cancelled(&self) -> bool {
        self.cancelled_count() > 0
    }

    pub fn is_partial(&self) -> bool {
        matches!(self.coverage(), ObservationCoverage::Partial)
    }
}

pub struct ObserveInventory<'a> {
    catalog: &'a dyn RepositoryCatalogPort,
    git: &'a dyn GitObservationPort,
    clock: &'a dyn ClockPort,
    cancellation: &'a dyn CancellationPort,
    identities: &'a mut dyn IdentityGeneratorPort,
    worktree_store: &'a mut dyn ObservationStorePort<Value = WorktreeGitState>,
    repository_store: &'a mut dyn ObservationStorePort<Value = Vec<Remote>>,
}

impl<'a> ObserveInventory<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        catalog: &'a dyn RepositoryCatalogPort,
        git: &'a dyn GitObservationPort,
        clock: &'a dyn ClockPort,
        cancellation: &'a dyn CancellationPort,
        identities: &'a mut dyn IdentityGeneratorPort,
        worktree_store: &'a mut dyn ObservationStorePort<Value = WorktreeGitState>,
        repository_store: &'a mut dyn ObservationStorePort<Value = Vec<Remote>>,
    ) -> Self {
        Self {
            catalog,
            git,
            clock,
            cancellation,
            identities,
            worktree_store,
            repository_store,
        }
    }

    pub fn execute(
        &mut self,
        policy: ObservationExecutionPolicy,
    ) -> PortResult<ObserveInventoryOutcome> {
        let targets = self.targets()?;
        let run_id = self.identities.next_observation_run_id()?;
        let results = execute_targets(
            targets,
            &run_id,
            policy,
            self.git,
            self.clock,
            self.cancellation,
        )?;

        let mut succeeded_count = 0;
        let mut failures = Vec::new();
        let mut deferred_error = None;

        for result in results {
            match result.outcome {
                TargetOutcome::Worktree(attempt) => {
                    record_attempt(&attempt, &mut succeeded_count, &mut failures);
                    self.worktree_store.save_attempt(&attempt)?;
                }
                TargetOutcome::RepositoryRemotes(attempt) => {
                    record_attempt(&attempt, &mut succeeded_count, &mut failures);
                    self.repository_store.save_attempt(&attempt)?;
                }
                TargetOutcome::Fatal(error) => {
                    if deferred_error.is_none() {
                        deferred_error = Some(error);
                    }
                }
            }
        }

        if let Some(error) = deferred_error {
            return Err(error);
        }

        Ok(ObserveInventoryOutcome::new(
            run_id,
            succeeded_count,
            failures,
        ))
    }

    fn targets(&self) -> PortResult<Vec<ObservationTarget>> {
        let mut repositories = self.catalog.repositories()?;
        repositories.sort_by(|left, right| left.id().as_str().cmp(right.id().as_str()));

        let mut targets = Vec::new();
        for repository in repositories {
            let repository_id = repository.id().clone();
            let mut locations = self.catalog.locations_for_repository(&repository_id)?;
            locations.sort_by(|left, right| left.id().as_str().cmp(right.id().as_str()));

            for location in &locations {
                if location.repository_id() != &repository_id {
                    return Err(invariant_violation());
                }
            }

            let repository_path = locations
                .iter()
                .find(|location| location.availability() == LocationAvailability::Available)
                .map(|location| location.path().to_path_buf());
            targets.push(ObservationTarget::RepositoryRemotes {
                repository_id: repository_id.clone(),
                path: repository_path,
            });

            let mut worktrees = self.catalog.worktrees_for_repository(&repository_id)?;
            worktrees.sort_by(|left, right| left.id().as_str().cmp(right.id().as_str()));
            for worktree in worktrees {
                if worktree.repository_id() != &repository_id {
                    return Err(invariant_violation());
                }
                let location = locations
                    .iter()
                    .find(|location| location.id() == worktree.location_id())
                    .ok_or_else(invariant_violation)?;
                let path = (location.availability() == LocationAvailability::Available)
                    .then(|| location.path().to_path_buf());
                targets.push(ObservationTarget::Worktree {
                    worktree_id: worktree.id().clone(),
                    path,
                });
            }
        }

        Ok(targets)
    }
}

fn record_attempt<T>(
    attempt: &ObservationAttempt<T>,
    succeeded_count: &mut usize,
    failures: &mut Vec<ObservationTargetFailure>,
) {
    match attempt {
        ObservationAttempt::Succeeded(_) => *succeeded_count += 1,
        ObservationAttempt::Failed(failure) => {
            failures.push(ObservationTargetFailure::from_failure(failure));
        }
    }
}

#[derive(Clone, Debug)]
enum ObservationTarget {
    RepositoryRemotes {
        repository_id: RepositoryId,
        path: Option<PathBuf>,
    },
    Worktree {
        worktree_id: WorktreeId,
        path: Option<PathBuf>,
    },
}

struct OrderedTargetResult {
    ordinal: usize,
    outcome: TargetOutcome,
}

enum TargetOutcome {
    RepositoryRemotes(ObservationAttempt<Vec<Remote>>),
    Worktree(ObservationAttempt<WorktreeGitState>),
    Fatal(PortError),
}

fn execute_targets(
    targets: Vec<ObservationTarget>,
    run_id: &ObservationRunId,
    policy: ObservationExecutionPolicy,
    git: &dyn GitObservationPort,
    clock: &dyn ClockPort,
    cancellation: &dyn CancellationPort,
) -> PortResult<Vec<OrderedTargetResult>> {
    if targets.is_empty() {
        return Ok(Vec::new());
    }

    let queue = Mutex::new(VecDeque::from_iter(targets.into_iter().enumerate()));
    let worker_count = policy
        .max_parallelism()
        .min(queue.lock().map_err(|_| unexpected_failure())?.len());

    let mut results = thread::scope(|scope| -> PortResult<Vec<OrderedTargetResult>> {
        let mut handles = Vec::with_capacity(worker_count);
        for _ in 0..worker_count {
            handles.push(scope.spawn(|| -> PortResult<Vec<OrderedTargetResult>> {
                let mut worker_results = Vec::new();
                loop {
                    let next = queue.lock().map_err(|_| unexpected_failure())?.pop_front();
                    let Some((ordinal, target)) = next else {
                        break;
                    };
                    worker_results.push(OrderedTargetResult {
                        ordinal,
                        outcome: execute_target(target, run_id, git, clock, cancellation),
                    });
                }
                Ok(worker_results)
            }));
        }

        let mut results = Vec::new();
        for handle in handles {
            let mut worker_results = handle.join().map_err(|_| unexpected_failure())??;
            results.append(&mut worker_results);
        }
        Ok(results)
    })?;
    results.sort_by_key(|result| result.ordinal);
    Ok(results)
}

fn execute_target(
    target: ObservationTarget,
    run_id: &ObservationRunId,
    git: &dyn GitObservationPort,
    clock: &dyn ClockPort,
    cancellation: &dyn CancellationPort,
) -> TargetOutcome {
    match target {
        ObservationTarget::RepositoryRemotes {
            repository_id,
            path,
        } => {
            let key = ObservationKey::repository_remotes(repository_id);
            if cancellation.is_cancelled() {
                return TargetOutcome::RepositoryRemotes(failed_attempt(
                    run_id,
                    key,
                    clock,
                    ObservationFailureKind::Cancelled,
                ));
            }
            let Some(path) = path else {
                return TargetOutcome::RepositoryRemotes(failed_attempt(
                    run_id,
                    key,
                    clock,
                    ObservationFailureKind::SubjectUnavailable,
                ));
            };

            match git.observe_repository_remotes(&path) {
                Ok(remotes) => TargetOutcome::RepositoryRemotes(successful_attempt(
                    run_id, key, clock, remotes,
                )),
                Err(error) => match observation_failure(error.kind()) {
                    Ok(kind) => {
                        TargetOutcome::RepositoryRemotes(failed_attempt(run_id, key, clock, kind))
                    }
                    Err(error) => TargetOutcome::Fatal(error),
                },
            }
        }
        ObservationTarget::Worktree { worktree_id, path } => {
            let key = ObservationKey::worktree_git_state(worktree_id);
            if cancellation.is_cancelled() {
                return TargetOutcome::Worktree(failed_attempt(
                    run_id,
                    key,
                    clock,
                    ObservationFailureKind::Cancelled,
                ));
            }
            let Some(path) = path else {
                return TargetOutcome::Worktree(failed_attempt(
                    run_id,
                    key,
                    clock,
                    ObservationFailureKind::SubjectUnavailable,
                ));
            };

            match git.observe_worktree(&path) {
                Ok(state) => TargetOutcome::Worktree(successful_attempt(run_id, key, clock, state)),
                Err(error) => match observation_failure(error.kind()) {
                    Ok(kind) => TargetOutcome::Worktree(failed_attempt(run_id, key, clock, kind)),
                    Err(error) => TargetOutcome::Fatal(error),
                },
            }
        }
    }
}

fn successful_attempt<T>(
    run_id: &ObservationRunId,
    key: ObservationKey,
    clock: &dyn ClockPort,
    value: T,
) -> ObservationAttempt<T> {
    ObservationAttempt::Succeeded(Observation::new(
        metadata(run_id, key, clock, ObservationCoverage::Complete),
        value,
    ))
}

fn failed_attempt<T>(
    run_id: &ObservationRunId,
    key: ObservationKey,
    clock: &dyn ClockPort,
    kind: ObservationFailureKind,
) -> ObservationAttempt<T> {
    ObservationAttempt::Failed(ObservationFailure::new(
        metadata(run_id, key, clock, ObservationCoverage::Partial),
        kind,
    ))
}

fn metadata(
    run_id: &ObservationRunId,
    key: ObservationKey,
    clock: &dyn ClockPort,
    coverage: ObservationCoverage,
) -> ObservationMetadata {
    ObservationMetadata::new(run_id.clone(), key, Freshness::new(clock.now()), coverage)
}

fn observation_failure(kind: PortErrorKind) -> PortResult<ObservationFailureKind> {
    match kind {
        PortErrorKind::PermissionDenied => Ok(ObservationFailureKind::SubjectUnavailable),
        PortErrorKind::ResourceUnavailable => Ok(ObservationFailureKind::SubjectDisappeared),
        PortErrorKind::ProcessExited { exit_code } => {
            Ok(ObservationFailureKind::ProcessExited { exit_code })
        }
        PortErrorKind::InvalidData => Ok(ObservationFailureKind::InvalidMachineOutput),
        PortErrorKind::OutputLimitExceeded => Ok(ObservationFailureKind::OutputLimitExceeded),
        PortErrorKind::TimedOut => Ok(ObservationFailureKind::TimedOut),
        PortErrorKind::Cancelled => Ok(ObservationFailureKind::Cancelled),
        PortErrorKind::IoFailure
        | PortErrorKind::StorageFailure
        | PortErrorKind::InvariantViolation
        | PortErrorKind::Unexpected => Err(PortError::new(kind)),
    }
}

const fn invariant_violation() -> PortError {
    PortError::new(PortErrorKind::InvariantViolation)
}

const fn unexpected_failure() -> PortError {
    PortError::new(PortErrorKind::Unexpected)
}
