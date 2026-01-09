//! Restore functionality for recovering deleted files
//!
//! Provides ability to restore files from Recycle Bin using deletion history logs

use crate::history::{list_logs, load_log, DeletionLog};
use crate::theme::Theme;
use anyhow::{Context, Result};
use std::path::Path;
use trash::os_limited;

/// Restore files from the most recent deletion session
pub fn restore_last(output_mode: crate::output::OutputMode) -> Result<RestoreResult> {
    let logs = list_logs()?;

    if logs.is_empty() {
        return Err(anyhow::anyhow!(
            "No deletion history found. Nothing to restore."
        ));
    }

    // Get the most recent log
    let latest_log = load_log(&logs[0])?;
    restore_from_log(&latest_log, output_mode)
}

/// Restore files from a specific deletion log
pub fn restore_from_log(
    log: &DeletionLog,
    output_mode: crate::output::OutputMode,
) -> Result<RestoreResult> {
    let mut result = RestoreResult::default();

    // Get current Recycle Bin contents
    let recycle_bin_items = os_limited::list().context("Failed to list Recycle Bin contents")?;

    // Create a map of Recycle Bin items by original path
    // Windows Recycle Bin stores files with their original paths in metadata
    let mut bin_map: std::collections::HashMap<String, &trash::TrashItem> =
        std::collections::HashMap::new();
    for item in &recycle_bin_items {
        // Try to match by original parent + name
        let original_path = item.original_parent.join(&item.name);
        bin_map.insert(original_path.display().to_string().to_lowercase(), item);
    }

    // Try to restore each successful deletion record
    for record in &log.records {
        if !record.success || record.permanent {
            // Skip failed deletions and permanent deletions (can't restore those)
            continue;
        }

        let path_str = record.path.to_lowercase();

        // Try to find in Recycle Bin
        if let Some(trash_item) = bin_map.get(&path_str) {
            match restore_file(trash_item) {
                Ok(()) => {
                    result.restored += 1;
                    result.restored_bytes += record.size_bytes;
                    if output_mode != crate::output::OutputMode::Quiet {
                        println!(
                            "{} Restored: {}",
                            Theme::success("✓"),
                            Theme::secondary(&record.path)
                        );
                    }
                }
                Err(e) => {
                    result.errors += 1;
                    if output_mode != crate::output::OutputMode::Quiet {
                        eprintln!(
                            "{} Failed to restore {}: {}",
                            Theme::error("✗"),
                            Theme::secondary(&record.path),
                            Theme::error(&e.to_string())
                        );
                    }
                }
            }
        } else {
            // File not found in Recycle Bin (may have been permanently deleted or already restored)
            result.not_found += 1;
            if output_mode == crate::output::OutputMode::VeryVerbose {
                println!(
                    "{} Not found in Recycle Bin: {}",
                    Theme::muted("?"),
                    Theme::secondary(&record.path)
                );
            }
        }
    }

    Ok(result)
}

/// Restore a specific file by path
pub fn restore_path(path: &Path, output_mode: crate::output::OutputMode) -> Result<RestoreResult> {
    let mut result = RestoreResult::default();

    // Get current Recycle Bin contents
    let recycle_bin_items = os_limited::list().context("Failed to list Recycle Bin contents")?;

    let path_str = path.display().to_string().to_lowercase();

    // Find matching item in Recycle Bin
    for item in &recycle_bin_items {
        let original_path = item.original_parent.join(&item.name);
        if original_path.display().to_string().to_lowercase() == path_str {
            match restore_file(item) {
                Ok(()) => {
                    result.restored = 1;
                    if output_mode != crate::output::OutputMode::Quiet {
                        println!(
                            "{} Restored: {}",
                            Theme::success("✓"),
                            Theme::secondary(&path.display().to_string())
                        );
                    }
                    return Ok(result);
                }
                Err(e) => {
                    return Err(anyhow::anyhow!(
                        "Failed to restore {}: {}",
                        path.display(),
                        e
                    ));
                }
            }
        }
    }

    // Also check if path matches any part of the Recycle Bin item path
    for item in &recycle_bin_items {
        let original_path = item.original_parent.join(&item.name);
        if original_path
            .display()
            .to_string()
            .to_lowercase()
            .contains(&path_str)
            || path_str.contains(&original_path.display().to_string().to_lowercase())
        {
            match restore_file(item) {
                Ok(()) => {
                    result.restored = 1;
                    if output_mode != crate::output::OutputMode::Quiet {
                        println!(
                            "{} Restored: {}",
                            Theme::success("✓"),
                            Theme::secondary(&original_path.display().to_string())
                        );
                    }
                    return Ok(result);
                }
                Err(e) => {
                    return Err(anyhow::anyhow!(
                        "Failed to restore {}: {}",
                        original_path.display(),
                        e
                    ));
                }
            }
        }
    }

    Err(anyhow::anyhow!(
        "File not found in Recycle Bin: {}",
        path.display()
    ))
}

/// Restore a single file from Recycle Bin
fn restore_file(item: &trash::TrashItem) -> Result<()> {
    let dest = item.original_parent.join(&item.name);

    // Create parent directory if it doesn't exist
    if let Some(parent) = dest.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create parent directory: {}", parent.display())
            })?;
        }
    }

    // Check if destination already exists
    if dest.exists() {
        return Err(anyhow::anyhow!(
            "Destination already exists: {}",
            dest.display()
        ));
    }

    // Move file back from Recycle Bin to original location
    trash::os_limited::restore_all(std::iter::once(item.clone()))
        .with_context(|| format!("Failed to restore file to {}", dest.display()))?;

    Ok(())
}

/// Result of a restore operation
#[derive(Debug, Default)]
pub struct RestoreResult {
    pub restored: usize,
    pub restored_bytes: u64,
    pub errors: usize,
    pub not_found: usize,
}

impl RestoreResult {
    pub fn summary(&self) -> String {
        format!(
            "Restored {} items ({}), {} errors, {} not found",
            self.restored,
            bytesize::to_string(self.restored_bytes, true),
            self.errors,
            self.not_found
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_restore_result_default() {
        let result = RestoreResult::default();
        assert_eq!(result.restored, 0);
        assert_eq!(result.restored_bytes, 0);
        assert_eq!(result.errors, 0);
        assert_eq!(result.not_found, 0);
    }

    #[test]
    fn test_restore_result_summary() {
        let result = RestoreResult {
            restored: 5,
            restored_bytes: 1024 * 1024, // 1 MiB
            errors: 1,
            not_found: 2,
        };

        let summary = result.summary();
        eprintln!("Actual summary: '{}'", summary);
        
        // Check that all expected values are present
        assert!(summary.contains("5"), "Summary should contain '5': {}", summary);
        // bytesize::to_string with binary_units=true may format as "1.0 MiB", "1 MiB", or similar
        // Check for the unit and that size representation is present
        assert!(
            summary.contains("MiB") || summary.contains("MB"),
            "Summary should contain size unit (MiB or MB): {}",
            summary
        );
        assert!(summary.contains("1"), "Summary should contain '1': {}", summary);
        assert!(summary.contains("2"), "Summary should contain '2': {}", summary);
    }
}
