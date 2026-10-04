// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use bulls_core::{
    Location, LocationAvailability, LocationId, Repository, RepositoryId, Worktree, WorktreeId,
};

use crate::OperationCompletion;
use crate::ports::{
    CatalogMissingScope, CatalogReconciliation, CatalogReconciliationCoverage,
    DiscoveryCompleteness, DiscoveryIssue, DiscoveryRequest, FilesystemBoundary,
    FilesystemIdentityEvidence, FilesystemIdentityPort, FilesystemObjectId, GitRepositoryLayout,
    GitRepositoryProbePort, IdentityGeneratorPort, LocationIdentityEvidence, PortError,
    PortErrorKind, PortResult, RepositoryCatalogPort, RepositoryDiscoveryPort,
    RepositoryIdentityEvidence,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepositoryIdentificationIssue {
    path: PathBuf,
    kind: PortErrorKind,
}

impl RepositoryIdentificationIssue {
    pub fn new(path: impl Into<PathBuf>, kind: PortErrorKind) -> Self {
        Self {
            path: path.into(),
            kind,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub const fn kind(&self) -> PortErrorKind {
        self.kind
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoverRepositoriesOutcome {
    root: PathBuf,
    coverage: CatalogReconciliationCoverage,
    discovery_issues: Vec<DiscoveryIssue>,
    identification_issues: Vec<RepositoryIdentificationIssue>,
    repository_count: usize,
    location_count: usize,
    worktree_count: usize,
}

impl DiscoverRepositoriesOutcome {
    pub(crate) fn new(
        root: impl Into<PathBuf>,
        coverage: CatalogReconciliationCoverage,
        discovery_issues: Vec<DiscoveryIssue>,
        identification_issues: Vec<RepositoryIdentificationIssue>,
        repository_count: usize,
        location_count: usize,
        worktree_count: usize,
    ) -> Self {
        Self {
            root: root.into(),
            coverage,
            discovery_issues,
            identification_issues,
            repository_count,
            location_count,
            worktree_count,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub const fn coverage(&self) -> CatalogReconciliationCoverage {
        self.coverage
    }

    pub fn discovery_issues(&self) -> &[DiscoveryIssue] {
        &self.discovery_issues
    }

    pub fn identification_issues(&self) -> &[RepositoryIdentificationIssue] {
        &self.identification_issues
    }

    pub const fn repository_count(&self) -> usize {
        self.repository_count
    }

    pub const fn location_count(&self) -> usize {
        self.location_count
    }

    pub const fn worktree_count(&self) -> usize {
        self.worktree_count
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveryRootFailure {
    root: PathBuf,
    kind: PortErrorKind,
}

impl DiscoveryRootFailure {
    pub fn new(root: impl Into<PathBuf>, kind: PortErrorKind) -> Self {
        Self {
            root: root.into(),
            kind,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub const fn kind(&self) -> PortErrorKind {
        self.kind
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DiscoveryBatchOutcome {
    completed: Vec<DiscoverRepositoriesOutcome>,
    failed_roots: Vec<DiscoveryRootFailure>,
}

impl DiscoveryBatchOutcome {
    pub fn new(
        completed: Vec<DiscoverRepositoriesOutcome>,
        failed_roots: Vec<DiscoveryRootFailure>,
    ) -> Self {
        Self {
            completed,
            failed_roots,
        }
    }

    pub fn completed(&self) -> &[DiscoverRepositoriesOutcome] {
        &self.completed
    }

    pub fn failed_roots(&self) -> &[DiscoveryRootFailure] {
        &self.failed_roots
    }

    pub fn requested_root_count(&self) -> usize {
        self.completed.len() + self.failed_roots.len()
    }

    pub fn completed_root_count(&self) -> usize {
        self.completed.len()
    }

    pub fn partial_root_count(&self) -> usize {
        self.completed
            .iter()
            .filter(|outcome| outcome.coverage() == CatalogReconciliationCoverage::Partial)
            .count()
    }

    pub fn failed_root_count(&self) -> usize {
        self.failed_roots.len()
    }

    pub fn discovery_issue_count(&self) -> usize {
        self.completed
            .iter()
            .map(|outcome| outcome.discovery_issues().len())
            .sum()
    }

    pub fn identification_issue_count(&self) -> usize {
        self.completed
            .iter()
            .map(|outcome| outcome.identification_issues().len())
            .sum()
    }

    pub fn repository_count(&self) -> usize {
        self.completed
            .iter()
            .map(DiscoverRepositoriesOutcome::repository_count)
            .sum()
    }

    pub fn location_count(&self) -> usize {
        self.completed
            .iter()
            .map(DiscoverRepositoriesOutcome::location_count)
            .sum()
    }

    pub fn worktree_count(&self) -> usize {
        self.completed
            .iter()
            .map(DiscoverRepositoriesOutcome::worktree_count)
            .sum()
    }

    pub fn completion(&self) -> OperationCompletion {
        if self.requested_root_count() == 0
            || (self.completed.is_empty() && !self.failed_roots.is_empty())
        {
            OperationCompletion::Failed
        } else if !self.failed_roots.is_empty() || self.partial_root_count() > 0 {
            OperationCompletion::Partial
        } else {
            OperationCompletion::Complete
        }
    }

    pub fn is_partial(&self) -> bool {
        !self.failed_roots.is_empty() || self.partial_root_count() > 0
    }
}

pub struct DiscoverRepositories<'a> {
    discovery: &'a dyn RepositoryDiscoveryPort,
    git: &'a dyn GitRepositoryProbePort,
    filesystem: &'a dyn FilesystemIdentityPort,
    catalog: &'a mut dyn RepositoryCatalogPort,
    identities: &'a mut dyn IdentityGeneratorPort,
}

impl<'a> DiscoverRepositories<'a> {
    pub fn new(
        discovery: &'a dyn RepositoryDiscoveryPort,
        git: &'a dyn GitRepositoryProbePort,
        filesystem: &'a dyn FilesystemIdentityPort,
        catalog: &'a mut dyn RepositoryCatalogPort,
        identities: &'a mut dyn IdentityGeneratorPort,
    ) -> Self {
        Self {
            discovery,
            git,
            filesystem,
            catalog,
            identities,
        }
    }

    pub fn execute(
        &mut self,
        request: &DiscoveryRequest,
    ) -> PortResult<DiscoverRepositoriesOutcome> {
        let discovery = match self.discovery.discover(request) {
            Ok(discovery) => discovery,
            Err(error) if error.kind() == PortErrorKind::ResourceUnavailable => {
                self.catalog.mark_discovery_root_offline(request.root())?;
                return Err(error);
            }
            Err(error) => return Err(error),
        };
        let mut state = ReconciliationState::default();
        let mut identification_issues = Vec::new();

        let root_filesystem_id = if request.exclusions().is_empty()
            && request.filesystem_boundary() == FilesystemBoundary::StayOnRootFilesystem
        {
            match self.filesystem.identity(discovery.root()) {
                Ok(FilesystemIdentityEvidence::Strong(object_id)) => {
                    Some(object_id.filesystem_id())
                }
                Ok(FilesystemIdentityEvidence::PathOnly) => None,
                Err(error) => {
                    identification_issues.push(RepositoryIdentificationIssue::new(
                        discovery.root(),
                        error.kind(),
                    ));
                    None
                }
            }
        } else {
            None
        };

        for candidate in discovery.candidates() {
            let layout = match self.git.probe(candidate) {
                Ok(layout) => layout,
                Err(error) if error.kind() == PortErrorKind::Cancelled => return Err(error),
                Err(error) => {
                    identification_issues.push(RepositoryIdentificationIssue::new(
                        candidate.path(),
                        error.kind(),
                    ));
                    continue;
                }
            };

            validate_layout(&layout)?;

            let common_dir_object_id = match self.filesystem.identity(layout.common_dir()) {
                Ok(identity) => identity.object_id(),
                Err(error) => {
                    identification_issues.push(RepositoryIdentificationIssue::new(
                        candidate.path(),
                        error.kind(),
                    ));
                    continue;
                }
            };

            let location_path = layout.worktree_path().unwrap_or(layout.common_dir());
            let location_object_id = if location_path == layout.common_dir() {
                common_dir_object_id
            } else {
                match self.filesystem.identity(location_path) {
                    Ok(identity) => identity.object_id(),
                    Err(error) => {
                        identification_issues.push(RepositoryIdentificationIssue::new(
                            candidate.path(),
                            error.kind(),
                        ));
                        continue;
                    }
                }
            };

            let repository_id =
                self.resolve_repository_id(&mut state, layout.common_dir(), common_dir_object_id)?;
            let location_id = self.resolve_location_id(
                &mut state,
                &repository_id,
                location_path,
                location_object_id,
            )?;

            state.add_repository(
                repository_id.clone(),
                layout.common_dir(),
                common_dir_object_id,
            );
            state.add_location(
                repository_id.clone(),
                location_id.clone(),
                location_path,
                location_object_id,
            );

            if layout.worktree_path().is_some() {
                let worktree_id =
                    self.resolve_worktree_id(&mut state, &repository_id, &location_id)?;
                state.add_worktree(repository_id, location_id, worktree_id);
            }
        }

        let coverage = if discovery.completeness() == DiscoveryCompleteness::Complete
            && identification_issues.is_empty()
        {
            CatalogReconciliationCoverage::Complete
        } else {
            CatalogReconciliationCoverage::Partial
        };

        let missing_scope = missing_scope(request, coverage, root_filesystem_id);
        let reconciliation = state.finish(discovery.root(), coverage, missing_scope);
        let outcome = DiscoverRepositoriesOutcome::new(
            discovery.root(),
            coverage,
            discovery.issues().to_vec(),
            identification_issues,
            reconciliation.repositories().len(),
            reconciliation.locations().len(),
            reconciliation.worktrees().len(),
        );

        self.catalog.reconcile(&reconciliation)?;
        Ok(outcome)
    }

    fn resolve_repository_id(
        &mut self,
        state: &mut ReconciliationState,
        common_dir: &Path,
        object_id: Option<FilesystemObjectId>,
    ) -> PortResult<RepositoryId> {
        if let Some(object_id) = object_id {
            if let Some(repository_id) = state.repository_ids_by_object.get(&object_id) {
                return Ok(repository_id.clone());
            }

            if let Some(repository_id) =
                self.catalog.repository_id_by_common_dir_object(object_id)?
            {
                ensure_repository_exists(self.catalog, &repository_id)?;
                state
                    .repository_ids_by_object
                    .insert(object_id, repository_id.clone());
                return Ok(repository_id);
            }
        } else {
            if let Some(repository_id) = state.repository_ids_by_path.get(common_dir) {
                return Ok(repository_id.clone());
            }

            if let Some(repository_id) = self.catalog.repository_id_by_common_dir(common_dir)? {
                ensure_repository_exists(self.catalog, &repository_id)?;
                state
                    .repository_ids_by_path
                    .insert(common_dir.to_path_buf(), repository_id.clone());
                return Ok(repository_id);
            }
        }

        let repository_id = self.identities.next_repository_id()?;
        if let Some(object_id) = object_id {
            state
                .repository_ids_by_object
                .insert(object_id, repository_id.clone());
        } else {
            state
                .repository_ids_by_path
                .insert(common_dir.to_path_buf(), repository_id.clone());
        }

        Ok(repository_id)
    }

    fn resolve_location_id(
        &mut self,
        state: &mut ReconciliationState,
        repository_id: &RepositoryId,
        path: &Path,
        object_id: Option<FilesystemObjectId>,
    ) -> PortResult<LocationId> {
        let location_id = if let Some(object_id) = object_id {
            if let Some(location_id) = state.location_ids_by_object.get(&object_id) {
                location_id.clone()
            } else if let Some(location_id) =
                self.catalog.location_id_by_filesystem_object(object_id)?
            {
                state
                    .location_ids_by_object
                    .insert(object_id, location_id.clone());
                location_id
            } else {
                let location_id = self.identities.next_location_id()?;
                state
                    .location_ids_by_object
                    .insert(object_id, location_id.clone());
                location_id
            }
        } else if let Some(location_id) = state.location_ids_by_path.get(path) {
            location_id.clone()
        } else if let Some(location) = self.catalog.location_by_path(path)? {
            let location_id = location.id().clone();
            state
                .location_ids_by_path
                .insert(path.to_path_buf(), location_id.clone());
            location_id
        } else {
            let location_id = self.identities.next_location_id()?;
            state
                .location_ids_by_path
                .insert(path.to_path_buf(), location_id.clone());
            location_id
        };

        ensure_location_belongs_to_repository(self.catalog, &location_id, repository_id)?;
        Ok(location_id)
    }

    fn resolve_worktree_id(
        &mut self,
        state: &mut ReconciliationState,
        repository_id: &RepositoryId,
        location_id: &LocationId,
    ) -> PortResult<WorktreeId> {
        if let Some(worktree_id) = state.worktree_ids_by_location.get(location_id) {
            return Ok(worktree_id.clone());
        }

        if let Some(worktree) = self
            .catalog
            .worktrees_for_repository(repository_id)?
            .into_iter()
            .find(|worktree| worktree.location_id() == location_id)
        {
            let worktree_id = worktree.id().clone();
            state
                .worktree_ids_by_location
                .insert(location_id.clone(), worktree_id.clone());
            return Ok(worktree_id);
        }

        let worktree_id = self.identities.next_worktree_id()?;
        state
            .worktree_ids_by_location
            .insert(location_id.clone(), worktree_id.clone());
        Ok(worktree_id)
    }
}

#[derive(Default)]
struct ReconciliationState {
    repository_ids_by_object: HashMap<FilesystemObjectId, RepositoryId>,
    repository_ids_by_path: HashMap<PathBuf, RepositoryId>,
    location_ids_by_object: HashMap<FilesystemObjectId, LocationId>,
    location_ids_by_path: HashMap<PathBuf, LocationId>,
    worktree_ids_by_location: HashMap<LocationId, WorktreeId>,
    repository_ids: HashSet<RepositoryId>,
    location_ids: HashSet<LocationId>,
    worktree_ids: HashSet<WorktreeId>,
    repositories: Vec<Repository>,
    locations: Vec<Location>,
    worktrees: Vec<Worktree>,
    repository_identities: Vec<RepositoryIdentityEvidence>,
    location_identities: Vec<LocationIdentityEvidence>,
}

impl ReconciliationState {
    fn add_repository(
        &mut self,
        repository_id: RepositoryId,
        common_dir: &Path,
        object_id: Option<FilesystemObjectId>,
    ) {
        if !self.repository_ids.insert(repository_id.clone()) {
            return;
        }

        self.repositories
            .push(Repository::new(repository_id.clone()));
        self.repository_identities
            .push(RepositoryIdentityEvidence::new(
                repository_id,
                common_dir,
                object_id,
            ));
    }

    fn add_location(
        &mut self,
        repository_id: RepositoryId,
        location_id: LocationId,
        path: &Path,
        object_id: Option<FilesystemObjectId>,
    ) {
        if !self.location_ids.insert(location_id.clone()) {
            return;
        }

        self.locations.push(Location::new(
            location_id.clone(),
            repository_id,
            path,
            LocationAvailability::Available,
        ));
        self.location_identities
            .push(LocationIdentityEvidence::new(location_id, object_id));
    }

    fn add_worktree(
        &mut self,
        repository_id: RepositoryId,
        location_id: LocationId,
        worktree_id: WorktreeId,
    ) {
        if !self.worktree_ids.insert(worktree_id.clone()) {
            return;
        }

        self.worktrees
            .push(Worktree::new(worktree_id, repository_id, location_id));
    }

    fn finish(
        self,
        discovery_root: &Path,
        coverage: CatalogReconciliationCoverage,
        missing_scope: CatalogMissingScope,
    ) -> CatalogReconciliation {
        CatalogReconciliation::new(discovery_root, coverage, missing_scope)
            .with_entities(self.repositories, self.locations, self.worktrees)
            .with_identity_evidence(self.repository_identities, self.location_identities)
    }
}

fn missing_scope(
    request: &DiscoveryRequest,
    coverage: CatalogReconciliationCoverage,
    root_filesystem_id: Option<u128>,
) -> CatalogMissingScope {
    if coverage != CatalogReconciliationCoverage::Complete || !request.exclusions().is_empty() {
        return CatalogMissingScope::None;
    }

    match request.filesystem_boundary() {
        FilesystemBoundary::CrossFilesystems => CatalogMissingScope::Tree,
        FilesystemBoundary::StayOnRootFilesystem => root_filesystem_id
            .map(CatalogMissingScope::Filesystem)
            .unwrap_or(CatalogMissingScope::None),
    }
}

fn validate_layout(layout: &GitRepositoryLayout) -> PortResult<()> {
    let paths_are_absolute = layout.git_dir().is_absolute()
        && layout.common_dir().is_absolute()
        && layout
            .worktree_path()
            .is_none_or(|worktree_path| worktree_path.is_absolute());

    if paths_are_absolute {
        Ok(())
    } else {
        Err(PortError::new(PortErrorKind::InvariantViolation))
    }
}

fn ensure_repository_exists(
    catalog: &dyn RepositoryCatalogPort,
    repository_id: &RepositoryId,
) -> PortResult<()> {
    if catalog.repository(repository_id)?.is_some() {
        Ok(())
    } else {
        Err(PortError::new(PortErrorKind::InvariantViolation))
    }
}

fn ensure_location_belongs_to_repository(
    catalog: &dyn RepositoryCatalogPort,
    location_id: &LocationId,
    repository_id: &RepositoryId,
) -> PortResult<()> {
    match catalog.location(location_id)? {
        Some(location) if location.repository_id() == repository_id => Ok(()),
        Some(_) => Err(PortError::new(PortErrorKind::InvariantViolation)),
        None => Ok(()),
    }
}
