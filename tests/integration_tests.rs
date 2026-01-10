//! Integration tests for wole
//!
//! These tests verify end-to-end workflows and interactions between modules

use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;
use wole::cli::ScanOptions;
use wole::config::Config;
use wole::history::{DeletionLog, DeletionRecord};
use wole::output::OutputMode;
use wole::scanner;
use wole::utils;

fn create_test_dir() -> TempDir {
    tempfile::tempdir().unwrap()
}

#[test]
fn test_scan_build_artifacts_inactive_project() {
    let temp_dir = create_test_dir();
    let project_dir = temp_dir.path().join("old-project");
    fs::create_dir_all(&project_dir).unwrap();

    // Create a package.json to mark it as a Node project
    fs::write(project_dir.join("package.json"), r#"{"name": "test"}"#).unwrap();

    // Create node_modules directory (build artifact)
    let node_modules = project_dir.join("node_modules");
    fs::create_dir_all(&node_modules).unwrap();
    fs::write(node_modules.join("test.txt"), "test").unwrap();

    let options = ScanOptions {
        build: true,
        cache: false,
        app_cache: false,
        temp: false,
        trash: false,
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
    let _results = scanner::scan_all(temp_dir.path(), options, OutputMode::Quiet, &config).unwrap();

    // Scan completed successfully (may or may not find items depending on git activity)
}

#[test]
fn test_config_exclusion_filtering() {
    let mut config = Config::default();
    config
        .exclusions
        .patterns
        .push("**/important/**".to_string());

    let test_path = PathBuf::from("C:/Users/test/important/file.txt");
    assert!(config.is_excluded(&test_path));

    let normal_path = PathBuf::from("C:/Users/test/normal/file.txt");
    assert!(!config.is_excluded(&normal_path));
}

#[test]
fn test_scan_empty_directory() {
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
    let results = scanner::scan_all(temp_dir.path(), options, OutputMode::Quiet, &config).unwrap();

    // Should return empty results
    assert_eq!(results.cache.items, 0);
    assert_eq!(results.temp.items, 0);
    assert_eq!(results.build.items, 0);
}

#[test]
fn test_config_defaults() {
    let config = Config::default();
    assert_eq!(config.thresholds.project_age_days, 14);
    assert_eq!(config.thresholds.min_age_days, 30);
    assert_eq!(config.thresholds.min_size_mb, 100);
}

#[test]
fn test_config_cli_overrides() {
    let mut config = Config::default();
    config.apply_cli_overrides(Some(30), Some(60), Some(200));

    assert_eq!(config.thresholds.project_age_days, 30);
    assert_eq!(config.thresholds.min_age_days, 60);
    assert_eq!(config.thresholds.min_size_mb, 200);
}

// ==================== Long Path Support Tests ====================

#[test]
fn test_long_path_conversion() {
    // Test that to_long_path adds the prefix correctly
    let normal_path = std::path::Path::new(r"C:\Users\test\file.txt");
    let _long_path = utils::to_long_path(normal_path);

    #[cfg(windows)]
    {
        let path_str = _long_path.to_str().unwrap();
        assert!(
            path_str.starts_with(r"\\?\"),
            "Path should start with \\\\?\\"
        );
    }

    // Test that already-prefixed paths are unchanged
    let prefixed = std::path::Path::new(r"\\?\C:\Users\test\file.txt");
    let result = utils::to_long_path(prefixed);
    assert!(result.to_str().unwrap().starts_with(r"\\?\"));
}

#[test]
fn test_safe_metadata_on_regular_file() {
    let temp_dir = create_test_dir();
    let test_file = temp_dir.path().join("test.txt");
    fs::write(&test_file, "hello world").unwrap();

    let meta = utils::safe_metadata(&test_file).unwrap();
    assert!(meta.is_file());
    assert_eq!(meta.len(), 11);
}

#[test]
fn test_safe_read_dir() {
    let temp_dir = create_test_dir();
    let subdir = temp_dir.path().join("subdir");
    fs::create_dir(&subdir).unwrap();
    fs::write(subdir.join("file1.txt"), "a").unwrap();
    fs::write(subdir.join("file2.txt"), "b").unwrap();

    let entries: Vec<_> = utils::safe_read_dir(&subdir)
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();

    assert_eq!(entries.len(), 2);
}

// ==================== Symlink Protection Tests ====================

#[test]
fn test_should_skip_entry_regular_dir() {
    let temp_dir = create_test_dir();
    let regular_dir = temp_dir.path().join("regular");
    fs::create_dir(&regular_dir).unwrap();

    // Regular directories should NOT be skipped
    assert!(!utils::should_skip_entry(&regular_dir));
}

#[test]
fn test_should_skip_entry_regular_file() {
    let temp_dir = create_test_dir();
    let regular_file = temp_dir.path().join("file.txt");
    fs::write(&regular_file, "test").unwrap();

    // Regular files should NOT be skipped
    assert!(!utils::should_skip_entry(&regular_file));
}

// ==================== History Logging Tests ====================

#[test]
fn test_deletion_log_creation() {
    let log = DeletionLog::new();

    assert_eq!(log.records.len(), 0);
    assert_eq!(log.total_bytes_cleaned, 0);
    assert_eq!(log.total_items, 0);
    assert_eq!(log.errors, 0);
}

#[test]
fn test_deletion_log_add_success() {
    let mut log = DeletionLog::new();
    let path = std::path::Path::new("/test/file.txt");

    log.log_success(path, 1024, "cache", false);

    assert_eq!(log.records.len(), 1);
    assert_eq!(log.total_bytes_cleaned, 1024);
    assert_eq!(log.total_items, 1);
    assert_eq!(log.errors, 0);
    assert!(log.records[0].success);
}

#[test]
fn test_deletion_log_add_failure() {
    let mut log = DeletionLog::new();
    let path = std::path::Path::new("/test/locked.txt");

    log.log_failure(path, 2048, "temp", true, "File is locked");

    assert_eq!(log.records.len(), 1);
    assert_eq!(log.total_bytes_cleaned, 0);
    assert_eq!(log.total_items, 1);
    assert_eq!(log.errors, 1);
    assert!(!log.records[0].success);
    assert_eq!(log.records[0].error, Some("File is locked".to_string()));
}

#[test]
fn test_deletion_log_mixed_results() {
    let mut log = DeletionLog::new();

    log.log_success(std::path::Path::new("/file1.txt"), 1000, "cache", false);
    log.log_success(std::path::Path::new("/file2.txt"), 2000, "temp", false);
    log.log_failure(
        std::path::Path::new("/locked.txt"),
        500,
        "cache",
        false,
        "Locked",
    );

    assert_eq!(log.records.len(), 3);
    assert_eq!(log.total_bytes_cleaned, 3000); // Only successful deletions count
    assert_eq!(log.total_items, 3);
    assert_eq!(log.errors, 1);
}

#[test]
fn test_deletion_record_success() {
    let record =
        DeletionRecord::success(std::path::Path::new("/test/file.txt"), 1024, "cache", false);

    assert!(record.success);
    assert!(record.error.is_none());
    assert_eq!(record.size_bytes, 1024);
    assert_eq!(record.category, "cache");
    assert!(!record.permanent);
}

#[test]
fn test_deletion_record_failure() {
    let record = DeletionRecord::failure(
        std::path::Path::new("/test/locked.txt"),
        2048,
        "temp",
        true,
        "Permission denied",
    );

    assert!(!record.success);
    assert_eq!(record.error, Some("Permission denied".to_string()));
    assert!(record.permanent);
}

#[test]
fn test_history_dir_creation() {
    // This test verifies get_history_dir works without panicking
    let result = wole::history::get_history_dir();
    assert!(result.is_ok());

    let dir = result.unwrap();
    assert!(dir.exists());
}

// ==================== Calculate Dir Size Tests ====================

#[test]
fn test_calculate_dir_size_empty() {
    let temp_dir = create_test_dir();
    let empty_dir = temp_dir.path().join("empty");
    fs::create_dir(&empty_dir).unwrap();

    let size = utils::calculate_dir_size(&empty_dir);
    assert_eq!(size, 0);
}

#[test]
fn test_calculate_dir_size_with_files() {
    let temp_dir = create_test_dir();
    let dir = temp_dir.path().join("with_files");
    fs::create_dir(&dir).unwrap();

    fs::write(dir.join("file1.txt"), "hello").unwrap(); // 5 bytes
    fs::write(dir.join("file2.txt"), "world!").unwrap(); // 6 bytes

    let size = utils::calculate_dir_size(&dir);
    assert_eq!(size, 11);
}

#[test]
fn test_calculate_dir_size_nested() {
    let temp_dir = create_test_dir();
    let dir = temp_dir.path().join("nested");
    fs::create_dir(&dir).unwrap();

    let subdir = dir.join("subdir");
    fs::create_dir(&subdir).unwrap();

    fs::write(dir.join("file1.txt"), "abc").unwrap(); // 3 bytes
    fs::write(subdir.join("file2.txt"), "defgh").unwrap(); // 5 bytes

    let size = utils::calculate_dir_size(&dir);
    assert_eq!(size, 8);
}
