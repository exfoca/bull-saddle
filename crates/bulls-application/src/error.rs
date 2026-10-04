// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use bulls_core::{LocationId, RepositoryId, WorktreeId};

use crate::{FailureCode, PublicFailureDescriptor, PublicFailureDetails, PublicFailureReport};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ApplicationErrorCode {
    RepositoryNotFound,
    RepositorySelectorAmbiguous,
    LocationNotFound,
    WorktreeNotFound,
}

impl ApplicationErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RepositoryNotFound => "repository_not_found",
            Self::RepositorySelectorAmbiguous => "repository_selector_ambiguous",
            Self::LocationNotFound => "location_not_found",
            Self::WorktreeNotFound => "worktree_not_found",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ApplicationError {
    RepositoryNotFound { repository_id: RepositoryId },
    RepositorySelectorAmbiguous { candidates: Vec<RepositoryId> },
    LocationNotFound { location_id: LocationId },
    WorktreeNotFound { worktree_id: WorktreeId },
}

impl ApplicationError {
    pub fn repository_not_found(repository_id: RepositoryId) -> Self {
        Self::RepositoryNotFound { repository_id }
    }

    pub fn repository_selector_ambiguous(candidates: Vec<RepositoryId>) -> Self {
        Self::RepositorySelectorAmbiguous { candidates }
    }

    pub fn location_not_found(location_id: LocationId) -> Self {
        Self::LocationNotFound { location_id }
    }

    pub fn worktree_not_found(worktree_id: WorktreeId) -> Self {
        Self::WorktreeNotFound { worktree_id }
    }

    pub const fn code(&self) -> ApplicationErrorCode {
        match self {
            Self::RepositoryNotFound { .. } => ApplicationErrorCode::RepositoryNotFound,
            Self::RepositorySelectorAmbiguous { .. } => {
                ApplicationErrorCode::RepositorySelectorAmbiguous
            }
            Self::LocationNotFound { .. } => ApplicationErrorCode::LocationNotFound,
            Self::WorktreeNotFound { .. } => ApplicationErrorCode::WorktreeNotFound,
        }
    }

    pub fn public_failure(&self) -> PublicFailureDescriptor {
        match self {
            Self::RepositorySelectorAmbiguous { candidates } => {
                PublicFailureDescriptor::with_details(
                    FailureCode::Application(self.code()),
                    PublicFailureDetails::RepositorySelectorAmbiguous {
                        candidates: candidates
                            .iter()
                            .map(|repository_id| repository_id.as_str().to_owned())
                            .collect(),
                    },
                )
            }
            Self::RepositoryNotFound { .. }
            | Self::LocationNotFound { .. }
            | Self::WorktreeNotFound { .. } => {
                PublicFailureDescriptor::new(FailureCode::Application(self.code()))
            }
        }
    }

    pub fn public_report(&self) -> PublicFailureReport {
        PublicFailureReport::new(self.public_failure())
    }
}

#[cfg(test)]
mod tests {
    use bulls_core::{LocationId, RepositoryId, WorktreeId};

    use super::{ApplicationError, ApplicationErrorCode};
    use crate::{FailureClass, FailureCode};

    #[test]
    fn application_errors_expose_stable_semantic_codes() {
        let cases = [
            (
                ApplicationError::repository_not_found(RepositoryId::from("repository-1")),
                ApplicationErrorCode::RepositoryNotFound,
            ),
            (
                ApplicationError::repository_selector_ambiguous(vec![RepositoryId::from(
                    "repository-2",
                )]),
                ApplicationErrorCode::RepositorySelectorAmbiguous,
            ),
            (
                ApplicationError::location_not_found(LocationId::from("location-1")),
                ApplicationErrorCode::LocationNotFound,
            ),
            (
                ApplicationError::worktree_not_found(WorktreeId::from("worktree-1")),
                ApplicationErrorCode::WorktreeNotFound,
            ),
        ];

        for (error, expected) in cases {
            assert_eq!(error.code(), expected);
        }
    }

    #[test]
    fn application_error_codes_have_stable_machine_names() {
        assert_eq!(
            ApplicationErrorCode::RepositoryNotFound.as_str(),
            "repository_not_found"
        );
        assert_eq!(
            ApplicationErrorCode::RepositorySelectorAmbiguous.as_str(),
            "repository_selector_ambiguous"
        );
        assert_eq!(
            ApplicationErrorCode::LocationNotFound.as_str(),
            "location_not_found"
        );
        assert_eq!(
            ApplicationErrorCode::WorktreeNotFound.as_str(),
            "worktree_not_found"
        );
    }

    #[test]
    fn application_errors_expose_privacy_safe_public_failures() {
        let error = ApplicationError::repository_not_found(RepositoryId::from("private-id"));
        let public_failure = error.public_failure();
        let public_report = error.public_report();

        assert_eq!(public_failure.class(), FailureClass::Expected);
        assert_eq!(
            public_failure.error_code(),
            FailureCode::Application(ApplicationErrorCode::RepositoryNotFound)
        );
        assert!(public_failure.causes().is_empty());
        assert_eq!(public_report.failure(), &public_failure);
    }

    #[test]
    fn ambiguous_selector_failure_exposes_only_safe_repository_candidates() {
        let error = ApplicationError::repository_selector_ambiguous(vec![
            RepositoryId::from("repository-b"),
            RepositoryId::from("repository-a"),
        ]);
        let failure = error.public_failure();

        assert_eq!(
            failure.error_code(),
            FailureCode::Application(ApplicationErrorCode::RepositorySelectorAmbiguous)
        );
        assert!(failure.causes().is_empty());
        assert_eq!(
            failure.details(),
            Some(&crate::PublicFailureDetails::RepositorySelectorAmbiguous {
                candidates: vec!["repository-b".to_owned(), "repository-a".to_owned()],
            })
        );
    }

    #[test]
    fn application_errors_preserve_typed_context() {
        let repository_id = RepositoryId::from("repository-1");
        let candidates = vec![RepositoryId::from("repository-2")];
        let location_id = LocationId::from("location-1");
        let worktree_id = WorktreeId::from("worktree-1");

        assert_eq!(
            ApplicationError::repository_not_found(repository_id.clone()),
            ApplicationError::RepositoryNotFound { repository_id }
        );
        assert_eq!(
            ApplicationError::repository_selector_ambiguous(candidates.clone()),
            ApplicationError::RepositorySelectorAmbiguous { candidates }
        );
        assert_eq!(
            ApplicationError::location_not_found(location_id.clone()),
            ApplicationError::LocationNotFound { location_id }
        );
        assert_eq!(
            ApplicationError::worktree_not_found(worktree_id.clone()),
            ApplicationError::WorktreeNotFound { worktree_id }
        );
    }
}
