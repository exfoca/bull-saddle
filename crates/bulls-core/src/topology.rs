// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};

use crate::{LocationId, RepositoryId, WorktreeId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Repository {
    id: RepositoryId,
}

impl Repository {
    pub fn new(id: RepositoryId) -> Self {
        Self { id }
    }

    pub fn id(&self) -> &RepositoryId {
        &self.id
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum LocationAvailability {
    Available,
    Missing,
    Offline,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Location {
    id: LocationId,
    repository_id: RepositoryId,
    path: PathBuf,
    availability: LocationAvailability,
}

impl Location {
    pub fn new(
        id: LocationId,
        repository_id: RepositoryId,
        path: impl Into<PathBuf>,
        availability: LocationAvailability,
    ) -> Self {
        Self {
            id,
            repository_id,
            path: path.into(),
            availability,
        }
    }

    pub fn id(&self) -> &LocationId {
        &self.id
    }

    pub fn repository_id(&self) -> &RepositoryId {
        &self.repository_id
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn availability(&self) -> LocationAvailability {
        self.availability
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Worktree {
    id: WorktreeId,
    repository_id: RepositoryId,
    location_id: LocationId,
}

impl Worktree {
    pub fn new(id: WorktreeId, repository_id: RepositoryId, location_id: LocationId) -> Self {
        Self {
            id,
            repository_id,
            location_id,
        }
    }

    pub fn id(&self) -> &WorktreeId {
        &self.id
    }

    pub fn repository_id(&self) -> &RepositoryId {
        &self.repository_id
    }

    pub fn location_id(&self) -> &LocationId {
        &self.location_id
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{Location, LocationAvailability, Repository, Worktree};
    use crate::{LocationId, RepositoryId, WorktreeId};

    #[test]
    fn repository_identity_is_independent_from_location_path_and_availability() {
        let repository = Repository::new(RepositoryId::from("repository-1"));
        let location = Location::new(
            LocationId::from("location-1"),
            repository.id().clone(),
            "/workspace/repository",
            LocationAvailability::Missing,
        );

        assert_eq!(location.repository_id(), repository.id());
        assert_eq!(location.path(), Path::new("/workspace/repository"));
        assert_eq!(location.availability(), LocationAvailability::Missing);
    }

    #[test]
    fn distinct_location_ids_can_preserve_distinct_paths_for_one_repository() {
        let repository = Repository::new(RepositoryId::from("repository-1"));
        let first = Location::new(
            LocationId::from("location-1"),
            repository.id().clone(),
            "/workspace/main",
            LocationAvailability::Available,
        );
        let second = Location::new(
            LocationId::from("location-2"),
            repository.id().clone(),
            "/workspace/linked",
            LocationAvailability::Offline,
        );

        assert_eq!(first.repository_id(), second.repository_id());
        assert_ne!(first.id(), second.id());
        assert_ne!(first.path(), second.path());
    }

    #[test]
    fn repository_can_have_multiple_distinct_worktrees() {
        let repository = Repository::new(RepositoryId::from("repository-1"));
        let first_worktree = Worktree::new(
            WorktreeId::from("worktree-1"),
            repository.id().clone(),
            LocationId::from("location-1"),
        );
        let second_worktree = Worktree::new(
            WorktreeId::from("worktree-2"),
            repository.id().clone(),
            LocationId::from("location-2"),
        );

        assert_eq!(first_worktree.repository_id(), repository.id());
        assert_eq!(second_worktree.repository_id(), repository.id());
        assert_ne!(first_worktree.id(), second_worktree.id());
        assert_ne!(first_worktree.location_id(), second_worktree.location_id());
    }

    #[test]
    fn repository_can_exist_without_a_worktree() {
        let repository = Repository::new(RepositoryId::from("bare-repository"));
        let location = Location::new(
            LocationId::from("bare-location"),
            repository.id().clone(),
            "/srv/git/repository.git",
            LocationAvailability::Available,
        );

        assert_eq!(location.repository_id(), repository.id());
        assert_eq!(location.path(), Path::new("/srv/git/repository.git"));
    }
}
