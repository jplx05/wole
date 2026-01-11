//! Browser database vacuum operation.

use super::super::result::OptimizeResult;
use std::env;
use std::fs;
use std::path::PathBuf;

/// Optimize browser SQLite databases using VACUUM
pub fn vacuum_browser_databases(dry_run: bool) -> OptimizeResult {
    let action = "Optimize Browser Databases";

    let local_app_data = match env::var("LOCALAPPDATA") {
        Ok(path) => PathBuf::from(path),
        Err(_) => {
            return OptimizeResult::failure(action, "Could not find LOCALAPPDATA path", false)
        }
    };

    let app_data = match env::var("APPDATA") {
        Ok(path) => PathBuf::from(path),
        Err(_) => return OptimizeResult::failure(action, "Could not find APPDATA path", false),
    };

    // Browser database paths
    let mut db_paths: Vec<PathBuf> = Vec::new();

    // Microsoft Edge
    let edge_default = local_app_data
        .join("Microsoft")
        .join("Edge")
        .join("User Data")
        .join("Default");
    for db_name in &["History", "Cookies", "Web Data", "Favicons"] {
        let path = edge_default.join(db_name);
        if path.exists() {
            db_paths.push(path);
        }
    }

    // Google Chrome
    let chrome_default = local_app_data
        .join("Google")
        .join("Chrome")
        .join("User Data")
        .join("Default");
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
                    for db_name in &[
                        "places.sqlite",
                        "cookies.sqlite",
                        "formhistory.sqlite",
                        "favicons.sqlite",
                    ] {
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
            &format!(
                "Dry run mode - would VACUUM {} database files",
                db_paths.len()
            ),
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
