// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use std::ffi::{OsStr, OsString};
use std::io::{self, Read};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ProcessExecutionPolicy {
    timeout: Duration,
    stdout_limit: usize,
    stderr_limit: usize,
}

impl ProcessExecutionPolicy {
    pub(crate) const fn new(timeout: Duration, stdout_limit: usize, stderr_limit: usize) -> Self {
        Self {
            timeout,
            stdout_limit,
            stderr_limit,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ProcessRequest {
    program: OsString,
    arguments: Vec<OsString>,
    environment_removals: Vec<OsString>,
    environment_overrides: Vec<(OsString, OsString)>,
    policy: ProcessExecutionPolicy,
}

impl ProcessRequest {
    pub(crate) fn new(program: impl AsRef<OsStr>, policy: ProcessExecutionPolicy) -> Self {
        Self {
            program: program.as_ref().to_owned(),
            arguments: Vec::new(),
            environment_removals: Vec::new(),
            environment_overrides: Vec::new(),
            policy,
        }
    }

    pub(crate) fn arg(mut self, argument: impl AsRef<OsStr>) -> Self {
        self.arguments.push(argument.as_ref().to_owned());
        self
    }

    pub(crate) fn env_remove(mut self, key: impl AsRef<OsStr>) -> Self {
        self.environment_removals.push(key.as_ref().to_owned());
        self
    }

    pub(crate) fn env(mut self, key: impl AsRef<OsStr>, value: impl AsRef<OsStr>) -> Self {
        self.environment_overrides
            .push((key.as_ref().to_owned(), value.as_ref().to_owned()));
        self
    }
}

#[derive(Debug)]
pub(crate) struct ProcessOutput {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

impl ProcessOutput {
    pub(crate) fn into_parts(self) -> (ExitStatus, Vec<u8>, Vec<u8>) {
        (self.status, self.stdout, self.stderr)
    }
}

#[derive(Debug)]
pub(crate) enum ProcessError {
    Io(io::Error),
    TimedOut,
    Cancelled,
    OutputLimitExceeded,
}

pub(crate) trait CancellationSignal {
    fn is_cancelled(&self) -> bool;
}

impl<F> CancellationSignal for F
where
    F: Fn() -> bool + ?Sized,
{
    fn is_cancelled(&self) -> bool {
        self()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct NativeProcessRunner;

impl NativeProcessRunner {
    pub(crate) const fn new() -> Self {
        Self
    }

    pub(crate) fn run<C>(
        &self,
        request: &ProcessRequest,
        cancellation: &C,
    ) -> Result<ProcessOutput, ProcessError>
    where
        C: CancellationSignal + ?Sized,
    {
        if cancellation.is_cancelled() {
            return Err(ProcessError::Cancelled);
        }

        let mut command = Command::new(&request.program);
        command
            .args(&request.arguments)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        for key in &request.environment_removals {
            command.env_remove(key);
        }
        for (key, value) in &request.environment_overrides {
            command.env(key, value);
        }

        let mut child = command.spawn().map_err(ProcessError::Io)?;
        let stdout = match child.stdout.take() {
            Some(stdout) => stdout,
            None => {
                terminate(&mut child);
                return Err(ProcessError::Io(io::Error::other(
                    "process stdout pipe is unavailable",
                )));
            }
        };
        let stderr = match child.stderr.take() {
            Some(stderr) => stderr,
            None => {
                terminate(&mut child);
                return Err(ProcessError::Io(io::Error::other(
                    "process stderr pipe is unavailable",
                )));
            }
        };

        let output_limit_exceeded = Arc::new(AtomicBool::new(false));
        let stdout_reader = capture_bounded(
            stdout,
            request.policy.stdout_limit,
            Arc::clone(&output_limit_exceeded),
        );
        let stderr_reader = capture_bounded(
            stderr,
            request.policy.stderr_limit,
            Arc::clone(&output_limit_exceeded),
        );
        let started_at = Instant::now();

        let completion = loop {
            if cancellation.is_cancelled() {
                terminate(&mut child);
                break Err(ProcessError::Cancelled);
            }
            if output_limit_exceeded.load(Ordering::Acquire) {
                terminate(&mut child);
                break Err(ProcessError::OutputLimitExceeded);
            }
            if started_at.elapsed() >= request.policy.timeout {
                terminate(&mut child);
                break Err(ProcessError::TimedOut);
            }

            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) => thread::sleep(Duration::from_millis(5)),
                Err(error) => {
                    terminate(&mut child);
                    break Err(ProcessError::Io(error));
                }
            }
        };

        let stdout = join_reader(stdout_reader);
        let stderr = join_reader(stderr_reader);
        let status = completion?;
        let stdout = stdout?;
        let stderr = stderr?;
        if output_limit_exceeded.load(Ordering::Acquire) {
            return Err(ProcessError::OutputLimitExceeded);
        }

        Ok(ProcessOutput {
            status,
            stdout,
            stderr,
        })
    }
}

fn capture_bounded<R>(
    mut reader: R,
    limit: usize,
    output_limit_exceeded: Arc<AtomicBool>,
) -> JoinHandle<io::Result<Vec<u8>>>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut output = Vec::with_capacity(limit.min(8192));
        let mut buffer = [0_u8; 8192];

        loop {
            let read = reader.read(&mut buffer)?;
            if read == 0 {
                break;
            }

            let remaining = limit.saturating_sub(output.len());
            let retained = read.min(remaining);
            output.extend_from_slice(&buffer[..retained]);

            if retained < read {
                output_limit_exceeded.store(true, Ordering::Release);
            }
        }

        Ok(output)
    })
}

fn join_reader(reader: JoinHandle<io::Result<Vec<u8>>>) -> Result<Vec<u8>, ProcessError> {
    reader
        .join()
        .map_err(|_| ProcessError::Io(io::Error::other("process output reader panicked")))?
        .map_err(ProcessError::Io)
}

fn terminate(child: &mut std::process::Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(test)]
mod tests {
    use std::io::{self, Write};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread;
    use std::time::Duration;

    use super::{NativeProcessRunner, ProcessError, ProcessExecutionPolicy, ProcessRequest};

    const FIXTURE_ENV: &str = "BULLS_PROCESS_TEST_FIXTURE";
    const FIXTURE_TEST: &str = "process::tests::process_fixture";

    fn fixture_request(mode: &str, policy: ProcessExecutionPolicy) -> ProcessRequest {
        let executable = std::env::current_exe().expect("test executable must be available");
        ProcessRequest::new(executable.as_os_str(), policy)
            .arg("--exact")
            .arg(FIXTURE_TEST)
            .arg("--nocapture")
            .env(FIXTURE_ENV, mode)
    }

    #[test]
    fn process_fixture() {
        match std::env::var(FIXTURE_ENV).as_deref() {
            Ok("output") => {
                io::stdout()
                    .write_all(b"bulls-process-output")
                    .expect("fixture stdout must be writable");
            }
            Ok("large-output") => {
                io::stdout()
                    .write_all(&[b'x'; 4096])
                    .expect("fixture stdout must be writable");
            }
            Ok("sleep") => thread::sleep(Duration::from_secs(2)),
            Ok("exit") => std::process::exit(23),
            _ => {}
        }
    }

    #[test]
    fn runner_captures_bounded_output_and_exit_status() {
        let policy = ProcessExecutionPolicy::new(Duration::from_secs(2), 4096, 4096);
        let output = NativeProcessRunner::new()
            .run(&fixture_request("output", policy), &|| false)
            .expect("fixture process must succeed");
        let (status, stdout, _stderr) = output.into_parts();

        assert!(status.success());
        assert!(
            stdout
                .windows(b"bulls-process-output".len())
                .any(|window| window == b"bulls-process-output")
        );
    }

    #[test]
    fn runner_reports_non_zero_exit_without_interpreting_process_semantics() {
        let policy = ProcessExecutionPolicy::new(Duration::from_secs(2), 4096, 4096);
        let output = NativeProcessRunner::new()
            .run(&fixture_request("exit", policy), &|| false)
            .expect("process runner must return the exit status");
        let (status, _stdout, _stderr) = output.into_parts();

        assert_eq!(status.code(), Some(23));
    }

    #[test]
    fn runner_terminates_processes_that_exceed_the_timeout() {
        let policy = ProcessExecutionPolicy::new(Duration::from_millis(50), 4096, 4096);
        let error = NativeProcessRunner::new()
            .run(&fixture_request("sleep", policy), &|| false)
            .expect_err("sleeping process must time out");

        assert!(matches!(error, ProcessError::TimedOut));
    }

    #[test]
    fn runner_terminates_processes_after_cancellation() {
        let policy = ProcessExecutionPolicy::new(Duration::from_secs(2), 4096, 4096);
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancellation = Arc::clone(&cancelled);
        let canceller = thread::spawn(move || {
            thread::sleep(Duration::from_millis(25));
            cancellation.store(true, Ordering::Release);
        });
        let error = NativeProcessRunner::new()
            .run(&fixture_request("sleep", policy), &|| {
                cancelled.load(Ordering::Acquire)
            })
            .expect_err("cancelled process must stop");
        canceller.join().expect("cancellation thread must finish");

        assert!(matches!(error, ProcessError::Cancelled));
    }

    #[test]
    fn runner_terminates_processes_that_exceed_output_bounds() {
        let policy = ProcessExecutionPolicy::new(Duration::from_secs(2), 128, 4096);
        let error = NativeProcessRunner::new()
            .run(&fixture_request("large-output", policy), &|| false)
            .expect_err("oversized process output must be rejected");

        assert!(matches!(error, ProcessError::OutputLimitExceeded));
    }
}
