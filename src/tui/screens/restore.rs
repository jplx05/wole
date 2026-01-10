//! Restore screen - restore files from last deletion

use crate::tui::{
    state::AppState,
    theme::Styles,
    widgets::{
        logo::{render_logo, render_tagline, LOGO_WITH_TAGLINE_HEIGHT},
        progress::render_progress_bar,
        shortcuts::{get_shortcuts, render_shortcuts},
    },
};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

pub fn render(f: &mut Frame, app_state: &AppState) {
    let area = f.area();

    let is_small = area.height < 20 || area.width < 60;
    let shortcuts_height = if is_small { 2 } else { 3 };

    let header_height = LOGO_WITH_TAGLINE_HEIGHT;

    // Layout: header, content, shortcuts
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(header_height),
            Constraint::Min(1),
            Constraint::Length(shortcuts_height),
        ])
        .split(area);

    // Render header
    render_header(f, chunks[0], is_small);

    // Render content
    render_content(f, chunks[1], app_state, is_small);

    // Shortcuts
    let shortcuts = get_shortcuts(&app_state.screen, Some(app_state));
    render_shortcuts(f, chunks[2], &shortcuts);
}

fn render_header(f: &mut Frame, area: Rect, _is_small: bool) {
    render_logo(f, area);
    render_tagline(f, area);
}

fn render_content(f: &mut Frame, area: Rect, app_state: &AppState, _is_small: bool) {
    if let crate::tui::state::Screen::Restore {
        ref progress,
        ref result,
        restore_all_bin,
    } = app_state.screen
    {
        if let Some(ref restore_result) = result {
            // Show restore results
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Min(1)])
                .split(area);

            // Title
            let title_text = if restore_all_bin {
                "Restore Complete - All Recycle Bin"
            } else {
                "Restore Complete"
            };
            let title_block = if restore_all_bin { "Restore All" } else { "Restore" };
            let title = Paragraph::new(title_text)
                .style(Styles::header())
                .alignment(Alignment::Center)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Styles::border())
                        .title(title_block),
                );
            f.render_widget(title, chunks[0]);

            // Results
            let mut lines = vec![Line::from(vec![
                Span::styled("Restored: ", Styles::primary()),
                Span::styled(
                    format!("{} items", restore_result.restored),
                    if restore_result.restored > 0 {
                        Styles::success()
                    } else {
                        Styles::secondary()
                    },
                ),
            ])];

            if restore_result.restored > 0 {
                lines.push(Line::from(vec![
                    Span::styled("Size: ", Styles::primary()),
                    Span::styled(
                        bytesize::to_string(restore_result.restored_bytes, true),
                        Styles::success(),
                    ),
                ]));
            }

            if restore_result.errors > 0 {
                lines.push(Line::from(vec![
                    Span::styled("Errors: ", Styles::primary()),
                    Span::styled(format!("{}", restore_result.errors), Styles::error()),
                ]));
            }

            if restore_result.not_found > 0 {
                lines.push(Line::from(vec![
                    Span::styled("Not found: ", Styles::primary()),
                    Span::styled(
                        format!("{} items", restore_result.not_found),
                        Styles::muted(),
                    ),
                ]));
            }

            if restore_result.restored == 0
                && restore_result.errors == 0
                && restore_result.not_found == 0
            {
                let message = if restore_all_bin {
                    "Recycle Bin is empty. Nothing to restore."
                } else {
                    "No files to restore from last deletion session."
                };
                lines.push(Line::from(vec![Span::styled(
                    message,
                    Styles::muted(),
                )]));
            }

            let content = Paragraph::new(lines)
                .style(Styles::primary())
                .alignment(Alignment::Left)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Styles::border()),
                );
            f.render_widget(content, chunks[1]);
        } else if let Some(ref prog) = progress {
            // Show restore progress
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3), // Title
                    Constraint::Length(3), // Progress bar
                    Constraint::Min(1),    // Current file display
                    Constraint::Length(3), // Status
                ])
                .split(area);

            // Title
            let title_text = if restore_all_bin {
                "Restoring all Recycle Bin contents..."
            } else {
                "Restoring files from last deletion session..."
            };
            let title_block = if restore_all_bin { "Restore All" } else { "Restore" };
            let title = Paragraph::new(title_text)
                .style(Styles::header())
                .alignment(Alignment::Center)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Styles::border())
                        .title(title_block),
                );
            f.render_widget(title, chunks[0]);

            // Progress bar
            let progress_pct = if prog.total > 0 {
                (prog.restored + prog.errors + prog.not_found) as f32 / prog.total as f32
            } else {
                0.0
            };

            render_progress_bar(
                f,
                chunks[1],
                "Restoring files...",
                progress_pct,
                None,
                &format!(
                    "{}/{}",
                    prog.restored + prog.errors + prog.not_found,
                    prog.total
                ),
                app_state.tick,
            );

            // Display current file being restored
            let current_file_text = if let Some(ref current_path) = prog.current_path {
                // Truncate path if too long
                let path_str = current_path.display().to_string();
                let max_len = (chunks[2].width as usize).saturating_sub(4); // Account for padding
                let display_path = if path_str.len() > max_len {
                    format!(
                        "...{}",
                        &path_str[path_str.len().saturating_sub(max_len.saturating_sub(3))..]
                    )
                } else {
                    path_str
                };
                format!("  Restoring: {}", display_path)
            } else {
                "  Preparing...".to_string()
            };

            let current_file_paragraph = Paragraph::new(Line::from(vec![Span::styled(
                current_file_text,
                Styles::primary(),
            )]))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Styles::border())
                    .title("CURRENT FILE"),
            );
            f.render_widget(current_file_paragraph, chunks[2]);

            // Status
            let status_text = format!(
                "  Restored: {} items   │   Errors: {}   │   Not found: {}",
                prog.restored, prog.errors, prog.not_found
            );
            let status_paragraph = Paragraph::new(status_text).block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Styles::border())
                    .title("STATUS"),
            );
            f.render_widget(status_paragraph, chunks[3]);
        } else {
            // Show "Preparing..." message
            let message_text = if restore_all_bin {
                "Preparing to restore all Recycle Bin contents..."
            } else {
                "Preparing to restore files from last deletion..."
            };
            let message_block = if restore_all_bin { "Restore All" } else { "Restore" };
            let message = Paragraph::new(message_text)
                .style(Styles::primary())
                .alignment(Alignment::Center)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Styles::border())
                        .title(message_block),
                );
            f.render_widget(message, area);
        }
    }
}
