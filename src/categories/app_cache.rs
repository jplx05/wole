use crate::config::Config;
use crate::output::{CategoryResult, OutputMode};
use crate::scan_events::ScanProgressEvent;
use crate::theme::Theme;
use crate::utils;
use anyhow::{Context, Result};
use bytesize;
use std::collections::HashSet;
use std::env;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

/// Application cache locations to scan
/// Each tuple is (name, path_from_localappdata_or_appdata)
const APP_CACHE_LOCATIONS: &[(&str, AppCacheLocation)] = &[
    (
        "Discord",
        AppCacheLocation::LocalAppDataNested(&["discord", "Cache"]),
    ),
    (
        "VS Code",
        AppCacheLocation::LocalAppDataNested(&["Code", "Cache"]),
    ),
    (
        "VS Code (User)",
        AppCacheLocation::LocalAppDataNested(&["Code", "User", "CachedData"]),
    ),
    (
        "Slack",
        AppCacheLocation::LocalAppDataNested(&["slack", "Cache"]),
    ),
    (
        "Spotify",
        AppCacheLocation::LocalAppDataNested(&["Spotify", "Storage"]),
    ),
    (
        "Steam",
        AppCacheLocation::LocalAppDataNested(&["Steam", "htmlcache"]),
    ),
    (
        "Telegram",
        AppCacheLocation::LocalAppDataNested(&["Telegram Desktop", "tdata"]),
    ),
    (
        "Zoom",
        AppCacheLocation::LocalAppDataNested(&["Zoom", "Cache"]),
    ),
    (
        "Teams",
        AppCacheLocation::LocalAppDataNested(&["Microsoft", "Teams", "Cache"]),
    ),
    (
        "Notion",
        AppCacheLocation::LocalAppDataNested(&["Notion", "Cache"]),
    ),
    (
        "Figma",
        AppCacheLocation::LocalAppDataNested(&["Figma", "Cache"]),
    ),
    (
        "Adobe",
        AppCacheLocation::LocalAppDataNested(&["Adobe", "Common"]),
    ),
    (
        "Adobe Acrobat",
        AppCacheLocation::LocalAppDataNested(&["Adobe", "Acrobat", "Cache"]),
    ),
    (
        "Dropbox",
        AppCacheLocation::LocalAppDataNested(&["Dropbox", "Cache"]),
    ),
    (
        "OneDrive",
        AppCacheLocation::LocalAppDataNested(&["Microsoft", "OneDrive", "Cache"]),
    ),
    (
        "GitHub Desktop",
        AppCacheLocation::LocalAppDataNested(&["GitHub Desktop", "Cache"]),
    ),
    (
        "Postman",
        AppCacheLocation::LocalAppDataNested(&["Postman", "Cache"]),
    ),
    (
        "Docker",
        AppCacheLocation::LocalAppDataNested(&["Docker", "Cache"]),
    ),
    (
        "DBeaver",
        AppCacheLocation::LocalAppDataNested(&["DBeaver", "Cache"]),
    ),
    (
        "JetBrains",
        AppCacheLocation::LocalAppDataNested(&["JetBrains", "Cache"]),
    ),
    (
        "IntelliJ IDEA",
        AppCacheLocation::LocalAppDataNested(&["JetBrains", "IntelliJIdea", "cache"]),
    ),
    (
        "PyCharm",
        AppCacheLocation::LocalAppDataNested(&["JetBrains", "PyCharm", "cache"]),
    ),
    (
        "WebStorm",
        AppCacheLocation::LocalAppDataNested(&["JetBrains", "WebStorm", "cache"]),
    ),
    (
        "Android Studio",
        AppCacheLocation::LocalAppDataNested(&["Google", "AndroidStudio", "cache"]),
    ),
    (
        "Unity",
        AppCacheLocation::LocalAppDataNested(&["Unity", "cache"]),
    ),
    (
        "Blender",
        AppCacheLocation::LocalAppDataNested(&["Blender Foundation", "Blender", "cache"]),
    ),
    (
        "OBS Studio",
        AppCacheLocation::LocalAppDataNested(&["obs-studio", "Cache"]),
    ),
    (
        "VLC",
        AppCacheLocation::LocalAppDataNested(&["vlc", "cache"]),
    ),
    (
        "WinRAR",
        AppCacheLocation::LocalAppDataNested(&["WinRAR", "Cache"]),
    ),
    (
        "7-Zip",
        AppCacheLocation::LocalAppDataNested(&["7-Zip", "Cache"]),
    ),
];

enum AppCacheLocation {
    LocalAppDataNested(&'static [&'static str]),
}

/// Common cache directory names used by applications
const CACHE_DIR_NAMES: &[&str] = &["Cache", "cache", "Caches", ".cache", "Cache_Data"];

/// Scan for app-specific cache directories
///
/// Scans %LOCALAPPDATA% and %APPDATA% for app directories containing cache folders.
/// Looks for common cache directory names like "Cache", "cache", "Caches", etc.
fn scan_app_caches(base_path: &Path, known_paths: &mut HashSet<PathBuf>) -> Vec<PathBuf> {
    let mut app_cache_paths = Vec::new();

    if !base_path.exists() {
        return app_cache_paths;
    }

    // Read the base directory (e.g., LOCALAPPDATA or APPDATA)
    let entries = match utils::safe_read_dir(base_path) {
        Ok(entries) => entries,
        Err(_) => return app_cache_paths,
    };

    for entry in entries.filter_map(|e| e.ok()) {
        let app_dir = entry.path();

        // Skip if not a directory
        if !app_dir.is_dir() {
            continue;
        }

        // Check for cache directories directly in the app directory
        for cache_name in CACHE_DIR_NAMES {
            let cache_path = app_dir.join(cache_name);
            if cache_path.exists() && cache_path.is_dir() && !known_paths.contains(&cache_path) {
                // Defer size calculation to the caller for parallelism
                known_paths.insert(cache_path.clone());
                app_cache_paths.push(cache_path);
            }
        }

        // Also check nested app directories (e.g., CompanyName\AppName\Cache)
        if let Ok(nested_entries) = utils::safe_read_dir(&app_dir) {
            for nested_entry in nested_entries.filter_map(|e| e.ok()) {
                let nested_dir = nested_entry.path();
                if !nested_dir.is_dir() {
                    continue;
                }

                // Check for cache directories in nested app directories
                for cache_name in CACHE_DIR_NAMES {
                    let cache_path = nested_dir.join(cache_name);
                    if cache_path.exists()
                        && cache_path.is_dir()
                        && !known_paths.contains(&cache_path)
                    {
                        // Defer size calculation to the caller for parallelism
                        known_paths.insert(cache_path.clone());
                        app_cache_paths.push(cache_path);
                    }
                }
            }
        }
    }

    app_cache_paths
}

/// Scan for application cache directories
///
/// Scan for application cache directories
///
/// Checks well-known Windows cache locations for various applications.
/// Also scans generically for app cache directories.
///
/// Optimized to calculate directory sizes in parallel.
pub fn scan(_root: &Path, config: &Config, output_mode: OutputMode) -> Result<CategoryResult> {
    let mut result = CategoryResult::default();
    let mut known_paths = HashSet::new();
    let mut candidates = Vec::new();

    let local_appdata = env::var("LOCALAPPDATA").ok().map(PathBuf::from);
    let appdata = env::var("APPDATA").ok().map(PathBuf::from);

    if output_mode != OutputMode::Quiet {
        println!("  {} Scanning application cache directories...", Theme::muted("→"));
    }

    // 1. Collect all candidate paths first (fast IO check)

    // Scan known application caches
    for (name, location) in APP_CACHE_LOCATIONS {
        let cache_path = match location {
            AppCacheLocation::LocalAppDataNested(subpaths) => local_appdata.as_ref().map(|p| {
                let mut path = p.clone();
                for subpath in *subpaths {
                    path = path.join(subpath);
                }
                path
            }),
        };

        if let Some(cache_path) = cache_path {
            if cache_path.exists() && !config.is_excluded(&cache_path) {
                known_paths.insert(cache_path.clone());
                candidates.push(cache_path);
                if output_mode != OutputMode::Quiet {
                    println!("    {} Found {} cache", Theme::muted("•"), name);
                }
            }
        }
    }

    // Scan app-specific caches in LOCALAPPDATA
    if let Some(ref local_appdata_path) = local_appdata {
        let app_caches = scan_app_caches(local_appdata_path, &mut known_paths);
        candidates.extend(app_caches);
    }

    // Scan app-specific caches in APPDATA
    if let Some(ref appdata_path) = appdata {
        let app_caches = scan_app_caches(appdata_path, &mut known_paths);
        candidates.extend(app_caches);
    }

    // 2. Calculate sizes sequentially per folder, but folder size check is parallel
    // This is much Kinder to the disk than starting N parallel walks
    let mut paths_with_sizes: Vec<(PathBuf, u64)> = candidates
        .iter()
        .map(|path| {
            let size = utils::calculate_dir_size(path);
            (path.clone(), size)
        })
        .filter(|(_, size)| *size > 0)
        .collect();

    // Sort by size descending
    paths_with_sizes.sort_by(|a, b| b.1.cmp(&a.1));

    // Show found caches
    if output_mode != OutputMode::Quiet && !paths_with_sizes.is_empty() {
        println!("  {} Found {} application caches:", Theme::muted("→"), paths_with_sizes.len());
        let show_count = match output_mode {
            OutputMode::VeryVerbose => paths_with_sizes.len(),
            OutputMode::Verbose => paths_with_sizes.len(),
            OutputMode::Normal => paths_with_sizes.len().min(10),
            OutputMode::Quiet => 0,
        };
        
        for (i, (path, size)) in paths_with_sizes.iter().take(show_count).enumerate() {
            let size_str = bytesize::to_string(*size, true);
            println!("      {} {} ({})", Theme::muted("→"), path.display(), Theme::size(&size_str));
            
            if i == 9 && output_mode == OutputMode::Normal && paths_with_sizes.len() > 10 {
                println!("      {} ... and {} more (use -v to see all)", 
                    Theme::muted("→"), 
                    paths_with_sizes.len() - 10);
                break;
            }
        }
    }

    // Store paths
    result.paths = paths_with_sizes.iter().map(|(p, _)| p.clone()).collect();
    result.size_bytes = paths_with_sizes.iter().map(|(_, size)| *size).sum();
    result.items = paths_with_sizes.len();

    Ok(result)
}

/// Scan with real-time progress events (for TUI).
/// Scan with real-time progress events (for TUI).
pub fn scan_with_progress(_root: &Path, tx: &Sender<ScanProgressEvent>) -> Result<CategoryResult> {
    const CATEGORY: &str = "Application cache";
    let mut result = CategoryResult::default();
    let mut files_with_sizes: Vec<(PathBuf, u64)> = Vec::new();
    let mut known_paths = HashSet::new();

    let local_appdata = env::var("LOCALAPPDATA").ok().map(PathBuf::from);
    let appdata = env::var("APPDATA").ok().map(PathBuf::from);

    // Estimate total: known locations + app cache scanning (approximate)
    let total = APP_CACHE_LOCATIONS.len() as u64 + 2; // +2 for LOCALAPPDATA and APPDATA app cache scans
    let mut completed = 0u64;

    let _ = tx.send(ScanProgressEvent::CategoryStarted {
        category: CATEGORY.to_string(),
        total_units: Some(total),
        current_path: None,
    });

    // Scan known application caches
    for (idx, (_name, location)) in APP_CACHE_LOCATIONS.iter().enumerate() {
        let cache_path = match location {
            AppCacheLocation::LocalAppDataNested(subpaths) => local_appdata.as_ref().map(|p| {
                let mut path = p.clone();
                for subpath in *subpaths {
                    path = path.join(subpath);
                }
                path
            }),
        };

        if let Some(cache_path) = cache_path {
            if cache_path.exists() {
                let size = utils::calculate_dir_size(&cache_path);
                if size > 0 {
                    known_paths.insert(cache_path.clone());
                    files_with_sizes.push((cache_path.clone(), size));
                }
            }

            completed = (idx + 1) as u64;
            let _ = tx.send(ScanProgressEvent::CategoryProgress {
                category: CATEGORY.to_string(),
                completed_units: completed,
                total_units: Some(total),
                current_path: Some(cache_path),
            });
        } else {
            completed = (idx + 1) as u64;
            let _ = tx.send(ScanProgressEvent::CategoryProgress {
                category: CATEGORY.to_string(),
                completed_units: completed,
                total_units: Some(total),
                current_path: None,
            });
        }
    }

    // Scan app-specific caches in LOCALAPPDATA
    if let Some(ref local_appdata_path) = local_appdata {
        let _ = tx.send(ScanProgressEvent::CategoryProgress {
            category: CATEGORY.to_string(),
            completed_units: completed + 1,
            total_units: Some(total),
            current_path: Some(local_appdata_path.clone()),
        });

        let app_caches = scan_app_caches(local_appdata_path, &mut known_paths);
        for cache_path in app_caches {
            let size = utils::calculate_dir_size(&cache_path);
            if size > 0 {
                files_with_sizes.push((cache_path, size));
            }
        }
        completed += 1;
    }

    // Scan app-specific caches in APPDATA
    if let Some(ref appdata_path) = appdata {
        let _ = tx.send(ScanProgressEvent::CategoryProgress {
            category: CATEGORY.to_string(),
            completed_units: completed + 1,
            total_units: Some(total),
            current_path: Some(appdata_path.clone()),
        });

        let app_caches = scan_app_caches(appdata_path, &mut known_paths);
        for cache_path in app_caches {
            let size = utils::calculate_dir_size(&cache_path);
            if size > 0 {
                files_with_sizes.push((cache_path, size));
            }
        }
    }

    // Sort by size descending
    files_with_sizes.sort_by(|a, b| b.1.cmp(&a.1));

    // Build final result
    for (path, size) in files_with_sizes {
        result.items += 1;
        result.size_bytes += size;
        result.paths.push(path);
    }

    let _ = tx.send(ScanProgressEvent::CategoryFinished {
        category: CATEGORY.to_string(),
        items: result.items,
        size_bytes: result.size_bytes,
    });

    Ok(result)
}

/// Clean (delete) an application cache directory by moving it to the Recycle Bin
pub fn clean(path: &Path) -> Result<()> {
    trash::delete(path).with_context(|| {
        format!(
            "Failed to delete application cache directory: {}",
            path.display()
        )
    })?;
    Ok(())
}
