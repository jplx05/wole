//! Flush DNS cache operation.

use super::super::result::OptimizeResult;
use std::process::Command;

/// Flush DNS cache using ipconfig /flushdns
pub fn flush_dns_cache(dry_run: bool) -> OptimizeResult {
    let action = "Flush DNS Cache";

    if dry_run {
        return OptimizeResult::skipped(
            action,
            "Dry run mode - would run: ipconfig /flushdns",
            false,
        );
    }

    match Command::new("ipconfig").arg("/flushdns").output() {
        Ok(output) => {
            if output.status.success() {
                OptimizeResult::success(action, "DNS cache flushed successfully", false)
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                OptimizeResult::failure(
                    action,
                    &format!("Failed to flush DNS cache: {}", stderr),
                    false,
                )
            }
        }
        Err(e) => {
            OptimizeResult::failure(action, &format!("Failed to execute ipconfig: {}", e), false)
        }
    }
}
