use crate::categories;
use crate::cli::ScanOptions;
use crate::config::Config;
use crate::git;
use crate::output::{CategoryResult, OutputMode, ScanResults};
use crate::progress;
use crate::scan_events::ScanProgressEvent;
use crate::theme::Theme;
use crate::utils;
use anyhow::Result;
// use rayon::prelude::*; // Disabled: using sequential scan to avoid thrashing
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::Sender;

/// Scan all requested categories and return aggregated results
///
/// Optimizations:
/// - Clears git cache before scanning for fresh results
/// - Scans categories in parallel using rayon (2-3x faster on multi-core)
/// - Handles errors gracefully - if one category fails, others continue
/// - Filters out paths matching exclusion patterns from config
pub fn scan_all(
    path: &Path,
    options: ScanOptions,
    mode: OutputMode,
    config: &Config,
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

            // Execute scan - pass config to all scanners for exclusion filtering
            let result = match task {
                ScanTask::Cache => categories::cache::scan(&path_owned, config, mode),
                ScanTask::AppCache => categories::app_cache::scan(&path_owned, config, mode),
                ScanTask::Temp => categories::temp::scan(&path_owned, config),
                ScanTask::Trash => categories::trash::scan(),
                ScanTask::Build(age) => {
                    categories::build::scan(&path_owned, *age, Some(&build_config), config, mode)
                }
                ScanTask::Downloads(age) => {
                    categories::downloads::scan(&path_owned, *age, config, mode)
                }
                ScanTask::Large(size) => categories::large::scan(&path_owned, *size, config, mode),
                ScanTask::Old(age) => categories::old::scan(&path_owned, *age, config, mode),
                ScanTask::Browser => categories::browser::scan(&path_owned, config),
                ScanTask::System => categories::system::scan(&path_owned, config),
                ScanTask::Empty => categories::empty::scan(&path_owned, config),
                ScanTask::Duplicates => {
                    // Duplicates returns a special result type
                    // Use scan_with_config to pass configuration
                    match categories::duplicates::scan_with_config(
                        &path_owned,
                        Some(&duplicates_config),
                        config,
                    ) {
                        Ok(dup_result) => {
                            // Store groups for enhanced display
                            *duplicate_groups.borrow_mut() = Some(dup_result.groups.clone());
                            Ok(dup_result.to_category_result())
                        }
                        Err(e) => Err(e),
                    }
                }
                ScanTask::Applications => categories::applications::scan(&path_owned, config, mode),
                ScanTask::WindowsUpdate => categories::windows_update::scan(&path_owned, config),
                ScanTask::EventLogs => categories::event_logs::scan(&path_owned, config),
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

    Ok(results)
}

/// Scan all requested categories and emit progress events for TUI.
pub fn scan_all_with_progress(
    path: &Path,
    options: ScanOptions,
    config: &Config,
    tx: &Sender<ScanProgressEvent>,
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
                ScanTask::Empty => {
                    send_started();
                    categories::empty::scan(&path_owned, config)
                }
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
        let results = scan_all(temp_dir.path(), options, OutputMode::Quiet, &config).unwrap();

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
