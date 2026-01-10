//! Dashboard screen - category selection

use crate::tui::{
    state::AppState,
    theme::Styles,
    widgets::{
        logo::{render_logo, render_tagline, LOGO_WITH_TAGLINE_HEIGHT},
        shortcuts::{get_shortcuts, render_shortcuts},
    },
};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

pub fn render(f: &mut Frame, app_state: &AppState) {
    let area = f.area();

    let is_small = area.height < 20 || area.width < 60;
    let shortcuts_height = if is_small { 2 } else { 3 };

    // Big header height for spacing + ASCII art title + 2 blank lines + tagline
    let header_height = LOGO_WITH_TAGLINE_HEIGHT;

    // Layout: header, content, shortcuts
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(header_height),
            Constraint::Min(1), // Content
            Constraint::Length(shortcuts_height),
        ])
        .split(area);

    // Render big title and tagline
    render_header(f, chunks[0], is_small);

    // Content area - single column layout
    render_content(f, chunks[1], app_state, is_small);

    // Shortcuts
    let shortcuts = get_shortcuts(&app_state.screen, Some(app_state));
    render_shortcuts(f, chunks[2], &shortcuts);
}

fn render_header(f: &mut Frame, area: Rect, _is_small: bool) {
    // Use reusable logo and tagline widgets
    render_logo(f, area);
    render_tagline(f, area);
}

fn render_actions(f: &mut Frame, area: Rect, app_state: &AppState) {
    let actions = [
        ("Scan", "Find cleanable files (safe, dry-run)"),
        ("Clean", "Delete selected files"),
        ("Analyze", "Explore disk usage (folder sizes)"),
        ("Restore", "Restore files from deletion or Recycle Bin"),
        ("Config", "View or modify settings"),
        ("Optimize", "Optimize Windows system performance"),
        ("Status", "Real-time system health dashboard"),
    ];

    let items: Vec<ListItem> = actions
        .iter()
        .enumerate()
        .map(|(i, (action, desc))| {
            let is_selected = i == app_state.action_cursor && app_state.focus_actions;
            let action_style = if is_selected {
                Styles::selected()
            } else {
                Styles::emphasis()
            };

            let prefix = if is_selected { "> " } else { "  " };

            // Always show full description - no truncation
            let line = Line::from(vec![
                Span::styled(prefix, action_style),
                Span::styled(*action, action_style),
                Span::raw("\n   "),
                Span::styled(*desc, Styles::secondary()),
            ]);
            ListItem::new(line)
        })
        .collect();

    let border_style = Styles::border();

    let title = if app_state.focus_actions {
        "ACTIONS"
    } else {
        "Actions"
    };

    // Adaptive padding based on screen size
    let padding = if area.width < 30 {
        ratatui::widgets::Padding::new(0, 1, 0, 1) // Minimal padding on small screens
    } else {
        ratatui::widgets::Padding::uniform(1)
    };

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(title)
            .padding(padding),
    );

    let mut list_state = ratatui::widgets::ListState::default();
    list_state.select(Some(app_state.action_cursor));
    f.render_stateful_widget(list, area, &mut list_state);
}

fn render_content(f: &mut Frame, area: Rect, app_state: &AppState, _is_small: bool) {
    // Single column layout - flow vertically, no columns
    // Use Min to ensure all actions are visible - each action is 2 lines, 4 actions = 8 lines + title + borders
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(12), // Actions section - enough space for all 4 actions with spacing
            Constraint::Min(0),  // Categories (flexible, uses remaining space)
        ])
        .split(area);

    // Actions section with proper spacing
    let action_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Title
            Constraint::Length(1), // Spacing
            Constraint::Min(14),   // Actions list - enough for all 6 actions (each is 2 lines)
        ])
        .split(chunks[0]);

    let (text, style) = if let Some(msg) = &app_state.dashboard_message {
        (
            msg.as_str(),
            Styles::warning().add_modifier(ratatui::style::Modifier::BOLD),
        )
    } else {
        ("What would you like to do?", Styles::primary())
    };

    let title = Paragraph::new(text)
        .style(style)
        .alignment(ratatui::layout::Alignment::Left);
    f.render_widget(title, action_chunks[0]);

    render_actions(f, action_chunks[2], app_state);

    // Categories section with proper spacing
    let category_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Title
            Constraint::Length(1), // Spacing
            Constraint::Min(1),    // Categories list
        ])
        .split(chunks[1]);

    // Title
    let title = Paragraph::new(Line::from(vec![Span::styled(
        "Select categories to scan:",
        Styles::header(),
    )]))
    .style(Styles::primary())
    .alignment(ratatui::layout::Alignment::Left);
    f.render_widget(title, category_chunks[0]);

    // Categories list
    let items: Vec<ListItem> = app_state
        .categories
        .iter()
        .enumerate()
        .map(|(i, cat)| {
            let is_selected = i == app_state.cursor && !app_state.focus_actions;
            let name_style = if is_selected {
                Styles::selected()
            } else if cat.enabled {
                Styles::emphasis()
            } else {
                Style::default()
            };

            // Split checkbox into brackets and inner content to style brackets separately when focused
            let bracket_style = if is_selected {
                Styles::selected()
            } else {
                Style::default()
            };

            let inner_content = if cat.enabled {
                ("X", Styles::checked())
            } else {
                (" ", Styles::secondary())
            };

            let prefix = if is_selected { "> " } else { "  " };
            // Truncate description on small screens
            let max_desc_len = (area.width.saturating_sub(20) as usize).max(15);
            let desc_text = if cat.description.len() > max_desc_len {
                format!("{}...", &cat.description[..max_desc_len])
            } else {
                cat.description.clone()
            };

            let line = Line::from(vec![
                Span::styled(prefix, name_style),
                Span::styled("[", bracket_style),
                Span::styled(inner_content.0, inner_content.1),
                Span::styled("]", bracket_style),
                Span::raw(" "),
                Span::styled(&cat.name, name_style),
                Span::raw("  "),
                Span::styled(desc_text, Styles::secondary()),
            ]);

            ListItem::new(line)
        })
        .collect();

    let border_style = Styles::border();

    // Adaptive title and padding
    let title = "Categories";

    let padding = if area.width < 30 {
        ratatui::widgets::Padding::new(0, 1, 0, 1) // Minimal padding on small screens
    } else {
        ratatui::widgets::Padding::uniform(1)
    };

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(title)
            .padding(padding),
    );

    let mut list_state = ratatui::widgets::ListState::default();
    list_state.select(Some(app_state.cursor));
    f.render_stateful_widget(list, category_chunks[2], &mut list_state);
}
