use crate::output::CategoryResult;
use crate::utils;
use anyhow::{Context, Result};
use std::env;
use std::path::{Path, PathBuf};

/// Cache locations to scan
/// Each tuple is (name, path_from_localappdata_or_userprofile)
const CACHE_LOCATIONS: &[(&str, CacheLocation)] = &[
    ("npm", CacheLocation::LocalAppData("npm-cache")),
    ("pip", CacheLocation::LocalAppDataNested(&["pip", "cache"])),
    ("yarn", CacheLocation::LocalAppDataNested(&["Yarn", "Cache"])),
    ("pnpm", CacheLocation::LocalAppData("pnpm-cache")),
    ("pnpm-store", CacheLocation::LocalAppData("pnpm-store")),
    ("NuGet", CacheLocation::LocalAppDataNested(&["NuGet", "v3-cache"])),
    ("Cargo", CacheLocation::UserProfileNested(&[".cargo", "registry"])),
    ("Go", CacheLocation::UserProfileNested(&["go", "pkg", "mod", "cache"])),
    ("Maven", CacheLocation::UserProfileNested(&[".m2", "repository"])),
    ("Gradle", CacheLocation::UserProfileNested(&[".gradle", "caches"])),
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
pub fn scan(_root: &Path) -> Result<CategoryResult> {
    let mut result = CategoryResult::default();
    let mut paths = Vec::new();

    let local_appdata = env::var("LOCALAPPDATA").ok().map(PathBuf::from);
    let userprofile = env::var("USERPROFILE").ok().map(PathBuf::from);

    for (_name, location) in CACHE_LOCATIONS {
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
            if cache_path.exists() {
                let size = utils::calculate_dir_size(&cache_path);
                if size > 0 {
                    result.items += 1;
                    result.size_bytes += size;
                    paths.push(cache_path);
                }
            }
        }
    }

    // Sort by size descending
    let mut paths_with_sizes: Vec<(PathBuf, u64)> = paths
        .into_iter()
        .map(|p| {
            let size = utils::calculate_dir_size(&p);
            (p, size)
        })
        .collect();
    paths_with_sizes.sort_by(|a, b| b.1.cmp(&a.1));

    result.paths = paths_with_sizes.into_iter().map(|(p, _)| p).collect();

    Ok(result)
}

/// Clean (delete) a cache directory by moving it to the Recycle Bin
pub fn clean(path: &Path) -> Result<()> {
    trash::delete(path)
        .with_context(|| format!("Failed to delete cache directory: {}", path.display()))?;
    Ok(())
}
