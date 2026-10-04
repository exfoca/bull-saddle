// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

//! Application use cases and orchestration for BullSaddle.

mod advisory;
mod configuration;
mod diagnostics;
mod discovery;
mod error;
mod observation;
mod operation;
mod operational_protocol;
pub mod ports;
mod protocol;
mod query;
mod read_models;
mod reset;
mod workspace;

pub use advisory::{
    Advisory, AdvisoryCode, AdvisoryEvaluator, AdvisoryEvidence, AdvisoryFreshness,
    AdvisoryProjection, AdvisoryRuleUnknown, AdvisorySubject, AdvisoryUnknownReason,
};
pub use bulls_core::{
    Head, LocationAvailability, ObservationCoverage, ObservationFailureKind, RepositoryId,
    UpstreamState, WorktreeId,
};
pub use configuration::{
    BullSaddleConfiguration, ConfigurationProjection, ConfigurationSource, DiscoveryConfiguration,
    GitProcessTimeout, LocalePreference,
};
pub use diagnostics::{
    FailureCause, FailureClass, FailureCode, PUBLIC_FAILURE_REPORT_SCHEMA_VERSION,
    PublicFailureDescriptor, PublicFailureDetails, PublicFailureReport,
};
pub use discovery::{
    DiscoverRepositories, DiscoverRepositoriesOutcome, DiscoveryBatchOutcome, DiscoveryRootFailure,
    RepositoryIdentificationIssue,
};
pub use error::{ApplicationError, ApplicationErrorCode};
pub use observation::{
    ObservationExecutionPolicy, ObservationTargetFailure, ObserveInventory, ObserveInventoryOutcome,
};
pub use operation::{OperationCompletion, WorkspaceOperationOutcome};
pub use operational_protocol::{
    OPERATION_PROTOCOL_SCHEMA_VERSION, ProtocolDiscoveryCoverage, ProtocolDiscoveryIssue,
    ProtocolDiscoveryIssueStage, ProtocolDiscoveryOutcome, ProtocolDiscoveryRoot,
    ProtocolDiscoveryRootFailure, ProtocolObservationTarget, ProtocolObservationTargetFailure,
    ProtocolOperationDocument, ProtocolOperationFailureCause, ProtocolOperationStatus,
    ProtocolRefreshOutcome,
};
pub use protocol::{
    ProtocolAdvisory, ProtocolAdvisoryCode, ProtocolAdvisoryEvidence, ProtocolAdvisoryFreshness,
    ProtocolAdvisoryProjection, ProtocolAdvisoryRuleUnknown, ProtocolAdvisorySubject,
    ProtocolAdvisoryUnknownReason, ProtocolBuildError, ProtocolDocument, ProtocolHead,
    ProtocolKnowledgeBool, ProtocolLocation, ProtocolLocationAvailability,
    ProtocolObservationAttempt, ProtocolObservationAttemptOutcome, ProtocolObservationCoverage,
    ProtocolObservationFailureKind, ProtocolObservationStatus, ProtocolObservationSuccess,
    ProtocolOverview, ProtocolOverviewView, ProtocolPath, ProtocolRemote, ProtocolRepositoryDetail,
    ProtocolRepositoryDetailView, ProtocolRepositoryList, ProtocolRepositoryListItem,
    ProtocolResultCompleteness, ProtocolResultMetadata, ProtocolTimestamp,
    ProtocolUpstreamRelation, ProtocolUpstreamState, ProtocolWorkspaceMirror, ProtocolWorktree,
    ProtocolWorktreeChanges, ProtocolWorktreeGitState, READ_PROTOCOL_SCHEMA_VERSION,
};
pub use query::{
    DEFAULT_QUERY_PAGE_SIZE, FreshnessConstraint, MAX_QUERY_PAGE_SIZE, QueryPage,
    RepositoryListQuery, RepositoryPredicate, RepositorySelector, RepositorySelectorResolution,
    WorkspaceQueryService,
};
pub use read_models::{
    Knowledge, LocationProjection, ObservationAttemptOutcomeProjection,
    ObservationAttemptProjection, ObservationEvidenceState, ObservationStatusProjection,
    ObservationSuccessProjection, OverviewProjection, RepositoryDetailProjection,
    RepositoryListItemProjection, RepositoryListProjection, WorkspaceMirror, WorktreeProjection,
};
pub use reset::{ResetWorkspace, ResetWorkspaceOutcome};

pub use workspace::WorkspaceRevision;
