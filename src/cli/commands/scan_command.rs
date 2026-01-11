//! Scan command feature.
//!
//! This module owns and handles the "wole scan" command behavior.

use crate::cli::ScanOptions;
use crate::config::Config;
use crate::output::{self, OutputMode};
use crate::scanner;
use crate::size;
use std::path::PathBuf;

pub(crate) fn handle_scan(
    all: bool,
    cache: bool,
    app_cache: bool,
    temp: bool,
    trash: bool,
    build: bool,
    downloads: bool,
    large: bool,
    old: bool,
    applications: bool,
    windows_update: bool,
    event_logs: bool,
    path: Option<PathBuf>,
    json: bool,
    project_age: u64,
    min_age: u64,
    min_size: String,
    _exclude: Vec<String>,
    output_mode: OutputMode,
) -> anyhow::Result<()> {
    // --all enables all categories
    let (
        cache,
        app_cache,
        temp,
        trash,
        build,
        downloads,
        large,
        old,
        applications,
        browser,
        system,
        empty,
        duplicates,
        windows_update,
        event_logs,
    ) = if all {
        (
            true, true, true, true, true, true, true, true, true, true, true, true, true, true,
            true,
        )
    } else if !cache
        && !app_cache
        && !temp
        && !trash
        && !build
        && !downloads
        && !large
        && !old
        && !applications
        && !windows_update
        && !event_logs
    {
        // No categories specified - show help message
        eprintln!("No categories specified. Use --all or specify categories like --cache, --app-cache, --temp, --build");
        eprintln!("Run 'wole scan --help' for more information.");
        return Ok(());
    } else {
        // Scan command doesn't support browser, system, empty, duplicates
        (
            cache,
            app_cache,
            temp,
            trash,
            build,
            downloads,
            large,
            old,
            applications,
            false,
            false,
            false,
            false,
            windows_update,
            event_logs,
        )
    };

    // Default to current directory to avoid stack overflow from OneDrive/UserDirs
    // PERFORMANCE FIX: Avoid OneDrive paths which are very slow to scan on Windows
    // Use current directory instead, which is faster and more predictable
    let scan_path = path.unwrap_or_else(|| {
        // Use current directory as default - faster and avoids OneDrive sync issues
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    });

    // Load config first
    let mut config = Config::load();

    // Apply CLI overrides to config
    config.apply_cli_overrides(
        Some(project_age),
        Some(min_age),
        Some(
            size::parse_size(&min_size).map_err(|e| {
                anyhow::anyhow!("Invalid size format '{}': {}", min_size, e)
            })? / (1024 * 1024),
        ), // Convert bytes to MB for config
    );

    // Use config values (after CLI overrides) for scan options
    let min_size_bytes = config.thresholds.min_size_mb * 1024 * 1024;

    let scan_options = ScanOptions {
        cache,
        app_cache,
        temp,
        trash,
        build,
        downloads,
        large,
        old,
        applications,
        browser,
        system,
        empty,
        duplicates,
        windows_update,
        event_logs,
        project_age_days: config.thresholds.project_age_days,
        min_age_days: config.thresholds.min_age_days,
        min_size_bytes,
    };

    let results = scanner::scan_all(&scan_path, scan_options.clone(), output_mode, &config)?;

    if json {
        output::print_json(&results)?;
    } else {
        output::print_human_with_options(&results, output_mode, Some(&scan_options));
    }

    Ok(())
}
