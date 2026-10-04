// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};

use bulls_core::{Location, LocationId, Repository, RepositoryId, Worktree};

use super::{FilesystemObjectId, PortResult};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CatalogReconciliationCoverage {
    Complete,
    Partial,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CatalogMissingScope {
    None,
    Tree,
    Filesystem(u128),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepositoryIdentityEvidence {
    repository_id: RepositoryId,
    common_dir: PathBuf,
    common_dir_object_id: Option<FilesystemObjectId>,
}

impl RepositoryIdentityEvidence {
    pub fn new(
        repository_id: RepositoryId,
        common_dir: impl Into<PathBuf>,
        common_dir_object_id: Option<FilesystemObjectId>,
    ) -> Self {
        Self {
            repository_id,
            common_dir: common_dir.into(),
            common_dir_object_id,
        }
    }

    pub fn repository_id(&self) -> &RepositoryId {
        &self.repository_id
    }

    pub fn common_dir(&self) -> &Path {
        &self.common_dir
    }

    pub const fn common_dir_object_id(&self) -> Option<FilesystemObjectId> {
        self.common_dir_object_id
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocationIdentityEvidence {
    location_id: LocationId,
    object_id: Option<FilesystemObjectId>,
}

impl LocationIdentityEvidence {
    pub const fn new(location_id: LocationId, object_id: Option<FilesystemObjectId>) -> Self {
        Self {
            location_id,
            object_id,
        }
    }

    pub const fn location_id(&self) -> &LocationId {
        &self.location_id
    }

    pub const fn object_id(&self) -> Option<FilesystemObjectId> {
        self.object_id
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogReconciliation {
    discovery_root: PathBuf,
    coverage: CatalogReconciliationCoverage,
    missing_scope: CatalogMissingScope,
    repositories: Vec<Repository>,
    locations: Vec<Location>,
    worktrees: Vec<Worktree>,
    repository_identities: Vec<RepositoryIdentityEvidence>,
    location_identities: Vec<LocationIdentityEvidence>,
}

impl CatalogReconciliation {
    pub fn new(
        discovery_root: impl Into<PathBuf>,
        coverage: CatalogReconciliationCoverage,
        missing_scope: CatalogMissingScope,
    ) -> Self {
        Self {
            discovery_root: discovery_root.into(),
            coverage,
            missing_scope,
            repositories: Vec::new(),
            locations: Vec::new(),
            worktrees: Vec::new(),
            repository_identities: Vec::new(),
            location_identities: Vec::new(),
        }
    }

    pub fn with_entities(
        mut self,
        repositories: Vec<Repository>,
        locations: Vec<Location>,
        worktrees: Vec<Worktree>,
    ) -> Self {
        self.repositories = repositories;
        self.locations = locations;
        self.worktrees = worktrees;
        self
    }

    pub fn with_identity_evidence(
        mut self,
        repository_identities: Vec<RepositoryIdentityEvidence>,
        location_identities: Vec<LocationIdentityEvidence>,
    ) -> Self {
        self.repository_identities = repository_identities;
        self.location_identities = location_identities;
        self
    }

    pub fn discovery_root(&self) -> &Path {
        &self.discovery_root
    }

    pub const fn coverage(&self) -> CatalogReconciliationCoverage {
        self.coverage
    }

    pub const fn missing_scope(&self) -> CatalogMissingScope {
        self.missing_scope
    }

    pub fn repositories(&self) -> &[Repository] {
        &self.repositories
    }

    pub fn locations(&self) -> &[Location] {
        &self.locations
    }

    pub fn worktrees(&self) -> &[Worktree] {
        &self.worktrees
    }

    pub fn repository_identities(&self) -> &[RepositoryIdentityEvidence] {
        &self.repository_identities
    }

    pub fn location_identities(&self) -> &[LocationIdentityEvidence] {
        &self.location_identities
    }
}

pub trait RepositoryCatalogPort {
    fn repositories(&self) -> PortResult<Vec<Repository>>;

    fn repository(&self, repository_id: &RepositoryId) -> PortResult<Option<Repository>>;

    fn location(&self, location_id: &LocationId) -> PortResult<Option<Location>>;

    fn location_by_path(&self, path: &Path) -> PortResult<Option<Location>>;

    fn repository_id_by_common_dir(&self, common_dir: &Path) -> PortResult<Option<RepositoryId>>;

    fn repository_id_by_common_dir_object(
        &self,
        object_id: FilesystemObjectId,
    ) -> PortResult<Option<RepositoryId>>;

    fn location_id_by_filesystem_object(
        &self,
        object_id: FilesystemObjectId,
    ) -> PortResult<Option<LocationId>>;

    fn locations_for_repository(&self, repository_id: &RepositoryId) -> PortResult<Vec<Location>>;

    fn worktrees_for_repository(&self, repository_id: &RepositoryId) -> PortResult<Vec<Worktree>>;

    fn mark_discovery_root_offline(&mut self, root: &Path) -> PortResult<()>;

    fn reconcile(&mut self, reconciliation: &CatalogReconciliation) -> PortResult<()>;
}
