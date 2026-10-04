// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use bulls_core::ObservationKey;
use serde::{Deserialize, Serialize};

use crate::ports::CatalogReconciliationCoverage;
use crate::protocol::{protocol_observation_coverage, protocol_observation_failure, usize_to_u64};
use crate::{
    DiscoveryBatchOutcome, FailureCause, ObserveInventoryOutcome, OperationCompletion,
    ProtocolObservationCoverage, ProtocolObservationFailureKind, ProtocolPath,
    WorkspaceOperationOutcome, WorkspaceRevision,
};

pub const OPERATION_PROTOCOL_SCHEMA_VERSION: u16 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolOperationStatus {
    Complete,
    Partial,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProtocolOperationDocument<T> {
    pub schema_version: u16,
    pub workspace_revision: u64,
    pub status: ProtocolOperationStatus,
    pub payload: T,
}

impl<T> ProtocolOperationDocument<T> {
    fn new(revision: WorkspaceRevision, status: ProtocolOperationStatus, payload: T) -> Self {
        Self {
            schema_version: OPERATION_PROTOCOL_SCHEMA_VERSION,
            workspace_revision: revision.value(),
            status,
            payload,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolDiscoveryCoverage {
    Complete,
    Partial,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolDiscoveryIssueStage {
    Filesystem,
    RepositoryIdentification,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProtocolOperationFailureCause {
    PermissionDenied,
    ResourceUnavailable,
    ProcessExited { exit_code: i32 },
    IoFailure,
    StorageFailure,
    InvalidData,
    OutputLimitExceeded,
    TimedOut,
    Cancelled,
    InvariantViolation,
    Unexpected,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProtocolDiscoveryIssue {
    pub stage: ProtocolDiscoveryIssueStage,
    pub path: ProtocolPath,
    pub cause: ProtocolOperationFailureCause,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProtocolDiscoveryRoot {
    pub root: ProtocolPath,
    pub coverage: ProtocolDiscoveryCoverage,
    pub issues: Vec<ProtocolDiscoveryIssue>,
    pub repository_count: u64,
    pub location_count: u64,
    pub worktree_count: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProtocolDiscoveryRootFailure {
    pub root: ProtocolPath,
    pub cause: ProtocolOperationFailureCause,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProtocolDiscoveryOutcome {
    pub requested_root_count: u64,
    pub completed_root_count: u64,
    pub partial_root_count: u64,
    pub failed_root_count: u64,
    pub discovery_issue_count: u64,
    pub identification_issue_count: u64,
    pub repository_count: u64,
    pub location_count: u64,
    pub worktree_count: u64,
    pub roots: Vec<ProtocolDiscoveryRoot>,
    pub failed_roots: Vec<ProtocolDiscoveryRootFailure>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProtocolObservationTarget {
    RepositoryRemotes { repository_id: String },
    WorktreeGitState { worktree_id: String },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProtocolObservationTargetFailure {
    pub target: ProtocolObservationTarget,
    pub failure: ProtocolObservationFailureKind,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProtocolRefreshOutcome {
    pub run_id: String,
    pub coverage: ProtocolObservationCoverage,
    pub target_count: u64,
    pub succeeded_count: u64,
    pub failed_count: u64,
    pub cancelled_count: u64,
    pub failures: Vec<ProtocolObservationTargetFailure>,
}

impl TryFrom<&WorkspaceOperationOutcome<DiscoveryBatchOutcome>>
    for ProtocolOperationDocument<ProtocolDiscoveryOutcome>
{
    type Error = crate::ProtocolBuildError;

    fn try_from(
        operation: &WorkspaceOperationOutcome<DiscoveryBatchOutcome>,
    ) -> Result<Self, Self::Error> {
        let value = operation.outcome();
        let roots = value
            .completed()
            .iter()
            .map(|outcome| {
                let mut issues = Vec::with_capacity(
                    outcome.discovery_issues().len() + outcome.identification_issues().len(),
                );
                issues.extend(outcome.discovery_issues().iter().map(|issue| {
                    ProtocolDiscoveryIssue {
                        stage: ProtocolDiscoveryIssueStage::Filesystem,
                        path: ProtocolPath::from_path(issue.path()),
                        cause: protocol_failure_cause(issue.kind().failure_cause()),
                    }
                }));
                issues.extend(outcome.identification_issues().iter().map(|issue| {
                    ProtocolDiscoveryIssue {
                        stage: ProtocolDiscoveryIssueStage::RepositoryIdentification,
                        path: ProtocolPath::from_path(issue.path()),
                        cause: protocol_failure_cause(issue.kind().failure_cause()),
                    }
                }));

                Ok(ProtocolDiscoveryRoot {
                    root: ProtocolPath::from_path(outcome.root()),
                    coverage: protocol_discovery_coverage(outcome.coverage()),
                    issues,
                    repository_count: usize_to_u64(outcome.repository_count())?,
                    location_count: usize_to_u64(outcome.location_count())?,
                    worktree_count: usize_to_u64(outcome.worktree_count())?,
                })
            })
            .collect::<Result<Vec<_>, crate::ProtocolBuildError>>()?;
        let failed_roots = value
            .failed_roots()
            .iter()
            .map(|failure| ProtocolDiscoveryRootFailure {
                root: ProtocolPath::from_path(failure.root()),
                cause: protocol_failure_cause(failure.kind().failure_cause()),
            })
            .collect();
        let payload = ProtocolDiscoveryOutcome {
            requested_root_count: usize_to_u64(value.requested_root_count())?,
            completed_root_count: usize_to_u64(value.completed_root_count())?,
            partial_root_count: usize_to_u64(value.partial_root_count())?,
            failed_root_count: usize_to_u64(value.failed_root_count())?,
            discovery_issue_count: usize_to_u64(value.discovery_issue_count())?,
            identification_issue_count: usize_to_u64(value.identification_issue_count())?,
            repository_count: usize_to_u64(value.repository_count())?,
            location_count: usize_to_u64(value.location_count())?,
            worktree_count: usize_to_u64(value.worktree_count())?,
            roots,
            failed_roots,
        };

        Ok(Self::new(
            operation.revision(),
            protocol_operation_status(value.completion()),
            payload,
        ))
    }
}

impl TryFrom<&WorkspaceOperationOutcome<ObserveInventoryOutcome>>
    for ProtocolOperationDocument<ProtocolRefreshOutcome>
{
    type Error = crate::ProtocolBuildError;

    fn try_from(
        operation: &WorkspaceOperationOutcome<ObserveInventoryOutcome>,
    ) -> Result<Self, Self::Error> {
        let value = operation.outcome();
        let failures = value
            .failures()
            .iter()
            .map(|failure| ProtocolObservationTargetFailure {
                target: protocol_observation_target(failure.key()),
                failure: protocol_observation_failure(failure.kind()),
            })
            .collect();
        let payload = ProtocolRefreshOutcome {
            run_id: value.run_id().as_str().to_owned(),
            coverage: protocol_observation_coverage(value.coverage()),
            target_count: usize_to_u64(value.target_count())?,
            succeeded_count: usize_to_u64(value.succeeded_count())?,
            failed_count: usize_to_u64(value.failed_count())?,
            cancelled_count: usize_to_u64(value.cancelled_count())?,
            failures,
        };

        Ok(Self::new(
            operation.revision(),
            protocol_operation_status(value.completion()),
            payload,
        ))
    }
}

const fn protocol_operation_status(value: OperationCompletion) -> ProtocolOperationStatus {
    match value {
        OperationCompletion::Complete => ProtocolOperationStatus::Complete,
        OperationCompletion::Partial => ProtocolOperationStatus::Partial,
        OperationCompletion::Failed => ProtocolOperationStatus::Failed,
        OperationCompletion::Cancelled => ProtocolOperationStatus::Cancelled,
    }
}

const fn protocol_discovery_coverage(
    value: CatalogReconciliationCoverage,
) -> ProtocolDiscoveryCoverage {
    match value {
        CatalogReconciliationCoverage::Complete => ProtocolDiscoveryCoverage::Complete,
        CatalogReconciliationCoverage::Partial => ProtocolDiscoveryCoverage::Partial,
    }
}

const fn protocol_failure_cause(value: FailureCause) -> ProtocolOperationFailureCause {
    match value {
        FailureCause::PermissionDenied => ProtocolOperationFailureCause::PermissionDenied,
        FailureCause::ResourceUnavailable => ProtocolOperationFailureCause::ResourceUnavailable,
        FailureCause::ProcessExited { exit_code } => {
            ProtocolOperationFailureCause::ProcessExited { exit_code }
        }
        FailureCause::IoFailure => ProtocolOperationFailureCause::IoFailure,
        FailureCause::StorageFailure => ProtocolOperationFailureCause::StorageFailure,
        FailureCause::InvalidData => ProtocolOperationFailureCause::InvalidData,
        FailureCause::OutputLimitExceeded => ProtocolOperationFailureCause::OutputLimitExceeded,
        FailureCause::TimedOut => ProtocolOperationFailureCause::TimedOut,
        FailureCause::Cancelled => ProtocolOperationFailureCause::Cancelled,
        FailureCause::InvariantViolation => ProtocolOperationFailureCause::InvariantViolation,
        FailureCause::Unexpected => ProtocolOperationFailureCause::Unexpected,
    }
}

fn protocol_observation_target(value: &ObservationKey) -> ProtocolObservationTarget {
    match value {
        ObservationKey::RepositoryRemotes(repository_id) => {
            ProtocolObservationTarget::RepositoryRemotes {
                repository_id: repository_id.as_str().to_owned(),
            }
        }
        ObservationKey::WorktreeGitState(worktree_id) => {
            ProtocolObservationTarget::WorktreeGitState {
                worktree_id: worktree_id.as_str().to_owned(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use bulls_core::{
        ObservationFailureKind, ObservationKey, ObservationRunId, RepositoryId, WorktreeId,
    };

    use super::{
        OPERATION_PROTOCOL_SCHEMA_VERSION, ProtocolDiscoveryIssueStage, ProtocolDiscoveryOutcome,
        ProtocolObservationTarget, ProtocolOperationDocument, ProtocolOperationFailureCause,
        ProtocolOperationStatus, ProtocolPath, ProtocolRefreshOutcome,
    };
    use crate::ports::{
        CatalogReconciliationCoverage, DiscoveryIssue, DiscoveryIssueKind, PortErrorKind,
    };
    use crate::{
        DiscoverRepositoriesOutcome, DiscoveryBatchOutcome, DiscoveryRootFailure,
        ObservationTargetFailure, ObserveInventoryOutcome, RepositoryIdentificationIssue,
        WorkspaceOperationOutcome, WorkspaceRevision,
    };

    #[test]
    fn discovery_protocol_preserves_partial_diagnostics_and_counts() {
        let completed = DiscoverRepositoriesOutcome::new(
            "/workspace",
            CatalogReconciliationCoverage::Partial,
            vec![DiscoveryIssue::new(
                "/workspace/private",
                DiscoveryIssueKind::PermissionDenied,
            )],
            vec![RepositoryIdentificationIssue::new(
                "/workspace/broken",
                PortErrorKind::ProcessExited { exit_code: 128 },
            )],
            3,
            4,
            2,
        );
        let outcome = DiscoveryBatchOutcome::new(
            vec![completed],
            vec![DiscoveryRootFailure::new(
                "/offline",
                PortErrorKind::ResourceUnavailable,
            )],
        );

        let operation = WorkspaceOperationOutcome::new(WorkspaceRevision::new(12), outcome);
        let document = ProtocolOperationDocument::<ProtocolDiscoveryOutcome>::try_from(&operation)
            .expect("discovery protocol must build");

        assert_eq!(document.schema_version, OPERATION_PROTOCOL_SCHEMA_VERSION);
        assert_eq!(document.workspace_revision, 12);
        assert_eq!(document.status, ProtocolOperationStatus::Partial);
        assert_eq!(document.payload.requested_root_count, 2);
        assert_eq!(document.payload.completed_root_count, 1);
        assert_eq!(document.payload.partial_root_count, 1);
        assert_eq!(document.payload.failed_root_count, 1);
        assert_eq!(document.payload.discovery_issue_count, 1);
        assert_eq!(document.payload.identification_issue_count, 1);
        assert_eq!(document.payload.repository_count, 3);
        assert_eq!(document.payload.location_count, 4);
        assert_eq!(document.payload.worktree_count, 2);
        assert_eq!(document.payload.roots[0].issues.len(), 2);
        assert_eq!(
            document.payload.roots[0].issues[0].stage,
            ProtocolDiscoveryIssueStage::Filesystem
        );
        assert_eq!(
            document.payload.roots[0].issues[0].cause,
            ProtocolOperationFailureCause::PermissionDenied
        );
        assert_eq!(
            document.payload.roots[0].issues[1].stage,
            ProtocolDiscoveryIssueStage::RepositoryIdentification
        );
        assert_eq!(
            document.payload.roots[0].issues[1].cause,
            ProtocolOperationFailureCause::ProcessExited { exit_code: 128 }
        );
        assert_eq!(
            document.payload.failed_roots[0].root,
            ProtocolPath::Utf8("/offline".to_owned())
        );

        let serialized = toml::Value::try_from(&document)
            .expect("discovery protocol serialization must remain representable");
        assert_eq!(serialized["status"].as_str(), Some("partial"));
        assert_eq!(
            serialized["payload"]["roots"][0]["issues"][0]["stage"].as_str(),
            Some("filesystem")
        );
        assert_eq!(
            serialized["payload"]["roots"][0]["issues"][0]["path"]["encoding"].as_str(),
            Some("utf8")
        );
    }

    #[test]
    fn discovery_protocol_distinguishes_total_failure_from_partial_success() {
        let outcome = DiscoveryBatchOutcome::new(
            Vec::new(),
            vec![DiscoveryRootFailure::new(
                PathBuf::from("/offline"),
                PortErrorKind::ResourceUnavailable,
            )],
        );

        let operation = WorkspaceOperationOutcome::new(WorkspaceRevision::INITIAL, outcome);
        let document = ProtocolOperationDocument::<ProtocolDiscoveryOutcome>::try_from(&operation)
            .expect("failed discovery protocol must build");

        assert_eq!(document.status, ProtocolOperationStatus::Failed);
        assert_eq!(document.payload.requested_root_count, 1);
        assert_eq!(document.payload.failed_root_count, 1);
    }

    #[test]
    fn refresh_protocol_preserves_failed_target_identity_and_cause() {
        let outcome = ObserveInventoryOutcome::new(
            ObservationRunId::from("observation-run-7"),
            1,
            vec![ObservationTargetFailure::new(
                ObservationKey::worktree_git_state(WorktreeId::from("worktree-9")),
                ObservationFailureKind::TimedOut,
            )],
        );

        let operation = WorkspaceOperationOutcome::new(WorkspaceRevision::new(13), outcome);
        let document = ProtocolOperationDocument::<ProtocolRefreshOutcome>::try_from(&operation)
            .expect("refresh protocol must build");

        assert_eq!(document.workspace_revision, 13);
        assert_eq!(document.status, ProtocolOperationStatus::Partial);
        assert_eq!(document.payload.run_id, "observation-run-7");
        assert_eq!(document.payload.target_count, 2);
        assert_eq!(document.payload.succeeded_count, 1);
        assert_eq!(document.payload.failed_count, 1);
        assert_eq!(document.payload.cancelled_count, 0);
        assert_eq!(
            document.payload.failures[0].target,
            ProtocolObservationTarget::WorktreeGitState {
                worktree_id: "worktree-9".to_owned()
            }
        );
        assert_eq!(
            document.payload.failures[0].failure,
            crate::ProtocolObservationFailureKind::TimedOut
        );
    }

    #[test]
    fn refresh_protocol_distinguishes_cancelled_and_failed_completion() {
        let cancelled = ObserveInventoryOutcome::new(
            ObservationRunId::from("observation-run-cancelled"),
            1,
            vec![ObservationTargetFailure::new(
                ObservationKey::repository_remotes(RepositoryId::from("repository-1")),
                ObservationFailureKind::Cancelled,
            )],
        );
        let failed = ObserveInventoryOutcome::new(
            ObservationRunId::from("observation-run-failed"),
            0,
            vec![ObservationTargetFailure::new(
                ObservationKey::repository_remotes(RepositoryId::from("repository-2")),
                ObservationFailureKind::SubjectUnavailable,
            )],
        );

        let cancelled_operation =
            WorkspaceOperationOutcome::new(WorkspaceRevision::new(14), cancelled);
        let failed_operation = WorkspaceOperationOutcome::new(WorkspaceRevision::new(15), failed);
        let cancelled_document =
            ProtocolOperationDocument::<ProtocolRefreshOutcome>::try_from(&cancelled_operation)
                .expect("cancelled refresh protocol must build");
        let failed_document =
            ProtocolOperationDocument::<ProtocolRefreshOutcome>::try_from(&failed_operation)
                .expect("failed refresh protocol must build");

        assert_eq!(
            cancelled_document.status,
            ProtocolOperationStatus::Cancelled
        );
        assert_eq!(cancelled_document.payload.cancelled_count, 1);
        assert_eq!(failed_document.status, ProtocolOperationStatus::Failed);
        assert_eq!(failed_document.payload.failed_count, 1);
    }
}
