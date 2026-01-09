use crate::output::CategoryResult;
use crate::utils;
use anyhow::{Context, Result};
use blake3::Hasher;
use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::{Read, BufReader};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Size of partial hash sample (first N bytes)
const PARTIAL_HASH_SIZE: usize = 4096; // 4KB

/// Maximum number of duplicate groups to return (prevents overwhelming output)
const MAX_GROUPS: usize = 50;

/// Duplicate file group
#[derive(Debug, Clone)]
pub struct DuplicateGroup {
    pub hash: String,
    pub size: u64,
    pub paths: Vec<PathBuf>,
}

/// Result for duplicate file detection
#[derive(Debug, Clone, Default)]
pub struct DuplicatesResult {
    pub groups: Vec<DuplicateGroup>,
    pub total_wasted: u64, // Size of all duplicates minus one copy each
}

impl DuplicatesResult {
    /// Convert to CategoryResult for compatibility with existing output system
    pub fn to_category_result(&self) -> CategoryResult {
        let mut paths = Vec::new();
        for group in &self.groups {
            // Add all paths except the first one (keep one copy)
            for path in group.paths.iter().skip(1) {
                paths.push(path.clone());
            }
        }
        
        CategoryResult {
            items: paths.len(),
            size_bytes: self.total_wasted,
            paths,
        }
    }
    
    /// Get total items count
    pub fn items(&self) -> usize {
        self.groups.iter().map(|g| g.paths.len()).sum()
    }
}

/// Scan for duplicate files in user directories
/// 
/// Uses a three-pass approach:
/// 1. Group files by size (files with unique sizes cannot be duplicates)
/// 2. For size groups > 1, compute partial hash (first 4KB)
/// 3. For partial hash matches, compute full hash
pub fn scan(root: &Path) -> Result<DuplicatesResult> {
    let mut result = DuplicatesResult::default();
    
    // Get user directories to scan
    let user_dirs = get_user_directories()?;
    
    // Step 1: Group files by size
    let mut size_groups: HashMap<u64, Vec<PathBuf>> = HashMap::new();
    
    for dir in user_dirs {
        if !dir.exists() {
            continue;
        }
        
        for entry in WalkDir::new(&dir)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| !should_skip_entry(e))
        {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            
            let path = entry.path();
            
            // Only process files
            if !entry.file_type().is_file() {
                continue;
            }
            
            // Skip hidden files
            if utils::is_hidden(path) {
                continue;
            }
            
            // Skip system paths
            if utils::is_system_path(path) {
                continue;
            }
            
            // Get file size
            if let Ok(metadata) = entry.metadata() {
                let size = metadata.len();
                if size > 0 {
                    size_groups.entry(size).or_insert_with(Vec::new).push(path.to_path_buf());
                }
            }
        }
    }
    
    // Step 2: For files with same size, compute partial hash
    let mut partial_hash_groups: HashMap<String, Vec<PathBuf>> = HashMap::new();
    
    for (size, paths) in size_groups {
        // Only check groups with more than one file
        if paths.len() < 2 {
            continue;
        }
        
        for path in paths {
            match compute_partial_hash(&path) {
                Ok(partial_hash) => {
                    partial_hash_groups
                        .entry(partial_hash)
                        .or_insert_with(Vec::new)
                        .push(path);
                }
                Err(_) => {
                    // Skip files we can't read
                    continue;
                }
            }
        }
    }
    
    // Step 3: For partial hash matches, compute full hash
    let mut full_hash_groups: HashMap<String, Vec<PathBuf>> = HashMap::new();
    
    for (partial_hash, paths) in partial_hash_groups {
        // Only check groups with more than one file
        if paths.len() < 2 {
            continue;
        }
        
        for path in paths {
            match compute_full_hash(&path) {
                Ok(full_hash) => {
                    full_hash_groups
                        .entry(full_hash)
                        .or_insert_with(Vec::new)
                        .push(path);
                }
                Err(_) => {
                    // Skip files we can't read
                    continue;
                }
            }
        }
    }
    
    // Build duplicate groups
    for (hash, paths) in full_hash_groups {
        // Only include groups with duplicates (2+ files)
        if paths.len() < 2 {
            continue;
        }
        
        // Get file size (all files in group have same size)
        let size = paths.first()
            .and_then(|p| std::fs::metadata(p).ok())
            .map(|m| m.len())
            .unwrap_or(0);
        
        // Calculate wasted space: (n-1) * size (keep one copy)
        let wasted = (paths.len() - 1) as u64 * size;
        
        result.groups.push(DuplicateGroup {
            hash,
            size,
            paths,
        });
        
        result.total_wasted += wasted;
    }
    
    // Sort groups by wasted space descending
    result.groups.sort_by(|a, b| {
        let wasted_a = (a.paths.len() - 1) as u64 * a.size;
        let wasted_b = (b.paths.len() - 1) as u64 * b.size;
        wasted_b.cmp(&wasted_a)
    });
    
    // Limit to top groups
    result.groups.truncate(MAX_GROUPS);
    
    Ok(result)
}

/// Get user directories to scan
fn get_user_directories() -> Result<Vec<PathBuf>> {
    let mut dirs = Vec::new();
    
    if let Ok(user_profile) = env::var("USERPROFILE") {
        let profile_path = PathBuf::from(&user_profile);
        dirs.push(profile_path.join("Downloads"));
        dirs.push(profile_path.join("Documents"));
        dirs.push(profile_path.join("Desktop"));
        dirs.push(profile_path.join("Pictures"));
        dirs.push(profile_path.join("Videos"));
        dirs.push(profile_path.join("Music"));
    }
    
    Ok(dirs)
}

/// Compute partial hash (first 4KB) of a file
fn compute_partial_hash(path: &Path) -> Result<String> {
    let file = File::open(path)
        .with_context(|| format!("Failed to open file: {}", path.display()))?;
    
    let mut reader = BufReader::new(file);
    let mut buffer = vec![0u8; PARTIAL_HASH_SIZE];
    
    let bytes_read = reader.read(&mut buffer)
        .with_context(|| format!("Failed to read file: {}", path.display()))?;
    
    buffer.truncate(bytes_read);
    
    let mut hasher = Hasher::new();
    hasher.update(&buffer);
    let hash = hasher.finalize();
    
    Ok(format!("{}", hash.to_hex()))
}

/// Compute full hash of a file
fn compute_full_hash(path: &Path) -> Result<String> {
    let file = File::open(path)
        .with_context(|| format!("Failed to open file: {}", path.display()))?;
    
    let mut reader = BufReader::new(file);
    let mut hasher = Hasher::new();
    let mut buffer = vec![0u8; 65536]; // 64KB buffer
    
    loop {
        let bytes_read = reader.read(&mut buffer)
            .with_context(|| format!("Failed to read file: {}", path.display()))?;
        
        if bytes_read == 0 {
            break;
        }
        
        hasher.update(&buffer[..bytes_read]);
    }
    
    let hash = hasher.finalize();
    Ok(format!("{}", hash.to_hex()))
}

/// Check if we should skip walking into this directory
fn should_skip_entry(entry: &walkdir::DirEntry) -> bool {
    if !entry.file_type().is_dir() {
        return false;
    }
    
    if let Some(name) = entry.file_name().to_str() {
        // Skip system directories
        if utils::is_system_path(entry.path()) {
            return true;
        }
        
        // Skip known build/cache directories
        return matches!(name.to_lowercase().as_str(),
            "node_modules" | ".git" | ".hg" | ".svn" |
            "target" | ".gradle" | "__pycache__" |
            ".venv" | "venv" | ".next" | ".nuxt" |
            "$recycle.bin" | "system volume information" |
            "appdata" | "programdata"
        );
    }
    
    false
}

/// Clean (delete) duplicate files by moving them to the Recycle Bin
/// Keeps the first file in each group, deletes the rest
pub fn clean(groups: &[DuplicateGroup], permanent: bool) -> Result<()> {
    for group in groups {
        // Keep the first file, delete the rest
        for path in group.paths.iter().skip(1) {
            if permanent {
                std::fs::remove_file(path)
                    .with_context(|| format!("Failed to permanently delete: {}", path.display()))?;
            } else {
                use trash;
                trash::delete(path)
                    .with_context(|| format!("Failed to delete: {}", path.display()))?;
            }
        }
    }
    Ok(())
}
