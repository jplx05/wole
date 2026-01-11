use crate::cli::ScanOptions;
use crate::theme::Theme;
use serde::Serialize;
use std::path::PathBuf;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

// Forward declaration for duplicate groups
pub use crate::categories::duplicates::DuplicateGroup;

/// Get emoji for a category name in CLI output
fn category_emoji(category_name: &str) -> &'static str {
    match category_name {
        "Installed Applications" | "Applications" => "📱",
        "Old Files" => "📅",
        "Downloads" | "Old Downloads" => "⬇️",
        "Large Files" | "Large" => "📦",
        "Package Cache" | "Package cache" => "📚",
        "Application Cache" | "Application cache" => "💾",
        "Temp Files" | "Temp" => "🗑️",
        "Trash" => "🗑️",
        "Build Artifacts" | "Build" => "🔨",
        "Browser Cache" | "Browser" => "🌐",
        "System Cache" | "System" => "⚙️",
        "Empty Folders" | "Empty" => "📁",
        "Duplicates" => "📋",
        "Windows Update" => "🔄",
        "Event Logs" => "📋",
        _ => "📁", // Default folder emoji
    }
}

/// Truncate a string to a maximum display width (adds ellipsis if needed).
fn truncate_to_width(s: &str, max_width: usize) -> String {
    if UnicodeWidthStr::width(s) <= max_width {
        return s.to_string();
    }

    let ellipsis = "…";
    let ellipsis_w = UnicodeWidthStr::width(ellipsis);
    let target = max_width.saturating_sub(ellipsis_w);

    let mut out = String::new();
    let mut w = 0usize;
    for ch in s.chars() {
        let cw = UnicodeWidthChar::width(ch).unwrap_or(0);
        if w + cw > target {
            break;
        }
        out.push(ch);
        w += cw;
    }
    out.push_str(ellipsis);
    out
}

/// Pad/truncate content to a specific display width (Unicode-aware).
fn pad_right_to_width(s: &str, width: usize) -> String {
    let truncated = truncate_to_width(s, width);
    let w = UnicodeWidthStr::width(truncated.as_str());
    format!("{}{}", truncated, " ".repeat(width.saturating_sub(w)))
}

/// Print a table row with borders and 1-space cell padding.
fn print_table_row(cols: &[(String, usize)]) {
    let mut row = String::from("│");
    for (content, width) in cols {
        row.push(' ');
        row.push_str(&pad_right_to_width(content, *width));
        row.push(' ');
        row.push('│');
    }
    println!("{}", row);
}

/// Print a horizontal separator line (Unicode box drawing).
/// Widths are content widths (excluding the 1-space left/right padding).
fn print_table_separator(widths: &[usize], left: &str, mid: &str, right: &str) {
    let mut sep = left.to_string();
    for (i, width) in widths.iter().enumerate() {
        if i > 0 {
            sep.push_str(mid);
        }
        // +2 for the 1-space padding on each side of the cell
        sep.push_str(&"─".repeat(width + 2));
    }
    sep.push_str(right);
    println!("{}", sep);
}

/// Output verbosity mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    Quiet,       // Only errors
    Normal,      // Standard output
    Verbose,     // More details
    VeryVerbose, // All details including file paths
}

#[derive(Default, Debug, Clone)]
pub struct ScanResults {
    pub cache: CategoryResult,
    pub app_cache: CategoryResult,
    pub temp: CategoryResult,
    pub trash: CategoryResult,
    pub build: CategoryResult,
    pub downloads: CategoryResult,
    pub large: CategoryResult,
    pub old: CategoryResult,
    pub applications: CategoryResult,
    pub browser: CategoryResult,
    pub system: CategoryResult,
    pub empty: CategoryResult,
    pub duplicates: CategoryResult,
    pub windows_update: CategoryResult,
    pub event_logs: CategoryResult,
    /// Optional duplicate groups for enhanced display (only populated for duplicates category)
    pub duplicates_groups: Option<Vec<DuplicateGroup>>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct CategoryResult {
    pub items: usize,
    pub size_bytes: u64,
    pub paths: Vec<PathBuf>,
}

impl CategoryResult {
    pub fn size_human(&self) -> String {
        bytesize::to_string(self.size_bytes, false)
    }
}

#[derive(Serialize)]
struct JsonResults {
    version: String,
    timestamp: String,
    categories: JsonCategories,
    summary: JsonSummary,
}

#[derive(Serialize)]
struct JsonCategories {
    cache: JsonCategory,
    app_cache: JsonCategory,
    temp: JsonCategory,
    trash: JsonCategory,
    build: JsonCategory,
    downloads: JsonCategory,
    large: JsonCategory,
    old: JsonCategory,
    applications: JsonCategory,
    browser: JsonCategory,
    system: JsonCategory,
    empty: JsonCategory,
    duplicates: JsonCategory,
    windows_update: JsonCategory,
    event_logs: JsonCategory,
}

#[derive(Serialize)]
struct JsonCategory {
    items: usize,
    size_bytes: u64,
    size_human: String,
    paths: Vec<String>,
}

#[derive(Serialize)]
struct JsonSummary {
    total_items: usize,
    total_bytes: u64,
    total_human: String,
}

pub fn print_human(results: &ScanResults, mode: OutputMode) {
    print_human_with_options(results, mode, None)
}

pub fn print_human_with_options(
    results: &ScanResults,
    mode: OutputMode,
    options: Option<&ScanOptions>,
) {
    if mode == OutputMode::Quiet {
        return;
    }

    println!();
    println!("{}", Theme::header("Wole Scan Results"));
    println!("{}", Theme::divider_bold(60));
    println!();

    // Table column widths
    // (content widths; padding handled by table helpers)
    let col_widths = [26, 7, 12, 24];

    // Print table header with borders
    print_table_separator(&col_widths, "┌", "┬", "┐");
    print_table_row(&[
        (Theme::primary("Category"), col_widths[0]),
        (Theme::primary("Items"), col_widths[1]),
        (Theme::primary("Size"), col_widths[2]),
        (Theme::primary("Status"), col_widths[3]),
    ]);
    print_table_separator(&col_widths, "├", "┼", "┤");

    let categories = [
        ("Package cache", &results.cache, "[OK] Safe to clean"),
        (
            "Application cache",
            &results.app_cache,
            "[OK] Safe to clean",
        ),
        ("Temp", &results.temp, "[OK] Safe to clean"),
        ("Trash", &results.trash, "[OK] Safe to clean"),
        ("Build", &results.build, "[OK] Inactive projects"),
        ("Downloads", &results.downloads, "[OK] Old files"),
        ("Large", &results.large, "[!] Review suggested"),
        ("Old", &results.old, "[!] Review suggested"),
        (
            "Applications",
            &results.applications,
            "[!] Review suggested",
        ),
        ("Browser", &results.browser, "[OK] Safe to clean"),
        ("System", &results.system, "[OK] Safe to clean"),
        ("Empty", &results.empty, "[OK] Safe to clean"),
        ("Duplicates", &results.duplicates, "[!] Review suggested"),
        (
            "Windows Update",
            &results.windows_update,
            "[!] Requires admin",
        ),
        ("Event Logs", &results.event_logs, "[!] Requires admin"),
    ];

    for (name, result, status) in categories {
        if result.items > 0 {
            let status_colored = if status.starts_with("[OK]") {
                Theme::status_safe(status)
            } else {
                Theme::status_review(status)
            };
            let emoji = category_emoji(name);
            let category_display = format!("{} {}", emoji, name);
            print_table_row(&[
                (Theme::category(&category_display), col_widths[0]),
                (Theme::value(&result.items.to_string()), col_widths[1]),
                (Theme::size(&result.size_human()), col_widths[2]),
                (status_colored, col_widths[3]),
            ]);

            // Special handling for duplicates: show groups in verbose mode
            if name == "Duplicates"
                && (mode == OutputMode::Verbose || mode == OutputMode::VeryVerbose)
            {
                if let Some(ref groups) = results.duplicates_groups {
                    let show_groups = if mode == OutputMode::Verbose {
                        std::cmp::min(5, groups.len())
                    } else {
                        groups.len()
                    };

                    for (idx, group) in groups.iter().take(show_groups).enumerate() {
                        println!(
                            "  {} Group {} ({} files, {} each):",
                            Theme::muted("└─"),
                            idx + 1,
                            group.paths.len(),
                            bytesize::to_string(group.size, false)
                        );
                        for path in &group.paths {
                            let file_type = crate::utils::detect_file_type(path);
                            let emoji = file_type.emoji();
                            println!(
                                "     {} {}",
                                emoji,
                                Theme::muted(&path.display().to_string())
                            );
                        }
                    }

                    if groups.len() > show_groups {
                        println!(
                            "  {} ... and {} more groups",
                            Theme::muted(""),
                            Theme::muted(&(groups.len() - show_groups).to_string())
                        );
                    }
                } else {
                    // Fallback to regular path display if groups not available
                    if mode == OutputMode::Verbose && !result.paths.is_empty() {
                        let show_count = std::cmp::min(3, result.paths.len());
                        for path in result.paths.iter().take(show_count) {
                            let file_type = crate::utils::detect_file_type(path);
                            let emoji = file_type.emoji();
                            println!("  {} {}", emoji, Theme::muted(&path.display().to_string()));
                        }
                        if result.paths.len() > show_count {
                            println!(
                                "  {} ... and {} more",
                                Theme::muted(""),
                                Theme::muted(&(result.paths.len() - show_count).to_string())
                            );
                        }
                    } else if mode == OutputMode::VeryVerbose {
                        for path in &result.paths {
                            let file_type = crate::utils::detect_file_type(path);
                            let emoji = file_type.emoji();
                            println!("  {} {}", emoji, Theme::muted(&path.display().to_string()));
                        }
                    }
                }
            } else {
                // Regular path display for other categories
                // In verbose mode, show first few paths
                if mode == OutputMode::Verbose && !result.paths.is_empty() {
                    let show_count = std::cmp::min(3, result.paths.len());
                    for path in result.paths.iter().take(show_count) {
                        let file_type = crate::utils::detect_file_type(path);
                        let emoji = file_type.emoji();
                        println!("  {} {}", emoji, Theme::muted(&path.display().to_string()));
                    }
                    if result.paths.len() > show_count {
                        println!(
                            "  {} ... and {} more",
                            Theme::muted(""),
                            Theme::muted(&(result.paths.len() - show_count).to_string())
                        );
                    }
                }

                // In very verbose mode, show all paths
                if mode == OutputMode::VeryVerbose {
                    for path in &result.paths {
                        let file_type = crate::utils::detect_file_type(path);
                        let emoji = file_type.emoji();
                        println!("  {} {}", emoji, Theme::muted(&path.display().to_string()));
                    }
                }
            }
        }
    }

    let total_items = results.cache.items
        + results.app_cache.items
        + results.temp.items
        + results.trash.items
        + results.build.items
        + results.downloads.items
        + results.large.items
        + results.old.items
        + results.applications.items
        + results.browser.items
        + results.system.items
        + results.empty.items
        + results.duplicates.items
        + results.windows_update.items
        + results.event_logs.items;
    let total_bytes = results.cache.size_bytes
        + results.app_cache.size_bytes
        + results.temp.size_bytes
        + results.trash.size_bytes
        + results.build.size_bytes
        + results.downloads.size_bytes
        + results.large.size_bytes
        + results.old.size_bytes
        + results.applications.size_bytes
        + results.browser.size_bytes
        + results.system.size_bytes
        + results.empty.size_bytes
        + results.duplicates.size_bytes
        + results.windows_update.size_bytes
        + results.event_logs.size_bytes;

    if total_items == 0 {
        print_table_separator(&col_widths, "└", "┴", "┘");
        println!();
        println!(
            "{}",
            Theme::success("Your system is clean! No reclaimable space found.")
        );
    } else {
        // Total row inside the same table box
        print_table_separator(&col_widths, "├", "┼", "┤");
        print_table_row(&[
            (Theme::header("Total"), col_widths[0]),
            (Theme::value(&total_items.to_string()), col_widths[1]),
            (
                Theme::size(&bytesize::to_string(total_bytes, false)),
                col_widths[2],
            ),
            (Theme::success("Reclaimable"), col_widths[3]),
        ]);
        print_table_separator(&col_widths, "└", "┴", "┘");
        println!();
        let clean_command = build_clean_command(options);
        println!(
            "Run {} to remove these files.",
            Theme::command(&clean_command)
        );
    }
    println!();
}

/// Build a clean command based on the scan options used
fn build_clean_command(options: Option<&ScanOptions>) -> String {
    let Some(opts) = options else {
        return "wole clean --all".to_string();
    };

    // Count how many categories are enabled
    let enabled_count = [
        opts.cache,
        opts.app_cache,
        opts.temp,
        opts.trash,
        opts.build,
        opts.downloads,
        opts.large,
        opts.old,
        opts.applications,
        opts.browser,
        opts.system,
        opts.empty,
        opts.duplicates,
        opts.windows_update,
        opts.event_logs,
    ]
    .iter()
    .filter(|&&x| x)
    .count();

    // If all categories are enabled, use --all
    if enabled_count == 15 {
        return "wole clean --all".to_string();
    }

    // Build command with specific flags
    let mut flags = Vec::new();
    if opts.cache {
        flags.push("--cache");
    }
    if opts.app_cache {
        flags.push("--app-cache");
    }
    if opts.temp {
        flags.push("--temp");
    }
    if opts.trash {
        flags.push("--trash");
    }
    if opts.build {
        flags.push("--build");
    }
    if opts.downloads {
        flags.push("--downloads");
    }
    if opts.large {
        flags.push("--large");
    }
    if opts.old {
        flags.push("--old");
    }
    if opts.applications {
        flags.push("--applications");
    }
    if opts.browser {
        flags.push("--browser");
    }
    if opts.system {
        flags.push("--system");
    }
    if opts.empty {
        flags.push("--empty");
    }
    if opts.duplicates {
        flags.push("--duplicates");
    }
    if opts.windows_update {
        flags.push("--windows-update");
    }
    if opts.event_logs {
        flags.push("--event-logs");
    }

    // If no flags (shouldn't happen, but be safe), fall back to --all
    if flags.is_empty() {
        return "wole clean --all".to_string();
    }

    format!("wole clean {}", flags.join(" "))
}

pub fn print_json(results: &ScanResults) -> anyhow::Result<()> {
    let json_results = JsonResults {
        version: "1.0".to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        categories: JsonCategories {
            cache: JsonCategory {
                items: results.cache.items,
                size_bytes: results.cache.size_bytes,
                size_human: results.cache.size_human(),
                paths: results
                    .cache
                    .paths
                    .iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect(),
            },
            app_cache: JsonCategory {
                items: results.app_cache.items,
                size_bytes: results.app_cache.size_bytes,
                size_human: results.app_cache.size_human(),
                paths: results
                    .app_cache
                    .paths
                    .iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect(),
            },
            temp: JsonCategory {
                items: results.temp.items,
                size_bytes: results.temp.size_bytes,
                size_human: results.temp.size_human(),
                paths: results
                    .temp
                    .paths
                    .iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect(),
            },
            trash: JsonCategory {
                items: results.trash.items,
                size_bytes: results.trash.size_bytes,
                size_human: results.trash.size_human(),
                paths: results
                    .trash
                    .paths
                    .iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect(),
            },
            build: JsonCategory {
                items: results.build.items,
                size_bytes: results.build.size_bytes,
                size_human: results.build.size_human(),
                paths: results
                    .build
                    .paths
                    .iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect(),
            },
            downloads: JsonCategory {
                items: results.downloads.items,
                size_bytes: results.downloads.size_bytes,
                size_human: results.downloads.size_human(),
                paths: results
                    .downloads
                    .paths
                    .iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect(),
            },
            large: JsonCategory {
                items: results.large.items,
                size_bytes: results.large.size_bytes,
                size_human: results.large.size_human(),
                paths: results
                    .large
                    .paths
                    .iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect(),
            },
            old: JsonCategory {
                items: results.old.items,
                size_bytes: results.old.size_bytes,
                size_human: results.old.size_human(),
                paths: results
                    .old
                    .paths
                    .iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect(),
            },
            applications: JsonCategory {
                items: results.applications.items,
                size_bytes: results.applications.size_bytes,
                size_human: results.applications.size_human(),
                paths: results
                    .applications
                    .paths
                    .iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect(),
            },
            browser: JsonCategory {
                items: results.browser.items,
                size_bytes: results.browser.size_bytes,
                size_human: results.browser.size_human(),
                paths: results
                    .browser
                    .paths
                    .iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect(),
            },
            system: JsonCategory {
                items: results.system.items,
                size_bytes: results.system.size_bytes,
                size_human: results.system.size_human(),
                paths: results
                    .system
                    .paths
                    .iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect(),
            },
            empty: JsonCategory {
                items: results.empty.items,
                size_bytes: results.empty.size_bytes,
                size_human: results.empty.size_human(),
                paths: results
                    .empty
                    .paths
                    .iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect(),
            },
            duplicates: JsonCategory {
                items: results.duplicates.items,
                size_bytes: results.duplicates.size_bytes,
                size_human: results.duplicates.size_human(),
                paths: results
                    .duplicates
                    .paths
                    .iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect(),
            },
            windows_update: JsonCategory {
                items: results.windows_update.items,
                size_bytes: results.windows_update.size_bytes,
                size_human: results.windows_update.size_human(),
                paths: results
                    .windows_update
                    .paths
                    .iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect(),
            },
            event_logs: JsonCategory {
                items: results.event_logs.items,
                size_bytes: results.event_logs.size_bytes,
                size_human: results.event_logs.size_human(),
                paths: results
                    .event_logs
                    .paths
                    .iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect(),
            },
        },
        summary: JsonSummary {
            total_items: results.cache.items
                + results.app_cache.items
                + results.temp.items
                + results.trash.items
                + results.build.items
                + results.downloads.items
                + results.large.items
                + results.old.items
                + results.applications.items
                + results.browser.items
                + results.system.items
                + results.empty.items
                + results.duplicates.items
                + results.windows_update.items
                + results.event_logs.items,
            total_bytes: results.cache.size_bytes
                + results.app_cache.size_bytes
                + results.temp.size_bytes
                + results.trash.size_bytes
                + results.build.size_bytes
                + results.downloads.size_bytes
                + results.large.size_bytes
                + results.old.size_bytes
                + results.applications.size_bytes
                + results.browser.size_bytes
                + results.system.size_bytes
                + results.empty.size_bytes
                + results.duplicates.size_bytes
                + results.windows_update.size_bytes
                + results.event_logs.size_bytes,
            total_human: bytesize::to_string(
                results.cache.size_bytes
                    + results.app_cache.size_bytes
                    + results.temp.size_bytes
                    + results.trash.size_bytes
                    + results.build.size_bytes
                    + results.downloads.size_bytes
                    + results.large.size_bytes
                    + results.old.size_bytes
                    + results.applications.size_bytes
                    + results.browser.size_bytes
                    + results.system.size_bytes
                    + results.empty.size_bytes
                    + results.duplicates.size_bytes
                    + results.windows_update.size_bytes
                    + results.event_logs.size_bytes,
                true,
            ),
        },
    };

    println!("{}", serde_json::to_string_pretty(&json_results)?);
    Ok(())
}

pub fn print_analyze(results: &ScanResults, mode: OutputMode) {
    if mode == OutputMode::Quiet {
        return;
    }

    println!();
    println!("Scan Results");
    println!();

    // Define categories with their display names
    let mut categories: Vec<(&str, &CategoryResult)> = vec![
        ("Trash", &results.trash),
        ("Large Files", &results.large),
        ("Windows Update", &results.windows_update),
        ("Event Logs", &results.event_logs),
        ("System Cache", &results.system),
        ("Build Artifacts", &results.build),
        ("Old Downloads", &results.downloads),
        ("Duplicates", &results.duplicates),
        ("Old Files", &results.old),
        ("Applications", &results.applications),
        ("Temp Files", &results.temp),
        ("Package Cache", &results.cache),
        ("Application Cache", &results.app_cache),
        ("Browser Cache", &results.browser),
        ("Empty Folders", &results.empty),
    ];

    // Filter out categories with no items and sort by size descending
    categories.retain(|(_, result)| result.items > 0);
    categories.sort_by(|a, b| b.1.size_bytes.cmp(&a.1.size_bytes));

    // Table column widths
    // (content widths; padding handled by table helpers)
    let col_widths = [30, 10, 12];

    // Print table header with borders
    print_table_separator(&col_widths, "┌", "┬", "┐");
    print_table_row(&[
        ("Category".to_string(), col_widths[0]),
        ("Files".to_string(), col_widths[1]),
        ("Size".to_string(), col_widths[2]),
    ]);
    print_table_separator(&col_widths, "├", "┼", "┤");

    // Print category rows
    for (name, result) in &categories {
        let emoji = category_emoji(name);
        let category_display = format!("{} {}", emoji, name);
        print_table_row(&[
            (category_display, col_widths[0]),
            (format_number(result.items as u64), col_widths[1]),
            (result.size_human(), col_widths[2]),
        ]);

        // Special handling for duplicates: show groups in verbose mode
        if *name == "Duplicates" && (mode == OutputMode::Verbose || mode == OutputMode::VeryVerbose)
        {
            if let Some(ref groups) = results.duplicates_groups {
                let show_groups = if mode == OutputMode::Verbose {
                    std::cmp::min(5, groups.len())
                } else {
                    groups.len()
                };

                for (idx, group) in groups.iter().take(show_groups).enumerate() {
                    println!(
                        "  {} Group {} ({} files, {} each):",
                        Theme::muted("└─"),
                        idx + 1,
                        group.paths.len(),
                        bytesize::to_string(group.size, false)
                    );
                    for path in &group.paths {
                        let file_type = crate::utils::detect_file_type(path);
                        let emoji = file_type.emoji();
                        println!(
                            "     {} {}",
                            emoji,
                            Theme::muted(&path.display().to_string())
                        );
                    }
                }

                if groups.len() > show_groups {
                    println!(
                        "  {} ... and {} more groups",
                        Theme::muted(""),
                        Theme::muted(&(groups.len() - show_groups).to_string())
                    );
                }
            } else {
                // Fallback to regular path display if groups not available
                if mode == OutputMode::Verbose && !result.paths.is_empty() {
                    let show_count = std::cmp::min(3, result.paths.len());
                    for path in result.paths.iter().take(show_count) {
                        let file_type = crate::utils::detect_file_type(path);
                        let emoji = file_type.emoji();
                        println!("  {} {}", emoji, Theme::muted(&path.display().to_string()));
                    }
                    if result.paths.len() > show_count {
                        println!(
                            "  {} ... and {} more",
                            Theme::muted(""),
                            Theme::muted(&(result.paths.len() - show_count).to_string())
                        );
                    }
                } else if mode == OutputMode::VeryVerbose {
                    for path in &result.paths {
                        let file_type = crate::utils::detect_file_type(path);
                        let emoji = file_type.emoji();
                        println!("  {} {}", emoji, Theme::muted(&path.display().to_string()));
                    }
                }
            }
        } else if (mode == OutputMode::Verbose || mode == OutputMode::VeryVerbose)
            && !result.paths.is_empty()
        {
            // Show paths for other categories in verbose mode
            let show_count = if mode == OutputMode::Verbose {
                std::cmp::min(3, result.paths.len())
            } else {
                result.paths.len()
            };
            for path in result.paths.iter().take(show_count) {
                let file_type = crate::utils::detect_file_type(path);
                let emoji = file_type.emoji();
                println!("  {} {}", emoji, Theme::muted(&path.display().to_string()));
            }
            if result.paths.len() > show_count && mode == OutputMode::Verbose {
                println!(
                    "  {} ... and {} more",
                    Theme::muted(""),
                    Theme::muted(&(result.paths.len() - show_count).to_string())
                );
            }
        }
    }

    // Calculate totals
    let total_items = results.cache.items
        + results.app_cache.items
        + results.temp.items
        + results.trash.items
        + results.build.items
        + results.downloads.items
        + results.large.items
        + results.old.items
        + results.applications.items
        + results.browser.items
        + results.system.items
        + results.empty.items
        + results.duplicates.items
        + results.windows_update.items
        + results.event_logs.items;
    let total_bytes = results.cache.size_bytes
        + results.app_cache.size_bytes
        + results.temp.size_bytes
        + results.trash.size_bytes
        + results.build.size_bytes
        + results.downloads.size_bytes
        + results.large.size_bytes
        + results.old.size_bytes
        + results.applications.size_bytes
        + results.browser.size_bytes
        + results.system.size_bytes
        + results.empty.size_bytes
        + results.duplicates.size_bytes
        + results.windows_update.size_bytes
        + results.event_logs.size_bytes;

    // Print separator and total
    print_table_separator(&col_widths, "├", "┼", "┤");
    print_table_row(&[
        ("Total".to_string(), col_widths[0]),
        (format_number(total_items as u64), col_widths[1]),
        (bytesize::to_string(total_bytes, false), col_widths[2]),
    ]);
    print_table_separator(&col_widths, "└", "┴", "┘");
    println!();
}

/// Print disk insights in CLI format with progress bars
pub fn print_disk_insights(
    insights: &crate::disk_usage::DiskInsights,
    root_path: &std::path::Path,
    top_n: usize,
    _sort_by: crate::disk_usage::SortBy,
    mode: OutputMode,
) {
    if mode == OutputMode::Quiet {
        return;
    }

    use crate::disk_usage::get_top_folders;

    // Get top folders
    let top_folders = get_top_folders(&insights.root, top_n);

    println!();
    println!(
        "{}  {}  |  Total: {}  |  {} files",
        Theme::header("Disk Insights"),
        Theme::primary(&root_path.display().to_string()),
        Theme::size(&bytesize::to_string(insights.total_size, false)),
        Theme::value(&format_number(insights.total_files))
    );
    println!();

    // Show root with 100% bar
    let root_bar = render_progress_bar(100.0, 20);
    println!(
        "{}  {}  {}  {}",
        Theme::secondary("#"),
        root_bar,
        Theme::value("100.0%"),
        Theme::size(&bytesize::to_string(insights.total_size, false))
    );
    println!("   {}", Theme::muted(&root_path.display().to_string()));
    println!();

    // Show top folders
    for (i, folder) in top_folders.iter().enumerate() {
        let num = i + 1;
        let size_str = bytesize::to_string(folder.size, false);
        let files_str = format_number(folder.file_count);

        // Get display name - use relative path from root if it's deeper than one level
        let display_name = if folder.path != root_path && folder.path.starts_with(root_path) {
            // Show relative path from root (e.g., "OneDrive/Pictures" instead of just "Pictures")
            folder
                .path
                .strip_prefix(root_path)
                .map(|p| {
                    // Remove leading separator and normalize
                    p.to_string_lossy()
                        .replace('\\', "/")
                        .trim_start_matches('/')
                        .to_string()
                })
                .unwrap_or_else(|_| folder.name.clone())
        } else {
            folder.name.clone()
        };

        // Calculate percentage relative to root total (for expanded directories,
        // folder.percentage is relative to its parent, not the root)
        let root_percentage = if insights.total_size > 0 {
            (folder.size as f64 / insights.total_size as f64) * 100.0
        } else {
            0.0
        };
        let bar = render_progress_bar(root_percentage, 20);

        println!(
            "{}  {}  {}  {}  {}  {}",
            Theme::value(&num.to_string()),
            bar,
            Theme::value(&format!("{:.1}%", root_percentage)),
            Theme::size(&size_str),
            Theme::category(&display_name),
            Theme::muted(&format!("({} files)", files_str))
        );
    }

    // Show largest files if available
    if !insights.largest_files.is_empty() {
        println!();
        println!("{}", Theme::divider(60));
        println!();
        println!("{}", Theme::primary("Largest Files:"));
        for (file_path, size) in insights.largest_files.iter().take(5) {
            let relative = crate::utils::to_relative_path(file_path, root_path);
            println!(
                "  {}  {}",
                Theme::size(&bytesize::to_string(*size, false)),
                Theme::muted(&relative)
            );
        }
    }

    println!();
    if mode == OutputMode::Normal || mode == OutputMode::Verbose {
        println!(
            "Run {} to explore interactively.",
            Theme::command("wole analyze --interactive")
        );
    }
    println!();
}

/// Render a progress bar with filled and empty blocks
fn render_progress_bar(percentage: f64, width: usize) -> String {
    let filled = (percentage / 100.0 * width as f64).round() as usize;
    let filled = filled.min(width);
    let empty = width.saturating_sub(filled);
    format!(
        "{}{}",
        Theme::size(&"█".repeat(filled)),
        Theme::muted(&"░".repeat(empty))
    )
}

/// Format number with commas
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
