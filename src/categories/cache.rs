use crate::config::Config;
use crate::output::{CategoryResult, OutputMode};
use crate::scan_events::{ScanPathReporter, ScanProgressEvent};
use crate::theme::Theme;
use crate::utils;
use anyhow::{Context, Result};
use bytesize;
use std::env;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::sync::Arc;

/// Package manager cache locations to scan
/// Each tuple is (name, path_from_localappdata_or_userprofile)
const CACHE_LOCATIONS: &[(&str, CacheLocation)] = &[
    ("npm", CacheLocation::LocalAppData("npm-cache")),
    ("pip", CacheLocation::LocalAppDataNested(&["pip", "cache"])),
    (
        "yarn",
        CacheLocation::LocalAppDataNested(&["Yarn", "Cache"]),
    ),
    ("pnpm", CacheLocation::LocalAppData("pnpm-cache")),
    ("pnpm-store", CacheLocation::LocalAppData("pnpm-store")),
    (
        "NuGet",
        CacheLocation::LocalAppDataNested(&["NuGet", "v3-cache"]),
    ),
    (
        "Cargo",
        CacheLocation::UserProfileNested(&[".cargo", "registry"]),
    ),
    (
        "Go",
        CacheLocation::UserProfileNested(&["go", "pkg", "mod", "cache"]),
    ),
    (
        "Maven",
        CacheLocation::UserProfileNested(&[".m2", "repository"]),
    ),
    (
        "Gradle",
        CacheLocation::UserProfileNested(&[".gradle", "caches"]),
    ),
];

enum CacheLocation {
    LocalAppData(&'static str),
    LocalAppDataNested(&'static [&'static str]),
    UserProfileNested(&'static [&'static str]),
}

/// Scan for package manager cache directories
///
/// Checks well-known Windows cache locations for various package managers.
/// Uses shared calculate_dir_size for consistent size calculation.
pub fn scan(_root: &Path, config: &Config, output_mode: OutputMode) -> Result<CategoryResult> {
    let mut result = CategoryResult::default();
    let mut candidates = Vec::new();

    let local_appdata = env::var("LOCALAPPDATA").ok().map(PathBuf::from);
    let userprofile = env::var("USERPROFILE").ok().map(PathBuf::from);

    if output_mode != OutputMode::Quiet {
        println!(
            "  {} Checking {} package manager cache locations...",
            Theme::muted("→"),
            CACHE_LOCATIONS.len()
        );
    }

    // 1. Collect candidate paths
    for (name, location) in CACHE_LOCATIONS {
        let cache_path = match location {
            CacheLocation::LocalAppData(subpath) => local_appdata.as_ref().map(|p| p.join(subpath)),
            CacheLocation::LocalAppDataNested(subpaths) => local_appdata.as_ref().map(|p| {
                let mut path = p.clone();
                for subpath in *subpaths {
                    path = path.join(subpath);
                }
                path
            }),
            CacheLocation::UserProfileNested(subpaths) => userprofile.as_ref().map(|p| {
                let mut path = p.clone();
                for subpath in *subpaths {
                    path = path.join(subpath);
                }
                path
            }),
        };

        if let Some(cache_path) = cache_path {
            if cache_path.exists() && !config.is_excluded(&cache_path) {
                candidates.push((name, cache_path));
                if output_mode != OutputMode::Quiet {
                    println!("    {} Found {} cache", Theme::muted("•"), name);
                }
            }
        }
    }

    // 2. Calculate sizes sequentially (one parallel walk at a time)
    let mut paths_with_sizes: Vec<(PathBuf, u64)> = candidates
        .iter()
        .map(|(_name, p)| {
            let size = utils::calculate_dir_size(p);
            (p.clone(), size)
        })
        .filter(|(_, size)| *size > 0)
        .collect();

    // Sort by size descending
    paths_with_sizes.sort_by(|a, b| b.1.cmp(&a.1));

    // Show found caches
    if output_mode != OutputMode::Quiet && !paths_with_sizes.is_empty() {
        println!(
            "  {} Found {} package caches:",
            Theme::muted("→"),
            paths_with_sizes.len()
        );
        let show_count = match output_mode {
            OutputMode::VeryVerbose => paths_with_sizes.len(),
            OutputMode::Verbose => paths_with_sizes.len(),
            OutputMode::Normal => paths_with_sizes.len().min(10),
            OutputMode::Quiet => 0,
        };

        for (i, (path, size)) in paths_with_sizes.iter().take(show_count).enumerate() {
            let size_str = bytesize::to_string(*size, false);
            println!(
                "      {} {} ({})",
                Theme::muted("→"),
                path.display(),
                Theme::size(&size_str)
            );

            if i == 9 && output_mode == OutputMode::Normal && paths_with_sizes.len() > 10 {
                println!(
                    "      {} ... and {} more (use -v to see all)",
                    Theme::muted("→"),
                    paths_with_sizes.len() - 10
                );
                break;
            }
        }
    }

    for (path, size) in paths_with_sizes {
        result.items += 1;
        result.size_bytes += size;
        result.paths.push(path);
    }

    Ok(result)
}

/// Scan with real-time progress events (for TUI).
/// Scan with real-time progress events (for TUI).
pub fn scan_with_progress(
    _root: &Path,
    config: &Config,
    tx: &Sender<ScanProgressEvent>,
) -> Result<CategoryResult> {
    const CATEGORY: &str = "Package Cache";
    let total = CACHE_LOCATIONS.len() as u64;

    let mut result = CategoryResult::default();
    let mut files_with_sizes: Vec<(PathBuf, u64)> = Vec::new();

    let local_appdata = env::var("LOCALAPPDATA").ok().map(PathBuf::from);
    let userprofile = env::var("USERPROFILE").ok().map(PathBuf::from);

    let _ = tx.send(ScanProgressEvent::CategoryStarted {
        category: CATEGORY.to_string(),
        total_units: Some(total),
        current_path: None,
    });

    let reporter = Arc::new(ScanPathReporter::new(CATEGORY, tx.clone(), 10));
    let on_path = |path: &Path| reporter.emit_path(path);

    // Scan known package manager caches
    for (idx, (_name, location)) in CACHE_LOCATIONS.iter().enumerate() {
        let cache_path = match location {
            CacheLocation::LocalAppData(subpath) => local_appdata.as_ref().map(|p| p.join(subpath)),
            CacheLocation::LocalAppDataNested(subpaths) => local_appdata.as_ref().map(|p| {
                let mut path = p.clone();
                for subpath in *subpaths {
                    path = path.join(subpath);
                }
                path
            }),
            CacheLocation::UserProfileNested(subpaths) => userprofile.as_ref().map(|p| {
                let mut path = p.clone();
                for subpath in *subpaths {
                    path = path.join(subpath);
                }
                path
            }),
        };

        if let Some(cache_path) = cache_path {
            if cache_path.exists() && !config.is_excluded(&cache_path) {
                let size = utils::calculate_dir_size_with_progress(&cache_path, &on_path);
                if size > 0 {
                    files_with_sizes.push((cache_path.clone(), size));
                }
            }

            let _ = tx.send(ScanProgressEvent::CategoryProgress {
                category: CATEGORY.to_string(),
                completed_units: (idx + 1) as u64,
                total_units: Some(total),
                current_path: Some(cache_path),
            });
        } else {
            let _ = tx.send(ScanProgressEvent::CategoryProgress {
                category: CATEGORY.to_string(),
                completed_units: (idx + 1) as u64,
                total_units: Some(total),
                current_path: None,
            });
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

/// Clean (delete) a package cache directory by moving it to the Recycle Bin
pub fn clean(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    trash::delete(path).with_context(|| {
        format!(
            "Failed to delete package cache directory: {}",
            path.display()
        )
    })?;
    Ok(())
}
