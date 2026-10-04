// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use std::fmt;

macro_rules! define_id {
    ($name:ident) => {
        #[derive(Clone, Debug, Eq, Hash, PartialEq)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.as_str())
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self::new(value)
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self::new(value)
            }
        }
    };
}

define_id!(RepositoryId);
define_id!(LocationId);
define_id!(WorktreeId);
define_id!(ObservationRunId);

#[cfg(test)]
mod tests {
    use super::{LocationId, ObservationRunId, RepositoryId, WorktreeId};

    #[test]
    fn identifiers_preserve_their_opaque_value() {
        let repository_id = RepositoryId::new("shared-token");
        let location_id = LocationId::new("shared-token");
        let worktree_id = WorktreeId::new("shared-token");
        let observation_run_id = ObservationRunId::new("shared-token");

        assert_eq!(repository_id.as_str(), "shared-token");
        assert_eq!(location_id.as_str(), "shared-token");
        assert_eq!(worktree_id.as_str(), "shared-token");
        assert_eq!(observation_run_id.as_str(), "shared-token");
    }

    #[test]
    fn identifiers_have_value_semantics_within_their_own_type() {
        let repository_id = RepositoryId::new("repository-1");
        let same_repository_id = RepositoryId::from("repository-1");
        let other_repository_id = RepositoryId::from("repository-2");

        assert_eq!(repository_id, same_repository_id);
        assert_ne!(repository_id, other_repository_id);
        assert_eq!(repository_id.to_string(), "repository-1");
    }
}
