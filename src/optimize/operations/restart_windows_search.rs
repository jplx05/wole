//! Restart Windows Search operation.

use super::super::admin_check::is_admin;
use super::super::result::OptimizeResult;
use std::process::Command;

/// Restart the Windows Search service (equivalent to Spotlight rebuild on macOS)
pub fn restart_windows_search(dry_run: bool) -> OptimizeResult {
    let action = "Restart Windows Search";

    if dry_run {
        return OptimizeResult::skipped(
            action,
            "Dry run mode - would restart WSearch service",
            true,
        );
    }

    if !is_admin() {
        return OptimizeResult::failure(action, "Administrator privileges required", true);
    }

    // Stop the service
    let stop_result = Command::new("net").args(["stop", "WSearch"]).output();

    // Small delay
    std::thread::sleep(std::time::Duration::from_millis(1000));

    // Start the service
    let start_result = Command::new("net").args(["start", "WSearch"]).output();

    match (stop_result, start_result) {
        (Ok(stop), Ok(start)) => {
            if start.status.success() {
                OptimizeResult::success(
                    action,
                    "Windows Search service restarted successfully",
                    true,
                )
            } else if stop.status.success() {
                OptimizeResult::failure(action, "Stopped service but failed to restart", true)
            } else {
                OptimizeResult::failure(action, "Failed to restart Windows Search service", true)
            }
        }
        _ => OptimizeResult::failure(action, "Failed to execute service commands", true),
    }
}
