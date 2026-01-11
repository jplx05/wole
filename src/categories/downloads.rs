use crate::config::Config;
use crate::output::{CategoryResult, OutputMode};
use crate::scan_events::{ScanPathReporter, ScanProgressEvent};
use crate::theme::Theme;
use anyhow::{Context, Result};
use bytesize;
use chrono::{Duration, Utc};
use std::env;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use walkdir::WalkDir;

/// Maximum number of results to return (prevents overwhelming output)
const MAX_RESULTS: usize = 200;

/// Scan Downloads folder for old files
///
/// Optimizations:
/// - Only scans top-level files (max_depth 1) - subfolders are usually intentional
/// - Checks config exclusions during traversal
/// - Sorts by size descending (biggest files first)
/// - Limits to top 200 results
/// - No git checks needed (Downloads is never a git repo)
pub fn scan(
    _root: &Path,
    min_age_days: u64,
    config: &Config,
    output_mode: OutputMode,
) -> Result<CategoryResult> {
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

    if output_mode != OutputMode::Quiet {
        println!(
            "  {} Scanning Downloads folder for files older than {} days...",
            Theme::muted("→"),
            min_age_days
        );
    }

    // Collect files with sizes for sorting
    let mut files_with_sizes: Vec<(PathBuf, u64)> = Vec::new();

    // Scan Downloads directory - TOP LEVEL ONLY
    // Subfolders in Downloads are usually intentionally organized
    for entry in WalkDir::new(&downloads_path)
        .max_depth(1) // Only top-level files
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            // Check user config exclusions IMMEDIATELY (prevents traversal)
            // Only check directories - files don't need exclusion checks during traversal
            if e.file_type().is_dir() && config.is_excluded(e.path()) {
                return false;
            }
            true
        })
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
                    Err(e)
                        if e.io_error()
                            .map(|io_err| io_err.kind() == std::io::ErrorKind::PermissionDenied)
                            .unwrap_or(false) =>
                    {
                        continue;
                    }
                    _ => {}
                }
            }
            Err(e)
                if e.io_error()
                    .map(|io_err| io_err.kind() == std::io::ErrorKind::PermissionDenied)
                    .unwrap_or(false) =>
            {
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

    // Show found files
    if output_mode != OutputMode::Quiet && !files_with_sizes.is_empty() {
        println!(
            "  {} Found {} old files in Downloads:",
            Theme::muted("→"),
            files_with_sizes.len()
        );
        let show_count = match output_mode {
            OutputMode::VeryVerbose => files_with_sizes.len(),
            OutputMode::Verbose => files_with_sizes.len(),
            OutputMode::Normal => 10.min(files_with_sizes.len()),
            OutputMode::Quiet => 0,
        };

        for (i, (path, size)) in files_with_sizes.iter().take(show_count).enumerate() {
            let size_str = bytesize::to_string(*size, false);
            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let file_type = crate::utils::detect_file_type(path);
            let emoji = file_type.emoji();
            println!(
                "      {} {} {} ({})",
                Theme::muted("→"),
                emoji,
                file_name,
                Theme::size(&size_str)
            );

            if i == 9 && output_mode == OutputMode::Normal && files_with_sizes.len() > 10 {
                println!(
                    "      {} ... and {} more (use -v to see all)",
                    Theme::muted("→"),
                    files_with_sizes.len() - 10
                );
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

/// Scan Downloads with TUI progress updates (current file path).
pub fn scan_with_progress(
    root: &Path,
    min_age_days: u64,
    config: &Config,
    output_mode: OutputMode,
    tx: &Sender<ScanProgressEvent>,
) -> Result<CategoryResult> {
    let reporter = ScanPathReporter::new("Old Downloads", tx.clone(), 75);
    let cutoff = Utc::now() - Duration::days(min_age_days as i64);

    let mut result = CategoryResult::default();
    let downloads_path = if let Ok(user_profile) = env::var("USERPROFILE") {
        PathBuf::from(&user_profile).join("Downloads")
    } else {
        return Ok(result);
    };
    if !downloads_path.exists() {
        return Ok(result);
    }

    let mut files_with_sizes: Vec<(PathBuf, u64)> = Vec::new();

    for entry in WalkDir::new(&downloads_path)
        .max_depth(1)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !(e.file_type().is_dir() && config.is_excluded(e.path())))
    {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        if entry.path() == downloads_path {
            continue;
        }

        reporter.emit_path(entry.path());

        match entry.metadata() {
            Ok(metadata) if metadata.is_file() => {
                if let Ok(modified) = metadata.modified() {
                    let modified_dt: chrono::DateTime<Utc> = modified.into();
                    if modified_dt < cutoff {
                        files_with_sizes.push((entry.path().to_path_buf(), metadata.len()));
                    }
                }
            }
            _ => {}
        }
    }

    files_with_sizes.sort_by(|a, b| b.1.cmp(&a.1));
    files_with_sizes.truncate(MAX_RESULTS);
    for (path, size) in files_with_sizes {
        result.items += 1;
        result.size_bytes += size;
        result.paths.push(path);
    }

    let _ = root;
    let _ = output_mode;
    Ok(result)
}

/// Clean (delete) a file from Downloads by moving it to the Recycle Bin
pub fn clean(path: &Path) -> Result<()> {
    crate::trash_ops::delete(path)
        .with_context(|| format!("Failed to delete file: {}", path.display()))?;
    Ok(())
}
