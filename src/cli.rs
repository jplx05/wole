use clap::{Parser, Subcommand, ArgAction};
use std::path::PathBuf;
use colored::Colorize;

use crate::scanner;
use crate::output::{self, OutputMode};
use crate::cleaner;
use crate::size;
use crate::config::Config;

#[derive(Parser)]
#[command(name = "sweeper")]
#[command(version)]
#[command(about = "🧹 Clean your Windows machine without fear")]
#[command(long_about = "Sweeper is a developer-focused CLI tool that safely identifies and removes \
    unused files to free up disk space.\n\n\
    Examples:\n  \
    sweeper scan --all              # Scan all categories\n  \
    sweeper scan --cache --temp     # Scan specific categories\n  \
    sweeper clean --all -y          # Clean all categories without confirmation\n  \
    sweeper scan --large --min-size 500MB  # Find files over 500MB")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
    
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
        
        /// Scan cache directories (npm, pip, nuget, etc.)
        #[arg(long)]
        cache: bool,
        
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
        
        /// Clean cache directories (npm, pip, nuget, etc.)
        #[arg(long)]
        cache: bool,
        
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
        /// Enable all scan categories
        #[arg(short = 'a', long)]
        all: bool,
        
        /// Scan cache directories (npm, pip, nuget, etc.)
        #[arg(long)]
        cache: bool,
        
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
        
        /// Root path to scan (default: home directory)
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
            Commands::Scan { all, cache, temp, trash, build, downloads, large, old, path, json, project_age, min_age, min_size, exclude } => {
                // --all enables all categories
                let (cache, temp, trash, build, downloads, large, old, browser, system, empty, duplicates) = if all {
                    (true, true, true, true, true, true, true, true, true, true, true)
                } else if !cache && !temp && !trash && !build && !downloads && !large && !old {
                    // No categories specified - show help message
                    eprintln!("No categories specified. Use --all or specify categories like --cache, --temp, --build");
                    eprintln!("Run 'sweeper scan --help' for more information.");
                    return Ok(());
                } else {
                    // Scan command doesn't support browser, system, empty, duplicates
                    (cache, temp, trash, build, downloads, large, old, false, false, false, false)
                };
                
                // Default to Documents folder instead of entire home directory
                // Home directory (especially with OneDrive) can have extremely deep structures
                // that cause stack overflow even with depth limits
                let scan_path = path.unwrap_or_else(|| {
                    if let Some(user_dirs) = directories::UserDirs::new() {
                        if let Some(documents) = user_dirs.document_dir() {
                            documents.to_path_buf()
                        } else {
                            user_dirs.home_dir().to_path_buf()
                        }
                    } else {
                        std::env::var("USERPROFILE")
                            .map(|p| PathBuf::from(p).join("Documents"))
                            .unwrap_or_else(|_| PathBuf::from("."))
                    }
                });
                
                let min_size_bytes = size::parse_size(&min_size)
                    .map_err(|e| anyhow::anyhow!("Invalid size format '{}': {}", min_size, e))?;
                
                // Load config and merge CLI exclusions
                let mut config = Config::load();
                config.exclusions.patterns.extend(exclude.iter().cloned());
                
                let results = scanner::scan_all(
                    &scan_path,
                    ScanOptions {
                        cache,
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
                        project_age_days: project_age,
                        min_age_days: min_age,
                        min_size_bytes,
                    },
                    output_mode,
                    &config,
                )?;
                
                if json {
                    output::print_json(&results)?;
                } else {
                    output::print_human(&results, output_mode);
                }
                
                Ok(())
            }
            Commands::Clean { all, cache, temp, trash, build, downloads, large, old, browser, system, empty, duplicates, path, json, yes, project_age, min_age, min_size, exclude, permanent, dry_run } => {
                // --all enables all categories
                let (cache, temp, trash, build, downloads, large, old, browser, system, empty, duplicates) = if all {
                    (true, true, true, true, true, true, true, true, true, true, true)
                } else if !cache && !temp && !trash && !build && !downloads && !large && !old && !browser && !system && !empty && !duplicates {
                    // No categories specified - show help message
                    eprintln!("No categories specified. Use --all or specify categories like --cache, --temp, --build");
                    eprintln!("Run 'sweeper clean --help' for more information.");
                    return Ok(());
                } else {
                    (cache, temp, trash, build, downloads, large, old, browser, system, empty, duplicates)
                };
                
                let scan_path = path.unwrap_or_else(|| {
                    directories::UserDirs::new()
                        .expect("Failed to get user directory")
                        .home_dir()
                        .to_path_buf()
                });
                
                let min_size_bytes = size::parse_size(&min_size)
                    .map_err(|e| anyhow::anyhow!("Invalid size format '{}': {}", min_size, e))?;
                
                // Load config and merge CLI exclusions
                let mut config = Config::load();
                config.exclusions.patterns.extend(exclude.iter().cloned());
                
                let results = scanner::scan_all(
                    &scan_path,
                    ScanOptions {
                        cache,
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
                        project_age_days: project_age,
                        min_age_days: min_age,
                        min_size_bytes,
                    },
                    output_mode,
                    &config,
                )?;
                
                if json {
                    output::print_json(&results)?;
                } else {
                    output::print_human(&results, output_mode);
                }
                
                cleaner::clean_all(&results, yes, output_mode, permanent, dry_run)?;
                
                Ok(())
            }
            Commands::Analyze { all, cache, temp, trash, build, downloads, large, old, browser, system, empty, duplicates, path, project_age, min_age, min_size, exclude } => {
                // --all enables all categories
                let (cache, temp, trash, build, downloads, large, old, browser, system, empty, duplicates) = if all {
                    (true, true, true, true, true, true, true, true, true, true, true)
                } else if !cache && !temp && !trash && !build && !downloads && !large && !old && !browser && !system && !empty && !duplicates {
                    // No categories specified - show help message
                    eprintln!("No categories specified. Use --all or specify categories like --cache, --temp, --build");
                    eprintln!("Run 'sweeper analyze --help' for more information.");
                    return Ok(());
                } else {
                    (cache, temp, trash, build, downloads, large, old, browser, system, empty, duplicates)
                };
                
                let scan_path = path.unwrap_or_else(|| {
                    directories::UserDirs::new()
                        .expect("Failed to get user directory")
                        .home_dir()
                        .to_path_buf()
                });
                
                let min_size_bytes = size::parse_size(&min_size)
                    .map_err(|e| anyhow::anyhow!("Invalid size format '{}': {}", min_size, e))?;
                
                // Load config and merge CLI exclusions
                let mut config = Config::load();
                config.exclusions.patterns.extend(exclude.iter().cloned());
                
                let results = scanner::scan_all(
                    &scan_path,
                    ScanOptions {
                        cache,
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
                        project_age_days: project_age,
                        min_age_days: min_age,
                        min_size_bytes,
                    },
                    output_mode,
                    &config,
                )?;
                
                output::print_analyze(&results, output_mode);
                
                Ok(())
            }
            Commands::Config { show, reset, edit } => {
                if show {
                    let config = Config::load();
                    println!("{}", "Current Configuration".bold());
                    println!("{}", "━".repeat(60).dimmed());
                    println!();
                    println!("Thresholds:");
                    println!("  Project age: {} days", config.thresholds.project_age_days);
                    println!("  Min age: {} days", config.thresholds.min_age_days);
                    println!("  Min size: {} MB", config.thresholds.min_size_mb);
                    println!();
                    println!("Exclusions:");
                    if config.exclusions.patterns.is_empty() {
                        println!("  (none)");
                    } else {
                        for pattern in &config.exclusions.patterns {
                            println!("  {}", pattern);
                        }
                    }
                    println!();
                    if let Ok(path) = Config::config_path() {
                        println!("Config file: {}", path.display());
                    }
                } else if reset {
                    let default_config = Config::default();
                    default_config.save()?;
                    println!("{} Configuration reset to defaults.", "✓".green());
                } else if edit {
                    if let Ok(path) = Config::config_path() {
                        // Create default config if it doesn't exist
                        if !path.exists() {
                            Config::default().save()?;
                        }
                        // Try to open in default editor
                        let editor = std::env::var("EDITOR").unwrap_or_else(|_| "notepad".to_string());
                        std::process::Command::new(editor)
                            .arg(&path)
                            .status()
                            .map_err(|e| anyhow::anyhow!("Failed to open editor: {}", e))?;
                    } else {
                        return Err(anyhow::anyhow!("Failed to get config file path"));
                    }
                } else {
                    // Default: show config
                    let config = Config::load();
                    println!("{}", "Current Configuration".bold());
                    println!("{}", "━".repeat(60).dimmed());
                    println!();
                    println!("Thresholds:");
                    println!("  Project age: {} days", config.thresholds.project_age_days);
                    println!("  Min age: {} days", config.thresholds.min_age_days);
                    println!("  Min size: {} MB", config.thresholds.min_size_mb);
                    println!();
                    println!("Exclusions:");
                    if config.exclusions.patterns.is_empty() {
                        println!("  (none)");
                    } else {
                        for pattern in &config.exclusions.patterns {
                            println!("  {}", pattern);
                        }
                    }
                    println!();
                    if let Ok(path) = Config::config_path() {
                        println!("Config file: {}", path.display());
                    }
                }
                Ok(())
            }
        }
    }
}

pub struct ScanOptions {
    pub cache: bool,
    pub temp: bool,
    pub trash: bool,
    pub build: bool,
    pub downloads: bool,
    pub large: bool,
    pub old: bool,
    pub browser: bool,
    pub system: bool,
    pub empty: bool,
    pub duplicates: bool,
    pub project_age_days: u64,
    pub min_age_days: u64,
    pub min_size_bytes: u64,
}
