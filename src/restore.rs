//! Restore functionality for recovering deleted files
//!
//! Provides ability to restore files from Recycle Bin using deletion history logs

use crate::history::{list_logs, load_log, DeletionLog, DeletionRecord};
use crate::theme::Theme;
use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
use std::env;
use std::io::Write;
use std::path::{Path, PathBuf};
use trash::os_limited;

/// Callback function type for progress updates during restoration
pub type RestoreProgressCallback =
    Box<dyn FnMut(Option<&Path>, usize, usize, usize, usize) -> Result<()>>;

/// Get the count of files that can be restored from the most recent deletion session
pub fn get_restore_count() -> Result<usize> {
    let logs = list_logs()?;

    if logs.is_empty() {
        return Ok(0);
    }

    // Get the most recent log
    let latest_log = load_log(&logs[0])?;

    // Count restorable items (successful, non-permanent deletions)
    let count = latest_log
        .records
        .iter()
        .filter(|r| r.success && !r.permanent)
        .count();

    Ok(count)
}

/// Restore files from the most recent deletion session
pub fn restore_last(output_mode: crate::output::OutputMode) -> Result<RestoreResult> {
    restore_last_with_progress(output_mode, None)
}

/// Restore files from the most recent deletion session with progress callback
pub fn restore_last_with_progress(
    output_mode: crate::output::OutputMode,
    progress_callback: Option<RestoreProgressCallback>,
) -> Result<RestoreResult> {
    let logs = list_logs()?;

    if logs.is_empty() {
        return Err(anyhow::anyhow!(
            "No deletion history found. Nothing to restore."
        ));
    }

    // Get the most recent log
    let latest_log = load_log(&logs[0])?;
    restore_from_log_with_progress(&latest_log, output_mode, progress_callback)
}

/// Normalize a path for comparison (handles case-insensitive matching on Windows)
pub fn normalize_path_for_comparison(path: &str) -> String {
    // On Windows, paths are case-insensitive, so we normalize to lowercase
    // Also normalize separators and remove trailing separators
    #[cfg(windows)]
    {
        path.replace('\\', "/").to_lowercase()
    }
    #[cfg(not(windows))]
    {
        path.to_string()
    }
}

/// Check if a path is in a temporary directory
fn is_temp_directory(path: &Path) -> bool {
    // Normalize path for comparison
    let path_normalized = normalize_path_for_comparison(&path.display().to_string());

    // Check %TEMP%
    if let Ok(temp_dir) = env::var("TEMP") {
        let temp_normalized = normalize_path_for_comparison(&temp_dir);
        if path_normalized.starts_with(&temp_normalized) {
            return true;
        }
    }

    // Check %LOCALAPPDATA%\Temp
    if let Ok(local_appdata) = env::var("LOCALAPPDATA") {
        let local_temp = PathBuf::from(&local_appdata).join("Temp");
        let local_temp_normalized =
            normalize_path_for_comparison(&local_temp.display().to_string());
        if path_normalized.starts_with(&local_temp_normalized) {
            return true;
        }
    }

    // Check for common temp directory patterns
    path_normalized.contains("/temp/") || path_normalized.contains("/tmp/")
}

/// Restore files from a specific deletion log
pub fn restore_from_log(
    log: &DeletionLog,
    output_mode: crate::output::OutputMode,
) -> Result<RestoreResult> {
    restore_from_log_with_progress(log, output_mode, None)
}

/// Restore files from a specific deletion log with progress callback
/// Uses bulk restore operations for better performance on Windows
pub fn restore_from_log_with_progress(
    log: &DeletionLog,
    output_mode: crate::output::OutputMode,
    mut progress_callback: Option<RestoreProgressCallback>,
) -> Result<RestoreResult> {
    let mut result = RestoreResult::default();

    // Get current Recycle Bin contents
    let recycle_bin_items = os_limited::list().context("Failed to list Recycle Bin contents")?;

    // Count total items to restore
    let total_to_restore = log
        .records
        .iter()
        .filter(|r| r.success && !r.permanent)
        .count();

    // Create a map of Recycle Bin items by original path
    // Windows Recycle Bin stores files with their original paths in metadata
    // Use normalized paths for better matching
    let mut bin_map: HashMap<String, trash::TrashItem> = HashMap::new();
    for item in &recycle_bin_items {
        // Try to match by original parent + name
        let original_path = item.original_parent.join(&item.name);
        let normalized = normalize_path_for_comparison(&original_path.display().to_string());
        bin_map.insert(normalized, item.clone());
    }

    // Collect all items to restore (for bulk operation)
    // Structure: (record, trash_item, size_bytes)
    let mut items_to_restore: Vec<(&DeletionRecord, trash::TrashItem, u64)> = Vec::new();
    let mut record_to_items: HashMap<String, Vec<(&DeletionRecord, trash::TrashItem, u64)>> =
        HashMap::new();

    // First pass: collect all items that need to be restored
    for record in &log.records {
        if !record.success || record.permanent {
            // Skip failed deletions and permanent deletions (can't restore those)
            continue;
        }

        let normalized_record_path = normalize_path_for_comparison(&record.path);

        // Try to find exact match first (for files)
        if let Some(trash_item) = bin_map.get(&normalized_record_path) {
            items_to_restore.push((record, trash_item.clone(), record.size_bytes));
            record_to_items
                .entry(record.path.clone())
                .or_default()
                .push((record, trash_item.clone(), record.size_bytes));
        } else {
            // No exact match - check if this was a directory
            // When a directory is deleted, Windows Recycle Bin stores individual files,
            // not the directory itself. So we need to find all items whose path starts
            // with the directory path.
            let normalized_record_path_with_sep = if normalized_record_path.ends_with('/') {
                normalized_record_path.clone()
            } else {
                format!("{}/", normalized_record_path)
            };

            // Find all Recycle Bin items that are children of this directory
            let mut found_any = false;
            for (bin_path, trash_item) in &bin_map {
                // Check if this Recycle Bin item is inside the directory we're restoring
                if bin_path.starts_with(&normalized_record_path_with_sep) {
                    found_any = true;
                    // For directory items, we don't have individual sizes, so use 0
                    // The total will be tracked via record.size_bytes
                    items_to_restore.push((record, trash_item.clone(), 0));
                    record_to_items
                        .entry(record.path.clone())
                        .or_default()
                        .push((record, trash_item.clone(), 0));
                }
            }

            if !found_any {
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
    }

    if items_to_restore.is_empty() {
        // Final progress update
        if let Some(ref mut callback) = progress_callback {
            callback(
                None,
                result.restored,
                total_to_restore,
                result.errors,
                result.not_found,
            )?;
        }
        return Ok(result);
    }

    // Restore in batches for better performance
    // Windows can be slow with many individual restore operations
    const BATCH_SIZE: usize = 100;

    // Inform user about bulk restore operation
    let spinner = if output_mode != crate::output::OutputMode::Quiet {
        Some(crate::progress::create_spinner(&format!(
            "Restoring {} items in bulk (batches of {})...",
            items_to_restore.len(),
            BATCH_SIZE
        )))
    } else {
        None
    };

    // Create all parent directories before bulk restore
    let mut parent_dirs: HashSet<PathBuf> = HashSet::new();
    for (_, trash_item, _) in &items_to_restore {
        let dest = trash_item.original_parent.join(&trash_item.name);
        if let Some(parent) = dest.parent() {
            parent_dirs.insert(PathBuf::from(parent));
        }
    }

    for parent in &parent_dirs {
        if !parent.exists() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                if output_mode != crate::output::OutputMode::Quiet {
                    eprintln!(
                        "[WARNING] Failed to create parent directory {}: {}",
                        Theme::secondary(&parent.display().to_string()),
                        Theme::error(&e.to_string())
                    );
                }
            }
        }
    }
    let mut restored_records: HashSet<String> = HashSet::new();
    let mut processed_count = 0;

    let total_batches = items_to_restore.len().div_ceil(BATCH_SIZE);
    let mut batch_num = 0;

    for batch in items_to_restore.chunks(BATCH_SIZE) {
        batch_num += 1;
        let batch_items: Vec<trash::TrashItem> = batch
            .iter()
            .map(|(_record, item, _size): &(&DeletionRecord, trash::TrashItem, u64)| item.clone())
            .collect();

        // Show batch progress
        if output_mode != crate::output::OutputMode::Quiet
            && output_mode != crate::output::OutputMode::VeryVerbose
        {
            print!("\rProcessing batch {}/{}...", batch_num, total_batches);
            std::io::stdout().flush().ok();
        }

        // Update progress callback
        if let Some(ref mut callback) = progress_callback {
            if let Some((record, _, _)) = batch.first() {
                callback(
                    Some(Path::new(&record.path)),
                    processed_count,
                    total_to_restore,
                    result.errors,
                    result.not_found,
                )?;
            }
        }

        // Try bulk restore
        match os_limited::restore_all(batch_items.iter().cloned()) {
            Ok(()) => {
                // Bulk restore succeeded - mark all records as restored
                // Track by record path to avoid double-counting directories
                for (record, _, _) in batch {
                    if !restored_records.contains(&record.path) {
                        restored_records.insert(record.path.clone());
                        result.restored += 1;
                        result.restored_bytes += record.size_bytes;
                    }
                }
                processed_count += batch.len();
            }
            Err(_e) => {
                // Bulk restore failed - fall back to individual restore
                for (record, trash_item, _size_bytes) in batch {
                    let dest = trash_item.original_parent.join(&trash_item.name);

                    // Skip if destination already exists (may have been restored in partial batch success)
                    if dest.exists() {
                        // Count as restored if it exists (likely from partial batch success)
                        if !restored_records.contains(&record.path) {
                            restored_records.insert(record.path.clone());
                            result.restored += 1;
                            result.restored_bytes += record.size_bytes;
                        }
                        processed_count += 1;
                        if output_mode == crate::output::OutputMode::VeryVerbose {
                            println!(
                                "{} Already exists (skipped): {}",
                                Theme::muted("?"),
                                Theme::secondary(&dest.display().to_string())
                            );
                        }
                        continue;
                    }

                    match restore_file(trash_item) {
                        Ok(()) => {
                            if !restored_records.contains(&record.path) {
                                restored_records.insert(record.path.clone());
                                result.restored += 1;
                                result.restored_bytes += record.size_bytes;
                            }
                            processed_count += 1;
                        }
                        Err(err) => {
                            result.errors += 1;
                            if output_mode != crate::output::OutputMode::Quiet {
                                eprintln!(
                                    "{} Failed to restore {}: {}",
                                    Theme::error("✗"),
                                    Theme::secondary(&record.path),
                                    Theme::error(&err.to_string())
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    // Clear spinner
    if let Some(sp) = spinner {
        crate::progress::finish_and_clear(&sp);
    }

    // Clear batch progress line
    if output_mode != crate::output::OutputMode::Quiet
        && output_mode != crate::output::OutputMode::VeryVerbose
    {
        print!("\r{}", " ".repeat(50)); // Clear the line
        print!("\r");
        std::io::stdout().flush().ok();
    }

    // Print summary for each restored record
    if output_mode != crate::output::OutputMode::Quiet {
        for (record_path, items) in &record_to_items {
            if restored_records.contains(record_path) {
                let item_count: usize = items.len();
                if item_count == 1 {
                    println!(
                        "{} Restored: {}",
                        Theme::success("✓"),
                        Theme::secondary(record_path)
                    );
                } else {
                    println!(
                        "{} Restored directory: {} ({} items)",
                        Theme::success("✓"),
                        Theme::secondary(record_path),
                        item_count
                    );
                }
            }
        }
    }

    // Final progress update
    if let Some(ref mut callback) = progress_callback {
        callback(
            None,
            result.restored,
            total_to_restore,
            result.errors,
            result.not_found,
        )?;
    }

    Ok(result)
}

/// Restore a specific file by path
pub fn restore_path(path: &Path, output_mode: crate::output::OutputMode) -> Result<RestoreResult> {
    let mut result = RestoreResult::default();

    // Get current Recycle Bin contents
    let recycle_bin_items = os_limited::list().context("Failed to list Recycle Bin contents")?;

    let normalized_path = normalize_path_for_comparison(&path.display().to_string());
    let normalized_path_with_sep = if normalized_path.ends_with('/') {
        normalized_path.clone()
    } else {
        format!("{}/", normalized_path)
    };

    // First try exact match (for files)
    for item in &recycle_bin_items {
        let original_path = item.original_parent.join(&item.name);
        let normalized_original =
            normalize_path_for_comparison(&original_path.display().to_string());

        if normalized_original == normalized_path {
            let restored_path = item.original_parent.join(&item.name);
            match restore_file(item) {
                Ok(()) => {
                    result.restored = 1;
                    // Get file size from restored file
                    result.restored_bytes = std::fs::metadata(&restored_path)
                        .map(|m| m.len())
                        .unwrap_or(0);
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

    // No exact match - check if this is a directory
    // Find all Recycle Bin items that are children of this directory
    let mut found_any = false;
    let mut restored_count = 0;
    let mut restored_bytes = 0u64;
    let mut restore_errors = Vec::new();

    for item in &recycle_bin_items {
        let original_path = item.original_parent.join(&item.name);
        let normalized_original =
            normalize_path_for_comparison(&original_path.display().to_string());

        // Check if this Recycle Bin item is inside the directory we're restoring
        if normalized_original.starts_with(&normalized_path_with_sep) {
            found_any = true;
            let restored_path = item.original_parent.join(&item.name);
            match restore_file(item) {
                Ok(()) => {
                    restored_count += 1;
                    // Get file size from restored file
                    restored_bytes += std::fs::metadata(&restored_path)
                        .map(|m| m.len())
                        .unwrap_or(0);
                }
                Err(e) => {
                    restore_errors.push((original_path.clone(), e));
                }
            }
        }
    }

    if found_any {
        if restored_count > 0 {
            result.restored = 1; // Count as one directory restored
            result.restored_bytes = restored_bytes;
            if output_mode != crate::output::OutputMode::Quiet {
                println!(
                    "{} Restored directory: {} ({} items)",
                    Theme::success("✓"),
                    Theme::secondary(&path.display().to_string()),
                    restored_count
                );
            }
        }

        // Report errors if any
        if !restore_errors.is_empty() {
            result.errors = restore_errors.len();
            if output_mode != crate::output::OutputMode::Quiet {
                for (error_path, error) in &restore_errors {
                    eprintln!(
                        "{} Failed to restore {}: {}",
                        Theme::error("✗"),
                        Theme::secondary(&error_path.display().to_string()),
                        Theme::error(&error.to_string())
                    );
                }
            }
        }

        Ok(result)
    } else {
        Err(anyhow::anyhow!(
            "File or directory not found in Recycle Bin: {}",
            path.display()
        ))
    }
}

/// Restore all contents of the Recycle Bin in bulk (much faster on Windows)
pub fn restore_all_bin(
    output_mode: crate::output::OutputMode,
    mut progress_callback: Option<RestoreProgressCallback>,
) -> Result<RestoreResult> {
    let mut result = RestoreResult::default();

    // Get current Recycle Bin contents
    let recycle_bin_items = os_limited::list().context("Failed to list Recycle Bin contents")?;

    if recycle_bin_items.is_empty() {
        if output_mode != crate::output::OutputMode::Quiet {
            println!(
                "{}",
                Theme::muted("Recycle Bin is empty. Nothing to restore.")
            );
        }
        return Ok(result);
    }

    let total_items = recycle_bin_items.len();

    // Restore in batches for better performance
    // Windows can be slow with many individual restore operations
    const BATCH_SIZE: usize = 100;

    // Inform user about bulk restore operation
    let spinner = if output_mode != crate::output::OutputMode::Quiet {
        Some(crate::progress::create_spinner(&format!(
            "Restoring {} items from Recycle Bin in bulk (batches of {})...",
            total_items, BATCH_SIZE
        )))
    } else {
        None
    };

    // Create all parent directories before bulk restore
    let mut parent_dirs: HashSet<PathBuf> = HashSet::new();
    for item in &recycle_bin_items {
        let dest = item.original_parent.join(&item.name);
        if let Some(parent) = dest.parent() {
            parent_dirs.insert(PathBuf::from(parent));
        }
    }

    for parent in &parent_dirs {
        if !parent.exists() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                if output_mode != crate::output::OutputMode::Quiet {
                    eprintln!(
                        "[WARNING] Failed to create parent directory {}: {}",
                        Theme::secondary(&parent.display().to_string()),
                        Theme::error(&e.to_string())
                    );
                }
            }
        }
    }
    let mut processed_count = 0;
    let total_batches = total_items.div_ceil(BATCH_SIZE);
    let mut batch_num = 0;

    for batch in recycle_bin_items.chunks(BATCH_SIZE) {
        batch_num += 1;

        // Show batch progress
        if output_mode != crate::output::OutputMode::Quiet
            && output_mode != crate::output::OutputMode::VeryVerbose
        {
            print!("\rProcessing batch {}/{}...", batch_num, total_batches);
            std::io::stdout().flush().ok();
        }

        // Update progress callback
        if let Some(ref mut callback) = progress_callback {
            if let Some(item) = batch.first() {
                let path = item.original_parent.join(&item.name);
                callback(
                    Some(&path),
                    processed_count,
                    total_items,
                    result.errors,
                    result.not_found,
                )?;
            }
        }

        // Try bulk restore
        match os_limited::restore_all(batch.iter().cloned()) {
            Ok(()) => {
                // Bulk restore succeeded
                for item in batch {
                    result.restored += 1;
                    // Try to get size from restored file
                    let restored_path = item.original_parent.join(&item.name);
                    if let Ok(metadata) = std::fs::metadata(&restored_path) {
                        result.restored_bytes += metadata.len();
                    }
                }
                processed_count += batch.len();
            }
            Err(_e) => {
                // Bulk restore failed - fall back to individual restore
                for item in batch {
                    let dest = item.original_parent.join(&item.name);

                    // Skip if destination already exists (may have been restored in partial batch success)
                    if dest.exists() {
                        // Count as restored if it exists (likely from partial batch success)
                        result.restored += 1;
                        if let Ok(metadata) = std::fs::metadata(&dest) {
                            result.restored_bytes += metadata.len();
                        }
                        processed_count += 1;
                        if output_mode == crate::output::OutputMode::VeryVerbose {
                            println!(
                                "{} Already exists (skipped): {}",
                                Theme::muted("?"),
                                Theme::secondary(&dest.display().to_string())
                            );
                        }
                        continue;
                    }

                    match restore_file(item) {
                        Ok(()) => {
                            result.restored += 1;
                            // Get file size from restored file
                            if let Ok(metadata) = std::fs::metadata(&dest) {
                                result.restored_bytes += metadata.len();
                            }
                            processed_count += 1;
                        }
                        Err(err) => {
                            result.errors += 1;
                            if output_mode != crate::output::OutputMode::Quiet {
                                eprintln!(
                                    "{} Failed to restore {}: {}",
                                    Theme::error("✗"),
                                    Theme::secondary(
                                        &item
                                            .original_parent
                                            .join(&item.name)
                                            .display()
                                            .to_string()
                                    ),
                                    Theme::error(&err.to_string())
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    // Clear spinner
    if let Some(sp) = spinner {
        crate::progress::finish_and_clear(&sp);
    }

    // Final progress update
    if let Some(ref mut callback) = progress_callback {
        callback(
            None,
            result.restored,
            total_items,
            result.errors,
            result.not_found,
        )?;
    }

    Ok(result)
}

/// Restore a single file from Recycle Bin
pub fn restore_file(item: &trash::TrashItem) -> Result<()> {
    let dest = item.original_parent.join(&item.name);

    // Check if parent directory exists and is accessible
    if let Some(parent) = dest.parent() {
        if !parent.exists() {
            // Try to create the parent directory
            match std::fs::create_dir_all(parent) {
                Ok(()) => {
                    // Verify parent directory was actually created
                    if !parent.exists() {
                        return Err(anyhow::anyhow!(
                            "Parent directory does not exist and could not be created: {}",
                            parent.display()
                        ));
                    }
                }
                Err(e) => {
                    return Err(anyhow::anyhow!(
                        "Failed to create parent directory {}: {}",
                        parent.display(),
                        e
                    ));
                }
            }
        } else {
            // Parent exists, but verify it's actually a directory and we have write access
            match std::fs::metadata(parent) {
                Ok(metadata) => {
                    if !metadata.is_dir() {
                        return Err(anyhow::anyhow!(
                            "Parent path exists but is not a directory: {}",
                            parent.display()
                        ));
                    }
                }
                Err(e) => {
                    return Err(anyhow::anyhow!(
                        "Cannot access parent directory {}: {}",
                        parent.display(),
                        e
                    ));
                }
            }
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
    // Capture the actual error from the trash crate to provide better diagnostics
    match trash::os_limited::restore_all(std::iter::once(item.clone())) {
        Ok(()) => Ok(()),
        Err(e) => {
            // Provide more detailed error information
            let error_msg = format!("{}", e);

            // Check if this is a temp directory and provide helpful context
            if is_temp_directory(&dest) {
                // Check if the error is the Windows Recycle Bin error code
                if error_msg.contains("0x80270022") || error_msg.contains("-2144927710") {
                    return Err(anyhow::anyhow!(
                        "Cannot restore to temp directory (likely cleaned up): {}\n\
                        The original temp directory may have been deleted by Windows.\n\
                        Temp files are typically safe to leave in the Recycle Bin.",
                        dest.display()
                    ));
                }
            }

            Err(anyhow::anyhow!(
                "Failed to restore file to {}: {}",
                dest.display(),
                error_msg
            ))
        }
    }
}

/// Result of a restore operation
#[derive(Debug, Default)]
pub struct RestoreResult {
    pub restored: usize,
    pub restored_bytes: u64,
    pub errors: usize,
    pub not_found: usize,
    pub error_reasons: Vec<String>, // Store error messages for display
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
        assert_eq!(result.error_reasons.len(), 0);
    }

    #[test]
    fn test_restore_result_summary() {
        let result = RestoreResult {
            restored: 5,
            restored_bytes: 1024 * 1024, // 1 MiB
            errors: 1,
            not_found: 2,
            error_reasons: vec![],
        };

        let summary = result.summary();
        eprintln!("Actual summary: '{}'", summary);

        // Check that all expected values are present
        assert!(
            summary.contains("5"),
            "Summary should contain '5': {}",
            summary
        );
        // bytesize::to_string with binary_units=true may format as "1.0 MiB", "1 MiB", or similar
        // Check for the unit and that size representation is present
        assert!(
            summary.contains("MiB") || summary.contains("MB"),
            "Summary should contain size unit (MiB or MB): {}",
            summary
        );
        assert!(
            summary.contains("1"),
            "Summary should contain '1': {}",
            summary
        );
        assert!(
            summary.contains("2"),
            "Summary should contain '2': {}",
            summary
        );
    }
}
