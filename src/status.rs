//! System status and health metrics

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::time::{Duration, Instant};
use sysinfo::System;

// Thread-local state for tracking metrics over time (for delta calculations)
thread_local! {
    static METRICS_STATE: RefCell<MetricsState> = RefCell::new(MetricsState::default());
}

#[derive(Debug)]
struct MetricsState {
    network: NetworkState,
    disk: DiskState,
    last_update: Instant,
}

impl Default for MetricsState {
    fn default() -> Self {
        Self {
            network: NetworkState::default(),
            disk: DiskState::default(),
            last_update: Instant::now(),
        }
    }
}

#[derive(Debug, Default)]
struct NetworkState {
    previous_received: u64,
    previous_transmitted: u64,
}

#[derive(Debug, Default)]
struct DiskState {
    _previous_read: u64,
    _previous_written: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStatus {
    pub health_score: u8,
    pub hardware: HardwareInfo,
    pub cpu: CpuMetrics,
    pub memory: MemoryMetrics,
    pub disk: DiskMetrics,
    pub power: Option<PowerMetrics>,
    pub network: NetworkMetrics,
    pub processes: Vec<ProcessInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareInfo {
    pub device_name: String,
    pub cpu_model: String,
    pub cpu_cores: usize,
    pub total_memory_gb: f64,
    pub os_name: String,
    pub os_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuMetrics {
    pub total_usage: f32,
    pub load_avg_1min: f64,
    pub load_avg_5min: f64,
    pub load_avg_15min: f64,
    pub cores: Vec<CoreMetrics>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreMetrics {
    pub id: usize,
    pub usage: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryMetrics {
    pub used_gb: f64,
    pub total_gb: f64,
    pub free_gb: f64,
    pub available_gb: f64,
    pub used_percent: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskMetrics {
    pub used_gb: f64,
    pub total_gb: f64,
    pub free_gb: f64,
    pub used_percent: f32,
    pub read_speed_mb: f64,
    pub write_speed_mb: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerMetrics {
    pub level_percent: f32,
    pub status: String,
    pub health: String,
    pub temperature_celsius: Option<f32>,
    pub cycles: Option<u32>,
    pub chemistry: Option<String>,
    pub design_capacity_mwh: Option<f64>,
    pub full_charge_capacity_mwh: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkMetrics {
    pub download_mb: f64,
    pub upload_mb: f64,
    pub proxy: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub name: String,
    pub cpu_usage: f32,
    pub memory_usage: f32,
}

/// Gather current system status
pub fn gather_status(system: &mut System) -> Result<SystemStatus> {
    // Use thread-local state for delta tracking
    METRICS_STATE.with(|state_cell| {
        let mut state = state_cell.borrow_mut();
        let now = Instant::now();
        let elapsed = state.last_update.elapsed();
        
        // Refresh only what we need for performance
        // CPU needs two refreshes for accurate usage calculation
        system.refresh_cpu_all();
        
        // Small delay for CPU measurements (reduced from 100ms to 20ms for responsiveness)
        std::thread::sleep(Duration::from_millis(20));
        system.refresh_cpu_all();
        
        // Refresh other metrics
        system.refresh_memory();
        system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

        // Gather hardware info
        let hardware = gather_hardware_info(system);
        
        // Gather CPU metrics
        let cpu = gather_cpu_metrics(system);
        
        // Gather memory metrics
        let memory = gather_memory_metrics(system);
        
        // Gather disk metrics (with state tracking for I/O speeds)
        let disk = gather_disk_metrics(system, &mut state.disk, elapsed);
        
        // Gather power/battery metrics
        let power = gather_power_metrics();
        
        // Gather network metrics (with state tracking for speeds)
        let network = gather_network_metrics(&mut state.network, elapsed);
        
        // Gather top processes (show 10 instead of 5)
        let processes = gather_top_processes(system, 10);
        
        // Calculate health score
        let health_score = calculate_health_score(&cpu, &memory, &disk, &power);
        
        // Update last update time
        state.last_update = now;
        
        Ok(SystemStatus {
            health_score,
            hardware,
            cpu,
            memory,
            disk,
            power,
            network,
            processes,
        })
    })
}

fn gather_hardware_info(system: &System) -> HardwareInfo {
    let device_name = if cfg!(windows) {
        std::env::var("COMPUTERNAME").unwrap_or_else(|_| "Unknown".to_string())
    } else if cfg!(target_os = "macos") {
        std::env::var("HOSTNAME").unwrap_or_else(|_| "Mac".to_string())
    } else {
        std::env::var("HOSTNAME").unwrap_or_else(|_| "Linux".to_string())
    };
    
    let cpu_model = system
        .cpus()
        .first()
        .map(|c| c.name().to_string())
        .unwrap_or_else(|| "Unknown CPU".to_string());
    
    let cpu_cores = system.cpus().len();
    
    let total_memory_gb = (system.total_memory() as f64) / (1024.0 * 1024.0 * 1024.0);
    
    let os_name = if cfg!(windows) {
        "Windows".to_string()
    } else if cfg!(target_os = "macos") {
        "macOS".to_string()
    } else {
        "Linux".to_string()
    };
    
    let os_version = sysinfo::System::long_os_version()
        .unwrap_or_else(|| "Unknown".to_string());
    
    HardwareInfo {
        device_name,
        cpu_model,
        cpu_cores,
        total_memory_gb,
        os_name,
        os_version,
    }
}

fn gather_cpu_metrics(system: &System) -> CpuMetrics {
    let cpus = system.cpus();
    let total_usage = if !cpus.is_empty() {
        cpus.iter().map(|c| c.cpu_usage()).sum::<f32>() / cpus.len() as f32
    } else {
        0.0
    };
    
    let load_avg = sysinfo::System::load_average();
    
    let cores: Vec<CoreMetrics> = cpus
        .iter()
        .enumerate()
        .map(|(i, cpu)| CoreMetrics {
            id: i,
            usage: cpu.cpu_usage(),
        })
        .collect();
    
    CpuMetrics {
        total_usage,
        load_avg_1min: load_avg.one,
        load_avg_5min: load_avg.five,
        load_avg_15min: load_avg.fifteen,
        cores,
    }
}

fn gather_memory_metrics(system: &System) -> MemoryMetrics {
    let total_bytes = system.total_memory();
    let used_bytes = system.used_memory();
    let free_bytes = system.free_memory();
    let available_bytes = system.available_memory();
    
    let total_gb = (total_bytes as f64) / (1024.0 * 1024.0 * 1024.0);
    let used_gb = (used_bytes as f64) / (1024.0 * 1024.0 * 1024.0);
    let free_gb = (free_bytes as f64) / (1024.0 * 1024.0 * 1024.0);
    let available_gb = (available_bytes as f64) / (1024.0 * 1024.0 * 1024.0);
    
    let used_percent = if total_bytes > 0 {
        (used_bytes as f32 / total_bytes as f32) * 100.0
    } else {
        0.0
    };
    
    MemoryMetrics {
        used_gb,
        total_gb,
        free_gb,
        available_gb,
        used_percent,
    }
}

fn gather_disk_metrics(_system: &mut System, _state: &mut DiskState, _elapsed: Duration) -> DiskMetrics {
    // sysinfo 0.32 API - use Disks struct separately
    use sysinfo::Disks;
    
    let mut disks = Disks::new_with_refreshed_list();
    disks.refresh();
    
    let mut total_bytes = 0u64;
    let mut used_bytes = 0u64;
    
    // Iterate over all disks and sum up totals
    for disk in disks.list() {
        total_bytes += disk.total_space();
        used_bytes += disk.total_space().saturating_sub(disk.available_space());
    }
    
    // Note: Disk I/O stats (read/write speeds) require sysinfo 0.37.2+
    // In sysinfo 0.32, disk I/O is only available per-process, not per-disk
    // For now, we'll show 0 for I/O speeds - this can be enhanced later by:
    // 1. Upgrading to sysinfo 0.37.2+ (has Disk::usage() method), or
    // 2. Summing process disk usage (expensive), or
    // 3. Using platform-specific APIs (Windows: Performance Counters, Linux: /proc/diskstats)
    let read_speed_mb = 0.0;
    let write_speed_mb = 0.0;
    
    // Calculate usage percentages
    let total_gb = (total_bytes as f64) / (1024.0 * 1024.0 * 1024.0);
    let used_gb = (used_bytes as f64) / (1024.0 * 1024.0 * 1024.0);
    let free_gb = total_gb - used_gb;
    let used_percent = if total_bytes > 0 {
        (used_bytes as f32 / total_bytes as f32) * 100.0
    } else {
        0.0
    };
    
    DiskMetrics {
        used_gb,
        total_gb,
        free_gb,
        used_percent,
        read_speed_mb,
        write_speed_mb,
    }
}

#[cfg(feature = "battery")]
fn gather_power_metrics() -> Option<PowerMetrics> {
    use battery::{Manager, units::{ratio::percent, energy::watt_hour}};
    
    let manager = Manager::new().ok()?;
    let mut batteries = manager.batteries().ok()?;
    let battery = batteries.next()?.ok()?;
    
    let level_percent = battery.state_of_charge().get::<percent>();
    let status = format!("{:?}", battery.state());
    // Battery 0.7 uses state_of_health() instead of health()
    let health_percent = battery.state_of_health().get::<percent>();
    let health = if health_percent >= 80.0 {
        "Good".to_string()
    } else if health_percent >= 50.0 {
        "Fair".to_string()
    } else {
        "Poor".to_string()
    };
    
    // Temperature in battery 0.7 is in Kelvin, convert to Celsius
    let temperature = battery.temperature().map(|t| {
        let kelvin = t.get::<battery::units::thermodynamic_temperature::kelvin>();
        kelvin - 273.15 // Convert Kelvin to Celsius
    });
    
    let cycles = battery.cycle_count();
    
    // Chemistry/Technology - returns Technology directly (not Option)
    let chemistry = Some(format!("{:?}", battery.technology()));
    
    // Design capacity (original capacity when new) - returns Energy directly, convert from Wh to mWh
    // Convert f32 to f64 for the struct field
    let design_capacity_mwh = Some(battery.energy_full_design().get::<watt_hour>() as f64 * 1000.0);
    
    // Full charge capacity (current maximum capacity) - returns Energy directly, convert from Wh to mWh
    // Convert f32 to f64 for the struct field
    let full_charge_capacity_mwh = Some(battery.energy_full().get::<watt_hour>() as f64 * 1000.0);
    
    Some(PowerMetrics {
        level_percent: level_percent as f32,
        status,
        health,
        temperature_celsius: temperature,
        cycles,
        chemistry,
        design_capacity_mwh,
        full_charge_capacity_mwh,
    })
}

#[cfg(not(feature = "battery"))]
fn gather_power_metrics() -> Option<PowerMetrics> {
    // Battery information not available without battery crate
    None
}

fn gather_network_metrics(state: &mut NetworkState, elapsed: Duration) -> NetworkMetrics {
    use sysinfo::Networks;
    
    // Create Networks instance and refresh data
    let mut networks = Networks::new_with_refreshed_list();
    networks.refresh(); // sysinfo 0.32 refresh() takes no arguments
    
    let mut current_received = 0u64;
    let mut current_transmitted = 0u64;
    
    // Sum up all network interfaces
    for (_interface, network) in &networks {
        current_received += network.received();
        current_transmitted += network.transmitted();
    }
    
    // Calculate speeds (MB/s) using delta tracking
    let elapsed_secs = elapsed.as_secs_f64();
    let download_mb = if elapsed_secs > 0.1 {
        // Only calculate if enough time has passed (100ms minimum)
        ((current_received.saturating_sub(state.previous_received)) as f64 / elapsed_secs) / (1024.0 * 1024.0)
    } else {
        0.0
    };
    
    let upload_mb = if elapsed_secs > 0.1 {
        ((current_transmitted.saturating_sub(state.previous_transmitted)) as f64 / elapsed_secs) / (1024.0 * 1024.0)
    } else {
        0.0
    };
    
    // Update state for next call
    state.previous_received = current_received;
    state.previous_transmitted = current_transmitted;
    
    // Check for proxy (Windows)
    let proxy = if cfg!(windows) {
        std::env::var("HTTP_PROXY")
            .or_else(|_| std::env::var("HTTPS_PROXY"))
            .ok()
    } else {
        None
    };
    
    NetworkMetrics {
        download_mb,
        upload_mb,
        proxy,
    }
}

fn gather_top_processes(system: &System, limit: usize) -> Vec<ProcessInfo> {
    let mut processes: Vec<ProcessInfo> = system
        .processes()
        .iter()
        .map(|(_, proc)| {
            let name = proc.name().to_string_lossy().to_string();
            let cpu_usage = proc.cpu_usage();
            let memory_usage = (proc.memory() as f32) / (system.total_memory() as f32) * 100.0;
            
            ProcessInfo {
                name,
                cpu_usage,
                memory_usage,
            }
        })
        .collect();
    
    // Sort by CPU usage descending
    processes.sort_by(|a, b| b.cpu_usage.partial_cmp(&a.cpu_usage).unwrap_or(std::cmp::Ordering::Equal));
    
    processes.into_iter().take(limit).collect()
}

/// Calculate health score (0-100) based on system metrics
fn calculate_health_score(
    cpu: &CpuMetrics,
    memory: &MemoryMetrics,
    disk: &DiskMetrics,
    power: &Option<PowerMetrics>,
) -> u8 {
    // CPU score: lower usage is better (0-100% usage -> 100-0 score)
    let cpu_score = (100.0 - cpu.total_usage).max(0.0);
    
    // Memory score: lower usage is better
    let memory_score = (100.0 - memory.used_percent).max(0.0);
    
    // Disk score: more free space is better
    let disk_free_percent = if disk.total_gb > 0.0 {
        (disk.free_gb / disk.total_gb) * 100.0
    } else {
        0.0
    };
    let disk_score = disk_free_percent;
    
    // Temperature score (if available): lower is better
    let temp_score = if let Some(power) = power {
        if let Some(temp) = power.temperature_celsius {
            // Normalize: 0°C = 100, 100°C = 0 (linear)
            (100.0 - temp).max(0.0).min(100.0)
        } else {
            100.0 // No temp data = assume good
        }
    } else {
        100.0 // No battery = assume good
    };
    
    // I/O load score (simplified - based on disk usage)
    let io_score = 100.0 - (disk.used_percent * 0.1).min(10.0);
    
    // Weighted average
    let total_score = (cpu_score * 0.25)
        + (memory_score * 0.25)
        + (disk_score as f32 * 0.25)
        + (temp_score * 0.15)
        + (io_score * 0.10);
    
    total_score.round() as u8
}

/// Format status for CLI output
pub fn format_cli_output(status: &SystemStatus) -> String {
    let mut output = String::new();
    
    // Header
    let health_indicator = match status.health_score {
        80..=100 => "●",
        60..=79 => "○",
        40..=59 => "◐",
        _ => "◯",
    };
    
    output.push_str(&format!(
        "Wole Status  Health {} {}  {} · {} · {:.1}GB · {}\n\n",
        health_indicator,
        status.health_score,
        status.hardware.device_name,
        status.hardware.cpu_model,
        status.hardware.total_memory_gb,
        status.hardware.os_name
    ));
    
    // CPU and Memory side by side
    output.push_str("CPU                                    Memory\n");
    
    // CPU Total
    let cpu_bar = create_progress_bar(status.cpu.total_usage / 100.0, 20);
    output.push_str(&format!(
        "Total   {}  {:.1}%    ",
        cpu_bar, status.cpu.total_usage
    ));
    
    // Memory Used
    let mem_bar = create_progress_bar(status.memory.used_percent / 100.0, 20);
    output.push_str(&format!(
        "Used    {}  {:.1}%\n",
        mem_bar, status.memory.used_percent
    ));
    
    // CPU Load
    output.push_str(&format!(
        "Load    {:.2} / {:.2} / {:.2} ({} cores)    ",
        status.cpu.load_avg_1min,
        status.cpu.load_avg_5min,
        status.cpu.load_avg_15min,
        status.cpu.cores.len()
    ));
    
    // Memory Total
    output.push_str(&format!(
        "Total   {:.1} / {:.1} GB\n",
        status.memory.used_gb, status.memory.total_gb
    ));
    
    // CPU Core (first core)
    if let Some(core) = status.cpu.cores.first() {
        let core_bar = create_progress_bar(core.usage / 100.0, 20);
        output.push_str(&format!(
            "Core {}  {}  {:.1}%    ",
            core.id + 1,
            core_bar,
            core.usage
        ));
    } else {
        output.push_str("        ");
    }
    
    // Memory Free
    output.push_str(&format!(
        "Free    {:.1} GB\n\n",
        status.memory.free_gb
    ));
    
    // Disk and Power side by side
    output.push_str("Disk                                   Power\n");
    
    // Disk Used
    let disk_bar = create_progress_bar(status.disk.used_percent / 100.0, 20);
    output.push_str(&format!(
        "Used    {}  {:.1}%    ",
        disk_bar, status.disk.used_percent
    ));
    
    // Power Level
    if let Some(power) = &status.power {
        let power_bar = create_progress_bar(power.level_percent / 100.0, 20);
        output.push_str(&format!(
            "Level   {}  {:.0}%\n",
            power_bar, power.level_percent
        ));
        
        // Disk Free
        output.push_str(&format!(
            "Free    {:.1} GB                       ",
            status.disk.free_gb
        ));
        
        // Power Status
        output.push_str(&format!("Status  {}\n", power.status));
        
        // Disk Read/Write
        output.push_str(&format!(
            "Read    {}  {:.1} MB/s                  ",
            create_speed_bar(status.disk.read_speed_mb / 100.0, 5),
            status.disk.read_speed_mb
        ));
        
        // Power Health
        output.push_str(&format!("Health  {}", power.health));
        
        if let Some(temp) = power.temperature_celsius {
            output.push_str(&format!(" · {:.0}°C", temp));
        }
        
        output.push('\n');
        
        // Disk Write
        output.push_str(&format!(
            "Write   {}  {:.1} MB/s                  ",
            create_speed_bar(status.disk.write_speed_mb / 100.0, 5),
            status.disk.write_speed_mb
        ));
        
        // Power Cycles
        if let Some(cycles) = power.cycles {
            output.push_str(&format!("Cycles  {}\n", cycles));
        } else {
            output.push('\n');
        }
        
        // Power Chemistry
        if let Some(ref chemistry) = power.chemistry {
            output.push_str(&format!(
                "                                    Chem    {}\n",
                chemistry
            ));
        }
        
        // Power Design Capacity
        if let Some(design_cap) = power.design_capacity_mwh {
            output.push_str(&format!(
                "                                    Design  {:.0} mWh\n",
                design_cap
            ));
        }
        
        // Power Full Charge Capacity
        if let Some(full_cap) = power.full_charge_capacity_mwh {
            output.push_str(&format!(
                "                                    Full    {:.0} mWh\n",
                full_cap
            ));
        }
    } else {
        output.push_str("Level   N/A\n");
        output.push_str(&format!(
            "Free    {:.1} GB                       ",
            status.disk.free_gb
        ));
        output.push_str("Status  Plugged In\n");
        output.push_str(&format!(
            "Read    {}  {:.1} MB/s\n",
            create_speed_bar(status.disk.read_speed_mb / 100.0, 5),
            status.disk.read_speed_mb
        ));
    }
    
    // Disk Write
    output.push_str(&format!(
        "Write   {}  {:.1} MB/s\n\n",
        create_speed_bar(status.disk.write_speed_mb / 100.0, 5),
        status.disk.write_speed_mb
    ));
    
    // Network
    output.push_str("Network\n");
    output.push_str(&format!(
        "Down    {}  {:.1} MB/s\n",
        create_speed_bar(status.network.download_mb / 10.0, 5),
        status.network.download_mb
    ));
    output.push_str(&format!(
        "Up      {}  {:.1} MB/s\n",
        create_speed_bar(status.network.upload_mb / 10.0, 5),
        status.network.upload_mb
    ));
    
    if let Some(proxy) = &status.network.proxy {
        output.push_str(&format!("Proxy   {}\n", proxy));
    }
    
    output.push('\n');
    
    // Top Processes
    output.push_str("Top Processes\n");
    for proc in &status.processes {
        let proc_bar = create_progress_bar(proc.cpu_usage / 100.0, 5);
        output.push_str(&format!(
            "{}  {}  {:.1}%\n",
            format!("{:15}", proc.name),
            proc_bar,
            proc.cpu_usage
        ));
    }
    
    output
}

fn create_progress_bar(value: f32, width: usize) -> String {
    let filled = (value * width as f32).round() as usize;
    let empty = width.saturating_sub(filled);
    
    format!("{}{}", "█".repeat(filled), "░".repeat(empty))
}

fn create_speed_bar(value: f64, width: usize) -> String {
    let filled = (value.min(1.0).max(0.0) * width as f64).round() as usize;
    let empty = width.saturating_sub(filled);
    format!("{}{}", "▮".repeat(filled), "▯".repeat(empty))
}
