//! Status screen - Real-time system health dashboard

use crate::status::SystemStatus;
use crate::tui::{
    state::AppState,
    theme::Styles,
    widgets::{
        logo::{render_logo, render_tagline, LOGO_WITH_TAGLINE_HEIGHT},
        shortcuts::{get_shortcuts, render_shortcuts},
    },
};
#[cfg(windows)]
use chrono;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::Color,
    widgets::{Block, Borders, Paragraph},
    Frame,
};

pub fn render(f: &mut Frame, app_state: &AppState) {
    let area = f.area();

    let is_small = area.height < 20 || area.width < 60;
    let shortcuts_height = if is_small { 2 } else { 3 };

    let header_height = LOGO_WITH_TAGLINE_HEIGHT;

    // Ensure we have minimum space
    let min_content_height = 20;
    let min_total_height = header_height + min_content_height + shortcuts_height;

    if area.height < min_total_height || area.width < 60 {
        let msg = Paragraph::new("Terminal too small. Please resize to at least 60x25")
            .style(Styles::warning())
            .alignment(Alignment::Center);
        f.render_widget(msg, area);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(header_height),
            Constraint::Min(min_content_height),
            Constraint::Length(shortcuts_height),
        ])
        .split(area);

    render_header(f, chunks[0], is_small);
    render_content(f, chunks[1], app_state, is_small);

    let shortcuts = get_shortcuts(&app_state.screen, Some(app_state));
    render_shortcuts(f, chunks[2], &shortcuts);
}

fn render_header(f: &mut Frame, area: Rect, _is_small: bool) {
    render_logo(f, area);
    render_tagline(f, area);
}

fn render_content(f: &mut Frame, area: Rect, app_state: &AppState, _is_small: bool) {
    if let crate::tui::state::Screen::Status {
        status,
        last_refresh,
    } = &app_state.screen
    {
        // Header with health score and live indicator
        let header_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2), // Health header (2 lines)
                Constraint::Min(1),    // Main content
            ])
            .split(area);

        render_status_header_with_indicator(f, header_chunks[0], status, last_refresh);

        // Main content area
        render_status_dashboard(f, header_chunks[1], status);
    }
}

fn render_status_header_with_indicator(
    f: &mut Frame,
    area: Rect,
    status: &SystemStatus,
    last_refresh: &std::time::Instant,
) {
    let health_indicator = match status.health_score {
        80..=100 => ("●", Color::Green),
        60..=79 => ("○", Color::Yellow),
        40..=59 => ("◐", Color::Magenta),
        _ => ("◯", Color::Red),
    };

    // Simple live indicator - just show if it's live or not
    let elapsed_ms = last_refresh.elapsed().as_millis();
    let live_indicator = if elapsed_ms < 3000 {
        "● Live"
    } else {
        "○ Updated"
    };

    let header_style = match status.health_score {
        80..=100 => Styles::success(),
        60..=79 => Styles::warning(),
        _ => Styles::error(),
    };

    // Split into two lines
    let lines = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(area);

    // Line 1: Health status with live indicator
    let health_text = format!(
        "Health status: {} {}",
        health_indicator.0, status.health_score
    );
    let health_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(lines[0]);

    let health_para = Paragraph::new(health_text)
        .style(header_style)
        .alignment(Alignment::Left);
    f.render_widget(health_para, health_chunks[0]);

    let live_para = Paragraph::new(live_indicator)
        .style(Styles::success())
        .alignment(Alignment::Right);
    f.render_widget(live_para, health_chunks[1]);

    // Line 2: Device information with uptime
    let uptime_str = format_uptime(status.hardware.uptime_seconds);
    let device_text = format!(
        "{} · {} · {:.1}GB · {} · Uptime: {}",
        status.hardware.device_name,
        status.hardware.cpu_model,
        status.hardware.total_memory_gb,
        status.hardware.os_name,
        uptime_str
    );
    let device_para = Paragraph::new(device_text)
        .style(Styles::secondary())
        .alignment(Alignment::Left);
    f.render_widget(device_para, lines[1]);
}

fn render_status_dashboard(f: &mut Frame, area: Rect, status: &SystemStatus) {
    // LAYOUT HIERARCHY:
    // 1. Primary Metrics: CPU, Memory, Disk (most important, side by side)
    // 2. Secondary Metrics: Network, Power (below primary)
    // 3. Detailed Analysis: Processes, I/O breakdown (bottom)

    let available_height = area.height as i32;
    let available_width = area.width as i32;

    // Minimum width check - if too narrow, show warning
    if available_width < 80 {
        let msg = Paragraph::new("Terminal too narrow. Please resize to at least 80 columns")
            .style(Styles::warning())
            .alignment(Alignment::Center);
        f.render_widget(msg, area);
        return;
    }

    // Fixed heights for sections - ensure they always render
    // These heights are adaptive based on available space
    // CPU section needs enough height for all cores (12 cores = 6 rows in 2-column layout)
    // Disk section now includes breakdown (up to 5 categories), so needs more height
    // So we need: CPU (1 total + 1 info + 6 cores = 8) vs Disk (4 basic + 1 separator + 5 breakdown = 10)
    // Use the maximum: 10 minimum for primary row
    let min_primary_height = 10u16; // CPU/Memory/Disk - enough for all 12 cores + disk breakdown
                                    // Secondary row now has 3 columns: Network, Power, System Diagnostics
                                    // Need enough height for the tallest section (Power or System Diagnostics)
    let min_secondary_height = 8u16; // Network + Power + System Diagnostics

    // Calculate Top Disk I/O section height (must be before processes_height calculation)
    #[cfg(windows)]
    let top_io_height = if !status.top_io_processes.is_empty() {
        (status.top_io_processes.len().min(5) + 2) as u16
    } else {
        0u16
    };
    #[cfg(not(windows))]
    let top_io_height = 0u16;

    let io_spacing = if top_io_height > 0 { 1u16 } else { 0u16 };

    // Calculate reserved space for fixed sections
    let reserved_for_others = min_primary_height as i32
        + 1
        + min_secondary_height as i32
        + 1
        + top_io_height as i32
        + io_spacing as i32;

    // Ensure we have minimum space - if not, reduce processes but keep essential sections
    if available_height < reserved_for_others + 8 {
        // Terminal too small - show minimal layout
        let msg = Paragraph::new("Terminal too small. Please resize to at least 80x30")
            .style(Styles::warning())
            .alignment(Alignment::Center);
        f.render_widget(msg, area);
        return;
    }

    // Use actual calculated heights (may be larger if space allows)
    let primary_metrics_height = min_primary_height;
    let primary_spacing = 1u16;
    let secondary_metrics_height = min_secondary_height;
    let secondary_spacing = 1u16;

    // Maximize processes section - use remaining space after other sections
    let processes_height = (available_height - reserved_for_others).max(8) as u16; // At least 8 lines

    let main_sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(primary_metrics_height), // Primary metrics (CPU/Memory/Disk) - FIXED HEIGHT
            Constraint::Length(primary_spacing),        // Spacing
            Constraint::Length(secondary_metrics_height), // Secondary metrics (Network/Power/System Diagnostics) - FIXED HEIGHT
            Constraint::Length(secondary_spacing),        // Spacing
            Constraint::Length(top_io_height),            // Top Disk I/O (Windows only)
            Constraint::Length(io_spacing),               // Spacing
            Constraint::Min(processes_height), // Processes section - MAXIMIZED to use remaining space
        ])
        .split(area);

    // Layout structure (fixed indices):
    // [0] primary metrics (CPU/Memory/Disk) - ALWAYS HERE with height > 0
    // [1] primary spacing
    // [2] secondary metrics (Network/Power/System Diagnostics) - ALWAYS HERE with height > 0
    // [3] secondary spacing
    // [4] top I/O (height 0 if no I/O)
    // [5] I/O spacing (height 0 if no I/O)
    // [6] processes - ALWAYS HERE

    // Primary metrics: CPU, Memory, Disk (side by side) - ALWAYS at index 0
    if !main_sections.is_empty() && main_sections[0].height > 0 {
        let primary_area = main_sections[0];

        // Ensure minimum width per column (at least 20 chars each)
        let min_col_width = 20u16;
        let total_min_width = min_col_width * 3 + 2; // 3 columns + 2 spacers

        if primary_area.width >= total_min_width {
            // Enough space - use percentage-based layout
            let primary_cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(34), // CPU
                    Constraint::Length(1),      // Spacing
                    Constraint::Percentage(33), // Memory
                    Constraint::Length(1),      // Spacing
                    Constraint::Percentage(33), // Disk
                ])
                .split(primary_area);

            render_cpu_section_compact(f, primary_cols[0], status);
            render_memory_section_compact(f, primary_cols[2], status);
            render_disk_section_compact(f, primary_cols[4], status);
        } else {
            // Too narrow - stack vertically instead
            let stacked = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(primary_area.height / 3),
                    Constraint::Length(1),
                    Constraint::Length(primary_area.height / 3),
                    Constraint::Length(1),
                    Constraint::Min(1),
                ])
                .split(primary_area);

            render_cpu_section_compact(f, stacked[0], status);
            render_memory_section_compact(f, stacked[2], status);
            render_disk_section_compact(f, stacked[4], status);
        }
    }

    // Secondary metrics: Network, Power, GPU (if available), and System Diagnostics - ALWAYS at index 2
    if main_sections.len() > 2 && main_sections[2].height > 0 {
        let secondary_area = main_sections[2];

        // GPU implementation commented out - not polished yet
        let has_gpu = false; // status.gpu.is_some();
        #[cfg(windows)]
        let has_boot_info = status.boot_info.is_some();
        #[cfg(not(windows))]
        let has_boot_info = false;

        // Check if we have temperature sensors to show
        let has_temp_sensors = !status.temperature_sensors.is_empty();

        // Determine layout: 5 columns if all available, 4 if GPU+boot+temp, otherwise 3
        let use_5_cols = has_gpu && has_boot_info && has_temp_sensors;
        let use_4_cols =
            (has_temp_sensors || has_boot_info) && has_gpu || (has_boot_info && has_temp_sensors);
        let min_col_width_5 = 18u16; // Smaller width for 5 columns
        let min_col_width_4 = 20u16;
        let min_col_width_3 = 25u16;

        let total_min_width_5 = min_col_width_5 * 5 + 4; // 5 columns + 4 spacers
        let total_min_width_4 = min_col_width_4 * 4 + 3; // 4 columns + 3 spacers
        let total_min_width_3 = min_col_width_3 * 3 + 2; // 3 columns + 2 spacers

        if secondary_area.width >= total_min_width_5 && use_5_cols {
            // 5-column layout: Network, Power, GPU, Temperature, System Diagnostics
            let secondary_cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(20), // Network
                    Constraint::Length(1),      // Spacing
                    Constraint::Percentage(20), // Power
                    Constraint::Length(1),      // Spacing
                    Constraint::Percentage(20), // GPU
                    Constraint::Length(1),      // Spacing
                    Constraint::Percentage(20), // Temperature
                    Constraint::Length(1),      // Spacing
                    Constraint::Percentage(20), // System Diagnostics
                ])
                .split(secondary_area);

            render_network_section(f, secondary_cols[0], status);
            render_power_section_compact(f, secondary_cols[2], status);
            // GPU implementation commented out - not polished yet
            // if let Some(ref gpu) = status.gpu {
            //     render_gpu_section(f, secondary_cols[4], gpu);
            // }
            if has_temp_sensors {
                render_temperature_sensors_section(
                    f,
                    secondary_cols[6],
                    &status.temperature_sensors,
                );
            }
            #[cfg(windows)]
            {
                if let Some(ref boot_info) = status.boot_info {
                    render_boot_info_section(f, secondary_cols[8], boot_info);
                }
            }
        } else if secondary_area.width >= total_min_width_4 && use_4_cols {
            // 4-column layout: Network, Power, GPU, and one of Temperature/System Diagnostics
            let secondary_cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(25), // Network
                    Constraint::Length(1),      // Spacing
                    Constraint::Percentage(25), // Power
                    Constraint::Length(1),      // Spacing
                    Constraint::Percentage(25), // GPU
                    Constraint::Length(1),      // Spacing
                    Constraint::Percentage(25), // Temperature or System Diagnostics
                ])
                .split(secondary_area);

            render_network_section(f, secondary_cols[0], status);
            render_power_section_compact(f, secondary_cols[2], status);
            // GPU implementation commented out - not polished yet
            // if let Some(ref gpu) = status.gpu {
            //     render_gpu_section(f, secondary_cols[4], gpu);
            // }

            // Show Temperature Sensors if available, otherwise System Diagnostics
            if has_temp_sensors {
                render_temperature_sensors_section(
                    f,
                    secondary_cols[6],
                    &status.temperature_sensors,
                );
            } else {
                #[cfg(windows)]
                {
                    if let Some(ref boot_info) = status.boot_info {
                        render_boot_info_section(f, secondary_cols[6], boot_info);
                    }
                }
            }
        } else if secondary_area.width >= total_min_width_3 {
            // 3-column layout: Network, Power, GPU/System Diagnostics/Temperature
            let secondary_cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(34), // Network
                    Constraint::Length(1),      // Spacing
                    Constraint::Percentage(33), // Power
                    Constraint::Length(1),      // Spacing
                    Constraint::Percentage(33), // GPU, System Diagnostics, or Temperature
                ])
                .split(secondary_area);

            render_network_section(f, secondary_cols[0], status);
            render_power_section_compact(f, secondary_cols[2], status);

            // Priority: GPU > Temperature Sensors > System Diagnostics
            // GPU implementation commented out - not polished yet
            // if let Some(ref gpu) = status.gpu {
            //     render_gpu_section(f, secondary_cols[4], gpu);
            // } else if has_temp_sensors {
            if has_temp_sensors {
                render_temperature_sensors_section(
                    f,
                    secondary_cols[4],
                    &status.temperature_sensors,
                );
            } else {
                #[cfg(windows)]
                {
                    if let Some(ref boot_info) = status.boot_info {
                        render_boot_info_section(f, secondary_cols[4], boot_info);
                    }
                }
                #[cfg(not(windows))]
                {
                    // Non-Windows: leave empty or show placeholder
                }
            }
        } else {
            // Too narrow - stack vertically
            // Always show Network and Power, plus GPU, Temperature Sensors, and Boot Info if available
            let num_sections = 2
                + (if has_gpu { 1 } else { 0 })
                + (if has_temp_sensors { 1 } else { 0 })
                + (if has_boot_info { 1 } else { 0 });
            let section_height = secondary_area.height / num_sections.max(1) as u16;
            let mut constraints = Vec::new();

            for i in 0..num_sections {
                if i > 0 {
                    constraints.push(Constraint::Length(1)); // Spacing
                }
                constraints.push(Constraint::Length(section_height));
            }

            let stacked = Layout::default()
                .direction(Direction::Vertical)
                .constraints(constraints)
                .split(secondary_area);

            let mut idx = 0;
            render_network_section(f, stacked[idx], status);
            idx += 2; // Skip spacing

            render_power_section_compact(f, stacked[idx], status);
            idx += 2; // Skip spacing

            // GPU implementation commented out - not polished yet
            // if let Some(ref gpu) = status.gpu {
            //     if idx < stacked.len() {
            //         render_gpu_section(f, stacked[idx], gpu);
            //         idx += 2; // Skip spacing
            //     }
            // }

            if has_temp_sensors && idx < stacked.len() {
                render_temperature_sensors_section(f, stacked[idx], &status.temperature_sensors);
                idx += 2; // Skip spacing
            }

            #[cfg(windows)]
            {
                if let Some(ref boot_info) = status.boot_info {
                    if idx < stacked.len() {
                        render_boot_info_section(f, stacked[idx], boot_info);
                    }
                }
            }
        }
    }

    // Top Disk I/O section (Windows only) - at index 4
    #[cfg(windows)]
    {
        if top_io_height > 0 && main_sections.len() > 4 && main_sections[4].height > 0 {
            render_top_io_section(f, main_sections[4], status);
        }
    }

    // Processes section - at index 6
    let process_idx = 6;
    if main_sections.len() > process_idx && main_sections[process_idx].height > 0 {
        render_processes_section(f, main_sections[process_idx], status);
    } else {
        // Fallback: try to render processes even if layout calculation was wrong
        // This ensures processes are always visible
        if main_sections.len() > 4 {
            render_processes_section(f, main_sections[main_sections.len() - 1], status);
        }
    }
}

fn render_cpu_section_compact(f: &mut Frame, area: Rect, status: &SystemStatus) {
    // Compact CPU section - shows only essential info
    let cpu_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Styles::border())
        .title("⚡ CPU");

    let inner = cpu_block.inner(area);
    f.render_widget(cpu_block, area);

    // Calculate how many rows we need for cores (2-column layout)
    // For 12 cores: (12 + 1) / 2 = 6.5 -> 6 rows needed
    let total_cores = status.cpu.cores.len();
    let cores_rows = total_cores.div_ceil(2).max(1);

    let lines = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),              // Total usage
            Constraint::Length(1),              // Processor info
            Constraint::Min(cores_rows as u16), // Cores (ensure enough rows for all cores)
        ])
        .split(inner);

    // Total CPU usage - prominent
    let total_bar = create_progress_bar(status.cpu.total_usage / 100.0, 15);
    let total_text = format!("Total {} {:.1}%", total_bar, status.cpu.total_usage);
    let total_para = Paragraph::new(total_text).style(Styles::primary());
    f.render_widget(total_para, lines[0]);

    // Processor info - compact (show CPU name/model)
    // Calculate available width dynamically based on actual area width
    let proc_text = if let Some(freq_mhz) = status.cpu.frequency_mhz {
        // Calculate fixed parts: " · " + frequency (e.g., "2.1 GHz") + " · " + cores (e.g., "12 cores")
        let freq_str = format!("{:.1} GHz", freq_mhz as f64 / 1000.0);
        let cores_str = format!("{} cores", status.cpu.cores.len());
        let fixed_parts_len = 3 + freq_str.len() + 3 + cores_str.len(); // " · " + freq + " · " + cores

        // Available width for brand: area width minus fixed parts
        let available_width = lines[1].width.saturating_sub(fixed_parts_len as u16);

        // Truncate CPU brand only if it exceeds available width
        let brand = if status.cpu.brand.len() > available_width as usize {
            let truncate_len = available_width.saturating_sub(1) as usize; // Reserve 1 char for ellipsis
            format!("{}…", &status.cpu.brand[..truncate_len])
        } else {
            status.cpu.brand.clone()
        };
        format!("{} · {} · {}", brand, freq_str, cores_str)
    } else {
        // Calculate fixed parts: " · " + cores (e.g., "12 cores")
        let cores_str = format!("{} cores", status.cpu.cores.len());
        let fixed_parts_len = 3 + cores_str.len(); // " · " + cores

        // Available width for brand: area width minus fixed parts
        let available_width = lines[1].width.saturating_sub(fixed_parts_len as u16);

        // Truncate CPU brand only if it exceeds available width
        let brand = if status.cpu.brand.len() > available_width as usize {
            let truncate_len = available_width.saturating_sub(1) as usize; // Reserve 1 char for ellipsis
            format!("{}…", &status.cpu.brand[..truncate_len])
        } else {
            status.cpu.brand.clone()
        };
        format!("{} · {}", brand, cores_str)
    };
    let proc_para = Paragraph::new(proc_text).style(Styles::secondary());
    f.render_widget(proc_para, lines[1]);

    // Cores - compact 2-column layout (show ALL cores)
    let cores_area = lines[2];
    if cores_area.height >= 2 {
        let core_cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(cores_area);

        let total_cores = status.cpu.cores.len();
        let cores_per_col = total_cores.div_ceil(2); // Split cores evenly between columns (12 cores = 6 per column)
        let available_rows = cores_area.height as usize;

        // Show ALL cores - render up to cores_per_col rows (which covers all cores in 2 columns)
        let rows_to_render = available_rows.min(cores_per_col);

        for row in 0..rows_to_render {
            // Left column - cores 0 to cores_per_col-1
            if let Some(core) = status.cpu.cores.get(row) {
                let core_bar = create_mini_bar(core.usage / 100.0, 5);
                let core_text = format!("C{:2} {} {:4.1}%", core.id + 1, core_bar, core.usage);
                let core_para = Paragraph::new(core_text).style(Styles::secondary());
                let core_area = Rect {
                    x: core_cols[0].x,
                    y: core_cols[0].y + row as u16,
                    width: core_cols[0].width,
                    height: 1,
                };
                f.render_widget(core_para, core_area);
            }

            // Right column - cores cores_per_col to total_cores-1
            if let Some(core) = status.cpu.cores.get(row + cores_per_col) {
                let core_bar = create_mini_bar(core.usage / 100.0, 5);
                let core_text = format!("C{:2} {} {:4.1}%", core.id + 1, core_bar, core.usage);
                let core_para = Paragraph::new(core_text).style(Styles::secondary());
                let core_area = Rect {
                    x: core_cols[1].x,
                    y: core_cols[1].y + row as u16,
                    width: core_cols[1].width,
                    height: 1,
                };
                f.render_widget(core_para, core_area);
            }
        }

        // If we couldn't show all cores due to space constraints, show a message
        if rows_to_render < cores_per_col && cores_area.height > 0 {
            let cores_shown = rows_to_render * 2;
            let remaining = total_cores - cores_shown;
            if remaining > 0 {
                let msg_text = format!("... {} more cores", remaining);
                let msg_area = Rect {
                    x: cores_area.x,
                    y: cores_area.y + cores_area.height - 1,
                    width: cores_area.width,
                    height: 1,
                };
                let msg_para = Paragraph::new(msg_text).style(Styles::secondary());
                f.render_widget(msg_para, msg_area);
            }
        }
    }
}

#[allow(dead_code)] // Kept for potential future use or debugging
fn render_cpu_section(f: &mut Frame, area: Rect, status: &SystemStatus) {
    let cpu_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Styles::border())
        .title("⚙ CPU");

    let inner = cpu_block.inner(area);
    f.render_widget(cpu_block, area);

    // On Windows, skip the Load line since load averages aren't meaningful
    let show_load = !cfg!(windows);

    let mut constraints = vec![
        Constraint::Length(1), // Total
    ];
    if show_load {
        constraints.push(Constraint::Length(1)); // Load (only on Unix)
    }
    constraints.extend([
        Constraint::Length(1), // Processor brand
        Constraint::Length(1), // Frequency/Vendor
        Constraint::Length(1), // Processes
        Constraint::Length(1), // Spacing
        Constraint::Min(1),    // Cores
    ]);

    let lines = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    // Total CPU usage
    let total_bar = create_progress_bar(status.cpu.total_usage / 100.0, 20);
    let total_text = format!("Total   {}  {:.1}%", total_bar, status.cpu.total_usage);
    let total_para = Paragraph::new(total_text).style(Styles::primary());
    f.render_widget(total_para, lines[0]);

    // Load averages (only on Unix systems)
    let mut line_idx = 1;
    if show_load {
        let load_text = format!(
            "Load    {:.2} / {:.2} / {:.2} ({} cores)",
            status.cpu.load_avg_1min,
            status.cpu.load_avg_5min,
            status.cpu.load_avg_15min,
            status.cpu.cores.len()
        );
        let load_para = Paragraph::new(load_text).style(Styles::secondary());
        f.render_widget(load_para, lines[line_idx]);
        line_idx += 1;
    }

    // Processor brand
    let brand_text = format!("Proc    {}", status.cpu.brand);
    let brand_para = Paragraph::new(brand_text).style(Styles::secondary());
    f.render_widget(brand_para, lines[line_idx]);
    line_idx += 1;

    // Frequency and vendor info
    let freq_text = if let Some(freq_mhz) = status.cpu.frequency_mhz {
        let freq_ghz = freq_mhz as f64 / 1000.0;
        format!("Freq    {:.2} GHz · {}", freq_ghz, status.cpu.vendor_id)
    } else {
        format!("Vendor  {}", status.cpu.vendor_id)
    };
    let freq_para = Paragraph::new(freq_text).style(Styles::secondary());
    f.render_widget(freq_para, lines[line_idx]);
    line_idx += 1;

    // Process count
    let proc_text = format!("Procs   {}", status.cpu.process_count);
    let proc_para = Paragraph::new(proc_text).style(Styles::secondary());
    f.render_widget(proc_para, lines[line_idx]);
    line_idx += 1;

    // Spacing line
    line_idx += 1;

    // Show cores in a 2-column layout to maximize space usage
    let cores_area = lines[line_idx];

    // Split cores area into two columns
    let core_columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(cores_area);

    // Calculate how many cores per column (half rounded up)
    // Limit to max cores that fit in available space
    let max_cores = status.cpu.cores.len().min(12);
    let cores_per_column = max_cores.div_ceil(2);
    let visible_rows = cores_area.height as usize;
    let rows_to_show = visible_rows.min(cores_per_column);

    // Render cores in two columns
    for row in 0..rows_to_show {
        // Left column
        if let Some(core) = status.cpu.cores.get(row) {
            let core_bar = create_progress_bar(core.usage / 100.0, 9); // Compact bar for 2-column layout
            let core_text = format!("Core {:2}  {}  {:4.1}%", core.id + 1, core_bar, core.usage);
            let core_para = Paragraph::new(core_text).style(Styles::secondary());
            let core_line_area = Rect {
                x: core_columns[0].x,
                y: core_columns[0].y + row as u16,
                width: core_columns[0].width,
                height: 1,
            };
            f.render_widget(core_para, core_line_area);
        }

        // Right column
        if let Some(core) = status.cpu.cores.get(row + cores_per_column) {
            let core_bar = create_progress_bar(core.usage / 100.0, 9); // Compact bar for 2-column layout
            let core_text = format!("Core {:2}  {}  {:4.1}%", core.id + 1, core_bar, core.usage);
            let core_para = Paragraph::new(core_text).style(Styles::secondary());
            let core_line_area = Rect {
                x: core_columns[1].x,
                y: core_columns[1].y + row as u16,
                width: core_columns[1].width,
                height: 1,
            };
            f.render_widget(core_para, core_line_area);
        }
    }
}

fn render_memory_section_compact(f: &mut Frame, area: Rect, status: &SystemStatus) {
    // Compact Memory section - shows only essential info
    let mem_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Styles::border())
        .title("💾 Memory");

    let inner = mem_block.inner(area);
    f.render_widget(mem_block, area);

    let lines = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Used
            Constraint::Length(1), // Total/Free
            Constraint::Length(1), // Swap (if applicable)
        ])
        .split(inner);

    // Used memory - prominent with bar
    let used_bar = create_progress_bar(status.memory.used_percent / 100.0, 15);
    let used_text = format!("Used {} {:.1}%", used_bar, status.memory.used_percent);
    let used_style = if status.memory.used_percent > 90.0 {
        Styles::error()
    } else if status.memory.used_percent > 75.0 {
        Styles::warning()
    } else {
        Styles::primary()
    };
    let used_para = Paragraph::new(used_text).style(used_style);
    f.render_widget(used_para, lines[0]);

    // Total/Free - compact
    let total_text = format!(
        "{:.1} / {:.1} GB · Free {:.1} GB",
        status.memory.used_gb, status.memory.total_gb, status.memory.free_gb
    );
    let total_para = Paragraph::new(total_text).style(Styles::secondary());
    f.render_widget(total_para, lines[1]);

    // Swap - compact (show GB amounts, not just percentage)
    if status.memory.swap_total_gb > 0.0 {
        let swap_bar = create_mini_bar(status.memory.swap_percent / 100.0, 8);
        let swap_text = format!(
            "Swap {} {:.1}% ({:.1} / {:.1} GB)",
            swap_bar,
            status.memory.swap_percent,
            status.memory.swap_used_gb,
            status.memory.swap_total_gb
        );
        let swap_style = if status.memory.swap_percent > 80.0 {
            Styles::error()
        } else if status.memory.swap_percent > 50.0 {
            Styles::warning()
        } else {
            Styles::secondary()
        };
        let swap_para = Paragraph::new(swap_text).style(swap_style);
        f.render_widget(swap_para, lines[2]);
    }
}

#[allow(dead_code)] // Kept for potential future use or debugging
fn render_memory_section(f: &mut Frame, area: Rect, status: &SystemStatus) {
    let mem_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Styles::border())
        .title("▦ Memory");

    let inner = mem_block.inner(area);
    f.render_widget(mem_block, area);

    let lines = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Used
            Constraint::Length(1), // Total
            Constraint::Length(1), // Free/Available (combined)
            Constraint::Length(1), // Swap
        ])
        .split(inner);

    // Used memory
    let used_bar = create_progress_bar(status.memory.used_percent / 100.0, 20);
    let used_text = format!("Used    {}  {:.1}%", used_bar, status.memory.used_percent);
    let used_para = Paragraph::new(used_text).style(Styles::primary());
    f.render_widget(used_para, lines[0]);

    // Total memory
    let total_text = format!(
        "Total   {:.1} / {:.1} GB",
        status.memory.used_gb, status.memory.total_gb
    );
    let total_para = Paragraph::new(total_text).style(Styles::secondary());
    f.render_widget(total_para, lines[1]);

    // Free/Available memory (combined since they're usually the same)
    let free_text = format!("Free    {:.1} GB", status.memory.free_gb);
    let free_para = Paragraph::new(free_text).style(Styles::secondary());
    f.render_widget(free_para, lines[2]);

    // Swap/Page file memory
    if status.memory.swap_total_gb > 0.0 {
        let swap_bar = create_progress_bar(status.memory.swap_percent / 100.0, 20);
        let swap_text = format!(
            "Swap    {}  {:.1}% ({:.1} / {:.1} GB)",
            swap_bar,
            status.memory.swap_percent,
            status.memory.swap_used_gb,
            status.memory.swap_total_gb
        );
        let swap_para = Paragraph::new(swap_text).style(Styles::secondary());
        f.render_widget(swap_para, lines[3]);
    }
}

fn render_disk_section_compact(f: &mut Frame, area: Rect, status: &SystemStatus) {
    // Compact Disk section - shows essential info on left, breakdown on right
    let disk_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Styles::border())
        .title("💿 Disk");

    let inner = disk_block.inner(area);
    f.render_widget(disk_block, area);

    #[cfg(windows)]
    let show_breakdown = true; // Always show breakdown column on Windows
    #[cfg(not(windows))]
    let show_breakdown = false;

    // Split into two columns: left for basic info, right for breakdown
    // Always show two columns on Windows (even if narrow, we'll make it work)
    if show_breakdown {
        // Two-column layout: Basic info | Breakdown
        // Adjust column widths based on available space
        let left_pct = if inner.width >= 50 { 50 } else { 45 };
        let right_pct = if inner.width >= 50 { 50 } else { 55 };

        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(left_pct),  // Left: Basic disk info
                Constraint::Length(1),             // Spacing
                Constraint::Percentage(right_pct), // Right: Breakdown
            ])
            .split(inner);

        // Left column: Basic disk info + volumes list
        // Calculate space needed for volumes (header + up to 3 volumes)
        let volumes_to_show = status.disks.len().min(3);
        let volumes_height = if !status.disks.is_empty() {
            1 + volumes_to_show // Header + volumes
        } else {
            0
        };

        let mut left_constraints = vec![
            Constraint::Length(1), // Used
            Constraint::Length(1), // Free
            Constraint::Length(1), // Read
            Constraint::Length(1), // Write
        ];

        if volumes_height > 0 {
            left_constraints.push(Constraint::Length(volumes_height as u16)); // Volumes section
        }

        let left_lines = Layout::default()
            .direction(Direction::Vertical)
            .constraints(left_constraints)
            .split(cols[0]);

        // Used disk - prominent with bar
        let used_bar = create_progress_bar(status.disk.used_percent / 100.0, 12);
        let used_text = format!("Used {} {:.1}%", used_bar, status.disk.used_percent);
        let used_style = if status.disk.used_percent > 95.0 {
            Styles::error()
        } else if status.disk.used_percent > 85.0 {
            Styles::warning()
        } else {
            Styles::primary()
        };
        let used_para = Paragraph::new(used_text).style(used_style);
        f.render_widget(used_para, left_lines[0]);

        // Free disk - compact
        let free_text = format!(
            "Free {:.1} / {:.1} GB",
            status.disk.free_gb, status.disk.total_gb
        );
        let free_para = Paragraph::new(free_text).style(Styles::secondary());
        f.render_widget(free_para, left_lines[1]);

        // Read speed - compact
        let (read_val, read_unit) = if status.disk.read_speed_mb < 1.0 {
            (status.disk.read_speed_mb * 1000.0, "Kbps")
        } else {
            (status.disk.read_speed_mb, "MB/s")
        };
        let read_bar = create_mini_bar((status.disk.read_speed_mb / 10.0) as f32, 6);
        let read_text = format!("Read {} {:.1} {}", read_bar, read_val, read_unit);
        let read_para = Paragraph::new(read_text).style(Styles::secondary());
        f.render_widget(read_para, left_lines[2]);

        // Write speed - compact
        let (write_val, write_unit) = if status.disk.write_speed_mb < 1.0 {
            (status.disk.write_speed_mb * 1000.0, "Kbps")
        } else {
            (status.disk.write_speed_mb, "MB/s")
        };
        let write_bar = create_mini_bar((status.disk.write_speed_mb / 10.0) as f32, 6);
        let write_text = format!("Write {} {:.1} {}", write_bar, write_val, write_unit);
        let write_para = Paragraph::new(write_text).style(Styles::secondary());
        f.render_widget(write_para, left_lines[3]);

        // Volumes list (one per line)
        if !status.disks.is_empty() && left_lines.len() > 4 {
            let volumes_area = left_lines[4];
            let volumes_lines = Layout::default()
                .direction(Direction::Vertical)
                .constraints(
                    std::iter::once(Constraint::Length(1)) // Header
                        .chain(std::iter::repeat_n(Constraint::Length(1), volumes_to_show)) // Each volume
                        .collect::<Vec<_>>(),
                )
                .split(volumes_area);

            // Header
            let volumes_header = Paragraph::new("Volumes:")
                .style(Styles::primary())
                .alignment(Alignment::Left);
            f.render_widget(volumes_header, volumes_lines[0]);

            // List volumes
            for (i, disk) in status.disks.iter().take(volumes_to_show).enumerate() {
                if i + 1 < volumes_lines.len() {
                    // Format: "[Type] C:\ x/x (%)"
                    let type_indicator = if disk.is_removable {
                        "[USB]"
                    } else if disk.disk_type == "SSD" {
                        "[SSD]"
                    } else if disk.disk_type == "HDD" {
                        "[HDD]"
                    } else {
                        "[EXT]" // Unknown type is likely virtual
                    };

                    let volume_text = format!(
                        "{} {}  {:.1}/{:.1} GB ({:.1}%)",
                        type_indicator,
                        disk.mount_point,
                        disk.used_gb,
                        disk.total_gb,
                        disk.used_percent
                    );

                    // Style based on usage (remove underline - use bold instead for warnings)
                    let volume_style = if disk.used_percent > 85.0 {
                        Styles::emphasis() // Bold only, no underline
                    } else {
                        Styles::secondary()
                    };

                    let volume_para = Paragraph::new(volume_text)
                        .style(volume_style)
                        .alignment(Alignment::Left);
                    f.render_widget(volume_para, volumes_lines[i + 1]);
                }
            }
        }

        // Right column: Disk breakdown (always show on Windows)
        #[cfg(windows)]
        {
            let right_lines = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1), // Header
                    Constraint::Length(1), // Category 1
                    Constraint::Length(1), // Category 2
                    Constraint::Length(1), // Category 3
                    Constraint::Length(1), // Category 4
                    Constraint::Length(1), // Category 5
                    Constraint::Length(1), // Total line
                ])
                .split(cols[2]);

            // Header - always show with volume info
            let header_text = if let Some(ref breakdown) = status.disk_breakdown {
                format!("Breakdown ({}):", breakdown.volume)
            } else {
                "Breakdown:".to_string()
            };
            let header_para = Paragraph::new(header_text)
                .style(Styles::primary())
                .alignment(Alignment::Left);
            f.render_widget(header_para, right_lines[0]);

            if let Some(ref breakdown) = status.disk_breakdown {
                if !breakdown.categories.is_empty() {
                    // Categories (up to 5)
                    let categories_to_show = breakdown.categories.len().min(5);
                    for (i, category) in breakdown
                        .categories
                        .iter()
                        .take(categories_to_show)
                        .enumerate()
                    {
                        if i + 1 < right_lines.len() {
                            let bar_width: usize = 8; // Compact bar for right column
                            let filled =
                                (category.percent / 100.0 * bar_width as f32).round() as usize;
                            let empty = bar_width.saturating_sub(filled);
                            let bar = "█".repeat(filled) + &"░".repeat(empty);

                            // Truncate category name if needed
                            let name = if category.name.len() > 6 {
                                format!("{}…", &category.name[..5])
                            } else {
                                category.name.clone()
                            };

                            // Format: "Name  XXX.X GB (XX.X%) ████░░░░"
                            let category_text = format!(
                                "{:<7} {:>6.1} GB ({:>4.1}%) {}",
                                name, category.size_gb, category.percent, bar
                            );
                            let category_para = Paragraph::new(category_text)
                                .style(Styles::secondary())
                                .alignment(Alignment::Left);
                            f.render_widget(category_para, right_lines[i + 1]);
                        }
                    }

                    // Show total at the bottom if we have space
                    if categories_to_show + 1 < right_lines.len() {
                        let total_used =
                            breakdown.categories.iter().map(|c| c.size_gb).sum::<f64>();
                        let total_text = format!("Total: {:.1} GB", total_used);
                        let total_para = Paragraph::new(total_text)
                            .style(Styles::primary())
                            .alignment(Alignment::Left);
                        f.render_widget(total_para, right_lines[categories_to_show + 1]);
                    }
                } else {
                    // No categories yet
                    let msg = Paragraph::new("Calculating...")
                        .style(Styles::secondary())
                        .alignment(Alignment::Left);
                    f.render_widget(msg, right_lines[1]);
                }
            } else {
                // Breakdown not available yet - show placeholder
                let msg = Paragraph::new("Calculating...\n(Press 'd')")
                    .style(Styles::secondary())
                    .alignment(Alignment::Left);
                f.render_widget(msg, right_lines[1]);
            }
        }
    } else {
        // Single column layout (no breakdown or not enough space)
        let volumes_to_show = status.disks.len().min(3);
        let volumes_height = if !status.disks.is_empty() {
            1 + volumes_to_show // Header + volumes
        } else {
            0
        };

        let mut constraints = vec![
            Constraint::Length(1), // Used
            Constraint::Length(1), // Free
            Constraint::Length(1), // Read
            Constraint::Length(1), // Write
        ];

        if volumes_height > 0 {
            constraints.push(Constraint::Length(volumes_height as u16)); // Volumes section
        }

        let lines = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(inner);

        // Used disk - prominent with bar
        let used_bar = create_progress_bar(status.disk.used_percent / 100.0, 15);
        let used_text = format!("Used {} {:.1}%", used_bar, status.disk.used_percent);
        let used_style = if status.disk.used_percent > 95.0 {
            Styles::error()
        } else if status.disk.used_percent > 85.0 {
            Styles::warning()
        } else {
            Styles::primary()
        };
        let used_para = Paragraph::new(used_text).style(used_style);
        f.render_widget(used_para, lines[0]);

        // Free disk - compact
        let free_text = format!(
            "Free {:.1} / {:.1} GB",
            status.disk.free_gb, status.disk.total_gb
        );
        let free_para = Paragraph::new(free_text).style(Styles::secondary());
        f.render_widget(free_para, lines[1]);

        // Read speed - compact
        let (read_val, read_unit) = if status.disk.read_speed_mb < 1.0 {
            (status.disk.read_speed_mb * 1000.0, "Kbps")
        } else {
            (status.disk.read_speed_mb, "MB/s")
        };
        let read_bar = create_mini_bar((status.disk.read_speed_mb / 10.0) as f32, 6);
        let read_text = format!("Read {} {:.1} {}", read_bar, read_val, read_unit);
        let read_para = Paragraph::new(read_text).style(Styles::secondary());
        f.render_widget(read_para, lines[2]);

        // Write speed - compact
        let (write_val, write_unit) = if status.disk.write_speed_mb < 1.0 {
            (status.disk.write_speed_mb * 1000.0, "Kbps")
        } else {
            (status.disk.write_speed_mb, "MB/s")
        };
        let write_bar = create_mini_bar((status.disk.write_speed_mb / 10.0) as f32, 6);
        let write_text = format!("Write {} {:.1} {}", write_bar, write_val, write_unit);
        let write_para = Paragraph::new(write_text).style(Styles::secondary());
        f.render_widget(write_para, lines[3]);

        // Volumes list (one per line)
        if !status.disks.is_empty() && lines.len() > 4 {
            let volumes_area = lines[4];
            let volumes_lines = Layout::default()
                .direction(Direction::Vertical)
                .constraints(
                    std::iter::once(Constraint::Length(1)) // Header
                        .chain(std::iter::repeat_n(Constraint::Length(1), volumes_to_show)) // Each volume
                        .collect::<Vec<_>>(),
                )
                .split(volumes_area);

            // Header
            let volumes_header = Paragraph::new("Volumes:")
                .style(Styles::primary())
                .alignment(Alignment::Left);
            f.render_widget(volumes_header, volumes_lines[0]);

            // List volumes
            for (i, disk) in status.disks.iter().take(volumes_to_show).enumerate() {
                if i + 1 < volumes_lines.len() {
                    // Format: "[Type] C:\ x/x (%)"
                    let type_indicator = if disk.is_removable {
                        "[USB]"
                    } else if disk.disk_type == "SSD" {
                        "[SSD]"
                    } else if disk.disk_type == "HDD" {
                        "[HDD]"
                    } else {
                        "[Virtual]" // Unknown type is likely virtual
                    };

                    let volume_text = format!(
                        "{} {}  {:.1}/{:.1} GB ({:.1}%)",
                        type_indicator,
                        disk.mount_point,
                        disk.used_gb,
                        disk.total_gb,
                        disk.used_percent
                    );

                    // Style based on usage (remove underline - use bold instead for warnings)
                    let volume_style = if disk.used_percent > 85.0 {
                        Styles::emphasis() // Bold only, no underline
                    } else {
                        Styles::secondary()
                    };

                    let volume_para = Paragraph::new(volume_text)
                        .style(volume_style)
                        .alignment(Alignment::Left);
                    f.render_widget(volume_para, volumes_lines[i + 1]);
                }
            }
        }
    }
}

#[allow(dead_code)] // Kept for potential future use or debugging
fn render_disk_section(f: &mut Frame, area: Rect, status: &SystemStatus) {
    let disk_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Styles::border())
        .title("▤ Disk");

    let inner = disk_block.inner(area);
    f.render_widget(disk_block, area);

    // Always show read/write lines
    let lines = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Used
            Constraint::Length(1), // Free
            Constraint::Length(1), // Read
            Constraint::Length(1), // Write
        ])
        .split(inner);

    // Used disk - just percentage
    let used_text = format!("Used    {:.1}%", status.disk.used_percent);
    let used_para = Paragraph::new(used_text).style(Styles::primary());
    f.render_widget(used_para, lines[0]);

    // Free disk - show "X.X GB / Y.Y GB" format
    let free_text = format!(
        "Free    {:.1} GB / {:.1} GB",
        status.disk.free_gb, status.disk.total_gb
    );
    let free_para = Paragraph::new(free_text).style(Styles::secondary());
    f.render_widget(free_para, lines[1]);

    // Read speed - use Kbps if < 1 MB/s, otherwise MB/s
    let (read_value, read_unit, read_bar_max) = if status.disk.read_speed_mb < 1.0 {
        let kbps = status.disk.read_speed_mb * 1000.0;
        (kbps, "Kbps", 100.0) // Scale bar for Kbps (0-100 Kbps)
    } else {
        (status.disk.read_speed_mb, "MB/s", 10.0) // Scale bar for MB/s (0-10 MB/s)
    };
    let read_bar = create_speed_bar(status.disk.read_speed_mb / read_bar_max, 5);
    let read_text = format!("Read    {}  {:.1} {}", read_bar, read_value, read_unit);
    let read_para = Paragraph::new(read_text).style(Styles::secondary());
    f.render_widget(read_para, lines[2]);

    // Write speed - use Kbps if < 1 MB/s, otherwise MB/s
    let (write_value, write_unit, write_bar_max) = if status.disk.write_speed_mb < 1.0 {
        let kbps = status.disk.write_speed_mb * 1000.0;
        (kbps, "Kbps", 100.0) // Scale bar for Kbps (0-100 Kbps)
    } else {
        (status.disk.write_speed_mb, "MB/s", 10.0) // Scale bar for MB/s (0-10 MB/s)
    };
    let write_bar = create_speed_bar(status.disk.write_speed_mb / write_bar_max, 5);
    let write_text = format!("Write   {}  {:.1} {}", write_bar, write_value, write_unit);
    let write_para = Paragraph::new(write_text).style(Styles::secondary());
    f.render_widget(write_para, lines[3]);
}

fn render_power_section_compact(f: &mut Frame, area: Rect, status: &SystemStatus) {
    // Power section with all details - Level spans full width, then 2-column layout below
    let power_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Styles::border())
        .title("🔋 Power");

    let inner = power_block.inner(area);
    f.render_widget(power_block, area);

    if let Some(power) = &status.power {
        // First, split into Level and content area
        let main_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // Level
                Constraint::Min(4),    // Content area (2 columns with 4 items each)
            ])
            .split(inner);

        // Battery level - spans full width
        let level_bar = create_progress_bar(power.level_percent / 100.0, 12);
        let level_text = format!("Level {} {:.1}%", level_bar, power.level_percent);
        let level_style = if power.level_percent < 20.0 {
            Styles::error()
        } else if power.level_percent < 40.0 {
            Styles::warning()
        } else {
            Styles::primary()
        };
        let level_para = Paragraph::new(level_text).style(level_style);
        f.render_widget(level_para, main_layout[0]);

        // Split content area into 2 columns
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(main_layout[1]);

        // Left column: Status, Time, Health, Cycles (4 items)
        let left_lines = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // Status
                Constraint::Length(1), // Time
                Constraint::Length(1), // Health
                Constraint::Length(1), // Cycles
            ])
            .split(columns[0]);

        // Right column: Voltage, Power, Design, Full (4 items)
        let mut right_constraints = vec![];
        if power.voltage_volts.is_some() {
            right_constraints.push(Constraint::Length(1)); // Voltage
        }
        if power.energy_rate_watts.is_some() {
            right_constraints.push(Constraint::Length(1)); // Power
        }
        if power.design_capacity_mwh.is_some() {
            right_constraints.push(Constraint::Length(1)); // Design
        }
        if power.full_charge_capacity_mwh.is_some() {
            right_constraints.push(Constraint::Length(1)); // Full
        }
        // Ensure we have 4 items even if some are missing
        while right_constraints.len() < 4 {
            right_constraints.push(Constraint::Length(1));
        }

        let right_lines = Layout::default()
            .direction(Direction::Vertical)
            .constraints(right_constraints)
            .split(columns[1]);

        let mut left_idx = 0;
        let mut right_idx = 0;

        // LEFT COLUMN

        // Status
        let status_text = format!("Status {}", power.status);
        let status_para = Paragraph::new(status_text).style(Styles::secondary());
        f.render_widget(status_para, left_lines[left_idx]);
        left_idx += 1;

        // Time remaining/charging
        if let Some(time_sec) = power.time_to_empty_seconds {
            let time_str = format_uptime(time_sec);
            let time_text = format!("Time   {} left", time_str);
            let time_para = Paragraph::new(time_text).style(Styles::secondary());
            f.render_widget(time_para, left_lines[left_idx]);
        } else if let Some(time_sec) = power.time_to_full_seconds {
            let time_str = format_uptime(time_sec);
            let time_text = format!("Charge {} full", time_str);
            let time_para = Paragraph::new(time_text).style(Styles::secondary());
            f.render_widget(time_para, left_lines[left_idx]);
        } else {
            // No time estimate - show empty
            let time_text = "Time   -";
            let time_para = Paragraph::new(time_text).style(Styles::secondary());
            f.render_widget(time_para, left_lines[left_idx]);
        }
        left_idx += 1;

        // Health (moved before Cycles)
        let health_text = format!("Health  {}", power.health);
        let health_para = Paragraph::new(health_text).style(Styles::secondary());
        f.render_widget(health_para, left_lines[left_idx]);
        left_idx += 1;

        // Cycles
        if let Some(cycles) = power.cycles {
            let cycles_text = format!("Cycles {}", cycles);
            let cycles_para = Paragraph::new(cycles_text).style(Styles::secondary());
            f.render_widget(cycles_para, left_lines[left_idx]);
        } else {
            let cycles_text = "Cycles -";
            let cycles_para = Paragraph::new(cycles_text).style(Styles::secondary());
            f.render_widget(cycles_para, left_lines[left_idx]);
        }

        // RIGHT COLUMN

        // Voltage
        if let Some(voltage) = power.voltage_volts {
            let volt_text = format!("Volt   {:.2} V", voltage);
            let volt_para = Paragraph::new(volt_text).style(Styles::secondary());
            if right_idx < right_lines.len() {
                f.render_widget(volt_para, right_lines[right_idx]);
                right_idx += 1;
            }
        } else if right_idx < right_lines.len() {
            // Placeholder if voltage not available
            let volt_text = "Volt   -";
            let volt_para = Paragraph::new(volt_text).style(Styles::secondary());
            f.render_widget(volt_para, right_lines[right_idx]);
            right_idx += 1;
        }

        // Power consumption
        if let Some(watts) = power.energy_rate_watts {
            let power_text = format!("Power   {:.1} W", watts);
            let power_para = Paragraph::new(power_text).style(Styles::secondary());
            if right_idx < right_lines.len() {
                f.render_widget(power_para, right_lines[right_idx]);
                right_idx += 1;
            }
        } else if right_idx < right_lines.len() {
            // Placeholder if power not available
            let power_text = "Power   -";
            let power_para = Paragraph::new(power_text).style(Styles::secondary());
            f.render_widget(power_para, right_lines[right_idx]);
            right_idx += 1;
        }

        // Design Capacity
        if let Some(design_cap) = power.design_capacity_mwh {
            let design_text = format!("Design  {:.0} mWh", design_cap);
            let design_para = Paragraph::new(design_text).style(Styles::secondary());
            if right_idx < right_lines.len() {
                f.render_widget(design_para, right_lines[right_idx]);
                right_idx += 1;
            }
        } else if right_idx < right_lines.len() {
            // Placeholder if design capacity not available
            let design_text = "Design  -";
            let design_para = Paragraph::new(design_text).style(Styles::secondary());
            f.render_widget(design_para, right_lines[right_idx]);
            right_idx += 1;
        }

        // Full Charge Capacity
        if let Some(full_cap) = power.full_charge_capacity_mwh {
            let full_text = format!("Full    {:.0} mWh", full_cap);
            let full_para = Paragraph::new(full_text).style(Styles::secondary());
            if right_idx < right_lines.len() {
                f.render_widget(full_para, right_lines[right_idx]);
            }
        } else if right_idx < right_lines.len() {
            // Placeholder if full capacity not available
            let full_text = "Full    -";
            let full_para = Paragraph::new(full_text).style(Styles::secondary());
            f.render_widget(full_para, right_lines[right_idx]);
        }
    } else {
        // No power info available
        let no_power = Paragraph::new("No battery info")
            .style(Styles::secondary())
            .alignment(Alignment::Center);
        f.render_widget(no_power, inner);
    }
}

#[allow(dead_code)] // Kept for potential future use or debugging
fn render_power_section(f: &mut Frame, area: Rect, status: &SystemStatus) {
    let power_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Styles::border())
        .title("⚡ Power");

    let inner = power_block.inner(area);
    f.render_widget(power_block, area);

    if let Some(power) = &status.power {
        let mut constraints = vec![
            Constraint::Length(1), // Level
            Constraint::Length(1), // Status
            Constraint::Length(1), // Health
            Constraint::Length(1), // Cycles
        ];

        if power.time_to_empty_seconds.is_some() || power.time_to_full_seconds.is_some() {
            constraints.push(Constraint::Length(1)); // Time estimate
        }
        if power.voltage_volts.is_some() {
            constraints.push(Constraint::Length(1)); // Voltage
        }
        if power.energy_rate_watts.is_some() {
            constraints.push(Constraint::Length(1)); // Energy rate
        }
        if power.design_capacity_mwh.is_some() {
            constraints.push(Constraint::Length(1)); // Design Capacity
        }
        if power.full_charge_capacity_mwh.is_some() {
            constraints.push(Constraint::Length(1)); // Full Charge Capacity
        }

        let lines = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(inner);

        let mut line_idx = 0;

        // Battery level - match Memory "Used" format (8-char label, 20-char bar, percentage)
        let level_bar = create_progress_bar(power.level_percent / 100.0, 20);
        let level_text = format!("Level   {}  {:.1}%", level_bar, power.level_percent);
        let level_para = Paragraph::new(level_text).style(Styles::primary());
        f.render_widget(level_para, lines[line_idx]);
        line_idx += 1;

        // Status - match Memory "Total" position
        let status_text = format!("Status  {}", power.status);
        let status_para = Paragraph::new(status_text).style(Styles::secondary());
        f.render_widget(status_para, lines[line_idx]);
        line_idx += 1;

        // Health - match Memory "Free" position
        let health_text = format!("Health  {}", power.health);
        let health_para = Paragraph::new(health_text).style(Styles::secondary());
        f.render_widget(health_para, lines[line_idx]);
        line_idx += 1;

        // Cycles - match Memory "Swap" position
        if let Some(cycles) = power.cycles {
            let cycles_text = format!("Cycles  {}", cycles);
            let cycles_para = Paragraph::new(cycles_text).style(Styles::secondary());
            f.render_widget(cycles_para, lines[line_idx]);
            line_idx += 1;
        } else {
            line_idx += 1;
        }

        // Time estimates
        if let Some(time_to_empty) = power.time_to_empty_seconds {
            let time_str = format_time(time_to_empty);
            let time_text = format!("Time    {} left", time_str);
            let time_para = Paragraph::new(time_text).style(Styles::secondary());
            f.render_widget(time_para, lines[line_idx]);
            line_idx += 1;
        } else if let Some(time_to_full) = power.time_to_full_seconds {
            let time_str = format_time(time_to_full);
            let time_text = format!("Time    {} to full", time_str);
            let time_para = Paragraph::new(time_text).style(Styles::secondary());
            f.render_widget(time_para, lines[line_idx]);
            line_idx += 1;
        }

        // Voltage
        if let Some(voltage) = power.voltage_volts {
            let voltage_text = format!("Volt    {:.2} V", voltage);
            let voltage_para = Paragraph::new(voltage_text).style(Styles::secondary());
            f.render_widget(voltage_para, lines[line_idx]);
            line_idx += 1;
        }

        // Energy rate
        if let Some(rate) = power.energy_rate_watts {
            let rate_text = format!("Power   {:.1} W", rate);
            let rate_para = Paragraph::new(rate_text).style(Styles::secondary());
            f.render_widget(rate_para, lines[line_idx]);
            line_idx += 1;
        }

        // Design Capacity
        if let Some(design_cap) = power.design_capacity_mwh {
            let design_text = format!("Design  {:.0} mWh", design_cap);
            let design_para = Paragraph::new(design_text).style(Styles::secondary());
            f.render_widget(design_para, lines[line_idx]);
            line_idx += 1;
        }

        // Full Charge Capacity
        if let Some(full_cap) = power.full_charge_capacity_mwh {
            let full_text = format!("Full    {:.0} mWh", full_cap);
            let full_para = Paragraph::new(full_text).style(Styles::secondary());
            f.render_widget(full_para, lines[line_idx]);
        }
    } else {
        // No battery - show plugged in status
        let text = Paragraph::new("Status  Plugged In")
            .style(Styles::secondary())
            .alignment(Alignment::Left);
        f.render_widget(text, inner);
    }
}

fn render_network_section(f: &mut Frame, area: Rect, status: &SystemStatus) {
    let network_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Styles::border())
        .title("⇅ Network");

    let inner = network_block.inner(area);
    f.render_widget(network_block, area);

    // Find the primary interface (one with real IPs, prefer IPv4 192.x.x.x, then fe80, then any)
    let primary_iface = status
        .network_interfaces
        .iter()
        .find(|iface| iface.ip_addresses.iter().any(|ip| ip.starts_with("192.")))
        .or_else(|| {
            status
                .network_interfaces
                .iter()
                .find(|iface| iface.ip_addresses.iter().any(|ip| ip.starts_with("fe80:")))
        })
        .or_else(|| {
            status
                .network_interfaces
                .iter()
                .find(|iface| !iface.ip_addresses.is_empty())
        })
        .or_else(|| status.network_interfaces.first());

    // Collect IPs: prefer IPv4 192.x.x.x, then fe80, then others
    let mut ipv4_192 = Vec::new();
    let mut ipv6_fe80 = Vec::new();

    if let Some(iface) = primary_iface {
        for ip in &iface.ip_addresses {
            if ip.starts_with("192.") {
                ipv4_192.push(ip.clone());
            } else if ip.starts_with("fe80:") {
                ipv6_fe80.push(ip.clone());
            }
        }
    }

    // Always show at least download/upload, even if no interfaces found
    let mut constraints = vec![
        Constraint::Length(1), // Download
        Constraint::Length(1), // Upload
    ];

    // Show connection status and type if available
    if let Some(iface) = primary_iface {
        if iface.is_up || !iface.ip_addresses.is_empty() {
            constraints.push(Constraint::Length(1)); // Connection status/type
        }
    }

    if status.network.proxy.is_some() {
        constraints.push(Constraint::Length(1)); // Proxy
    }

    // Show MAC if available and valid
    if let Some(iface) = primary_iface {
        if let Some(ref mac) = iface.mac_address {
            if !mac.starts_with("00:00:00:00:00:00") {
                constraints.push(Constraint::Length(1)); // MAC
            }
        }
    }

    // Show IPs
    if !ipv4_192.is_empty() {
        constraints.push(Constraint::Length(1)); // IPv4
    }
    if !ipv6_fe80.is_empty() {
        constraints.push(Constraint::Length(1)); // IPv6 fe80
    }

    // If no IPs found, show a message
    if ipv4_192.is_empty() && ipv6_fe80.is_empty() && primary_iface.is_none() {
        constraints.push(Constraint::Length(1)); // No network message
    }

    let lines = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    let mut line_idx = 0;

    // Download - use Kbps if < 1 MB/s, otherwise MB/s
    let (down_value, down_unit, down_bar_max) = if status.network.download_mb < 1.0 {
        let kbps = status.network.download_mb * 1000.0;
        (kbps, "Kbps", 100.0) // Scale bar for Kbps (0-100 Kbps)
    } else {
        (status.network.download_mb, "MB/s", 10.0) // Scale bar for MB/s (0-10 MB/s)
    };
    let down_bar = create_speed_bar(status.network.download_mb / down_bar_max, 5);
    let down_text = format!("Down    {}  {:.1} {}", down_bar, down_value, down_unit);
    let down_para = Paragraph::new(down_text).style(Styles::secondary());
    f.render_widget(down_para, lines[line_idx]);
    line_idx += 1;

    // Upload - use Kbps if < 1 MB/s, otherwise MB/s
    let (up_value, up_unit, up_bar_max) = if status.network.upload_mb < 1.0 {
        let kbps = status.network.upload_mb * 1000.0;
        (kbps, "Kbps", 100.0) // Scale bar for Kbps (0-100 Kbps)
    } else {
        (status.network.upload_mb, "MB/s", 10.0) // Scale bar for MB/s (0-10 MB/s)
    };
    let up_bar = create_speed_bar(status.network.upload_mb / up_bar_max, 5);
    let up_text = format!("Up      {}  {:.1} {}", up_bar, up_value, up_unit);
    let up_para = Paragraph::new(up_text).style(Styles::secondary());
    f.render_widget(up_para, lines[line_idx]);
    line_idx += 1;

    // Connection status and type
    if let Some(iface) = primary_iface {
        if iface.is_up || !iface.ip_addresses.is_empty() {
            let conn_status = if iface.is_up || !iface.ip_addresses.is_empty() {
                "Connected"
            } else {
                "Disconnected"
            };
            let conn_type = iface.connection_type.as_deref().unwrap_or("Unknown");

            let conn_text = format!("Status  {} · {}", conn_status, conn_type);
            let conn_para = Paragraph::new(conn_text).style(Styles::secondary());
            f.render_widget(conn_para, lines[line_idx]);
            line_idx += 1;
        }
    }

    // Proxy
    if let Some(proxy) = &status.network.proxy {
        let proxy_text = format!("Proxy   {}", proxy);
        let proxy_para = Paragraph::new(proxy_text).style(Styles::secondary());
        f.render_widget(proxy_para, lines[line_idx]);
        line_idx += 1;
    }

    // Show MAC address (if valid)
    if let Some(iface) = primary_iface {
        if let Some(ref mac) = iface.mac_address {
            if !mac.starts_with("00:00:00:00:00:00") {
                let mac_text = format!("MAC     {}", mac);
                let mac_para = Paragraph::new(mac_text).style(Styles::secondary());
                f.render_widget(mac_para, lines[line_idx]);
                line_idx += 1;
            }
        }
    }

    // Show IPv4 192.x.x.x addresses
    if !ipv4_192.is_empty() {
        let ip_text = format!("IPv4    {}", ipv4_192[0]);
        let ip_para = Paragraph::new(ip_text).style(Styles::secondary());
        f.render_widget(ip_para, lines[line_idx]);
        line_idx += 1;
    }

    // Show IPv6 fe80 addresses
    if !ipv6_fe80.is_empty() {
        let ip_text = format!("IPv6    {}", ipv6_fe80[0]);
        let ip_para = Paragraph::new(ip_text).style(Styles::secondary());
        f.render_widget(ip_para, lines[line_idx]);
    } else if ipv4_192.is_empty() && primary_iface.is_none() {
        // Show message if no network interfaces found
        let msg_text = "No active network";
        let msg_para = Paragraph::new(msg_text).style(Styles::secondary());
        f.render_widget(msg_para, lines[line_idx]);
    }
}

#[cfg(windows)]
fn render_top_io_section(f: &mut Frame, area: Rect, status: &SystemStatus) {
    if status.top_io_processes.is_empty() {
        return;
    }

    let io_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Styles::border())
        .title("💾 Top Disk I/O");

    let inner = io_block.inner(area);
    f.render_widget(io_block, area);

    let process_count = status.top_io_processes.len().min(5);
    let lines = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            (0..process_count)
                .map(|_| Constraint::Length(1))
                .collect::<Vec<_>>(),
        )
        .split(inner);

    for (i, io_proc) in status
        .top_io_processes
        .iter()
        .take(process_count)
        .enumerate()
    {
        let read_mb = io_proc.read_bytes_per_sec / 1_000_000.0;
        let write_mb = io_proc.write_bytes_per_sec / 1_000_000.0;

        // Format process name (truncate if too long)
        let name = if io_proc.name.len() > 18 {
            format!("{}…", &io_proc.name[..17])
        } else {
            io_proc.name.clone()
        };

        // Format I/O rates - use Kbps if < 1 MB/s
        let (read_val, read_unit) = if read_mb < 1.0 {
            (read_mb * 1000.0, "Kbps")
        } else {
            (read_mb, "MB/s")
        };
        let (write_val, write_unit) = if write_mb < 1.0 {
            (write_mb * 1000.0, "Kbps")
        } else {
            (write_mb, "MB/s")
        };

        let io_text = format!(
            "{:<19} R: {:>5.1} {}  W: {:>5.1} {}",
            name, read_val, read_unit, write_val, write_unit
        );
        let io_para = Paragraph::new(io_text).style(Styles::secondary());
        f.render_widget(io_para, lines[i]);
    }
}

fn render_processes_section(f: &mut Frame, area: Rect, status: &SystemStatus) {
    // Maximized processes section with better visual presentation
    let processes_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Styles::border())
        .title(format!(
            "▶ Top Processes (showing {} of {})",
            status.processes.len().min(20),
            status.cpu.process_count
        ));

    let inner = processes_block.inner(area);
    f.render_widget(processes_block, area);

    // Check if we have processes to display
    if status.processes.is_empty() {
        let msg = Paragraph::new(format!(
            "Loading... ({} total processes)",
            status.cpu.process_count
        ))
        .style(Styles::secondary())
        .alignment(Alignment::Center);
        f.render_widget(msg, inner);
        return;
    }

    if inner.height < 3 {
        return; // Need at least 3 lines for header + 1 process
    }

    // Calculate how many processes we can fit
    let available_rows = inner.height as usize - 1; // Reserve 1 line for header
    let processes_to_show = status.processes.len().min(available_rows).min(20); // Show up to 20 processes

    // Create a table-like layout with header
    let header_area = Rect {
        x: inner.x,
        y: inner.y,
        width: inner.width,
        height: 1,
    };

    // Header row with column labels
    let header_text = format!(
        "{:<20} {:>6} {:>8} {:>10} {:>8} {:>12}",
        "Process", "PID", "CPU %", "Memory", "Handles", "Page Faults"
    );
    let header_para = Paragraph::new(header_text)
        .style(Styles::primary())
        .alignment(Alignment::Left);
    f.render_widget(header_para, header_area);

    // Separator line
    let separator = "─".repeat(inner.width as usize);
    let separator_area = Rect {
        x: inner.x,
        y: inner.y + 1,
        width: inner.width,
        height: 1,
    };
    let separator_para = Paragraph::new(separator).style(Styles::border());
    f.render_widget(separator_para, separator_area);

    // Process rows - use full width for better readability
    let process_start_y = inner.y + 2;
    let max_process_rows = (inner.height - 2) as usize; // Subtract header and separator

    for (i, proc) in status
        .processes
        .iter()
        .take(processes_to_show.min(max_process_rows))
        .enumerate()
    {
        let row_y = process_start_y + i as u16;
        if row_y >= inner.y + inner.height {
            break;
        }

        // Format process name (truncate if needed)
        let name = if proc.name.len() > 18 {
            format!("{}…", &proc.name[..17])
        } else {
            proc.name.clone()
        };

        // CPU usage bar - more visual
        let cpu_bar_width = 12;
        let cpu_bar = create_progress_bar(proc.cpu_usage.min(100.0) / 100.0, cpu_bar_width);

        // Memory formatting
        let memory_str = if proc.memory_mb >= 1024.0 {
            format!("{:.1} GB", proc.memory_mb / 1024.0)
        } else {
            format!("{:.0} MB", proc.memory_mb)
        };

        // Handle count (Windows only)
        #[cfg(windows)]
        let handles_str = if let Some(h) = proc.handle_count {
            if h > 1000 {
                format!("{}⚠️", h)
            } else {
                format!("{}", h)
            }
        } else {
            "-".to_string()
        };
        #[cfg(not(windows))]
        let handles_str = "-".to_string();

        // Page faults (Windows only)
        #[cfg(windows)]
        let faults_str = if let Some(f) = proc.page_faults_per_sec {
            if f > 100 {
                format!("{}⚠️", f)
            } else {
                format!("{}", f)
            }
        } else {
            "-".to_string()
        };
        #[cfg(not(windows))]
        let faults_str = "-".to_string();

        // Format the process row
        let proc_text = format!(
            "{:<20} {:>6} {} {:>6.1}% {:>8} {:>8} {:>12}",
            name,
            proc.pid,
            cpu_bar,
            proc.cpu_usage.min(100.0),
            memory_str,
            handles_str,
            faults_str
        );

        // Style based on CPU usage
        let proc_style = if proc.cpu_usage > 50.0 {
            Styles::warning()
        } else {
            Styles::secondary()
        };

        let proc_area = Rect {
            x: inner.x,
            y: row_y,
            width: inner.width,
            height: 1,
        };

        let proc_para = Paragraph::new(proc_text)
            .style(proc_style)
            .alignment(Alignment::Left);
        f.render_widget(proc_para, proc_area);
    }
}

fn create_mini_bar(value: f32, width: usize) -> String {
    let filled = (value.clamp(0.0, 1.0) * width as f32).round() as usize;
    let empty = width.saturating_sub(filled);
    format!("{}{}", "▰".repeat(filled), "▱".repeat(empty))
}

fn create_progress_bar(value: f32, width: usize) -> String {
    let filled = (value.clamp(0.0, 1.0) * width as f32).round() as usize;
    let empty = width.saturating_sub(filled);
    // Use block characters for clear progress indication
    format!("{}{}", "▰".repeat(filled), "▱".repeat(empty))
}

fn create_speed_bar(value: f64, width: usize) -> String {
    let filled = (value.clamp(0.0, 1.0) * width as f64).round() as usize;
    let empty = width.saturating_sub(filled);
    format!("{}{}", "▰".repeat(filled), "▱".repeat(empty))
}

#[cfg(windows)]
#[allow(dead_code)]
fn render_disk_breakdown_placeholder(f: &mut Frame, area: Rect) {
    let breakdown_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Styles::border())
        .title("💿 Disk Breakdown");

    let inner = breakdown_block.inner(area);
    f.render_widget(breakdown_block, area);

    let msg = Paragraph::new("Calculating... (Press 'd' to refresh manually)")
        .style(Styles::secondary())
        .alignment(Alignment::Center);
    f.render_widget(msg, inner);
}

#[cfg(windows)]
#[allow(dead_code)]
fn render_disk_breakdown_section(
    f: &mut Frame,
    area: Rect,
    breakdown: &crate::status::DiskBreakdown,
) {
    let breakdown_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Styles::border())
        .title("💿 Disk Breakdown");

    let inner = breakdown_block.inner(area);
    f.render_widget(breakdown_block, area);

    if breakdown.categories.is_empty() {
        let msg = Paragraph::new("No data available (Press 'd' to refresh)")
            .style(Styles::secondary())
            .alignment(Alignment::Center);
        f.render_widget(msg, inner);
        return;
    }

    // Calculate how many categories we can fit
    let available_rows = inner.height as usize;
    let categories_to_show = breakdown
        .categories
        .len()
        .min(available_rows.saturating_sub(2)); // Reserve 2 for header/footer

    let mut constraints = vec![Constraint::Length(1)]; // Header
    for _ in 0..categories_to_show {
        constraints.push(Constraint::Length(1));
    }
    constraints.push(Constraint::Length(1)); // Footer

    let lines = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    // Header
    let header_text = format!(
        "{:<20} {:>8} {:>6} {}",
        "Category", "Size", "Percent", "Usage"
    );
    let header_para = Paragraph::new(header_text)
        .style(Styles::primary())
        .alignment(Alignment::Left);
    f.render_widget(header_para, lines[0]);

    // Categories
    for (i, category) in breakdown
        .categories
        .iter()
        .take(categories_to_show)
        .enumerate()
    {
        let bar_width: usize = 30;
        let filled = (category.percent / 100.0 * bar_width as f32).round() as usize;
        let empty = bar_width.saturating_sub(filled);
        let bar = "█".repeat(filled) + &"░".repeat(empty);

        let category_text = format!(
            "{:<20} {:>7.1} GB {:>5.1}% {}",
            category.name, category.size_gb, category.percent, bar
        );
        let category_para = Paragraph::new(category_text)
            .style(Styles::secondary())
            .alignment(Alignment::Left);
        f.render_widget(category_para, lines[i + 1]);
    }

    // Footer with cache info and total
    let footer_text = if let Some(cached_at) = breakdown.cached_at {
        format!(
            "Total: {:.1} GB · Cached at: {}",
            breakdown.total_disk_gb,
            cached_at.format("%H:%M")
        )
    } else {
        format!("Total: {:.1} GB", breakdown.total_disk_gb)
    };
    let footer_para = Paragraph::new(footer_text)
        .style(Styles::secondary())
        .alignment(Alignment::Left);
    f.render_widget(footer_para, lines[lines.len() - 1]);
}

#[cfg(windows)]
fn render_boot_info_section(f: &mut Frame, area: Rect, boot_info: &crate::status::BootInfo) {
    let boot_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Styles::border())
        .title("🔄 System Diagnostics");

    let inner = boot_block.inner(area);
    f.render_widget(boot_block, area);

    let lines = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Uptime
            Constraint::Length(1), // Last Boot
            Constraint::Length(1), // Boot Time
            Constraint::Length(1), // Shutdown Type
        ])
        .split(inner);

    // Uptime
    let uptime_str = format_uptime_detailed(boot_info.uptime_seconds);
    let uptime_text = format!("Uptime:      {}", uptime_str);
    let uptime_para = Paragraph::new(uptime_text)
        .style(Styles::primary())
        .alignment(Alignment::Left);
    f.render_widget(uptime_para, lines[0]);

    // Last Boot
    let boot_time_str = boot_info.last_boot_time.format("%Y-%m-%d %H:%M:%S");
    let days_ago = chrono::Local::now()
        .signed_duration_since(boot_info.last_boot_time)
        .num_days();
    let ago_str = if days_ago == 0 {
        "today".to_string()
    } else if days_ago == 1 {
        "yesterday".to_string()
    } else {
        format!("{}d ago", days_ago)
    };
    let boot_text = format!("Last Boot:   {} ({})", boot_time_str, ago_str);
    let boot_para = Paragraph::new(boot_text)
        .style(Styles::secondary())
        .alignment(Alignment::Left);
    f.render_widget(boot_para, lines[1]);

    // Boot Duration
    let boot_duration_text = format!(
        "Boot Time:   ~{} seconds (estimated)",
        boot_info.boot_duration_seconds
    );
    let boot_duration_para = Paragraph::new(boot_duration_text)
        .style(Styles::secondary())
        .alignment(Alignment::Left);
    f.render_widget(boot_duration_para, lines[2]);

    // Shutdown Type
    let shutdown_text = format!(
        "Shutdown:    {} (graceful shutdown)",
        boot_info.shutdown_type
    );
    let shutdown_para = Paragraph::new(shutdown_text)
        .style(Styles::secondary())
        .alignment(Alignment::Left);
    f.render_widget(shutdown_para, lines[3]);
}

fn format_uptime_detailed(seconds: u64) -> String {
    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;
    let minutes = (seconds % 3600) / 60;

    if days > 0 {
        format!("{} days, {} hours, {} minutes", days, hours, minutes)
    } else if hours > 0 {
        format!("{} hours, {} minutes", hours, minutes)
    } else {
        format!("{} minutes", minutes)
    }
}

// GPU implementation commented out - not polished yet
/*fn render_gpu_section(f: &mut Frame, area: Rect, gpu: &crate::status::GpuMetrics) {
    let gpu_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Styles::border())
        .title("🎮 GPU");

    let inner = gpu_block.inner(area);
    f.render_widget(gpu_block, area);

    // Build constraints based on available data
    let mut constraints = vec![
        Constraint::Length(1), // Name
    ];

    // Memory line (dedicated + shared combined)
    if (gpu.memory_dedicated_used_mb.is_some() || gpu.memory_dedicated_total_mb.is_some())
        || (gpu.memory_shared_used_mb.is_some() || gpu.memory_shared_total_mb.is_some())
    {
        constraints.push(Constraint::Length(1)); // Memory (dedicated + shared)
    }

    // Utilization line (overall + engines combined)
    if gpu.utilization_percent.is_some()
        || gpu.render_engine_percent.is_some()
        || gpu.copy_engine_percent.is_some()
        || gpu.compute_engine_percent.is_some()
        || gpu.video_engine_percent.is_some()
    {
        constraints.push(Constraint::Length(1)); // Utilization + engines
    }

    // Temperature line (temp + threshold)
    if gpu.temperature_celsius.is_some() || gpu.temperature_threshold_celsius.is_some() {
        constraints.push(Constraint::Length(1)); // Temperature + threshold
    }

    // Optional: Clock speed and Power (if available)
    if gpu.clock_speed_mhz.is_some() {
        constraints.push(Constraint::Length(1)); // Clock speed
    }
    if gpu.power_usage_watts.is_some() {
        constraints.push(Constraint::Length(1)); // Power
    }

    let lines = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    let mut line_idx = 0;

    // GPU Name with driver version (truncate if too long)
    let name_display = if let Some(ref driver) = gpu.driver_version {
        let name_with_driver = format!("{} ({})", gpu.name, driver);
        if name_with_driver.len() > (inner.width as usize).saturating_sub(5) {
            format!(
                "{}…",
                &name_with_driver[..(inner.width as usize).saturating_sub(6)]
            )
        } else {
            name_with_driver
        }
    } else {
        let name = if gpu.name.len() > (inner.width as usize).saturating_sub(10) {
            format!(
                "{}…",
                &gpu.name[..(inner.width as usize).saturating_sub(11)]
            )
        } else {
            gpu.name.clone()
        };
        format!("{} ({})", name, gpu.vendor)
    };
    let name_para = Paragraph::new(name_display)
        .style(Styles::primary())
        .alignment(Alignment::Left);
    f.render_widget(name_para, lines[line_idx]);
    line_idx += 1;

    // Memory line: Dedicated + Shared combined (like Task Manager)
    if line_idx < lines.len() {
        let mut mem_parts = Vec::new();

        // Dedicated memory
        if let (Some(used), Some(total)) =
            (gpu.memory_dedicated_used_mb, gpu.memory_dedicated_total_mb)
        {
            let used_gb = used as f64 / 1024.0;
            let total_gb = total as f64 / 1024.0;
            let mem_percent = if total > 0 {
                (used as f32 / total as f32) * 100.0
            } else {
                0.0
            };
            mem_parts.push(format!(
                "{:.1} / {:.1} GB ({:.1}%)",
                used_gb, total_gb, mem_percent
            ));
        }

        // Shared memory
        if let (Some(shared_used), Some(shared_total)) =
            (gpu.memory_shared_used_mb, gpu.memory_shared_total_mb)
        {
            let shared_used_gb = shared_used as f64 / 1024.0;
            let shared_total_gb = shared_total as f64 / 1024.0;
            let shared_percent = if shared_total > 0 {
                (shared_used as f32 / shared_total as f32) * 100.0
            } else {
                0.0
            };
            mem_parts.push(format!(
                "Shared {:.1} / {:.1} GB ({:.1}%)",
                shared_used_gb, shared_total_gb, shared_percent
            ));
        }

        if !mem_parts.is_empty() {
            let mem_text = format!("Memory   {}", mem_parts.join("  "));
            let mem_para = Paragraph::new(mem_text).style(Styles::secondary());
            f.render_widget(mem_para, lines[line_idx]);
            line_idx += 1;
        }
    }

    // Utilization line: Overall + Engines (like Task Manager)
    if line_idx < lines.len() {
        let mut util_parts = Vec::new();

        // Overall utilization
        if let Some(util) = gpu.utilization_percent {
            util_parts.push(format!("{:.0}%", util));
        }

        // Engine breakdowns
        if let Some(render) = gpu.render_engine_percent {
            util_parts.push(format!("3D: {:.0}%", render));
        }
        if let Some(copy) = gpu.copy_engine_percent {
            util_parts.push(format!("Copy: {:.0}%", copy));
        }
        if let Some(compute) = gpu.compute_engine_percent {
            util_parts.push(format!("Compute: {:.0}%", compute));
        }
        if let Some(video) = gpu.video_engine_percent {
            util_parts.push(format!("Video: {:.0}%", video));
        }

        if !util_parts.is_empty() {
            let util_text = format!(
                "Utilization  {}  │  {}",
                util_parts[0],
                util_parts[1..].join("  ")
            );
            let util_para = Paragraph::new(util_text).style(Styles::primary());
            f.render_widget(util_para, lines[line_idx]);
            line_idx += 1;
        }
    }

    // Temperature line: Temp + Threshold (like Task Manager)
    if line_idx < lines.len() {
        let mut temp_parts = Vec::new();

        if let Some(temp) = gpu.temperature_celsius {
            temp_parts.push(format!("{:.0}°C", temp));
        }

        if let Some(threshold) = gpu.temperature_threshold_celsius {
            temp_parts.push(format!("Threshold: {:.0}°C", threshold));
        }

        if !temp_parts.is_empty() {
            let temp_style = if let Some(temp) = gpu.temperature_celsius {
                if temp > 85.0 {
                    Styles::error()
                } else if temp > 75.0 {
                    Styles::warning()
                } else {
                    Styles::secondary()
                }
            } else {
                Styles::secondary()
            };

            let temp_text = if temp_parts.len() > 1 {
                format!("Temperature  {}  │  {}", temp_parts[0], temp_parts[1])
            } else {
                format!("Temperature  {}", temp_parts[0])
            };
            let temp_para = Paragraph::new(temp_text).style(temp_style);
            f.render_widget(temp_para, lines[line_idx]);
            line_idx += 1;
        }
    }

    // Clock Speed
    if let Some(clock) = gpu.clock_speed_mhz {
        let clock_ghz = clock as f64 / 1000.0;
        let clock_text = format!("Clock   {:.1} GHz", clock_ghz);
        let clock_para = Paragraph::new(clock_text).style(Styles::secondary());
        if line_idx < lines.len() {
            f.render_widget(clock_para, lines[line_idx]);
            line_idx += 1;
        }
    }

    // Power Usage
    if let Some(power) = gpu.power_usage_watts {
        let power_text = format!("Power   {:.1} W", power);
        let power_para = Paragraph::new(power_text).style(Styles::secondary());
        if line_idx < lines.len() {
            f.render_widget(power_para, lines[line_idx]);
        }
    }
}
*/
// End of GPU rendering function comment

fn render_temperature_sensors_section(
    f: &mut Frame,
    area: Rect,
    sensors: &[crate::status::TemperatureSensor],
) {
    if sensors.is_empty() {
        return;
    }

    let temp_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Styles::border())
        .title("🌡️  Temperature");

    let inner = temp_block.inner(area);
    f.render_widget(temp_block, area);

    // Show up to 5 sensors (or as many as fit)
    let sensors_to_show = sensors.len().min(5).min(inner.height as usize);

    if sensors_to_show == 0 {
        return;
    }

    let lines = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            (0..sensors_to_show)
                .map(|_| Constraint::Length(1))
                .collect::<Vec<_>>(),
        )
        .split(inner);

    for (i, sensor) in sensors.iter().take(sensors_to_show).enumerate() {
        if i < lines.len() {
            // Truncate label if too long
            let label = if sensor.label.len() > (inner.width as usize).saturating_sub(15) {
                format!(
                    "{}…",
                    &sensor.label[..(inner.width as usize).saturating_sub(16)]
                )
            } else {
                sensor.label.clone()
            };

            // Determine style based on temperature
            let temp_style = if let Some(critical) = sensor.critical_celsius {
                if sensor.temperature_celsius >= critical {
                    Styles::error()
                } else if let Some(max) = sensor.max_celsius {
                    if sensor.temperature_celsius >= max * 0.9 {
                        Styles::warning()
                    } else {
                        Styles::secondary()
                    }
                } else {
                    Styles::secondary()
                }
            } else if sensor.temperature_celsius > 85.0 {
                Styles::error()
            } else if sensor.temperature_celsius > 75.0 {
                Styles::warning()
            } else {
                Styles::secondary()
            };

            // Format temperature with max/critical indicators
            let temp_text = if let Some(critical) = sensor.critical_celsius {
                if sensor.temperature_celsius >= critical {
                    format!("{:<12} {:.0}°C (CRIT)", label, sensor.temperature_celsius)
                } else if let Some(max) = sensor.max_celsius {
                    format!(
                        "{:<12} {:.0}°C / {:.0}°C",
                        label, sensor.temperature_celsius, max
                    )
                } else {
                    format!("{:<12} {:.0}°C", label, sensor.temperature_celsius)
                }
            } else if let Some(max) = sensor.max_celsius {
                format!(
                    "{:<12} {:.0}°C / {:.0}°C",
                    label, sensor.temperature_celsius, max
                )
            } else {
                format!("{:<12} {:.0}°C", label, sensor.temperature_celsius)
            };

            let temp_para = Paragraph::new(temp_text)
                .style(temp_style)
                .alignment(Alignment::Left);
            f.render_widget(temp_para, lines[i]);
        }
    }
}

fn format_uptime(seconds: u64) -> String {
    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;
    let minutes = (seconds % 3600) / 60;

    if days > 0 {
        format!("{}d {}h {}m", days, hours, minutes)
    } else if hours > 0 {
        format!("{}h {}m", hours, minutes)
    } else {
        format!("{}m", minutes)
    }
}

#[allow(dead_code)] // Kept for potential future use
fn format_time(seconds: u64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;

    if hours > 0 {
        format!("{}h {}m", hours, minutes)
    } else {
        format!("{}m", minutes)
    }
}
