//! Scanning screen with progress bars

use crate::tui::{
    state::AppState,
    theme::Styles,
    widgets::{
        logo::{render_logo, render_tagline, LOGO_WITH_TAGLINE_HEIGHT},
        progress::render_category_progress,
        shortcuts::{get_shortcuts, render_shortcuts},
    },
};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::spinner;

/// Generate a short fun comparison for the amount of space found
#[allow(dead_code)]
fn fun_comparison_short(bytes: u64) -> Option<String> {
    const MB: u64 = 1_000_000;
    const GB: u64 = 1_000_000_000;

    let game_size: u64 = 50 * GB; // ~50 GB for AAA game
    let node_modules_size: u64 = 500 * MB; // ~500 MB average node_modules
    let floppy_size: u64 = 1_440_000; // 1.44 MB floppy disk

    if bytes >= 10 * GB {
        let count = bytes / game_size;
        if count >= 1 {
            Some(format!("(~{} game installs!)", count))
        } else {
            Some("(partial game install!)".to_string())
        }
    } else if bytes >= 500 * MB {
        let count = bytes / node_modules_size;
        Some(format!("(~{} node_modules!)", count))
    } else if bytes >= 10 * MB {
        let count = bytes / floppy_size;
        Some(format!("(~{} floppies!)", count))
    } else {
        None
    }
}

pub fn render(f: &mut Frame, app_state: &AppState) {
    let area = f.area();
    let spinner = spinner::get_spinner(app_state.tick);

    // Detect small viewport to adjust rendering
    let is_small = area.height < 20 || area.width < 60;

    // Adjust constraints for small viewports
    let status_height = if is_small { 2 } else { 3 };
    let shortcuts_height = if is_small { 2 } else { 3 };
    let min_progress_height = if is_small { 3 } else { 8 };

    // Layout: logo+tagline, status, progress, shortcuts (no stats/progress section while scanning)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(LOGO_WITH_TAGLINE_HEIGHT), // Logo + 2 blank lines + tagline
            Constraint::Length(status_height),            // Status with spinner
            Constraint::Min(min_progress_height),         // Progress bars
            Constraint::Length(shortcuts_height),         // Shortcuts
        ])
        .split(area);

    // Logo and tagline (using reusable widgets)
    render_logo(f, chunks[0]);
    render_tagline(f, chunks[0]);

    // Status with animated spinner
    if let crate::tui::state::Screen::Scanning { ref progress } = app_state.screen {
        let status_text = if progress.current_category.is_empty() {
            format!("{}  Scanning...", spinner)
        } else {
            format!("{}  Scanning {}...", spinner, progress.current_category)
        };

        let status_lines = vec![Line::from(vec![Span::styled(
            status_text,
            Styles::emphasis(),
        )])];
        // Use simpler borders on small viewports to avoid rendering issues
        let borders = if is_small {
            Borders::TOP | Borders::BOTTOM
        } else {
            Borders::ALL
        };
        let padding = if is_small {
            ratatui::widgets::Padding::new(0, 0, 0, 0)
        } else {
            ratatui::widgets::Padding::uniform(1)
        };
        let status = Paragraph::new(status_lines).block(
            Block::default()
                .borders(borders)
                .border_style(Styles::border())
                .title("SCANNING")
                .padding(padding),
        );
        f.render_widget(status, chunks[1]);

        // Check if scanning is still in progress (not all categories completed)
        let is_scanning = progress.category_progress.iter().any(|cat| !cat.completed);

        // Progress bars with current category display (similar to file deletion)
        if progress.category_progress.is_empty() {
            // Animated initialization message
            let dots = match (app_state.tick / 5) % 4 {
                0 => "",
                1 => ".",
                2 => "..",
                3 => "...",
                _ => "",
            };
            let empty_msg = Paragraph::new(Line::from(vec![Span::styled(
                format!("{}  Initializing scan{}", spinner, dots),
                Styles::emphasis(),
            )]))
            .block(
                Block::default()
                    .borders(if is_small {
                        Borders::TOP | Borders::BOTTOM
                    } else {
                        Borders::ALL
                    })
                    .border_style(Styles::border())
                    .title("CATEGORIES"),
            );
            f.render_widget(empty_msg, chunks[2]);
        } else {
            // Split the progress area to show current file being scanned
            let progress_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(1),                                  // Progress bars (flexible)
                    Constraint::Length(if is_scanning { 3 } else { 0 }), // Current file display (only while scanning)
                ])
                .split(chunks[2]);

            render_category_progress(
                f,
                progress_chunks[0],
                &progress.category_progress,
                app_state.tick,
            );

            // Show current file being scanned (similar to file deletion)
            // Always show loader when scanning, even if no category status yet
            if is_scanning {
                // Use faster spinner animation - multiply by 4 to make it more noticeable
                // (spinner divides by 2 internally, so tick*4 gives us tick*2 speed)
                let loader_spinner = spinner::get_spinner(app_state.tick * 4);

                let current_file_text = if let Some(ref current_path) = progress.current_path {
                    let path_str = crate::utils::display_path(current_path);
                    let max_len = (progress_chunks[1].width as usize).saturating_sub(20);
                    let display_path = if path_str.len() > max_len {
                        format!(
                            "...{}",
                            &path_str[path_str.len().saturating_sub(max_len.saturating_sub(3))..]
                        )
                    } else {
                        path_str
                    };
                    format!("{}  Reading: {}", loader_spinner, display_path)
                } else if !progress.current_category.is_empty() {
                    format!(
                        "{}  Scanning: {}",
                        loader_spinner, progress.current_category
                    )
                } else {
                    format!("{}  Scanning...", loader_spinner)
                };

                let current_file_paragraph = Paragraph::new(Line::from(vec![Span::styled(
                    current_file_text,
                    Styles::primary(),
                )]))
                .block(
                    Block::default()
                        .borders(if is_small {
                            Borders::TOP | Borders::BOTTOM
                        } else {
                            Borders::ALL
                        })
                        .border_style(Styles::border())
                        .title("CURRENT FILE"),
                );
                f.render_widget(current_file_paragraph, progress_chunks[1]);
            }
        }
    } else {
        // Fallback
        let is_small = area.height < 20 || area.width < 60;
        let empty_msg = Paragraph::new(Line::from(vec![Span::styled(
            "No scan in progress",
            Styles::secondary(),
        )]))
        .block(
            Block::default()
                .borders(if is_small {
                    Borders::TOP | Borders::BOTTOM
                } else {
                    Borders::ALL
                })
                .border_style(Styles::border()),
        );
        f.render_widget(empty_msg, chunks[2]);
    }

    // Shortcuts
    let shortcuts = get_shortcuts(&app_state.screen, Some(app_state));
    render_shortcuts(f, chunks[3], &shortcuts);
}

/// Render cleaning progress (similar to scanning)
pub fn render_cleaning(f: &mut Frame, app_state: &AppState) {
    let area = f.area();

    // Detect small viewport to adjust rendering
    let is_small = area.height < 20 || area.width < 60;
    let status_height = if is_small { 2 } else { 3 };
    let stats_height = if is_small { 2 } else { 3 };
    let shortcuts_height = if is_small { 2 } else { 3 };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(LOGO_WITH_TAGLINE_HEIGHT), // Logo + 2 blank lines + tagline
            Constraint::Length(status_height),            // Status
            Constraint::Min(1),                           // Progress
            Constraint::Length(stats_height),             // Stats
            Constraint::Length(shortcuts_height),         // Shortcuts
        ])
        .split(area);

    // Logo and tagline (using reusable widgets)
    render_logo(f, chunks[0]);
    render_tagline(f, chunks[0]);

    // Header with animated spinner
    // Use faster animation for cleaning (every 2 ticks instead of default)
    let cleaning_spinner = spinner::get_spinner(app_state.tick * 2);
    let header = Paragraph::new(Line::from(vec![Span::styled(
        format!("{}  Cleaning files...", cleaning_spinner),
        Styles::emphasis(),
    )]))
    .block(
        Block::default()
            .borders(if is_small {
                Borders::TOP | Borders::BOTTOM
            } else {
                Borders::ALL
            })
            .border_style(Styles::border())
            .title("CLEANING"),
    );
    f.render_widget(header, chunks[1]);

    // Progress
    if let crate::tui::state::Screen::Cleaning { ref progress } = app_state.screen {
        let progress_pct = if progress.total > 0 {
            progress.cleaned as f32 / progress.total as f32
        } else {
            0.0
        };

        use crate::tui::widgets::progress::render_progress_bar;
        use ratatui::layout::{Constraint, Direction, Layout};

        let progress_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Progress bar
                Constraint::Min(1),    // Current file display
            ])
            .split(chunks[2]);

        render_progress_bar(
            f,
            progress_chunks[0],
            &progress.current_category,
            progress_pct,
            None,
            &format!("{}/{}", progress.cleaned, progress.total),
            app_state.tick,
        );

        // Display current item being processed with animated loader
        let current_file_text = if let Some(ref current_path) = progress.current_path {
            // Truncate path if too long
            let path_str = current_path.display().to_string();
            let max_len = (progress_chunks[1].width as usize).saturating_sub(20); // Account for padding and "Working: " prefix
            let display_path = if path_str.len() > max_len {
                format!(
                    "...{}",
                    &path_str[path_str.len().saturating_sub(max_len.saturating_sub(3))..]
                )
            } else {
                path_str
            };
            // Use animated spinner for current file
            let file_spinner = spinner::get_spinner(app_state.tick * 2);
            format!("{}  Working: {}", file_spinner, display_path)
        } else {
            // Show animated "Preparing..." with spinner
            let prep_spinner = spinner::get_spinner(app_state.tick * 2);
            let dots = match (app_state.tick / 5) % 4 {
                0 => "",
                1 => ".",
                2 => "..",
                3 => "...",
                _ => "",
            };
            format!("{}  Preparing{}", prep_spinner, dots)
        };

        let current_file_paragraph = Paragraph::new(Line::from(vec![Span::styled(
            current_file_text,
            Styles::primary(),
        )]))
        .block(
            Block::default()
                .borders(if is_small {
                    Borders::TOP | Borders::BOTTOM
                } else {
                    Borders::ALL
                })
                .border_style(Styles::border())
                .title("CURRENT FILE"),
        );
        f.render_widget(current_file_paragraph, progress_chunks[1]);

        // Status
        let status_text = format!(
            "  Cleaned: {} items   │   Errors: {}",
            progress.cleaned, progress.errors
        );
        let status_paragraph = Paragraph::new(status_text).block(
            Block::default()
                .borders(if is_small {
                    Borders::TOP | Borders::BOTTOM
                } else {
                    Borders::ALL
                })
                .border_style(Styles::border())
                .title("STATUS"),
        );
        f.render_widget(status_paragraph, chunks[3]);
    }

    // Shortcuts (empty for cleaning)
    let shortcuts = get_shortcuts(&app_state.screen, Some(app_state));
    render_shortcuts(f, chunks[4], &shortcuts);
}
