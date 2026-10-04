// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::observation::ObservationExecutionPolicy;
use crate::ports::{BareRepositoryDiscovery, FilesystemBoundary, SymlinkTraversal};

const DEFAULT_GIT_PROCESS_TIMEOUT_MS: u64 = 10_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalePreference {
    explicit: Option<String>,
}

impl LocalePreference {
    pub const fn auto() -> Self {
        Self { explicit: None }
    }

    pub fn explicit(locale: impl Into<String>) -> Option<Self> {
        let locale = locale.into();
        if locale.is_empty() || locale.eq_ignore_ascii_case("auto") || locale.trim() != locale {
            return None;
        }

        Some(Self {
            explicit: Some(locale),
        })
    }

    pub fn explicit_value(&self) -> Option<&str> {
        self.explicit.as_deref()
    }
}

impl Default for LocalePreference {
    fn default() -> Self {
        Self::auto()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveryConfiguration {
    roots: Vec<PathBuf>,
    exclusions: Vec<PathBuf>,
    bare_repositories: BareRepositoryDiscovery,
    symlink_traversal: SymlinkTraversal,
    filesystem_boundary: FilesystemBoundary,
}

impl DiscoveryConfiguration {
    pub fn new(
        roots: Vec<PathBuf>,
        exclusions: Vec<PathBuf>,
        bare_repositories: BareRepositoryDiscovery,
        symlink_traversal: SymlinkTraversal,
        filesystem_boundary: FilesystemBoundary,
    ) -> Option<Self> {
        if roots.iter().any(|root| !root.is_absolute())
            || exclusions
                .iter()
                .any(|exclusion| exclusion.as_os_str().is_empty())
        {
            return None;
        }

        Some(Self {
            roots,
            exclusions,
            bare_repositories,
            symlink_traversal,
            filesystem_boundary,
        })
    }

    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    pub fn exclusions(&self) -> &[PathBuf] {
        &self.exclusions
    }

    pub const fn bare_repositories(&self) -> BareRepositoryDiscovery {
        self.bare_repositories
    }

    pub const fn symlink_traversal(&self) -> SymlinkTraversal {
        self.symlink_traversal
    }

    pub const fn filesystem_boundary(&self) -> FilesystemBoundary {
        self.filesystem_boundary
    }
}

impl Default for DiscoveryConfiguration {
    fn default() -> Self {
        Self {
            roots: Vec::new(),
            exclusions: Vec::new(),
            bare_repositories: BareRepositoryDiscovery::Disabled,
            symlink_traversal: SymlinkTraversal::DoNotFollow,
            filesystem_boundary: FilesystemBoundary::StayOnRootFilesystem,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GitProcessTimeout {
    milliseconds: u64,
}

impl GitProcessTimeout {
    pub fn new(milliseconds: u64) -> Option<Self> {
        (milliseconds > 0).then_some(Self { milliseconds })
    }

    pub const fn milliseconds(self) -> u64 {
        self.milliseconds
    }

    pub fn duration(self) -> Duration {
        Duration::from_millis(self.milliseconds)
    }
}

impl Default for GitProcessTimeout {
    fn default() -> Self {
        Self {
            milliseconds: DEFAULT_GIT_PROCESS_TIMEOUT_MS,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BullSaddleConfiguration {
    locale: LocalePreference,
    discovery: DiscoveryConfiguration,
    observation_policy: ObservationExecutionPolicy,
    git_process_timeout: GitProcessTimeout,
}

impl BullSaddleConfiguration {
    pub fn new(locale: LocalePreference) -> Self {
        Self {
            locale,
            discovery: DiscoveryConfiguration::default(),
            observation_policy: ObservationExecutionPolicy::default(),
            git_process_timeout: GitProcessTimeout::default(),
        }
    }

    pub fn with_discovery(mut self, discovery: DiscoveryConfiguration) -> Self {
        self.discovery = discovery;
        self
    }

    pub fn with_observation_policy(
        mut self,
        observation_policy: ObservationExecutionPolicy,
    ) -> Self {
        self.observation_policy = observation_policy;
        self
    }

    pub fn with_git_process_timeout(mut self, git_process_timeout: GitProcessTimeout) -> Self {
        self.git_process_timeout = git_process_timeout;
        self
    }

    pub const fn locale(&self) -> &LocalePreference {
        &self.locale
    }

    pub const fn discovery(&self) -> &DiscoveryConfiguration {
        &self.discovery
    }

    pub const fn observation_policy(&self) -> ObservationExecutionPolicy {
        self.observation_policy
    }

    pub const fn git_process_timeout(&self) -> GitProcessTimeout {
        self.git_process_timeout
    }
}

impl Default for BullSaddleConfiguration {
    fn default() -> Self {
        Self::new(LocalePreference::default())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigurationSource {
    Defaults,
    File,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigurationProjection {
    file_path: PathBuf,
    source: ConfigurationSource,
    configuration: BullSaddleConfiguration,
}

impl ConfigurationProjection {
    pub fn new(
        file_path: impl Into<PathBuf>,
        source: ConfigurationSource,
        configuration: BullSaddleConfiguration,
    ) -> Option<Self> {
        let file_path = file_path.into();
        file_path.is_absolute().then_some(Self {
            file_path,
            source,
            configuration,
        })
    }

    pub fn file_path(&self) -> &Path {
        &self.file_path
    }

    pub const fn source(&self) -> ConfigurationSource {
        self.source
    }

    pub const fn configuration(&self) -> &BullSaddleConfiguration {
        &self.configuration
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{
        BullSaddleConfiguration, ConfigurationProjection, ConfigurationSource,
        DiscoveryConfiguration, GitProcessTimeout, LocalePreference,
    };
    use crate::observation::ObservationExecutionPolicy;
    use crate::ports::{BareRepositoryDiscovery, FilesystemBoundary, SymlinkTraversal};

    #[test]
    fn default_configuration_uses_conservative_runtime_policies() {
        let configuration = BullSaddleConfiguration::default();

        assert_eq!(configuration.locale().explicit_value(), None);
        assert!(configuration.discovery().roots().is_empty());
        assert!(configuration.discovery().exclusions().is_empty());
        assert_eq!(
            configuration.discovery().bare_repositories(),
            BareRepositoryDiscovery::Disabled
        );
        assert_eq!(
            configuration.discovery().symlink_traversal(),
            SymlinkTraversal::DoNotFollow
        );
        assert_eq!(
            configuration.discovery().filesystem_boundary(),
            FilesystemBoundary::StayOnRootFilesystem
        );
        assert_eq!(configuration.observation_policy().max_parallelism(), 4);
        assert_eq!(configuration.git_process_timeout().milliseconds(), 10_000);
    }

    #[test]
    fn explicit_runtime_policies_remain_semantic_application_data() {
        let root = std::env::temp_dir().join("bulls-discovery-root");
        let discovery = DiscoveryConfiguration::new(
            vec![root.clone()],
            vec![PathBuf::from("vendor")],
            BareRepositoryDiscovery::Enabled,
            SymlinkTraversal::FollowWithinRoot,
            FilesystemBoundary::CrossFilesystems,
        )
        .expect("valid discovery configuration must be accepted");
        let observation = ObservationExecutionPolicy::new(8)
            .expect("positive observation parallelism must be accepted");
        let timeout =
            GitProcessTimeout::new(15_000).expect("positive process timeout must be accepted");
        let locale = LocalePreference::explicit("pt-BR")
            .expect("well-formed explicit locale must be accepted");

        let configuration = BullSaddleConfiguration::new(locale)
            .with_discovery(discovery)
            .with_observation_policy(observation)
            .with_git_process_timeout(timeout);

        assert_eq!(configuration.locale().explicit_value(), Some("pt-BR"));
        assert_eq!(configuration.discovery().roots(), &[root]);
        assert_eq!(
            configuration.discovery().exclusions(),
            &[PathBuf::from("vendor")]
        );
        assert_eq!(configuration.observation_policy().max_parallelism(), 8);
        assert_eq!(configuration.git_process_timeout().milliseconds(), 15_000);
    }

    #[test]
    fn invalid_persisted_policy_values_are_rejected_by_semantic_types() {
        assert!(
            DiscoveryConfiguration::new(
                vec![PathBuf::from("relative/root")],
                Vec::new(),
                BareRepositoryDiscovery::Disabled,
                SymlinkTraversal::DoNotFollow,
                FilesystemBoundary::StayOnRootFilesystem,
            )
            .is_none()
        );
        assert!(
            DiscoveryConfiguration::new(
                Vec::new(),
                vec![PathBuf::new()],
                BareRepositoryDiscovery::Disabled,
                SymlinkTraversal::DoNotFollow,
                FilesystemBoundary::StayOnRootFilesystem,
            )
            .is_none()
        );
        assert!(ObservationExecutionPolicy::new(0).is_none());
        assert!(GitProcessTimeout::new(0).is_none());
    }

    #[test]
    fn reserved_or_ambiguous_locale_values_are_rejected() {
        assert!(LocalePreference::explicit("").is_none());
        assert!(LocalePreference::explicit("auto").is_none());
        assert!(LocalePreference::explicit("AUTO").is_none());
        assert!(LocalePreference::explicit(" pt-BR").is_none());
    }

    #[test]
    fn configuration_projection_preserves_source_and_requires_absolute_file_path() {
        let configuration = BullSaddleConfiguration::default();
        let file_path = std::env::temp_dir().join("bulls").join("config.toml");
        let projection = ConfigurationProjection::new(
            file_path.clone(),
            ConfigurationSource::Defaults,
            configuration.clone(),
        )
        .expect("absolute configuration path must be accepted");

        assert_eq!(projection.file_path(), file_path.as_path());
        assert_eq!(projection.source(), ConfigurationSource::Defaults);
        assert_eq!(projection.configuration(), &configuration);
        assert!(
            ConfigurationProjection::new(
                "relative/config.toml",
                ConfigurationSource::File,
                configuration,
            )
            .is_none()
        );
    }
}
