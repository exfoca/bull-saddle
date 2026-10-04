// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use std::io;

use bulls_application::ports::{PortError, PortErrorKind};
use bulls_application::{
    ApplicationError, ApplicationErrorCode, FailureCause, FailureClass, FailureCode,
    ProtocolBuildError, PublicFailureDescriptor, PublicFailureReport,
};

use crate::cli::{ParseError, ParseErrorKind};
use crate::i18n::{Locale, Message, render as render_message};
use crate::render::JsonOutputError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum CliExitCode {
    Success = 0,
    Usage = 2,
    ExpectedFailure = 3,
    OperationalFailure = 4,
    InternalFailure = 70,
    Interrupted = 130,
}

impl CliExitCode {
    pub(crate) const fn value(self) -> u8 {
        self as u8
    }
}

#[derive(Debug)]
pub(crate) enum CliError {
    Usage(ParseError),
    Runtime(PortError),
    Application(ApplicationError),
    RepositoryNotFound,
    Protocol(ProtocolBuildError),
    JsonSerialization,
    SignalHandler,
    WorkingDirectory,
    Output(io::ErrorKind),
}

impl CliError {
    pub(crate) fn exit_code(&self) -> CliExitCode {
        match self {
            Self::Usage(_) => CliExitCode::Usage,
            Self::Runtime(error) if error.kind() == PortErrorKind::Cancelled => {
                CliExitCode::Interrupted
            }
            Self::Runtime(error) => exit_code_for_failure_class(error.public_failure().class()),
            Self::Application(error) => exit_code_for_failure_class(error.public_failure().class()),
            Self::RepositoryNotFound => CliExitCode::ExpectedFailure,
            Self::Protocol(_) | Self::JsonSerialization => CliExitCode::InternalFailure,
            Self::SignalHandler | Self::WorkingDirectory | Self::Output(_) => {
                CliExitCode::OperationalFailure
            }
        }
    }

    pub(crate) fn safe_message(&self, locale: Locale) -> String {
        match self {
            Self::Usage(error) => parse_error_message(locale, error),
            Self::Runtime(error) => {
                let failure = error.public_failure();
                render_message(
                    locale,
                    Message::ErrorRuntime {
                        error_code: failure.error_code().as_str(),
                    },
                )
            }
            Self::Application(ApplicationError::RepositorySelectorAmbiguous { candidates }) => {
                let candidates = join_repository_ids(candidates);
                render_message(
                    locale,
                    Message::ErrorRepositorySelectorAmbiguous {
                        candidates: &candidates,
                    },
                )
            }
            Self::Application(_) => render_message(locale, Message::ErrorApplication),
            Self::RepositoryNotFound => render_message(locale, Message::ErrorRepositoryNotFound),
            Self::Protocol(error) => render_message(locale, protocol_error_message(*error)),
            Self::JsonSerialization => render_message(locale, Message::ErrorJsonSerialization),
            Self::SignalHandler => render_message(locale, Message::ErrorSignalHandler),
            Self::WorkingDirectory => render_message(locale, Message::ErrorWorkingDirectory),
            Self::Output(_) => render_message(locale, Message::ErrorOutput),
        }
    }

    pub(crate) fn public_report(&self) -> Option<PublicFailureReport> {
        let failure = match self {
            Self::Usage(_) | Self::Output(_) => return None,
            Self::Runtime(error) => return Some(error.public_report()),
            Self::Application(error) => return Some(error.public_report()),
            Self::RepositoryNotFound => PublicFailureDescriptor::new(FailureCode::Application(
                ApplicationErrorCode::RepositoryNotFound,
            )),
            Self::Protocol(_) => PublicFailureDescriptor::new(FailureCode::InvariantViolation),
            Self::JsonSerialization => PublicFailureDescriptor::new(FailureCode::UnexpectedFailure),
            Self::SignalHandler => PublicFailureDescriptor::with_causes(
                FailureCode::OperationFailed,
                vec![FailureCause::ResourceUnavailable],
            ),
            Self::WorkingDirectory => PublicFailureDescriptor::with_causes(
                FailureCode::OperationFailed,
                vec![FailureCause::IoFailure],
            ),
        };
        Some(PublicFailureReport::new(failure))
    }

    pub(crate) const fn is_broken_pipe(&self) -> bool {
        matches!(self, Self::Output(io::ErrorKind::BrokenPipe))
    }
}

fn parse_error_message(locale: Locale, error: &ParseError) -> String {
    let message = match error.kind() {
        ParseErrorKind::InvalidUtf8(label) => invalid_utf8_message(label),
        ParseErrorKind::UnknownOption {
            command: None,
            option,
        } => Message::ErrorUnknownOption { option },
        ParseErrorKind::UnknownOption {
            command: Some(command),
            option,
        } => Message::ErrorUnknownOptionForCommand {
            command: command.as_str(),
            option,
        },
        ParseErrorKind::UnknownCommand(command) => Message::ErrorUnknownCommand { command },
        ParseErrorKind::UnknownHelpTopic(topic) => Message::ErrorUnknownHelpTopic { topic },
        ParseErrorKind::TooManyHelpTopics => Message::ErrorTooManyHelpTopics,
        ParseErrorKind::EmptyDiscoveryRoot => Message::ErrorEmptyDiscoveryRoot,
        ParseErrorKind::DuplicateOption(option) => Message::ErrorDuplicateOption { option },
        ParseErrorKind::MissingOptionValue(option) => Message::ErrorMissingOptionValue { option },
        ParseErrorKind::RepositoriesOptionsOnly => Message::ErrorRepositoriesOptionsOnly,
        ParseErrorKind::LimitOutOfRange(maximum) => {
            let maximum = maximum.to_string();
            return render_message(locale, Message::ErrorLimitOutOfRange { maximum: &maximum });
        }
        ParseErrorKind::FormatOptionOnly(command) => Message::ErrorFormatOptionOnly {
            command: command.as_str(),
        },
        ParseErrorKind::InvalidOutputFormat => Message::ErrorInvalidOutputFormat,
        ParseErrorKind::VerboseRequiresHumanOutput => Message::ErrorVerboseRequiresHumanOutput,
        ParseErrorKind::ConfigurationActionCount => Message::ErrorConfigurationActionCount,
        ParseErrorKind::InvalidConfigurationAction(action) => {
            Message::ErrorInvalidConfigurationAction { action }
        }
        ParseErrorKind::InspectSelectorCount => Message::ErrorInspectSelectorCount,
        ParseErrorKind::InvalidRepositorySelector => Message::ErrorInvalidRepositorySelector,
        ParseErrorKind::InvalidInteger(option) => Message::ErrorInvalidInteger { option },
        ParseErrorKind::UnexpectedGlobalArgument => Message::ErrorUnexpectedGlobalArgument,
        ParseErrorKind::CommandTakesNoArguments(command) => Message::ErrorCommandTakesNoArguments {
            command: command.as_str(),
        },
    };
    render_message(locale, message)
}

fn invalid_utf8_message(label: &str) -> Message<'_> {
    match label {
        "command" => Message::ErrorInvalidUtf8Command,
        "help topic" => Message::ErrorInvalidUtf8HelpTopic,
        "configuration action" => Message::ErrorInvalidUtf8ConfigurationAction,
        "repository option" => Message::ErrorInvalidUtf8RepositoryOption,
        "option" => Message::ErrorInvalidUtf8Option,
        _ => Message::ErrorInvalidUtf8 { label },
    }
}

const fn protocol_error_message(error: ProtocolBuildError) -> Message<'static> {
    match error {
        ProtocolBuildError::CountOutOfRange => Message::ErrorProtocolCountOutOfRange,
        ProtocolBuildError::TimestampOutOfRange => Message::ErrorProtocolTimestampOutOfRange,
        ProtocolBuildError::InconsistentWorkspaceRevision => {
            Message::ErrorProtocolInconsistentWorkspaceRevision
        }
    }
}

fn join_repository_ids(repository_ids: &[bulls_application::RepositoryId]) -> String {
    repository_ids
        .iter()
        .map(bulls_application::RepositoryId::as_str)
        .collect::<Vec<_>>()
        .join(", ")
}

impl From<PortError> for CliError {
    fn from(error: PortError) -> Self {
        Self::Runtime(error)
    }
}

impl From<io::Error> for CliError {
    fn from(error: io::Error) -> Self {
        Self::Output(error.kind())
    }
}

impl From<ProtocolBuildError> for CliError {
    fn from(error: ProtocolBuildError) -> Self {
        Self::Protocol(error)
    }
}

impl From<JsonOutputError> for CliError {
    fn from(error: JsonOutputError) -> Self {
        match error {
            JsonOutputError::Serialization => Self::JsonSerialization,
            JsonOutputError::Output(kind) => Self::Output(kind),
        }
    }
}

const fn exit_code_for_failure_class(class: FailureClass) -> CliExitCode {
    match class {
        FailureClass::Expected => CliExitCode::ExpectedFailure,
        FailureClass::Operational => CliExitCode::OperationalFailure,
        FailureClass::Defect => CliExitCode::InternalFailure,
    }
}

#[cfg(test)]
mod tests {
    use bulls_application::ports::{PortError, PortErrorKind};
    use bulls_application::{ApplicationError, ProtocolBuildError, RepositoryId};

    use super::{CliError, CliExitCode};
    use crate::i18n::test_locale;

    #[test]
    fn runtime_failure_classes_map_to_stable_exit_categories() {
        let expected = CliError::Runtime(PortError::new(PortErrorKind::Cancelled));
        let operational = CliError::Runtime(PortError::new(PortErrorKind::TimedOut));
        let defect = CliError::Runtime(PortError::new(PortErrorKind::InvariantViolation));

        assert_eq!(expected.exit_code(), CliExitCode::Interrupted);
        assert_eq!(operational.exit_code(), CliExitCode::OperationalFailure);
        assert_eq!(defect.exit_code(), CliExitCode::InternalFailure);
    }

    #[test]
    fn runtime_error_message_uses_only_the_public_failure_contract() {
        let error = CliError::Runtime(PortError::new(PortErrorKind::StorageFailure));

        assert_eq!(
            error.safe_message(test_locale("en-US")),
            "BullSaddle operation failed: operation_failed."
        );
    }

    #[test]
    fn protocol_error_message_preserves_safe_static_diagnostic() {
        let error = CliError::Protocol(ProtocolBuildError::InconsistentWorkspaceRevision);

        assert_eq!(
            error.safe_message(test_locale("en-US")),
            "Unable to build structured command output: protocol source contains inconsistent workspace revisions."
        );
    }

    #[test]
    fn human_error_messages_are_localized_without_changing_public_codes() {
        let error = CliError::RepositoryNotFound;

        assert_eq!(
            error.safe_message(test_locale("en-US")),
            "Repository not found."
        );
        assert_eq!(
            error.safe_message(test_locale("pt-BR")),
            "Repositório não encontrado."
        );
        assert_eq!(
            error
                .public_report()
                .expect("failure report must exist")
                .failure()
                .error_code()
                .as_str(),
            "repository_not_found"
        );
    }

    #[test]
    fn ambiguous_selector_errors_preserve_candidates_and_expected_failure_semantics() {
        let error = CliError::Application(ApplicationError::repository_selector_ambiguous(vec![
            RepositoryId::from("repository-a"),
            RepositoryId::from("repository-b"),
        ]));

        assert_eq!(error.exit_code(), CliExitCode::ExpectedFailure);
        assert_eq!(
            error.safe_message(test_locale("en-US")),
            "Repository selector is ambiguous. Candidates: repository-a, repository-b."
        );
        let report = error.public_report().expect("ambiguity report must exist");
        assert_eq!(
            report.failure().error_code().as_str(),
            "repository_selector_ambiguous"
        );
        assert!(report.failure().details().is_some());
    }

    #[test]
    fn working_directory_failure_is_operational_and_typed() {
        let error = CliError::WorkingDirectory;
        let report = error
            .public_report()
            .expect("working directory report must exist");

        assert_eq!(error.exit_code(), CliExitCode::OperationalFailure);
        assert_eq!(report.failure().error_code().as_str(), "operation_failed");
        assert_eq!(
            report.failure().causes(),
            &[bulls_application::FailureCause::IoFailure]
        );
    }

    #[test]
    fn public_reports_cover_runtime_and_expected_failures_without_private_context() {
        let runtime = CliError::Runtime(PortError::new(PortErrorKind::StorageFailure));
        let runtime_report = runtime.public_report().expect("runtime report must exist");
        assert_eq!(
            runtime_report.failure().error_code().as_str(),
            "operation_failed"
        );

        let missing = CliError::RepositoryNotFound;
        let missing_report = missing.public_report().expect("missing report must exist");
        assert_eq!(
            missing_report.failure().error_code().as_str(),
            "repository_not_found"
        );
        assert!(missing_report.failure().causes().is_empty());
    }

    #[test]
    fn signal_handler_failure_is_operational_and_privacy_safe() {
        let error = CliError::SignalHandler;
        let report = error
            .public_report()
            .expect("signal failure report must exist");

        assert_eq!(error.exit_code(), CliExitCode::OperationalFailure);
        assert_eq!(report.failure().error_code().as_str(), "operation_failed");
        assert_eq!(report.failure().causes().len(), 1);
        assert_eq!(
            report.failure().causes()[0].as_str(),
            "resource_unavailable"
        );
    }

    #[test]
    fn broken_pipe_is_explicitly_classified() {
        let error = CliError::Output(std::io::ErrorKind::BrokenPipe);

        assert!(error.is_broken_pipe());
        assert_eq!(error.exit_code(), CliExitCode::OperationalFailure);
        assert!(error.public_report().is_none());
    }
}
