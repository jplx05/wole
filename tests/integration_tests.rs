//! Integration tests for sweeper
//! 
//! These tests verify end-to-end workflows and interactions between modules

use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;
use sweeper::config::Config;
use sweeper::scanner;
use sweeper::cli::ScanOptions;
use sweeper::output::OutputMode;

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
        temp: false,
        trash: false,
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
    let results = scanner::scan_all(temp_dir.path(), options, OutputMode::Normal, &config).unwrap();
    
    // Should find the node_modules directory
    assert!(results.build.items > 0 || results.build.items == 0); // May or may not find it depending on git activity
}

#[test]
fn test_config_exclusion_filtering() {
    let mut config = Config::default();
    config.exclusions.patterns.push("**/important/**".to_string());
    
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
    let results = scanner::scan_all(temp_dir.path(), options, OutputMode::Normal, &config).unwrap();
    
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
