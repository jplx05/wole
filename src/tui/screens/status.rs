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
    if let crate::tui::state::Screen::Status { status, last_refresh } = &app_state.screen {
        // Header with health score and live indicator
        let header_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2), // Health header (2 lines)
                Constraint::Length(1), // Spacing
                Constraint::Min(1),    // Main content
            ])
            .split(area);

        render_status_header_with_indicator(f, header_chunks[0], status, last_refresh);

        // Main content area
        render_status_dashboard(f, header_chunks[2], status);
    }
}

fn render_status_header_with_indicator(f: &mut Frame, area: Rect, status: &SystemStatus, last_refresh: &std::time::Instant) {
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
    let health_text = format!("Health status: {} {}", health_indicator.0, status.health_score);
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

    // Line 2: Device information
    let device_text = format!(
        "{} · {} · {:.1}GB · {}",
        status.hardware.device_name,
        status.hardware.cpu_model,
        status.hardware.total_memory_gb,
        status.hardware.os_name
    );
    let device_para = Paragraph::new(device_text)
        .style(Styles::secondary())
        .alignment(Alignment::Left);
    f.render_widget(device_para, lines[1]);
}

fn render_status_dashboard(f: &mut Frame, area: Rect, status: &SystemStatus) {
    // Main layout: top section (columns) and bottom section (network/processes)
    let main_sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(15), // Top section (CPU/Memory/Disk/Power)
            Constraint::Length(1),  // Spacing
            Constraint::Length(5),  // Network section
            Constraint::Length(1),  // Spacing
            Constraint::Min(5),     // Processes section
        ])
        .split(area);

    // Top section: Two column layout
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(main_sections[0]);

    // Left column: CPU and Memory
    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8), // CPU
            Constraint::Length(1), // Spacing
            Constraint::Length(6), // Memory
        ])
        .split(columns[0]);

    render_cpu_section(f, left_chunks[0], status);
    render_memory_section(f, left_chunks[2], status);

    // Right column: Disk and Power
    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8), // Disk
            Constraint::Length(1), // Spacing
            Constraint::Length(9), // Power (increased to accommodate new fields)
        ])
        .split(columns[1]);

    render_disk_section(f, right_chunks[0], status);
    render_power_section(f, right_chunks[2], status);

    // Network and Processes in bottom sections
    render_network_section(f, main_sections[2], status);
    render_processes_section(f, main_sections[4], status);
}

fn render_cpu_section(f: &mut Frame, area: Rect, status: &SystemStatus) {
    let cpu_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Styles::border())
        .title("⚙ CPU");

    let inner = cpu_block.inner(area);
    f.render_widget(cpu_block, area);

    let lines = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Total
            Constraint::Length(1), // Load
            Constraint::Length(1), // Spacing
            Constraint::Min(1),    // Cores
        ])
        .split(inner);

    // Total CPU usage
    let total_bar = create_progress_bar(status.cpu.total_usage / 100.0, 20);
    let total_text = format!("Total   {}  {:.1}%", total_bar, status.cpu.total_usage);
    let total_para = Paragraph::new(total_text).style(Styles::primary());
    f.render_widget(total_para, lines[0]);

    // Load averages
    let load_text = format!(
        "Load    {:.2} / {:.2} / {:.2} ({} cores)",
        status.cpu.load_avg_1min,
        status.cpu.load_avg_5min,
        status.cpu.load_avg_15min,
        status.cpu.cores.len()
    );
    let load_para = Paragraph::new(load_text).style(Styles::secondary());
    f.render_widget(load_para, lines[1]);

    // Show first few cores
    let core_count = (lines[2].height as usize).min(status.cpu.cores.len()).min(4);
    for (i, core) in status.cpu.cores.iter().take(core_count).enumerate() {
        if i < lines[2].height as usize {
            let core_bar = create_progress_bar(core.usage / 100.0, 20);
            let core_text = format!("Core {}  {}  {:.1}%", core.id + 1, core_bar, core.usage);
            let core_para = Paragraph::new(core_text).style(Styles::secondary());
            let core_area = Rect {
                x: lines[2].x,
                y: lines[2].y + i as u16,
                width: lines[2].width,
                height: 1,
            };
            f.render_widget(core_para, core_area);
        }
    }
}

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
            Constraint::Length(1), // Free
            Constraint::Length(1), // Available
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

    // Free memory
    let free_text = format!("Free    {:.1} GB", status.memory.free_gb);
    let free_para = Paragraph::new(free_text).style(Styles::secondary());
    f.render_widget(free_para, lines[2]);

    // Available memory
    let avail_text = format!("Avail   {:.1} GB", status.memory.available_gb);
    let avail_para = Paragraph::new(avail_text).style(Styles::secondary());
    f.render_widget(avail_para, lines[3]);
}

fn render_disk_section(f: &mut Frame, area: Rect, status: &SystemStatus) {
    let disk_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Styles::border())
        .title("▤ Disk");

    let inner = disk_block.inner(area);
    f.render_widget(disk_block, area);

    let lines = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Used
            Constraint::Length(1), // Free
            Constraint::Length(1), // Read
            Constraint::Length(1), // Write
        ])
        .split(inner);

    // Used disk
    let used_bar = create_progress_bar(status.disk.used_percent / 100.0, 20);
    let used_text = format!("Used    {}  {:.1}%", used_bar, status.disk.used_percent);
    let used_para = Paragraph::new(used_text).style(Styles::primary());
    f.render_widget(used_para, lines[0]);

    // Free disk
    let free_text = format!("Free    {:.1} GB", status.disk.free_gb);
    let free_para = Paragraph::new(free_text).style(Styles::secondary());
    f.render_widget(free_para, lines[1]);

    // Read speed
    let read_bar = create_speed_bar(status.disk.read_speed_mb / 100.0, 5);
    let read_text = format!("Read    {}  {:.1} MB/s", read_bar, status.disk.read_speed_mb);
    let read_para = Paragraph::new(read_text).style(Styles::secondary());
    f.render_widget(read_para, lines[2]);

    // Write speed
    let write_bar = create_speed_bar(status.disk.write_speed_mb / 100.0, 5);
    let write_text = format!("Write   {}  {:.1} MB/s", write_bar, status.disk.write_speed_mb);
    let write_para = Paragraph::new(write_text).style(Styles::secondary());
    f.render_widget(write_para, lines[3]);
}

fn render_power_section(f: &mut Frame, area: Rect, status: &SystemStatus) {
    let power_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Styles::border())
        .title("⚡ Power");

    let inner = power_block.inner(area);
    f.render_widget(power_block, area);

    if let Some(power) = &status.power {
        let lines = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // Level
                Constraint::Length(1), // Status
                Constraint::Length(1), // Health
                Constraint::Length(1), // Cycles
                Constraint::Length(1), // Chemistry
                Constraint::Length(1), // Design Capacity
                Constraint::Length(1), // Full Charge Capacity
                Constraint::Length(1), // Temperature
            ])
            .split(inner);

        // Battery level
        let level_bar = create_progress_bar(power.level_percent / 100.0, 20);
        let level_text = format!("Level   {}  {:.0}%", level_bar, power.level_percent);
        let level_para = Paragraph::new(level_text).style(Styles::primary());
        f.render_widget(level_para, lines[0]);

        // Status
        let status_text = format!("Status  {}", power.status);
        let status_para = Paragraph::new(status_text).style(Styles::secondary());
        f.render_widget(status_para, lines[1]);

        // Health
        let health_text = format!("Health  {}", power.health);
        let health_para = Paragraph::new(health_text).style(Styles::secondary());
        f.render_widget(health_para, lines[2]);

        // Cycles
        if let Some(cycles) = power.cycles {
            let cycles_text = format!("Cycles  {}", cycles);
            let cycles_para = Paragraph::new(cycles_text).style(Styles::secondary());
            f.render_widget(cycles_para, lines[3]);
        }

        // Chemistry
        if let Some(ref chemistry) = power.chemistry {
            let chem_text = format!("Chem    {}", chemistry);
            let chem_para = Paragraph::new(chem_text).style(Styles::secondary());
            f.render_widget(chem_para, lines[4]);
        }

        // Design Capacity
        if let Some(design_cap) = power.design_capacity_mwh {
            let design_text = format!("Design  {:.0} mWh", design_cap);
            let design_para = Paragraph::new(design_text).style(Styles::secondary());
            f.render_widget(design_para, lines[5]);
        }

        // Full Charge Capacity
        if let Some(full_cap) = power.full_charge_capacity_mwh {
            let full_text = format!("Full    {:.0} mWh", full_cap);
            let full_para = Paragraph::new(full_text).style(Styles::secondary());
            f.render_widget(full_para, lines[6]);
        }

        // Temperature
        if let Some(temp) = power.temperature_celsius {
            let temp_text = format!("Temp    {:.0}°C", temp);
            let temp_para = Paragraph::new(temp_text).style(Styles::secondary());
            f.render_widget(temp_para, lines[7]);
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

    let lines = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Download
            Constraint::Length(1), // Upload
            Constraint::Length(1), // Proxy
        ])
        .split(inner);

    // Download
    let down_bar = create_speed_bar(status.network.download_mb / 10.0, 5);
    let down_text = format!("Down    {}  {:.1} MB/s", down_bar, status.network.download_mb);
    let down_para = Paragraph::new(down_text).style(Styles::secondary());
    f.render_widget(down_para, lines[0]);

    // Upload
    let up_bar = create_speed_bar(status.network.upload_mb / 10.0, 5);
    let up_text = format!("Up      {}  {:.1} MB/s", up_bar, status.network.upload_mb);
    let up_para = Paragraph::new(up_text).style(Styles::secondary());
    f.render_widget(up_para, lines[1]);

    // Proxy
    if let Some(proxy) = &status.network.proxy {
        let proxy_text = format!("Proxy   {}", proxy);
        let proxy_para = Paragraph::new(proxy_text).style(Styles::secondary());
        f.render_widget(proxy_para, lines[2]);
    }
}

fn render_processes_section(f: &mut Frame, area: Rect, status: &SystemStatus) {
    let processes_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Styles::border())
        .title("▶ Processes");

    let inner = processes_block.inner(area);
    f.render_widget(processes_block, area);

    // Two column layout for processes
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(inner);

    // Left column - first 5 processes
    let left_count = (inner.height as usize).min(5);
    for (i, proc) in status.processes.iter().take(left_count).enumerate() {
        if i < left_count {
            let proc_bar = create_progress_bar(proc.cpu_usage / 100.0, 5);
            let proc_text = format!(
                "{:15}  {}  {:.1}%",
                &proc.name[..proc.name.len().min(15)],
                proc_bar,
                proc.cpu_usage
            );
            let proc_para = Paragraph::new(proc_text).style(Styles::secondary());
            let proc_area = Rect {
                x: columns[0].x,
                y: columns[0].y + i as u16,
                width: columns[0].width,
                height: 1,
            };
            f.render_widget(proc_para, proc_area);
        }
    }

    // Right column - next 5 processes (6-10)
    let right_count = (inner.height as usize).min(5);
    for (i, proc) in status.processes.iter().skip(5).take(right_count).enumerate() {
        if i < right_count {
            let proc_bar = create_progress_bar(proc.cpu_usage / 100.0, 5);
            let proc_text = format!(
                "{:15}  {}  {:.1}%",
                &proc.name[..proc.name.len().min(15)],
                proc_bar,
                proc.cpu_usage
            );
            let proc_para = Paragraph::new(proc_text).style(Styles::secondary());
            let proc_area = Rect {
                x: columns[1].x,
                y: columns[1].y + i as u16,
                width: columns[1].width,
                height: 1,
            };
            f.render_widget(proc_para, proc_area);
        }
    }
}

fn create_progress_bar(value: f32, width: usize) -> String {
    let filled = (value.min(1.0).max(0.0) * width as f32).round() as usize;
    let empty = width.saturating_sub(filled);
    format!("{}{}", "█".repeat(filled), "░".repeat(empty))
}

fn create_speed_bar(value: f64, width: usize) -> String {
    let filled = (value.min(1.0).max(0.0) * width as f64).round() as usize;
    let empty = width.saturating_sub(filled);
    format!("{}{}", "▮".repeat(filled), "▯".repeat(empty))
}
