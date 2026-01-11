//! Restart font cache service operation.

use super::super::admin_check::is_admin;
use super::super::result::OptimizeResult;
use std::process::Command;

/// Restart the Windows Font Cache Service
pub fn restart_font_cache_service(dry_run: bool) -> OptimizeResult {
    let action = "Restart Font Cache Service";

    if dry_run {
        return OptimizeResult::skipped(
            action,
            "Dry run mode - would restart FontCache service",
            true,
        );
    }

    if !is_admin() {
        return OptimizeResult::failure(action, "Administrator privileges required", true);
    }

    // Stop the service
    let stop_result = Command::new("net").args(["stop", "FontCache"]).output();

    // Small delay
    std::thread::sleep(std::time::Duration::from_millis(500));

    // Start the service
    let start_result = Command::new("net").args(["start", "FontCache"]).output();

    match (stop_result, start_result) {
        (Ok(stop), Ok(start)) => {
            if start.status.success() {
                OptimizeResult::success(action, "Font cache service restarted successfully", true)
            } else if stop.status.success() {
                OptimizeResult::failure(action, "Stopped service but failed to restart", true)
            } else {
                OptimizeResult::failure(action, "Failed to restart font cache service", true)
            }
        }
        _ => OptimizeResult::failure(action, "Failed to execute service commands", true),
    }
}
