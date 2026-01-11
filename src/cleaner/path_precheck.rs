//! Path precheck feature.
//!
//! This module owns path eligibility checks prior to deletion.

use crate::utils;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PrecheckOutcome {
    Eligible,
    Missing,
    Locked,
    BlockedSystem,
}

/// Check if a path is locked by another process (Windows-specific)
///
/// Attempts to open the path with DELETE access and full sharing. If it fails with
/// sharing/access errors, the path is considered in use and likely not deletable.
#[cfg(windows)]
pub(crate) fn is_path_locked(path: &Path) -> bool {
    use std::fs::OpenOptions;
    use std::os::windows::fs::OpenOptionsExt;

    if !path.exists() {
        return false;
    }

    const FILE_SHARE_READ: u32 = 0x00000001;
    const FILE_SHARE_WRITE: u32 = 0x00000002;
    const FILE_SHARE_DELETE: u32 = 0x00000004;
    const DELETE: u32 = 0x00010000;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x02000000;

    let mut options = OpenOptions::new();
    options
        .access_mode(DELETE)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE);
    if path.is_dir() {
        options.custom_flags(FILE_FLAG_BACKUP_SEMANTICS);
    }

    match options.open(path) {
        Ok(_) => false,
        Err(e) if matches!(e.raw_os_error(), Some(5) | Some(32) | Some(33)) => true, // ERROR_ACCESS_DENIED, ERROR_SHARING_VIOLATION, ERROR_LOCK_VIOLATION
        Err(_) => false,
    }
}

#[cfg(not(windows))]
pub(crate) fn is_path_locked(_path: &Path) -> bool {
    // On Unix, file locking works differently (advisory locks)
    // We don't check for locks here as files can still be deleted
    false
}

pub(crate) fn precheck_path(path: &Path) -> PrecheckOutcome {
    if utils::is_system_path(path) {
        return PrecheckOutcome::BlockedSystem;
    }

    if !path.exists() {
        return PrecheckOutcome::Missing;
    }

    if is_path_locked(path) {
        return PrecheckOutcome::Locked;
    }

    PrecheckOutcome::Eligible
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_dir() -> TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn test_is_path_locked_regular_file() {
        let temp_dir = create_test_dir();
        let file = temp_dir.path().join("unlocked.txt");
        fs::write(&file, "test").unwrap();

        // File should not be locked
        assert!(!is_path_locked(&file));
    }

    #[test]
    fn test_is_path_locked_directory() {
        let temp_dir = create_test_dir();
        let dir = temp_dir.path().join("testdir");
        fs::create_dir(&dir).unwrap();

        // Directories without open handles should not be locked
        assert!(!is_path_locked(&dir));
    }

    #[test]
    fn test_is_path_locked_nonexistent() {
        let temp_dir = create_test_dir();
        let nonexistent = temp_dir.path().join("nonexistent.txt");

        // Non-existent files are not locked
        assert!(!is_path_locked(&nonexistent));
    }
}
