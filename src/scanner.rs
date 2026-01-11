use crate::categories;
use crate::cli::ScanOptions;
use crate::config::Config;
use crate::git;
use crate::output::{CategoryResult, OutputMode, ScanResults};
use crate::progress;
use crate::scan_cache::{FileSignature, ScanCache, ScanStats};
use crate::scan_events::ScanProgressEvent;
use crate::theme::Theme;
use crate::utils;
use anyhow::Result;
// use rayon::prelude::*; // Disabled: using sequential scan to avoid thrashing
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::Sender;

/// Try incremental scan for a category
/// Returns Ok(Some(result)) if cache was used, Ok(None) if full scan needed, Err on error
fn try_incremental_scan(
    category_name: &str,
    _task: &ScanTask,
    _path: &Path,
    _config: &Config,
    cache: &mut ScanCache,
    _scan_session_id: i64,
    _mode: OutputMode,
) -> Result<Option<CategoryResult>> {
    // Get the previous category-specific scan ID (without incrementing)
    let previous_category_scan_id = match cache.get_previous_category_scan_id(category_name)? {
        Some(id) => id,
        None => return Ok(None), // Category was never scanned before, need full scan
    };

    // Get cached paths for this category from the previous category scan
    let cached_paths = cache.get_cached_category(category_name, previous_category_scan_id)?;

    if cached_paths.is_empty() {
        return Ok(None); // No cached paths, need full scan
    }

    // Check which files changed
    let status_map = cache.check_files_batch(&cached_paths)?;

    let mut unchanged_paths = HashSet::new();
    let mut changed_paths = Vec::new();

    for (path, status) in status_map {
        match status {
            crate::scan_cache::FileStatus::Unchanged => {
                unchanged_paths.insert(path);
            }
            crate::scan_cache::FileStatus::Modified | crate::scan_cache::FileStatus::New => {
                changed_paths.push(path);
            }
            crate::scan_cache::FileStatus::Deleted => {
                // File deleted, skip it
            }
        }
    }

    // If most files are unchanged, use cache
    // Otherwise, do full scan (more efficient than checking many files)
    let unchanged_ratio = if cached_paths.is_empty() {
        0.0
    } else {
        unchanged_paths.len() as f64 / cached_paths.len() as f64
    };

    // Only use cache if >50% of files are unchanged
    if unchanged_ratio < 0.5 {
        return Ok(None);
    }

    // CRITICAL: We can't detect new files that weren't in the previous scan without
    // performing a full scan. The cache check only validates previously cached paths,
    // so any new files added to the category since the last scan won't be detected.
    // To avoid silently omitting new files, we must force a full scan when we can't
    // reliably detect additions. This ensures scan results are always complete.
    //
    // If there are changed files, we need to rescan them anyway, so fall back to full scan.
    // Even if no files changed, we can't know if new files were added without scanning.
    if !changed_paths.is_empty() {
        // Changed files detected - need to rescan them
        return Ok(None);
    }

    // Even with no changed files, we can't reliably detect new files that weren't
    // previously cached. To ensure completeness, force a full scan.
    // TODO: In future, could add a mechanism to discover new paths (e.g., quick directory
    // traversal to detect new files) to make incremental scanning more effective.
    return Ok(None);
}

/// Execute full category scan
#[allow(clippy::too_many_arguments)]
fn execute_category_scan(
    _category_name: &str,
    task: &ScanTask,
    path: &Path,
    config: &Config,
    mode: OutputMode,
    build_config: &crate::config::CategoryConfig,
    duplicates_config: &crate::config::DuplicatesConfig,
    duplicate_groups: &std::cell::RefCell<
        Option<Vec<crate::categories::duplicates::DuplicateGroup>>,
    >,
) -> Result<CategoryResult> {
    match task {
        ScanTask::Cache => categories::cache::scan(path, config, mode),
        ScanTask::AppCache => categories::app_cache::scan(path, config, mode),
        ScanTask::Temp => categories::temp::scan(path, config),
        ScanTask::Trash => categories::trash::scan(),
        ScanTask::Build(age) => {
            categories::build::scan(path, *age, Some(build_config), config, mode)
        }
        ScanTask::Downloads(age) => categories::downloads::scan(path, *age, config, mode),
        ScanTask::Large(size) => categories::large::scan(path, *size, config, mode),
        ScanTask::Old(age) => categories::old::scan(path, *age, config, mode),
        ScanTask::Browser => categories::browser::scan(path, config),
        ScanTask::System => categories::system::scan(path, config),
        ScanTask::Empty => categories::empty::scan(path, config),
        ScanTask::Duplicates => {
            match categories::duplicates::scan_with_config(path, Some(duplicates_config), config) {
                Ok(dup_result) => {
                    // Store groups for enhanced display
                    *duplicate_groups.borrow_mut() = Some(dup_result.groups.clone());
                    Ok(dup_result.to_category_result())
                }
                Err(e) => Err(e),
            }
        }
        ScanTask::Applications => categories::applications::scan(path, config, mode),
        ScanTask::WindowsUpdate => categories::windows_update::scan(path, config),
        ScanTask::EventLogs => categories::event_logs::scan(path, config),
    }
}

/// Scan all requested categories and return aggregated results
///
/// Optimizations:
/// - Clears git cache before scanning for fresh results
/// - Scans categories in parallel using rayon (2-3x faster on multi-core)
/// - Handles errors gracefully - if one category fails, others continue
/// - Filters out paths matching exclusion patterns from config
/// - Supports incremental scanning via scan_cache parameter
pub fn scan_all(
    path: &Path,
    options: ScanOptions,
    mode: OutputMode,
    config: &Config,
    mut scan_cache: Option<&mut ScanCache>,
) -> Result<ScanResults> {
    // Clear git cache for fresh scan
    git::clear_cache();

    let mut results = ScanResults::default();

    // Build list of enabled categories
    let mut enabled: Vec<(&str, ScanTask)> = Vec::new();

    if options.cache {
        enabled.push(("cache", ScanTask::Cache));
    }
    if options.app_cache {
        enabled.push(("app_cache", ScanTask::AppCache));
    }
    if options.temp {
        enabled.push(("temp", ScanTask::Temp));
    }
    if options.trash {
        enabled.push(("trash", ScanTask::Trash));
    }
    if options.build {
        enabled.push(("build", ScanTask::Build(options.project_age_days)));
    }
    if options.downloads {
        enabled.push(("downloads", ScanTask::Downloads(options.min_age_days)));
    }
    if options.large {
        enabled.push(("large", ScanTask::Large(options.min_size_bytes)));
    }
    if options.old {
        enabled.push(("old", ScanTask::Old(options.min_age_days)));
    }
    if options.browser {
        enabled.push(("browser", ScanTask::Browser));
    }
    if options.system {
        enabled.push(("system", ScanTask::System));
    }
    if options.empty {
        enabled.push(("empty", ScanTask::Empty));
    }
    if options.duplicates {
        enabled.push(("duplicates", ScanTask::Duplicates));
    }

    if options.applications {
        enabled.push(("applications", ScanTask::Applications));
    }

    if options.windows_update {
        enabled.push(("windows_update", ScanTask::WindowsUpdate));
    }

    if options.event_logs {
        enabled.push(("event_logs", ScanTask::EventLogs));
    }

    let total_categories = enabled.len();

    if total_categories == 0 {
        return Ok(results);
    }

    // Start scan session if cache is enabled
    let mut scan_id: Option<i64> = None;
    let mut cache_enabled = scan_cache.is_some() && config.cache.enabled;
    let mut use_incremental = false;
    let mut is_first_scan = false;

    if let Some(cache) = scan_cache.as_mut() {
        if cache_enabled {
            let categories: Vec<&str> = enabled.iter().map(|(name, _)| *name).collect();
            let previous_scan_id = match cache.get_previous_scan_id() {
                Ok(id) => id,
                Err(e) => {
                    if mode != OutputMode::Quiet {
                        eprintln!(
                            "Warning: Failed to read scan cache state: {}. Continuing without cache.",
                            e
                        );
                    }
                    cache_enabled = false;
                    None
                }
            };

            if cache_enabled {
                is_first_scan = previous_scan_id.is_none();
                use_incremental = previous_scan_id.is_some();
                let scan_type = if use_incremental {
                    "incremental"
                } else {
                    "full"
                };
                match cache.start_scan(scan_type, &categories) {
                    Ok(id) => scan_id = Some(id),
                    Err(e) => {
                        if mode != OutputMode::Quiet {
                            eprintln!(
                                "Warning: Failed to start scan cache session: {}. Continuing without cache.",
                                e
                            );
                        }
                        use_incremental = false;
                    }
                }
            }
        }
    }

    // On first scan, optionally perform full-disk traversal to build a deep baseline (CLI mode).
    // Default is category-only scanning (faster/lighter).
    if is_first_scan && config.cache.full_disk_baseline && mode != OutputMode::Quiet {
        if let Some(cache) = scan_cache.as_mut() {
            if let Some(id) = scan_id {
                if let Err(e) = perform_full_disk_traversal_cli_grouped(path, config, cache, id) {
                    eprintln!(
                        "Warning: Full disk traversal failed: {}. Continuing with category scans.",
                        e
                    );
                }
            }
        }
    }

    // Create spinner for visual feedback (unless quiet mode)
    let spinner = if mode != OutputMode::Quiet {
        Some(progress::create_spinner("Scanning..."))
    } else {
        None
    };

    // Progress counter for parallel tasks
    let scanned_count = AtomicUsize::new(0);
    let path_owned = path.to_path_buf();

    // Clone configs for use in parallel closure (needs to be Send + Sync)
    let build_config = config.categories.build.clone();
    let duplicates_config = config.categories.duplicates.clone();
    let config_clone = config.clone(); // Clone full config for parallel access

    // Store duplicate groups separately (needs to be stored after scan)
    use std::cell::RefCell;
    let duplicate_groups: RefCell<Option<Vec<crate::categories::duplicates::DuplicateGroup>>> =
        RefCell::new(None);

    // Run scans sequentially to avoid disk thrashing and thread pool explosion
    // Each individual scanner (large, duplicates, build) manages its own parallelism
    // and uses the full system resources. Running them in parallel causes massive
    // I/O contention and "loading so bad" system freezes.
    let scan_results: Vec<(&str, Result<CategoryResult>)> = enabled
        .iter()
        .map(|(name, task)| {
            // Clone config for this task
            let config = &config_clone;

            // Update progress
            let count = scanned_count.fetch_add(1, Ordering::SeqCst) + 1;
            if let Some(ref sp) = spinner {
                sp.set_message(format!(
                    "Scanning {} ({}/{})...",
                    name, count, total_categories
                ));
            }

            // Show category header in Normal+ mode
            if mode != OutputMode::Quiet {
                println!();
                println!("{}", Theme::header(&format!("Scanning {}", name)));
            }

            // Try incremental scan if cache is available
            let result = if use_incremental {
                if let (Some(cache), Some(scan_session_id)) = (scan_cache.as_mut(), scan_id) {
                    // Attempt incremental scan (pass scan_session_id, not category scan_id)
                    match try_incremental_scan(
                        name,
                        task,
                        &path_owned,
                        config,
                        cache,
                        scan_session_id,
                        mode,
                    ) {
                        Ok(Some(cached_result)) => {
                            // Used cache successfully
                            Ok(cached_result)
                        }
                        Ok(None) => {
                            // Need to do full scan for this category
                            execute_category_scan(
                                name,
                                task,
                                &path_owned,
                                config,
                                mode,
                                &build_config,
                                &duplicates_config,
                                &duplicate_groups,
                            )
                        }
                        Err(e) => {
                            // Cache error, fall back to full scan
                            if mode != OutputMode::Quiet {
                                eprintln!(
                                    "Warning: Cache error for {}: {}. Falling back to full scan.",
                                    name, e
                                );
                            }
                            execute_category_scan(
                                name,
                                task,
                                &path_owned,
                                config,
                                mode,
                                &build_config,
                                &duplicates_config,
                                &duplicate_groups,
                            )
                        }
                    }
                } else {
                    // Full scan (no cache or cache disabled)
                    execute_category_scan(
                        name,
                        task,
                        &path_owned,
                        config,
                        mode,
                        &build_config,
                        &duplicates_config,
                        &duplicate_groups,
                    )
                }
            } else {
                // Full scan (no cache or cache disabled)
                execute_category_scan(
                    name,
                    task,
                    &path_owned,
                    config,
                    mode,
                    &build_config,
                    &duplicates_config,
                    &duplicate_groups,
                )
            };

            (*name, result)
        })
        .collect();

    // Clear spinner
    if let Some(sp) = spinner {
        progress::finish_and_clear(&sp);
    }

    // Aggregate results
    for (category, result) in scan_results {
        match (category, result) {
            ("cache", Ok(r)) => results.cache = r,
            ("app_cache", Ok(r)) => results.app_cache = r,
            ("temp", Ok(r)) => results.temp = r,
            ("trash", Ok(r)) => results.trash = r,
            ("build", Ok(r)) => results.build = r,
            ("downloads", Ok(r)) => results.downloads = r,
            ("large", Ok(r)) => results.large = r,
            ("old", Ok(r)) => results.old = r,
            ("browser", Ok(r)) => results.browser = r,
            ("system", Ok(r)) => results.system = r,
            ("empty", Ok(r)) => results.empty = r,
            ("duplicates", Ok(r)) => {
                results.duplicates = r;
                // Store duplicate groups for enhanced display
                results.duplicates_groups = duplicate_groups.borrow().clone();
            }
            ("applications", Ok(r)) => results.applications = r,
            ("windows_update", Ok(r)) => results.windows_update = r,
            ("event_logs", Ok(r)) => results.event_logs = r,
            (name, Err(e)) => {
                if mode != OutputMode::Quiet {
                    eprintln!("[WARNING] {} scan failed: {}", name, e);
                }
            }
            _ => {}
        }
    }

    // Note: Exclusions are now handled during traversal in each scanner's filter_entry,
    // so filter_exclusions is no longer needed. However, we keep it as a safety net
    // for any paths that might have been missed (should be rare).
    // This can be removed entirely once we verify all scanners properly handle exclusions.
    filter_exclusions(&mut results, config);

    // Save scanned files to cache and finish scan session if cache is enabled (non-fatal if it fails)
    if let Some(cache) = scan_cache.as_mut() {
        if let Some(scan_session_id) = cache.current_scan_id() {
            // Save all scanned files to cache using per-category scan IDs
            // Group files by category and save each group with its category-specific scan ID
            let mut category_batches: std::collections::HashMap<
                String,
                Vec<(FileSignature, String)>,
            > = std::collections::HashMap::new();

            // Helper to add paths from a category result
            let mut add_category_paths = |paths: &[PathBuf], category: &str| {
                for path in paths {
                    if let Ok(sig) = FileSignature::from_path(path, false) {
                        category_batches
                            .entry(category.to_string())
                            .or_default()
                            .push((sig, category.to_string()));
                    }
                }
            };

            add_category_paths(&results.cache.paths, "cache");
            add_category_paths(&results.app_cache.paths, "app_cache");
            add_category_paths(&results.temp.paths, "temp");
            add_category_paths(&results.trash.paths, "trash");
            add_category_paths(&results.build.paths, "build");
            add_category_paths(&results.downloads.paths, "downloads");
            add_category_paths(&results.large.paths, "large");
            add_category_paths(&results.old.paths, "old");
            add_category_paths(&results.browser.paths, "browser");
            add_category_paths(&results.system.paths, "system");
            add_category_paths(&results.empty.paths, "empty");
            add_category_paths(&results.duplicates.paths, "duplicates");
            add_category_paths(&results.applications.paths, "applications");
            add_category_paths(&results.windows_update.paths, "windows_update");
            add_category_paths(&results.event_logs.paths, "event_logs");

            // Save each category's files with its category-specific scan ID
            for (category, files) in category_batches {
                if !files.is_empty() {
                    // Get category scan ID (increments if category was scanned before)
                    if let Ok(category_scan_id) =
                        cache.get_category_scan_id(&category, scan_session_id)
                    {
                        let _ = cache.upsert_files_batch(&files, category_scan_id);
                    }
                }
            }

            let total_files = results.cache.items
                + results.app_cache.items
                + results.temp.items
                + results.trash.items
                + results.build.items
                + results.downloads.items
                + results.large.items
                + results.old.items
                + results.applications.items
                + results.browser.items
                + results.system.items
                + results.empty.items
                + results.duplicates.items
                + results.windows_update.items
                + results.event_logs.items;

            let removed = cache.cleanup_stale(scan_session_id).unwrap_or(0);
            let stats = ScanStats {
                total_files,
                removed_files: removed,
                ..Default::default()
            };
            let _ = cache.finish_scan(scan_session_id, stats);
        }
    }

    Ok(results)
}

/// Perform full disk traversal for first scan (CLI version with single-line updates)
fn perform_full_disk_traversal_cli_grouped(
    root_path: &Path,
    config: &Config,
    scan_cache: &mut ScanCache,
    scan_id: i64,
) -> Result<()> {
    use std::collections::HashMap;
    use std::io::{self, Write};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Mutex;
    use std::time::{Duration, Instant};
    use walkdir::WalkDir;

    let files_processed = AtomicU64::new(0);
    let cache_updates: Mutex<Vec<(crate::scan_cache::FileSignature, String)>> =
        Mutex::new(Vec::new());
    // Smaller batches smooth out CPU/disk spikes (lighter on the device).
    const BATCH_SIZE: usize = 500;
    const YIELD_EVERY_FILES: u64 = 10_000;
    const SLEEP_AFTER_BATCH: Duration = Duration::from_millis(5);
    let mut last_yield = Instant::now();

    // Track parent folders (top-level milestones) and their stats
    let mut parent_folders: HashMap<PathBuf, (usize, u64)> = HashMap::new(); // (file_count, total_size)
    let mut current_parent: Option<PathBuf> = None;
    let mut spinner_frame = 0u8;
    let mut last_update = std::time::Instant::now();
    const UPDATE_INTERVAL_MS: u64 = 100; // Update display every 100ms

    // Determine what constitutes a "parent folder" - use depth 1 or 2 from root
    const PARENT_DEPTH_THRESHOLD: usize = 2;

    // Helper to print/update a single line (overwrites previous line)
    let print_line = |message: &str| {
        print!("\r{}", message);
        // Clear to end of line to avoid leftover characters
        print!("\x1b[K");
        io::stdout().flush().ok();
    };

    // Helper to get spinner character
    let get_spinner = |frame: u8| -> char {
        match frame % 4 {
            0 => '⠋',
            1 => '⠙',
            2 => '⠹',
            3 => '⠸',
            _ => '⠋',
        }
    };

    // Use walkdir (sequential) for CLI to avoid thread safety issues
    for entry in WalkDir::new(root_path)
        .max_depth(20) // Reasonable depth limit
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            let entry_path = e.path();

            // Skip system paths
            if crate::utils::is_system_path(entry_path) {
                return false;
            }

            // Skip symlinks
            if e.file_type().is_symlink() {
                return false;
            }

            // Only check reparse points for directories (optimization: skip syscall for files)
            if e.file_type().is_dir() && crate::utils::is_windows_reparse_point(entry_path) {
                return false;
            }

            // Check exclusions
            if config.is_excluded(entry_path) {
                return false;
            }

            true
        })
    {
        match entry {
            Ok(e) => {
                let entry_path = e.path();
                let depth = e.depth();

                if e.file_type().is_dir() {
                    // Determine if this is a "parent folder" (milestone)
                    if depth <= PARENT_DEPTH_THRESHOLD {
                        // When we encounter a new parent folder, print summary of previous parent if it had files
                        if let Some(ref prev_parent) = current_parent {
                            if let Some((file_count, total_size)) = parent_folders.get(prev_parent)
                            {
                                if *file_count > 0 {
                                    // Move to new line and print summary
                                    println!();
                                    let folder_name = prev_parent
                                        .file_name()
                                        .and_then(|n| n.to_str())
                                        .map(|s| s.to_string())
                                        .unwrap_or_else(|| prev_parent.display().to_string());
                                    println!(
                                        "✓ Scanned {}: {} files, {}",
                                        folder_name,
                                        file_count,
                                        bytesize::to_string(*total_size, false)
                                    );
                                }
                            }
                        }

                        // Start new parent folder
                        current_parent = Some(entry_path.to_path_buf());
                        parent_folders.insert(entry_path.to_path_buf(), (0, 0));

                        // Print "Scanning {folder}" on single line
                        let folder_name = entry_path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| entry_path.display().to_string());
                        let spinner = get_spinner(spinner_frame);
                        print_line(&format!("{} Scanning {}", spinner, folder_name));
                        spinner_frame = spinner_frame.wrapping_add(1);
                    }
                } else if e.file_type().is_file() {
                    // Add file to current parent folder
                    if let Some(ref parent) = current_parent {
                        let (count, size) = parent_folders.entry(parent.clone()).or_insert((0, 0));

                        // Get file size
                        if let Ok(metadata) = std::fs::metadata(entry_path) {
                            let file_size = metadata.len();
                            *count += 1;
                            *size += file_size;

                            // Update display with file name (throttled)
                            if last_update.elapsed().as_millis() >= UPDATE_INTERVAL_MS as u128 {
                                let file_name = entry_path
                                    .file_name()
                                    .and_then(|n| n.to_str())
                                    .map(|s| s.to_string())
                                    .unwrap_or_else(|| entry_path.display().to_string());
                                let spinner = get_spinner(spinner_frame);
                                print_line(&format!("{} Reading {}", spinner, file_name));
                                spinner_frame = spinner_frame.wrapping_add(1);
                                last_update = std::time::Instant::now();
                            }
                        }
                    }

                    // Create file signature (no hash for first scan - too slow)
                    if let Ok(sig) = crate::scan_cache::FileSignature::from_path(entry_path, false)
                    {
                        // Use "baseline" as category for first scan files
                        let mut updates = cache_updates.lock().unwrap();
                        updates.push((sig, "baseline".to_string()));

                        // Batch update cache periodically
                        if updates.len() >= BATCH_SIZE {
                            let batch = std::mem::take(&mut *updates);
                            if let Err(e) = scan_cache.upsert_files_batch(&batch, scan_id) {
                                eprintln!("\nWarning: Failed to update cache batch: {}", e);
                            }
                            // Light throttling: smooth CPU spikes around DB flushes.
                            std::thread::sleep(SLEEP_AFTER_BATCH);
                        }

                        let processed = files_processed.fetch_add(1, Ordering::Relaxed) + 1;
                        if processed.is_multiple_of(YIELD_EVERY_FILES)
                            && last_yield.elapsed() >= Duration::from_millis(50)
                        {
                            std::thread::yield_now();
                            last_yield = Instant::now();
                        }
                    }
                }
            }
            Err(_) => {
                // Skip errors during traversal
            }
        }
    }

    // Print final parent folder summary
    if let Some(ref parent) = current_parent {
        if let Some((file_count, total_size)) = parent_folders.get(parent) {
            if *file_count > 0 {
                // Move to new line and print summary
                println!();
                let folder_name = parent
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| parent.display().to_string());
                println!(
                    "✓ Scanned {}: {} files, {}",
                    folder_name,
                    file_count,
                    bytesize::to_string(*total_size, false)
                );
            }
        }
    }

    // Clear the progress line
    print_line("");
    println!();

    // Flush remaining cache updates
    let remaining = cache_updates.into_inner().unwrap();
    if !remaining.is_empty() {
        scan_cache.upsert_files_batch(&remaining, scan_id)?;
        std::thread::sleep(SLEEP_AFTER_BATCH);
    }

    Ok(())
}

/// Perform full disk traversal for first scan (TUI version with progress events)
fn perform_full_disk_traversal(
    root_path: &Path,
    config: &Config,
    tx: &Sender<ScanProgressEvent>,
    scan_cache: &mut ScanCache,
    scan_id: i64,
) -> Result<()> {
    use std::time::{Duration, Instant};
    use walkdir::WalkDir;

    let mut cache_updates: Vec<(crate::scan_cache::FileSignature, String)> = Vec::new();
    // Smaller batches smooth out CPU/disk spikes (lighter on the device).
    const BATCH_SIZE: usize = 500;
    const EVENT_INTERVAL: Duration = Duration::from_millis(100); // Throttle UI events (reduced overhead for large scans)
    const YIELD_EVERY_FILES: u64 = 10_000;
    const SLEEP_AFTER_BATCH: Duration = Duration::from_millis(5);
    let mut last_event = Instant::now();
    let mut processed_files: u64 = 0;
    let mut last_yield = Instant::now();

    // Use walkdir (sequential) for TUI to avoid lifetime issues
    for entry in WalkDir::new(root_path)
        .max_depth(20) // Reasonable depth limit
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            let entry_path = e.path();

            // Skip system paths
            if crate::utils::is_system_path(entry_path) {
                return false;
            }

            // Skip symlinks
            if e.file_type().is_symlink() {
                return false;
            }

            // Only check reparse points for directories (optimization: skip syscall for files)
            if e.file_type().is_dir() && crate::utils::is_windows_reparse_point(entry_path) {
                return false;
            }

            // Check exclusions
            if config.is_excluded(entry_path) {
                return false;
            }

            true
        })
    {
        match entry {
            Ok(e) => {
                let entry_path = e.path();

                if e.file_type().is_dir() {
                    // Emit folder reading event (throttled)
                    if last_event.elapsed() >= EVENT_INTERVAL {
                        let _ = tx.send(ScanProgressEvent::ReadingFolder {
                            path: entry_path.to_path_buf(),
                        });
                        last_event = Instant::now();
                    }
                } else if e.file_type().is_file() {
                    // Emit file reading event (throttled)
                    if last_event.elapsed() >= EVENT_INTERVAL {
                        let _ = tx.send(ScanProgressEvent::ReadingFile {
                            path: entry_path.to_path_buf(),
                        });
                        last_event = Instant::now();
                    }

                    // Create file signature (no hash for first scan - too slow)
                    if let Ok(sig) = crate::scan_cache::FileSignature::from_path(entry_path, false)
                    {
                        // Use "baseline" as category for first scan files
                        cache_updates.push((sig, "baseline".to_string()));

                        // Batch update cache periodically
                        if cache_updates.len() >= BATCH_SIZE {
                            let batch = std::mem::take(&mut cache_updates);
                            if let Err(e) = scan_cache.upsert_files_batch(&batch, scan_id) {
                                eprintln!("Warning: Failed to update cache batch: {}", e);
                            }
                            // Light throttling: smooth CPU spikes around DB flushes.
                            std::thread::sleep(SLEEP_AFTER_BATCH);
                        }

                        processed_files += 1;
                        if processed_files.is_multiple_of(YIELD_EVERY_FILES)
                            && last_yield.elapsed() >= Duration::from_millis(50)
                        {
                            std::thread::yield_now();
                            last_yield = Instant::now();
                        }
                    }
                }
            }
            Err(_) => {
                // Skip errors during traversal
            }
        }
    }

    // Flush remaining cache updates
    if !cache_updates.is_empty() {
        scan_cache.upsert_files_batch(&cache_updates, scan_id)?;
        std::thread::sleep(SLEEP_AFTER_BATCH);
    }

    Ok(())
}

/// Scan all requested categories and emit progress events for TUI.
pub fn scan_all_with_progress(
    path: &Path,
    options: ScanOptions,
    config: &Config,
    tx: &Sender<ScanProgressEvent>,
    mut scan_cache: Option<&mut ScanCache>,
) -> Result<ScanResults> {
    // Clear git cache for fresh scan
    git::clear_cache();

    let mut results = ScanResults::default();

    #[derive(Clone, Copy)]
    struct ScanJob {
        key: &'static str,
        display: &'static str,
        task: ScanTask,
    }

    let mut enabled: Vec<ScanJob> = Vec::new();

    if options.cache {
        enabled.push(ScanJob {
            key: "cache",
            display: "Package Cache",
            task: ScanTask::Cache,
        });
    }
    if options.app_cache {
        enabled.push(ScanJob {
            key: "app_cache",
            display: "Application Cache",
            task: ScanTask::AppCache,
        });
    }
    if options.temp {
        enabled.push(ScanJob {
            key: "temp",
            display: "Temp Files",
            task: ScanTask::Temp,
        });
    }
    if options.trash {
        enabled.push(ScanJob {
            key: "trash",
            display: "Trash",
            task: ScanTask::Trash,
        });
    }
    if options.build {
        enabled.push(ScanJob {
            key: "build",
            display: "Build Artifacts",
            task: ScanTask::Build(options.project_age_days),
        });
    }
    if options.downloads {
        enabled.push(ScanJob {
            key: "downloads",
            display: "Old Downloads",
            task: ScanTask::Downloads(options.min_age_days),
        });
    }
    if options.large {
        enabled.push(ScanJob {
            key: "large",
            display: "Large Files",
            task: ScanTask::Large(options.min_size_bytes),
        });
    }
    if options.old {
        enabled.push(ScanJob {
            key: "old",
            display: "Old Files",
            task: ScanTask::Old(options.min_age_days),
        });
    }
    if options.browser {
        enabled.push(ScanJob {
            key: "browser",
            display: "Browser Cache",
            task: ScanTask::Browser,
        });
    }
    if options.system {
        enabled.push(ScanJob {
            key: "system",
            display: "System Cache",
            task: ScanTask::System,
        });
    }
    if options.empty {
        enabled.push(ScanJob {
            key: "empty",
            display: "Empty Folders",
            task: ScanTask::Empty,
        });
    }
    if options.duplicates {
        enabled.push(ScanJob {
            key: "duplicates",
            display: "Duplicates",
            task: ScanTask::Duplicates,
        });
    }
    if options.applications {
        enabled.push(ScanJob {
            key: "applications",
            display: "Installed Applications",
            task: ScanTask::Applications,
        });
    }
    if options.windows_update {
        enabled.push(ScanJob {
            key: "windows_update",
            display: "Windows Update",
            task: ScanTask::WindowsUpdate,
        });
    }
    if options.event_logs {
        enabled.push(ScanJob {
            key: "event_logs",
            display: "Event Logs",
            task: ScanTask::EventLogs,
        });
    }

    if enabled.is_empty() {
        return Ok(results);
    }

    // Check if this is a first scan and perform full disk traversal BEFORE category scans
    let is_first_scan = if let Some(cache) = scan_cache.as_ref() {
        matches!(cache.get_previous_scan_id(), Ok(None))
    } else {
        false
    };

    // Start scan session if cache is enabled
    let mut scan_id: Option<i64> = None;
    let cache_enabled = scan_cache.is_some() && config.cache.enabled;

    if let Some(cache) = scan_cache.as_mut() {
        if cache_enabled {
            let categories: Vec<&str> = enabled.iter().map(|job| job.key).collect();
            let scan_type = if is_first_scan { "full" } else { "incremental" };
            match cache.start_scan(scan_type, &categories) {
                Ok(id) => scan_id = Some(id),
                Err(e) => {
                    eprintln!("Warning: Failed to start scan cache session: {}. Continuing without cache.", e);
                    // Cache error - will continue without cache
                }
            }
        }
    }

    // On first scan, optionally perform full disk traversal to build baseline BEFORE category scans
    // (disabled by default; enable via config.cache.full_disk_baseline).
    if is_first_scan && config.cache.full_disk_baseline {
        if let Some(cache) = scan_cache.as_mut() {
            if let Some(id) = scan_id {
                // Perform full disk traversal with progress reporting
                if let Err(e) = perform_full_disk_traversal(path, config, tx, cache, id) {
                    eprintln!(
                        "Warning: Full disk traversal failed: {}. Continuing with category scans.",
                        e
                    );
                }
            }
        }
    }

    let path_owned = path.to_path_buf();

    // Clone configs for use in scan tasks
    let build_config = config.categories.build.clone();
    let duplicates_config = config.categories.duplicates.clone();

    // Store duplicate groups separately (needs to be stored after scan)
    use std::cell::RefCell;
    let duplicate_groups: RefCell<Option<Vec<crate::categories::duplicates::DuplicateGroup>>> =
        RefCell::new(None);

    let scan_results: Vec<(&str, &str, Result<CategoryResult>)> = enabled
        .iter()
        .map(|job| {
            let display = job.display;

            let send_started = || {
                let _ = tx.send(ScanProgressEvent::CategoryStarted {
                    category: display.to_string(),
                    total_units: None,
                    current_path: None,
                });
            };

            let result = match job.task {
                ScanTask::Cache => categories::cache::scan_with_progress(&path_owned, config, tx),
                ScanTask::AppCache => {
                    categories::app_cache::scan_with_progress(&path_owned, config, tx)
                }
                ScanTask::Temp => categories::temp::scan_with_progress(&path_owned, config, tx),
                ScanTask::Trash => {
                    send_started();
                    categories::trash::scan()
                }
                ScanTask::Build(age) => {
                    send_started();
                    categories::build::scan(
                        &path_owned,
                        age,
                        Some(&build_config),
                        config,
                        OutputMode::Quiet,
                    )
                }
                ScanTask::Downloads(age) => {
                    send_started();
                    categories::downloads::scan(&path_owned, age, config, OutputMode::Quiet)
                }
                ScanTask::Large(size) => {
                    send_started();
                    categories::large::scan(&path_owned, size, config, OutputMode::Quiet)
                }
                ScanTask::Old(age) => {
                    send_started();
                    categories::old::scan(&path_owned, age, config, OutputMode::Quiet)
                }
                ScanTask::Browser => {
                    send_started();
                    categories::browser::scan(&path_owned, config)
                }
                ScanTask::System => {
                    send_started();
                    categories::system::scan(&path_owned, config)
                }
                ScanTask::Empty => categories::empty::scan_with_progress(&path_owned, config, tx),
                ScanTask::Duplicates => {
                    send_started();
                    match categories::duplicates::scan_with_config(
                        &path_owned,
                        Some(&duplicates_config),
                        config,
                    ) {
                        Ok(dup_result) => {
                            *duplicate_groups.borrow_mut() = Some(dup_result.groups.clone());
                            Ok(dup_result.to_category_result())
                        }
                        Err(e) => Err(e),
                    }
                }
                ScanTask::Applications => {
                    categories::applications::scan_with_progress(&path_owned, config, tx)
                }
                ScanTask::WindowsUpdate => {
                    send_started();
                    categories::windows_update::scan(&path_owned, config)
                }
                ScanTask::EventLogs => {
                    send_started();
                    categories::event_logs::scan(&path_owned, config)
                }
            };

            if let Ok(ref category_result) = result {
                if !matches!(
                    job.task,
                    ScanTask::Cache | ScanTask::AppCache | ScanTask::Temp | ScanTask::Applications
                ) {
                    let _ = tx.send(ScanProgressEvent::CategoryFinished {
                        category: display.to_string(),
                        items: category_result.items,
                        size_bytes: category_result.size_bytes,
                    });
                }
            } else if !matches!(
                job.task,
                ScanTask::Cache | ScanTask::AppCache | ScanTask::Temp | ScanTask::Applications
            ) {
                let _ = tx.send(ScanProgressEvent::CategoryFinished {
                    category: display.to_string(),
                    items: 0,
                    size_bytes: 0,
                });
            }

            (job.key, display, result)
        })
        .collect();

    for (category, _display, result) in scan_results {
        match (category, result) {
            ("cache", Ok(r)) => results.cache = r,
            ("app_cache", Ok(r)) => results.app_cache = r,
            ("temp", Ok(r)) => results.temp = r,
            ("trash", Ok(r)) => results.trash = r,
            ("build", Ok(r)) => results.build = r,
            ("downloads", Ok(r)) => results.downloads = r,
            ("large", Ok(r)) => results.large = r,
            ("old", Ok(r)) => results.old = r,
            ("browser", Ok(r)) => results.browser = r,
            ("system", Ok(r)) => results.system = r,
            ("empty", Ok(r)) => results.empty = r,
            ("duplicates", Ok(r)) => {
                results.duplicates = r;
                results.duplicates_groups = duplicate_groups.borrow().clone();
            }
            ("applications", Ok(r)) => results.applications = r,
            ("windows_update", Ok(r)) => results.windows_update = r,
            ("event_logs", Ok(r)) => results.event_logs = r,
            (_name, Err(_e)) => {}
            _ => {}
        }
    }

    filter_exclusions(&mut results, config);

    // Save scanned files to cache and finish scan session if cache is enabled (non-fatal if it fails)
    if let Some(cache) = scan_cache.as_mut() {
        if let Some(scan_session_id) = cache.current_scan_id() {
            // Save all scanned files to cache using per-category scan IDs
            // Group files by category and save each group with its category-specific scan ID
            let mut category_batches: std::collections::HashMap<
                String,
                Vec<(FileSignature, String)>,
            > = std::collections::HashMap::new();

            // Helper to add paths from a category result
            let mut add_category_paths = |paths: &[PathBuf], category: &str| {
                for path in paths {
                    if let Ok(sig) = FileSignature::from_path(path, false) {
                        category_batches
                            .entry(category.to_string())
                            .or_default()
                            .push((sig, category.to_string()));
                    }
                }
            };

            add_category_paths(&results.cache.paths, "cache");
            add_category_paths(&results.app_cache.paths, "app_cache");
            add_category_paths(&results.temp.paths, "temp");
            add_category_paths(&results.trash.paths, "trash");
            add_category_paths(&results.build.paths, "build");
            add_category_paths(&results.downloads.paths, "downloads");
            add_category_paths(&results.large.paths, "large");
            add_category_paths(&results.old.paths, "old");
            add_category_paths(&results.browser.paths, "browser");
            add_category_paths(&results.system.paths, "system");
            add_category_paths(&results.empty.paths, "empty");
            add_category_paths(&results.duplicates.paths, "duplicates");
            add_category_paths(&results.applications.paths, "applications");
            add_category_paths(&results.windows_update.paths, "windows_update");
            add_category_paths(&results.event_logs.paths, "event_logs");

            // Save each category's files with its category-specific scan ID
            for (category, files) in category_batches {
                if !files.is_empty() {
                    // Get category scan ID (increments if category was scanned before)
                    if let Ok(category_scan_id) =
                        cache.get_category_scan_id(&category, scan_session_id)
                    {
                        let _ = cache.upsert_files_batch(&files, category_scan_id);
                    }
                }
            }

            let total_files = results.cache.items
                + results.app_cache.items
                + results.temp.items
                + results.trash.items
                + results.build.items
                + results.downloads.items
                + results.large.items
                + results.old.items
                + results.applications.items
                + results.browser.items
                + results.system.items
                + results.empty.items
                + results.duplicates.items
                + results.windows_update.items
                + results.event_logs.items;

            // Cleanup and finish are non-fatal - scan already completed
            let removed = cache.cleanup_stale(scan_session_id).unwrap_or(0);
            let stats = ScanStats {
                total_files,
                removed_files: removed,
                ..Default::default()
            };
            let _ = cache.finish_scan(scan_session_id, stats);
        }
    }

    Ok(results)
}

/// Scan task enum for parallel execution
#[derive(Clone, Copy)]
enum ScanTask {
    Cache,
    AppCache,
    Temp,
    Trash,
    Build(u64),
    Downloads(u64),
    Large(u64),
    Old(u64),
    Browser,
    System,
    Empty,
    Duplicates,
    Applications,
    WindowsUpdate,
    EventLogs,
}

/// Filter out paths matching exclusion patterns
///
/// Optimized to avoid recalculating sizes - uses pre-calculated sizes from scan results
fn filter_exclusions(results: &mut ScanResults, config: &Config) {
    // Helper to filter paths and recalculate size_bytes efficiently
    let filter_and_recalculate = |paths: &mut Vec<std::path::PathBuf>, size_bytes: &mut u64| {
        let original_count = paths.len();
        let mut excluded_size = 0u64;

        // Filter out excluded paths and track their sizes
        paths.retain(|path| {
            let is_excluded = config.is_excluded(path);
            if is_excluded {
                // Calculate size of excluded path before removing
                if let Ok(metadata) = std::fs::metadata(path) {
                    if metadata.is_file() {
                        excluded_size += metadata.len();
                    } else if metadata.is_dir() {
                        excluded_size += utils::calculate_dir_size(path);
                    }
                }
            }
            !is_excluded
        });

        // Subtract excluded sizes instead of recalculating everything
        if excluded_size > 0 {
            *size_bytes = size_bytes.saturating_sub(excluded_size);
        }

        // If we filtered out many paths, the size estimate might be off
        // Only recalculate if we filtered out a significant portion (>10%)
        if original_count > 0 && (original_count - paths.len()) * 100 / original_count > 10 {
            // Recalculate for accuracy when many paths were excluded
            *size_bytes = 0;
            for path in paths.iter() {
                if let Ok(metadata) = std::fs::metadata(path) {
                    if metadata.is_file() {
                        *size_bytes += metadata.len();
                    } else if metadata.is_dir() {
                        *size_bytes += utils::calculate_dir_size(path);
                    }
                }
            }
        }
    };

    filter_and_recalculate(&mut results.cache.paths, &mut results.cache.size_bytes);
    filter_and_recalculate(
        &mut results.app_cache.paths,
        &mut results.app_cache.size_bytes,
    );
    filter_and_recalculate(&mut results.temp.paths, &mut results.temp.size_bytes);
    filter_and_recalculate(&mut results.trash.paths, &mut results.trash.size_bytes);
    filter_and_recalculate(&mut results.build.paths, &mut results.build.size_bytes);
    filter_and_recalculate(
        &mut results.downloads.paths,
        &mut results.downloads.size_bytes,
    );
    filter_and_recalculate(&mut results.large.paths, &mut results.large.size_bytes);
    filter_and_recalculate(&mut results.old.paths, &mut results.old.size_bytes);
    filter_and_recalculate(&mut results.browser.paths, &mut results.browser.size_bytes);
    filter_and_recalculate(&mut results.system.paths, &mut results.system.size_bytes);
    filter_and_recalculate(&mut results.empty.paths, &mut results.empty.size_bytes);
    filter_and_recalculate(
        &mut results.duplicates.paths,
        &mut results.duplicates.size_bytes,
    );
    filter_and_recalculate(
        &mut results.applications.paths,
        &mut results.applications.size_bytes,
    );

    // Recalculate item counts after filtering
    results.cache.items = results.cache.paths.len();
    results.app_cache.items = results.app_cache.paths.len();
    results.temp.items = results.temp.paths.len();
    results.trash.items = results.trash.paths.len();
    results.build.items = results.build.paths.len();
    results.downloads.items = results.downloads.paths.len();
    results.large.items = results.large.paths.len();
    results.old.items = results.old.paths.len();
    results.browser.items = results.browser.paths.len();
    results.system.items = results.system.paths.len();
    results.empty.items = results.empty.paths.len();
    results.duplicates.items = results.duplicates.paths.len();
    results.applications.items = results.applications.paths.len();
    results.windows_update.items = results.windows_update.paths.len();
    results.event_logs.items = results.event_logs.paths.len();
}

/// Calculate total size of paths (files only - not used for directories)
/// NOTE: This function is no longer used since each scanner calculates sizes correctly
#[allow(dead_code)]
fn calculate_total_size(paths: &[std::path::PathBuf]) -> u64 {
    paths
        .iter()
        .filter_map(|p| std::fs::metadata(p).ok())
        .map(|m| m.len())
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::ScanOptions;
    use crate::config::Config;
    use crate::output::OutputMode;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn create_test_dir() -> TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn test_scan_all_no_categories() {
        let temp_dir = create_test_dir();
        let options = ScanOptions {
            cache: false,
            app_cache: false,
            temp: false,
            trash: false,
            build: false,
            downloads: false,
            large: false,
            old: false,
            applications: false,
            browser: false,
            system: false,
            empty: false,
            duplicates: false,
            windows_update: false,
            event_logs: false,
            project_age_days: 14,
            min_age_days: 30,
            min_size_bytes: 100 * 1024 * 1024,
        };
        let config = Config::default();

        // Use Quiet mode in tests to avoid spinner thread issues
        let results = scan_all(temp_dir.path(), options, OutputMode::Quiet, &config, None).unwrap();

        assert_eq!(results.cache.items, 0);
        assert_eq!(results.temp.items, 0);
        assert_eq!(results.build.items, 0);
    }

    #[test]
    fn test_filter_exclusions() {
        let mut results = ScanResults::default();
        let mut config = Config::default();

        // Add some test paths
        results
            .cache
            .paths
            .push(PathBuf::from("C:/Users/test/important-project/file.txt"));
        results
            .cache
            .paths
            .push(PathBuf::from("C:/Users/test/normal/file.txt"));
        results.cache.items = 2;
        results.cache.size_bytes = 1000;

        // Add exclusion pattern
        config
            .exclusions
            .patterns
            .push("**/important-project/**".to_string());

        // Filter exclusions
        filter_exclusions(&mut results, &config);

        // Should have filtered out the important-project path
        assert_eq!(results.cache.items, 1);
        assert_eq!(results.cache.paths.len(), 1);
        assert_eq!(
            results.cache.paths[0],
            PathBuf::from("C:/Users/test/normal/file.txt")
        );
    }

    #[test]
    fn test_calculate_total_size() {
        let temp_dir = create_test_dir();
        let file1 = temp_dir.path().join("file1.txt");
        let file2 = temp_dir.path().join("file2.txt");

        fs::write(&file1, "hello").unwrap();
        fs::write(&file2, "world").unwrap();

        let paths = vec![file1, file2];
        let total = calculate_total_size(&paths);

        assert_eq!(total, 10); // 5 bytes + 5 bytes
    }
}
