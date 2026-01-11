//! Restart Explorer operation.

use super::super::result::OptimizeResult;
use std::process::{Command, Stdio};

/// Restart Windows Explorer (equivalent to Dock refresh on macOS)
pub fn restart_explorer(dry_run: bool) -> OptimizeResult {
    let action = "Restart Explorer";

    if dry_run {
        return OptimizeResult::skipped(action, "Dry run mode - would restart explorer.exe", false);
    }

    do_restart_explorer()
}

/// Internal function to restart Explorer
pub(crate) fn do_restart_explorer() -> OptimizeResult {
    let action = "Restart Explorer";

    // Kill explorer gracefully - redirect output to prevent TUI corruption
    let kill_result = Command::new("taskkill")
        .args(["/F", "/IM", "explorer.exe"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();

    // Small delay to let it fully close
    std::thread::sleep(std::time::Duration::from_millis(500));

    // Start explorer again - redirect output to prevent TUI corruption
    let start_result = Command::new("cmd")
        .args(["/C", "start", "explorer.exe"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();

    match (kill_result, start_result) {
        (Ok(_), Ok(_)) => {
            // Both commands spawned successfully
            // Give Explorer a moment to start
            std::thread::sleep(std::time::Duration::from_millis(500));
            OptimizeResult::success(action, "Explorer restarted successfully", false)
        }
        (Ok(_), Err(e)) => OptimizeResult::failure(
            action,
            &format!("Killed Explorer but failed to restart: {}", e),
            false,
        ),
        (Err(e), _) => {
            OptimizeResult::failure(action, &format!("Failed to kill Explorer: {}", e), false)
        }
    }
}
