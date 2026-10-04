// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

//! Domain model and stable contracts for BullSaddle.

mod git;
mod identity;
mod observation;
mod topology;

pub use git::{
    Branch, Head, HeadKind, Remote, Upstream, UpstreamDivergence, UpstreamRelation, UpstreamState,
    UpstreamStateKind, WorktreeChanges, WorktreeGitState,
};
pub use identity::{LocationId, ObservationRunId, RepositoryId, WorktreeId};
pub use observation::{
    Freshness, Observation, ObservationAttempt, ObservationCoverage, ObservationFailure,
    ObservationFailureKind, ObservationKey, ObservationKind, ObservationMetadata,
};
pub use topology::{Location, LocationAvailability, Repository, Worktree};
