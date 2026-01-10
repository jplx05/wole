//! Reusable WOLE ASCII logo widget
//!
//! Provides consistent branding across all TUI screens with optional animation support.

use crate::tui::theme::Styles;
use ratatui::{
    layout::{Alignment, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

/// ASCII art lines for the WOLE logo
const LOGO_LINES: &[&str] = &[
    "  ██╗    ██╗ ██████╗ ██╗     ███████╗",
    "  ██║    ██║██╔═══██╗██║     ██╔════╝",
    "  ██║ █╗ ██║██║   ██║██║     █████╗  ",
    "  ██║███╗██║██║   ██║██║     ██╔══╝  ",
    "  ╚███╔███╔╝╚██████╔╝███████╗███████╗",
    "   ╚══╝╚══╝  ╚═════╝ ╚══════╝╚══════╝",
];

/// Height of the logo in lines
pub const LOGO_HEIGHT: u16 = 6;

/// Width of the logo in characters
pub const LOGO_WIDTH: u16 = 40;

/// Render the WOLE logo at the given area
/// Adds a line of spacing before the logo
pub fn render_logo(f: &mut Frame, area: Rect) {
    // Add spacing before logo by offsetting the y position
    let logo_area = Rect {
        x: area.x,
        y: area.y + 1,
        width: area.width,
        height: LOGO_HEIGHT,
    };
    render_logo_with_style(f, logo_area, Alignment::Left, false, 0)
}

/// Render the WOLE logo centered
/// Adds a line of spacing before the logo
pub fn render_logo_centered(f: &mut Frame, area: Rect) {
    // Add spacing before logo by offsetting the y position
    let logo_area = Rect {
        x: area.x,
        y: area.y + 1,
        width: area.width,
        height: LOGO_HEIGHT,
    };
    render_logo_with_style(f, logo_area, Alignment::Center, false, 0)
}

/// Render the WOLE logo with a sweep-in animation effect
///
/// `progress` is 0.0 to 1.0, where 1.0 means fully revealed
/// Adds a line of spacing before the logo
pub fn render_logo_animated(f: &mut Frame, area: Rect, progress: f32) {
    // Add spacing before logo by offsetting the y position
    let logo_area = Rect {
        x: area.x,
        y: area.y + 1,
        width: area.width,
        height: LOGO_HEIGHT,
    };

    let reveal_chars = ((LOGO_WIDTH as f32) * progress.clamp(0.0, 1.0)) as usize;

    let title_lines: Vec<Line> = LOGO_LINES
        .iter()
        .map(|line| {
            let chars: Vec<char> = line.chars().collect();
            let visible: String = chars.iter().take(reveal_chars).collect();
            Line::from(vec![Span::styled(visible, Styles::title())])
        })
        .collect();

    let title_paragraph = Paragraph::new(title_lines).alignment(Alignment::Left);
    f.render_widget(title_paragraph, logo_area);
}

/// Render the WOLE logo with custom alignment and optional animation
pub fn render_logo_with_style(
    f: &mut Frame,
    area: Rect,
    alignment: Alignment,
    animated: bool,
    tick: u64,
) {
    if animated {
        // Subtle shimmer effect on the logo
        let shimmer_offset = (tick as usize / 3) % LOGO_WIDTH as usize;

        let title_lines: Vec<Line> = LOGO_LINES
            .iter()
            .map(|line| {
                let chars: Vec<char> = line.chars().collect();
                let spans: Vec<Span> = chars
                    .iter()
                    .enumerate()
                    .map(|(i, c)| {
                        // Create a moving highlight effect
                        let distance = (i as i32 - shimmer_offset as i32).unsigned_abs();
                        if distance < 3 && *c != ' ' {
                            Span::styled(c.to_string(), Styles::emphasis())
                        } else {
                            Span::styled(c.to_string(), Styles::title())
                        }
                    })
                    .collect();
                Line::from(spans)
            })
            .collect();

        let title_paragraph = Paragraph::new(title_lines).alignment(alignment);
        f.render_widget(title_paragraph, area);
    } else {
        // Static logo
        let title_lines: Vec<Line> = LOGO_LINES
            .iter()
            .map(|line| Line::from(vec![Span::styled(*line, Styles::title())]))
            .collect();

        let title_paragraph = Paragraph::new(title_lines).alignment(alignment);
        f.render_widget(title_paragraph, area);
    }
}

/// Get the logo as a vector of styled lines (for embedding in other widgets)
pub fn get_logo_lines() -> Vec<Line<'static>> {
    LOGO_LINES
        .iter()
        .map(|line| Line::from(vec![Span::styled(*line, Styles::title())]))
        .collect()
}

/// Render the tagline below the logo
/// Should be called after render_logo, positioned directly after the logo ends
/// Aligned to match the logo's visual start (accounts for logo's leading spaces)
pub fn render_tagline(f: &mut Frame, area: Rect) {
    // Tagline is positioned directly after logo ends (no blank line)
    // Logo starts at area.y + 1 (spacing), ends at area.y + 1 + LOGO_HEIGHT
    // So tagline starts at area.y + 1 + LOGO_HEIGHT
    let tagline_y = area.y + 1 + LOGO_HEIGHT;

    // Logo has 2 leading spaces, so add 2 spaces to tagline to align visually
    // Use plain text instead of hyperlink to avoid terminal auto-detection issues
    let tagline = Paragraph::new(Line::from(vec![
        Span::styled("  Reclaim disk space on Windows", Styles::secondary()),
        Span::styled(" • ", Styles::secondary()),
        Span::styled("jpaulpoliquit/wole", Styles::secondary()),
    ]))
    .alignment(Alignment::Left);

    let tagline_area = Rect {
        x: area.x,
        y: tagline_y,
        width: area.width,
        height: 1,
    };
    f.render_widget(tagline, tagline_area);
}

/// Render the tagline centered below the logo
pub fn render_tagline_centered(f: &mut Frame, area: Rect) {
    let tagline_y = area.y + 1 + LOGO_HEIGHT;

    // Use plain text instead of hyperlink to avoid terminal auto-detection issues
    let tagline = Paragraph::new(Line::from(vec![
        Span::styled("Reclaim disk space on Windows", Styles::secondary()),
        Span::styled(" • ", Styles::secondary()),
        Span::styled("jpaulpoliquit/wole", Styles::secondary()),
    ]))
    .alignment(Alignment::Center);

    let tagline_area = Rect {
        x: area.x,
        y: tagline_y,
        width: area.width,
        height: 1,
    };
    f.render_widget(tagline, tagline_area);
}

/// Total height needed for logo + tagline + 1 blank line after tagline
pub const LOGO_WITH_TAGLINE_HEIGHT: u16 = LOGO_HEIGHT + 1 + 1 + 1; // spacing before + logo + tagline + 1 blank after tagline
