// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use bulls_core::{LocationId, ObservationRunId, RepositoryId, WorktreeId};

use super::PortResult;

pub trait IdentityGeneratorPort {
    fn next_repository_id(&mut self) -> PortResult<RepositoryId>;

    fn next_location_id(&mut self) -> PortResult<LocationId>;

    fn next_worktree_id(&mut self) -> PortResult<WorktreeId>;

    fn next_observation_run_id(&mut self) -> PortResult<ObservationRunId>;
}
