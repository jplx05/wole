//! Reusable WOLE ASCII logo widget
//!
//! Provides consistent branding across all TUI screens with optional animation support.

use crate::tui::theme::Styles;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
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
/// Logo is positioned in a 2-column layout (left: logo, right: git/tagline)
pub fn render_logo(f: &mut Frame, area: Rect) {
    // Add spacing before logo by offsetting the y position
    let logo_area = Rect {
        x: area.x,
        y: area.y + 1,
        width: area.width,
        height: LOGO_HEIGHT,
    };
    
    // Create 2-column layout: left column for logo, right column for git/tagline
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(LOGO_WIDTH), // Left column for logo (fixed width)
            Constraint::Length(1),  // Minimal spacing between columns
            Constraint::Length(50), // Right column for git/tagline (enough for full tagline)
        ])
        .split(logo_area);
    
    // Render logo in the left column
    render_logo_with_style(f, columns[0], Alignment::Left, false, 0)
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

/// Render the tagline and git link to the right of the logo
/// Git link is aligned with the top of the logo, tagline is below it
/// Uses 2-column layout: left column for logo, right column for git/tagline
pub fn render_tagline(f: &mut Frame, area: Rect) {
    // Logo starts at area.y + 1 (spacing)
    let logo_start_y = area.y + 1;
    let logo_area = Rect {
        x: area.x,
        y: logo_start_y,
        width: area.width,
        height: LOGO_HEIGHT,
    };
    
    // Create 2-column layout: left column for logo, right column for git/tagline
    // Must match the layout in render_logo() exactly
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(LOGO_WIDTH), // Left column for logo (fixed width)
            Constraint::Length(1),  // Minimal spacing between columns
            Constraint::Length(50), // Right column for git/tagline (enough for full tagline)
        ])
        .split(logo_area);
    
    // Git link is in the right column, aligned with the top of the logo
    let git_link = Paragraph::new(Line::from(vec![
        Span::styled("github.com/jplx05/wole", Styles::secondary()),
    ]))
    .alignment(Alignment::Left);

    let git_area = Rect {
        x: columns[2].x,
        y: columns[2].y,
        width: columns[2].width,
        height: 1,
    };
    f.render_widget(git_link, git_area);

    // Tagline is below the git link with a line space, also in the right column
    let tagline = Paragraph::new(Line::from(vec![
        Span::styled("Deep clean and optimize your Windows PC", Styles::secondary()),
    ]))
    .alignment(Alignment::Left);

    let tagline_area = Rect {
        x: columns[2].x,
        y: columns[2].y + 2, // Add extra line space after GitHub line
        width: columns[2].width,
        height: 1,
    };
    f.render_widget(tagline, tagline_area);
}

/// Render the tagline centered below the logo (legacy function for backward compatibility)
pub fn render_tagline_centered(f: &mut Frame, area: Rect) {
    // For centered mode, use the same left-side layout
    render_tagline(f, area);
}

/// Total height needed for logo + tagline + 1 blank line after tagline
pub const LOGO_WITH_TAGLINE_HEIGHT: u16 = LOGO_HEIGHT + 1 + 1 + 1; // spacing before + logo + tagline + 1 blank after tagline
