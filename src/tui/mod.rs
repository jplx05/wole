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
use std::path::PathBuf;
use std::time::Duration;

use self::events::{handle_event, handle_mouse_event};
use self::screens::render;
use self::state::AppState;
use crate::cleaner;
use crate::cli::ScanOptions;
use crate::config::Config;
use crate::restore;
use crate::scan_cache::ScanCache;
use crate::scan_events::ScanProgressEvent;
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
    let mut last_tick_update = std::time::Instant::now();

    // Main event loop
    loop {
        // Increment tick frequently when scanning, cleaning, or restoring (for smooth spinner animation)
        if matches!(app_state.screen, crate::tui::state::Screen::Scanning { .. })
            || matches!(app_state.screen, crate::tui::state::Screen::Cleaning { .. })
            || matches!(
                app_state.screen,
                crate::tui::state::Screen::Restore {
                    progress: Some(_),
                    ..
                }
            )
        {
            if last_tick_update.elapsed().as_millis() >= 100 {
                app_state.tick = app_state.tick.wrapping_add(1);
                last_tick_update = std::time::Instant::now();
            }
        } else {
            // For other screens, increment tick normally for animations
            app_state.tick = app_state.tick.wrapping_add(1);
        }

        // Auto-refresh Status screen every 2 seconds
        if let crate::tui::state::Screen::Status {
            ref mut status,
            ref mut last_refresh,
        } = app_state.screen
        {
            // Trigger disk breakdown scan in background on first load if cache is empty
            #[cfg(windows)]
            {
                use crate::status::gather_disk_breakdown_cached_only;
                use std::sync::atomic::{AtomicBool, Ordering};
                static DISK_BREAKDOWN_TRIGGERED: AtomicBool = AtomicBool::new(false);

                // Check if we need to trigger background scan (only once per session)
                if !DISK_BREAKDOWN_TRIGGERED.load(Ordering::Relaxed) {
                    if gather_disk_breakdown_cached_only().is_none() {
                        // Cache is empty, trigger background scan
                        use crate::status::refresh_disk_breakdown_async;
                        refresh_disk_breakdown_async();
                    }
                    DISK_BREAKDOWN_TRIGGERED.store(true, Ordering::Relaxed);
                }
            }

            if last_refresh.elapsed().as_secs() >= 2 {
                use crate::status::gather_status;
                use sysinfo::System;

                // Create system and gather status (optimized refresh inside)
                let mut system = System::new();
                if let Ok(new_status) = gather_status(&mut system) {
                    **status = new_status;
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
                                error_reasons: result.error_reasons,
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

        // Handle pending scan - start scan in background if not already running
        if scan_pending {
            scan_pending = false;

            // Check if we're still in Scanning screen (might have been cancelled)
            if !matches!(app_state.screen, crate::tui::state::Screen::Scanning { .. }) {
                // Screen changed (likely cancelled), skip scanning
                continue;
            }

            // Check if scan is already running (we store the receiver in app_state)
            // For now, just start the scan - the main loop will check for completion

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
                        std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
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

            // Increment tick when scan starts (for spinner animation)
            app_state.tick = app_state.tick.wrapping_add(1);

            // Redraw to show the scanning state
            terminal.draw(|f| render(f, &mut app_state))?;

            // Perform actual scan with progress updates (runs in background, main loop continues)
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
                Ok((cleaned, cleaned_bytes, errors, failed_temp_files)) => {
                    app_state.screen = crate::tui::state::Screen::Success {
                        cleaned,
                        cleaned_bytes,
                        errors,
                        failed_temp_files,
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

/// Check if we can reuse existing scan results
/// Returns true if existing results can be reused for the current scan configuration
///
/// Reuse is allowed if:
/// - Current enabled categories are a subset of (or equal to) previously scanned categories
/// - This means if user scans 5 categories, then disables one, we can reuse results for the remaining 4
fn can_reuse_scan_results(app_state: &AppState) -> bool {
    // Check if we have existing scan results
    let Some(_) = app_state.scan_results else {
        return false;
    };

    // Check if we have stored categories from the last scan
    let Some(ref last_categories) = app_state.last_scan_categories else {
        // No previous scan categories stored, can't verify - perform new scan to be safe
        return false;
    };

    // Get currently enabled categories
    let current_categories: std::collections::HashSet<String> = app_state
        .categories
        .iter()
        .filter(|cat| cat.enabled)
        .map(|cat| cat.name.clone())
        .collect();

    // Reuse if current categories are a subset of (or equal to) last scan categories
    // This handles the case: scan 5 categories → disable 1 → clean (show 4 categories from existing results)
    // But if user enables a NEW category that wasn't scanned, we need a new scan
    current_categories.is_subset(last_categories)
}

/// Perform a scan with progress updates
fn perform_scan_with_progress(
    app_state: &mut AppState,
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
) -> anyhow::Result<()> {
    // Check if we can reuse existing scan results
    if can_reuse_scan_results(app_state) {
        // We have existing results that match, just update the progress display
        // and proceed to process them
        let enabled_categories: Vec<String> = app_state
            .categories
            .iter()
            .filter(|cat| cat.enabled)
            .map(|cat| cat.name.clone())
            .collect();

        let total_categories = enabled_categories.len();

        // Update progress to show all categories as complete
        if let crate::tui::state::Screen::Scanning { ref mut progress } = app_state.screen {
            for cat_progress in &mut progress.category_progress {
                cat_progress.completed = true;
                cat_progress.progress_pct = 1.0;
            }
            progress.total_scanned = total_categories;
        }

        // Get results from existing scan
        let results = app_state.scan_results.as_ref().unwrap();

        // Update progress with actual sizes
        let mut running_total_items = 0;
        let mut running_total_bytes = 0u64;

        for cat_progress_name in &enabled_categories {
            let (items, size) = match cat_progress_name.as_str() {
                "Package Cache" => (results.cache.items, results.cache.size_bytes),
                "Application Cache" => (results.app_cache.items, results.app_cache.size_bytes),
                "Temp Files" => (results.temp.items, results.temp.size_bytes),
                "Trash" => (results.trash.items, results.trash.size_bytes),
                "Build Artifacts" => (results.build.items, results.build.size_bytes),
                "Old Downloads" => (results.downloads.items, results.downloads.size_bytes),
                "Large Files" => (results.large.items, results.large.size_bytes),
                "Old Files" => (results.old.items, results.old.size_bytes),
                "Installed Applications" => {
                    (results.applications.items, results.applications.size_bytes)
                }
                "Browser Cache" => (results.browser.items, results.browser.size_bytes),
                "System Cache" => (results.system.items, results.system.size_bytes),
                "Empty Folders" => (results.empty.items, results.empty.size_bytes),
                "Duplicates" => (results.duplicates.items, results.duplicates.size_bytes),
                "Windows Update" => (
                    results.windows_update.items,
                    results.windows_update.size_bytes,
                ),
                "Event Logs" => (results.event_logs.items, results.event_logs.size_bytes),
                _ => (0, 0),
            };

            running_total_items += items;
            running_total_bytes += size;

            if let crate::tui::state::Screen::Scanning { ref mut progress } = app_state.screen {
                for cat_progress in &mut progress.category_progress {
                    if cat_progress.name == *cat_progress_name {
                        cat_progress.size = Some(size);
                        break;
                    }
                }
                progress.total_found = running_total_items;
                progress.total_size = running_total_bytes;
            }
        }

        // Redraw to show the updated progress
        let _ = terminal.draw(|f| render(f, app_state));
        // Small delay to make the update visible
        std::thread::sleep(Duration::from_millis(100));

        // Don't update last_scan_categories here - keep the original set
        // This allows reusing results even if user temporarily disables some categories
        // Example: scan with 5 categories → disable 1 → clean (reuse) → re-enable → clean (still reuse)

        // Results are already in app_state.scan_results, so we're done
        return Ok(());
    }

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
    let mut windows_update = false;
    let mut event_logs = false;

    for cat in &app_state.categories {
        match cat.name.as_str() {
            "Package Cache" => cache = cat.enabled,
            "Application Cache" => app_cache = cat.enabled,
            "Temp Files" => temp = cat.enabled,
            "Trash" => trash = cat.enabled,
            "Build Artifacts" => build = cat.enabled,
            "Old Downloads" => downloads = cat.enabled,
            "Large Files" => large = cat.enabled,
            "Old Files" => old = cat.enabled,
            "Installed Applications" => applications = cat.enabled,
            "Browser Cache" => browser = cat.enabled,
            "System Cache" => system = cat.enabled,
            "Empty Folders" => empty = cat.enabled,
            "Duplicates" => duplicates = cat.enabled,
            "Windows Update" => windows_update = cat.enabled,
            "Event Logs" => event_logs = cat.enabled,
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
        windows_update,
        event_logs,
        project_age_days: config.thresholds.project_age_days,
        min_age_days: config.thresholds.min_age_days,
        min_size_bytes,
    };

    let mut first_scan_full_disk = false;
    if config.cache.enabled {
        if let Ok(cache) = ScanCache::open() {
            first_scan_full_disk = matches!(cache.get_previous_scan_id(), Ok(None));
        }
    }

    if first_scan_full_disk {
        let root_path = crate::utils::get_root_disk_path();
        app_state.scan_path = root_path.clone();
        if let crate::tui::state::Screen::Scanning { ref mut progress } = app_state.screen {
            progress.notice = Some("First scan: scanning all categories from root to build baseline (this may take longer)".to_string());
            progress.current_path = Some(root_path);
        }
    }

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

    // Run scan in background thread - this is a blocking call but we need results
    // The main loop will continue running and updating tick/redrawing while we wait
    let scan_path = app_state.scan_path.clone();
    let scan_options = options.clone();
    let scan_config = config.clone();
    let use_cache = scan_config.cache.enabled;

    let (result_tx, result_rx) = std::sync::mpsc::channel();
    let (progress_tx, progress_rx) = std::sync::mpsc::channel();
    let _scan_handle = std::thread::spawn(move || {
        let mut scan_cache = if use_cache {
            ScanCache::open().ok()
        } else {
            None
        };
        let result = scanner::scan_all_with_progress(
            &scan_path,
            scan_options,
            &scan_config,
            &progress_tx,
            scan_cache.as_mut(),
        );
        let _ = result_tx.send(result);
    });

    // Wait for scan to complete, manually updating tick and redrawing for spinner animation
    let mut last_tick_update = std::time::Instant::now();
    let mut last_progress_draw = std::time::Instant::now();
    let mut running_total_items = 0usize;
    let mut running_total_bytes = 0u64;
    let mut completed_categories: std::collections::HashSet<String> =
        std::collections::HashSet::new();

    let mut apply_progress_event = |event: ScanProgressEvent, app_state: &mut AppState| {
        if let crate::tui::state::Screen::Scanning { ref mut progress } = app_state.screen {
            match event {
                ScanProgressEvent::ReadingFolder { path } => {
                    // First scan: show folder being read
                    progress.current_category = "Building baseline".to_string();
                    progress.current_path = Some(path);
                }
                ScanProgressEvent::ReadingFile { path } => {
                    // First scan: show file being read
                    progress.current_category = "Building baseline".to_string();
                    progress.current_path = Some(path);
                }
                ScanProgressEvent::CategoryStarted {
                    category,
                    current_path,
                    ..
                } => {
                    progress.current_category = category.clone();
                    if let Some(path) = current_path {
                        progress.current_path = Some(path);
                    }
                    if let Some(cat_progress) = progress
                        .category_progress
                        .iter_mut()
                        .find(|c| c.name == category)
                    {
                        cat_progress.completed = false;
                        cat_progress.progress_pct = 0.0;
                    }
                }
                ScanProgressEvent::CategoryProgress {
                    category,
                    completed_units,
                    total_units,
                    current_path,
                } => {
                    progress.current_category = category.clone();
                    if let Some(path) = current_path {
                        progress.current_path = Some(path);
                    }
                    if let Some(cat_progress) = progress
                        .category_progress
                        .iter_mut()
                        .find(|c| c.name == category)
                    {
                        if let Some(total) = total_units {
                            if total > 0 {
                                cat_progress.progress_pct = completed_units as f32 / total as f32;
                            }
                        }
                    }
                }
                ScanProgressEvent::CategoryFinished {
                    category,
                    items,
                    size_bytes,
                } => {
                    progress.current_category = category.clone();
                    if let Some(cat_progress) = progress
                        .category_progress
                        .iter_mut()
                        .find(|c| c.name == category)
                    {
                        cat_progress.completed = true;
                        cat_progress.progress_pct = 1.0;
                        cat_progress.size = Some(size_bytes);
                    }

                    if completed_categories.insert(category) {
                        running_total_items += items;
                        running_total_bytes += size_bytes;
                        progress.total_found = running_total_items;
                        progress.total_size = running_total_bytes;
                    }

                    progress.total_scanned = completed_categories.len();
                }
            }
        }
    };

    let results = loop {
        let mut progress_updated = false;
        while let Ok(event) = progress_rx.try_recv() {
            apply_progress_event(event, app_state);
            progress_updated = true;
        }
        if progress_updated && last_progress_draw.elapsed().as_millis() >= 50 {
            let _ = terminal.draw(|f| render(f, app_state));
            last_progress_draw = std::time::Instant::now();
        }

        match result_rx.try_recv() {
            Ok(Ok(results)) => break results,
            Ok(Err(e)) => return Err(e),
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                // Scan still in progress, check for cancellation
                if !matches!(app_state.screen, crate::tui::state::Screen::Scanning { .. }) {
                    return Ok(());
                }

                // Increment tick frequently for smooth spinner animation (every 100ms)
                if last_tick_update.elapsed().as_millis() >= 100 {
                    app_state.tick = app_state.tick.wrapping_add(1);
                    last_tick_update = std::time::Instant::now();
                    // Redraw terminal to show spinner animation
                    let _ = terminal.draw(|f| render(f, app_state));
                    last_progress_draw = last_tick_update;
                }

                // Process events to allow cancellation
                while event::poll(Duration::from_millis(0)).unwrap_or(false) {
                    if let Ok(Event::Key(key)) = event::read() {
                        if key.kind == KeyEventKind::Press {
                            handle_event(app_state, key.code, key.modifiers);
                            if !matches!(
                                app_state.screen,
                                crate::tui::state::Screen::Scanning { .. }
                            ) {
                                return Ok(());
                            }
                        }
                    }
                }
                // Small sleep to avoid busy-waiting
                std::thread::sleep(Duration::from_millis(16)); // ~60fps
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                return Err(anyhow::anyhow!("Scan thread disconnected"));
            }
        }
    };

    while let Ok(event) = progress_rx.try_recv() {
        apply_progress_event(event, app_state);
    }

    // Check if scan was cancelled after the scan completes
    if !matches!(app_state.screen, crate::tui::state::Screen::Scanning { .. }) {
        // Scan was cancelled, return early without processing results
        return Ok(());
    }

    if completed_categories.len() < total_categories {
        for cat_progress_name in &enabled_categories {
            if completed_categories.contains(cat_progress_name) {
                continue;
            }
            if !matches!(app_state.screen, crate::tui::state::Screen::Scanning { .. }) {
                return Ok(());
            }

            let (items, size) = match cat_progress_name.as_str() {
                "Package Cache" => (results.cache.items, results.cache.size_bytes),
                "Application Cache" => (results.app_cache.items, results.app_cache.size_bytes),
                "Temp Files" => (results.temp.items, results.temp.size_bytes),
                "Trash" => (results.trash.items, results.trash.size_bytes),
                "Build Artifacts" => (results.build.items, results.build.size_bytes),
                "Old Downloads" => (results.downloads.items, results.downloads.size_bytes),
                "Large Files" => (results.large.items, results.large.size_bytes),
                "Old Files" => (results.old.items, results.old.size_bytes),
                "Installed Applications" => {
                    (results.applications.items, results.applications.size_bytes)
                }
                "Browser Cache" => (results.browser.items, results.browser.size_bytes),
                "System Cache" => (results.system.items, results.system.size_bytes),
                "Empty Folders" => (results.empty.items, results.empty.size_bytes),
                "Duplicates" => (results.duplicates.items, results.duplicates.size_bytes),
                _ => (0, 0),
            };

            running_total_items += items;
            running_total_bytes += size;

            if let crate::tui::state::Screen::Scanning { ref mut progress } = app_state.screen {
                for cat_progress in &mut progress.category_progress {
                    if cat_progress.name == *cat_progress_name {
                        cat_progress.size = Some(size);
                        cat_progress.completed = true;
                        cat_progress.progress_pct = 1.0;
                        break;
                    }
                }

                progress.total_found = running_total_items;
                progress.total_size = running_total_bytes;
                progress.total_scanned = total_categories;
            }
        }
    }

    app_state.scan_results = Some(results);

    // Store enabled categories for future reuse checks
    app_state.last_scan_categories = Some(
        app_state
            .categories
            .iter()
            .filter(|cat| cat.enabled)
            .map(|cat| cat.name.clone())
            .collect(),
    );

    // If this was a first scan, get cache stats for summary
    if first_scan_full_disk {
        if let Ok(cache) = ScanCache::open() {
            if let Ok(stats) = cache.get_cache_stats() {
                app_state.first_scan_stats = Some(stats);
            }
        }
    }

    Ok(())
}

/// Perform cleanup of selected items with real-time progress updates
/// Returns (cleaned_count, cleaned_bytes, error_count, failed_temp_files)
fn perform_cleanup(
    app_state: &mut AppState,
    permanent: bool,
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
) -> anyhow::Result<(u64, u64, usize, Vec<PathBuf>)> {
    use crate::categories;
    use crate::history::DeletionLog;

    // Create deletion log for audit trail
    let mut history = DeletionLog::new();

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
                // Log trash cleanup success
                history.log_success(
                    std::path::Path::new("Recycle Bin"),
                    trash_total_bytes,
                    "trash",
                    true,
                );
            }
            Err(e) => {
                // If trash cleaning fails, all trash items failed
                trash_errors = trash_items.len();
                // Log trash cleanup failure
                history.log_failure(
                    std::path::Path::new("Recycle Bin"),
                    trash_total_bytes,
                    "trash",
                    true,
                    &e.to_string(),
                );
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
    // Special categories (Browser, System, Empty, Package Cache) need individual handling
    // Package Cache needs individual handling to avoid Windows dialogs blocking batch deletion
    // Temp Files are processed separately with smaller batches to reduce batch failures
    // All other categories can be batch deleted together

    let mut applications_items: Vec<(usize, std::path::PathBuf, u64)> = Vec::new();
    let mut special_items: Vec<(usize, String, std::path::PathBuf, u64)> = Vec::new();
    let mut cache_items: Vec<(usize, std::path::PathBuf, u64)> = Vec::new();
    let mut temp_items: Vec<(usize, std::path::PathBuf, u64)> = Vec::new();
    let mut batch_items: Vec<(usize, std::path::PathBuf, u64)> = Vec::new();

    for (idx, category, path, size) in items_to_clean {
        match category.as_str() {
            "Installed Applications" => {
                // Applications need a real uninstall step; don't batch-delete folders.
                applications_items.push((idx, path, size));
            }
            "Browser Cache" | "System Cache" | "Empty Folders" => {
                special_items.push((idx, category, path, size));
            }
            "Package Cache" => {
                // Process cache individually to handle Windows dialogs
                cache_items.push((idx, path, size));
            }
            "Temp Files" => {
                // Process temp files separately with smaller batches
                // Temp files are more likely to be locked, so smaller batches reduce failures
                temp_items.push((idx, path, size));
            }
            _ => {
                batch_items.push((idx, path, size));
            }
        }
    }

    // Handle installed applications: uninstall + delete leftover artifacts.
    // IMPORTANT: uninstall is not safely restorable, even when permanent=false.
    if !applications_items.is_empty() {
        if let crate::tui::state::Screen::Cleaning { ref mut progress } = app_state.screen {
            progress.current_category =
                format!("Uninstalling {} applications...", applications_items.len());
        }
        let _ = terminal.draw(|f| render(f, app_state));

        let mut last_tick_update = std::time::Instant::now();

        for (_idx, install_path, size_bytes) in applications_items {
            // Update current path display (uses install folder path; display name is shown elsewhere).
            if let crate::tui::state::Screen::Cleaning { ref mut progress } = app_state.screen {
                progress.current_path = Some(install_path.clone());
            }

            // Keep spinner moving.
            if last_tick_update.elapsed().as_millis() >= 100 {
                app_state.tick = app_state.tick.wrapping_add(1);
                last_tick_update = std::time::Instant::now();
                let _ = terminal.draw(|f| render(f, app_state));
            }

            let display = crate::categories::applications::get_app_display_name(&install_path)
                .unwrap_or_else(|| install_path.display().to_string());

            let mut had_error = false;

            // Tighten: uninstall must succeed before deleting any artifacts.
            if crate::categories::applications::get_app_uninstall_string(&install_path).is_none() {
                had_error = true;
            } else if let Err(_e) = crate::categories::applications::uninstall(&install_path) {
                had_error = true;
            }

            // Post-check: if it still appears installed, skip artifact deletion (tight/safe).
            if !had_error && crate::categories::applications::is_still_installed(&install_path) {
                had_error = true;
            }

            if !had_error {
                // Remove tightly-scoped leftovers after uninstall succeeds.
                let artifacts =
                    crate::categories::applications::get_app_artifact_paths(&install_path);
                for artifact in artifacts {
                    match cleaner::delete_with_precheck(&artifact, permanent) {
                        Ok(cleaner::DeleteOutcome::Deleted) => {}
                        Ok(
                            cleaner::DeleteOutcome::SkippedMissing
                            | cleaner::DeleteOutcome::SkippedSystem,
                        ) => {}
                        Ok(cleaner::DeleteOutcome::SkippedLocked) => had_error = true,
                        Err(_) => had_error = true,
                    }
                }
            }

            // Log + counters (always treat as permanent in history to avoid restore offering).
            let log_as_permanent = true;
            if had_error {
                errors += 1;
                history.log_failure(
                    &install_path,
                    size_bytes,
                    "applications",
                    log_as_permanent,
                    &format!("Application uninstall/cleanup had errors: {}", display),
                );
            } else {
                cleaned += 1;
                cleaned_bytes += size_bytes;
                history.log_success(&install_path, size_bytes, "applications", log_as_permanent);
            }

            // Update progress after each app
            if let crate::tui::state::Screen::Cleaning { ref mut progress } = app_state.screen {
                progress.cleaned = cleaned;
                progress.errors = errors;
            }
            app_state.tick = app_state.tick.wrapping_add(1);
            let _ = terminal.draw(|f| render(f, app_state));
        }
    }

    // Handle special categories first (they need individual processing)
    if !special_items.is_empty() {
        if let crate::tui::state::Screen::Cleaning { ref mut progress } = app_state.screen {
            progress.current_category = "Cleaning special items...".to_string();
        }
        let _ = terminal.draw(|f| render(f, app_state));

        // Track last tick update for continuous animation
        let mut last_tick_update = std::time::Instant::now();

        for (_idx, category, path, size_bytes) in special_items {
            // Update current path and tick for animation
            if let crate::tui::state::Screen::Cleaning { ref mut progress } = app_state.screen {
                progress.current_path = Some(path.clone());
            }

            // Continuously update tick and redraw for smooth spinner animation
            if last_tick_update.elapsed().as_millis() >= 100 {
                app_state.tick = app_state.tick.wrapping_add(1);
                last_tick_update = std::time::Instant::now();
                let _ = terminal.draw(|f| render(f, app_state));
            }

            let delete_result = cleaner::delete_with_precheck(&path, permanent);

            match delete_result {
                Ok(cleaner::DeleteOutcome::Deleted) => {
                    cleaned += 1;
                    cleaned_bytes += size_bytes;
                    // Log success
                    let category_lower = category.to_lowercase();
                    history.log_success(&path, size_bytes, &category_lower, permanent);
                }
                Ok(
                    cleaner::DeleteOutcome::SkippedMissing | cleaner::DeleteOutcome::SkippedSystem,
                ) => {}
                Ok(cleaner::DeleteOutcome::SkippedLocked) => {
                    errors += 1;
                    let category_lower = category.to_lowercase();
                    history.log_failure(
                        &path,
                        size_bytes,
                        &category_lower,
                        permanent,
                        "Path is locked by another process",
                    );
                }
                Err(e) => {
                    errors += 1;
                    // Log failure
                    let category_lower = category.to_lowercase();
                    history.log_failure(
                        &path,
                        size_bytes,
                        &category_lower,
                        permanent,
                        &e.to_string(),
                    );
                }
            }

            // Update progress after each item
            if let crate::tui::state::Screen::Cleaning { ref mut progress } = app_state.screen {
                progress.cleaned = cleaned;
                progress.errors = errors;
            }
            app_state.tick = app_state.tick.wrapping_add(1);
            let _ = terminal.draw(|f| render(f, app_state));
        }
    }

    // Handle cache items individually to avoid Windows dialogs blocking batch deletion
    if !cache_items.is_empty() {
        if let crate::tui::state::Screen::Cleaning { ref mut progress } = app_state.screen {
            progress.current_category = format!("Cleaning {} cache items...", cache_items.len());
        }
        let _ = terminal.draw(|f| render(f, app_state));

        // Track last tick update for continuous animation
        let mut last_tick_update = std::time::Instant::now();

        for (_idx, path, size_bytes) in cache_items {
            // Update current file being processed
            if let crate::tui::state::Screen::Cleaning { ref mut progress } = app_state.screen {
                progress.current_path = Some(path.clone());
            }

            // Continuously update tick and redraw for smooth spinner animation
            if last_tick_update.elapsed().as_millis() >= 100 {
                app_state.tick = app_state.tick.wrapping_add(1);
                last_tick_update = std::time::Instant::now();
                let _ = terminal.draw(|f| render(f, app_state));
            }

            match cleaner::delete_with_precheck(&path, permanent) {
                Ok(cleaner::DeleteOutcome::Deleted) => {
                    cleaned += 1;
                    cleaned_bytes += size_bytes;
                    // Log success
                    history.log_success(&path, size_bytes, "cache", permanent);
                }
                Ok(
                    cleaner::DeleteOutcome::SkippedMissing | cleaner::DeleteOutcome::SkippedSystem,
                ) => {}
                Ok(cleaner::DeleteOutcome::SkippedLocked) => {
                    errors += 1;
                    history.log_failure(
                        &path,
                        size_bytes,
                        "cache",
                        permanent,
                        "Path is locked by another process",
                    );
                }
                Err(e) => {
                    errors += 1;
                    // Log failure
                    history.log_failure(&path, size_bytes, "cache", permanent, &e.to_string());
                }
            }

            // Update progress after each item
            if let crate::tui::state::Screen::Cleaning { ref mut progress } = app_state.screen {
                progress.cleaned = cleaned;
                progress.errors = errors;
            }
            // Redraw to show progress with updated tick
            app_state.tick = app_state.tick.wrapping_add(1);
            let _ = terminal.draw(|f| render(f, app_state));
        }
    }

    // Track failed temp files to show to user
    let mut failed_temp_files: Vec<PathBuf> = Vec::new();

    // Handle temp files separately with smaller batches to reduce failures
    // Temp files are more likely to be locked by running applications
    if !temp_items.is_empty() {
        // Calculate total bytes for temp items
        let temp_total_bytes: u64 = temp_items.iter().map(|(_, _, size)| size).sum();

        // Extract just the paths for batch deletion
        let paths: Vec<std::path::PathBuf> = temp_items.iter().map(|(_, p, _)| p.clone()).collect();

        // Calculate sizes BEFORE deletion (critical for accurate logging)
        use std::collections::HashMap;
        let mut path_sizes: HashMap<PathBuf, u64> = HashMap::new();
        for (_, path, size) in &temp_items {
            path_sizes.insert(path.clone(), *size);
        }

        // Process temp files in smaller batches to reduce batch failures
        // Smaller batches mean if one file is locked, fewer files need to be retried
        const TEMP_BATCH_SIZE: usize = 25; // Smaller than regular batch size
        let mut temp_success = 0;
        let mut temp_errors = 0;
        let mut deleted_paths = Vec::new();
        let mut skipped_paths = Vec::new();

        // Track last tick update for continuous animation
        let mut last_tick_update = std::time::Instant::now();

        for batch_chunk in paths.chunks(TEMP_BATCH_SIZE) {
            // Update UI to show temp file deletion progress
            if let crate::tui::state::Screen::Cleaning { ref mut progress } = app_state.screen {
                progress.current_category =
                    format!("Cleaning temp files... ({} total)", paths.len());
                // Show first file in current batch as current file being processed
                if let Some(first_path) = batch_chunk.first() {
                    progress.current_path = Some(first_path.clone());
                }
            }

            // Continuously update tick and redraw for smooth spinner animation
            // Update every 100ms for smooth animation (same as scanner)
            if last_tick_update.elapsed().as_millis() >= 100 {
                app_state.tick = app_state.tick.wrapping_add(1);
                last_tick_update = std::time::Instant::now();
                let _ = terminal.draw(|f| render(f, app_state));
            }

            // Delete this batch
            let batch_result = cleaner::clean_paths_batch(batch_chunk, permanent);
            temp_success += batch_result.success_count;
            temp_errors += batch_result.error_count;
            deleted_paths.extend(batch_result.deleted_paths);
            skipped_paths.extend(batch_result.skipped_paths);

            // Update progress after each batch
            if let crate::tui::state::Screen::Cleaning { ref mut progress } = app_state.screen {
                progress.cleaned = cleaned + temp_success as u64;
                progress.errors = errors + temp_errors;
            }
            // Redraw to show progress with updated tick
            app_state.tick = app_state.tick.wrapping_add(1);
            let _ = terminal.draw(|f| render(f, app_state));
        }

        // Update totals
        cleaned += temp_success as u64;
        errors += temp_errors;

        // Log temp file deletion results
        let mut path_to_category: HashMap<PathBuf, String> = HashMap::new();
        for (_, path, _) in &temp_items {
            if let Some(item) = app_state.all_items.iter().find(|i| i.path == *path) {
                path_to_category.insert(path.clone(), item.category.to_lowercase());
            }
        }

        // Log successes
        for path in &deleted_paths {
            if let Some(size) = path_sizes.get(path) {
                let category = path_to_category
                    .get(path)
                    .cloned()
                    .unwrap_or_else(|| "temp".to_string());
                history.log_success(path, *size, &category, permanent);
            }
        }

        // Log failures (paths that weren't deleted) and track them
        for path in &paths {
            if !deleted_paths.contains(path) && !skipped_paths.contains(path) {
                failed_temp_files.push(path.clone());
                if let Some(size) = path_sizes.get(path) {
                    let category = path_to_category
                        .get(path)
                        .cloned()
                        .unwrap_or_else(|| "temp".to_string());
                    history.log_failure(
                        path,
                        *size,
                        &category,
                        permanent,
                        "Temp file deletion failed (may be locked)",
                    );
                }
            }
        }

        // Estimate cleaned bytes based on success ratio
        if temp_success > 0 {
            if temp_errors == 0 {
                // All succeeded - add all bytes
                cleaned_bytes += temp_total_bytes;
            } else {
                // Partial success - estimate based on ratio
                let ratio = temp_success as f64 / paths.len() as f64;
                cleaned_bytes += (temp_total_bytes as f64 * ratio) as u64;
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
        // Calculate total bytes for batch items
        let batch_total_bytes: u64 = batch_items.iter().map(|(_, _, size)| size).sum();

        // Extract just the paths for batch deletion
        let paths: Vec<std::path::PathBuf> =
            batch_items.iter().map(|(_, p, _)| p.clone()).collect();

        // Calculate sizes BEFORE deletion (critical for accurate logging)
        use std::collections::HashMap;
        let mut path_sizes: HashMap<PathBuf, u64> = HashMap::new();
        for (_, path, size) in &batch_items {
            path_sizes.insert(path.clone(), *size);
        }

        // Process in smaller batches with progress updates to show current file and keep animation going
        const BATCH_SIZE: usize = 50; // Process 50 files at a time for better progress visibility
        let mut batch_success = 0;
        let mut batch_errors = 0;
        let mut deleted_paths = Vec::new();
        let mut skipped_paths = Vec::new();

        // Track last tick update for continuous animation
        let mut last_tick_update = std::time::Instant::now();

        for batch_chunk in paths.chunks(BATCH_SIZE) {
            // Update UI to show batch deletion progress
            if let crate::tui::state::Screen::Cleaning { ref mut progress } = app_state.screen {
                progress.current_category = format!("Batch deleting {} files...", paths.len());
                // Show first file in current batch as current file being processed
                if let Some(first_path) = batch_chunk.first() {
                    progress.current_path = Some(first_path.clone());
                }
            }

            // Continuously update tick and redraw for smooth spinner animation
            // Update every 100ms for smooth animation (same as scanner)
            if last_tick_update.elapsed().as_millis() >= 100 {
                app_state.tick = app_state.tick.wrapping_add(1);
                last_tick_update = std::time::Instant::now();
                let _ = terminal.draw(|f| render(f, app_state));
            }

            // Delete this batch
            let batch_result = cleaner::clean_paths_batch(batch_chunk, permanent);
            batch_success += batch_result.success_count;
            batch_errors += batch_result.error_count;
            deleted_paths.extend(batch_result.deleted_paths);
            skipped_paths.extend(batch_result.skipped_paths);

            // Update progress after each batch
            if let crate::tui::state::Screen::Cleaning { ref mut progress } = app_state.screen {
                progress.cleaned = cleaned + batch_success as u64;
                progress.errors = errors + batch_errors;
            }
            // Redraw to show progress with updated tick
            app_state.tick = app_state.tick.wrapping_add(1);
            let _ = terminal.draw(|f| render(f, app_state));
        }

        // Already updated above during batch processing
        cleaned += batch_success as u64;
        errors += batch_errors;

        // Log batch deletion results
        // Create a map of path -> category for efficient lookup
        let mut path_to_category: HashMap<PathBuf, String> = HashMap::new();
        for (_, path, _) in &batch_items {
            if let Some(item) = app_state.all_items.iter().find(|i| i.path == *path) {
                path_to_category.insert(path.clone(), item.category.to_lowercase());
            }
        }

        // Log successes
        for path in &deleted_paths {
            if let Some(size) = path_sizes.get(path) {
                let category = path_to_category
                    .get(path)
                    .cloned()
                    .unwrap_or_else(|| "unknown".to_string());
                history.log_success(path, *size, &category, permanent);
            }
        }

        // Log failures (paths that weren't deleted)
        for path in &paths {
            if !deleted_paths.contains(path) && !skipped_paths.contains(path) {
                if let Some(size) = path_sizes.get(path) {
                    let category = path_to_category
                        .get(path)
                        .cloned()
                        .unwrap_or_else(|| "unknown".to_string());
                    history.log_failure(path, *size, &category, permanent, "Batch deletion failed");
                }
            }
        }

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

    // Save deletion history log
    if let Err(e) = history.save() {
        // Log error but don't fail the cleanup operation
        // In production, this is silently ignored to avoid disrupting the UI
        #[cfg(debug_assertions)]
        eprintln!("[DEBUG] Failed to save deletion log: {}", e);
    }

    Ok((cleaned, cleaned_bytes, errors, failed_temp_files))
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
    let mut error_reasons: Vec<String> = Vec::new(); // Track error messages
    let mut files_since_redraw = 0;
    let mut last_redraw = std::time::Instant::now();
    let mut last_tick_update = std::time::Instant::now();
    const REDRAW_INTERVAL_MS: u64 = 50;
    const REDRAW_INTERVAL_FILES: usize = 5;

    // Process each record
    for record in &latest_log.records {
        // Continuously update tick and redraw for smooth spinner animation
        if last_tick_update.elapsed().as_millis() >= 100 {
            app_state.tick = app_state.tick.wrapping_add(1);
            last_tick_update = std::time::Instant::now();
            let _ = terminal.draw(|f| render(f, app_state));
        }
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
                Err(e) => {
                    result.errors += 1;
                    // Store error message (limit to first 5 errors to avoid cluttering)
                    if error_reasons.len() < 5 {
                        error_reasons.push(format!("{}: {}", record.path, e));
                    }

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
                        Err(e) => {
                            restore_errors += 1;
                            // Store error message (limit to first 5 errors)
                            if error_reasons.len() < 5 {
                                let original_path =
                                    trash_item.original_parent.join(&trash_item.name);
                                error_reasons.push(format!("{}: {}", original_path.display(), e));
                            }
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

    // Attach error reasons to result
    result.error_reasons = error_reasons;
    Ok(result)
}

/// Perform restoration of all Recycle Bin contents with real-time progress updates
fn perform_restore_all_bin(
    app_state: &mut AppState,
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
) -> anyhow::Result<restore::RestoreResult> {
    // Get current Recycle Bin contents
    let recycle_bin_items =
        trash::os_limited::list().context("Failed to list Recycle Bin contents")?;

    if recycle_bin_items.is_empty() {
        return Ok(restore::RestoreResult::default());
    }

    let mut result = restore::RestoreResult::default();
    let mut error_reasons: Vec<String> = Vec::new(); // Track error messages
    const BATCH_SIZE: usize = 100;
    let mut last_tick_update = std::time::Instant::now();

    // Create all parent directories before bulk restore
    let mut parent_dirs: std::collections::HashSet<std::path::PathBuf> =
        std::collections::HashSet::new();
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
        // Continuously update tick and redraw for smooth spinner animation
        if last_tick_update.elapsed().as_millis() >= 100 {
            app_state.tick = app_state.tick.wrapping_add(1);
            last_tick_update = std::time::Instant::now();
        }

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

                    match restore::restore_file(item) {
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
                        Err(err) => {
                            result.errors += 1;
                            // Store error message (limit to first 5 errors)
                            if error_reasons.len() < 5 {
                                error_reasons.push(format!("{}: {}", dest.display(), err));
                            }

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

    // Attach error reasons to result
    result.error_reasons = error_reasons;
    Ok(result)
}
