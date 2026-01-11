//! Clean command feature.
//!
//! This module owns and handles the "wole clean" command behavior.

use crate::cleaner;
use crate::cli::ScanOptions;
use crate::config::Config;
use crate::output::{self, OutputMode};
use crate::scanner;
use crate::size;
use std::path::PathBuf;

pub(crate) fn handle_clean(
    all: bool,
    cache: bool,
    app_cache: bool,
    temp: bool,
    trash: bool,
    build: bool,
    downloads: bool,
    large: bool,
    old: bool,
    browser: bool,
    system: bool,
    empty: bool,
    duplicates: bool,
    applications: bool,
    windows_update: bool,
    event_logs: bool,
    path: Option<PathBuf>,
    json: bool,
    yes: bool,
    project_age: u64,
    min_age: u64,
    min_size: String,
    exclude: Vec<String>,
    permanent: bool,
    dry_run: bool,
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
        && !browser
        && !system
        && !empty
        && !duplicates
        && !applications
        && !windows_update
        && !event_logs
    {
        // No categories specified - show help message
        eprintln!("No categories specified. Use --all or specify categories like --cache, --app-cache, --temp, --build");
        eprintln!("Run 'wole clean --help' for more information.");
        return Ok(());
    } else {
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
            browser,
            system,
            empty,
            duplicates,
            windows_update,
            event_logs,
        )
    };

    let scan_path = path.unwrap_or_else(|| {
        directories::UserDirs::new()
            .expect("Failed to get user directory")
            .home_dir()
            .to_path_buf()
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

    // Merge CLI exclusions
    config.exclusions.patterns.extend(exclude.iter().cloned());

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

    cleaner::clean_all(&results, yes, output_mode, permanent, dry_run)?;

    Ok(())
}
