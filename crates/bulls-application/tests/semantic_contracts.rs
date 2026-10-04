// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use bulls_application::{
    ApplicationError, ApplicationErrorCode, FailureClass, FailureCode,
    PUBLIC_FAILURE_REPORT_SCHEMA_VERSION, WorkspaceRevision,
};
use bulls_core::{LocationId, RepositoryId, WorktreeId};

#[test]
fn public_error_codes_are_independent_from_typed_context() {
    let repository_a = ApplicationError::repository_not_found(RepositoryId::from("repository-a"));
    let repository_b = ApplicationError::repository_not_found(RepositoryId::from("repository-b"));
    let ambiguity =
        ApplicationError::repository_selector_ambiguous(vec![RepositoryId::from("repository-c")]);
    let location = ApplicationError::location_not_found(LocationId::from("location-1"));
    let worktree = ApplicationError::worktree_not_found(WorktreeId::from("worktree-1"));

    assert_ne!(repository_a, repository_b);
    assert_eq!(
        repository_a.code(),
        ApplicationErrorCode::RepositoryNotFound
    );
    assert_eq!(
        repository_b.code(),
        ApplicationErrorCode::RepositoryNotFound
    );
    assert_eq!(
        ambiguity.code(),
        ApplicationErrorCode::RepositorySelectorAmbiguous
    );
    assert_eq!(location.code(), ApplicationErrorCode::LocationNotFound);
    assert_eq!(worktree.code(), ApplicationErrorCode::WorktreeNotFound);
}

#[test]
fn public_errors_preserve_typed_context_for_presentation_consumers() {
    let repository_id = RepositoryId::from("repository-1");
    let candidates = vec![
        RepositoryId::from("repository-2"),
        RepositoryId::from("repository-3"),
    ];
    let location_id = LocationId::from("location-1");
    let worktree_id = WorktreeId::from("worktree-1");

    assert!(matches!(
        ApplicationError::repository_not_found(repository_id.clone()),
        ApplicationError::RepositoryNotFound {
            repository_id: actual
        } if actual == repository_id
    ));
    assert!(matches!(
        ApplicationError::repository_selector_ambiguous(candidates.clone()),
        ApplicationError::RepositorySelectorAmbiguous {
            candidates: actual
        } if actual == candidates
    ));
    assert!(matches!(
        ApplicationError::location_not_found(location_id.clone()),
        ApplicationError::LocationNotFound {
            location_id: actual
        } if actual == location_id
    ));
    assert!(matches!(
        ApplicationError::worktree_not_found(worktree_id.clone()),
        ApplicationError::WorktreeNotFound {
            worktree_id: actual
        } if actual == worktree_id
    ));
}

#[test]
fn public_failure_descriptors_are_independent_from_private_context() {
    let first = ApplicationError::repository_not_found(RepositoryId::from(
        "/home/alice/projects/customer-secret",
    ));
    let second = ApplicationError::repository_not_found(RepositoryId::from(
        "/mnt/private/bob/internal-repository",
    ));

    assert_ne!(first, second);
    assert_eq!(first.public_failure(), second.public_failure());
    assert_eq!(first.public_failure().class(), FailureClass::Expected);
    assert_eq!(
        first.public_failure().error_code(),
        FailureCode::Application(ApplicationErrorCode::RepositoryNotFound)
    );
    assert!(first.public_failure().causes().is_empty());
}

#[test]
fn public_failure_reports_cannot_correlate_equivalent_private_contexts() {
    let cases = [
        (
            ApplicationError::repository_not_found(RepositoryId::from(
                "/home/alice/projects/customer-secret",
            )),
            ApplicationError::repository_not_found(RepositoryId::from(
                "/mnt/private/bob/internal-repository",
            )),
        ),
        (
            ApplicationError::location_not_found(LocationId::from(
                "/home/alice/projects/customer-secret",
            )),
            ApplicationError::location_not_found(LocationId::from(
                "/mnt/private/bob/internal-repository",
            )),
        ),
        (
            ApplicationError::worktree_not_found(WorktreeId::from(
                "/home/alice/projects/customer-secret",
            )),
            ApplicationError::worktree_not_found(WorktreeId::from(
                "/mnt/private/bob/internal-repository",
            )),
        ),
    ];

    for (first, second) in cases {
        let first_report = first.public_report();
        let second_report = second.public_report();

        assert_eq!(first_report, second_report);
        assert_eq!(
            first_report.schema_version(),
            PUBLIC_FAILURE_REPORT_SCHEMA_VERSION
        );
        assert_eq!(
            first_report.application_version(),
            env!("CARGO_PKG_VERSION")
        );
        assert_eq!(first_report.failure(), &first.public_failure());

        let rendered = format!("{first_report:?}");
        assert!(!rendered.contains("alice"));
        assert!(!rendered.contains("customer-secret"));
        assert!(!rendered.contains("bob"));
        assert!(!rendered.contains("internal-repository"));
    }
}

#[test]
fn workspace_revision_has_monotonic_value_semantics() {
    let initial = WorkspaceRevision::INITIAL;
    let next = WorkspaceRevision::new(1);

    assert_eq!(initial.value(), 0);
    assert_eq!(next.value(), 1);
    assert!(next > initial);
}
