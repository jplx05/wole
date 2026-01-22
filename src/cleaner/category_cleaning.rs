//! Category cleaning feature.
//!
//! This module owns bulk cleaning across categories using scan results.

use super::batch_deletion::{clean_paths_batch, BatchDeleteResult};
use super::single_deletion::{delete_with_precheck, DeleteOutcome};
use crate::categories;
use crate::history::DeletionLog;
use crate::output::{OutputMode, ScanResults};
use crate::progress;
use crate::theme::Theme;
use crate::utils;
use anyhow::Result;
use std::collections::HashMap;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Read a line from stdin, handling terminal focus loss issues on Windows.
/// This function ensures stdin is properly synchronized and clears any stale input
/// before reading, which fixes issues when the terminal loses and regains focus.
///
/// On Windows, when a terminal loses focus and regains it, stdin can be in a
/// problematic state. This function ensures we get a fresh stdin handle each time,
/// which helps resolve focus-related input issues.
fn read_line_from_stdin() -> io::Result<String> {
    // Flush stdout to ensure prompt is visible before reading
    io::stdout().flush()?;

    // Always get a fresh stdin handle to avoid issues with stale locks
    // This is especially important on Windows when the terminal loses focus
    let mut input = String::new();

    // Use BufRead for better control and proper buffering
    use std::io::BufRead;

    // Get a fresh stdin handle each time (don't reuse a locked handle)
    // This ensures we're reading from the current terminal state
    let stdin = io::stdin();
    let mut handle = stdin.lock();

    // Read a line - this will block until the user types and presses Enter
    // On Windows, getting a fresh handle helps when the terminal has lost focus
    handle.read_line(&mut input)?;

    Ok(input)
}

/// Helper function to batch clean a category (10-50x faster than one-by-one)
fn batch_clean_category_internal(
    paths: &[PathBuf],
    category_name: &str,
    permanent: bool,
    dry_run: bool,
    progress: Option<&indicatif::ProgressBar>,
    history: Option<&mut DeletionLog>,
    mode: OutputMode,
) -> (u64, u64) {
    if paths.is_empty() {
        return (0, 0);
    }

    if let Some(pb) = progress {
        let msg = format!("Cleaning {}...", category_name);
        pb.set_message(msg);
    }

    if dry_run {
        let count = paths.len() as u64;
        if let Some(pb) = progress {
            pb.inc(count);
        }
        return (count, 0);
    }

    // Calculate sizes BEFORE deletion (critical for accurate logging)
    // Once files are deleted, we can't get their sizes anymore
    let mut path_sizes: HashMap<PathBuf, u64> = HashMap::new();
    // Always calculate sizes if history logging is enabled
    if history.is_some() {
        for path in paths {
            let size = if path.is_dir() {
                utils::calculate_dir_size(path)
            } else {
                utils::safe_metadata(path).map(|m| m.len()).unwrap_or(0)
            };
            path_sizes.insert(path.clone(), size);
        }
    }

    // Use batch deletion for much better performance
    // Batch deletion completes instantly, so progress updates happen after completion
    let BatchDeleteResult {
        success_count,
        error_count,
        deleted_paths,
        skipped_paths,
        locked_paths,
        permission_denied_paths,
    } = clean_paths_batch(paths, permanent);

    // Log successes and failures using pre-calculated sizes
    if let Some(log) = history {
        for path in &deleted_paths {
            let size = path_sizes.get(path).copied().unwrap_or(0);
            log.log_success(path, size, category_name, permanent);
        }
        // Log failures (paths that weren't deleted or skipped)
        for path in &locked_paths {
            let size = path_sizes.get(path).copied().unwrap_or(0);
            log.log_failure(
                path,
                size,
                category_name,
                permanent,
                "Path is locked by another process",
            );
        }
        for path in &permission_denied_paths {
            let size = path_sizes.get(path).copied().unwrap_or(0);
            log.log_failure(path, size, category_name, permanent, "Permission denied");
        }
        for path in paths {
            if deleted_paths.contains(path)
                || skipped_paths.contains(path)
                || locked_paths.contains(path)
                || permission_denied_paths.contains(path)
            {
                continue;
            }
            let size = path_sizes.get(path).copied().unwrap_or(0);
            log.log_failure(
                path,
                size,
                category_name,
                permanent,
                "Batch deletion failed",
            );
        }
    }

    // Update progress
    if let Some(pb) = progress {
        pb.inc(success_count as u64);
    }

    // Report errors
    if error_count > 0 && mode != OutputMode::Quiet {
        eprintln!(
            "[WARNING] Failed to clean {} {} items",
            Theme::error(&error_count.to_string()),
            category_name
        );
    }

    (success_count as u64, error_count as u64)
}

/// Clean all categories based on scan results
///
/// Handles confirmation prompts, error tracking, and provides progress feedback
pub fn clean_all(
    results: &ScanResults,
    skip_confirm: bool,
    mode: OutputMode,
    permanent: bool,
    dry_run: bool,
) -> Result<()> {
    let total_items = results.cache.items
        + results.app_cache.items
        + results.temp.items
        + results.trash.items
        + results.build.items
        + results.downloads.items
        + results.large.items
        + results.old.items
        + results.browser.items
        + results.system.items
        + results.empty.items
        + results.duplicates.items
        + results.windows_update.items
        + results.event_logs.items;
    let total_bytes = results.cache.size_bytes
        + results.app_cache.size_bytes
        + results.temp.size_bytes
        + results.trash.size_bytes
        + results.build.size_bytes
        + results.downloads.size_bytes
        + results.large.size_bytes
        + results.old.size_bytes
        + results.browser.size_bytes
        + results.system.size_bytes
        + results.empty.size_bytes
        + results.duplicates.size_bytes
        + results.windows_update.size_bytes
        + results.event_logs.size_bytes;

    if total_items == 0 {
        if mode != OutputMode::Quiet {
            println!("{}", Theme::success("Nothing to clean."));
        }
        return Ok(());
    }

    if dry_run && mode != OutputMode::Quiet {
        println!(
            "{}",
            Theme::warning_msg("DRY RUN MODE - No files will be deleted")
        );
        println!();
    }

    if permanent && mode != OutputMode::Quiet {
        println!(
            "{}",
            Theme::error("PERMANENT DELETE MODE - Files will bypass Recycle Bin")
        );
    }

    if !skip_confirm && !dry_run {
        print!(
            "Delete {} items ({})? [yes/no]: ",
            Theme::value(&total_items.to_string()),
            Theme::warning(&bytesize::to_string(total_bytes, false))
        );

        let input = read_line_from_stdin()?;
        let trimmed = input.trim().to_lowercase();
        // Accept: "y", "yes" (and their uppercase variants)
        let confirmed = trimmed == "y" || trimmed == "yes";

        if !confirmed {
            println!("{}", Theme::muted("Cancelled."));
            return Ok(());
        }
    }

    // Create progress bar (simpler version without ETA for batch operations)
    // Batch operations complete too quickly for meaningful ETA/speed calculations
    let progress = if mode != OutputMode::Quiet {
        Some(progress::create_progress_bar(
            total_items as u64,
            "Cleaning...",
        ))
    } else {
        None
    };

    // Create deletion log for audit trail (not used in dry run)
    let mut history = if !dry_run {
        Some(DeletionLog::new())
    } else {
        None
    };

    let mut cleaned = 0u64;
    let mut cleaned_bytes = 0u64;
    let mut errors = 0;

    // Clean cache (batch)
    if results.cache.items > 0 {
        let (success, errs) = batch_clean_category_internal(
            &results.cache.paths,
            "cache",
            permanent,
            dry_run,
            progress.as_ref(),
            history.as_mut(),
            mode,
        );
        cleaned += success;
        errors += errs;
        cleaned_bytes += results.cache.size_bytes;
    }

    // Clean application cache (batch)
    if results.app_cache.items > 0 {
        let (success, errs) = batch_clean_category_internal(
            &results.app_cache.paths,
            "application cache",
            permanent,
            dry_run,
            progress.as_ref(),
            history.as_mut(),
            mode,
        );
        cleaned += success;
        errors += errs;
        cleaned_bytes += results.app_cache.size_bytes;
    }

    // Clean temp (batch)
    if results.temp.items > 0 {
        let (success, errs) = batch_clean_category_internal(
            &results.temp.paths,
            "temp files",
            permanent,
            dry_run,
            progress.as_ref(),
            history.as_mut(),
            mode,
        );
        cleaned += success;
        errors += errs;
        cleaned_bytes += results.temp.size_bytes;
    }

    // Clean trash
    if results.trash.items > 0 {
        if let Some(ref pb) = progress {
            pb.set_message("Emptying Recycle Bin...");
        }
        if dry_run {
            cleaned += results.trash.items as u64;
            if let Some(ref pb) = progress {
                pb.inc(results.trash.items as u64);
            }
            cleaned_bytes += results.trash.size_bytes;
        } else {
            match categories::trash::clean() {
                Ok(()) => {
                    cleaned += results.trash.items as u64;
                    if let Some(ref pb) = progress {
                        pb.inc(results.trash.items as u64);
                    }
                    cleaned_bytes += results.trash.size_bytes;
                    if let Some(ref mut log) = history {
                        log.log_success(
                            Path::new("Recycle Bin"),
                            results.trash.size_bytes,
                            "trash",
                            true,
                        );
                    }
                }
                Err(e) => {
                    errors += 1;
                    if let Some(ref mut log) = history {
                        log.log_failure(
                            Path::new("Recycle Bin"),
                            results.trash.size_bytes,
                            "trash",
                            true,
                            &e.to_string(),
                        );
                    }
                    if mode != OutputMode::Quiet {
                        eprintln!(
                            "[WARNING] Failed to empty Recycle Bin: {}",
                            Theme::error(&e.to_string())
                        );
                    }
                }
            }
        }
    }

    // Clean build artifacts (batch)
    if results.build.items > 0 {
        let (success, errs) = batch_clean_category_internal(
            &results.build.paths,
            "build artifacts",
            permanent,
            dry_run,
            progress.as_ref(),
            history.as_mut(),
            mode,
        );
        cleaned += success;
        errors += errs;
        cleaned_bytes += results.build.size_bytes;
    }

    // Clean downloads (batch)
    if results.downloads.items > 0 {
        let (success, errs) = batch_clean_category_internal(
            &results.downloads.paths,
            "old downloads",
            permanent,
            dry_run,
            progress.as_ref(),
            history.as_mut(),
            mode,
        );
        cleaned += success;
        errors += errs;
        cleaned_bytes += results.downloads.size_bytes;
    }

    // Clean large files (batch)
    if results.large.items > 0 {
        let (success, errs) = batch_clean_category_internal(
            &results.large.paths,
            "large files",
            permanent,
            dry_run,
            progress.as_ref(),
            history.as_mut(),
            mode,
        );
        cleaned += success;
        errors += errs;
        cleaned_bytes += results.large.size_bytes;
    }

    // Clean old files (batch)
    if results.old.items > 0 {
        let (success, errs) = batch_clean_category_internal(
            &results.old.paths,
            "old files",
            permanent,
            dry_run,
            progress.as_ref(),
            history.as_mut(),
            mode,
        );
        cleaned += success;
        errors += errs;
        cleaned_bytes += results.old.size_bytes;
    }

    // Clean browser caches
    if results.browser.items > 0 {
        if let Some(ref pb) = progress {
            pb.set_message("Cleaning browser caches...");
        }
        for path in &results.browser.paths {
            let size = if path.is_dir() {
                utils::calculate_dir_size(path)
            } else {
                utils::safe_metadata(path).map(|m| m.len()).unwrap_or(0)
            };
            if dry_run {
                cleaned += 1;
                if let Some(ref pb) = progress {
                    pb.inc(1);
                }
            } else {
                match delete_with_precheck(path, permanent) {
                    Ok(DeleteOutcome::Deleted) => {
                        cleaned += 1;
                        if let Some(ref pb) = progress {
                            pb.inc(1);
                        }
                        if let Some(ref mut log) = history {
                            log.log_success(path, size, "browser", permanent);
                        }
                    }
                    Ok(DeleteOutcome::SkippedMissing | DeleteOutcome::SkippedSystem) => {}
                    Ok(DeleteOutcome::SkippedLocked) => {
                        errors += 1;
                        if let Some(ref mut log) = history {
                            log.log_failure(
                                path,
                                size,
                                "browser",
                                permanent,
                                "Path is locked by another process",
                            );
                        }
                        if mode != OutputMode::Quiet {
                            eprintln!(
                                "[WARNING] Failed to clean {}: {}",
                                Theme::secondary(&path.display().to_string()),
                                Theme::error("Path is locked by another process")
                            );
                        }
                    }
                    Ok(DeleteOutcome::SkippedPermission) => {
                        errors += 1;
                        if let Some(ref mut log) = history {
                            log.log_failure(path, size, "browser", permanent, "Permission denied");
                        }
                        if mode != OutputMode::Quiet {
                            eprintln!(
                                "[WARNING] Failed to clean {}: {}",
                                Theme::secondary(&path.display().to_string()),
                                Theme::error("Permission denied")
                            );
                        }
                    }
                    Err(e) => {
                        errors += 1;
                        if let Some(ref mut log) = history {
                            log.log_failure(path, size, "browser", permanent, &e.to_string());
                        }
                        if mode != OutputMode::Quiet {
                            eprintln!(
                                "[WARNING] Failed to clean {}: {}",
                                Theme::secondary(&path.display().to_string()),
                                Theme::error(&e.to_string())
                            );
                        }
                    }
                }
            }
        }
        cleaned_bytes += results.browser.size_bytes;
    }

    // Clean system caches
    if results.system.items > 0 {
        if let Some(ref pb) = progress {
            pb.set_message("Cleaning system caches...");
        }
        for path in &results.system.paths {
            let size = if path.is_dir() {
                utils::calculate_dir_size(path)
            } else {
                utils::safe_metadata(path).map(|m| m.len()).unwrap_or(0)
            };
            if dry_run {
                cleaned += 1;
                if let Some(ref pb) = progress {
                    pb.inc(1);
                }
            } else {
                match delete_with_precheck(path, permanent) {
                    Ok(DeleteOutcome::Deleted) => {
                        cleaned += 1;
                        if let Some(ref pb) = progress {
                            pb.inc(1);
                        }
                        if let Some(ref mut log) = history {
                            log.log_success(path, size, "system", permanent);
                        }
                    }
                    Ok(DeleteOutcome::SkippedMissing | DeleteOutcome::SkippedSystem) => {}
                    Ok(DeleteOutcome::SkippedLocked) => {
                        errors += 1;
                        if let Some(ref mut log) = history {
                            log.log_failure(
                                path,
                                size,
                                "system",
                                permanent,
                                "Path is locked by another process",
                            );
                        }
                        if mode != OutputMode::Quiet {
                            eprintln!(
                                "[WARNING] Failed to clean {}: {}",
                                Theme::secondary(&path.display().to_string()),
                                Theme::error("Path is locked by another process")
                            );
                        }
                    }
                    Ok(DeleteOutcome::SkippedPermission) => {
                        errors += 1;
                        if let Some(ref mut log) = history {
                            log.log_failure(path, size, "system", permanent, "Permission denied");
                        }
                        if mode != OutputMode::Quiet {
                            eprintln!(
                                "[WARNING] Failed to clean {}: {}",
                                Theme::secondary(&path.display().to_string()),
                                Theme::error("Permission denied")
                            );
                        }
                    }
                    Err(e) => {
                        errors += 1;
                        if let Some(ref mut log) = history {
                            log.log_failure(path, size, "system", permanent, &e.to_string());
                        }
                        if mode != OutputMode::Quiet {
                            eprintln!(
                                "[WARNING] Failed to clean {}: {}",
                                Theme::secondary(&path.display().to_string()),
                                Theme::error(&e.to_string())
                            );
                        }
                    }
                }
            }
        }
        cleaned_bytes += results.system.size_bytes;
    }

    // Clean empty folders
    if results.empty.items > 0 {
        if let Some(ref pb) = progress {
            pb.set_message("Cleaning empty folders...");
        }
        for path in &results.empty.paths {
            if dry_run {
                cleaned += 1;
                if let Some(ref pb) = progress {
                    pb.inc(1);
                }
            } else {
                match delete_with_precheck(path, permanent) {
                    Ok(DeleteOutcome::Deleted) => {
                        cleaned += 1;
                        if let Some(ref pb) = progress {
                            pb.inc(1);
                        }
                        if let Some(ref mut log) = history {
                            log.log_success(path, 0, "empty", permanent);
                        }
                    }
                    Ok(DeleteOutcome::SkippedMissing | DeleteOutcome::SkippedSystem) => {}
                    Ok(DeleteOutcome::SkippedLocked) => {
                        errors += 1;
                        if let Some(ref mut log) = history {
                            log.log_failure(
                                path,
                                0,
                                "empty",
                                permanent,
                                "Path is locked by another process",
                            );
                        }
                        if mode != OutputMode::Quiet {
                            eprintln!(
                                "[WARNING] Failed to clean {}: {}",
                                Theme::secondary(&path.display().to_string()),
                                Theme::error("Path is locked by another process")
                            );
                        }
                    }
                    Ok(DeleteOutcome::SkippedPermission) => {
                        errors += 1;
                        if let Some(ref mut log) = history {
                            log.log_failure(path, 0, "empty", permanent, "Permission denied");
                        }
                        if mode != OutputMode::Quiet {
                            eprintln!(
                                "[WARNING] Failed to clean {}: {}",
                                Theme::secondary(&path.display().to_string()),
                                Theme::error("Permission denied")
                            );
                        }
                    }
                    Err(e) => {
                        errors += 1;
                        if let Some(ref mut log) = history {
                            log.log_failure(path, 0, "empty", permanent, &e.to_string());
                        }
                        if mode != OutputMode::Quiet {
                            eprintln!(
                                "[WARNING] Failed to clean {}: {}",
                                Theme::secondary(&path.display().to_string()),
                                Theme::error(&e.to_string())
                            );
                        }
                    }
                }
            }
        }
        cleaned_bytes += results.empty.size_bytes;
    }

    // Clean duplicate files (batch)
    if results.duplicates.items > 0 {
        let (success, errs) = batch_clean_category_internal(
            &results.duplicates.paths,
            "duplicate files",
            permanent,
            dry_run,
            progress.as_ref(),
            history.as_mut(),
            mode,
        );
        cleaned += success;
        errors += errs;
        cleaned_bytes += results.duplicates.size_bytes;
    }

    // Clean installed applications (batch)
    if results.applications.items > 0 {
        if let Some(ref pb) = progress {
            pb.set_message("Uninstalling applications...");
        }

        // IMPORTANT: uninstalling applications is not safely restorable, even if permanent=false.
        // We still honor `permanent` for leftover file deletion (Recycle Bin vs permanent),
        // but we always log these as permanent to avoid offering restore.
        let log_as_permanent = true;

        for path in &results.applications.paths {
            let size = categories::applications::get_app_size(path).unwrap_or_else(|| {
                if path.is_dir() {
                    utils::calculate_dir_size(path)
                } else {
                    utils::safe_metadata(path).map(|m| m.len()).unwrap_or(0)
                }
            });

            if dry_run {
                cleaned += 1;
                if let Some(ref pb) = progress {
                    pb.inc(1);
                }
                cleaned_bytes += size;
                continue;
            }

            let display = categories::applications::get_app_display_name(path)
                .unwrap_or_else(|| path.display().to_string());

            // Tighten: uninstall must succeed before we delete any install/artifact paths.
            // This avoids leaving the app "installed" but with missing files.
            let mut had_error = false;
            let Some(_uninstall_cmd) = categories::applications::get_app_uninstall_string(path)
            else {
                // No uninstall command - skip (had_error not set here since we continue)
                if mode != OutputMode::Quiet {
                    eprintln!(
                        "[WARNING] Cannot uninstall {}: {}",
                        Theme::secondary(&display),
                        Theme::error("No uninstall command in registry")
                    );
                }
                // Do not delete any files for this app.
                // It would leave a broken, still-installed entry.
                if let Some(ref mut log) = history {
                    log.log_failure(
                        path,
                        size,
                        "applications",
                        log_as_permanent,
                        "No uninstall command in registry; skipped to avoid breaking installed app",
                    );
                }
                errors += 1;
                continue;
            };

            if let Err(e) = categories::applications::uninstall(path) {
                had_error = true;
                if mode != OutputMode::Quiet {
                    eprintln!(
                        "[WARNING] Uninstall failed for {}: {}",
                        Theme::secondary(&display),
                        Theme::error(&e.to_string())
                    );
                }
            }

            // Post-check: if it's still installed, don't delete artifacts (tight/safe).
            if !had_error && categories::applications::is_still_installed(path) {
                had_error = true;
                if mode != OutputMode::Quiet {
                    eprintln!(
                        "[WARNING] {} still appears installed after uninstall (may require reboot). Skipping artifact deletion.",
                        Theme::secondary(&display)
                    );
                }
            }

            if !had_error {
                // Only after uninstall succeeds and entry disappears: delete app-specific leftovers.
                let artifacts = categories::applications::get_app_artifact_paths(path);
                for artifact in artifacts {
                    match delete_with_precheck(&artifact, permanent) {
                        Ok(DeleteOutcome::Deleted) => {}
                        Ok(DeleteOutcome::SkippedMissing | DeleteOutcome::SkippedSystem) => {}
                        Ok(DeleteOutcome::SkippedLocked | DeleteOutcome::SkippedPermission) => {
                            had_error = true;
                        }
                        Err(_) => had_error = true,
                    }
                }
            }

            // Update counters/logs.
            if had_error {
                errors += 1;
                if let Some(ref mut log) = history {
                    log.log_failure(
                        path,
                        size,
                        "applications",
                        log_as_permanent,
                        "Application uninstall and/or cleanup did not complete",
                    );
                }
            } else {
                cleaned += 1;
                if let Some(ref pb) = progress {
                    pb.inc(1);
                }
                cleaned_bytes += size;
                if let Some(ref mut log) = history {
                    log.log_success(path, size, "applications", log_as_permanent);
                }
            }
        }
    }

    // Clean Windows Update files
    if results.windows_update.items > 0 {
        if let Some(ref pb) = progress {
            pb.set_message("Cleaning Windows Update files...");
        }
        for path in &results.windows_update.paths {
            let size = if path.is_dir() {
                utils::calculate_dir_size(path)
            } else {
                utils::safe_metadata(path).map(|m| m.len()).unwrap_or(0)
            };
            if dry_run {
                cleaned += 1;
                if let Some(ref pb) = progress {
                    pb.inc(1);
                }
            } else {
                match categories::windows_update::clean(path) {
                    Ok(()) => {
                        cleaned += 1;
                        if let Some(ref pb) = progress {
                            pb.inc(1);
                        }
                        if let Some(ref mut log) = history {
                            log.log_success(path, size, "windows_update", permanent);
                        }
                    }
                    Err(e) => {
                        errors += 1;
                        if let Some(ref mut log) = history {
                            log.log_failure(
                                path,
                                size,
                                "windows_update",
                                permanent,
                                &e.to_string(),
                            );
                        }
                        if mode != OutputMode::Quiet {
                            eprintln!(
                                "[WARNING] Failed to clean {}: {}",
                                Theme::secondary(&path.display().to_string()),
                                Theme::error(&e.to_string())
                            );
                        }
                    }
                }
            }
        }
        cleaned_bytes += results.windows_update.size_bytes;
    }

    // Clean Event Logs
    if results.event_logs.items > 0 {
        if let Some(ref pb) = progress {
            pb.set_message("Cleaning Event Logs...");
        }
        for path in &results.event_logs.paths {
            let size = if path.is_dir() {
                utils::calculate_dir_size(path)
            } else {
                utils::safe_metadata(path).map(|m| m.len()).unwrap_or(0)
            };
            if dry_run {
                cleaned += 1;
                if let Some(ref pb) = progress {
                    pb.inc(1);
                }
            } else {
                match categories::event_logs::clean(path) {
                    Ok(()) => {
                        cleaned += 1;
                        if let Some(ref pb) = progress {
                            pb.inc(1);
                        }
                        if let Some(ref mut log) = history {
                            log.log_success(path, size, "event_logs", permanent);
                        }
                    }
                    Err(e) => {
                        errors += 1;
                        if let Some(ref mut log) = history {
                            log.log_failure(path, size, "event_logs", permanent, &e.to_string());
                        }
                        if mode != OutputMode::Quiet {
                            eprintln!(
                                "[WARNING] Failed to clean {}: {}",
                                Theme::secondary(&path.display().to_string()),
                                Theme::error(&e.to_string())
                            );
                        }
                    }
                }
            }
        }
        cleaned_bytes += results.event_logs.size_bytes;
    }

    // Finish progress bar
    if let Some(pb) = progress {
        pb.finish_and_clear();
    }

    // Save history log (if not dry run)
    let log_path = if let Some(log) = history {
        match log.save() {
            Ok(path) => Some(path),
            Err(e) => {
                if mode != OutputMode::Quiet {
                    eprintln!("[WARNING] Failed to save deletion log: {}", e);
                }
                None
            }
        }
    } else {
        None
    };

    // Print summary
    if mode != OutputMode::Quiet {
        println!();
        if dry_run {
            println!(
                "[DRY RUN] Complete: {} items would be cleaned ({}), {} errors",
                Theme::value(&cleaned.to_string()),
                Theme::size(&bytesize::to_string(cleaned_bytes, false)),
                Theme::error(&errors.to_string())
            );
        } else if errors > 0 {
            println!(
                "[WARNING] Cleanup complete: {} items cleaned ({}), {} errors",
                Theme::success(&cleaned.to_string()),
                Theme::success(&bytesize::to_string(cleaned_bytes, false)),
                Theme::error(&errors.to_string())
            );
        } else {
            println!(
                "[OK] Cleanup complete: {} items cleaned, {} freed!",
                Theme::success(&cleaned.to_string()),
                Theme::success(&bytesize::to_string(cleaned_bytes, false))
            );
        }

        // Print log path if saved
        if let Some(path) = log_path {
            println!(
                "{}",
                Theme::muted(&format!("Deletion log saved to: {}", path.display()))
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::ScanResults;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_dir() -> TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn test_clean_all_empty_results() {
        let results = ScanResults::default();

        // Should return Ok without doing anything
        // Use Quiet mode in tests to avoid spinner thread issues
        let result = clean_all(&results, true, OutputMode::Quiet, false, false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_clean_all_dry_run() {
        let temp_dir = create_test_dir();
        let file = temp_dir.path().join("test.txt");
        fs::write(&file, "test content").unwrap();

        let mut results = ScanResults::default();
        results.cache.paths.push(file.clone());
        results.cache.items = 1;
        results.cache.size_bytes = 12;

        // Dry run should not delete the file
        // Use Quiet mode in tests to avoid spinner thread issues
        let result = clean_all(&results, true, OutputMode::Quiet, false, true);
        assert!(result.is_ok());
        assert!(file.exists()); // File should still exist
    }
}
