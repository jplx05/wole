//! Restore selection screen - choose restore type

use crate::tui::{
    state::AppState,
    theme::Styles,
    widgets::{
        logo::{render_logo, render_tagline, LOGO_WITH_TAGLINE_HEIGHT},
        shortcuts::{get_shortcuts, render_shortcuts},
    },
};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
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
    if let crate::tui::state::Screen::RestoreSelection { cursor } = app_state.screen {
        let restore_options = [
            (
                "Restore from Last Deletion",
                "Restore files from the most recent deletion session",
            ),
            (
                "Restore All Recycle Bin",
                "Restore all contents from the Recycle Bin",
            ),
        ];

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Title
                Constraint::Min(1),   // Options list
            ])
            .split(area);

        // Title
        let title = Paragraph::new("Select Restore Type")
            .style(Styles::header())
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Styles::border())
                    .title("RESTORE"),
            );
        f.render_widget(title, chunks[0]);

        // Options list
        let items: Vec<ListItem> = restore_options
            .iter()
            .enumerate()
            .map(|(i, (title, desc))| {
                let is_selected = i == cursor;
                let title_style = if is_selected {
                    Styles::selected()
                } else {
                    Styles::emphasis()
                };

                let prefix = if is_selected { "> " } else { "  " };

                let line = Line::from(vec![
                    Span::styled(prefix, title_style),
                    Span::styled(*title, title_style),
                    Span::raw("\n   "),
                    Span::styled(*desc, Styles::secondary()),
                ]);
                ListItem::new(line)
            })
            .collect();

        let border_style = Styles::border();

        let padding = if area.width < 30 {
            ratatui::widgets::Padding::new(0, 1, 0, 1)
        } else {
            ratatui::widgets::Padding::uniform(1)
        };

        let list = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border_style)
                .title("OPTIONS")
                .padding(padding),
        );

        let mut list_state = ratatui::widgets::ListState::default();
        list_state.select(Some(cursor));
        f.render_stateful_widget(list, chunks[1], &mut list_state);
    }
}
