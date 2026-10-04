// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use bulls_application::WorkspaceRevision;
use bulls_application::ports::{PlatformPaths, PortResult, WorkspaceResetPort};
use rusqlite::TransactionBehavior;

use crate::error::sqlite_port_error;
use crate::storage::{SqliteStorage, advance_workspace_revision, read_workspace_revision};

pub struct SqliteWorkspaceReset {
    storage: SqliteStorage,
}

impl SqliteWorkspaceReset {
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
}

impl WorkspaceResetPort for SqliteWorkspaceReset {
    fn reset_workspace_knowledge(&mut self) -> PortResult<WorkspaceRevision> {
        let transaction = self
            .storage
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| sqlite_port_error(&error))?;
        let deleted_repositories = transaction
            .execute("DELETE FROM repositories", [])
            .map_err(|error| sqlite_port_error(&error))?;

        if deleted_repositories > 0 {
            advance_workspace_revision(&transaction)?;
        }
        let revision = read_workspace_revision(&transaction)?;

        transaction
            .commit()
            .map_err(|error| sqlite_port_error(&error))?;
        Ok(revision)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    use bulls_application::ports::{PortErrorKind, WorkspaceResetPort};

    use super::SqliteWorkspaceReset;

    static TEST_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let sequence = TEST_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "bulls-workspace-reset-test-{}-{}",
                std::process::id(),
                sequence
            ));
            fs::create_dir_all(&path).expect("test directory must be created");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn reset_rolls_back_catalog_deletion_when_revision_cannot_advance() {
        let root = TestDirectory::new();
        let mut reset =
            SqliteWorkspaceReset::open(root.path()).expect("workspace reset storage must open");
        reset
            .storage
            .connection()
            .execute(
                "INSERT INTO repositories (repository_id) VALUES ('repository-1')",
                [],
            )
            .expect("repository fixture must be inserted");
        reset
            .storage
            .connection()
            .execute(
                "UPDATE workspace_revision SET revision = ?1 WHERE singleton = 1",
                [i64::MAX],
            )
            .expect("workspace revision fixture must be updated");

        let error = reset
            .reset_workspace_knowledge()
            .expect_err("revision overflow guard must fail the reset");

        assert_eq!(error.kind(), PortErrorKind::InvariantViolation);
        let repository_count: i64 = reset
            .storage
            .connection()
            .query_row("SELECT COUNT(*) FROM repositories", [], |row| row.get(0))
            .expect("repository count must remain readable");
        assert_eq!(repository_count, 1);
        let revision: i64 = reset
            .storage
            .connection()
            .query_row(
                "SELECT revision FROM workspace_revision WHERE singleton = 1",
                [],
                |row| row.get(0),
            )
            .expect("workspace revision must remain readable");
        assert_eq!(revision, i64::MAX);
    }
}
