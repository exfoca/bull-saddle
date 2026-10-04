// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bulls_application::ports::{
    ObservationStorePort, PlatformPaths, PortResult, WorkspaceObservationState,
};
use bulls_core::{
    Branch, Freshness, Head, Observation, ObservationAttempt, ObservationCoverage,
    ObservationFailure, ObservationFailureKind, ObservationKey, ObservationMetadata,
    ObservationRunId, Remote, RepositoryId, Upstream, UpstreamDivergence, UpstreamState,
    WorktreeChanges, WorktreeGitState, WorktreeId,
};
use rusqlite::{OptionalExtension, Row, TransactionBehavior, params};

use crate::error::{invalid_data, invariant_violation, sqlite_port_error};
use crate::storage::{SqliteStorage, advance_workspace_revision};

const NANOS_PER_SECOND: i128 = 1_000_000_000;

pub struct SqliteWorktreeObservationStore {
    storage: SqliteStorage,
}

impl SqliteWorktreeObservationStore {
    pub fn open(data_dir: impl Into<PathBuf>) -> PortResult<Self> {
        Ok(Self {
            storage: SqliteStorage::open(data_dir)?,
        })
    }

    pub fn from_platform_paths(paths: &PlatformPaths) -> PortResult<Self> {
        Self::open(paths.data_dir())
    }

    pub fn from_storage(storage: SqliteStorage) -> Self {
        Self { storage }
    }

    pub fn path(&self) -> &Path {
        self.storage.path()
    }
}

impl ObservationStorePort for SqliteWorktreeObservationStore {
    type Value = WorktreeGitState;

    fn latest_observation(
        &self,
        key: &ObservationKey,
    ) -> PortResult<Option<Observation<Self::Value>>> {
        let worktree_id = worktree_id(key)?;
        self.load_latest(worktree_id, true)?
            .map(|attempt| match attempt {
                ObservationAttempt::Succeeded(observation) => Ok(observation),
                ObservationAttempt::Failed(_) => Err(invalid_data()),
            })
            .transpose()
    }

    fn latest_attempt(
        &self,
        key: &ObservationKey,
    ) -> PortResult<Option<ObservationAttempt<Self::Value>>> {
        self.load_latest(worktree_id(key)?, false)
    }

    fn save_attempt(&mut self, attempt: &ObservationAttempt<Self::Value>) -> PortResult<()> {
        let metadata = attempt.metadata();
        let worktree_id = worktree_id(metadata.key())?;
        let timestamp = encode_system_time(metadata.freshness().observed_at())?;
        let coverage = encode_coverage(metadata.coverage());

        let fields = WorktreeAttemptFields::from_attempt(attempt);
        let transaction = self
            .storage
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| sqlite_port_error(&error))?;
        transaction
            .execute(
                "INSERT INTO worktree_git_observation_attempts (\
                    run_id, worktree_id, observed_at_seconds, observed_at_nanos, coverage, \
                    outcome, failure_kind, exit_code, head_kind, branch_name, upstream_kind, \
                    upstream_remote, upstream_branch, ahead, behind, staged, unstaged, untracked\
                 ) VALUES (\
                    ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18\
                 )",
                params![
                    metadata.run_id().as_str(),
                    worktree_id.as_str(),
                    timestamp.seconds,
                    timestamp.nanos,
                    coverage,
                    fields.outcome,
                    fields.failure_kind,
                    fields.exit_code,
                    fields.head_kind,
                    fields.branch_name,
                    fields.upstream_kind,
                    fields.upstream_remote,
                    fields.upstream_branch,
                    fields.ahead,
                    fields.behind,
                    fields.staged,
                    fields.unstaged,
                    fields.untracked,
                ],
            )
            .map_err(|error| sqlite_port_error(&error))?;
        advance_workspace_revision(&transaction)?;
        transaction
            .commit()
            .map_err(|error| sqlite_port_error(&error))
    }
}

impl SqliteWorktreeObservationStore {
    fn load_latest(
        &self,
        worktree_id: &WorktreeId,
        successes_only: bool,
    ) -> PortResult<Option<ObservationAttempt<WorktreeGitState>>> {
        let outcome_filter = if successes_only {
            "AND outcome = 'success'"
        } else {
            ""
        };
        let sql = format!(
            "SELECT run_id, observed_at_seconds, observed_at_nanos, coverage, outcome, \
                    failure_kind, exit_code, head_kind, branch_name, upstream_kind, \
                    upstream_remote, upstream_branch, ahead, behind, staged, unstaged, untracked \
             FROM worktree_git_observation_attempts \
             WHERE worktree_id = ?1 {outcome_filter} \
             ORDER BY observed_at_seconds DESC, observed_at_nanos DESC, attempt_id DESC \
             LIMIT 1"
        );

        self.storage
            .connection()
            .query_row(&sql, [worktree_id.as_str()], |row| {
                read_worktree_attempt(row, worktree_id)
            })
            .optional()
            .map_err(|error| sqlite_port_error(&error))?
            .map(WorktreeAttemptRow::into_attempt)
            .transpose()
    }
}

pub struct SqliteRepositoryRemotesObservationStore {
    storage: SqliteStorage,
}

impl SqliteRepositoryRemotesObservationStore {
    pub fn open(data_dir: impl Into<PathBuf>) -> PortResult<Self> {
        Ok(Self {
            storage: SqliteStorage::open(data_dir)?,
        })
    }

    pub fn from_platform_paths(paths: &PlatformPaths) -> PortResult<Self> {
        Self::open(paths.data_dir())
    }

    pub fn from_storage(storage: SqliteStorage) -> Self {
        Self { storage }
    }

    pub fn path(&self) -> &Path {
        self.storage.path()
    }
}

impl ObservationStorePort for SqliteRepositoryRemotesObservationStore {
    type Value = Vec<Remote>;

    fn latest_observation(
        &self,
        key: &ObservationKey,
    ) -> PortResult<Option<Observation<Self::Value>>> {
        let repository_id = repository_id(key)?;
        self.load_latest(repository_id, true)?
            .map(|attempt| match attempt {
                ObservationAttempt::Succeeded(observation) => Ok(observation),
                ObservationAttempt::Failed(_) => Err(invalid_data()),
            })
            .transpose()
    }

    fn latest_attempt(
        &self,
        key: &ObservationKey,
    ) -> PortResult<Option<ObservationAttempt<Self::Value>>> {
        self.load_latest(repository_id(key)?, false)
    }

    fn save_attempt(&mut self, attempt: &ObservationAttempt<Self::Value>) -> PortResult<()> {
        let metadata = attempt.metadata();
        let repository_id = repository_id(metadata.key())?;
        let timestamp = encode_system_time(metadata.freshness().observed_at())?;
        let coverage = encode_coverage(metadata.coverage());
        let (outcome, failure_kind, exit_code) = attempt_outcome(attempt);
        let remote_count = match attempt {
            ObservationAttempt::Succeeded(observation) => {
                Some(i64::try_from(observation.value().len()).map_err(|_| invariant_violation())?)
            }
            ObservationAttempt::Failed(_) => None,
        };

        let transaction = self
            .storage
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| sqlite_port_error(&error))?;
        transaction
            .execute(
                "INSERT INTO repository_remote_observation_attempts (\
                    run_id, repository_id, observed_at_seconds, observed_at_nanos, coverage, \
                    outcome, failure_kind, exit_code, remote_count\
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    metadata.run_id().as_str(),
                    repository_id.as_str(),
                    timestamp.seconds,
                    timestamp.nanos,
                    coverage,
                    outcome,
                    failure_kind,
                    exit_code,
                    remote_count,
                ],
            )
            .map_err(|error| sqlite_port_error(&error))?;
        let attempt_id = transaction.last_insert_rowid();

        if let ObservationAttempt::Succeeded(observation) = attempt {
            for (ordinal, remote) in observation.value().iter().enumerate() {
                let ordinal = i64::try_from(ordinal).map_err(|_| invariant_violation())?;
                transaction
                    .execute(
                        "INSERT INTO repository_remote_observation_values \
                             (attempt_id, ordinal, remote_name) VALUES (?1, ?2, ?3)",
                        params![attempt_id, ordinal, remote.name()],
                    )
                    .map_err(|error| sqlite_port_error(&error))?;
            }
        }

        advance_workspace_revision(&transaction)?;
        transaction
            .commit()
            .map_err(|error| sqlite_port_error(&error))
    }
}

impl SqliteRepositoryRemotesObservationStore {
    fn load_latest(
        &self,
        repository_id: &RepositoryId,
        successes_only: bool,
    ) -> PortResult<Option<ObservationAttempt<Vec<Remote>>>> {
        let outcome_filter = if successes_only {
            "AND outcome = 'success'"
        } else {
            ""
        };
        let sql = format!(
            "SELECT attempt_id, run_id, observed_at_seconds, observed_at_nanos, coverage, \
                    outcome, failure_kind, exit_code, remote_count \
             FROM repository_remote_observation_attempts \
             WHERE repository_id = ?1 {outcome_filter} \
             ORDER BY observed_at_seconds DESC, observed_at_nanos DESC, attempt_id DESC \
             LIMIT 1"
        );
        let row = self
            .storage
            .connection()
            .query_row(&sql, [repository_id.as_str()], |row| {
                read_repository_attempt(row, repository_id)
            })
            .optional()
            .map_err(|error| sqlite_port_error(&error))?;

        row.map(|row| self.repository_attempt(row)).transpose()
    }

    fn repository_attempt(
        &self,
        row: RepositoryAttemptRow,
    ) -> PortResult<ObservationAttempt<Vec<Remote>>> {
        let remotes =
            load_repository_remotes_for_attempt(self.storage.connection(), row.attempt_id)?;
        row.into_attempt(remotes)
    }
}

pub(crate) fn load_latest_worktree_git_states(
    connection: &rusqlite::Connection,
) -> PortResult<Vec<WorkspaceObservationState<WorktreeGitState>>> {
    let mut statement = connection
        .prepare(
            "WITH ranked AS (\
                 SELECT attempt_id, run_id, worktree_id, observed_at_seconds, observed_at_nanos, \
                        coverage, outcome, failure_kind, exit_code, head_kind, branch_name, \
                        upstream_kind, upstream_remote, upstream_branch, ahead, behind, staged, \
                        unstaged, untracked, \
                        ROW_NUMBER() OVER (\
                            PARTITION BY worktree_id \
                            ORDER BY observed_at_seconds DESC, observed_at_nanos DESC, attempt_id DESC\
                        ) AS latest_rank, \
                        ROW_NUMBER() OVER (\
                            PARTITION BY worktree_id, outcome \
                            ORDER BY observed_at_seconds DESC, observed_at_nanos DESC, attempt_id DESC\
                        ) AS outcome_rank \
                 FROM worktree_git_observation_attempts\
             ) \
             SELECT run_id, observed_at_seconds, observed_at_nanos, coverage, outcome, \
                    failure_kind, exit_code, head_kind, branch_name, upstream_kind, \
                    upstream_remote, upstream_branch, ahead, behind, staged, unstaged, untracked, \
                    worktree_id, latest_rank, outcome_rank \
             FROM ranked \
             WHERE latest_rank = 1 OR (outcome = 'success' AND outcome_rank = 1) \
             ORDER BY worktree_id, observed_at_seconds DESC, observed_at_nanos DESC, attempt_id DESC",
        )
        .map_err(|error| sqlite_port_error(&error))?;
    let rows = statement
        .query_map([], |row| {
            let worktree_id = WorktreeId::from(row.get::<_, String>(17)?);
            Ok((
                read_worktree_attempt(row, &worktree_id)?,
                row.get::<_, i64>(18)?,
                row.get::<_, i64>(19)?,
            ))
        })
        .map_err(|error| sqlite_port_error(&error))?;

    let mut states = HashMap::<
        WorktreeId,
        (
            Option<ObservationAttempt<WorktreeGitState>>,
            Option<Observation<WorktreeGitState>>,
        ),
    >::new();

    for row in rows {
        let (row, latest_rank, outcome_rank) = row.map_err(|error| sqlite_port_error(&error))?;
        let worktree_id = row.worktree_id.clone();
        let attempt = row.into_attempt()?;
        let state = states.entry(worktree_id).or_default();

        if latest_rank == 1 {
            state.0 = Some(attempt.clone());
        }
        if outcome_rank == 1
            && let ObservationAttempt::Succeeded(observation) = &attempt
        {
            state.1 = Some(observation.clone());
        }
    }

    let mut states = states
        .into_values()
        .map(|(latest_attempt, latest_observation)| {
            Ok(WorkspaceObservationState::new(
                latest_attempt.ok_or_else(invalid_data)?,
                latest_observation,
            ))
        })
        .collect::<PortResult<Vec<_>>>()?;
    states.sort_by(|left, right| {
        left.latest_attempt()
            .metadata()
            .key()
            .worktree_id()
            .map(WorktreeId::as_str)
            .cmp(
                &right
                    .latest_attempt()
                    .metadata()
                    .key()
                    .worktree_id()
                    .map(WorktreeId::as_str),
            )
    });

    Ok(states)
}

pub(crate) fn load_latest_repository_remotes_states(
    connection: &rusqlite::Connection,
) -> PortResult<Vec<WorkspaceObservationState<Vec<Remote>>>> {
    let mut statement = connection
        .prepare(
            "WITH ranked AS (\
                 SELECT attempt_id, run_id, repository_id, observed_at_seconds, observed_at_nanos, \
                        coverage, outcome, failure_kind, exit_code, remote_count, \
                        ROW_NUMBER() OVER (\
                            PARTITION BY repository_id \
                            ORDER BY observed_at_seconds DESC, observed_at_nanos DESC, attempt_id DESC\
                        ) AS latest_rank, \
                        ROW_NUMBER() OVER (\
                            PARTITION BY repository_id, outcome \
                            ORDER BY observed_at_seconds DESC, observed_at_nanos DESC, attempt_id DESC\
                        ) AS outcome_rank \
                 FROM repository_remote_observation_attempts\
             ) \
             SELECT attempt_id, run_id, observed_at_seconds, observed_at_nanos, coverage, \
                    outcome, failure_kind, exit_code, remote_count, repository_id, \
                    latest_rank, outcome_rank \
             FROM ranked \
             WHERE latest_rank = 1 OR (outcome = 'success' AND outcome_rank = 1) \
             ORDER BY repository_id, observed_at_seconds DESC, observed_at_nanos DESC, attempt_id DESC",
        )
        .map_err(|error| sqlite_port_error(&error))?;
    let rows = statement
        .query_map([], |row| {
            let repository_id = RepositoryId::from(row.get::<_, String>(9)?);
            Ok((
                read_repository_attempt(row, &repository_id)?,
                row.get::<_, i64>(10)?,
                row.get::<_, i64>(11)?,
            ))
        })
        .map_err(|error| sqlite_port_error(&error))?;
    let rows = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| sqlite_port_error(&error))?;
    drop(statement);

    let mut remote_values = load_latest_repository_remote_values(connection)?;
    let mut states = HashMap::<
        RepositoryId,
        (
            Option<ObservationAttempt<Vec<Remote>>>,
            Option<Observation<Vec<Remote>>>,
        ),
    >::new();

    for (row, latest_rank, outcome_rank) in rows {
        let repository_id = row.repository_id.clone();
        let remotes = remote_values.remove(&row.attempt_id).unwrap_or_default();
        let attempt = row.into_attempt(remotes)?;
        let state = states.entry(repository_id).or_default();

        if latest_rank == 1 {
            state.0 = Some(attempt.clone());
        }
        if outcome_rank == 1
            && let ObservationAttempt::Succeeded(observation) = &attempt
        {
            state.1 = Some(observation.clone());
        }
    }

    let mut states = states
        .into_values()
        .map(|(latest_attempt, latest_observation)| {
            Ok(WorkspaceObservationState::new(
                latest_attempt.ok_or_else(invalid_data)?,
                latest_observation,
            ))
        })
        .collect::<PortResult<Vec<_>>>()?;
    states.sort_by(|left, right| {
        left.latest_attempt()
            .metadata()
            .key()
            .repository_id()
            .map(RepositoryId::as_str)
            .cmp(
                &right
                    .latest_attempt()
                    .metadata()
                    .key()
                    .repository_id()
                    .map(RepositoryId::as_str),
            )
    });

    Ok(states)
}

fn load_latest_repository_remote_values(
    connection: &rusqlite::Connection,
) -> PortResult<HashMap<i64, Vec<Remote>>> {
    let mut statement = connection
        .prepare(
            "WITH ranked AS (\
                 SELECT attempt_id, repository_id, outcome, \
                        ROW_NUMBER() OVER (\
                            PARTITION BY repository_id \
                            ORDER BY observed_at_seconds DESC, observed_at_nanos DESC, attempt_id DESC\
                        ) AS latest_rank, \
                        ROW_NUMBER() OVER (\
                            PARTITION BY repository_id, outcome \
                            ORDER BY observed_at_seconds DESC, observed_at_nanos DESC, attempt_id DESC\
                        ) AS outcome_rank \
                 FROM repository_remote_observation_attempts\
             ) \
             SELECT remote_values.attempt_id, remote_values.remote_name \
             FROM ranked \
             JOIN repository_remote_observation_values remote_values \
               ON remote_values.attempt_id = ranked.attempt_id \
             WHERE ranked.latest_rank = 1 \
                OR (ranked.outcome = 'success' AND ranked.outcome_rank = 1) \
             ORDER BY remote_values.attempt_id, remote_values.ordinal",
        )
        .map_err(|error| sqlite_port_error(&error))?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| sqlite_port_error(&error))?;

    let mut values = HashMap::<i64, Vec<Remote>>::new();
    for row in rows {
        let (attempt_id, remote_name) = row.map_err(|error| sqlite_port_error(&error))?;
        values
            .entry(attempt_id)
            .or_default()
            .push(Remote::new(remote_name));
    }

    Ok(values)
}

fn load_repository_remotes_for_attempt(
    connection: &rusqlite::Connection,
    attempt_id: i64,
) -> PortResult<Vec<Remote>> {
    let mut statement = connection
        .prepare(
            "SELECT remote_name FROM repository_remote_observation_values \
             WHERE attempt_id = ?1 ORDER BY ordinal",
        )
        .map_err(|error| sqlite_port_error(&error))?;
    statement
        .query_map([attempt_id], |row| row.get::<_, String>(0))
        .map_err(|error| sqlite_port_error(&error))?
        .map(|row| {
            row.map(Remote::new)
                .map_err(|error| sqlite_port_error(&error))
        })
        .collect()
}

struct TimestampParts {
    seconds: i64,
    nanos: i64,
}

fn encode_system_time(value: SystemTime) -> PortResult<TimestampParts> {
    let total_nanos = match value.duration_since(UNIX_EPOCH) {
        Ok(duration) => duration_to_nanos(duration),
        Err(error) => -duration_to_nanos(error.duration()),
    };
    let seconds = total_nanos.div_euclid(NANOS_PER_SECOND);
    let nanos = total_nanos.rem_euclid(NANOS_PER_SECOND);

    Ok(TimestampParts {
        seconds: i64::try_from(seconds).map_err(|_| invalid_data())?,
        nanos: i64::try_from(nanos).map_err(|_| invalid_data())?,
    })
}

fn decode_system_time(seconds: i64, nanos: i64) -> PortResult<SystemTime> {
    if !(0..1_000_000_000).contains(&nanos) {
        return Err(invalid_data());
    }

    let total_nanos = i128::from(seconds)
        .checked_mul(NANOS_PER_SECOND)
        .and_then(|value| value.checked_add(i128::from(nanos)))
        .ok_or_else(invalid_data)?;
    if total_nanos >= 0 {
        UNIX_EPOCH
            .checked_add(nanos_to_duration(total_nanos as u128)?)
            .ok_or_else(invalid_data)
    } else {
        UNIX_EPOCH
            .checked_sub(nanos_to_duration(total_nanos.unsigned_abs())?)
            .ok_or_else(invalid_data)
    }
}

fn duration_to_nanos(duration: Duration) -> i128 {
    i128::from(duration.as_secs()) * NANOS_PER_SECOND + i128::from(duration.subsec_nanos())
}

fn nanos_to_duration(total_nanos: u128) -> PortResult<Duration> {
    let seconds = total_nanos / NANOS_PER_SECOND as u128;
    let nanos = total_nanos % NANOS_PER_SECOND as u128;
    Ok(Duration::new(
        u64::try_from(seconds).map_err(|_| invalid_data())?,
        u32::try_from(nanos).map_err(|_| invalid_data())?,
    ))
}

fn worktree_id(key: &ObservationKey) -> PortResult<&WorktreeId> {
    key.worktree_id().ok_or_else(invariant_violation)
}

fn repository_id(key: &ObservationKey) -> PortResult<&RepositoryId> {
    key.repository_id().ok_or_else(invariant_violation)
}

fn encode_coverage(coverage: ObservationCoverage) -> &'static str {
    match coverage {
        ObservationCoverage::Complete => "complete",
        ObservationCoverage::Partial => "partial",
    }
}

fn decode_coverage(value: &str) -> PortResult<ObservationCoverage> {
    match value {
        "complete" => Ok(ObservationCoverage::Complete),
        "partial" => Ok(ObservationCoverage::Partial),
        _ => Err(invalid_data()),
    }
}

fn encode_failure(kind: ObservationFailureKind) -> (&'static str, Option<i32>) {
    match kind {
        ObservationFailureKind::SubjectUnavailable => ("subject_unavailable", None),
        ObservationFailureKind::ProcessExited { exit_code } => ("process_exited", Some(exit_code)),
        ObservationFailureKind::TimedOut => ("timed_out", None),
        ObservationFailureKind::Cancelled => ("cancelled", None),
        ObservationFailureKind::OutputLimitExceeded => ("output_limit_exceeded", None),
        ObservationFailureKind::InvalidMachineOutput => ("invalid_machine_output", None),
        ObservationFailureKind::SubjectDisappeared => ("subject_disappeared", None),
    }
}

fn decode_failure(
    kind: Option<&str>,
    exit_code: Option<i32>,
) -> PortResult<ObservationFailureKind> {
    match (kind, exit_code) {
        (Some("subject_unavailable"), None) => Ok(ObservationFailureKind::SubjectUnavailable),
        (Some("process_exited"), Some(exit_code)) => {
            Ok(ObservationFailureKind::ProcessExited { exit_code })
        }
        (Some("timed_out"), None) => Ok(ObservationFailureKind::TimedOut),
        (Some("cancelled"), None) => Ok(ObservationFailureKind::Cancelled),
        (Some("output_limit_exceeded"), None) => Ok(ObservationFailureKind::OutputLimitExceeded),
        (Some("invalid_machine_output"), None) => Ok(ObservationFailureKind::InvalidMachineOutput),
        (Some("subject_disappeared"), None) => Ok(ObservationFailureKind::SubjectDisappeared),
        _ => Err(invalid_data()),
    }
}

fn attempt_outcome<T>(
    attempt: &ObservationAttempt<T>,
) -> (&'static str, Option<&'static str>, Option<i32>) {
    match attempt {
        ObservationAttempt::Succeeded(_) => ("success", None, None),
        ObservationAttempt::Failed(failure) => {
            let (kind, exit_code) = encode_failure(failure.kind());
            ("failure", Some(kind), exit_code)
        }
    }
}

struct WorktreeAttemptFields<'a> {
    outcome: &'static str,
    failure_kind: Option<&'static str>,
    exit_code: Option<i32>,
    head_kind: Option<&'static str>,
    branch_name: Option<&'a str>,
    upstream_kind: Option<&'static str>,
    upstream_remote: Option<&'a str>,
    upstream_branch: Option<&'a str>,
    ahead: Option<Vec<u8>>,
    behind: Option<Vec<u8>>,
    staged: Option<bool>,
    unstaged: Option<bool>,
    untracked: Option<bool>,
}

impl<'a> WorktreeAttemptFields<'a> {
    fn from_attempt(attempt: &'a ObservationAttempt<WorktreeGitState>) -> Self {
        match attempt {
            ObservationAttempt::Failed(failure) => {
                let (failure_kind, exit_code) = encode_failure(failure.kind());
                Self {
                    outcome: "failure",
                    failure_kind: Some(failure_kind),
                    exit_code,
                    head_kind: None,
                    branch_name: None,
                    upstream_kind: None,
                    upstream_remote: None,
                    upstream_branch: None,
                    ahead: None,
                    behind: None,
                    staged: None,
                    unstaged: None,
                    untracked: None,
                }
            }
            ObservationAttempt::Succeeded(observation) => {
                let value = observation.value();
                let (head_kind, branch_name) = match value.head() {
                    Head::Unborn(branch) => ("unborn", Some(branch.name())),
                    Head::Branch(branch) => ("branch", Some(branch.name())),
                    Head::Detached => ("detached", None),
                };
                let upstream = encode_upstream(value.upstream());
                let changes = value.changes();
                Self {
                    outcome: "success",
                    failure_kind: None,
                    exit_code: None,
                    head_kind: Some(head_kind),
                    branch_name,
                    upstream_kind: upstream.kind,
                    upstream_remote: upstream.remote,
                    upstream_branch: upstream.branch,
                    ahead: upstream.ahead,
                    behind: upstream.behind,
                    staged: Some(changes.staged()),
                    unstaged: Some(changes.unstaged()),
                    untracked: Some(changes.untracked()),
                }
            }
        }
    }
}

struct EncodedUpstream<'a> {
    kind: Option<&'static str>,
    remote: Option<&'a str>,
    branch: Option<&'a str>,
    ahead: Option<Vec<u8>>,
    behind: Option<Vec<u8>>,
}

fn encode_upstream(upstream: Option<&UpstreamState>) -> EncodedUpstream<'_> {
    match upstream {
        None => EncodedUpstream {
            kind: None,
            remote: None,
            branch: None,
            ahead: None,
            behind: None,
        },
        Some(UpstreamState::Unconfigured) => EncodedUpstream {
            kind: Some("unconfigured"),
            remote: None,
            branch: None,
            ahead: None,
            behind: None,
        },
        Some(UpstreamState::Gone(upstream)) => EncodedUpstream {
            kind: Some("gone"),
            remote: Some(upstream.remote().name()),
            branch: Some(upstream.branch().name()),
            ahead: None,
            behind: None,
        },
        Some(UpstreamState::Tracking {
            upstream,
            divergence,
        }) => EncodedUpstream {
            kind: Some("tracking"),
            remote: Some(upstream.remote().name()),
            branch: Some(upstream.branch().name()),
            ahead: Some(divergence.ahead().to_be_bytes().to_vec()),
            behind: Some(divergence.behind().to_be_bytes().to_vec()),
        },
    }
}

struct WorktreeAttemptRow {
    run_id: String,
    worktree_id: WorktreeId,
    observed_at_seconds: i64,
    observed_at_nanos: i64,
    coverage: String,
    outcome: String,
    failure_kind: Option<String>,
    exit_code: Option<i32>,
    head_kind: Option<String>,
    branch_name: Option<String>,
    upstream_kind: Option<String>,
    upstream_remote: Option<String>,
    upstream_branch: Option<String>,
    ahead: Option<Vec<u8>>,
    behind: Option<Vec<u8>>,
    staged: Option<bool>,
    unstaged: Option<bool>,
    untracked: Option<bool>,
}

impl WorktreeAttemptRow {
    fn metadata(&self) -> PortResult<ObservationMetadata> {
        Ok(ObservationMetadata::new(
            ObservationRunId::from(self.run_id.clone()),
            ObservationKey::worktree_git_state(self.worktree_id.clone()),
            Freshness::new(decode_system_time(
                self.observed_at_seconds,
                self.observed_at_nanos,
            )?),
            decode_coverage(&self.coverage)?,
        ))
    }

    fn into_attempt(self) -> PortResult<ObservationAttempt<WorktreeGitState>> {
        let metadata = self.metadata()?;
        match self.outcome.as_str() {
            "failure" => Ok(ObservationAttempt::Failed(ObservationFailure::new(
                metadata,
                decode_failure(self.failure_kind.as_deref(), self.exit_code)?,
            ))),
            "success" => {
                if self.failure_kind.is_some() || self.exit_code.is_some() {
                    return Err(invalid_data());
                }
                let head_kind = self.head_kind.as_deref().ok_or_else(invalid_data)?;
                let upstream = decode_upstream(
                    self.upstream_kind.as_deref(),
                    self.upstream_remote.as_deref(),
                    self.upstream_branch.as_deref(),
                    self.ahead,
                    self.behind,
                )?;
                let changes = WorktreeChanges::new(
                    self.staged.ok_or_else(invalid_data)?,
                    self.unstaged.ok_or_else(invalid_data)?,
                    self.untracked.ok_or_else(invalid_data)?,
                );
                let value = match (head_kind, self.branch_name) {
                    ("unborn", Some(branch)) => WorktreeGitState::unborn(
                        Branch::new(branch),
                        upstream.ok_or_else(invalid_data)?,
                    ),
                    ("branch", Some(branch)) => WorktreeGitState::branch(
                        Branch::new(branch),
                        upstream.ok_or_else(invalid_data)?,
                    ),
                    ("detached", None) if upstream.is_none() => WorktreeGitState::detached(),
                    _ => return Err(invalid_data()),
                }
                .with_changes(changes);

                Ok(ObservationAttempt::Succeeded(Observation::new(
                    metadata, value,
                )))
            }
            _ => Err(invalid_data()),
        }
    }
}

fn read_worktree_attempt(
    row: &Row<'_>,
    worktree_id: &WorktreeId,
) -> rusqlite::Result<WorktreeAttemptRow> {
    Ok(WorktreeAttemptRow {
        run_id: row.get(0)?,
        worktree_id: worktree_id.clone(),
        observed_at_seconds: row.get(1)?,
        observed_at_nanos: row.get(2)?,
        coverage: row.get(3)?,
        outcome: row.get(4)?,
        failure_kind: row.get(5)?,
        exit_code: row.get(6)?,
        head_kind: row.get(7)?,
        branch_name: row.get(8)?,
        upstream_kind: row.get(9)?,
        upstream_remote: row.get(10)?,
        upstream_branch: row.get(11)?,
        ahead: row.get(12)?,
        behind: row.get(13)?,
        staged: row.get(14)?,
        unstaged: row.get(15)?,
        untracked: row.get(16)?,
    })
}

fn decode_upstream(
    kind: Option<&str>,
    remote: Option<&str>,
    branch: Option<&str>,
    ahead: Option<Vec<u8>>,
    behind: Option<Vec<u8>>,
) -> PortResult<Option<UpstreamState>> {
    match (kind, remote, branch, ahead, behind) {
        (None, None, None, None, None) => Ok(None),
        (Some("unconfigured"), None, None, None, None) => Ok(Some(UpstreamState::Unconfigured)),
        (Some("gone"), Some(remote), Some(branch), None, None) => Ok(Some(UpstreamState::Gone(
            Upstream::new(Remote::new(remote), Branch::new(branch)),
        ))),
        (Some("tracking"), Some(remote), Some(branch), Some(ahead), Some(behind)) => {
            Ok(Some(UpstreamState::Tracking {
                upstream: Upstream::new(Remote::new(remote), Branch::new(branch)),
                divergence: UpstreamDivergence::new(decode_u64(ahead)?, decode_u64(behind)?),
            }))
        }
        _ => Err(invalid_data()),
    }
}

fn decode_u64(value: Vec<u8>) -> PortResult<u64> {
    let bytes: [u8; 8] = value.try_into().map_err(|_| invalid_data())?;
    Ok(u64::from_be_bytes(bytes))
}

struct RepositoryAttemptRow {
    attempt_id: i64,
    run_id: String,
    repository_id: RepositoryId,
    observed_at_seconds: i64,
    observed_at_nanos: i64,
    coverage: String,
    outcome: String,
    failure_kind: Option<String>,
    exit_code: Option<i32>,
    remote_count: Option<i64>,
}

impl RepositoryAttemptRow {
    fn metadata(&self) -> PortResult<ObservationMetadata> {
        Ok(ObservationMetadata::new(
            ObservationRunId::from(self.run_id.clone()),
            ObservationKey::repository_remotes(self.repository_id.clone()),
            Freshness::new(decode_system_time(
                self.observed_at_seconds,
                self.observed_at_nanos,
            )?),
            decode_coverage(&self.coverage)?,
        ))
    }

    fn into_attempt(self, remotes: Vec<Remote>) -> PortResult<ObservationAttempt<Vec<Remote>>> {
        let metadata = self.metadata()?;
        match self.outcome.as_str() {
            "failure" => {
                if !remotes.is_empty() || self.remote_count.is_some() {
                    return Err(invalid_data());
                }
                Ok(ObservationAttempt::Failed(ObservationFailure::new(
                    metadata,
                    decode_failure(self.failure_kind.as_deref(), self.exit_code)?,
                )))
            }
            "success" => {
                if self.failure_kind.is_some() || self.exit_code.is_some() {
                    return Err(invalid_data());
                }
                if i64::try_from(remotes.len()).map_err(|_| invalid_data())?
                    != self.remote_count.ok_or_else(invalid_data)?
                {
                    return Err(invalid_data());
                }
                Ok(ObservationAttempt::Succeeded(Observation::new(
                    metadata, remotes,
                )))
            }
            _ => Err(invalid_data()),
        }
    }
}

fn read_repository_attempt(
    row: &Row<'_>,
    repository_id: &RepositoryId,
) -> rusqlite::Result<RepositoryAttemptRow> {
    Ok(RepositoryAttemptRow {
        attempt_id: row.get(0)?,
        run_id: row.get(1)?,
        repository_id: repository_id.clone(),
        observed_at_seconds: row.get(2)?,
        observed_at_nanos: row.get(3)?,
        coverage: row.get(4)?,
        outcome: row.get(5)?,
        failure_kind: row.get(6)?,
        exit_code: row.get(7)?,
        remote_count: row.get(8)?,
    })
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use super::{decode_system_time, encode_system_time};

    #[test]
    fn sqlite_timestamp_round_trip_preserves_subsecond_values_on_both_sides_of_epoch() {
        let values = [
            UNIX_EPOCH + Duration::new(42, 123_456_789),
            UNIX_EPOCH - Duration::new(1, 500_000_000),
        ];

        for value in values {
            let encoded = encode_system_time(value).expect("timestamp must encode");
            let decoded =
                decode_system_time(encoded.seconds, encoded.nanos).expect("timestamp must decode");
            assert_eq!(decoded, value);
        }
    }
}
