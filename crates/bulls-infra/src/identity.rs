// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use bulls_application::ports::{IdentityGeneratorPort, PortError, PortErrorKind, PortResult};
use bulls_core::{LocationId, ObservationRunId, RepositoryId, WorktreeId};

static IDENTITY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Default)]
pub struct NativeIdentityGenerator;

impl NativeIdentityGenerator {
    pub const fn new() -> Self {
        Self
    }
}

impl IdentityGeneratorPort for NativeIdentityGenerator {
    fn next_repository_id(&mut self) -> PortResult<RepositoryId> {
        generate_id("repository").map(RepositoryId::from)
    }

    fn next_location_id(&mut self) -> PortResult<LocationId> {
        generate_id("location").map(LocationId::from)
    }

    fn next_worktree_id(&mut self) -> PortResult<WorktreeId> {
        generate_id("worktree").map(WorktreeId::from)
    }

    fn next_observation_run_id(&mut self) -> PortResult<ObservationRunId> {
        generate_id("observation-run").map(ObservationRunId::from)
    }
}

fn generate_id(kind: &str) -> PortResult<String> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| PortError::new(PortErrorKind::InvariantViolation))?
        .as_nanos();
    let sequence = IDENTITY_SEQUENCE.fetch_add(1, Ordering::Relaxed);

    Ok(format!(
        "{kind}-{timestamp:032x}-{:08x}-{sequence:016x}",
        std::process::id()
    ))
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use bulls_application::ports::IdentityGeneratorPort;

    use super::NativeIdentityGenerator;

    #[test]
    fn generated_identities_are_opaque_and_distinct() {
        let mut generator = NativeIdentityGenerator::new();
        let repository = generator
            .next_repository_id()
            .expect("repository identity generation must succeed");
        let location = generator
            .next_location_id()
            .expect("location identity generation must succeed");
        let worktree = generator
            .next_worktree_id()
            .expect("worktree identity generation must succeed");
        let observation_run = generator
            .next_observation_run_id()
            .expect("observation run identity generation must succeed");

        let values = HashSet::from([
            repository.as_str().to_owned(),
            location.as_str().to_owned(),
            worktree.as_str().to_owned(),
            observation_run.as_str().to_owned(),
        ]);
        assert_eq!(values.len(), 4);
        assert!(repository.as_str().starts_with("repository-"));
        assert!(location.as_str().starts_with("location-"));
        assert!(worktree.as_str().starts_with("worktree-"));
        assert!(observation_run.as_str().starts_with("observation-run-"));
    }
}
