// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use crate::WorkspaceRevision;

use super::PortResult;

pub trait WorkspaceRevisionPort {
    fn workspace_revision(&self) -> PortResult<WorkspaceRevision>;
}
