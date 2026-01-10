use crate::config::Config;
use crate::git;
use crate::output::{CategoryResult, OutputMode};
use crate::project;
use crate::theme::Theme;
use crate::utils;
use anyhow::{Context, Result};
use bytesize;
use jwalk::WalkDir;
use std::env;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Maximum number of results to return
const MAX_RESULTS: usize = 100;

/// Scan for large files in user directories
///
/// Optimizations:
/// - Uses cached git root lookups (100x faster)
/// - Skips walking into node_modules, .git, etc. (early bailout)
/// - Checks config exclusions during traversal (prevents walking excluded trees)
/// - Sorts by size descending (biggest first)
/// - Limits to top 100 results
/// - Detects file types (video, archive, disk image, etc.)
pub fn scan(_root: &Path, min_size_bytes: u64, config: &Config, output_mode: OutputMode) -> Result<CategoryResult> {
    let mut result = CategoryResult::default();

    // Get user directories to scan
    let user_dirs = get_user_directories()?;

    if output_mode != OutputMode::Quiet && !user_dirs.is_empty() {
        println!("  {} Scanning {} directories for large files...", Theme::muted("→"), user_dirs.len());
    }

    // Collect files with sizes for sorting
    let mut files_with_sizes: Vec<(PathBuf, u64)> = Vec::new();

    for dir in &user_dirs {
        if output_mode != OutputMode::Quiet {
            println!("    {} Scanning {}", Theme::muted("•"), dir.display());
        }
        scan_directory(dir, min_size_bytes, &mut files_with_sizes, config, output_mode)?;
    }

    // Sort by size descending (biggest first)
    files_with_sizes.sort_by(|a, b| b.1.cmp(&a.1));

    // Limit results
    files_with_sizes.truncate(MAX_RESULTS);

    // Show found files
    if output_mode != OutputMode::Quiet && !files_with_sizes.is_empty() {
        println!("  {} Found {} large files:", Theme::muted("→"), files_with_sizes.len());
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

/// Scan a directory for large files with parallel traversal
fn scan_directory(
    dir: &Path,
    min_size_bytes: u64,
    files: &mut Vec<(PathBuf, u64)>,
    config: &Config,
    _output_mode: OutputMode,
) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }

    const MAX_DEPTH: usize = 20;

    // Clone config for thread-safe access (jwalk requires 'static)
    let config_clone = Arc::new(config.clone());
    // Clone again for the second closure
    let config_clone_for_each = Arc::clone(&config_clone);

    // Use Arc<Mutex<>> for thread-safe collection that can be shared
    let found_files: Arc<Mutex<Vec<(PathBuf, u64)>>> = Arc::new(Mutex::new(Vec::new()));
    // Clone Arc for the closure
    let found_files_clone = Arc::clone(&found_files);

    // Use jwalk for parallel directory traversal
    WalkDir::new(dir)
        .max_depth(MAX_DEPTH)
        .follow_links(false)
        .parallelism(jwalk::Parallelism::RayonDefaultPool {
            busy_timeout: std::time::Duration::from_secs(1),
        })
        .process_read_dir(move |_depth, _path, _read_dir_state, children| {
            // Filter out directories we don't want to descend into
            children.retain(|entry| {
                if let Ok(ref e) = entry {
                    let path = e.path();

                    // Skip symlinks
                    if e.file_type().is_symlink() {
                        return false;
                    }

                    if e.file_type().is_dir() {
                        // Skip system/build directories (inline for speed)
                        if let Some(name) = path.file_name() {
                            let name_lower = name.to_string_lossy().to_lowercase();
                            if matches!(
                                name_lower.as_str(),
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

                        // Check user exclusions
                        if config_clone.is_excluded(&path) {
                            return false;
                        }
                    }
                }
                true
            });
        })
        .into_iter()
        .filter_map(|e| e.ok())
        .for_each(move |entry| {
            let path = entry.path();

            // Check if it's a file
            if !entry.file_type().is_file() {
                return;
            }

            // Get metadata and check size
            let metadata = match entry.metadata() {
                Ok(m) => m,
                Err(_) => return,
            };

            // Check size threshold first (fast)
            if metadata.len() < min_size_bytes {
                return;
            }

            // Skip hidden files
            if utils::is_hidden(&path) {
                return;
            }

            // Skip files in active projects (using CACHED git lookup for performance)
            // This is a critical safety check to prevent deletion of files from projects
            // the user is actively working on
            // PERFORMANCE: Both find_git_root_cached and is_project_active are now cached
            if let Some(project_root) = git::find_git_root_cached(&path) {
                // Use project_age_days from config (defaults to 14 if not set)
                let project_age_days = config_clone_for_each.thresholds.project_age_days;
                if let Ok(true) = project::is_project_active(&project_root, project_age_days) {
                    return; // Skip files from active projects
                }
            }

            let mut files_guard = found_files_clone.lock().unwrap();
            files_guard.push((path, metadata.len()));
        });

    // Move collected files to output
    // Use Arc::try_unwrap to get the inner Mutex, then into_inner to get the Vec
    let mut collected = Arc::try_unwrap(found_files).unwrap().into_inner().unwrap();
    files.append(&mut collected);

    Ok(())
}

/// Get file type for a large file (for display purposes)
pub fn get_file_type(path: &Path) -> utils::FileType {
    utils::detect_file_type(path)
}

/// Clean (delete) a large file by moving it to the Recycle Bin
pub fn clean(path: &Path) -> Result<()> {
    trash::delete(path)
        .with_context(|| format!("Failed to delete large file: {}", path.display()))?;
    Ok(())
}
