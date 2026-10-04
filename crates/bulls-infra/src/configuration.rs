// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use bulls_application::ports::{
    BareRepositoryDiscovery, ConfigurationPort, FilesystemBoundary, PlatformPaths, PortResult,
    SymlinkTraversal,
};
use bulls_application::{
    BullSaddleConfiguration, DiscoveryConfiguration, GitProcessTimeout, LocalePreference,
    ObservationExecutionPolicy,
};
use serde::{Deserialize, Serialize};

use crate::error::{invalid_data, invariant_violation, io_port_error};

const CONFIGURATION_FILE_NAME: &str = "config.toml";
const CONFIGURATION_SCHEMA_VERSION: u16 = 2;
const AUTOMATIC_LOCALE: &str = "auto";
static TEMPORARY_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TomlConfigurationStore {
    path: PathBuf,
}

impl TomlConfigurationStore {
    pub fn new(config_dir: impl Into<PathBuf>) -> PortResult<Self> {
        let config_dir = config_dir.into();
        if !config_dir.is_absolute() {
            return Err(invariant_violation());
        }

        Ok(Self {
            path: config_dir.join(CONFIGURATION_FILE_NAME),
        })
    }

    pub fn from_platform_paths(paths: &PlatformPaths) -> PortResult<Self> {
        Self::new(paths.config_dir())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl ConfigurationPort for TomlConfigurationStore {
    type Configuration = BullSaddleConfiguration;

    fn load(&self) -> PortResult<Option<Self::Configuration>> {
        let contents = match fs::read_to_string(&self.path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(io_port_error(&error)),
        };

        let document: ConfigurationDocument =
            toml::from_str(&contents).map_err(|_| invalid_data())?;

        if document.schema_version != CONFIGURATION_SCHEMA_VERSION {
            return Err(invalid_data());
        }

        configuration_from_document(document).map(Some)
    }

    fn save(&mut self, configuration: &Self::Configuration) -> PortResult<()> {
        let parent = self.path.parent().ok_or_else(invariant_violation)?;
        fs::create_dir_all(parent).map_err(|error| io_port_error(&error))?;

        let document = configuration_document(configuration)?;
        let contents = toml::to_string_pretty(&document).map_err(|_| invalid_data())?;
        let temporary_path = temporary_path(&self.path)?;

        let write_result = write_synchronized(&temporary_path, contents.as_bytes());
        if let Err(error) = write_result {
            let _ = fs::remove_file(&temporary_path);
            return Err(error);
        }

        if let Err(error) = fs::rename(&temporary_path, &self.path) {
            let _ = fs::remove_file(&temporary_path);
            return Err(io_port_error(&error));
        }

        Ok(())
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ConfigurationDocument {
    schema_version: u16,
    locale: String,
    discovery: DiscoveryDocument,
    observation: ObservationDocument,
    git: GitDocument,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct DiscoveryDocument {
    roots: Vec<String>,
    exclusions: Vec<String>,
    bare_repositories: BareRepositoryDiscoveryDocument,
    symlink_traversal: SymlinkTraversalDocument,
    filesystem_boundary: FilesystemBoundaryDocument,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ObservationDocument {
    max_parallelism: u64,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct GitDocument {
    process_timeout_ms: u64,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum BareRepositoryDiscoveryDocument {
    Disabled,
    Enabled,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum SymlinkTraversalDocument {
    DoNotFollow,
    FollowWithinRoot,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum FilesystemBoundaryDocument {
    StayOnRootFilesystem,
    CrossFilesystems,
}

fn configuration_from_document(
    document: ConfigurationDocument,
) -> PortResult<BullSaddleConfiguration> {
    let locale = if document.locale == AUTOMATIC_LOCALE {
        LocalePreference::auto()
    } else {
        LocalePreference::explicit(document.locale).ok_or_else(invalid_data)?
    };
    let roots = document
        .discovery
        .roots
        .into_iter()
        .map(PathBuf::from)
        .collect();
    let exclusions = document
        .discovery
        .exclusions
        .into_iter()
        .map(PathBuf::from)
        .collect();
    let discovery = DiscoveryConfiguration::new(
        roots,
        exclusions,
        document.discovery.bare_repositories.into(),
        document.discovery.symlink_traversal.into(),
        document.discovery.filesystem_boundary.into(),
    )
    .ok_or_else(invalid_data)?;
    let max_parallelism = usize::try_from(document.observation.max_parallelism)
        .ok()
        .and_then(ObservationExecutionPolicy::new)
        .ok_or_else(invalid_data)?;
    let process_timeout =
        GitProcessTimeout::new(document.git.process_timeout_ms).ok_or_else(invalid_data)?;

    Ok(BullSaddleConfiguration::new(locale)
        .with_discovery(discovery)
        .with_observation_policy(max_parallelism)
        .with_git_process_timeout(process_timeout))
}

fn configuration_document(
    configuration: &BullSaddleConfiguration,
) -> PortResult<ConfigurationDocument> {
    let locale = configuration
        .locale()
        .explicit_value()
        .unwrap_or(AUTOMATIC_LOCALE)
        .to_owned();
    let roots = configuration
        .discovery()
        .roots()
        .iter()
        .map(|path| path_text(path))
        .collect::<PortResult<Vec<_>>>()?;
    let exclusions = configuration
        .discovery()
        .exclusions()
        .iter()
        .map(|path| path_text(path))
        .collect::<PortResult<Vec<_>>>()?;
    let max_parallelism = u64::try_from(configuration.observation_policy().max_parallelism())
        .map_err(|_| invalid_data())?;

    Ok(ConfigurationDocument {
        schema_version: CONFIGURATION_SCHEMA_VERSION,
        locale,
        discovery: DiscoveryDocument {
            roots,
            exclusions,
            bare_repositories: configuration.discovery().bare_repositories().into(),
            symlink_traversal: configuration.discovery().symlink_traversal().into(),
            filesystem_boundary: configuration.discovery().filesystem_boundary().into(),
        },
        observation: ObservationDocument { max_parallelism },
        git: GitDocument {
            process_timeout_ms: configuration.git_process_timeout().milliseconds(),
        },
    })
}

fn path_text(path: &Path) -> PortResult<String> {
    path.to_str()
        .map(ToOwned::to_owned)
        .ok_or_else(invalid_data)
}

impl From<BareRepositoryDiscovery> for BareRepositoryDiscoveryDocument {
    fn from(value: BareRepositoryDiscovery) -> Self {
        match value {
            BareRepositoryDiscovery::Disabled => Self::Disabled,
            BareRepositoryDiscovery::Enabled => Self::Enabled,
        }
    }
}

impl From<BareRepositoryDiscoveryDocument> for BareRepositoryDiscovery {
    fn from(value: BareRepositoryDiscoveryDocument) -> Self {
        match value {
            BareRepositoryDiscoveryDocument::Disabled => Self::Disabled,
            BareRepositoryDiscoveryDocument::Enabled => Self::Enabled,
        }
    }
}

impl From<SymlinkTraversal> for SymlinkTraversalDocument {
    fn from(value: SymlinkTraversal) -> Self {
        match value {
            SymlinkTraversal::DoNotFollow => Self::DoNotFollow,
            SymlinkTraversal::FollowWithinRoot => Self::FollowWithinRoot,
        }
    }
}

impl From<SymlinkTraversalDocument> for SymlinkTraversal {
    fn from(value: SymlinkTraversalDocument) -> Self {
        match value {
            SymlinkTraversalDocument::DoNotFollow => Self::DoNotFollow,
            SymlinkTraversalDocument::FollowWithinRoot => Self::FollowWithinRoot,
        }
    }
}

impl From<FilesystemBoundary> for FilesystemBoundaryDocument {
    fn from(value: FilesystemBoundary) -> Self {
        match value {
            FilesystemBoundary::StayOnRootFilesystem => Self::StayOnRootFilesystem,
            FilesystemBoundary::CrossFilesystems => Self::CrossFilesystems,
        }
    }
}

impl From<FilesystemBoundaryDocument> for FilesystemBoundary {
    fn from(value: FilesystemBoundaryDocument) -> Self {
        match value {
            FilesystemBoundaryDocument::StayOnRootFilesystem => Self::StayOnRootFilesystem,
            FilesystemBoundaryDocument::CrossFilesystems => Self::CrossFilesystems,
        }
    }
}

fn temporary_path(path: &Path) -> PortResult<PathBuf> {
    let parent = path.parent().ok_or_else(invariant_violation)?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(invariant_violation)?;
    let sequence = TEMPORARY_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);

    Ok(parent.join(format!(
        ".{file_name}.{}.{}.tmp",
        std::process::id(),
        sequence
    )))
}

fn write_synchronized(path: &Path, contents: &[u8]) -> PortResult<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| io_port_error(&error))?;

    file.write_all(contents)
        .map_err(|error| io_port_error(&error))?;
    file.sync_all().map_err(|error| io_port_error(&error))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    use bulls_application::ports::{
        BareRepositoryDiscovery, ConfigurationPort, FilesystemBoundary, PortErrorKind,
        SymlinkTraversal,
    };
    use bulls_application::{
        BullSaddleConfiguration, DiscoveryConfiguration, GitProcessTimeout, LocalePreference,
        ObservationExecutionPolicy,
    };

    use super::TomlConfigurationStore;

    static TEST_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let sequence = TEST_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "bulls-configuration-test-{}-{}",
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
    fn missing_configuration_does_not_create_configuration_directory() {
        let root = TestDirectory::new();
        let config_dir = root.path().join("not-created-yet");
        let store = TomlConfigurationStore::new(&config_dir)
            .expect("absolute configuration path must be accepted");

        assert_eq!(
            store.load().expect("missing configuration must be valid"),
            None
        );
        assert!(!config_dir.exists());
    }

    #[test]
    fn configuration_round_trip_preserves_runtime_policies() {
        let root = TestDirectory::new();
        let mut store = TomlConfigurationStore::new(root.path().join("config"))
            .expect("absolute configuration path must be accepted");
        let discovery = DiscoveryConfiguration::new(
            vec![root.path().join("workspace")],
            vec![PathBuf::from("vendor")],
            BareRepositoryDiscovery::Enabled,
            SymlinkTraversal::FollowWithinRoot,
            FilesystemBoundary::CrossFilesystems,
        )
        .expect("valid discovery configuration must be accepted");
        let expected = BullSaddleConfiguration::new(
            LocalePreference::explicit("pt-BR")
                .expect("well-formed explicit locale must be accepted"),
        )
        .with_discovery(discovery)
        .with_observation_policy(
            ObservationExecutionPolicy::new(8)
                .expect("positive observation parallelism must be accepted"),
        )
        .with_git_process_timeout(
            GitProcessTimeout::new(15_000).expect("positive process timeout must be accepted"),
        );

        store
            .save(&BullSaddleConfiguration::default())
            .expect("initial save must succeed");
        store
            .save(&expected)
            .expect("replacement save must succeed");

        assert_eq!(
            store.load().expect("configuration load must succeed"),
            Some(expected)
        );
    }

    #[test]
    fn invalid_schema_unknown_fields_or_invalid_policy_values_are_rejected() {
        let root = TestDirectory::new();
        let config_dir = root.path().join("config");
        let mut store = TomlConfigurationStore::new(&config_dir)
            .expect("absolute configuration path must be accepted");
        store
            .save(&BullSaddleConfiguration::default())
            .expect("valid fixture must save");
        let valid = fs::read_to_string(store.path()).expect("valid fixture must be readable");

        fs::write(
            store.path(),
            valid.replacen("schema_version = 2", "schema_version = 1", 1),
        )
        .expect("fixture must be written");
        assert_eq!(
            store
                .load()
                .expect_err("unsupported schema must fail")
                .kind(),
            PortErrorKind::InvalidData
        );

        fs::write(store.path(), format!("{valid}unexpected = true\n"))
            .expect("fixture must be written");
        assert_eq!(
            store.load().expect_err("unknown field must fail").kind(),
            PortErrorKind::InvalidData
        );

        fs::write(
            store.path(),
            valid
                .replacen("max_parallelism = 4", "max_parallelism = 0", 1)
                .replacen("process_timeout_ms = 10000", "process_timeout_ms = 0", 1),
        )
        .expect("fixture must be written");
        assert_eq!(
            store
                .load()
                .expect_err("invalid operational policies must fail")
                .kind(),
            PortErrorKind::InvalidData
        );

        store
            .save(&BullSaddleConfiguration::default())
            .expect("valid configuration must replace invalid fixture");
    }

    #[test]
    fn relative_configuration_directory_is_rejected() {
        let error = TomlConfigurationStore::new("relative/config")
            .expect_err("relative paths must never resolve through the current directory");

        assert_eq!(error.kind(), PortErrorKind::InvariantViolation);
    }
}
