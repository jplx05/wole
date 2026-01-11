//! Disk usage analysis and reporting

use crate::cli::ScanOptions;
use crate::config::Config;
use crate::output::{CategoryResult, OutputMode};
use crate::progress;
use anyhow::Result;
use rayon::prelude::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Category of cleanable files
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Category {
    Cache,
    AppCache,
    Temp,
    Trash,
    Build,
    Downloads,
    Large,
    Old,
    Browser,
    System,
    Empty,
    Duplicates,
    Applications,
}

impl Category {
    pub fn display_name(&self) -> &'static str {
        match self {
            Category::Cache => "Cache",
            Category::AppCache => "Application Cache",
            Category::Temp => "Temp Files",
            Category::Trash => "Trash",
            Category::Build => "Build Artifacts",
            Category::Downloads => "Old Downloads",
            Category::Large => "Large Files",
            Category::Old => "Old Files",
            Category::Browser => "Browser Cache",
            Category::System => "System Cache",
            Category::Empty => "Empty Folders",
            Category::Duplicates => "Duplicates",
            Category::Applications => "Installed Applications",
        }
    }
}

/// Represents a single cleanable file or directory
#[derive(Debug, Clone)]
pub struct CleanableFile {
    pub path: PathBuf,
    pub size: u64,
    pub category: Category,
    pub reason: String,
    pub is_directory: bool,
}

/// Result of a scan operation
#[derive(Debug, Default)]
pub struct ScanResult {
    pub files: Vec<CleanableFile>,
    pub errors: Vec<String>,
}

impl ScanResult {
    pub fn new() -> Self {
        Self {
            files: Vec::new(),
            errors: Vec::new(),
        }
    }

    pub fn add_file(&mut self, file: CleanableFile) {
        self.files.push(file);
    }

    pub fn add_files(&mut self, files: Vec<CleanableFile>) {
        self.files.extend(files);
    }

    pub fn add_error(&mut self, error: String) {
        self.errors.push(error);
    }

    pub fn total_count(&self) -> usize {
        self.files.len()
    }

    pub fn total_size(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }

    pub fn by_category(&self) -> HashMap<Category, Vec<&CleanableFile>> {
        let mut groups: HashMap<Category, Vec<&CleanableFile>> = HashMap::new();
        for file in &self.files {
            groups.entry(file.category).or_default().push(file);
        }
        groups
    }
}

/// Trait for scanners that can identify cleanable files
pub trait Scanner: Send + Sync {
    fn name(&self) -> &'static str;
    fn category(&self) -> Category;
    fn scan(
        &self,
        path: &Path,
        options: &ScanOptions,
        config: &Config,
    ) -> Result<Vec<CleanableFile>>;
}

/// Run all enabled scanners and aggregate results
pub fn run_scan(path: &Path, options: &ScanOptions, config: &Config) -> Result<ScanResult> {
    // Clear git cache for fresh scan
    crate::git::clear_cache();

    let mut result = ScanResult::new();
    let mut scanners: Vec<Box<dyn Scanner>> = Vec::new();

    // Build list of scanners based on options
    if options.cache {
        scanners.push(Box::new(CacheScannerAdapter));
    }

    if options.app_cache {
        scanners.push(Box::new(AppCacheScannerAdapter));
    }

    if options.temp {
        scanners.push(Box::new(TempScannerAdapter));
    }

    if options.trash {
        scanners.push(Box::new(TrashScannerAdapter));
    }

    if options.build {
        scanners.push(Box::new(BuildScannerAdapter));
    }

    if options.downloads {
        scanners.push(Box::new(DownloadsScannerAdapter));
    }

    if options.large {
        scanners.push(Box::new(LargeScannerAdapter));
    }

    if options.old {
        scanners.push(Box::new(OldScannerAdapter));
    }

    if options.browser {
        scanners.push(Box::new(BrowserScannerAdapter));
    }

    if options.system {
        scanners.push(Box::new(SystemScannerAdapter));
    }

    if options.empty {
        scanners.push(Box::new(EmptyScannerAdapter));
    }

    if options.duplicates {
        scanners.push(Box::new(DuplicatesScannerAdapter));
    }

    if options.applications {
        scanners.push(Box::new(ApplicationsScannerAdapter));
    }

    if scanners.is_empty() {
        return Ok(result);
    }

    // Show progress
    let spinner = progress::create_spinner("Scanning for cleanable files...");

    // Run scanners in parallel
    let scan_results: Vec<(String, Result<Vec<CleanableFile>>)> = scanners
        .par_iter()
        .map(|scanner| {
            let name = scanner.name().to_string();
            let files = scanner.scan(path, options, config);
            (name, files)
        })
        .collect();

    // Aggregate results
    for (name, files_result) in scan_results {
        match files_result {
            Ok(files) => {
                result.add_files(files);
            }
            Err(e) => {
                result.add_error(format!("{}: {}", name, e));
            }
        }
    }

    progress::finish_and_clear(&spinner);

    // Deduplicate results (same path shouldn't appear twice)
    let mut seen_paths = std::collections::HashSet::new();
    result.files.retain(|f| seen_paths.insert(f.path.clone()));

    Ok(result)
}

/// Print a summary report of scan results
pub fn print_report(result: &ScanResult) {
    let by_category = result.by_category();

    // Calculate category totals
    let mut category_stats: Vec<(Category, usize, u64)> = by_category
        .iter()
        .map(|(cat, files)| {
            let count = files.len();
            let size: u64 = files.iter().map(|f| f.size).sum();
            (*cat, count, size)
        })
        .collect();

    // Sort by size descending
    category_stats.sort_by(|a, b| b.2.cmp(&a.2));

    // Print header
    println!();
    println!("Wole Scan Results");
    println!("{}", "=".repeat(60));
    println!();

    // Print category breakdown
    println!("{:<20} {:>10} {:>12}", "Category", "Files", "Size");
    println!("{}", "-".repeat(60));

    for (category, count, size) in &category_stats {
        println!(
            "{:<20} {:>10} {:>12}",
            category.display_name(),
            format_number(*count as u64),
            format_size(*size)
        );
    }

    println!("{}", "-".repeat(60));

    // Print total
    println!(
        "{:<20} {:>10} {:>12}",
        "Total",
        format_number(result.total_count() as u64),
        format_size(result.total_size())
    );

    // Print any errors
    if !result.errors.is_empty() {
        println!();
        println!(
            "[WARNING] {} scanner(s) encountered errors:",
            result.errors.len()
        );
        for error in &result.errors {
            println!("  {}", error);
        }
    }

    println!();
}

/// Print detailed breakdown of scan results
pub fn print_detailed_report(result: &ScanResult) {
    let by_category = result.by_category();

    // Sort categories by total size
    let mut categories: Vec<_> = by_category.iter().collect();
    categories.sort_by(|a, b| {
        let size_a: u64 = a.1.iter().map(|f| f.size).sum();
        let size_b: u64 = b.1.iter().map(|f| f.size).sum();
        size_b.cmp(&size_a)
    });

    println!();
    println!("Detailed Analysis");
    println!("{}", "=".repeat(60));
    println!();

    for (category, files) in categories {
        if files.is_empty() {
            continue;
        }

        let total_size: u64 = files.iter().map(|f| f.size).sum();
        print_category_header(category.display_name(), total_size, files.len());

        // Show top 5 largest items
        let mut sorted_files: Vec<_> = files.iter().collect();
        sorted_files.sort_by(|a, b| b.size.cmp(&a.size));

        for file in sorted_files.iter().take(5) {
            print_file_entry(&file.path, file.size, 1);
        }

        if files.len() > 5 {
            println!("  ...and {} more items...", files.len() - 5);
        }
    }

    println!("{}", "=".repeat(60));
    println!(
        "Total: {} items, {} reclaimable",
        result.total_count(),
        format_size(result.total_size())
    );
    println!();
}

/// Print JSON output of scan results
pub fn print_json_report(result: &ScanResult) -> Result<()> {
    let output = serde_json::json!({
        "summary": {
            "total_files": result.total_count(),
            "total_size": result.total_size(),
            "total_size_formatted": format_size(result.total_size()),
        },
        "by_category": result.by_category().iter().map(|(cat, files)| {
            let size: u64 = files.iter().map(|f| f.size).sum();
            serde_json::json!({
                "category": cat.display_name(),
                "count": files.len(),
                "size": size,
                "size_formatted": format_size(size),
            })
        }).collect::<Vec<_>>(),
        "files": result.files.iter().map(|f| {
            serde_json::json!({
                "path": f.path.display().to_string(),
                "size": f.size,
                "size_formatted": format_size(f.size),
                "category": f.category.display_name(),
                "reason": f.reason,
                "is_directory": f.is_directory,
            })
        }).collect::<Vec<_>>(),
        "errors": result.errors,
    });

    println!("{}", serde_json::to_string_pretty(&output)?);

    Ok(())
}

/// Group files by category for interactive selection
pub fn group_by_category(files: &[CleanableFile]) -> HashMap<Category, Vec<&CleanableFile>> {
    let mut groups: HashMap<Category, Vec<&CleanableFile>> = HashMap::new();

    for file in files {
        groups.entry(file.category).or_default().push(file);
    }

    groups
}

// Helper functions

fn format_number(n: u64) -> String {
    // Simple number formatting with commas
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

fn format_size(bytes: u64) -> String {
    bytesize::to_string(bytes, false)
}

fn print_category_header(name: &str, size: u64, _count: usize) {
    println!("{} ({})", name, format_size(size));
    println!("{}", "-".repeat(60));
}

fn print_file_entry(path: &Path, size: u64, _depth: usize) {
    println!("  {}  {}", format_size(size), path.display());
}

// Scanner adapters that wrap existing category scan functions

struct CacheScannerAdapter;
impl Scanner for CacheScannerAdapter {
    fn name(&self) -> &'static str {
        "Cache Scanner"
    }

    fn category(&self) -> Category {
        Category::Cache
    }

    fn scan(
        &self,
        _path: &Path,
        _options: &ScanOptions,
        config: &Config,
    ) -> Result<Vec<CleanableFile>> {
        use crate::categories::cache;
        let result = cache::scan(std::path::Path::new(""), config, OutputMode::Normal)?;
        Ok(convert_category_result(
            result,
            Category::Cache,
            "Package manager cache",
            config,
        ))
    }
}

struct AppCacheScannerAdapter;
impl Scanner for AppCacheScannerAdapter {
    fn name(&self) -> &'static str {
        "App Cache Scanner"
    }

    fn category(&self) -> Category {
        Category::AppCache
    }

    fn scan(
        &self,
        _path: &Path,
        _options: &ScanOptions,
        config: &Config,
    ) -> Result<Vec<CleanableFile>> {
        use crate::categories::app_cache;
        let result = app_cache::scan(std::path::Path::new(""), config, OutputMode::Normal)?;
        Ok(convert_category_result(
            result,
            Category::AppCache,
            "Application cache",
            config,
        ))
    }
}

struct TempScannerAdapter;
impl Scanner for TempScannerAdapter {
    fn name(&self) -> &'static str {
        "Temp Scanner"
    }

    fn category(&self) -> Category {
        Category::Temp
    }

    fn scan(
        &self,
        _path: &Path,
        _options: &ScanOptions,
        config: &Config,
    ) -> Result<Vec<CleanableFile>> {
        use crate::categories::temp;
        let result = temp::scan(std::path::Path::new(""), config)?;
        Ok(convert_category_result(
            result,
            Category::Temp,
            "Temporary files",
            config,
        ))
    }
}

struct TrashScannerAdapter;
impl Scanner for TrashScannerAdapter {
    fn name(&self) -> &'static str {
        "Trash Scanner"
    }

    fn category(&self) -> Category {
        Category::Trash
    }

    fn scan(
        &self,
        _path: &Path,
        _options: &ScanOptions,
        config: &Config,
    ) -> Result<Vec<CleanableFile>> {
        use crate::categories::trash;
        let result = trash::scan()?;
        Ok(convert_category_result(
            result,
            Category::Trash,
            "Recycle Bin contents",
            config,
        ))
    }
}

struct BuildScannerAdapter;
impl Scanner for BuildScannerAdapter {
    fn name(&self) -> &'static str {
        "Build Scanner"
    }

    fn category(&self) -> Category {
        Category::Build
    }

    fn scan(
        &self,
        path: &Path,
        options: &ScanOptions,
        config: &Config,
    ) -> Result<Vec<CleanableFile>> {
        use crate::categories::build;
        let result = build::scan(
            path,
            options.project_age_days,
            Some(&config.categories.build),
            config,
            OutputMode::Normal,
        )?;
        Ok(convert_category_result(
            result,
            Category::Build,
            "Build artifacts from inactive projects",
            config,
        ))
    }
}

struct DownloadsScannerAdapter;
impl Scanner for DownloadsScannerAdapter {
    fn name(&self) -> &'static str {
        "Downloads Scanner"
    }

    fn category(&self) -> Category {
        Category::Downloads
    }

    fn scan(
        &self,
        _path: &Path,
        options: &ScanOptions,
        config: &Config,
    ) -> Result<Vec<CleanableFile>> {
        use crate::categories::downloads;
        let result = downloads::scan(
            std::path::Path::new(""),
            options.min_age_days,
            config,
            OutputMode::Normal,
        )?;
        Ok(convert_category_result(
            result,
            Category::Downloads,
            "Old downloads",
            config,
        ))
    }
}

struct LargeScannerAdapter;
impl Scanner for LargeScannerAdapter {
    fn name(&self) -> &'static str {
        "Large Files Scanner"
    }

    fn category(&self) -> Category {
        Category::Large
    }

    fn scan(
        &self,
        _path: &Path,
        options: &ScanOptions,
        config: &Config,
    ) -> Result<Vec<CleanableFile>> {
        use crate::categories::large;
        let result = large::scan(
            std::path::Path::new(""),
            options.min_size_bytes,
            config,
            OutputMode::Normal,
        )?;
        Ok(convert_category_result(
            result,
            Category::Large,
            "Large files",
            config,
        ))
    }
}

struct OldScannerAdapter;
impl Scanner for OldScannerAdapter {
    fn name(&self) -> &'static str {
        "Old Files Scanner"
    }

    fn category(&self) -> Category {
        Category::Old
    }

    fn scan(
        &self,
        path: &Path,
        options: &ScanOptions,
        config: &Config,
    ) -> Result<Vec<CleanableFile>> {
        use crate::categories::old;
        let result = old::scan(path, options.min_age_days, config, OutputMode::Normal)?;
        Ok(convert_category_result(
            result,
            Category::Old,
            "Old files",
            config,
        ))
    }
}

struct BrowserScannerAdapter;
impl Scanner for BrowserScannerAdapter {
    fn name(&self) -> &'static str {
        "Browser Scanner"
    }

    fn category(&self) -> Category {
        Category::Browser
    }

    fn scan(
        &self,
        path: &Path,
        _options: &ScanOptions,
        config: &Config,
    ) -> Result<Vec<CleanableFile>> {
        use crate::categories::browser;
        let result = browser::scan(path, config)?;
        Ok(convert_category_result(
            result,
            Category::Browser,
            "Browser cache",
            config,
        ))
    }
}

struct SystemScannerAdapter;
impl Scanner for SystemScannerAdapter {
    fn name(&self) -> &'static str {
        "System Scanner"
    }

    fn category(&self) -> Category {
        Category::System
    }

    fn scan(
        &self,
        path: &Path,
        _options: &ScanOptions,
        config: &Config,
    ) -> Result<Vec<CleanableFile>> {
        use crate::categories::system;
        let result = system::scan(path, config)?;
        Ok(convert_category_result(
            result,
            Category::System,
            "System cache",
            config,
        ))
    }
}

struct EmptyScannerAdapter;
impl Scanner for EmptyScannerAdapter {
    fn name(&self) -> &'static str {
        "Empty Folders Scanner"
    }

    fn category(&self) -> Category {
        Category::Empty
    }

    fn scan(
        &self,
        path: &Path,
        _options: &ScanOptions,
        config: &Config,
    ) -> Result<Vec<CleanableFile>> {
        use crate::categories::empty;
        let result = empty::scan(path, config)?;
        Ok(convert_category_result(
            result,
            Category::Empty,
            "Empty folders",
            config,
        ))
    }
}

struct DuplicatesScannerAdapter;
impl Scanner for DuplicatesScannerAdapter {
    fn name(&self) -> &'static str {
        "Duplicates Scanner"
    }

    fn category(&self) -> Category {
        Category::Duplicates
    }

    fn scan(
        &self,
        path: &Path,
        _options: &ScanOptions,
        config: &Config,
    ) -> Result<Vec<CleanableFile>> {
        use crate::categories::duplicates;
        let result =
            duplicates::scan_with_config(path, Some(&config.categories.duplicates), config)?;
        let category_result = result.to_category_result();
        Ok(convert_category_result(
            category_result,
            Category::Duplicates,
            "Duplicate files",
            config,
        ))
    }
}

struct ApplicationsScannerAdapter;
impl Scanner for ApplicationsScannerAdapter {
    fn name(&self) -> &'static str {
        "Applications Scanner"
    }

    fn category(&self) -> Category {
        Category::Applications
    }

    fn scan(
        &self,
        _path: &Path,
        _options: &ScanOptions,
        config: &Config,
    ) -> Result<Vec<CleanableFile>> {
        use crate::categories::applications;
        let result = applications::scan(std::path::Path::new(""), config, OutputMode::Normal)?;
        Ok(convert_category_result(
            result,
            Category::Applications,
            "Installed application",
            config,
        ))
    }
}

/// Convert CategoryResult to Vec<CleanableFile>, filtering excluded paths
fn convert_category_result(
    result: CategoryResult,
    category: Category,
    reason: &str,
    config: &Config,
) -> Vec<CleanableFile> {
    result
        .paths
        .into_iter()
        .filter(|path| !config.is_excluded(path))
        .map(|path| {
            // For Installed Applications we already computed real folder sizes during the scan
            // (registry EstimatedSize or a directory walk). `metadata.len()` is not meaningful
            // for directories on Windows and will show tiny values (e.g. 4 KiB).
            let size = if category == Category::Applications {
                crate::categories::applications::get_app_size(&path)
                    .unwrap_or_else(|| std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0))
            } else {
                std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0)
            };
            let is_directory = path.is_dir();
            CleanableFile {
                path,
                size,
                category,
                reason: reason.to_string(),
                is_directory,
            }
        })
        .collect()
}
