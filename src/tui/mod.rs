//! TUI module for interactive terminal interface
//!
//! Provides a full-screen terminal UI using Ratatui for interactive file cleanup

pub mod events;
pub mod screens;
pub mod state;
pub mod theme;
pub mod widgets;

use anyhow::Result;
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

        terminal.draw(|f| render(f, &mut app_state))?;

        // Handle pending restore
        if matches!(
            app_state.screen,
            crate::tui::state::Screen::Restore { result: None }
        ) {
            // Perform restore operation
            match crate::restore::restore_last(crate::output::OutputMode::Quiet) {
                Ok(result) => {
                    app_state.screen = crate::tui::state::Screen::Restore {
                        result: Some(crate::tui::state::RestoreResult {
                            restored: result.restored,
                            restored_bytes: result.restored_bytes,
                            errors: result.errors,
                            not_found: result.not_found,
                        }),
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
                        if let Ok(userprofile) = std::env::var("USERPROFILE") {
                            std::path::PathBuf::from(&userprofile)
                        } else {
                            std::env::current_dir()
                                .unwrap_or_else(|_| std::path::PathBuf::from("."))
                        }
                    })
                } else if let Ok(userprofile) = std::env::var("USERPROFILE") {
                    std::path::PathBuf::from(&userprofile)
                } else {
                    std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
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
                match scan_directory(&scan_path, 3) {
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
    let mut current_category = String::new();

    // Initialize progress
    if let crate::tui::state::Screen::Cleaning { ref mut progress } = app_state.screen {
        progress.total = total;
        progress.cleaned = trash_cleaned;
        progress.errors = trash_errors;
    }

    // Process each non-trash item individually
    // Use batched updates for better performance - update UI every N files or every 50ms
    let mut last_redraw = std::time::Instant::now();
    let mut files_since_redraw = 0;
    const REDRAW_INTERVAL_MS: u64 = 50; // Redraw at most every 50ms
    const REDRAW_INTERVAL_FILES: usize = 5; // Or every 5 files, whichever comes first

    for (_item_idx, category, path, size_bytes) in items_to_clean {
        // Update current category if it changed
        if current_category != category {
            current_category = category.clone();
            if let crate::tui::state::Screen::Cleaning { ref mut progress } = app_state.screen {
                progress.current_category = format!("Cleaning {}...", category);
            }
        }

        // Update current path being deleted
        if let crate::tui::state::Screen::Cleaning { ref mut progress } = app_state.screen {
            // Convert to relative path for display (returns String, convert to PathBuf)
            let relative_path_str = crate::utils::to_relative_path(&path, &app_state.scan_path);
            progress.current_path = Some(std::path::PathBuf::from(relative_path_str));
        }

        // Perform the actual deletion (do this BEFORE redraw for better responsiveness)
        let delete_result = match category.as_str() {
            "Browser" => categories::browser::clean(&path),
            "System" => categories::system::clean(&path),
            "Empty" => categories::empty::clean(&path),
            _ => {
                // Use standard clean_path for other categories
                cleaner::clean_path(&path, permanent)
            }
        };

        match delete_result {
            Ok(()) => {
                cleaned += 1;
                cleaned_bytes += size_bytes;

                // Update progress
                if let crate::tui::state::Screen::Cleaning { ref mut progress } = app_state.screen {
                    progress.cleaned = cleaned;
                }
            }
            Err(_e) => {
                errors += 1;

                // Update progress
                if let crate::tui::state::Screen::Cleaning { ref mut progress } = app_state.screen {
                    progress.errors = errors;
                }
            }
        }

        files_since_redraw += 1;
        let should_redraw = files_since_redraw >= REDRAW_INTERVAL_FILES
            || last_redraw.elapsed().as_millis() >= REDRAW_INTERVAL_MS as u128;

        // Redraw terminal periodically (not after every file for better performance)
        if should_redraw {
            let _ = terminal.draw(|f| render(f, app_state));
            last_redraw = std::time::Instant::now();
            files_since_redraw = 0;
        }
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
