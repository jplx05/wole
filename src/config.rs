use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub thresholds: Thresholds,
    
    #[serde(default)]
    pub paths: Paths,
    
    #[serde(default)]
    pub exclusions: Exclusions,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Paths {
    #[serde(default)]
    pub scan_roots: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Exclusions {
    #[serde(default)]
    pub patterns: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            thresholds: Thresholds::default(),
            paths: Paths::default(),
            exclusions: Exclusions::default(),
        }
    }
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

impl Default for Paths {
    fn default() -> Self {
        Self {
            scan_roots: Vec::new(),
        }
    }
}

impl Default for Exclusions {
    fn default() -> Self {
        Self {
            patterns: Vec::new(),
        }
    }
}

fn default_project_age() -> u64 { 14 }
fn default_min_age() -> u64 { 30 }
fn default_min_size_mb() -> u64 { 100 }

impl Config {
    /// Get the config file path: %APPDATA%\sweeper\config.toml
    pub fn config_path() -> Result<PathBuf> {
        let appdata = std::env::var("APPDATA")
            .context("APPDATA environment variable not set")?;
        let config_dir = PathBuf::from(appdata).join("sweeper");
        Ok(config_dir.join("config.toml"))
    }
    
    /// Load config from file or return defaults
    pub fn load() -> Self {
        match Self::config_path() {
            Ok(path) if path.exists() => {
                match fs::read_to_string(&path) {
                    Ok(content) => {
                        match toml::from_str(&content) {
                            Ok(config) => config,
                            Err(e) => {
                                eprintln!("Warning: Failed to parse config file: {}", e);
                                Self::default()
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Warning: Failed to read config file: {}", e);
                        Self::default()
                    }
                }
            }
            _ => Self::default(),
        }
    }
    
    /// Save config to file
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        
        // Create directory if it doesn't exist
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .context("Failed to create config directory")?;
        }
        
        let toml = toml::to_string_pretty(self)
            .context("Failed to serialize config")?;
        
        fs::write(&path, toml)
            .context("Failed to write config file")?;
        
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
    pub fn is_excluded(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();
        for pattern in &self.exclusions.patterns {
            // Simple glob matching: ** matches any path segment
            if matches_pattern(&path_str, pattern) {
                return true;
            }
        }
        false
    }
}

/// Simple glob pattern matching
/// Supports ** for recursive matching
fn matches_pattern(path: &str, pattern: &str) -> bool {
    // Convert pattern to regex-like matching
    let pattern = pattern.replace("**", ".*");
    
    // Simple case-insensitive matching
    let path_lower = path.to_lowercase();
    let pattern_lower = pattern.to_lowercase();
    
    // Check if pattern matches (simplified - could use proper glob crate)
    if pattern_lower.contains(".*") {
        // Wildcard pattern - split on .* to get literal parts
        // Remove any anchors before splitting to avoid treating them as literals
        let pattern_to_split = pattern_lower.trim_start_matches('^').trim_end_matches('$');
        let parts: Vec<&str> = pattern_to_split.split(".*").collect();
        let mut search_path = path_lower.as_str();
        for part in parts {
            if part.is_empty() {
                continue;
            }
            if let Some(pos) = search_path.find(part) {
                search_path = &search_path[pos + part.len()..];
            } else {
                return false;
            }
        }
        true
    } else {
        // No wildcards - simple substring match
        path_lower.contains(&pattern_lower)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_config_load_default() {
        let config = Config::load();
        assert_eq!(config.thresholds.project_age_days, 14);
        assert_eq!(config.thresholds.min_age_days, 30);
        assert_eq!(config.thresholds.min_size_mb, 100);
    }
    
    #[test]
    fn test_exclusion_patterns() {
        let mut config = Config::default();
        config.exclusions.patterns.push("**/important-project/**".to_string());
        config.exclusions.patterns.push("**/backup/**".to_string());
        
        assert!(config.is_excluded(Path::new("C:/Users/me/important-project/file.txt")));
        assert!(config.is_excluded(Path::new("C:/backup/data.txt")));
        assert!(!config.is_excluded(Path::new("C:/Users/me/other/file.txt")));
    }
    
    #[test]
    fn test_config_apply_cli_overrides() {
        let mut config = Config::default();
        config.apply_cli_overrides(Some(21), Some(45), Some(150));
        
        assert_eq!(config.thresholds.project_age_days, 21);
        assert_eq!(config.thresholds.min_age_days, 45);
        assert_eq!(config.thresholds.min_size_mb, 150);
    }
    
    #[test]
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