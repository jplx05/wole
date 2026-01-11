//! Scan command feature.
//!
//! This module owns and handles the "wole scan" command behavior.

use crate::cli::ScanOptions;
use crate::config::Config;
use crate::output::{self, OutputMode};
use crate::scanner;
use crate::size;
use crate::theme::Theme;
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
    exclude: Vec<String>,
    force_full: bool,
    no_cache: bool,
    clear_cache: bool,
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
            size::parse_size(&min_size)
                .map_err(|e| anyhow::anyhow!("Invalid size format '{}': {}", min_size, e))?
                / (1024 * 1024),
        ), // Convert bytes to MB for config
    );

    // Merge CLI exclusions
    config.exclusions.patterns.extend(exclude.iter().cloned());

    // Handle cache flags
    let use_cache = !no_cache && config.cache.enabled && !force_full;

    if clear_cache {
        if let Ok(mut scan_cache) = crate::scan_cache::ScanCache::open() {
            // Get categories to clear
            let categories: Vec<&str> = if all {
                vec![
                    "cache",
                    "app_cache",
                    "temp",
                    "trash",
                    "build",
                    "downloads",
                    "large",
                    "old",
                    "applications",
                    "windows_update",
                    "event_logs",
                ]
            } else {
                let mut cats = Vec::new();
                if cache {
                    cats.push("cache");
                }
                if app_cache {
                    cats.push("app_cache");
                }
                if temp {
                    cats.push("temp");
                }
                if trash {
                    cats.push("trash");
                }
                if build {
                    cats.push("build");
                }
                if downloads {
                    cats.push("downloads");
                }
                if large {
                    cats.push("large");
                }
                if old {
                    cats.push("old");
                }
                if applications {
                    cats.push("applications");
                }
                if windows_update {
                    cats.push("windows_update");
                }
                if event_logs {
                    cats.push("event_logs");
                }
                cats
            };

            if categories.is_empty() {
                // Full reset: clear cache + scan history so first-scan detection triggers again.
                scan_cache.clear_all()?;
            } else {
                scan_cache.invalidate(Some(&categories))?;
            }

            if output_mode != OutputMode::Quiet {
                println!("Cache cleared for specified categories.");
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

    // Open scan cache if enabled
    let mut scan_cache = if use_cache {
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

    let mut first_scan_detected = false;
    if let Some(cache) = scan_cache.as_ref() {
        match cache.get_previous_scan_id() {
            Ok(None) => first_scan_detected = true,
            Ok(Some(_)) => {}
            Err(e) => {
                if output_mode != OutputMode::Quiet {
                    eprintln!(
                        "Warning: Failed to read scan cache state: {}. Continuing without first-scan baseline.",
                        e
                    );
                }
            }
        }
    }

    if first_scan_detected && output_mode != OutputMode::Quiet {
        if config.cache.full_disk_baseline {
            // Deep baseline will be handled by scanner (full-disk traversal + category scans).
            if json {
                eprintln!("First scan: building deep baseline (full-disk traversal enabled).");
            } else {
                println!();
                println!(
                    "{}",
                    Theme::warning(
                        "First scan: building deep baseline (full-disk traversal enabled)."
                    )
                );
                println!(
                    "{}",
                    Theme::muted("This is heavier/slower. Turn it off via config: cache.full_disk_baseline = false")
                );
                println!();
            }
        } else {
            // Fast default: category-only scan (no full-disk walk).
            if json {
                eprintln!("First scan: building cache from category scans (fast baseline).");
            } else {
                println!();
                println!(
                    "{}",
                    Theme::warning(
                        "First scan: building cache from category scans (fast baseline)."
                    )
                );
                println!(
                    "{}",
                    Theme::muted(
                        "Tip: enable deep baseline via config: cache.full_disk_baseline = true"
                    )
                );
                println!();
            }
        }
    }

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
    if first_scan_detected && output_mode != OutputMode::Quiet {
        if let Some(cache) = scan_cache.as_ref() {
            if let Ok((total_files, total_storage)) = cache.get_cache_stats() {
                println!();
                println!(
                    "{}",
                    Theme::header("First Scan Complete - Cache Baseline Built")
                );
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
                    Theme::primary(&bytesize::to_string(total_storage, false))
                );

                if let Some((total, avail)) = get_disk_space_for_path(&scan_path) {
                    let used = total.saturating_sub(avail);
                    let total_gb = total as f64 / 1_000_000_000.0;
                    println!(
                        "  {} Disk (OS): {} used / {} total ({} free, ~{:.0} GB total)",
                        Theme::muted("→"),
                        Theme::primary(&bytesize::to_string(used, false)),
                        Theme::primary(&bytesize::to_string(total, false)),
                        Theme::primary(&bytesize::to_string(avail, false)),
                        total_gb
                    );
                }
                println!();
                println!(
                    "{}",
                    Theme::muted("Future scans will use incremental cache for faster results.")
                );
                println!();
            }
        }
    }

    Ok(())
}
