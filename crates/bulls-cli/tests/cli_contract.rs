// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(unix)]
use std::{
    os::unix::fs::PermissionsExt,
    process::Stdio,
    thread,
    time::{Duration, Instant},
};

static TEST_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let sequence = TEST_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "bulls-cli-contract-test-{}-{}",
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

fn bulls(arguments: &[&str]) -> Output {
    let root = TestDirectory::new();
    isolated_bulls(root.path(), arguments.iter().copied())
}

fn isolated_bulls<I, S>(user_root: &Path, arguments: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    isolated_bulls_with_locale(user_root, arguments, "C")
}

fn isolated_bulls_with_locale<I, S>(user_root: &Path, arguments: I, locale: &str) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    isolated_command(user_root, locale)
        .args(arguments)
        .output()
        .expect("isolated bulls process must start")
}

fn isolated_command(user_root: &Path, locale: &str) -> Command {
    fs::create_dir_all(user_root).expect("isolated user root must be created");
    let mut command = Command::new(env!("CARGO_BIN_EXE_bulls"));
    command
        .env("HOME", user_root.join("home"))
        .env("XDG_CONFIG_HOME", user_root.join("xdg-config"))
        .env("XDG_DATA_HOME", user_root.join("xdg-data"))
        .env("XDG_STATE_HOME", user_root.join("xdg-state"))
        .env("XDG_CACHE_HOME", user_root.join("xdg-cache"))
        .env("APPDATA", user_root.join("appdata"))
        .env("LOCALAPPDATA", user_root.join("local-appdata"))
        .env("USERPROFILE", user_root.join("profile"))
        .env("LC_ALL", locale)
        .env_remove("LC_MESSAGES")
        .env_remove("LANG");
    command
}

fn isolated_bulls_with_lang<I, S>(user_root: &Path, arguments: I, locale: &str) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    isolated_command(user_root, "C")
        .env_remove("LC_ALL")
        .env_remove("LC_MESSAGES")
        .env("LANG", locale)
        .args(arguments)
        .output()
        .expect("isolated bulls process must start")
}

struct LocaleCase {
    environment: &'static str,
    usage: &'static str,
    unknown_command: &'static str,
    discovery: &'static str,
    refresh: &'static str,
    repositories: &'static str,
    repository_prefix: &'static str,
    overview: &'static str,
    failure_report: &'static str,
}

const SUPPORTED_LOCALE_CASES: &[LocaleCase] = &[
    LocaleCase {
        environment: "en_US.UTF-8",
        usage: "Usage",
        unknown_command: "Unknown command: unknown",
        discovery: "Discovery",
        refresh: "Refresh",
        repositories: "Repositories",
        repository_prefix: "Repository: ",
        overview: "Repositories: 1",
        failure_report: "Failure report",
    },
    LocaleCase {
        environment: "pt_BR.UTF-8",
        usage: "Uso",
        unknown_command: "Comando desconhecido: unknown",
        discovery: "Descoberta",
        refresh: "Atualização",
        repositories: "Repositórios",
        repository_prefix: "Repositório: ",
        overview: "Repositórios: 1",
        failure_report: "Relatório de falha",
    },
    LocaleCase {
        environment: "es_MX.UTF-8",
        usage: "Uso",
        unknown_command: "Comando desconocido: unknown",
        discovery: "Descubrimiento",
        refresh: "Actualización",
        repositories: "Repositorios",
        repository_prefix: "Repositorio: ",
        overview: "Repositorios: 1",
        failure_report: "Informe de fallo",
    },
    LocaleCase {
        environment: "de_DE.UTF-8",
        usage: "Verwendung",
        unknown_command: "Unbekannter Befehl: unknown",
        discovery: "Erkennung",
        refresh: "Aktualisierung",
        repositories: "Repositories",
        repository_prefix: "Repository: ",
        overview: "Repositories: 1",
        failure_report: "Fehlerbericht",
    },
    LocaleCase {
        environment: "ja_JP.UTF-8",
        usage: "使用方法",
        unknown_command: "不明なコマンド: unknown",
        discovery: "検出",
        refresh: "更新",
        repositories: "リポジトリ",
        repository_prefix: "リポジトリ: ",
        overview: "リポジトリ: 1",
        failure_report: "障害レポート",
    },
    LocaleCase {
        environment: "zh_CN.UTF-8",
        usage: "用法",
        unknown_command: "未知命令: unknown",
        discovery: "发现",
        refresh: "刷新",
        repositories: "仓库",
        repository_prefix: "仓库: ",
        overview: "仓库: 1",
        failure_report: "故障报告",
    },
];

#[cfg(target_os = "macos")]
fn configuration_dir(user_root: &Path) -> PathBuf {
    user_root
        .join("home")
        .join("Library")
        .join("Application Support")
        .join("bulls")
}

#[cfg(target_os = "windows")]
fn configuration_dir(user_root: &Path) -> PathBuf {
    user_root.join("appdata").join("bulls")
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn configuration_dir(user_root: &Path) -> PathBuf {
    user_root.join("xdg-config").join("bulls")
}

#[cfg(target_os = "macos")]
fn data_dir(user_root: &Path) -> PathBuf {
    configuration_dir(user_root)
}

#[cfg(target_os = "windows")]
fn data_dir(user_root: &Path) -> PathBuf {
    user_root.join("local-appdata").join("bulls")
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn data_dir(user_root: &Path) -> PathBuf {
    user_root.join("xdg-data").join("bulls")
}

fn write_configuration(user_root: &Path, locale: &str) {
    write_configuration_with_parallelism(user_root, locale, 4);
}

fn write_configuration_with_parallelism(user_root: &Path, locale: &str, max_parallelism: usize) {
    let config_dir = configuration_dir(user_root);
    fs::create_dir_all(&config_dir).expect("configuration directory must be created");
    fs::write(
        config_dir.join("config.toml"),
        format!(
            r#"schema_version = 2
locale = "{locale}"

[discovery]
roots = []
exclusions = []
bare_repositories = "disabled"
symlink_traversal = "do_not_follow"
filesystem_boundary = "stay_on_root_filesystem"

[observation]
max_parallelism = {max_parallelism}

[git]
process_timeout_ms = 10000
"#
        ),
    )
    .expect("configuration fixture must be written");
}

fn git_init(path: &Path) {
    fs::create_dir_all(path).expect("repository directory must be created");
    let status = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["init", "-q"])
        .env("GIT_TERMINAL_PROMPT", "0")
        .status()
        .expect("Git fixture command must start");
    assert!(status.success(), "Git fixture command must succeed");
}

#[test]
fn root_help_is_a_successful_public_contract() {
    let output = bulls(&["--help"]);

    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("bulls <command>")
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn version_uses_the_workspace_package_version() {
    let output = bulls(&["--version"]);

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!("bulls {}\n", env!("CARGO_PKG_VERSION"))
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn help_and_version_do_not_create_runtime_state() {
    let root = TestDirectory::new();
    let user = root.path().join("user");

    let help = isolated_bulls(&user, [OsStr::new("--help")]);
    let version = isolated_bulls(&user, [OsStr::new("--version")]);

    assert!(help.status.success());
    assert!(version.status.success());
    for path in [
        user.join("xdg-config"),
        user.join("xdg-data"),
        user.join("xdg-state"),
        user.join("xdg-cache"),
        user.join("appdata"),
        user.join("local-appdata"),
        user.join("profile"),
        user.join("home"),
    ] {
        assert!(!path.exists(), "presentation-only command created {path:?}");
    }
}

#[test]
fn automatic_locale_uses_environment_without_persisting_state() {
    let root = TestDirectory::new();
    let user = root.path().join("user");

    let output = isolated_bulls_with_locale(&user, [OsStr::new("--help")], "pt_BR.UTF-8");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Uso:"));
    assert!(stdout.contains("Comandos:"));
    assert!(!configuration_dir(&user).exists());
}

#[test]
fn automatic_locale_does_not_promote_unregistered_regional_variants() {
    let root = TestDirectory::new();
    let user = root.path().join("user");

    let output = isolated_bulls_with_locale(&user, [OsStr::new("--help")], "pt_PT.UTF-8");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Usage:"));
    assert!(!stdout.contains("Uso:"));
    assert!(!configuration_dir(&user).exists());
}

#[test]
fn explicit_portuguese_locale_localizes_help_errors_and_human_queries() {
    let root = TestDirectory::new();
    let user = root.path().join("user");
    write_configuration(&user, "pt-BR");

    let help = isolated_bulls(&user, [OsStr::new("--help")]);
    assert!(help.status.success());
    let help_stdout = String::from_utf8(help.stdout).unwrap();
    assert!(help_stdout.contains("Uso:"));
    assert!(help_stdout.contains("Ler o inventário de repositórios conhecidos"));

    let syntax = isolated_bulls(&user, [OsStr::new("inspect")]);
    assert_eq!(syntax.status.code(), Some(2));
    let syntax_stderr = String::from_utf8(syntax.stderr).unwrap();
    assert!(syntax_stderr.contains("Erro: O uso exige exatamente um seletor de repositório."));
    assert!(syntax_stderr.contains("Uso: bulls inspect <repository-selector>"));
    assert!(
        !data_dir(&user).join("bulls.sqlite").exists(),
        "presentation-only commands must not initialize runtime storage"
    );

    let overview = isolated_bulls(&user, [OsStr::new("overview")]);
    assert!(overview.status.success());
    let overview_stdout = String::from_utf8(overview.stdout).unwrap();
    assert!(!overview_stdout.contains("Revisão do workspace:"));
    assert!(overview_stdout.contains("Repositórios:"));
    assert!(overview_stdout.contains("Recomendações:"));

    let missing = isolated_bulls(
        &user,
        [
            OsStr::new("inspect"),
            OsStr::new("repository-does-not-exist"),
        ],
    );
    assert_eq!(missing.status.code(), Some(3));
    let missing_stderr = String::from_utf8(missing.stderr).unwrap();
    assert!(missing_stderr.contains("Erro: Repositório não encontrado."));
    assert!(missing_stderr.contains("Relatório de falha"));
    assert!(missing_stderr.contains("Classe: esperada"));
    assert!(missing_stderr.contains("Código do erro: repository_not_found"));
}

#[test]
fn json_semantics_are_invariant_across_supported_locales() {
    let root = TestDirectory::new();
    let user = root.path().join("user");
    let mut overview_baseline = None;
    let mut failure_baseline = None;

    for locale in SUPPORTED_LOCALE_CASES {
        let overview = isolated_bulls_with_lang(
            &user,
            [OsStr::new("overview"), OsStr::new("--format=json")],
            locale.environment,
        );
        assert!(overview.status.success(), "locale: {}", locale.environment);
        let overview: serde_json::Value = serde_json::from_slice(&overview.stdout)
            .expect("localized overview JSON must be valid");
        if let Some(expected) = &overview_baseline {
            assert_eq!(
                &overview, expected,
                "overview JSON changed with locale {}",
                locale.environment
            );
        } else {
            overview_baseline = Some(overview);
        }

        let failure = isolated_bulls_with_lang(
            &user,
            [
                OsStr::new("inspect"),
                OsStr::new("repository-does-not-exist"),
                OsStr::new("--format=json"),
            ],
            locale.environment,
        );
        assert_eq!(
            failure.status.code(),
            Some(3),
            "locale: {}",
            locale.environment
        );
        assert!(failure.stdout.is_empty());
        let failure: serde_json::Value =
            serde_json::from_slice(&failure.stderr).expect("localized failure JSON must be valid");
        if let Some(expected) = &failure_baseline {
            assert_eq!(
                &failure, expected,
                "failure JSON changed with locale {}",
                locale.environment
            );
        } else {
            failure_baseline = Some(failure);
        }
    }
}

#[test]
fn exit_codes_are_invariant_across_supported_locales() {
    let root = TestDirectory::new();
    let user = root.path().join("user");

    for locale in SUPPORTED_LOCALE_CASES {
        let usage = isolated_bulls_with_locale(&user, [OsStr::new("unknown")], locale.environment);
        assert_eq!(
            usage.status.code(),
            Some(2),
            "usage exit code changed with locale {}",
            locale.environment
        );

        let expected_failure = isolated_bulls_with_locale(
            &user,
            [
                OsStr::new("inspect"),
                OsStr::new("repository-does-not-exist"),
            ],
            locale.environment,
        );
        assert_eq!(
            expected_failure.status.code(),
            Some(3),
            "expected-failure exit code changed with locale {}",
            locale.environment
        );
    }
}

#[test]
fn all_supported_locales_cover_representative_human_cli_surfaces() {
    let root = TestDirectory::new();
    let user = root.path().join("user");
    let workspace = root.path().join("workspace");
    let repository = workspace.join("repository");
    git_init(&repository);

    let bootstrap = isolated_bulls(&user, [OsStr::new("discover"), workspace.as_os_str()]);
    assert!(bootstrap.status.success());
    let repositories = isolated_bulls(&user, [OsStr::new("repos"), OsStr::new("--format=json")]);
    assert!(repositories.status.success());
    let repositories: serde_json::Value =
        serde_json::from_slice(&repositories.stdout).expect("repository JSON must be valid");
    let repository_id = repositories["payload"]["items"][0]["id"]
        .as_str()
        .expect("repository ID must be present")
        .to_owned();

    for locale in SUPPORTED_LOCALE_CASES {
        let help = isolated_bulls_with_locale(&user, [OsStr::new("--help")], locale.environment);
        assert!(help.status.success(), "locale: {}", locale.environment);
        let help = String::from_utf8(help.stdout).expect("localized help must be UTF-8");
        assert!(
            help.contains(&format!("{}:", locale.usage)),
            "locale: {}",
            locale.environment
        );
        assert!(
            help.contains("  bulls <command>"),
            "locale: {}",
            locale.environment
        );

        let syntax = isolated_bulls_with_locale(&user, [OsStr::new("unknown")], locale.environment);
        assert_eq!(syntax.status.code(), Some(2));
        let syntax = String::from_utf8(syntax.stderr).expect("localized error must be UTF-8");
        assert!(
            syntax.contains(locale.unknown_command),
            "locale: {}",
            locale.environment
        );

        let discovery = isolated_bulls_with_locale(
            &user,
            [OsStr::new("discover"), workspace.as_os_str()],
            locale.environment,
        );
        assert!(discovery.status.success(), "locale: {}", locale.environment);
        let discovery =
            String::from_utf8(discovery.stdout).expect("localized discovery must be UTF-8");
        assert!(
            discovery.contains(locale.discovery),
            "locale: {}",
            locale.environment
        );

        let refresh =
            isolated_bulls_with_locale(&user, [OsStr::new("refresh")], locale.environment);
        assert!(refresh.status.success(), "locale: {}", locale.environment);
        let refresh = String::from_utf8(refresh.stdout).expect("localized refresh must be UTF-8");
        assert!(
            refresh.contains(locale.refresh),
            "locale: {}",
            locale.environment
        );

        let repositories =
            isolated_bulls_with_locale(&user, [OsStr::new("repos")], locale.environment);
        assert!(
            repositories.status.success(),
            "locale: {}",
            locale.environment
        );
        let repositories =
            String::from_utf8(repositories.stdout).expect("localized repos must be UTF-8");
        assert!(
            repositories.contains(locale.repositories),
            "locale: {}",
            locale.environment
        );

        let inspection = isolated_bulls_with_locale(
            &user,
            [OsStr::new("inspect"), OsStr::new(repository_id.as_str())],
            locale.environment,
        );
        assert!(
            inspection.status.success(),
            "locale: {}",
            locale.environment
        );
        let inspection =
            String::from_utf8(inspection.stdout).expect("localized inspection must be UTF-8");
        assert!(
            inspection.contains(&format!("{}{}", locale.repository_prefix, repository_id)),
            "locale: {}",
            locale.environment
        );

        let overview =
            isolated_bulls_with_locale(&user, [OsStr::new("overview")], locale.environment);
        assert!(overview.status.success(), "locale: {}", locale.environment);
        let overview =
            String::from_utf8(overview.stdout).expect("localized overview must be UTF-8");
        assert!(
            overview.contains(locale.overview),
            "locale: {}",
            locale.environment
        );

        let failure = isolated_bulls_with_locale(
            &user,
            [
                OsStr::new("inspect"),
                OsStr::new("repository-does-not-exist"),
            ],
            locale.environment,
        );
        assert_eq!(failure.status.code(), Some(3));
        let failure =
            String::from_utf8(failure.stderr).expect("localized failure report must be UTF-8");
        assert!(
            failure.contains(locale.failure_report),
            "locale: {}",
            locale.environment
        );
    }
}

#[test]
fn unknown_commands_fail_with_usage_semantics() {
    let output = bulls(&["unknown"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("Unknown command: unknown"));
    assert!(stderr.contains("Usage:"));
}

#[test]
fn inspect_without_selector_fails_before_runtime_startup() {
    let output = bulls(&["inspect"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("Usage requires exactly one repository selector."));
    assert!(stderr.contains("Usage: bulls inspect <repository-selector>"));
}

#[test]
fn explicit_discovery_roots_are_transient_and_make_a_clean_repository_refreshable() {
    let root = TestDirectory::new();
    let user = root.path().join("user");
    let workspace = root.path().join("workspace with spaces");
    let repository = workspace.join("repository");
    git_init(&repository);

    let discovery = isolated_bulls(&user, [OsStr::new("discover"), workspace.as_os_str()]);
    assert!(discovery.status.success());
    assert!(discovery.stderr.is_empty());
    let stdout = String::from_utf8(discovery.stdout).unwrap();
    assert!(stdout.contains("Status: complete"));
    assert!(stdout.contains("Roots requested: 1"));
    assert!(stdout.contains("Repositories: 1"));
    assert!(stdout.contains("Worktrees: 1"));

    let refresh = isolated_bulls(&user, [OsStr::new("refresh")]);
    assert!(refresh.status.success());
    assert!(refresh.stderr.is_empty());
    let stdout = String::from_utf8(refresh.stdout).unwrap();
    assert!(stdout.contains("Status: complete"));
    assert!(stdout.contains("Targets: 2"));
    assert!(stdout.contains("Succeeded: 2"));
    assert!(stdout.contains("Failed: 0"));

    let configured_discovery = isolated_bulls(&user, [OsStr::new("discover")]);
    assert_eq!(configured_discovery.status.code(), Some(3));
    assert!(configured_discovery.stderr.is_empty());
    let stdout = String::from_utf8(configured_discovery.stdout).unwrap();
    assert!(stdout.contains("Status: failed"));
    assert!(stdout.contains("Roots requested: 0"));
}

#[test]
fn reset_discards_bullsaddle_state_preserves_configuration_and_leaves_git_untouched() {
    let root = TestDirectory::new();
    let user = root.path().join("user");
    let workspace = root.path().join("workspace");
    let repository = workspace.join("repository");
    git_init(&repository);
    fs::write(repository.join("keep.txt"), "repository-owned data\n")
        .expect("repository fixture must be written");
    write_configuration(&user, "en-US");
    let configuration_path = configuration_dir(&user).join("config.toml");
    let configuration_before =
        fs::read(&configuration_path).expect("configuration fixture must be readable");

    let discovery = isolated_bulls(&user, [OsStr::new("discover"), workspace.as_os_str()]);
    assert!(discovery.status.success());
    let refresh = isolated_bulls(&user, [OsStr::new("refresh")]);
    assert!(refresh.status.success());

    let before = isolated_bulls(&user, [OsStr::new("overview"), OsStr::new("--format=json")]);
    assert!(before.status.success());
    let before: serde_json::Value =
        serde_json::from_slice(&before.stdout).expect("overview before reset must be valid JSON");
    assert_eq!(before["payload"]["overview"]["repository_count"], 1);
    let revision_before = before["workspace_revision"]
        .as_u64()
        .expect("workspace revision must be numeric");

    let reset = isolated_bulls(&user, [OsStr::new("reset")]);
    assert!(reset.status.success());
    assert!(reset.stderr.is_empty());
    let reset_stdout = String::from_utf8(reset.stdout).expect("reset output must be UTF-8");
    assert!(reset_stdout.contains("Workspace reset"));
    assert!(reset_stdout.contains("Status: complete"));
    assert!(reset_stdout.contains("User configuration was preserved."));

    let after = isolated_bulls(&user, [OsStr::new("overview"), OsStr::new("--format=json")]);
    assert!(after.status.success());
    let after: serde_json::Value =
        serde_json::from_slice(&after.stdout).expect("overview after reset must be valid JSON");
    assert_eq!(after["payload"]["overview"]["repository_count"], 0);
    assert_eq!(after["payload"]["overview"]["worktree_count"], 0);
    let revision_after = after["workspace_revision"]
        .as_u64()
        .expect("workspace revision must be numeric");
    assert!(revision_after > revision_before);

    let repeated = isolated_bulls(&user, [OsStr::new("reset")]);
    assert!(repeated.status.success());
    let after_repeated =
        isolated_bulls(&user, [OsStr::new("overview"), OsStr::new("--format=json")]);
    let after_repeated: serde_json::Value = serde_json::from_slice(&after_repeated.stdout)
        .expect("overview after repeated reset must be valid JSON");
    assert_eq!(after_repeated["workspace_revision"], revision_after);

    assert_eq!(
        fs::read(&configuration_path).expect("configuration must remain readable"),
        configuration_before
    );
    assert!(repository.join(".git").is_dir());
    assert_eq!(
        fs::read_to_string(repository.join("keep.txt")).expect("repository data must remain"),
        "repository-owned data\n"
    );
}

#[test]
fn inspect_accepts_stable_id_absolute_path_and_current_directory_selectors() {
    let root = TestDirectory::new();
    let user = root.path().join("user");
    let workspace = root.path().join("workspace");
    let repository = workspace.join("repository");
    git_init(&repository);

    let discovery = isolated_bulls(&user, [OsStr::new("discover"), workspace.as_os_str()]);
    assert!(discovery.status.success());

    let repositories = isolated_bulls(&user, [OsStr::new("repos"), OsStr::new("--format=json")]);
    assert!(repositories.status.success());
    let repositories: serde_json::Value =
        serde_json::from_slice(&repositories.stdout).expect("repository JSON must be valid");
    let repository_id = repositories["payload"]["items"][0]["id"]
        .as_str()
        .expect("repository ID must be present")
        .to_owned();

    let by_id = isolated_bulls(
        &user,
        [
            OsStr::new("inspect"),
            OsStr::new(repository_id.as_str()),
            OsStr::new("--format=json"),
        ],
    );
    let by_path = isolated_bulls(
        &user,
        [
            OsStr::new("inspect"),
            repository.as_os_str(),
            OsStr::new("--format=json"),
        ],
    );
    let by_current_directory = isolated_command(&user, "C")
        .current_dir(&repository)
        .args(["inspect", ".", "--format=json"])
        .output()
        .expect("inspect from repository directory must start");

    for output in [&by_id, &by_path, &by_current_directory] {
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        let document: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("inspect JSON must be valid");
        assert_eq!(
            document["payload"]["repository"]["id"].as_str(),
            Some(repository_id.as_str())
        );
    }
}

#[test]
fn inspect_path_selector_uses_persisted_locations_without_requiring_the_path_to_exist() {
    let root = TestDirectory::new();
    let user = root.path().join("user");
    let workspace = root.path().join("workspace");
    let repository = workspace.join("repository");
    git_init(&repository);

    let discovery = isolated_bulls(&user, [OsStr::new("discover"), workspace.as_os_str()]);
    assert!(discovery.status.success());
    fs::remove_dir_all(&repository).expect("repository fixture must be removable");

    let inspection = isolated_bulls(
        &user,
        [
            OsStr::new("inspect"),
            repository.as_os_str(),
            OsStr::new("--format=json"),
        ],
    );

    assert!(inspection.status.success());
    assert!(inspection.stderr.is_empty());
    let document: serde_json::Value =
        serde_json::from_slice(&inspection.stdout).expect("inspect JSON must be valid");
    assert_eq!(
        document["payload"]["repository"]["locations"][0]["path"]["value"],
        repository.to_string_lossy().as_ref()
    );
}

#[test]
fn discovery_preserves_completed_work_and_reports_failed_roots_as_partial() {
    let root = TestDirectory::new();
    let user = root.path().join("user");
    let workspace = root.path().join("workspace");
    let repository = workspace.join("repository");
    let missing = root.path().join("missing");
    git_init(&repository);

    let output = isolated_bulls(
        &user,
        [
            OsStr::new("discover"),
            workspace.as_os_str(),
            missing.as_os_str(),
        ],
    );

    assert_eq!(output.status.code(), Some(3));
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Status: partial"));
    assert!(stdout.contains("Roots requested: 2"));
    assert!(stdout.contains("Roots completed: 1"));
    assert!(stdout.contains("Roots failed: 1"));
    assert!(stdout.contains("Repositories: 1"));
    assert!(
        !stdout.contains(missing.to_string_lossy().as_ref()),
        "default discovery output must not expose failed root paths"
    );

    let verbose = isolated_bulls(
        &user,
        [
            OsStr::new("discover"),
            OsStr::new("--verbose"),
            workspace.as_os_str(),
            missing.as_os_str(),
        ],
    );
    assert_eq!(verbose.status.code(), Some(3));
    assert!(verbose.stderr.is_empty());
    let verbose_stdout = String::from_utf8(verbose.stdout).unwrap();
    assert!(verbose_stdout.contains("Details:"));
    assert!(verbose_stdout.contains("Failed root:"));
    assert!(verbose_stdout.contains(missing.to_string_lossy().as_ref()));
    assert!(verbose_stdout.contains("Cause:"));
}

#[test]
fn discovery_reports_partial_coverage_inside_a_completed_root() {
    let root = TestDirectory::new();
    let user = root.path().join("user");
    let workspace = root.path().join("workspace");
    let repository = workspace.join("repository");
    let invalid_repository = workspace.join("invalid-repository");
    git_init(&repository);
    fs::create_dir_all(&invalid_repository).expect("invalid repository directory must be created");
    fs::write(invalid_repository.join(".git"), b"not-a-gitdir\n")
        .expect("invalid Git marker must be written");

    let output = isolated_bulls(&user, [OsStr::new("discover"), workspace.as_os_str()]);

    assert_eq!(output.status.code(), Some(3));
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Status: partial"));
    assert!(stdout.contains("Roots requested: 1"));
    assert!(stdout.contains("Roots completed: 1"));
    assert!(stdout.contains("Roots partial: 1"));
    assert!(stdout.contains("Roots failed: 0"));
    assert!(stdout.contains("Identification issues: 1"));
    assert!(stdout.contains("Repositories: 1"));
    assert!(
        !stdout.contains(invalid_repository.to_string_lossy().as_ref()),
        "default discovery output must not expose issue paths"
    );

    let verbose = isolated_bulls(
        &user,
        [
            OsStr::new("discover"),
            OsStr::new("--verbose"),
            workspace.as_os_str(),
        ],
    );
    assert_eq!(verbose.status.code(), Some(3));
    assert!(verbose.stderr.is_empty());
    let verbose_stdout = String::from_utf8(verbose.stdout).unwrap();
    assert!(verbose_stdout.contains("Identification issue:"));
    assert!(verbose_stdout.contains(invalid_repository.to_string_lossy().as_ref()));
    assert!(verbose_stdout.contains("Cause:"));
}

#[test]
fn operational_json_exposes_versioned_discovery_and_refresh_contracts() {
    let root = TestDirectory::new();
    let user = root.path().join("user");
    let workspace = root.path().join("workspace");
    let repository = workspace.join("repository");
    git_init(&repository);

    let discovery = isolated_bulls(
        &user,
        [
            OsStr::new("discover"),
            workspace.as_os_str(),
            OsStr::new("--format=json"),
        ],
    );
    assert!(discovery.status.success());
    assert!(discovery.stderr.is_empty());
    let discovery: serde_json::Value =
        serde_json::from_slice(&discovery.stdout).expect("discovery JSON must be valid");
    assert_eq!(discovery["schema_version"], 1);
    assert_eq!(discovery["status"], "complete");
    assert!(discovery["workspace_revision"].is_u64());
    assert_eq!(discovery["payload"]["requested_root_count"], 1);
    assert_eq!(discovery["payload"]["completed_root_count"], 1);
    assert_eq!(discovery["payload"]["failed_root_count"], 0);
    assert_eq!(discovery["payload"]["repository_count"], 1);
    assert_eq!(discovery["payload"]["roots"][0]["root"]["encoding"], "utf8");
    assert_eq!(
        discovery["payload"]["roots"][0]["root"]["value"],
        workspace.to_string_lossy().as_ref()
    );

    let refresh = isolated_bulls(
        &user,
        [
            OsStr::new("refresh"),
            OsStr::new("--format"),
            OsStr::new("json"),
        ],
    );
    assert!(refresh.status.success());
    assert!(refresh.stderr.is_empty());
    let refresh: serde_json::Value =
        serde_json::from_slice(&refresh.stdout).expect("refresh JSON must be valid");
    assert_eq!(refresh["schema_version"], 1);
    assert_eq!(refresh["status"], "complete");
    assert!(refresh["workspace_revision"].is_u64());
    assert_eq!(refresh["payload"]["coverage"], "complete");
    assert_eq!(refresh["payload"]["target_count"], 2);
    assert_eq!(refresh["payload"]["succeeded_count"], 2);
    assert_eq!(refresh["payload"]["failed_count"], 0);
    assert_eq!(refresh["payload"]["cancelled_count"], 0);
    assert_eq!(refresh["payload"]["failures"].as_array().unwrap().len(), 0);
}

#[test]
fn discovery_json_preserves_partial_and_failed_completion_semantics() {
    let root = TestDirectory::new();
    let user = root.path().join("user");
    let workspace = root.path().join("workspace");
    let repository = workspace.join("repository");
    let missing = root.path().join("missing");
    git_init(&repository);

    let partial = isolated_bulls(
        &user,
        [
            OsStr::new("discover"),
            workspace.as_os_str(),
            missing.as_os_str(),
            OsStr::new("--format=json"),
        ],
    );
    assert_eq!(partial.status.code(), Some(3));
    assert!(partial.stderr.is_empty());
    let partial: serde_json::Value =
        serde_json::from_slice(&partial.stdout).expect("partial discovery JSON must be valid");
    assert_eq!(partial["status"], "partial");
    assert_eq!(partial["payload"]["completed_root_count"], 1);
    assert_eq!(partial["payload"]["failed_root_count"], 1);
    assert_eq!(
        partial["payload"]["failed_roots"][0]["root"]["value"],
        missing.to_string_lossy().as_ref()
    );
    assert!(
        partial["payload"]["failed_roots"][0]["cause"]["kind"]
            .as_str()
            .is_some()
    );

    let failed = isolated_bulls(
        &user,
        [
            OsStr::new("discover"),
            missing.as_os_str(),
            OsStr::new("--format=json"),
        ],
    );
    assert_eq!(failed.status.code(), Some(4));
    assert!(failed.stderr.is_empty());
    let failed: serde_json::Value =
        serde_json::from_slice(&failed.stdout).expect("failed discovery JSON must be valid");
    assert_eq!(failed["status"], "failed");
    assert_eq!(failed["payload"]["completed_root_count"], 0);
    assert_eq!(failed["payload"]["failed_root_count"], 1);
}

#[cfg(unix)]
#[test]
fn sigint_cancels_refresh_terminates_git_and_returns_130() {
    let root = TestDirectory::new();
    let user = root.path().join("user");
    let workspace = root.path().join("workspace");
    let repository = workspace.join("repository");
    let fake_bin = root.path().join("fake-bin");
    let ready = root.path().join("git-ready");
    let git_pid_file = root.path().join("git-pid");
    git_init(&repository);
    let discovery = isolated_bulls(&user, [OsStr::new("discover"), workspace.as_os_str()]);
    assert!(discovery.status.success());
    write_configuration_with_parallelism(&user, "en-US", 1);
    fs::create_dir_all(&fake_bin).expect("fake binary directory must be created");

    let fake_git = fake_bin.join("git");
    fs::write(
        &fake_git,
        "#!/bin/sh\nprintf '%s\\n' \"$$\" > \"$BULLS_SIGNAL_TEST_PID\"\n: > \"$BULLS_SIGNAL_TEST_READY\"\nexec sleep 30\n",
    )
    .expect("fake Git executable must be written");
    let mut permissions = fs::metadata(&fake_git)
        .expect("fake Git metadata must be available")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_git, permissions).expect("fake Git must become executable");

    let mut path_entries = vec![fake_bin];
    if let Some(path) = std::env::var_os("PATH") {
        path_entries.extend(std::env::split_paths(&path));
    }
    let path = std::env::join_paths(path_entries).expect("test PATH must be representable");

    let mut child = isolated_command(&user, "C")
        .arg("refresh")
        .env("PATH", path)
        .env("BULLS_SIGNAL_TEST_READY", &ready)
        .env("BULLS_SIGNAL_TEST_PID", &git_pid_file)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("BullSaddle refresh process must start");

    let deadline = Instant::now() + Duration::from_secs(3);
    while !ready.exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    if !ready.exists() {
        let _ = child.kill();
        let _ = child.wait();
        panic!("fake Git process did not start before the test deadline");
    }

    let bulls_pid = child.id().to_string();
    let interrupt = Command::new("kill")
        .args(["-INT", bulls_pid.as_str()])
        .status()
        .expect("SIGINT command must start");
    assert!(
        interrupt.success(),
        "SIGINT must be delivered to BullSaddle"
    );

    let output = child
        .wait_with_output()
        .expect("interrupted BullSaddle process must terminate");
    assert_eq!(output.status.code(), Some(130));
    let stdout = String::from_utf8(output.stdout).expect("stdout must be UTF-8");
    assert!(stdout.contains("Status: cancelled"));
    assert!(stdout.contains("Targets: 2"));
    assert!(stdout.contains("Succeeded: 0"));
    assert!(stdout.contains("Failed: 2"));
    assert!(output.stderr.is_empty());

    let git_pid = fs::read_to_string(&git_pid_file)
        .expect("fake Git PID must be recorded")
        .trim()
        .to_owned();
    let git_alive = Command::new("kill")
        .args(["-0", git_pid.as_str()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("process liveness check must start")
        .success();
    assert!(
        !git_alive,
        "cancelled Git process must not survive BullSaddle"
    );
}

#[cfg(unix)]
#[test]
fn sigint_during_discovery_probe_terminates_git_and_returns_130() {
    let root = TestDirectory::new();
    let user = root.path().join("user");
    let workspace = root.path().join("workspace");
    let repository = workspace.join("repository");
    let fake_bin = root.path().join("fake-bin");
    let ready = root.path().join("git-ready");
    let git_pid_file = root.path().join("git-pid");
    fs::create_dir_all(repository.join(".git")).expect("repository marker must be created");
    fs::create_dir_all(&fake_bin).expect("fake binary directory must be created");

    let fake_git = fake_bin.join("git");
    fs::write(
        &fake_git,
        "#!/bin/sh\nprintf '%s\n' \"$$\" > \"$BULLS_SIGNAL_TEST_PID\"\n: > \"$BULLS_SIGNAL_TEST_READY\"\nexec sleep 30\n",
    )
    .expect("fake Git executable must be written");
    let mut permissions = fs::metadata(&fake_git)
        .expect("fake Git metadata must be available")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_git, permissions).expect("fake Git must become executable");

    let mut path_entries = vec![fake_bin];
    if let Some(path) = std::env::var_os("PATH") {
        path_entries.extend(std::env::split_paths(&path));
    }
    let path = std::env::join_paths(path_entries).expect("test PATH must be representable");

    let mut child = isolated_command(&user, "C")
        .arg("discover")
        .arg(&workspace)
        .arg("--format=json")
        .env("PATH", path)
        .env("BULLS_SIGNAL_TEST_READY", &ready)
        .env("BULLS_SIGNAL_TEST_PID", &git_pid_file)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("BullSaddle discovery process must start");

    let deadline = Instant::now() + Duration::from_secs(3);
    while !ready.exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    if !ready.exists() {
        let _ = child.kill();
        let _ = child.wait();
        panic!("fake Git process did not start before the test deadline");
    }

    let bulls_pid = child.id().to_string();
    let interrupt = Command::new("kill")
        .args(["-INT", bulls_pid.as_str()])
        .status()
        .expect("SIGINT command must start");
    assert!(
        interrupt.success(),
        "SIGINT must be delivered to BullSaddle"
    );

    let output = child
        .wait_with_output()
        .expect("interrupted BullSaddle process must terminate");
    assert_eq!(output.status.code(), Some(130));
    assert!(output.stdout.is_empty());
    let failure: serde_json::Value =
        serde_json::from_slice(&output.stderr).expect("cancelled discovery error must be JSON");
    assert_eq!(failure["failure"]["class"], "expected");
    assert_eq!(failure["failure"]["error_code"], "operation_cancelled");
    assert_eq!(failure["failure"]["causes"][0]["code"], "cancelled");

    let git_pid = fs::read_to_string(&git_pid_file)
        .expect("fake Git PID must be recorded")
        .trim()
        .to_owned();
    let git_alive = Command::new("kill")
        .args(["-0", git_pid.as_str()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("process liveness check must start")
        .success();
    assert!(
        !git_alive,
        "cancelled discovery Git process must not survive BullSaddle"
    );
}

#[test]
fn refresh_total_target_failure_is_failed_in_human_and_json_output() {
    let root = TestDirectory::new();
    let user = root.path().join("user");
    let workspace = root.path().join("workspace");
    let repository = workspace.join("repository");
    git_init(&repository);

    let discovery = isolated_bulls(&user, [OsStr::new("discover"), workspace.as_os_str()]);
    assert!(discovery.status.success());

    fs::remove_dir_all(&repository).expect("repository fixture must be removed");
    let refresh = isolated_bulls(&user, [OsStr::new("refresh")]);

    assert_eq!(refresh.status.code(), Some(4));
    assert!(refresh.stderr.is_empty());
    let stdout = String::from_utf8(refresh.stdout).unwrap();
    assert!(stdout.contains("Status: failed"));
    assert!(stdout.contains("Targets: 2"));
    assert!(stdout.contains("Succeeded: 0"));
    assert!(stdout.contains("Failed: 2"));

    let structured = isolated_bulls(&user, [OsStr::new("refresh"), OsStr::new("--format=json")]);
    assert_eq!(structured.status.code(), Some(4));
    assert!(structured.stderr.is_empty());
    let structured: serde_json::Value =
        serde_json::from_slice(&structured.stdout).expect("failed refresh JSON must be valid");
    assert_eq!(structured["status"], "failed");
    assert_eq!(structured["payload"]["coverage"], "partial");
    assert_eq!(structured["payload"]["target_count"], 2);
    assert_eq!(structured["payload"]["succeeded_count"], 0);
    assert_eq!(structured["payload"]["failed_count"], 2);
    assert_eq!(
        structured["payload"]["failures"].as_array().unwrap().len(),
        2
    );
}

#[test]
fn first_run_human_output_explains_unobserved_state_without_refreshing_implicitly() {
    let root = TestDirectory::new();
    let user = root.path().join("user");
    let workspace = root.path().join("workspace");
    let repository = workspace.join("repository");
    git_init(&repository);

    let discovery = isolated_bulls(&user, [OsStr::new("discover"), workspace.as_os_str()]);
    assert!(discovery.status.success());
    assert!(discovery.stderr.is_empty());
    let discovery_stdout = String::from_utf8(discovery.stdout).unwrap();
    assert!(discovery_stdout.contains("Hint: run `bulls refresh`"));

    let repositories = isolated_bulls(&user, [OsStr::new("repos")]);
    assert!(repositories.status.success());
    assert!(repositories.stderr.is_empty());
    let repositories_stdout = String::from_utf8(repositories.stdout).unwrap();
    assert!(repositories_stdout.contains("Local work: unknown"));
    assert!(repositories_stdout.contains("Remotes: unknown"));
    assert!(repositories_stdout.contains("Hint: run `bulls refresh`"));
    let repository_id = repositories_stdout
        .lines()
        .find_map(|line| line.strip_prefix("Repository: "))
        .expect("repository list must expose a repository ID")
        .to_owned();

    let inspection = isolated_bulls(
        &user,
        [OsStr::new("inspect"), OsStr::new(repository_id.as_str())],
    );
    assert!(inspection.status.success());
    assert!(inspection.stderr.is_empty());
    let inspection_stdout = String::from_utf8(inspection.stdout).unwrap();
    assert!(inspection_stdout.contains("Remotes: unknown — never observed"));
    assert!(inspection_stdout.contains("Git state: unknown — never observed"));
    assert!(inspection_stdout.contains("Hint: run `bulls refresh`"));
    assert!(!inspection_stdout.contains("Workspace revision:"));

    let overview = isolated_bulls(&user, [OsStr::new("overview")]);
    assert!(overview.status.success());
    assert!(overview.stderr.is_empty());
    let overview_stdout = String::from_utf8(overview.stdout).unwrap();
    assert!(overview_stdout.contains("Repositories with unknown local work: 1"));
    assert!(overview_stdout.contains("Repositories with unknown remotes: 1"));
    assert!(overview_stdout.contains("Hint: run `bulls refresh`"));
    assert!(!overview_stdout.contains("Workspace revision:"));
}

#[test]
fn repository_queries_expose_locations_pagination_and_advisories_without_refreshing() {
    let root = TestDirectory::new();
    let user = root.path().join("user");
    let workspace = root.path().join("workspace");
    let repository_a = workspace.join("repository-a");
    let repository_b = workspace.join("repository-b");
    git_init(&repository_a);
    git_init(&repository_b);

    let discovery = isolated_bulls(&user, [OsStr::new("discover"), workspace.as_os_str()]);
    assert!(discovery.status.success());
    let refresh = isolated_bulls(&user, [OsStr::new("refresh")]);
    assert!(refresh.status.success());

    let page = isolated_bulls(
        &user,
        [
            OsStr::new("repos"),
            OsStr::new("--offset"),
            OsStr::new("1"),
            OsStr::new("--limit"),
            OsStr::new("1"),
        ],
    );
    assert!(page.status.success());
    assert!(page.stderr.is_empty());
    let page_stdout = String::from_utf8(page.stdout).unwrap();
    assert_eq!(page_stdout.matches("Repository: ").count(), 1);
    assert!(page_stdout.contains("[available]"));
    assert!(
        page_stdout.contains("Showing 1 repositories from offset 1 of 2 matching repositories")
    );

    let repositories = isolated_bulls(&user, [OsStr::new("repos")]);
    assert!(repositories.status.success());
    let repositories_stdout = String::from_utf8(repositories.stdout).unwrap();
    assert!(repositories_stdout.contains(repository_a.to_string_lossy().as_ref()));
    assert!(repositories_stdout.contains(repository_b.to_string_lossy().as_ref()));
    let repository_id = repositories_stdout
        .lines()
        .find_map(|line| line.strip_prefix("Repository: "))
        .expect("repository list must expose a repository ID")
        .to_owned();

    let inspection = isolated_bulls(
        &user,
        [OsStr::new("inspect"), OsStr::new(repository_id.as_str())],
    );
    assert!(inspection.status.success());
    assert!(inspection.stderr.is_empty());
    let inspection_stdout = String::from_utf8(inspection.stdout).unwrap();
    assert!(!inspection_stdout.contains("Workspace revision: "));
    assert!(inspection_stdout.contains("HEAD: unborn"));
    assert!(inspection_stdout.contains("Observation: complete — "));
    assert!(inspection_stdout.contains("Upstream: unconfigured"));
    assert!(inspection_stdout.contains("Advisories: "));
    assert!(inspection_stdout.contains("no remotes are known for this repository"));

    let overview = isolated_bulls(&user, [OsStr::new("overview")]);
    assert!(overview.status.success());
    assert!(overview.stderr.is_empty());
    let overview_stdout = String::from_utf8(overview.stdout).unwrap();
    assert!(!overview_stdout.contains("Workspace revision: "));
    assert!(overview_stdout.contains("Repositories: 2"));
    assert!(overview_stdout.contains("Advisories: "));
    assert!(overview_stdout.contains("Repositories without remotes: 2"));
}

#[test]
fn inspection_distinguishes_latest_failure_from_last_successful_observation() {
    let root = TestDirectory::new();
    let user = root.path().join("user");
    let workspace = root.path().join("workspace");
    let repository = workspace.join("repository");
    git_init(&repository);

    let discovery = isolated_bulls(&user, [OsStr::new("discover"), workspace.as_os_str()]);
    assert!(discovery.status.success());
    let refresh = isolated_bulls(&user, [OsStr::new("refresh")]);
    assert!(refresh.status.success());

    let repositories = isolated_bulls(&user, [OsStr::new("repos"), OsStr::new("--format=json")]);
    assert!(repositories.status.success());
    let repositories: serde_json::Value =
        serde_json::from_slice(&repositories.stdout).expect("repository JSON must be valid");
    let repository_id = repositories["payload"]["items"][0]["id"]
        .as_str()
        .expect("repository ID must be present")
        .to_owned();

    fs::remove_dir_all(&repository).expect("repository fixture must be removable");
    let failed_refresh = isolated_bulls(&user, [OsStr::new("refresh")]);
    assert_eq!(failed_refresh.status.code(), Some(4));

    let inspection = isolated_bulls(
        &user,
        [OsStr::new("inspect"), OsStr::new(repository_id.as_str())],
    );
    assert!(inspection.status.success());
    assert!(inspection.stderr.is_empty());
    let inspection_stdout = String::from_utf8(inspection.stdout).unwrap();
    assert!(inspection_stdout.contains("Observation: failed:"));
    assert!(inspection_stdout.contains("last successful observation"));
    assert!(inspection_stdout.contains("HEAD: unborn"));
}

#[test]
fn query_json_reuses_the_versioned_read_protocol_and_keeps_advisories_structured() {
    let root = TestDirectory::new();
    let user = root.path().join("user");
    let workspace = root.path().join("workspace");
    let repository = workspace.join("repository");
    git_init(&repository);

    let discovery = isolated_bulls(&user, [OsStr::new("discover"), workspace.as_os_str()]);
    assert!(discovery.status.success());
    let refresh = isolated_bulls(&user, [OsStr::new("refresh")]);
    assert!(refresh.status.success());

    let repositories = isolated_bulls(
        &user,
        [
            OsStr::new("repos"),
            OsStr::new("--limit"),
            OsStr::new("1"),
            OsStr::new("--format"),
            OsStr::new("json"),
        ],
    );
    assert!(repositories.status.success());
    assert!(repositories.stderr.is_empty());
    let repositories_json: serde_json::Value =
        serde_json::from_slice(&repositories.stdout).expect("repository JSON must be valid");
    assert_eq!(repositories_json["schema_version"], 1);
    assert_eq!(
        repositories_json["payload"]["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let repository_id = repositories_json["payload"]["items"][0]["id"]
        .as_str()
        .expect("repository protocol must expose an ID")
        .to_owned();

    let inspection = isolated_bulls(
        &user,
        [
            OsStr::new("inspect"),
            OsStr::new(repository_id.as_str()),
            OsStr::new("--format=json"),
        ],
    );
    assert!(inspection.status.success());
    assert!(inspection.stderr.is_empty());
    let inspection_json: serde_json::Value =
        serde_json::from_slice(&inspection.stdout).expect("inspect JSON must be valid");
    assert_eq!(inspection_json["schema_version"], 1);
    assert_eq!(
        inspection_json["payload"]["repository"]["id"],
        repository_id
    );
    assert!(inspection_json["payload"]["advisories"]["advisories"].is_array());
    assert_eq!(
        inspection_json["workspace_revision"],
        repositories_json["workspace_revision"]
    );

    let overview = isolated_bulls(
        &user,
        [
            OsStr::new("overview"),
            OsStr::new("--format"),
            OsStr::new("json"),
        ],
    );
    assert!(overview.status.success());
    assert!(overview.stderr.is_empty());
    let overview_json: serde_json::Value =
        serde_json::from_slice(&overview.stdout).expect("overview JSON must be valid");
    assert_eq!(overview_json["schema_version"], 1);
    assert_eq!(overview_json["payload"]["overview"]["repository_count"], 1);
    assert!(overview_json["payload"]["advisories"]["advisories"].is_array());
    assert_eq!(
        overview_json["workspace_revision"],
        repositories_json["workspace_revision"]
    );

    let overview_text = String::from_utf8(overview.stdout).unwrap();
    assert!(!overview_text.contains("Workspace revision:"));
    assert!(!overview_text.contains("Repositories without remotes:"));
}

#[test]
fn json_failures_use_the_privacy_safe_public_failure_report_on_stderr() {
    let root = TestDirectory::new();
    let user = root.path().join("user");
    let private_identifier = "private-repository-identifier";

    let output = isolated_bulls(
        &user,
        [
            OsStr::new("inspect"),
            OsStr::new(private_identifier),
            OsStr::new("--format"),
            OsStr::new("json"),
        ],
    );

    assert_eq!(output.status.code(), Some(3));
    assert!(output.stdout.is_empty());
    let report: serde_json::Value =
        serde_json::from_slice(&output.stderr).expect("failure report must be valid JSON");
    assert_eq!(report["schema_version"], 2);
    assert_eq!(report["failure"]["class"], "expected");
    assert_eq!(report["failure"]["error_code"], "repository_not_found");
    assert_eq!(report["failure"]["causes"].as_array().unwrap().len(), 0);
    assert!(
        !String::from_utf8(output.stderr)
            .unwrap()
            .contains(private_identifier)
    );
}

#[test]
fn verbose_discovery_is_human_only_and_fails_before_runtime_startup() {
    let root = TestDirectory::new();
    let user = root.path().join("user");

    let output = isolated_bulls(
        &user,
        [
            OsStr::new("discover"),
            OsStr::new("--verbose"),
            OsStr::new("--format=json"),
        ],
    );

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("--verbose"));
    assert!(stderr.contains("human output"));
    assert!(stderr.contains("Usage: bulls discover"));
    assert!(
        !data_dir(&user).join("bulls.sqlite").exists(),
        "invalid presentation options must fail before runtime startup"
    );
}
