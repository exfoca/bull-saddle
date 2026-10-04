// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Remote {
    name: String,
}

impl Remote {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Branch {
    name: String,
}

impl Branch {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum HeadKind {
    Unborn,
    Branch,
    Detached,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Head {
    Unborn(Branch),
    Branch(Branch),
    Detached,
}

impl Head {
    pub const fn kind(&self) -> HeadKind {
        match self {
            Self::Unborn(_) => HeadKind::Unborn,
            Self::Branch(_) => HeadKind::Branch,
            Self::Detached => HeadKind::Detached,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Upstream {
    remote: Remote,
    branch: Branch,
}

impl Upstream {
    pub fn new(remote: Remote, branch: Branch) -> Self {
        Self { remote, branch }
    }

    pub fn remote(&self) -> &Remote {
        &self.remote
    }

    pub fn branch(&self) -> &Branch {
        &self.branch
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum UpstreamRelation {
    Synchronized,
    Ahead,
    Behind,
    Diverged,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct UpstreamDivergence {
    ahead: u64,
    behind: u64,
}

impl UpstreamDivergence {
    pub const fn new(ahead: u64, behind: u64) -> Self {
        Self { ahead, behind }
    }

    pub const fn ahead(&self) -> u64 {
        self.ahead
    }

    pub const fn behind(&self) -> u64 {
        self.behind
    }

    pub const fn relation(&self) -> UpstreamRelation {
        match (self.ahead, self.behind) {
            (0, 0) => UpstreamRelation::Synchronized,
            (_, 0) => UpstreamRelation::Ahead,
            (0, _) => UpstreamRelation::Behind,
            (_, _) => UpstreamRelation::Diverged,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum UpstreamStateKind {
    Unconfigured,
    Gone,
    Tracking,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UpstreamState {
    Unconfigured,
    Gone(Upstream),
    Tracking {
        upstream: Upstream,
        divergence: UpstreamDivergence,
    },
}

impl UpstreamState {
    pub const fn kind(&self) -> UpstreamStateKind {
        match self {
            Self::Unconfigured => UpstreamStateKind::Unconfigured,
            Self::Gone(_) => UpstreamStateKind::Gone,
            Self::Tracking { .. } => UpstreamStateKind::Tracking,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct WorktreeChanges {
    staged: bool,
    unstaged: bool,
    untracked: bool,
}

impl WorktreeChanges {
    pub const fn new(staged: bool, unstaged: bool, untracked: bool) -> Self {
        Self {
            staged,
            unstaged,
            untracked,
        }
    }

    pub const fn staged(self) -> bool {
        self.staged
    }

    pub const fn unstaged(self) -> bool {
        self.unstaged
    }

    pub const fn untracked(self) -> bool {
        self.untracked
    }

    pub const fn has_local_work(self) -> bool {
        self.staged || self.unstaged || self.untracked
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorktreeGitState {
    head: Head,
    upstream: Option<UpstreamState>,
    changes: WorktreeChanges,
}

impl WorktreeGitState {
    pub fn unborn(branch: Branch, upstream: UpstreamState) -> Self {
        Self {
            head: Head::Unborn(branch),
            upstream: Some(upstream),
            changes: WorktreeChanges::default(),
        }
    }

    pub fn branch(branch: Branch, upstream: UpstreamState) -> Self {
        Self {
            head: Head::Branch(branch),
            upstream: Some(upstream),
            changes: WorktreeChanges::default(),
        }
    }

    pub const fn detached() -> Self {
        Self {
            head: Head::Detached,
            upstream: None,
            changes: WorktreeChanges {
                staged: false,
                unstaged: false,
                untracked: false,
            },
        }
    }

    pub fn head(&self) -> &Head {
        &self.head
    }

    pub fn upstream(&self) -> Option<&UpstreamState> {
        self.upstream.as_ref()
    }

    pub const fn changes(&self) -> WorktreeChanges {
        self.changes
    }

    pub const fn with_changes(mut self, changes: WorktreeChanges) -> Self {
        self.changes = changes;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Branch, Head, HeadKind, Remote, Upstream, UpstreamDivergence, UpstreamRelation,
        UpstreamState, UpstreamStateKind, WorktreeChanges, WorktreeGitState,
    };

    #[test]
    fn head_distinguishes_unborn_branch_and_detached_states() {
        let unborn = Head::Unborn(Branch::new("main"));
        let branch = Head::Branch(Branch::new("main"));
        let detached = Head::Detached;

        assert_ne!(unborn, branch);
        assert_ne!(branch, detached);
        assert_ne!(unborn, detached);
    }

    #[test]
    fn head_kind_exposes_semantics_without_branch_payloads() {
        assert_eq!(Head::Unborn(Branch::new("main")).kind(), HeadKind::Unborn);
        assert_eq!(
            Head::Branch(Branch::new("feature")).kind(),
            HeadKind::Branch
        );
        assert_eq!(Head::Detached.kind(), HeadKind::Detached);
    }

    #[test]
    fn upstream_keeps_remote_and_branch_semantically_separate() {
        let upstream = Upstream::new(Remote::new("origin"), Branch::new("main"));

        assert_eq!(upstream.remote().name(), "origin");
        assert_eq!(upstream.branch().name(), "main");
    }

    #[test]
    fn divergence_derives_the_canonical_upstream_relation() {
        assert_eq!(
            UpstreamDivergence::new(0, 0).relation(),
            UpstreamRelation::Synchronized
        );
        assert_eq!(
            UpstreamDivergence::new(3, 0).relation(),
            UpstreamRelation::Ahead
        );
        assert_eq!(
            UpstreamDivergence::new(0, 2).relation(),
            UpstreamRelation::Behind
        );
        assert_eq!(
            UpstreamDivergence::new(3, 2).relation(),
            UpstreamRelation::Diverged
        );
    }

    #[test]
    fn worktree_state_distinguishes_missing_upstream_from_gone_upstream() {
        let unconfigured =
            WorktreeGitState::branch(Branch::new("main"), UpstreamState::Unconfigured);
        let gone = WorktreeGitState::branch(
            Branch::new("main"),
            UpstreamState::Gone(Upstream::new(Remote::new("origin"), Branch::new("main"))),
        );

        assert_eq!(unconfigured.upstream(), Some(&UpstreamState::Unconfigured));
        assert!(matches!(gone.upstream(), Some(UpstreamState::Gone(_))));
    }

    #[test]
    fn upstream_state_kind_exposes_semantics_without_tracking_payloads() {
        let upstream = Upstream::new(Remote::new("origin"), Branch::new("main"));

        assert_eq!(
            UpstreamState::Unconfigured.kind(),
            UpstreamStateKind::Unconfigured
        );
        assert_eq!(
            UpstreamState::Gone(upstream.clone()).kind(),
            UpstreamStateKind::Gone
        );
        assert_eq!(
            UpstreamState::Tracking {
                upstream,
                divergence: UpstreamDivergence::new(1, 0),
            }
            .kind(),
            UpstreamStateKind::Tracking
        );
    }

    #[test]
    fn worktree_changes_track_presence_without_collecting_file_lists() {
        let changes = WorktreeChanges::new(true, false, true);
        let state = WorktreeGitState::detached().with_changes(changes);

        assert!(state.changes().staged());
        assert!(!state.changes().unstaged());
        assert!(state.changes().untracked());
        assert!(state.changes().has_local_work());
        assert!(!WorktreeChanges::default().has_local_work());
    }

    #[test]
    fn detached_worktree_state_has_no_current_branch_upstream() {
        let state = WorktreeGitState::detached();

        assert_eq!(state.head(), &Head::Detached);
        assert!(state.upstream().is_none());
    }
}
