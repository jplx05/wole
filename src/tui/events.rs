//! Event handling for TUI

use crate::tui::state::AppState;
use crate::tui::widgets::logo::LOGO_WITH_TAGLINE_HEIGHT;
use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use std::process::Command;

/// Result of handling an event
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventResult {
    Continue,
    Quit,
}

/// Handle a keyboard event
pub fn handle_event(
    app_state: &mut AppState,
    key: KeyCode,
    modifiers: KeyModifiers,
) -> EventResult {
    match app_state.screen {
        crate::tui::state::Screen::Dashboard => handle_dashboard_event(app_state, key, modifiers),
        crate::tui::state::Screen::Config => handle_config_event(app_state, key, modifiers),
        crate::tui::state::Screen::Scanning { .. } => {
            handle_scanning_event(app_state, key, modifiers)
        }
        crate::tui::state::Screen::Results => handle_results_event(app_state, key, modifiers),
        crate::tui::state::Screen::Preview { .. } => {
            handle_preview_event(app_state, key, modifiers)
        }
        crate::tui::state::Screen::Confirm { .. } => {
            handle_confirm_event(app_state, key, modifiers)
        }
        crate::tui::state::Screen::Cleaning { .. } => {
            handle_cleaning_event(app_state, key, modifiers)
        }
        crate::tui::state::Screen::Success { .. } => {
            handle_success_event(app_state, key, modifiers)
        }
        crate::tui::state::Screen::Restore { .. } => {
            handle_restore_event(app_state, key, modifiers)
        }
        crate::tui::state::Screen::DiskInsights { .. } => {
            handle_disk_insights_event(app_state, key, modifiers)
        }
    }
}

/// Handle a mouse event
pub fn handle_mouse_event(app_state: &mut AppState, mouse: MouseEvent) -> EventResult {
    match mouse.kind {
        // Standard scrolling: Wheel Down -> View Down (Index Increase)
        MouseEventKind::ScrollDown => match app_state.screen {
            crate::tui::state::Screen::Dashboard => {
                handle_dashboard_event(app_state, KeyCode::Down, KeyModifiers::empty())
            }
            crate::tui::state::Screen::Config => {
                handle_config_event(app_state, KeyCode::Down, KeyModifiers::empty())
            }
            crate::tui::state::Screen::Results => {
                handle_results_event(app_state, KeyCode::Down, KeyModifiers::empty())
            }
            crate::tui::state::Screen::Confirm { .. } => {
                handle_confirm_event(app_state, KeyCode::Down, KeyModifiers::empty())
            }
            crate::tui::state::Screen::DiskInsights { .. } => {
                handle_disk_insights_event(app_state, KeyCode::Down, KeyModifiers::empty())
            }
            _ => EventResult::Continue,
        },
        MouseEventKind::ScrollUp => match app_state.screen {
            crate::tui::state::Screen::Dashboard => {
                handle_dashboard_event(app_state, KeyCode::Up, KeyModifiers::empty())
            }
            crate::tui::state::Screen::Config => {
                handle_config_event(app_state, KeyCode::Up, KeyModifiers::empty())
            }
            crate::tui::state::Screen::Results => {
                handle_results_event(app_state, KeyCode::Up, KeyModifiers::empty())
            }
            crate::tui::state::Screen::Confirm { .. } => {
                handle_confirm_event(app_state, KeyCode::Up, KeyModifiers::empty())
            }
            crate::tui::state::Screen::DiskInsights { .. } => {
                handle_disk_insights_event(app_state, KeyCode::Up, KeyModifiers::empty())
            }
            _ => EventResult::Continue,
        },
        MouseEventKind::Down(MouseButton::Left) => match app_state.screen {
            crate::tui::state::Screen::Dashboard => {
                handle_dashboard_click(app_state, mouse.row, mouse.column)
            }
            crate::tui::state::Screen::Results => {
                handle_results_click(app_state, mouse.row, mouse.column)
            }
            crate::tui::state::Screen::Confirm { .. } => {
                handle_confirm_click(app_state, mouse.row, mouse.column)
            }
            crate::tui::state::Screen::Config => {
                handle_config_click(app_state, mouse.row, mouse.column)
            }
            _ => EventResult::Continue,
        },
        _ => EventResult::Continue,
    }
}

fn handle_dashboard_click(app_state: &mut AppState, row: u16, _col: u16) -> EventResult {
    // Layout in dashboard.rs:
    // Header: LOGO_WITH_TAGLINE_HEIGHT
    // Actions Title: 1
    // Spacing: 1
    // Actions List: 4 items (2 lines each?) -> No, ListItem is 1 cell height unless wrapped.
    // Dashboard actions are 2 lines each (Action + Desc). 4 * 2 = 8 lines?
    // Let's check dashboard.rs code.
    // ListItem::new(line) - line contains \n so it is 2 lines?
    // Wait, ratatui List items height is 1 by default unless wrapped?
    // In dashboard.rs: Span::raw("\n   ") is used. This forces multiline.
    // So each action is 2 lines. 4 actions = 8 lines. Border adds 2 lines. Padding 2 lines.
    // Total Actions Height ~ 12.

    let header_height = LOGO_WITH_TAGLINE_HEIGHT; // logo + tagline + spacing
    let _start_actions = header_height + 1; // +1 for "What would you like to do?"

    // This is approximate since we don't have the exact layout calc here.
    // Clicking is often brittle in TUI without re-running layout logic.
    // Simplified: If user clicks generally in the bottom area, we focus categories.
    // If top area, focus actions.

    if row < header_height + 14 {
        app_state.focus_actions = true;
        // Try to map row to action
        // It's hard to be precise without layout info.
    } else {
        app_state.focus_actions = false;
        // Map row to category index
        // Categories start below actions.
        // Let's assume approx start Y.
        // It is better to just toggle focus for now to avoid frustration.
    }
    EventResult::Continue
}

fn handle_results_click(app_state: &mut AppState, row: u16, _col: u16) -> EventResult {
    let header_height = LOGO_WITH_TAGLINE_HEIGHT;
    let summary_height = 5;
    let list_start_y = header_height + summary_height + 1; // +1 for border

    if row >= list_start_y && row < list_start_y + app_state.visible_height as u16 {
        let visual_index = (row - list_start_y) as usize;
        let data_index = app_state.scroll_offset + visual_index;

        // Bounds check
        if data_index < app_state.results_rows().len() {
            // Peek at the row to see if it's a spacer BEFORE updating cursor
            let rows = app_state.results_rows();
            if let Some(r) = rows.get(data_index) {
                if matches!(r, crate::tui::state::ResultsRow::Spacer) {
                    return EventResult::Continue;
                }
            }

            app_state.cursor = data_index;
            // If they clicked the checkbox column (approx first 6 chars), toggle
            if _col < 8 {
                handle_results_event(app_state, KeyCode::Char(' '), KeyModifiers::empty());
            } else if _col > 8 {
                // Determine what row is clicked
                // If it's a folder/category header, toggle expansion
                // If it's an item, maybe open it? Or just select.
                // Current behavior: Click selects (moves cursor).
                // Double click? Not easily supported.

                // Check if it is a header row, if so toggle expansion
                let rows = app_state.results_rows();
                if let Some(
                    crate::tui::state::ResultsRow::CategoryHeader { .. }
                    | crate::tui::state::ResultsRow::FolderHeader { .. },
                ) = rows.get(data_index)
                {
                    // Toggle expansion on click
                    handle_results_event(app_state, KeyCode::Enter, KeyModifiers::empty());
                }
            }
        }
    }
    EventResult::Continue
}

fn handle_confirm_click(app_state: &mut AppState, row: u16, _col: u16) -> EventResult {
    // Similar layout to results
    let header_height = LOGO_WITH_TAGLINE_HEIGHT;
    let summary_height = 5; // Warning message height is approx 5
    let list_start_y = header_height + summary_height + 1;

    if row >= list_start_y && row < list_start_y + app_state.visible_height as u16 {
        let visual_index = (row - list_start_y) as usize;
        let data_index = app_state.scroll_offset + visual_index;
        let rows = app_state.confirm_rows();

        if data_index < rows.len() {
            // Check for spacer
            if let Some(r) = rows.get(data_index) {
                if matches!(r, crate::tui::state::ConfirmRow::Spacer) {
                    return EventResult::Continue;
                }
            }

            app_state.cursor = data_index;
            if _col < 8 {
                handle_confirm_event(app_state, KeyCode::Char(' '), KeyModifiers::empty());
            } else {
                if let Some(r) = rows.get(data_index) {
                    match r {
                        crate::tui::state::ConfirmRow::CategoryHeader { .. }
                        | crate::tui::state::ConfirmRow::FolderHeader { .. } => {
                            handle_confirm_event(app_state, KeyCode::Enter, KeyModifiers::empty());
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    EventResult::Continue
}

fn handle_config_click(_app_state: &mut AppState, _row: u16, _col: u16) -> EventResult {
    // Config layout: Header + Content
    // Header = LOGO_WITH_TAGLINE_HEIGHT
    // Config content starts.
    // Config has 6 fields, spaced out.
    // This is hard to map without exact lines.
    // For now, allow scrolling and maybe focus change if we can guess.
    EventResult::Continue
}

fn handle_dashboard_event(
    app_state: &mut AppState,
    key: KeyCode,
    _modifiers: KeyModifiers,
) -> EventResult {
    // Clear any temporary message on key press
    app_state.dashboard_message = None;

    match key {
        KeyCode::Char('q') | KeyCode::Esc => {
            // Save category selections before quitting
            app_state.sync_categories_to_config();
            EventResult::Quit
        }
        KeyCode::Tab => {
            // Switch focus between panels
            app_state.focus_actions = !app_state.focus_actions;
            EventResult::Continue
        }
        KeyCode::Left => {
            // Focus actions panel
            app_state.focus_actions = true;
            EventResult::Continue
        }
        KeyCode::Right => {
            // Focus categories panel
            app_state.focus_actions = false;
            EventResult::Continue
        }
        KeyCode::Up => {
            if app_state.focus_actions {
                // Navigate in actions list
                if app_state.action_cursor > 0 {
                    app_state.action_cursor -= 1;
                }
            } else {
                // Navigate in categories list
                if app_state.cursor > 0 {
                    app_state.cursor -= 1;
                }
            }
            EventResult::Continue
        }
        KeyCode::Down => {
            if app_state.focus_actions {
                // Navigate in actions list (5 actions: Scan, Clean, Analyze, Restore, Config)
                if app_state.action_cursor < 4 {
                    app_state.action_cursor += 1;
                }
            } else {
                // Navigate in categories list
                if app_state.cursor < app_state.categories.len().saturating_sub(1) {
                    app_state.cursor += 1;
                }
            }
            EventResult::Continue
        }
        KeyCode::Char(' ') => {
            // Toggle category selection (only works when focused on categories)
            if !app_state.focus_actions {
                if let Some(cat) = app_state.categories.get_mut(app_state.cursor) {
                    cat.enabled = !cat.enabled;
                    // Save category selections to config
                    app_state.sync_categories_to_config();
                }
            }
            EventResult::Continue
        }
        KeyCode::Char('a') | KeyCode::Char('A') => {
            // Toggle all categories
            let all_enabled = app_state.categories.iter().all(|c| c.enabled);
            for cat in &mut app_state.categories {
                cat.enabled = !all_enabled;
            }
            // Save category selections to config
            app_state.sync_categories_to_config();
            EventResult::Continue
        }
        KeyCode::Enter => {
            // Based on action cursor, perform different actions
            if let 0..=2 = app_state.action_cursor {
                // Scan/Clean/Analyze require at least one category to be enabled
                if !app_state.categories.iter().any(|c| c.enabled) {
                    app_state.dashboard_message =
                        Some("⚠ Please select at least one category first!".to_string());
                    return EventResult::Continue;
                }
            }

            match app_state.action_cursor {
                0 => {
                    // Scan action
                    app_state.pending_action = crate::tui::state::PendingAction::None;
                    // Initialize progress bars for all selected categories
                    let mut category_progress = Vec::new();
                    for cat in &app_state.categories {
                        if cat.enabled {
                            category_progress.push(crate::tui::state::CategoryProgress {
                                name: cat.name.clone(),
                                completed: false,
                                progress_pct: 0.0,
                                size: None,
                            });
                        }
                    }

                    // Start scanning
                    app_state.screen = crate::tui::state::Screen::Scanning {
                        progress: crate::tui::state::ScanProgress {
                            current_category: String::new(),
                            current_path: None,
                            category_progress,
                            total_scanned: 0,
                            total_found: 0,
                            total_size: 0,
                        },
                    };
                }
                1 => {
                    // Clean action - need scan results first
                    app_state.pending_action = crate::tui::state::PendingAction::Clean;
                    let mut category_progress = Vec::new();
                    for cat in &app_state.categories {
                        if cat.enabled {
                            category_progress.push(crate::tui::state::CategoryProgress {
                                name: cat.name.clone(),
                                completed: false,
                                progress_pct: 0.0,
                                size: None,
                            });
                        }
                    }
                    app_state.screen = crate::tui::state::Screen::Scanning {
                        progress: crate::tui::state::ScanProgress {
                            current_category: String::new(),
                            current_path: None,
                            category_progress,
                            total_scanned: 0,
                            total_found: 0,
                            total_size: 0,
                        },
                    };
                }
                2 => {
                    // Analyze action - launch Disk Insights
                    // Determine scan path (default to user profile)
                    let scan_path = if let Ok(userprofile) = std::env::var("USERPROFILE") {
                        std::path::PathBuf::from(&userprofile)
                    } else {
                        std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
                    };

                    // Set pending action to trigger disk insights scan
                    app_state.pending_action = crate::tui::state::PendingAction::Analyze;
                    // Show scanning screen - the scan will be performed in the event loop
                    app_state.screen = crate::tui::state::Screen::Scanning {
                        progress: crate::tui::state::ScanProgress {
                            current_category: "Disk Insights".to_string(),
                            current_path: Some(scan_path),
                            category_progress: vec![crate::tui::state::CategoryProgress {
                                name: "Analyzing disk usage".to_string(),
                                completed: false,
                                progress_pct: 0.0,
                                size: None,
                            }],
                            total_scanned: 0,
                            total_found: 0,
                            total_size: 0,
                        },
                    };
                }
                3 => {
                    // Restore action
                    app_state.screen = crate::tui::state::Screen::Restore { result: None };
                }
                4 => {
                    // Config action - show config screen
                    // Ensure config exists on disk so we can open it
                    app_state.config = crate::config::Config::load_or_create();
                    app_state.apply_config_to_state();
                    app_state.reset_config_editor();
                    app_state.screen = crate::tui::state::Screen::Config;
                }
                _ => {}
            }
            EventResult::Continue
        }
        _ => EventResult::Continue,
    }
}

fn handle_config_event(
    app_state: &mut AppState,
    key: KeyCode,
    _modifiers: KeyModifiers,
) -> EventResult {
    use crate::tui::state::ConfigEditorMode;

    // Config screen fields:
    // 0 project_age_days (u64)
    // 1 min_age_days (u64)
    // 2 min_size_mb (u64)
    // 3 default_scan_path (string/none)
    // 4 animations (bool)
    // 5 refresh_rate_ms (u64)
    // 6 show_storage_info (bool)
    let fields_len = 7usize;

    // Editing mode has its own key handling.
    if let ConfigEditorMode::Editing { ref mut buffer } = app_state.config_editor.mode {
        match key {
            KeyCode::Esc => {
                app_state.config_editor.mode = ConfigEditorMode::View;
                app_state.config_editor.message = Some("Edit cancelled.".to_string());
                return EventResult::Continue;
            }
            KeyCode::Backspace => {
                buffer.pop();
                return EventResult::Continue;
            }
            KeyCode::Enter => {
                let selected = app_state.config_editor.selected;
                let raw = buffer.trim().to_string();

                let mut changed = false;
                let mut err: Option<String> = None;

                match selected {
                    0 => match raw.parse::<u64>() {
                        Ok(v) => {
                            app_state.config.thresholds.project_age_days = v;
                            changed = true;
                        }
                        Err(_) => err = Some("Invalid number for project age days.".to_string()),
                    },
                    1 => match raw.parse::<u64>() {
                        Ok(v) => {
                            app_state.config.thresholds.min_age_days = v;
                            changed = true;
                        }
                        Err(_) => err = Some("Invalid number for min age days.".to_string()),
                    },
                    2 => match raw.parse::<u64>() {
                        Ok(v) => {
                            app_state.config.thresholds.min_size_mb = v;
                            changed = true;
                        }
                        Err(_) => err = Some("Invalid number for min size (MB).".to_string()),
                    },
                    3 => {
                        if raw.is_empty() {
                            app_state.config.ui.default_scan_path = None;
                        } else {
                            app_state.config.ui.default_scan_path = Some(raw);
                        }
                        changed = true;
                    }
                    5 => match raw.parse::<u64>() {
                        Ok(v) => {
                            app_state.config.ui.refresh_rate_ms = v;
                            changed = true;
                        }
                        Err(_) => err = Some("Invalid number for refresh rate (ms).".to_string()),
                    },
                    _ => {}
                }

                if let Some(msg) = err {
                    app_state.config_editor.message = Some(msg);
                    return EventResult::Continue;
                }

                if changed {
                    app_state.apply_config_to_state();
                    match app_state.config.save() {
                        Ok(()) => app_state.config_editor.message = Some("Saved.".to_string()),
                        Err(e) => {
                            app_state.config_editor.message = Some(format!("Save failed: {e}"))
                        }
                    }
                }

                app_state.config_editor.mode = ConfigEditorMode::View;
                return EventResult::Continue;
            }
            KeyCode::Char(c) => {
                let selected = app_state.config_editor.selected;
                // Numeric fields accept digits only.
                let is_numeric = matches!(selected, 0 | 1 | 2 | 5);
                if is_numeric {
                    if c.is_ascii_digit() {
                        buffer.push(c);
                    }
                } else {
                    // Path field: accept most printable chars.
                    if !c.is_control() {
                        buffer.push(c);
                    }
                }
                return EventResult::Continue;
            }
            _ => return EventResult::Continue,
        }
    }

    // View mode.
    match key {
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q') => {
            app_state.screen = crate::tui::state::Screen::Dashboard;
            EventResult::Continue
        }
        KeyCode::Backspace | KeyCode::Left => {
            app_state.screen = crate::tui::state::Screen::Dashboard;
            EventResult::Continue
        }
        KeyCode::Up => {
            if app_state.config_editor.selected > 0 {
                app_state.config_editor.selected -= 1;
            }
            EventResult::Continue
        }
        KeyCode::Down => {
            if app_state.config_editor.selected + 1 < fields_len {
                app_state.config_editor.selected += 1;
            }
            EventResult::Continue
        }
        KeyCode::Char(' ') => {
            // Space toggles boolean fields when selected.
            match app_state.config_editor.selected {
                4 => {
                    app_state.config.ui.animations = !app_state.config.ui.animations;
                    match app_state.config.save() {
                        Ok(()) => app_state.config_editor.message = Some("Saved.".to_string()),
                        Err(e) => {
                            app_state.config_editor.message = Some(format!("Save failed: {e}"))
                        }
                    }
                    app_state.apply_config_to_state();
                }
                6 => {
                    app_state.config.ui.show_storage_info = !app_state.config.ui.show_storage_info;
                    match app_state.config.save() {
                        Ok(()) => app_state.config_editor.message = Some("Saved.".to_string()),
                        Err(e) => {
                            app_state.config_editor.message = Some(format!("Save failed: {e}"))
                        }
                    }
                    app_state.apply_config_to_state();
                }
                _ => {}
            }
            EventResult::Continue
        }
        KeyCode::Enter => {
            match app_state.config_editor.selected {
                4 => {
                    // Toggle bool
                    app_state.config.ui.animations = !app_state.config.ui.animations;
                    match app_state.config.save() {
                        Ok(()) => app_state.config_editor.message = Some("Saved.".to_string()),
                        Err(e) => {
                            app_state.config_editor.message = Some(format!("Save failed: {e}"))
                        }
                    }
                    app_state.apply_config_to_state();
                }
                6 => {
                    // Toggle bool
                    app_state.config.ui.show_storage_info = !app_state.config.ui.show_storage_info;
                    match app_state.config.save() {
                        Ok(()) => app_state.config_editor.message = Some("Saved.".to_string()),
                        Err(e) => {
                            app_state.config_editor.message = Some(format!("Save failed: {e}"))
                        }
                    }
                    app_state.apply_config_to_state();
                }
                0 => {
                    app_state.config_editor.mode = ConfigEditorMode::Editing {
                        buffer: app_state.config.thresholds.project_age_days.to_string(),
                    };
                    app_state.config_editor.message =
                        Some("Edit value, then Enter to save (Esc cancels).".to_string());
                }
                1 => {
                    app_state.config_editor.mode = ConfigEditorMode::Editing {
                        buffer: app_state.config.thresholds.min_age_days.to_string(),
                    };
                    app_state.config_editor.message =
                        Some("Edit value, then Enter to save (Esc cancels).".to_string());
                }
                2 => {
                    app_state.config_editor.mode = ConfigEditorMode::Editing {
                        buffer: app_state.config.thresholds.min_size_mb.to_string(),
                    };
                    app_state.config_editor.message =
                        Some("Edit value, then Enter to save (Esc cancels).".to_string());
                }
                3 => {
                    let current = app_state
                        .config
                        .ui
                        .default_scan_path
                        .clone()
                        .unwrap_or_default();
                    app_state.config_editor.mode = ConfigEditorMode::Editing { buffer: current };
                    app_state.config_editor.message = Some(
                        "Edit path (blank = auto-detect). Enter saves; Esc cancels.".to_string(),
                    );
                }
                5 => {
                    app_state.config_editor.mode = ConfigEditorMode::Editing {
                        buffer: app_state.config.ui.refresh_rate_ms.to_string(),
                    };
                    app_state.config_editor.message =
                        Some("Edit value, then Enter to save (Esc cancels).".to_string());
                }
                _ => {}
            }
            EventResult::Continue
        }
        KeyCode::Char('s') | KeyCode::Char('S') => {
            match app_state.config.save() {
                Ok(()) => app_state.config_editor.message = Some("Saved.".to_string()),
                Err(e) => app_state.config_editor.message = Some(format!("Save failed: {e}")),
            }
            app_state.apply_config_to_state();
            EventResult::Continue
        }
        KeyCode::Char('r') | KeyCode::Char('R') => {
            app_state.config = crate::config::Config::load_or_create();
            app_state.apply_config_to_state();
            app_state.config_editor.message = Some("Reloaded from disk.".to_string());
            EventResult::Continue
        }
        KeyCode::Char('o') | KeyCode::Char('O') => {
            open_config_file();
            EventResult::Continue
        }
        _ => EventResult::Continue,
    }
}

fn open_config_file() {
    // Ensure the file exists
    let _ = crate::config::Config::load_or_create();
    let Ok(path) = crate::config::Config::config_path() else {
        return;
    };

    let path_str = path.display().to_string();
    let quoted_path = format!("\"{}\"", path_str);

    // Best-effort: spawn and ignore any errors.
    if cfg!(target_os = "windows") {
        // `start` needs a window title argument; empty string is fine.
        let _ = Command::new("cmd")
            // Quote the path so spaces work correctly.
            .args(["/C", "start", "", &quoted_path])
            .spawn();
    } else if cfg!(target_os = "macos") {
        let _ = Command::new("open").arg(&path_str).spawn();
    } else {
        let _ = Command::new("xdg-open").arg(&path_str).spawn();
    }
}

/// Open a file or directory in the system's default application/file manager
/// For files, opens the parent folder and selects/focuses the file
fn open_file(path: &std::path::Path) {
    let path_str = path.display().to_string();
    // Best-effort: spawn and ignore any errors.
    if cfg!(target_os = "windows") {
        if path.is_dir() {
            // For directories, just open them
            let _ = Command::new("explorer").arg(&path_str).spawn();
        } else {
            // For files, open the parent folder and select the file
            // explorer /select,"<file_path>" opens Explorer and selects the file
            // We pass "/select," and the path as separate arguments.
            // process::Command handles quoting of the path if it has spaces.
            let _ = Command::new("explorer")
                .arg("/select,")
                .arg(&path_str)
                .spawn();
        }
    } else if cfg!(target_os = "macos") {
        if path.is_dir() {
            // For directories, just open them
            let _ = Command::new("open").arg(&path_str).spawn();
        } else {
            // For files, reveal in Finder (opens parent folder and selects file)
            let _ = Command::new("open").args(["-R", &path_str]).spawn();
        }
    } else {
        // Linux: Open parent directory (file selection not universally supported)
        if path.is_dir() {
            let _ = Command::new("xdg-open").arg(&path_str).spawn();
        } else {
            // Try to open parent directory and select file if possible
            if let Some(parent) = path.parent() {
                // Some file managers support selecting a file, but it's not universal
                // For now, just open the parent directory
                let parent_str = parent.display().to_string();
                let _ = Command::new("xdg-open").arg(&parent_str).spawn();
            }
        }
    }
}

fn handle_scanning_event(
    app_state: &mut AppState,
    key: KeyCode,
    _modifiers: KeyModifiers,
) -> EventResult {
    match key {
        KeyCode::Esc => {
            // Cancel scan - return to dashboard
            app_state.screen = crate::tui::state::Screen::Dashboard;
            app_state.pending_action = crate::tui::state::PendingAction::None;
            EventResult::Continue
        }
        _ => EventResult::Continue,
    }
}

fn handle_results_event(
    app_state: &mut AppState,
    key: KeyCode,
    modifiers: KeyModifiers,
) -> EventResult {
    // If in search mode, handle typing
    if app_state.search_mode {
        match key {
            KeyCode::Esc => {
                // Exit search mode, clear query
                app_state.search_mode = false;
                app_state.search_query.clear();
                app_state.cursor = 0;
                app_state.scroll_offset = 0;
                return EventResult::Continue;
            }
            KeyCode::Enter => {
                // Confirm search, stay in results with filter active
                app_state.search_mode = false;
                return EventResult::Continue;
            }
            KeyCode::Backspace => {
                app_state.search_query.pop();
                app_state.cursor = 0;
                app_state.scroll_offset = 0;
                return EventResult::Continue;
            }
            KeyCode::Char(c) => {
                // Only accept printable characters
                if !c.is_control() {
                    app_state.search_query.push(c);
                    app_state.cursor = 0;
                    app_state.scroll_offset = 0;
                }
                return EventResult::Continue;
            }
            // Allow navigation while searching - will fall through to normal handling
            KeyCode::Up | KeyCode::Down => {
                // Fall through to normal navigation handling
            }
            _ => return EventResult::Continue,
        }
    }

    // Get rows (filtered if search query is active)
    let rows = if app_state.search_query.is_empty() {
        app_state.results_rows()
    } else {
        app_state.filtered_results_rows()
    };
    let max_row = rows.len().saturating_sub(1);

    if rows.is_empty() {
        app_state.cursor = 0;
        app_state.scroll_offset = 0;
    } else {
        // Ensure cursor is within valid bounds
        if app_state.cursor > max_row {
            app_state.cursor = max_row;
        }
        // Ensure scroll_offset is valid
        if app_state.scroll_offset > max_row {
            app_state.scroll_offset = max_row;
        }
    }

    // Helper: move cursor to next/prev selectable row (skip spacers).
    // Also ensures cursor stays within bounds and scrolls into view.
    fn move_cursor(
        app_state: &mut AppState,
        rows: &[crate::tui::state::ResultsRow],
        delta: i32,
        visible_height: usize,
    ) {
        if rows.is_empty() {
            app_state.cursor = 0;
            app_state.scroll_offset = 0;
            return;
        }

        let max_row = rows.len().saturating_sub(1);
        let mut cur = app_state.cursor as i32;

        loop {
            cur += delta;
            if cur < 0 {
                // Can't go further up, clamp to 0
                if rows[0] != crate::tui::state::ResultsRow::Spacer {
                    app_state.cursor = 0;
                }
                break;
            }
            if cur as usize >= rows.len() {
                // Can't go further down, clamp to max_row
                if max_row < rows.len() && rows[max_row] != crate::tui::state::ResultsRow::Spacer {
                    app_state.cursor = max_row;
                }
                break;
            }
            if rows[cur as usize] != crate::tui::state::ResultsRow::Spacer {
                app_state.cursor = cur as usize;
                break;
            }
        }

        // Ensure cursor is within bounds
        if app_state.cursor > max_row {
            app_state.cursor = max_row;
        }

        // Adjust scroll to keep cursor visible
        if app_state.cursor < app_state.scroll_offset {
            // Cursor is above visible area, scroll up
            app_state.scroll_offset = app_state.cursor;
        } else if app_state.cursor >= app_state.scroll_offset + visible_height {
            // Cursor is below visible area, scroll down
            app_state.scroll_offset = app_state
                .cursor
                .saturating_sub(visible_height.saturating_sub(1));
        }

        // Ensure scroll_offset is valid
        let max_scroll = max_row.saturating_sub(visible_height.saturating_sub(1));
        if app_state.scroll_offset > max_scroll {
            app_state.scroll_offset = max_scroll.max(0);
        }
    }

    let visible_height = app_state.visible_height;

    match key {
        KeyCode::Char('q') | KeyCode::Char('Q') => EventResult::Quit,
        KeyCode::Char('/') => {
            // Enter search mode
            app_state.search_mode = true;
            EventResult::Continue
        }
        KeyCode::Esc => {
            // If there's an active search filter, clear it; otherwise go back to Dashboard
            if !app_state.search_query.is_empty() {
                app_state.search_query.clear();
                app_state.cursor = 0;
                app_state.scroll_offset = 0;
            } else {
                // Go back to Dashboard
                app_state.screen = crate::tui::state::Screen::Dashboard;
            }
            EventResult::Continue
        }
        KeyCode::Backspace => {
            // If there's an active search filter, clear it to show all items; otherwise go back to Dashboard
            if !app_state.search_query.is_empty() {
                app_state.search_query.clear();
                app_state.cursor = 0;
                app_state.scroll_offset = 0;
            } else {
                // Go back to Dashboard
                app_state.screen = crate::tui::state::Screen::Dashboard;
            }
            EventResult::Continue
        }
        KeyCode::Up => {
            move_cursor(app_state, &rows, -1, visible_height);
            EventResult::Continue
        }
        KeyCode::Down => {
            move_cursor(app_state, &rows, 1, visible_height);
            EventResult::Continue
        }
        KeyCode::Right => {
            if !rows.is_empty() && app_state.cursor < rows.len() {
                let row = rows[app_state.cursor];
                match row {
                    crate::tui::state::ResultsRow::CategoryHeader { group_idx } => {
                        if let Some(group) = app_state.category_groups.get_mut(group_idx) {
                            if !group.expanded {
                                group.expanded = true;
                            } else {
                                move_cursor(app_state, &rows, 1, visible_height);
                            }
                        }
                    }
                    crate::tui::state::ResultsRow::FolderHeader {
                        group_idx,
                        folder_idx,
                    } => {
                        if let Some(group) = app_state.category_groups.get_mut(group_idx) {
                            if let Some(folder) = group.folder_groups.get_mut(folder_idx) {
                                if !folder.expanded {
                                    folder.expanded = true;
                                } else {
                                    move_cursor(app_state, &rows, 1, visible_height);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            EventResult::Continue
        }
        KeyCode::Left => {
            if !rows.is_empty() && app_state.cursor < rows.len() {
                let row = rows[app_state.cursor];
                match row {
                    crate::tui::state::ResultsRow::CategoryHeader { group_idx } => {
                        if let Some(group) = app_state.category_groups.get_mut(group_idx) {
                            if group.expanded {
                                group.expanded = false;
                            }
                        }
                    }
                    crate::tui::state::ResultsRow::FolderHeader {
                        group_idx,
                        folder_idx,
                    } => {
                        // If expanded, collapse
                        let mut collapsed_now = false;
                        if let Some(group) = app_state.category_groups.get_mut(group_idx) {
                            if let Some(folder) = group.folder_groups.get_mut(folder_idx) {
                                if folder.expanded {
                                    folder.expanded = false;
                                    collapsed_now = true;
                                }
                            }
                        }

                        // If we didn't just collapse it (was already collapsed), jump to parent category
                        if !collapsed_now {
                            // Find category header for this group
                            // Search backwards for CategoryHeader with same group_idx
                            for i in (0..app_state.cursor).rev() {
                                if let crate::tui::state::ResultsRow::CategoryHeader {
                                    group_idx: g_idx,
                                } = rows[i]
                                {
                                    if g_idx == group_idx {
                                        app_state.cursor = i;
                                        // Ensure scroll
                                        if app_state.cursor < app_state.scroll_offset {
                                            app_state.scroll_offset = app_state.cursor;
                                        }
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    crate::tui::state::ResultsRow::Item { item_idx: _ } => {
                        // Find parent header (Folder or Category)
                        // Search backwards for the first header
                        for i in (0..app_state.cursor).rev() {
                            match rows[i] {
                                crate::tui::state::ResultsRow::FolderHeader { .. }
                                | crate::tui::state::ResultsRow::CategoryHeader { .. } => {
                                    app_state.cursor = i;
                                    if app_state.cursor < app_state.scroll_offset {
                                        app_state.scroll_offset = app_state.cursor;
                                    }
                                    break;
                                }
                                _ => {}
                            }
                        }
                    }
                    crate::tui::state::ResultsRow::Spacer => {}
                }
            }
            EventResult::Continue
        }
        KeyCode::Tab => {
            // Jump to next category header (wrap around).
            if rows.is_empty() {
                return EventResult::Continue;
            }

            let start = (app_state.cursor + 1).min(rows.len());
            let mut found = None;

            for (i, row) in rows.iter().enumerate().skip(start) {
                if matches!(row, crate::tui::state::ResultsRow::CategoryHeader { .. }) {
                    found = Some(i);
                    break;
                }
            }
            if found.is_none() {
                for (i, row) in rows
                    .iter()
                    .enumerate()
                    .take(app_state.cursor.min(rows.len().saturating_sub(1)) + 1)
                {
                    if matches!(row, crate::tui::state::ResultsRow::CategoryHeader { .. }) {
                        found = Some(i);
                        break;
                    }
                }
            }

            if let Some(i) = found {
                app_state.cursor = i;

                // Ensure cursor is scrolled into view
                let visible_height = app_state.visible_height;
                if app_state.cursor < app_state.scroll_offset {
                    // Cursor is above visible area, scroll up
                    app_state.scroll_offset = app_state.cursor;
                } else if app_state.cursor >= app_state.scroll_offset + visible_height {
                    // Cursor is below visible area, scroll down to show it
                    app_state.scroll_offset = app_state
                        .cursor
                        .saturating_sub(visible_height.saturating_sub(1));
                }

                // Ensure scroll_offset is valid
                let max_scroll = max_row.saturating_sub(visible_height.saturating_sub(1));
                if app_state.scroll_offset > max_scroll {
                    app_state.scroll_offset = max_scroll.max(0);
                }
            }
            EventResult::Continue
        }
        KeyCode::BackTab => {
            // Jump to previous category header (wrap around).
            if rows.is_empty() {
                return EventResult::Continue;
            }

            let mut found = None;

            if app_state.cursor > 0 {
                for i in (0..=app_state.cursor - 1).rev() {
                    if matches!(
                        rows[i],
                        crate::tui::state::ResultsRow::CategoryHeader { .. }
                    ) {
                        found = Some(i);
                        break;
                    }
                }
            }
            if found.is_none() {
                for i in (app_state.cursor..rows.len()).rev() {
                    if matches!(
                        rows[i],
                        crate::tui::state::ResultsRow::CategoryHeader { .. }
                    ) {
                        found = Some(i);
                        break;
                    }
                }
            }

            if let Some(i) = found {
                app_state.cursor = i;

                // Ensure cursor is scrolled into view
                let visible_height = app_state.visible_height;
                if app_state.cursor < app_state.scroll_offset {
                    // Cursor is above visible area, scroll up
                    app_state.scroll_offset = app_state.cursor;
                } else if app_state.cursor >= app_state.scroll_offset + visible_height {
                    // Cursor is below visible area, scroll down to show it
                    app_state.scroll_offset = app_state
                        .cursor
                        .saturating_sub(visible_height.saturating_sub(1));
                }

                // Ensure scroll_offset is valid
                let max_scroll = max_row.saturating_sub(visible_height.saturating_sub(1));
                if app_state.scroll_offset > max_scroll {
                    app_state.scroll_offset = max_scroll.max(0);
                }
            }
            EventResult::Continue
        }
        KeyCode::Char(' ') => {
            let Some(row) = rows.get(app_state.cursor) else {
                return EventResult::Continue;
            };

            match *row {
                crate::tui::state::ResultsRow::Item { item_idx } => {
                    app_state.toggle_items([item_idx]);
                }
                crate::tui::state::ResultsRow::FolderHeader {
                    group_idx,
                    folder_idx,
                } => {
                    let items = app_state.folder_item_indices(group_idx, folder_idx);
                    app_state.toggle_items(items);
                }
                crate::tui::state::ResultsRow::CategoryHeader { group_idx } => {
                    let items = app_state.category_item_indices(group_idx);
                    app_state.toggle_items(items);
                }
                crate::tui::state::ResultsRow::Spacer => {}
            }

            EventResult::Continue
        }
        KeyCode::Enter => {
            // Check for Ctrl+Enter to expand/collapse sibling groups
            if modifiers.contains(KeyModifiers::CONTROL) {
                let Some(row) = rows.get(app_state.cursor) else {
                    return EventResult::Continue;
                };

                // Expand/collapse sibling groups based on current row
                match *row {
                    crate::tui::state::ResultsRow::CategoryHeader { group_idx: _ } => {
                        // Expand/collapse all sibling categories
                        // Determine the current state (if any sibling is expanded, collapse all; otherwise expand all)
                        let any_expanded = app_state.category_groups.iter().any(|g| g.expanded);
                        for group in &mut app_state.category_groups {
                            group.expanded = !any_expanded;
                        }
                    }
                    crate::tui::state::ResultsRow::FolderHeader {
                        group_idx,
                        folder_idx: _,
                    } => {
                        // Expand/collapse all sibling folders in the same category
                        if let Some(group) = app_state.category_groups.get_mut(group_idx) {
                            // Determine the current state (if any sibling folder is expanded, collapse all; otherwise expand all)
                            let any_expanded = group.folder_groups.iter().any(|f| f.expanded);
                            for folder in &mut group.folder_groups {
                                folder.expanded = !any_expanded;
                            }
                        }
                    }
                    crate::tui::state::ResultsRow::Item { item_idx: _ } => {
                        // Find the parent folder or category and expand/collapse siblings
                        // Search backwards to find the folder header or category header
                        let mut category_group_idx = None;
                        let mut folder_idx = None;

                        for i in (0..=app_state.cursor).rev() {
                            if let Some(r) = rows.get(i) {
                                match *r {
                                    crate::tui::state::ResultsRow::FolderHeader {
                                        group_idx,
                                        folder_idx: f_idx,
                                    } => {
                                        category_group_idx = Some(group_idx);
                                        folder_idx = Some(f_idx);
                                        break;
                                    }
                                    crate::tui::state::ResultsRow::CategoryHeader { group_idx } => {
                                        category_group_idx = Some(group_idx);
                                        break;
                                    }
                                    _ => {}
                                }
                            }
                        }

                        if let Some(group_idx) = category_group_idx {
                            if let Some(_f_idx) = folder_idx {
                                // Item is under a folder - expand/collapse all sibling folders
                                if let Some(group) = app_state.category_groups.get_mut(group_idx) {
                                    let any_expanded =
                                        group.folder_groups.iter().any(|f| f.expanded);
                                    for folder in &mut group.folder_groups {
                                        folder.expanded = !any_expanded;
                                    }
                                }
                            } else {
                                // Item is directly under category - expand/collapse all sibling categories
                                let any_expanded =
                                    app_state.category_groups.iter().any(|g| g.expanded);
                                for group in &mut app_state.category_groups {
                                    group.expanded = !any_expanded;
                                }
                            }
                        }
                    }
                    crate::tui::state::ResultsRow::Spacer => {}
                }
                return EventResult::Continue;
            }

            // Regular Enter (without Ctrl) - expand/collapse groups
            let Some(row) = rows.get(app_state.cursor) else {
                return EventResult::Continue;
            };

            match *row {
                crate::tui::state::ResultsRow::Item { item_idx } => {
                    // Open the file in the system's default application
                    if let Some(item) = app_state.all_items.get(item_idx) {
                        open_file(&item.path);
                    }
                }
                crate::tui::state::ResultsRow::FolderHeader {
                    group_idx,
                    folder_idx,
                } => {
                    if let Some(group) = app_state.category_groups.get_mut(group_idx) {
                        if let Some(folder) = group.folder_groups.get_mut(folder_idx) {
                            folder.expanded = !folder.expanded;
                        }
                    }
                }
                crate::tui::state::ResultsRow::CategoryHeader { group_idx } => {
                    if let Some(group) = app_state.category_groups.get_mut(group_idx) {
                        group.expanded = !group.expanded;
                    }
                }
                crate::tui::state::ResultsRow::Spacer => {}
            }

            EventResult::Continue
        }
        KeyCode::Char('c') | KeyCode::Char('C') => {
            // Confirm deletion
            if app_state.selected_count() > 0 {
                // Snapshot current selection when entering confirm screen
                app_state.confirm_snapshot = app_state.selected_items.clone();
                // Cache confirm groups for stable ordering
                app_state.cache_confirm_groups();
                app_state.cursor = 0;
                app_state.scroll_offset = 0;
                app_state.screen = crate::tui::state::Screen::Confirm { permanent: false };
            }
            EventResult::Continue
        }
        _ => EventResult::Continue,
    }
}

fn handle_preview_event(
    app_state: &mut AppState,
    key: KeyCode,
    _modifiers: KeyModifiers,
) -> EventResult {
    match key {
        KeyCode::Esc => {
            // Back to results
            app_state.screen = crate::tui::state::Screen::Results;
            EventResult::Continue
        }
        KeyCode::Char('d') | KeyCode::Char('D') => {
            // Delete this item
            if let crate::tui::state::Screen::Preview { index } = app_state.screen {
                app_state.selected_items.insert(index);
                // Snapshot current selection when entering confirm screen
                app_state.confirm_snapshot = app_state.selected_items.clone();
                // Cache confirm groups for stable ordering
                app_state.cache_confirm_groups();
                app_state.cursor = 0;
                app_state.scroll_offset = 0;
                app_state.screen = crate::tui::state::Screen::Confirm { permanent: false };
            }
            EventResult::Continue
        }
        KeyCode::Char('e') | KeyCode::Char('E') => {
            // Exclude from results
            if let crate::tui::state::Screen::Preview { index } = app_state.screen {
                // Remove the item and reindex selection set.
                app_state.all_items.remove(index);

                let mut new_selected = std::collections::HashSet::new();
                for &sel in &app_state.selected_items {
                    if sel == index {
                        continue;
                    }
                    if sel > index {
                        new_selected.insert(sel - 1);
                    } else {
                        new_selected.insert(sel);
                    }
                }
                app_state.selected_items = new_selected;

                // Rebuild grouping so indices remain valid.
                app_state.rebuild_groups_from_all_items();

                // Reset cursor/scroll to a safe position.
                app_state.cursor = 0;
                app_state.scroll_offset = 0;
                app_state.screen = crate::tui::state::Screen::Results;
            }
            EventResult::Continue
        }
        _ => EventResult::Continue,
    }
}

fn handle_confirm_event(
    app_state: &mut AppState,
    key: KeyCode,
    _modifiers: KeyModifiers,
) -> EventResult {
    let rows = app_state.confirm_rows();
    let max_row = rows.len().saturating_sub(1);

    if !rows.is_empty() {
        // Ensure cursor is within valid bounds
        if app_state.cursor > max_row {
            app_state.cursor = max_row;
        }
        // Ensure scroll_offset is valid
        if app_state.scroll_offset > max_row {
            app_state.scroll_offset = max_row;
        }
    }

    // Helper: move cursor to next/prev selectable row (skip spacers).
    fn move_cursor(
        app_state: &mut AppState,
        rows: &[crate::tui::state::ConfirmRow],
        delta: i32,
        visible_height: usize,
    ) {
        if rows.is_empty() {
            app_state.cursor = 0;
            app_state.scroll_offset = 0;
            return;
        }

        let max_row = rows.len().saturating_sub(1);
        let mut cur = app_state.cursor as i32;

        loop {
            cur += delta;
            if cur < 0 {
                if rows[0] != crate::tui::state::ConfirmRow::Spacer {
                    app_state.cursor = 0;
                }
                break;
            }
            if cur as usize >= rows.len() {
                if max_row < rows.len() && rows[max_row] != crate::tui::state::ConfirmRow::Spacer {
                    app_state.cursor = max_row;
                }
                break;
            }
            if rows[cur as usize] != crate::tui::state::ConfirmRow::Spacer {
                app_state.cursor = cur as usize;
                break;
            }
        }

        if app_state.cursor > max_row {
            app_state.cursor = max_row;
        }

        // Adjust scroll to keep cursor visible
        if app_state.cursor < app_state.scroll_offset {
            app_state.scroll_offset = app_state.cursor;
        } else if app_state.cursor >= app_state.scroll_offset + visible_height {
            app_state.scroll_offset = app_state
                .cursor
                .saturating_sub(visible_height.saturating_sub(1));
        }

        let max_scroll = max_row.saturating_sub(visible_height.saturating_sub(1));
        if app_state.scroll_offset > max_scroll {
            app_state.scroll_offset = max_scroll.max(0);
        }
    }

    let visible_height = app_state.visible_height;

    match key {
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
            // Cancel - back to results
            // Clear confirm snapshot and cache since we're leaving confirm screen
            app_state.confirm_snapshot.clear();
            app_state.clear_confirm_cache();
            app_state.screen = crate::tui::state::Screen::Results;
            EventResult::Continue
        }
        KeyCode::Up => {
            move_cursor(app_state, &rows, -1, visible_height);
            EventResult::Continue
        }
        KeyCode::Down => {
            move_cursor(app_state, &rows, 1, visible_height);
            EventResult::Continue
        }
        KeyCode::Char(' ') => {
            // Toggle selection
            let Some(row) = rows.get(app_state.cursor) else {
                return EventResult::Continue;
            };

            match *row {
                crate::tui::state::ConfirmRow::Item { item_idx } => {
                    // Toggle selection - item stays visible, checkbox updates
                    app_state.toggle_items([item_idx]);
                }
                crate::tui::state::ConfirmRow::FolderHeader {
                    cat_idx,
                    folder_idx,
                } => {
                    // Toggle all items in this folder
                    let confirm_groups = app_state.confirm_category_groups();
                    if let Some(group) = confirm_groups.get(cat_idx) {
                        if let Some(folder) = group.folder_groups.get(folder_idx) {
                            let folder_items: Vec<usize> = folder.items.clone();
                            app_state.toggle_items(folder_items);
                        }
                    }
                }
                crate::tui::state::ConfirmRow::CategoryHeader { cat_idx } => {
                    // Toggle all items in this category (from confirm_snapshot)
                    let confirm_groups = app_state.confirm_category_groups();
                    if let Some(group) = confirm_groups.get(cat_idx) {
                        // Get all items in this category from the snapshot
                        let item_indices: Vec<usize> = if group.grouped_by_folder {
                            group
                                .folder_groups
                                .iter()
                                .flat_map(|fg| fg.items.iter().copied())
                                .collect()
                        } else {
                            group.items.clone()
                        };
                        app_state.toggle_items(item_indices);
                    }
                }
                _ => {}
            }

            EventResult::Continue
        }
        KeyCode::Enter => {
            // Toggle expansion for category or folder headers
            let Some(row) = rows.get(app_state.cursor) else {
                return EventResult::Continue;
            };

            match *row {
                crate::tui::state::ConfirmRow::CategoryHeader { cat_idx } => {
                    // Get category name and toggle expansion
                    let confirm_groups = app_state.confirm_category_groups();
                    if let Some(group) = confirm_groups.get(cat_idx) {
                        app_state.toggle_confirm_category(&group.name);
                    }
                }
                crate::tui::state::ConfirmRow::FolderHeader {
                    cat_idx,
                    folder_idx,
                } => {
                    // Toggle folder expansion
                    let confirm_groups = app_state.confirm_category_groups();
                    if let Some(group) = confirm_groups.get(cat_idx) {
                        if let Some(folder) = group.folder_groups.get(folder_idx) {
                            // Find and toggle the folder in the original category_groups
                            app_state.toggle_confirm_folder(&group.name, &folder.folder_name);
                        }
                    }
                }
                _ => {}
            }

            EventResult::Continue
        }
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            // Confirm deletion (to Recycle Bin)
            if app_state.selected_count() == 0 {
                // No items selected, do nothing
                return EventResult::Continue;
            }
            if let crate::tui::state::Screen::Confirm { permanent } = app_state.screen {
                app_state.permanent_delete = permanent;
                // Clear confirm snapshot and cache since we're leaving confirm screen
                app_state.confirm_snapshot.clear();
                app_state.clear_confirm_cache();
                app_state.screen = crate::tui::state::Screen::Cleaning {
                    progress: crate::tui::state::CleanProgress {
                        current_category: String::new(),
                        current_path: None,
                        cleaned: 0,
                        total: app_state.selected_count() as u64,
                        errors: 0,
                    },
                };
            }
            EventResult::Continue
        }
        KeyCode::Char('p') | KeyCode::Char('P') => {
            // Permanent delete - toggle the permanent flag in Confirm screen
            if app_state.selected_count() == 0 {
                // No items selected, do nothing
                return EventResult::Continue;
            }
            if let crate::tui::state::Screen::Confirm { ref mut permanent } = app_state.screen {
                *permanent = true;
                // Then trigger cleaning with permanent flag set
                app_state.permanent_delete = true;
                // Clear confirm snapshot and cache since we're leaving confirm screen
                app_state.confirm_snapshot.clear();
                app_state.clear_confirm_cache();
                app_state.screen = crate::tui::state::Screen::Cleaning {
                    progress: crate::tui::state::CleanProgress {
                        current_category: String::new(),
                        current_path: None,
                        cleaned: 0,
                        total: app_state.selected_count() as u64,
                        errors: 0,
                    },
                };
            }
            EventResult::Continue
        }
        _ => EventResult::Continue,
    }
}

fn handle_cleaning_event(
    _app_state: &mut AppState,
    _key: KeyCode,
    _modifiers: KeyModifiers,
) -> EventResult {
    // Cleaning is in progress - ignore input until complete
    EventResult::Continue
}

fn handle_success_event(
    app_state: &mut AppState,
    key: KeyCode,
    _modifiers: KeyModifiers,
) -> EventResult {
    match key {
        KeyCode::Esc | KeyCode::Backspace | KeyCode::Char('b') | KeyCode::Char('B') => {
            // Navigate back to Results if there are remaining items
            if !app_state.all_items.is_empty() {
                app_state.screen = crate::tui::state::Screen::Results;
                // Reset cursor to safe position
                app_state.cursor = 0;
                app_state.scroll_offset = 0;
            } else {
                // No items left, return to dashboard
                *app_state = AppState::new();
            }
            EventResult::Continue
        }
        _ => {
            // Any other key returns to dashboard with a fresh start
            *app_state = AppState::new();
            EventResult::Continue
        }
    }
}

fn handle_restore_event(
    app_state: &mut AppState,
    key: KeyCode,
    _modifiers: KeyModifiers,
) -> EventResult {
    match key {
        KeyCode::Esc
        | KeyCode::Backspace
        | KeyCode::Char('b')
        | KeyCode::Char('B')
        | KeyCode::Char('q')
        | KeyCode::Char('Q') => {
            // Return to dashboard
            *app_state = AppState::new();
            EventResult::Continue
        }
        _ => EventResult::Continue,
    }
}

fn handle_disk_insights_event(
    app_state: &mut AppState,
    key: KeyCode,
    _modifiers: KeyModifiers,
) -> EventResult {
    use crate::disk_usage::{find_folder_by_path, SortBy};

    if let crate::tui::state::Screen::DiskInsights {
        ref insights,
        ref mut current_path,
        ref mut cursor,
        ref mut sort_by,
    } = app_state.screen
    {
        // Get current folder node
        let current_node =
            find_folder_by_path(&insights.root, current_path).unwrap_or(&insights.root);

        // Filter children folders if search query is active
        let mut children = current_node.children.clone();
        let mut files = current_node.files.clone();
        if !app_state.search_query.is_empty() {
            let query = app_state.search_query.to_lowercase();
            children.retain(|child| child.name.to_lowercase().contains(&query));
            files.retain(|file| file.name.to_lowercase().contains(&query));
        }

        // Sort children folders (must match render order)
        match *sort_by {
            SortBy::Size => children.sort_by(|a, b| b.size.cmp(&a.size)),
            SortBy::Name => children.sort_by(|a, b| a.name.cmp(&b.name)),
            SortBy::Files => children.sort_by(|a, b| b.file_count.cmp(&a.file_count)),
        }

        // Sort files (must match render order)
        match *sort_by {
            SortBy::Size => files.sort_by(|a, b| b.size.cmp(&a.size)),
            SortBy::Name => files.sort_by(|a, b| a.name.cmp(&b.name)),
            SortBy::Files => {
                // For files, Files sort doesn't make sense, so use size
                files.sort_by(|a, b| b.size.cmp(&a.size));
            }
        }

        let children_count = children.len();
        let files_count = files.len();
        let total_items = children_count + files_count;

        // Ensure cursor is within bounds of filtered list
        if *cursor >= total_items && total_items > 0 {
            *cursor = total_items.saturating_sub(1);
        }

        // If in search mode, handle typing
        if app_state.search_mode {
            match key {
                KeyCode::Esc => {
                    // Exit search mode, clear query
                    app_state.search_mode = false;
                    app_state.search_query.clear();
                    *cursor = 0;
                    return EventResult::Continue;
                }
                KeyCode::Enter => {
                    // Confirm search, stay in disk insights with filter active
                    app_state.search_mode = false;
                    *cursor = 0;
                    return EventResult::Continue;
                }
                KeyCode::Backspace => {
                    app_state.search_query.pop();
                    *cursor = 0;
                    return EventResult::Continue;
                }
                KeyCode::Char(c) => {
                    // Only accept printable characters
                    if !c.is_control() {
                        app_state.search_query.push(c);
                        *cursor = 0;
                    }
                    return EventResult::Continue;
                }
                // Allow navigation while searching - will fall through to normal handling
                KeyCode::Up | KeyCode::Down => {
                    // Fall through to normal navigation handling
                }
                _ => return EventResult::Continue,
            }
        }

        match key {
            KeyCode::Char('q') | KeyCode::Char('Q') => {
                // Go back to Results if there are scan results, otherwise Dashboard
                if !app_state.all_items.is_empty() || !app_state.category_groups.is_empty() {
                    app_state.screen = crate::tui::state::Screen::Results;
                } else {
                    app_state.screen = crate::tui::state::Screen::Dashboard;
                }
                app_state.search_query.clear();
                EventResult::Continue
            }
            KeyCode::Esc => {
                // If there's an active search filter, clear it; otherwise go back to Results or Dashboard
                if !app_state.search_query.is_empty() {
                    app_state.search_query.clear();
                    *cursor = 0;
                    EventResult::Continue
                } else {
                    // Go back to Results if there are scan results, otherwise Dashboard
                    if !app_state.all_items.is_empty() || !app_state.category_groups.is_empty() {
                        app_state.screen = crate::tui::state::Screen::Results;
                    } else {
                        app_state.screen = crate::tui::state::Screen::Dashboard;
                    }
                    app_state.search_query.clear();
                    EventResult::Continue
                }
            }
            KeyCode::Char('/') => {
                // Enter search mode
                app_state.search_mode = true;
                EventResult::Continue
            }
            KeyCode::Backspace => {
                // If there's an active search filter, clear it; otherwise navigate back to parent or Results
                if !app_state.search_query.is_empty() {
                    app_state.search_query.clear();
                    *cursor = 0;
                } else {
                    // Navigate back to parent if not at root
                    if let Some(parent) = current_path.parent() {
                        if parent != insights.root.path.as_path()
                            && parent.starts_with(insights.root.path.as_path())
                        {
                            *current_path = parent.to_path_buf();
                            *cursor = 0;
                        } else {
                            // At root, go back to Results if there are scan results, otherwise Dashboard
                            if !app_state.all_items.is_empty()
                                || !app_state.category_groups.is_empty()
                            {
                                app_state.screen = crate::tui::state::Screen::Results;
                            } else {
                                app_state.screen = crate::tui::state::Screen::Dashboard;
                            }
                            app_state.search_query.clear();
                        }
                    } else {
                        // At root, go back to Results if there are scan results, otherwise Dashboard
                        if !app_state.all_items.is_empty() || !app_state.category_groups.is_empty()
                        {
                            app_state.screen = crate::tui::state::Screen::Results;
                        } else {
                            app_state.screen = crate::tui::state::Screen::Dashboard;
                        }
                        app_state.search_query.clear();
                    }
                }
                EventResult::Continue
            }
            KeyCode::Up => {
                if *cursor > 0 {
                    *cursor -= 1;
                } else {
                    // Wrap to end if at top
                    *cursor = total_items.saturating_sub(1);
                }
                EventResult::Continue
            }
            KeyCode::Down => {
                if *cursor < total_items.saturating_sub(1) {
                    *cursor += 1;
                } else {
                    // Wrap to beginning if at bottom
                    *cursor = 0;
                }
                EventResult::Continue
            }
            KeyCode::Enter => {
                // Drill into selected folder or open file
                if *cursor < children_count {
                    // Selected item is a folder - drill into it
                    let selected_child = &children[*cursor];
                    // Only navigate if folder has files or has subdirectories
                    if selected_child.file_count > 0
                        || !selected_child.children.is_empty()
                        || !selected_child.files.is_empty()
                    {
                        *current_path = selected_child.path.clone();
                        *cursor = 0;
                        // Clear search when entering a folder
                        app_state.search_query.clear();
                    }
                    // If folder is empty (0 files and no children), do nothing
                } else {
                    // Selected item is a file - open it in file explorer
                    let file_index = *cursor - children_count;
                    if file_index < files.len() {
                        let selected_file = &files[file_index];
                        open_file(&selected_file.path);
                    }
                }
                EventResult::Continue
            }
            KeyCode::Char('s') | KeyCode::Char('S') => {
                // Cycle sort order: size -> name -> files -> size
                *sort_by = match *sort_by {
                    SortBy::Size => SortBy::Name,
                    SortBy::Name => SortBy::Files,
                    SortBy::Files => SortBy::Size,
                };
                EventResult::Continue
            }
            KeyCode::Char('l') | KeyCode::Char('L') => {
                // Show largest files (could open a modal or switch view)
                // For now, just continue - could be enhanced later
                EventResult::Continue
            }
            _ => EventResult::Continue,
        }
    } else {
        EventResult::Continue
    }
}
