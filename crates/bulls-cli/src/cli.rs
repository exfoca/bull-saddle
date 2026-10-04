// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use std::ffi::OsString;
use std::path::PathBuf;

use bulls_application::{DEFAULT_QUERY_PAGE_SIZE, MAX_QUERY_PAGE_SIZE, QueryPage};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CommandName {
    Discover,
    Refresh,
    Config,
    Reset,
    Repositories,
    Inspect,
    Overview,
}

impl CommandName {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Discover => "discover",
            Self::Refresh => "refresh",
            Self::Config => "config",
            Self::Reset => "reset",
            Self::Repositories => "repos",
            Self::Inspect => "inspect",
            Self::Overview => "overview",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "discover" => Some(Self::Discover),
            "refresh" => Some(Self::Refresh),
            "config" => Some(Self::Config),
            "reset" => Some(Self::Reset),
            "repos" => Some(Self::Repositories),
            "inspect" => Some(Self::Inspect),
            "overview" => Some(Self::Overview),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OutputFormat {
    Human,
    Json,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DiagnosticDetail {
    Summary,
    Verbose,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum CliCommand {
    Discover {
        roots: Vec<PathBuf>,
        format: OutputFormat,
        diagnostics: DiagnosticDetail,
    },
    Refresh {
        format: OutputFormat,
    },
    ConfigShow,
    Reset,
    Repositories {
        page: QueryPage,
        format: OutputFormat,
    },
    Inspect {
        selector: OsString,
        format: OutputFormat,
    },
    Overview {
        format: OutputFormat,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CliRequest {
    command: CliCommand,
}

impl CliRequest {
    pub(crate) const fn new(command: CliCommand) -> Self {
        Self { command }
    }

    pub(crate) const fn output_format(&self) -> OutputFormat {
        match &self.command {
            CliCommand::Discover { format, .. }
            | CliCommand::Refresh { format }
            | CliCommand::Repositories { format, .. }
            | CliCommand::Inspect { format, .. }
            | CliCommand::Overview { format } => *format,
            CliCommand::ConfigShow | CliCommand::Reset => OutputFormat::Human,
        }
    }

    pub(crate) const fn supports_cancellation(&self) -> bool {
        matches!(
            &self.command,
            CliCommand::Discover { .. } | CliCommand::Refresh { .. }
        )
    }

    pub(crate) fn into_command(self) -> CliCommand {
        self.command
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HelpTopic {
    Root,
    Command(CommandName),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ParseOutcome {
    Help(HelpTopic),
    Version,
    Run(CliRequest),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ParseErrorKind {
    InvalidUtf8(String),
    UnknownOption {
        command: Option<CommandName>,
        option: String,
    },
    UnknownCommand(String),
    UnknownHelpTopic(String),
    TooManyHelpTopics,
    EmptyDiscoveryRoot,
    DuplicateOption(&'static str),
    MissingOptionValue(&'static str),
    RepositoriesOptionsOnly,
    LimitOutOfRange(usize),
    FormatOptionOnly(CommandName),
    InvalidOutputFormat,
    VerboseRequiresHumanOutput,
    ConfigurationActionCount,
    InvalidConfigurationAction(String),
    InspectSelectorCount,
    InvalidRepositorySelector,
    InvalidInteger(String),
    UnexpectedGlobalArgument,
    CommandTakesNoArguments(CommandName),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ParseError {
    kind: ParseErrorKind,
    help_topic: HelpTopic,
}

impl ParseError {
    fn root(kind: ParseErrorKind) -> Self {
        Self {
            kind,
            help_topic: HelpTopic::Root,
        }
    }

    fn command(command: CommandName, kind: ParseErrorKind) -> Self {
        Self {
            kind,
            help_topic: HelpTopic::Command(command),
        }
    }

    pub(crate) const fn kind(&self) -> &ParseErrorKind {
        &self.kind
    }

    pub(crate) const fn help_topic(&self) -> HelpTopic {
        self.help_topic
    }
}

pub(crate) fn parse<I, S>(arguments: I) -> Result<ParseOutcome, ParseError>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let arguments = arguments.into_iter().map(Into::into).collect::<Vec<_>>();
    let Some(first) = arguments.first() else {
        return Ok(ParseOutcome::Help(HelpTopic::Root));
    };

    if is_help(first) {
        return ensure_no_trailing_arguments(&arguments[1..], ParseOutcome::Help(HelpTopic::Root));
    }
    if is_version(first) {
        return ensure_no_trailing_arguments(&arguments[1..], ParseOutcome::Version);
    }

    let first = utf8(first, "command")?;
    if first == "help" {
        return parse_help(&arguments[1..]);
    }
    if first.starts_with('-') {
        return Err(ParseError::root(ParseErrorKind::UnknownOption {
            command: None,
            option: first.to_owned(),
        }));
    }

    let command = CommandName::parse(first)
        .ok_or_else(|| ParseError::root(ParseErrorKind::UnknownCommand(first.to_owned())))?;
    parse_command(command, &arguments[1..])
}

fn parse_help(arguments: &[OsString]) -> Result<ParseOutcome, ParseError> {
    match arguments {
        [] => Ok(ParseOutcome::Help(HelpTopic::Root)),
        [topic] => {
            let topic = utf8(topic, "help topic")?;
            let command = CommandName::parse(topic).ok_or_else(|| {
                ParseError::root(ParseErrorKind::UnknownHelpTopic(topic.to_owned()))
            })?;
            Ok(ParseOutcome::Help(HelpTopic::Command(command)))
        }
        _ => Err(ParseError::root(ParseErrorKind::TooManyHelpTopics)),
    }
}

fn parse_command(command: CommandName, arguments: &[OsString]) -> Result<ParseOutcome, ParseError> {
    if arguments.len() == 1 && is_help(&arguments[0]) {
        return Ok(ParseOutcome::Help(HelpTopic::Command(command)));
    }

    let cli_command = match command {
        CommandName::Discover => {
            let (roots, format, diagnostics) = parse_discovery_arguments(arguments)?;
            CliCommand::Discover {
                roots,
                format,
                diagnostics,
            }
        }
        CommandName::Refresh => CliCommand::Refresh {
            format: parse_format_option(command, arguments)?,
        },
        CommandName::Config => parse_configuration_arguments(arguments)?,
        CommandName::Reset => {
            ensure_command_has_no_arguments(command, arguments)?;
            CliCommand::Reset
        }
        CommandName::Repositories => {
            let (page, format) = parse_repository_options(arguments)?;
            CliCommand::Repositories { page, format }
        }
        CommandName::Inspect => {
            let (selector, format) = parse_inspect_arguments(arguments)?;
            CliCommand::Inspect { selector, format }
        }
        CommandName::Overview => CliCommand::Overview {
            format: parse_format_option(command, arguments)?,
        },
    };

    Ok(ParseOutcome::Run(CliRequest::new(cli_command)))
}

fn parse_configuration_arguments(arguments: &[OsString]) -> Result<CliCommand, ParseError> {
    let command = CommandName::Config;
    match arguments {
        [action] => {
            let action = utf8_for_command(action, command, "configuration action")?;
            if action == "show" {
                Ok(CliCommand::ConfigShow)
            } else if action.starts_with('-') {
                Err(ParseError::command(
                    command,
                    ParseErrorKind::UnknownOption {
                        command: Some(command),
                        option: action.to_owned(),
                    },
                ))
            } else {
                Err(ParseError::command(
                    command,
                    ParseErrorKind::InvalidConfigurationAction(action.to_owned()),
                ))
            }
        }
        _ => Err(ParseError::command(
            command,
            ParseErrorKind::ConfigurationActionCount,
        )),
    }
}

fn parse_discovery_arguments(
    arguments: &[OsString],
) -> Result<(Vec<PathBuf>, OutputFormat, DiagnosticDetail), ParseError> {
    let command = CommandName::Discover;
    let mut roots = Vec::new();
    let mut format = OutputFormat::Human;
    let mut diagnostics = DiagnosticDetail::Summary;
    let mut seen_format = false;
    let mut seen_verbose = false;
    let mut parse_options = true;
    let mut index = 0;

    while index < arguments.len() {
        let argument = &arguments[index];
        if parse_options && argument.to_str() == Some("--") {
            parse_options = false;
            index += 1;
            continue;
        }

        if parse_options && argument.to_str() == Some("--format") {
            if seen_format {
                return Err(ParseError::command(
                    command,
                    ParseErrorKind::DuplicateOption("--format"),
                ));
            }
            let value = arguments.get(index + 1).ok_or_else(|| {
                ParseError::command(command, ParseErrorKind::MissingOptionValue("--format"))
            })?;
            format = parse_output_format(value, command)?;
            seen_format = true;
            index += 2;
            continue;
        }

        if parse_options
            && argument
                .to_str()
                .is_some_and(|value| value.starts_with("--format="))
        {
            if seen_format {
                return Err(ParseError::command(
                    command,
                    ParseErrorKind::DuplicateOption("--format"),
                ));
            }
            let value = argument
                .to_str()
                .expect("checked UTF-8 option must remain UTF-8");
            format = parse_output_format_text(&value["--format=".len()..], command)?;
            seen_format = true;
            index += 1;
            continue;
        }

        if parse_options && argument.to_str() == Some("--verbose") {
            if seen_verbose {
                return Err(ParseError::command(
                    command,
                    ParseErrorKind::DuplicateOption("--verbose"),
                ));
            }
            diagnostics = DiagnosticDetail::Verbose;
            seen_verbose = true;
            index += 1;
            continue;
        }

        if parse_options
            && argument
                .to_str()
                .is_some_and(|value| value.starts_with('-'))
        {
            let option = argument.to_string_lossy();
            return Err(ParseError::command(
                command,
                ParseErrorKind::UnknownOption {
                    command: Some(command),
                    option: option.into_owned(),
                },
            ));
        }
        if argument.is_empty() {
            return Err(ParseError::command(
                command,
                ParseErrorKind::EmptyDiscoveryRoot,
            ));
        }

        roots.push(PathBuf::from(argument.as_os_str()));
        index += 1;
    }

    if diagnostics == DiagnosticDetail::Verbose && format == OutputFormat::Json {
        return Err(ParseError::command(
            command,
            ParseErrorKind::VerboseRequiresHumanOutput,
        ));
    }

    Ok((roots, format, diagnostics))
}

fn parse_repository_options(
    arguments: &[OsString],
) -> Result<(QueryPage, OutputFormat), ParseError> {
    let command = CommandName::Repositories;
    let mut offset = 0_usize;
    let mut limit = DEFAULT_QUERY_PAGE_SIZE;
    let mut format = OutputFormat::Human;
    let mut seen_offset = false;
    let mut seen_limit = false;
    let mut seen_format = false;
    let mut index = 0;

    while index < arguments.len() {
        let argument = utf8_for_command(&arguments[index], command, "repository option")?;
        match argument {
            "--offset" => {
                if seen_offset {
                    return Err(ParseError::command(
                        command,
                        ParseErrorKind::DuplicateOption("--offset"),
                    ));
                }
                let value = arguments.get(index + 1).ok_or_else(|| {
                    ParseError::command(command, ParseErrorKind::MissingOptionValue("--offset"))
                })?;
                offset = parse_usize_value(value, command, "--offset")?;
                seen_offset = true;
                index += 2;
            }
            "--limit" => {
                if seen_limit {
                    return Err(ParseError::command(
                        command,
                        ParseErrorKind::DuplicateOption("--limit"),
                    ));
                }
                let value = arguments.get(index + 1).ok_or_else(|| {
                    ParseError::command(command, ParseErrorKind::MissingOptionValue("--limit"))
                })?;
                limit = parse_usize_value(value, command, "--limit")?;
                seen_limit = true;
                index += 2;
            }
            "--format" => {
                if seen_format {
                    return Err(ParseError::command(
                        command,
                        ParseErrorKind::DuplicateOption("--format"),
                    ));
                }
                let value = arguments.get(index + 1).ok_or_else(|| {
                    ParseError::command(command, ParseErrorKind::MissingOptionValue("--format"))
                })?;
                format = parse_output_format(value, command)?;
                seen_format = true;
                index += 2;
            }
            _ if argument.starts_with("--offset=") => {
                if seen_offset {
                    return Err(ParseError::command(
                        command,
                        ParseErrorKind::DuplicateOption("--offset"),
                    ));
                }
                offset = parse_usize_text(&argument["--offset=".len()..], command, "--offset")?;
                seen_offset = true;
                index += 1;
            }
            _ if argument.starts_with("--limit=") => {
                if seen_limit {
                    return Err(ParseError::command(
                        command,
                        ParseErrorKind::DuplicateOption("--limit"),
                    ));
                }
                limit = parse_usize_text(&argument["--limit=".len()..], command, "--limit")?;
                seen_limit = true;
                index += 1;
            }
            _ if argument.starts_with("--format=") => {
                if seen_format {
                    return Err(ParseError::command(
                        command,
                        ParseErrorKind::DuplicateOption("--format"),
                    ));
                }
                format = parse_output_format_text(&argument["--format=".len()..], command)?;
                seen_format = true;
                index += 1;
            }
            _ if argument.starts_with('-') => {
                return Err(ParseError::command(
                    command,
                    ParseErrorKind::UnknownOption {
                        command: Some(command),
                        option: argument.to_owned(),
                    },
                ));
            }
            _ => {
                return Err(ParseError::command(
                    command,
                    ParseErrorKind::RepositoriesOptionsOnly,
                ));
            }
        }
    }

    let page = QueryPage::new(offset, limit).ok_or_else(|| {
        ParseError::command(
            command,
            ParseErrorKind::LimitOutOfRange(MAX_QUERY_PAGE_SIZE),
        )
    })?;
    Ok((page, format))
}

fn parse_format_option(
    command: CommandName,
    arguments: &[OsString],
) -> Result<OutputFormat, ParseError> {
    match arguments {
        [] => Ok(OutputFormat::Human),
        [option, value] if option.to_str() == Some("--format") => {
            parse_output_format(value, command)
        }
        [option] => {
            let option = utf8_for_command(option, command, "option")?;
            if let Some(value) = option.strip_prefix("--format=") {
                parse_output_format_text(value, command)
            } else {
                Err(ParseError::command(
                    command,
                    ParseErrorKind::UnknownOption {
                        command: Some(command),
                        option: option.to_owned(),
                    },
                ))
            }
        }
        _ => Err(ParseError::command(
            command,
            ParseErrorKind::FormatOptionOnly(command),
        )),
    }
}

fn ensure_command_has_no_arguments(
    command: CommandName,
    arguments: &[OsString],
) -> Result<(), ParseError> {
    if arguments.is_empty() {
        Ok(())
    } else {
        Err(ParseError::command(
            command,
            ParseErrorKind::CommandTakesNoArguments(command),
        ))
    }
}

fn parse_output_format(value: &OsString, command: CommandName) -> Result<OutputFormat, ParseError> {
    let value = utf8_for_command(value, command, "--format")?;
    parse_output_format_text(value, command)
}

fn parse_output_format_text(value: &str, command: CommandName) -> Result<OutputFormat, ParseError> {
    match value {
        "human" => Ok(OutputFormat::Human),
        "json" => Ok(OutputFormat::Json),
        _ => Err(ParseError::command(
            command,
            ParseErrorKind::InvalidOutputFormat,
        )),
    }
}

fn parse_inspect_arguments(arguments: &[OsString]) -> Result<(OsString, OutputFormat), ParseError> {
    let command = CommandName::Inspect;
    let mut selector = None;
    let mut format = OutputFormat::Human;
    let mut seen_format = false;
    let mut parse_options = true;
    let mut index = 0;

    while index < arguments.len() {
        let argument = &arguments[index];
        if parse_options && argument.to_str() == Some("--") {
            parse_options = false;
            index += 1;
            continue;
        }
        if parse_options && argument.to_str() == Some("--format") {
            if seen_format {
                return Err(ParseError::command(
                    command,
                    ParseErrorKind::DuplicateOption("--format"),
                ));
            }
            let value = arguments.get(index + 1).ok_or_else(|| {
                ParseError::command(command, ParseErrorKind::MissingOptionValue("--format"))
            })?;
            format = parse_output_format(value, command)?;
            seen_format = true;
            index += 2;
            continue;
        }
        if parse_options
            && argument
                .to_str()
                .is_some_and(|value| value.starts_with("--format="))
        {
            if seen_format {
                return Err(ParseError::command(
                    command,
                    ParseErrorKind::DuplicateOption("--format"),
                ));
            }
            let value = argument
                .to_str()
                .expect("checked UTF-8 option must remain UTF-8");
            format = parse_output_format_text(&value["--format=".len()..], command)?;
            seen_format = true;
            index += 1;
            continue;
        }
        if parse_options
            && argument
                .to_str()
                .is_some_and(|value| value.starts_with('-'))
        {
            return Err(ParseError::command(
                command,
                ParseErrorKind::UnknownOption {
                    command: Some(command),
                    option: argument.to_string_lossy().into_owned(),
                },
            ));
        }
        if selector.is_some() {
            return Err(ParseError::command(
                command,
                ParseErrorKind::InspectSelectorCount,
            ));
        }
        if argument.is_empty() {
            return Err(ParseError::command(
                command,
                ParseErrorKind::InvalidRepositorySelector,
            ));
        }
        selector = Some(argument.clone());
        index += 1;
    }

    let selector = selector
        .ok_or_else(|| ParseError::command(command, ParseErrorKind::InspectSelectorCount))?;
    Ok((selector, format))
}

fn parse_usize_value(
    value: &OsString,
    command: CommandName,
    option: &str,
) -> Result<usize, ParseError> {
    let value = utf8_for_command(value, command, option)?;
    parse_usize_text(value, command, option)
}

fn parse_usize_text(value: &str, command: CommandName, option: &str) -> Result<usize, ParseError> {
    value.parse::<usize>().map_err(|_| {
        ParseError::command(command, ParseErrorKind::InvalidInteger(option.to_owned()))
    })
}

fn utf8_for_command<'a>(
    value: &'a OsString,
    command: CommandName,
    label: &str,
) -> Result<&'a str, ParseError> {
    value
        .to_str()
        .ok_or_else(|| ParseError::command(command, ParseErrorKind::InvalidUtf8(label.to_owned())))
}

fn ensure_no_trailing_arguments(
    arguments: &[OsString],
    outcome: ParseOutcome,
) -> Result<ParseOutcome, ParseError> {
    if arguments.is_empty() {
        Ok(outcome)
    } else {
        Err(ParseError::root(ParseErrorKind::UnexpectedGlobalArgument))
    }
}

fn utf8<'a>(value: &'a OsString, label: &str) -> Result<&'a str, ParseError> {
    value
        .to_str()
        .ok_or_else(|| ParseError::root(ParseErrorKind::InvalidUtf8(label.to_owned())))
}

fn is_help(value: &OsString) -> bool {
    matches!(value.to_str(), Some("-h" | "--help"))
}

fn is_version(value: &OsString) -> bool {
    matches!(value.to_str(), Some("-V" | "--version"))
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::PathBuf;

    use bulls_application::{DEFAULT_QUERY_PAGE_SIZE, QueryPage};

    use super::{
        CliCommand, CommandName, DiagnosticDetail, HelpTopic, OutputFormat, ParseOutcome, parse,
    };

    #[test]
    fn empty_arguments_show_root_help() {
        assert_eq!(
            parse::<_, &str>([]).unwrap(),
            ParseOutcome::Help(HelpTopic::Root)
        );
    }

    #[test]
    fn global_help_and_version_are_terminal_actions() {
        assert_eq!(
            parse(["--help"]).unwrap(),
            ParseOutcome::Help(HelpTopic::Root)
        );
        assert_eq!(parse(["-V"]).unwrap(), ParseOutcome::Version);
        assert!(parse(["--version", "overview"]).is_err());
    }

    #[test]
    fn help_command_resolves_known_topics() {
        assert_eq!(
            parse(["help", "refresh"]).unwrap(),
            ParseOutcome::Help(HelpTopic::Command(CommandName::Refresh))
        );
        assert!(parse(["help", "unknown"]).is_err());
    }

    #[test]
    fn command_help_does_not_build_a_runtime_request() {
        assert_eq!(
            parse(["overview", "--help"]).unwrap(),
            ParseOutcome::Help(HelpTopic::Command(CommandName::Overview))
        );
    }

    #[test]
    fn format_only_commands_reject_positional_arguments() {
        for command in ["refresh", "overview"] {
            assert!(parse([command, "unexpected"]).is_err());
        }
    }

    #[test]
    fn reset_is_an_explicit_no_argument_command() {
        assert_eq!(
            parse(["reset"]).unwrap(),
            ParseOutcome::Run(super::CliRequest::new(CliCommand::Reset))
        );
        assert_eq!(
            parse(["reset", "--help"]).unwrap(),
            ParseOutcome::Help(HelpTopic::Command(CommandName::Reset))
        );
        assert!(matches!(
            parse(["reset", "--format=json"]).unwrap_err().kind(),
            super::ParseErrorKind::CommandTakesNoArguments(CommandName::Reset)
        ));
    }

    #[test]
    fn configuration_inspection_requires_explicit_show_action() {
        assert_eq!(
            parse(["config", "show"]).unwrap(),
            ParseOutcome::Run(super::CliRequest::new(CliCommand::ConfigShow))
        );
        assert_eq!(
            parse(["config", "--help"]).unwrap(),
            ParseOutcome::Help(HelpTopic::Command(CommandName::Config))
        );
        assert!(matches!(
            parse(["config"]).unwrap_err().kind(),
            super::ParseErrorKind::ConfigurationActionCount
        ));
        assert!(matches!(
            parse(["config", "set"]).unwrap_err().kind(),
            super::ParseErrorKind::InvalidConfigurationAction(action) if action == "set"
        ));
        assert!(parse(["config", "show", "extra"]).is_err());
    }

    #[test]
    fn inspect_requires_exactly_one_repository_selector() {
        assert!(parse(["inspect"]).is_err());
        assert!(parse(["inspect", "repo-1", "repo-2"]).is_err());
        assert!(parse(["inspect", "--unknown"]).is_err());

        let parsed = parse(["inspect", "repo-1"]).unwrap();
        assert_eq!(
            parsed,
            ParseOutcome::Run(super::CliRequest::new(CliCommand::Inspect {
                selector: OsString::from("repo-1"),
                format: OutputFormat::Human,
            }))
        );
    }

    #[test]
    fn all_public_commands_have_typed_requests() {
        let cases = [
            (
                "refresh",
                CliCommand::Refresh {
                    format: OutputFormat::Human,
                },
            ),
            (
                "repos",
                CliCommand::Repositories {
                    page: QueryPage::new(0, DEFAULT_QUERY_PAGE_SIZE)
                        .expect("default page must be valid"),
                    format: OutputFormat::Human,
                },
            ),
            (
                "overview",
                CliCommand::Overview {
                    format: OutputFormat::Human,
                },
            ),
        ];

        for (argument, expected) in cases {
            let ParseOutcome::Run(request) = parse([argument]).unwrap() else {
                panic!("command must produce a runtime request");
            };
            assert_eq!(request.into_command(), expected);
        }
    }

    #[test]
    fn repositories_accept_explicit_pagination_without_duplicating_query_limits() {
        let parsed = parse(["repos", "--offset", "25", "--limit=50"]).unwrap();
        assert_eq!(
            parsed,
            ParseOutcome::Run(super::CliRequest::new(CliCommand::Repositories {
                page: QueryPage::new(25, 50).expect("page must be valid"),
                format: OutputFormat::Human,
            }))
        );

        assert!(parse(["repos", "--limit", "0"]).is_err());
        assert!(parse(["repos", "--limit", "257"]).is_err());
        assert!(parse(["repos", "--offset", "not-a-number"]).is_err());
        assert!(parse(["repos", "--limit", "5", "--limit", "6"]).is_err());
        assert!(parse(["repos", "unexpected"]).is_err());
    }

    #[test]
    fn commands_with_structured_output_accept_json_without_changing_the_human_default() {
        let repos = parse(["repos", "--format=json"]).unwrap();
        assert_eq!(
            repos,
            ParseOutcome::Run(super::CliRequest::new(CliCommand::Repositories {
                page: QueryPage::new(0, DEFAULT_QUERY_PAGE_SIZE)
                    .expect("default page must be valid"),
                format: OutputFormat::Json,
            }))
        );

        let inspect = parse(["inspect", "repo-1", "--format", "json"]).unwrap();
        assert_eq!(
            inspect,
            ParseOutcome::Run(super::CliRequest::new(CliCommand::Inspect {
                selector: OsString::from("repo-1"),
                format: OutputFormat::Json,
            }))
        );

        let overview = parse(["overview", "--format", "json"]).unwrap();
        assert_eq!(
            overview,
            ParseOutcome::Run(super::CliRequest::new(CliCommand::Overview {
                format: OutputFormat::Json,
            }))
        );

        let refresh = parse(["refresh", "--format", "json"]).unwrap();
        assert_eq!(
            refresh,
            ParseOutcome::Run(super::CliRequest::new(CliCommand::Refresh {
                format: OutputFormat::Json,
            }))
        );

        let discover = parse(["discover", "--format=json", "--verbose"]).unwrap_err();
        assert_eq!(
            discover.kind(),
            &super::ParseErrorKind::VerboseRequiresHumanOutput
        );

        assert!(parse(["overview", "--format", "yaml"]).is_err());
        assert!(parse(["repos", "--format=json", "--format", "human"]).is_err());
    }

    #[test]
    fn discover_parses_format_and_verbose_without_converting_roots_to_strings() {
        let parsed = parse(["discover", "--verbose", "/workspace", "--format", "human"]).unwrap();
        assert_eq!(
            parsed,
            ParseOutcome::Run(super::CliRequest::new(CliCommand::Discover {
                roots: vec![PathBuf::from("/workspace")],
                format: OutputFormat::Human,
                diagnostics: DiagnosticDetail::Verbose,
            }))
        );

        assert!(parse(["discover", "--verbose", "--verbose"]).is_err());
        assert!(parse(["discover", "--format=json", "--format", "human"]).is_err());
    }

    #[test]
    fn discover_accepts_transient_roots_without_requiring_utf8_conversion() {
        let parsed = parse(["discover", "/workspace/one", "/workspace/two"]).unwrap();
        assert_eq!(
            parsed,
            ParseOutcome::Run(super::CliRequest::new(CliCommand::Discover {
                roots: vec![
                    PathBuf::from("/workspace/one"),
                    PathBuf::from("/workspace/two"),
                ],
                format: OutputFormat::Human,
                diagnostics: DiagnosticDetail::Summary,
            }))
        );
    }

    #[test]
    fn discover_rejects_unknown_options_but_supports_the_option_terminator() {
        assert!(parse(["discover", "--unknown"]).is_err());

        let parsed = parse(["discover", "--", "-repository"]).unwrap();
        assert_eq!(
            parsed,
            ParseOutcome::Run(super::CliRequest::new(CliCommand::Discover {
                roots: vec![PathBuf::from("-repository")],
                format: OutputFormat::Human,
                diagnostics: DiagnosticDetail::Summary,
            }))
        );
    }

    #[cfg(unix)]
    #[test]
    fn discover_preserves_non_utf8_root_arguments() {
        use std::os::unix::ffi::OsStringExt;

        let root = OsString::from_vec(vec![b'/', b't', b'm', b'p', b'/', 0xff]);
        let parsed = parse([OsString::from("discover"), root.clone()]).unwrap();
        assert_eq!(
            parsed,
            ParseOutcome::Run(super::CliRequest::new(CliCommand::Discover {
                roots: vec![PathBuf::from(root)],
                format: OutputFormat::Human,
                diagnostics: DiagnosticDetail::Summary,
            }))
        );
    }

    #[cfg(unix)]
    #[test]
    fn inspect_preserves_non_utf8_path_selectors() {
        use std::os::unix::ffi::OsStringExt;

        let selector = OsString::from_vec(vec![b'/', b't', b'm', b'p', b'/', 0xff]);
        let parsed = parse([OsString::from("inspect"), selector.clone()]).unwrap();
        assert_eq!(
            parsed,
            ParseOutcome::Run(super::CliRequest::new(CliCommand::Inspect {
                selector,
                format: OutputFormat::Human,
            }))
        );
    }

    #[test]
    fn inspect_option_terminator_allows_path_selectors_starting_with_dash() {
        let parsed = parse(["inspect", "--", "-repository"]).unwrap();
        assert_eq!(
            parsed,
            ParseOutcome::Run(super::CliRequest::new(CliCommand::Inspect {
                selector: OsString::from("-repository"),
                format: OutputFormat::Human,
            }))
        );
    }

    #[test]
    fn unknown_root_syntax_is_rejected_before_runtime_startup() {
        assert!(parse(["unknown"]).is_err());
        assert!(parse(["--unknown"]).is_err());
    }

    #[test]
    fn only_long_running_runtime_commands_install_cancellation() {
        let discover = match parse(["discover"]).unwrap() {
            ParseOutcome::Run(request) => request,
            other => panic!("expected discover request, got {other:?}"),
        };
        let refresh = match parse(["refresh"]).unwrap() {
            ParseOutcome::Run(request) => request,
            other => panic!("expected refresh request, got {other:?}"),
        };
        let repositories = match parse(["repos"]).unwrap() {
            ParseOutcome::Run(request) => request,
            other => panic!("expected repositories request, got {other:?}"),
        };
        let overview = match parse(["overview"]).unwrap() {
            ParseOutcome::Run(request) => request,
            other => panic!("expected overview request, got {other:?}"),
        };

        assert!(discover.supports_cancellation());
        assert!(refresh.supports_cancellation());
        assert!(!repositories.supports_cancellation());
        assert!(!overview.supports_cancellation());
    }
}
