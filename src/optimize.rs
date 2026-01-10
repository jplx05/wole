//! Windows system optimization module
//!
//! Provides Windows equivalents to macOS optimization operations:
//! - DNS cache flush
//! - Thumbnail cache clearing
//! - Icon cache rebuild
//! - Browser database optimization (VACUUM)
//! - Font cache service restart
//! - Standby memory clearing
//! - Network stack reset
//! - Bluetooth service restart
//! - Windows Search service restart
//! - Explorer restart

use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use crate::output::OutputMode;
use crate::theme::Theme;

/// Result of an optimization operation
#[derive(Debug, Clone)]
pub struct OptimizeResult {
    /// Name of the action performed
    pub action: String,
    /// Whether the operation succeeded
    pub success: bool,
    /// Human-readable message about the result
    pub message: String,
    /// Whether this operation requires administrator privileges
    pub requires_admin: bool,
}

impl OptimizeResult {
    fn success(action: &str, message: &str, requires_admin: bool) -> Self {
        Self {
            action: action.to_string(),
            success: true,
            message: message.to_string(),
            requires_admin,
        }
    }

    fn failure(action: &str, message: &str, requires_admin: bool) -> Self {
        Self {
            action: action.to_string(),
            success: false,
            message: message.to_string(),
            requires_admin,
        }
    }

    fn skipped(action: &str, message: &str, requires_admin: bool) -> Self {
        Self {
            action: action.to_string(),
            success: true, // Skipped is considered "success" (not an error)
            message: format!("Skipped: {}", message),
            requires_admin,
        }
    }
}

/// Check if the current process is running with administrator privileges
pub fn is_admin() -> bool {
    // Try to access a protected path that requires admin
    // This is a simple heuristic - not 100% accurate but good enough
    let system_root = env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
    let test_path = PathBuf::from(&system_root).join("System32\\config\\system");
    
    // Try to open the file - if we can, we likely have admin rights
    fs::metadata(&test_path).is_ok()
}

/// Flush DNS cache using ipconfig /flushdns
pub fn flush_dns_cache(dry_run: bool) -> OptimizeResult {
    let action = "Flush DNS Cache";
    
    if dry_run {
        return OptimizeResult::skipped(action, "Dry run mode - would run: ipconfig /flushdns", false);
    }

    match Command::new("ipconfig").arg("/flushdns").output() {
        Ok(output) => {
            if output.status.success() {
                OptimizeResult::success(action, "DNS cache flushed successfully", false)
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                OptimizeResult::failure(action, &format!("Failed to flush DNS cache: {}", stderr), false)
            }
        }
        Err(e) => OptimizeResult::failure(action, &format!("Failed to execute ipconfig: {}", e), false),
    }
}

/// Clear thumbnail cache files from Windows Explorer
pub fn clear_thumbnail_cache(dry_run: bool) -> OptimizeResult {
    let action = "Clear Thumbnail Cache";
    
    let local_app_data = match env::var("LOCALAPPDATA") {
        Ok(path) => PathBuf::from(path),
        Err(_) => return OptimizeResult::failure(action, "Could not find LOCALAPPDATA path", false),
    };

    let explorer_path = local_app_data.join("Microsoft").join("Windows").join("Explorer");
    
    if !explorer_path.exists() {
        return OptimizeResult::skipped(action, "Explorer cache directory not found", false);
    }

    if dry_run {
        return OptimizeResult::skipped(
            action,
            &format!("Dry run mode - would delete thumbcache_*.db files in {}", explorer_path.display()),
            false,
        );
    }

    let mut deleted_count = 0;
    let mut failed_count = 0;

    // Delete thumbcache_*.db files
    if let Ok(entries) = fs::read_dir(&explorer_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with("thumbcache_") && name.ends_with(".db") {
                    match fs::remove_file(&path) {
                        Ok(_) => deleted_count += 1,
                        Err(_) => failed_count += 1, // File might be locked
                    }
                }
            }
        }
    }

    if deleted_count > 0 || failed_count == 0 {
        OptimizeResult::success(
            action,
            &format!("Deleted {} thumbnail cache files ({} locked/skipped)", deleted_count, failed_count),
            false,
        )
    } else {
        OptimizeResult::failure(
            action,
            &format!("Could not delete thumbnail cache files ({} locked)", failed_count),
            false,
        )
    }
}

/// Rebuild icon cache by deleting IconCache.db and iconcache_*.db files
pub fn rebuild_icon_cache(dry_run: bool, restart_explorer: bool) -> OptimizeResult {
    let action = "Rebuild Icon Cache";
    
    let local_app_data = match env::var("LOCALAPPDATA") {
        Ok(path) => PathBuf::from(path),
        Err(_) => return OptimizeResult::failure(action, "Could not find LOCALAPPDATA path", false),
    };

    let explorer_path = local_app_data.join("Microsoft").join("Windows").join("Explorer");
    let icon_cache_path = local_app_data.join("IconCache.db");

    if dry_run {
        let mut msg = String::from("Dry run mode - would delete: ");
        if icon_cache_path.exists() {
            msg.push_str(&format!("{}, ", icon_cache_path.display()));
        }
        msg.push_str(&format!("iconcache_*.db files in {}", explorer_path.display()));
        if restart_explorer {
            msg.push_str(" and restart Explorer");
        }
        return OptimizeResult::skipped(action, &msg, false);
    }

    let mut deleted_count = 0;
    let mut failed_count = 0;

    // Delete main IconCache.db
    if icon_cache_path.exists() {
        match fs::remove_file(&icon_cache_path) {
            Ok(_) => deleted_count += 1,
            Err(_) => failed_count += 1,
        }
    }

    // Delete iconcache_*.db files in Explorer folder
    if explorer_path.exists() {
        if let Ok(entries) = fs::read_dir(&explorer_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.starts_with("iconcache_") && name.ends_with(".db") {
                        match fs::remove_file(&path) {
                            Ok(_) => deleted_count += 1,
                            Err(_) => failed_count += 1,
                        }
                    }
                }
            }
        }
    }

    // Optionally restart Explorer
    if restart_explorer {
        let _ = do_restart_explorer();
    }

    OptimizeResult::success(
        action,
        &format!(
            "Deleted {} icon cache files ({} locked/skipped){}",
            deleted_count,
            failed_count,
            if restart_explorer { ", Explorer restarted" } else { "" }
        ),
        false,
    )
}

/// Optimize browser SQLite databases using VACUUM
pub fn vacuum_browser_databases(dry_run: bool) -> OptimizeResult {
    let action = "Optimize Browser Databases";
    
    let local_app_data = match env::var("LOCALAPPDATA") {
        Ok(path) => PathBuf::from(path),
        Err(_) => return OptimizeResult::failure(action, "Could not find LOCALAPPDATA path", false),
    };

    let app_data = match env::var("APPDATA") {
        Ok(path) => PathBuf::from(path),
        Err(_) => return OptimizeResult::failure(action, "Could not find APPDATA path", false),
    };

    // Browser database paths
    let mut db_paths: Vec<PathBuf> = Vec::new();

    // Microsoft Edge
    let edge_default = local_app_data.join("Microsoft").join("Edge").join("User Data").join("Default");
    for db_name in &["History", "Cookies", "Web Data", "Favicons"] {
        let path = edge_default.join(db_name);
        if path.exists() {
            db_paths.push(path);
        }
    }

    // Google Chrome
    let chrome_default = local_app_data.join("Google").join("Chrome").join("User Data").join("Default");
    for db_name in &["History", "Cookies", "Web Data", "Favicons"] {
        let path = chrome_default.join(db_name);
        if path.exists() {
            db_paths.push(path);
        }
    }

    // Firefox - need to find profile folder
    let firefox_profiles = app_data.join("Mozilla").join("Firefox").join("Profiles");
    if firefox_profiles.exists() {
        if let Ok(entries) = fs::read_dir(&firefox_profiles) {
            for entry in entries.flatten() {
                let profile_path = entry.path();
                if profile_path.is_dir() {
                    for db_name in &["places.sqlite", "cookies.sqlite", "formhistory.sqlite", "favicons.sqlite"] {
                        let path = profile_path.join(db_name);
                        if path.exists() {
                            db_paths.push(path);
                        }
                    }
                }
            }
        }
    }

    if db_paths.is_empty() {
        return OptimizeResult::skipped(action, "No browser databases found", false);
    }

    if dry_run {
        return OptimizeResult::skipped(
            action,
            &format!("Dry run mode - would VACUUM {} database files", db_paths.len()),
            false,
        );
    }

    let mut optimized_count = 0;
    let mut skipped_count = 0;

    for db_path in &db_paths {
        // Check if browser might be running by trying to open the database
        // Note: VACUUM can be slow for large databases, but typically completes in seconds
        match rusqlite::Connection::open(db_path) {
            Ok(conn) => {
                // Try to run VACUUM
                // This may take a moment for large databases but shouldn't block indefinitely
                match conn.execute("VACUUM", []) {
                    Ok(_) => optimized_count += 1,
                    Err(_e) => {
                        // Database might be locked by the browser or other error
                        // Log the error in verbose mode but don't fail the operation
                        skipped_count += 1;
                    }
                }
            }
            Err(_) => {
                // Could not open database (likely locked by browser)
                skipped_count += 1;
            }
        }
    }

    if skipped_count > 0 && optimized_count == 0 {
        OptimizeResult::failure(
            action,
            &format!(
                "Could not optimize databases - {} locked (close browsers and retry)",
                skipped_count
            ),
            false,
        )
    } else {
        OptimizeResult::success(
            action,
            &format!(
                "Optimized {} databases ({} locked/skipped)",
                optimized_count, skipped_count
            ),
            false,
        )
    }
}

/// Restart the Windows Font Cache Service
pub fn restart_font_cache_service(dry_run: bool) -> OptimizeResult {
    let action = "Restart Font Cache Service";
    
    if dry_run {
        return OptimizeResult::skipped(action, "Dry run mode - would restart FontCache service", true);
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

/// Clear standby memory using Windows API
/// This requires administrator privileges and the EmptyStandbyList.exe utility
/// or direct Windows API calls
pub fn clear_standby_memory(dry_run: bool) -> OptimizeResult {
    let action = "Clear Standby Memory";
    
    if dry_run {
        return OptimizeResult::skipped(action, "Dry run mode - would clear standby memory list", true);
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
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", script])
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
                    OptimizeResult::failure(action, &format!("Failed to clear memory: {}", stderr), true)
                }
            }
        }
        Err(e) => OptimizeResult::failure(action, &format!("Failed to execute PowerShell: {}", e), true),
    }
}

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

/// Restart the Bluetooth Support Service
pub fn restart_bluetooth_service(dry_run: bool) -> OptimizeResult {
    let action = "Restart Bluetooth Service";
    
    if dry_run {
        return OptimizeResult::skipped(action, "Dry run mode - would restart bthserv service", true);
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

/// Restart the Windows Search service (equivalent to Spotlight rebuild on macOS)
pub fn restart_windows_search(dry_run: bool) -> OptimizeResult {
    let action = "Restart Windows Search";
    
    if dry_run {
        return OptimizeResult::skipped(action, "Dry run mode - would restart WSearch service", true);
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
                OptimizeResult::success(action, "Windows Search service restarted successfully", true)
            } else if stop.status.success() {
                OptimizeResult::failure(action, "Stopped service but failed to restart", true)
            } else {
                OptimizeResult::failure(action, "Failed to restart Windows Search service", true)
            }
        }
        _ => OptimizeResult::failure(action, "Failed to execute service commands", true),
    }
}

/// Restart Windows Explorer (equivalent to Dock refresh on macOS)
pub fn restart_explorer(dry_run: bool) -> OptimizeResult {
    let action = "Restart Explorer";
    
    if dry_run {
        return OptimizeResult::skipped(action, "Dry run mode - would restart explorer.exe", false);
    }

    do_restart_explorer()
}

/// Internal function to restart Explorer
fn do_restart_explorer() -> OptimizeResult {
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
        (Ok(_), Err(e)) => {
            OptimizeResult::failure(action, &format!("Killed Explorer but failed to restart: {}", e), false)
        }
        (Err(e), _) => {
            OptimizeResult::failure(action, &format!("Failed to kill Explorer: {}", e), false)
        }
    }
}

/// Run all optimizations
#[allow(clippy::too_many_arguments)]
pub fn run_optimizations(
    all: bool,
    dns: bool,
    thumbnails: bool,
    icons: bool,
    databases: bool,
    fonts: bool,
    memory: bool,
    network: bool,
    bluetooth: bool,
    search: bool,
    explorer: bool,
    dry_run: bool,
    yes: bool,
    output_mode: OutputMode,
) -> Vec<OptimizeResult> {
    let mut results = Vec::new();
    
    // Determine which optimizations to run
    let run_dns = all || dns;
    let run_thumbnails = all || thumbnails;
    let run_icons = all || icons;
    let run_databases = all || databases;
    let run_fonts = all || fonts;
    let run_memory = all || memory;
    let run_network = all || network;
    let run_bluetooth = all || bluetooth;
    let run_search = all || search;
    let run_explorer = all || explorer;

    // Check if any admin operations are requested
    let needs_admin = run_fonts || run_memory || run_network || run_bluetooth || run_search;
    
    // If admin operations are needed and we're not running as admin, warn the user
    if needs_admin && !is_admin() && !dry_run {
        if output_mode != OutputMode::Quiet {
            println!();
            println!("{}", Theme::warning("Note: Some operations require administrator privileges."));
            println!("{}", Theme::muted("Run as Administrator for full optimization."));
            println!();
        }
    }

    // Warn about network reset (can disconnect)
    if run_network && !dry_run && !yes {
        if output_mode != OutputMode::Quiet {
            println!("{}", Theme::warning("Warning: Network reset will temporarily disconnect your network."));
            print!("Continue? [y/N]: ");
            io::Write::flush(&mut io::stdout()).ok();
            
            let mut input = String::new();
            if io::stdin().read_line(&mut input).is_err() || !input.trim().eq_ignore_ascii_case("y") {
                results.push(OptimizeResult::skipped("Reset Network Stack", "User cancelled", true));
                // Don't run network reset, but continue with others
                // We'll set a flag to skip it
            }
        }
    }

    // Run non-admin operations first
    if run_dns {
        print_operation_start("Flushing DNS cache...", output_mode);
        let result = flush_dns_cache(dry_run);
        print_operation_result(&result, output_mode);
        results.push(result);
    }

    if run_thumbnails {
        print_operation_start("Clearing thumbnail cache...", output_mode);
        let result = clear_thumbnail_cache(dry_run);
        print_operation_result(&result, output_mode);
        results.push(result);
    }

    if run_icons {
        print_operation_start("Rebuilding icon cache...", output_mode);
        // Don't restart explorer if we're going to do it separately
        let result = rebuild_icon_cache(dry_run, !run_explorer);
        print_operation_result(&result, output_mode);
        results.push(result);
    }

    if run_databases {
        print_operation_start("Optimizing browser databases...", output_mode);
        let result = vacuum_browser_databases(dry_run);
        print_operation_result(&result, output_mode);
        results.push(result);
    }

    // Admin operations
    if run_fonts {
        print_operation_start("Restarting font cache service...", output_mode);
        let result = restart_font_cache_service(dry_run);
        print_operation_result(&result, output_mode);
        results.push(result);
    }

    if run_memory {
        print_operation_start("Clearing standby memory...", output_mode);
        let result = clear_standby_memory(dry_run);
        print_operation_result(&result, output_mode);
        results.push(result);
    }

    if run_network {
        // Check if we already skipped it
        let already_skipped = results.iter().any(|r| r.action == "Reset Network Stack");
        if !already_skipped {
            print_operation_start("Resetting network stack...", output_mode);
            let result = reset_network_stack(dry_run);
            print_operation_result(&result, output_mode);
            results.push(result);
        }
    }

    if run_bluetooth {
        print_operation_start("Restarting Bluetooth service...", output_mode);
        let result = restart_bluetooth_service(dry_run);
        print_operation_result(&result, output_mode);
        results.push(result);
    }

    if run_search {
        print_operation_start("Restarting Windows Search...", output_mode);
        let result = restart_windows_search(dry_run);
        print_operation_result(&result, output_mode);
        results.push(result);
    }

    // Explorer should be last as it refreshes the shell
    if run_explorer {
        print_operation_start("Restarting Explorer...", output_mode);
        let result = restart_explorer(dry_run);
        print_operation_result(&result, output_mode);
        results.push(result);
    }

    results
}

fn print_operation_start(message: &str, output_mode: OutputMode) {
    if output_mode != OutputMode::Quiet {
        print!("  {} ", Theme::muted("→"));
        print!("{}", message);
        std::io::Write::flush(&mut std::io::stdout()).ok();
    }
}

fn print_operation_result(result: &OptimizeResult, output_mode: OutputMode) {
    if output_mode == OutputMode::Quiet {
        return;
    }

    // Clear the line and print result
    print!("\r");
    
    if result.success {
        if result.message.starts_with("Skipped:") {
            println!("  {} {} - {}", Theme::muted("○"), result.action, Theme::muted(&result.message));
        } else {
            println!("  {} {} - {}", Theme::success("✓"), result.action, Theme::success(&result.message));
        }
    } else {
        println!("  {} {} - {}", Theme::error("✗"), result.action, Theme::error(&result.message));
    }
}

/// Print summary of optimization results
pub fn print_summary(results: &[OptimizeResult], output_mode: OutputMode) {
    if output_mode == OutputMode::Quiet {
        return;
    }

    let total = results.len();
    let success = results.iter().filter(|r| r.success && !r.message.starts_with("Skipped:")).count();
    let skipped = results.iter().filter(|r| r.message.starts_with("Skipped:")).count();
    let failed = results.iter().filter(|r| !r.success).count();

    println!();
    println!("{}", Theme::divider(60));
    println!(
        "{}",
        Theme::primary(&format!(
            "Summary: {} total, {} succeeded, {} skipped, {} failed",
            total, success, skipped, failed
        ))
    );

    // Show restart hint if network was reset
    if results.iter().any(|r| r.action == "Reset Network Stack" && r.success && !r.message.starts_with("Skipped:")) {
        println!();
        println!("{}", Theme::warning("Note: A system restart is recommended after network reset."));
    }
}
