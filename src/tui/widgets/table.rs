//! Table widget for displaying scan results

use crate::tui::{
    state::ResultItem,
    theme::{category_style, Styles},
};
use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
    Frame,
};

/// Render the results table
#[allow(clippy::too_many_arguments)]
pub fn render_results_table(
    f: &mut Frame,
    area: Rect,
    items: &[ResultItem],
    selected: &std::collections::HashSet<usize>,
    cursor: usize,
    scroll_offset: usize,
    total_size: u64,
    total_count: usize,
) {
    use ratatui::layout::{Constraint, Direction, Layout};

    // Header with summary
    let header_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    // Summary header
    let summary = format!(
        "{} reclaimable ── {} items",
        bytesize::to_string(total_size, false),
        total_count
    );
    let header = Paragraph::new(Line::from(vec![
        Span::styled("SCAN RESULTS", Styles::title()),
        Span::raw(" ── "),
        Span::styled(summary, Styles::emphasis()),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Styles::border()),
    );
    f.render_widget(header, header_chunks[0]);

    // Table
    let table_area = header_chunks[1];

    // Calculate visible range
    let visible_rows = (table_area.height as usize).saturating_sub(2); // Account for borders
    let start_idx = scroll_offset;
    let end_idx = (start_idx + visible_rows).min(items.len());

    // Build table rows
    let mut rows = Vec::new();

    // Header row
    let header_row = Row::new(vec![
        Cell::from(""),
        Cell::from("PATH").style(Styles::header()),
        Cell::from("SIZE").style(Styles::header()),
        Cell::from("AGE").style(Styles::header()),
        Cell::from("TYPE").style(Styles::header()),
    ]);

    rows.push(header_row);

    // Data rows
    for (idx_in_slice, item) in items[start_idx..end_idx].iter().enumerate() {
        let global_idx = start_idx + idx_in_slice;
        let is_selected = selected.contains(&global_idx);
        let is_cursor = global_idx == cursor;

        // Checkbox
        let checkbox = if is_selected { "[X]" } else { "[ ]" };
        let checkbox_style = if is_selected {
            Styles::checked()
        } else {
            Styles::secondary()
        };

        // Path (truncated)
        let path_str = item.path.display().to_string();
        let max_path_len = 40;
        let path_display = if path_str.len() > max_path_len {
            format!("{}...", &path_str[..max_path_len])
        } else {
            path_str
        };

        // Size
        let size_str = bytesize::to_string(item.size_bytes, false);

        // Age
        let age_str = item
            .age_days
            .map(|d| format!("{}d", d))
            .unwrap_or_else(|| "--".to_string());

        // Type with color
        let type_style = category_style(item.safe);
        let type_span = Span::styled(item.category.clone(), type_style);

        // Row style
        let row_style = if is_cursor {
            Styles::selected()
        } else {
            Style::default()
        };

        let row = Row::new(vec![
            Cell::from(checkbox).style(checkbox_style),
            Cell::from(path_display),
            Cell::from(size_str),
            Cell::from(age_str),
            Cell::from(type_span),
        ])
        .style(row_style);

        rows.push(row);
    }

    // Show "more items" indicator if needed
    if end_idx < items.len() {
        rows.push(Row::new(vec![
            Cell::from(""),
            Cell::from(format!("... {} more items", items.len() - end_idx))
                .style(Styles::secondary()),
            Cell::from(""),
            Cell::from(""),
            Cell::from(""),
        ]));
    }

    let table = Table::new(
        rows,
        &[
            Constraint::Length(3),      // Checkbox
            Constraint::Percentage(50), // Path
            Constraint::Length(12),     // Size
            Constraint::Length(8),      // Age
            Constraint::Length(12),     // Type
        ],
    )
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Styles::border()),
    );

    f.render_widget(table, table_area);
}
