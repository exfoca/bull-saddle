// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};

use bulls_application::ports::{
    PlatformPaths, PortResult, WorkspaceReadPort, WorkspaceReadSnapshot,
};

use crate::catalog::{load_all_locations, load_all_repositories, load_all_worktrees};
use crate::error::sqlite_port_error;
use crate::observation_store::{
    load_latest_repository_remotes_states, load_latest_worktree_git_states,
};
use crate::storage::{SqliteStorage, read_workspace_revision};

pub struct SqliteWorkspaceReadStore {
    storage: SqliteStorage,
}

impl SqliteWorkspaceReadStore {
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

impl WorkspaceReadPort for SqliteWorkspaceReadStore {
    fn read_workspace(&self) -> PortResult<WorkspaceReadSnapshot> {
        let transaction = self
            .storage
            .connection()
            .unchecked_transaction()
            .map_err(|error| sqlite_port_error(&error))?;

        let snapshot = WorkspaceReadSnapshot::new(
            read_workspace_revision(&transaction)?,
            load_all_repositories(&transaction)?,
            load_all_locations(&transaction)?,
            load_all_worktrees(&transaction)?,
            load_latest_repository_remotes_states(&transaction)?,
            load_latest_worktree_git_states(&transaction)?,
        );

        transaction
            .commit()
            .map_err(|error| sqlite_port_error(&error))?;

        Ok(snapshot)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

    use bulls_application::ports::{
        CatalogMissingScope, CatalogReconciliation, CatalogReconciliationCoverage,
        RepositoryCatalogPort, WorkspaceReadPort,
    };
    use bulls_core::{
        Location, LocationAvailability, LocationId, Repository, RepositoryId, Worktree, WorktreeId,
    };
    use rusqlite::trace::{TraceEvent, TraceEventCodes};

    use super::SqliteWorkspaceReadStore;
    use crate::catalog::SqliteRepositoryCatalog;

    static TEST_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);
    static READ_STATEMENT_COUNT: AtomicUsize = AtomicUsize::new(0);
    static TRACE_LOCK: Mutex<()> = Mutex::new(());

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let sequence = TEST_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "bulls-workspace-read-query-count-test-{}-{}",
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

    fn seed_catalog(data_dir: &Path, repository_count: usize) {
        let workspace = data_dir.join("workspace");
        let mut repositories = Vec::with_capacity(repository_count);
        let mut locations = Vec::with_capacity(repository_count);
        let mut worktrees = Vec::with_capacity(repository_count);

        for index in 0..repository_count {
            let repository_id = RepositoryId::from(format!("repository-{index}"));
            let location_id = LocationId::from(format!("location-{index}"));
            let worktree_id = WorktreeId::from(format!("worktree-{index}"));

            repositories.push(Repository::new(repository_id.clone()));
            locations.push(Location::new(
                location_id.clone(),
                repository_id.clone(),
                workspace.join(format!("repository-{index}")),
                LocationAvailability::Available,
            ));
            worktrees.push(Worktree::new(worktree_id, repository_id, location_id));
        }

        let mut catalog =
            SqliteRepositoryCatalog::open(data_dir).expect("catalog storage must open");
        catalog
            .reconcile(
                &CatalogReconciliation::new(
                    &workspace,
                    CatalogReconciliationCoverage::Partial,
                    CatalogMissingScope::None,
                )
                .with_entities(repositories, locations, worktrees),
            )
            .expect("catalog reconciliation must succeed");
    }

    fn trace_read_statement(event: TraceEvent<'_>) {
        if let TraceEvent::Stmt(_, sql) = event {
            let first_token = sql.trim_start().split_ascii_whitespace().next();
            if matches!(first_token, Some("SELECT" | "WITH")) {
                READ_STATEMENT_COUNT.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    fn workspace_read_statement_count(repository_count: usize) -> usize {
        let root = TestDirectory::new();
        let data_dir = root.path().join("data");
        seed_catalog(&data_dir, repository_count);

        let reader = SqliteWorkspaceReadStore::open(&data_dir).expect("workspace reader must open");
        let _trace_guard = TRACE_LOCK.lock().expect("trace lock must remain usable");
        READ_STATEMENT_COUNT.store(0, Ordering::Relaxed);
        reader.storage.connection().trace_v2(
            TraceEventCodes::SQLITE_TRACE_STMT,
            Some(trace_read_statement),
        );

        reader
            .read_workspace()
            .expect("workspace snapshot must load");

        reader
            .storage
            .connection()
            .trace_v2(TraceEventCodes::SQLITE_TRACE_STMT, None);
        READ_STATEMENT_COUNT.load(Ordering::Relaxed)
    }

    #[test]
    fn workspace_read_statement_count_does_not_scale_with_workspace_cardinality() {
        let single_repository_count = workspace_read_statement_count(1);
        let many_repository_count = workspace_read_statement_count(64);

        assert_eq!(single_repository_count, 7);
        assert_eq!(many_repository_count, single_repository_count);
    }
}
