//! Disk Insights screen - interactive folder navigation

use crate::disk_usage::{find_folder_by_path, SortBy};
use crate::tui::{
    state::AppState,
    theme::Styles,
    widgets::{
        logo::{render_logo, render_tagline, LOGO_WITH_TAGLINE_HEIGHT},
        shortcuts::{get_shortcuts, render_shortcuts},
    },
};
use bytesize::to_string as bytesize_to_string;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

pub fn render(f: &mut Frame, app_state: &mut AppState) {
    let area = f.area();

    // Extract values we need to avoid borrowing issues
    let (insights_clone, current_path_clone, cursor, sort_by) =
        if let crate::tui::state::Screen::DiskInsights {
            ref insights,
            ref current_path,
            cursor,
            sort_by,
        } = app_state.screen
        {
            (insights.clone(), current_path.clone(), cursor, sort_by)
        } else {
            return;
        };

    let shortcuts_height = 3;

    // Layout: logo, header, search bar, content, shortcuts
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(LOGO_WITH_TAGLINE_HEIGHT), // Logo + tagline
            Constraint::Length(3),                        // Header
            Constraint::Length(3),                        // Search bar
            Constraint::Min(1),                           // Content
            Constraint::Length(shortcuts_height),
        ])
        .split(area);

    // Render logo and tagline
    render_logo(f, chunks[0]);
    render_tagline(f, chunks[0]);

    // Render header
    render_header(f, chunks[1], &insights_clone, &current_path_clone);

    // Render search bar
    render_search_bar(f, chunks[2], app_state);

    // Render content
    render_content(
        f,
        chunks[3],
        &insights_clone,
        &current_path_clone,
        cursor,
        sort_by,
        app_state,
    );

    // Render shortcuts
    let shortcuts = get_shortcuts(&app_state.screen, Some(app_state));
    render_shortcuts(f, chunks[4], &shortcuts);
}

fn render_header(
    f: &mut Frame,
    area: Rect,
    insights: &crate::disk_usage::DiskInsights,
    current_path: &std::path::Path,
) {
    // Build breadcrumb path
    let root_path = &insights.root.path;
    let mut breadcrumb_parts = Vec::new();

    if current_path != root_path {
        // Get relative path from root
        if let Ok(relative) = current_path.strip_prefix(root_path) {
            for component in relative.components() {
                if let std::path::Component::Normal(name) = component {
                    breadcrumb_parts.push(name.to_string_lossy().to_string());
                }
            }
        }
    }

    let breadcrumb_str = if breadcrumb_parts.is_empty() {
        root_path.display().to_string()
    } else {
        format!("{} > {}", root_path.display(), breadcrumb_parts.join(" > "))
    };

    // Find current folder node
    let current_node = find_folder_by_path(&insights.root, current_path).unwrap_or(&insights.root);

    let header_text = format!(
        "{}  |  Total: {}  |  {} files",
        breadcrumb_str,
        bytesize_to_string(current_node.size, true),
        format_number(current_node.file_count)
    );

    let header = Paragraph::new(Line::from(vec![
        Span::styled("Disk Insights", Styles::header()),
        Span::raw("  "),
        Span::styled(&header_text, Styles::secondary()),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Styles::border()),
    );

    f.render_widget(header, area);
}

fn render_search_bar(f: &mut Frame, area: Rect, app_state: &AppState) {
    let search_text = if app_state.search_mode {
        format!("/ {}_", app_state.search_query) // Cursor indicator
    } else if app_state.search_query.is_empty() {
        "Press / to filter folders...".to_string()
    } else {
        format!("Filter: {} (Esc to clear)", app_state.search_query)
    };

    let style = if app_state.search_mode {
        Styles::emphasis()
    } else {
        Styles::secondary()
    };

    let paragraph = Paragraph::new(search_text).style(style).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Styles::border())
            .title("SEARCH"),
    );

    f.render_widget(paragraph, area);
}

fn render_content(
    f: &mut Frame,
    area: Rect,
    insights: &crate::disk_usage::DiskInsights,
    current_path: &std::path::Path,
    cursor: usize,
    sort_by: SortBy,
    app_state: &AppState,
) {
    // Find current folder node
    let current_node = find_folder_by_path(&insights.root, current_path).unwrap_or(&insights.root);

    // Get children folders and files
    let mut children = current_node.children.clone();
    let mut files = current_node.files.clone();

    // Filter by search query if active
    if !app_state.search_query.is_empty() {
        let query = app_state.search_query.to_lowercase();
        children.retain(|child| child.name.to_lowercase().contains(&query));
        files.retain(|file| file.name.to_lowercase().contains(&query));
    }

    // Sort children folders
    match sort_by {
        SortBy::Size => children.sort_by(|a, b| b.size.cmp(&a.size)),
        SortBy::Name => children.sort_by(|a, b| a.name.cmp(&b.name)),
        SortBy::Files => children.sort_by(|a, b| b.file_count.cmp(&a.file_count)),
    }

    // Sort files
    match sort_by {
        SortBy::Size => files.sort_by(|a, b| b.size.cmp(&a.size)),
        SortBy::Name => files.sort_by(|a, b| a.name.cmp(&b.name)),
        SortBy::Files => {
            // For files, Files sort doesn't make sense, so use size
            files.sort_by(|a, b| b.size.cmp(&a.size));
        }
    }

    // Combine folders and files into a single list for display
    // Folders come first, then files
    let total_items = children.len() + files.len();

    // Clamp cursor to valid range
    let cursor = cursor.min(total_items.saturating_sub(1));

    // Calculate max size for relative percentage calculation
    let max_size = children
        .iter()
        .map(|c| c.size)
        .chain(files.iter().map(|f| f.size))
        .max()
        .unwrap_or(current_node.size)
        .max(1);

    // Build list items - folders first, then files
    let mut items: Vec<ListItem> = Vec::new();

    // Add folders
    for (i, child) in children.iter().enumerate() {
        let is_selected = i == cursor;
        let style = if is_selected {
            Styles::selected()
        } else {
            Style::default()
        };

        let relative_pct = if max_size > 0 {
            (child.size as f64 / max_size as f64) * 100.0
        } else {
            0.0
        };

        let bar_width: usize = 20;
        let filled = (relative_pct / 100.0 * bar_width as f64).round() as usize;
        let filled = if child.size > 0 && filled == 0 {
            1
        } else {
            filled.min(bar_width)
        };
        let empty = bar_width.saturating_sub(filled);
        let bar_filled = "█".repeat(filled);
        let bar_empty = "░".repeat(empty);

        let prefix = if is_selected { "> " } else { "  " };
        let num_str = (i + 1).to_string();
        let size_str = bytesize_to_string(child.size, true);
        let files_str = format!("({} files)", format_number(child.file_count));
        let pct_str = format!("{:.1}%", child.percentage);

        let line = Line::from(vec![
            Span::styled(prefix.to_string(), style),
            Span::styled(num_str, style),
            Span::raw(" "),
            Span::styled(
                bar_filled,
                if is_selected {
                    Styles::selected()
                } else {
                    Styles::emphasis()
                },
            ),
            Span::styled(bar_empty, Styles::secondary()),
            Span::raw("  "),
            Span::styled(pct_str, Styles::emphasis()),
            Span::raw("  "),
            Span::styled(size_str, Styles::emphasis()),
            Span::raw("  "),
            Span::styled(child.name.clone(), style),
            Span::raw("  "),
            Span::styled(files_str, Styles::secondary()),
        ]);

        items.push(ListItem::new(line));
    }

    // Add files
    for (i, file) in files.iter().enumerate() {
        let item_index = children.len() + i;
        let is_selected = item_index == cursor;
        let style = if is_selected {
            Styles::selected()
        } else {
            Style::default()
        };

        let relative_pct = if max_size > 0 {
            (file.size as f64 / max_size as f64) * 100.0
        } else {
            0.0
        };

        let bar_width: usize = 20;
        let filled = (relative_pct / 100.0 * bar_width as f64).round() as usize;
        let filled = if file.size > 0 && filled == 0 {
            1
        } else {
            filled.min(bar_width)
        };
        let empty = bar_width.saturating_sub(filled);
        let bar_filled = "█".repeat(filled);
        let bar_empty = "░".repeat(empty);

        let prefix = if is_selected { "> " } else { "  " };
        let num_str = (item_index + 1).to_string();
        let size_str = bytesize_to_string(file.size, true);
        let pct_str = if current_node.size > 0 {
            format!(
                "{:.1}%",
                (file.size as f64 / current_node.size as f64) * 100.0
            )
        } else {
            "0.0%".to_string()
        };

        let line = Line::from(vec![
            Span::styled(prefix.to_string(), style),
            Span::styled(num_str, style),
            Span::raw(" "),
            Span::styled(
                bar_filled,
                if is_selected {
                    Styles::selected()
                } else {
                    Styles::emphasis()
                },
            ),
            Span::styled(bar_empty, Styles::secondary()),
            Span::raw("  "),
            Span::styled(pct_str, Styles::emphasis()),
            Span::raw("  "),
            Span::styled(size_str, Styles::emphasis()),
            Span::raw("  "),
            Span::styled(file.name.clone(), style),
            Span::raw("  "),
            Span::styled("(file)".to_string(), Styles::secondary()),
        ]);

        items.push(ListItem::new(line));
    }

    // Determine title based on content
    let title = if !children.is_empty() && !files.is_empty() {
        "Folders & Files"
    } else if !children.is_empty() {
        "Folders"
    } else if !files.is_empty() {
        "Files"
    } else {
        "Empty"
    };

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Styles::border())
            .title(title),
    );

    let mut list_state = ratatui::widgets::ListState::default();
    list_state.select(Some(cursor));

    f.render_stateful_widget(list, area, &mut list_state);
}

fn format_number(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}
