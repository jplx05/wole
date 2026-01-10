use crate::config::Config;
use crate::output::CategoryResult;
use crate::utils;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Scan for Windows Event Log files that can be cleaned
///
/// Includes:
/// - Event log files (.evtx) older than 30 days from C:\Windows\System32\winevt\Logs
/// - Event log archive files
pub fn scan(_root: &Path, config: &Config) -> Result<CategoryResult> {
    let mut result = CategoryResult::default();
    let mut paths = Vec::new();

    // Get Windows directory
    let windows_dir = std::env::var("SystemRoot")
        .ok()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("C:\\Windows"));

    // Event logs are in System32\winevt\Logs
    let event_logs_path = windows_dir.join("System32").join("winevt").join("Logs");

    if !event_logs_path.exists() {
        return Ok(result);
    }

    // Get current time for age comparison
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let min_age_seconds = 30 * 24 * 60 * 60; // 30 days

    // Scan for .evtx files older than 30 days
    if let Ok(entries) = std::fs::read_dir(&event_logs_path) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();

            // Skip if excluded
            if config.is_excluded(&path) {
                continue;
            }

            // Only process .evtx files
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if ext.to_lowercase() != "evtx" {
                    continue;
                }
            } else {
                continue;
            }

            // Check file metadata
            if let Ok(metadata) = std::fs::metadata(&path) {
                if !metadata.is_file() {
                    continue;
                }

                // Check file age (use modified time)
                if let Ok(modified) = metadata.modified() {
                    if let Ok(modified_duration) = modified.duration_since(UNIX_EPOCH) {
                        let file_age = modified_duration.as_secs();
                        let age = now.saturating_sub(file_age);

                        // Only include files older than 30 days
                        if age >= min_age_seconds {
                            let size = metadata.len();
                            if size > 0 {
                                result.items += 1;
                                result.size_bytes += size;
                                paths.push(path);
                            }
                        }
                    }
                }
            }
        }
    }

    result.paths = paths;
    Ok(result)
}

/// Clean Windows Event Log files
///
/// Note: Requires administrator privileges for some operations
pub fn clean(path: &Path) -> Result<()> {
    // CRITICAL SAFETY CHECK: Never allow deletion of system paths directly
    // Event logs require special handling
    if crate::utils::is_system_path(path) {
        // For event logs, we can delete old .evtx files directly
        // but we should be careful not to delete active logs
        if !path.exists() {
            return Ok(());
        }

        // Only delete .evtx files (event log files)
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if ext.to_lowercase() != "evtx" {
                return Ok(());
            }
        } else {
            return Ok(());
        }

        // Check if file is older than 30 days (safety check)
        if let Ok(metadata) = std::fs::metadata(path) {
            if let Ok(modified) = metadata.modified() {
                let now = SystemTime::now();
                if let Ok(age) = now.duration_since(modified) {
                    // Only delete if older than 30 days
                    if age.as_secs() < 30 * 24 * 60 * 60 {
                        return Ok(());
                    }
                }
            }
        }

        // Delete the file (permanent delete for event logs - they're system files)
        utils::safe_remove_file(path)
            .with_context(|| format!("Failed to delete event log: {}", path.display()))?;
        return Ok(());
    }

    // Non-system paths can be deleted normally
    if !path.exists() {
        return Ok(());
    }

    trash::delete(path)
        .with_context(|| format!("Failed to delete event log: {}", path.display()))?;
    Ok(())
}

/// Clear all event logs using wevtutil (requires admin)
///
/// This clears the event logs but doesn't delete the files - Windows will recreate them
pub fn clear_event_logs() -> Result<()> {
    use std::process::Command;

    // List of common event logs to clear
    let event_logs = vec![
        "Application",
        "System",
        "Security",
        "Setup",
        "ForwardedEvents",
    ];

    let mut cleared = 0;
    let mut failed = 0;

    for log_name in event_logs {
        let output = Command::new("wevtutil")
            .args(["cl", log_name])
            .output()
            .with_context(|| format!("Failed to clear event log: {}", log_name))?;

        if output.status.success() {
            cleared += 1;
        } else {
            failed += 1;
            // Log might not exist or might require different permissions
            // Continue with other logs
        }
    }

    if failed > 0 && cleared == 0 {
        return Err(anyhow::anyhow!(
            "Failed to clear any event logs (may require admin privileges)"
        ));
    }

    Ok(())
}
