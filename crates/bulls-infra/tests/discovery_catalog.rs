// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use bulls_application::DiscoverRepositories;
use bulls_application::ports::{DiscoveryRequest, RepositoryCatalogPort};
use bulls_infra::{
    GitCliRepositoryProbe, NativeFilesystemIdentity, NativeIdentityGenerator,
    NativeRepositoryDiscovery, SqliteRepositoryCatalog,
};

static TEST_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let sequence = TEST_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "bulls-discovery-catalog-test-{}-{}",
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
fn discovery_identity_and_catalog_survive_restart() {
    let root = TestDirectory::new();
    let workspace = root.path().join("workspace");
    let repository = workspace.join("repository");
    let data_dir = root.path().join("data");
    fs::create_dir_all(&repository).expect("repository directory must be created");

    let status = Command::new("git")
        .arg("-C")
        .arg(&repository)
        .args(["init", "-q"])
        .status()
        .expect("Git fixture command must start");
    assert!(status.success(), "Git fixture command must succeed");

    let discovery = NativeRepositoryDiscovery::new();
    let git = GitCliRepositoryProbe::new();
    let filesystem = NativeFilesystemIdentity::new();
    let request = DiscoveryRequest::new(&workspace);

    let (repository_id, location_id) = {
        let mut catalog =
            SqliteRepositoryCatalog::open(&data_dir).expect("catalog storage must open");
        let mut identities = NativeIdentityGenerator::new();
        DiscoverRepositories::new(&discovery, &git, &filesystem, &mut catalog, &mut identities)
            .execute(&request)
            .expect("initial discovery must succeed");

        let repository = catalog
            .repositories()
            .expect("repository must persist")
            .remove(0);
        let location = catalog
            .locations_for_repository(repository.id())
            .expect("location must persist")
            .remove(0);
        (repository.id().clone(), location.id().clone())
    };

    #[cfg(unix)]
    let expected_repository_path = {
        let moved = workspace.join("repository-moved");
        fs::rename(&repository, &moved).expect("repository rename must succeed");
        moved
    };
    #[cfg(not(unix))]
    let expected_repository_path = repository.clone();

    let mut catalog =
        SqliteRepositoryCatalog::open(&data_dir).expect("catalog storage must reopen");
    let mut identities = NativeIdentityGenerator::new();
    DiscoverRepositories::new(&discovery, &git, &filesystem, &mut catalog, &mut identities)
        .execute(&request)
        .expect("repeated discovery must succeed");

    let repositories = catalog.repositories().expect("repository must reload");
    assert_eq!(repositories.len(), 1);
    assert_eq!(repositories[0].id(), &repository_id);
    let locations = catalog
        .locations_for_repository(&repository_id)
        .expect("location must reload");
    assert_eq!(locations.len(), 1);
    assert_eq!(locations[0].id(), &location_id);
    assert_eq!(locations[0].path(), expected_repository_path);
}
