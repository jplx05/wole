use crate::output::CategoryResult;
use anyhow::Result;
use chrono::{Duration, Utc};
use std::env;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Maximum number of results to return
const MAX_RESULTS: usize = 500;

/// Scan for temporary files older than 1 day
/// 
/// Checks %TEMP% and %LOCALAPPDATA%\Temp directories
/// Optimizations:
/// - Limits depth to 3 levels (deep temp files are usually system files)
/// - Sorts by size descending
/// - Limits to top 500 results
pub fn scan(_root: &Path) -> Result<CategoryResult> {
    let mut result = CategoryResult::default();
    
    let cutoff = Utc::now() - Duration::days(1);
    
    // Collect files with sizes for sorting
    let mut files_with_sizes: Vec<(PathBuf, u64)> = Vec::new();
    
    // %TEMP% directory
    if let Ok(temp_dir) = env::var("TEMP") {
        scan_temp_dir(&PathBuf::from(&temp_dir), &cutoff, &mut files_with_sizes);
    }
    
    // %LOCALAPPDATA%\Temp
    if let Ok(local_appdata) = env::var("LOCALAPPDATA") {
        let local_temp = PathBuf::from(&local_appdata).join("Temp");
        if local_temp.exists() {
            scan_temp_dir(&local_temp, &cutoff, &mut files_with_sizes);
        }
    }
    
    // Sort by size descending
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

fn scan_temp_dir(
    temp_path: &Path,
    cutoff: &chrono::DateTime<Utc>,
    files: &mut Vec<(PathBuf, u64)>,
) {
    for entry in WalkDir::new(temp_path)
        .max_depth(3)
        .follow_links(false)
        .into_iter()
    {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        
        let metadata = match entry.metadata() {
            Ok(m) if m.is_file() => m,
            _ => continue,
        };
        
        if let Ok(modified) = metadata.modified() {
            let modified_dt: chrono::DateTime<Utc> = modified.into();
            if modified_dt < *cutoff {
                files.push((entry.path().to_path_buf(), metadata.len()));
            }
        }
    }
}

/// Clean (delete) a temp file by moving it to the Recycle Bin
pub fn clean(path: &Path) -> Result<()> {
    trash::delete(path)
        .with_context(|| format!("Failed to delete temp file: {}", path.display()))?;
    Ok(())
}
