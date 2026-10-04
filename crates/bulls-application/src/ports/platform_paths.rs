// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};

use super::PortResult;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlatformPaths {
    config_dir: PathBuf,
    data_dir: PathBuf,
    state_dir: PathBuf,
    cache_dir: PathBuf,
}

impl PlatformPaths {
    pub fn new(
        config_dir: PathBuf,
        data_dir: PathBuf,
        state_dir: PathBuf,
        cache_dir: PathBuf,
    ) -> Self {
        Self {
            config_dir,
            data_dir,
            state_dir,
            cache_dir,
        }
    }

    pub fn config_dir(&self) -> &Path {
        &self.config_dir
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn state_dir(&self) -> &Path {
        &self.state_dir
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }
}

pub trait PlatformPathsPort {
    fn user_paths(&self) -> PortResult<PlatformPaths>;
}
