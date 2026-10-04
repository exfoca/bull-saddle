// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use bulls_application::ports::{
    CancellationPort, ConfigurationPort, DiscoveryRequest, PlatformPaths, PlatformPathsPort,
    PortError, PortErrorKind, PortResult, WorkspaceRevisionPort,
};
use bulls_application::{
    AdvisoryProjection, BullSaddleConfiguration, ConfigurationProjection, ConfigurationSource,
    DiscoverRepositories, DiscoverRepositoriesOutcome, DiscoveryBatchOutcome,
    DiscoveryConfiguration, DiscoveryRootFailure, LocalePreference, ObserveInventory,
    ObserveInventoryOutcome, OverviewProjection, RepositoryDetailProjection, RepositoryId,
    RepositoryListProjection, RepositoryListQuery, RepositorySelector,
    RepositorySelectorResolution, ResetWorkspace, ResetWorkspaceOutcome, WorkspaceMirror,
    WorkspaceOperationOutcome, WorkspaceQueryService, WorktreeId, WorktreeProjection,
};
use bulls_infra::{
    GitCliObservation, GitCliRepositoryProbe, NativeClock, NativeFilesystemIdentity,
    NativeIdentityGenerator, NativePlatformPaths, NativeRepositoryDiscovery,
    SqliteRepositoryCatalog, SqliteRepositoryRemotesObservationStore, SqliteStorage,
    SqliteWorkspaceReadStore, SqliteWorkspaceReset, SqliteWorkspaceRevisionStore,
    SqliteWorktreeObservationStore, TomlConfigurationStore,
};

#[derive(Clone, Debug, Default)]
pub struct RuntimeCancellationHandle {
    cancelled: Arc<AtomicBool>,
}

impl RuntimeCancellationHandle {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    fn callback(&self) -> Arc<dyn Fn() -> bool + Send + Sync> {
        let cancellation = self.clone();
        Arc::new(move || cancellation.is_cancelled())
    }
}

impl CancellationPort for RuntimeCancellationHandle {
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

pub struct BullSaddleRuntime {
    paths: PlatformPaths,
    configuration_store: TomlConfigurationStore,
    configuration_source: ConfigurationSource,
    configuration: BullSaddleConfiguration,
    discovery: NativeRepositoryDiscovery,
    git_topology: GitCliRepositoryProbe,
    git_observation: GitCliObservation,
    filesystem: NativeFilesystemIdentity,
    catalog: SqliteRepositoryCatalog,
    workspace_read: SqliteWorkspaceReadStore,
    workspace_reset: SqliteWorkspaceReset,
    workspace_revision: SqliteWorkspaceRevisionStore,
    identities: NativeIdentityGenerator,
    clock: NativeClock,
    worktree_observations: SqliteWorktreeObservationStore,
    repository_observations: SqliteRepositoryRemotesObservationStore,
    cancellation: RuntimeCancellationHandle,
}

impl BullSaddleRuntime {
    pub fn open() -> PortResult<Self> {
        let paths = NativePlatformPaths::new().user_paths()?;
        Self::from_platform_paths(paths)
    }

    pub fn load_locale_preference() -> PortResult<LocalePreference> {
        let paths = NativePlatformPaths::new().user_paths()?;
        let store = TomlConfigurationStore::from_platform_paths(&paths)?;
        let configuration = store.load()?.unwrap_or_default();
        Ok(configuration.locale().clone())
    }

    pub fn from_platform_paths(paths: PlatformPaths) -> PortResult<Self> {
        let configuration_store = TomlConfigurationStore::from_platform_paths(&paths)?;
        let loaded_configuration = configuration_store.load()?;
        let configuration_source = if loaded_configuration.is_some() {
            ConfigurationSource::File
        } else {
            ConfigurationSource::Defaults
        };
        let configuration = loaded_configuration.unwrap_or_default();
        let cancellation = RuntimeCancellationHandle::default();
        let git_cancellation = cancellation.callback();
        let git_process_timeout = configuration.git_process_timeout().duration();
        let storage = SqliteStorage::from_platform_paths(&paths)?;
        let workspace_read = SqliteWorkspaceReadStore::from_storage(storage.open_peer()?);
        let workspace_reset = SqliteWorkspaceReset::from_storage(storage.open_peer()?);
        let workspace_revision = SqliteWorkspaceRevisionStore::from_storage(storage.open_peer()?);
        let worktree_observations =
            SqliteWorktreeObservationStore::from_storage(storage.open_peer()?);
        let repository_observations =
            SqliteRepositoryRemotesObservationStore::from_storage(storage.open_peer()?);
        let catalog = SqliteRepositoryCatalog::from_storage(storage);

        Ok(Self {
            paths,
            configuration_store,
            configuration_source,
            configuration,
            discovery: NativeRepositoryDiscovery::with_cancellation(cancellation.callback()),
            git_topology: GitCliRepositoryProbe::with_timeout_and_cancellation(
                git_process_timeout,
                Arc::clone(&git_cancellation),
            ),
            git_observation: GitCliObservation::with_timeout_and_cancellation(
                git_process_timeout,
                git_cancellation,
            ),
            filesystem: NativeFilesystemIdentity::new(),
            catalog,
            workspace_read,
            workspace_reset,
            workspace_revision,
            identities: NativeIdentityGenerator::new(),
            clock: NativeClock::new(),
            worktree_observations,
            repository_observations,
            cancellation,
        })
    }

    pub const fn paths(&self) -> &PlatformPaths {
        &self.paths
    }

    pub const fn configuration(&self) -> &BullSaddleConfiguration {
        &self.configuration
    }

    pub fn configuration_projection(&self) -> PortResult<ConfigurationProjection> {
        ConfigurationProjection::new(
            self.configuration_store.path(),
            self.configuration_source,
            self.configuration.clone(),
        )
        .ok_or(PortError::new(PortErrorKind::InvariantViolation))
    }

    pub fn save_configuration(
        &mut self,
        configuration: &BullSaddleConfiguration,
    ) -> PortResult<()> {
        self.configuration_store.save(configuration)?;
        self.apply_git_process_policy(configuration);
        self.configuration_source = ConfigurationSource::File;
        self.configuration = configuration.clone();
        Ok(())
    }

    pub fn cancellation_handle(&self) -> RuntimeCancellationHandle {
        self.cancellation.clone()
    }

    pub fn shutdown(self) {
        self.cancellation.cancel();
    }

    pub fn discover(&mut self) -> PortResult<WorkspaceOperationOutcome<DiscoveryBatchOutcome>> {
        let roots = self.configuration.discovery().roots().to_vec();
        self.discover_roots(&roots)
    }

    pub fn discover_roots(
        &mut self,
        roots: &[PathBuf],
    ) -> PortResult<WorkspaceOperationOutcome<DiscoveryBatchOutcome>> {
        let configuration = self.configuration.discovery().clone();
        let mut completed = Vec::new();
        let mut failed_roots = Vec::new();

        for root in roots {
            let request = discovery_request(root, &configuration);
            match self.execute_discovery(&request) {
                Ok(outcome) => completed.push(outcome),
                Err(error) if is_isolated_discovery_failure(error.kind()) => {
                    failed_roots.push(DiscoveryRootFailure::new(root, error.kind()));
                }
                Err(error) => return Err(error),
            }
        }

        let revision = self.workspace_revision.workspace_revision()?;
        Ok(WorkspaceOperationOutcome::new(
            revision,
            DiscoveryBatchOutcome::new(completed, failed_roots),
        ))
    }

    fn execute_discovery(
        &mut self,
        request: &DiscoveryRequest,
    ) -> PortResult<DiscoverRepositoriesOutcome> {
        DiscoverRepositories::new(
            &self.discovery,
            &self.git_topology,
            &self.filesystem,
            &mut self.catalog,
            &mut self.identities,
        )
        .execute(request)
    }

    pub fn reset(&mut self) -> PortResult<WorkspaceOperationOutcome<ResetWorkspaceOutcome>> {
        ResetWorkspace::new(&mut self.workspace_reset).execute()
    }

    pub fn refresh(&mut self) -> PortResult<WorkspaceOperationOutcome<ObserveInventoryOutcome>> {
        let outcome = ObserveInventory::new(
            &self.catalog,
            &self.git_observation,
            &self.clock,
            &self.cancellation,
            &mut self.identities,
            &mut self.worktree_observations,
            &mut self.repository_observations,
        )
        .execute(self.configuration.observation_policy())?;
        let revision = self.workspace_revision.workspace_revision()?;
        Ok(WorkspaceOperationOutcome::new(revision, outcome))
    }

    fn apply_git_process_policy(&mut self, configuration: &BullSaddleConfiguration) {
        let cancellation = self.cancellation.callback();
        let timeout = configuration.git_process_timeout().duration();
        self.git_topology = GitCliRepositoryProbe::with_timeout_and_cancellation(
            timeout,
            Arc::clone(&cancellation),
        );
        self.git_observation =
            GitCliObservation::with_timeout_and_cancellation(timeout, cancellation);
    }

    pub fn overview(&self) -> PortResult<OverviewProjection> {
        WorkspaceQueryService::new(&self.workspace_read).overview()
    }

    pub fn repositories(
        &self,
        query: &RepositoryListQuery,
    ) -> PortResult<RepositoryListProjection> {
        WorkspaceQueryService::new(&self.workspace_read).repositories(query)
    }

    pub fn repository(
        &self,
        repository_id: &RepositoryId,
    ) -> PortResult<Option<RepositoryDetailProjection>> {
        WorkspaceQueryService::new(&self.workspace_read).repository(repository_id)
    }

    pub fn resolve_repository_selector(
        &self,
        selector: &RepositorySelector,
    ) -> PortResult<RepositorySelectorResolution> {
        WorkspaceQueryService::new(&self.workspace_read).resolve_repository_selector(selector)
    }

    pub fn worktree(&self, worktree_id: &WorktreeId) -> PortResult<Option<WorktreeProjection>> {
        WorkspaceQueryService::new(&self.workspace_read).worktree(worktree_id)
    }

    pub fn mirror(&self) -> PortResult<WorkspaceMirror> {
        WorkspaceQueryService::new(&self.workspace_read).mirror()
    }

    pub fn advisories(&self) -> PortResult<AdvisoryProjection> {
        WorkspaceQueryService::new(&self.workspace_read).advisories()
    }

    pub fn overview_with_advisories(&self) -> PortResult<(OverviewProjection, AdvisoryProjection)> {
        WorkspaceQueryService::new(&self.workspace_read).overview_with_advisories()
    }

    pub fn repository_with_advisories(
        &self,
        repository_id: &RepositoryId,
    ) -> PortResult<Option<(RepositoryDetailProjection, AdvisoryProjection)>> {
        WorkspaceQueryService::new(&self.workspace_read).repository_with_advisories(repository_id)
    }
}

fn discovery_request(root: &Path, configuration: &DiscoveryConfiguration) -> DiscoveryRequest {
    let mut request = DiscoveryRequest::new(root)
        .with_bare_repositories(configuration.bare_repositories())
        .with_symlink_traversal(configuration.symlink_traversal())
        .with_filesystem_boundary(configuration.filesystem_boundary());

    for exclusion in configuration.exclusions() {
        request = request.with_exclusion(exclusion.clone());
    }

    request
}

const fn is_isolated_discovery_failure(kind: PortErrorKind) -> bool {
    matches!(
        kind,
        PortErrorKind::PermissionDenied
            | PortErrorKind::ResourceUnavailable
            | PortErrorKind::IoFailure
            | PortErrorKind::InvalidData
    )
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::sync::atomic::{AtomicU64, Ordering};

    use bulls_application::ports::{
        BareRepositoryDiscovery, FilesystemBoundary, PlatformPaths, SymlinkTraversal,
    };
    use bulls_application::{
        AdvisoryCode, BullSaddleConfiguration, ConfigurationSource, DiscoveryConfiguration,
        GitProcessTimeout, Knowledge, LocalePreference, ObservationExecutionPolicy,
        ProtocolAdvisoryCode, ProtocolAdvisoryProjection, ProtocolDocument, ProtocolOverview,
        ProtocolWorkspaceMirror, READ_PROTOCOL_SCHEMA_VERSION, RepositoryListQuery,
    };

    use super::BullSaddleRuntime;

    static TEST_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let sequence = TEST_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "bulls-runtime-test-{}-{}",
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

    fn git(path: &Path, arguments: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(path)
            .args(arguments)
            .env("GIT_TERMINAL_PROMPT", "0")
            .status()
            .expect("Git fixture command must start");
        assert!(status.success(), "Git fixture command must succeed");
    }

    #[test]
    fn runtime_cancellation_reaches_refresh_and_filesystem_discovery() {
        let root = TestDirectory::new();
        let first_workspace = root.path().join("first-workspace");
        let first_repository = first_workspace.join("repository");
        let second_workspace = root.path().join("second-workspace");
        let second_repository = second_workspace.join("repository");
        let user = root.path().join("user");
        fs::create_dir_all(&first_repository).expect("first repository directory must be created");
        fs::create_dir_all(&second_repository)
            .expect("second repository directory must be created");
        git(&first_repository, &["init", "-q"]);
        git(&second_repository, &["init", "-q"]);

        let paths = PlatformPaths::new(
            user.join("config"),
            user.join("data"),
            user.join("state"),
            user.join("cache"),
        );
        let mut runtime = BullSaddleRuntime::from_platform_paths(paths)
            .expect("runtime composition must succeed");
        let first_discovery = DiscoveryConfiguration::new(
            vec![first_workspace.clone()],
            Vec::new(),
            BareRepositoryDiscovery::Disabled,
            SymlinkTraversal::DoNotFollow,
            FilesystemBoundary::StayOnRootFilesystem,
        )
        .expect("first discovery configuration must be valid");
        runtime
            .save_configuration(&BullSaddleConfiguration::default().with_discovery(first_discovery))
            .expect("first discovery configuration must save");
        runtime.discover().expect("initial discovery must succeed");

        let cancellation = runtime.cancellation_handle();
        cancellation.cancel();
        assert!(cancellation.is_cancelled());

        let observation = runtime
            .refresh()
            .expect("cancelled refresh must persist target failures");
        let observation = observation.outcome();
        assert_eq!(observation.target_count(), 2);
        assert_eq!(observation.succeeded_count(), 0);
        assert_eq!(observation.failed_count(), 2);
        assert_eq!(observation.cancelled_count(), 2);
        assert!(observation.was_cancelled());

        let second_discovery = DiscoveryConfiguration::new(
            vec![second_workspace.clone()],
            Vec::new(),
            BareRepositoryDiscovery::Disabled,
            SymlinkTraversal::DoNotFollow,
            FilesystemBoundary::StayOnRootFilesystem,
        )
        .expect("second discovery configuration must be valid");
        runtime
            .save_configuration(
                &BullSaddleConfiguration::default().with_discovery(second_discovery),
            )
            .expect("second discovery configuration must save");
        let error = runtime
            .discover()
            .expect_err("cancelled filesystem discovery must abort the batch");
        assert_eq!(
            error.kind(),
            bulls_application::ports::PortErrorKind::Cancelled
        );
    }

    #[test]
    fn runtime_discovery_uses_configured_policies_and_isolates_unavailable_roots() {
        let root = TestDirectory::new();
        let workspace = root.path().join("workspace");
        let repository = workspace.join("repository");
        let excluded_repository = workspace.join("vendor").join("excluded");
        let missing_root = root.path().join("offline");
        let user = root.path().join("user");
        fs::create_dir_all(&repository).expect("repository directory must be created");
        fs::create_dir_all(&excluded_repository)
            .expect("excluded repository directory must be created");
        git(&repository, &["init", "-q"]);
        git(&excluded_repository, &["init", "-q"]);

        let paths = PlatformPaths::new(
            user.join("config"),
            user.join("data"),
            user.join("state"),
            user.join("cache"),
        );
        let mut runtime = BullSaddleRuntime::from_platform_paths(paths)
            .expect("runtime composition must succeed");
        let discovery = DiscoveryConfiguration::new(
            vec![missing_root.clone(), workspace.clone()],
            vec![PathBuf::from("vendor")],
            BareRepositoryDiscovery::Disabled,
            SymlinkTraversal::DoNotFollow,
            FilesystemBoundary::StayOnRootFilesystem,
        )
        .expect("discovery configuration must be valid");
        runtime
            .save_configuration(&BullSaddleConfiguration::default().with_discovery(discovery))
            .expect("discovery configuration must save");

        let operation = runtime.discover().expect("discovery batch must complete");
        let outcome = operation.outcome();

        assert_eq!(outcome.requested_root_count(), 2);
        assert_eq!(outcome.completed().len(), 1);
        assert_eq!(outcome.partial_root_count(), 0);
        assert_eq!(outcome.discovery_issue_count(), 0);
        assert_eq!(outcome.identification_issue_count(), 0);
        assert!(outcome.is_partial());
        assert_eq!(outcome.completed()[0].root(), workspace.as_path());
        assert_eq!(outcome.completed()[0].repository_count(), 1);
        assert_eq!(outcome.failed_roots().len(), 1);
        assert_eq!(outcome.failed_roots()[0].root(), missing_root.as_path());
        assert_eq!(
            outcome.failed_roots()[0].kind(),
            bulls_application::ports::PortErrorKind::ResourceUnavailable
        );
    }

    #[test]
    fn transient_discovery_roots_reuse_policy_without_mutating_configuration() {
        let root = TestDirectory::new();
        let configured_workspace = root.path().join("configured-workspace");
        let explicit_workspace = root.path().join("explicit-workspace");
        let explicit_repository = explicit_workspace.join("repository");
        let excluded_repository = explicit_workspace.join("vendor").join("excluded");
        let user = root.path().join("user");
        fs::create_dir_all(&configured_workspace)
            .expect("configured workspace directory must be created");
        fs::create_dir_all(&explicit_repository)
            .expect("explicit repository directory must be created");
        fs::create_dir_all(&excluded_repository)
            .expect("excluded repository directory must be created");
        git(&explicit_repository, &["init", "-q"]);
        git(&excluded_repository, &["init", "-q"]);

        let paths = PlatformPaths::new(
            user.join("config"),
            user.join("data"),
            user.join("state"),
            user.join("cache"),
        );
        let mut runtime = BullSaddleRuntime::from_platform_paths(paths)
            .expect("runtime composition must succeed");
        let discovery = DiscoveryConfiguration::new(
            vec![configured_workspace.clone()],
            vec![PathBuf::from("vendor")],
            BareRepositoryDiscovery::Disabled,
            SymlinkTraversal::DoNotFollow,
            FilesystemBoundary::StayOnRootFilesystem,
        )
        .expect("discovery configuration must be valid");
        runtime
            .save_configuration(&BullSaddleConfiguration::default().with_discovery(discovery))
            .expect("discovery configuration must save");

        let operation = runtime
            .discover_roots(std::slice::from_ref(&explicit_workspace))
            .expect("transient discovery must succeed");
        let outcome = operation.outcome();

        assert_eq!(outcome.requested_root_count(), 1);
        assert_eq!(outcome.completed().len(), 1);
        assert_eq!(outcome.completed()[0].repository_count(), 1);
        assert!(!outcome.is_partial());
        assert_eq!(
            runtime.configuration().discovery().roots(),
            &[configured_workspace]
        );
    }

    #[test]
    fn runtime_shutdown_and_reopen_preserve_state_without_mutating_reads() {
        let root = TestDirectory::new();
        let workspace = root.path().join("workspace");
        let repository = workspace.join("repository");
        let user = root.path().join("user");
        fs::create_dir_all(&repository).expect("repository directory must be created");
        git(&repository, &["init", "-q"]);

        let paths = PlatformPaths::new(
            user.join("config"),
            user.join("data"),
            user.join("state"),
            user.join("cache"),
        );
        let discovery = DiscoveryConfiguration::new(
            vec![workspace],
            Vec::new(),
            BareRepositoryDiscovery::Disabled,
            SymlinkTraversal::DoNotFollow,
            FilesystemBoundary::StayOnRootFilesystem,
        )
        .expect("discovery configuration must be valid");
        let configuration = BullSaddleConfiguration::new(
            LocalePreference::explicit("pt-BR").expect("locale must be accepted"),
        )
        .with_discovery(discovery)
        .with_observation_policy(
            ObservationExecutionPolicy::new(2)
                .expect("runtime observation parallelism must be valid"),
        )
        .with_git_process_timeout(
            GitProcessTimeout::new(12_000).expect("runtime process timeout must be valid"),
        );

        let mut runtime = BullSaddleRuntime::from_platform_paths(paths.clone())
            .expect("runtime composition must succeed");
        runtime
            .save_configuration(&configuration)
            .expect("configuration must save through the runtime");
        runtime.discover().expect("runtime discovery must succeed");
        runtime.refresh().expect("runtime observation must succeed");

        let before_shutdown = runtime
            .overview()
            .expect("runtime overview before shutdown must succeed");
        let cancellation = runtime.cancellation_handle();
        runtime.shutdown();
        assert!(cancellation.is_cancelled());

        let runtime = BullSaddleRuntime::from_platform_paths(paths)
            .expect("runtime must reopen persisted state");
        assert_eq!(runtime.configuration(), &configuration);
        assert!(!runtime.cancellation_handle().is_cancelled());

        let overview = runtime
            .overview()
            .expect("reopened runtime overview must succeed");
        assert_eq!(overview, before_shutdown);

        let repositories = runtime
            .repositories(&RepositoryListQuery::default())
            .expect("reopened repository query must succeed");
        assert_eq!(repositories.revision(), overview.revision());
        assert_eq!(repositories.items().len(), 1);

        let repository_id = repositories.items()[0].id().clone();
        let repository = runtime
            .repository(&repository_id)
            .expect("reopened repository detail query must succeed")
            .expect("persisted repository must remain queryable");
        assert_eq!(repository.revision(), overview.revision());
        assert_eq!(repository.worktrees().len(), 1);

        let worktree_id = repository.worktrees()[0].id().clone();
        let worktree = runtime
            .worktree(&worktree_id)
            .expect("reopened worktree query must succeed")
            .expect("persisted worktree must remain queryable");
        assert_eq!(worktree.revision(), overview.revision());

        let mirror = runtime
            .mirror()
            .expect("reopened mirror query must succeed");
        assert_eq!(mirror.revision(), overview.revision());

        let advisories = runtime
            .advisories()
            .expect("reopened advisory query must succeed");
        assert_eq!(advisories.revision(), overview.revision());

        let after_reads = runtime
            .overview()
            .expect("overview after pure reads must succeed");
        assert_eq!(after_reads.revision(), overview.revision());
    }

    #[test]
    fn runtime_configuration_projection_distinguishes_defaults_from_persisted_file() {
        let root = TestDirectory::new();
        let user = root.path().join("user");
        let paths = PlatformPaths::new(
            user.join("config"),
            user.join("data"),
            user.join("state"),
            user.join("cache"),
        );
        let expected_path = paths.config_dir().join("config.toml");
        let mut runtime = BullSaddleRuntime::from_platform_paths(paths.clone())
            .expect("runtime composition must succeed");

        let defaults = runtime
            .configuration_projection()
            .expect("default configuration projection must succeed");
        assert_eq!(defaults.file_path(), expected_path.as_path());
        assert_eq!(defaults.source(), ConfigurationSource::Defaults);
        assert_eq!(
            defaults.configuration(),
            &BullSaddleConfiguration::default()
        );

        let configuration = BullSaddleConfiguration::new(
            LocalePreference::explicit("pt-BR").expect("locale must be accepted"),
        )
        .with_observation_policy(
            ObservationExecutionPolicy::new(7).expect("parallelism must be accepted"),
        )
        .with_git_process_timeout(GitProcessTimeout::new(9_000).expect("timeout must be accepted"));
        runtime
            .save_configuration(&configuration)
            .expect("configuration must save");

        let persisted = runtime
            .configuration_projection()
            .expect("persisted configuration projection must succeed");
        assert_eq!(persisted.file_path(), expected_path.as_path());
        assert_eq!(persisted.source(), ConfigurationSource::File);
        assert_eq!(persisted.configuration(), &configuration);

        runtime.shutdown();
        let runtime = BullSaddleRuntime::from_platform_paths(paths)
            .expect("runtime must reopen persisted configuration");
        let reopened = runtime
            .configuration_projection()
            .expect("reopened configuration projection must succeed");
        assert_eq!(reopened.source(), ConfigurationSource::File);
        assert_eq!(reopened.configuration(), &configuration);
    }

    #[test]
    fn runtime_reset_discards_workspace_knowledge_without_touching_configuration_or_repository() {
        let root = TestDirectory::new();
        let workspace = root.path().join("workspace");
        let repository = workspace.join("repository");
        let user = root.path().join("user");
        fs::create_dir_all(&repository).expect("repository directory must be created");
        fs::write(repository.join("keep.txt"), "repository-owned data\n")
            .expect("repository fixture must be written");
        git(&repository, &["init", "-q"]);

        let paths = PlatformPaths::new(
            user.join("config"),
            user.join("data"),
            user.join("state"),
            user.join("cache"),
        );
        let discovery = DiscoveryConfiguration::new(
            vec![workspace],
            Vec::new(),
            BareRepositoryDiscovery::Disabled,
            SymlinkTraversal::DoNotFollow,
            FilesystemBoundary::StayOnRootFilesystem,
        )
        .expect("discovery configuration must be valid");
        let configuration = BullSaddleConfiguration::new(
            LocalePreference::explicit("pt-BR").expect("locale must be accepted"),
        )
        .with_discovery(discovery);

        let mut runtime = BullSaddleRuntime::from_platform_paths(paths.clone())
            .expect("runtime composition must succeed");
        runtime
            .save_configuration(&configuration)
            .expect("configuration must save");
        runtime.discover().expect("discovery must succeed");
        runtime.refresh().expect("refresh must succeed");

        let before = runtime
            .overview()
            .expect("overview before reset must succeed");
        assert_eq!(before.repository_count(), 1);
        assert_eq!(before.worktree_count(), 1);

        let reset = runtime.reset().expect("workspace reset must succeed");
        assert!(reset.revision() > before.revision());
        let after = runtime
            .overview()
            .expect("overview after reset must succeed");
        assert_eq!(after.revision(), reset.revision());
        assert_eq!(after.repository_count(), 0);
        assert_eq!(after.worktree_count(), 0);
        assert_eq!(runtime.configuration(), &configuration);
        assert!(repository.join(".git").is_dir());
        assert_eq!(
            fs::read_to_string(repository.join("keep.txt")).expect("repository data must remain"),
            "repository-owned data\n"
        );

        let repeated = runtime.reset().expect("repeated reset must succeed");
        assert_eq!(repeated.revision(), reset.revision());

        runtime.shutdown();
        let runtime =
            BullSaddleRuntime::from_platform_paths(paths).expect("runtime must reopen after reset");
        assert_eq!(runtime.configuration(), &configuration);
        let reopened = runtime.overview().expect("reopened overview must succeed");
        assert_eq!(reopened.revision(), reset.revision());
        assert_eq!(reopened.repository_count(), 0);
        assert_eq!(reopened.worktree_count(), 0);
        assert!(repository.join(".git").is_dir());
    }

    #[test]
    fn runtime_composes_discovery_observation_persistence_and_configuration_once() {
        let root = TestDirectory::new();
        let workspace = root.path().join("workspace");
        let repository = workspace.join("repository");
        let user = root.path().join("user");
        fs::create_dir_all(&repository).expect("repository directory must be created");
        git(&repository, &["init", "-q"]);

        let paths = PlatformPaths::new(
            user.join("config"),
            user.join("data"),
            user.join("state"),
            user.join("cache"),
        );
        let mut runtime = BullSaddleRuntime::from_platform_paths(paths.clone())
            .expect("runtime composition must succeed");
        assert_eq!(runtime.configuration(), &BullSaddleConfiguration::default());

        let discovery_configuration = DiscoveryConfiguration::new(
            vec![workspace.clone()],
            vec![PathBuf::from("vendor")],
            BareRepositoryDiscovery::Enabled,
            SymlinkTraversal::FollowWithinRoot,
            FilesystemBoundary::CrossFilesystems,
        )
        .expect("runtime discovery configuration must be valid");
        let configuration = BullSaddleConfiguration::new(
            LocalePreference::explicit("pt-BR").expect("locale must be accepted"),
        )
        .with_discovery(discovery_configuration)
        .with_observation_policy(
            ObservationExecutionPolicy::new(2)
                .expect("runtime observation parallelism must be valid"),
        )
        .with_git_process_timeout(
            GitProcessTimeout::new(12_000).expect("runtime process timeout must be valid"),
        );
        runtime
            .save_configuration(&configuration)
            .expect("configuration must save through the runtime");
        assert_eq!(runtime.configuration(), &configuration);

        let discovery = runtime.discover().expect("runtime discovery must succeed");
        let discovery_revision = discovery.revision();
        assert_eq!(discovery.outcome().completed().len(), 1);
        assert!(discovery.outcome().failed_roots().is_empty());
        assert_eq!(discovery.outcome().completed()[0].repository_count(), 1);
        assert_eq!(discovery.outcome().completed()[0].location_count(), 1);
        assert_eq!(discovery.outcome().completed()[0].worktree_count(), 1);

        let observation = runtime.refresh().expect("runtime observation must succeed");
        assert!(observation.revision() > discovery_revision);
        let observation_revision = observation.revision();
        assert_eq!(observation.outcome().target_count(), 2);
        assert_eq!(observation.outcome().succeeded_count(), 2);
        assert_eq!(observation.outcome().failed_count(), 0);
        assert_eq!(runtime.paths(), &paths);
        assert!(paths.data_dir().join("bulls.sqlite").is_file());

        let overview = runtime.overview().expect("runtime overview must succeed");
        assert_eq!(overview.revision(), observation_revision);
        assert_eq!(overview.repository_count(), 1);
        assert_eq!(overview.available_repository_count(), 1);
        assert_eq!(overview.worktree_count(), 1);
        assert_eq!(overview.repositories_with_unknown_local_work_count(), 0);
        assert_eq!(overview.repositories_without_remotes_count(), 1);
        assert_eq!(overview.repositories_with_unknown_remotes_count(), 0);

        let repositories = runtime
            .repositories(&RepositoryListQuery::default())
            .expect("runtime repository query must succeed");
        assert_eq!(repositories.revision(), overview.revision());
        assert_eq!(repositories.total_matching(), 1);
        assert_eq!(repositories.items().len(), 1);
        assert_eq!(
            repositories.items()[0].has_remotes(),
            &Knowledge::Known(false)
        );
        assert_eq!(
            repositories.items()[0].has_local_work(),
            &Knowledge::Known(false)
        );

        let repository_id = repositories.items()[0].id().clone();
        let repository_detail = runtime
            .repository(&repository_id)
            .expect("runtime repository detail query must succeed")
            .expect("discovered repository must be queryable");
        assert_eq!(repository_detail.revision(), overview.revision());
        assert_eq!(repository_detail.worktrees().len(), 1);

        let worktree_id = repository_detail.worktrees()[0].id().clone();
        let worktree = runtime
            .worktree(&worktree_id)
            .expect("runtime worktree query must succeed")
            .expect("discovered worktree must be queryable");
        assert_eq!(worktree.revision(), overview.revision());

        let mirror = runtime.mirror().expect("runtime mirror query must succeed");
        assert_eq!(mirror.revision(), overview.revision());
        assert_eq!(mirror.repositories().len(), 1);

        let advisories = runtime
            .advisories()
            .expect("runtime advisory query must succeed");
        assert_eq!(advisories.revision(), overview.revision());
        assert!(
            advisories
                .advisories()
                .iter()
                .any(|advisory| { advisory.code() == AdvisoryCode::RepositoryWithoutRemotes })
        );

        let overview_protocol = ProtocolDocument::<ProtocolOverview>::try_from(&overview)
            .expect("overview protocol must build");
        let mirror_protocol = ProtocolDocument::<ProtocolWorkspaceMirror>::try_from(&mirror)
            .expect("mirror protocol must build");
        let advisory_protocol =
            ProtocolDocument::<ProtocolAdvisoryProjection>::try_from(&advisories)
                .expect("advisory protocol must build");
        assert_eq!(
            overview_protocol.schema_version,
            READ_PROTOCOL_SCHEMA_VERSION
        );
        assert_eq!(
            overview_protocol.workspace_revision,
            overview.revision().value()
        );
        assert_eq!(overview_protocol.payload.repository_count, 1);
        assert_eq!(
            mirror_protocol.workspace_revision,
            overview_protocol.workspace_revision
        );
        assert_eq!(mirror_protocol.payload.repositories.len(), 1);
        assert_eq!(
            advisory_protocol.workspace_revision,
            overview_protocol.workspace_revision
        );
        assert!(
            advisory_protocol.payload.advisories.iter().any(|advisory| {
                advisory.code == ProtocolAdvisoryCode::RepositoryWithoutRemotes
            })
        );

        let repeated_overview = runtime
            .overview()
            .expect("repeated runtime overview must succeed");
        assert_eq!(repeated_overview.revision(), overview.revision());
    }
}
