// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use crate::{FailureCause, FailureCode, PublicFailureDescriptor, PublicFailureReport};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PortErrorKind {
    PermissionDenied,
    ResourceUnavailable,
    ProcessExited { exit_code: i32 },
    IoFailure,
    StorageFailure,
    InvalidData,
    OutputLimitExceeded,
    TimedOut,
    Cancelled,
    InvariantViolation,
    Unexpected,
}

impl PortErrorKind {
    const fn failure_code(self) -> FailureCode {
        match self {
            Self::TimedOut => FailureCode::OperationTimedOut,
            Self::Cancelled => FailureCode::OperationCancelled,
            Self::InvariantViolation => FailureCode::InvariantViolation,
            Self::Unexpected => FailureCode::UnexpectedFailure,
            Self::PermissionDenied
            | Self::ResourceUnavailable
            | Self::ProcessExited { .. }
            | Self::IoFailure
            | Self::StorageFailure
            | Self::InvalidData
            | Self::OutputLimitExceeded => FailureCode::OperationFailed,
        }
    }

    pub const fn failure_cause(self) -> FailureCause {
        match self {
            Self::PermissionDenied => FailureCause::PermissionDenied,
            Self::ResourceUnavailable => FailureCause::ResourceUnavailable,
            Self::ProcessExited { exit_code } => FailureCause::ProcessExited { exit_code },
            Self::IoFailure => FailureCause::IoFailure,
            Self::StorageFailure => FailureCause::StorageFailure,
            Self::InvalidData => FailureCause::InvalidData,
            Self::OutputLimitExceeded => FailureCause::OutputLimitExceeded,
            Self::TimedOut => FailureCause::TimedOut,
            Self::Cancelled => FailureCause::Cancelled,
            Self::InvariantViolation => FailureCause::InvariantViolation,
            Self::Unexpected => FailureCause::Unexpected,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PortError {
    kind: PortErrorKind,
}

impl PortError {
    pub const fn new(kind: PortErrorKind) -> Self {
        Self { kind }
    }

    pub const fn kind(&self) -> PortErrorKind {
        self.kind
    }

    pub fn public_failure(&self) -> PublicFailureDescriptor {
        PublicFailureDescriptor::with_causes(
            self.kind.failure_code(),
            vec![self.kind.failure_cause()],
        )
    }

    pub fn public_report(&self) -> PublicFailureReport {
        PublicFailureReport::new(self.public_failure())
    }
}

pub type PortResult<T> = Result<T, PortError>;

#[cfg(test)]
mod tests {
    use super::{PortError, PortErrorKind, PortResult};
    use crate::{FailureCause, FailureClass, FailureCode, PUBLIC_FAILURE_REPORT_SCHEMA_VERSION};

    #[test]
    fn port_errors_map_to_the_existing_failure_contract() {
        let cases = [
            (
                PortErrorKind::PermissionDenied,
                FailureCode::OperationFailed,
                FailureCause::PermissionDenied,
                FailureClass::Operational,
            ),
            (
                PortErrorKind::ResourceUnavailable,
                FailureCode::OperationFailed,
                FailureCause::ResourceUnavailable,
                FailureClass::Operational,
            ),
            (
                PortErrorKind::ProcessExited { exit_code: 128 },
                FailureCode::OperationFailed,
                FailureCause::ProcessExited { exit_code: 128 },
                FailureClass::Operational,
            ),
            (
                PortErrorKind::IoFailure,
                FailureCode::OperationFailed,
                FailureCause::IoFailure,
                FailureClass::Operational,
            ),
            (
                PortErrorKind::StorageFailure,
                FailureCode::OperationFailed,
                FailureCause::StorageFailure,
                FailureClass::Operational,
            ),
            (
                PortErrorKind::InvalidData,
                FailureCode::OperationFailed,
                FailureCause::InvalidData,
                FailureClass::Operational,
            ),
            (
                PortErrorKind::OutputLimitExceeded,
                FailureCode::OperationFailed,
                FailureCause::OutputLimitExceeded,
                FailureClass::Operational,
            ),
            (
                PortErrorKind::TimedOut,
                FailureCode::OperationTimedOut,
                FailureCause::TimedOut,
                FailureClass::Operational,
            ),
            (
                PortErrorKind::Cancelled,
                FailureCode::OperationCancelled,
                FailureCause::Cancelled,
                FailureClass::Expected,
            ),
            (
                PortErrorKind::InvariantViolation,
                FailureCode::InvariantViolation,
                FailureCause::InvariantViolation,
                FailureClass::Defect,
            ),
            (
                PortErrorKind::Unexpected,
                FailureCode::UnexpectedFailure,
                FailureCause::Unexpected,
                FailureClass::Defect,
            ),
        ];

        for (kind, expected_code, expected_cause, expected_class) in cases {
            let error = PortError::new(kind);
            let failure = error.public_failure();

            assert_eq!(error.kind(), kind);
            assert_eq!(failure.error_code(), expected_code);
            assert_eq!(failure.causes(), &[expected_cause]);
            assert_eq!(failure.class(), expected_class);
        }
    }

    #[test]
    fn port_failure_reports_expose_only_controlled_diagnostics() {
        let error = PortError::new(PortErrorKind::ProcessExited { exit_code: 128 });
        let report = error.public_report();

        assert_eq!(
            report.schema_version(),
            PUBLIC_FAILURE_REPORT_SCHEMA_VERSION
        );
        assert_eq!(report.application_version(), env!("CARGO_PKG_VERSION"));
        assert_eq!(report.failure(), &error.public_failure());
        assert_eq!(
            report.failure().causes(),
            &[FailureCause::ProcessExited { exit_code: 128 }]
        );
    }

    #[test]
    fn port_result_uses_the_boundary_error_type() {
        let result: PortResult<()> = Err(PortError::new(PortErrorKind::TimedOut));

        assert_eq!(result, Err(PortError::new(PortErrorKind::TimedOut)));
    }
}
