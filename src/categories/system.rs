use crate::config::Config;
use crate::output::CategoryResult;
use crate::utils;
use anyhow::{Context, Result};
use std::env;
use std::path::{Path, PathBuf};
use trash;

/// Scan for Windows system cache files
///
/// Includes:
/// - Thumbnail cache (thumbcache_*.db)
/// - Windows Update cache (if accessible)
/// - Icon cache
pub fn scan(_root: &Path, config: &Config) -> Result<CategoryResult> {
    let mut result = CategoryResult::default();
    let mut paths = Vec::new();

    let local_appdata = env::var("LOCALAPPDATA").ok().map(PathBuf::from);

    // Scan thumbnail cache
    if let Some(ref local_appdata_path) = local_appdata {
        let explorer_path = local_appdata_path
            .join("Microsoft")
            .join("Windows")
            .join("Explorer");
        if explorer_path.exists() {
            // Look for thumbcache_*.db files
            if let Ok(entries) = std::fs::read_dir(&explorer_path) {
                for entry in entries.filter_map(|e| e.ok()) {
                    let path = entry.path();
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        if name.starts_with("thumbcache_")
                            && name.ends_with(".db")
                            && !config.is_excluded(&path)
                        {
                            if let Ok(metadata) = std::fs::metadata(&path) {
                                if metadata.is_file() {
                                    let size = metadata.len();
                                    if size > 0 {
                                        result.items += 1;
                                        result.size_bytes += size;
                                        paths.push(path);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Scan icon cache
        let icon_cache = local_appdata_path.join("IconCache.db");
        if icon_cache.exists() {
            if let Ok(metadata) = std::fs::metadata(&icon_cache) {
                let size = metadata.len();
                if size > 0 {
                    result.items += 1;
                    result.size_bytes += size;
                    paths.push(icon_cache);
                }
            }
        }
    }

    // Scan Windows Update cache (requires admin, gracefully skip if denied)
    let windows_update_path = PathBuf::from("C:\\Windows\\SoftwareDistribution\\Download");
    if windows_update_path.exists() && !config.is_excluded(&windows_update_path) {
        match utils::calculate_dir_size(&windows_update_path) {
            size if size > 0 => {
                result.items += 1;
                result.size_bytes += size;
                paths.push(windows_update_path);
            }
            _ => {}
        }
    }

    // Sort by size descending
    let mut paths_with_sizes: Vec<(PathBuf, u64)> = paths
        .into_iter()
        .map(|p| {
            let size = if p.is_dir() {
                utils::calculate_dir_size(&p)
            } else {
                std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0)
            };
            (p, size)
        })
        .collect();
    paths_with_sizes.sort_by(|a, b| b.1.cmp(&a.1));

    result.paths = paths_with_sizes.into_iter().map(|(p, _)| p).collect();

    Ok(result)
}

/// Clean (delete) a system cache file/directory by moving it to the Recycle Bin
pub fn clean(path: &Path) -> Result<()> {
    trash::delete(path)
        .with_context(|| format!("Failed to delete system cache: {}", path.display()))?;
    Ok(())
}
