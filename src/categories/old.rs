use crate::git;
use crate::output::CategoryResult;
use crate::project;
use crate::utils;
use anyhow::{Context, Result};
use chrono::{Duration, Utc};
use std::env;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Maximum number of results to return
const MAX_RESULTS: usize = 200;

/// Minimum file size to consider (skip tiny files that add noise)
const MIN_FILE_SIZE: u64 = 10 * 1024; // 10 KB

/// Scan for old files in user directories
/// 
/// Optimizations:
/// - Uses cached git root lookups (100x faster)
/// - Skips walking into node_modules, .git, etc. (early bailout)
/// - Skips files smaller than 10KB (reduces noise)
/// - Sorts by size descending (biggest first)
/// - Limits to top 200 results
pub fn scan(_root: &Path, min_age_days: u64) -> Result<CategoryResult> {
    let mut result = CategoryResult::default();
    
    let cutoff = Utc::now() - Duration::days(min_age_days as i64);
    
    // Get user directories to scan
    let user_dirs = get_user_directories()?;
    
    // Collect files with sizes for sorting
    let mut files_with_sizes: Vec<(PathBuf, u64)> = Vec::new();
    
    for dir in user_dirs {
        scan_directory(&dir, &cutoff, &mut files_with_sizes)?;
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

/// Get user directories to scan (Downloads, Documents, Desktop, Pictures, Videos, Music)
fn get_user_directories() -> Result<Vec<PathBuf>> {
    let mut dirs = Vec::new();
    
    if let Ok(user_profile) = env::var("USERPROFILE") {
        let profile_path = PathBuf::from(&user_profile);
        dirs.push(profile_path.join("Downloads"));
        dirs.push(profile_path.join("Documents"));
        dirs.push(profile_path.join("Desktop"));
        dirs.push(profile_path.join("Pictures"));
        dirs.push(profile_path.join("Videos"));
        dirs.push(profile_path.join("Music"));
    }
    
    Ok(dirs)
}

/// Scan a directory for old files with optimizations
fn scan_directory(
    dir: &Path,
    cutoff: &chrono::DateTime<Utc>,
    files: &mut Vec<(PathBuf, u64)>,
) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    
    for entry in WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !should_skip_entry(e))  // Early skip optimization
    {
        let entry = match entry {
            Ok(e) => e,
            Err(e) if e.io_error().map(|io| io.kind() == std::io::ErrorKind::PermissionDenied).unwrap_or(false) => {
                continue;
            }
            Err(_) => continue,
        };
        
        let path = entry.path();
        
        // Check if it's a file
        let metadata = match entry.metadata() {
            Ok(m) if m.is_file() => m,
            Ok(_) => continue,
            Err(_) => continue,
        };
        
        // Skip hidden files
        if utils::is_hidden(path) {
            continue;
        }
        
        // Skip tiny files (reduces noise)
        if metadata.len() < MIN_FILE_SIZE {
            continue;
        }
        
        // Check age threshold
        let modified = match metadata.modified() {
            Ok(m) => m,
            Err(_) => continue,
        };
        let modified_dt: chrono::DateTime<Utc> = modified.into();
        if modified_dt >= *cutoff {
            continue; // File is too new
        }
        
        // Skip files in active projects (using CACHED git lookup)
        if let Some(project_root) = git::find_git_root_cached(path) {
            if let Ok(true) = project::is_project_active(&project_root, 14) {
                continue; // Skip active projects
            }
        }
        
        files.push((path.to_path_buf(), metadata.len()));
    }
    
    Ok(())
}

/// Check if we should skip walking into this directory
fn should_skip_entry(entry: &walkdir::DirEntry) -> bool {
    if !entry.file_type().is_dir() {
        return false;
    }
    
    if let Some(name) = entry.file_name().to_str() {
        // Skip these directories entirely
        return matches!(name.to_lowercase().as_str(),
            "node_modules" | ".git" | ".hg" | ".svn" |
            "target" | ".gradle" | "__pycache__" |
            ".venv" | "venv" | ".next" | ".nuxt" |
            "windows" | "program files" | "program files (x86)" |
            "$recycle.bin" | "system volume information" |
            "appdata" | "programdata"
        );
    }
    
    false
}

/// Clean (delete) an old file by moving it to the Recycle Bin
pub fn clean(path: &Path) -> Result<()> {
    trash::delete(path)
        .with_context(|| format!("Failed to delete old file: {}", path.display()))?;
    Ok(())
}
