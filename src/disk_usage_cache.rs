//! Disk insights cache - simple file-based caching for folder tree structures

use crate::disk_usage::DiskInsights;
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Get cache directory for disk insights
fn get_cache_dir() -> Result<PathBuf> {
    let base_dir = if cfg!(windows) {
        std::env::var("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                std::env::var("USERPROFILE")
                    .map(|p| PathBuf::from(p).join("AppData").join("Local"))
                    .unwrap_or_else(|_| PathBuf::from("."))
            })
    } else {
        std::env::var("HOME")
            .map(|h| PathBuf::from(h).join(".local").join("share"))
            .unwrap_or_else(|_| PathBuf::from("."))
    };

    let cache_dir = base_dir.join("wole").join("cache").join("disk_insights");
    std::fs::create_dir_all(&cache_dir)
        .with_context(|| format!("Failed to create cache directory: {}", cache_dir.display()))?;
    Ok(cache_dir)
}

/// Normalize path for cache key generation
/// On Windows, converts to lowercase and replaces \ with _
/// On Unix, replaces / with _
fn normalize_path_for_cache(path: &Path) -> String {
    let path_str = path.to_string_lossy();
    #[cfg(windows)]
    {
        path_str
            .to_lowercase()
            .replace('\\', "_")
            .replace(':', "")
            .replace(' ', "_")
    }
    #[cfg(not(windows))]
    {
        path_str.replace('/', "_").replace(' ', "_")
    }
}

/// Generate cache key hash from path, depth, and root directory mtime
pub fn get_cache_key(path: &Path, depth: u8) -> Result<(String, u64)> {
    // Get root directory mtime for cache invalidation
    let metadata = fs::metadata(path)
        .with_context(|| format!("Failed to get metadata for: {}", path.display()))?;
    
    let mtime = metadata
        .modified()
        .or_else(|_| metadata.created())
        .unwrap_or_else(|_| SystemTime::UNIX_EPOCH);
    
    let mtime_secs = mtime
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    
    let normalized_path = normalize_path_for_cache(path);
    let key = format!("{}_{}_{}", normalized_path, depth, mtime_secs);
    
    // Use a hash of the key for filename (to avoid filesystem issues with long paths)
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    let hash = format!("{:x}", hasher.finish());
    
    Ok((hash, mtime_secs))
}

/// Load cached disk insights if available and valid
pub fn load_cached_insights(path: &Path, depth: u8) -> Result<Option<DiskInsights>> {
    // Get current mtime first to generate cache key
    let (cache_key_hash, _) = get_cache_key(path, depth)?;
    
    let cache_dir = get_cache_dir()?;
    let cache_file = cache_dir.join(format!("{}.json", cache_key_hash));
    
    if !cache_file.exists() {
        return Ok(None);
    }
    
    // Read cache file
    let cache_data = fs::read_to_string(&cache_file)
        .with_context(|| format!("Failed to read cache file: {}", cache_file.display()))?;
    
    // Parse JSON
    let insights: DiskInsights = serde_json::from_str(&cache_data)
        .with_context(|| format!("Failed to parse cache file: {}", cache_file.display()))?;
    
    // Verify the cached path matches (safety check)
    if insights.root.path != path {
        // Path mismatch, invalidate cache
        let _ = fs::remove_file(&cache_file);
        return Ok(None);
    }
    
    // Verify mtime hasn't changed by regenerating key with current mtime
    // If the hash matches, mtime is the same (since hash includes mtime)
    let (current_key_hash, _) = get_cache_key(path, depth)?;
    if current_key_hash != cache_key_hash {
        // Mtime changed, cache is invalid
        let _ = fs::remove_file(&cache_file);
        return Ok(None);
    }
    
    Ok(Some(insights))
}

/// Save disk insights to cache
pub fn save_cached_insights(path: &Path, depth: u8, insights: &DiskInsights) -> Result<()> {
    let (cache_key_hash, _) = get_cache_key(path, depth)?;
    let cache_dir = get_cache_dir()?;
    let cache_file = cache_dir.join(format!("{}.json", cache_key_hash));
    
    // Serialize to JSON
    let json_data = serde_json::to_string_pretty(insights)
        .context("Failed to serialize disk insights to JSON")?;
    
    // Write to cache file atomically (write to temp file, then rename)
    let temp_file = cache_file.with_extension("tmp");
    fs::write(&temp_file, json_data)
        .with_context(|| format!("Failed to write cache file: {}", temp_file.display()))?;
    
    fs::rename(&temp_file, &cache_file)
        .with_context(|| format!("Failed to rename cache file: {}", cache_file.display()))?;
    
    Ok(())
}

/// Invalidate cache for a specific path (optional cleanup)
pub fn invalidate_cache(path: &Path) -> Result<()> {
    let cache_dir = get_cache_dir()?;
    
    // Find all cache files that match this path (by checking normalized path prefix)
    let normalized_path = normalize_path_for_cache(path);
    
    if cache_dir.exists() {
        for entry in fs::read_dir(&cache_dir)? {
            let entry = entry?;
            let file_path = entry.path();
            
            // Try to read and check if it matches
            if let Ok(cache_data) = fs::read_to_string(&file_path) {
                if let Ok(insights) = serde_json::from_str::<DiskInsights>(&cache_data) {
                    let cached_normalized = normalize_path_for_cache(&insights.root.path);
                    if cached_normalized.starts_with(&normalized_path) {
                        let _ = fs::remove_file(&file_path);
                    }
                }
            }
        }
    }
    
    Ok(())
}
