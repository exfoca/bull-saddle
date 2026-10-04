// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use std::io::{self, Write};
use std::time::{Duration, SystemTime};

use bulls_application::ports::{BareRepositoryDiscovery, FilesystemBoundary, SymlinkTraversal};
use bulls_application::{
    Advisory, AdvisoryCode, AdvisoryEvidence, AdvisoryProjection, AdvisorySubject,
    ConfigurationProjection, ConfigurationSource, DEFAULT_QUERY_PAGE_SIZE, DiscoveryBatchOutcome,
    FailureCause, FailureClass, Head, Knowledge, LocationAvailability, MAX_QUERY_PAGE_SIZE,
    ObservationCoverage, ObservationEvidenceState, ObservationFailureKind,
    ObservationStatusProjection, ObserveInventoryOutcome, OperationCompletion, OverviewProjection,
    PublicFailureReport, RepositoryDetailProjection, RepositoryListProjection, UpstreamState,
};
use serde::Serialize;

use crate::cli::{CommandName, DiagnosticDetail, HelpTopic};
use crate::i18n::{Locale, Message, render as render_message};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum JsonOutputError {
    Serialization,
    Output(io::ErrorKind),
}

pub(crate) fn json(writer: &mut impl Write, value: &impl Serialize) -> Result<(), JsonOutputError> {
    let payload = serde_json::to_vec(value).map_err(|_| JsonOutputError::Serialization)?;
    writer
        .write_all(&payload)
        .map_err(|error| JsonOutputError::Output(error.kind()))?;
    writer
        .write_all(b"\n")
        .map_err(|error| JsonOutputError::Output(error.kind()))
}

pub(crate) fn failure_report(
    writer: &mut impl Write,
    report: &PublicFailureReport,
    locale: Locale,
) -> io::Result<()> {
    let failure = report.failure();
    writeln!(
        writer,
        "{}",
        render_message(locale, Message::FailureReportTitle)
    )?;
    writeln!(
        writer,
        "{}: {}",
        render_message(locale, Message::FailureReportSchemaVersion),
        report.schema_version()
    )?;
    writeln!(
        writer,
        "{}: {}",
        render_message(locale, Message::FailureReportApplicationVersion),
        report.application_version()
    )?;
    writeln!(
        writer,
        "{}: {}",
        render_message(locale, Message::FailureReportClass),
        failure_class(locale, failure.class())
    )?;
    writeln!(
        writer,
        "{}: {}",
        render_message(locale, Message::FailureReportErrorCode),
        failure.error_code().as_str()
    )?;
    if failure.causes().is_empty() {
        writeln!(
            writer,
            "{}: {}",
            render_message(locale, Message::FailureReportCauses),
            render_message(locale, Message::FailureReportNone)
        )?;
    } else {
        writeln!(
            writer,
            "{}:",
            render_message(locale, Message::FailureReportCauses)
        )?;
        for cause in failure.causes() {
            writeln!(writer, "  - {}", failure_cause(locale, cause))?;
        }
    }
    Ok(())
}

fn failure_class(locale: Locale, class: FailureClass) -> String {
    let message = match class {
        FailureClass::Expected => Message::FailureClassExpected,
        FailureClass::Operational => Message::FailureClassOperational,
        FailureClass::Defect => Message::FailureClassDefect,
    };
    render_message(locale, message)
}

fn failure_cause(locale: Locale, cause: &FailureCause) -> String {
    let message = match cause {
        FailureCause::PermissionDenied => Message::FailureCausePermissionDenied,
        FailureCause::ResourceUnavailable => Message::FailureCauseResourceUnavailable,
        FailureCause::ProcessExited { exit_code } => {
            let exit_code = exit_code.to_string();
            return render_message(
                locale,
                Message::FailureCauseProcessExited {
                    exit_code: &exit_code,
                },
            );
        }
        FailureCause::IoFailure => Message::FailureCauseIoFailure,
        FailureCause::StorageFailure => Message::FailureCauseStorageFailure,
        FailureCause::InvalidData => Message::FailureCauseInvalidData,
        FailureCause::OutputLimitExceeded => Message::FailureCauseOutputLimitExceeded,
        FailureCause::TimedOut => Message::FailureCauseTimedOut,
        FailureCause::Cancelled => Message::FailureCauseCancelled,
        FailureCause::InvariantViolation => Message::FailureCauseInvariantViolation,
        FailureCause::Unexpected => Message::FailureCauseUnexpected,
    };
    render_message(locale, message)
}

pub(crate) fn help(writer: &mut impl Write, topic: HelpTopic, locale: Locale) -> io::Result<()> {
    match topic {
        HelpTopic::Root => root_help(writer, locale),
        HelpTopic::Command(command) => command_help(writer, command, locale),
    }
}

pub(crate) fn version(writer: &mut impl Write) -> io::Result<()> {
    writeln!(writer, "bulls {}", env!("CARGO_PKG_VERSION"))
}

pub(crate) fn discovery(
    writer: &mut impl Write,
    outcome: &DiscoveryBatchOutcome,
    diagnostics: DiagnosticDetail,
    suggest_refresh: bool,
    locale: Locale,
) -> io::Result<()> {
    let roots_requested = outcome.requested_root_count().to_string();
    let roots_completed = outcome.completed_root_count().to_string();
    let roots_partial = outcome.partial_root_count().to_string();
    let roots_failed = outcome.failed_root_count().to_string();
    let discovery_issues = outcome.discovery_issue_count().to_string();
    let identification_issues = outcome.identification_issue_count().to_string();
    let repositories = outcome.repository_count().to_string();
    let locations = outcome.location_count().to_string();
    let worktrees = outcome.worktree_count().to_string();

    writeln!(writer, "{}", render_message(locale, Message::Discovery))?;
    writeln!(
        writer,
        "{}",
        render_message(locale, operation_status_message(outcome.completion()))
    )?;
    writeln!(
        writer,
        "{}",
        render_message(
            locale,
            Message::DiscoveryRootsRequested {
                count: &roots_requested,
            },
        )
    )?;
    writeln!(
        writer,
        "{}",
        render_message(
            locale,
            Message::DiscoveryRootsCompleted {
                count: &roots_completed,
            },
        )
    )?;
    writeln!(
        writer,
        "{}",
        render_message(
            locale,
            Message::DiscoveryRootsPartial {
                count: &roots_partial,
            },
        )
    )?;
    writeln!(
        writer,
        "{}",
        render_message(
            locale,
            Message::DiscoveryRootsFailed {
                count: &roots_failed,
            },
        )
    )?;
    writeln!(
        writer,
        "{}",
        render_message(
            locale,
            Message::DiscoveryIssues {
                count: &discovery_issues,
            },
        )
    )?;
    writeln!(
        writer,
        "{}",
        render_message(
            locale,
            Message::DiscoveryIdentificationIssues {
                count: &identification_issues,
            },
        )
    )?;
    writeln!(
        writer,
        "{}",
        render_message(
            locale,
            Message::DiscoveryRepositories {
                count: &repositories,
            },
        )
    )?;
    writeln!(
        writer,
        "{}",
        render_message(locale, Message::DiscoveryLocations { count: &locations },)
    )?;
    writeln!(
        writer,
        "{}",
        render_message(locale, Message::DiscoveryWorktrees { count: &worktrees })
    )?;

    if diagnostics == DiagnosticDetail::Verbose {
        discovery_diagnostics(writer, outcome, locale)?;
    }

    if outcome.requested_root_count() == 0 {
        writeln!(
            writer,
            "{}",
            render_message(locale, Message::DiscoveryHintRoots)
        )?;
    }
    if suggest_refresh {
        write_refresh_hint(writer, locale)?;
    }
    Ok(())
}

fn discovery_diagnostics(
    writer: &mut impl Write,
    outcome: &DiscoveryBatchOutcome,
    locale: Locale,
) -> io::Result<()> {
    if outcome.discovery_issue_count() == 0
        && outcome.identification_issue_count() == 0
        && outcome.failed_root_count() == 0
    {
        return Ok(());
    }

    writeln!(
        writer,
        "{}",
        render_message(locale, Message::DiscoveryDetails)
    )?;
    for root in outcome.completed() {
        if root.discovery_issues().is_empty() && root.identification_issues().is_empty() {
            continue;
        }

        let root_path = format!("{:?}", root.root());
        writeln!(
            writer,
            "  {}",
            render_message(locale, Message::DiscoveryRoot { path: &root_path },)
        )?;
        for issue in root.discovery_issues() {
            let issue_path = format!("{:?}", issue.path());
            writeln!(
                writer,
                "    {}",
                render_message(locale, Message::DiscoveryIssue { path: &issue_path },)
            )?;
            write_cause(writer, "      ", issue.kind().failure_cause(), locale)?;
        }
        for issue in root.identification_issues() {
            let issue_path = format!("{:?}", issue.path());
            writeln!(
                writer,
                "    {}",
                render_message(
                    locale,
                    Message::DiscoveryIdentificationIssue { path: &issue_path },
                )
            )?;
            write_cause(writer, "      ", issue.kind().failure_cause(), locale)?;
        }
    }

    for failure in outcome.failed_roots() {
        let root_path = format!("{:?}", failure.root());
        writeln!(
            writer,
            "  {}",
            render_message(locale, Message::DiscoveryFailedRoot { path: &root_path },)
        )?;
        write_cause(writer, "    ", failure.kind().failure_cause(), locale)?;
    }

    Ok(())
}

fn write_cause(
    writer: &mut impl Write,
    indent: &str,
    cause: FailureCause,
    locale: Locale,
) -> io::Result<()> {
    let cause = failure_cause(locale, &cause);
    writeln!(
        writer,
        "{indent}{}",
        render_message(locale, Message::DiscoveryCause { cause: &cause })
    )
}

pub(crate) fn refresh(
    writer: &mut impl Write,
    outcome: &ObserveInventoryOutcome,
    locale: Locale,
) -> io::Result<()> {
    let targets = outcome.target_count().to_string();
    let succeeded = outcome.succeeded_count().to_string();
    let failed = outcome.failed_count().to_string();

    writeln!(writer, "{}", render_message(locale, Message::Refresh))?;
    writeln!(
        writer,
        "{}",
        render_message(locale, operation_status_message(outcome.completion()))
    )?;
    writeln!(
        writer,
        "{}",
        render_message(locale, Message::RefreshTargets { count: &targets })
    )?;
    writeln!(
        writer,
        "{}",
        render_message(locale, Message::RefreshSucceeded { count: &succeeded })
    )?;
    writeln!(
        writer,
        "{}",
        render_message(locale, Message::RefreshFailed { count: &failed })
    )
}

const fn operation_status_message(completion: OperationCompletion) -> Message<'static> {
    match completion {
        OperationCompletion::Complete => Message::OperationStatusComplete,
        OperationCompletion::Partial => Message::OperationStatusPartial,
        OperationCompletion::Failed => Message::OperationStatusFailed,
        OperationCompletion::Cancelled => Message::OperationStatusCancelled,
    }
}

pub(crate) fn reset(writer: &mut impl Write, locale: Locale) -> io::Result<()> {
    writeln!(writer, "{}", render_message(locale, Message::ResetTitle))?;
    writeln!(
        writer,
        "{}",
        render_message(locale, Message::OperationStatusComplete)
    )?;
    writeln!(
        writer,
        "{}",
        render_message(locale, Message::ResetDiscarded)
    )?;
    writeln!(
        writer,
        "{}",
        render_message(locale, Message::ResetRebuildHint)
    )
}

pub(crate) fn configuration(
    writer: &mut impl Write,
    projection: &ConfigurationProjection,
    locale: Locale,
) -> io::Result<()> {
    let configuration = projection.configuration();
    let file_path = format!("{:?}", projection.file_path());
    let locale_value = configuration.locale().explicit_value().unwrap_or("auto");
    let observation_parallelism = configuration
        .observation_policy()
        .max_parallelism()
        .to_string();
    let git_process_timeout = configuration
        .git_process_timeout()
        .milliseconds()
        .to_string();

    writeln!(
        writer,
        "{}",
        render_message(locale, Message::ConfigurationTitle)
    )?;
    writeln!(
        writer,
        "{}",
        render_message(locale, Message::ConfigurationFile { path: &file_path },)
    )?;
    writeln!(
        writer,
        "{}",
        render_message(
            locale,
            match projection.source() {
                ConfigurationSource::Defaults => Message::ConfigurationSourceDefaults,
                ConfigurationSource::File => Message::ConfigurationSourceFile,
            },
        )
    )?;
    writeln!(
        writer,
        "{}",
        render_message(
            locale,
            Message::ConfigurationLocale {
                locale: locale_value,
            },
        )
    )?;

    write_configuration_paths(
        writer,
        configuration.discovery().roots(),
        locale,
        Message::ConfigurationDiscoveryRootsHeading,
        Message::ConfigurationDiscoveryRootsEmpty,
    )?;
    write_configuration_paths(
        writer,
        configuration.discovery().exclusions(),
        locale,
        Message::ConfigurationExclusionsHeading,
        Message::ConfigurationExclusionsEmpty,
    )?;

    writeln!(
        writer,
        "{}",
        render_message(
            locale,
            match configuration.discovery().bare_repositories() {
                BareRepositoryDiscovery::Disabled => {
                    Message::ConfigurationBareRepositoriesDisabled
                }
                BareRepositoryDiscovery::Enabled => Message::ConfigurationBareRepositoriesEnabled,
            },
        )
    )?;
    writeln!(
        writer,
        "{}",
        render_message(
            locale,
            match configuration.discovery().symlink_traversal() {
                SymlinkTraversal::DoNotFollow => {
                    Message::ConfigurationSymlinkTraversalDoNotFollow
                }
                SymlinkTraversal::FollowWithinRoot => {
                    Message::ConfigurationSymlinkTraversalFollowWithinRoot
                }
            },
        )
    )?;
    writeln!(
        writer,
        "{}",
        render_message(
            locale,
            match configuration.discovery().filesystem_boundary() {
                FilesystemBoundary::StayOnRootFilesystem => {
                    Message::ConfigurationFilesystemBoundaryStayOnRoot
                }
                FilesystemBoundary::CrossFilesystems => {
                    Message::ConfigurationFilesystemBoundaryCrossFilesystems
                }
            },
        )
    )?;
    writeln!(
        writer,
        "{}",
        render_message(
            locale,
            Message::ConfigurationObservationParallelism {
                count: &observation_parallelism,
            },
        )
    )?;
    writeln!(
        writer,
        "{}",
        render_message(
            locale,
            Message::ConfigurationGitProcessTimeout {
                milliseconds: &git_process_timeout,
            },
        )
    )
}

fn write_configuration_paths(
    writer: &mut impl Write,
    paths: &[std::path::PathBuf],
    locale: Locale,
    heading: Message<'static>,
    empty: Message<'static>,
) -> io::Result<()> {
    if paths.is_empty() {
        return writeln!(writer, "{}", render_message(locale, empty));
    }

    writeln!(writer, "{}", render_message(locale, heading))?;
    for path in paths {
        writeln!(writer, "  - {path:?}")?;
    }
    Ok(())
}

pub(crate) fn repositories(
    writer: &mut impl Write,
    projection: &RepositoryListProjection,
    locale: Locale,
) -> io::Result<()> {
    let has_unknown_observation_evidence = projection.items().iter().any(|repository| {
        matches!(repository.has_local_work(), Knowledge::Unknown)
            || matches!(repository.has_remotes(), Knowledge::Unknown)
    });

    writeln!(
        writer,
        "{}",
        render_message(locale, Message::RepositoriesTitle)
    )?;

    for (index, repository) in projection.items().iter().enumerate() {
        if index > 0 {
            writeln!(writer)?;
        }

        let repository_id = repository.id().to_string();
        writeln!(
            writer,
            "{}",
            render_message(
                locale,
                Message::RepositoriesRepository {
                    repository_id: &repository_id,
                },
            )
        )?;

        if repository.locations().is_empty() {
            writeln!(
                writer,
                "  {}",
                render_message(locale, Message::RepositoriesLocationsEmpty)
            )?;
        } else {
            writeln!(
                writer,
                "  {}",
                render_message(locale, Message::RepositoriesLocationsHeading)
            )?;
            for location in repository.locations() {
                let path = format!("{:?}", location.path());
                writeln!(
                    writer,
                    "    {}",
                    render_message(locale, location_message(location.availability(), &path),)
                )?;
            }
        }

        let worktree_count = repository.worktree_count().to_string();
        writeln!(
            writer,
            "  {}",
            render_message(
                locale,
                Message::RepositoriesWorktrees {
                    count: &worktree_count,
                },
            )
        )?;
        writeln!(
            writer,
            "  {}",
            render_message(
                locale,
                repository_local_work_message(repository.has_local_work()),
            )
        )?;
        writeln!(
            writer,
            "  {}",
            render_message(locale, repository_remotes_message(repository.has_remotes()))
        )?;
    }

    let shown = projection.items().len().to_string();
    let offset = projection.offset().to_string();
    let total = projection.total_matching().to_string();
    writeln!(writer)?;
    writeln!(
        writer,
        "{}",
        render_message(
            locale,
            Message::RepositoriesPagination {
                shown: &shown,
                offset: &offset,
                total: &total,
            },
        )
    )?;
    if has_unknown_observation_evidence {
        write_refresh_hint(writer, locale)?;
    }
    Ok(())
}

pub(crate) fn repository(
    writer: &mut impl Write,
    projection: &RepositoryDetailProjection,
    advisories: &AdvisoryProjection,
    locale: Locale,
) -> io::Result<()> {
    let now = SystemTime::now();
    let repository_id = projection.id().to_string();
    let location_count = projection.locations().len().to_string();

    writeln!(
        writer,
        "{}",
        render_message(
            locale,
            Message::RepositoryHeader {
                repository_id: &repository_id,
            },
        )
    )?;
    writeln!(
        writer,
        "{}",
        render_message(
            locale,
            Message::RepositoryLocations {
                count: &location_count,
            },
        )
    )?;
    for location in projection.locations() {
        let path = format!("{:?}", location.path());
        writeln!(
            writer,
            "  {}",
            render_message(locale, location_message(location.availability(), &path))
        )?;
    }

    match projection.remotes() {
        Some([]) => writeln!(
            writer,
            "{}",
            render_message(locale, Message::RepositoryRemotesNone)
        )?,
        Some(remotes) => {
            let remote_count = remotes.len().to_string();
            writeln!(
                writer,
                "{}",
                render_message(
                    locale,
                    Message::RepositoryRemotesCount {
                        count: &remote_count,
                    },
                )
            )?;
            for remote in remotes {
                writeln!(writer, "  {}", remote.name())?;
            }
        }
        None => writeln!(
            writer,
            "{}",
            unknown_observation_message(
                locale,
                projection.remotes_observation(),
                now,
                UnknownObservationContext::RepositoryRemotes,
            )
        )?,
    }
    if projection.remotes().is_some() {
        write_observation_status(writer, "", projection.remotes_observation(), now, locale)?;
    }

    let worktree_count = projection.worktrees().len().to_string();
    writeln!(
        writer,
        "{}",
        render_message(
            locale,
            Message::RepositoryWorktrees {
                count: &worktree_count,
            },
        )
    )?;
    for worktree in projection.worktrees() {
        let worktree_id = worktree.id().to_string();
        let path = format!("{:?}", worktree.location().path());

        writeln!(
            writer,
            "  {}",
            render_message(
                locale,
                Message::RepositoryWorktreeHeader {
                    worktree_id: &worktree_id,
                },
            )
        )?;
        writeln!(
            writer,
            "    {}",
            render_message(locale, Message::RepositoryWorktreePath { path: &path })
        )?;
        writeln!(
            writer,
            "    {}",
            render_message(
                locale,
                worktree_availability_message(worktree.location().availability()),
            )
        )?;

        match worktree.git_state() {
            Some(state) => {
                match state.head() {
                    Head::Unborn(branch) => writeln!(
                        writer,
                        "    {}",
                        render_message(
                            locale,
                            Message::RepositoryWorktreeHeadUnborn {
                                branch: branch.name(),
                            },
                        )
                    )?,
                    Head::Branch(branch) => writeln!(
                        writer,
                        "    {}",
                        render_message(
                            locale,
                            Message::RepositoryWorktreeHeadBranch {
                                branch: branch.name(),
                            },
                        )
                    )?,
                    Head::Detached => writeln!(
                        writer,
                        "    {}",
                        render_message(locale, Message::RepositoryWorktreeHeadDetached)
                    )?,
                }

                match state.upstream() {
                    Some(UpstreamState::Unconfigured) => writeln!(
                        writer,
                        "    {}",
                        render_message(locale, Message::RepositoryWorktreeUpstreamUnconfigured)
                    )?,
                    Some(UpstreamState::Gone(upstream)) => writeln!(
                        writer,
                        "    {}",
                        render_message(
                            locale,
                            Message::RepositoryWorktreeUpstreamGone {
                                remote: upstream.remote().name(),
                                branch: upstream.branch().name(),
                            },
                        )
                    )?,
                    Some(UpstreamState::Tracking {
                        upstream,
                        divergence,
                    }) => {
                        let ahead = divergence.ahead().to_string();
                        let behind = divergence.behind().to_string();
                        writeln!(
                            writer,
                            "    {}",
                            render_message(
                                locale,
                                Message::RepositoryWorktreeUpstreamTracking {
                                    remote: upstream.remote().name(),
                                    branch: upstream.branch().name(),
                                    ahead: &ahead,
                                    behind: &behind,
                                },
                            )
                        )?;
                    }
                    None => writeln!(
                        writer,
                        "    {}",
                        render_message(locale, Message::RepositoryWorktreeUpstreamNotApplicable)
                    )?,
                }

                let changes = state.changes();
                let staged = yes_no(locale, changes.staged());
                let unstaged = yes_no(locale, changes.unstaged());
                let untracked = yes_no(locale, changes.untracked());
                writeln!(
                    writer,
                    "    {}",
                    render_message(
                        locale,
                        Message::RepositoryWorktreeChanges {
                            staged: &staged,
                            unstaged: &unstaged,
                            untracked: &untracked,
                        },
                    )
                )?;
            }
            None => writeln!(
                writer,
                "    {}",
                unknown_observation_message(
                    locale,
                    worktree.observation(),
                    now,
                    UnknownObservationContext::WorktreeGitState,
                )
            )?,
        }

        if worktree.git_state().is_some() {
            write_observation_status(writer, "    ", worktree.observation(), now, locale)?;
        }
    }

    render_repository_advisories(writer, projection, advisories, locale)?;
    if projection.has_unknown_observation_evidence() {
        write_refresh_hint(writer, locale)?;
    }
    Ok(())
}

pub(crate) fn overview(
    writer: &mut impl Write,
    projection: &OverviewProjection,
    advisories: &AdvisoryProjection,
    locale: Locale,
) -> io::Result<()> {
    let repository_count = projection.repository_count().to_string();
    let available_repository_count = projection.available_repository_count().to_string();
    let worktree_count = projection.worktree_count().to_string();
    let local_work_count = projection.repositories_with_local_work_count().to_string();
    let unknown_local_work_count = projection
        .repositories_with_unknown_local_work_count()
        .to_string();
    let without_remotes_count = projection.repositories_without_remotes_count().to_string();
    let unknown_remotes_count = projection
        .repositories_with_unknown_remotes_count()
        .to_string();

    for message in [
        Message::OverviewRepositories {
            count: &repository_count,
        },
        Message::OverviewAvailableRepositories {
            count: &available_repository_count,
        },
        Message::OverviewWorktrees {
            count: &worktree_count,
        },
        Message::OverviewRepositoriesWithLocalWork {
            count: &local_work_count,
        },
        Message::OverviewRepositoriesWithUnknownLocalWork {
            count: &unknown_local_work_count,
        },
        Message::OverviewRepositoriesWithoutRemotes {
            count: &without_remotes_count,
        },
        Message::OverviewRepositoriesWithUnknownRemotes {
            count: &unknown_remotes_count,
        },
    ] {
        writeln!(writer, "{}", render_message(locale, message))?;
    }

    let advisory_count = advisories.advisories().len().to_string();
    writeln!(
        writer,
        "{}",
        render_message(
            locale,
            Message::AdvisoriesCount {
                count: &advisory_count,
            },
        )
    )?;
    for code in [
        AdvisoryCode::RepositoryWithoutRemotes,
        AdvisoryCode::RepositoryHasMultipleWorktrees,
        AdvisoryCode::WorktreeWithoutUpstream,
        AdvisoryCode::LocalCommitsAheadOfKnownUpstream,
    ] {
        let count = advisories
            .advisories()
            .iter()
            .filter(|advisory| advisory.code() == code)
            .count();
        if count > 0 {
            let count = count.to_string();
            writeln!(
                writer,
                "  {}",
                render_message(locale, advisory_summary_message(code, &count))
            )?;
        }
    }

    let unknown_count = advisories.unknown_rules().len().to_string();
    writeln!(
        writer,
        "{}",
        render_message(
            locale,
            Message::AdvisoriesEvidenceUnavailable {
                count: &unknown_count,
            },
        )
    )?;
    if projection.has_unknown_observation_evidence() {
        write_refresh_hint(writer, locale)?;
    }
    Ok(())
}

fn write_observation_status(
    writer: &mut impl Write,
    indent: &str,
    status: &ObservationStatusProjection,
    now: SystemTime,
    locale: Locale,
) -> io::Result<()> {
    writeln!(
        writer,
        "{indent}{}",
        observation_status(locale, status, now)
    )
}

fn observation_status(
    locale: Locale,
    status: &ObservationStatusProjection,
    now: SystemTime,
) -> String {
    match status.evidence_state() {
        ObservationEvidenceState::NeverObserved => {
            render_message(locale, Message::ObservationStatusNeverObserved)
        }
        ObservationEvidenceState::Observed {
            observed_at,
            coverage,
        } => {
            let age = relative_age(locale, observed_at, now);
            render_message(
                locale,
                match coverage {
                    ObservationCoverage::Complete => {
                        Message::ObservationStatusObservedComplete { age: &age }
                    }
                    ObservationCoverage::Partial => {
                        Message::ObservationStatusObservedPartial { age: &age }
                    }
                },
            )
        }
        ObservationEvidenceState::LatestAttemptFailed {
            attempted_at,
            failure,
            latest_success,
        } => {
            let failure = observation_failure(locale, failure);
            let attempted_age = relative_age(locale, attempted_at, now);
            match latest_success {
                Some(success) => {
                    let success_age = relative_age(locale, success.observed_at(), now);
                    render_message(
                        locale,
                        match success.coverage() {
                            ObservationCoverage::Complete => {
                                Message::ObservationStatusFailedLastSuccessComplete {
                                    failure: &failure,
                                    attempted_age: &attempted_age,
                                    success_age: &success_age,
                                }
                            }
                            ObservationCoverage::Partial => {
                                Message::ObservationStatusFailedLastSuccessPartial {
                                    failure: &failure,
                                    attempted_age: &attempted_age,
                                    success_age: &success_age,
                                }
                            }
                        },
                    )
                }
                None => render_message(
                    locale,
                    Message::ObservationStatusFailedNoSuccess {
                        failure: &failure,
                        age: &attempted_age,
                    },
                ),
            }
        }
    }
}

#[derive(Clone, Copy)]
enum UnknownObservationContext {
    RepositoryRemotes,
    WorktreeGitState,
}

fn unknown_observation_message(
    locale: Locale,
    status: &ObservationStatusProjection,
    now: SystemTime,
    context: UnknownObservationContext,
) -> String {
    match status.evidence_state() {
        ObservationEvidenceState::NeverObserved => render_message(
            locale,
            match context {
                UnknownObservationContext::RepositoryRemotes => {
                    Message::RepositoryRemotesUnknownNeverObserved
                }
                UnknownObservationContext::WorktreeGitState => {
                    Message::RepositoryWorktreeGitStateUnknownNeverObserved
                }
            },
        ),
        ObservationEvidenceState::LatestAttemptFailed {
            attempted_at,
            failure,
            latest_success: None,
        } => {
            let failure = observation_failure(locale, failure);
            let age = relative_age(locale, attempted_at, now);
            render_message(
                locale,
                match context {
                    UnknownObservationContext::RepositoryRemotes => {
                        Message::RepositoryRemotesUnknownFailed {
                            failure: &failure,
                            age: &age,
                        }
                    }
                    UnknownObservationContext::WorktreeGitState => {
                        Message::RepositoryWorktreeGitStateUnknownFailed {
                            failure: &failure,
                            age: &age,
                        }
                    }
                },
            )
        }
        _ => render_message(
            locale,
            match context {
                UnknownObservationContext::RepositoryRemotes => Message::RepositoryRemotesUnknown,
                UnknownObservationContext::WorktreeGitState => {
                    Message::RepositoryWorktreeGitStateUnknown
                }
            },
        ),
    }
}

fn observation_failure(locale: Locale, failure: ObservationFailureKind) -> String {
    let message = match failure {
        ObservationFailureKind::SubjectUnavailable => Message::ObservationFailureSubjectUnavailable,
        ObservationFailureKind::ProcessExited { exit_code } => {
            let exit_code = exit_code.to_string();
            return render_message(
                locale,
                Message::ObservationFailureProcessExited {
                    exit_code: &exit_code,
                },
            );
        }
        ObservationFailureKind::TimedOut => Message::ObservationFailureTimedOut,
        ObservationFailureKind::Cancelled => Message::ObservationFailureCancelled,
        ObservationFailureKind::OutputLimitExceeded => {
            Message::ObservationFailureOutputLimitExceeded
        }
        ObservationFailureKind::InvalidMachineOutput => {
            Message::ObservationFailureInvalidMachineOutput
        }
        ObservationFailureKind::SubjectDisappeared => Message::ObservationFailureSubjectDisappeared,
    };
    render_message(locale, message)
}

fn relative_age(locale: Locale, observed_at: SystemTime, now: SystemTime) -> String {
    let (message, duration) = match now.duration_since(observed_at) {
        Ok(age) => (TimeDirection::Ago, compact_duration(age)),
        Err(error) => (TimeDirection::In, compact_duration(error.duration())),
    };

    match message {
        TimeDirection::Ago => render_message(
            locale,
            Message::TimeAgo {
                duration: &duration,
            },
        ),
        TimeDirection::In => render_message(
            locale,
            Message::TimeIn {
                duration: &duration,
            },
        ),
    }
}

#[derive(Clone, Copy)]
enum TimeDirection {
    Ago,
    In,
}

fn compact_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 60 * 60 {
        format!("{}m", seconds / 60)
    } else if seconds < 24 * 60 * 60 {
        format!("{}h", seconds / (60 * 60))
    } else {
        format!("{}d", seconds / (24 * 60 * 60))
    }
}

fn write_refresh_hint(writer: &mut impl Write, locale: Locale) -> io::Result<()> {
    writeln!(
        writer,
        "{}",
        render_message(locale, Message::RefreshHintUnknownGitState)
    )
}

fn render_repository_advisories(
    writer: &mut impl Write,
    projection: &RepositoryDetailProjection,
    advisories: &AdvisoryProjection,
    locale: Locale,
) -> io::Result<()> {
    let relevant = advisories
        .advisories()
        .iter()
        .filter(|advisory| advisory_matches_repository(advisory, projection))
        .collect::<Vec<_>>();
    let unknown_count = advisories
        .unknown_rules()
        .iter()
        .filter(|unknown| subject_matches_repository(unknown.subject(), projection))
        .count();

    let advisory_count = relevant.len().to_string();
    writeln!(
        writer,
        "{}",
        render_message(
            locale,
            Message::AdvisoriesCount {
                count: &advisory_count,
            },
        )
    )?;
    for advisory in relevant {
        writeln!(writer, "  - {}", advisory_message(locale, advisory))?;
    }

    let unknown_count = unknown_count.to_string();
    writeln!(
        writer,
        "{}",
        render_message(
            locale,
            Message::AdvisoriesEvidenceUnavailable {
                count: &unknown_count,
            },
        )
    )
}

fn advisory_message(locale: Locale, advisory: &Advisory) -> String {
    match (advisory.subject(), advisory.evidence()) {
        (
            AdvisorySubject::Repository(repository_id),
            AdvisoryEvidence::RepositoryRemoteCount { remote_count: 0 },
        ) => render_message(
            locale,
            Message::AdvisoryRepositoryNoRemotes {
                repository_id: repository_id.as_str(),
            },
        ),
        (
            AdvisorySubject::Repository(repository_id),
            AdvisoryEvidence::RepositoryRemoteCount { remote_count },
        ) => {
            let count = remote_count.to_string();
            render_message(
                locale,
                Message::AdvisoryRepositoryRemoteCount {
                    repository_id: repository_id.as_str(),
                    count: &count,
                },
            )
        }
        (
            AdvisorySubject::Repository(repository_id),
            AdvisoryEvidence::RepositoryWorktreeCount { worktree_count },
        ) => {
            let count = worktree_count.to_string();
            render_message(
                locale,
                Message::AdvisoryRepositoryWorktreeCount {
                    repository_id: repository_id.as_str(),
                    count: &count,
                },
            )
        }
        (
            AdvisorySubject::Worktree(worktree_id),
            AdvisoryEvidence::WorktreeUpstreamUnconfigured,
        ) => render_message(
            locale,
            Message::AdvisoryWorktreeUpstreamUnconfigured {
                worktree_id: worktree_id.as_str(),
            },
        ),
        (
            AdvisorySubject::Worktree(worktree_id),
            AdvisoryEvidence::KnownUpstreamDivergence { ahead, behind },
        ) => {
            let ahead = ahead.to_string();
            let behind = behind.to_string();
            render_message(
                locale,
                Message::AdvisoryWorktreeKnownUpstreamDivergence {
                    worktree_id: worktree_id.as_str(),
                    ahead: &ahead,
                    behind: &behind,
                },
            )
        }
        _ => render_message(locale, Message::AdvisoryInconsistentSubject),
    }
}

fn advisory_matches_repository(
    advisory: &Advisory,
    repository: &RepositoryDetailProjection,
) -> bool {
    subject_matches_repository(advisory.subject(), repository)
}

fn subject_matches_repository(
    subject: &AdvisorySubject,
    repository: &RepositoryDetailProjection,
) -> bool {
    match subject {
        AdvisorySubject::Workspace => false,
        AdvisorySubject::Repository(repository_id) => repository_id == repository.id(),
        AdvisorySubject::Worktree(worktree_id) => repository
            .worktrees()
            .iter()
            .any(|worktree| worktree.id() == worktree_id),
    }
}

fn advisory_summary_message<'a>(code: AdvisoryCode, count: &'a str) -> Message<'a> {
    match code {
        AdvisoryCode::RepositoryWithoutRemotes => {
            Message::OverviewAdvisoryRepositoryWithoutRemotes { count }
        }
        AdvisoryCode::RepositoryHasMultipleWorktrees => {
            Message::OverviewAdvisoryRepositoryHasMultipleWorktrees { count }
        }
        AdvisoryCode::WorktreeWithoutUpstream => {
            Message::OverviewAdvisoryWorktreeWithoutUpstream { count }
        }
        AdvisoryCode::LocalCommitsAheadOfKnownUpstream => {
            Message::OverviewAdvisoryWorktreeAheadOfKnownUpstream { count }
        }
    }
}

fn repository_local_work_message(value: &Knowledge<bool>) -> Message<'static> {
    match value {
        Knowledge::Known(true) => Message::RepositoriesLocalWorkYes,
        Knowledge::Known(false) => Message::RepositoriesLocalWorkNo,
        Knowledge::Unknown => Message::RepositoriesLocalWorkUnknown,
    }
}

fn repository_remotes_message(value: &Knowledge<bool>) -> Message<'static> {
    match value {
        Knowledge::Known(true) => Message::RepositoriesRemotesYes,
        Knowledge::Known(false) => Message::RepositoriesRemotesNo,
        Knowledge::Unknown => Message::RepositoriesRemotesUnknown,
    }
}

fn yes_no(locale: Locale, value: bool) -> String {
    render_message(
        locale,
        if value {
            Message::ValueYes
        } else {
            Message::ValueNo
        },
    )
}

fn location_message<'a>(availability: LocationAvailability, path: &'a str) -> Message<'a> {
    match availability {
        LocationAvailability::Available => Message::LocationAvailable { path },
        LocationAvailability::Missing => Message::LocationMissing { path },
        LocationAvailability::Offline => Message::LocationOffline { path },
    }
}

fn worktree_availability_message(value: LocationAvailability) -> Message<'static> {
    match value {
        LocationAvailability::Available => Message::RepositoryWorktreeAvailabilityAvailable,
        LocationAvailability::Missing => Message::RepositoryWorktreeAvailabilityMissing,
        LocationAvailability::Offline => Message::RepositoryWorktreeAvailabilityOffline,
    }
}

fn help_section(writer: &mut impl Write, locale: Locale, message: Message<'_>) -> io::Result<()> {
    writeln!(writer, "{}:", render_message(locale, message))
}

fn help_detail(writer: &mut impl Write, locale: Locale, message: Message<'_>) -> io::Result<()> {
    writeln!(writer, "      {}", render_message(locale, message))
}

fn help_behavior(writer: &mut impl Write, locale: Locale, message: Message<'_>) -> io::Result<()> {
    writeln!(writer, "  {}", render_message(locale, message))
}

fn root_help(writer: &mut impl Write, locale: Locale) -> io::Result<()> {
    writeln!(writer, "{}", render_message(locale, Message::HelpRootTitle))?;
    writeln!(writer)?;
    writeln!(
        writer,
        "{}",
        render_message(locale, Message::HelpRootSummary)
    )?;
    writeln!(writer)?;
    help_section(writer, locale, Message::HelpSectionUsage)?;
    writeln!(writer, "  bulls <command>")?;
    writeln!(writer, "  bulls help <command>")?;
    writeln!(writer)?;
    help_section(writer, locale, Message::HelpSectionWorkflow)?;
    writeln!(
        writer,
        "  bulls discover <root>       {}",
        render_message(locale, Message::HelpRootWorkflowDiscover)
    )?;
    writeln!(
        writer,
        "  bulls refresh               {}",
        render_message(locale, Message::HelpRootWorkflowRefresh)
    )?;
    writeln!(
        writer,
        "  bulls repos                 {}",
        render_message(locale, Message::HelpRootWorkflowRepositories)
    )?;
    writeln!(
        writer,
        "  bulls inspect <selector>    {}",
        render_message(locale, Message::HelpRootWorkflowInspect)
    )?;
    writeln!(
        writer,
        "  bulls overview              {}",
        render_message(locale, Message::HelpRootWorkflowOverview)
    )?;
    writeln!(writer)?;
    help_section(writer, locale, Message::HelpSectionCommands)?;
    writeln!(
        writer,
        "  discover    {}",
        render_message(locale, Message::HelpCommandDiscoverDescription)
    )?;
    writeln!(
        writer,
        "  refresh     {}",
        render_message(locale, Message::HelpCommandRefreshDescription)
    )?;
    writeln!(
        writer,
        "  config      {}",
        render_message(locale, Message::HelpCommandConfigDescription)
    )?;
    writeln!(
        writer,
        "  reset       {}",
        render_message(locale, Message::HelpCommandResetDescription)
    )?;
    writeln!(
        writer,
        "  repos       {}",
        render_message(locale, Message::HelpCommandRepositoriesDescription)
    )?;
    writeln!(
        writer,
        "  inspect     {}",
        render_message(locale, Message::HelpCommandInspectDescription)
    )?;
    writeln!(
        writer,
        "  overview    {}",
        render_message(locale, Message::HelpCommandOverviewDescription)
    )?;
    writeln!(writer)?;
    help_section(writer, locale, Message::HelpSectionBehavior)?;
    help_behavior(writer, locale, Message::HelpRootBehaviorPersistedQueries)?;
    help_behavior(writer, locale, Message::HelpRootBehaviorNoImplicitNetwork)?;
    help_behavior(writer, locale, Message::HelpRootBehaviorReadOnlyGit)?;
    writeln!(writer)?;
    help_section(writer, locale, Message::HelpSectionOptions)?;
    writeln!(
        writer,
        "  -h, --help       {}",
        render_message(locale, Message::HelpOptionShowHelp)
    )?;
    writeln!(
        writer,
        "  -V, --version    {}",
        render_message(locale, Message::HelpOptionShowVersion)
    )?;
    writeln!(writer)?;
    writeln!(
        writer,
        "{}",
        render_message(locale, Message::HelpRootCommandSpecificHint)
    )
}

fn command_help(writer: &mut impl Write, command: CommandName, locale: Locale) -> io::Result<()> {
    match command {
        CommandName::Discover => {
            writeln!(
                writer,
                "{}: bulls discover [--format <human|json>] [--verbose] [root ...]",
                render_message(locale, Message::HelpSectionUsage)
            )?;
            writeln!(writer)?;
            writeln!(
                writer,
                "{}",
                render_message(locale, Message::HelpDiscoverSummary)
            )?;
            writeln!(writer)?;
            help_section(writer, locale, Message::HelpSectionArguments)?;
            writeln!(writer, "  [root ...]")?;
            help_detail(writer, locale, Message::HelpDiscoverArgumentRoots)?;
            help_detail(writer, locale, Message::HelpDiscoverArgumentExplicitRoots)?;
            writeln!(writer)?;
            help_section(writer, locale, Message::HelpSectionOptions)?;
            writeln!(writer, "  --format <human|json>")?;
            help_detail(writer, locale, Message::HelpOptionOutputFormat)?;
            writeln!(writer, "  --verbose")?;
            help_detail(writer, locale, Message::HelpDiscoverOptionVerbose)?;
            writeln!(writer, "  -h, --help")?;
            help_detail(writer, locale, Message::HelpOptionShowHelp)?;
            writeln!(writer)?;
            help_section(writer, locale, Message::HelpSectionBehavior)?;
            help_behavior(writer, locale, Message::HelpDiscoverBehaviorReconcileOnly)?;
            help_behavior(writer, locale, Message::HelpDiscoverBehaviorPartial)?;
            help_behavior(writer, locale, Message::HelpNetworkNone)?;
            help_behavior(writer, locale, Message::HelpProtocolOperation)?;
            writeln!(writer)?;
            help_section(writer, locale, Message::HelpSectionExamples)?;
            writeln!(writer, "  bulls discover")?;
            writeln!(writer, "  bulls discover ~/Projects")?;
            writeln!(writer, "  bulls discover ~/Projects ~/Research")?;
            writeln!(writer, "  bulls discover ~/Projects --verbose")?;
            writeln!(writer, "  bulls discover ~/Projects --format json")?;
            writeln!(writer, "  bulls discover -- ./-repositories")?;
            writeln!(writer)?;
            help_section(writer, locale, Message::HelpSectionNotes)?;
            help_behavior(writer, locale, Message::HelpDiscoverNoteLeadingDash)?;
            help_behavior(writer, locale, Message::HelpDiscoverNoteVerbosePrivacy)
        }
        CommandName::Refresh => {
            writeln!(
                writer,
                "{}: bulls refresh [--format <human|json>]",
                render_message(locale, Message::HelpSectionUsage)
            )?;
            writeln!(writer)?;
            writeln!(
                writer,
                "{}",
                render_message(locale, Message::HelpRefreshSummary)
            )?;
            writeln!(writer)?;
            help_section(writer, locale, Message::HelpSectionOptions)?;
            writeln!(writer, "  --format <human|json>")?;
            help_detail(writer, locale, Message::HelpOptionOutputFormat)?;
            writeln!(writer, "  -h, --help")?;
            help_detail(writer, locale, Message::HelpOptionShowHelp)?;
            writeln!(writer)?;
            help_section(writer, locale, Message::HelpSectionBehavior)?;
            help_behavior(writer, locale, Message::HelpRefreshBehaviorNoDiscovery)?;
            help_behavior(writer, locale, Message::HelpRefreshBehaviorPersists)?;
            help_behavior(writer, locale, Message::HelpNetworkNone)?;
            help_behavior(writer, locale, Message::HelpRefreshBehaviorCancel)?;
            help_behavior(writer, locale, Message::HelpProtocolOperation)?;
            writeln!(writer)?;
            help_section(writer, locale, Message::HelpSectionExamples)?;
            writeln!(writer, "  bulls refresh")?;
            writeln!(writer, "  bulls refresh --format json")
        }
        CommandName::Config => {
            writeln!(
                writer,
                "{}: bulls config show",
                render_message(locale, Message::HelpSectionUsage)
            )?;
            writeln!(writer)?;
            writeln!(
                writer,
                "{}",
                render_message(locale, Message::HelpConfigSummary)
            )?;
            writeln!(writer)?;
            help_section(writer, locale, Message::HelpSectionArguments)?;
            writeln!(writer, "  show")?;
            help_detail(writer, locale, Message::HelpConfigArgumentShow)?;
            writeln!(writer)?;
            help_section(writer, locale, Message::HelpSectionOptions)?;
            writeln!(writer, "  -h, --help")?;
            help_detail(writer, locale, Message::HelpOptionShowHelp)?;
            writeln!(writer)?;
            help_section(writer, locale, Message::HelpSectionBehavior)?;
            help_behavior(writer, locale, Message::HelpConfigBehaviorReadOnly)?;
            writeln!(writer)?;
            help_section(writer, locale, Message::HelpSectionExamples)?;
            writeln!(writer, "  bulls config show")
        }
        CommandName::Reset => {
            writeln!(
                writer,
                "{}: bulls reset",
                render_message(locale, Message::HelpSectionUsage)
            )?;
            writeln!(writer)?;
            writeln!(
                writer,
                "{}",
                render_message(locale, Message::HelpResetSummary)
            )?;
            writeln!(writer)?;
            help_section(writer, locale, Message::HelpSectionBehavior)?;
            help_behavior(writer, locale, Message::HelpResetBehaviorRemoves)?;
            help_behavior(writer, locale, Message::HelpResetBehaviorRevision)?;
            help_behavior(writer, locale, Message::HelpResetBehaviorNoGitModification)?;
            help_behavior(
                writer,
                locale,
                Message::HelpResetBehaviorPreserveConfiguration,
            )?;
            writeln!(writer)?;
            help_section(writer, locale, Message::HelpSectionOptions)?;
            writeln!(writer, "  -h, --help")?;
            help_detail(writer, locale, Message::HelpOptionShowHelp)?;
            writeln!(writer)?;
            help_section(writer, locale, Message::HelpSectionExamples)?;
            writeln!(writer, "  bulls reset")?;
            writeln!(writer, "  bulls discover ~/Projects")?;
            writeln!(writer, "  bulls refresh")?;
            writeln!(writer)?;
            help_section(writer, locale, Message::HelpSectionNotes)?;
            help_behavior(writer, locale, Message::HelpResetNoteRebuild)
        }
        CommandName::Repositories => {
            writeln!(
                writer,
                "{}: bulls repos [--offset <n>] [--limit <n>] [--format <human|json>]",
                render_message(locale, Message::HelpSectionUsage)
            )?;
            writeln!(writer)?;
            writeln!(
                writer,
                "{}",
                render_message(locale, Message::HelpRepositoriesSummary)
            )?;
            writeln!(writer)?;
            help_section(writer, locale, Message::HelpSectionOptions)?;
            writeln!(writer, "  --offset <n>")?;
            help_detail(writer, locale, Message::HelpRepositoriesOptionOffset)?;
            writeln!(writer, "  --limit <n>")?;
            let default = DEFAULT_QUERY_PAGE_SIZE.to_string();
            let maximum = MAX_QUERY_PAGE_SIZE.to_string();
            help_detail(
                writer,
                locale,
                Message::HelpRepositoriesOptionLimit {
                    default: &default,
                    maximum: &maximum,
                },
            )?;
            writeln!(writer, "  --format <human|json>")?;
            help_detail(writer, locale, Message::HelpOptionOutputFormat)?;
            writeln!(writer, "  -h, --help")?;
            help_detail(writer, locale, Message::HelpOptionShowHelp)?;
            writeln!(writer)?;
            help_section(writer, locale, Message::HelpSectionBehavior)?;
            help_behavior(
                writer,
                locale,
                Message::HelpRepositoriesBehaviorPersistedOnly,
            )?;
            help_behavior(writer, locale, Message::HelpProtocolRead)?;
            writeln!(writer)?;
            help_section(writer, locale, Message::HelpSectionExamples)?;
            writeln!(writer, "  bulls repos")?;
            writeln!(writer, "  bulls repos --limit 10")?;
            writeln!(writer, "  bulls repos --offset 10 --limit 10")?;
            writeln!(writer, "  bulls repos --format json")
        }
        CommandName::Inspect => {
            writeln!(
                writer,
                "{}: bulls inspect <repository-selector> [--format <human|json>]",
                render_message(locale, Message::HelpSectionUsage)
            )?;
            writeln!(writer)?;
            writeln!(
                writer,
                "{}",
                render_message(locale, Message::HelpInspectSummary)
            )?;
            writeln!(writer)?;
            help_section(writer, locale, Message::HelpSectionArguments)?;
            writeln!(writer, "  <repository-selector>")?;
            help_detail(writer, locale, Message::HelpInspectArgumentSelector)?;
            help_detail(writer, locale, Message::HelpInspectArgumentIdPrecedence)?;
            help_detail(writer, locale, Message::HelpInspectArgumentAmbiguity)?;
            writeln!(writer)?;
            help_section(writer, locale, Message::HelpSectionOptions)?;
            writeln!(writer, "  --format <human|json>")?;
            help_detail(writer, locale, Message::HelpOptionOutputFormat)?;
            writeln!(writer, "  -h, --help")?;
            help_detail(writer, locale, Message::HelpOptionShowHelp)?;
            writeln!(writer)?;
            help_section(writer, locale, Message::HelpSectionBehavior)?;
            help_behavior(writer, locale, Message::HelpInspectBehaviorPersistedOnly)?;
            help_behavior(writer, locale, Message::HelpInspectBehaviorFreshness)?;
            help_behavior(writer, locale, Message::HelpInspectBehaviorAheadBehind)?;
            help_behavior(writer, locale, Message::HelpProtocolRead)?;
            writeln!(writer)?;
            help_section(writer, locale, Message::HelpSectionExamples)?;
            writeln!(
                writer,
                "  bulls inspect repository-00000000000000000000000000000001"
            )?;
            writeln!(writer, "  bulls inspect .")?;
            writeln!(writer, "  bulls inspect ~/Projects/BullSaddle/bullsaddle")?;
            writeln!(writer, "  bulls inspect . --format json")
        }
        CommandName::Overview => {
            writeln!(
                writer,
                "{}: bulls overview [--format <human|json>]",
                render_message(locale, Message::HelpSectionUsage)
            )?;
            writeln!(writer)?;
            writeln!(
                writer,
                "{}",
                render_message(locale, Message::HelpOverviewSummary)
            )?;
            writeln!(writer)?;
            help_section(writer, locale, Message::HelpSectionOptions)?;
            writeln!(writer, "  --format <human|json>")?;
            help_detail(writer, locale, Message::HelpOptionOutputFormat)?;
            writeln!(writer, "  -h, --help")?;
            help_detail(writer, locale, Message::HelpOptionShowHelp)?;
            writeln!(writer)?;
            help_section(writer, locale, Message::HelpSectionBehavior)?;
            help_behavior(writer, locale, Message::HelpOverviewBehaviorAggregate)?;
            help_behavior(writer, locale, Message::HelpOverviewBehaviorUnknown)?;
            help_behavior(writer, locale, Message::HelpOverviewBehaviorNoIo)?;
            help_behavior(writer, locale, Message::HelpProtocolRead)?;
            writeln!(writer)?;
            help_section(writer, locale, Message::HelpSectionExamples)?;
            writeln!(writer, "  bulls overview")?;
            writeln!(writer, "  bulls overview --format json")
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{Duration, UNIX_EPOCH};

    use bulls_application::ports::{BareRepositoryDiscovery, FilesystemBoundary, SymlinkTraversal};
    use bulls_application::{
        BullSaddleConfiguration, ConfigurationProjection, ConfigurationSource,
        DiscoveryConfiguration, GitProcessTimeout, LocalePreference,
        ObservationAttemptOutcomeProjection, ObservationAttemptProjection, ObservationCoverage,
        ObservationExecutionPolicy, ObservationFailureKind, ObservationStatusProjection,
        ObservationSuccessProjection,
    };

    use super::{
        UnknownObservationContext, configuration, observation_status, relative_age, reset,
        unknown_observation_message,
    };
    use crate::i18n::test_locale;

    #[test]
    fn configuration_rendering_exposes_effective_policy_without_mutating_or_reparsing_it() {
        let root = std::env::temp_dir().join("bulls-config-render-root");
        let configuration_value = BullSaddleConfiguration::new(
            LocalePreference::explicit("pt-BR").expect("locale must be accepted"),
        )
        .with_discovery(
            DiscoveryConfiguration::new(
                vec![root.clone()],
                vec![PathBuf::from("vendor")],
                BareRepositoryDiscovery::Enabled,
                SymlinkTraversal::FollowWithinRoot,
                FilesystemBoundary::CrossFilesystems,
            )
            .expect("discovery configuration must be accepted"),
        )
        .with_observation_policy(
            ObservationExecutionPolicy::new(6).expect("parallelism must be accepted"),
        )
        .with_git_process_timeout(
            GitProcessTimeout::new(12_500).expect("timeout must be accepted"),
        );
        let projection = ConfigurationProjection::new(
            std::env::temp_dir().join("bulls").join("config.toml"),
            ConfigurationSource::File,
            configuration_value,
        )
        .expect("configuration projection must be accepted");
        let mut output = Vec::new();

        configuration(&mut output, &projection, test_locale("en-US"))
            .expect("configuration rendering must succeed");
        let output = String::from_utf8(output).expect("rendered output must be UTF-8");

        assert!(output.contains("Configuration"));
        assert!(output.contains("Source: file"));
        assert!(output.contains("Locale: pt-BR"));
        assert!(output.contains(&format!("  - {root:?}")));
        assert!(output.contains("  - \"vendor\""));
        assert!(output.contains("Bare repositories: enabled"));
        assert!(output.contains("Symlink traversal: follow within root"));
        assert!(output.contains("Filesystem boundary: cross filesystems"));
        assert!(output.contains("Observation parallelism: 6"));
        assert!(output.contains("Git process timeout: 12500 ms"));

        let mut portuguese_output = Vec::new();
        configuration(&mut portuguese_output, &projection, test_locale("pt-BR"))
            .expect("Portuguese configuration rendering must succeed");
        let portuguese_output =
            String::from_utf8(portuguese_output).expect("rendered output must be UTF-8");

        assert!(portuguese_output.contains("Configuração"));
        assert!(portuguese_output.contains("Origem: arquivo"));
        assert!(portuguese_output.contains("Raízes de descoberta:"));
        assert!(portuguese_output.contains("Repositórios bare: habilitado"));
        assert!(portuguese_output.contains("Paralelismo de observação: 6"));
        assert!(portuguese_output.contains("Timeout de processo Git: 12500 ms"));
    }

    #[test]
    fn reset_rendering_is_catalog_driven_in_both_locales() {
        let mut english = Vec::new();
        reset(&mut english, test_locale("en-US")).expect("English reset rendering must succeed");
        let english = String::from_utf8(english).expect("rendered output must be UTF-8");
        assert!(english.contains("Workspace reset"));
        assert!(english.contains("Status: complete"));
        assert!(english.contains("User configuration was preserved."));

        let mut portuguese = Vec::new();
        reset(&mut portuguese, test_locale("pt-BR"))
            .expect("Portuguese reset rendering must succeed");
        let portuguese = String::from_utf8(portuguese).expect("rendered output must be UTF-8");
        assert!(portuguese.contains("Reset do workspace"));
        assert!(portuguese.contains("Status: completo"));
        assert!(portuguese.contains("A configuração do usuário foi preservada."));
    }

    #[test]
    fn relative_age_uses_compact_locale_aware_units() {
        let now = UNIX_EPOCH + Duration::from_secs(3 * 60 * 60);

        assert_eq!(
            relative_age(test_locale("en-US"), now - Duration::from_secs(90), now),
            "1m ago"
        );
        assert_eq!(
            relative_age(
                test_locale("pt-BR"),
                now - Duration::from_secs(2 * 60 * 60),
                now
            ),
            "há 2h"
        );
        assert_eq!(
            relative_age(test_locale("en-US"), now + Duration::from_secs(30), now),
            "in 30s"
        );
    }

    #[test]
    fn observation_status_distinguishes_failed_attempt_from_last_success() {
        let succeeded_at = UNIX_EPOCH + Duration::from_secs(60);
        let failed_at = UNIX_EPOCH + Duration::from_secs(120);
        let now = UNIX_EPOCH + Duration::from_secs(180);
        let status = ObservationStatusProjection::new(
            Some(ObservationAttemptProjection::new(
                failed_at,
                ObservationCoverage::Partial,
                ObservationAttemptOutcomeProjection::Failed(ObservationFailureKind::TimedOut),
            )),
            Some(ObservationSuccessProjection::new(
                succeeded_at,
                ObservationCoverage::Complete,
            )),
        );

        assert_eq!(
            observation_status(test_locale("en-US"), &status, now),
            "Observation: failed: timed out — 1m ago; last successful observation 2m ago (complete)"
        );
        assert_eq!(
            observation_status(test_locale("pt-BR"), &status, now),
            "Observação: falhou: tempo esgotado — há 1m; última observação bem-sucedida há 2m (completo)"
        );
    }

    #[test]
    fn unknown_observation_explains_absence_without_collapsing_it_to_false() {
        let status = ObservationStatusProjection::new(None, None);
        let now = UNIX_EPOCH + Duration::from_secs(180);

        assert_eq!(
            unknown_observation_message(
                test_locale("en-US"),
                &status,
                now,
                UnknownObservationContext::RepositoryRemotes,
            ),
            "Remotes: unknown — never observed"
        );
        assert_eq!(
            unknown_observation_message(
                test_locale("pt-BR"),
                &status,
                now,
                UnknownObservationContext::WorktreeGitState,
            ),
            "Estado Git: desconhecido — sem observação anterior"
        );
    }
}
