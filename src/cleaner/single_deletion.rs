//! Single deletion feature.
//!
//! This module owns single-path deletion and precheck-based deletion.

use super::path_precheck::{is_path_locked, precheck_path, PrecheckOutcome};
use crate::utils;
use anyhow::{Context, Result};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeleteOutcome {
    Deleted,
    SkippedMissing,
    SkippedLocked,
    SkippedPermission,
    SkippedSystem,
}

fn locked_error_codes() -> &'static [i32] {
    &[32, 33]
}

fn classify_permission_denied(path: &Path) -> DeleteOutcome {
    if is_path_locked(path) {
        DeleteOutcome::SkippedLocked
    } else {
        DeleteOutcome::SkippedPermission
    }
}

fn classify_io_error(path: &Path, err: &std::io::Error) -> Option<DeleteOutcome> {
    if err.kind() == std::io::ErrorKind::NotFound {
        return Some(DeleteOutcome::SkippedMissing);
    }

    if let Some(code) = err.raw_os_error() {
        if locked_error_codes().contains(&code) {
            return Some(DeleteOutcome::SkippedLocked);
        }

        if code == 5 {
            return Some(classify_permission_denied(path));
        }
    }

    if err.kind() == std::io::ErrorKind::PermissionDenied {
        return Some(classify_permission_denied(path));
    }

    None
}

pub(crate) fn classify_anyhow_error(path: &Path, err: &anyhow::Error) -> Option<DeleteOutcome> {
    if let Some(io_err) = err.downcast_ref::<std::io::Error>() {
        return classify_io_error(path, io_err);
    }

    for cause in err.chain() {
        if let Some(io_err) = cause.downcast_ref::<std::io::Error>() {
            return classify_io_error(path, io_err);
        }
    }

    None
}

pub fn delete_with_precheck(path: &Path, permanent: bool) -> Result<DeleteOutcome> {
    match precheck_path(path) {
        PrecheckOutcome::Missing => return Ok(DeleteOutcome::SkippedMissing),
        PrecheckOutcome::Locked => return Ok(DeleteOutcome::SkippedLocked),
        PrecheckOutcome::BlockedSystem => return Ok(DeleteOutcome::SkippedSystem),
        PrecheckOutcome::Eligible => {}
    }

    if permanent {
        let result = if path.is_dir() {
            utils::safe_remove_dir_all(path)
        } else {
            utils::safe_remove_file(path)
        };

        match result {
            Ok(()) => Ok(DeleteOutcome::Deleted),
            Err(err) => match classify_io_error(path, &err) {
                Some(outcome) => Ok(outcome),
                None => {
                    if !path.exists() {
                        Ok(DeleteOutcome::SkippedMissing)
                    } else {
                        Err(err).with_context(|| {
                            format!("Failed to permanently delete: {}", path.display())
                        })
                    }
                }
            },
        }
    } else {
        match crate::trash_ops::delete(path) {
            Ok(()) => Ok(DeleteOutcome::Deleted),
            Err(err) => match classify_anyhow_error(path, &err) {
                Some(outcome) => Ok(outcome),
                None => {
                    if !path.exists() {
                        Ok(DeleteOutcome::SkippedMissing)
                    } else {
                        Err(err).with_context(|| format!("Failed to delete: {}", path.display()))
                    }
                }
            },
        }
    }
}

/// Clean a single path, optionally permanently
///
/// Features:
/// - Checks for locked files before deletion (Windows)
/// - Uses long path support for paths > 260 characters
/// - Provides clear error messages
/// - **CRITICAL**: Blocks deletion of system directories for safety
pub fn clean_path(path: &Path, permanent: bool) -> Result<()> {
    // CRITICAL SAFETY CHECK: Never allow deletion of system paths
    // This provides defense-in-depth even if a system path somehow gets into the deletion list
    if utils::is_system_path(path) {
        return Err(anyhow::anyhow!(
            "Cannot delete system path: {}. System directories are protected from deletion.",
            path.display()
        ));
    }

    // Check if file is locked (Windows only)
    if is_path_locked(path) {
        return Err(anyhow::anyhow!("Path is locked by another process"));
    }

    if permanent {
        // Permanent delete - bypass Recycle Bin
        // Use safe_* functions for long path support
        if path.is_dir() {
            utils::safe_remove_dir_all(path).with_context(|| {
                format!("Failed to permanently delete directory: {}", path.display())
            })?;
        } else {
            utils::safe_remove_file(path).with_context(|| {
                format!("Failed to permanently delete file: {}", path.display())
            })?;
        }
    } else {
        // Move to Recycle Bin
        // Note: trash crate should handle long paths internally
        crate::trash_ops::delete(path)
            .with_context(|| format!("Failed to delete: {}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_dir() -> TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn test_clean_path_nonexistent() {
        let temp_dir = create_test_dir();
        let nonexistent = temp_dir.path().join("nonexistent.txt");

        // Cleaning a non-existent file should fail
        let result = clean_path(&nonexistent, true);
        assert!(result.is_err());
    }
}
