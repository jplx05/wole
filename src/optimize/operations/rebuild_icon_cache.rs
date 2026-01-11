//! Rebuild icon cache operation.

use super::restart_explorer::do_restart_explorer;
use super::super::result::OptimizeResult;
use std::env;
use std::fs;
use std::path::PathBuf;

/// Rebuild icon cache by deleting IconCache.db and iconcache_*.db files
pub fn rebuild_icon_cache(dry_run: bool, restart_explorer: bool) -> OptimizeResult {
    let action = "Rebuild Icon Cache";

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
    let icon_cache_path = local_app_data.join("IconCache.db");

    if dry_run {
        let mut msg = String::from("Dry run mode - would delete: ");
        if icon_cache_path.exists() {
            msg.push_str(&format!("{}, ", icon_cache_path.display()));
        }
        msg.push_str(&format!(
            "iconcache_*.db files in {}",
            explorer_path.display()
        ));
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
            if restart_explorer {
                ", Explorer restarted"
            } else {
                ""
            }
        ),
        false,
    )
}
