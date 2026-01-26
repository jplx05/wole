//! Batch deletion feature.
//!
//! This module owns batch deletion operations and results.

use super::path_precheck::{precheck_path, PrecheckOutcome};
use super::single_deletion::{classify_anyhow_error, delete_with_precheck, DeleteOutcome};
use crate::debug_log;
use std::path::PathBuf;

#[derive(Debug)]
pub struct BatchDeleteResult {
    pub success_count: usize,
    pub error_count: usize,
    pub deleted_paths: Vec<PathBuf>,
    pub skipped_paths: Vec<PathBuf>,
    pub locked_paths: Vec<PathBuf>,
    pub permission_denied_paths: Vec<PathBuf>,
}

impl BatchDeleteResult {
    fn empty() -> Self {
        Self {
            success_count: 0,
            error_count: 0,
            deleted_paths: Vec::new(),
            skipped_paths: Vec::new(),
            locked_paths: Vec::new(),
            permission_denied_paths: Vec::new(),
        }
    }
}

fn partition_existing(paths: Vec<PathBuf>) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut remaining = Vec::new();
    let mut deleted = Vec::new();

    for path in paths {
        if path.exists() {
            remaining.push(path);
        } else {
            deleted.push(path);
        }
    }

    (remaining, deleted)
}

/// Batch clean multiple paths - MUCH faster than one-by-one deletion
///
/// For Recycle Bin deletion, uses `trash::delete_all()` which is 10-50x faster
/// than calling `trash::delete()` in a loop due to reduced COM/Shell API overhead.
///
/// **CRITICAL**: System paths are filtered out before deletion for safety.
///
/// Returns a detailed batch deletion result
pub fn clean_paths_batch(paths: &[PathBuf], permanent: bool) -> BatchDeleteResult {
    if paths.is_empty() {
        return BatchDeleteResult::empty();
    }

    let first_path = paths
        .first()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let last_path = paths
        .last()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    debug_log::cleaning_log(&format!(
        "batch delete start: permanent={} count={} first={} last={}",
        permanent,
        paths.len(),
        first_path,
        last_path
    ));

    let mut success_count = 0;
    let mut error_count = 0;
    let mut deleted_paths = Vec::with_capacity(paths.len());
    let mut skipped_paths: Vec<PathBuf> = Vec::new();
    let mut locked_paths: Vec<PathBuf> = Vec::new();
    let mut permission_denied_paths: Vec<PathBuf> = Vec::new();

    if permanent {
        // Permanent deletes are already fast (direct filesystem ops)
        // Delete one-by-one to track individual successes/failures
        for path in paths {
            match delete_with_precheck(path, true) {
                Ok(DeleteOutcome::Deleted) => {
                    success_count += 1;
                    deleted_paths.push(path.clone());
                }
                Ok(DeleteOutcome::SkippedMissing | DeleteOutcome::SkippedSystem) => {
                    skipped_paths.push(path.clone());
                }
                Ok(DeleteOutcome::SkippedLocked) => {
                    error_count += 1;
                    locked_paths.push(path.clone());
                }
                Ok(DeleteOutcome::SkippedPermission) => {
                    error_count += 1;
                    permission_denied_paths.push(path.clone());
                }
                Err(_) => error_count += 1,
            }
        }
    } else {
        // Batch to Recycle Bin - this is the big performance win
        // First, filter out locked, missing, and system paths (they would cause batch to fail)
        let mut unlocked: Vec<PathBuf> = Vec::new();
        for path in paths {
            match precheck_path(path) {
                PrecheckOutcome::Missing | PrecheckOutcome::BlockedSystem => {
                    skipped_paths.push(path.clone());
                }
                PrecheckOutcome::Locked => {
                    error_count += 1;
                    locked_paths.push(path.clone());
                }
                PrecheckOutcome::Eligible => unlocked.push(path.clone()),
            }
        }

        if !unlocked.is_empty() {
            // Try batch delete first (fastest path)
            match crate::trash_ops::delete_all(&unlocked) {
                Ok(()) => {
                    success_count += unlocked.len();
                    deleted_paths.extend(unlocked);
                }
                Err(_err) => {
                    debug_log::cleaning_log(&format!(
                        "batch delete_all failed: count={} error={}",
                        unlocked.len(),
                        _err
                    ));
                    let (mut remaining, deleted) = partition_existing(unlocked);
                    success_count += deleted.len();
                    deleted_paths.extend(deleted);

                    // Batch failed - try smaller batches first (in case one bad file causes failure)
                    // Then fallback to one-by-one if that also fails
                    const BATCH_SIZE: usize = 100;
                    #[allow(unused_assignments)]
                    let mut _batch_success = false;

                    // Try deleting in smaller batches
                    if remaining.len() > BATCH_SIZE {
                        debug_log::cleaning_log(&format!(
                            "batch delete fallback: splitting into chunks of {} (remaining={})",
                            BATCH_SIZE,
                            remaining.len()
                        ));
                        let batches: Vec<Vec<PathBuf>> = remaining
                            .chunks(BATCH_SIZE)
                            .map(|chunk| chunk.to_vec())
                            .collect();

                        let mut new_remaining: Vec<PathBuf> = Vec::new();
                        for batch in batches {
                            match crate::trash_ops::delete_all(&batch) {
                                Ok(()) => {
                                    success_count += batch.len();
                                    deleted_paths.extend(batch);
                                    _batch_success = true;
                                }
                                Err(batch_err) => {
                                    debug_log::cleaning_log(&format!(
                                        "batch chunk delete_all failed: count={} error={}",
                                        batch.len(),
                                        batch_err
                                    ));
                                    // This batch failed, keep any that still exist for one-by-one
                                    let (still_remaining, deleted) = partition_existing(batch);
                                    success_count += deleted.len();
                                    deleted_paths.extend(deleted);
                                    new_remaining.extend(still_remaining);
                                }
                            }
                        }
                        remaining = new_remaining;
                    }

                    // Fallback to one-by-one for any remaining files
                    if !remaining.is_empty() {
                        #[cfg(debug_assertions)]
                        if !_batch_success {
                            eprintln!(
                                "[DEBUG] Batch delete failed: {}, falling back to one-by-one for {} files",
                                _err,
                                remaining.len()
                            );
                        }
                        for path in remaining {
                            // Double-check file exists before attempting deletion
                            if !path.exists() {
                                success_count += 1;
                                deleted_paths.push(path);
                                continue;
                            }
                            match crate::trash_ops::delete(&path) {
                                Ok(()) => {
                                    success_count += 1;
                                    deleted_paths.push(path.clone());
                                }
                                Err(_err) => {
                                    if !path.exists() {
                                        success_count += 1;
                                        deleted_paths.push(path.clone());
                                    } else {
                                        match classify_anyhow_error(&path, &_err) {
                                            Some(DeleteOutcome::SkippedLocked) => {
                                                error_count += 1;
                                                locked_paths.push(path.clone());
                                            }
                                            Some(DeleteOutcome::SkippedPermission) => {
                                                error_count += 1;
                                                permission_denied_paths.push(path.clone());
                                            }
                                            _ => {
                                                error_count += 1;
                                            }
                                        }
                                    }
                                    debug_log::cleaning_log(&format!(
                                        "delete failed: path={} error={}",
                                        path.display(),
                                        _err
                                    ));
                                    #[cfg(debug_assertions)]
                                    eprintln!(
                                        "[DEBUG] Failed to delete {}: {}",
                                        path.display(),
                                        _err
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    debug_log::cleaning_log(&format!(
        "batch delete done: success={} errors={} skipped={} locked={} permission_denied={}",
        success_count,
        error_count,
        skipped_paths.len(),
        locked_paths.len(),
        permission_denied_paths.len()
    ));

    BatchDeleteResult {
        success_count,
        error_count,
        deleted_paths,
        skipped_paths,
        locked_paths,
        permission_denied_paths,
    }
}
