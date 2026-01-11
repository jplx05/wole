use crate::config::Config;
use crate::output::CategoryResult;
use crate::utils;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Scan for Windows Update files that can be cleaned
///
/// Includes:
/// - Windows Update download cache (C:\Windows\SoftwareDistribution\Download)
/// - Windows Update logs (C:\Windows\Logs\WindowsUpdate)
/// - Component Store (WinSxS) - scan only, requires DISM for cleanup
pub fn scan(_root: &Path, config: &Config) -> Result<CategoryResult> {
    let mut result = CategoryResult::default();
    let mut paths = Vec::new();

    // Get Windows directory
    let windows_dir = std::env::var("SystemRoot")
        .ok()
        .map(PathBuf::from)
        .or_else(|| Some(PathBuf::from("C:\\Windows")));

    if let Some(ref windows_path) = windows_dir {
        // 1. Windows Update download cache
        let update_download_path = windows_path.join("SoftwareDistribution").join("Download");
        if update_download_path.exists() && !config.is_excluded(&update_download_path) {
            match utils::calculate_dir_size(&update_download_path) {
                size if size > 0 => {
                    result.items += 1;
                    result.size_bytes += size;
                    paths.push(update_download_path);
                }
                _ => {}
            }
        }

        // 2. Windows Update logs
        let update_logs_path = windows_path.join("Logs").join("WindowsUpdate");
        if update_logs_path.exists() && !config.is_excluded(&update_logs_path) {
            match utils::calculate_dir_size(&update_logs_path) {
                size if size > 0 => {
                    result.items += 1;
                    result.size_bytes += size;
                    paths.push(update_logs_path);
                }
                _ => {}
            }
        }

        // 3. Component Store (WinSxS) - scan only, show size but note it requires DISM
        // We scan it but don't add to paths since it requires special handling
        let winsxs_path = windows_path.join("WinSxS");
        if winsxs_path.exists() && !config.is_excluded(&winsxs_path) {
            // Only scan if we can access it (may require admin)
            match utils::calculate_dir_size(&winsxs_path) {
                size if size > 0 => {
                    // Note: We don't add this to paths because cleanup requires DISM
                    // But we can show it in the scan results
                    // For now, we'll just note it exists but don't include it in cleanable paths
                }
                _ => {}
            }
        }
    }

    result.paths = paths;
    Ok(result)
}

/// Clean Windows Update files
///
/// Note: Some operations may require administrator privileges
pub fn clean(path: &Path) -> Result<()> {
    // CRITICAL SAFETY CHECK: Never allow deletion of system paths directly
    // Windows Update files require special handling
    if crate::utils::is_system_path(path) {
        // For Windows Update cleanup, we need to use Windows commands
        // Check if this is the SoftwareDistribution\Download folder
        let path_str = path.to_string_lossy();
        if path_str.contains("SoftwareDistribution\\Download")
            || path_str.contains("SoftwareDistribution/Download")
        {
            // Use Windows Update service stop + cleanup + start
            return clean_windows_update_downloads();
        }

        if path_str.contains("Logs\\WindowsUpdate") || path_str.contains("Logs/WindowsUpdate") {
            // Clean update logs - safer to delete directly
            return clean_update_logs(path);
        }

        // For other system paths, skip gracefully
        return Ok(());
    }

    // Non-system paths can be deleted normally
    if !path.exists() {
        return Ok(());
    }

    crate::trash_ops::delete(path)
        .with_context(|| format!("Failed to delete Windows Update files: {}", path.display()))?;
    Ok(())
}

/// Clean Windows Update download cache using Windows Update service
fn clean_windows_update_downloads() -> Result<()> {
    // Stop Windows Update service
    let stop_result = Command::new("net")
        .args(["stop", "wuauserv"])
        .output()
        .with_context(|| "Failed to stop Windows Update service (may require admin)")?;

    if !stop_result.status.success() {
        let stderr = String::from_utf8_lossy(&stop_result.stderr);
        // Service might already be stopped, which is fine
        if !stderr.contains("is not started") {
            return Err(anyhow::anyhow!(
                "Failed to stop Windows Update service: {}",
                stderr
            ));
        }
    }

    // Small delay to ensure service stops
    std::thread::sleep(std::time::Duration::from_millis(1000));

    // Clean the download folder
    let windows_dir = std::env::var("SystemRoot")
        .ok()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("C:\\Windows"));
    let download_path = windows_dir.join("SoftwareDistribution").join("Download");

    if download_path.exists() {
        // Delete contents of Download folder
        if let Ok(entries) = std::fs::read_dir(&download_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let _ = utils::safe_remove_dir_all(&path);
                } else {
                    let _ = utils::safe_remove_file(&path);
                }
            }
        }
    }

    // Restart Windows Update service
    let _start_result = Command::new("net").args(["start", "wuauserv"]).output();

    Ok(())
}

/// Clean Windows Update logs
fn clean_update_logs(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }

    // Delete log files in the directory
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let entry_path = entry.path();
            if entry_path.is_file() {
                // Only delete .log and .etl files
                if let Some(ext) = entry_path.extension().and_then(|e| e.to_str()) {
                    if ext == "log" || ext == "etl" {
                        let _ = utils::safe_remove_file(&entry_path);
                    }
                }
            }
        }
    }

    Ok(())
}

/// Get Component Store (WinSxS) size using DISM
///
/// Returns the reclaimable size in bytes, or None if DISM is not available
pub fn get_winsxs_reclaimable_size() -> Option<u64> {
    // Run DISM to get component store size
    let output = Command::new("dism")
        .args(["/Online", "/Cleanup-Image", "/AnalyzeComponentStore"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse DISM output for "Component Store Cleanup Recommended" size
    // DISM output format: "Component Store Cleanup Recommended: X.XX GB"
    for line in stdout.lines() {
        if line.contains("Component Store Cleanup Recommended") {
            // Try to extract size
            if let Some(size_str) = line.split(':').nth(1) {
                let size_str = size_str.trim();
                // Parse size (e.g., "2.45 GB" or "1234.56 MB")
                if let Some(space_pos) = size_str.find(' ') {
                    let number_str = &size_str[..space_pos];
                    let unit = &size_str[space_pos + 1..];

                    if let Ok(number) = number_str.parse::<f64>() {
                        let bytes = match unit.to_uppercase().as_str() {
                            "GB" | "GB " => (number * 1024.0 * 1024.0 * 1024.0) as u64,
                            "MB" | "MB " => (number * 1024.0 * 1024.0) as u64,
                            "KB" | "KB " => (number * 1024.0) as u64,
                            "B" | "B " => number as u64,
                            _ => continue,
                        };
                        return Some(bytes);
                    }
                }
            }
        }
    }

    None
}

/// Clean Component Store (WinSxS) using DISM
///
/// Requires administrator privileges
pub fn clean_winsxs() -> Result<()> {
    // Run DISM cleanup
    let output = Command::new("dism")
        .args([
            "/Online",
            "/Cleanup-Image",
            "/StartComponentCleanup",
            "/ResetBase",
        ])
        .output()
        .with_context(|| "Failed to run DISM cleanup (requires admin)")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!("DISM cleanup failed: {}", stderr));
    }

    Ok(())
}
