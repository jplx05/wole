use clap::{ArgAction, Parser, Subcommand};
use std::path::PathBuf;

use crate::output::OutputMode;

pub mod commands;
mod interactive_menu;

#[derive(Parser)]
#[command(name = "wole")]
#[command(version)]
#[command(about = "Reclaim disk space on Windows by cleaning unused files")]
#[command(
    long_about = "Wole is a developer-focused CLI tool that safely identifies and removes \
        unused files to free up disk space.\n\n\
        Interactive Mode:\n  \
        wole                          # Launch interactive TUI mode\n  \
        wole analyze --interactive    # Launch TUI for disk insights\n\n\
        Examples:\n  \
        wole scan --all              # Scan all categories\n  \
        wole scan --cache --temp     # Scan specific categories\n  \
        wole clean --all -y          # Clean all categories without confirmation\n  \
        wole scan --large --min-size 500MB  # Find files over 500MB\n  \
        wole remove                  # Uninstall wole from your system"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Increase output verbosity (-v, -vv for more)
    #[arg(short = 'v', long, action = ArgAction::Count, global = true)]
    pub verbose: u8,

    /// Suppress all output except errors
    #[arg(short = 'q', long, global = true, conflicts_with = "verbose")]
    pub quiet: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Find cleanable files (dry-run, safe to run anytime)
    #[command(visible_alias = "s")]
    Scan {
        /// Enable all scan categories
        #[arg(short = 'a', long)]
        all: bool,

        /// Scan package manager cache directories (npm, pip, nuget, etc.)
        #[arg(long)]
        cache: bool,

        /// Scan application cache directories (Discord, VS Code, Slack, etc.)
        #[arg(long)]
        app_cache: bool,

        /// Scan temporary files (system temp folders)
        #[arg(long)]
        temp: bool,

        /// Scan Recycle Bin contents
        #[arg(long)]
        trash: bool,

        /// Scan build artifacts from inactive projects (node_modules, target, etc.)
        #[arg(long)]
        build: bool,

        /// Scan Downloads folder for old files
        #[arg(long)]
        downloads: bool,

        /// Scan for files over size threshold
        #[arg(long)]
        large: bool,

        /// Scan for files not accessed in N days
        #[arg(long)]
        old: bool,

        /// Scan for installed applications (Windows only)
        #[arg(long)]
        applications: bool,

        /// Scan Windows Update files (download cache, logs) - requires admin
        #[arg(long)]
        windows_update: bool,

        /// Scan Windows Event Log files (old .evtx files) - requires admin
        #[arg(long)]
        event_logs: bool,

        /// Root path to scan (default: home directory)
        #[arg(long, value_name = "PATH")]
        path: Option<PathBuf>,

        /// Output results as JSON for scripting
        #[arg(long)]
        json: bool,

        /// Project inactivity threshold in days [default: 14]
        #[arg(long, default_value = "14", value_name = "DAYS")]
        project_age: u64,

        /// Minimum file age in days for --downloads and --old [default: 30]
        #[arg(long, default_value = "30", value_name = "DAYS")]
        min_age: u64,

        /// Minimum file size for --large (e.g., 100MB, 1GB) [default: 100MB]
        #[arg(long, default_value = "100MB", value_name = "SIZE")]
        min_size: String,

        /// Exclude paths matching pattern (repeatable)
        #[arg(long, value_name = "PATTERN")]
        exclude: Vec<String>,
    },

    /// Delete files found by scan (with confirmation)
    #[command(visible_alias = "c")]
    Clean {
        /// Enable all clean categories
        #[arg(short = 'a', long)]
        all: bool,

        /// Clean package manager cache directories (npm, pip, nuget, etc.)
        #[arg(long)]
        cache: bool,

        /// Clean application cache directories (Discord, VS Code, Slack, etc.)
        #[arg(long)]
        app_cache: bool,

        /// Clean temporary files (system temp folders)
        #[arg(long)]
        temp: bool,

        /// Empty Recycle Bin
        #[arg(long)]
        trash: bool,

        /// Clean build artifacts from inactive projects (node_modules, target, etc.)
        #[arg(long)]
        build: bool,

        /// Clean old files in Downloads folder
        #[arg(long)]
        downloads: bool,

        /// Clean files over size threshold
        #[arg(long)]
        large: bool,

        /// Clean files not accessed in N days
        #[arg(long)]
        old: bool,

        /// Clean browser caches (Chrome, Edge, Firefox)
        #[arg(long)]
        browser: bool,

        /// Clean Windows system caches (thumbnails, updates, icons)
        #[arg(long)]
        system: bool,

        /// Clean empty folders
        #[arg(long)]
        empty: bool,

        /// Clean duplicate files (keeps one copy)
        #[arg(long)]
        duplicates: bool,

        /// Clean installed applications (Windows only)
        #[arg(long)]
        applications: bool,

        /// Clean Windows Update files (download cache, logs) - requires admin
        #[arg(long)]
        windows_update: bool,

        /// Clean Windows Event Log files (old .evtx files) - requires admin
        #[arg(long)]
        event_logs: bool,

        /// Root path to scan (default: home directory)
        #[arg(long, value_name = "PATH")]
        path: Option<PathBuf>,

        /// Output results as JSON for scripting
        #[arg(long)]
        json: bool,

        /// Skip confirmation prompt (use with caution!)
        #[arg(short = 'y', long = "yes")]
        yes: bool,

        /// Project inactivity threshold in days [default: 14]
        #[arg(long, default_value = "14", value_name = "DAYS")]
        project_age: u64,

        /// Minimum file age in days for --downloads and --old [default: 30]
        #[arg(long, default_value = "30", value_name = "DAYS")]
        min_age: u64,

        /// Minimum file size for --large (e.g., 100MB, 1GB) [default: 100MB]
        #[arg(long, default_value = "100MB", value_name = "SIZE")]
        min_size: String,

        /// Exclude paths matching pattern (repeatable)
        #[arg(long, value_name = "PATTERN")]
        exclude: Vec<String>,

        /// Permanently delete (bypass Recycle Bin)
        #[arg(long)]
        permanent: bool,

        /// Preview only, don't delete
        #[arg(long)]
        dry_run: bool,
    },

    /// Show detailed analysis with file lists
    #[command(visible_alias = "a")]
    Analyze {
        /// Enable disk insights mode (analyze folder sizes, default when no category flags)
        #[arg(long)]
        disk: bool,

        /// Scan entire disk instead of user directory (only applies to disk insights mode)
        #[arg(long)]
        entire_disk: bool,

        /// Launch interactive TUI for disk insights
        #[arg(short = 'i', long)]
        interactive: bool,

        /// Maximum depth to scan [default: 3, recommended: 10+ for entire disk]
        #[arg(long, default_value = "3", value_name = "DEPTH")]
        depth: u8,

        /// Show top N folders [default: 10]
        #[arg(long, value_name = "N")]
        top: Option<usize>,

        /// Sort order: size, name, or files [default: size]
        #[arg(long, value_name = "SORT")]
        sort: Option<String>,

        /// Enable all scan categories (legacy cleanable file analysis)
        #[arg(short = 'a', long)]
        all: bool,

        /// Scan package manager cache directories (npm, pip, nuget, etc.)
        #[arg(long)]
        cache: bool,

        /// Scan application cache directories (Discord, VS Code, Slack, etc.)
        #[arg(long)]
        app_cache: bool,

        /// Scan temporary files (system temp folders)
        #[arg(long)]
        temp: bool,

        /// Scan Recycle Bin contents
        #[arg(long)]
        trash: bool,

        /// Scan build artifacts from inactive projects (node_modules, target, etc.)
        #[arg(long)]
        build: bool,

        /// Scan Downloads folder for old files
        #[arg(long)]
        downloads: bool,

        /// Scan for files over size threshold
        #[arg(long)]
        large: bool,

        /// Scan for files not accessed in N days
        #[arg(long)]
        old: bool,

        /// Scan browser caches (Chrome, Edge, Firefox)
        #[arg(long)]
        browser: bool,

        /// Scan Windows system caches (thumbnails, updates, icons)
        #[arg(long)]
        system: bool,

        /// Scan for empty folders
        #[arg(long)]
        empty: bool,

        /// Scan for duplicate files
        #[arg(long)]
        duplicates: bool,

        /// Scan for installed applications (Windows only)
        #[arg(long)]
        applications: bool,

        /// Root path to scan (default: user profile)
        #[arg(long, value_name = "PATH")]
        path: Option<PathBuf>,

        /// Project inactivity threshold in days [default: 14]
        #[arg(long, default_value = "14", value_name = "DAYS")]
        project_age: u64,

        /// Minimum file age in days for --downloads and --old [default: 30]
        #[arg(long, default_value = "30", value_name = "DAYS")]
        min_age: u64,

        /// Minimum file size for --large (e.g., 100MB, 1GB) [default: 100MB]
        #[arg(long, default_value = "100MB", value_name = "SIZE")]
        min_size: String,

        /// Exclude paths matching pattern (repeatable)
        #[arg(long, value_name = "PATTERN")]
        exclude: Vec<String>,
    },

    /// View or modify configuration
    Config {
        /// Show current configuration
        #[arg(long)]
        show: bool,

        /// Reset to defaults
        #[arg(long)]
        reset: bool,

        /// Open config file in editor
        #[arg(long)]
        edit: bool,
    },

    /// Restore files from the last deletion session
    #[command(visible_alias = "r")]
    Restore {
        /// Restore from the last deletion session
        #[arg(long)]
        last: bool,

        /// Restore a specific file path
        #[arg(long, value_name = "PATH")]
        path: Option<PathBuf>,

        /// Restore from a specific log file
        #[arg(long, value_name = "LOG_FILE")]
        from: Option<PathBuf>,

        /// Restore all contents of the Recycle Bin in bulk (faster on Windows)
        #[arg(long)]
        all: bool,
    },

    /// Uninstall wole from your system
    Remove {
        /// Also remove config directory (%APPDATA%\wole)
        #[arg(long)]
        config: bool,

        /// Also remove data directory (%LOCALAPPDATA%\wole, including history)
        #[arg(long)]
        data: bool,

        /// Skip confirmation prompt
        #[arg(short = 'y', long = "yes")]
        yes: bool,
    },

    /// Check for and install updates
    Update {
        /// Skip confirmation prompt
        #[arg(short = 'y', long = "yes")]
        yes: bool,

        /// Check for updates without installing
        #[arg(long)]
        check: bool,
    },

    /// Optimize Windows system performance
    #[command(visible_alias = "o")]
    Optimize {
        /// Run all optimizations
        #[arg(short = 'a', long)]
        all: bool,

        /// Flush DNS cache
        #[arg(long)]
        dns: bool,

        /// Clear thumbnail cache
        #[arg(long)]
        thumbnails: bool,

        /// Rebuild icon cache and restart Explorer
        #[arg(long)]
        icons: bool,

        /// Optimize browser databases (VACUUM)
        #[arg(long)]
        databases: bool,

        /// Restart font cache service (requires admin)
        #[arg(long)]
        fonts: bool,

        /// Clear standby memory (requires admin)
        #[arg(long)]
        memory: bool,

        /// Reset network stack - Winsock + IP (requires admin)
        #[arg(long)]
        network: bool,

        /// Restart Bluetooth service (requires admin)
        #[arg(long)]
        bluetooth: bool,

        /// Restart Windows Search service (requires admin)
        #[arg(long)]
        search: bool,

        /// Restart Windows Explorer
        #[arg(long)]
        explorer: bool,

        /// Preview only, don't execute
        #[arg(long)]
        dry_run: bool,

        /// Skip confirmation for admin operations
        #[arg(short = 'y', long = "yes")]
        yes: bool,
    },

    // GPU implementation commented out - not polished yet
    // /// Debug GPU metrics collection (for troubleshooting)
    // #[command(visible_alias = "gpu-debug")]
    // DebugGpu,
    /// Show real-time system status dashboard
    #[command(visible_alias = "st")]
    Status {
        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Continuous refresh mode (updates every second)
        #[arg(short = 'w', long)]
        watch: bool,
    },

    /// Manage Windows startup programs
    #[command(visible_alias = "su")]
    Startup {
        /// List all startup programs
        #[arg(short = 'l', long)]
        list: bool,

        /// Disable a startup program by name
        #[arg(short = 'd', long, value_name = "NAME")]
        disable: Option<String>,

        /// Enable a startup program by name
        #[arg(short = 'e', long, value_name = "NAME")]
        enable: Option<String>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

impl Cli {
    pub fn parse() -> Self {
        <Self as Parser>::parse()
    }

    pub fn run(self) -> anyhow::Result<()> {
        let output_mode = if self.quiet {
            OutputMode::Quiet
        } else if self.verbose >= 2 {
            OutputMode::VeryVerbose
        } else if self.verbose == 1 {
            OutputMode::Verbose
        } else {
            OutputMode::Normal
        };

        match self.command {
            None => {
                // No command provided - show interactive menu
                Self::show_interactive_menu();
                Ok(())
            }
            Some(command) => match command {
                Commands::Scan {
                    all,
                    cache,
                    app_cache,
                    temp,
                    trash,
                    build,
                    downloads,
                    large,
                    old,
                    applications,
                    windows_update,
                    event_logs,
                    path,
                    json,
                    project_age,
                    min_age,
                    min_size,
                    exclude,
                } => commands::scan_command::handle_scan(
                    all,
                    cache,
                    app_cache,
                    temp,
                    trash,
                    build,
                    downloads,
                    large,
                    old,
                    applications,
                    windows_update,
                    event_logs,
                    path,
                    json,
                    project_age,
                    min_age,
                    min_size,
                    exclude,
                    output_mode,
                ),
                Commands::Clean {
                    all,
                    cache,
                    app_cache,
                    temp,
                    trash,
                    build,
                    downloads,
                    large,
                    old,
                    browser,
                    system,
                    empty,
                    duplicates,
                    applications,
                    windows_update,
                    event_logs,
                    path,
                    json,
                    yes,
                    project_age,
                    min_age,
                    min_size,
                    exclude,
                    permanent,
                    dry_run,
                } => commands::clean_command::handle_clean(
                    all,
                    cache,
                    app_cache,
                    temp,
                    trash,
                    build,
                    downloads,
                    large,
                    old,
                    browser,
                    system,
                    empty,
                    duplicates,
                    applications,
                    windows_update,
                    event_logs,
                    path,
                    json,
                    yes,
                    project_age,
                    min_age,
                    min_size,
                    exclude,
                    permanent,
                    dry_run,
                    output_mode,
                ),
                Commands::Analyze {
                    disk,
                    entire_disk,
                    interactive,
                    depth,
                    top,
                    sort,
                    all,
                    cache,
                    app_cache,
                    temp,
                    trash,
                    build,
                    downloads,
                    large,
                    old,
                    browser,
                    system,
                    empty,
                    duplicates,
                    applications,
                    path,
                    project_age,
                    min_age,
                    min_size,
                    exclude,
                } => commands::analyze_command::handle_analyze(
                    disk,
                    entire_disk,
                    interactive,
                    depth,
                    top,
                    sort,
                    all,
                    cache,
                    app_cache,
                    temp,
                    trash,
                    build,
                    downloads,
                    large,
                    old,
                    browser,
                    system,
                    empty,
                    duplicates,
                    applications,
                    path,
                    project_age,
                    min_age,
                    min_size,
                    exclude,
                    output_mode,
                ),
                Commands::Config { show, reset, edit } => {
                    commands::config_command::handle_config(show, reset, edit)
                }
                Commands::Restore {
                    last,
                    path,
                    from,
                    all,
                } => commands::restore_command::handle_restore(
                    last,
                    path,
                    from,
                    all,
                    self.quiet,
                    self.verbose,
                ),
                Commands::Remove { config, data, yes } => {
                    commands::remove_command::handle_remove(
                        config,
                        data,
                        yes,
                        self.quiet,
                        self.verbose,
                    )
                }
                Commands::Update { yes, check } => {
                    commands::update_command::handle_update(yes, check, output_mode)
                }
                Commands::Optimize {
                    all,
                    dns,
                    thumbnails,
                    icons,
                    databases,
                    fonts,
                    memory,
                    network,
                    bluetooth,
                    search,
                    explorer,
                    dry_run,
                    yes,
                } => commands::optimize_command::handle_optimize(
                    all,
                    dns,
                    thumbnails,
                    icons,
                    databases,
                    fonts,
                    memory,
                    network,
                    bluetooth,
                    search,
                    explorer,
                    dry_run,
                    yes,
                    output_mode,
                ),
                // GPU implementation commented out - not polished yet
                // Commands::DebugGpu => {
                //     // Force debug mode
                //     std::env::set_var("WOLE_GPU_DEBUG", "1");
                //
                //     println!("=== GPU Debug Information ===\n");
                //     println!("Collecting GPU metrics with debug output enabled...\n");
                //
                //     // This will trigger the debug prints in status.rs
                //     if let Some(gpu) = status::gather_gpu_metrics() {
                //         println!("\n=== Collected GPU Metrics ===");
                //         println!("Name: {}", gpu.name);
                //         println!("Vendor: {}", gpu.vendor);
                //         println!("Utilization: {:?}%", gpu.utilization_percent);
                //         println!("3D Engine: {:?}%", gpu.render_engine_percent);
                //         println!("Copy Engine: {:?}%", gpu.copy_engine_percent);
                //         println!("Compute Engine: {:?}%", gpu.compute_engine_percent);
                //         println!("Video Engine: {:?}%", gpu.video_engine_percent);
                //         println!(
                //             "Dedicated Memory: {:?} / {:?} MB",
                //             gpu.memory_dedicated_used_mb, gpu.memory_dedicated_total_mb
                //         );
                //         println!(
                //             "Shared Memory: {:?} / {:?} MB",
                //             gpu.memory_shared_used_mb, gpu.memory_shared_total_mb
                //         );
                //         println!("Temperature: {:?}°C", gpu.temperature_celsius);
                //         println!(
                //             "Temperature Threshold: {:?}°C",
                //             gpu.temperature_threshold_celsius
                //         );
                //         println!("Driver Version: {:?}", gpu.driver_version);
                //     } else {
                //         println!("\nNo GPU metrics collected.");
                //     }
                //     Ok(())
                // }
                Commands::Status { json, watch } => {
                    commands::status_command::handle_status(json, watch)
                }
                Commands::Startup {
                    list,
                    disable,
                    enable,
                    json,
                } => commands::startup_command::handle_startup(list, disable, enable, json),
            },
        }
    }
}

#[derive(Clone)]
pub struct ScanOptions {
    pub cache: bool,
    pub app_cache: bool,
    pub temp: bool,
    pub trash: bool,
    pub build: bool,
    pub downloads: bool,
    pub large: bool,
    pub old: bool,
    pub applications: bool,
    pub browser: bool,
    pub system: bool,
    pub empty: bool,
    pub duplicates: bool,
    pub windows_update: bool,
    pub event_logs: bool,
    pub project_age_days: u64,
    pub min_age_days: u64,
    pub min_size_bytes: u64,
}
