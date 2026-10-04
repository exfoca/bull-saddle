// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use std::cmp::Ordering;
use std::collections::HashSet;
use std::time::SystemTime;

use bulls_core::{Head, ObservationCoverage, RepositoryId, UpstreamState, WorktreeId};

use crate::{
    ObservationStatusProjection, RepositoryDetailProjection, WorkspaceMirror, WorkspaceRevision,
    WorktreeProjection,
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum AdvisoryCode {
    RepositoryWithoutRemotes,
    RepositoryHasMultipleWorktrees,
    WorktreeWithoutUpstream,
    LocalCommitsAheadOfKnownUpstream,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum AdvisorySubject {
    Workspace,
    Repository(RepositoryId),
    Worktree(WorktreeId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdvisoryFreshness {
    WorkspaceRevision(WorkspaceRevision),
    ObservedAt(SystemTime),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdvisoryEvidence {
    RepositoryRemoteCount { remote_count: usize },
    RepositoryWorktreeCount { worktree_count: usize },
    WorktreeUpstreamUnconfigured,
    KnownUpstreamDivergence { ahead: u64, behind: u64 },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Advisory {
    code: AdvisoryCode,
    subject: AdvisorySubject,
    evidence: AdvisoryEvidence,
    freshness: AdvisoryFreshness,
}

impl Advisory {
    fn new(
        code: AdvisoryCode,
        subject: AdvisorySubject,
        evidence: AdvisoryEvidence,
        freshness: AdvisoryFreshness,
    ) -> Self {
        Self {
            code,
            subject,
            evidence,
            freshness,
        }
    }

    pub const fn code(&self) -> AdvisoryCode {
        self.code
    }

    pub const fn subject(&self) -> &AdvisorySubject {
        &self.subject
    }

    pub const fn evidence(&self) -> AdvisoryEvidence {
        self.evidence
    }

    pub const fn freshness(&self) -> AdvisoryFreshness {
        self.freshness
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdvisoryUnknownReason {
    MissingObservation,
    PartialObservation,
    MissingEvidence,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdvisoryRuleUnknown {
    code: AdvisoryCode,
    subject: AdvisorySubject,
    reason: AdvisoryUnknownReason,
}

impl AdvisoryRuleUnknown {
    fn new(code: AdvisoryCode, subject: AdvisorySubject, reason: AdvisoryUnknownReason) -> Self {
        Self {
            code,
            subject,
            reason,
        }
    }

    pub const fn code(&self) -> AdvisoryCode {
        self.code
    }

    pub const fn subject(&self) -> &AdvisorySubject {
        &self.subject
    }

    pub const fn reason(&self) -> AdvisoryUnknownReason {
        self.reason
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdvisoryProjection {
    revision: WorkspaceRevision,
    advisories: Vec<Advisory>,
    unknown_rules: Vec<AdvisoryRuleUnknown>,
}

impl AdvisoryProjection {
    fn new(
        revision: WorkspaceRevision,
        advisories: Vec<Advisory>,
        unknown_rules: Vec<AdvisoryRuleUnknown>,
    ) -> Self {
        Self {
            revision,
            advisories,
            unknown_rules,
        }
    }

    pub const fn revision(&self) -> WorkspaceRevision {
        self.revision
    }

    pub fn advisories(&self) -> &[Advisory] {
        &self.advisories
    }

    pub fn unknown_rules(&self) -> &[AdvisoryRuleUnknown] {
        &self.unknown_rules
    }
}

pub struct AdvisoryEvaluator;

impl AdvisoryEvaluator {
    pub fn evaluate(mirror: &WorkspaceMirror) -> AdvisoryProjection {
        let mut accumulator = AdvisoryAccumulator::default();

        for repository in mirror.repositories() {
            accumulator.record(repository_without_remotes(repository));
            accumulator.record(repository_has_multiple_worktrees(repository));

            for worktree in repository.worktrees() {
                accumulator.record(worktree_without_upstream(worktree));
                accumulator.record(local_commits_ahead_of_known_upstream(worktree));
            }
        }

        accumulator.finish(mirror.revision())
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct AdvisoryKey {
    code: AdvisoryCode,
    subject: AdvisorySubject,
}

impl AdvisoryKey {
    fn new(code: AdvisoryCode, subject: &AdvisorySubject) -> Self {
        Self {
            code,
            subject: subject.clone(),
        }
    }
}

enum RuleEvaluation {
    Matched(Advisory),
    NotMatched,
    Unknown(AdvisoryRuleUnknown),
}

#[derive(Default)]
struct AdvisoryAccumulator {
    advisories: Vec<Advisory>,
    advisory_keys: HashSet<AdvisoryKey>,
    unknown_rules: Vec<AdvisoryRuleUnknown>,
    unknown_keys: HashSet<AdvisoryKey>,
}

impl AdvisoryAccumulator {
    fn record(&mut self, evaluation: RuleEvaluation) {
        match evaluation {
            RuleEvaluation::Matched(advisory) => {
                let key = AdvisoryKey::new(advisory.code(), advisory.subject());
                if self.advisory_keys.insert(key.clone()) {
                    self.unknown_keys.remove(&key);
                    self.unknown_rules.retain(|unknown| {
                        AdvisoryKey::new(unknown.code(), unknown.subject()) != key
                    });
                    self.advisories.push(advisory);
                }
            }
            RuleEvaluation::NotMatched => {}
            RuleEvaluation::Unknown(unknown) => {
                let key = AdvisoryKey::new(unknown.code(), unknown.subject());
                if !self.advisory_keys.contains(&key) && self.unknown_keys.insert(key) {
                    self.unknown_rules.push(unknown);
                }
            }
        }
    }

    fn finish(mut self, revision: WorkspaceRevision) -> AdvisoryProjection {
        self.advisories.sort_by(compare_advisories);
        self.unknown_rules.sort_by(compare_unknown_rules);
        AdvisoryProjection::new(revision, self.advisories, self.unknown_rules)
    }
}

fn repository_without_remotes(repository: &RepositoryDetailProjection) -> RuleEvaluation {
    let code = AdvisoryCode::RepositoryWithoutRemotes;
    let subject = AdvisorySubject::Repository(repository.id().clone());
    let observed_at = match complete_observation_time(repository.remotes_observation()) {
        Ok(observed_at) => observed_at,
        Err(reason) => return unknown(code, subject, reason),
    };
    let Some(remotes) = repository.remotes() else {
        return unknown(code, subject, AdvisoryUnknownReason::MissingEvidence);
    };

    if remotes.is_empty() {
        RuleEvaluation::Matched(Advisory::new(
            code,
            subject,
            AdvisoryEvidence::RepositoryRemoteCount { remote_count: 0 },
            AdvisoryFreshness::ObservedAt(observed_at),
        ))
    } else {
        RuleEvaluation::NotMatched
    }
}

fn repository_has_multiple_worktrees(repository: &RepositoryDetailProjection) -> RuleEvaluation {
    let worktree_count = repository.worktrees().len();
    if worktree_count <= 1 {
        return RuleEvaluation::NotMatched;
    }

    RuleEvaluation::Matched(Advisory::new(
        AdvisoryCode::RepositoryHasMultipleWorktrees,
        AdvisorySubject::Repository(repository.id().clone()),
        AdvisoryEvidence::RepositoryWorktreeCount { worktree_count },
        AdvisoryFreshness::WorkspaceRevision(repository.revision()),
    ))
}

fn worktree_without_upstream(worktree: &WorktreeProjection) -> RuleEvaluation {
    let code = AdvisoryCode::WorktreeWithoutUpstream;
    let subject = AdvisorySubject::Worktree(worktree.id().clone());
    let observed_at = match complete_observation_time(worktree.observation()) {
        Ok(observed_at) => observed_at,
        Err(reason) => return unknown(code, subject, reason),
    };
    let Some(state) = worktree.git_state() else {
        return unknown(code, subject, AdvisoryUnknownReason::MissingEvidence);
    };

    match state.head() {
        Head::Detached => RuleEvaluation::NotMatched,
        Head::Unborn(_) | Head::Branch(_) => match state.upstream() {
            Some(UpstreamState::Unconfigured) => RuleEvaluation::Matched(Advisory::new(
                code,
                subject,
                AdvisoryEvidence::WorktreeUpstreamUnconfigured,
                AdvisoryFreshness::ObservedAt(observed_at),
            )),
            Some(UpstreamState::Gone(_) | UpstreamState::Tracking { .. }) => {
                RuleEvaluation::NotMatched
            }
            None => unknown(code, subject, AdvisoryUnknownReason::MissingEvidence),
        },
    }
}

fn local_commits_ahead_of_known_upstream(worktree: &WorktreeProjection) -> RuleEvaluation {
    let code = AdvisoryCode::LocalCommitsAheadOfKnownUpstream;
    let subject = AdvisorySubject::Worktree(worktree.id().clone());
    let observed_at = match complete_observation_time(worktree.observation()) {
        Ok(observed_at) => observed_at,
        Err(reason) => return unknown(code, subject, reason),
    };
    let Some(state) = worktree.git_state() else {
        return unknown(code, subject, AdvisoryUnknownReason::MissingEvidence);
    };

    let Some(upstream) = state.upstream() else {
        return match state.head() {
            Head::Detached => RuleEvaluation::NotMatched,
            Head::Unborn(_) | Head::Branch(_) => {
                unknown(code, subject, AdvisoryUnknownReason::MissingEvidence)
            }
        };
    };
    let UpstreamState::Tracking { divergence, .. } = upstream else {
        return RuleEvaluation::NotMatched;
    };
    if divergence.ahead() == 0 {
        return RuleEvaluation::NotMatched;
    }

    RuleEvaluation::Matched(Advisory::new(
        code,
        subject,
        AdvisoryEvidence::KnownUpstreamDivergence {
            ahead: divergence.ahead(),
            behind: divergence.behind(),
        },
        AdvisoryFreshness::ObservedAt(observed_at),
    ))
}

fn complete_observation_time(
    status: &ObservationStatusProjection,
) -> Result<SystemTime, AdvisoryUnknownReason> {
    let Some(success) = status.latest_success() else {
        return Err(AdvisoryUnknownReason::MissingObservation);
    };
    if success.coverage() != ObservationCoverage::Complete {
        return Err(AdvisoryUnknownReason::PartialObservation);
    }
    Ok(success.observed_at())
}

fn unknown(
    code: AdvisoryCode,
    subject: AdvisorySubject,
    reason: AdvisoryUnknownReason,
) -> RuleEvaluation {
    RuleEvaluation::Unknown(AdvisoryRuleUnknown::new(code, subject, reason))
}

fn compare_advisories(left: &Advisory, right: &Advisory) -> Ordering {
    compare_subjects(left.subject(), right.subject())
        .then_with(|| advisory_code_rank(left.code()).cmp(&advisory_code_rank(right.code())))
}

fn compare_unknown_rules(left: &AdvisoryRuleUnknown, right: &AdvisoryRuleUnknown) -> Ordering {
    compare_subjects(left.subject(), right.subject())
        .then_with(|| advisory_code_rank(left.code()).cmp(&advisory_code_rank(right.code())))
}

fn compare_subjects(left: &AdvisorySubject, right: &AdvisorySubject) -> Ordering {
    match (left, right) {
        (AdvisorySubject::Workspace, AdvisorySubject::Workspace) => Ordering::Equal,
        (AdvisorySubject::Workspace, _) => Ordering::Less,
        (_, AdvisorySubject::Workspace) => Ordering::Greater,
        (AdvisorySubject::Repository(left), AdvisorySubject::Repository(right)) => {
            left.as_str().cmp(right.as_str())
        }
        (AdvisorySubject::Repository(_), AdvisorySubject::Worktree(_)) => Ordering::Less,
        (AdvisorySubject::Worktree(_), AdvisorySubject::Repository(_)) => Ordering::Greater,
        (AdvisorySubject::Worktree(left), AdvisorySubject::Worktree(right)) => {
            left.as_str().cmp(right.as_str())
        }
    }
}

const fn advisory_code_rank(code: AdvisoryCode) -> u8 {
    match code {
        AdvisoryCode::RepositoryWithoutRemotes => 0,
        AdvisoryCode::RepositoryHasMultipleWorktrees => 1,
        AdvisoryCode::WorktreeWithoutUpstream => 2,
        AdvisoryCode::LocalCommitsAheadOfKnownUpstream => 3,
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use bulls_core::{
        Branch, LocationAvailability, LocationId, ObservationCoverage, Remote, RepositoryId,
        Upstream, UpstreamDivergence, UpstreamState, WorktreeGitState, WorktreeId,
    };

    use super::{
        AdvisoryCode, AdvisoryEvaluator, AdvisoryEvidence, AdvisoryFreshness, AdvisorySubject,
        AdvisoryUnknownReason,
    };
    use crate::{
        LocationProjection, ObservationAttemptOutcomeProjection, ObservationAttemptProjection,
        ObservationStatusProjection, ObservationSuccessProjection, OverviewProjection,
        RepositoryDetailProjection, WorkspaceMirror, WorkspaceRevision, WorktreeProjection,
    };

    fn status(seconds: u64, coverage: ObservationCoverage) -> ObservationStatusProjection {
        let observed_at = UNIX_EPOCH + Duration::from_secs(seconds);
        ObservationStatusProjection::new(
            Some(ObservationAttemptProjection::new(
                observed_at,
                coverage,
                ObservationAttemptOutcomeProjection::Succeeded,
            )),
            Some(ObservationSuccessProjection::new(observed_at, coverage)),
        )
    }

    fn location(id: &str) -> LocationProjection {
        LocationProjection::new(
            LocationId::from(id),
            format!("/workspace/{id}").into(),
            LocationAvailability::Available,
        )
    }

    fn worktree(
        id: &str,
        repository_id: &RepositoryId,
        state: WorktreeGitState,
        observation: ObservationStatusProjection,
    ) -> WorktreeProjection {
        WorktreeProjection::new(
            WorkspaceRevision::new(9),
            WorktreeId::from(id),
            repository_id.clone(),
            location(id),
            Some(state),
            observation,
        )
    }

    fn repository(
        id: &str,
        remotes: Option<Vec<Remote>>,
        remotes_observation: ObservationStatusProjection,
        worktrees: Vec<WorktreeProjection>,
    ) -> RepositoryDetailProjection {
        RepositoryDetailProjection::new(
            WorkspaceRevision::new(9),
            RepositoryId::from(id),
            Vec::new(),
            remotes,
            remotes_observation,
            worktrees,
        )
    }

    fn mirror(repositories: Vec<RepositoryDetailProjection>) -> WorkspaceMirror {
        let revision = WorkspaceRevision::new(9);
        WorkspaceMirror::new(
            revision,
            OverviewProjection::new(revision, repositories.len(), 0, 0, 0, 0, 0, 0),
            repositories,
        )
    }

    #[test]
    fn evaluator_derives_structured_advisories_with_stable_evidence_and_ordering() {
        let repository_id = RepositoryId::from("repository-a");
        let unconfigured = worktree(
            "worktree-a",
            &repository_id,
            WorktreeGitState::branch(Branch::new("main"), UpstreamState::Unconfigured),
            status(20, ObservationCoverage::Complete),
        );
        let ahead = worktree(
            "worktree-b",
            &repository_id,
            WorktreeGitState::branch(
                Branch::new("feature"),
                UpstreamState::Tracking {
                    upstream: Upstream::new(Remote::new("origin"), Branch::new("feature")),
                    divergence: UpstreamDivergence::new(4, 1),
                },
            ),
            status(30, ObservationCoverage::Complete),
        );
        let mirror = mirror(vec![repository(
            "repository-a",
            Some(Vec::new()),
            status(10, ObservationCoverage::Complete),
            vec![ahead, unconfigured],
        )]);

        let first = AdvisoryEvaluator::evaluate(&mirror);
        let second = AdvisoryEvaluator::evaluate(&mirror);

        assert_eq!(first, second);
        assert_eq!(first.revision(), WorkspaceRevision::new(9));
        assert_eq!(first.advisories().len(), 4);
        assert_eq!(
            first
                .advisories()
                .iter()
                .map(|advisory| advisory.code())
                .collect::<Vec<_>>(),
            vec![
                AdvisoryCode::RepositoryWithoutRemotes,
                AdvisoryCode::RepositoryHasMultipleWorktrees,
                AdvisoryCode::WorktreeWithoutUpstream,
                AdvisoryCode::LocalCommitsAheadOfKnownUpstream,
            ]
        );
        assert_eq!(
            first.advisories()[0].subject(),
            &AdvisorySubject::Repository(repository_id.clone())
        );
        assert_eq!(
            first.advisories()[0].evidence(),
            AdvisoryEvidence::RepositoryRemoteCount { remote_count: 0 }
        );
        assert_eq!(
            first.advisories()[0].freshness(),
            AdvisoryFreshness::ObservedAt(UNIX_EPOCH + Duration::from_secs(10))
        );
        assert_eq!(
            first.advisories()[3].evidence(),
            AdvisoryEvidence::KnownUpstreamDivergence {
                ahead: 4,
                behind: 1,
            }
        );
        assert!(first.unknown_rules().is_empty());
    }

    #[test]
    fn partial_or_missing_observations_remain_unknown_instead_of_becoming_negative_facts() {
        let repository_id = RepositoryId::from("repository-a");
        let partial_worktree = worktree(
            "worktree-a",
            &repository_id,
            WorktreeGitState::branch(Branch::new("main"), UpstreamState::Unconfigured),
            status(20, ObservationCoverage::Partial),
        );
        let mirror = mirror(vec![repository(
            "repository-a",
            Some(Vec::new()),
            status(10, ObservationCoverage::Partial),
            vec![partial_worktree],
        )]);

        let projection = AdvisoryEvaluator::evaluate(&mirror);

        assert!(projection.advisories().is_empty());
        assert_eq!(projection.unknown_rules().len(), 3);
        assert!(
            projection
                .unknown_rules()
                .iter()
                .all(|unknown| { unknown.reason() == AdvisoryUnknownReason::PartialObservation })
        );
        assert!(
            projection
                .unknown_rules()
                .iter()
                .any(|unknown| { unknown.code() == AdvisoryCode::RepositoryWithoutRemotes })
        );
        assert!(
            projection
                .unknown_rules()
                .iter()
                .any(|unknown| { unknown.code() == AdvisoryCode::WorktreeWithoutUpstream })
        );
        assert!(
            projection.unknown_rules().iter().any(|unknown| {
                unknown.code() == AdvisoryCode::LocalCommitsAheadOfKnownUpstream
            })
        );
    }

    #[test]
    fn repository_catalog_rule_uses_workspace_revision_and_needs_no_observation() {
        let repository_id = RepositoryId::from("repository-a");
        let first = worktree(
            "worktree-a",
            &repository_id,
            WorktreeGitState::detached(),
            ObservationStatusProjection::new(None, None),
        );
        let second = worktree(
            "worktree-b",
            &repository_id,
            WorktreeGitState::detached(),
            ObservationStatusProjection::new(None, None),
        );
        let mirror = mirror(vec![repository(
            "repository-a",
            None,
            ObservationStatusProjection::new(None, None),
            vec![first, second],
        )]);

        let projection = AdvisoryEvaluator::evaluate(&mirror);
        let advisory = projection
            .advisories()
            .iter()
            .find(|advisory| advisory.code() == AdvisoryCode::RepositoryHasMultipleWorktrees)
            .expect("catalog advisory must be derived");

        assert_eq!(
            advisory.evidence(),
            AdvisoryEvidence::RepositoryWorktreeCount { worktree_count: 2 }
        );
        assert_eq!(
            advisory.freshness(),
            AdvisoryFreshness::WorkspaceRevision(WorkspaceRevision::new(9))
        );
    }

    #[test]
    fn duplicate_semantic_advisories_are_deduplicated_by_code_and_subject() {
        let repository = repository(
            "repository-a",
            Some(Vec::new()),
            status(10, ObservationCoverage::Complete),
            Vec::new(),
        );
        let mirror = mirror(vec![repository.clone(), repository]);

        let projection = AdvisoryEvaluator::evaluate(&mirror);

        assert_eq!(projection.advisories().len(), 1);
        assert_eq!(
            projection.advisories()[0].code(),
            AdvisoryCode::RepositoryWithoutRemotes
        );
    }
}
