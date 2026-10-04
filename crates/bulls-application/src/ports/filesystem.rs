// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use super::PortResult;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FilesystemObjectId {
    filesystem_id: u128,
    object_id: u128,
}

impl FilesystemObjectId {
    pub const fn new(filesystem_id: u128, object_id: u128) -> Self {
        Self {
            filesystem_id,
            object_id,
        }
    }

    pub const fn filesystem_id(self) -> u128 {
        self.filesystem_id
    }

    pub const fn object_id(self) -> u128 {
        self.object_id
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FilesystemIdentityEvidence {
    Strong(FilesystemObjectId),
    PathOnly,
}

impl FilesystemIdentityEvidence {
    pub const fn strong(object_id: FilesystemObjectId) -> Self {
        Self::Strong(object_id)
    }

    pub const fn object_id(self) -> Option<FilesystemObjectId> {
        match self {
            Self::Strong(object_id) => Some(object_id),
            Self::PathOnly => None,
        }
    }

    pub const fn is_strong(self) -> bool {
        matches!(self, Self::Strong(_))
    }
}

pub trait FilesystemIdentityPort {
    fn identity(&self, path: &Path) -> PortResult<FilesystemIdentityEvidence>;
}

#[cfg(test)]
mod tests {
    use super::{FilesystemIdentityEvidence, FilesystemObjectId};

    #[test]
    fn filesystem_identity_distinguishes_strong_from_path_only_evidence() {
        let object_id = FilesystemObjectId::new(7, 11);
        let strong = FilesystemIdentityEvidence::strong(object_id);

        assert!(strong.is_strong());
        assert_eq!(strong.object_id(), Some(object_id));
        assert!(!FilesystemIdentityEvidence::PathOnly.is_strong());
        assert_eq!(FilesystemIdentityEvidence::PathOnly.object_id(), None);
    }
}
