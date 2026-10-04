// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use bulls_application::WorkspaceRevision;
use bulls_application::ports::{PlatformPaths, PortResult};
use rusqlite::{Connection, Error as SqliteError, Transaction, TransactionBehavior};

use crate::error::{invalid_data, invariant_violation, io_port_error, sqlite_port_error};

const STORAGE_FILE_NAME: &str = "bulls.sqlite";
const STORAGE_BUSY_TIMEOUT: Duration = Duration::from_secs(5);
const STORAGE_SCHEMA_VERSION: i64 = 4;

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        sql: "",
    },
    Migration {
        version: 2,
        sql: r#"
CREATE TABLE repositories (
    repository_id TEXT PRIMARY KEY NOT NULL
) STRICT;

CREATE TABLE locations (
    location_id TEXT PRIMARY KEY NOT NULL,
    repository_id TEXT NOT NULL REFERENCES repositories(repository_id) ON DELETE CASCADE,
    path BLOB NOT NULL,
    availability TEXT NOT NULL CHECK (availability IN ('available', 'missing', 'offline')),
    UNIQUE (location_id, repository_id)
) STRICT;

CREATE TABLE worktrees (
    worktree_id TEXT PRIMARY KEY NOT NULL,
    repository_id TEXT NOT NULL,
    location_id TEXT NOT NULL UNIQUE,
    FOREIGN KEY (location_id, repository_id)
        REFERENCES locations(location_id, repository_id) ON DELETE CASCADE
) STRICT;

CREATE TABLE repository_identities (
    repository_id TEXT PRIMARY KEY NOT NULL
        REFERENCES repositories(repository_id) ON DELETE CASCADE,
    common_dir BLOB NOT NULL,
    filesystem_id BLOB,
    object_id BLOB,
    CHECK ((filesystem_id IS NULL) = (object_id IS NULL))
) STRICT;

CREATE INDEX repository_identities_common_dir_idx
    ON repository_identities(common_dir);
CREATE INDEX repository_identities_object_idx
    ON repository_identities(filesystem_id, object_id)
    WHERE filesystem_id IS NOT NULL;

CREATE TABLE location_identities (
    location_id TEXT PRIMARY KEY NOT NULL
        REFERENCES locations(location_id) ON DELETE CASCADE,
    filesystem_id BLOB,
    object_id BLOB,
    CHECK ((filesystem_id IS NULL) = (object_id IS NULL))
) STRICT;

CREATE INDEX location_identities_object_idx
    ON location_identities(filesystem_id, object_id)
    WHERE filesystem_id IS NOT NULL;
"#,
    },
    Migration {
        version: 3,
        sql: r#"
CREATE TABLE worktree_git_observation_attempts (
    attempt_id INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id TEXT NOT NULL,
    worktree_id TEXT NOT NULL REFERENCES worktrees(worktree_id) ON DELETE CASCADE,
    observed_at_seconds INTEGER NOT NULL,
    observed_at_nanos INTEGER NOT NULL CHECK (observed_at_nanos >= 0 AND observed_at_nanos < 1000000000),
    coverage TEXT NOT NULL CHECK (coverage IN ('complete', 'partial')),
    outcome TEXT NOT NULL CHECK (outcome IN ('success', 'failure')),
    failure_kind TEXT,
    exit_code INTEGER,
    head_kind TEXT CHECK (head_kind IS NULL OR head_kind IN ('unborn', 'branch', 'detached')),
    branch_name TEXT,
    upstream_kind TEXT CHECK (upstream_kind IS NULL OR upstream_kind IN ('unconfigured', 'gone', 'tracking')),
    upstream_remote TEXT,
    upstream_branch TEXT,
    ahead BLOB CHECK (ahead IS NULL OR length(ahead) = 8),
    behind BLOB CHECK (behind IS NULL OR length(behind) = 8),
    staged INTEGER CHECK (staged IS NULL OR staged IN (0, 1)),
    unstaged INTEGER CHECK (unstaged IS NULL OR unstaged IN (0, 1)),
    untracked INTEGER CHECK (untracked IS NULL OR untracked IN (0, 1)),
    CHECK (
        (outcome = 'success'
            AND failure_kind IS NULL
            AND exit_code IS NULL
            AND head_kind IS NOT NULL
            AND staged IS NOT NULL
            AND unstaged IS NOT NULL
            AND untracked IS NOT NULL)
        OR
        (outcome = 'failure'
            AND failure_kind IS NOT NULL
            AND head_kind IS NULL
            AND branch_name IS NULL
            AND upstream_kind IS NULL
            AND upstream_remote IS NULL
            AND upstream_branch IS NULL
            AND ahead IS NULL
            AND behind IS NULL
            AND staged IS NULL
            AND unstaged IS NULL
            AND untracked IS NULL)
    ),
    CHECK (
        (failure_kind = 'process_exited' AND exit_code IS NOT NULL)
        OR
        (failure_kind IS NOT NULL AND failure_kind != 'process_exited' AND exit_code IS NULL)
        OR
        (failure_kind IS NULL AND exit_code IS NULL)
    ),
    CHECK (
        (head_kind = 'detached'
            AND branch_name IS NULL
            AND upstream_kind IS NULL
            AND upstream_remote IS NULL
            AND upstream_branch IS NULL
            AND ahead IS NULL
            AND behind IS NULL)
        OR
        (head_kind IN ('unborn', 'branch')
            AND branch_name IS NOT NULL
            AND upstream_kind IS NOT NULL)
        OR
        head_kind IS NULL
    ),
    CHECK (
        (upstream_kind = 'unconfigured'
            AND upstream_remote IS NULL
            AND upstream_branch IS NULL
            AND ahead IS NULL
            AND behind IS NULL)
        OR
        (upstream_kind = 'gone'
            AND upstream_remote IS NOT NULL
            AND upstream_branch IS NOT NULL
            AND ahead IS NULL
            AND behind IS NULL)
        OR
        (upstream_kind = 'tracking'
            AND upstream_remote IS NOT NULL
            AND upstream_branch IS NOT NULL
            AND ahead IS NOT NULL
            AND behind IS NOT NULL)
        OR
        upstream_kind IS NULL
    )
) STRICT;

CREATE INDEX worktree_git_observation_latest_idx
    ON worktree_git_observation_attempts(
        worktree_id, observed_at_seconds DESC, observed_at_nanos DESC, attempt_id DESC
    );
CREATE INDEX worktree_git_observation_run_idx
    ON worktree_git_observation_attempts(run_id);

CREATE TABLE repository_remote_observation_attempts (
    attempt_id INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id TEXT NOT NULL,
    repository_id TEXT NOT NULL REFERENCES repositories(repository_id) ON DELETE CASCADE,
    observed_at_seconds INTEGER NOT NULL,
    observed_at_nanos INTEGER NOT NULL CHECK (observed_at_nanos >= 0 AND observed_at_nanos < 1000000000),
    coverage TEXT NOT NULL CHECK (coverage IN ('complete', 'partial')),
    outcome TEXT NOT NULL CHECK (outcome IN ('success', 'failure')),
    failure_kind TEXT,
    exit_code INTEGER,
    remote_count INTEGER CHECK (remote_count IS NULL OR remote_count >= 0),
    CHECK (
        (outcome = 'success'
            AND failure_kind IS NULL
            AND exit_code IS NULL
            AND remote_count IS NOT NULL)
        OR
        (outcome = 'failure'
            AND failure_kind IS NOT NULL
            AND remote_count IS NULL)
    ),
    CHECK (
        (failure_kind = 'process_exited' AND exit_code IS NOT NULL)
        OR
        (failure_kind IS NOT NULL AND failure_kind != 'process_exited' AND exit_code IS NULL)
        OR
        (failure_kind IS NULL AND exit_code IS NULL)
    )
) STRICT;

CREATE INDEX repository_remote_observation_latest_idx
    ON repository_remote_observation_attempts(
        repository_id, observed_at_seconds DESC, observed_at_nanos DESC, attempt_id DESC
    );
CREATE INDEX repository_remote_observation_run_idx
    ON repository_remote_observation_attempts(run_id);

CREATE TABLE repository_remote_observation_values (
    attempt_id INTEGER NOT NULL
        REFERENCES repository_remote_observation_attempts(attempt_id) ON DELETE CASCADE,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    remote_name TEXT NOT NULL,
    PRIMARY KEY (attempt_id, ordinal),
    UNIQUE (attempt_id, remote_name)
) STRICT;
"#,
    },
    Migration {
        version: 4,
        sql: r#"
CREATE TABLE workspace_revision (
    singleton INTEGER PRIMARY KEY NOT NULL CHECK (singleton = 1),
    revision INTEGER NOT NULL CHECK (revision >= 0)
) STRICT;

INSERT INTO workspace_revision (singleton, revision) VALUES (1, 0);
"#,
    },
];

struct Migration {
    version: i64,
    sql: &'static str,
}

pub struct SqliteStorage {
    path: PathBuf,
    connection: Connection,
}

impl fmt::Debug for SqliteStorage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SqliteStorage")
            .finish_non_exhaustive()
    }
}

impl SqliteStorage {
    pub fn open(data_dir: impl Into<PathBuf>) -> PortResult<Self> {
        let data_dir = data_dir.into();
        if !data_dir.is_absolute() {
            return Err(invariant_violation());
        }

        fs::create_dir_all(&data_dir).map_err(|error| io_port_error(&error))?;

        let path = data_dir.join(STORAGE_FILE_NAME);
        let mut connection = Connection::open(&path).map_err(|error| sqlite_port_error(&error))?;
        configure_connection(&connection)?;
        apply_migrations(&mut connection, MIGRATIONS, STORAGE_SCHEMA_VERSION)?;

        Ok(Self { path, connection })
    }

    pub fn from_platform_paths(paths: &PlatformPaths) -> PortResult<Self> {
        Self::open(paths.data_dir())
    }

    pub fn open_peer(&self) -> PortResult<Self> {
        let connection = Connection::open(&self.path).map_err(|error| sqlite_port_error(&error))?;
        configure_connection(&connection)?;
        ensure_current_schema(&connection)?;

        Ok(Self {
            path: self.path.clone(),
            connection,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn schema_version(&self) -> PortResult<i64> {
        schema_version(&self.connection)
    }

    pub(crate) fn workspace_revision(&self) -> PortResult<WorkspaceRevision> {
        read_workspace_revision(&self.connection)
    }

    pub(crate) fn connection(&self) -> &Connection {
        &self.connection
    }

    pub(crate) fn connection_mut(&mut self) -> &mut Connection {
        &mut self.connection
    }
}

fn ensure_current_schema(connection: &Connection) -> PortResult<()> {
    if schema_version(connection)? == STORAGE_SCHEMA_VERSION {
        Ok(())
    } else {
        Err(invalid_data())
    }
}

fn configure_connection(connection: &Connection) -> PortResult<()> {
    connection
        .busy_timeout(STORAGE_BUSY_TIMEOUT)
        .map_err(|error| sqlite_port_error(&error))?;
    connection
        .pragma_update(None, "foreign_keys", true)
        .map_err(|error| sqlite_port_error(&error))
}

fn apply_migrations(
    connection: &mut Connection,
    migrations: &[Migration],
    supported_version: i64,
) -> PortResult<()> {
    let current_version = schema_version(connection)?;
    if current_version < 0 || current_version > supported_version {
        return Err(invalid_data());
    }

    if current_version == 0 {
        ensure_unversioned_database_is_empty(connection)?;
    }

    if current_version == supported_version {
        return Ok(());
    }

    validate_migration_sequence(current_version, supported_version, migrations)?;

    run_transaction(connection, |transaction| {
        for migration in migrations
            .iter()
            .filter(|migration| migration.version > current_version)
        {
            transaction.execute_batch(migration.sql)?;
            transaction.pragma_update(None, "user_version", migration.version)?;
        }

        Ok(())
    })
}

fn validate_migration_sequence(
    current_version: i64,
    supported_version: i64,
    migrations: &[Migration],
) -> PortResult<()> {
    let mut expected_version = current_version + 1;

    for migration in migrations
        .iter()
        .filter(|migration| migration.version > current_version)
    {
        if migration.version != expected_version || migration.version > supported_version {
            return Err(invariant_violation());
        }
        expected_version += 1;
    }

    if expected_version != supported_version + 1 {
        return Err(invariant_violation());
    }

    Ok(())
}

fn ensure_unversioned_database_is_empty(connection: &Connection) -> PortResult<()> {
    let object_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'",
            [],
            |row| row.get(0),
        )
        .map_err(|error| sqlite_port_error(&error))?;

    if object_count != 0 {
        return Err(invalid_data());
    }

    Ok(())
}

fn schema_version(connection: &Connection) -> PortResult<i64> {
    connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|error| sqlite_port_error(&error))
}

pub(crate) fn advance_workspace_revision(transaction: &Transaction<'_>) -> PortResult<()> {
    let affected = transaction
        .execute(
            "UPDATE workspace_revision SET revision = revision + 1 \
             WHERE singleton = 1 AND revision < ?1",
            [i64::MAX],
        )
        .map_err(|error| sqlite_port_error(&error))?;
    if affected != 1 {
        return Err(invariant_violation());
    }

    Ok(())
}

pub(crate) fn read_workspace_revision(connection: &Connection) -> PortResult<WorkspaceRevision> {
    let revision: i64 = connection
        .query_row(
            "SELECT revision FROM workspace_revision WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| sqlite_port_error(&error))?;
    let revision = u64::try_from(revision).map_err(|_| invalid_data())?;

    Ok(WorkspaceRevision::new(revision))
}

fn run_transaction<T>(
    connection: &mut Connection,
    operation: impl FnOnce(&Transaction<'_>) -> Result<T, SqliteError>,
) -> PortResult<T> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| sqlite_port_error(&error))?;
    let value = operation(&transaction).map_err(|error| sqlite_port_error(&error))?;
    transaction
        .commit()
        .map_err(|error| sqlite_port_error(&error))?;

    Ok(value)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    use bulls_application::ports::{PlatformPaths, PortErrorKind};
    use rusqlite::Connection;

    use super::{
        MIGRATIONS, Migration, STORAGE_SCHEMA_VERSION, SqliteStorage, apply_migrations,
        validate_migration_sequence,
    };

    static TEST_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let sequence = TEST_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "bulls-storage-test-{}-{}",
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
    fn storage_lifecycle_uses_data_directory_and_initializes_current_schema() {
        let root = TestDirectory::new();
        let paths = PlatformPaths::new(
            root.path().join("config"),
            root.path().join("data"),
            root.path().join("state"),
            root.path().join("cache"),
        );
        let expected_path = paths.data_dir().join("bulls.sqlite");

        let storage =
            SqliteStorage::from_platform_paths(&paths).expect("initial storage open must succeed");
        assert_eq!(storage.path(), expected_path);
        assert!(storage.path().is_file());
        assert_eq!(
            storage.schema_version().expect("schema must be readable"),
            STORAGE_SCHEMA_VERSION
        );

        let foreign_keys: i64 = storage
            .connection
            .pragma_query_value(None, "foreign_keys", |row| row.get(0))
            .expect("foreign key pragma must be readable");
        assert_eq!(foreign_keys, 1);
        drop(storage);

        drop(SqliteStorage::from_platform_paths(&paths).expect("reopening storage must succeed"));

        let connection = Connection::open(expected_path).expect("test connection must open");
        let mut statement = connection
            .prepare(
                "SELECT name FROM sqlite_schema \
                 WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
            )
            .expect("schema query must prepare");
        let tables: Vec<String> = statement
            .query_map([], |row| row.get(0))
            .expect("schema query must succeed")
            .collect::<Result<_, _>>()
            .expect("schema rows must decode");
        assert_eq!(
            tables,
            [
                "location_identities",
                "locations",
                "repositories",
                "repository_identities",
                "repository_remote_observation_attempts",
                "repository_remote_observation_values",
                "workspace_revision",
                "worktree_git_observation_attempts",
                "worktrees",
            ]
        );
    }

    #[test]
    fn peer_connections_reuse_initialized_storage_without_running_migrations() {
        let root = TestDirectory::new();
        let paths = PlatformPaths::new(
            root.path().join("config"),
            root.path().join("data"),
            root.path().join("state"),
            root.path().join("cache"),
        );

        let storage =
            SqliteStorage::from_platform_paths(&paths).expect("storage bootstrap must succeed");
        let peer = storage
            .open_peer()
            .expect("peer connection must open after bootstrap");

        assert_eq!(peer.path(), storage.path());
        assert_eq!(
            peer.schema_version().expect("peer schema must be readable"),
            STORAGE_SCHEMA_VERSION
        );
        let foreign_keys: i64 = peer
            .connection
            .pragma_query_value(None, "foreign_keys", |row| row.get(0))
            .expect("peer foreign key pragma must be readable");
        assert_eq!(foreign_keys, 1);
    }

    #[test]
    fn relative_storage_directory_is_rejected() {
        let error = SqliteStorage::open("relative/data")
            .expect_err("relative storage must never resolve through the current directory");

        assert_eq!(error.kind(), PortErrorKind::InvariantViolation);
    }

    #[test]
    fn version_one_storage_migrates_to_current_schema() {
        let root = TestDirectory::new();
        let data_dir = root.path().join("data");
        fs::create_dir_all(&data_dir).expect("data directory must be created");
        let path = data_dir.join("bulls.sqlite");
        let connection = Connection::open(&path).expect("fixture database must open");
        connection
            .pragma_update(None, "user_version", 1)
            .expect("fixture schema version must be written");
        drop(connection);

        let storage = SqliteStorage::open(&data_dir).expect("version one storage must migrate");

        assert_eq!(
            storage.schema_version().expect("schema must be readable"),
            STORAGE_SCHEMA_VERSION
        );
        let table_count: i64 = storage
            .connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_schema \
                 WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
                [],
                |row| row.get(0),
            )
            .expect("catalog tables must exist");
        assert_eq!(table_count, 9);
    }

    #[test]
    fn version_two_catalog_storage_migrates_to_observation_schema() {
        let root = TestDirectory::new();
        let data_dir = root.path().join("data");
        fs::create_dir_all(&data_dir).expect("data directory must be created");
        let path = data_dir.join("bulls.sqlite");
        let connection = Connection::open(&path).expect("fixture database must open");
        let catalog_migration = MIGRATIONS
            .iter()
            .find(|migration| migration.version == 2)
            .expect("catalog migration must exist");
        connection
            .execute_batch(catalog_migration.sql)
            .expect("catalog fixture schema must be created");
        connection
            .pragma_update(None, "user_version", 2)
            .expect("fixture schema version must be written");
        drop(connection);

        let storage = SqliteStorage::open(&data_dir)
            .expect("version two storage must migrate to observation schema");

        assert_eq!(
            storage.schema_version().expect("schema must be readable"),
            STORAGE_SCHEMA_VERSION
        );
        let observation_table_count: i64 = storage
            .connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_schema \
                 WHERE type = 'table' AND name LIKE '%observation%'",
                [],
                |row| row.get(0),
            )
            .expect("observation tables must exist");
        assert_eq!(observation_table_count, 3);
    }

    #[test]
    fn version_three_observation_storage_migrates_to_workspace_revision_schema() {
        let root = TestDirectory::new();
        let data_dir = root.path().join("data");
        fs::create_dir_all(&data_dir).expect("data directory must be created");
        let path = data_dir.join("bulls.sqlite");
        let connection = Connection::open(&path).expect("fixture database must open");
        for version in [2, 3] {
            let migration = MIGRATIONS
                .iter()
                .find(|migration| migration.version == version)
                .expect("fixture migration must exist");
            connection
                .execute_batch(migration.sql)
                .expect("fixture migration must succeed");
        }
        connection
            .pragma_update(None, "user_version", 3)
            .expect("fixture schema version must be written");
        drop(connection);

        let storage = SqliteStorage::open(&data_dir)
            .expect("version three storage must migrate to workspace revision schema");

        assert_eq!(
            storage.schema_version().expect("schema must be readable"),
            STORAGE_SCHEMA_VERSION
        );
        assert_eq!(
            storage
                .workspace_revision()
                .expect("workspace revision must be readable")
                .value(),
            0
        );
    }

    #[test]
    fn unsupported_future_schema_is_rejected() {
        let root = TestDirectory::new();
        let data_dir = root.path().join("data");
        fs::create_dir_all(&data_dir).expect("data directory must be created");
        let path = data_dir.join("bulls.sqlite");
        let connection = Connection::open(&path).expect("fixture database must open");
        connection
            .pragma_update(None, "user_version", STORAGE_SCHEMA_VERSION + 1)
            .expect("fixture schema version must be written");
        drop(connection);

        let error = SqliteStorage::open(&data_dir)
            .expect_err("future storage schema must not be opened by an older application");

        assert_eq!(error.kind(), PortErrorKind::InvalidData);
    }

    #[test]
    fn unversioned_non_bulls_database_is_not_adopted() {
        let root = TestDirectory::new();
        let data_dir = root.path().join("data");
        fs::create_dir_all(&data_dir).expect("data directory must be created");
        let path = data_dir.join("bulls.sqlite");
        let connection = Connection::open(&path).expect("fixture database must open");
        connection
            .execute_batch("CREATE TABLE foreign_data (value INTEGER NOT NULL);")
            .expect("foreign fixture must be created");
        drop(connection);

        let error = SqliteStorage::open(&data_dir)
            .expect_err("unversioned foreign schema must not be adopted");

        assert_eq!(error.kind(), PortErrorKind::InvalidData);
    }

    #[test]
    fn migration_sequence_rejects_gaps_duplicates_reordering_and_future_entries() {
        fn assert_invalid(migrations: &[Migration], supported_version: i64) {
            let error = validate_migration_sequence(0, supported_version, migrations)
                .expect_err("invalid migration sequence must be rejected");

            assert_eq!(error.kind(), PortErrorKind::InvariantViolation);
        }

        assert_invalid(
            &[Migration {
                version: 1,
                sql: "",
            }],
            2,
        );
        assert_invalid(
            &[
                Migration {
                    version: 1,
                    sql: "",
                },
                Migration {
                    version: 1,
                    sql: "",
                },
                Migration {
                    version: 2,
                    sql: "",
                },
            ],
            2,
        );
        assert_invalid(
            &[
                Migration {
                    version: 2,
                    sql: "",
                },
                Migration {
                    version: 1,
                    sql: "",
                },
            ],
            2,
        );
        assert_invalid(
            &[
                Migration {
                    version: 1,
                    sql: "",
                },
                Migration {
                    version: 2,
                    sql: "",
                },
                Migration {
                    version: 3,
                    sql: "",
                },
            ],
            2,
        );
    }

    #[test]
    fn migration_sequence_validates_only_versions_after_the_current_schema() {
        let migrations = [
            Migration {
                version: 1,
                sql: "",
            },
            Migration {
                version: 2,
                sql: "",
            },
            Migration {
                version: 3,
                sql: "",
            },
        ];

        validate_migration_sequence(1, 3, &migrations)
            .expect("existing migration history must not invalidate later upgrades");
    }

    #[test]
    fn failed_migration_rolls_back_schema_and_version_together() {
        let mut connection = Connection::open_in_memory().expect("in-memory database must open");
        let migrations = [
            Migration {
                version: 1,
                sql: "CREATE TABLE migration_probe (value INTEGER NOT NULL);",
            },
            Migration {
                version: 2,
                sql: "INVALID SQL",
            },
        ];

        let error = apply_migrations(&mut connection, &migrations, 2)
            .expect_err("invalid migration must fail");
        let object_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_schema WHERE name = 'migration_probe'",
                [],
                |row| row.get(0),
            )
            .expect("schema query must succeed");
        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("schema version must be readable");

        assert_eq!(error.kind(), PortErrorKind::StorageFailure);
        assert_eq!(object_count, 0);
        assert_eq!(version, 0);
    }
}
