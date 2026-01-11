//! Clean command feature.
//!
//! This module owns and handles the "wole clean" command behavior.

use crate::cleaner;
use crate::cli::ScanOptions;
use crate::config::Config;
use crate::output::{self, OutputMode};
use crate::scanner;
use crate::size;
use crate::theme::Theme;
use crate::utils;
use std::path::{Path, PathBuf};

// Helper function to format numbers (copied from output.rs for local use)
fn format_number(n: u64) -> String {
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

fn get_disk_space_for_path(path: &Path) -> Option<(u64, u64)> {
    use sysinfo::Disks;

    let mut disks = Disks::new_with_refreshed_list();
    disks.refresh();

    // Choose disk with the longest mount-point prefix match.
    let mut best: Option<(usize, u64, u64)> = None; // (match_len, total, avail)
    for disk in disks.list() {
        let mount = disk.mount_point();
        if path.starts_with(mount) {
            let len = mount.as_os_str().len();
            let total = disk.total_space();
            let avail = disk.available_space();
            if best.map(|b| len > b.0).unwrap_or(true) {
                best = Some((len, total, avail));
            }
        }
    }

    best.map(|(_, total, avail)| (total, avail))
}

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

    let mut scan_path = path.unwrap_or_else(|| {
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

    let mut scan_cache = if config.cache.enabled {
        match crate::scan_cache::ScanCache::open() {
            Ok(cache) => Some(cache),
            Err(e) => {
                if output_mode != OutputMode::Quiet {
                    eprintln!(
                        "Warning: Failed to open scan cache: {}. Continuing without cache.",
                        e
                    );
                }
                None
            }
        }
    } else {
        None
    };

    let mut first_scan_full_disk = false;
    if let Some(cache) = scan_cache.as_ref() {
        match cache.get_previous_scan_id() {
            Ok(None) => {
                first_scan_full_disk = true;
            }
            Ok(Some(_)) => {}
            Err(e) => {
                if output_mode != OutputMode::Quiet {
                    eprintln!(
                        "Warning: Failed to read scan cache state: {}. Continuing without full disk baseline.",
                        e
                    );
                }
            }
        }
    }

    if first_scan_full_disk {
        scan_path = utils::get_root_disk_path();
        if output_mode != OutputMode::Quiet {
            if json {
                eprintln!("First scan detected: scanning all categories from root to build baseline.");
            } else {
                println!();
                println!("{}", crate::theme::Theme::warning("First scan detected: scanning all enabled categories from root to build baseline."));
                println!("{}", crate::theme::Theme::muted("This may take longer than usual. Future scans will be faster using incremental cache."));
                println!();
            }
        }
    }

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

    let results = scanner::scan_all(
        &scan_path,
        scan_options.clone(),
        output_mode,
        &config,
        scan_cache.as_mut(),
    )?;

    if json {
        output::print_json(&results)?;
    } else {
        output::print_human_with_options(&results, output_mode, Some(&scan_options));
    }

    // After first scan, show cache statistics
    if first_scan_full_disk && output_mode != OutputMode::Quiet {
        if let Some(cache) = scan_cache.as_ref() {
            if let Ok((total_files, total_storage)) = cache.get_cache_stats() {
                println!();
                println!("{}", Theme::header("First Scan Complete - Cache Baseline Built"));
                println!("{}", Theme::divider(60));
                println!();
                println!(
                    "  {} Total files indexed: {}",
                    Theme::muted("→"),
                    Theme::primary(&format_number(total_files as u64))
                );
                println!(
                    "  {} Total file bytes indexed: {}",
                    Theme::muted("→"),
                    Theme::primary(&bytesize::to_string(total_storage, true))
                );

                if let Some((total, avail)) = get_disk_space_for_path(&scan_path) {
                    let used = total.saturating_sub(avail);
                    let total_gb = total as f64 / 1_000_000_000.0;
                    println!(
                        "  {} Disk (OS): {} used / {} total ({} free, ~{:.0} GB total)",
                        Theme::muted("→"),
                        Theme::primary(&bytesize::to_string(used, true)),
                        Theme::primary(&bytesize::to_string(total, true)),
                        Theme::primary(&bytesize::to_string(avail, true)),
                        total_gb
                    );
                }
                println!();
                println!("{}", Theme::muted("Future scans will use incremental cache for faster results."));
                println!();
            }
        }
    }

    cleaner::clean_all(&results, yes, output_mode, permanent, dry_run)?;

    Ok(())
}
