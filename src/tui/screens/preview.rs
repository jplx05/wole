//! Preview screen with split file tree view

use crate::tui::{
    state::AppState,
    theme::Styles,
    widgets::{
        logo::{render_logo, render_tagline, LOGO_WITH_TAGLINE_HEIGHT},
        shortcuts::{get_shortcuts, render_shortcuts},
        tree::render_tree,
    },
};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use std::path::Path;

pub fn render(f: &mut Frame, app_state: &AppState) {
    let area = f.area();

    // Layout: logo+tagline, warning message, split view, shortcuts
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(LOGO_WITH_TAGLINE_HEIGHT), // Logo + 2 blank lines + tagline
            Constraint::Length(5),                        // Warning message
            Constraint::Min(1),                           // Split view
            Constraint::Length(3),                        // Shortcuts
        ])
        .split(area);

    // Logo and tagline
    render_logo(f, chunks[0]);
    render_tagline(f, chunks[0]);

    // Warning message - make it very clear this is a preview
    let index = if let crate::tui::state::Screen::Preview { index } = app_state.screen {
        index
    } else {
        0
    };

    let item = app_state.all_items.get(index);
    let selected_count = app_state.selected_items.len();

    let warning_lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  ⚠  ", Styles::warning()),
            Span::styled("PREVIEW MODE - No files deleted yet", Styles::title()),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Pressing [D] will delete ", Styles::primary()),
            Span::styled(format!("{}", selected_count), Styles::danger()),
            Span::styled(
                if selected_count == 1 {
                    " file "
                } else {
                    " files "
                },
                Styles::primary(),
            ),
            Span::styled("from the previous page", Styles::primary()),
        ]),
        Line::from(vec![Span::styled(
            "  This screen shows details for one selected file only",
            Styles::secondary(),
        )]),
    ];

    let warning = Paragraph::new(warning_lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Styles::warning())
            .title("PREVIEW - NOT DELETED YET")
            .padding(ratatui::widgets::Padding::uniform(1)),
    );
    f.render_widget(warning, chunks[1]);

    // Split view: tree left, info right
    if let Some(item) = item {
        let split_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(chunks[2]);

        // Left: File tree
        render_tree(
            f,
            split_chunks[0],
            &item.path,
            item.size_bytes,
            &app_state.scan_path,
        );

        // Right: Will Delete preview
        render_delete_preview(f, split_chunks[1], item, &app_state.scan_path);
    }

    // Shortcuts
    let shortcuts = get_shortcuts(&app_state.screen, Some(app_state));
    render_shortcuts(f, chunks[3], &shortcuts);
}

fn render_delete_preview(
    f: &mut Frame,
    area: Rect,
    item: &crate::tui::state::ResultItem,
    base_path: &Path,
) {
    let path_display = crate::utils::to_relative_path(&item.path, base_path);
    let path_truncated = if path_display.len() > 50 {
        format!("{}...", &path_display[..50])
    } else {
        path_display
    };

    let lines = vec![
        Line::from(vec![Span::styled(
            "THIS FILE WILL BE DELETED:",
            Styles::danger(),
        )]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Path: ", Styles::header()),
            Span::styled(path_truncated, Styles::primary()),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Size: ", Styles::header()),
            Span::styled(
                bytesize::to_string(item.size_bytes, false),
                Styles::emphasis(),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Category: ", Styles::header()),
            Span::styled(&item.category, crate::tui::theme::category_style(item.safe)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Status: ", Styles::header()),
            Span::styled(
                if item.safe {
                    "Safe to delete"
                } else {
                    "Review recommended"
                },
                if item.safe {
                    Styles::success()
                } else {
                    Styles::warning()
                },
            ),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "  ⚠ Remember: [D] deletes ALL selected files, not just this one",
            Styles::warning(),
        )]),
    ];

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Styles::danger())
            .title("FILE DETAILS - PREVIEW ONLY")
            .padding(ratatui::widgets::Padding::uniform(1)),
    );

    f.render_widget(paragraph, area);
}
