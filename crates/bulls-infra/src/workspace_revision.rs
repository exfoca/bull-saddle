// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};

use bulls_application::WorkspaceRevision;
use bulls_application::ports::{PlatformPaths, PortResult, WorkspaceRevisionPort};

use crate::storage::SqliteStorage;

pub struct SqliteWorkspaceRevisionStore {
    storage: SqliteStorage,
}

impl SqliteWorkspaceRevisionStore {
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

impl WorkspaceRevisionPort for SqliteWorkspaceRevisionStore {
    fn workspace_revision(&self) -> PortResult<WorkspaceRevision> {
        self.storage.workspace_revision()
    }
}
