//! Shortcuts bar widget

use crate::tui::theme::Styles;
use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

/// Render shortcuts bar at the bottom of the screen
pub fn render_shortcuts(f: &mut Frame, area: Rect, shortcuts: &[(&str, &str)]) {
    if shortcuts.is_empty() {
        return;
    }

    // Calculate available width (accounting for padding)
    let available_width = area.width.saturating_sub(2);

    // Build spans and check if they fit
    let mut spans: Vec<Span> = vec![];
    let mut current_width = 0;

    for (i, (key, desc)) in shortcuts.iter().enumerate() {
        let separator = if i > 0 { " • " } else { "" };
        let key_text = format!("[{}]", key);
        let desc_text = format!(" {}", desc);
        let item_text = format!("{}{}{}", separator, key_text, desc_text);
        let item_width = item_text.len() as u16;

        // Check if adding this item would exceed width
        if i > 0 && current_width + item_width > available_width {
            // Add ellipsis and break
            spans.push(Span::styled(" ...", Styles::secondary()));
            break;
        }

        if i > 0 {
            spans.push(Span::styled(separator, Styles::secondary()));
        }
        spans.push(Span::styled(key_text, Styles::emphasis()));
        spans.push(Span::styled(desc_text, Styles::secondary()));

        current_width += item_width;
    }

    let line = Line::from(spans);
    let paragraph = Paragraph::new(line)
        .block(
            Block::default()
                .borders(Borders::TOP)
                .border_style(Styles::border())
                .padding(ratatui::widgets::Padding::new(0, 1, 0, 1)),
        )
        .style(Styles::secondary())
        .alignment(ratatui::layout::Alignment::Left)
        .wrap(ratatui::widgets::Wrap { trim: true });

    f.render_widget(paragraph, area);
}

/// Get shortcuts for a screen type
pub fn get_shortcuts(
    screen: &crate::tui::state::Screen,
    app_state: Option<&crate::tui::state::AppState>,
) -> Vec<(&'static str, &'static str)> {
    match screen {
        crate::tui::state::Screen::Dashboard => vec![
            ("Tab", "Switch Panel"),
            ("↑↓", "Navigate"),
            ("Space", "Toggle Category"),
            ("Enter", "Execute Action"),
            ("A", "Select All"),
            ("Q", "Quit"),
        ],
        crate::tui::state::Screen::Config => vec![
            ("↑↓", "Select Field"),
            ("Enter", "Edit/Toggle"),
            ("Space", "Toggle (bool)"),
            ("S", "Save"),
            ("R", "Reload"),
            ("O", "Open File"),
            ("Esc", "Back"),
        ],
        crate::tui::state::Screen::Scanning { .. } => vec![("Esc", "Cancel")],
        crate::tui::state::Screen::Results => {
            if app_state.map(|s| s.search_mode).unwrap_or(false) {
                vec![
                    ("Type", "Search"),
                    ("Enter", "Confirm"),
                    ("Esc", "Cancel"),
                    ("↑↓", "Navigate"),
                ]
            } else if app_state
                .map(|s| !s.search_query.is_empty())
                .unwrap_or(false)
            {
                vec![
                    ("/", "Search"),
                    ("Esc", "Clear Filter"),
                    ("↑↓", "Navigate"),
                    ("Tab", "Next Category"),
                    ("Space", "Toggle"),
                    ("Enter", "Open File/Expand"),
                    ("Ctrl+Enter", "Toggle All"),
                    ("C", "Clean Selected"),
                    ("Q", "Quit"),
                ]
            } else {
                vec![
                    ("/", "Search"),
                    ("↑↓", "Navigate"),
                    ("Tab", "Next Category"),
                    ("Space", "Toggle"),
                    ("Enter", "Open File/Expand"),
                    ("Ctrl+Enter", "Toggle All"),
                    ("C", "Clean Selected"),
                    ("Esc", "Back"),
                    ("Q", "Quit"),
                ]
            }
        }
        crate::tui::state::Screen::Preview { .. } => {
            vec![("Esc", "Back"), ("D", "Delete"), ("E", "Exclude")]
        }
        crate::tui::state::Screen::Confirm { .. } => vec![
            ("↑↓", "Navigate"),
            ("Space", "Toggle"),
            ("Enter", "Expand"),
            ("Y", "Delete"),
            ("N", "Cancel"),
            ("P", "Permanent"),
        ],
        crate::tui::state::Screen::Cleaning { .. } => vec![],
        crate::tui::state::Screen::Success { .. } => {
            // Check if there are remaining items to show back navigation
            let has_remaining = app_state
                .map(|state| !state.all_items.is_empty())
                .unwrap_or(false);

            if has_remaining {
                vec![("Esc/B", "Back to Results"), ("Any Key", "Dashboard")]
            } else {
                vec![("Any Key", "Dashboard")]
            }
        }
        crate::tui::state::Screen::RestoreSelection { .. } => vec![
            ("↑↓", "Navigate"),
            ("Enter", "Select"),
            ("Esc/B/Q", "Back"),
        ],
        crate::tui::state::Screen::Restore { .. } => vec![("Esc/B/Q", "Back to Dashboard")],
        crate::tui::state::Screen::DiskInsights { .. } => vec![
            ("↑↓", "Navigate"),
            ("Enter", "Drill In"),
            ("Backspace", "Go Back"),
            ("S", "Sort"),
            ("Q/Esc", "Quit"),
        ],
        crate::tui::state::Screen::Status { .. } => vec![
            ("Esc/Q", "Back"),
            ("R", "Refresh"),
        ],
        crate::tui::state::Screen::Optimize { .. } => {
            if app_state
                .and_then(|s| {
                    if let crate::tui::state::Screen::Optimize { running, .. } = &s.screen {
                        Some(*running)
                    } else {
                        None
                    }
                })
                .unwrap_or(false)
            {
                vec![("Esc", "Cancel")]
            } else if app_state
                .and_then(|s| {
                    if let crate::tui::state::Screen::Optimize { results, cursor, .. } = &s.screen {
                        if !results.is_empty() {
                            if let Some(result) = results.get(*cursor) {
                                // Check if selected result is a failed operation
                                Some(!result.success)
                            } else {
                                Some(false)
                            }
                        } else {
                            Some(false)
                        }
                    } else {
                        None
                    }
                })
                .unwrap_or(false)
            {
                vec![
                    ("↑↓", "Navigate"),
                    ("Enter", "Retry"),
                    ("Esc/Q", "Back to Options"),
                ]
            } else if app_state
                .and_then(|s| {
                    if let crate::tui::state::Screen::Optimize { results, .. } = &s.screen {
                        Some(!results.is_empty())
                    } else {
                        None
                    }
                })
                .unwrap_or(false)
            {
                vec![
                    ("↑↓", "Navigate"),
                    ("Enter", "Back to Options"),
                    ("Esc/Q", "Back to Options"),
                ]
            } else {
                vec![
                    ("↑↓", "Navigate"),
                    ("Space", "Toggle"),
                    ("A", "Select All"),
                    ("Enter", "Run"),
                    ("Esc/Q", "Back"),
                ]
            }
        }
    }
}
