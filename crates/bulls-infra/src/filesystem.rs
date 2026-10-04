// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use std::fs::{self, Metadata};
use std::path::Path;

use bulls_application::ports::{
    FilesystemIdentityEvidence, FilesystemIdentityPort, FilesystemObjectId, PortResult,
};

use crate::error::io_port_error;

#[derive(Clone, Copy, Debug, Default)]
pub struct NativeFilesystemIdentity;

impl NativeFilesystemIdentity {
    pub const fn new() -> Self {
        Self
    }
}

impl FilesystemIdentityPort for NativeFilesystemIdentity {
    fn identity(&self, path: &Path) -> PortResult<FilesystemIdentityEvidence> {
        let metadata = fs::metadata(path).map_err(|error| io_port_error(&error))?;
        Ok(filesystem_identity(path, &metadata))
    }
}

#[cfg(unix)]
pub(crate) fn filesystem_identity(_path: &Path, metadata: &Metadata) -> FilesystemIdentityEvidence {
    use std::os::unix::fs::MetadataExt;

    FilesystemIdentityEvidence::strong(FilesystemObjectId::new(
        u128::from(metadata.dev()),
        u128::from(metadata.ino()),
    ))
}

#[cfg(windows)]
pub(crate) fn filesystem_identity(path: &Path, _metadata: &Metadata) -> FilesystemIdentityEvidence {
    match file_id::get_high_res_file_id(path) {
        Ok(file_id::FileId::HighRes {
            volume_serial_number,
            file_id,
        }) => FilesystemIdentityEvidence::strong(FilesystemObjectId::new(
            u128::from(volume_serial_number),
            file_id,
        )),
        Ok(_) | Err(_) => FilesystemIdentityEvidence::PathOnly,
    }
}

#[cfg(not(any(unix, windows)))]
pub(crate) fn filesystem_identity(
    _path: &Path,
    _metadata: &Metadata,
) -> FilesystemIdentityEvidence {
    FilesystemIdentityEvidence::PathOnly
}

pub(crate) fn filesystem_object_id(path: &Path, metadata: &Metadata) -> Option<FilesystemObjectId> {
    filesystem_identity(path, metadata).object_id()
}

pub(crate) fn filesystem_id(path: &Path, metadata: &Metadata) -> Option<u128> {
    filesystem_object_id(path, metadata).map(FilesystemObjectId::filesystem_id)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    use bulls_application::ports::{FilesystemIdentityEvidence, FilesystemIdentityPort};

    use super::NativeFilesystemIdentity;

    static TEST_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let sequence = TEST_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "bulls-filesystem-test-{}-{}",
                std::process::id(),
                sequence
            ));
            fs::create_dir_all(&path).expect("test directory must be created");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn strong_filesystem_identity_survives_rename_when_available() {
        let root = TestDirectory::new();
        let first = root.path().join("before");
        let second = root.path().join("after");
        fs::create_dir(&first).expect("fixture directory must be created");

        let identity = NativeFilesystemIdentity::new();
        let before = identity
            .identity(&first)
            .expect("filesystem identity lookup must succeed");

        if before == FilesystemIdentityEvidence::PathOnly {
            return;
        }

        fs::rename(&first, &second).expect("fixture directory must be renamed");

        let after = identity
            .identity(&second)
            .expect("filesystem identity lookup must succeed");

        assert!(matches!(before, FilesystemIdentityEvidence::Strong(_)));
        assert_eq!(before, after);
    }
}
