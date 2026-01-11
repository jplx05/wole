//! Analyze command feature.
//!
//! This module owns and handles the "wole analyze" command behavior.

use crate::cli::ScanOptions;
use crate::config::Config;
use crate::output::{self, OutputMode};
use crate::scanner;
use crate::size;
use std::path::PathBuf;

pub(crate) fn handle_analyze(
    disk: bool,
    entire_disk: bool,
    interactive: bool,
    depth: u8,
    top: Option<usize>,
    sort: Option<String>,
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
    path: Option<PathBuf>,
    project_age: u64,
    min_age: u64,
    min_size: String,
    exclude: Vec<String>,
    output_mode: OutputMode,
) -> anyhow::Result<()> {
    // Load config first
    let config = Config::load();

    // Determine if we're in disk insights mode or legacy cleanable file mode
    let has_category_flags = cache
        || app_cache
        || temp
        || trash
        || build
        || downloads
        || large
        || old
        || browser
        || system
        || empty
        || duplicates
        || applications
        || all;
    let disk_mode = disk || (!has_category_flags); // Default to disk mode if no category flags

    if disk_mode {
        // Disk insights mode
        use crate::disk_usage::{scan_directory, SortBy};
        use crate::utils;

        // Determine scan path
        let scan_path = if let Some(custom_path) = path {
            // User specified a custom path
            custom_path
        } else if entire_disk {
            // User wants to scan entire disk
            utils::get_root_disk_path()
        } else {
            // Default to user directory
            if let Ok(userprofile) = std::env::var("USERPROFILE") {
                PathBuf::from(&userprofile)
            } else {
                std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
            }
        };

        if !scan_path.exists() {
            return Err(anyhow::anyhow!(
                "Path does not exist: {}",
                scan_path.display()
            ));
        }

        // Parse sort option
        let sort_by = match sort.as_deref() {
            Some("name") => SortBy::Name,
            Some("files") => SortBy::Files,
            _ => SortBy::Size,
        };

        // Adjust depth based on scan type and config
        // If user specified depth explicitly, use it; otherwise use config defaults
        let effective_depth = if depth == 3 {
            // User didn't specify depth, use config defaults
            if entire_disk {
                config.ui.scan_depth_entire_disk
            } else {
                config.ui.scan_depth_user
            }
        } else {
            // User specified depth explicitly via CLI, use it (overrides config)
            depth
        };

        // Scan directory
        let spinner = if output_mode != OutputMode::Quiet {
            Some(crate::progress::create_spinner(&format!(
                "Scanning {} (depth: {})...",
                scan_path.display(),
                effective_depth
            )))
        } else {
            None
        };

        let insights = scan_directory(&scan_path, effective_depth)?;

        if let Some(sp) = spinner {
            crate::progress::finish_and_clear(&sp);
        }

        if interactive {
            // Launch TUI mode
            use crate::tui;
            let mut app_state = tui::state::AppState::new();
            app_state.screen = tui::state::Screen::DiskInsights {
                insights: insights.clone(),
                current_path: scan_path.clone(),
                cursor: 0,
                sort_by,
            };
            tui::run(Some(app_state))?;
        } else {
            // CLI output mode
            output::print_disk_insights(
                &insights,
                &scan_path,
                top.unwrap_or(10),
                sort_by,
                output_mode,
            );
        }

        Ok(())
    } else {
        // Legacy cleanable file analysis mode
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
        ) = if all {
            (
                true, true, true, true, true, true, true, true, true, true, true, true, true,
            )
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

        let results = scanner::scan_all(
            &scan_path,
            ScanOptions {
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
                windows_update: false,
                event_logs: false,
                project_age_days: config.thresholds.project_age_days,
                min_age_days: config.thresholds.min_age_days,
                min_size_bytes,
            },
            output_mode,
            &config,
        )?;

        // Launch TUI if interactive mode requested
        if interactive {
            use crate::tui;
            let mut app_state = tui::state::AppState::new();
            app_state.scan_path = scan_path;
            app_state.config = config;
            // Store scan results and process them
            app_state.scan_results = Some(results);
            app_state.flatten_results();
            app_state.screen = tui::state::Screen::Results;
            tui::run(Some(app_state))?;
        } else {
            output::print_analyze(&results, output_mode);
        }

        Ok(())
    }
}
