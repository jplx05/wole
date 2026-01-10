use crate::config::Config;
use crate::output::{CategoryResult, OutputMode};
use crate::scan_events::{ScanPathReporter, ScanProgressEvent};
use crate::theme::Theme;
use anyhow::{Context, Result};
use lazy_static::lazy_static;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(windows)]
use winreg::enums::*;
#[cfg(windows)]
use winreg::RegKey;

// NOTE: These must be module-level statics.
// Previously we (incorrectly) created separate `lazy_static!` maps inside `scan()` and inside
// `get_app_display_name()`, which meant the TUI could never find the stored names.
lazy_static! {
    static ref APP_DISPLAY_NAMES: Mutex<HashMap<PathBuf, String>> = Mutex::new(HashMap::new());
    static ref APP_SIZES: Mutex<HashMap<PathBuf, u64>> = Mutex::new(HashMap::new());
    static ref APP_LAST_OPENED: Mutex<HashMap<PathBuf, SystemTime>> = Mutex::new(HashMap::new());
    static ref APP_UNINSTALL_STRINGS: Mutex<HashMap<PathBuf, String>> = Mutex::new(HashMap::new());
    static ref APP_PUBLISHERS: Mutex<HashMap<PathBuf, String>> = Mutex::new(HashMap::new());
}

/// Information about an installed application
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct InstalledApp {
    display_name: String,
    install_location: PathBuf,
    publisher: Option<String>,
    estimated_size: Option<u64>,
    uninstall_string: Option<String>, // UninstallString or QuietUninstallString from registry
    is_uninstallable: bool,           // true if app can be uninstalled
}

/// Check if an application should be excluded (only truly system-critical apps)
#[allow(dead_code)]
fn should_exclude_app(app: &InstalledApp) -> bool {
    // Only exclude apps in critical Windows system directories
    // This allows Microsoft Store apps and regular Microsoft applications
    let install_str = app.install_location.to_string_lossy().to_lowercase();

    // Exclude only core Windows system directories
    if install_str.contains("\\windows\\system32\\")
        || install_str.contains("\\windows\\syswow64\\")
        || install_str.contains("\\windows\\winsxs\\")
        || install_str.contains("\\windows\\servicing\\")
    {
        return true;
    }

    // Exclude Windows Update and Windows Defender (critical security components)
    if let Some(ref publisher) = app.publisher {
        if publisher.contains("Microsoft Corporation") || publisher.contains("Microsoft") {
            let name_lower = app.display_name.to_lowercase();
            // Only exclude critical Windows security/update components
            if name_lower.contains("windows defender")
                || name_lower.contains("windows security")
                || name_lower == "windows update"
                || name_lower.starts_with("windows update ")
            {
                return true;
            }
        }
    }

    false
}

#[cfg(windows)]
/// Read installed applications from Windows registry
#[allow(dead_code)]
fn read_registry_apps() -> Result<Vec<InstalledApp>> {
    let mut apps = Vec::new();
    // Track seen apps by normalized path to avoid duplicates across registry locations
    let mut seen_paths = std::collections::HashSet::new();

    // Registry paths to scan
    let registry_paths = vec![
        (
            RegKey::predef(HKEY_LOCAL_MACHINE),
            "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
        ),
        (
            RegKey::predef(HKEY_CURRENT_USER),
            "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
        ),
        (
            RegKey::predef(HKEY_LOCAL_MACHINE),
            "SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
        ),
    ];

    for (hive, path) in registry_paths {
        if let Ok(key) = hive.open_subkey(path) {
            // Enumerate all subkeys (each represents an installed app)
            // Handle enumeration errors gracefully
            for subkey_name_result in key.enum_keys() {
                let subkey_name = match subkey_name_result {
                    Ok(name) => name,
                    Err(_) => continue, // Skip entries we can't read
                };

                if let Ok(subkey) = key.open_subkey(&subkey_name) {
                    // Read DisplayName
                    let display_name = subkey
                        .get_value::<String, _>("DisplayName")
                        .unwrap_or_default();

                    if display_name.is_empty() {
                        continue;
                    }

                    // Read InstallLocation (often empty for some apps). If missing, derive from
                    // DisplayIcon / InstallSource (these commonly point at the real exe location).
                    let install_location = {
                        let install_location_str = subkey
                            .get_value::<String, _>("InstallLocation")
                            .unwrap_or_default();

                        if !install_location_str.trim().is_empty() {
                            PathBuf::from(install_location_str)
                        } else {
                            // Try DisplayIcon (e.g. "\"C:\\Program Files\\Foo\\foo.exe\",0")
                            let display_icon = subkey
                                .get_value::<String, _>("DisplayIcon")
                                .ok()
                                .unwrap_or_default();
                            let install_source = subkey
                                .get_value::<String, _>("InstallSource")
                                .ok()
                                .unwrap_or_default();

                            derive_install_location(&display_icon)
                                .or_else(|| derive_install_location(&install_source))
                                .unwrap_or_default()
                        }
                    };

                    if install_location.as_os_str().is_empty() {
                        continue;
                    }

                    // Normalize the path (canonicalize if possible for consistent comparison)
                    let normalized_location = install_location
                        .canonicalize()
                        .ok()
                        .unwrap_or_else(|| install_location.clone());

                    // If registry value points at a file, use its parent folder.
                    let normalized_location = if normalized_location.is_file() {
                        normalized_location
                            .parent()
                            .map(|p| p.to_path_buf())
                            .unwrap_or(normalized_location)
                    } else {
                        normalized_location
                    };

                    // Verify the directory exists
                    if !normalized_location.exists() || !normalized_location.is_dir() {
                        continue;
                    }

                    // Check for duplicates across registry locations (use normalized path)
                    // On Windows, paths are case-insensitive, so normalize for comparison
                    let path_key = normalized_location.to_string_lossy().to_lowercase();
                    if seen_paths.contains(&path_key) {
                        continue; // Skip duplicate
                    }
                    seen_paths.insert(path_key);

                    // Check if app is uninstallable
                    let uninstall_string = subkey
                        .get_value::<String, _>("QuietUninstallString")
                        .ok()
                        .or_else(|| subkey.get_value::<String, _>("UninstallString").ok())
                        .filter(|s| !s.is_empty());

                    let is_uninstallable = uninstall_string.is_some();

                    // Read Publisher
                    let publisher = subkey
                        .get_value::<String, _>("Publisher")
                        .ok()
                        .filter(|s| !s.is_empty());

                    // Read EstimatedSize (in bytes, stored as DWORD)
                    let estimated_size = subkey
                        .get_value::<u32, _>("EstimatedSize")
                        .ok()
                        .map(|size| size as u64 * 1024); // Convert KB to bytes

                    let app = InstalledApp {
                        display_name,
                        install_location: normalized_location, // Use normalized path
                        publisher,
                        estimated_size,
                        uninstall_string,
                        is_uninstallable,
                    };

                    // Filter out system-critical apps.
                    // We keep non-uninstallable apps too as long as we have a real install folder.
                    if !should_exclude_app(&app) {
                        apps.push(app);
                    }
                }
            }
        }
    }

    Ok(apps)
}

#[cfg(not(windows))]
/// Stub for non-Windows platforms
#[allow(dead_code)]
fn read_registry_apps() -> Result<Vec<InstalledApp>> {
    Ok(Vec::new())
}

/// Scan for installed applications
#[allow(unused_variables)]
pub fn scan(_root: &Path, config: &Config, output_mode: OutputMode) -> Result<CategoryResult> {
    #[cfg(windows)]
    {
        if output_mode != OutputMode::Quiet {
            println!("  {} Scanning installed applications...", Theme::muted("→"));
        }

        let apps = read_registry_apps()?;

        #[derive(Clone)]
        struct AppEntry {
            install_location: PathBuf,
            display_name: String,
            size: u64,
            last_opened: Option<SystemTime>,
            uninstall_string: Option<String>,
            publisher: Option<String>,
        }

        let mut apps_with_sizes: Vec<AppEntry> = Vec::new();

        for app in apps {
            if config.is_excluded(&app.install_location) {
                continue;
            }

            // Tighten: only include apps we can actually uninstall.
            // Otherwise we risk "deleting files" while the app still appears installed.
            if !app.is_uninstallable {
                continue;
            }

            // Calculate size: use EstimatedSize from registry if available, otherwise walk directory
            // Handle edge case: if directory doesn't exist or can't be read, use estimated size or skip
            let size = if let Some(est_size) = app.estimated_size {
                est_size
            } else {
                // Verify directory still exists before calculating size
                if app.install_location.exists() && app.install_location.is_dir() {
                    crate::utils::calculate_dir_size(&app.install_location)
                } else {
                    // Directory was deleted/moved since registry read - skip this app
                    continue;
                }
            };

            // Include apps even with 0 size if they're uninstallable
            // (some apps might have 0 size but still be uninstallable - e.g., registry-only entries)
            let last_opened = std::fs::metadata(&app.install_location)
                .ok()
                .and_then(|m| m.accessed().ok().or_else(|| m.modified().ok()));

            apps_with_sizes.push(AppEntry {
                install_location: app.install_location.clone(),
                display_name: app.display_name.clone(),
                size,
                last_opened,
                uninstall_string: app.uninstall_string.clone(),
                publisher: app.publisher.clone(),
            });
            if output_mode != OutputMode::Quiet {
                println!(
                    "    {} Found {} ({})",
                    Theme::muted("•"),
                    app.display_name,
                    Theme::size(&bytesize::to_string(size, true))
                );
            }
        }

        // Sort by "last opened" (most recent first), then by size descending
        apps_with_sizes.sort_by(|a, b| {
            let a_key = a
                .last_opened
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let b_key = b
                .last_opened
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            b_key.cmp(&a_key).then_with(|| b.size.cmp(&a.size))
        });

        // Build result
        let paths: Vec<PathBuf> = apps_with_sizes
            .iter()
            .map(|e| e.install_location.clone())
            .collect();
        let size_bytes: u64 = apps_with_sizes.iter().map(|e| e.size).sum();
        let items = apps_with_sizes.len();

        // Store display names + per-app sizes + uninstall commands (used by the TUI / cleaner)
        // Normalize paths for consistent lookup
        let mut names_map = APP_DISPLAY_NAMES.lock().unwrap();
        let mut sizes_map = APP_SIZES.lock().unwrap();
        let mut opened_map = APP_LAST_OPENED.lock().unwrap();
        let mut uninstall_map = APP_UNINSTALL_STRINGS.lock().unwrap();
        let mut publisher_map = APP_PUBLISHERS.lock().unwrap();
        names_map.clear();
        sizes_map.clear();
        opened_map.clear();
        uninstall_map.clear();
        publisher_map.clear();
        for entry in &apps_with_sizes {
            let path = &entry.install_location;
            let normalized_path = path.canonicalize().ok().unwrap_or_else(|| path.clone());
            names_map.insert(normalized_path.clone(), entry.display_name.clone());
            sizes_map.insert(normalized_path.clone(), entry.size);
            if let Some(t) = entry.last_opened {
                opened_map.insert(normalized_path.clone(), t);
            }
            if let Some(ref u) = entry.uninstall_string {
                uninstall_map.insert(normalized_path.clone(), u.clone());
            }
            if let Some(ref p) = entry.publisher {
                publisher_map.insert(normalized_path, p.clone());
            }
        }

        let result = CategoryResult {
            paths,
            size_bytes,
            items,
        };

        if output_mode != OutputMode::Quiet && !apps_with_sizes.is_empty() {
            println!(
                "  {} Found {} installed applications:",
                Theme::muted("→"),
                apps_with_sizes.len()
            );
            let show_count = match output_mode {
                OutputMode::VeryVerbose => apps_with_sizes.len(),
                OutputMode::Verbose => apps_with_sizes.len(),
                OutputMode::Normal => apps_with_sizes.len().min(10),
                OutputMode::Quiet => 0,
            };

            for (i, entry) in apps_with_sizes.iter().take(show_count).enumerate() {
                let size_str = bytesize::to_string(entry.size, true);
                println!(
                    "      {} {} ({})",
                    Theme::muted("→"),
                    entry.install_location.display(),
                    Theme::size(&size_str)
                );

                if i == 9 && output_mode == OutputMode::Normal && apps_with_sizes.len() > 10 {
                    println!(
                        "      {} ... and {} more (use -v to see all)",
                        Theme::muted("→"),
                        apps_with_sizes.len() - 10
                    );
                    break;
                }
            }
        }

        Ok(result)
    }

    #[cfg(not(windows))]
    {
        if output_mode != OutputMode::Quiet {
            println!(
                "  {} Applications scanning is only available on Windows",
                Theme::muted("→")
            );
        }
        Ok(CategoryResult::default())
    }
}

/// Scan with real-time progress events (for TUI)
pub fn scan_with_progress(
    _root: &Path,
    config: &Config,
    tx: &Sender<ScanProgressEvent>,
) -> Result<CategoryResult> {
    const CATEGORY: &str = "Installed Applications";

    #[cfg(windows)]
    {
        let apps = read_registry_apps()?;

        let total = apps.len() as u64;

        let _ = tx.send(ScanProgressEvent::CategoryStarted {
            category: CATEGORY.to_string(),
            total_units: Some(total),
            current_path: None,
        });

        let reporter = Arc::new(ScanPathReporter::new(CATEGORY, tx.clone(), 10));
        let on_path = |path: &Path| reporter.emit_path(path);

        #[derive(Clone)]
        struct AppEntry {
            install_location: PathBuf,
            display_name: String,
            size: u64,
            last_opened: Option<SystemTime>,
            uninstall_string: Option<String>,
            publisher: Option<String>,
        }

        let mut apps_with_sizes: Vec<AppEntry> = Vec::new();

        for (idx, app) in apps.iter().enumerate() {
            if config.is_excluded(&app.install_location) {
                continue;
            }
            // Tighten: only include apps we can actually uninstall.
            if !app.is_uninstallable {
                continue;
            }
            // Calculate size
            // Handle edge case: if directory doesn't exist or can't be read, use estimated size or skip
            let size = if let Some(est_size) = app.estimated_size {
                est_size
            } else {
                // Verify directory still exists before calculating size
                if app.install_location.exists() && app.install_location.is_dir() {
                    crate::utils::calculate_dir_size_with_progress(&app.install_location, &on_path)
                } else {
                    // Directory was deleted/moved since registry read - skip this app
                    continue;
                }
            };

            // Include apps even with 0 size if they're uninstallable
            // (some apps might have 0 size but still be uninstallable - e.g., registry-only entries)
            let last_opened = std::fs::metadata(&app.install_location)
                .ok()
                .and_then(|m| m.accessed().ok().or_else(|| m.modified().ok()));

            apps_with_sizes.push(AppEntry {
                install_location: app.install_location.clone(),
                display_name: app.display_name.clone(),
                size,
                last_opened,
                uninstall_string: app.uninstall_string.clone(),
                publisher: app.publisher.clone(),
            });

            let completed = (idx + 1) as u64;
            let _ = tx.send(ScanProgressEvent::CategoryProgress {
                category: CATEGORY.to_string(),
                completed_units: completed,
                total_units: Some(total),
                current_path: Some(app.install_location.clone()),
            });
        }

        // Sort by "last opened" (most recent first), then by size descending
        apps_with_sizes.sort_by(|a, b| {
            let a_key = a
                .last_opened
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let b_key = b
                .last_opened
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            b_key.cmp(&a_key).then_with(|| b.size.cmp(&a.size))
        });

        // Store display names + per-app sizes + uninstall commands (used by the TUI / cleaner)
        // Normalize paths for consistent lookup
        let mut names_map = APP_DISPLAY_NAMES.lock().unwrap();
        let mut sizes_map = APP_SIZES.lock().unwrap();
        let mut opened_map = APP_LAST_OPENED.lock().unwrap();
        let mut uninstall_map = APP_UNINSTALL_STRINGS.lock().unwrap();
        let mut publisher_map = APP_PUBLISHERS.lock().unwrap();
        names_map.clear();
        sizes_map.clear();
        opened_map.clear();
        uninstall_map.clear();
        publisher_map.clear();
        for entry in &apps_with_sizes {
            let path = &entry.install_location;
            let normalized_path = path.canonicalize().ok().unwrap_or_else(|| path.clone());
            names_map.insert(normalized_path.clone(), entry.display_name.clone());
            sizes_map.insert(normalized_path.clone(), entry.size);
            if let Some(t) = entry.last_opened {
                opened_map.insert(normalized_path.clone(), t);
            }
            if let Some(ref u) = entry.uninstall_string {
                uninstall_map.insert(normalized_path.clone(), u.clone());
            }
            if let Some(ref p) = entry.publisher {
                publisher_map.insert(normalized_path, p.clone());
            }
        }

        // Build final result
        let mut items = 0;
        let mut size_bytes = 0;
        let mut paths = Vec::new();
        for entry in apps_with_sizes {
            items += 1;
            size_bytes += entry.size;
            paths.push(entry.install_location);
        }

        let result = CategoryResult {
            paths,
            size_bytes,
            items,
        };

        let _ = tx.send(ScanProgressEvent::CategoryFinished {
            category: CATEGORY.to_string(),
            items: result.items,
            size_bytes: result.size_bytes,
        });

        Ok(result)
    }

    #[cfg(not(windows))]
    {
        let _ = tx.send(ScanProgressEvent::CategoryStarted {
            category: CATEGORY.to_string(),
            total_units: Some(0),
            current_path: None,
        });

        let _ = tx.send(ScanProgressEvent::CategoryFinished {
            category: CATEGORY.to_string(),
            items: 0,
            size_bytes: 0,
        });

        Ok(CategoryResult::default())
    }
}

/// Get display name for an application path (if available)
/// Handles path normalization for consistent lookup
pub fn get_app_display_name(path: &Path) -> Option<String> {
    let map = APP_DISPLAY_NAMES.lock().ok()?;

    // Try to find by exact path first
    if let Some(name) = map.get(path) {
        return Some(name.clone());
    }

    // Try canonicalized path
    if let Ok(canonical) = path.canonicalize() {
        if let Some(name) = map.get(&canonical) {
            return Some(name.clone());
        }
    }

    // Try case-insensitive lookup on Windows (since Windows paths are case-insensitive)
    #[cfg(windows)]
    {
        let path_lower = path.to_string_lossy().to_lowercase();
        for (stored_path, name) in map.iter() {
            if stored_path.to_string_lossy().to_lowercase() == path_lower {
                return Some(name.clone());
            }
        }
    }

    None
}

/// Get size (bytes) for an installed application path (if available).
/// Uses the same normalization logic as `get_app_display_name`.
pub fn get_app_size(path: &Path) -> Option<u64> {
    let map = APP_SIZES.lock().ok()?;

    if let Some(sz) = map.get(path) {
        return Some(*sz);
    }

    if let Ok(canonical) = path.canonicalize() {
        if let Some(sz) = map.get(&canonical) {
            return Some(*sz);
        }
    }

    #[cfg(windows)]
    {
        let path_lower = path.to_string_lossy().to_lowercase();
        for (stored_path, sz) in map.iter() {
            if stored_path.to_string_lossy().to_lowercase() == path_lower {
                return Some(*sz);
            }
        }
    }

    None
}

/// Get "last opened" (best-effort) for an installed application path (if available).
/// This is populated during scan from folder accessed() (fallback to modified()).
pub fn get_app_last_opened(path: &Path) -> Option<SystemTime> {
    let map = APP_LAST_OPENED.lock().ok()?;

    if let Some(t) = map.get(path) {
        return Some(*t);
    }

    if let Ok(canonical) = path.canonicalize() {
        if let Some(t) = map.get(&canonical) {
            return Some(*t);
        }
    }

    #[cfg(windows)]
    {
        let path_lower = path.to_string_lossy().to_lowercase();
        for (stored_path, t) in map.iter() {
            if stored_path.to_string_lossy().to_lowercase() == path_lower {
                return Some(*t);
            }
        }
    }

    None
}

#[cfg(windows)]
fn derive_install_location(raw: &str) -> Option<PathBuf> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }

    // Expand %VAR% if present (best-effort)
    let expanded = expand_percent_env_vars(s);
    let s = expanded.trim();

    // 1) Quoted path: "C:\Path With Spaces\app.exe",0
    if let Some(stripped) = s.strip_prefix('"') {
        if let Some(end_quote) = stripped.find('"') {
            let candidate = &stripped[..end_quote];
            return normalize_install_candidate(candidate);
        }
    }

    // 2) Unquoted but may contain ",0" or args: take before comma first
    let before_comma = s.split(',').next().unwrap_or(s).trim();

    // If it looks like a path with args (…\app.exe /something), cut at the first " .exe"
    // We can't safely split on whitespace because unquoted Windows paths can contain spaces.
    if let Some(cut) = find_executable_prefix(before_comma) {
        return normalize_install_candidate(cut);
    }

    // Otherwise treat it as a directory path
    normalize_install_candidate(before_comma)
}

#[cfg(windows)]
fn normalize_install_candidate(candidate: &str) -> Option<PathBuf> {
    let p = PathBuf::from(candidate.trim());
    if p.as_os_str().is_empty() {
        return None;
    }
    if p.exists() {
        if p.is_dir() {
            return Some(p);
        }
        if p.is_file() {
            return p.parent().map(|pp| pp.to_path_buf());
        }
    }
    None
}

#[cfg(windows)]
fn expand_percent_env_vars(input: &str) -> String {
    // Very small %VAR% expander (best-effort).
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '%' {
            let mut var = String::new();
            while let Some(&c) = chars.peek() {
                chars.next();
                if c == '%' {
                    break;
                }
                var.push(c);
            }
            if !var.is_empty() {
                if let Ok(val) = std::env::var(&var) {
                    out.push_str(&val);
                    continue;
                }
            }
            // If we couldn't expand, keep the original token
            out.push('%');
            out.push_str(&var);
            out.push('%');
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(windows)]
fn find_executable_prefix(s: &str) -> Option<&str> {
    // Find the end of an executable path inside the string.
    // Common endings in registry values: .exe, .dll, .ico
    for ext in [".exe", ".dll", ".ico"] {
        if let Some(idx) = s.to_lowercase().find(ext) {
            let end = idx + ext.len();
            if end <= s.len() {
                return Some(&s[..end]);
            }
        }
    }
    None
}

/// Get uninstall command for an application path (if available).
/// This is captured during scan from registry (QuietUninstallString preferred).
pub fn get_app_uninstall_string(path: &Path) -> Option<String> {
    let map = APP_UNINSTALL_STRINGS.lock().ok()?;

    if let Some(cmd) = map.get(path) {
        return Some(cmd.clone());
    }
    if let Ok(canonical) = path.canonicalize() {
        if let Some(cmd) = map.get(&canonical) {
            return Some(cmd.clone());
        }
    }

    #[cfg(windows)]
    {
        let path_lower = path.to_string_lossy().to_lowercase();
        for (stored_path, cmd) in map.iter() {
            if stored_path.to_string_lossy().to_lowercase() == path_lower {
                return Some(cmd.clone());
            }
        }
    }

    None
}

/// Get publisher for an application path (if available).
pub fn get_app_publisher(path: &Path) -> Option<String> {
    let map = APP_PUBLISHERS.lock().ok()?;

    if let Some(p) = map.get(path) {
        return Some(p.clone());
    }
    if let Ok(canonical) = path.canonicalize() {
        if let Some(p) = map.get(&canonical) {
            return Some(p.clone());
        }
    }

    #[cfg(windows)]
    {
        let path_lower = path.to_string_lossy().to_lowercase();
        for (stored_path, p) in map.iter() {
            if stored_path.to_string_lossy().to_lowercase() == path_lower {
                return Some(p.clone());
            }
        }
    }

    None
}

#[cfg(windows)]
fn sanitize_windows_component(s: &str) -> String {
    // Avoid wild deletions; keep it filesystem-friendly.
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, ' ' | '-' | '_' | '.') {
            out.push(ch);
        }
    }
    let trimmed = out.trim();
    if trimmed.is_empty() {
        "Unknown".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(windows)]
fn is_generic_component(name: &str) -> bool {
    let n = name.trim().to_lowercase();
    matches!(
        n.as_str(),
        "microsoft"
            | "windows"
            | "common files"
            | "program files"
            | "program files (x86)"
            | "system32"
            | "syswow64"
            | "users"
            | "public"
    )
}

#[cfg(windows)]
fn split_cmdline_best_effort(raw: &str) -> (String, Vec<String>) {
    // Best-effort parser for registry command lines:
    // - handles quoted executable
    // - otherwise tries to cut at .exe/.cmd/.bat
    let expanded = expand_percent_env_vars(raw);
    let s = expanded.trim();
    if s.is_empty() {
        return (String::new(), Vec::new());
    }

    if let Some(stripped) = s.strip_prefix('"') {
        if let Some(end_quote) = stripped.find('"') {
            let exe = stripped[..end_quote].to_string();
            let rest = stripped[end_quote + 1..].trim();
            let args = if rest.is_empty() {
                Vec::new()
            } else {
                rest.split_whitespace().map(|x| x.to_string()).collect()
            };
            return (exe, args);
        }
    }

    // Unquoted: attempt to find end of an executable path.
    let lower = s.to_lowercase();
    for ext in [".exe", ".cmd", ".bat"] {
        if let Some(idx) = lower.find(ext) {
            let end = idx + ext.len();
            let exe = s[..end].trim().to_string();
            let rest = s[end..].trim();
            let args = if rest.is_empty() {
                Vec::new()
            } else {
                rest.split_whitespace().map(|x| x.to_string()).collect()
            };
            return (exe, args);
        }
    }

    // Fallback: first token is executable.
    let mut parts = s.split_whitespace();
    let exe = parts.next().unwrap_or_default().to_string();
    let args = parts.map(|x| x.to_string()).collect();
    (exe, args)
}

#[cfg(windows)]
fn extract_msi_product_code(uninstall_cmd: &str) -> Option<String> {
    // Look for a GUID in braces.
    let s = uninstall_cmd;
    let start = s.find('{')?;
    let end = s[start..].find('}')? + start;
    if end <= start {
        return None;
    }
    Some(s[start..=end].to_string())
}

#[cfg(windows)]
fn msi_uninstall_command(product_code: &str) -> (String, Vec<String>) {
    // Silent MSI uninstall.
    (
        "msiexec.exe".to_string(),
        vec![
            "/x".to_string(),
            product_code.to_string(),
            "/qn".to_string(),
            "/norestart".to_string(),
        ],
    )
}

/// Build a list of likely leftover artifact paths for an installed application.
///
/// This is intentionally conservative: it targets app-specific leaves (not whole vendor roots),
/// based on the install path layout and display name.
pub fn get_app_artifact_paths(install_location: &Path) -> Vec<PathBuf> {
    #[cfg(not(windows))]
    {
        let _ = install_location;
        return Vec::new();
    }

    #[cfg(windows)]
    {
        use std::collections::HashSet;

        let mut out: HashSet<PathBuf> = HashSet::new();
        out.insert(install_location.to_path_buf());

        let display_name = get_app_display_name(install_location).unwrap_or_else(|| {
            install_location
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Application")
                .to_string()
        });
        let display_leaf = sanitize_windows_component(&display_name);
        let display_leaf_ok = !display_leaf.is_empty() && !is_generic_component(&display_leaf);

        let local_appdata = std::env::var("LOCALAPPDATA").ok().map(PathBuf::from);
        let roaming_appdata = std::env::var("APPDATA").ok().map(PathBuf::from);
        let program_data = std::env::var("ProgramData").ok().map(PathBuf::from);
        let userprofile = std::env::var("USERPROFILE").ok().map(PathBuf::from);

        // Heuristic vendor/product from install location (Program Files layout).
        // Example: C:\Program Files\Vendor\Product\...
        let pf = std::env::var("ProgramFiles").ok().map(PathBuf::from);
        let pf86 = std::env::var("ProgramFiles(x86)").ok().map(PathBuf::from);

        let install_str = install_location.to_string_lossy().replace('/', "\\");
        let install_lower = install_str.to_lowercase();

        let mut vendor: Option<String> = None;
        let mut product: Option<String> = None;

        for root in [pf, pf86].into_iter().flatten() {
            let root_str = root.to_string_lossy().replace('/', "\\");
            let mut root_lower = root_str.to_lowercase();
            if !root_lower.ends_with('\\') {
                root_lower.push('\\');
            }
            if install_lower.starts_with(&root_lower) {
                let rel = &install_str[root_lower.len()..];
                let mut comps = rel.split('\\').filter(|c| !c.is_empty());
                vendor = comps.next().map(sanitize_windows_component);
                product = comps.next().map(sanitize_windows_component);
                break;
            }
        }

        // If install is directly under Program Files\Product, treat vendor=product.
        let vendor = vendor.unwrap_or_else(|| display_leaf.clone());
        let product = product.unwrap_or_else(|| display_leaf.clone());
        let vendor_ok = !vendor.is_empty() && !is_generic_component(&vendor);
        let product_ok = !product.is_empty() && !is_generic_component(&product);

        for base in [
            local_appdata.clone(),
            roaming_appdata.clone(),
            program_data.clone(),
        ]
        .into_iter()
        .flatten()
        {
            if vendor_ok && product_ok {
                // Tighten: only delete a *vendor/product* leaf, not vendor roots or generic names.
                // This avoids nuking things like %APPDATA%\Microsoft or similarly broad folders.
                out.insert(base.join(&vendor).join(&product));
            }
        }

        // Start menu shortcuts (per-user and all-users).
        if let Some(roaming) = roaming_appdata {
            let programs = roaming
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs");
            if product_ok {
                out.insert(programs.join(&product));
                out.insert(programs.join(format!("{}.lnk", product)));
            }
            if vendor_ok && product_ok {
                out.insert(programs.join(&vendor).join(&product));
            }
            if display_leaf_ok {
                out.insert(programs.join(format!("{}.lnk", display_leaf)));
            }
        }
        if let Some(pd) = program_data {
            let programs = pd
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs");
            if product_ok {
                out.insert(programs.join(&product));
                out.insert(programs.join(format!("{}.lnk", product)));
            }
            if vendor_ok && product_ok {
                out.insert(programs.join(&vendor).join(&product));
            }
            if display_leaf_ok {
                out.insert(programs.join(format!("{}.lnk", display_leaf)));
            }
        }

        // Desktop shortcuts.
        if let Some(up) = userprofile {
            let desktop = up.join("Desktop");
            if product_ok {
                out.insert(desktop.join(format!("{}.lnk", product)));
            }
            if display_leaf_ok {
                out.insert(desktop.join(format!("{}.lnk", display_leaf)));
            }
        }

        // Drop non-existent paths to keep the deletion list tight.
        let mut v: Vec<PathBuf> = out.into_iter().collect();
        v.retain(|p| p.exists());
        v
    }
}

/// Attempt to uninstall an application by running its registry uninstall command.
///
/// - If the uninstall command looks like MSI, we force a silent `/x ... /qn /norestart`.
/// - Otherwise we run the stored uninstall string as-is (best effort).
///
/// Returns Ok(()) if we launched and the process exited successfully.
pub fn uninstall(install_location: &Path) -> Result<()> {
    #[cfg(not(windows))]
    {
        let _ = install_location;
        return Ok(());
    }

    #[cfg(windows)]
    {
        use anyhow::anyhow;
        use std::process::Command;

        let Some(raw_cmd) = get_app_uninstall_string(install_location) else {
            // Some registry entries have no uninstall command; caller can fall back to file deletion.
            return Err(anyhow!(
                "No uninstall command available for this application"
            ));
        };

        // If it looks like MSI, prefer a known-safe silent uninstall.
        let lower = raw_cmd.to_lowercase();
        if lower.contains("msiexec") || lower.contains("{") {
            if let Some(guid) = extract_msi_product_code(&raw_cmd) {
                let (exe, args) = msi_uninstall_command(&guid);
                let status = Command::new(exe).args(args).status()?;
                if status.success() {
                    return Ok(());
                }
                return Err(anyhow!(
                    "MSI uninstall failed (exit code: {:?})",
                    status.code()
                ));
            }
        }

        let (exe, args) = split_cmdline_best_effort(&raw_cmd);
        if exe.is_empty() {
            return Err(anyhow!("Uninstall command was empty"));
        }

        let status = Command::new(exe).args(args).status()?;
        if status.success() {
            Ok(())
        } else {
            Err(anyhow!("Uninstall failed (exit code: {:?})", status.code()))
        }
    }
}

/// Best-effort check: is this app still present in the uninstall registry list?
///
/// This is used to avoid claiming success when the app still shows as installed.
pub fn is_still_installed(install_location: &Path) -> bool {
    #[cfg(not(windows))]
    {
        let _ = install_location;
        return false;
    }

    #[cfg(windows)]
    {
        let Ok(apps) = read_registry_apps() else {
            // If we cannot read registry, don't block cleanup but also don't claim "still installed".
            return false;
        };

        let target = install_location
            .canonicalize()
            .ok()
            .unwrap_or_else(|| install_location.to_path_buf())
            .to_string_lossy()
            .to_lowercase();

        for app in apps {
            let loc = app
                .install_location
                .canonicalize()
                .ok()
                .unwrap_or_else(|| app.install_location.clone())
                .to_string_lossy()
                .to_lowercase();
            if loc == target {
                return true;
            }
        }

        false
    }
}

/// Clean (delete) an installed application directory by moving it to the Recycle Bin
pub fn clean(path: &Path) -> Result<()> {
    trash::delete(path).with_context(|| {
        format!(
            "Failed to delete installed application directory: {}",
            path.display()
        )
    })?;
    Ok(())
}
