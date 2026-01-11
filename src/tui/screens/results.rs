//! Results screen with grouped categories

use crate::tui::{
    state::AppState,
    theme::{category_style, Styles},
    widgets::{
        logo::{render_logo, render_tagline, LOGO_WITH_TAGLINE_HEIGHT},
        shortcuts::{get_shortcuts, render_shortcuts},
    },
};
use crate::utils::{detect_file_type, FileType};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
    Frame,
};
use std::time::SystemTime;

fn truncate_end(s: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let len = s.chars().count();
    if len <= max_chars {
        return s.to_string();
    }
    if max_chars <= 3 {
        return "...".chars().take(max_chars).collect();
    }
    let take = max_chars.saturating_sub(3);
    format!("{}...", s.chars().take(take).collect::<String>())
}

/// Get emoji for a category name
fn category_emoji(category_name: &str) -> &'static str {
    match category_name {
        "Installed Applications" => "📱",
        "Old Files" => "📅",
        "Downloads" => "⬇️",
        "Large Files" => "📦",
        "Package Cache" => "📚",
        "Application Cache" => "💾",
        "Temp Files" => "🗑️",
        "Trash" => "🗑️",
        "Build Artifacts" => "🔨",
        "Browser Cache" => "🌐",
        "System Cache" => "⚙️",
        "Empty Folders" => "📁",
        "Duplicates" => "📋",
        "Windows Update" => "🔄",
        "Event Logs" => "📋",
        _ => "📁", // Default folder emoji
    }
}

/// Get emoji for a folder based on dominant file type in its items
fn folder_emoji(app_state: &AppState, folder: &crate::tui::state::FolderGroup) -> &'static str {
    use std::collections::HashMap;
    
    // Count file types in this folder
    let mut type_counts: HashMap<FileType, usize> = HashMap::new();
    
    for &item_idx in &folder.items {
        if let Some(item) = app_state.all_items.get(item_idx) {
            let file_type = detect_file_type(&item.path);
            *type_counts.entry(file_type).or_insert(0) += 1;
        }
    }
    
    // Find the dominant file type
    if let Some((dominant_type, _)) = type_counts.iter().max_by_key(|(_, &count)| count) {
        dominant_type.emoji()
    } else {
        "📁" // Default folder emoji if no items
    }
}

fn format_ago(t: Option<SystemTime>) -> String {
    let Some(t) = t else {
        return "--".to_string();
    };
    let Ok(elapsed) = t.elapsed() else {
        return "--".to_string();
    };
    let secs = elapsed.as_secs();
    // Always show at least "1m ago" for recent/active apps (no seconds)
    if secs < 60 {
        "1m ago".to_string()
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86400 {
        format!("{}h ago", secs / 3600)
    } else if secs < 86400 * 14 {
        format!("{}d ago", secs / 86400)
    } else if secs < 86400 * 365 {
        format!("{}w ago", secs / (86400 * 7))
    } else {
        format!("{}y ago", secs / (86400 * 365))
    }
}

/// Disk space information
#[derive(Debug, Clone, Copy)]
struct DiskSpace {
    free_bytes: u64,
    total_bytes: u64,
}

/// Get disk space information on the system drive
fn get_disk_space() -> Option<DiskSpace> {
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
                return Some(DiskSpace {
                    free_bytes: free_bytes_available,
                    total_bytes,
                });
            }
        }
        None
    }

    #[cfg(not(windows))]
    {
        None
    }
}

/// Generate a fun comparison for the amount of space
fn fun_comparison(bytes: u64) -> Option<String> {
    const MB: u64 = 1_000_000;
    const GB: u64 = 1_000_000_000;

    let game_size: u64 = 50 * GB; // ~50 GB for AAA game
    let node_modules_size: u64 = 500 * MB; // ~500 MB average node_modules
    let floppy_size: u64 = 1_440_000; // 1.44 MB floppy disk

    if bytes >= 10 * GB {
        let count = bytes / game_size;
        let gb = bytes as f64 / GB as f64;
        if count >= 1 {
            Some(format!("~{} AAA game installs (~{:.1} GB)", count, gb))
        } else {
            Some(format!("a partial game install (~{:.1} GB)", gb))
        }
    } else if bytes >= 500 * MB {
        let count = bytes / node_modules_size;
        let gb = bytes as f64 / GB as f64;
        Some(format!("~{} node_modules folders (~{:.1} GB)", count, gb))
    } else if bytes >= 10 * MB {
        let count = bytes / floppy_size;
        let mb = bytes as f64 / MB as f64;
        Some(format!("~{} floppy disks (~{:.0} MB)", count, mb))
    } else {
        None
    }
}

pub fn render(f: &mut Frame, app_state: &mut AppState) {
    let area = f.area();

    // Layout: logo+tagline, summary, search bar (always visible), grouped results, shortcuts
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(LOGO_WITH_TAGLINE_HEIGHT), // Logo + 2 blank lines + tagline
            Constraint::Length(5),                        // Summary
            Constraint::Length(3),                        // Search bar (always visible)
            Constraint::Min(10),                          // Grouped results
            Constraint::Length(3),                        // Shortcuts
        ])
        .split(area);

    // Logo and tagline (using reusable widgets)
    render_logo(f, chunks[0]);
    render_tagline(f, chunks[0]);

    // Summary
    let total_size = app_state.selected_size();
    let total_items = app_state.all_items.len();
    let selected_count = app_state.selected_count();
    let categories_count = app_state.category_groups.len();
    let disk_space = get_disk_space();
    let show_storage_info = app_state.config.ui.show_storage_info;

    let mut summary_lines = vec![Line::from(vec![
        Span::styled("  Found: ", Styles::secondary()),
        Span::styled(format!("{} items", total_items), Styles::emphasis()),
        Span::styled(" │ ", Styles::secondary()),
        Span::styled("Selected: ", Styles::secondary()),
        Span::styled(format!("{}", selected_count), Styles::checked()),
        Span::styled(" │ ", Styles::secondary()),
        Span::styled("Reclaimable: ", Styles::secondary()),
        Span::styled(bytesize::to_string(total_size, true), Styles::emphasis()),
        Span::styled(" │ ", Styles::secondary()),
        Span::styled("Categories: ", Styles::secondary()),
        Span::styled(format!("{}", categories_count), Styles::emphasis()),
    ])];

    // Second line: storage info or free space and fun comparison
    let mut line2_spans = vec![Span::styled("  ", Styles::secondary())];

    if let Some(disk) = disk_space {
        if show_storage_info {
            // Show current storage (used) and storage after deletion (used after reclaiming space)
            let current_storage = disk.total_bytes - disk.free_bytes;
            let storage_after = current_storage.saturating_sub(total_size);

            line2_spans.push(Span::styled("Current storage: ", Styles::secondary()));
            line2_spans.push(Span::styled(
                bytesize::to_string(current_storage, true),
                Styles::emphasis(),
            ));
            line2_spans.push(Span::styled(" │ ", Styles::secondary()));
            line2_spans.push(Span::styled("Storage after: ", Styles::secondary()));
            line2_spans.push(Span::styled(
                bytesize::to_string(storage_after, true),
                Styles::emphasis(),
            ));
        } else {
            // Show free space (original behavior)
            line2_spans.push(Span::styled("Free space: ", Styles::secondary()));
            line2_spans.push(Span::styled(
                bytesize::to_string(disk.free_bytes, true),
                Styles::emphasis(),
            ));
        }
    }

    if let Some(comparison) = fun_comparison(total_size) {
        if disk_space.is_some() {
            line2_spans.push(Span::styled(" │ ", Styles::secondary()));
        }
        line2_spans.push(Span::styled(
            format!("That's like {} worth of space!", comparison),
            Styles::secondary(),
        ));
    }

    summary_lines.push(Line::from(line2_spans));
    summary_lines.push(Line::from(""));
    summary_lines.push(Line::from(vec![
        Span::styled("  Press ", Styles::secondary()),
        Span::styled("[C]", Styles::emphasis()),
        Span::styled(" to clean selected items", Styles::secondary()),
    ]));

    let summary = Paragraph::new(summary_lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Styles::border())
            .title("SCAN RESULTS"),
    );
    f.render_widget(summary, chunks[1]);

    // Search bar (always visible)
    render_search_bar(f, chunks[2], app_state);

    // Grouped results
    render_grouped_results(f, chunks[3], app_state);

    // Shortcuts - always in chunks[4]
    let shortcuts = get_shortcuts(&app_state.screen, Some(app_state));
    render_shortcuts(f, chunks[4], &shortcuts);
}

fn render_search_bar(f: &mut Frame, area: Rect, app_state: &AppState) {
    // Parse query to detect type filter (for display purposes)
    let (type_filter, extension_filter, text_query) = {
        let query = app_state.search_query.trim();
        if query.is_empty() {
            (None, None, String::new())
        } else if let Some(type_part) = query.strip_prefix("/type:") {
            let type_part = type_part.trim();
            let (type_str, text) = if let Some(space_idx) = type_part.find(' ') {
                let type_str = type_part[..space_idx].trim().to_lowercase();
                let text = type_part[space_idx..].trim().to_string();
                (type_str, text)
            } else {
                (type_part.to_lowercase(), String::new())
            };
            
            // Check if it's an extension
            let is_extension = type_str.starts_with('.') || 
                (type_str.len() <= 5 && !type_str.contains(' ') && 
                 !matches!(type_str.as_str(), "video" | "audio" | "image" | "code" | "text" | "document" | "archive" | "installer" | "database" | "backup" | "font" | "log" | "certificate" | "system" | "build" | "subtitle" | "cad" | "gis" | "vm" | "container" | "webasset" | "game" | "other"));
            
            if is_extension {
                let ext = if type_str.starts_with('.') {
                    type_str[1..].to_string()
                } else {
                    type_str.clone()
                };
                (None, Some(ext), text)
            } else {
                let file_type = match_file_type_string(&type_str);
                (file_type, None, text)
            }
        } else if let Some(type_part) = query.strip_prefix("type:") {
            let type_part = type_part.trim();
            let (type_str, text) = if let Some(space_idx) = type_part.find(' ') {
                let type_str = type_part[..space_idx].trim().to_lowercase();
                let text = type_part[space_idx..].trim().to_string();
                (type_str, text)
            } else {
                (type_part.to_lowercase(), String::new())
            };
            
            let is_extension = type_str.starts_with('.') || 
                (type_str.len() <= 5 && !type_str.contains(' ') && 
                 !matches!(type_str.as_str(), "video" | "audio" | "image" | "code" | "text" | "document" | "archive" | "installer" | "database" | "backup" | "font" | "log" | "certificate" | "system" | "build" | "subtitle" | "cad" | "gis" | "vm" | "container" | "webasset" | "game" | "other"));
            
            if is_extension {
                let ext = if type_str.starts_with('.') {
                    type_str[1..].to_string()
                } else {
                    type_str.clone()
                };
                (None, Some(ext), text)
            } else {
                let file_type = match_file_type_string(&type_str);
                (file_type, None, text)
            }
        } else {
            (None, None, query.to_lowercase())
        }
    };

    let search_text = if app_state.search_mode {
        format!("/ {}_", app_state.search_query) // Cursor indicator
    } else if app_state.search_query.is_empty() {
        "Press / to filter results... Use /type:image, /type:.jpg, etc.".to_string()
    } else {
        let mut filter_text = String::new();
        let has_extension_filter = extension_filter.is_some();
        if let Some(ref ext) = extension_filter {
            filter_text.push_str(&format!("Extension: .{} ", ext));
        } else if let Some(file_type) = type_filter {
            filter_text.push_str(&format!("Type: {} ", file_type.as_str()));
        }
        if !text_query.is_empty() {
            filter_text.push_str(&format!("Text: {}", text_query));
        } else if has_extension_filter || type_filter.is_some() {
            filter_text.push_str("(all matching)");
        } else {
            filter_text.push_str(&app_state.search_query);
        }
        format!("Filter: {} (Esc to clear)", filter_text)
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

/// Match a string to a FileType enum value (case-insensitive, partial match)
/// Also supports file extensions like ".jpg", "jpg", ".mp4", etc.
fn match_file_type_string(type_str: &str) -> Option<crate::utils::FileType> {
    use crate::utils::FileType;
    let type_lower = type_str.to_lowercase();
    
    // If it starts with ., definitely treat as extension
    if type_lower.starts_with('.') {
        let ext = &type_lower[1..];
        let test_path_str = format!("file.{}", ext);
        let test_path = std::path::Path::new(&test_path_str);
        let detected_type = crate::utils::detect_file_type(test_path);
        if detected_type != FileType::Other {
            return Some(detected_type);
        }
        return None;
    }
    
    // Try exact match for type names first
    let type_match = match type_lower.as_str() {
        "video" => Some(FileType::Video),
        "audio" => Some(FileType::Audio),
        "image" => Some(FileType::Image),
        "diskimage" | "disk image" | "disk" => Some(FileType::DiskImage),
        "archive" => Some(FileType::Archive),
        "installer" => Some(FileType::Installer),
        "document" | "doc" => Some(FileType::Document),
        "spreadsheet" | "sheet" => Some(FileType::Spreadsheet),
        "presentation" | "pres" => Some(FileType::Presentation),
        "code" | "source" | "src" => Some(FileType::Code),
        "text" => Some(FileType::Text),
        "database" | "db" => Some(FileType::Database),
        "backup" => Some(FileType::Backup),
        "font" | "fonts" => Some(FileType::Font),
        "log" | "logs" => Some(FileType::Log),
        "certificate" | "cert" | "crypto" => Some(FileType::Certificate),
        "system" | "sys" => Some(FileType::System),
        "build" => Some(FileType::Build),
        "subtitle" | "sub" | "subs" => Some(FileType::Subtitle),
        "cad" => Some(FileType::CAD),
        "3d" | "3dmodel" | "3d model" | "model" => Some(FileType::Model3D),
        "gis" | "map" | "maps" => Some(FileType::GIS),
        "vm" | "virtualmachine" | "virtual machine" => Some(FileType::VirtualMachine),
        "container" | "docker" => Some(FileType::Container),
        "webasset" | "web asset" | "web" => Some(FileType::WebAsset),
        "game" | "games" => Some(FileType::Game),
        "other" => Some(FileType::Other),
        _ => None,
    };
    
    // If type name matched, return it
    if type_match.is_some() {
        return type_match;
    }
    
    // If no type name match and it's a short string (likely an extension), try as extension
    if type_lower.len() <= 4 && !type_lower.contains(' ') {
        let test_path_str = format!("file.{}", type_lower);
        let test_path = std::path::Path::new(&test_path_str);
        let detected_type = crate::utils::detect_file_type(test_path);
        if detected_type != FileType::Other {
            return Some(detected_type);
        }
    }
    
    None
}

fn render_grouped_results(f: &mut Frame, area: Rect, app_state: &mut AppState) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Styles::border())
        .title("CATEGORIES");

    let inner = block.inner(area);
    f.render_widget(block, area);

    if app_state.category_groups.is_empty() {
        let empty = Paragraph::new(Line::from(vec![Span::styled(
            "  No items found",
            Styles::secondary(),
        )]));
        f.render_widget(empty, inner);
        return;
    }

    // Build display lines from a flattened row model so navigation matches rendering.
    let mut lines: Vec<Line> = Vec::new();
    let rows = if app_state.search_query.is_empty() {
        app_state.results_rows()
    } else {
        app_state.filtered_results_rows()
    };

    // If rows is empty but we have category groups, something went wrong
    // Try to show items directly as a fallback - show ALL categories
    if rows.is_empty() && !app_state.category_groups.is_empty() {
        if app_state.search_query.is_empty() {
            // Fallback: show items directly from all category groups
            for (group_idx, group) in app_state.category_groups.iter().enumerate() {
                let item_indices = app_state.category_item_indices(group_idx);
                if !item_indices.is_empty() {
                    // Show category name as header
                    if app_state.category_groups.len() > 1 {
                        let category_emoji_icon = category_emoji(&group.name);
                        lines.push(Line::from(vec![
                            Span::styled("  ", Style::default()),
                            Span::styled(format!("{} ", category_emoji_icon), Styles::secondary()),
                            Span::styled(
                                format!("{} ({} items)", group.name, item_indices.len()),
                                Styles::emphasis(),
                            ),
                        ]));
                    }
                    // Show items directly without folder grouping
                    for &item_idx in &item_indices {
                        let Some(item) = app_state.all_items.get(item_idx) else {
                            continue;
                        };
                        let is_selected = app_state.selected_items.contains(&item_idx);
                        let checkbox = if is_selected { "[X]" } else { "[ ]" };
                        let checkbox_style = if is_selected {
                            Styles::checked()
                        } else {
                            Styles::secondary()
                        };
                        // For applications, use display name from the registry map.
                        // Fallbacks are only for rare cases where lookup fails.
                        let display_str = if item.category == "Installed Applications" {
                            item.display_name
                                .clone()
                                .or_else(|| {
                                    item.path
                                        .file_name()
                                        .and_then(|n| n.to_str())
                                        .map(|s| s.to_string())
                                })
                                .unwrap_or_else(|| {
                                    crate::utils::to_relative_path(&item.path, &app_state.scan_path)
                                })
                        } else {
                            crate::utils::to_relative_path(&item.path, &app_state.scan_path)
                        };
                        let size_str = bytesize::to_string(item.size_bytes, true);
                        let ago_str = if item.category == "Installed Applications" {
                            Some(format_ago(item.last_opened))
                        } else {
                            None
                        };
                        let indent = if app_state.category_groups.len() > 1 {
                            "    "
                        } else {
                            "  "
                        };
                        // Add emoji based on file type
                        let file_type = detect_file_type(&item.path);
                        let emoji = file_type.emoji();
                        lines.push(Line::from(vec![
                            Span::styled(indent, Style::default()),
                            Span::styled(checkbox, checkbox_style),
                            Span::styled(" ", Style::default()),
                            Span::styled(format!("{} ", emoji), Styles::secondary()),
                            Span::styled(display_str, Styles::primary()),
                            Span::styled(format!("  {:>8}", size_str), Styles::secondary()),
                            if let Some(ago) = ago_str {
                                Span::styled(format!(" | {:>8}", ago), Styles::secondary())
                            } else {
                                Span::raw("")
                            },
                        ]));
                    }
                    if app_state.category_groups.len() > 1
                        && group_idx < app_state.category_groups.len() - 1
                    {
                        lines.push(Line::from(""));
                    }
                }
            }
            if lines.is_empty() {
                lines.push(Line::from(vec![Span::styled(
                    "  No items to display",
                    Styles::secondary(),
                )]));
            }
        } else {
            lines.push(Line::from(vec![Span::styled(
                "  No matches found",
                Styles::secondary(),
            )]));
        }
    }

    fn tri_checkbox(selected: usize, total: usize) -> (&'static str, Style) {
        if total == 0 || selected == 0 {
            ("[ ]", Styles::secondary())
        } else if selected == total {
            ("[X]", Styles::checked())
        } else {
            ("[-]", Styles::warning())
        }
    }

    // Track the current folder path at each nesting depth so items can be displayed
    // relative to their parent folder (tree-style).
    let mut folder_stack: Vec<String> = Vec::new();

    // When there's only one category, skip category header and adjust indentation
    let skip_category_header = app_state.category_groups.len() == 1;
    let base_indent = if skip_category_header { "" } else { "    " };

    for (row_idx, row) in rows.iter().enumerate() {
        let is_cursor = row_idx == app_state.cursor;
        let row_style = if is_cursor {
            Styles::selected()
        } else {
            Style::default()
        };
        let apply_sel = |s: Style| {
            if is_cursor {
                s.patch(Styles::selected())
            } else {
                s
            }
        };
        let prefix = if is_cursor { ">" } else { " " };

        match *row {
            crate::tui::state::ResultsRow::CategoryHeader { group_idx } => {
                // Skip rendering category header if there's only one category
                if skip_category_header {
                    continue;
                }

                let Some(group) = app_state.category_groups.get(group_idx) else {
                    continue;
                };
                folder_stack.clear();

                let icon = if group.safe { "✓" } else { "!" };
                let icon_style = category_style(group.safe);
                let category_emoji_icon = category_emoji(&group.name);

                let item_indices = app_state.category_item_indices(group_idx);
                let selected_in_group = item_indices
                    .iter()
                    .filter(|&&idx| app_state.selected_items.contains(&idx))
                    .count();
                let total_in_group = item_indices.len();

                let (checkbox, checkbox_style) = tri_checkbox(selected_in_group, total_in_group);
                let exp_marker = if group.expanded || !app_state.search_query.is_empty() {
                    "▾"
                } else {
                    "▸"
                };

                let header_line = Line::from(vec![
                    Span::styled(format!(" {} ", prefix), row_style),
                    Span::styled(checkbox, apply_sel(checkbox_style)),
                    Span::styled(" ", row_style),
                    Span::styled(format!("{} {} ", exp_marker, icon), apply_sel(icon_style)),
                    Span::styled(format!("{} ", category_emoji_icon), apply_sel(Styles::secondary())),
                    Span::styled(format!("{:<12}", group.name), apply_sel(Styles::emphasis())),
                    Span::styled(
                        format!("{:>8}", bytesize::to_string(group.total_size, true)),
                        apply_sel(Styles::primary()),
                    ),
                    Span::styled("    ", apply_sel(Styles::secondary())),
                    Span::styled(
                        format!("{}/{} items", selected_in_group, total_in_group),
                        apply_sel(Styles::secondary()),
                    ),
                    if group.safe {
                        Span::styled("  [safe to delete]", apply_sel(Styles::checked()))
                    } else {
                        Span::styled("  [review recommended]", apply_sel(Styles::warning()))
                    },
                ]);
                lines.push(header_line);
            }
            crate::tui::state::ResultsRow::FolderHeader {
                group_idx,
                folder_idx,
                depth,
            } => {
                let Some(group) = app_state.category_groups.get(group_idx) else {
                    continue;
                };
                let Some(folder) = group.folder_groups.get(folder_idx) else {
                    continue;
                };
                // Capture parent folder key BEFORE we update the stack.
                let parent_key = if depth > 0 {
                    folder_stack.get(depth - 1).cloned()
                } else {
                    None
                };

                // Update folder stack for stripping item prefixes.
                let folder_path = std::path::PathBuf::from(&folder.folder_name);
                let folder_key = crate::utils::to_relative_path(&folder_path, &app_state.scan_path);
                if folder_stack.len() <= depth {
                    folder_stack.resize(depth + 1, String::new());
                }
                folder_stack[depth] = folder_key;
                folder_stack.truncate(depth + 1);

                let selected_in_folder = folder
                    .items
                    .iter()
                    .filter(|&&idx| app_state.selected_items.contains(&idx))
                    .count();
                let total_in_folder = folder.items.len();
                let (checkbox, checkbox_style) = tri_checkbox(selected_in_folder, total_in_folder);
                let exp_marker = if folder.expanded || !app_state.search_query.is_empty() {
                    "▾"
                } else {
                    "▸"
                };

                // Truncate folder path to avoid line wrapping.
                // Convert folder_name (which may be absolute) to relative path
                let folder_path = std::path::PathBuf::from(&folder.folder_name);
                let mut folder_str =
                    crate::utils::to_relative_path(&folder_path, &app_state.scan_path);

                // If nested, show folder name relative to its parent folder.
                if let Some(parent) = parent_key {
                    if parent != "(root)" && !parent.is_empty() {
                        let normalized_parent = parent.replace('\\', "/");
                        let normalized_folder = folder_str.replace('\\', "/");
                        if normalized_folder.starts_with(&normalized_parent) {
                            let remaining = &normalized_folder[normalized_parent.len()..];
                            folder_str =
                                remaining.strip_prefix('/').unwrap_or(remaining).to_string();
                        }
                    }
                }
                let size_str = bytesize::to_string(folder.total_size, true);
                let folder_emoji_icon = folder_emoji(app_state, folder);

                // Indent folder headers by nesting depth.
                let indent = format!("{base_indent}{}", "  ".repeat(depth));
                let fixed = indent.len() + 2 /*prefix*/ + 1 /*space*/ + 3 /*checkbox*/ + 1 /*space*/ + 2 /*exp*/ + 1 /*space*/ + 2 /*emoji + space*/ + 2 /*two spaces before size*/ + 8 + 2 /*two spaces before count*/ + 10;
                let max_len = (inner.width as usize).saturating_sub(fixed).max(8);
                let folder_display = truncate_end(&folder_str, max_len);

                let folder_header = Line::from(vec![
                    Span::styled(format!("{}{} ", indent, prefix), row_style),
                    Span::styled(checkbox, apply_sel(checkbox_style)),
                    Span::styled(" ", row_style),
                    Span::styled(format!("{} ", exp_marker), apply_sel(Styles::secondary())),
                    Span::styled(format!("{} ", folder_emoji_icon), apply_sel(Styles::secondary())),
                    Span::styled(folder_display, apply_sel(Styles::emphasis())),
                    Span::styled(format!("  {:>8}", size_str), apply_sel(Styles::primary())),
                    Span::styled(
                        format!("  ({}/{})", selected_in_folder, total_in_folder),
                        apply_sel(Styles::secondary()),
                    ),
                ]);
                lines.push(folder_header);
            }
            crate::tui::state::ResultsRow::Item { item_idx, depth } => {
                let Some(item) = app_state.all_items.get(item_idx) else {
                    continue;
                };

                let is_selected = app_state.selected_items.contains(&item_idx);
                let checkbox = if is_selected { "[X]" } else { "[ ]" };
                let checkbox_style = if is_selected {
                    Styles::checked()
                } else {
                    Styles::secondary()
                };

                // Indent items by their nesting depth.
                let indent = format!("{base_indent}{}", "  ".repeat(depth));

                // For applications, show the registry display name (fallback to filename/path).
                let path_str = if item.category == "Installed Applications" {
                    item.display_name
                        .clone()
                        .or_else(|| {
                            item.path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .map(|s| s.to_string())
                        })
                        .unwrap_or_else(|| {
                            crate::utils::to_relative_path(&item.path, &app_state.scan_path)
                        })
                } else {
                    // Truncate the path to avoid line wrapping.
                    let mut pstr = crate::utils::to_relative_path(&item.path, &app_state.scan_path);

                    // If we're nested under a folder header, strip the folder prefix from the path.
                    if depth > 0 {
                        if let Some(folder_path) = folder_stack.get(depth - 1) {
                            if folder_path != "(root)" && !folder_path.is_empty() {
                                // Normalize paths for comparison (handle both / and \)
                                let normalized_folder = folder_path.replace('\\', "/");
                                let normalized_path = pstr.replace('\\', "/");

                                // Strip the folder path prefix from the item path
                                if normalized_path.starts_with(&normalized_folder) {
                                    // Remove the folder path and the following separator
                                    let remaining = &normalized_path[normalized_folder.len()..];
                                    pstr = if let Some(stripped) = remaining.strip_prefix('/') {
                                        stripped.to_string()
                                    } else if remaining.is_empty() {
                                        // If the item path is exactly the folder path, show just the filename
                                        item.path
                                            .file_name()
                                            .and_then(|n| n.to_str())
                                            .map(|s| s.to_string())
                                            .unwrap_or_else(|| remaining.to_string())
                                    } else {
                                        remaining.to_string()
                                    };
                                }
                            }
                        }
                    }
                    pstr
                };

                let size_str = bytesize::to_string(item.size_bytes, true);
                let ago_str = if item.category == "Installed Applications" {
                    Some(format_ago(item.last_opened))
                } else {
                    None
                };

                // Add emoji based on file type
                let file_type = detect_file_type(&item.path);
                let emoji = file_type.emoji();
                
                let fixed = indent.len()
                    + 3 /*prefix+spaces*/
                    + 3 /*checkbox*/
                    + 1 /*space*/
                    + 3 /*emoji + space*/
                    + 2 /*two spaces before size*/
                    + 8 /*size*/
                    + if ago_str.is_some() { 3 /*" | "*/ + 8 /*ago*/ } else { 0 };
                let max_len = (inner.width as usize).saturating_sub(fixed).max(8);
                let path_display = truncate_end(&path_str, max_len);

                // Add underline to path when cursor is on this item
                let path_style = if is_cursor {
                    row_style.add_modifier(Modifier::UNDERLINED)
                } else {
                    row_style
                };

                let item_line = Line::from(vec![
                    Span::styled(format!("{}{} ", indent, prefix), row_style),
                    Span::styled(checkbox, apply_sel(checkbox_style)),
                    Span::styled(" ", row_style),
                    Span::styled(format!("{} ", emoji), apply_sel(Styles::secondary())),
                    Span::styled(path_display, path_style),
                    Span::styled(format!("  {:>8}", size_str), apply_sel(Styles::secondary())),
                    if let Some(ago) = ago_str {
                        Span::styled(format!(" | {:>8}", ago), apply_sel(Styles::secondary()))
                    } else {
                        Span::raw("")
                    },
                ]);
                lines.push(item_line);
            }
            crate::tui::state::ResultsRow::Spacer => {
                folder_stack.clear();
                lines.push(Line::from(""));
            }
        }
    }

    // Handle scrolling
    let visible_height = inner.height as usize;
    // Update cached visible height in app state for event handlers
    app_state.visible_height = visible_height;
    let total_lines = lines.len();
    let scroll = app_state
        .scroll_offset
        .min(total_lines.saturating_sub(visible_height));

    let visible_lines: Vec<Line> = lines
        .into_iter()
        .skip(scroll)
        .take(visible_height)
        .collect();

    let paragraph = Paragraph::new(visible_lines);
    f.render_widget(paragraph, inner);

    // Scrollbar
    if total_lines > visible_height {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("▲"))
            .end_symbol(Some("▼"));
        let mut scrollbar_state = ScrollbarState::new(total_lines).position(scroll);
        f.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
    }
}
