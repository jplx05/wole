use crate::output::CategoryResult;
use anyhow::{Context, Result};
use chrono::{Duration, Utc};
use std::env;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Maximum number of results to return (prevents overwhelming output)
const MAX_RESULTS: usize = 200;

/// Scan Downloads folder for old files
/// 
/// Optimizations:
/// - Only scans top-level files (max_depth 1) - subfolders are usually intentional
/// - Sorts by size descending (biggest files first)
/// - Limits to top 200 results
/// - No git checks needed (Downloads is never a git repo)
pub fn scan(_root: &Path, min_age_days: u64) -> Result<CategoryResult> {
    let mut result = CategoryResult::default();
    
    let cutoff = Utc::now() - Duration::days(min_age_days as i64);
    
    // Get Downloads folder
    let downloads_path = if let Ok(user_profile) = env::var("USERPROFILE") {
        PathBuf::from(&user_profile).join("Downloads")
    } else {
        return Ok(result); // Can't find Downloads folder
    };
    
    if !downloads_path.exists() {
        return Ok(result); // Downloads folder doesn't exist
    }
    
    // Collect files with sizes for sorting
    let mut files_with_sizes: Vec<(PathBuf, u64)> = Vec::new();
    
    // Scan Downloads directory - TOP LEVEL ONLY
    // Subfolders in Downloads are usually intentionally organized
    for entry in WalkDir::new(&downloads_path)
        .max_depth(1)  // Only top-level files
        .follow_links(false)
        .into_iter()
    {
        match entry {
            Ok(entry) => {
                // Skip the root directory itself
                if entry.path() == downloads_path {
                    continue;
                }
                
                match entry.metadata() {
                    Ok(metadata) if metadata.is_file() => {
                        if let Ok(modified) = metadata.modified() {
                            let modified_dt: chrono::DateTime<Utc> = modified.into();
                            if modified_dt < cutoff {
                                files_with_sizes.push((entry.path().to_path_buf(), metadata.len()));
                            }
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                        continue;
                    }
                    _ => {}
                }
            }
            Err(e) if e.io_error().map(|io_err| io_err.kind() == std::io::ErrorKind::PermissionDenied).unwrap_or(false) => {
                continue;
            }
            Err(_) => {
                continue;
            }
        }
    }
    
    // Sort by size descending (biggest first)
    files_with_sizes.sort_by(|a, b| b.1.cmp(&a.1));
    
    // Limit results
    files_with_sizes.truncate(MAX_RESULTS);
    
    // Build result
    for (path, size) in files_with_sizes {
        result.items += 1;
        result.size_bytes += size;
        result.paths.push(path);
    }
    
    Ok(result)
}

/// Clean (delete) a file from Downloads by moving it to the Recycle Bin
pub fn clean(path: &Path) -> Result<()> {
    trash::delete(path)
        .with_context(|| format!("Failed to delete file: {}", path.display()))?;
    Ok(())
}
