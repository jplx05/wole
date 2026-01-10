//! Config screen - show config path and how to edit

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use crate::tui::{
    state::AppState,
    theme::Styles,
    widgets::{
        logo::{render_logo, render_tagline, LOGO_WITH_TAGLINE_HEIGHT},
        shortcuts::{get_shortcuts, render_shortcuts},
    },
};

pub fn render(f: &mut Frame, app_state: &AppState) {
    let area = f.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(LOGO_WITH_TAGLINE_HEIGHT), // Logo + 2 blank lines + tagline
            Constraint::Length(3),                        // Header
            Constraint::Min(1),                           // Body
            Constraint::Length(3),                        // Shortcuts
        ])
        .split(area);

    // Logo and tagline
    render_logo(f, chunks[0]);
    render_tagline(f, chunks[0]);

    render_header(f, chunks[1]);
    render_body(f, chunks[2], app_state);

    // Shortcuts
    let shortcuts = get_shortcuts(&app_state.screen, Some(app_state));
    render_shortcuts(f, chunks[3], &shortcuts);
}

fn render_header(f: &mut Frame, area: Rect) {
    let header = Paragraph::new(Line::from(vec![
        Span::styled("Configuration", Styles::title()),
        Span::styled("  (edit the TOML file)", Styles::secondary()),
    ]))
    .block(
        Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Styles::border())
            .padding(ratatui::widgets::Padding::new(1, 1, 0, 0)),
    );

    f.render_widget(header, area);
}

fn render_body(f: &mut Frame, area: Rect, app_state: &AppState) {
    let config = &app_state.config;

    let config_path_buf = crate::config::Config::config_path().ok();
    let config_path = config_path_buf
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<could not determine config path>".to_string());
    let config_exists = config_path_buf.as_ref().is_some_and(|p| p.exists());
    let config_dir_exists = config_path_buf
        .as_ref()
        .and_then(|p| p.parent())
        .is_some_and(|p| p.exists());

    let selected = app_state.config_editor.selected;
    let (editing, edit_buffer) = match &app_state.config_editor.mode {
        crate::tui::state::ConfigEditorMode::View => (false, None),
        crate::tui::state::ConfigEditorMode::Editing { buffer } => (true, Some(buffer.as_str())),
    };

    let mut field_lines: Vec<Line> = Vec::new();
    let field_style = |idx: usize| -> Style {
        if idx == selected {
            Styles::selected()
        } else {
            Styles::primary()
        }
    };

    // 0 project_age_days
    field_lines.push(Line::from(vec![
        Span::styled("  Project age days: ", Styles::secondary()),
        Span::styled(
            if editing && selected == 0 {
                edit_buffer.unwrap_or("").to_string()
            } else {
                format!("{}", config.thresholds.project_age_days)
            },
            field_style(0),
        ),
    ]));
    // 1 min_age_days
    field_lines.push(Line::from(vec![
        Span::styled("  Min age days:     ", Styles::secondary()),
        Span::styled(
            if editing && selected == 1 {
                edit_buffer.unwrap_or("").to_string()
            } else {
                format!("{}", config.thresholds.min_age_days)
            },
            field_style(1),
        ),
    ]));
    // 2 min_size_mb
    field_lines.push(Line::from(vec![
        Span::styled("  Min size (MB):    ", Styles::secondary()),
        Span::styled(
            if editing && selected == 2 {
                edit_buffer.unwrap_or("").to_string()
            } else {
                format!("{}", config.thresholds.min_size_mb)
            },
            field_style(2),
        ),
    ]));

    // 3 default_scan_path
    let current_scan_path = config
        .ui
        .default_scan_path
        .as_deref()
        .unwrap_or("(auto-detect)");
    field_lines.push(Line::from(vec![Span::styled(
        "  Default scan path:",
        Styles::secondary(),
    )]));
    field_lines.push(Line::from(vec![
        Span::styled("    ", Styles::secondary()),
        Span::styled(
            if editing && selected == 3 {
                let b = edit_buffer.unwrap_or("");
                if b.is_empty() {
                    "(auto-detect)".to_string()
                } else {
                    b.to_string()
                }
            } else {
                current_scan_path.to_string()
            },
            field_style(3),
        ),
    ]));

    // 4 animations
    field_lines.push(Line::from(vec![
        Span::styled("  Animations:       ", Styles::secondary()),
        Span::styled(format!("{}", config.ui.animations), field_style(4)),
        Span::styled("   (Space/Enter toggles)", Styles::secondary()),
    ]));

    // 5 refresh_rate_ms
    field_lines.push(Line::from(vec![
        Span::styled("  Refresh (ms):     ", Styles::secondary()),
        Span::styled(
            if editing && selected == 5 {
                edit_buffer.unwrap_or("").to_string()
            } else {
                format!("{}", config.ui.refresh_rate_ms)
            },
            field_style(5),
        ),
    ]));

    // 6 show_storage_info
    field_lines.push(Line::from(vec![
        Span::styled("  Show storage info:", Styles::secondary()),
        Span::styled(format!("{}", config.ui.show_storage_info), field_style(6)),
        Span::styled("   (Space/Enter toggles)", Styles::secondary()),
    ]));

    // 7 scan_depth_user
    field_lines.push(Line::from(vec![
        Span::styled("  Scan depth (user):  ", Styles::secondary()),
        Span::styled(
            if editing && selected == 7 {
                edit_buffer.unwrap_or("").to_string()
            } else {
                format!("{}", config.ui.scan_depth_user)
            },
            field_style(7),
        ),
    ]));

    // 8 scan_depth_entire_disk
    field_lines.push(Line::from(vec![
        Span::styled("  Scan depth (disk):  ", Styles::secondary()),
        Span::styled(
            if editing && selected == 8 {
                edit_buffer.unwrap_or("").to_string()
            } else {
                format!("{}", config.ui.scan_depth_entire_disk)
            },
            field_style(8),
        ),
    ]));

    let text = Text::from(vec![
        Line::from(vec![Span::styled("Config file:", Styles::header())]),
        Line::from(vec![Span::styled(
            format!("  {}", config_path),
            Styles::primary(),
        )]),
        Line::from(vec![
            Span::styled("  Exists: ", Styles::secondary()),
            Span::styled(
                if config_exists {
                    "yes"
                } else {
                    "no (will be created on open)"
                },
                if config_exists {
                    Styles::primary()
                } else {
                    Styles::warning()
                },
            ),
        ]),
        Line::from(vec![
            Span::styled("  Folder exists: ", Styles::secondary()),
            Span::styled(
                if config_dir_exists { "yes" } else { "no" },
                if config_dir_exists {
                    Styles::primary()
                } else {
                    Styles::warning()
                },
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Editable settings:", Styles::header()),
            Span::styled("  (↑↓ select, Enter edit)", Styles::secondary()),
        ]),
        // Insert fields here
        Line::from(""),
        Line::from(""),
        // Placeholder for fields; we append below.
        Line::from(vec![Span::styled("Tips:", Styles::header())]),
        Line::from(vec![
            Span::styled("  - Press ", Styles::secondary()),
            Span::styled("Enter", Styles::emphasis()),
            Span::styled(" to edit/toggle the selected field.", Styles::secondary()),
        ]),
        Line::from(vec![
            Span::styled("  - Press ", Styles::secondary()),
            Span::styled("Esc", Styles::emphasis()),
            Span::styled(" to go back.", Styles::secondary()),
        ]),
        Line::from(vec![
            Span::styled("  - Press ", Styles::secondary()),
            Span::styled("S", Styles::emphasis()),
            Span::styled(" to save, ", Styles::secondary()),
            Span::styled("R", Styles::emphasis()),
            Span::styled(" to reload, ", Styles::secondary()),
            Span::styled("O", Styles::emphasis()),
            Span::styled(" to open the config file.", Styles::secondary()),
        ]),
    ]);

    // Build final text by splicing in field_lines + message.
    let mut combined: Vec<Line> = Vec::new();
    for line in text.lines.iter().take(10) {
        combined.push(line.clone());
    }
    // Add the editable fields block
    combined.push(Line::from(""));
    combined.extend(field_lines);
    combined.push(Line::from(""));

    if let Some(msg) = &app_state.config_editor.message {
        combined.push(Line::from(vec![
            Span::styled("Status: ", Styles::secondary()),
            Span::styled(msg.as_str(), Styles::primary()),
        ]));
        combined.push(Line::from(""));
    }

    // Append tips (last 6 lines from the original text vector)
    let tail_start = text.lines.len().saturating_sub(6);
    for line in text.lines.iter().skip(tail_start) {
        combined.push(line.clone());
    }

    let body_text = Text::from(combined);

    let body = Paragraph::new(body_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Styles::border())
                .title("Config")
                .padding(ratatui::widgets::Padding::uniform(1)),
        )
        .wrap(Wrap { trim: true });

    f.render_widget(body, area);
}
