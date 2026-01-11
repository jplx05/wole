use crate::config::Config;
use crate::output::CategoryResult;
use crate::utils;
use anyhow::{Context, Result};
use std::env;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Browser cache locations to scan
/// Each tuple is (name, path_from_localappdata)
const BROWSER_CACHES: &[(&str, &[&str])] = &[
    // Chrome family
    // Chrome family
    (
        "Chrome",
        &[
            "Google",
            "Chrome",
            "User Data",
            "Default",
            "Cache",
            "Cache_Data",
        ],
    ),
    (
        "Chrome (Beta)",
        &[
            "Google",
            "Chrome Beta",
            "User Data",
            "Default",
            "Cache",
            "Cache_Data",
        ],
    ),
    (
        "Chrome (Dev)",
        &[
            "Google",
            "Chrome Dev",
            "User Data",
            "Default",
            "Cache",
            "Cache_Data",
        ],
    ),
    // Edge family
    (
        "Edge",
        &[
            "Microsoft",
            "Edge",
            "User Data",
            "Default",
            "Cache",
            "Cache_Data",
        ],
    ),
    (
        "Edge (Beta)",
        &[
            "Microsoft",
            "Edge Beta",
            "User Data",
            "Default",
            "Cache",
            "Cache_Data",
        ],
    ),
    (
        "Edge (Dev)",
        &[
            "Microsoft",
            "Edge Dev",
            "User Data",
            "Default",
            "Cache",
            "Cache_Data",
        ],
    ),
    // Brave
    (
        "Brave",
        &[
            "BraveSoftware",
            "Brave-Browser",
            "User Data",
            "Default",
            "Cache",
            "Cache_Data",
        ],
    ),
    (
        "Brave (Beta)",
        &[
            "BraveSoftware",
            "Brave-Browser-Beta",
            "User Data",
            "Default",
            "Cache",
            "Cache_Data",
        ],
    ),
    // Opera
    ("Opera", &["Opera Software", "Opera Stable", "Cache"]),
    // Arc
    (
        "Arc",
        &[
            "The Browser Company",
            "Arc",
            "User Data",
            "Default",
            "Cache",
            "Cache_Data",
        ],
    ),
    // Comet
    (
        "Perplexity",
        &[
            "Perplexity",
            "Perplexity",
            "User Data",
            "Default",
            "Cache",
            "Cache_Data",
        ],
    ),
    // Atlast by OpenAI
    (
        "Atlas",
        &[
            "OpenAI",
            "Atlast",
            "User Data",
            "Default",
            "Cache",
            "Cache_Data",
        ],
    ),
    // Vivaldi
    (
        "Vivaldi",
        &["Vivaldi", "User Data", "Default", "Cache", "Cache_Data"],
    ),
    // Firefox (profile handled separately)
    // ("Firefox", profile-based, see scan impl)
    // Chromium (unbranded)
    (
        "Chromium",
        &["Chromium", "User Data", "Default", "Cache", "Cache_Data"],
    ),
    // Sidekick
    (
        "Sidekick",
        &[
            "Redundant",
            "Sidekick",
            "User Data",
            "Default",
            "Cache",
            "Cache_Data",
        ],
    ),
    // Yandex Browser
    (
        "Yandex",
        &["Yandex", "YandexBrowser", "User Data", "Default", "Cache"],
    ),
    // Avast Secure Browser
    (
        "Avast Secure Browser",
        &[
            "AVAST Software",
            "Browser",
            "User Data",
            "Default",
            "Cache",
            "Cache_Data",
        ],
    ),
    // CCleaner Browser
    (
        "CCleaner Browser",
        &[
            "CCleaner",
            "CCleaner Browser",
            "User Data",
            "Default",
            "Cache",
            "Cache_Data",
        ],
    ),
    // Torch Browser
    (
        "Torch",
        &["Torch", "User Data", "Default", "Cache", "Cache_Data"],
    ),
    // Epic Privacy Browser
    (
        "Epic",
        &[
            "Epic Privacy Browser",
            "User Data",
            "Default",
            "Cache",
            "Cache_Data",
        ],
    ),
];

/// Scan for browser cache directories
///
/// Checks well-known Windows cache locations for Chrome, Edge, and Firefox.
pub fn scan(_root: &Path, config: &Config) -> Result<CategoryResult> {
    let mut result = CategoryResult::default();
    let mut paths = Vec::new();

    let local_appdata = env::var("LOCALAPPDATA").ok().map(PathBuf::from);

    // Scan Chrome and Edge caches (fixed paths)
    if let Some(ref local_appdata_path) = local_appdata {
        for (_name, subpaths) in BROWSER_CACHES {
            let mut cache_path = local_appdata_path.clone();
            for subpath in *subpaths {
                cache_path = cache_path.join(subpath);
            }

            if cache_path.exists() && !config.is_excluded(&cache_path) {
                let size = utils::calculate_dir_size(&cache_path);
                if size > 0 {
                    result.items += 1;
                    result.size_bytes += size;
                    paths.push(cache_path);
                }
            }
        }
    }

    // Scan Firefox profiles (need to glob for profile directories)
    if let Some(ref local_appdata_path) = local_appdata {
        let firefox_profiles = local_appdata_path
            .join("Mozilla")
            .join("Firefox")
            .join("Profiles");
        if firefox_profiles.exists() {
            // Walk through profile directories
            for entry in WalkDir::new(&firefox_profiles)
                .max_depth(2)
                .follow_links(false)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let path = entry.path();
                if path.is_dir()
                    && path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| n.contains(".default"))
                        .unwrap_or(false)
                {
                    // Found a Firefox profile directory, check for cache2
                    let cache2_path = path.join("cache2");
                    if cache2_path.exists() && !config.is_excluded(&cache2_path) {
                        let size = utils::calculate_dir_size(&cache2_path);
                        if size > 0 {
                            result.items += 1;
                            result.size_bytes += size;
                            paths.push(cache2_path);
                        }
                    }
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

/// Clean (delete) a browser cache directory by moving it to the Recycle Bin
pub fn clean(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    crate::trash_ops::delete(path)
        .with_context(|| format!("Failed to delete browser cache: {}", path.display()))?;
    Ok(())
}
