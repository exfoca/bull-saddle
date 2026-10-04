// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use bulls_core::{
    Location, LocationAvailability, LocationId, ObservationAttempt, Remote, Repository,
    RepositoryId, Worktree, WorktreeGitState, WorktreeId,
};

use crate::ports::{
    PortError, PortErrorKind, PortResult, WorkspaceObservationState, WorkspaceReadPort,
    WorkspaceReadSnapshot,
};
use crate::{
    AdvisoryEvaluator, AdvisoryProjection, Knowledge, LocationProjection,
    ObservationAttemptOutcomeProjection, ObservationAttemptProjection, ObservationStatusProjection,
    ObservationSuccessProjection, OverviewProjection, RepositoryDetailProjection,
    RepositoryListItemProjection, RepositoryListProjection, WorkspaceMirror, WorkspaceRevision,
    WorktreeProjection,
};

pub const DEFAULT_QUERY_PAGE_SIZE: usize = 100;
pub const MAX_QUERY_PAGE_SIZE: usize = 256;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QueryPage {
    offset: usize,
    limit: usize,
}

impl QueryPage {
    pub const fn new(offset: usize, limit: usize) -> Option<Self> {
        if limit == 0 || limit > MAX_QUERY_PAGE_SIZE {
            return None;
        }
        Some(Self { offset, limit })
    }

    pub const fn offset(self) -> usize {
        self.offset
    }

    pub const fn limit(self) -> usize {
        self.limit
    }
}

impl Default for QueryPage {
    fn default() -> Self {
        Self {
            offset: 0,
            limit: DEFAULT_QUERY_PAGE_SIZE,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FreshnessConstraint {
    observed_at_or_after: Option<SystemTime>,
}

impl FreshnessConstraint {
    pub const fn any() -> Self {
        Self {
            observed_at_or_after: None,
        }
    }

    pub const fn observed_at_or_after(value: SystemTime) -> Self {
        Self {
            observed_at_or_after: Some(value),
        }
    }

    pub const fn minimum_observed_at(self) -> Option<SystemTime> {
        self.observed_at_or_after
    }

    pub fn accepts(self, observed_at: SystemTime) -> bool {
        self.observed_at_or_after
            .is_none_or(|minimum| observed_at >= minimum)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepositoryPredicate {
    LocationAvailability(LocationAvailability),
    HasRemotes(bool),
    HasLocalWork(bool),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepositoryListQuery {
    page: QueryPage,
    freshness: FreshnessConstraint,
    predicates: Vec<RepositoryPredicate>,
}

impl RepositoryListQuery {
    pub fn new(page: QueryPage) -> Self {
        Self {
            page,
            freshness: FreshnessConstraint::any(),
            predicates: Vec::new(),
        }
    }

    pub fn with_freshness(mut self, freshness: FreshnessConstraint) -> Self {
        self.freshness = freshness;
        self
    }

    pub fn with_predicate(mut self, predicate: RepositoryPredicate) -> Self {
        self.predicates.push(predicate);
        self
    }

    pub const fn page(&self) -> QueryPage {
        self.page
    }

    pub const fn freshness(&self) -> FreshnessConstraint {
        self.freshness
    }

    pub fn predicates(&self) -> &[RepositoryPredicate] {
        &self.predicates
    }
}

impl Default for RepositoryListQuery {
    fn default() -> Self {
        Self::new(QueryPage::default())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepositorySelector {
    repository_id_candidate: Option<RepositoryId>,
    path_candidate: PathBuf,
}

impl RepositorySelector {
    pub fn from_input(value: impl Into<OsString>, working_directory: &Path) -> Self {
        let value = value.into();
        let input_path = Path::new(&value);
        let path_candidate = if input_path.is_absolute() {
            input_path.to_path_buf()
        } else {
            working_directory.join(input_path)
        };

        Self {
            repository_id_candidate: value.to_str().map(RepositoryId::from),
            path_candidate: normalize_lexical_path(&path_candidate),
        }
    }

    pub fn repository_id_candidate(&self) -> Option<&RepositoryId> {
        self.repository_id_candidate.as_ref()
    }

    pub fn path_candidate(&self) -> &Path {
        &self.path_candidate
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RepositorySelectorResolution {
    Resolved(RepositoryId),
    NotFound,
    Ambiguous { candidates: Vec<RepositoryId> },
}

impl RepositorySelectorResolution {
    pub fn candidates(&self) -> &[RepositoryId] {
        match self {
            Self::Ambiguous { candidates } => candidates,
            Self::Resolved(_) | Self::NotFound => &[],
        }
    }
}

pub struct WorkspaceQueryService<'a> {
    read: &'a dyn WorkspaceReadPort,
}

impl<'a> WorkspaceQueryService<'a> {
    pub const fn new(read: &'a dyn WorkspaceReadPort) -> Self {
        Self { read }
    }

    pub fn overview(&self) -> PortResult<OverviewProjection> {
        let snapshot = self.read.read_workspace()?;
        ProjectionContext::new(&snapshot)?.overview()
    }

    pub fn repositories(
        &self,
        query: &RepositoryListQuery,
    ) -> PortResult<RepositoryListProjection> {
        let snapshot = self.read.read_workspace()?;
        ProjectionContext::new(&snapshot)?.repositories(query)
    }

    pub fn repository(
        &self,
        repository_id: &RepositoryId,
    ) -> PortResult<Option<RepositoryDetailProjection>> {
        let snapshot = self.read.read_workspace()?;
        ProjectionContext::new(&snapshot)?.repository(repository_id)
    }

    pub fn resolve_repository_selector(
        &self,
        selector: &RepositorySelector,
    ) -> PortResult<RepositorySelectorResolution> {
        let snapshot = self.read.read_workspace()?;
        ProjectionContext::new(&snapshot)?.resolve_repository_selector(selector)
    }

    pub fn worktree(&self, worktree_id: &WorktreeId) -> PortResult<Option<WorktreeProjection>> {
        let snapshot = self.read.read_workspace()?;
        ProjectionContext::new(&snapshot)?.worktree(worktree_id)
    }

    pub fn mirror(&self) -> PortResult<WorkspaceMirror> {
        let snapshot = self.read.read_workspace()?;
        ProjectionContext::new(&snapshot)?.mirror()
    }

    pub fn advisories(&self) -> PortResult<AdvisoryProjection> {
        let snapshot = self.read.read_workspace()?;
        let mirror = ProjectionContext::new(&snapshot)?.mirror()?;
        Ok(AdvisoryEvaluator::evaluate(&mirror))
    }

    pub fn overview_with_advisories(&self) -> PortResult<(OverviewProjection, AdvisoryProjection)> {
        let snapshot = self.read.read_workspace()?;
        let mirror = ProjectionContext::new(&snapshot)?.mirror()?;
        let overview = mirror.overview().clone();
        Ok((overview, AdvisoryEvaluator::evaluate(&mirror)))
    }

    pub fn repository_with_advisories(
        &self,
        repository_id: &RepositoryId,
    ) -> PortResult<Option<(RepositoryDetailProjection, AdvisoryProjection)>> {
        let snapshot = self.read.read_workspace()?;
        let mirror = ProjectionContext::new(&snapshot)?.mirror()?;
        let repository = mirror
            .repositories()
            .iter()
            .find(|repository| repository.id() == repository_id)
            .cloned();
        let Some(repository) = repository else {
            return Ok(None);
        };
        Ok(Some((repository, AdvisoryEvaluator::evaluate(&mirror))))
    }
}

struct ProjectionContext<'a> {
    revision: WorkspaceRevision,
    repositories: Vec<&'a Repository>,
    locations_by_repository: HashMap<RepositoryId, Vec<&'a Location>>,
    locations_by_id: HashMap<LocationId, &'a Location>,
    worktrees_by_repository: HashMap<RepositoryId, Vec<&'a Worktree>>,
    worktrees_by_id: HashMap<WorktreeId, &'a Worktree>,
    remotes_by_repository: HashMap<RepositoryId, &'a WorkspaceObservationState<Vec<Remote>>>,
    git_state_by_worktree: HashMap<WorktreeId, &'a WorkspaceObservationState<WorktreeGitState>>,
}

impl<'a> ProjectionContext<'a> {
    fn new(snapshot: &'a WorkspaceReadSnapshot) -> PortResult<Self> {
        let repository_ids = snapshot
            .repositories()
            .iter()
            .map(|repository| repository.id().clone())
            .collect::<HashSet<_>>();
        if repository_ids.len() != snapshot.repositories().len() {
            return Err(invariant_violation());
        }

        let mut repositories = snapshot.repositories().iter().collect::<Vec<_>>();
        repositories.sort_by(|left, right| left.id().as_str().cmp(right.id().as_str()));

        let mut locations_by_repository = HashMap::<RepositoryId, Vec<&Location>>::new();
        let mut locations_by_id = HashMap::<LocationId, &Location>::new();
        for location in snapshot.locations() {
            if !repository_ids.contains(location.repository_id())
                || locations_by_id
                    .insert(location.id().clone(), location)
                    .is_some()
            {
                return Err(invariant_violation());
            }
            locations_by_repository
                .entry(location.repository_id().clone())
                .or_default()
                .push(location);
        }
        for locations in locations_by_repository.values_mut() {
            locations.sort_by(|left, right| left.id().as_str().cmp(right.id().as_str()));
        }

        let mut worktrees_by_repository = HashMap::<RepositoryId, Vec<&Worktree>>::new();
        let mut worktrees_by_id = HashMap::<WorktreeId, &Worktree>::new();
        for worktree in snapshot.worktrees() {
            let Some(location) = locations_by_id.get(worktree.location_id()) else {
                return Err(invariant_violation());
            };
            if !repository_ids.contains(worktree.repository_id())
                || location.repository_id() != worktree.repository_id()
                || worktrees_by_id
                    .insert(worktree.id().clone(), worktree)
                    .is_some()
            {
                return Err(invariant_violation());
            }
            worktrees_by_repository
                .entry(worktree.repository_id().clone())
                .or_default()
                .push(worktree);
        }
        for worktrees in worktrees_by_repository.values_mut() {
            worktrees.sort_by(|left, right| left.id().as_str().cmp(right.id().as_str()));
        }

        let mut remotes_by_repository = HashMap::new();
        for state in snapshot.repository_remotes() {
            let repository_id = state
                .latest_attempt()
                .metadata()
                .key()
                .repository_id()
                .ok_or_else(invariant_violation)?;
            if !repository_ids.contains(repository_id)
                || !observation_subject_matches_repository(state, repository_id)
                || remotes_by_repository
                    .insert(repository_id.clone(), state)
                    .is_some()
            {
                return Err(invariant_violation());
            }
        }

        let mut git_state_by_worktree = HashMap::new();
        for state in snapshot.worktree_git_states() {
            let worktree_id = state
                .latest_attempt()
                .metadata()
                .key()
                .worktree_id()
                .ok_or_else(invariant_violation)?;
            if !worktrees_by_id.contains_key(worktree_id)
                || !observation_subject_matches_worktree(state, worktree_id)
                || git_state_by_worktree
                    .insert(worktree_id.clone(), state)
                    .is_some()
            {
                return Err(invariant_violation());
            }
        }

        Ok(Self {
            revision: snapshot.revision(),
            repositories,
            locations_by_repository,
            locations_by_id,
            worktrees_by_repository,
            worktrees_by_id,
            remotes_by_repository,
            git_state_by_worktree,
        })
    }

    fn overview(&self) -> PortResult<OverviewProjection> {
        let summaries = self
            .repositories
            .iter()
            .map(|repository| self.repository_list_item(repository.id()))
            .collect::<PortResult<Vec<_>>>()?;

        let available_repository_count = summaries
            .iter()
            .filter(|item| item.available_location_count() > 0)
            .count();
        let repositories_with_local_work_count = summaries
            .iter()
            .filter(|item| matches!(item.has_local_work(), Knowledge::Known(true)))
            .count();
        let repositories_with_unknown_local_work_count = summaries
            .iter()
            .filter(|item| matches!(item.has_local_work(), Knowledge::Unknown))
            .count();
        let repositories_without_remotes_count = summaries
            .iter()
            .filter(|item| matches!(item.has_remotes(), Knowledge::Known(false)))
            .count();
        let repositories_with_unknown_remotes_count = summaries
            .iter()
            .filter(|item| matches!(item.has_remotes(), Knowledge::Unknown))
            .count();

        Ok(OverviewProjection::new(
            self.revision,
            summaries.len(),
            available_repository_count,
            self.worktrees_by_id.len(),
            repositories_with_local_work_count,
            repositories_with_unknown_local_work_count,
            repositories_without_remotes_count,
            repositories_with_unknown_remotes_count,
        ))
    }

    fn repositories(&self, query: &RepositoryListQuery) -> PortResult<RepositoryListProjection> {
        let mut matches = Vec::new();
        for repository in &self.repositories {
            if self.matches_repository(repository.id(), query)? {
                matches.push(repository.id());
            }
        }

        let total_matching = matches.len();
        let page = query.page();
        let items = matches
            .into_iter()
            .skip(page.offset())
            .take(page.limit())
            .map(|repository_id| self.repository_list_item(repository_id))
            .collect::<PortResult<Vec<_>>>()?;

        Ok(RepositoryListProjection::new(
            self.revision,
            total_matching,
            page.offset(),
            items,
        ))
    }

    fn repository(
        &self,
        repository_id: &RepositoryId,
    ) -> PortResult<Option<RepositoryDetailProjection>> {
        if !self
            .repositories
            .iter()
            .any(|repository| repository.id() == repository_id)
        {
            return Ok(None);
        }

        Ok(Some(self.repository_detail(repository_id)?))
    }

    fn resolve_repository_selector(
        &self,
        selector: &RepositorySelector,
    ) -> PortResult<RepositorySelectorResolution> {
        if let Some(repository_id) = selector.repository_id_candidate()
            && self
                .repositories
                .iter()
                .any(|repository| repository.id() == repository_id)
        {
            return Ok(RepositorySelectorResolution::Resolved(
                repository_id.clone(),
            ));
        }

        let mut candidates = self
            .locations_by_repository
            .iter()
            .filter(|(_, locations)| {
                locations
                    .iter()
                    .any(|location| location.path() == selector.path_candidate())
            })
            .map(|(repository_id, _)| repository_id.clone())
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        candidates.dedup();

        match candidates.as_slice() {
            [] => Ok(RepositorySelectorResolution::NotFound),
            [repository_id] => Ok(RepositorySelectorResolution::Resolved(
                repository_id.clone(),
            )),
            _ => Ok(RepositorySelectorResolution::Ambiguous { candidates }),
        }
    }

    fn worktree(&self, worktree_id: &WorktreeId) -> PortResult<Option<WorktreeProjection>> {
        self.worktrees_by_id
            .get(worktree_id)
            .map(|worktree| self.worktree_projection(worktree))
            .transpose()
    }

    fn mirror(&self) -> PortResult<WorkspaceMirror> {
        let overview = self.overview()?;
        let repositories = self
            .repositories
            .iter()
            .map(|repository| self.repository_detail(repository.id()))
            .collect::<PortResult<Vec<_>>>()?;

        Ok(WorkspaceMirror::new(self.revision, overview, repositories))
    }

    fn repository_detail(
        &self,
        repository_id: &RepositoryId,
    ) -> PortResult<RepositoryDetailProjection> {
        let locations = self
            .locations_by_repository
            .get(repository_id)
            .into_iter()
            .flatten()
            .map(|location| location_projection(location))
            .collect::<Vec<_>>();

        let remotes_state = self.remotes_by_repository.get(repository_id).copied();
        let mut remotes = remotes_state
            .and_then(WorkspaceObservationState::latest_observation)
            .map(|observation| observation.value().clone());
        if let Some(remotes) = remotes.as_mut() {
            remotes.sort_by(|left, right| left.name().cmp(right.name()));
        }

        let worktrees = self
            .worktrees_by_repository
            .get(repository_id)
            .into_iter()
            .flatten()
            .map(|worktree| self.worktree_projection(worktree))
            .collect::<PortResult<Vec<_>>>()?;

        Ok(RepositoryDetailProjection::new(
            self.revision,
            repository_id.clone(),
            locations,
            remotes,
            observation_status(remotes_state),
            worktrees,
        ))
    }

    fn worktree_projection(&self, worktree: &Worktree) -> PortResult<WorktreeProjection> {
        let location = self
            .locations_by_id
            .get(worktree.location_id())
            .ok_or_else(invariant_violation)?;
        let state = self.git_state_by_worktree.get(worktree.id()).copied();
        let git_state = state
            .and_then(WorkspaceObservationState::latest_observation)
            .map(|observation| observation.value().clone());

        Ok(WorktreeProjection::new(
            self.revision,
            worktree.id().clone(),
            worktree.repository_id().clone(),
            location_projection(location),
            git_state,
            observation_status(state),
        ))
    }

    fn repository_list_item(
        &self,
        repository_id: &RepositoryId,
    ) -> PortResult<RepositoryListItemProjection> {
        let locations = self
            .locations_by_repository
            .get(repository_id)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let worktrees = self
            .worktrees_by_repository
            .get(repository_id)
            .map(Vec::as_slice)
            .unwrap_or(&[]);

        Ok(RepositoryListItemProjection::new(
            repository_id.clone(),
            locations
                .iter()
                .map(|location| location_projection(location))
                .collect(),
            worktrees.len(),
            self.has_remotes(repository_id, FreshnessConstraint::any()),
            self.has_local_work(repository_id, FreshnessConstraint::any())?,
        ))
    }

    fn matches_repository(
        &self,
        repository_id: &RepositoryId,
        query: &RepositoryListQuery,
    ) -> PortResult<bool> {
        for predicate in query.predicates() {
            let matches = match *predicate {
                RepositoryPredicate::LocationAvailability(availability) => self
                    .locations_by_repository
                    .get(repository_id)
                    .into_iter()
                    .flatten()
                    .any(|location| location.availability() == availability),
                RepositoryPredicate::HasRemotes(expected) => {
                    matches_known_bool(self.has_remotes(repository_id, query.freshness()), expected)
                }
                RepositoryPredicate::HasLocalWork(expected) => matches_known_bool(
                    self.has_local_work(repository_id, query.freshness())?,
                    expected,
                ),
            };
            if !matches {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn has_remotes(
        &self,
        repository_id: &RepositoryId,
        freshness: FreshnessConstraint,
    ) -> Knowledge<bool> {
        let Some(observation) = self
            .remotes_by_repository
            .get(repository_id)
            .and_then(|state| state.latest_observation())
        else {
            return Knowledge::Unknown;
        };
        if !freshness.accepts(observation.freshness().observed_at()) {
            return Knowledge::Unknown;
        }
        Knowledge::Known(!observation.value().is_empty())
    }

    fn has_local_work(
        &self,
        repository_id: &RepositoryId,
        freshness: FreshnessConstraint,
    ) -> PortResult<Knowledge<bool>> {
        let worktrees = self
            .worktrees_by_repository
            .get(repository_id)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        if worktrees.is_empty() {
            return Ok(Knowledge::Known(false));
        }

        let mut unknown = false;
        for worktree in worktrees {
            let Some(observation) = self
                .git_state_by_worktree
                .get(worktree.id())
                .and_then(|state| state.latest_observation())
            else {
                unknown = true;
                continue;
            };
            if !freshness.accepts(observation.freshness().observed_at()) {
                unknown = true;
                continue;
            }
            if observation.value().changes().has_local_work() {
                return Ok(Knowledge::Known(true));
            }
        }

        if unknown {
            Ok(Knowledge::Unknown)
        } else {
            Ok(Knowledge::Known(false))
        }
    }
}

fn matches_known_bool(value: Knowledge<bool>, expected: bool) -> bool {
    matches!(value, Knowledge::Known(actual) if actual == expected)
}

fn normalize_lexical_path(path: &Path) -> PathBuf {
    path.components().collect()
}

fn location_projection(location: &Location) -> LocationProjection {
    LocationProjection::new(
        location.id().clone(),
        location.path().to_path_buf(),
        location.availability(),
    )
}

fn observation_status<T>(
    state: Option<&WorkspaceObservationState<T>>,
) -> ObservationStatusProjection {
    let Some(state) = state else {
        return ObservationStatusProjection::new(None, None);
    };

    let latest_attempt = state.latest_attempt();
    let metadata = latest_attempt.metadata();
    let outcome = match latest_attempt {
        ObservationAttempt::Succeeded(_) => ObservationAttemptOutcomeProjection::Succeeded,
        ObservationAttempt::Failed(failure) => {
            ObservationAttemptOutcomeProjection::Failed(failure.kind())
        }
    };
    let latest_attempt = ObservationAttemptProjection::new(
        metadata.freshness().observed_at(),
        metadata.coverage(),
        outcome,
    );
    let latest_success = state.latest_observation().map(|observation| {
        ObservationSuccessProjection::new(
            observation.freshness().observed_at(),
            observation.metadata().coverage(),
        )
    });

    ObservationStatusProjection::new(Some(latest_attempt), latest_success)
}

fn observation_subject_matches_repository<T>(
    state: &WorkspaceObservationState<T>,
    repository_id: &RepositoryId,
) -> bool {
    state.latest_observation().is_none_or(|observation| {
        observation.metadata().key().repository_id() == Some(repository_id)
    })
}

fn observation_subject_matches_worktree<T>(
    state: &WorkspaceObservationState<T>,
    worktree_id: &WorktreeId,
) -> bool {
    state
        .latest_observation()
        .is_none_or(|observation| observation.metadata().key().worktree_id() == Some(worktree_id))
}

const fn invariant_violation() -> PortError {
    PortError::new(PortErrorKind::InvariantViolation)
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::time::{Duration, UNIX_EPOCH};

    use bulls_core::{
        Branch, Freshness, Head, Observation, ObservationCoverage, ObservationFailure,
        ObservationFailureKind, ObservationKey, ObservationMetadata, ObservationRunId,
        UpstreamState, WorktreeChanges,
    };

    use super::{
        FreshnessConstraint, QueryPage, RepositoryListQuery, RepositoryPredicate,
        RepositorySelector, RepositorySelectorResolution, WorkspaceQueryService,
    };
    use crate::ports::{
        PortResult, WorkspaceObservationState, WorkspaceReadPort, WorkspaceReadSnapshot,
    };
    use crate::{AdvisoryCode, Knowledge, ObservationAttemptOutcomeProjection, WorkspaceRevision};
    use bulls_core::{
        Location, LocationAvailability, LocationId, ObservationAttempt, Remote, Repository,
        RepositoryId, Worktree, WorktreeGitState, WorktreeId,
    };

    struct StubWorkspaceReadPort {
        snapshot: WorkspaceReadSnapshot,
        reads: Cell<usize>,
    }

    impl StubWorkspaceReadPort {
        fn new(snapshot: WorkspaceReadSnapshot) -> Self {
            Self {
                snapshot,
                reads: Cell::new(0),
            }
        }
    }

    impl WorkspaceReadPort for StubWorkspaceReadPort {
        fn read_workspace(&self) -> PortResult<WorkspaceReadSnapshot> {
            self.reads.set(self.reads.get() + 1);
            Ok(self.snapshot.clone())
        }
    }

    fn metadata(key: ObservationKey, seconds: u64) -> ObservationMetadata {
        ObservationMetadata::new(
            ObservationRunId::from(format!("run-{seconds}")),
            key,
            Freshness::new(UNIX_EPOCH + Duration::from_secs(seconds)),
            ObservationCoverage::Complete,
        )
    }

    fn successful_state<T: Clone>(
        key: ObservationKey,
        seconds: u64,
        value: T,
    ) -> WorkspaceObservationState<T> {
        let observation = Observation::new(metadata(key, seconds), value);
        WorkspaceObservationState::new(
            ObservationAttempt::Succeeded(observation.clone()),
            Some(observation),
        )
    }

    fn workspace_snapshot() -> WorkspaceReadSnapshot {
        let repository_a = RepositoryId::from("repository-a");
        let repository_b = RepositoryId::from("repository-b");
        let worktree_a = WorktreeId::from("worktree-a");
        let worktree_b = WorktreeId::from("worktree-b");

        WorkspaceReadSnapshot::new(
            WorkspaceRevision::new(7),
            vec![
                Repository::new(repository_b.clone()),
                Repository::new(repository_a.clone()),
            ],
            vec![
                Location::new(
                    LocationId::from("location-b"),
                    repository_b.clone(),
                    "/workspace/b",
                    LocationAvailability::Missing,
                ),
                Location::new(
                    LocationId::from("location-a"),
                    repository_a.clone(),
                    "/workspace/a",
                    LocationAvailability::Available,
                ),
            ],
            vec![
                Worktree::new(
                    worktree_b.clone(),
                    repository_b.clone(),
                    LocationId::from("location-b"),
                ),
                Worktree::new(
                    worktree_a.clone(),
                    repository_a.clone(),
                    LocationId::from("location-a"),
                ),
            ],
            vec![successful_state(
                ObservationKey::repository_remotes(repository_a),
                10,
                Vec::<Remote>::new(),
            )],
            vec![successful_state(
                ObservationKey::worktree_git_state(worktree_a),
                10,
                WorktreeGitState::branch(Branch::new("main"), UpstreamState::Unconfigured)
                    .with_changes(WorktreeChanges::new(false, true, false)),
            )],
        )
    }

    #[test]
    fn overview_preserves_unknown_state_instead_of_treating_it_as_false() {
        let read = StubWorkspaceReadPort::new(workspace_snapshot());
        let overview = WorkspaceQueryService::new(&read)
            .overview()
            .expect("overview query must succeed");

        assert_eq!(overview.revision(), WorkspaceRevision::new(7));
        assert_eq!(overview.repository_count(), 2);
        assert_eq!(overview.available_repository_count(), 1);
        assert_eq!(overview.worktree_count(), 2);
        assert_eq!(overview.repositories_with_local_work_count(), 1);
        assert_eq!(overview.repositories_with_unknown_local_work_count(), 1);
        assert_eq!(overview.repositories_without_remotes_count(), 1);
        assert_eq!(overview.repositories_with_unknown_remotes_count(), 1);
        assert_eq!(read.reads.get(), 1);
    }

    #[test]
    fn repository_query_composes_predicates_freshness_ordering_and_pagination() {
        let read = StubWorkspaceReadPort::new(workspace_snapshot());
        let query = RepositoryListQuery::new(QueryPage::new(0, 1).expect("page must be valid"))
            .with_predicate(RepositoryPredicate::LocationAvailability(
                LocationAvailability::Available,
            ))
            .with_predicate(RepositoryPredicate::HasLocalWork(true))
            .with_freshness(FreshnessConstraint::observed_at_or_after(
                UNIX_EPOCH + Duration::from_secs(5),
            ));
        let projection = WorkspaceQueryService::new(&read)
            .repositories(&query)
            .expect("repository query must succeed");

        assert_eq!(projection.total_matching(), 1);
        assert_eq!(projection.items().len(), 1);
        assert_eq!(projection.items()[0].id().as_str(), "repository-a");
        assert_eq!(projection.items()[0].locations().len(), 1);
        assert_eq!(
            projection.items()[0].locations()[0].path(),
            std::path::Path::new("/workspace/a")
        );
        assert_eq!(
            projection.items()[0].has_local_work(),
            &Knowledge::Known(true)
        );

        let stale_query = RepositoryListQuery::default()
            .with_predicate(RepositoryPredicate::HasLocalWork(true))
            .with_freshness(FreshnessConstraint::observed_at_or_after(
                UNIX_EPOCH + Duration::from_secs(11),
            ));
        let stale_projection = WorkspaceQueryService::new(&read)
            .repositories(&stale_query)
            .expect("stale repository query must succeed");

        assert_eq!(stale_projection.total_matching(), 0);
    }

    #[test]
    fn detail_keeps_last_success_when_the_latest_attempt_failed() {
        let repository_id = RepositoryId::from("repository-a");
        let successful = Observation::new(
            metadata(
                ObservationKey::repository_remotes(repository_id.clone()),
                10,
            ),
            vec![Remote::new("origin")],
        );
        let failed_metadata = metadata(
            ObservationKey::repository_remotes(repository_id.clone()),
            20,
        );
        let remotes_state = WorkspaceObservationState::new(
            ObservationAttempt::Failed(ObservationFailure::new(
                failed_metadata,
                ObservationFailureKind::TimedOut,
            )),
            Some(successful),
        );
        let snapshot = WorkspaceReadSnapshot::new(
            WorkspaceRevision::new(8),
            vec![Repository::new(repository_id.clone())],
            Vec::new(),
            Vec::new(),
            vec![remotes_state],
            Vec::new(),
        );
        let read = StubWorkspaceReadPort::new(snapshot);
        let detail = WorkspaceQueryService::new(&read)
            .repository(&repository_id)
            .expect("repository query must succeed")
            .expect("repository must exist");

        assert_eq!(detail.remotes().expect("known remotes")[0].name(), "origin");
        assert!(matches!(
            detail
                .remotes_observation()
                .latest_attempt()
                .expect("attempt must exist")
                .outcome(),
            ObservationAttemptOutcomeProjection::Failed(ObservationFailureKind::TimedOut)
        ));
        assert_eq!(
            detail
                .remotes_observation()
                .latest_success()
                .expect("success must be retained")
                .observed_at(),
            UNIX_EPOCH + Duration::from_secs(10)
        );
    }

    #[test]
    fn mirror_is_a_single_revision_consistent_projection_and_not_a_second_read_path() {
        let read = StubWorkspaceReadPort::new(workspace_snapshot());
        let mirror = WorkspaceQueryService::new(&read)
            .mirror()
            .expect("mirror query must succeed");

        assert_eq!(mirror.revision(), WorkspaceRevision::new(7));
        assert_eq!(mirror.overview().revision(), mirror.revision());
        assert_eq!(mirror.repositories().len(), 2);
        assert_eq!(mirror.repositories()[0].id().as_str(), "repository-a");
        assert_eq!(mirror.repositories()[1].id().as_str(), "repository-b");
        assert!(
            mirror
                .repositories()
                .iter()
                .flat_map(|repository| repository.worktrees())
                .all(|worktree| worktree.revision() == mirror.revision())
        );
        assert_eq!(read.reads.get(), 1);
    }

    #[test]
    fn advisory_query_derives_from_one_workspace_read_without_a_refresh_path() {
        let read = StubWorkspaceReadPort::new(workspace_snapshot());
        let projection = WorkspaceQueryService::new(&read)
            .advisories()
            .expect("advisory query must succeed");

        assert!(
            projection
                .advisories()
                .iter()
                .any(|advisory| advisory.code() == AdvisoryCode::RepositoryWithoutRemotes)
        );
        assert!(
            projection
                .advisories()
                .iter()
                .any(|advisory| advisory.code() == AdvisoryCode::WorktreeWithoutUpstream)
        );
        assert_eq!(read.reads.get(), 1);
    }

    #[test]
    fn combined_human_views_share_one_workspace_snapshot_and_revision() {
        let read = StubWorkspaceReadPort::new(workspace_snapshot());
        let service = WorkspaceQueryService::new(&read);

        let (overview, overview_advisories) = service
            .overview_with_advisories()
            .expect("combined overview query must succeed");
        assert_eq!(overview.revision(), overview_advisories.revision());
        assert_eq!(read.reads.get(), 1);

        let repository_id = RepositoryId::from("repository-a");
        let (repository, repository_advisories) = service
            .repository_with_advisories(&repository_id)
            .expect("combined repository query must succeed")
            .expect("repository must exist");
        assert_eq!(repository.revision(), repository_advisories.revision());
        assert_eq!(read.reads.get(), 2);
    }

    #[test]
    fn worktree_projection_exposes_known_git_state_and_location_without_refresh() {
        let read = StubWorkspaceReadPort::new(workspace_snapshot());
        let worktree_id = WorktreeId::from("worktree-a");
        let worktree = WorkspaceQueryService::new(&read)
            .worktree(&worktree_id)
            .expect("worktree query must succeed")
            .expect("worktree must exist");

        assert_eq!(
            worktree.location().path(),
            std::path::Path::new("/workspace/a")
        );
        assert!(matches!(
            worktree.git_state().expect("state must be known").head(),
            Head::Branch(branch) if branch.name() == "main"
        ));
        assert_eq!(read.reads.get(), 1);
    }

    #[test]
    fn repository_selector_resolves_canonical_id_before_path_interpretation() {
        let snapshot = WorkspaceReadSnapshot::new(
            WorkspaceRevision::new(8),
            vec![
                Repository::new(RepositoryId::from("repository-b")),
                Repository::new(RepositoryId::from("repository-a")),
            ],
            vec![Location::new(
                LocationId::from("location-collision"),
                RepositoryId::from("repository-b"),
                "/workspace/repository-a",
                LocationAvailability::Available,
            )],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        let read = StubWorkspaceReadPort::new(snapshot);
        let selector =
            RepositorySelector::from_input("repository-a", std::path::Path::new("/workspace"));

        let resolution = WorkspaceQueryService::new(&read)
            .resolve_repository_selector(&selector)
            .expect("selector resolution must succeed");

        assert_eq!(
            resolution,
            RepositorySelectorResolution::Resolved(RepositoryId::from("repository-a"))
        );
        assert_eq!(read.reads.get(), 1);
    }

    #[test]
    fn repository_selector_resolves_absolute_and_relative_location_paths() {
        let read = StubWorkspaceReadPort::new(workspace_snapshot());
        let service = WorkspaceQueryService::new(&read);

        let absolute =
            RepositorySelector::from_input("/workspace/a", std::path::Path::new("/unrelated"));
        let relative = RepositorySelector::from_input(".", std::path::Path::new("/workspace/a"));

        assert_eq!(
            service
                .resolve_repository_selector(&absolute)
                .expect("absolute path selector must resolve"),
            RepositorySelectorResolution::Resolved(RepositoryId::from("repository-a"))
        );
        assert_eq!(
            service
                .resolve_repository_selector(&relative)
                .expect("relative path selector must resolve"),
            RepositorySelectorResolution::Resolved(RepositoryId::from("repository-a"))
        );
    }

    #[test]
    fn repository_selector_preserves_ambiguous_paths_as_explicit_candidates() {
        let snapshot = WorkspaceReadSnapshot::new(
            WorkspaceRevision::new(9),
            vec![
                Repository::new(RepositoryId::from("repository-b")),
                Repository::new(RepositoryId::from("repository-a")),
            ],
            vec![
                Location::new(
                    LocationId::from("location-b"),
                    RepositoryId::from("repository-b"),
                    "/workspace/shared",
                    LocationAvailability::Missing,
                ),
                Location::new(
                    LocationId::from("location-a"),
                    RepositoryId::from("repository-a"),
                    "/workspace/shared",
                    LocationAvailability::Available,
                ),
            ],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        let read = StubWorkspaceReadPort::new(snapshot);
        let selector =
            RepositorySelector::from_input("/workspace/shared", std::path::Path::new("/workspace"));

        let resolution = WorkspaceQueryService::new(&read)
            .resolve_repository_selector(&selector)
            .expect("ambiguous selector must remain a successful query result");

        assert_eq!(
            resolution,
            RepositorySelectorResolution::Ambiguous {
                candidates: vec![
                    RepositoryId::from("repository-a"),
                    RepositoryId::from("repository-b"),
                ],
            }
        );
        assert_eq!(
            resolution
                .candidates()
                .iter()
                .map(RepositoryId::as_str)
                .collect::<Vec<_>>(),
            vec!["repository-a", "repository-b"]
        );
    }

    #[test]
    fn repository_selector_reports_not_found_without_fuzzy_matching() {
        let read = StubWorkspaceReadPort::new(workspace_snapshot());
        let selector =
            RepositorySelector::from_input("repository", std::path::Path::new("/workspace"));

        let resolution = WorkspaceQueryService::new(&read)
            .resolve_repository_selector(&selector)
            .expect("missing selector must remain a successful query result");

        assert_eq!(resolution, RepositorySelectorResolution::NotFound);
    }

    #[cfg(unix)]
    #[test]
    fn repository_selector_preserves_non_utf8_paths_without_lossy_identity() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let path = std::path::PathBuf::from(OsString::from_vec(vec![
            b'/', b'w', b'o', b'r', b'k', b's', b'p', b'a', b'c', b'e', b'/', 0xff,
        ]));
        let snapshot = WorkspaceReadSnapshot::new(
            WorkspaceRevision::new(10),
            vec![Repository::new(RepositoryId::from("repository-non-utf8"))],
            vec![Location::new(
                LocationId::from("location-non-utf8"),
                RepositoryId::from("repository-non-utf8"),
                path.clone(),
                LocationAvailability::Available,
            )],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        let read = StubWorkspaceReadPort::new(snapshot);
        let selector =
            RepositorySelector::from_input(path.into_os_string(), std::path::Path::new("/"));

        let resolution = WorkspaceQueryService::new(&read)
            .resolve_repository_selector(&selector)
            .expect("non-UTF-8 path selector must resolve without lossy conversion");

        assert_eq!(
            resolution,
            RepositorySelectorResolution::Resolved(RepositoryId::from("repository-non-utf8"))
        );
    }
}
