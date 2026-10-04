// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use serde::Serialize;
use serde::ser::{SerializeStruct, Serializer};

use crate::ApplicationErrorCode;

pub const PUBLIC_FAILURE_REPORT_SCHEMA_VERSION: u16 = 2;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureClass {
    Expected,
    Operational,
    Defect,
}

impl FailureClass {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Expected => "expected",
            Self::Operational => "operational",
            Self::Defect => "defect",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FailureCode {
    Application(ApplicationErrorCode),
    OperationFailed,
    OperationTimedOut,
    OperationCancelled,
    InvariantViolation,
    UnexpectedFailure,
}

impl Serialize for FailureCode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl FailureCode {
    pub const fn class(self) -> FailureClass {
        match self {
            Self::Application(_) | Self::OperationCancelled => FailureClass::Expected,
            Self::OperationFailed | Self::OperationTimedOut => FailureClass::Operational,
            Self::InvariantViolation | Self::UnexpectedFailure => FailureClass::Defect,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Application(code) => code.as_str(),
            Self::OperationFailed => "operation_failed",
            Self::OperationTimedOut => "operation_timed_out",
            Self::OperationCancelled => "operation_cancelled",
            Self::InvariantViolation => "invariant_violation",
            Self::UnexpectedFailure => "unexpected_failure",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum FailureCause {
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

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PublicFailureDetails {
    RepositorySelectorAmbiguous { candidates: Vec<String> },
}

impl FailureCause {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PermissionDenied => "permission_denied",
            Self::ResourceUnavailable => "resource_unavailable",
            Self::ProcessExited { .. } => "process_exited",
            Self::IoFailure => "io_failure",
            Self::StorageFailure => "storage_failure",
            Self::InvalidData => "invalid_data",
            Self::OutputLimitExceeded => "output_limit_exceeded",
            Self::TimedOut => "timed_out",
            Self::Cancelled => "cancelled",
            Self::InvariantViolation => "invariant_violation",
            Self::Unexpected => "unexpected",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublicFailureDescriptor {
    error_code: FailureCode,
    causes: Vec<FailureCause>,
    details: Option<PublicFailureDetails>,
}

impl Serialize for PublicFailureDescriptor {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("PublicFailureDescriptor", 4)?;
        state.serialize_field("class", &self.class())?;
        state.serialize_field("error_code", &self.error_code)?;
        state.serialize_field("causes", &self.causes)?;
        if let Some(details) = &self.details {
            state.serialize_field("details", details)?;
        }
        state.end()
    }
}

impl PublicFailureDescriptor {
    pub fn new(error_code: FailureCode) -> Self {
        Self {
            error_code,
            causes: Vec::new(),
            details: None,
        }
    }

    pub fn with_causes(error_code: FailureCode, causes: Vec<FailureCause>) -> Self {
        Self {
            error_code,
            causes,
            details: None,
        }
    }

    pub fn with_details(error_code: FailureCode, details: PublicFailureDetails) -> Self {
        Self {
            error_code,
            causes: Vec::new(),
            details: Some(details),
        }
    }

    pub const fn class(&self) -> FailureClass {
        self.error_code.class()
    }

    pub const fn error_code(&self) -> FailureCode {
        self.error_code
    }

    pub fn causes(&self) -> &[FailureCause] {
        &self.causes
    }

    pub const fn details(&self) -> Option<&PublicFailureDetails> {
        self.details.as_ref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PublicFailureReport {
    schema_version: u16,
    application_version: &'static str,
    failure: PublicFailureDescriptor,
}

impl PublicFailureReport {
    pub fn new(failure: PublicFailureDescriptor) -> Self {
        Self {
            schema_version: PUBLIC_FAILURE_REPORT_SCHEMA_VERSION,
            application_version: env!("CARGO_PKG_VERSION"),
            failure,
        }
    }

    pub const fn schema_version(&self) -> u16 {
        self.schema_version
    }

    pub const fn application_version(&self) -> &'static str {
        self.application_version
    }

    pub const fn failure(&self) -> &PublicFailureDescriptor {
        &self.failure
    }
}

#[cfg(test)]
mod tests {
    use super::{
        FailureCause, FailureClass, FailureCode, PUBLIC_FAILURE_REPORT_SCHEMA_VERSION,
        PublicFailureDescriptor, PublicFailureDetails, PublicFailureReport,
    };
    use crate::ApplicationErrorCode;

    #[test]
    fn failure_classes_have_stable_machine_names() {
        assert_eq!(FailureClass::Expected.as_str(), "expected");
        assert_eq!(FailureClass::Operational.as_str(), "operational");
        assert_eq!(FailureClass::Defect.as_str(), "defect");
    }

    #[test]
    fn failure_codes_derive_their_class_without_external_pairing() {
        let cases = [
            (
                FailureCode::Application(ApplicationErrorCode::RepositoryNotFound),
                FailureClass::Expected,
                "repository_not_found",
            ),
            (
                FailureCode::OperationCancelled,
                FailureClass::Expected,
                "operation_cancelled",
            ),
            (
                FailureCode::OperationFailed,
                FailureClass::Operational,
                "operation_failed",
            ),
            (
                FailureCode::OperationTimedOut,
                FailureClass::Operational,
                "operation_timed_out",
            ),
            (
                FailureCode::InvariantViolation,
                FailureClass::Defect,
                "invariant_violation",
            ),
            (
                FailureCode::UnexpectedFailure,
                FailureClass::Defect,
                "unexpected_failure",
            ),
        ];

        for (code, expected_class, expected_name) in cases {
            assert_eq!(code.class(), expected_class);
            assert_eq!(code.as_str(), expected_name);
        }
    }

    #[test]
    fn failure_causes_have_stable_machine_names() {
        let cases = [
            (FailureCause::PermissionDenied, "permission_denied"),
            (FailureCause::ResourceUnavailable, "resource_unavailable"),
            (
                FailureCause::ProcessExited { exit_code: 128 },
                "process_exited",
            ),
            (FailureCause::IoFailure, "io_failure"),
            (FailureCause::StorageFailure, "storage_failure"),
            (FailureCause::InvalidData, "invalid_data"),
            (FailureCause::OutputLimitExceeded, "output_limit_exceeded"),
            (FailureCause::TimedOut, "timed_out"),
            (FailureCause::Cancelled, "cancelled"),
            (FailureCause::InvariantViolation, "invariant_violation"),
            (FailureCause::Unexpected, "unexpected"),
        ];

        for (cause, expected) in cases {
            assert_eq!(cause.as_str(), expected);
        }
    }

    #[test]
    fn public_failure_descriptor_preserves_only_typed_safe_causality() {
        let descriptor = PublicFailureDescriptor::with_causes(
            FailureCode::OperationFailed,
            vec![
                FailureCause::ProcessExited { exit_code: 128 },
                FailureCause::PermissionDenied,
            ],
        );

        assert_eq!(descriptor.class(), FailureClass::Operational);
        assert_eq!(descriptor.error_code(), FailureCode::OperationFailed);
        assert_eq!(
            descriptor.causes(),
            &[
                FailureCause::ProcessExited { exit_code: 128 },
                FailureCause::PermissionDenied,
            ]
        );
    }

    #[test]
    fn public_failure_report_adds_only_controlled_product_metadata() {
        let failure = PublicFailureDescriptor::with_causes(
            FailureCode::OperationTimedOut,
            vec![FailureCause::TimedOut],
        );
        let report = PublicFailureReport::new(failure.clone());

        assert_eq!(
            report.schema_version(),
            PUBLIC_FAILURE_REPORT_SCHEMA_VERSION
        );
        assert_eq!(report.application_version(), env!("CARGO_PKG_VERSION"));
        assert_eq!(report.failure(), &failure);
        assert_eq!(report.failure().class(), FailureClass::Operational);
    }

    #[test]
    fn public_failure_report_serialization_exposes_only_stable_safe_fields() {
        let report = PublicFailureReport::new(PublicFailureDescriptor::with_causes(
            FailureCode::OperationFailed,
            vec![
                FailureCause::ProcessExited { exit_code: 128 },
                FailureCause::PermissionDenied,
            ],
        ));

        let serialized =
            toml::Value::try_from(&report).expect("public failure report must serialize");

        assert_eq!(serialized["schema_version"].as_integer(), Some(2));
        assert_eq!(
            serialized["application_version"].as_str(),
            Some(env!("CARGO_PKG_VERSION"))
        );
        assert_eq!(serialized["failure"]["class"].as_str(), Some("operational"));
        assert_eq!(
            serialized["failure"]["error_code"].as_str(),
            Some("operation_failed")
        );
        assert_eq!(
            serialized["failure"]["causes"][0]["code"].as_str(),
            Some("process_exited")
        );
        assert_eq!(
            serialized["failure"]["causes"][0]["exit_code"].as_integer(),
            Some(128)
        );
        assert_eq!(
            serialized["failure"]["causes"][1]["code"].as_str(),
            Some("permission_denied")
        );
    }

    #[test]
    fn public_failure_details_expose_only_typed_safe_selector_candidates() {
        let report = PublicFailureReport::new(PublicFailureDescriptor::with_details(
            FailureCode::Application(ApplicationErrorCode::RepositorySelectorAmbiguous),
            PublicFailureDetails::RepositorySelectorAmbiguous {
                candidates: vec!["repository-a".to_owned(), "repository-b".to_owned()],
            },
        ));

        let serialized =
            toml::Value::try_from(&report).expect("public failure report must serialize");

        assert_eq!(
            serialized["failure"]["details"]["kind"].as_str(),
            Some("repository_selector_ambiguous")
        );
        assert_eq!(
            serialized["failure"]["details"]["candidates"][0].as_str(),
            Some("repository-a")
        );
        assert_eq!(
            serialized["failure"]["details"]["candidates"][1].as_str(),
            Some("repository-b")
        );
    }
}
