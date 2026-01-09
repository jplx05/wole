use crate::categories;
use crate::cli::ScanOptions;
use crate::config::Config;
use crate::git;
use crate::output::{OutputMode, ScanResults, CategoryResult};
use crate::progress;
use anyhow::Result;
use colored::*;
use rayon::prelude::*;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Scan all requested categories and return aggregated results
/// 
/// Optimizations:
/// - Clears git cache before scanning for fresh results
/// - Scans categories in parallel using rayon (2-3x faster on multi-core)
/// - Handles errors gracefully - if one category fails, others continue
/// - Filters out paths matching exclusion patterns from config
pub fn scan_all(path: &Path, options: ScanOptions, mode: OutputMode, config: &Config) -> Result<ScanResults> {
    // Clear git cache for fresh scan
    git::clear_cache();
    
    let mut results = ScanResults::default();
    
    // Build list of enabled categories
    let mut enabled: Vec<(&str, ScanTask)> = Vec::new();
    
    if options.cache {
        enabled.push(("cache", ScanTask::Cache));
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
    
    // Run scans in parallel using rayon
    let scan_results: Vec<(&str, Result<CategoryResult>)> = enabled
        .par_iter()
        .map(|(name, task)| {
            // Update progress
            let count = scanned_count.fetch_add(1, Ordering::SeqCst) + 1;
            if let Some(ref sp) = spinner {
                sp.set_message(format!("Scanning {} ({}/{})...", name, count, total_categories));
            }
            
            // Execute scan
            let result = match task {
                ScanTask::Cache => categories::cache::scan(&path_owned),
                ScanTask::Temp => categories::temp::scan(&path_owned),
                ScanTask::Trash => categories::trash::scan(),
                ScanTask::Build(age) => categories::build::scan(&path_owned, *age),
                ScanTask::Downloads(age) => categories::downloads::scan(&path_owned, *age),
                ScanTask::Large(size) => categories::large::scan(&path_owned, *size),
                ScanTask::Old(age) => categories::old::scan(&path_owned, *age),
                ScanTask::Browser => categories::browser::scan(&path_owned),
                ScanTask::System => categories::system::scan(&path_owned),
                ScanTask::Empty => categories::empty::scan(&path_owned),
                ScanTask::Duplicates => {
                    // Duplicates returns a special result type
                    match categories::duplicates::scan(&path_owned) {
                        Ok(dup_result) => Ok(dup_result.to_category_result()),
                        Err(e) => Err(e),
                    }
                }
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
            ("temp", Ok(r)) => results.temp = r,
            ("trash", Ok(r)) => results.trash = r,
            ("build", Ok(r)) => results.build = r,
            ("downloads", Ok(r)) => results.downloads = r,
            ("large", Ok(r)) => results.large = r,
            ("old", Ok(r)) => results.old = r,
            ("browser", Ok(r)) => results.browser = r,
            ("system", Ok(r)) => results.system = r,
            ("empty", Ok(r)) => results.empty = r,
            ("duplicates", Ok(r)) => results.duplicates = r,
            (name, Err(e)) => {
                if mode != OutputMode::Quiet {
                    eprintln!("{} {} scan failed: {}", "Warning:".yellow(), name, e);
                }
            }
            _ => {}
        }
    }
    
    // Filter out excluded paths
    filter_exclusions(&mut results, config);
    
    Ok(results)
}

/// Scan task enum for parallel execution
#[derive(Clone, Copy)]
enum ScanTask {
    Cache,
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
}

/// Filter out paths matching exclusion patterns
fn filter_exclusions(results: &mut ScanResults, config: &Config) {
    let filter_paths = |paths: &mut Vec<std::path::PathBuf>| {
        paths.retain(|path| !config.is_excluded(path));
    };
    
    filter_paths(&mut results.cache.paths);
    filter_paths(&mut results.temp.paths);
    filter_paths(&mut results.trash.paths);
    filter_paths(&mut results.build.paths);
    filter_paths(&mut results.downloads.paths);
    filter_paths(&mut results.large.paths);
    filter_paths(&mut results.old.paths);
    filter_paths(&mut results.browser.paths);
    filter_paths(&mut results.system.paths);
    filter_paths(&mut results.empty.paths);
    filter_paths(&mut results.duplicates.paths);
    
    // Recalculate item counts and sizes after filtering
    results.cache.items = results.cache.paths.len();
    results.cache.size_bytes = calculate_total_size(&results.cache.paths);
    
    results.temp.items = results.temp.paths.len();
    results.temp.size_bytes = calculate_total_size(&results.temp.paths);
    
    results.trash.items = results.trash.paths.len();
    // Note: trash.size_bytes remains 0 as size calculation is intentionally skipped
    // (would require reading from Recycle Bin which is expensive)
    
    results.build.items = results.build.paths.len();
    results.build.size_bytes = calculate_total_size(&results.build.paths);
    
    results.downloads.items = results.downloads.paths.len();
    results.downloads.size_bytes = calculate_total_size(&results.downloads.paths);
    
    results.large.items = results.large.paths.len();
    results.large.size_bytes = calculate_total_size(&results.large.paths);
    
    results.old.items = results.old.paths.len();
    results.old.size_bytes = calculate_total_size(&results.old.paths);
    
    results.browser.items = results.browser.paths.len();
    results.browser.size_bytes = calculate_total_size(&results.browser.paths);
    
    results.system.items = results.system.paths.len();
    results.system.size_bytes = calculate_total_size(&results.system.paths);
    
    results.empty.items = results.empty.paths.len();
    results.empty.size_bytes = calculate_total_size(&results.empty.paths);
    
    results.duplicates.items = results.duplicates.paths.len();
    results.duplicates.size_bytes = calculate_total_size(&results.duplicates.paths);
}

/// Calculate total size of paths
fn calculate_total_size(paths: &[std::path::PathBuf]) -> u64 {
    paths.iter()
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
            temp: false,
            trash: false,
            build: false,
            downloads: false,
            large: false,
            old: false,
            browser: false,
            system: false,
            empty: false,
            duplicates: false,
            project_age_days: 14,
            min_age_days: 30,
            min_size_bytes: 100 * 1024 * 1024,
        };
        let config = Config::default();
        
        let results = scan_all(temp_dir.path(), options, OutputMode::Normal, &config).unwrap();
        
        assert_eq!(results.cache.items, 0);
        assert_eq!(results.temp.items, 0);
        assert_eq!(results.build.items, 0);
    }
    
    #[test]
    fn test_filter_exclusions() {
        let mut results = ScanResults::default();
        let mut config = Config::default();
        
        // Add some test paths
        results.cache.paths.push(PathBuf::from("C:/Users/test/important-project/file.txt"));
        results.cache.paths.push(PathBuf::from("C:/Users/test/normal/file.txt"));
        results.cache.items = 2;
        results.cache.size_bytes = 1000;
        
        // Add exclusion pattern
        config.exclusions.patterns.push("**/important-project/**".to_string());
        
        // Filter exclusions
        filter_exclusions(&mut results, &config);
        
        // Should have filtered out the important-project path
        assert_eq!(results.cache.items, 1);
        assert_eq!(results.cache.paths.len(), 1);
        assert_eq!(results.cache.paths[0], PathBuf::from("C:/Users/test/normal/file.txt"));
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
