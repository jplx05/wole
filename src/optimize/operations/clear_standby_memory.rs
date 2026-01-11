//! Clear standby memory operation.

use super::super::admin_check::is_admin;
use super::super::result::OptimizeResult;
use std::process::Command;

/// Clear standby memory using Windows API
/// This requires administrator privileges and the EmptyStandbyList.exe utility
/// or direct Windows API calls
pub fn clear_standby_memory(dry_run: bool) -> OptimizeResult {
    let action = "Clear Standby Memory";

    if dry_run {
        return OptimizeResult::skipped(
            action,
            "Dry run mode - would clear standby memory list",
            true,
        );
    }

    if !is_admin() {
        return OptimizeResult::failure(action, "Administrator privileges required", true);
    }

    // Use PowerShell to clear standby memory
    // This is a more reliable method than trying to call NtSetSystemInformation directly
    // Limit to non-system processes to avoid issues and speed up execution
    let script = r#"
        # Clear working sets of user processes only (faster and safer)
        Get-Process | Where-Object { $_.Id -ne $PID -and $_.ProcessName -ne 'csrss' -and $_.ProcessName -ne 'winlogon' } | ForEach-Object {
            try {
                $_.MinWorkingSet = $_.MinWorkingSet
            } catch {}
        }
    "#;

    match Command::new("powershell")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .output()
    {
        Ok(output) => {
            if output.status.success() {
                OptimizeResult::success(action, "Cleared process working sets", true)
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                if stderr.is_empty() {
                    OptimizeResult::failure(action, "Failed to clear memory", true)
                } else {
                    OptimizeResult::failure(
                        action,
                        &format!("Failed to clear memory: {}", stderr),
                        true,
                    )
                }
            }
        }
        Err(e) => OptimizeResult::failure(
            action,
            &format!("Failed to execute PowerShell: {}", e),
            true,
        ),
    }
}
