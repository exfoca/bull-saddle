// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

mod cancellation;
mod catalog;
mod clock;
mod configuration;
mod discovery;
mod error;
mod filesystem;
mod git_observation;
mod git_topology;
mod identity;
mod observation_store;
mod platform_paths;
mod workspace_read;
mod workspace_reset;
mod workspace_revision;

pub use cancellation::CancellationPort;
pub use catalog::{
    CatalogMissingScope, CatalogReconciliation, CatalogReconciliationCoverage,
    LocationIdentityEvidence, RepositoryCatalogPort, RepositoryIdentityEvidence,
};
pub use clock::ClockPort;
pub use configuration::ConfigurationPort;
pub use discovery::{
    BareRepositoryDiscovery, DiscoveryCompleteness, DiscoveryIssue, DiscoveryIssueKind,
    DiscoveryOutcome, DiscoveryRequest, FilesystemBoundary, RepositoryCandidate,
    RepositoryDiscoveryPort, SymlinkTraversal,
};
pub use error::{PortError, PortErrorKind, PortResult};
pub use filesystem::{FilesystemIdentityEvidence, FilesystemIdentityPort, FilesystemObjectId};
pub use git_observation::GitObservationPort;
pub use git_topology::{GitRepositoryLayout, GitRepositoryProbePort};
pub use identity::IdentityGeneratorPort;
pub use observation_store::ObservationStorePort;
pub use platform_paths::{PlatformPaths, PlatformPathsPort};
pub use workspace_read::{WorkspaceObservationState, WorkspaceReadPort, WorkspaceReadSnapshot};
pub use workspace_reset::WorkspaceResetPort;
pub use workspace_revision::WorkspaceRevisionPort;
