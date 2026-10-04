// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use std::fmt;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;

use bulls_core::{
    Head, LocationAvailability, ObservationCoverage, ObservationFailureKind, UpstreamRelation,
    UpstreamState, WorktreeGitState,
};
use serde::{Deserialize, Serialize};

use crate::{
    AdvisoryCode, AdvisoryEvidence, AdvisoryFreshness, AdvisoryProjection, AdvisoryRuleUnknown,
    AdvisorySubject, AdvisoryUnknownReason, Knowledge, LocationProjection,
    ObservationAttemptOutcomeProjection, ObservationAttemptProjection, ObservationStatusProjection,
    ObservationSuccessProjection, OverviewProjection, RepositoryDetailProjection,
    RepositoryListItemProjection, RepositoryListProjection, WorkspaceMirror, WorkspaceRevision,
    WorktreeProjection,
};

pub const READ_PROTOCOL_SCHEMA_VERSION: u16 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProtocolBuildError {
    CountOutOfRange,
    TimestampOutOfRange,
    InconsistentWorkspaceRevision,
}

impl fmt::Display for ProtocolBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::CountOutOfRange => "protocol count is outside the supported range",
            Self::TimestampOutOfRange => "protocol timestamp is outside the supported range",
            Self::InconsistentWorkspaceRevision => {
                "protocol source contains inconsistent workspace revisions"
            }
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for ProtocolBuildError {}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProtocolDocument<T> {
    pub schema_version: u16,
    pub workspace_revision: u64,
    pub result: ProtocolResultMetadata,
    pub payload: T,
}

impl<T> ProtocolDocument<T> {
    fn new(revision: WorkspaceRevision, result: ProtocolResultMetadata, payload: T) -> Self {
        Self {
            schema_version: READ_PROTOCOL_SCHEMA_VERSION,
            workspace_revision: revision.value(),
            result,
            payload,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolResultCompleteness {
    Complete,
    Partial,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProtocolResultMetadata {
    pub completeness: ProtocolResultCompleteness,
    pub unknown_evidence_count: u64,
}

impl ProtocolResultMetadata {
    const fn from_unknown_evidence_count(unknown_evidence_count: u64) -> Self {
        let completeness = if unknown_evidence_count == 0 {
            ProtocolResultCompleteness::Complete
        } else {
            ProtocolResultCompleteness::Partial
        };
        Self {
            completeness,
            unknown_evidence_count,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProtocolTimestamp {
    pub unix_seconds: i64,
    pub nanoseconds: u32,
}

impl TryFrom<SystemTime> for ProtocolTimestamp {
    type Error = ProtocolBuildError;

    fn try_from(value: SystemTime) -> Result<Self, Self::Error> {
        match value.duration_since(UNIX_EPOCH) {
            Ok(duration) => Ok(Self {
                unix_seconds: i64::try_from(duration.as_secs())
                    .map_err(|_| ProtocolBuildError::TimestampOutOfRange)?,
                nanoseconds: duration.subsec_nanos(),
            }),
            Err(error) => {
                let duration = error.duration();
                let whole_seconds = i64::try_from(duration.as_secs())
                    .map_err(|_| ProtocolBuildError::TimestampOutOfRange)?;
                if duration.subsec_nanos() == 0 {
                    return Ok(Self {
                        unix_seconds: whole_seconds
                            .checked_neg()
                            .ok_or(ProtocolBuildError::TimestampOutOfRange)?,
                        nanoseconds: 0,
                    });
                }

                Ok(Self {
                    unix_seconds: whole_seconds
                        .checked_add(1)
                        .and_then(i64::checked_neg)
                        .ok_or(ProtocolBuildError::TimestampOutOfRange)?,
                    nanoseconds: 1_000_000_000 - duration.subsec_nanos(),
                })
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "encoding", content = "value", rename_all = "snake_case")]
pub enum ProtocolPath {
    Utf8(String),
    UnixBytes(Vec<u8>),
    WindowsWide(Vec<u16>),
    Lossy(String),
}

impl ProtocolPath {
    pub(crate) fn from_path(path: &Path) -> Self {
        if let Some(value) = path.to_str() {
            return Self::Utf8(value.to_owned());
        }

        #[cfg(unix)]
        {
            Self::UnixBytes(path.as_os_str().as_bytes().to_vec())
        }

        #[cfg(windows)]
        {
            Self::WindowsWide(path.as_os_str().encode_wide().collect())
        }

        #[cfg(not(any(unix, windows)))]
        {
            Self::Lossy(path.to_string_lossy().into_owned())
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolLocationAvailability {
    Available,
    Missing,
    Offline,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProtocolLocation {
    pub id: String,
    pub path: ProtocolPath,
    pub availability: ProtocolLocationAvailability,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProtocolRemote {
    pub name: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolObservationCoverage {
    Complete,
    Partial,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProtocolObservationFailureKind {
    SubjectUnavailable,
    ProcessExited { exit_code: i32 },
    TimedOut,
    Cancelled,
    OutputLimitExceeded,
    InvalidMachineOutput,
    SubjectDisappeared,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ProtocolObservationAttemptOutcome {
    Succeeded,
    Failed {
        failure: ProtocolObservationFailureKind,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProtocolObservationAttempt {
    pub observed_at: ProtocolTimestamp,
    pub coverage: ProtocolObservationCoverage,
    pub outcome: ProtocolObservationAttemptOutcome,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProtocolObservationSuccess {
    pub observed_at: ProtocolTimestamp,
    pub coverage: ProtocolObservationCoverage,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProtocolObservationStatus {
    pub latest_attempt: Option<ProtocolObservationAttempt>,
    pub latest_success: Option<ProtocolObservationSuccess>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProtocolHead {
    Unborn { branch: String },
    Branch { branch: String },
    Detached,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolUpstreamRelation {
    Synchronized,
    Ahead,
    Behind,
    Diverged,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProtocolUpstreamState {
    Unconfigured,
    Gone {
        remote: String,
        branch: String,
    },
    Tracking {
        remote: String,
        branch: String,
        ahead: u64,
        behind: u64,
        relation: ProtocolUpstreamRelation,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProtocolWorktreeChanges {
    pub staged: bool,
    pub unstaged: bool,
    pub untracked: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProtocolWorktreeGitState {
    pub head: ProtocolHead,
    pub upstream: Option<ProtocolUpstreamState>,
    pub changes: ProtocolWorktreeChanges,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProtocolWorktree {
    pub id: String,
    pub repository_id: String,
    pub location: ProtocolLocation,
    pub git_state: Option<ProtocolWorktreeGitState>,
    pub observation: ProtocolObservationStatus,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", content = "value", rename_all = "snake_case")]
pub enum ProtocolKnowledgeBool {
    Known(bool),
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProtocolRepositoryListItem {
    pub id: String,
    pub locations: Vec<ProtocolLocation>,
    pub location_count: u64,
    pub available_location_count: u64,
    pub worktree_count: u64,
    pub has_remotes: ProtocolKnowledgeBool,
    pub has_local_work: ProtocolKnowledgeBool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProtocolRepositoryList {
    pub total_matching: u64,
    pub offset: u64,
    pub items: Vec<ProtocolRepositoryListItem>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProtocolRepositoryDetail {
    pub id: String,
    pub locations: Vec<ProtocolLocation>,
    pub remotes: Option<Vec<ProtocolRemote>>,
    pub remotes_observation: ProtocolObservationStatus,
    pub worktrees: Vec<ProtocolWorktree>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProtocolOverview {
    pub repository_count: u64,
    pub available_repository_count: u64,
    pub worktree_count: u64,
    pub repositories_with_local_work_count: u64,
    pub repositories_with_unknown_local_work_count: u64,
    pub repositories_without_remotes_count: u64,
    pub repositories_with_unknown_remotes_count: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProtocolOverviewView {
    pub overview: ProtocolOverview,
    pub advisories: ProtocolAdvisoryProjection,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProtocolRepositoryDetailView {
    pub repository: ProtocolRepositoryDetail,
    pub advisories: ProtocolAdvisoryProjection,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProtocolWorkspaceMirror {
    pub overview: ProtocolOverview,
    pub repositories: Vec<ProtocolRepositoryDetail>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolAdvisoryCode {
    RepositoryWithoutRemotes,
    RepositoryHasMultipleWorktrees,
    WorktreeWithoutUpstream,
    LocalCommitsAheadOfKnownUpstream,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProtocolAdvisorySubject {
    Workspace,
    Repository { repository_id: String },
    Worktree { worktree_id: String },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProtocolAdvisoryFreshness {
    WorkspaceRevision { workspace_revision: u64 },
    ObservedAt { observed_at: ProtocolTimestamp },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProtocolAdvisoryEvidence {
    RepositoryRemoteCount { remote_count: u64 },
    RepositoryWorktreeCount { worktree_count: u64 },
    WorktreeUpstreamUnconfigured,
    KnownUpstreamDivergence { ahead: u64, behind: u64 },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProtocolAdvisory {
    pub code: ProtocolAdvisoryCode,
    pub subject: ProtocolAdvisorySubject,
    pub evidence: ProtocolAdvisoryEvidence,
    pub freshness: ProtocolAdvisoryFreshness,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolAdvisoryUnknownReason {
    MissingObservation,
    PartialObservation,
    MissingEvidence,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProtocolAdvisoryRuleUnknown {
    pub code: ProtocolAdvisoryCode,
    pub subject: ProtocolAdvisorySubject,
    pub reason: ProtocolAdvisoryUnknownReason,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProtocolAdvisoryProjection {
    pub advisories: Vec<ProtocolAdvisory>,
    pub unknown_rules: Vec<ProtocolAdvisoryRuleUnknown>,
}

impl TryFrom<&OverviewProjection> for ProtocolDocument<ProtocolOverview> {
    type Error = ProtocolBuildError;

    fn try_from(value: &OverviewProjection) -> Result<Self, Self::Error> {
        let unknown_evidence_count = overview_unknown_evidence_count(value)?;
        Ok(Self::new(
            value.revision(),
            ProtocolResultMetadata::from_unknown_evidence_count(unknown_evidence_count),
            protocol_overview(value)?,
        ))
    }
}

impl TryFrom<&RepositoryListProjection> for ProtocolDocument<ProtocolRepositoryList> {
    type Error = ProtocolBuildError;

    fn try_from(value: &RepositoryListProjection) -> Result<Self, Self::Error> {
        let mut unknown_evidence_count = 0_u64;
        let items = value
            .items()
            .iter()
            .map(|item| {
                unknown_evidence_count = checked_add(
                    unknown_evidence_count,
                    knowledge_unknown_count(item.has_remotes())
                        + knowledge_unknown_count(item.has_local_work()),
                )?;
                protocol_repository_list_item(item)
            })
            .collect::<Result<Vec<_>, ProtocolBuildError>>()?;

        Ok(Self::new(
            value.revision(),
            ProtocolResultMetadata::from_unknown_evidence_count(unknown_evidence_count),
            ProtocolRepositoryList {
                total_matching: usize_to_u64(value.total_matching())?,
                offset: usize_to_u64(value.offset())?,
                items,
            },
        ))
    }
}

impl TryFrom<&RepositoryDetailProjection> for ProtocolDocument<ProtocolRepositoryDetail> {
    type Error = ProtocolBuildError;

    fn try_from(value: &RepositoryDetailProjection) -> Result<Self, Self::Error> {
        validate_repository_revision(value, value.revision())?;
        let unknown_evidence_count = repository_unknown_evidence_count(value)?;
        Ok(Self::new(
            value.revision(),
            ProtocolResultMetadata::from_unknown_evidence_count(unknown_evidence_count),
            protocol_repository_detail(value)?,
        ))
    }
}

impl TryFrom<&WorktreeProjection> for ProtocolDocument<ProtocolWorktree> {
    type Error = ProtocolBuildError;

    fn try_from(value: &WorktreeProjection) -> Result<Self, Self::Error> {
        Ok(Self::new(
            value.revision(),
            ProtocolResultMetadata::from_unknown_evidence_count(observation_unknown_count(
                value.observation(),
            )),
            protocol_worktree(value)?,
        ))
    }
}

impl TryFrom<&WorkspaceMirror> for ProtocolDocument<ProtocolWorkspaceMirror> {
    type Error = ProtocolBuildError;

    fn try_from(value: &WorkspaceMirror) -> Result<Self, Self::Error> {
        let revision = value.revision();
        if value.overview().revision() != revision {
            return Err(ProtocolBuildError::InconsistentWorkspaceRevision);
        }

        let mut unknown_evidence_count = 0_u64;
        let repositories = value
            .repositories()
            .iter()
            .map(|repository| {
                validate_repository_revision(repository, revision)?;
                unknown_evidence_count = checked_add(
                    unknown_evidence_count,
                    repository_unknown_evidence_count(repository)?,
                )?;
                protocol_repository_detail(repository)
            })
            .collect::<Result<Vec<_>, ProtocolBuildError>>()?;

        Ok(Self::new(
            revision,
            ProtocolResultMetadata::from_unknown_evidence_count(unknown_evidence_count),
            ProtocolWorkspaceMirror {
                overview: protocol_overview(value.overview())?,
                repositories,
            },
        ))
    }
}

impl TryFrom<&AdvisoryProjection> for ProtocolDocument<ProtocolAdvisoryProjection> {
    type Error = ProtocolBuildError;

    fn try_from(value: &AdvisoryProjection) -> Result<Self, Self::Error> {
        let payload = protocol_advisory_projection(value)?;
        let unknown_evidence_count = usize_to_u64(payload.unknown_rules.len())?;

        Ok(Self::new(
            value.revision(),
            ProtocolResultMetadata::from_unknown_evidence_count(unknown_evidence_count),
            payload,
        ))
    }
}

impl TryFrom<(&OverviewProjection, &AdvisoryProjection)>
    for ProtocolDocument<ProtocolOverviewView>
{
    type Error = ProtocolBuildError;

    fn try_from(
        (overview, advisories): (&OverviewProjection, &AdvisoryProjection),
    ) -> Result<Self, Self::Error> {
        if overview.revision() != advisories.revision() {
            return Err(ProtocolBuildError::InconsistentWorkspaceRevision);
        }

        let advisory_payload = protocol_advisory_projection(advisories)?;
        let unknown_evidence_count = checked_add(
            overview_unknown_evidence_count(overview)?,
            usize_to_u64(advisory_payload.unknown_rules.len())?,
        )?;

        Ok(Self::new(
            overview.revision(),
            ProtocolResultMetadata::from_unknown_evidence_count(unknown_evidence_count),
            ProtocolOverviewView {
                overview: protocol_overview(overview)?,
                advisories: advisory_payload,
            },
        ))
    }
}

impl TryFrom<(&RepositoryDetailProjection, &AdvisoryProjection)>
    for ProtocolDocument<ProtocolRepositoryDetailView>
{
    type Error = ProtocolBuildError;

    fn try_from(
        (repository, advisories): (&RepositoryDetailProjection, &AdvisoryProjection),
    ) -> Result<Self, Self::Error> {
        validate_repository_revision(repository, repository.revision())?;
        if repository.revision() != advisories.revision() {
            return Err(ProtocolBuildError::InconsistentWorkspaceRevision);
        }

        let advisory_payload = protocol_repository_advisory_projection(repository, advisories)?;
        let unknown_evidence_count = checked_add(
            repository_unknown_evidence_count(repository)?,
            usize_to_u64(advisory_payload.unknown_rules.len())?,
        )?;

        Ok(Self::new(
            repository.revision(),
            ProtocolResultMetadata::from_unknown_evidence_count(unknown_evidence_count),
            ProtocolRepositoryDetailView {
                repository: protocol_repository_detail(repository)?,
                advisories: advisory_payload,
            },
        ))
    }
}

fn protocol_overview(value: &OverviewProjection) -> Result<ProtocolOverview, ProtocolBuildError> {
    Ok(ProtocolOverview {
        repository_count: usize_to_u64(value.repository_count())?,
        available_repository_count: usize_to_u64(value.available_repository_count())?,
        worktree_count: usize_to_u64(value.worktree_count())?,
        repositories_with_local_work_count: usize_to_u64(
            value.repositories_with_local_work_count(),
        )?,
        repositories_with_unknown_local_work_count: usize_to_u64(
            value.repositories_with_unknown_local_work_count(),
        )?,
        repositories_without_remotes_count: usize_to_u64(
            value.repositories_without_remotes_count(),
        )?,
        repositories_with_unknown_remotes_count: usize_to_u64(
            value.repositories_with_unknown_remotes_count(),
        )?,
    })
}

fn protocol_repository_list_item(
    value: &RepositoryListItemProjection,
) -> Result<ProtocolRepositoryListItem, ProtocolBuildError> {
    Ok(ProtocolRepositoryListItem {
        id: value.id().as_str().to_owned(),
        locations: value.locations().iter().map(protocol_location).collect(),
        location_count: usize_to_u64(value.location_count())?,
        available_location_count: usize_to_u64(value.available_location_count())?,
        worktree_count: usize_to_u64(value.worktree_count())?,
        has_remotes: protocol_knowledge_bool(value.has_remotes()),
        has_local_work: protocol_knowledge_bool(value.has_local_work()),
    })
}

fn protocol_repository_detail(
    value: &RepositoryDetailProjection,
) -> Result<ProtocolRepositoryDetail, ProtocolBuildError> {
    Ok(ProtocolRepositoryDetail {
        id: value.id().as_str().to_owned(),
        locations: value.locations().iter().map(protocol_location).collect(),
        remotes: value.remotes().map(|remotes| {
            remotes
                .iter()
                .map(|remote| ProtocolRemote {
                    name: remote.name().to_owned(),
                })
                .collect()
        }),
        remotes_observation: protocol_observation_status(value.remotes_observation())?,
        worktrees: value
            .worktrees()
            .iter()
            .map(protocol_worktree)
            .collect::<Result<Vec<_>, ProtocolBuildError>>()?,
    })
}

fn protocol_location(value: &LocationProjection) -> ProtocolLocation {
    ProtocolLocation {
        id: value.id().as_str().to_owned(),
        path: ProtocolPath::from_path(value.path()),
        availability: match value.availability() {
            LocationAvailability::Available => ProtocolLocationAvailability::Available,
            LocationAvailability::Missing => ProtocolLocationAvailability::Missing,
            LocationAvailability::Offline => ProtocolLocationAvailability::Offline,
        },
    }
}

fn protocol_worktree(value: &WorktreeProjection) -> Result<ProtocolWorktree, ProtocolBuildError> {
    Ok(ProtocolWorktree {
        id: value.id().as_str().to_owned(),
        repository_id: value.repository_id().as_str().to_owned(),
        location: protocol_location(value.location()),
        git_state: value.git_state().map(protocol_git_state),
        observation: protocol_observation_status(value.observation())?,
    })
}

fn protocol_git_state(value: &WorktreeGitState) -> ProtocolWorktreeGitState {
    let head = match value.head() {
        Head::Unborn(branch) => ProtocolHead::Unborn {
            branch: branch.name().to_owned(),
        },
        Head::Branch(branch) => ProtocolHead::Branch {
            branch: branch.name().to_owned(),
        },
        Head::Detached => ProtocolHead::Detached,
    };
    let upstream = value.upstream().map(|upstream| match upstream {
        UpstreamState::Unconfigured => ProtocolUpstreamState::Unconfigured,
        UpstreamState::Gone(upstream) => ProtocolUpstreamState::Gone {
            remote: upstream.remote().name().to_owned(),
            branch: upstream.branch().name().to_owned(),
        },
        UpstreamState::Tracking {
            upstream,
            divergence,
        } => ProtocolUpstreamState::Tracking {
            remote: upstream.remote().name().to_owned(),
            branch: upstream.branch().name().to_owned(),
            ahead: divergence.ahead(),
            behind: divergence.behind(),
            relation: protocol_upstream_relation(divergence.relation()),
        },
    });
    let changes = value.changes();

    ProtocolWorktreeGitState {
        head,
        upstream,
        changes: ProtocolWorktreeChanges {
            staged: changes.staged(),
            unstaged: changes.unstaged(),
            untracked: changes.untracked(),
        },
    }
}

const fn protocol_upstream_relation(value: UpstreamRelation) -> ProtocolUpstreamRelation {
    match value {
        UpstreamRelation::Synchronized => ProtocolUpstreamRelation::Synchronized,
        UpstreamRelation::Ahead => ProtocolUpstreamRelation::Ahead,
        UpstreamRelation::Behind => ProtocolUpstreamRelation::Behind,
        UpstreamRelation::Diverged => ProtocolUpstreamRelation::Diverged,
    }
}

fn protocol_observation_status(
    value: &ObservationStatusProjection,
) -> Result<ProtocolObservationStatus, ProtocolBuildError> {
    Ok(ProtocolObservationStatus {
        latest_attempt: value
            .latest_attempt()
            .map(protocol_observation_attempt)
            .transpose()?,
        latest_success: value
            .latest_success()
            .map(protocol_observation_success)
            .transpose()?,
    })
}

fn protocol_observation_attempt(
    value: ObservationAttemptProjection,
) -> Result<ProtocolObservationAttempt, ProtocolBuildError> {
    let outcome = match value.outcome() {
        ObservationAttemptOutcomeProjection::Succeeded => {
            ProtocolObservationAttemptOutcome::Succeeded
        }
        ObservationAttemptOutcomeProjection::Failed(failure) => {
            ProtocolObservationAttemptOutcome::Failed {
                failure: protocol_observation_failure(failure),
            }
        }
    };

    Ok(ProtocolObservationAttempt {
        observed_at: value.observed_at().try_into()?,
        coverage: protocol_observation_coverage(value.coverage()),
        outcome,
    })
}

fn protocol_observation_success(
    value: ObservationSuccessProjection,
) -> Result<ProtocolObservationSuccess, ProtocolBuildError> {
    Ok(ProtocolObservationSuccess {
        observed_at: value.observed_at().try_into()?,
        coverage: protocol_observation_coverage(value.coverage()),
    })
}

pub(crate) const fn protocol_observation_coverage(
    value: ObservationCoverage,
) -> ProtocolObservationCoverage {
    match value {
        ObservationCoverage::Complete => ProtocolObservationCoverage::Complete,
        ObservationCoverage::Partial => ProtocolObservationCoverage::Partial,
    }
}

pub(crate) const fn protocol_observation_failure(
    value: ObservationFailureKind,
) -> ProtocolObservationFailureKind {
    match value {
        ObservationFailureKind::SubjectUnavailable => {
            ProtocolObservationFailureKind::SubjectUnavailable
        }
        ObservationFailureKind::ProcessExited { exit_code } => {
            ProtocolObservationFailureKind::ProcessExited { exit_code }
        }
        ObservationFailureKind::TimedOut => ProtocolObservationFailureKind::TimedOut,
        ObservationFailureKind::Cancelled => ProtocolObservationFailureKind::Cancelled,
        ObservationFailureKind::OutputLimitExceeded => {
            ProtocolObservationFailureKind::OutputLimitExceeded
        }
        ObservationFailureKind::InvalidMachineOutput => {
            ProtocolObservationFailureKind::InvalidMachineOutput
        }
        ObservationFailureKind::SubjectDisappeared => {
            ProtocolObservationFailureKind::SubjectDisappeared
        }
    }
}

const fn protocol_knowledge_bool(value: &Knowledge<bool>) -> ProtocolKnowledgeBool {
    match value {
        Knowledge::Known(value) => ProtocolKnowledgeBool::Known(*value),
        Knowledge::Unknown => ProtocolKnowledgeBool::Unknown,
    }
}

fn protocol_advisory_projection(
    value: &AdvisoryProjection,
) -> Result<ProtocolAdvisoryProjection, ProtocolBuildError> {
    Ok(ProtocolAdvisoryProjection {
        advisories: value
            .advisories()
            .iter()
            .map(protocol_advisory)
            .collect::<Result<Vec<_>, ProtocolBuildError>>()?,
        unknown_rules: value
            .unknown_rules()
            .iter()
            .map(protocol_advisory_unknown)
            .collect(),
    })
}

fn protocol_repository_advisory_projection(
    repository: &RepositoryDetailProjection,
    value: &AdvisoryProjection,
) -> Result<ProtocolAdvisoryProjection, ProtocolBuildError> {
    Ok(ProtocolAdvisoryProjection {
        advisories: value
            .advisories()
            .iter()
            .filter(|advisory| advisory_subject_matches_repository(advisory.subject(), repository))
            .map(protocol_advisory)
            .collect::<Result<Vec<_>, ProtocolBuildError>>()?,
        unknown_rules: value
            .unknown_rules()
            .iter()
            .filter(|unknown| advisory_subject_matches_repository(unknown.subject(), repository))
            .map(protocol_advisory_unknown)
            .collect(),
    })
}

fn advisory_subject_matches_repository(
    subject: &AdvisorySubject,
    repository: &RepositoryDetailProjection,
) -> bool {
    match subject {
        AdvisorySubject::Workspace => false,
        AdvisorySubject::Repository(repository_id) => repository_id == repository.id(),
        AdvisorySubject::Worktree(worktree_id) => repository
            .worktrees()
            .iter()
            .any(|worktree| worktree.id() == worktree_id),
    }
}

fn protocol_advisory(value: &crate::Advisory) -> Result<ProtocolAdvisory, ProtocolBuildError> {
    Ok(ProtocolAdvisory {
        code: protocol_advisory_code(value.code()),
        subject: protocol_advisory_subject(value.subject()),
        evidence: protocol_advisory_evidence(value.evidence())?,
        freshness: protocol_advisory_freshness(value.freshness())?,
    })
}

fn protocol_advisory_unknown(value: &AdvisoryRuleUnknown) -> ProtocolAdvisoryRuleUnknown {
    ProtocolAdvisoryRuleUnknown {
        code: protocol_advisory_code(value.code()),
        subject: protocol_advisory_subject(value.subject()),
        reason: match value.reason() {
            AdvisoryUnknownReason::MissingObservation => {
                ProtocolAdvisoryUnknownReason::MissingObservation
            }
            AdvisoryUnknownReason::PartialObservation => {
                ProtocolAdvisoryUnknownReason::PartialObservation
            }
            AdvisoryUnknownReason::MissingEvidence => {
                ProtocolAdvisoryUnknownReason::MissingEvidence
            }
        },
    }
}

const fn protocol_advisory_code(value: AdvisoryCode) -> ProtocolAdvisoryCode {
    match value {
        AdvisoryCode::RepositoryWithoutRemotes => ProtocolAdvisoryCode::RepositoryWithoutRemotes,
        AdvisoryCode::RepositoryHasMultipleWorktrees => {
            ProtocolAdvisoryCode::RepositoryHasMultipleWorktrees
        }
        AdvisoryCode::WorktreeWithoutUpstream => ProtocolAdvisoryCode::WorktreeWithoutUpstream,
        AdvisoryCode::LocalCommitsAheadOfKnownUpstream => {
            ProtocolAdvisoryCode::LocalCommitsAheadOfKnownUpstream
        }
    }
}

fn protocol_advisory_subject(value: &AdvisorySubject) -> ProtocolAdvisorySubject {
    match value {
        AdvisorySubject::Workspace => ProtocolAdvisorySubject::Workspace,
        AdvisorySubject::Repository(repository_id) => ProtocolAdvisorySubject::Repository {
            repository_id: repository_id.as_str().to_owned(),
        },
        AdvisorySubject::Worktree(worktree_id) => ProtocolAdvisorySubject::Worktree {
            worktree_id: worktree_id.as_str().to_owned(),
        },
    }
}

fn protocol_advisory_freshness(
    value: AdvisoryFreshness,
) -> Result<ProtocolAdvisoryFreshness, ProtocolBuildError> {
    match value {
        AdvisoryFreshness::WorkspaceRevision(revision) => {
            Ok(ProtocolAdvisoryFreshness::WorkspaceRevision {
                workspace_revision: revision.value(),
            })
        }
        AdvisoryFreshness::ObservedAt(observed_at) => Ok(ProtocolAdvisoryFreshness::ObservedAt {
            observed_at: observed_at.try_into()?,
        }),
    }
}

fn protocol_advisory_evidence(
    value: AdvisoryEvidence,
) -> Result<ProtocolAdvisoryEvidence, ProtocolBuildError> {
    match value {
        AdvisoryEvidence::RepositoryRemoteCount { remote_count } => {
            Ok(ProtocolAdvisoryEvidence::RepositoryRemoteCount {
                remote_count: usize_to_u64(remote_count)?,
            })
        }
        AdvisoryEvidence::RepositoryWorktreeCount { worktree_count } => {
            Ok(ProtocolAdvisoryEvidence::RepositoryWorktreeCount {
                worktree_count: usize_to_u64(worktree_count)?,
            })
        }
        AdvisoryEvidence::WorktreeUpstreamUnconfigured => {
            Ok(ProtocolAdvisoryEvidence::WorktreeUpstreamUnconfigured)
        }
        AdvisoryEvidence::KnownUpstreamDivergence { ahead, behind } => {
            Ok(ProtocolAdvisoryEvidence::KnownUpstreamDivergence { ahead, behind })
        }
    }
}

fn overview_unknown_evidence_count(value: &OverviewProjection) -> Result<u64, ProtocolBuildError> {
    usize_to_u64(
        value
            .repositories_with_unknown_local_work_count()
            .checked_add(value.repositories_with_unknown_remotes_count())
            .ok_or(ProtocolBuildError::CountOutOfRange)?,
    )
}

fn repository_unknown_evidence_count(
    value: &RepositoryDetailProjection,
) -> Result<u64, ProtocolBuildError> {
    let mut count = observation_unknown_count(value.remotes_observation());
    for worktree in value.worktrees() {
        count = checked_add(count, observation_unknown_count(worktree.observation()))?;
    }
    Ok(count)
}

fn observation_unknown_count(value: &ObservationStatusProjection) -> u64 {
    match value.latest_success() {
        Some(success) if success.coverage() == ObservationCoverage::Complete => 0,
        Some(_) | None => 1,
    }
}

const fn knowledge_unknown_count(value: &Knowledge<bool>) -> u64 {
    match value {
        Knowledge::Known(_) => 0,
        Knowledge::Unknown => 1,
    }
}

fn validate_repository_revision(
    value: &RepositoryDetailProjection,
    expected: WorkspaceRevision,
) -> Result<(), ProtocolBuildError> {
    if value.revision() != expected
        || value
            .worktrees()
            .iter()
            .any(|worktree| worktree.revision() != expected)
    {
        return Err(ProtocolBuildError::InconsistentWorkspaceRevision);
    }
    Ok(())
}

pub(crate) fn usize_to_u64(value: usize) -> Result<u64, ProtocolBuildError> {
    u64::try_from(value).map_err(|_| ProtocolBuildError::CountOutOfRange)
}

fn checked_add(left: u64, right: u64) -> Result<u64, ProtocolBuildError> {
    left.checked_add(right)
        .ok_or(ProtocolBuildError::CountOutOfRange)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{Duration, UNIX_EPOCH};

    use bulls_core::{
        Branch, LocationAvailability, LocationId, ObservationCoverage, Remote, RepositoryId,
        Upstream, UpstreamDivergence, UpstreamState, WorktreeChanges, WorktreeGitState, WorktreeId,
    };

    use super::{
        ProtocolAdvisoryCode, ProtocolDocument, ProtocolHead, ProtocolKnowledgeBool,
        ProtocolOverviewView, ProtocolPath, ProtocolRepositoryDetailView, ProtocolRepositoryList,
        ProtocolResultCompleteness, ProtocolTimestamp, ProtocolUpstreamRelation,
        ProtocolUpstreamState, ProtocolWorkspaceMirror, READ_PROTOCOL_SCHEMA_VERSION,
    };
    use crate::{
        AdvisoryEvaluator, Knowledge, LocationProjection, ObservationAttemptOutcomeProjection,
        ObservationAttemptProjection, ObservationStatusProjection, ObservationSuccessProjection,
        OverviewProjection, RepositoryDetailProjection, RepositoryListItemProjection,
        RepositoryListProjection, WorkspaceMirror, WorkspaceRevision, WorktreeProjection,
    };

    fn complete_status(seconds: u64) -> ObservationStatusProjection {
        let observed_at = UNIX_EPOCH + Duration::from_secs(seconds);
        ObservationStatusProjection::new(
            Some(ObservationAttemptProjection::new(
                observed_at,
                ObservationCoverage::Complete,
                ObservationAttemptOutcomeProjection::Succeeded,
            )),
            Some(ObservationSuccessProjection::new(
                observed_at,
                ObservationCoverage::Complete,
            )),
        )
    }

    fn mirror() -> WorkspaceMirror {
        let revision = WorkspaceRevision::new(7);
        let location = LocationProjection::new(
            LocationId::from("location-1"),
            PathBuf::from("/workspace/repository"),
            LocationAvailability::Available,
        );
        let worktree = WorktreeProjection::new(
            revision,
            WorktreeId::from("worktree-1"),
            RepositoryId::from("repository-1"),
            location.clone(),
            Some(
                WorktreeGitState::branch(
                    Branch::new("main"),
                    UpstreamState::Tracking {
                        upstream: Upstream::new(Remote::new("origin"), Branch::new("main")),
                        divergence: UpstreamDivergence::new(2, 1),
                    },
                )
                .with_changes(WorktreeChanges::new(true, false, true)),
            ),
            complete_status(42),
        );
        let repository = RepositoryDetailProjection::new(
            revision,
            RepositoryId::from("repository-1"),
            vec![location],
            Some(vec![Remote::new("origin")]),
            complete_status(41),
            vec![worktree],
        );
        let overview = OverviewProjection::new(revision, 1, 1, 1, 1, 0, 0, 0);

        WorkspaceMirror::new(revision, overview, vec![repository])
    }

    #[test]
    fn mirror_protocol_is_versioned_and_preserves_machine_semantics() {
        let document = ProtocolDocument::<ProtocolWorkspaceMirror>::try_from(&mirror())
            .expect("mirror protocol must build");

        assert_eq!(document.schema_version, READ_PROTOCOL_SCHEMA_VERSION);
        assert_eq!(document.workspace_revision, 7);
        assert_eq!(
            document.result.completeness,
            ProtocolResultCompleteness::Complete
        );
        assert_eq!(document.result.unknown_evidence_count, 0);
        assert_eq!(document.payload.repositories[0].id, "repository-1");
        assert_eq!(
            document.payload.repositories[0].locations[0].path,
            ProtocolPath::Utf8("/workspace/repository".to_owned())
        );
        assert_eq!(
            document.payload.repositories[0].worktrees[0]
                .git_state
                .as_ref()
                .expect("git state must be present")
                .head,
            ProtocolHead::Branch {
                branch: "main".to_owned()
            }
        );
        assert!(matches!(
            document.payload.repositories[0].worktrees[0]
                .git_state
                .as_ref()
                .expect("git state must be present")
                .upstream,
            Some(ProtocolUpstreamState::Tracking {
                ahead: 2,
                behind: 1,
                relation: ProtocolUpstreamRelation::Diverged,
                ..
            })
        ));

        let serialized = toml::Value::try_from(&document)
            .expect("mirror protocol serialization must remain representable");
        assert_eq!(serialized["schema_version"].as_integer(), Some(1));
        assert_eq!(serialized["workspace_revision"].as_integer(), Some(7));
        assert_eq!(
            serialized["result"]["completeness"].as_str(),
            Some("complete")
        );
        assert_eq!(
            serialized["payload"]["repositories"][0]["locations"][0]["path"]["encoding"].as_str(),
            Some("utf8")
        );
        assert_eq!(
            serialized["payload"]["repositories"][0]["worktrees"][0]["git_state"]["head"]["kind"]
                .as_str(),
            Some("branch")
        );
        assert_eq!(
            serialized["payload"]["repositories"][0]["worktrees"][0]["git_state"]
                ["upstream"]["kind"]
                .as_str(),
            Some("tracking")
        );
    }

    #[test]
    fn list_protocol_marks_unknown_evidence_as_partial() {
        let projection = RepositoryListProjection::new(
            WorkspaceRevision::new(9),
            1,
            0,
            vec![RepositoryListItemProjection::new(
                RepositoryId::from("repository-1"),
                vec![LocationProjection::new(
                    LocationId::from("location-1"),
                    PathBuf::from("/workspace/repository"),
                    LocationAvailability::Available,
                )],
                1,
                Knowledge::Unknown,
                Knowledge::Known(false),
            )],
        );

        let document = ProtocolDocument::<ProtocolRepositoryList>::try_from(&projection)
            .expect("list protocol must build");

        assert_eq!(
            document.result.completeness,
            ProtocolResultCompleteness::Partial
        );
        assert_eq!(document.result.unknown_evidence_count, 1);
        assert_eq!(
            document.payload.items[0].has_remotes,
            ProtocolKnowledgeBool::Unknown
        );
        assert_eq!(
            document.payload.items[0].locations[0].path,
            ProtocolPath::Utf8("/workspace/repository".to_owned())
        );

        let serialized = toml::Value::try_from(&document)
            .expect("repository list protocol serialization must remain representable");
        assert_eq!(
            serialized["result"]["completeness"].as_str(),
            Some("partial")
        );
        assert_eq!(
            serialized["result"]["unknown_evidence_count"].as_integer(),
            Some(1)
        );
        assert_eq!(
            serialized["payload"]["items"][0]["has_remotes"]["state"].as_str(),
            Some("unknown")
        );
        assert!(
            serialized["payload"]["items"][0]["has_remotes"]
                .get("value")
                .is_none()
        );
        assert_eq!(
            serialized["payload"]["items"][0]["has_local_work"]["state"].as_str(),
            Some("known")
        );
        assert_eq!(
            serialized["payload"]["items"][0]["has_local_work"]["value"].as_bool(),
            Some(false)
        );
    }

    #[test]
    fn advisory_protocol_keeps_codes_evidence_and_unknown_rules_structured() {
        let mirror = mirror();
        let projection = AdvisoryEvaluator::evaluate(&mirror);
        let document =
            super::ProtocolDocument::<super::ProtocolAdvisoryProjection>::try_from(&projection)
                .expect("advisory protocol must build");

        assert!(document.payload.advisories.iter().any(
            |advisory| advisory.code == ProtocolAdvisoryCode::LocalCommitsAheadOfKnownUpstream
        ));
        assert!(document.payload.unknown_rules.is_empty());
        assert_eq!(
            document.result.completeness,
            ProtocolResultCompleteness::Complete
        );
    }

    #[test]
    fn combined_overview_protocol_preserves_advisories_in_one_revisioned_document() {
        let mirror = mirror();
        let advisories = AdvisoryEvaluator::evaluate(&mirror);
        let document =
            ProtocolDocument::<ProtocolOverviewView>::try_from((mirror.overview(), &advisories))
                .expect("combined overview protocol must build");

        assert_eq!(document.workspace_revision, mirror.revision().value());
        assert_eq!(document.payload.overview.repository_count, 1);
        assert!(
            document
                .payload
                .advisories
                .advisories
                .iter()
                .any(|advisory| {
                    advisory.code == ProtocolAdvisoryCode::LocalCommitsAheadOfKnownUpstream
                })
        );
    }

    #[test]
    fn combined_repository_protocol_filters_advisories_to_the_inspected_repository() {
        let mirror = mirror();
        let advisories = AdvisoryEvaluator::evaluate(&mirror);
        let repository = &mirror.repositories()[0];
        let document =
            ProtocolDocument::<ProtocolRepositoryDetailView>::try_from((repository, &advisories))
                .expect("combined repository protocol must build");

        assert_eq!(document.payload.repository.id, "repository-1");
        assert!(
            document
                .payload
                .advisories
                .advisories
                .iter()
                .all(|advisory| {
                    !matches!(&advisory.subject, super::ProtocolAdvisorySubject::Workspace)
                })
        );
    }

    #[test]
    fn combined_protocol_rejects_projection_revision_mismatch() {
        let mirror = mirror();
        let advisories = AdvisoryEvaluator::evaluate(&WorkspaceMirror::new(
            WorkspaceRevision::new(8),
            OverviewProjection::new(WorkspaceRevision::new(8), 0, 0, 0, 0, 0, 0, 0),
            Vec::new(),
        ));

        assert_eq!(
            ProtocolDocument::<ProtocolOverviewView>::try_from((mirror.overview(), &advisories)),
            Err(super::ProtocolBuildError::InconsistentWorkspaceRevision)
        );
        assert_eq!(
            ProtocolDocument::<ProtocolRepositoryDetailView>::try_from((
                &mirror.repositories()[0],
                &advisories,
            )),
            Err(super::ProtocolBuildError::InconsistentWorkspaceRevision)
        );
    }

    #[test]
    fn timestamp_protocol_normalizes_values_before_unix_epoch() {
        let timestamp = ProtocolTimestamp::try_from(UNIX_EPOCH - Duration::from_millis(500))
            .expect("timestamp must build");

        assert_eq!(timestamp.unix_seconds, -1);
        assert_eq!(timestamp.nanoseconds, 500_000_000);
    }

    #[test]
    fn mirror_protocol_rejects_mixed_workspace_revisions() {
        let mirror = WorkspaceMirror::new(
            WorkspaceRevision::new(7),
            OverviewProjection::new(WorkspaceRevision::new(8), 0, 0, 0, 0, 0, 0, 0),
            Vec::new(),
        );

        assert_eq!(
            ProtocolDocument::<ProtocolWorkspaceMirror>::try_from(&mirror),
            Err(super::ProtocolBuildError::InconsistentWorkspaceRevision)
        );
    }

    #[cfg(unix)]
    #[test]
    fn protocol_path_preserves_non_utf8_unix_paths_without_loss() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let path = PathBuf::from(OsString::from_vec(vec![b'/', b't', b'm', b'p', b'/', 0xff]));

        assert_eq!(
            ProtocolPath::from_path(&path),
            ProtocolPath::UnixBytes(vec![b'/', b't', b'm', b'p', b'/', 0xff])
        );
    }
}
