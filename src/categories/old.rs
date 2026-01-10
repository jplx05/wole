use crate::config::Config;
use crate::git;
use crate::output::{CategoryResult, OutputMode};
use crate::project;
use crate::theme::Theme;
use crate::utils;
use anyhow::{Context, Result};
use bytesize;
use chrono::{Duration, Utc};
use std::env;
use std::path::{Path, PathBuf};

/// Maximum number of results to return
const MAX_RESULTS: usize = 200;

/// Minimum file size to consider (skip tiny files that add noise)
const MIN_FILE_SIZE: u64 = 10 * 1024; // 10 KB

/// Scan for old files in user directories
///
/// Optimizations:
/// - Uses cached git root lookups (100x faster)
/// - Skips walking into node_modules, .git, etc. (early bailout)
/// - Checks config exclusions during traversal (prevents walking excluded trees)
/// - Skips files smaller than 10KB (reduces noise)
/// - Sorts by size descending (biggest first)
/// - Limits to top 200 results
pub fn scan(_root: &Path, min_age_days: u64, config: &Config, output_mode: OutputMode) -> Result<CategoryResult> {
    let mut result = CategoryResult::default();

    let cutoff = Utc::now() - Duration::days(min_age_days as i64);

    // Get user directories to scan
    let user_dirs = get_user_directories()?;

    if output_mode != OutputMode::Quiet && !user_dirs.is_empty() {
        println!("  {} Scanning {} directories for old files (older than {} days)...", 
            Theme::muted("→"), user_dirs.len(), min_age_days);
    }

    // Collect files with sizes for sorting
    let mut files_with_sizes: Vec<(PathBuf, u64)> = Vec::new();

    for dir in &user_dirs {
        if output_mode != OutputMode::Quiet {
            println!("    {} Scanning {}", Theme::muted("•"), dir.display());
        }
        scan_directory(dir, &cutoff, &mut files_with_sizes, config, output_mode)?;
    }

    // Sort by size descending (biggest first)
    files_with_sizes.sort_by(|a, b| b.1.cmp(&a.1));

    // Limit results
    files_with_sizes.truncate(MAX_RESULTS);

    // Show found files
    if output_mode != OutputMode::Quiet && !files_with_sizes.is_empty() {
        println!("  {} Found {} old files:", Theme::muted("→"), files_with_sizes.len());
        let show_count = match output_mode {
            OutputMode::VeryVerbose => files_with_sizes.len(),
            OutputMode::Verbose => files_with_sizes.len(),
            OutputMode::Normal => 10.min(files_with_sizes.len()),
            OutputMode::Quiet => 0,
        };
        
        for (i, (path, size)) in files_with_sizes.iter().take(show_count).enumerate() {
            let size_str = bytesize::to_string(*size, true);
            println!("      {} {} ({})", Theme::muted("→"), path.display(), Theme::size(&size_str));
            
            if i == 9 && output_mode == OutputMode::Normal && files_with_sizes.len() > 10 {
                println!("      {} ... and {} more (use -v to see all)", 
                    Theme::muted("→"), 
                    files_with_sizes.len() - 10);
                break;
            }
        }
    }

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

use std::sync::Arc;

/// Scan a directory for old files with optimizations
fn scan_directory(
    dir: &Path,
    cutoff: &chrono::DateTime<Utc>,
    files: &mut Vec<(PathBuf, u64)>,
    config: &Config,
    _output_mode: OutputMode,
) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }

    use jwalk::WalkDir;

    const MAX_DEPTH: usize = 20;

    // Clone config for thread-safe access
    let config_arc = Arc::new(config.clone());

    let walk = WalkDir::new(dir)
        .max_depth(MAX_DEPTH)
        .follow_links(false)
        .parallelism(jwalk::Parallelism::RayonDefaultPool {
            busy_timeout: std::time::Duration::from_secs(1),
        })
        .process_read_dir(move |_depth, _path, _state, children| {
            let config = Arc::clone(&config_arc);
            children.retain(|entry| {
                if let Ok(ref e) = entry {
                    let path = e.path();

                    // 1. Skip symlinks/junctions
                    if e.file_type().is_symlink() || utils::is_windows_reparse_point(&path) {
                        return false;
                    }

                    if e.file_type().is_dir() {
                        // 2. Skip based on name
                        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                            let name_low = name.to_lowercase();
                            if matches!(
                                name_low.as_str(),
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
                                    | "windows"
                                    | "program files"
                                    | "program files (x86)"
                                    | "$recycle.bin"
                                    | "system volume information"
                                    | "appdata"
                                    | "programdata"
                            ) {
                                return false;
                            }
                        }

                        // 3. Skip based on config
                        if config.is_excluded(&path) {
                            return false;
                        }
                    }
                }
                true
            });
        });

    for e in walk.into_iter().flatten() {
        let path = e.path();

        // We only care about files
        if !e.file_type().is_file() {
            continue;
        }

        let metadata = match e.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };

        // Skip tiny files
        if metadata.len() < MIN_FILE_SIZE {
            continue;
        }

            // Check age
            if let Ok(modified) = metadata.modified() {
                let modified_dt: chrono::DateTime<Utc> = modified.into();
                if modified_dt < *cutoff {
                    // Skip files in active projects (using CACHED git lookup)
                    // PERFORMANCE: Both find_git_root_cached and is_project_active are now cached
                    if let Some(project_root) = git::find_git_root_cached(&path) {
                        // Use project_age_days from config (defaults to 14 if not set)
                        let project_age_days = config.thresholds.project_age_days;
                        if let Ok(true) = project::is_project_active(&project_root, project_age_days) {
                            continue;
                        }
                    }

                    files.push((path, metadata.len()));
                }
            }
    }

    Ok(())
}

/// Clean (delete) an old file by moving it to the Recycle Bin
pub fn clean(path: &Path) -> Result<()> {
    trash::delete(path)
        .with_context(|| format!("Failed to delete old file: {}", path.display()))?;
    Ok(())
}
