// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use std::env;
use std::path::PathBuf;

use bulls_application::ports::{
    PlatformPaths, PlatformPathsPort, PortError, PortErrorKind, PortResult,
};

const APPLICATION_DIRECTORY: &str = "bulls";

#[derive(Clone, Copy, Debug, Default)]
pub struct NativePlatformPaths;

impl NativePlatformPaths {
    pub const fn new() -> Self {
        Self
    }
}

impl PlatformPathsPort for NativePlatformPaths {
    fn user_paths(&self) -> PortResult<PlatformPaths> {
        resolve_user_paths()
    }
}

fn environment_path(name: &str) -> Option<PathBuf> {
    absolute_path(PathBuf::from(env::var_os(name)?))
}

fn absolute_path(path: PathBuf) -> Option<PathBuf> {
    // Caminhos relativos fariam o diretório de execução participar do estado do usuário.
    path.is_absolute().then_some(path)
}

fn unavailable_paths() -> PortError {
    PortError::new(PortErrorKind::ResourceUnavailable)
}

#[cfg(any(
    test,
    target_os = "linux",
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "dragonfly"
))]
fn resolve_xdg_paths(
    config_home: Option<PathBuf>,
    data_home: Option<PathBuf>,
    state_home: Option<PathBuf>,
    cache_home: Option<PathBuf>,
    home: Option<PathBuf>,
) -> PortResult<PlatformPaths> {
    let config_dir = config_home
        .or_else(|| home.as_ref().map(|path| path.join(".config")))
        .ok_or_else(unavailable_paths)?
        .join(APPLICATION_DIRECTORY);
    let data_dir = data_home
        .or_else(|| home.as_ref().map(|path| path.join(".local/share")))
        .ok_or_else(unavailable_paths)?
        .join(APPLICATION_DIRECTORY);
    let state_dir = state_home
        .or_else(|| home.as_ref().map(|path| path.join(".local/state")))
        .ok_or_else(unavailable_paths)?
        .join(APPLICATION_DIRECTORY);
    let cache_dir = cache_home
        .or_else(|| home.as_ref().map(|path| path.join(".cache")))
        .ok_or_else(unavailable_paths)?
        .join(APPLICATION_DIRECTORY);

    Ok(PlatformPaths::new(
        config_dir, data_dir, state_dir, cache_dir,
    ))
}

#[cfg(any(test, target_os = "macos"))]
fn resolve_macos_paths(home: Option<PathBuf>) -> PortResult<PlatformPaths> {
    let home = home.ok_or_else(unavailable_paths)?;
    let application_support = home
        .join("Library")
        .join("Application Support")
        .join(APPLICATION_DIRECTORY);
    let cache_dir = home
        .join("Library")
        .join("Caches")
        .join(APPLICATION_DIRECTORY);

    Ok(PlatformPaths::new(
        application_support.clone(),
        application_support.clone(),
        application_support,
        cache_dir,
    ))
}

#[cfg(any(test, target_os = "windows"))]
fn resolve_windows_paths(
    app_data: Option<PathBuf>,
    local_app_data: Option<PathBuf>,
    user_profile: Option<PathBuf>,
) -> PortResult<PlatformPaths> {
    let roaming_base = app_data
        .or_else(|| {
            user_profile
                .as_ref()
                .map(|path| path.join("AppData").join("Roaming"))
        })
        .ok_or_else(unavailable_paths)?;
    let local_base = local_app_data
        .or_else(|| {
            user_profile
                .as_ref()
                .map(|path| path.join("AppData").join("Local"))
        })
        .ok_or_else(unavailable_paths)?;

    let config_dir = roaming_base.join(APPLICATION_DIRECTORY);
    let local_dir = local_base.join(APPLICATION_DIRECTORY);

    Ok(PlatformPaths::new(
        config_dir,
        local_dir.clone(),
        local_dir.clone(),
        local_dir,
    ))
}

#[cfg(any(
    target_os = "linux",
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "dragonfly"
))]
fn resolve_user_paths() -> PortResult<PlatformPaths> {
    resolve_xdg_paths(
        environment_path("XDG_CONFIG_HOME"),
        environment_path("XDG_DATA_HOME"),
        environment_path("XDG_STATE_HOME"),
        environment_path("XDG_CACHE_HOME"),
        environment_path("HOME"),
    )
}

#[cfg(target_os = "macos")]
fn resolve_user_paths() -> PortResult<PlatformPaths> {
    resolve_macos_paths(environment_path("HOME"))
}

#[cfg(target_os = "windows")]
fn resolve_user_paths() -> PortResult<PlatformPaths> {
    resolve_windows_paths(
        environment_path("APPDATA"),
        environment_path("LOCALAPPDATA"),
        environment_path("USERPROFILE"),
    )
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "dragonfly",
    target_os = "macos",
    target_os = "windows"
)))]
fn resolve_user_paths() -> PortResult<PlatformPaths> {
    Err(unavailable_paths())
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use bulls_application::ports::PortErrorKind;

    use super::{absolute_path, resolve_macos_paths, resolve_windows_paths, resolve_xdg_paths};

    #[test]
    fn xdg_paths_prefer_explicit_platform_directories() {
        let paths = resolve_xdg_paths(
            Some("/xdg/config".into()),
            Some("/xdg/data".into()),
            Some("/xdg/state".into()),
            Some("/xdg/cache".into()),
            None,
        )
        .expect("explicit XDG directories must be sufficient");

        assert_eq!(paths.config_dir(), Path::new("/xdg/config/bulls"));
        assert_eq!(paths.data_dir(), Path::new("/xdg/data/bulls"));
        assert_eq!(paths.state_dir(), Path::new("/xdg/state/bulls"));
        assert_eq!(paths.cache_dir(), Path::new("/xdg/cache/bulls"));
    }

    #[test]
    fn xdg_paths_fall_back_to_home_without_using_current_directory() {
        let paths = resolve_xdg_paths(None, None, None, None, Some("/home/tester".into()))
            .expect("home must provide all XDG fallbacks");

        assert_eq!(paths.config_dir(), Path::new("/home/tester/.config/bulls"));
        assert_eq!(
            paths.data_dir(),
            Path::new("/home/tester/.local/share/bulls")
        );
        assert_eq!(
            paths.state_dir(),
            Path::new("/home/tester/.local/state/bulls")
        );
        assert_eq!(paths.cache_dir(), Path::new("/home/tester/.cache/bulls"));
    }

    #[test]
    fn xdg_paths_fail_when_no_absolute_user_base_exists() {
        let error = resolve_xdg_paths(None, None, None, None, None)
            .expect_err("missing user directories must not fall back to the current directory");

        assert_eq!(error.kind(), PortErrorKind::ResourceUnavailable);
    }

    #[test]
    fn macos_paths_keep_application_state_outside_the_source_tree() {
        let paths = resolve_macos_paths(Some("/Users/tester".into()))
            .expect("home must resolve macOS user directories");

        let application_support = Path::new("/Users/tester/Library/Application Support/bulls");
        assert_eq!(paths.config_dir(), application_support);
        assert_eq!(paths.data_dir(), application_support);
        assert_eq!(paths.state_dir(), application_support);
        assert_eq!(
            paths.cache_dir(),
            Path::new("/Users/tester/Library/Caches/bulls")
        );
    }

    #[test]
    fn windows_paths_separate_roaming_configuration_from_local_state() {
        let paths = resolve_windows_paths(Some("/roaming".into()), Some("/local".into()), None)
            .expect("Windows application data directories must resolve");

        assert_eq!(paths.config_dir(), Path::new("/roaming/bulls"));
        assert_eq!(paths.data_dir(), Path::new("/local/bulls"));
        assert_eq!(paths.state_dir(), Path::new("/local/bulls"));
        assert_eq!(paths.cache_dir(), Path::new("/local/bulls"));
    }

    #[test]
    fn relative_environment_paths_are_rejected() {
        assert_eq!(absolute_path(PathBuf::from("relative/path")), None);
    }
}
