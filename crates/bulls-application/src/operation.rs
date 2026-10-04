// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use crate::WorkspaceRevision;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum OperationCompletion {
    Complete,
    Partial,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceOperationOutcome<T> {
    revision: WorkspaceRevision,
    outcome: T,
}

impl<T> WorkspaceOperationOutcome<T> {
    pub fn new(revision: WorkspaceRevision, outcome: T) -> Self {
        Self { revision, outcome }
    }

    pub const fn revision(&self) -> WorkspaceRevision {
        self.revision
    }

    pub const fn outcome(&self) -> &T {
        &self.outcome
    }

    pub fn into_outcome(self) -> T {
        self.outcome
    }
}
