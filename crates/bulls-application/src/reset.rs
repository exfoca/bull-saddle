// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use crate::WorkspaceOperationOutcome;
use crate::ports::{PortResult, WorkspaceResetPort};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ResetWorkspaceOutcome;

pub struct ResetWorkspace<'a> {
    workspace: &'a mut dyn WorkspaceResetPort,
}

impl<'a> ResetWorkspace<'a> {
    pub fn new(workspace: &'a mut dyn WorkspaceResetPort) -> Self {
        Self { workspace }
    }

    pub fn execute(&mut self) -> PortResult<WorkspaceOperationOutcome<ResetWorkspaceOutcome>> {
        let revision = self.workspace.reset_workspace_knowledge()?;
        Ok(WorkspaceOperationOutcome::new(
            revision,
            ResetWorkspaceOutcome,
        ))
    }
}

#[cfg(test)]
mod tests {
    use crate::ports::{PortResult, WorkspaceResetPort};
    use crate::{ResetWorkspace, WorkspaceRevision};

    struct ResetStub {
        revision: WorkspaceRevision,
        calls: usize,
    }

    impl WorkspaceResetPort for ResetStub {
        fn reset_workspace_knowledge(&mut self) -> PortResult<WorkspaceRevision> {
            self.calls += 1;
            Ok(self.revision)
        }
    }

    #[test]
    fn reset_workspace_returns_the_revision_created_by_the_storage_boundary() {
        let mut port = ResetStub {
            revision: WorkspaceRevision::new(9),
            calls: 0,
        };

        let outcome = ResetWorkspace::new(&mut port)
            .execute()
            .expect("workspace reset must succeed");

        assert_eq!(outcome.revision(), WorkspaceRevision::new(9));
        assert_eq!(port.calls, 1);
    }
}
