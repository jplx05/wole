//! System status and health metrics

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};
use sysinfo::System;

// Thread-local state for tracking metrics over time (for delta calculations)
thread_local! {
    static METRICS_STATE: RefCell<MetricsState> = RefCell::new(MetricsState::default());
}

#[cfg(windows)]
use std::sync::RwLock;

#[cfg(windows)]
lazy_static::lazy_static! {
    static ref DISK_BREAKDOWN_CACHE: RwLock<Option<(DiskBreakdown, std::time::Instant)>> =
        RwLock::new(None);
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

#[cfg(windows)]
use windows::{
    core::w,
    Win32::{
        Foundation::ERROR_SUCCESS,
        System::Performance::{
            PdhAddCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterValue,
            PdhOpenQueryW, PDH_FMT_COUNTERVALUE, PDH_FMT_DOUBLE,
        },
    },
};

#[derive(Default)]
struct DiskState {
    #[cfg(windows)]
    io_monitor: Option<WindowsDiskIOMonitor>,
    #[cfg(not(windows))]
    _phantom: std::marker::PhantomData<()>,
}

#[cfg(windows)]
impl std::fmt::Debug for DiskState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DiskState")
            .field("io_monitor", &"<WindowsDiskIOMonitor>")
            .finish()
    }
}

#[cfg(not(windows))]
impl std::fmt::Debug for DiskState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DiskState")
            .field("_phantom", &self._phantom)
            .finish()
    }
}

#[cfg(windows)]
struct WindowsDiskIOMonitor {
    query: isize,         // PDH_HQUERY is a type alias for isize
    read_counter: isize,  // PDH_HCOUNTER is a type alias for isize
    write_counter: isize, // PDH_HCOUNTER is a type alias for isize
    initialized: bool,
}

#[cfg(windows)]
impl WindowsDiskIOMonitor {
    fn new() -> Result<Self, String> {
        unsafe {
            let mut query: isize = 0;
            let status = PdhOpenQueryW(None, 0, &mut query);

            if status != ERROR_SUCCESS.0 {
                return Err(format!("Failed to open PDH query: {}", status));
            }

            // Add Disk Read Bytes/sec counter for _Total (all physical disks)
            let mut read_counter: isize = 0;
            let read_status = PdhAddCounterW(
                query,
                w!("\\PhysicalDisk(_Total)\\Disk Read Bytes/sec"),
                0,
                &mut read_counter,
            );

            if read_status != ERROR_SUCCESS.0 {
                let _ = PdhCloseQuery(query);
                return Err(format!("Failed to add read counter: {}", read_status));
            }

            // Add Disk Write Bytes/sec counter for _Total (all physical disks)
            let mut write_counter: isize = 0;
            let write_status = PdhAddCounterW(
                query,
                w!("\\PhysicalDisk(_Total)\\Disk Write Bytes/sec"),
                0,
                &mut write_counter,
            );

            if write_status != ERROR_SUCCESS.0 {
                let _ = PdhCloseQuery(query);
                return Err(format!("Failed to add write counter: {}", write_status));
            }

            Ok(Self {
                query,
                read_counter,
                write_counter,
                initialized: false,
            })
        }
    }

    fn collect(&mut self, delay: Duration) -> Result<(f64, f64), String> {
        unsafe {
            // First collection - initialize counters
            if !self.initialized {
                let status = PdhCollectQueryData(self.query);
                if status != ERROR_SUCCESS.0 {
                    return Err(format!("Failed to collect initial data: {}", status));
                }
                self.initialized = true;
                std::thread::sleep(delay);
            }

            // Second collection - get actual values
            let status = PdhCollectQueryData(self.query);
            if status != ERROR_SUCCESS.0 {
                return Err(format!("Failed to collect query data: {}", status));
            }

            // Get read bytes/sec (already in bytes/sec, pre-calculated)
            let mut read_value = PDH_FMT_COUNTERVALUE::default();
            let read_status = PdhGetFormattedCounterValue(
                self.read_counter,
                PDH_FMT_DOUBLE,
                None,
                &mut read_value,
            );

            let read_bytes_per_sec = if read_status == ERROR_SUCCESS.0 {
                read_value.Anonymous.doubleValue
            } else {
                0.0
            };

            // Get write bytes/sec (already in bytes/sec, pre-calculated)
            let mut write_value = PDH_FMT_COUNTERVALUE::default();
            let write_status = PdhGetFormattedCounterValue(
                self.write_counter,
                PDH_FMT_DOUBLE,
                None,
                &mut write_value,
            );

            let write_bytes_per_sec = if write_status == ERROR_SUCCESS.0 {
                write_value.Anonymous.doubleValue
            } else {
                0.0
            };

            // Convert bytes/sec to MB/sec
            Ok((
                read_bytes_per_sec / 1_000_000.0,
                write_bytes_per_sec / 1_000_000.0,
            ))
        }
    }
}

#[cfg(windows)]
impl Drop for WindowsDiskIOMonitor {
    fn drop(&mut self) {
        unsafe {
            // PdhCloseQuery handles invalid handles gracefully, so we can always call it
            let _ = PdhCloseQuery(self.query);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStatus {
    pub health_score: u8,
    pub hardware: HardwareInfo,
    pub cpu: CpuMetrics,
    pub memory: MemoryMetrics,
    pub disk: DiskMetrics,
    pub disks: Vec<DiskInfo>,
    pub power: Option<PowerMetrics>,
    pub network: NetworkMetrics,
    pub network_interfaces: Vec<NetworkInterface>,
    pub temperature_sensors: Vec<TemperatureSensor>,
    pub gpu: Option<GpuMetrics>,
    pub processes: Vec<ProcessInfo>,
    #[cfg(windows)]
    pub top_io_processes: Vec<ProcessIOMetrics>,
    #[cfg(windows)]
    pub disk_breakdown: Option<DiskBreakdown>,
    #[cfg(windows)]
    pub boot_info: Option<BootInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareInfo {
    pub device_name: String,
    pub cpu_model: String,
    pub cpu_cores: usize,
    pub total_memory_gb: f64,
    pub os_name: String,
    pub os_version: String,
    pub uptime_seconds: u64,
    pub boot_time_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuMetrics {
    pub total_usage: f32,
    pub load_avg_1min: f64,
    pub load_avg_5min: f64,
    pub load_avg_15min: f64,
    pub frequency_mhz: Option<u64>,
    pub vendor_id: String,
    pub brand: String,
    pub process_count: usize,
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
    pub swap_used_gb: f64,
    pub swap_total_gb: f64,
    pub swap_percent: f32,
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
    pub time_to_empty_seconds: Option<u64>,
    pub time_to_full_seconds: Option<u64>,
    pub voltage_volts: Option<f32>,
    pub energy_rate_watts: Option<f32>,
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
    pub pid: u32,
    pub cpu_usage: f32,
    pub memory_usage: f32,
    pub memory_mb: f64,
    pub disk_read_mb: f64,
    pub disk_write_mb: f64,
    #[cfg(windows)]
    pub handle_count: Option<u32>,
    #[cfg(windows)]
    pub page_faults_per_sec: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessIOMetrics {
    pub name: String,
    pub pid: u32,
    pub read_bytes_per_sec: f64,
    pub write_bytes_per_sec: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskInfo {
    pub name: String,
    pub mount_point: String,
    pub filesystem: String,
    pub disk_type: String,
    pub is_removable: bool,
    pub used_gb: f64,
    pub total_gb: f64,
    pub free_gb: f64,
    pub used_percent: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInterface {
    pub name: String,
    pub mac_address: Option<String>,
    pub ip_addresses: Vec<String>,
    pub connection_type: Option<String>,
    pub is_up: bool,
    pub download_mb: f64,
    pub upload_mb: f64,
    pub total_received_mb: f64,
    pub total_sent_mb: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemperatureSensor {
    pub label: String,
    pub temperature_celsius: f32,
    pub max_celsius: Option<f32>,
    pub critical_celsius: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuMetrics {
    pub name: String,
    pub vendor: String,
    pub utilization_percent: Option<f32>, // Overall GPU utilization %
    pub render_engine_percent: Option<f32>, // 3D engine utilization %
    pub copy_engine_percent: Option<f32>, // Copy engine utilization %
    pub compute_engine_percent: Option<f32>, // Compute engine utilization %
    pub video_engine_percent: Option<f32>, // Video decode/encode engine utilization %
    pub memory_dedicated_used_mb: Option<u64>, // Dedicated VRAM used
    pub memory_dedicated_total_mb: Option<u64>, // Dedicated VRAM total
    pub memory_shared_used_mb: Option<u64>, // Shared system RAM used by GPU
    pub memory_shared_total_mb: Option<u64>, // Shared system RAM total
    pub memory_utilization_percent: Option<f32>, // Memory utilization %
    pub temperature_celsius: Option<f32>,
    pub temperature_threshold_celsius: Option<f32>, // Temperature threshold/max
    pub clock_speed_mhz: Option<u64>,
    pub power_usage_watts: Option<f32>,
    pub driver_version: Option<String>,
    pub pci_bus: Option<u32>,
}

#[cfg(windows)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskBreakdown {
    pub volume: String, // e.g., "C:\"
    pub categories: Vec<DiskCategory>,
    pub total_disk_gb: f64,
    pub cached_at: Option<chrono::DateTime<chrono::Local>>,
}

#[cfg(windows)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskCategory {
    pub name: String,
    pub size_gb: f64,
    pub percent: f32,
}

#[cfg(windows)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootInfo {
    pub uptime_seconds: u64,
    pub last_boot_time: chrono::DateTime<chrono::Local>,
    pub boot_duration_seconds: u64,
    pub shutdown_type: String,
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

        // Gather per-disk details
        let disks = gather_disk_details();

        // Gather power/battery metrics
        let power = gather_power_metrics();

        // Gather network metrics (with state tracking for speeds)
        let network = gather_network_metrics(&mut state.network, elapsed);

        // Gather network interface details
        let network_interfaces = gather_network_interfaces(&mut state.network, elapsed);

        // Gather temperature sensors
        let temperature_sensors = gather_temperature_sensors();

        // GPU implementation commented out - not polished yet
        // Gather GPU metrics
        let gpu = None; // gather_gpu_metrics();

        // Gather top processes (show 10 instead of 5)
        #[cfg(windows)]
        let (handle_counts, page_faults) = {
            let handles = gather_process_handle_counts();
            let faults = gather_process_page_faults();
            (handles, faults)
        };

        #[cfg(windows)]
        let processes = gather_top_processes_with_wmi(system, 10, &handle_counts, &page_faults);
        #[cfg(not(windows))]
        let processes = gather_top_processes(system, 10);

        // Gather top I/O processes via WMI
        #[cfg(windows)]
        let top_io_processes = gather_process_io_metrics();
        #[cfg(not(windows))]
        let top_io_processes = Vec::new();

        // Gather disk breakdown (cached, expensive operation)
        // Only use cached data to avoid blocking - don't scan on first load
        #[cfg(windows)]
        let disk_breakdown = gather_disk_breakdown_cached_only();

        // Gather boot info
        #[cfg(windows)]
        let boot_info = gather_boot_info();

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
            disks,
            power,
            network,
            network_interfaces,
            temperature_sensors,
            gpu,
            processes,
            #[cfg(windows)]
            top_io_processes,
            #[cfg(windows)]
            disk_breakdown,
            #[cfg(windows)]
            boot_info,
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

    let os_version = sysinfo::System::long_os_version().unwrap_or_else(|| "Unknown".to_string());

    let uptime_seconds = sysinfo::System::uptime();
    let boot_time_seconds = sysinfo::System::boot_time();

    HardwareInfo {
        device_name,
        cpu_model,
        cpu_cores,
        total_memory_gb,
        os_name,
        os_version,
        uptime_seconds,
        boot_time_seconds,
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

    // Get CPU frequency, vendor, and brand from first CPU
    let frequency_mhz = cpus.first().map(|c| {
        // sysinfo 0.32 uses frequency() which returns u64 MHz
        c.frequency()
    });

    let vendor_id = cpus
        .first()
        .map(|c| c.vendor_id().to_string())
        .unwrap_or_else(|| "Unknown".to_string());

    let brand = cpus
        .first()
        .map(|c| c.brand().to_string())
        .unwrap_or_else(|| "Unknown".to_string());

    // Get process count
    let process_count = system.processes().len();

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
        frequency_mhz,
        vendor_id,
        brand,
        process_count,
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

    // Swap/page file memory
    let swap_total_bytes = system.total_swap();
    let swap_used_bytes = system.used_swap();
    let swap_total_gb = (swap_total_bytes as f64) / (1024.0 * 1024.0 * 1024.0);
    let swap_used_gb = (swap_used_bytes as f64) / (1024.0 * 1024.0 * 1024.0);
    let swap_percent = if swap_total_bytes > 0 {
        (swap_used_bytes as f32 / swap_total_bytes as f32) * 100.0
    } else {
        0.0
    };

    MemoryMetrics {
        used_gb,
        total_gb,
        free_gb,
        available_gb,
        used_percent,
        swap_used_gb,
        swap_total_gb,
        swap_percent,
    }
}

fn gather_disk_details() -> Vec<DiskInfo> {
    use sysinfo::{DiskKind, Disks};

    let mut disks = Disks::new_with_refreshed_list();
    disks.refresh();

    disks
        .list()
        .iter()
        .map(|disk| {
            let name = disk.name().to_string_lossy().to_string();
            let mount_point = disk.mount_point().display().to_string();
            let filesystem = disk.file_system().to_string_lossy().to_string();
            let disk_type = match disk.kind() {
                DiskKind::HDD => "HDD".to_string(),
                DiskKind::SSD => "SSD".to_string(),
                DiskKind::Unknown(_) => "Unknown".to_string(),
            };
            let is_removable = disk.is_removable();

            let total_bytes = disk.total_space();
            let available_bytes = disk.available_space();
            let used_bytes = total_bytes.saturating_sub(available_bytes);

            let total_gb = (total_bytes as f64) / (1024.0 * 1024.0 * 1024.0);
            let used_gb = (used_bytes as f64) / (1024.0 * 1024.0 * 1024.0);
            let free_gb = (available_bytes as f64) / (1024.0 * 1024.0 * 1024.0);
            let used_percent = if total_bytes > 0 {
                (used_bytes as f32 / total_bytes as f32) * 100.0
            } else {
                0.0
            };

            DiskInfo {
                name,
                mount_point,
                filesystem,
                disk_type,
                is_removable,
                used_gb,
                total_gb,
                free_gb,
                used_percent,
            }
        })
        .collect()
}

#[cfg(windows)]
fn gather_disk_io_speeds(state: &mut DiskState, _elapsed: Duration) -> (f64, f64) {
    // On Windows, use Performance Data Helper (PDH) API
    // PDH requires two samples to calculate rates - first sample initializes, second gives the rate

    if state.io_monitor.is_none() {
        // Initialize monitor on first call
        match WindowsDiskIOMonitor::new() {
            Ok(monitor) => {
                state.io_monitor = Some(monitor);
                // First call - just initialize, return zeros
                // Next call (after ~2 seconds) will return actual values
                return (0.0, 0.0);
            }
            Err(e) => {
                // If initialization fails, log and continue with zeros
                #[cfg(debug_assertions)]
                eprintln!("[DEBUG] Failed to initialize disk I/O monitor: {}", e);
                return (0.0, 0.0);
            }
        }
    }

    // Collect I/O stats - PDH needs a small delay between samples for accurate rates
    if let Some(ref mut monitor) = state.io_monitor {
        // Use a small delay (50ms) since we're called every ~2 seconds
        // This ensures we get fresh data without blocking too long
        match monitor.collect(Duration::from_millis(50)) {
            Ok((read_mb, write_mb)) => {
                // Values are already in MB/sec from collect()
                (read_mb, write_mb)
            }
            Err(e) => {
                #[cfg(debug_assertions)]
                eprintln!("[DEBUG] Failed to collect disk I/O: {}", e);
                (0.0, 0.0)
            }
        }
    } else {
        (0.0, 0.0)
    }
}

#[cfg(not(windows))]
fn gather_disk_io_speeds(_state: &mut DiskState, _elapsed: Duration) -> (f64, f64) {
    // On non-Windows, disk I/O stats require sysinfo 0.37.2+ or platform-specific APIs
    // For now, return zeros (can be enhanced with /proc/diskstats on Linux)
    (0.0, 0.0)
}

fn gather_disk_metrics(
    _system: &mut System,
    state: &mut DiskState,
    elapsed: Duration,
) -> DiskMetrics {
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

    // Get disk I/O speeds using platform-specific methods
    let (read_speed_mb, write_speed_mb) = gather_disk_io_speeds(state, elapsed);

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
    use battery::{
        units::{
            electric_potential::volt, energy::watt_hour, power::watt, ratio::percent, time::second,
        },
        Manager,
    };

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
    // Only show if it's not Unknown
    let chemistry = {
        let tech = battery.technology();
        let tech_str = format!("{:?}", tech);
        if tech_str == "Unknown" {
            None
        } else {
            Some(tech_str)
        }
    };

    // Design capacity (original capacity when new) - returns Energy directly, convert from Wh to mWh
    // Convert f32 to f64 for the struct field
    let design_capacity_mwh = Some(battery.energy_full_design().get::<watt_hour>() as f64 * 1000.0);

    // Full charge capacity (current maximum capacity) - returns Energy directly, convert from Wh to mWh
    // Convert f32 to f64 for the struct field
    let full_charge_capacity_mwh = Some(battery.energy_full().get::<watt_hour>() as f64 * 1000.0);

    // Time estimates
    let time_to_empty_seconds = battery.time_to_empty().map(|t| t.get::<second>() as u64);
    let time_to_full_seconds = battery.time_to_full().map(|t| t.get::<second>() as u64);

    // Voltage
    let voltage_volts = Some(battery.voltage().get::<volt>());

    // Energy rate (power draw/charge rate)
    let energy_rate_watts = Some(battery.energy_rate().get::<watt>());

    Some(PowerMetrics {
        level_percent,
        status,
        health,
        temperature_celsius: temperature,
        cycles,
        chemistry,
        design_capacity_mwh,
        full_charge_capacity_mwh,
        time_to_empty_seconds,
        time_to_full_seconds,
        voltage_volts,
        energy_rate_watts,
    })
}

#[cfg(not(feature = "battery"))]
fn gather_power_metrics() -> Option<PowerMetrics> {
    // Battery information not available without battery crate
    None
}

fn gather_network_interfaces(
    _state: &mut NetworkState,
    _elapsed: Duration,
) -> Vec<NetworkInterface> {
    use sysinfo::Networks;

    let mut networks = Networks::new_with_refreshed_list();
    networks.refresh();

    networks
        .iter()
        .map(|(name, network)| {
            let mac_address = {
                let mac = network.mac_address();
                Some(format!(
                    "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                    mac.0[0], mac.0[1], mac.0[2], mac.0[3], mac.0[4], mac.0[5]
                ))
            };

            let ip_addresses: Vec<String> = network
                .ip_networks()
                .iter()
                .map(|ip_net| ip_net.addr.to_string())
                .collect();

            // Infer connection type from interface name (sysinfo 0.32 doesn't have connection_type())
            let name_lower = name.to_lowercase();
            let connection_type = if name_lower.contains("wifi")
                || name_lower.contains("wireless")
                || name_lower.contains("wlan")
                || name_lower.contains("802.11")
                || name_lower.contains("wi-fi")
            {
                Some("WiFi".to_string())
            } else if name_lower.contains("ethernet")
                || name_lower.contains("lan")
                || name_lower.contains("eth")
                || name_lower.contains("local area")
            {
                Some("Ethernet".to_string())
            } else if name_lower.contains("vethernet") || name_lower.contains("veth") {
                Some("Virtual".to_string())
            } else {
                None
            };

            // Check if interface is up (has IPs and has traffic)
            let is_up =
                !ip_addresses.is_empty() && (network.received() > 0 || network.transmitted() > 0);

            let received = network.received();
            let transmitted = network.transmitted();

            // Calculate speeds (simplified - would need per-interface state tracking for accuracy)
            // For now, show 0.0 as we don't track per-interface deltas
            let download_mb = 0.0;
            let upload_mb = 0.0;

            let total_received_mb = (received as f64) / (1024.0 * 1024.0);
            let total_sent_mb = (transmitted as f64) / (1024.0 * 1024.0);

            NetworkInterface {
                name: name.to_string(),
                mac_address,
                ip_addresses,
                connection_type,
                is_up,
                download_mb,
                upload_mb,
                total_received_mb,
                total_sent_mb,
            }
        })
        .collect()
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
        ((current_received.saturating_sub(state.previous_received)) as f64 / elapsed_secs)
            / (1024.0 * 1024.0)
    } else {
        0.0
    };

    let upload_mb = if elapsed_secs > 0.1 {
        ((current_transmitted.saturating_sub(state.previous_transmitted)) as f64 / elapsed_secs)
            / (1024.0 * 1024.0)
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

fn gather_temperature_sensors() -> Vec<TemperatureSensor> {
    use sysinfo::Components;

    let components = Components::new_with_refreshed_list();

    components
        .list()
        .iter()
        .map(|component| TemperatureSensor {
            label: component.label().to_string(),
            temperature_celsius: component.temperature(),
            max_celsius: Some(component.max()),
            critical_celsius: component.critical(),
        })
        .collect()
}

// GPU implementation commented out - not polished yet
pub fn gather_gpu_metrics() -> Option<GpuMetrics> {
    None
    // #[cfg(windows)]
    // {
    //     // Try NVIDIA first (nvidia-smi)
    //     if let Some(nvidia_metrics) = gather_nvidia_gpu_metrics() {
    //         return Some(nvidia_metrics);
    //     }
    //
    //     // Fallback to DXGI for other GPUs
    //     gather_dxgi_gpu_metrics()
    // }
    //
    // #[cfg(not(windows))]
    // {
    //     // On non-Windows, try to detect NVIDIA via nvidia-smi
    //     gather_nvidia_gpu_metrics()
    // }
}

// GPU implementation commented out - not polished yet
// All GPU-related functions below are commented out
/*#[cfg(windows)]
fn gather_nvidia_gpu_metrics() -> Option<GpuMetrics> {
    use std::process::Command;

    // Check if nvidia-smi exists
    let output = Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,utilization.gpu,utilization.memory,memory.used,memory.total,temperature.gpu,clocks.current.graphics,power.draw",
            "--format=csv,noheader,nounits"
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8(output.stdout).ok()?;
    let line = stdout.lines().next()?;
    let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();

    if parts.len() < 8 {
        return None;
    }

    let name = parts[0].to_string();
    let utilization_gpu = parts[1].parse::<f32>().ok();
    let utilization_memory = parts[2].parse::<f32>().ok();
    let memory_used_mb = parts[3].parse::<u64>().ok();
    let memory_total_mb = parts[4].parse::<u64>().ok();
    let temperature = parts[5].parse::<f32>().ok();
    let clock_speed_mhz = parts[6].parse::<u64>().ok();
    let power_watts = parts[7].parse::<f32>().ok();

    Some(GpuMetrics {
        name,
        vendor: "NVIDIA".to_string(),
        utilization_percent: utilization_gpu,
        render_engine_percent: None, // nvidia-smi doesn't provide engine-level breakdown
        copy_engine_percent: None,
        compute_engine_percent: None,
        video_engine_percent: None,
        memory_dedicated_used_mb: memory_used_mb,
        memory_dedicated_total_mb: memory_total_mb,
        memory_shared_used_mb: None,
        memory_shared_total_mb: None,
        memory_utilization_percent: utilization_memory,
        temperature_celsius: temperature,
        temperature_threshold_celsius: None, // nvidia-smi doesn't provide threshold
        clock_speed_mhz,
        power_usage_watts: power_watts,
        driver_version: None,
        pci_bus: None,
    })
}

#[cfg(not(windows))]
fn gather_nvidia_gpu_metrics() -> Option<GpuMetrics> {
    use std::process::Command;

    // Check if nvidia-smi exists
    let output = Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,utilization.gpu,utilization.memory,memory.used,memory.total,temperature.gpu,clocks.current.graphics,power.draw",
            "--format=csv,noheader,nounits"
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8(output.stdout).ok()?;
    let line = stdout.lines().next()?;
    let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();

    if parts.len() < 8 {
        return None;
    }

    let name = parts[0].to_string();
    let utilization_gpu = parts[1].parse::<f32>().ok();
    let utilization_memory = parts[2].parse::<f32>().ok();
    let memory_used_mb = parts[3].parse::<u64>().ok();
    let memory_total_mb = parts[4].parse::<u64>().ok();
    let temperature = parts[5].parse::<f32>().ok();
    let clock_speed_mhz = parts[6].parse::<u64>().ok();
    let power_watts = parts[7].parse::<f32>().ok();

    Some(GpuMetrics {
        name,
        vendor: "NVIDIA".to_string(),
        utilization_percent: utilization_gpu,
        render_engine_percent: None, // nvidia-smi doesn't provide engine-level breakdown
        copy_engine_percent: None,
        compute_engine_percent: None,
        video_engine_percent: None,
        memory_dedicated_used_mb: memory_used_mb,
        memory_dedicated_total_mb: memory_total_mb,
        memory_shared_used_mb: None,
        memory_shared_total_mb: None,
        memory_utilization_percent: utilization_memory,
        temperature_celsius: temperature,
        temperature_threshold_celsius: None, // nvidia-smi doesn't provide threshold
        clock_speed_mhz,
        power_usage_watts: power_watts,
        driver_version: None,
        pci_bus: None,
    })
}

#[cfg(windows)]
fn dxgi_luid_patterns(desc: &windows::Win32::Graphics::Dxgi::DXGI_ADAPTER_DESC1) -> Vec<String> {
    // WMI/Perf counter instance strings use adapter LUID in the form:
    // - "00000000:0000C0D0" (HighPart:LowPart)  (common for WMI formatted perf classes)
    // - "luid_0x????????_0x????????" (sometimes with additional suffixes like "_phys_0" or embedded in pid_* strings)
    //
    // Unfortunately the ordering isn't consistently documented (high/low vs low/high),
    // so we generate both patterns and match by substring.
    let luid = desc.AdapterLuid;
    let low = luid.LowPart;
    let high = luid.HighPart as u32;

    vec![
        // Colon form (common): HHHHHHHH:LLLLLLLL
        format!("{high:08x}:{low:08x}"),
        format!("{low:08x}:{high:08x}"),
        // Underscore form without 0x
        format!("luid_{high:08x}_{low:08x}"),
        format!("luid_{low:08x}_{high:08x}"),
        format!("luid_0x{high:08x}_0x{low:08x}"),
        format!("luid_0x{low:08x}_0x{high:08x}"),
    ]
}

#[cfg(windows)]
#[allow(dead_code)]
fn query_gpu_shared_memory_pdh_aggregate() -> (Option<u64>, Option<u64>) {
    unsafe {
        let mut query: isize = 0;
        if PdhOpenQueryW(None, 0, &mut query) != ERROR_SUCCESS.0 {
            return (None, None);
        }

        // Query shared GPU memory by aggregating across all processes
        // Counter: \GPU Process Memory(*)\Shared Usage
        // We need to enumerate instances and sum them up
        let mut counter: isize = 0;
        let counter_path = w!("\\GPU Process Memory(*)\\Shared Usage");
        if PdhAddCounterW(query, counter_path, 0, &mut counter) != ERROR_SUCCESS.0 {
            let _ = PdhCloseQuery(query);
            return (None, None);
        }

        // Collect data
        let _ = PdhCollectQueryData(query);
        std::thread::sleep(std::time::Duration::from_millis(100));
        let _ = PdhCollectQueryData(query);

        // Get the value - PDH with wildcard returns aggregated value
        let mut value = PDH_FMT_COUNTERVALUE::default();
        if PdhGetFormattedCounterValue(counter, PDH_FMT_LARGE, None, &mut value) == ERROR_SUCCESS.0
        {
            let shared_bytes = value.Anonymous.largeValue as u64;
            let shared_mb = shared_bytes / 1_000_000;

            // Try to get total shared memory limit
            let mut total_counter: isize = 0;
            let total_path = w!("\\GPU Process Memory(_Total)\\Shared Limit");
            let mut total_value = PDH_FMT_COUNTERVALUE::default();
            let total_mb = if PdhAddCounterW(query, total_path, 0, &mut total_counter)
                == ERROR_SUCCESS.0
            {
                let _ = PdhCollectQueryData(query);
                if PdhGetFormattedCounterValue(total_counter, PDH_FMT_LARGE, None, &mut total_value)
                    == ERROR_SUCCESS.0
                {
                    (total_value.Anonymous.largeValue as u64) / 1_000_000
                } else {
                    // Fallback: try to get from system memory info
                    // Shared GPU memory is typically a portion of system RAM
                    // For AMD iGPU, it's often around 50% of available RAM
                    let mut sys_info = sysinfo::System::new();
                    sys_info.refresh_memory();
                    let total_ram_mb = sys_info.total_memory() / 1_000_000;
                    // AMD iGPU typically reserves ~50% of RAM for shared GPU memory
                    if total_ram_mb > 0 {
                        total_ram_mb / 2
                    } else {
                        0
                    }
                }
            } else {
                0
            };

            let _ = PdhCloseQuery(query);
            if shared_mb > 0 || total_mb > 0 {
                return (Some(shared_mb), Some(total_mb));
            }
        }

        let _ = PdhCloseQuery(query);
        (None, None)
    }
}

#[cfg(windows)]
#[allow(dead_code)]
fn query_gpu_adapter_memory_wmi(
    luid_patterns: &[String],
) -> (Option<u64>, Option<u64>, Option<u64>, Option<u64>) {
    use std::collections::HashMap;
    use std::convert::TryFrom;
    use wmi::{COMLibrary, Variant, WMIConnection};

    if luid_patterns.is_empty() {
        return (None, None, None, None);
    }

    let patterns: Vec<String> = luid_patterns.iter().map(|s| s.to_lowercase()).collect();

    let com_lib = match COMLibrary::new() {
        Ok(lib) => lib,
        Err(_) => return (None, None, None, None),
    };

    let wmi_con = match WMIConnection::new(com_lib) {
        Ok(con) => con,
        Err(_) => return (None, None, None, None),
    };

    // This is the closest to what Task Manager uses for "Dedicated/Shared GPU memory".
    // Values are in bytes.
    let query = "SELECT Name, DedicatedUsage, DedicatedLimit, SharedUsage, SharedLimit FROM Win32_PerfFormattedData_GPUPerformanceCounters_GPUAdapterMemory";
    let results: Vec<HashMap<String, Variant>> =
        match wmi_con.raw_query::<HashMap<String, Variant>>(query) {
            Ok(r) => r,
            Err(e) => {
                if std::env::var_os("WOLE_GPU_DEBUG").is_some() {
                    eprintln!("[gpu-debug] GPUAdapterMemory WMI query failed: {e:?}");
                }
                return (None, None, None, None);
            }
        };

    if std::env::var_os("WOLE_GPU_DEBUG").is_some() {
        eprintln!("[gpu-debug] GPUAdapterMemory rows: {}", results.len());
        eprintln!("[gpu-debug] LUID patterns: {:?}", patterns);
        for (i, row) in results.iter().take(8).enumerate() {
            let name = row
                .get("Name")
                .and_then(|v| String::try_from(v.clone()).ok())
                .unwrap_or_default();
            eprintln!("[gpu-debug] GPUAdapterMemory[{i}] Name={name}");
        }
    }

    let mut dedicated_used_mb: Option<u64> = None;
    let mut dedicated_total_mb: Option<u64> = None;
    let mut shared_used_mb: Option<u64> = None;
    let mut shared_total_mb: Option<u64> = None;

    for row in results {
        let name = row
            .get("Name")
            .and_then(|v| String::try_from(v.clone()).ok())
            .unwrap_or_default()
            .to_lowercase();

        if !patterns.iter().any(|p| name.contains(p)) {
            continue;
        }

        if std::env::var_os("WOLE_GPU_DEBUG").is_some() {
            eprintln!("[gpu-debug] GPUAdapterMemory match: {name}");
        }

        let du = row
            .get("DedicatedUsage")
            .and_then(|v| u64::try_from(v.clone()).ok())
            .map(|bytes| bytes / 1_000_000);
        let dl = row
            .get("DedicatedLimit")
            .and_then(|v| u64::try_from(v.clone()).ok())
            .map(|bytes| bytes / 1_000_000);
        let su = row
            .get("SharedUsage")
            .and_then(|v| u64::try_from(v.clone()).ok())
            .map(|bytes| bytes / 1_000_000);
        let sl = row
            .get("SharedLimit")
            .and_then(|v| u64::try_from(v.clone()).ok())
            .map(|bytes| bytes / 1_000_000);

        // Multiple instances can exist per adapter (e.g. phys_0/phys_1).
        // For Task Manager-like "total", taking max tends to align better than summing.
        if let Some(v) = du {
            dedicated_used_mb = Some(dedicated_used_mb.unwrap_or(0).max(v));
        }
        if let Some(v) = dl {
            dedicated_total_mb = Some(dedicated_total_mb.unwrap_or(0).max(v));
        }
        if let Some(v) = su {
            shared_used_mb = Some(shared_used_mb.unwrap_or(0).max(v));
        }
        if let Some(v) = sl {
            shared_total_mb = Some(shared_total_mb.unwrap_or(0).max(v));
        }
    }

    (
        dedicated_used_mb,
        dedicated_total_mb,
        shared_used_mb,
        shared_total_mb,
    )
}

#[cfg(windows)]
fn gather_dxgi_gpu_metrics() -> Option<GpuMetrics> {
    use windows::{core::*, Win32::Graphics::Dxgi::*};

    unsafe {
        // Create DXGI factory - CreateDXGIFactory1 returns IDXGIFactory1, we need to cast it
        let factory1: IDXGIFactory1 = match CreateDXGIFactory1() {
            Ok(f) => f,
            Err(_) => return None,
        };

        // Cast to IDXGIFactory4 for newer features
        let factory: IDXGIFactory4 = match factory1.cast() {
            Ok(f) => f,
            Err(_) => return None,
        };

        // Enumerate adapters
        let mut adapter_index = 0u32;
        loop {
            let adapter: IDXGIAdapter = match factory.EnumAdapters(adapter_index) {
                Ok(a) => a,
                Err(_) => break None,
            };

            // Cast to IDXGIAdapter1 to use GetDesc1()
            let adapter1: IDXGIAdapter1 = match adapter.cast() {
                Ok(a) => a,
                Err(_) => {
                    adapter_index += 1;
                    continue;
                }
            };

            let desc = match adapter1.GetDesc1() {
                Ok(d) => d,
                Err(_) => {
                    adapter_index += 1;
                    continue;
                }
            };

            // Skip software adapters (Microsoft Basic Render Driver)
            let description = String::from_utf16_lossy(&desc.Description);
            if description.contains("Microsoft Basic Render Driver")
                || description.contains("Software Adapter")
            {
                adapter_index += 1;
                continue;
            }

            // Adapter LUID (for WMI perf counter instance matching)
            let luid_patterns = dxgi_luid_patterns(&desc);

            // Try to get memory info from IDXGIAdapter3
            let adapter3_result: Result<IDXGIAdapter3> = adapter1.cast();
            let (dedicated_used_mb, dedicated_total_mb, shared_used_mb, shared_total_mb) =
                if let Ok(adapter3) = adapter3_result {
                    // Query dedicated (local) memory
                    // DXGI is reliable - use it directly
                    let mut local_mem_info = DXGI_QUERY_VIDEO_MEMORY_INFO::default();
                    let dedicated = match adapter3.QueryVideoMemoryInfo(
                        0,
                        DXGI_MEMORY_SEGMENT_GROUP_LOCAL,
                        &mut local_mem_info,
                    ) {
                        Ok(_) => {
                            let budget_mb = local_mem_info.Budget / 1_000_000;
                            // CurrentUsage is per-process, but Budget - AvailableForReservation gives us actual used
                            // For aggregate, use CurrentUsage (it's what Task Manager shows)
                            let current_usage_mb = local_mem_info.CurrentUsage / 1_000_000;
                            (Some(current_usage_mb), Some(budget_mb))
                        }
                        Err(_) => (None, None),
                    };

                    // Query shared (non-local) memory from DXGI
                    // DXGI is reliable - use it directly (Task Manager uses this too)
                    let mut shared_mem_info = DXGI_QUERY_VIDEO_MEMORY_INFO::default();
                    let shared_dxgi = match adapter3.QueryVideoMemoryInfo(
                        0,
                        DXGI_MEMORY_SEGMENT_GROUP_NON_LOCAL,
                        &mut shared_mem_info,
                    ) {
                        Ok(_) => {
                            let budget_mb = shared_mem_info.Budget / 1_000_000;
                            // CurrentUsage shows actual shared memory in use
                            let current_usage_mb = shared_mem_info.CurrentUsage / 1_000_000;
                            (Some(current_usage_mb), Some(budget_mb))
                        }
                        Err(_) => (None, None),
                    };

                    // Fallback: If DXGI shared memory is 0/0, try Win32_VideoController
                    // This is what Task Manager uses as fallback for some GPUs
                    let (shared_used_fallback, shared_total_fallback) =
                        if shared_dxgi.1 == Some(0) || shared_dxgi.1.is_none() {
                            query_gpu_shared_memory_wmi_fallback(&description)
                        } else {
                            (None, None)
                        };

                    // Use DXGI values, fallback to WMI if DXGI returns 0
                    let final_ded_used = dedicated.0;
                    let final_ded_total = dedicated.1;
                    let final_shared_used = shared_dxgi.0.or(shared_used_fallback);
                    let final_shared_total = shared_dxgi.1.or(shared_total_fallback);

                    (
                        final_ded_used,
                        final_ded_total,
                        final_shared_used,
                        final_shared_total,
                    )
                } else {
                    (None, None, None, None)
                };

            // Determine vendor from description
            let vendor = if description.to_lowercase().contains("nvidia") {
                "NVIDIA"
            } else if description.to_lowercase().contains("amd")
                || description.to_lowercase().contains("radeon")
            {
                "AMD"
            } else if description.to_lowercase().contains("intel") {
                "Intel"
            } else {
                "Unknown"
            };

            // Query GPU utilization and engine metrics via WMI, filtered by adapter LUID
            let (utilization, render_engine, copy_engine, compute_engine, video_engine) =
                query_gpu_engine_utilization(&luid_patterns);

            // Get driver version and PCI bus info from WMI
            let (driver_version, pci_bus_wmi) = query_gpu_info_wmi(&description);

            // Try to get temperature and threshold from WMI
            let (temperature, threshold) = gather_gpu_temperature_wmi(&description);

            return Some(GpuMetrics {
                name: description,
                vendor: vendor.to_string(),
                utilization_percent: utilization,
                render_engine_percent: render_engine,
                copy_engine_percent: copy_engine,
                compute_engine_percent: compute_engine,
                video_engine_percent: video_engine,
                memory_dedicated_used_mb: dedicated_used_mb,
                memory_dedicated_total_mb: dedicated_total_mb,
                memory_shared_used_mb: shared_used_mb,
                memory_shared_total_mb: shared_total_mb,
                memory_utilization_percent: if let (Some(used), Some(total)) =
                    (dedicated_used_mb, dedicated_total_mb)
                {
                    if total > 0 {
                        Some((used as f32 / total as f32) * 100.0)
                    } else {
                        None
                    }
                } else {
                    None
                },
                temperature_celsius: temperature,
                temperature_threshold_celsius: threshold,
                clock_speed_mhz: None,   // DXGI doesn't provide clock speed
                power_usage_watts: None, // DXGI doesn't provide power usage
                driver_version,
                pci_bus: pci_bus_wmi,
            });
        }
    }
}

#[cfg(windows)]
type GpuUtilizationResult = (
    Option<f32>,
    Option<f32>,
    Option<f32>,
    Option<f32>,
    Option<f32>,
);

#[cfg(windows)]
fn query_gpu_utilization_powershell() -> Option<GpuUtilizationResult> {
    use std::process::Command;

    // Use PowerShell Get-Counter for reliable GPU utilization aggregation
    // Query each engine type separately to get breakdown
    let script = r#"
        try {
            # Get overall utilization (all engines)
            $allCounters = Get-Counter -Counter "\GPU Engine(*)\Utilization Percentage" -ErrorAction Stop
            $allSamples = $allCounters.CounterSamples | Where-Object { $_.CookedValue -gt 0 }
            $overall = if ($allSamples) {
                ($allSamples | Measure-Object -Property CookedValue -Average).Average
            } else { 0 }

            # Get 3D/Render engine utilization
            $renderCounters = Get-Counter -Counter "\GPU Engine(*engtype_3D*)\Utilization Percentage" -ErrorAction SilentlyContinue
            $renderSamples = $renderCounters.CounterSamples | Where-Object { $_.CookedValue -gt 0 }
            $render = if ($renderSamples) {
                ($renderSamples | Measure-Object -Property CookedValue -Maximum).Maximum
            } else { 0 }

            # Get Copy engine utilization
            $copyCounters = Get-Counter -Counter "\GPU Engine(*engtype_Copy*)\Utilization Percentage" -ErrorAction SilentlyContinue
            $copySamples = $copyCounters.CounterSamples | Where-Object { $_.CookedValue -gt 0 }
            $copy = if ($copySamples) {
                ($copySamples | Measure-Object -Property CookedValue -Maximum).Maximum
            } else { 0 }

            # Get Compute engine utilization
            $computeCounters = Get-Counter -Counter "\GPU Engine(*engtype_Compute*)\Utilization Percentage" -ErrorAction SilentlyContinue
            $computeSamples = $computeCounters.CounterSamples | Where-Object { $_.CookedValue -gt 0 }
            $compute = if ($computeSamples) {
                ($computeSamples | Measure-Object -Property CookedValue -Maximum).Maximum
            } else { 0 }

            # Get Video engine utilization
            $videoCounters = Get-Counter -Counter "\GPU Engine(*engtype_Video*)\Utilization Percentage" -ErrorAction SilentlyContinue
            $videoSamples = $videoCounters.CounterSamples | Where-Object { $_.CookedValue -gt 0 }
            $video = if ($videoSamples) {
                ($videoSamples | Measure-Object -Property CookedValue -Maximum).Maximum
            } else { 0 }

            "$overall,$render,$copy,$compute,$video"
        } catch {
            Write-Error $_
            ""
        }
    "#;

    let output = Command::new("powershell")
        .args(["-NoProfile", "-Command", script])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let result = String::from_utf8(output.stdout).ok()?.trim().to_string();
    if result.is_empty() {
        return None;
    }

    let parts: Vec<&str> = result.split(',').collect();

    if parts.len() == 5 {
        let overall = parts[0].parse::<f32>().ok().filter(|&v| v > 0.0);
        let render = parts[1].parse::<f32>().ok().filter(|&v| v > 0.0);
        let copy = parts[2].parse::<f32>().ok().filter(|&v| v > 0.0);
        let compute = parts[3].parse::<f32>().ok().filter(|&v| v > 0.0);
        let video = parts[4].parse::<f32>().ok().filter(|&v| v > 0.0);

        Some((overall, render, copy, compute, video))
    } else {
        None
    }
}

#[cfg(windows)]
fn query_gpu_engine_utilization(luid_patterns: &[String]) -> GpuUtilizationResult {
    // Try PowerShell first (most reliable for aggregate utilization)
    if let Some(result) = query_gpu_utilization_powershell() {
        return result;
    }

    // Fallback to WMI (per-process, less reliable)
    use std::collections::HashMap;
    use std::convert::TryFrom;
    use wmi::{COMLibrary, Variant, WMIConnection};

    if luid_patterns.is_empty() {
        return (None, None, None, None, None);
    }
    let patterns: Vec<String> = luid_patterns.iter().map(|s| s.to_lowercase()).collect();

    let com_lib = match COMLibrary::new() {
        Ok(lib) => lib,
        Err(_) => return (None, None, None, None, None),
    };

    let wmi_con = match WMIConnection::new(com_lib) {
        Ok(con) => con,
        Err(_) => return (None, None, None, None, None),
    };

    // Query GPU engine utilization from WMI Performance Counters
    // Filter by adapter LUID and aggregate across all processes
    let query = "SELECT Name, UtilizationPercentage FROM Win32_PerfFormattedData_GPUPerformanceCounters_GPUEngine";

    let results: Vec<HashMap<String, Variant>> =
        match wmi_con.raw_query::<HashMap<String, Variant>>(query) {
            Ok(r) => r,
            Err(e) => {
                if std::env::var_os("WOLE_GPU_DEBUG").is_some() {
                    eprintln!("[gpu-debug] GPUEngine WMI query failed: {e:?}");
                }
                return (None, None, None, None, None);
            }
        };

    if results.is_empty() {
        return (None, None, None, None, None);
    }

    if std::env::var_os("WOLE_GPU_DEBUG").is_some() {
        eprintln!("[gpu-debug] GPUEngine rows: {}", results.len());
        eprintln!("[gpu-debug] LUID patterns: {:?}", patterns);
    }

    // Aggregate utilization across all matching processes for this adapter
    let mut overall_sum = 0.0f32;
    let mut overall_count = 0u32;
    let mut render_max: Option<f32> = None;
    let mut copy_max: Option<f32> = None;
    let mut compute_max: Option<f32> = None;
    let mut video_max: Option<f32> = None;

    for row in results {
        let name = row
            .get("Name")
            .and_then(|v| String::try_from(v.clone()).ok())
            .unwrap_or_default();

        let name_lower = name.to_lowercase();
        if !patterns.iter().any(|p| name_lower.contains(p)) {
            continue;
        }

        let util = row.get("UtilizationPercentage").and_then(|v| {
            if let Ok(val) = u32::try_from(v.clone()) {
                Some(val as f32)
            } else if let Ok(val) = u64::try_from(v.clone()) {
                Some(val as f32)
            } else if let Ok(val) = f64::try_from(v.clone()) {
                Some(val as f32)
            } else {
                None
            }
        });

        if let Some(util_val) = util {
            if util_val > 0.0 {
                overall_sum += util_val;
                overall_count += 1;
            }

            // Track max per engine type (Task Manager shows max, not average)
            if name_lower.contains("engtype_3d")
                || name_lower.contains("engtype_render")
                || name_lower.contains("high priority 3d")
            {
                render_max = Some(render_max.unwrap_or(0.0).max(util_val));
            } else if name_lower.contains("engtype_copy") {
                copy_max = Some(copy_max.unwrap_or(0.0).max(util_val));
            } else if name_lower.contains("engtype_compute")
                || name_lower.contains("high priority compute")
            {
                compute_max = Some(compute_max.unwrap_or(0.0).max(util_val));
            } else if name_lower.contains("engtype_video")
                || name_lower.contains("engtype_decode")
                || name_lower.contains("engtype_encode")
            {
                video_max = Some(video_max.unwrap_or(0.0).max(util_val));
            }
        }
    }

    // Calculate average for overall (only count non-zero values)
    let overall = if overall_count > 0 {
        Some(overall_sum / overall_count as f32)
    } else {
        None
    };

    (overall, render_max, copy_max, compute_max, video_max)
}

#[cfg(windows)]
fn query_gpu_info_wmi(gpu_name: &str) -> (Option<String>, Option<u32>) {
    use std::collections::HashMap;
    use std::convert::TryFrom;
    use wmi::{COMLibrary, Variant, WMIConnection};

    let com_lib = match COMLibrary::new() {
        Ok(lib) => lib,
        Err(_) => return (None, None),
    };

    let wmi_con = match WMIConnection::new(com_lib) {
        Ok(con) => con,
        Err(_) => return (None, None),
    };

    // Query GPU driver version and PCI bus info
    let query = format!(
        "SELECT DriverVersion, PNPDeviceID FROM Win32_VideoController WHERE Name LIKE '%{}%'",
        gpu_name.chars().take(20).collect::<String>()
    );

    let results: Vec<HashMap<String, Variant>> =
        match wmi_con.raw_query::<HashMap<String, Variant>>(&query) {
            Ok(r) => r,
            Err(_) => return (None, None),
        };

    if let Some(row) = results.into_iter().next() {
        let driver_version = row
            .get("DriverVersion")
            .and_then(|v| String::try_from(v.clone()).ok());

        // Extract PCI bus from PNPDeviceID (format: PCI\\VEN_XXXX&DEV_XXXX&SUBSYS_XXXX&REV_XX\\X&XXXXXXXX&0&XXXX)
        // For now, return None as PCI bus extraction requires parsing the device instance path
        // which is complex and varies by system
        return (driver_version, None);
    }

    (None, None)
}

#[cfg(windows)]
fn query_gpu_shared_memory_wmi_fallback(gpu_name: &str) -> (Option<u64>, Option<u64>) {
    use std::collections::HashMap;
    use std::convert::TryFrom;
    use wmi::{COMLibrary, Variant, WMIConnection};

    let com_lib = match COMLibrary::new() {
        Ok(lib) => lib,
        Err(_) => return (None, None),
    };

    let wmi_con = match WMIConnection::new(com_lib) {
        Ok(con) => con,
        Err(_) => return (None, None),
    };

    // Try Win32_VideoController for shared memory (fallback when DXGI returns 0)
    // Note: AdapterRAM is total VRAM, not shared memory, but some drivers report it differently
    let query = format!(
        "SELECT AdapterRAM FROM Win32_VideoController WHERE Name LIKE '%{}%'",
        gpu_name.chars().take(30).collect::<String>()
    );

    let results: Vec<HashMap<String, Variant>> =
        match wmi_con.raw_query::<HashMap<String, Variant>>(&query) {
            Ok(r) => r,
            Err(_) => return (None, None),
        };

    for row in results {
        if let Some(adapter_ram) = row.get("AdapterRAM") {
            if let Ok(ram_bytes) = u64::try_from(adapter_ram.clone()) {
                // AdapterRAM is in bytes, convert to MB
                let total_mb = ram_bytes / 1_000_000;
                // For shared memory, this might be the total GPU-accessible memory
                // Return as total, usage is unknown from this source
                return (None, Some(total_mb));
            }
        }
    }

    (None, None)
}

#[cfg(windows)]
fn gather_gpu_temperature_wmi(gpu_name: &str) -> (Option<f32>, Option<f32>) {
    use std::collections::HashMap;
    use std::convert::TryFrom;
    use wmi::{COMLibrary, Variant, WMIConnection};

    let com_lib = match COMLibrary::new() {
        Ok(lib) => lib,
        Err(_) => return (None, None),
    };

    let wmi_con = match WMIConnection::new(com_lib) {
        Ok(con) => con,
        Err(_) => return (None, None),
    };

    // Try Win32_VideoController first (where Task Manager gets temperature)
    // Some GPUs expose CurrentTemperature here
    let query_vc = format!(
        "SELECT CurrentTemperature FROM Win32_VideoController WHERE Name LIKE '%{}%'",
        gpu_name.chars().take(30).collect::<String>()
    );

    if let Ok(results_vc) = wmi_con.raw_query::<HashMap<String, Variant>>(&query_vc) {
        for row in results_vc {
            if let Some(temp_var) = row.get("CurrentTemperature") {
                // Win32_VideoController.CurrentTemperature is in Kelvin (not tenths)
                if let Ok(temp_kelvin) = u32::try_from(temp_var.clone()) {
                    let temp = temp_kelvin as f32 - 273.15;
                    if (0.0..=120.0).contains(&temp) {
                        // Try to get threshold from thermal zones
                        let threshold = query_gpu_temperature_threshold_wmi(&wmi_con);
                        return (Some(temp), threshold);
                    }
                } else if let Ok(temp_kelvin) = i32::try_from(temp_var.clone()) {
                    let temp = temp_kelvin as f32 - 273.15;
                    if (0.0..=120.0).contains(&temp) {
                        let threshold = query_gpu_temperature_threshold_wmi(&wmi_con);
                        return (Some(temp), threshold);
                    }
                }
            }
        }
    }

    // Fallback: Try MSAcpi_ThermalZoneTemperature (ACPI thermal zones)
    let query = "SELECT CurrentTemperature, CriticalTripPoint FROM MSAcpi_ThermalZoneTemperature WHERE InstanceName LIKE '%GPU%' OR InstanceName LIKE '%Graphics%'";

    let results: Vec<HashMap<String, Variant>> =
        match wmi_con.raw_query::<HashMap<String, Variant>>(query) {
            Ok(r) => r,
            Err(_) => return (None, None),
        };

    for row in results {
        let mut temp_celsius = None;
        let mut threshold_celsius = None;

        if let Some(temp_var) = row.get("CurrentTemperature") {
            if let Ok(temp_value) = u32::try_from(temp_var.clone()) {
                // WMI returns temperature in tenths of Kelvin, convert to Celsius
                let temp_kelvin = temp_value as f32 / 10.0;
                let temp = temp_kelvin - 273.15;
                // Sanity check: GPU temps should be between 0-120°C
                if (0.0..=120.0).contains(&temp) {
                    temp_celsius = Some(temp);
                }
            }
        }

        if let Some(threshold_var) = row.get("CriticalTripPoint") {
            if let Ok(threshold_value) = u32::try_from(threshold_var.clone()) {
                // WMI returns threshold in tenths of Kelvin, convert to Celsius
                let threshold_kelvin = threshold_value as f32 / 10.0;
                let threshold = threshold_kelvin - 273.15;
                if (0.0..=120.0).contains(&threshold) {
                    threshold_celsius = Some(threshold);
                }
            }
        }

        if temp_celsius.is_some() || threshold_celsius.is_some() {
            return (temp_celsius, threshold_celsius);
        }
    }

    // Fallback: Try Win32_TemperatureProbe (less common)
    let query2 = "SELECT CurrentReading, UpperThresholdCritical FROM Win32_TemperatureProbe WHERE Description LIKE '%GPU%' OR Description LIKE '%Graphics%'";
    if let Ok(results2) = wmi_con.raw_query::<HashMap<String, Variant>>(query2) {
        for row in results2 {
            let mut temp_celsius = None;
            let mut threshold_celsius = None;

            if let Some(temp_var) = row.get("CurrentReading") {
                if let Ok(temp_value) = i32::try_from(temp_var.clone()) {
                    let temp = temp_value as f32 / 10.0;
                    if (0.0..=120.0).contains(&temp) {
                        temp_celsius = Some(temp);
                    }
                }
            }

            if let Some(threshold_var) = row.get("UpperThresholdCritical") {
                if let Ok(threshold_value) = i32::try_from(threshold_var.clone()) {
                    let threshold = threshold_value as f32 / 10.0;
                    if (0.0..=120.0).contains(&threshold) {
                        threshold_celsius = Some(threshold);
                    }
                }
            }

            if temp_celsius.is_some() || threshold_celsius.is_some() {
                return (temp_celsius, threshold_celsius);
            }
        }
    }

    // Final fallback: Try sysinfo Components for temperature (no threshold)
    use sysinfo::Components;
    let components = Components::new_with_refreshed_list();
    for component in components.list() {
        let label_lower = component.label().to_lowercase();
        if label_lower.contains("gpu")
            || label_lower.contains("graphics")
            || label_lower.contains(&gpu_name.to_lowercase())
        {
            let temp = component.temperature();
            if temp > 0.0 && temp <= 120.0 {
                let threshold = component.critical().or_else(|| Some(component.max()));
                return (Some(temp), threshold);
            }
        }
    }

    (None, None)
}

#[cfg(windows)]
fn query_gpu_temperature_threshold_wmi(wmi_con: &wmi::WMIConnection) -> Option<f32> {
    use std::collections::HashMap;
    use std::convert::TryFrom;
    use wmi::Variant;

    // Try to get threshold from thermal zones
    let query = "SELECT CriticalTripPoint FROM MSAcpi_ThermalZoneTemperature WHERE InstanceName LIKE '%GPU%' OR InstanceName LIKE '%Graphics%'";

    if let Ok(results) = wmi_con.raw_query::<HashMap<String, Variant>>(query) {
        for row in results {
            if let Some(threshold_var) = row.get("CriticalTripPoint") {
                if let Ok(threshold_value) = u32::try_from(threshold_var.clone()) {
                    let threshold_kelvin = threshold_value as f32 / 10.0;
                    let threshold = threshold_kelvin - 273.15;
                    if (0.0..=120.0).contains(&threshold) {
                        return Some(threshold);
                    }
                }
            }
        }
    }

    None
}*/
// End of GPU implementation comment block

#[cfg(windows)]
fn gather_top_processes_with_wmi(
    system: &System,
    limit: usize,
    handle_counts: &std::collections::HashMap<u32, u32>,
    page_faults: &std::collections::HashMap<u32, u32>,
) -> Vec<ProcessInfo> {
    let mut processes: Vec<ProcessInfo> = system
        .processes()
        .iter()
        .map(|(pid, proc)| {
            let name = proc.name().to_string_lossy().to_string();
            let pid_u32 = pid.as_u32();
            let cpu_usage = proc.cpu_usage();
            let memory_bytes = proc.memory();
            let memory_usage = (memory_bytes as f32) / (system.total_memory() as f32) * 100.0;
            let memory_mb = (memory_bytes as f64) / (1024.0 * 1024.0);

            // Disk I/O
            let disk_usage = proc.disk_usage();
            let disk_read_mb = (disk_usage.read_bytes as f64) / (1024.0 * 1024.0);
            let disk_write_mb = (disk_usage.written_bytes as f64) / (1024.0 * 1024.0);

            ProcessInfo {
                name,
                pid: pid_u32,
                cpu_usage,
                memory_usage,
                memory_mb,
                disk_read_mb,
                disk_write_mb,
                handle_count: handle_counts.get(&pid_u32).copied(),
                page_faults_per_sec: page_faults.get(&pid_u32).copied(),
            }
        })
        .collect();

    // Sort by CPU usage descending
    processes.sort_by(|a, b| {
        b.cpu_usage
            .partial_cmp(&a.cpu_usage)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    processes.into_iter().take(limit).collect()
}

#[cfg(not(windows))]
#[allow(dead_code)] // Used on non-Windows platforms via conditional compilation
fn gather_top_processes(system: &System, limit: usize) -> Vec<ProcessInfo> {
    let mut processes: Vec<ProcessInfo> = system
        .processes()
        .iter()
        .map(|(pid, proc)| {
            let name = proc.name().to_string_lossy().to_string();
            let cpu_usage = proc.cpu_usage();
            let memory_bytes = proc.memory();
            let memory_usage = (memory_bytes as f32) / (system.total_memory() as f32) * 100.0;
            let memory_mb = (memory_bytes as f64) / (1024.0 * 1024.0);

            // Disk I/O
            let disk_usage = proc.disk_usage();
            let disk_read_mb = (disk_usage.read_bytes as f64) / (1024.0 * 1024.0);
            let disk_write_mb = (disk_usage.written_bytes as f64) / (1024.0 * 1024.0);

            ProcessInfo {
                name,
                pid: pid.as_u32(),
                cpu_usage,
                memory_usage,
                memory_mb,
                disk_read_mb,
                disk_write_mb,
                #[cfg(windows)]
                handle_count: None,
                #[cfg(windows)]
                page_faults_per_sec: None,
            }
        })
        .collect();

    // Sort by CPU usage descending
    processes.sort_by(|a, b| {
        b.cpu_usage
            .partial_cmp(&a.cpu_usage)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    processes.into_iter().take(limit).collect()
}

#[cfg(windows)]
fn gather_process_io_metrics() -> Vec<ProcessIOMetrics> {
    use std::collections::HashMap;
    use std::convert::TryFrom;
    use wmi::{COMLibrary, Variant, WMIConnection};

    let com_lib = match COMLibrary::new() {
        Ok(lib) => lib,
        Err(_) => return Vec::new(),
    };

    let wmi_con = match WMIConnection::new(com_lib) {
        Ok(con) => con,
        Err(_) => return Vec::new(),
    };

    // Query per-process I/O rates
    let query = "SELECT Name, IDProcess, IOReadBytesPerSec, IOWriteBytesPerSec FROM Win32_PerfFormattedData_PerfProc_Process WHERE Name != '_Total' AND Name != 'Idle'";

    let results: Vec<HashMap<String, Variant>> = match wmi_con.raw_query(query) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };

    let mut io_metrics: Vec<ProcessIOMetrics> = results
        .into_iter()
        .filter_map(|row| {
            let name_var = row.get("Name")?;
            let name = String::try_from(name_var.clone()).ok()?;

            let pid_var = row.get("IDProcess")?;
            let pid = u32::try_from(pid_var.clone()).ok()?;

            // WMI returns these as u64 values (bytes/sec)
            let read_var = row.get("IOReadBytesPerSec")?;
            let read_bytes_per_sec = u64::try_from(read_var.clone()).unwrap_or(0) as f64;

            let write_var = row.get("IOWriteBytesPerSec")?;
            let write_bytes_per_sec = u64::try_from(write_var.clone()).unwrap_or(0) as f64;

            // Only include processes with actual I/O activity
            if read_bytes_per_sec > 0.0 || write_bytes_per_sec > 0.0 {
                Some(ProcessIOMetrics {
                    name,
                    pid,
                    read_bytes_per_sec,
                    write_bytes_per_sec,
                })
            } else {
                None
            }
        })
        .collect();

    // Sort by total I/O (read + write) descending
    io_metrics.sort_by(|a, b| {
        let total_a = a.read_bytes_per_sec + a.write_bytes_per_sec;
        let total_b = b.read_bytes_per_sec + b.write_bytes_per_sec;
        total_b
            .partial_cmp(&total_a)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Return top 5 I/O processes
    io_metrics.into_iter().take(5).collect()
}

#[cfg(not(windows))]
fn gather_process_io_metrics() -> Vec<ProcessIOMetrics> {
    Vec::new()
}

#[cfg(windows)]
fn gather_process_handle_counts() -> HashMap<u32, u32> {
    use std::collections::HashMap;
    use std::convert::TryFrom;
    use wmi::{COMLibrary, Variant, WMIConnection};

    let mut handle_map = HashMap::new();

    let com_lib = match COMLibrary::new() {
        Ok(lib) => lib,
        Err(_) => return handle_map,
    };

    let wmi_con = match WMIConnection::new(com_lib) {
        Ok(con) => con,
        Err(_) => return handle_map,
    };

    let query = "SELECT ProcessId, HandleCount FROM Win32_Process";

    let results: Vec<HashMap<String, Variant>> = match wmi_con.raw_query(query) {
        Ok(r) => r,
        Err(_) => return handle_map,
    };

    for row in results {
        if let (Some(pid_var), Some(handle_var)) = (row.get("ProcessId"), row.get("HandleCount")) {
            if let (Ok(pid), Ok(handles)) = (
                u32::try_from(pid_var.clone()),
                u32::try_from(handle_var.clone()),
            ) {
                handle_map.insert(pid, handles);
            }
        }
    }

    handle_map
}

#[cfg(not(windows))]
fn gather_process_handle_counts() -> HashMap<u32, u32> {
    HashMap::new()
}

#[cfg(windows)]
fn gather_process_page_faults() -> HashMap<u32, u32> {
    use std::collections::HashMap;
    use std::convert::TryFrom;
    use wmi::{COMLibrary, Variant, WMIConnection};

    let mut fault_map = HashMap::new();

    let com_lib = match COMLibrary::new() {
        Ok(lib) => lib,
        Err(_) => return fault_map,
    };

    let wmi_con = match WMIConnection::new(com_lib) {
        Ok(con) => con,
        Err(_) => return fault_map,
    };

    let query = "SELECT IDProcess, PageFaultsPerSec FROM Win32_PerfFormattedData_PerfProc_Process WHERE Name != '_Total'";

    let results: Vec<HashMap<String, Variant>> = match wmi_con.raw_query(query) {
        Ok(r) => r,
        Err(_) => return fault_map,
    };

    for row in results {
        if let (Some(pid_var), Some(fault_var)) =
            (row.get("IDProcess"), row.get("PageFaultsPerSec"))
        {
            if let (Ok(pid), Ok(faults)) = (
                u32::try_from(pid_var.clone()),
                u32::try_from(fault_var.clone()),
            ) {
                fault_map.insert(pid, faults);
            }
        }
    }

    fault_map
}

#[cfg(not(windows))]
fn gather_process_page_faults() -> HashMap<u32, u32> {
    HashMap::new()
}

#[cfg(windows)]
pub fn gather_disk_breakdown_cached_only() -> Option<DiskBreakdown> {
    const CACHE_DURATION_SECS: u64 = 300; // 5 minutes

    // Only return cached data - don't block on scanning
    if let Ok(cache) = DISK_BREAKDOWN_CACHE.read() {
        if let Some((breakdown, timestamp)) = cache.as_ref() {
            if timestamp.elapsed().as_secs() < CACHE_DURATION_SECS {
                return Some(breakdown.clone());
            }
        }
    }
    None
}

/// Refresh disk breakdown asynchronously in a background thread
/// This allows the UI to remain responsive while scanning
#[cfg(windows)]
pub fn refresh_disk_breakdown_async() {
    std::thread::spawn(|| {
        let _ = gather_disk_breakdown();
    });
}

#[cfg(windows)]
fn gather_disk_breakdown() -> Option<DiskBreakdown> {
    const CACHE_DURATION_SECS: u64 = 300; // 5 minutes

    // Check cache first
    let cached = {
        if let Ok(cache) = DISK_BREAKDOWN_CACHE.read() {
            if let Some((breakdown, timestamp)) = cache.as_ref() {
                if timestamp.elapsed().as_secs() < CACHE_DURATION_SECS {
                    return Some(breakdown.clone());
                }
            }
        }
        None
    };

    if let Some(cached) = cached {
        return Some(cached);
    }

    // Get total disk space for C:\
    let total_disk_bytes = match get_disk_space_ex("C:\\") {
        Ok((total, _, _)) => total,
        Err(_) => return None,
    };
    let total_disk_gb = (total_disk_bytes as f64) / (1024.0 * 1024.0 * 1024.0);

    // Define categories and their paths
    let categories_config = vec![
        ("Users/Profiles", vec!["C:\\Users\\"]),
        (
            "Program Files",
            vec!["C:\\Program Files\\", "C:\\Program Files (x86)\\"],
        ),
        ("AppData", vec!["C:\\ProgramData\\"]),
        ("Windows", vec!["C:\\Windows\\"]),
    ];

    let mut categories = Vec::new();
    let mut total_calculated = 0u64;

    // Calculate size for each category
    for (name, paths) in categories_config {
        let mut category_size = 0u64;
        for path_str in paths {
            if let Ok(size) = get_directory_size_safe(Path::new(path_str)) {
                category_size += size;
            }
        }
        total_calculated += category_size;

        let size_gb = (category_size as f64) / (1024.0 * 1024.0 * 1024.0);
        let percent = if total_disk_bytes > 0 {
            (category_size as f32 / total_disk_bytes as f32) * 100.0
        } else {
            0.0
        };

        categories.push(DiskCategory {
            name: name.to_string(),
            size_gb,
            percent,
        });
    }

    // Calculate "Other" category (everything else on C:\)
    let other_size = total_disk_bytes.saturating_sub(total_calculated);
    let other_gb = (other_size as f64) / (1024.0 * 1024.0 * 1024.0);
    let other_percent = if total_disk_bytes > 0 {
        (other_size as f32 / total_disk_bytes as f32) * 100.0
    } else {
        0.0
    };

    categories.push(DiskCategory {
        name: "Other".to_string(),
        size_gb: other_gb,
        percent: other_percent,
    });

    // Sort by size descending
    categories.sort_by(|a, b| {
        b.size_gb
            .partial_cmp(&a.size_gb)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let breakdown = DiskBreakdown {
        volume: "C:\\".to_string(), // Breakdown is for C:\ drive
        categories,
        total_disk_gb,
        cached_at: Some(chrono::Local::now()),
    };

    // Update cache (shared across all threads)
    if let Ok(mut cache) = DISK_BREAKDOWN_CACHE.write() {
        *cache = Some((breakdown.clone(), Instant::now()));
    }

    Some(breakdown)
}

#[cfg(windows)]
fn get_disk_space_ex(drive: &str) -> Result<(u64, u64, u64), String> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    let path: Vec<u16> = OsStr::new(drive)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let mut free_available: u64 = 0;
    let mut total_bytes: u64 = 0;
    let mut free_bytes: u64 = 0;

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
            &mut free_available,
            &mut total_bytes,
            &mut free_bytes,
        );

        if result != 0 {
            Ok((total_bytes, free_bytes, free_available))
        } else {
            Err("GetDiskFreeSpaceEx failed".to_string())
        }
    }
}

#[cfg(windows)]
fn get_directory_size_safe(path: &Path) -> Result<u64, std::io::Error> {
    use std::fs;

    let mut total = 0u64;

    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Ok(metadata) = entry.metadata() {
                if metadata.is_dir() {
                    // Recursively calculate subdirectory size
                    if let Ok(subdir_size) = get_directory_size_safe(&entry.path()) {
                        total += subdir_size;
                    }
                    // Continue on error (permission denied, etc.)
                } else {
                    total += metadata.len();
                }
            }
            // Skip inaccessible files silently
        }
    }

    Ok(total)
}

#[cfg(windows)]
fn gather_boot_info() -> Option<BootInfo> {
    use std::collections::HashMap;
    use std::convert::TryFrom;
    use wmi::{COMLibrary, Variant, WMIConnection};

    let com_lib = COMLibrary::new().ok()?;
    let wmi_con = WMIConnection::new(com_lib).ok()?;

    // Query LastBootUpTime from Win32_OperatingSystem
    let query = "SELECT LastBootUpTime FROM Win32_OperatingSystem";
    let results: Vec<HashMap<String, Variant>> = wmi_con.raw_query(query).ok()?;

    let boot_time_str = results.first()?.get("LastBootUpTime")?;
    let boot_time_str = String::try_from(boot_time_str.clone()).ok()?;

    // Parse WMI datetime: "20240112153020.500000+300"
    // Format: YYYYMMDDHHMMSS.microseconds+timezone
    let last_boot_time = parse_wmi_datetime(&boot_time_str).ok()?;

    // Calculate uptime
    let now = chrono::Local::now();
    let uptime_duration = now.signed_duration_since(last_boot_time);
    let uptime_seconds = uptime_duration.num_seconds().max(0) as u64;

    // Estimate boot duration (default ~47 seconds, can be tuned)
    let boot_duration_seconds = 47u64;

    // Determine shutdown type (simplified - assume clean for now)
    // TODO: Could check Event Log for clean/dirty shutdown indicators
    let shutdown_type = "Clean".to_string();

    Some(BootInfo {
        uptime_seconds,
        last_boot_time,
        boot_duration_seconds,
        shutdown_type,
    })
}

#[cfg(windows)]
fn parse_wmi_datetime(
    s: &str,
) -> Result<chrono::DateTime<chrono::Local>, Box<dyn std::error::Error>> {
    use chrono::TimeZone;

    // Input: "20240112153020.500000+300"
    // Parse YYYYMMDDHHMMSS part (first 14 characters)

    if s.len() < 14 {
        return Err("Invalid WMI datetime format".into());
    }

    let year = s[0..4].parse::<i32>()?;
    let month = s[4..6].parse::<u32>()?;
    let day = s[6..8].parse::<u32>()?;
    let hour = s[8..10].parse::<u32>()?;
    let minute = s[10..12].parse::<u32>()?;
    let second = s[12..14].parse::<u32>()?;

    let naive_dt = chrono::NaiveDateTime::new(
        chrono::NaiveDate::from_ymd_opt(year, month, day).ok_or("Invalid date")?,
        chrono::NaiveTime::from_hms_opt(hour, minute, second).ok_or("Invalid time")?,
    );

    // Convert to local timezone
    let local_dt = chrono::Local
        .from_local_datetime(&naive_dt)
        .single()
        .ok_or("Ambiguous datetime")?;

    Ok(local_dt)
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
            (100.0 - temp).clamp(0.0, 100.0)
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

    // CPU Load (only on Unix systems)
    if !cfg!(windows) {
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
    } else {
        // On Windows, skip Load line and go straight to Memory Total
        output.push_str(&format!(
            "                                    Total   {:.1} / {:.1} GB\n",
            status.memory.used_gb, status.memory.total_gb
        ));
    }

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
    output.push_str(&format!("Free    {:.1} GB\n\n", status.memory.free_gb));

    // Disk and Power side by side
    output.push_str("Disk                                   Power\n");

    // Disk Used - just percentage
    output.push_str(&format!(
        "Used    {:.1}%                            ",
        status.disk.used_percent
    ));

    // Power Level
    if let Some(power) = &status.power {
        let power_bar = create_progress_bar(power.level_percent / 100.0, 20);
        output.push_str(&format!(
            "Level   {}  {:.0}%\n",
            power_bar, power.level_percent
        ));

        // Disk Free - show "X.X GB / Y.Y GB" format
        output.push_str(&format!(
            "Free    {:.1} GB / {:.1} GB                    ",
            status.disk.free_gb, status.disk.total_gb
        ));

        // Power Status
        output.push_str(&format!("Status  {}\n", power.status));

        // Disk Read (only if non-zero)
        if status.disk.read_speed_mb > 0.0 {
            output.push_str(&format!(
                "Read    {}  {:.1} MB/s                  ",
                create_speed_bar(status.disk.read_speed_mb / 100.0, 5),
                status.disk.read_speed_mb
            ));
        } else {
            output.push_str("                                    ");
        }

        // Power Health
        output.push_str(&format!("Health  {}", power.health));

        if let Some(temp) = power.temperature_celsius {
            output.push_str(&format!(" · {:.0}°C", temp));
        }

        output.push('\n');

        // Disk Write (only if non-zero)
        if status.disk.write_speed_mb > 0.0 {
            output.push_str(&format!(
                "Write   {}  {:.1} MB/s                  ",
                create_speed_bar(status.disk.write_speed_mb / 100.0, 5),
                status.disk.write_speed_mb
            ));
        } else {
            output.push_str("                                    ");
        }

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
            "Free    {:.1} GB / {:.1} GB                    ",
            status.disk.free_gb, status.disk.total_gb
        ));
        output.push_str("Status  Plugged In\n");
        if status.disk.read_speed_mb > 0.0 {
            output.push_str(&format!(
                "Read    {}  {:.1} MB/s\n",
                create_speed_bar(status.disk.read_speed_mb / 100.0, 5),
                status.disk.read_speed_mb
            ));
        }
    }

    // Disk Write (only if non-zero and power section doesn't exist, since write is shown inline with power)
    if status.power.is_none() && status.disk.write_speed_mb > 0.0 {
        output.push_str(&format!(
            "Write   {}  {:.1} MB/s\n\n",
            create_speed_bar(status.disk.write_speed_mb / 100.0, 5),
            status.disk.write_speed_mb
        ));
    } else {
        output.push('\n');
    }

    // Network - use Kbps if < 1 MB/s, otherwise MB/s
    output.push_str("Network\n");
    if status.network.download_mb < 1.0 {
        let kbps = status.network.download_mb * 1000.0;
        output.push_str(&format!(
            "Down    {}  {:.1} Kbps\n",
            create_speed_bar(status.network.download_mb / 100.0, 5),
            kbps
        ));
    } else {
        output.push_str(&format!(
            "Down    {}  {:.1} MB/s\n",
            create_speed_bar(status.network.download_mb / 10.0, 5),
            status.network.download_mb
        ));
    }
    if status.network.upload_mb < 1.0 {
        let kbps = status.network.upload_mb * 1000.0;
        output.push_str(&format!(
            "Up      {}  {:.1} Kbps\n",
            create_speed_bar(status.network.upload_mb / 100.0, 5),
            kbps
        ));
    } else {
        output.push_str(&format!(
            "Up      {}  {:.1} MB/s\n",
            create_speed_bar(status.network.upload_mb / 10.0, 5),
            status.network.upload_mb
        ));
    }

    if let Some(proxy) = &status.network.proxy {
        output.push_str(&format!("Proxy   {}\n", proxy));
    }

    output.push('\n');

    // Top Processes
    output.push_str("Top Processes\n");
    for proc in &status.processes {
        let proc_bar = create_progress_bar(proc.cpu_usage / 100.0, 5);
        output.push_str(&format!(
            "{:15}  {}  {:.1}%\n",
            proc.name, proc_bar, proc.cpu_usage
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
    let filled = (value.clamp(0.0, 1.0) * width as f64).round() as usize;
    let empty = width.saturating_sub(filled);
    format!("{}{}", "▮".repeat(filled), "▯".repeat(empty))
}
