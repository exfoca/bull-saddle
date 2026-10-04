// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{HashMap, HashSet};
#[cfg(any(unix, windows))]
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use bulls_application::ports::{
    CatalogMissingScope, CatalogReconciliation, CatalogReconciliationCoverage, FilesystemObjectId,
    PlatformPaths, PortResult, RepositoryCatalogPort,
};
use bulls_core::{
    Location, LocationAvailability, LocationId, Repository, RepositoryId, Worktree, WorktreeId,
};
use rusqlite::{OptionalExtension, TransactionBehavior, params};

use crate::error::{invalid_data, invariant_violation, sqlite_port_error};
use crate::storage::{SqliteStorage, advance_workspace_revision};

#[cfg(not(any(unix, windows)))]
compile_error!(
    "BullSaddle catalog path persistence requires Unix or Windows path encoding support."
);

pub struct SqliteRepositoryCatalog {
    storage: SqliteStorage,
}

impl SqliteRepositoryCatalog {
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

impl RepositoryCatalogPort for SqliteRepositoryCatalog {
    fn repositories(&self) -> PortResult<Vec<Repository>> {
        load_all_repositories(self.storage.connection())
    }

    fn repository(&self, repository_id: &RepositoryId) -> PortResult<Option<Repository>> {
        self.storage
            .connection()
            .query_row(
                "SELECT repository_id FROM repositories WHERE repository_id = ?1",
                [repository_id.as_str()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map(|value| value.map(RepositoryId::from).map(Repository::new))
            .map_err(|error| sqlite_port_error(&error))
    }

    fn location(&self, location_id: &LocationId) -> PortResult<Option<Location>> {
        let row = self
            .storage
            .connection()
            .query_row(
                "SELECT location_id, repository_id, path, availability \
                 FROM locations WHERE location_id = ?1",
                [location_id.as_str()],
                read_location_row,
            )
            .optional()
            .map_err(|error| sqlite_port_error(&error))?;

        row.map(location_from_row).transpose()
    }

    fn location_by_path(&self, path: &Path) -> PortResult<Option<Location>> {
        let row = self
            .storage
            .connection()
            .query_row(
                "SELECT location_id, repository_id, path, availability \
                 FROM locations WHERE path = ?1 \
                 ORDER BY CASE availability \
                     WHEN 'available' THEN 0 \
                     WHEN 'offline' THEN 1 \
                     ELSE 2 END, rowid DESC LIMIT 1",
                [encode_path(path)],
                read_location_row,
            )
            .optional()
            .map_err(|error| sqlite_port_error(&error))?;

        row.map(location_from_row).transpose()
    }

    fn repository_id_by_common_dir(&self, common_dir: &Path) -> PortResult<Option<RepositoryId>> {
        self.storage
            .connection()
            .query_row(
                "SELECT ri.repository_id \
                 FROM repository_identities ri \
                 WHERE ri.common_dir = ?1 \
                 ORDER BY EXISTS( \
                     SELECT 1 FROM locations l \
                     WHERE l.repository_id = ri.repository_id \
                       AND l.availability = 'available' \
                 ) DESC, ri.rowid DESC \
                 LIMIT 1",
                [encode_path(common_dir)],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map(|value| value.map(RepositoryId::from))
            .map_err(|error| sqlite_port_error(&error))
    }

    fn repository_id_by_common_dir_object(
        &self,
        object_id: FilesystemObjectId,
    ) -> PortResult<Option<RepositoryId>> {
        self.storage
            .connection()
            .query_row(
                "SELECT repository_id FROM repository_identities \
                 WHERE filesystem_id = ?1 AND object_id = ?2 \
                 ORDER BY rowid DESC LIMIT 1",
                params![
                    encode_u128(object_id.filesystem_id()),
                    encode_u128(object_id.object_id())
                ],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map(|value| value.map(RepositoryId::from))
            .map_err(|error| sqlite_port_error(&error))
    }

    fn location_id_by_filesystem_object(
        &self,
        object_id: FilesystemObjectId,
    ) -> PortResult<Option<LocationId>> {
        self.storage
            .connection()
            .query_row(
                "SELECT location_id FROM location_identities \
                 WHERE filesystem_id = ?1 AND object_id = ?2 \
                 ORDER BY rowid DESC LIMIT 1",
                params![
                    encode_u128(object_id.filesystem_id()),
                    encode_u128(object_id.object_id())
                ],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map(|value| value.map(LocationId::from))
            .map_err(|error| sqlite_port_error(&error))
    }

    fn locations_for_repository(&self, repository_id: &RepositoryId) -> PortResult<Vec<Location>> {
        let mut statement = self
            .storage
            .connection()
            .prepare(
                "SELECT location_id, repository_id, path, availability \
                 FROM locations WHERE repository_id = ?1 ORDER BY location_id",
            )
            .map_err(|error| sqlite_port_error(&error))?;
        let rows = statement
            .query_map([repository_id.as_str()], read_location_row)
            .map_err(|error| sqlite_port_error(&error))?;

        rows.map(|row| {
            row.map_err(|error| sqlite_port_error(&error))
                .and_then(location_from_row)
        })
        .collect()
    }

    fn worktrees_for_repository(&self, repository_id: &RepositoryId) -> PortResult<Vec<Worktree>> {
        let mut statement = self
            .storage
            .connection()
            .prepare(
                "SELECT worktree_id, repository_id, location_id \
                 FROM worktrees WHERE repository_id = ?1 ORDER BY worktree_id",
            )
            .map_err(|error| sqlite_port_error(&error))?;
        let rows = statement
            .query_map([repository_id.as_str()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|error| sqlite_port_error(&error))?;

        rows.map(|row| {
            row.map(|(worktree_id, repository_id, location_id)| {
                Worktree::new(
                    WorktreeId::from(worktree_id),
                    RepositoryId::from(repository_id),
                    LocationId::from(location_id),
                )
            })
            .map_err(|error| sqlite_port_error(&error))
        })
        .collect()
    }

    fn mark_discovery_root_offline(&mut self, root: &Path) -> PortResult<()> {
        if !root.is_absolute() {
            return Ok(());
        }

        let transaction = self
            .storage
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| sqlite_port_error(&error))?;
        let affected: Vec<_> = load_location_paths(&transaction)?
            .into_iter()
            .filter(|(_, path)| path.starts_with(root))
            .map(|(location_id, _)| location_id)
            .collect();

        let mut changed = false;
        for location_id in affected {
            changed |= transaction
                .execute(
                    "UPDATE locations SET availability = 'offline' \
                     WHERE location_id = ?1 AND availability != 'offline'",
                    [location_id.as_str()],
                )
                .map_err(|error| sqlite_port_error(&error))?
                != 0;
        }

        if changed {
            advance_workspace_revision(&transaction)?;
        }
        transaction
            .commit()
            .map_err(|error| sqlite_port_error(&error))
    }

    fn reconcile(&mut self, reconciliation: &CatalogReconciliation) -> PortResult<()> {
        validate_reconciliation(reconciliation)?;

        let transaction = self
            .storage
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| sqlite_port_error(&error))?;

        for repository in reconciliation.repositories() {
            transaction
                .execute(
                    "INSERT INTO repositories (repository_id) VALUES (?1) \
                     ON CONFLICT(repository_id) DO NOTHING",
                    [repository.id().as_str()],
                )
                .map_err(|error| sqlite_port_error(&error))?;
        }

        for location in reconciliation.locations() {
            transaction
                .execute(
                    "INSERT INTO locations (location_id, repository_id, path, availability) \
                     VALUES (?1, ?2, ?3, 'available') \
                     ON CONFLICT(location_id) DO UPDATE SET \
                         repository_id = excluded.repository_id, \
                         path = excluded.path, \
                         availability = 'available'",
                    params![
                        location.id().as_str(),
                        location.repository_id().as_str(),
                        encode_path(location.path())
                    ],
                )
                .map_err(|error| sqlite_port_error(&error))?;
        }

        for worktree in reconciliation.worktrees() {
            transaction
                .execute(
                    "INSERT INTO worktrees (worktree_id, repository_id, location_id) \
                     VALUES (?1, ?2, ?3) \
                     ON CONFLICT(worktree_id) DO UPDATE SET \
                         repository_id = excluded.repository_id, \
                         location_id = excluded.location_id",
                    params![
                        worktree.id().as_str(),
                        worktree.repository_id().as_str(),
                        worktree.location_id().as_str()
                    ],
                )
                .map_err(|error| sqlite_port_error(&error))?;
        }

        for evidence in reconciliation.repository_identities() {
            let (filesystem_id, object_id) = encode_object_id(evidence.common_dir_object_id());
            transaction
                .execute(
                    "INSERT INTO repository_identities \
                         (repository_id, common_dir, filesystem_id, object_id) \
                     VALUES (?1, ?2, ?3, ?4) \
                     ON CONFLICT(repository_id) DO UPDATE SET \
                         common_dir = excluded.common_dir, \
                         filesystem_id = excluded.filesystem_id, \
                         object_id = excluded.object_id",
                    params![
                        evidence.repository_id().as_str(),
                        encode_path(evidence.common_dir()),
                        filesystem_id,
                        object_id
                    ],
                )
                .map_err(|error| sqlite_port_error(&error))?;
        }

        for evidence in reconciliation.location_identities() {
            let (filesystem_id, object_id) = encode_object_id(evidence.object_id());
            transaction
                .execute(
                    "INSERT INTO location_identities \
                         (location_id, filesystem_id, object_id) \
                     VALUES (?1, ?2, ?3) \
                     ON CONFLICT(location_id) DO UPDATE SET \
                         filesystem_id = excluded.filesystem_id, \
                         object_id = excluded.object_id",
                    params![evidence.location_id().as_str(), filesystem_id, object_id],
                )
                .map_err(|error| sqlite_port_error(&error))?;
        }

        reconcile_missing_locations(&transaction, reconciliation)?;
        advance_workspace_revision(&transaction)?;
        transaction
            .commit()
            .map_err(|error| sqlite_port_error(&error))
    }
}

pub(crate) fn load_all_repositories(
    connection: &rusqlite::Connection,
) -> PortResult<Vec<Repository>> {
    let mut statement = connection
        .prepare("SELECT repository_id FROM repositories ORDER BY repository_id")
        .map_err(|error| sqlite_port_error(&error))?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| sqlite_port_error(&error))?;

    rows.map(|row| {
        row.map(RepositoryId::from)
            .map(Repository::new)
            .map_err(|error| sqlite_port_error(&error))
    })
    .collect()
}

pub(crate) fn load_all_locations(connection: &rusqlite::Connection) -> PortResult<Vec<Location>> {
    let mut statement = connection
        .prepare(
            "SELECT location_id, repository_id, path, availability \
             FROM locations ORDER BY repository_id, location_id",
        )
        .map_err(|error| sqlite_port_error(&error))?;
    let rows = statement
        .query_map([], read_location_row)
        .map_err(|error| sqlite_port_error(&error))?;

    rows.map(|row| {
        row.map_err(|error| sqlite_port_error(&error))
            .and_then(location_from_row)
    })
    .collect()
}

pub(crate) fn load_all_worktrees(connection: &rusqlite::Connection) -> PortResult<Vec<Worktree>> {
    let mut statement = connection
        .prepare(
            "SELECT worktree_id, repository_id, location_id \
             FROM worktrees ORDER BY repository_id, worktree_id",
        )
        .map_err(|error| sqlite_port_error(&error))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|error| sqlite_port_error(&error))?;

    rows.map(|row| {
        let (worktree_id, repository_id, location_id) =
            row.map_err(|error| sqlite_port_error(&error))?;
        Ok(Worktree::new(
            WorktreeId::from(worktree_id),
            RepositoryId::from(repository_id),
            LocationId::from(location_id),
        ))
    })
    .collect()
}

type LocationRow = (String, String, Vec<u8>, String);

fn read_location_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<LocationRow> {
    Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
}

fn location_from_row(row: LocationRow) -> PortResult<Location> {
    let (location_id, repository_id, path, availability) = row;
    Ok(Location::new(
        LocationId::from(location_id),
        RepositoryId::from(repository_id),
        decode_path(path)?,
        decode_availability(&availability)?,
    ))
}

fn validate_reconciliation(reconciliation: &CatalogReconciliation) -> PortResult<()> {
    if !reconciliation.discovery_root().is_absolute()
        || (reconciliation.coverage() == CatalogReconciliationCoverage::Partial
            && reconciliation.missing_scope() != CatalogMissingScope::None)
    {
        return Err(invariant_violation());
    }

    let repositories: HashSet<_> = reconciliation
        .repositories()
        .iter()
        .map(|repository| repository.id().clone())
        .collect();
    if repositories.len() != reconciliation.repositories().len() {
        return Err(invariant_violation());
    }

    let mut locations = HashMap::new();
    for location in reconciliation.locations() {
        if !location.path().is_absolute()
            || location.availability() != LocationAvailability::Available
            || !repositories.contains(location.repository_id())
            || locations
                .insert(location.id().clone(), location.repository_id().clone())
                .is_some()
        {
            return Err(invariant_violation());
        }
    }

    let mut worktree_ids = HashSet::new();
    let mut worktree_locations = HashSet::new();
    for worktree in reconciliation.worktrees() {
        if locations.get(worktree.location_id()) != Some(worktree.repository_id())
            || !worktree_ids.insert(worktree.id().clone())
            || !worktree_locations.insert(worktree.location_id().clone())
        {
            return Err(invariant_violation());
        }
    }

    let mut repository_identity_ids = HashSet::new();
    for evidence in reconciliation.repository_identities() {
        if !evidence.common_dir().is_absolute()
            || !repositories.contains(evidence.repository_id())
            || !repository_identity_ids.insert(evidence.repository_id().clone())
        {
            return Err(invariant_violation());
        }
    }

    let mut location_identity_ids = HashSet::new();
    for evidence in reconciliation.location_identities() {
        if !locations.contains_key(evidence.location_id())
            || !location_identity_ids.insert(evidence.location_id().clone())
        {
            return Err(invariant_violation());
        }
    }

    Ok(())
}

fn reconcile_missing_locations(
    transaction: &rusqlite::Transaction<'_>,
    reconciliation: &CatalogReconciliation,
) -> PortResult<()> {
    let scope = reconciliation.missing_scope();
    if scope == CatalogMissingScope::None {
        return Ok(());
    }

    let observed: HashSet<_> = reconciliation
        .locations()
        .iter()
        .map(|location| location.id().clone())
        .collect();
    let mut statement = transaction
        .prepare(
            "SELECT l.location_id, l.path, li.filesystem_id \
             FROM locations l \
             LEFT JOIN location_identities li ON li.location_id = l.location_id",
        )
        .map_err(|error| sqlite_port_error(&error))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, Option<Vec<u8>>>(2)?,
            ))
        })
        .map_err(|error| sqlite_port_error(&error))?;

    let mut missing = Vec::new();
    for row in rows {
        let (location_id, path, filesystem_id) = row.map_err(|error| sqlite_port_error(&error))?;
        let location_id = LocationId::from(location_id);
        if observed.contains(&location_id) {
            continue;
        }

        let path = decode_path(path)?;
        if !path.starts_with(reconciliation.discovery_root()) {
            continue;
        }

        let in_scope = match scope {
            CatalogMissingScope::None => false,
            CatalogMissingScope::Tree => true,
            CatalogMissingScope::Filesystem(expected) => filesystem_id
                .map(decode_u128)
                .transpose()?
                .is_some_and(|actual| actual == expected),
        };
        if in_scope {
            missing.push(location_id);
        }
    }
    drop(statement);

    for location_id in missing {
        transaction
            .execute(
                "UPDATE locations SET availability = 'missing' WHERE location_id = ?1",
                [location_id.as_str()],
            )
            .map_err(|error| sqlite_port_error(&error))?;
    }

    Ok(())
}

fn load_location_paths(
    connection: &rusqlite::Connection,
) -> PortResult<Vec<(LocationId, PathBuf)>> {
    let mut statement = connection
        .prepare("SELECT location_id, path FROM locations")
        .map_err(|error| sqlite_port_error(&error))?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
        })
        .map_err(|error| sqlite_port_error(&error))?;

    rows.map(|row| {
        let (location_id, path) = row.map_err(|error| sqlite_port_error(&error))?;
        Ok((LocationId::from(location_id), decode_path(path)?))
    })
    .collect()
}

fn decode_availability(value: &str) -> PortResult<LocationAvailability> {
    match value {
        "available" => Ok(LocationAvailability::Available),
        "missing" => Ok(LocationAvailability::Missing),
        "offline" => Ok(LocationAvailability::Offline),
        _ => Err(invalid_data()),
    }
}

fn encode_object_id(object_id: Option<FilesystemObjectId>) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
    object_id.map_or((None, None), |object_id| {
        (
            Some(encode_u128(object_id.filesystem_id())),
            Some(encode_u128(object_id.object_id())),
        )
    })
}

fn encode_u128(value: u128) -> Vec<u8> {
    value.to_be_bytes().to_vec()
}

fn decode_u128(value: Vec<u8>) -> PortResult<u128> {
    let bytes: [u8; 16] = value.try_into().map_err(|_| invalid_data())?;
    Ok(u128::from_be_bytes(bytes))
}

#[cfg(unix)]
fn encode_path(path: &Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;

    path.as_os_str().as_bytes().to_vec()
}

#[cfg(unix)]
fn decode_path(value: Vec<u8>) -> PortResult<PathBuf> {
    use std::os::unix::ffi::OsStringExt;

    Ok(PathBuf::from(OsString::from_vec(value)))
}

#[cfg(windows)]
fn encode_path(path: &Path) -> Vec<u8> {
    use std::os::windows::ffi::OsStrExt;

    path.as_os_str()
        .encode_wide()
        .flat_map(u16::to_le_bytes)
        .collect()
}

#[cfg(windows)]
fn decode_path(value: Vec<u8>) -> PortResult<PathBuf> {
    use std::os::windows::ffi::OsStringExt;

    if value.len() % 2 != 0 {
        return Err(invalid_data());
    }

    let wide = value
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect::<Vec<_>>();
    Ok(PathBuf::from(OsString::from_wide(&wide)))
}
