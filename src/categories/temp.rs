use crate::config::Config;
use crate::output::CategoryResult;
use crate::scan_events::{ScanPathReporter, ScanProgressEvent};
use anyhow::{Context, Result};
use chrono::{Duration, Utc};
use std::env;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use walkdir::WalkDir;

/// Maximum number of results to return
const MAX_RESULTS: usize = 500;

/// Scan for temporary files older than 1 day
///
/// Checks %TEMP% and %LOCALAPPDATA%\Temp directories
/// Optimizations:
/// - Limits depth to 3 levels (deep temp files are usually system files)
/// - Checks config exclusions during traversal (prevents walking excluded trees)
/// - Sorts by size descending
/// - Limits to top 500 results
pub fn scan(_root: &Path, config: &Config) -> Result<CategoryResult> {
    let mut result = CategoryResult::default();

    let cutoff = Utc::now() - Duration::days(1);

    // Collect files with sizes for sorting
    let mut files_with_sizes: Vec<(PathBuf, u64)> = Vec::new();

    // %TEMP% directory
    if let Ok(temp_dir) = env::var("TEMP") {
        scan_temp_dir(
            &PathBuf::from(&temp_dir),
            &cutoff,
            &mut files_with_sizes,
            config,
            None,
        );
    }

    // %LOCALAPPDATA%\Temp
    if let Ok(local_appdata) = env::var("LOCALAPPDATA") {
        let local_temp = PathBuf::from(&local_appdata).join("Temp");
        if local_temp.exists() {
            scan_temp_dir(&local_temp, &cutoff, &mut files_with_sizes, config, None);
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

/// Scan with real-time progress events (for TUI).
pub fn scan_with_progress(
    _root: &Path,
    config: &Config,
    tx: &Sender<ScanProgressEvent>,
) -> Result<CategoryResult> {
    const CATEGORY: &str = "Temp Files";
    let cutoff = Utc::now() - Duration::days(1);

    let mut result = CategoryResult::default();
    let mut files_with_sizes: Vec<(PathBuf, u64)> = Vec::new();

    // Build a list of temp roots to scan.
    let mut temp_roots: Vec<PathBuf> = Vec::new();
    if let Ok(temp_dir) = env::var("TEMP") {
        temp_roots.push(PathBuf::from(&temp_dir));
    }
    if let Ok(local_appdata) = env::var("LOCALAPPDATA") {
        let local_temp = PathBuf::from(&local_appdata).join("Temp");
        temp_roots.push(local_temp);
    }
    // Deduplicate.
    temp_roots.sort();
    temp_roots.dedup();

    let total = temp_roots.len() as u64;
    let _ = tx.send(ScanProgressEvent::CategoryStarted {
        category: CATEGORY.to_string(),
        total_units: Some(total.max(1)),
        current_path: None,
    });

    if temp_roots.is_empty() {
        let _ = tx.send(ScanProgressEvent::CategoryFinished {
            category: CATEGORY.to_string(),
            items: 0,
            size_bytes: 0,
        });
        return Ok(result);
    }

    let reporter = ScanPathReporter::new(CATEGORY, tx.clone(), 10);

    for (idx, root) in temp_roots.iter().enumerate() {
        if root.exists() {
            scan_temp_dir(
                root,
                &cutoff,
                &mut files_with_sizes,
                config,
                Some(&reporter),
            );
        }
        let _ = tx.send(ScanProgressEvent::CategoryProgress {
            category: CATEGORY.to_string(),
            completed_units: (idx + 1) as u64,
            total_units: Some(total),
            current_path: Some(root.clone()),
        });
    }

    // Sort by size descending
    files_with_sizes.sort_by(|a, b| b.1.cmp(&a.1));
    files_with_sizes.truncate(MAX_RESULTS);

    for (path, size) in files_with_sizes {
        result.items += 1;
        result.size_bytes += size;
        result.paths.push(path);
    }

    let _ = tx.send(ScanProgressEvent::CategoryFinished {
        category: CATEGORY.to_string(),
        items: result.items,
        size_bytes: result.size_bytes,
    });

    Ok(result)
}

fn scan_temp_dir(
    temp_path: &Path,
    cutoff: &chrono::DateTime<Utc>,
    files: &mut Vec<(PathBuf, u64)>,
    config: &Config,
    reporter: Option<&ScanPathReporter>,
) {
    for entry in WalkDir::new(temp_path)
        .max_depth(3)
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
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        if let Some(reporter) = reporter {
            reporter.emit_path(entry.path());
        }

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
