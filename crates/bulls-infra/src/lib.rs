// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

//! Infrastructure adapters for BullSaddle.

mod catalog;
mod clock;
mod configuration;
mod discovery;
mod error;
mod filesystem;
mod git_observation;
mod git_process;
mod git_topology;
mod identity;
mod observation_store;
mod platform_paths;
mod process;
mod storage;
mod workspace_read;
mod workspace_reset;
mod workspace_revision;

pub use catalog::SqliteRepositoryCatalog;
pub use clock::NativeClock;
pub use configuration::TomlConfigurationStore;
pub use discovery::NativeRepositoryDiscovery;
pub use filesystem::NativeFilesystemIdentity;
pub use git_observation::GitCliObservation;
pub use git_topology::GitCliRepositoryProbe;
pub use identity::NativeIdentityGenerator;
pub use observation_store::{
    SqliteRepositoryRemotesObservationStore, SqliteWorktreeObservationStore,
};
pub use platform_paths::NativePlatformPaths;
pub use storage::SqliteStorage;
pub use workspace_read::SqliteWorkspaceReadStore;
pub use workspace_reset::SqliteWorkspaceReset;
pub use workspace_revision::SqliteWorkspaceRevisionStore;
