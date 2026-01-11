//! Restart Bluetooth service operation.

use super::super::admin_check::is_admin;
use super::super::result::OptimizeResult;
use std::process::Command;

/// Restart the Bluetooth Support Service
pub fn restart_bluetooth_service(dry_run: bool) -> OptimizeResult {
    let action = "Restart Bluetooth Service";

    if dry_run {
        return OptimizeResult::skipped(
            action,
            "Dry run mode - would restart bthserv service",
            true,
        );
    }

    if !is_admin() {
        return OptimizeResult::failure(action, "Administrator privileges required", true);
    }

    // Stop the service
    let stop_result = Command::new("net").args(["stop", "bthserv"]).output();

    // Small delay
    std::thread::sleep(std::time::Duration::from_millis(500));

    // Start the service
    let start_result = Command::new("net").args(["start", "bthserv"]).output();

    match (stop_result, start_result) {
        (Ok(stop), Ok(start)) => {
            if start.status.success() {
                OptimizeResult::success(action, "Bluetooth service restarted successfully", true)
            } else if stop.status.success() {
                OptimizeResult::failure(action, "Stopped service but failed to restart", true)
            } else {
                // Service might not exist or is disabled
                let stderr = String::from_utf8_lossy(&stop.stderr);
                if stderr.contains("is not started") || stderr.contains("could not be found") {
                    OptimizeResult::skipped(action, "Bluetooth service not available", true)
                } else {
                    OptimizeResult::failure(action, "Failed to restart Bluetooth service", true)
                }
            }
        }
        _ => OptimizeResult::failure(action, "Failed to execute service commands", true),
    }
}
