// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use std::ffi::OsStr;
use std::path::Path;
use std::sync::Arc;
#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

use bulls_application::ports::{PortError, PortErrorKind, PortResult};

use crate::error::process_port_error;
use crate::process::{NativeProcessRunner, ProcessExecutionPolicy, ProcessRequest};

#[derive(Clone)]
pub(crate) struct GitProcessRunner {
    runner: NativeProcessRunner,
    policy: ProcessExecutionPolicy,
    cancellation: Arc<dyn Fn() -> bool + Send + Sync>,
    #[cfg(test)]
    command_count: Arc<AtomicUsize>,
}

impl GitProcessRunner {
    pub(crate) fn new(policy: ProcessExecutionPolicy) -> Self {
        Self {
            runner: NativeProcessRunner::new(),
            policy,
            cancellation: Arc::new(|| false),
            #[cfg(test)]
            command_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub(crate) fn with_cancellation(
        policy: ProcessExecutionPolicy,
        cancellation: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Self {
        Self {
            runner: NativeProcessRunner::new(),
            policy,
            cancellation,
            #[cfg(test)]
            command_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    #[cfg(test)]
    pub(crate) fn command_count(&self) -> usize {
        self.command_count.load(Ordering::Acquire)
    }

    pub(crate) fn run<I, S>(&self, repository: &Path, arguments: I) -> PortResult<Vec<u8>>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        #[cfg(test)]
        self.command_count.fetch_add(1, Ordering::AcqRel);
        let mut request = ProcessRequest::new("git", self.policy)
            .arg("-C")
            .arg(repository.as_os_str())
            .arg("-c")
            .arg("core.quotePath=false")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_COMMON_DIR")
            .env_remove("GIT_INDEX_FILE")
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("LC_ALL", "C");

        for argument in arguments {
            request = request.arg(argument);
        }

        let output = self
            .runner
            .run(&request, self.cancellation.as_ref())
            .map_err(process_port_error)?;
        let (status, stdout, _stderr) = output.into_parts();

        if status.success() {
            Ok(stdout)
        } else {
            Err(PortError::new(PortErrorKind::ProcessExited {
                exit_code: status.code().unwrap_or(-1),
            }))
        }
    }
}
