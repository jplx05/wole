//! Directory tree widget for preview screen

use crate::tui::theme::Styles;
use crate::utils;
use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use std::path::Path;
use std::path::PathBuf;

/// Render a simple directory tree view
pub fn render_tree(f: &mut Frame, area: Rect, path: &PathBuf, size_bytes: u64, base_path: &Path) {
    // For now, render a simplified tree view
    // In a full implementation, this would recursively build the tree

    let path_str = utils::to_relative_path(path, base_path);
    let size_str = bytesize::to_string(size_bytes, true);

    // Get file metadata for additional info
    let metadata = std::fs::metadata(path).ok();
    let modified = metadata
        .and_then(|m| m.modified().ok())
        .and_then(|t| {
            let duration = t.duration_since(std::time::UNIX_EPOCH).ok()?;
            chrono::DateTime::<chrono::Utc>::from_timestamp(duration.as_secs() as i64, 0)
        })
        .map(|dt: chrono::DateTime<chrono::Utc>| {
            dt.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_else(|| "Unknown".to_string());

    let lines = vec![
        Line::from(vec![
            Span::styled("Path: ", Styles::header()),
            Span::styled(path_str, Styles::primary()),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Size: ", Styles::header()),
            Span::styled(size_str, Styles::emphasis()),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Modified: ", Styles::header()),
            Span::styled(modified, Styles::secondary()),
        ]),
    ];

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Styles::border())
            .title("FILE DETAILS")
            .padding(ratatui::widgets::Padding::uniform(1)),
    );

    f.render_widget(paragraph, area);
}
