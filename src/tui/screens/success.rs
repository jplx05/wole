//! Success screen after cleanup

use crate::tui::{
    state::AppState,
    theme::Styles,
    widgets::{
        logo::{render_logo, render_tagline, LOGO_WITH_TAGLINE_HEIGHT},
        shortcuts::{get_shortcuts, render_shortcuts},
    },
};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

/// Get available disk space on the system drive (Windows: C:\, Unix: /)
fn get_free_space() -> Option<u64> {
    #[cfg(windows)]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;

        let path: Vec<u16> = OsStr::new("C:\\")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let mut free_bytes_available: u64 = 0;
        let mut total_bytes: u64 = 0;
        let mut total_free_bytes: u64 = 0;

        unsafe {
            extern "system" {
                fn GetDiskFreeSpaceExW(
                    lpDirectoryName: *const u16,
                    lpFreeBytesAvailableToCaller: *mut u64,
                    lpTotalNumberOfBytes: *mut u64,
                    lpTotalNumberOfFreeBytes: *mut u64,
                ) -> i32;
            }

            let result = GetDiskFreeSpaceExW(
                path.as_ptr(),
                &mut free_bytes_available,
                &mut total_bytes,
                &mut total_free_bytes,
            );

            if result != 0 {
                return Some(free_bytes_available);
            }
        }
        None
    }

    #[cfg(not(windows))]
    {
        None // Simplified for now, could add statvfs for Unix
    }
}

/// Generate a fun comparison for the amount of space
fn fun_comparison(bytes: u64) -> Option<String> {
    // Size references (approximate):
    // - 4K movie: ~4.5 GB (average streaming 4K)
    // - HD movie: ~1.5 GB
    // - MP3 song: ~4 MB
    // - Photo: ~5 MB
    // - eBook: ~2 MB
    // - npm package: ~50 MB average

    const MB: u64 = 1_000_000;
    const GB: u64 = 1_000_000_000;

    let game_size: u64 = 50 * GB; // ~50 GB for AAA game
    let node_modules_size: u64 = 500 * MB; // ~500 MB average node_modules
    let floppy_size: u64 = 1_440_000; // 1.44 MB floppy disk

    if bytes >= 10 * GB {
        let count = bytes / game_size;
        let gb = bytes as f64 / GB as f64;
        if count >= 1 {
            Some(format!(
                "That's like ~{} AAA game installs (~{:.1} GB) worth of space!",
                count, gb
            ))
        } else {
            Some(format!(
                "That's like a partial game install (~{:.1} GB) worth of space!",
                gb
            ))
        }
    } else if bytes >= 500 * MB {
        let count = bytes / node_modules_size;
        let gb = bytes as f64 / GB as f64;
        Some(format!(
            "That's like ~{} node_modules folders (~{:.1} GB) worth of space!",
            count, gb
        ))
    } else if bytes >= 10 * MB {
        let count = bytes / floppy_size;
        let mb = bytes as f64 / MB as f64;
        Some(format!(
            "That's like ~{} floppy disks (~{:.0} MB) worth of space!",
            count, mb
        ))
    } else if bytes >= MB {
        Some("Every megabyte counts!".to_string())
    } else {
        None
    }
}

pub fn render(f: &mut Frame, app_state: &AppState) {
    let area = f.area();

    // Layout: logo+tagline, success message, stats, actions, shortcuts
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(LOGO_WITH_TAGLINE_HEIGHT), // Logo + 2 blank lines + tagline
            Constraint::Length(6),                        // Success message
            Constraint::Min(10), // Stats (increased to accommodate failed files list)
            Constraint::Length(3), // Continue message
            Constraint::Length(3), // Shortcuts
        ])
        .split(area);

    // Logo and tagline (using reusable widgets)
    render_logo(f, chunks[0]);
    render_tagline(f, chunks[0]);

    // Get free space for display
    let free_space = get_free_space();

    // Success message with celebration
    if let crate::tui::state::Screen::Success { cleaned_bytes, .. } = app_state.screen {
        let mut success_lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("  ✓ ", Styles::success()),
                Span::styled("CLEANUP COMPLETE!", Styles::title()),
            ]),
            Line::from(""),
        ];

        // Show space freed and free space now
        if let Some(free) = free_space {
            success_lines.push(Line::from(vec![
                Span::styled("    Space freed: ", Styles::secondary()),
                Span::styled(bytesize::to_string(cleaned_bytes, true), Styles::emphasis()),
                Span::styled(" │ Free space now: ", Styles::secondary()),
                Span::styled(bytesize::to_string(free, true), Styles::emphasis()),
            ]));
        } else {
            success_lines.push(Line::from(vec![
                Span::styled("    Successfully freed ", Styles::primary()),
                Span::styled(bytesize::to_string(cleaned_bytes, true), Styles::emphasis()),
                Span::styled(" of disk space", Styles::primary()),
            ]));
        }

        let success_paragraph = Paragraph::new(success_lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Styles::success()),
        );
        f.render_widget(success_paragraph, chunks[1]);
    }

    // Stats breakdown
    if let crate::tui::state::Screen::Success {
        cleaned,
        cleaned_bytes,
        errors,
        ref failed_temp_files,
    } = app_state.screen
    {
        // Count categories that were processed
        let categories_processed = app_state.category_groups.len();

        let mut stats_lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("    Files cleaned:       ", Styles::secondary()),
                Span::styled(format!("{}", cleaned), Styles::emphasis()),
            ]),
            Line::from(vec![
                Span::styled("    Categories cleaned:  ", Styles::secondary()),
                Span::styled(format!("{}", categories_processed), Styles::emphasis()),
            ]),
            Line::from(vec![
                Span::styled("    Space freed:         ", Styles::secondary()),
                Span::styled(bytesize::to_string(cleaned_bytes, true), Styles::emphasis()),
            ]),
        ];

        // Add free space if available
        if let Some(free) = free_space {
            stats_lines.push(Line::from(vec![
                Span::styled("    Free space now:      ", Styles::secondary()),
                Span::styled(bytesize::to_string(free, true), Styles::emphasis()),
            ]));
        }

        // Add errors line
        stats_lines.push(if errors > 0 {
            Line::from(vec![
                Span::styled("    Errors:              ", Styles::secondary()),
                Span::styled(format!("{}", errors), Styles::warning()),
            ])
        } else {
            Line::from(vec![
                Span::styled("    Errors:              ", Styles::secondary()),
                Span::styled("0", Styles::success()),
            ])
        });

        // Add error explanation if there are errors
        if errors > 0 {
            stats_lines.push(Line::from(""));

            // If we have specific failed temp files, show them
            if !failed_temp_files.is_empty() {
                stats_lines.push(Line::from(vec![
                    Span::styled("    ", Styles::secondary()),
                    Span::styled("⚠ ", Styles::warning()),
                    Span::styled(
                        format!(
                            "{} temp file(s) couldn't be deleted:",
                            failed_temp_files.len()
                        ),
                        Styles::warning(),
                    ),
                ]));
                stats_lines.push(Line::from(vec![
                    Span::styled("    ", Styles::secondary()),
                    Span::styled(
                        "   They may be locked by running applications.",
                        Styles::secondary(),
                    ),
                ]));

                // Show up to 5 failed files (to avoid overwhelming the screen)
                let max_display = 5;
                let display_count = failed_temp_files.len().min(max_display);
                for failed_path in failed_temp_files.iter().take(display_count) {
                    // Convert to relative path for display
                    let display_path =
                        crate::utils::to_relative_path(failed_path, &app_state.scan_path);
                    // Truncate long paths
                    let max_path_len = 60;
                    let truncated_path = if display_path.len() > max_path_len {
                        format!(
                            "...{}",
                            &display_path[display_path.len().saturating_sub(max_path_len - 3)..]
                        )
                    } else {
                        display_path
                    };

                    stats_lines.push(Line::from(vec![
                        Span::styled("    ", Styles::secondary()),
                        Span::styled(format!("  • {}", truncated_path), Styles::secondary()),
                    ]));
                }

                if failed_temp_files.len() > max_display {
                    stats_lines.push(Line::from(vec![
                        Span::styled("    ", Styles::secondary()),
                        Span::styled(
                            format!("  ... and {} more", failed_temp_files.len() - max_display),
                            Styles::secondary(),
                        ),
                    ]));
                }

                stats_lines.push(Line::from(""));
                stats_lines.push(Line::from(vec![
                    Span::styled("    ", Styles::secondary()),
                    Span::styled(
                        "   Try closing apps and running cleanup again.",
                        Styles::secondary(),
                    ),
                ]));
            } else {
                // Check if temp files were likely involved by checking if any category group is "Temp Files"
                let has_temp_files = app_state
                    .category_groups
                    .iter()
                    .any(|group| group.name == "Temp Files");

                if has_temp_files {
                    stats_lines.push(Line::from(vec![
                        Span::styled("    ", Styles::secondary()),
                        Span::styled("⚠ ", Styles::warning()),
                        Span::styled("Some temp files couldn't be deleted.", Styles::warning()),
                    ]));
                    stats_lines.push(Line::from(vec![
                        Span::styled("    ", Styles::secondary()),
                        Span::styled(
                            "   They may be locked by running applications.",
                            Styles::secondary(),
                        ),
                    ]));
                    stats_lines.push(Line::from(vec![
                        Span::styled("    ", Styles::secondary()),
                        Span::styled(
                            "   Try closing apps and running cleanup again.",
                            Styles::secondary(),
                        ),
                    ]));
                } else {
                    stats_lines.push(Line::from(vec![
                        Span::styled("    ", Styles::secondary()),
                        Span::styled("⚠ ", Styles::warning()),
                        Span::styled("Some files couldn't be deleted.", Styles::warning()),
                    ]));
                    stats_lines.push(Line::from(vec![
                        Span::styled("    ", Styles::secondary()),
                        Span::styled(
                            "   They may be locked or in use by other processes.",
                            Styles::secondary(),
                        ),
                    ]));
                }
            }
        }

        stats_lines.push(Line::from(""));

        // Add fun comparison if applicable
        if let Some(comparison) = fun_comparison(cleaned_bytes) {
            stats_lines.push(Line::from(vec![Span::styled(
                format!("    {}", comparison),
                Styles::emphasis(),
            )]));
        } else {
            stats_lines.push(Line::from(vec![Span::styled(
                "    Your system is now cleaner and faster!",
                Styles::secondary(),
            )]));
        }

        let stats = Paragraph::new(stats_lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Styles::border())
                .title("SUMMARY"),
        );
        f.render_widget(stats, chunks[2]);
    }

    // Continue message - show navigation options
    let has_remaining_items = !app_state.all_items.is_empty();
    let message_text = if has_remaining_items {
        vec![Line::from(vec![
            Span::styled("  Press ", Styles::secondary()),
            Span::styled("[Esc] or [B]", Styles::emphasis()),
            Span::styled(
                " to return to results, or any other key for dashboard",
                Styles::secondary(),
            ),
        ])]
    } else {
        vec![Line::from(vec![Span::styled(
            "  Press any key to return to dashboard...",
            Styles::secondary(),
        )])]
    };

    let message = Paragraph::new(message_text).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Styles::border())
            .title(if has_remaining_items {
                "NAVIGATION"
            } else {
                "CONTINUE"
            }),
    );
    f.render_widget(message, chunks[3]);

    // Shortcuts
    let shortcuts = get_shortcuts(&app_state.screen, Some(app_state));
    render_shortcuts(f, chunks[4], &shortcuts);
}
