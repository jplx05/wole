//! Optimize screen - Windows system optimization

use crate::optimize::OptimizeResult;
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
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

pub fn render(f: &mut Frame, app_state: &AppState) {
    let area = f.area();

    let is_small = area.height < 20 || area.width < 60;
    let shortcuts_height = if is_small { 2 } else { 3 };

    let header_height = LOGO_WITH_TAGLINE_HEIGHT;
    
    // Ensure we have minimum space: header + content (7 lines) + shortcuts
    let min_content_height = 7;
    let min_total_height = header_height + min_content_height + shortcuts_height;
    
    // If viewport is too small, show a message instead
    if area.height < min_total_height || area.width < 20 {
        let msg = Paragraph::new("Terminal too small. Please resize to at least 20x25")
            .style(Styles::warning())
            .alignment(ratatui::layout::Alignment::Center);
        f.render_widget(msg, area);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(header_height),
            Constraint::Min(min_content_height), // Ensure minimum content height
            Constraint::Length(shortcuts_height),
        ])
        .split(area);

    render_header(f, chunks[0], is_small);
    render_content(f, chunks[1], app_state, is_small);

    let shortcuts = get_shortcuts(&app_state.screen, Some(app_state));
    render_shortcuts(f, chunks[2], &shortcuts);
}

fn render_header(f: &mut Frame, area: Rect, _is_small: bool) {
    render_logo(f, area);
    render_tagline(f, area);
}

fn render_content(f: &mut Frame, area: Rect, app_state: &AppState, _is_small: bool) {
    if let crate::tui::state::Screen::Optimize {
        cursor,
        selected,
        results,
        running,
        message,
    } = &app_state.screen
    {
        // Calculate how much space we need
        // Title: 1 line, spacing: 1 line, content: variable
        let title_height = 1;
        let spacing_height = 1;
        let content_height = area.height.saturating_sub(title_height + spacing_height);
        
        // Ensure we have at least some space for content
        // List widget needs: 2 borders + 2 padding + at least 2 lines for one item = 6 lines minimum
        // But we'll be more generous and require 7 lines to avoid any rendering artifacts
        if content_height < 7 || area.width < 20 {
            // Not enough space, just show a message
            let msg = Paragraph::new("Not enough space to display optimizations")
                .style(Styles::warning())
                .alignment(ratatui::layout::Alignment::Left);
            f.render_widget(msg, area);
            return;
        }

        // Use the calculated content height (already validated to be >= 7)
        let safe_content_height = content_height;
        
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(title_height), // Title
                Constraint::Length(spacing_height), // Spacing
                Constraint::Length(safe_content_height), // Content - use exact height to prevent overflow
            ])
            .split(area);

        // Title
        let title_text = if *running {
            "Running optimizations..."
        } else if !results.is_empty() {
            "Optimization Results"
        } else {
            "Select optimizations to run:"
        };

        let title = Paragraph::new(title_text)
            .style(Styles::primary())
            .alignment(ratatui::layout::Alignment::Left);
        f.render_widget(title, chunks[0]);

        if *running {
            // Show progress message
            let progress_text = if results.is_empty() {
                "Starting optimizations..."
            } else {
                &format!("Completed {} optimizations...", results.len())
            };
            let progress = Paragraph::new(progress_text)
                .style(Styles::secondary())
                .alignment(ratatui::layout::Alignment::Left);
            f.render_widget(progress, chunks[2]);
        } else if !results.is_empty() {
            // Show results with cursor support and optional message
            render_results_with_message(f, chunks[2], results, cursor, message);
        } else {
            // Show optimization options
            render_options(f, chunks[2], cursor, selected);
        }
    }
}

fn render_options(f: &mut Frame, area: Rect, cursor: &usize, selected: &std::collections::HashSet<usize>) {
    let options = [
        ("DNS Cache", "Flush DNS cache (ipconfig /flushdns)", false),
        ("Thumbnails", "Clear thumbnail cache", false),
        ("Icons", "Rebuild icon cache and restart Explorer", false),
        ("Databases", "Optimize browser databases (VACUUM)", false),
        ("Fonts", "Restart Font Cache Service - fixes font display issues (requires admin)", true),
        ("Memory", "Clear standby memory - frees up RAM (requires admin)", true),
        ("Network", "Reset network stack - fixes connection issues (requires admin)", true),
        ("Bluetooth", "Restart Bluetooth service - fixes Bluetooth problems (requires admin)", true),
        ("Search", "Restart Windows Search - rebuilds search index (requires admin)", true),
        ("Explorer", "Restart Windows Explorer - refreshes desktop and file manager", false),
    ];

    // Ensure area is valid (at least 7x20 for borders, padding, and at least one item)
    // List needs: 2 borders + 2 padding + 2 lines per item = minimum 6, but use 7 to be safe
    if area.width < 20 || area.height < 7 {
        let msg = Paragraph::new("Window too small")
            .style(Styles::warning())
            .alignment(ratatui::layout::Alignment::Center);
        f.render_widget(msg, area);
        return;
    }
    
    // Ensure area doesn't exceed terminal bounds (safety check)
    let terminal_area = f.area();
    let safe_area = Rect {
        x: area.x,
        y: area.y,
        width: area.width.min(terminal_area.width.saturating_sub(area.x)),
        height: area.height.min(terminal_area.height.saturating_sub(area.y)),
    };
    
    // Double-check the safe area is still valid
    if safe_area.width < 20 || safe_area.height < 7 {
        let msg = Paragraph::new("Window too small")
            .style(Styles::warning())
            .alignment(ratatui::layout::Alignment::Center);
        f.render_widget(msg, area);
        return;
    }

    // Calculate max description length based on available width
    // Account for: prefix (2) + checkbox (3) + space (1) + name + admin_note + indent (3) = ~15-20 chars
    let max_desc_width = safe_area.width.saturating_sub(20).max(20) as usize;
    
    // Create items, but limit rendering to what fits
    let items: Vec<ListItem> = options
        .iter()
        .enumerate()
        .map(|(i, (name, desc, needs_admin))| {
            let is_selected = i == *cursor;
            let is_checked = selected.contains(&i);
            let name_style = if is_selected {
                Styles::selected()
            } else {
                Styles::emphasis()
            };

            let prefix = if is_selected { "> " } else { "  " };
            let checkbox = if is_checked { "[X]" } else { "[ ]" };
            let checkbox_style = if is_checked {
                Styles::checked()
            } else {
                Styles::secondary()
            };

            let admin_note = if *needs_admin {
                " (admin)"
            } else {
                ""
            };
            
            // Truncate description if too long to prevent wrapping/overflow
            let desc_text = if desc.len() > max_desc_width {
                format!("{}...", &desc[..max_desc_width.saturating_sub(3)])
            } else {
                desc.to_string()
            };

            let line = Line::from(vec![
                Span::styled(prefix, name_style),
                Span::styled(checkbox, checkbox_style),
                Span::raw(" "),
                Span::styled(*name, name_style),
                Span::styled(admin_note, Styles::muted()),
                Span::raw("\n   "),
                Span::styled(desc_text, Styles::secondary()),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Styles::border())
                .title("Optimizations")
                .padding(ratatui::widgets::Padding::uniform(1)),
        );

    let mut list_state = ratatui::widgets::ListState::default();
    list_state.select(Some(*cursor));
    f.render_stateful_widget(list, safe_area, &mut list_state);
}

fn render_results_with_message(f: &mut Frame, area: Rect, results: &[OptimizeResult], cursor: &usize, message: &Option<String>) {
    // If there's a message, split the area to show it
    if let Some(msg) = message {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(5),    // Results list
                Constraint::Length(3), // Message area (border + padding + text)
            ])
            .split(area);
        
        render_results(f, chunks[0], results, cursor);
        
        // Render message box
        let message_widget = Paragraph::new(msg.as_str())
            .style(Styles::warning())
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Styles::border())
                    .title("Note")
            )
            .alignment(ratatui::layout::Alignment::Left);
        f.render_widget(message_widget, chunks[1]);
    } else {
        render_results(f, area, results, cursor);
    }
}

fn render_results(f: &mut Frame, area: Rect, results: &[OptimizeResult], cursor: &usize) {
    // Ensure area is valid (at least 7x20 for borders, padding, and at least one item)
    if area.width < 20 || area.height < 7 {
        let msg = Paragraph::new("Window too small")
            .style(Styles::warning())
            .alignment(ratatui::layout::Alignment::Center);
        f.render_widget(msg, area);
        return;
    }
    
    // Ensure area doesn't exceed terminal bounds (safety check)
    let terminal_area = f.area();
    let safe_area = Rect {
        x: area.x,
        y: area.y,
        width: area.width.min(terminal_area.width.saturating_sub(area.x)),
        height: area.height.min(terminal_area.height.saturating_sub(area.y)),
    };
    
    // Double-check the safe area is still valid
    if safe_area.width < 20 || safe_area.height < 7 {
        let msg = Paragraph::new("Window too small")
            .style(Styles::warning())
            .alignment(ratatui::layout::Alignment::Center);
        f.render_widget(msg, area);
        return;
    }

    let items: Vec<ListItem> = results
        .iter()
        .enumerate()
        .map(|(i, result)| {
            let is_selected = i == *cursor;
            
            let icon = if result.success {
                if result.message.starts_with("Skipped:") {
                    "○"
                } else {
                    "✓"
                }
            } else {
                "✗"
            };

            let icon_style = if result.success {
                if result.message.starts_with("Skipped:") {
                    Styles::muted()
                } else {
                    Styles::success()
                }
            } else {
                Styles::error()
            };

            let message_style = if result.success {
                if result.message.starts_with("Skipped:") {
                    Styles::muted()
                } else {
                    Styles::success()
                }
            } else {
                Styles::error()
            };

            let action_style = if is_selected {
                Styles::selected()
            } else {
                Styles::emphasis()
            };

            let prefix = if is_selected { "> " } else { "  " };

            let line = Line::from(vec![
                Span::styled(prefix, action_style),
                Span::styled(icon, icon_style),
                Span::raw(" "),
                Span::styled(&result.action, action_style),
                Span::raw(" - "),
                Span::styled(&result.message, message_style),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Styles::border())
                .title("Results")
                .padding(ratatui::widgets::Padding::uniform(1)),
        );

    // Use stateful widget to support cursor navigation
    let mut list_state = ratatui::widgets::ListState::default();
    list_state.select(Some(*cursor.min(&results.len().saturating_sub(1))));
    f.render_stateful_widget(list, safe_area, &mut list_state);
}
