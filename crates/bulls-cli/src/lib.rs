// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

mod cli;
mod error;
mod i18n;
mod render;
mod signals;

use std::ffi::OsString;
use std::io::{self, Write};
use std::process::ExitCode;

use bulls_application::{
    ApplicationError, DiscoveryBatchOutcome, OperationCompletion, ProtocolDiscoveryOutcome,
    ProtocolDocument, ProtocolOperationDocument, ProtocolOverviewView, ProtocolRefreshOutcome,
    ProtocolRepositoryDetailView, ProtocolRepositoryList, RepositoryListQuery, RepositorySelector,
    RepositorySelectorResolution,
};
use bulls_runtime::BullSaddleRuntime;

use cli::{CliCommand, CliRequest, OutputFormat, ParseOutcome, parse};
use error::{CliError, CliExitCode};
use i18n::{Locale, Message, render as render_message, resolve_locale};

pub fn run_from_env() -> ExitCode {
    let stdout = io::stdout();
    let stderr = io::stderr();
    let mut stdout = stdout.lock();
    let mut stderr = stderr.lock();
    let exit_code = run(std::env::args_os().skip(1), &mut stdout, &mut stderr);
    ExitCode::from(exit_code.value())
}

fn run<I, S>(arguments: I, stdout: &mut impl Write, stderr: &mut impl Write) -> CliExitCode
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    run_with_locale(arguments, stdout, stderr, presentation_locale())
}

fn run_with_locale<I, S>(
    arguments: I,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
    locale: Locale,
) -> CliExitCode
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    match run_inner(arguments, stdout, locale) {
        Ok(exit_code) => exit_code,
        Err((error, _)) if error.is_broken_pipe() => CliExitCode::Success,
        Err((error, format)) => {
            let exit_code = error.exit_code();
            render_error(stderr, &error, format, locale);
            exit_code
        }
    }
}

fn presentation_locale() -> Locale {
    let preference = BullSaddleRuntime::load_locale_preference().unwrap_or_default();
    resolve_locale(&preference)
}

fn run_inner<I, S>(
    arguments: I,
    stdout: &mut impl Write,
    locale: Locale,
) -> Result<CliExitCode, (CliError, OutputFormat)>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let outcome = parse(arguments)
        .map_err(CliError::Usage)
        .map_err(|error| (error, OutputFormat::Human))?;

    match outcome {
        ParseOutcome::Help(topic) => {
            render::help(stdout, topic, locale)
                .map_err(CliError::from)
                .map_err(|error| (error, OutputFormat::Human))?;
            Ok(CliExitCode::Success)
        }
        ParseOutcome::Version => {
            render::version(stdout)
                .map_err(CliError::from)
                .map_err(|error| (error, OutputFormat::Human))?;
            Ok(CliExitCode::Success)
        }
        ParseOutcome::Run(request) => {
            let format = request.output_format();
            execute(request, stdout, locale).map_err(|error| (error, format))
        }
    }
}

fn render_error(stderr: &mut impl Write, error: &CliError, format: OutputFormat, locale: Locale) {
    if let Some(report) = error.public_report() {
        match format {
            OutputFormat::Human => {
                let _ = writeln!(
                    stderr,
                    "{}: {}",
                    render_message(locale, Message::Error),
                    error.safe_message(locale)
                );
                let _ = render::failure_report(stderr, &report, locale);
            }
            OutputFormat::Json => {
                let _ = render::json(stderr, &report);
            }
        }
        return;
    }

    let _ = writeln!(
        stderr,
        "{}: {}",
        render_message(locale, Message::Error),
        error.safe_message(locale)
    );
    if let CliError::Usage(parse_error) = error {
        let _ = writeln!(stderr);
        let _ = render::help(stderr, parse_error.help_topic(), locale);
    }
}

fn execute(
    request: CliRequest,
    stdout: &mut impl Write,
    locale: Locale,
) -> Result<CliExitCode, CliError> {
    let mut runtime = BullSaddleRuntime::open()?;
    let result = (|| {
        if request.supports_cancellation() {
            signals::install_cancellation_handler(runtime.cancellation_handle())
                .map_err(|_| CliError::SignalHandler)?;
        }
        execute_with_runtime(&mut runtime, request, stdout, locale)
    })();
    runtime.shutdown();
    result
}

fn execute_with_runtime(
    runtime: &mut BullSaddleRuntime,
    request: CliRequest,
    stdout: &mut impl Write,
    locale: Locale,
) -> Result<CliExitCode, CliError> {
    let exit_code = match request.into_command() {
        CliCommand::Discover {
            roots,
            format,
            diagnostics,
        } => {
            let operation = if roots.is_empty() {
                runtime.discover()?
            } else {
                runtime.discover_roots(&roots)?
            };
            let exit_code = discovery_exit_code(operation.outcome());
            match format {
                OutputFormat::Human => {
                    let suggest_refresh = matches!(
                        operation.outcome().completion(),
                        OperationCompletion::Complete | OperationCompletion::Partial
                    ) && operation.outcome().repository_count() > 0;
                    render::discovery(
                        stdout,
                        operation.outcome(),
                        diagnostics,
                        suggest_refresh,
                        locale,
                    )?
                }
                OutputFormat::Json => {
                    let document = ProtocolOperationDocument::<ProtocolDiscoveryOutcome>::try_from(
                        &operation,
                    )?;
                    render::json(stdout, &document)?;
                }
            }
            exit_code
        }
        CliCommand::Refresh { format } => {
            let operation = runtime.refresh()?;
            let exit_code = operation_exit_code(operation.outcome().completion());
            match format {
                OutputFormat::Human => render::refresh(stdout, operation.outcome(), locale)?,
                OutputFormat::Json => {
                    let document =
                        ProtocolOperationDocument::<ProtocolRefreshOutcome>::try_from(&operation)?;
                    render::json(stdout, &document)?;
                }
            }
            exit_code
        }
        CliCommand::ConfigShow => {
            let projection = runtime.configuration_projection()?;
            render::configuration(stdout, &projection, locale)?;
            CliExitCode::Success
        }
        CliCommand::Reset => {
            runtime.reset()?;
            render::reset(stdout, locale)?;
            CliExitCode::Success
        }
        CliCommand::Repositories { page, format } => {
            let projection = runtime.repositories(&RepositoryListQuery::new(page))?;
            match format {
                OutputFormat::Human => render::repositories(stdout, &projection, locale)?,
                OutputFormat::Json => {
                    let document =
                        ProtocolDocument::<ProtocolRepositoryList>::try_from(&projection)?;
                    render::json(stdout, &document)?;
                }
            }
            CliExitCode::Success
        }
        CliCommand::Inspect { selector, format } => {
            let working_directory =
                std::env::current_dir().map_err(|_| CliError::WorkingDirectory)?;
            let selector = RepositorySelector::from_input(selector, &working_directory);
            let repository_id = match runtime.resolve_repository_selector(&selector)? {
                RepositorySelectorResolution::Resolved(repository_id) => repository_id,
                RepositorySelectorResolution::NotFound => return Err(CliError::RepositoryNotFound),
                RepositorySelectorResolution::Ambiguous { candidates } => {
                    return Err(CliError::Application(
                        ApplicationError::repository_selector_ambiguous(candidates),
                    ));
                }
            };
            let (projection, advisories) = runtime
                .repository_with_advisories(&repository_id)?
                .ok_or(CliError::RepositoryNotFound)?;
            match format {
                OutputFormat::Human => {
                    render::repository(stdout, &projection, &advisories, locale)?
                }
                OutputFormat::Json => {
                    let document = ProtocolDocument::<ProtocolRepositoryDetailView>::try_from((
                        &projection,
                        &advisories,
                    ))?;
                    render::json(stdout, &document)?;
                }
            }
            CliExitCode::Success
        }
        CliCommand::Overview { format } => {
            let (projection, advisories) = runtime.overview_with_advisories()?;
            match format {
                OutputFormat::Human => render::overview(stdout, &projection, &advisories, locale)?,
                OutputFormat::Json => {
                    let document = ProtocolDocument::<ProtocolOverviewView>::try_from((
                        &projection,
                        &advisories,
                    ))?;
                    render::json(stdout, &document)?;
                }
            }
            CliExitCode::Success
        }
    };
    Ok(exit_code)
}

fn discovery_exit_code(outcome: &DiscoveryBatchOutcome) -> CliExitCode {
    if outcome.requested_root_count() == 0 {
        CliExitCode::ExpectedFailure
    } else {
        operation_exit_code(outcome.completion())
    }
}

const fn operation_exit_code(completion: OperationCompletion) -> CliExitCode {
    match completion {
        OperationCompletion::Complete => CliExitCode::Success,
        OperationCompletion::Partial => CliExitCode::ExpectedFailure,
        OperationCompletion::Failed => CliExitCode::OperationalFailure,
        OperationCompletion::Cancelled => CliExitCode::Interrupted,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    use bulls_application::ports::PlatformPaths;
    use bulls_application::{
        BullSaddleConfiguration, DEFAULT_QUERY_PAGE_SIZE, GitProcessTimeout, LocalePreference,
        MAX_QUERY_PAGE_SIZE, ObservationExecutionPolicy,
    };
    use bulls_runtime::BullSaddleRuntime;

    use super::{CliExitCode, execute_with_runtime, run_with_locale};
    use crate::cli::{CliCommand, CliRequest};
    use crate::i18n::{Locale, test_locale};

    static TEST_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    fn help_output(arguments: &[&str], locale: Locale) -> String {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let exit = run_with_locale(arguments.iter().copied(), &mut stdout, &mut stderr, locale);

        assert_eq!(exit, CliExitCode::Success);
        assert!(stderr.is_empty());
        String::from_utf8(stdout).expect("help output must be UTF-8")
    }

    #[test]
    fn root_help_is_successful_without_stderr_output() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let exit = run_with_locale::<_, &str>([], &mut stdout, &mut stderr, test_locale("en-US"));

        assert_eq!(exit, CliExitCode::Success);
        assert!(
            String::from_utf8(stdout)
                .unwrap()
                .contains("bulls <command>")
        );
        assert!(stderr.is_empty());
    }

    #[test]
    fn root_help_can_be_rendered_in_portuguese() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let exit = run_with_locale::<_, &str>([], &mut stdout, &mut stderr, test_locale("pt-BR"));

        assert_eq!(exit, CliExitCode::Success);
        let stdout = String::from_utf8(stdout).unwrap();
        assert!(stdout.contains("Uso:"));
        assert!(stdout.contains("Ler o inventário de repositórios conhecidos"));
        assert!(stderr.is_empty());
    }

    #[test]
    fn root_help_explains_the_workspace_lifecycle_and_safety_boundaries() {
        let stdout = help_output(&[], test_locale("en-US"));

        for command in [
            "discover", "refresh", "config", "reset", "repos", "inspect", "overview",
        ] {
            assert!(stdout.contains(command));
        }
        assert!(stdout.contains("Typical workflow:"));
        assert!(stdout.contains("bulls discover <root>"));
        assert!(stdout.contains("bulls refresh"));
        assert!(stdout.contains("Query commands read persisted BullSaddle knowledge"));
        assert!(stdout.contains("does not implicitly fetch, pull, push or clone"));
        assert!(stdout.contains("bulls <command> --help"));
    }

    #[test]
    fn operational_command_help_documents_phase_9a_contracts() {
        let discover = help_output(&["discover", "--help"], test_locale("en-US"));
        assert!(discover.contains("Arguments:"));
        assert!(discover.contains("--verbose"));
        assert!(discover.contains("Human output only"));
        assert!(discover.contains("partial result"));
        assert!(discover.contains("local filesystem paths may be sensitive"));
        assert!(discover.contains("bulls discover -- ./-repositories"));

        let refresh = help_output(&["refresh", "--help"], test_locale("en-US"));
        assert!(refresh.contains("does not discover new repositories"));
        assert!(refresh.contains("fetch, pull or push"));
        assert!(refresh.contains("cancelled with Ctrl-C"));
        assert!(refresh.contains("bulls refresh --format json"));

        let config = help_output(&["config", "--help"], test_locale("en-US"));
        assert!(config.contains("platform-resolved configuration path"));
        assert!(config.contains("built-in defaults"));
        assert!(config.contains("This command is read-only"));

        let reset = help_output(&["reset", "--help"], test_locale("en-US"));
        assert!(reset.contains("known repository inventory"));
        assert!(reset.contains("Does not modify Git repositories"));
        assert!(reset.contains("Preserves BullSaddle user configuration"));
        assert!(reset.contains("Run discovery and refresh after reset"));
    }

    #[test]
    fn query_command_help_documents_persistence_selectors_and_freshness() {
        let repos = help_output(&["repos", "--help"], test_locale("en-US"));
        assert!(repos.contains("--offset <n>"));
        assert!(repos.contains(&format!(
            "Default: {DEFAULT_QUERY_PAGE_SIZE}. Maximum: {MAX_QUERY_PAGE_SIZE}"
        )));
        assert!(repos.contains("does not run discovery, refresh or Git processes"));

        let inspect = help_output(&["inspect", "--help"], test_locale("en-US"));
        assert!(inspect.contains("A stable RepositoryId, '.', a repository path"));
        assert!(inspect.contains("Fuzzy matching is never performed"));
        assert!(inspect.contains("ambiguous path returns explicit candidates"));
        assert!(inspect.contains("freshness describe persisted evidence"));
        assert!(inspect.contains("Ahead/behind values are relative to upstream information"));

        let overview = help_output(&["overview", "--help"], test_locale("en-US"));
        assert!(overview.contains("deterministic advisories"));
        assert!(overview.contains("Unknown remains unknown"));
        assert!(overview.contains("does not run discovery, refresh, Git processes or network"));
    }

    #[test]
    fn command_help_is_semantically_localized_in_portuguese() {
        let discover = help_output(&["discover", "--help"], test_locale("pt-BR"));
        assert!(discover.contains("Argumentos:"));
        assert!(discover.contains("Comportamento:"));
        assert!(discover.contains("Exemplos:"));
        assert!(discover.contains("Somente saída humana"));

        let inspect = help_output(&["inspect", "--help"], test_locale("pt-BR"));
        assert!(inspect.contains("Correspondência aproximada nunca é realizada"));
        assert!(inspect.contains("evidência persistida"));

        let reset = help_output(&["reset", "--help"], test_locale("pt-BR"));
        assert!(reset.contains("Não modifica repositórios Git"));
        assert!(reset.contains("Preserva a configuração do usuário do BullSaddle"));
    }

    #[test]
    fn help_subcommand_and_command_help_flag_render_the_same_content() {
        let via_help = help_output(&["help", "inspect"], test_locale("en-US"));
        let via_flag = help_output(&["inspect", "--help"], test_locale("en-US"));

        assert_eq!(via_help, via_flag);
    }

    #[test]
    fn configuration_show_reads_the_effective_runtime_configuration() {
        let sequence = TEST_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "bulls-cli-config-test-{}-{sequence}",
            std::process::id()
        ));
        let user = root.join("user");
        let paths = PlatformPaths::new(
            user.join("config"),
            user.join("data"),
            user.join("state"),
            user.join("cache"),
        );
        let mut runtime = BullSaddleRuntime::from_platform_paths(paths)
            .expect("runtime composition must succeed");
        let configuration = BullSaddleConfiguration::new(
            LocalePreference::explicit("pt-BR").expect("locale must be accepted"),
        )
        .with_observation_policy(
            ObservationExecutionPolicy::new(5).expect("parallelism must be accepted"),
        )
        .with_git_process_timeout(GitProcessTimeout::new(8_500).expect("timeout must be accepted"));
        runtime
            .save_configuration(&configuration)
            .expect("configuration must save");
        let mut stdout = Vec::new();

        let exit = execute_with_runtime(
            &mut runtime,
            CliRequest::new(CliCommand::ConfigShow),
            &mut stdout,
            test_locale("en-US"),
        )
        .expect("configuration inspection must succeed");

        assert_eq!(exit, CliExitCode::Success);
        let stdout = String::from_utf8(stdout).expect("configuration output must be UTF-8");
        assert!(stdout.contains("Source: file"));
        assert!(stdout.contains("Locale: pt-BR"));
        assert!(stdout.contains("Observation parallelism: 5"));
        assert!(stdout.contains("Git process timeout: 8500 ms"));

        runtime.shutdown();
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn configuration_help_is_available_without_opening_runtime_state() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let exit = run_with_locale(
            ["config", "--help"],
            &mut stdout,
            &mut stderr,
            test_locale("en-US"),
        );

        assert_eq!(exit, CliExitCode::Success);
        let stdout = String::from_utf8(stdout).unwrap();
        assert!(stdout.contains("Usage: bulls config show"));
        assert!(stdout.contains("does not modify configuration"));
        assert!(stderr.is_empty());
    }

    #[test]
    fn version_is_successful_without_runtime_state() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let exit = run_with_locale(
            ["--version"],
            &mut stdout,
            &mut stderr,
            test_locale("en-US"),
        );

        assert_eq!(exit, CliExitCode::Success);
        assert_eq!(
            String::from_utf8(stdout).unwrap(),
            format!("bulls {}\n", env!("CARGO_PKG_VERSION"))
        );
        assert!(stderr.is_empty());
    }

    #[test]
    fn syntax_errors_use_the_usage_exit_code_and_contextual_help() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let exit = run_with_locale(["inspect"], &mut stdout, &mut stderr, test_locale("en-US"));

        assert_eq!(exit, CliExitCode::Usage);
        assert!(stdout.is_empty());
        let stderr = String::from_utf8(stderr).unwrap();
        assert!(stderr.contains("Usage requires exactly one repository selector."));
        assert!(stderr.contains("Usage: bulls inspect <repository-selector>"));
    }
}
