//! Config command feature.
//!
//! This module owns and handles the "wole config" command behavior.

use crate::config::Config;
use crate::theme::Theme;
use bytesize;

pub(crate) fn handle_config(
    show: bool,
    reset: bool,
    edit: bool,
    clear_cache: bool,
) -> anyhow::Result<()> {
    if show {
        let config = Config::load_or_create();
        println!("{}", Theme::header("Current Configuration"));
        println!("{}", Theme::divider_bold(60));
        println!();
        println!("Thresholds:");
        println!("  Project age: {} days", config.thresholds.project_age_days);
        println!("  Min age: {} days", config.thresholds.min_age_days);
        println!("  Min size: {} MB", config.thresholds.min_size_mb);
        println!();
        println!("Paths:");
        if config.paths.scan_roots.is_empty() {
            println!("  (none - using default)");
        } else {
            for path in &config.paths.scan_roots {
                println!("  {}", path);
            }
        }
        println!();
        println!("Exclusions:");
        if config.exclusions.patterns.is_empty() {
            println!("  (none)");
        } else {
            for pattern in &config.exclusions.patterns {
                println!("  {}", pattern);
            }
        }
        println!();
        println!("UI Settings:");
        if let Some(ref path) = config.ui.default_scan_path {
            println!("  Default scan path: {}", path);
        } else {
            println!("  Default scan path: (auto-detect)");
        }
        println!("  Output mode: {}", config.ui.output_mode);
        println!("  Animations: {}", config.ui.animations);
        println!("  Refresh rate: {} ms", config.ui.refresh_rate_ms);
        println!();
        println!("Safety Settings:");
        println!("  Always confirm: {}", config.safety.always_confirm);
        println!("  Default permanent: {}", config.safety.default_permanent);
        println!("  Max no-confirm files: {}", config.safety.max_no_confirm);
        println!(
            "  Max no-confirm size: {} MB",
            config.safety.max_size_no_confirm_mb
        );
        println!("  Skip locked files: {}", config.safety.skip_locked_files);
        println!("  Dry run default: {}", config.safety.dry_run_default);
        println!();
        println!("Performance Settings:");
        println!(
            "  Scan threads: {} (0 = auto)",
            config.performance.scan_threads
        );
        println!("  Batch size: {}", config.performance.batch_size);
        println!(
            "  Parallel scanning: {}",
            config.performance.parallel_scanning
        );
        println!();
        println!("History Settings:");
        println!("  Enabled: {}", config.history.enabled);
        println!(
            "  Max entries: {} (0 = unlimited)",
            config.history.max_entries
        );
        println!(
            "  Max age: {} days (0 = forever)",
            config.history.max_age_days
        );
        println!();
        println!("Cache Settings:");
        println!("  Enabled: {}", config.cache.enabled);
        println!("  Full disk baseline: {}", config.cache.full_disk_baseline);
        println!("  Max age: {} days", config.cache.max_age_days);
        println!(
            "  Content hash threshold: {}",
            bytesize::to_string(config.cache.content_hash_threshold_bytes, false)
        );
        println!();
        if let Ok(path) = Config::config_path() {
            println!("Config file: {}", path.display());
        }
    } else if clear_cache {
        match crate::scan_cache::ScanCache::open() {
            Ok(mut cache) => {
                cache.clear_all()?;
                println!("{} Scan cache cleared successfully.", Theme::success("OK"));
            }
            Err(e) => {
                return Err(anyhow::anyhow!("Failed to clear cache: {}", e));
            }
        }
    } else if reset {
        let default_config = Config::default();
        default_config.save()?;
        println!("{} Configuration reset to defaults.", Theme::success("OK"));
    } else if edit {
        if let Ok(path) = Config::config_path() {
            // Create default config if it doesn't exist
            if !path.exists() {
                Config::default().save()?;
            }
            // Try to open in default editor
            let editor = std::env::var("EDITOR").unwrap_or_else(|_| "notepad".to_string());
            std::process::Command::new(editor)
                .arg(&path)
                .status()
                .map_err(|e| anyhow::anyhow!("Failed to open editor: {}", e))?;
        } else {
            return Err(anyhow::anyhow!("Failed to get config file path"));
        }
    } else {
        // Default: show config
        let config = Config::load_or_create();
        println!("{}", Theme::header("Current Configuration"));
        println!("{}", Theme::divider_bold(60));
        println!();
        println!("Thresholds:");
        println!("  Project age: {} days", config.thresholds.project_age_days);
        println!("  Min age: {} days", config.thresholds.min_age_days);
        println!("  Min size: {} MB", config.thresholds.min_size_mb);
        println!();
        println!("Paths:");
        if config.paths.scan_roots.is_empty() {
            println!("  (none - using default)");
        } else {
            for path in &config.paths.scan_roots {
                println!("  {}", path);
            }
        }
        println!();
        println!("Exclusions:");
        if config.exclusions.patterns.is_empty() {
            println!("  (none)");
        } else {
            for pattern in &config.exclusions.patterns {
                println!("  {}", pattern);
            }
        }
        println!();
        println!("UI Settings:");
        if let Some(ref path) = config.ui.default_scan_path {
            println!("  Default scan path: {}", path);
        } else {
            println!("  Default scan path: (auto-detect)");
        }
        println!("  Output mode: {}", config.ui.output_mode);
        println!("  Animations: {}", config.ui.animations);
        println!("  Refresh rate: {} ms", config.ui.refresh_rate_ms);
        println!();
        println!("Safety Settings:");
        println!("  Always confirm: {}", config.safety.always_confirm);
        println!("  Default permanent: {}", config.safety.default_permanent);
        println!("  Max no-confirm files: {}", config.safety.max_no_confirm);
        println!(
            "  Max no-confirm size: {} MB",
            config.safety.max_size_no_confirm_mb
        );
        println!("  Skip locked files: {}", config.safety.skip_locked_files);
        println!("  Dry run default: {}", config.safety.dry_run_default);
        println!();
        println!("Performance Settings:");
        println!(
            "  Scan threads: {} (0 = auto)",
            config.performance.scan_threads
        );
        println!("  Batch size: {}", config.performance.batch_size);
        println!(
            "  Parallel scanning: {}",
            config.performance.parallel_scanning
        );
        println!();
        println!("History Settings:");
        println!("  Enabled: {}", config.history.enabled);
        println!(
            "  Max entries: {} (0 = unlimited)",
            config.history.max_entries
        );
        println!(
            "  Max age: {} days (0 = forever)",
            config.history.max_age_days
        );
        println!();
        println!("Cache Settings:");
        println!("  Enabled: {}", config.cache.enabled);
        println!("  Full disk baseline: {}", config.cache.full_disk_baseline);
        println!("  Max age: {} days", config.cache.max_age_days);
        println!(
            "  Content hash threshold: {}",
            bytesize::to_string(config.cache.content_hash_threshold_bytes, false)
        );
        println!();
        if let Ok(path) = Config::config_path() {
            println!("Config file: {}", path.display());
        }
    }
    Ok(())
}
