//! Reset network stack operation.

use super::super::admin_check::is_admin;
use super::super::result::OptimizeResult;
use std::process::Command;

/// Reset network stack (Winsock and IP configuration)
pub fn reset_network_stack(dry_run: bool) -> OptimizeResult {
    let action = "Reset Network Stack";

    if dry_run {
        return OptimizeResult::skipped(
            action,
            "Dry run mode - would run: netsh winsock reset, netsh int ip reset",
            true,
        );
    }

    if !is_admin() {
        return OptimizeResult::failure(action, "Administrator privileges required", true);
    }

    // Reset Winsock
    let winsock_result = Command::new("netsh").args(["winsock", "reset"]).output();

    // Reset IP configuration
    let ip_result = Command::new("netsh").args(["int", "ip", "reset"]).output();

    match (winsock_result, ip_result) {
        (Ok(ws), Ok(ip)) => {
            let ws_ok = ws.status.success();
            let ip_ok = ip.status.success();

            if ws_ok && ip_ok {
                OptimizeResult::success(
                    action,
                    "Network stack reset successfully (restart required to take effect)",
                    true,
                )
            } else if ws_ok {
                OptimizeResult::success(action, "Winsock reset, but IP reset failed", true)
            } else if ip_ok {
                OptimizeResult::success(action, "IP reset, but Winsock reset failed", true)
            } else {
                OptimizeResult::failure(action, "Failed to reset network stack", true)
            }
        }
        _ => OptimizeResult::failure(action, "Failed to execute netsh commands", true),
    }
}
