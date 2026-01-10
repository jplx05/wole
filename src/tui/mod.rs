//! TUI module for interactive terminal interface
//!
//! Provides a full-screen terminal UI using Ratatui for interactive file cleanup

pub mod events;
pub mod screens;
pub mod state;
pub mod theme;
pub mod widgets;

use anyhow::{Context, Result};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::stdout;
use std::time::Duration;

use self::events::{handle_event, handle_mouse_event};
use self::screens::render;
use self::state::AppState;
use crate::cleaner;
use crate::cli::ScanOptions;
use crate::config::Config;
use crate::output::OutputMode;
use crate::restore;
use crate::scanner;

/// Run the TUI application
pub fn run(initial_state: Option<AppState>) -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Initialize app state (use provided or create new)
    let mut app_state = initial_state.unwrap_or_default();
    let mut scan_pending = false;
    let mut clean_pending = false;

    // Main event loop
    loop {
        // Increment tick for animations
        app_state.tick = app_state.tick.wrapping_add(1);

        // Auto-refresh Status screen every 2 seconds
        if let crate::tui::state::Screen::Status { ref mut status, ref mut last_refresh } = app_state.screen {
            if last_refresh.elapsed().as_secs() >= 2 {
                use sysinfo::System;
                use crate::status::gather_status;
                
                // Create system and gather status (optimized refresh inside)
                let mut system = System::new();
                if let Ok(new_status) = gather_status(&mut system) {
                    *status = new_status;
                    *last_refresh = std::time::Instant::now();
                }
            }
        }

        terminal.draw(|f| render(f, &mut app_state))?;

        // Handle pending restore
        if let crate::tui::state::Screen::Restore {
            progress: None,
            result: None,
            restore_all_bin,
        } = &app_state.screen
        {
            // Initialize restore progress
            let result = if *restore_all_bin {
                // For restore all bin, get count from Recycle Bin
                trash::os_limited::list()
                    .map(|items| items.len())
                    .map_err(|e| anyhow::anyhow!("Failed to list Recycle Bin: {}", e))
            } else {
                // For restore from last deletion, get count from history
                restore::get_restore_count()
                    .map_err(|e| anyhow::anyhow!("Failed to get restore count: {}", e))
            };
            
            match result {
                Ok(total) => {
                    app_state.screen = crate::tui::state::Screen::Restore {
                        progress: Some(crate::tui::state::RestoreProgress {
                            current_path: None,
                            restored: 0,
                            total,
                            errors: 0,
                            not_found: 0,
                            restored_bytes: 0,
                        }),
                        result: None,
                        restore_all_bin: *restore_all_bin,
                    };
                }
                Err(e) => {
                    // On error, show error message and return to dashboard
                    eprintln!("Restore error: {}", e);
                    app_state.screen = crate::tui::state::Screen::Dashboard;
                }
            }
            continue;
        }

        // Handle restore in progress
        if let crate::tui::state::Screen::Restore {
            ref mut progress,
            result: None,
            restore_all_bin,
        } = app_state.screen
        {
            if progress.is_some() {
                // Perform restore operation with progress updates
                let result = if restore_all_bin {
                    perform_restore_all_bin(&mut app_state, &mut terminal)
                } else {
                    perform_restore(&mut app_state, &mut terminal)
                };
                
                match result {
                    Ok(result) => {
                        app_state.screen = crate::tui::state::Screen::Restore {
                            progress: None,
                            result: Some(crate::tui::state::RestoreResult {
                                restored: result.restored,
                                restored_bytes: result.restored_bytes,
                                errors: result.errors,
                                not_found: result.not_found,
                            }),
                            restore_all_bin,
                        };
                    }
                    Err(e) => {
                        // On error, show error message and return to dashboard
                        eprintln!("Restore error: {}", e);
                        app_state.screen = crate::tui::state::Screen::Dashboard;
                    }
                }
            }
            continue;
        }

        // Handle pending scan
        if scan_pending {
            scan_pending = false;

            // Check if we're still in Scanning screen (might have been cancelled)
            if !matches!(app_state.screen, crate::tui::state::Screen::Scanning { .. }) {
                // Screen changed (likely cancelled), skip scanning
                continue;
            }

            // Check if this is a Disk Insights scan (Analyze action)
            if app_state.pending_action == crate::tui::state::PendingAction::Analyze {
                // Perform Disk Insights scan
                let scan_path = if let crate::tui::state::Screen::Scanning { ref progress } =
                    app_state.screen
                {
                    progress.current_path.clone().unwrap_or_else(|| {
                        // Default to user directory
                        if let Ok(userprofile) = std::env::var("USERPROFILE") {
                            std::path::PathBuf::from(&userprofile)
                        } else {
                            std::env::current_dir()
                                .unwrap_or_else(|_| std::path::PathBuf::from("."))
                        }
                    })
                } else {
                    // Default to user directory
                    if let Ok(userprofile) = std::env::var("USERPROFILE") {
                        std::path::PathBuf::from(&userprofile)
                    } else {
                        std::env::current_dir()
                            .unwrap_or_else(|_| std::path::PathBuf::from("."))
                    }
                };

                // Update progress message
                if let crate::tui::state::Screen::Scanning { ref mut progress } = app_state.screen {
                    progress.current_category = "Disk Insights".to_string();
                    progress.current_path = Some(scan_path.clone());
                }

                // Redraw to show the scanning state before blocking
                terminal.draw(|f| render(f, &mut app_state))?;

                // Perform disk insights scan
                use crate::disk_usage::{scan_directory, SortBy};
                use crate::utils;
                
                // Determine appropriate depth based on scan path and config
                let config = crate::config::Config::load();
                let is_root_disk = scan_path == utils::get_root_disk_path();
                let effective_depth = if is_root_disk {
                    config.ui.scan_depth_entire_disk
                } else {
                    config.ui.scan_depth_user
                };
                
                match scan_directory(&scan_path, effective_depth) {
                    Ok(insights) => {
                        // Check if scan was cancelled
                        if !matches!(app_state.screen, crate::tui::state::Screen::Scanning { .. }) {
                            continue;
                        }

                        // Show Disk Insights screen
                        app_state.screen = crate::tui::state::Screen::DiskInsights {
                            insights,
                            current_path: scan_path,
                            cursor: 0,
                            sort_by: SortBy::Size,
                        };
                        app_state.pending_action = crate::tui::state::PendingAction::None;
                    }
                    Err(e) => {
                        // On error, return to dashboard
                        eprintln!("Disk insights scan error: {}", e);
                        app_state.screen = crate::tui::state::Screen::Dashboard;
                        app_state.pending_action = crate::tui::state::PendingAction::None;
                    }
                }
                continue;
            }

            // Regular cleanable file scan
            if let crate::tui::state::Screen::Scanning { ref mut progress } = app_state.screen {
                // Initialize progress to 0% for all categories
                for cat_progress in &mut progress.category_progress {
                    cat_progress.progress_pct = 0.0;
                }
            }

            // Redraw to show the scanning state before blocking
            terminal.draw(|f| render(f, &mut app_state))?;

            // Perform actual scan with progress updates
            match perform_scan_with_progress(&mut app_state, &mut terminal) {
                Ok(()) => {
                    // Check if scan was cancelled (screen changed during scan)
                    if !matches!(app_state.screen, crate::tui::state::Screen::Scanning { .. }) {
                        // Scan was cancelled, screen already changed to Dashboard
                        continue;
                    }

                    // Mark all categories as complete
                    if let crate::tui::state::Screen::Scanning { ref mut progress } =
                        app_state.screen
                    {
                        for cat_progress in &mut progress.category_progress {
                            cat_progress.completed = true;
                            cat_progress.progress_pct = 1.0;
                        }
                    }
                    app_state.flatten_results();

                    // Check which action was selected to determine next screen
                    match app_state.pending_action {
                        crate::tui::state::PendingAction::Clean => {
                            // If there are selected items, proceed to confirmation
                            if app_state.selected_count() > 0 {
                                // Snapshot current selection when entering confirm screen
                                // and cache groups so ordering stays stable across redraws.
                                // (Without this, HashSet iteration can reorder the file list each frame.)
                                app_state.confirm_snapshot = app_state.selected_items.clone();
                                app_state.cache_confirm_groups();
                                app_state.cursor = 0;
                                app_state.scroll_offset = 0;
                                app_state.screen =
                                    crate::tui::state::Screen::Confirm { permanent: false };
                            } else {
                                // No items selected, show results
                                app_state.screen = crate::tui::state::Screen::Results;
                            }
                            app_state.pending_action = crate::tui::state::PendingAction::None;
                        }
                        crate::tui::state::PendingAction::Analyze => {
                            // This shouldn't happen here (handled above), but just in case
                            app_state.screen = crate::tui::state::Screen::Results;
                            app_state.pending_action = crate::tui::state::PendingAction::None;
                        }
                        crate::tui::state::PendingAction::None => {
                            // Regular scan - show results
                            app_state.screen = crate::tui::state::Screen::Results;
                        }
                    }
                }
                Err(e) => {
                    // On error, return to dashboard
                    eprintln!("Scan error: {}", e);
                    app_state.screen = crate::tui::state::Screen::Dashboard;
                    app_state.pending_action = crate::tui::state::PendingAction::None;
                }
            }
            continue;
        }

        // Handle pending cleanup
        if clean_pending {
            clean_pending = false;
            // Extract values before pattern match to avoid borrow conflicts
            let permanent_delete = app_state.permanent_delete;

            // Check if we're in Cleaning screen
            if !matches!(app_state.screen, crate::tui::state::Screen::Cleaning { .. }) {
                continue; // Not in cleaning screen, skip
            }

            // Now perform cleanup with real-time updates
            match perform_cleanup(&mut app_state, permanent_delete, &mut terminal) {
                Ok((cleaned, cleaned_bytes, errors)) => {
                    app_state.screen = crate::tui::state::Screen::Success {
                        cleaned,
                        cleaned_bytes,
                        errors,
                    };
                    app_state.permanent_delete = false; // Reset flag
                }
                Err(e) => {
                    eprintln!("Cleanup error: {}", e);
                    app_state.screen = crate::tui::state::Screen::Results;
                    app_state.permanent_delete = false; // Reset flag
                }
            }
            continue;
        }

        // Use polling with timeout for animation updates
        if event::poll(Duration::from_millis(100))? {
            // Read and handle the first event
            match event::read()? {
                Event::Key(key) => {
                    if key.kind == KeyEventKind::Press {
                        match handle_event(&mut app_state, key.code, key.modifiers) {
                            events::EventResult::Quit => break,
                            events::EventResult::Continue => {
                                // Check if we need to trigger a scan
                                if let crate::tui::state::Screen::Scanning { .. } = app_state.screen
                                {
                                    scan_pending = true;
                                }
                                // Check if we need to trigger cleanup
                                if let crate::tui::state::Screen::Cleaning { .. } = app_state.screen
                                {
                                    clean_pending = true;
                                }
                            }
                        }
                    }
                }
                Event::Mouse(mouse) => match handle_mouse_event(&mut app_state, mouse) {
                    events::EventResult::Quit => break,
                    events::EventResult::Continue => {}
                },
                _ => {}
            }

            // Drain any other pending events to prevent lag (smooth scrolling)
            let mut quit = false;
            while event::poll(Duration::from_millis(0))? {
                match event::read()? {
                    Event::Key(key) => {
                        if key.kind == KeyEventKind::Press {
                            match handle_event(&mut app_state, key.code, key.modifiers) {
                                events::EventResult::Quit => {
                                    quit = true;
                                    break;
                                }
                                events::EventResult::Continue => {
                                    if let crate::tui::state::Screen::Scanning { .. } =
                                        app_state.screen
                                    {
                                        scan_pending = true;
                                    }
                                    if let crate::tui::state::Screen::Cleaning { .. } =
                                        app_state.screen
                                    {
                                        clean_pending = true;
                                    }
                                }
                            }
                        }
                    }
                    Event::Mouse(mouse) => match handle_mouse_event(&mut app_state, mouse) {
                        events::EventResult::Quit => {
                            quit = true;
                            break;
                        }
                        events::EventResult::Continue => {}
                    },
                    _ => {}
                }
            }
            if quit {
                break;
            }
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}

/// Perform a scan with progress updates
fn perform_scan_with_progress(
    app_state: &mut AppState,
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
) -> anyhow::Result<()> {
    // Get enabled categories
    let enabled_categories: Vec<String> = app_state
        .categories
        .iter()
        .filter(|cat| cat.enabled)
        .map(|cat| cat.name.clone())
        .collect();

    let total_categories = enabled_categories.len();

    // Build scan options from selected categories
    let mut cache = false;
    let mut app_cache = false;
    let mut temp = false;
    let mut trash = false;
    let mut build = false;
    let mut downloads = false;
    let mut large = false;
    let mut old = false;
    let mut applications = false;
    let mut browser = false;
    let mut system = false;
    let mut empty = false;
    let mut duplicates = false;

    for cat in &app_state.categories {
        match cat.name.as_str() {
            "Package cache" => cache = cat.enabled,
            "Application cache" => app_cache = cat.enabled,
            "Temp" => temp = cat.enabled,
            "Trash" => trash = cat.enabled,
            "Build" => build = cat.enabled,
            "Downloads" => downloads = cat.enabled,
            "Large" => large = cat.enabled,
            "Old" => old = cat.enabled,
            "Applications" => applications = cat.enabled,
            "Browser" => browser = cat.enabled,
            "System" => system = cat.enabled,
            "Empty" => empty = cat.enabled,
            "Duplicates" => duplicates = cat.enabled,
            _ => {}
        }
    }

    // Load config first to use its values (create default file if needed)
    let config = Config::load_or_create();

    // Use config values for thresholds
    let min_size_bytes = config.thresholds.min_size_mb * 1024 * 1024;

    let options = ScanOptions {
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
        project_age_days: config.thresholds.project_age_days,
        min_age_days: config.thresholds.min_age_days,
        min_size_bytes,
    };

    // Update progress incrementally before scan (simulated progress)
    // Simulate progress by updating each category incrementally
    for (idx, cat_name) in enabled_categories.iter().enumerate() {
        // Check if scan was cancelled (screen changed from Scanning)
        if !matches!(app_state.screen, crate::tui::state::Screen::Scanning { .. }) {
            // Scan was cancelled, return early
            return Ok(());
        }

        // Process any pending events (non-blocking) to allow cancellation
        while event::poll(Duration::from_millis(0)).unwrap_or(false) {
            if let Ok(Event::Key(key)) = event::read() {
                if key.kind == KeyEventKind::Press {
                    handle_event(app_state, key.code, key.modifiers);
                    // Check again if screen changed
                    if !matches!(app_state.screen, crate::tui::state::Screen::Scanning { .. }) {
                        return Ok(());
                    }
                }
            }
        }

        // Update progress in a separate scope to drop the borrow before drawing
        {
            if let crate::tui::state::Screen::Scanning { ref mut progress } = app_state.screen {
                progress.current_category = cat_name.clone();
                progress.total_scanned = idx + 1; // Update scanned count incrementally
                for cat_progress in &mut progress.category_progress {
                    if cat_progress.name == *cat_name {
                        // Progress from 0.1 to 0.9 based on position
                        cat_progress.progress_pct =
                            ((idx + 1) as f32 / (total_categories + 1) as f32) * 0.9;
                    }
                }
            }
        }
        // Redraw to show progress update (mutable borrow is dropped)
        let _ = terminal.draw(|f| render(f, app_state));
        // Small delay to make progress visible
        std::thread::sleep(Duration::from_millis(50));
    }

    // Check if scan was cancelled before starting the actual scan
    if !matches!(app_state.screen, crate::tui::state::Screen::Scanning { .. }) {
        // Scan was cancelled, return early
        return Ok(());
    }

    let results = scanner::scan_all(
        &app_state.scan_path,
        options,
        OutputMode::Quiet, // Quiet mode for TUI
        &config,
    )?;

    // Check if scan was cancelled after the scan completes
    if !matches!(app_state.screen, crate::tui::state::Screen::Scanning { .. }) {
        // Scan was cancelled, return early without processing results
        return Ok(());
    }

    // Update progress incrementally as we process results
    // Process each category and update totals incrementally
    let mut running_total_items = 0;
    let mut running_total_bytes = 0u64;

    // Update category progress with sizes and update totals incrementally
    for cat_progress_name in &enabled_categories {
        // Check if scan was cancelled (screen changed from Scanning)
        if !matches!(app_state.screen, crate::tui::state::Screen::Scanning { .. }) {
            // Scan was cancelled, return early without processing remaining results
            return Ok(());
        }

        // Process any pending events (non-blocking) to allow cancellation
        while event::poll(Duration::from_millis(0)).unwrap_or(false) {
            if let Ok(Event::Key(key)) = event::read() {
                if key.kind == KeyEventKind::Press {
                    handle_event(app_state, key.code, key.modifiers);
                    // Check again if screen changed
                    if !matches!(app_state.screen, crate::tui::state::Screen::Scanning { .. }) {
                        return Ok(());
                    }
                }
            }
        }

        let (items, size) = match cat_progress_name.as_str() {
            "Cache" => (results.cache.items, results.cache.size_bytes),
            "Temp" => (results.temp.items, results.temp.size_bytes),
            "Trash" => (results.trash.items, results.trash.size_bytes),
            "Build" => (results.build.items, results.build.size_bytes),
            "Downloads" => (results.downloads.items, results.downloads.size_bytes),
            "Large" => (results.large.items, results.large.size_bytes),
            "Old" => (results.old.items, results.old.size_bytes),
            "Browser" => (results.browser.items, results.browser.size_bytes),
            "System" => (results.system.items, results.system.size_bytes),
            "Empty" => (results.empty.items, results.empty.size_bytes),
            "Duplicates" => (results.duplicates.items, results.duplicates.size_bytes),
            _ => (0, 0),
        };

        // Update running totals
        running_total_items += items;
        running_total_bytes += size;

        // Update progress in a scoped block to drop the mutable borrow before rendering
        {
            if let crate::tui::state::Screen::Scanning { ref mut progress } = app_state.screen {
                // Find and update the matching category progress
                for cat_progress in &mut progress.category_progress {
                    if cat_progress.name == *cat_progress_name {
                        // Update progress state incrementally
                        cat_progress.size = Some(size);
                        cat_progress.completed = true;
                        cat_progress.progress_pct = 1.0;
                        break;
                    }
                }

                // Update totals in progress state
                progress.total_found = running_total_items;
                progress.total_size = running_total_bytes;
                progress.total_scanned = total_categories; // All categories scanned
            }
        }

        // Redraw to show incremental updates (mutable borrow is dropped)
        let _ = terminal.draw(|f| render(f, app_state));
        // Small delay to make updates visible
        std::thread::sleep(Duration::from_millis(30));
    }

    app_state.scan_results = Some(results);
    Ok(())
}

/// Perform cleanup of selected items with real-time progress updates
fn perform_cleanup(
    app_state: &mut AppState,
    permanent: bool,
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
) -> anyhow::Result<(u64, u64, usize)> {
    use crate::categories;

    // Collect all items to clean with their categories
    // Separate trash items since they're cleaned all at once
    let mut items_to_clean: Vec<(usize, String, std::path::PathBuf, u64)> = Vec::new();
    let mut trash_items: Vec<(usize, u64)> = Vec::new();
    let mut trash_total_bytes = 0u64;

    for &index in &app_state.selected_items {
        if let Some(item) = app_state.all_items.get(index) {
            if item.category == "Trash" {
                trash_items.push((index, item.size_bytes));
                trash_total_bytes += item.size_bytes;
            } else {
                items_to_clean.push((
                    index,
                    item.category.clone(),
                    item.path.clone(),
                    item.size_bytes,
                ));
            }
        }
    }

    // Handle trash items first (all at once)
    let mut trash_cleaned = 0u64;
    let mut trash_errors = 0usize;

    if !trash_items.is_empty() {
        if let crate::tui::state::Screen::Cleaning { ref mut progress } = app_state.screen {
            progress.current_category = "Cleaning Trash...".to_string();
            progress.current_path = Some(std::path::PathBuf::from("Recycle Bin"));
        }
        let _ = terminal.draw(|f| render(f, app_state));

        match categories::trash::clean() {
            Ok(()) => {
                // All trash items are cleaned
                trash_cleaned = trash_items.len() as u64;
            }
            Err(_e) => {
                // If trash cleaning fails, all trash items failed
                trash_errors = trash_items.len();
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    let total = (items_to_clean.len() + trash_items.len()) as u64;
    let mut cleaned = trash_cleaned;
    let mut cleaned_bytes = if trash_cleaned > 0 {
        trash_total_bytes
    } else {
        0
    };
    let mut errors = trash_errors;

    // Initialize progress
    if let crate::tui::state::Screen::Cleaning { ref mut progress } = app_state.screen {
        progress.total = total;
        progress.cleaned = trash_cleaned;
        progress.errors = trash_errors;
    }

    // === BATCH DELETION (10-50x faster than one-by-one) ===
    // Group items by category for optimized processing
    // Special categories (Browser, System, Empty) need individual handling
    // All other categories can be batch deleted together

    let mut special_items: Vec<(usize, String, std::path::PathBuf, u64)> = Vec::new();
    let mut batch_items: Vec<(usize, std::path::PathBuf, u64)> = Vec::new();

    for (idx, category, path, size) in items_to_clean {
        match category.as_str() {
            "Browser" | "System" | "Empty" => {
                special_items.push((idx, category, path, size));
            }
            _ => {
                batch_items.push((idx, path, size));
            }
        }
    }

    // Handle special categories first (they need individual processing)
    if !special_items.is_empty() {
        if let crate::tui::state::Screen::Cleaning { ref mut progress } = app_state.screen {
            progress.current_category = "Cleaning special items...".to_string();
        }
        let _ = terminal.draw(|f| render(f, app_state));

        for (_idx, category, path, size_bytes) in special_items {
            let delete_result = match category.as_str() {
                "Browser" => categories::browser::clean(&path),
                "System" => categories::system::clean(&path),
                "Empty" => categories::empty::clean(&path),
                _ => unreachable!(),
            };

            match delete_result {
                Ok(()) => {
                    cleaned += 1;
                    cleaned_bytes += size_bytes;
                }
                Err(_) => {
                    errors += 1;
                }
            }
        }

        // Update progress
        if let crate::tui::state::Screen::Cleaning { ref mut progress } = app_state.screen {
            progress.cleaned = cleaned;
            progress.errors = errors;
        }
        let _ = terminal.draw(|f| render(f, app_state));
    }

    // Batch delete all remaining items (FAST PATH)
    if !batch_items.is_empty() {
        // Update UI to show batch deletion in progress
        if let crate::tui::state::Screen::Cleaning { ref mut progress } = app_state.screen {
            progress.current_category = format!("Batch deleting {} files...", batch_items.len());
            progress.current_path = None;
        }
        let _ = terminal.draw(|f| render(f, app_state));

        // Calculate total bytes for batch items
        let batch_total_bytes: u64 = batch_items.iter().map(|(_, _, size)| size).sum();

        // Extract just the paths for batch deletion
        let paths: Vec<std::path::PathBuf> =
            batch_items.iter().map(|(_, p, _)| p.clone()).collect();

        // Use the new batch deletion function (10-50x faster!)
        let (batch_success, batch_errors, _deleted_paths) =
            cleaner::clean_paths_batch(&paths, permanent);

        cleaned += batch_success as u64;
        errors += batch_errors;

        // Estimate cleaned bytes based on success ratio
        if batch_success > 0 {
            if batch_errors == 0 {
                // All succeeded - add all bytes
                cleaned_bytes += batch_total_bytes;
            } else {
                // Partial success - estimate based on ratio
                let ratio = batch_success as f64 / paths.len() as f64;
                cleaned_bytes += (batch_total_bytes as f64 * ratio) as u64;
            }
        }

        // Update final progress
        if let crate::tui::state::Screen::Cleaning { ref mut progress } = app_state.screen {
            progress.cleaned = cleaned;
            progress.errors = errors;
            progress.current_category = "Complete".to_string();
        }
        let _ = terminal.draw(|f| render(f, app_state));
    }

    // Final redraw to ensure UI is up to date
    let _ = terminal.draw(|f| render(f, app_state));

    // Remove cleaned items from the list
    let mut indices_to_remove: Vec<usize> = app_state.selected_items.iter().cloned().collect();
    indices_to_remove.sort();
    indices_to_remove.reverse(); // Remove from end to preserve indices

    for idx in indices_to_remove {
        app_state.all_items.remove(idx);
    }
    app_state.selected_items.clear();

    // Rebuild groups from remaining items so navigation back to Results works
    app_state.rebuild_groups_from_all_items();

    Ok((cleaned, cleaned_bytes, errors))
}

/// Perform restoration with real-time progress updates
fn perform_restore(
    app_state: &mut AppState,
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
) -> anyhow::Result<restore::RestoreResult> {
    // Get the most recent log
    use crate::history::{list_logs, load_log};
    let logs = list_logs()?;
    if logs.is_empty() {
        return Err(anyhow::anyhow!(
            "No deletion history found. Nothing to restore."
        ));
    }

    let latest_log = load_log(&logs[0])?;

    // Get current Recycle Bin contents
    let recycle_bin_items =
        trash::os_limited::list().context("Failed to list Recycle Bin contents")?;

    // Create a map of Recycle Bin items by normalized original path
    let mut bin_map: std::collections::HashMap<String, &trash::TrashItem> =
        std::collections::HashMap::new();
    for item in &recycle_bin_items {
        let original_path = item.original_parent.join(&item.name);
        let normalized =
            restore::normalize_path_for_comparison(&original_path.display().to_string());
        bin_map.insert(normalized, item);
    }

    let mut result = restore::RestoreResult::default();
    let mut files_since_redraw = 0;
    let mut last_redraw = std::time::Instant::now();
    const REDRAW_INTERVAL_MS: u64 = 50;
    const REDRAW_INTERVAL_FILES: usize = 5;

    // Process each record
    for record in &latest_log.records {
        if !record.success || record.permanent {
            continue;
        }

        let record_path = std::path::PathBuf::from(&record.path);
        let normalized_record_path = restore::normalize_path_for_comparison(&record.path);

        // Update progress state
        if let crate::tui::state::Screen::Restore {
            progress: Some(ref mut prog),
            ..
        } = app_state.screen
        {
            // Convert to relative path for display
            let relative_path_str =
                crate::utils::to_relative_path(&record_path, &app_state.scan_path);
            prog.current_path = Some(std::path::PathBuf::from(relative_path_str));
        }

        // Try to find exact match first (for files)
        if let Some(trash_item) = bin_map.get(&normalized_record_path) {
            match restore::restore_file(trash_item) {
                Ok(()) => {
                    result.restored += 1;
                    result.restored_bytes += record.size_bytes;

                    // Update progress
                    if let crate::tui::state::Screen::Restore {
                        progress: Some(ref mut prog),
                        ..
                    } = app_state.screen
                    {
                        prog.restored = result.restored;
                        prog.restored_bytes = result.restored_bytes;
                    }
                }
                Err(_e) => {
                    result.errors += 1;

                    // Update progress
                    if let crate::tui::state::Screen::Restore {
                        progress: Some(ref mut prog),
                        ..
                    } = app_state.screen
                    {
                        prog.errors = result.errors;
                    }
                }
            }
        } else {
            // No exact match - check if this was a directory
            // When a directory is deleted, Windows Recycle Bin stores individual files,
            // not the directory itself. So we need to find all items whose path starts
            // with the directory path.
            let normalized_record_path_with_sep = if normalized_record_path.ends_with('/') {
                normalized_record_path.clone()
            } else {
                format!("{}/", normalized_record_path)
            };

            // Find all Recycle Bin items that are children of this directory
            let mut found_any = false;
            let mut restored_count = 0;
            let mut restore_errors = 0;

            for (bin_path, trash_item) in &bin_map {
                // Check if this Recycle Bin item is inside the directory we're restoring
                if bin_path.starts_with(&normalized_record_path_with_sep) {
                    found_any = true;
                    match restore::restore_file(trash_item) {
                        Ok(()) => {
                            restored_count += 1;
                        }
                        Err(_e) => {
                            restore_errors += 1;
                        }
                    }
                }
            }

            if found_any {
                if restored_count > 0 {
                    result.restored += 1; // Count as one directory restored
                    result.restored_bytes += record.size_bytes; // Use the logged size
                }
                result.errors += restore_errors;

                // Update progress
                if let crate::tui::state::Screen::Restore {
                    progress: Some(ref mut prog),
                    ..
                } = app_state.screen
                {
                    prog.restored = result.restored;
                    prog.restored_bytes = result.restored_bytes;
                    prog.errors = result.errors;
                }
            } else {
                result.not_found += 1;

                // Update progress
                if let crate::tui::state::Screen::Restore {
                    progress: Some(ref mut prog),
                    ..
                } = app_state.screen
                {
                    prog.not_found = result.not_found;
                }
            }
        }

        files_since_redraw += 1;
        let should_redraw = files_since_redraw >= REDRAW_INTERVAL_FILES
            || last_redraw.elapsed().as_millis() >= REDRAW_INTERVAL_MS as u128;

        // Redraw terminal periodically
        if should_redraw {
            let _ = terminal.draw(|f| render(f, app_state));
            last_redraw = std::time::Instant::now();
            files_since_redraw = 0;
        }
    }

    // Final redraw
    let _ = terminal.draw(|f| render(f, app_state));

    Ok(result)
}

/// Perform restoration of all Recycle Bin contents with real-time progress updates
fn perform_restore_all_bin(
    app_state: &mut AppState,
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
) -> anyhow::Result<restore::RestoreResult> {
    // Get current Recycle Bin contents
    let recycle_bin_items = trash::os_limited::list()
        .context("Failed to list Recycle Bin contents")?;

    if recycle_bin_items.is_empty() {
        return Ok(restore::RestoreResult::default());
    }

    let mut result = restore::RestoreResult::default();
    const BATCH_SIZE: usize = 100;

    // Create all parent directories before bulk restore
    let mut parent_dirs: std::collections::HashSet<std::path::PathBuf> = std::collections::HashSet::new();
    for item in &recycle_bin_items {
        let dest = item.original_parent.join(&item.name);
        if let Some(parent) = dest.parent() {
            parent_dirs.insert(std::path::PathBuf::from(parent));
        }
    }

    for parent in &parent_dirs {
        if !parent.exists() {
            let _ = std::fs::create_dir_all(parent);
        }
    }

    // Restore in batches for better performance
    for batch in recycle_bin_items.chunks(BATCH_SIZE) {
        // Update progress state
        if let crate::tui::state::Screen::Restore {
            progress: Some(ref mut prog),
            ..
        } = app_state.screen
        {
            if let Some(item) = batch.first() {
                let path = item.original_parent.join(&item.name);
                let relative_path_str = crate::utils::to_relative_path(&path, &app_state.scan_path);
                prog.current_path = Some(std::path::PathBuf::from(relative_path_str));
            }
        }

        // Redraw terminal periodically
        let _ = terminal.draw(|f| render(f, app_state));

        // Try bulk restore
        match trash::os_limited::restore_all(batch.iter().cloned()) {
            Ok(()) => {
                // Bulk restore succeeded
                for item in batch {
                    result.restored += 1;
                    // Try to get size from restored file
                    let restored_path = item.original_parent.join(&item.name);
                    if let Ok(metadata) = std::fs::metadata(&restored_path) {
                        result.restored_bytes += metadata.len();
                    }
                }

                // Update progress
                if let crate::tui::state::Screen::Restore {
                    progress: Some(ref mut prog),
                    ..
                } = app_state.screen
                {
                    prog.restored = result.restored;
                    prog.restored_bytes = result.restored_bytes;
                }
            }
            Err(_e) => {
                // Bulk restore failed - fall back to individual restore
                for item in batch {
                    let dest = item.original_parent.join(&item.name);
                    
                    // Skip if destination already exists
                    if dest.exists() {
                        result.restored += 1;
                        if let Ok(metadata) = std::fs::metadata(&dest) {
                            result.restored_bytes += metadata.len();
                        }
                        continue;
                    }

                    match restore::restore_file(&item) {
                        Ok(()) => {
                            result.restored += 1;
                            // Get file size from restored file
                            if let Ok(metadata) = std::fs::metadata(&dest) {
                                result.restored_bytes += metadata.len();
                            }

                            // Update progress
                            if let crate::tui::state::Screen::Restore {
                                progress: Some(ref mut prog),
                                ..
                            } = app_state.screen
                            {
                                prog.restored = result.restored;
                                prog.restored_bytes = result.restored_bytes;
                            }
                        }
                        Err(_err) => {
                            result.errors += 1;

                            // Update progress
                            if let crate::tui::state::Screen::Restore {
                                progress: Some(ref mut prog),
                                ..
                            } = app_state.screen
                            {
                                prog.errors = result.errors;
                            }
                        }
                    }
                }
            }
        }
    }

    // Final redraw
    let _ = terminal.draw(|f| render(f, app_state));

    Ok(result)
}
