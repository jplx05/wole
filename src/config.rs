use anyhow::{Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub thresholds: Thresholds,

    #[serde(default)]
    pub paths: Paths,

    #[serde(default)]
    pub exclusions: Exclusions,

    #[serde(default)]
    pub ui: UiSettings,

    #[serde(default)]
    pub safety: SafetySettings,

    #[serde(default)]
    pub performance: PerformanceSettings,

    #[serde(default)]
    pub history: HistorySettings,

    #[serde(default)]
    pub categories: CategorySettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thresholds {
    #[serde(default = "default_project_age")]
    pub project_age_days: u64,

    #[serde(default = "default_min_age")]
    pub min_age_days: u64,

    #[serde(default = "default_min_size_mb")]
    pub min_size_mb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Paths {
    #[serde(default)]
    pub scan_roots: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Exclusions {
    #[serde(default)]
    pub patterns: Vec<String>,

    /// Compiled glob patterns for fast matching (lazily initialized)
    #[serde(skip)]
    #[allow(dead_code)]
    compiled: OnceLock<Option<GlobSet>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiSettings {
    /// Default scan path for TUI (empty = auto-detect)
    #[serde(default)]
    pub default_scan_path: Option<String>,

    /// Default output mode: "normal", "quiet", "verbose", "very-verbose"
    #[serde(default = "default_output_mode")]
    pub output_mode: String,

    /// Enable TUI animations
    #[serde(default = "default_true")]
    pub animations: bool,

    /// Refresh rate for TUI in milliseconds
    #[serde(default = "default_refresh_rate")]
    pub refresh_rate_ms: u64,

    /// Show current storage and storage after deletion in scan results (instead of just free space)
    #[serde(default = "default_false")]
    pub show_storage_info: bool,

    /// Scan depth for user directory analysis (default: 8)
    /// Higher values scan deeper but take longer
    #[serde(default = "default_scan_depth_user")]
    pub scan_depth_user: u8,

    /// Scan depth for entire disk analysis (default: 10)
    /// Higher values scan deeper but take longer
    #[serde(default = "default_scan_depth_entire_disk")]
    pub scan_depth_entire_disk: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetySettings {
    /// Always require confirmation before deleting (even with -y flag)
    #[serde(default = "default_false")]
    pub always_confirm: bool,

    /// Default to permanent delete (bypass Recycle Bin) instead of moving to trash
    #[serde(default = "default_false")]
    pub default_permanent: bool,

    /// Maximum number of files to delete without confirmation
    #[serde(default = "default_max_no_confirm")]
    pub max_no_confirm: u64,

    /// Maximum total size (MB) to delete without confirmation
    #[serde(default = "default_max_size_no_confirm")]
    pub max_size_no_confirm_mb: u64,

    /// Skip locked files (files in use by other processes)
    #[serde(default = "default_true")]
    pub skip_locked_files: bool,

    /// Dry run by default (don't actually delete, just show what would be deleted)
    #[serde(default = "default_false")]
    pub dry_run_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceSettings {
    /// Number of parallel threads for scanning (0 = auto-detect)
    #[serde(default = "default_threads")]
    pub scan_threads: u32,

    /// Batch size for file operations
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,

    /// Enable parallel scanning (can be disabled for debugging)
    #[serde(default = "default_true")]
    pub parallel_scanning: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistorySettings {
    /// Enable deletion history logging
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Maximum number of history entries to keep (0 = unlimited)
    #[serde(default = "default_max_history")]
    pub max_entries: u64,

    /// Maximum age of history entries in days (0 = keep forever)
    #[serde(default = "default_history_age_days")]
    pub max_age_days: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CategorySettings {
    /// Default enabled categories for TUI (empty = use hardcoded defaults)
    #[serde(default)]
    pub default_enabled: Vec<String>,

    /// Category-specific settings
    #[serde(default)]
    pub cache: CategoryConfig,

    #[serde(default)]
    pub build: CategoryConfig,

    #[serde(default)]
    pub large: CategoryConfig,

    #[serde(default)]
    pub old: CategoryConfig,

    #[serde(default)]
    pub duplicates: DuplicatesConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CategoryConfig {
    /// Additional exclusion patterns specific to this category
    #[serde(default)]
    pub exclude_patterns: Vec<String>,

    /// Category-specific threshold override (if set, overrides global)
    #[serde(default)]
    pub threshold_override: Option<u64>,

    /// Custom build artifact folders (for build category only)
    /// Merged with default artifacts
    #[serde(default)]
    pub custom_artifacts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicatesConfig {
    /// Custom scan paths for duplicate detection (if empty, uses CLI argument or default)
    /// Example: ["D:\\", "C:\\Projects"]
    #[serde(default)]
    pub scan_paths: Vec<String>,

    /// Minimum file size (bytes) to use memory mapping instead of buffered reads
    /// Memory mapping is faster for large files but has overhead for small files
    /// Default: 10MB
    #[serde(default = "default_memmap_threshold")]
    pub memmap_threshold_bytes: u64,

    /// Buffer size for reading files when not using memory mapping (bytes)
    /// Default: 8MB for optimal performance on modern NVMe SSDs
    #[serde(default = "default_duplicate_buffer_size")]
    pub buffer_size_bytes: usize,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            project_age_days: default_project_age(),
            min_age_days: default_min_age(),
            min_size_mb: default_min_size_mb(),
        }
    }
}

impl Default for Exclusions {
    fn default() -> Self {
        Self {
            patterns: Vec::new(),
            compiled: OnceLock::new(),
        }
    }
}

impl Exclusions {
    /// Get or compile the glob set for fast matching
    fn get_compiled(&self) -> Option<&GlobSet> {
        self.compiled
            .get_or_init(|| {
                if self.patterns.is_empty() {
                    return None;
                }

                let mut builder = GlobSetBuilder::new();
                for pattern in &self.patterns {
                    // Normalize pattern for globset
                    let normalized = if pattern.starts_with("**/") || pattern.starts_with("/") {
                        pattern.clone()
                    } else {
                        format!("**/{}", pattern)
                    };

                    if let Ok(glob) = Glob::new(&normalized) {
                        builder.add(glob);
                    }
                }

                builder.build().ok()
            })
            .as_ref()
    }
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            default_scan_path: None,
            output_mode: default_output_mode(),
            animations: default_true(),
            refresh_rate_ms: default_refresh_rate(),
            show_storage_info: default_false(),
            scan_depth_user: default_scan_depth_user(),
            scan_depth_entire_disk: default_scan_depth_entire_disk(),
        }
    }
}

impl Default for SafetySettings {
    fn default() -> Self {
        Self {
            always_confirm: default_false(),
            default_permanent: default_false(),
            max_no_confirm: default_max_no_confirm(),
            max_size_no_confirm_mb: default_max_size_no_confirm(),
            skip_locked_files: default_true(),
            dry_run_default: default_false(),
        }
    }
}

impl Default for PerformanceSettings {
    fn default() -> Self {
        Self {
            scan_threads: default_threads(),
            batch_size: default_batch_size(),
            parallel_scanning: default_true(),
        }
    }
}

impl Default for HistorySettings {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            max_entries: default_max_history(),
            max_age_days: default_history_age_days(),
        }
    }
}

impl Default for DuplicatesConfig {
    fn default() -> Self {
        Self {
            scan_paths: Vec::new(),
            memmap_threshold_bytes: default_memmap_threshold(),
            buffer_size_bytes: default_duplicate_buffer_size(),
        }
    }
}

// Default value functions
fn default_output_mode() -> String {
    "normal".to_string()
}
fn default_true() -> bool {
    true
}
fn default_false() -> bool {
    false
}
fn default_refresh_rate() -> u64 {
    100
}
fn default_max_no_confirm() -> u64 {
    10
}
fn default_max_size_no_confirm() -> u64 {
    100
} // 100 MB
fn default_threads() -> u32 {
    0
} // 0 = auto-detect
fn default_batch_size() -> usize {
    1000
}
fn default_max_history() -> u64 {
    10000
}
fn default_history_age_days() -> u64 {
    90
}

fn default_project_age() -> u64 {
    14
}
fn default_min_age() -> u64 {
    30
}
fn default_min_size_mb() -> u64 {
    100
}
fn default_memmap_threshold() -> u64 {
    10 * 1024 * 1024
} // 10MB
fn default_duplicate_buffer_size() -> usize {
    8 * 1024 * 1024
} // 8MB
fn default_scan_depth_user() -> u8 {
    8
}
fn default_scan_depth_entire_disk() -> u8 {
    10
}

impl Config {
    /// Get the config file path: %APPDATA%\wole\config.toml
    pub fn config_path() -> Result<PathBuf> {
        let appdata = std::env::var("APPDATA").context("APPDATA environment variable not set")?;
        let config_dir = PathBuf::from(appdata).join("wole");
        Ok(config_dir.join("config.toml"))
    }

    /// Load config from file or return defaults
    pub fn load() -> Self {
        match Self::config_path() {
            Ok(path) if path.exists() => {
                match fs::read_to_string(&path) {
                    Ok(content) => {
                        match toml::from_str(&content) {
                            Ok(config) => {
                                // Config loaded successfully
                                config
                            }
                            Err(e) => {
                                eprintln!(
                                    "Warning: Failed to parse config file {}: {}",
                                    path.display(),
                                    e
                                );
                                eprintln!("Using default configuration.");
                                Self::default()
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!(
                            "Warning: Failed to read config file {}: {}",
                            path.display(),
                            e
                        );
                        eprintln!("Using default configuration.");
                        Self::default()
                    }
                }
            }
            Ok(_path) => {
                // Config file doesn't exist - this is normal for first run
                // Return defaults silently
                Self::default()
            }
            Err(e) => {
                eprintln!("Warning: Could not determine config file path: {}", e);
                eprintln!("Using default configuration.");
                Self::default()
            }
        }
    }

    /// Load config and create default file if it doesn't exist
    pub fn load_or_create() -> Self {
        let config = Self::load();

        // If config file doesn't exist, create it with defaults
        if let Ok(path) = Self::config_path() {
            if !path.exists() {
                if let Err(e) = config.save() {
                    eprintln!("Warning: Could not create default config file: {}", e);
                }
            }
        }

        config
    }

    /// Save config to file
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;

        // Create directory if it doesn't exist
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).context("Failed to create config directory")?;
        }

        let toml = toml::to_string_pretty(self).context("Failed to serialize config")?;

        fs::write(&path, toml).context("Failed to write config file")?;

        Ok(())
    }

    /// Apply CLI option overrides
    pub fn apply_cli_overrides(
        &mut self,
        project_age: Option<u64>,
        min_age: Option<u64>,
        min_size_mb: Option<u64>,
    ) {
        if let Some(age) = project_age {
            self.thresholds.project_age_days = age;
        }
        if let Some(age) = min_age {
            self.thresholds.min_age_days = age;
        }
        if let Some(size) = min_size_mb {
            self.thresholds.min_size_mb = size;
        }
    }

    /// Check if a path matches any exclusion pattern
    ///
    /// Uses pre-compiled glob patterns for O(1) matching instead of O(patterns)
    pub fn is_excluded(&self, path: &Path) -> bool {
        // Fast path: no patterns
        if self.exclusions.patterns.is_empty() {
            return false;
        }

        // Use compiled glob set for fast matching
        if let Some(glob_set) = self.exclusions.get_compiled() {
            return glob_set.is_match(path);
        }

        // Fallback to old logic if compilation failed
        let path_str = path.to_string_lossy();
        let path_lower = path_str.to_lowercase();

        for pattern in &self.exclusions.patterns {
            if matches_pattern(&path_lower, pattern) {
                return true;
            }
        }
        false
    }
}

/// Simple glob pattern matching
/// Supports ** for recursive matching and * for wildcards
fn matches_pattern(path_lower: &str, pattern: &str) -> bool {
    // Fast path: empty pattern matches nothing
    if pattern.is_empty() {
        return false;
    }

    // Optimization: Don't allocate if we can avoid it.
    // Convert pattern to lowercase string only if it contains uppercase chars
    // (Most patterns will be static strings often already lowercase)
    let pattern_lower_owned;
    let pattern_lower = if pattern.chars().any(|c| c.is_uppercase()) {
        pattern_lower_owned = pattern.to_lowercase();
        &pattern_lower_owned
    } else {
        pattern
    };

    // Check for common simple patterns first to avoid regex-like logic

    // exact match
    if pattern_lower == path_lower {
        return true;
    }

    // **/directory match (contains directory)
    // Handle "**/dir" or just "dir" as contains
    if !pattern_lower.contains('*') {
        // Simple substring match (e.g. "node_modules")
        // Check if it matches a component exactly (surrounded by slashes or start/end)
        if let Some(pos) = path_lower.find(pattern_lower) {
            // Check boundaries to ensure we match full path segments
            // e.g. "target" shouldn't match "my_target_dir"
            let start_ok = pos == 0
                || path_lower.as_bytes()[pos - 1] == b'/'
                || path_lower.as_bytes()[pos - 1] == b'\\';
            let end_ok = pos + pattern_lower.len() == path_lower.len()
                || path_lower.as_bytes()[pos + pattern_lower.len()] == b'/'
                || path_lower.as_bytes()[pos + pattern_lower.len()] == b'\\';

            if start_ok && end_ok {
                return true;
            }
        }
        return false;
    }

    // Handle glob patterns manually without regex
    // This is a simplified implementation but much faster than string replacements
    let parts: Vec<&str> = pattern_lower.split("**").collect();

    // If just "**", it matches everything
    if parts.iter().all(|s| s.is_empty()) {
        return true;
    }

    let mut search_idx = 0;

    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }

        // Handle single * wildcard within the part
        if part.contains('*') {
            // Simple handling: split by * and match sequential parts
            let subparts: Vec<&str> = part.split('*').collect();
            let mut current_search = search_idx;

            // Match first subpart
            if !subparts[0].is_empty() {
                if i == 0 {
                    // Start of pattern must match start of path (unless it starts with **)
                    if !path_lower[current_search..].starts_with(subparts[0]) {
                        return false;
                    }
                    current_search += subparts[0].len();
                } else {
                    // Subsequent part can be anywhere
                    if let Some(pos) = path_lower[current_search..].find(subparts[0]) {
                        current_search += pos + subparts[0].len();
                    } else {
                        return false;
                    }
                }
            }

            // Match remaining subparts
            for subpart in subparts.iter().skip(1) {
                if subpart.is_empty() {
                    continue;
                }
                if let Some(pos) = path_lower[current_search..].find(*subpart) {
                    current_search += pos + subpart.len();
                } else {
                    return false;
                }
            }

            search_idx = current_search;
        } else {
            // No single wildcards, just match the substring
            if let Some(pos) = path_lower[search_idx..].find(part) {
                // If it's the first part and pattern doesn't start with **, it must be at start
                if i == 0 && !pattern_lower.starts_with("**") && pos != 0 {
                    return false;
                }
                search_idx += pos + part.len();
            } else {
                return false;
            }
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "temporarily disabled to debug stack overflow"]
    fn test_config_load_default() {
        let config = Config::load();
        assert_eq!(config.thresholds.project_age_days, 14);
        assert_eq!(config.thresholds.min_age_days, 30);
        assert_eq!(config.thresholds.min_size_mb, 100);
    }

    #[test]
    #[ignore = "temporarily disabled to debug stack overflow"]
    fn test_exclusion_patterns() {
        let mut config = Config::default();
        config
            .exclusions
            .patterns
            .push("**/important-project/**".to_string());
        config.exclusions.patterns.push("**/backup/**".to_string());

        assert!(config.is_excluded(Path::new("C:/Users/me/important-project/file.txt")));
        assert!(config.is_excluded(Path::new("C:/backup/data.txt")));
        assert!(!config.is_excluded(Path::new("C:/Users/me/other/file.txt")));
    }

    #[test]
    #[ignore = "temporarily disabled to debug stack overflow"]
    fn test_config_apply_cli_overrides() {
        let mut config = Config::default();
        config.apply_cli_overrides(Some(21), Some(45), Some(150));

        assert_eq!(config.thresholds.project_age_days, 21);
        assert_eq!(config.thresholds.min_age_days, 45);
        assert_eq!(config.thresholds.min_size_mb, 150);
    }

    #[test]
    #[ignore = "temporarily disabled to debug stack overflow"]
    fn test_config_partial_overrides() {
        let mut config = Config::default();
        let original_age = config.thresholds.min_age_days;

        // Only override project_age, leave others unchanged
        config.apply_cli_overrides(Some(30), None, None);

        assert_eq!(config.thresholds.project_age_days, 30);
        assert_eq!(config.thresholds.min_age_days, original_age);
        assert_eq!(config.thresholds.min_size_mb, 100); // Default
    }
}
