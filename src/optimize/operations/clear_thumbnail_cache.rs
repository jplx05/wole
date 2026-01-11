//! Clear thumbnail cache operation.

use super::super::result::OptimizeResult;
use std::env;
use std::fs;
use std::path::PathBuf;

/// Clear thumbnail cache files from Windows Explorer
pub fn clear_thumbnail_cache(dry_run: bool) -> OptimizeResult {
    let action = "Clear Thumbnail Cache";

    let local_app_data = match env::var("LOCALAPPDATA") {
        Ok(path) => PathBuf::from(path),
        Err(_) => {
            return OptimizeResult::failure(action, "Could not find LOCALAPPDATA path", false)
        }
    };

    let explorer_path = local_app_data
        .join("Microsoft")
        .join("Windows")
        .join("Explorer");

    if !explorer_path.exists() {
        return OptimizeResult::skipped(action, "Explorer cache directory not found", false);
    }

    if dry_run {
        return OptimizeResult::skipped(
            action,
            &format!(
                "Dry run mode - would delete thumbcache_*.db files in {}",
                explorer_path.display()
            ),
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
            &format!(
                "Deleted {} thumbnail cache files ({} locked/skipped)",
                deleted_count, failed_count
            ),
            false,
        )
    } else {
        OptimizeResult::failure(
            action,
            &format!(
                "Could not delete thumbnail cache files ({} locked)",
                failed_count
            ),
            false,
        )
    }
}
