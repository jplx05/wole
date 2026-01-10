//! Progress bar widgets

use crate::tui::theme::Styles;
use bytesize;
use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

/// Render a progress bar
pub fn render_progress_bar(
    f: &mut Frame,
    area: Rect,
    label: &str,
    progress: f32,
    size: Option<u64>,
    status: &str,
    tick: u64,
) {
    use ratatui::layout::{Constraint, Direction, Layout};

    // Ensure minimum width for proper rendering
    if area.width < 20 {
        // If area is too small, just render label and status
        let text = format!(
            "{}: {} {}",
            label,
            status,
            size.map(|s| bytesize::to_string(s, true))
                .unwrap_or_else(|| "---".to_string())
        );
        let paragraph = Paragraph::new(text).style(Styles::primary());
        f.render_widget(paragraph, area);
        return;
    }

    // Split area into spinner, label, gauge, and status
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(2),  // Spinner (fixed width)
            Constraint::Length(15), // Label (fixed width)
            Constraint::Min(10),    // Gauge (flexible, minimum 10)
            Constraint::Length(15), // Status (fixed width, reduced since no %)
        ])
        .split(area);

    // Render spinner before label
    use crate::spinner;
    let spinner_char = spinner::get_spinner(tick);
    let spinner_paragraph = Paragraph::new(spinner_char).style(Styles::emphasis());
    f.render_widget(spinner_paragraph, chunks[0]);

    // Render label
    let label_text = if label.len() > 13 {
        format!("{}...", &label[..13])
    } else {
        format!("{:13}", label)
    };
    let label_paragraph = Paragraph::new(label_text).style(Styles::emphasis());
    f.render_widget(label_paragraph, chunks[1]);

    // Custom Animated Bar Drawing
    let gauge_area = chunks[2];
    let width = gauge_area.width as usize;

    if width > 0 {
        let filled_width = ((width as f32 * progress).round() as usize).min(width);

        let mut bar_spans = Vec::new();

        // Color depends on state
        let color = if progress >= 1.0 {
            Styles::success()
        } else if progress > 0.0 {
            Styles::emphasis()
        } else {
            Styles::secondary()
        };

        // Draw filled part
        let filled_char = "█";
        if filled_width > 0 {
            let filled_str = filled_char.repeat(filled_width);
            bar_spans.push(Span::styled(filled_str, color));
        }

        // Draw Animated Head (if active and not full)
        if progress < 1.0 && progress > 0.0 && filled_width < width {
            // Pulse animation for the head
            let frames = ["▓", "▒", "░"];
            let idx = (tick as usize / 2) % frames.len();
            bar_spans.push(Span::styled(frames[idx], color));
        } else if filled_width < width {
            // Static filler for empty space immediately after filled part if we didn't draw a head
            // (Only happens if we decided not to draw head, but logic above covers it)
        }

        // Draw empty part (solid, no gradient)
        let empty_width = width.saturating_sub(
            filled_width
                + if progress < 1.0 && progress > 0.0 {
                    1
                } else {
                    0
                },
        );
        if empty_width > 0 {
            let empty_char = " ";
            let empty_str = empty_char.repeat(empty_width);
            bar_spans.push(Span::styled(empty_str, Styles::secondary()));
        }

        // Overlay percentage text
        // (Ratatui doesn't easily support layering text over other widgets without canvas,
        // so we'll just put it in the status or rely on the visual bar.
        // Or we can construct a single string if we weren't doing colors.
        // For now, let's append the % at the end of the bar area if space allows, or overlay it?)

        // Actually, let's keep it simple: The bar is the visual indicator.
        // We'll put the exact % in the status area or overlay it if we manually check bounds.
        // For a clean look, let's just render the bar spans.

        // Note: Render percentage in the "Status" area if current status is brief?
        // Or render it right on top? To render text on top of bar characters we need a custom widget impl.
        // Let's stick to placing the bar. We can add percentage to the status string passed in.

        let bar_line = Line::from(bar_spans);
        let bar_block = Block::default().borders(Borders::NONE);
        let bar_widget = Paragraph::new(bar_line).block(bar_block);
        f.render_widget(bar_widget, gauge_area);
    }

    // Render size and status on the right side (no percentage)
    let size_text = if let Some(size_bytes) = size {
        bytesize::to_string(size_bytes, true)
    } else {
        "---".to_string()
    };

    // Status without percentage
    let display_status = status.to_string();

    let status_text = format!("{:>8} {}", size_text, display_status);
    let status_line = Line::from(vec![Span::styled(status_text, Styles::secondary())]);

    let status_paragraph = Paragraph::new(status_line);
    f.render_widget(status_paragraph, chunks[3]);
}

/// Render multiple category progress bars
pub fn render_category_progress(
    f: &mut Frame,
    area: Rect,
    categories: &[crate::tui::state::CategoryProgress],
    tick: u64,
) {
    use ratatui::layout::{Constraint, Direction, Layout};

    if categories.is_empty() {
        // Show empty state with helpful message
        let empty_lines = vec![
            ratatui::text::Line::from(vec![ratatui::text::Span::styled(
                "No categories selected",
                Styles::secondary(),
            )]),
            ratatui::text::Line::from(""),
            ratatui::text::Line::from(vec![ratatui::text::Span::styled(
                "   Go back to select categories first.",
                Styles::secondary(),
            )]),
        ];
        let empty_msg = Paragraph::new(empty_lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Styles::border())
                .padding(ratatui::widgets::Padding::uniform(1)),
        );
        f.render_widget(empty_msg, area);
        return;
    }

    // Ensure we have enough height - each progress bar needs at least 1 line
    let num_categories = categories.len();
    let available_height = area.height;

    if available_height < num_categories as u16 {
        // Not enough space - show a scrollable message
        let msg = Paragraph::new(format!(
            "Showing {} categories (need {} lines)",
            available_height, num_categories
        ))
        .style(Styles::secondary());
        f.render_widget(msg, area);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            (0..num_categories)
                .map(|_| Constraint::Length(1)) // One line per category
                .collect::<Vec<_>>(),
        )
        .split(area);

    for (i, cat) in categories.iter().enumerate() {
        if let Some(chunk) = chunks.get(i) {
            // Ensure chunk has minimum width
            if chunk.width < 10 {
                continue;
            }

            // Just show spinner and category name
            use crate::spinner;
            let spinner_char = spinner::get_spinner(tick);
            let _text = format!("{}  {}", spinner_char, cat.name);
            let line = Line::from(vec![
                Span::styled(spinner_char, Styles::emphasis()),
                Span::raw("  "),
                Span::styled(&cat.name, Styles::primary()),
            ]);
            let paragraph = Paragraph::new(line);
            f.render_widget(paragraph, *chunk);
        }
    }
}
