use crate::config::Config;
use crate::output::CategoryResult;
use crate::scan_events::{ScanPathReporter, ScanProgressEvent};
use crate::utils;
use anyhow::{Context, Result};
use std::env;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use walkdir::WalkDir;

/// Scan for empty folders in user directories
///
/// An empty folder is one that contains no files (recursively).
/// Folders that only contain other empty folders are also considered empty.
pub fn scan(_root: &Path, config: &Config) -> Result<CategoryResult> {
    scan_internal(_root, config, None)
}

/// Scan for empty folders with TUI progress updates (current directory path).
pub fn scan_with_progress(
    root: &Path,
    config: &Config,
    tx: &Sender<ScanProgressEvent>,
) -> Result<CategoryResult> {
    const CATEGORY: &str = "Empty Folders";

    let _ = tx.send(ScanProgressEvent::CategoryStarted {
        category: CATEGORY.to_string(),
        total_units: None,
        current_path: None,
    });

    let reporter = ScanPathReporter::new(CATEGORY, tx.clone(), 75);
    scan_internal(root, config, Some(reporter))
}

/// Internal scan function that optionally uses a progress reporter
fn scan_internal(
    _root: &Path,
    config: &Config,
    reporter: Option<ScanPathReporter>,
) -> Result<CategoryResult> {
    let mut result = CategoryResult::default();
    let mut paths = Vec::new();

    // Get user directories to scan
    let user_dirs = get_user_directories()?;

    for dir in user_dirs {
        if !dir.exists() {
            continue;
        }

        // Walk directories, checking each one
        // Limit depth to prevent stack overflow, especially on Windows with smaller stack size
        // Use a very conservative limit for Windows test threads (2MB stack)
        const MAX_DEPTH: usize = 10;
        for entry in WalkDir::new(&dir)
            .max_depth(MAX_DEPTH)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| {
                // 1. Check hardcoded skips first (fast)
                if should_skip_entry(e) {
                    return false;
                }

                // 2. Check user config exclusions IMMEDIATELY (prevents traversal)
                // Only check directories - files don't need exclusion checks during traversal
                if e.file_type().is_dir() && config.is_excluded(e.path()) {
                    return false;
                }

                true
            })
        {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            let path = entry.path();

            // Only check directories
            if !path.is_dir() {
                continue;
            }

            // Skip if it's a system path
            if utils::is_system_path(path) {
                continue;
            }

            // Emit path progress for TUI
            if let Some(ref reporter) = reporter {
                reporter.emit_path(path);
            }

            // Check if directory is empty
            if is_dir_empty(path)? {
                result.items += 1;
                // Empty folders don't take up meaningful space, but we count them
                result.size_bytes += 0;
                paths.push(path.to_path_buf());
            }
        }
    }

    result.paths = paths;
    Ok(result)
}

/// Get user directories to scan
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

/// Check if a directory is empty (contains no files, recursively)
///
/// Uses a limited depth walk to avoid stack overflow on deep directory structures.
/// This is called for each directory found during the main scan, so we keep the depth
/// limit conservative to prevent excessive recursion.
fn is_dir_empty(path: &Path) -> Result<bool> {
    let mut has_files = false;

    // Use a very conservative depth limit since this is called for every directory
    // in the main scan, creating nested recursion
    const MAX_CHECK_DEPTH: usize = 5;

    for entry in WalkDir::new(path)
        .max_depth(MAX_CHECK_DEPTH)
        .follow_links(false)
        .into_iter()
    {
        match entry {
            Ok(entry) => {
                // Skip symlinks, junctions, and reparse points when probing emptiness
                if entry.file_type().is_dir() && utils::should_skip_entry(entry.path()) {
                    continue;
                }
                if entry.file_type().is_file() {
                    has_files = true;
                    break;
                }
            }
            Err(_) => {
                // Skip errors (permission denied, etc.)
                // If we can't read it, assume it's not empty to be safe
                continue;
            }
        }
    }

    Ok(!has_files)
}

/// Check if we should skip walking into this directory
fn should_skip_entry(entry: &walkdir::DirEntry) -> bool {
    if !entry.file_type().is_dir() {
        return false;
    }

    // Skip symlinks, junctions, and reparse points (prevents infinite loops)
    if utils::should_skip_entry(entry.path()) {
        return true;
    }

    if let Some(name) = entry.file_name().to_str() {
        // Skip system directories
        if utils::is_system_path(entry.path()) {
            return true;
        }

        // Skip known build/cache directories (they're handled by other categories)
        return matches!(
            name.to_lowercase().as_str(),
            "node_modules"
                | ".git"
                | ".hg"
                | ".svn"
                | "target"
                | ".gradle"
                | "__pycache__"
                | ".venv"
                | "venv"
                | ".next"
                | ".nuxt"
                | "$recycle.bin"
                | "system volume information"
                | "appdata"
                | "programdata"
        );
    }

    false
}

/// Clean (delete) an empty folder by moving it to the Recycle Bin
pub fn clean(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    crate::trash_ops::delete(path)
        .with_context(|| format!("Failed to delete empty folder: {}", path.display()))?;
    Ok(())
}
