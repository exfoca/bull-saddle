// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use std::io;

use bulls_application::ports::{PortError, PortErrorKind};
use rusqlite::{Error as SqliteError, ErrorCode};

use crate::process::ProcessError;

pub(crate) fn io_port_error(error: &io::Error) -> PortError {
    let kind = match error.kind() {
        io::ErrorKind::PermissionDenied => PortErrorKind::PermissionDenied,
        io::ErrorKind::InvalidData => PortErrorKind::InvalidData,
        _ => PortErrorKind::IoFailure,
    };

    PortError::new(kind)
}

pub(crate) fn process_port_error(error: ProcessError) -> PortError {
    let kind = match error {
        ProcessError::Io(error) => return io_port_error(&error),
        ProcessError::TimedOut => PortErrorKind::TimedOut,
        ProcessError::Cancelled => PortErrorKind::Cancelled,
        ProcessError::OutputLimitExceeded => PortErrorKind::OutputLimitExceeded,
    };

    PortError::new(kind)
}

pub(crate) fn sqlite_port_error(error: &SqliteError) -> PortError {
    let kind = match error.sqlite_error_code() {
        Some(ErrorCode::PermissionDenied | ErrorCode::ReadOnly) => PortErrorKind::PermissionDenied,
        Some(ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked | ErrorCode::CannotOpen) => {
            PortErrorKind::ResourceUnavailable
        }
        Some(ErrorCode::DatabaseCorrupt | ErrorCode::NotADatabase | ErrorCode::TypeMismatch) => {
            PortErrorKind::InvalidData
        }
        Some(ErrorCode::OperationAborted | ErrorCode::OperationInterrupted) => {
            PortErrorKind::Cancelled
        }
        _ => PortErrorKind::StorageFailure,
    };

    PortError::new(kind)
}

pub(crate) const fn invalid_data() -> PortError {
    PortError::new(PortErrorKind::InvalidData)
}

pub(crate) const fn invariant_violation() -> PortError {
    PortError::new(PortErrorKind::InvariantViolation)
}
