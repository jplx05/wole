use crate::config::{Config, DuplicatesConfig};
use crate::output::CategoryResult;
use crate::scan_events::{ScanPathReporter, ScanProgressEvent};
use crate::utils;
use anyhow::{Context, Result};
use blake3::Hasher;
use jwalk::WalkDir;
use memmap2::MmapOptions;
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::sync::Arc;

/// Size of partial hash sample (first N bytes)
const PARTIAL_HASH_SIZE: usize = 4096; // 4KB

/// Maximum number of duplicate groups to return (prevents overwhelming output)
const MAX_GROUPS: usize = 50;

/// Extract the number from a filename suffix pattern like " (1)" or " (2)"
/// Returns u32::MAX if no number is found (to sort files without numbers first)
fn extract_suffix_number(filename: &str) -> u32 {
    // Look for pattern like " (1)" or " (123)" before the file extension
    if let Some(start) = filename.rfind(" (") {
        if let Some(end) = filename[start + 2..].find(')') {
            let num_str = &filename[start + 2..start + 2 + end];
            if let Ok(num) = num_str.parse::<u32>() {
                return num;
            }
        }
    }
    u32::MAX // Return max value so files without numbers sort first
}

/// Check if a filename appears to be a duplicate (has common duplicate patterns)
fn is_duplicate_filename(filename: &str) -> bool {
    // Check for patterns like " (1)", " - Copy", " - Copy (2)", etc.
    filename.contains(" (") && filename.contains(")")
        || filename.contains(" - Copy")
        || filename.contains(" - copy")
        || filename.contains("_copy")
        || filename.contains("_Copy")
}

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
    /// Only includes duplicate files, not the original (keeps files without duplicate patterns like "(1)")
    pub fn to_category_result(&self) -> CategoryResult {
        let mut paths = Vec::new();
        for group in &self.groups {
            // Separate files into those with duplicate patterns and those without
            let mut originals: Vec<&PathBuf> = Vec::new();
            let mut duplicates: Vec<&PathBuf> = Vec::new();

            for path in &group.paths {
                let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

                if is_duplicate_filename(filename) {
                    duplicates.push(path);
                } else {
                    originals.push(path);
                }
            }

            // Sort duplicates by suffix number (lower numbers first, to delete higher copies first)
            duplicates.sort_by(|a, b| {
                let a_name = a.file_name().and_then(|n| n.to_str()).unwrap_or("");
                let b_name = b.file_name().and_then(|n| n.to_str()).unwrap_or("");
                let a_num = extract_suffix_number(a_name);
                let b_num = extract_suffix_number(b_name);
                a_num.cmp(&b_num)
            });

            // If there are files with duplicate patterns, ONLY flag those as duplicates
            // (never flag files without patterns like "lto_meeting_isa.ogg")
            if !duplicates.is_empty() {
                // Add all files with duplicate patterns
                for path in &duplicates {
                    paths.push((*path).clone());
                }
            } else {
                // No files have duplicate patterns - fall back to keeping one and flagging the rest
                // Sort originals alphabetically and keep the first one
                originals.sort_by(|a, b| {
                    let a_name = a.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    let b_name = b.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    a_name.cmp(b_name)
                });

                // Add all but the first one
                for path in originals.iter().skip(1) {
                    paths.push((*path).clone());
                }
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

/// Scan for duplicate files
///
/// Uses a three-pass approach:
/// 1. Group files by size (files with unique sizes cannot be duplicates)
/// 2. For size groups > 1, compute partial hash (first 4KB)
/// 3. For partial hash matches, compute full hash
///
/// # Arguments
/// * `root` - Default scan path (used if config doesn't specify scan_paths)
///
/// Note: This function uses default config. For custom configuration, use `scan_with_config`.
pub fn scan(root: &Path) -> Result<DuplicatesResult> {
    let default_config = Config::default();
    scan_with_config(root, None, &default_config)
}

/// Scan for duplicate files with configuration
pub fn scan_with_config(
    root: &Path,
    config: Option<&DuplicatesConfig>,
    global_config: &Config,
) -> Result<DuplicatesResult> {
    scan_with_config_internal(root, config, global_config, None)
}

/// Scan for duplicate files with configuration + TUI progress updates (current file path).
pub fn scan_with_config_with_progress(
    root: &Path,
    config: Option<&DuplicatesConfig>,
    global_config: &Config,
    tx: &Sender<ScanProgressEvent>,
) -> Result<DuplicatesResult> {
    let reporter = Arc::new(ScanPathReporter::new("Duplicates", tx.clone(), 75));
    scan_with_config_internal(root, config, global_config, Some(reporter))
}

fn scan_with_config_internal(
    root: &Path,
    config: Option<&DuplicatesConfig>,
    global_config: &Config,
    reporter: Option<Arc<ScanPathReporter>>,
) -> Result<DuplicatesResult> {
    let mut result = DuplicatesResult::default();

    // Determine scan roots: use config paths if provided, otherwise use root argument
    let mut scan_roots: Vec<PathBuf> = if let Some(cfg) = config {
        if !cfg.scan_paths.is_empty() {
            // Use configured scan paths
            cfg.scan_paths
                .iter()
                .map(PathBuf::from)
                .collect::<Vec<PathBuf>>()
        } else {
            // Fall back to root argument
            vec![root.to_path_buf()]
        }
    } else {
        // No config provided, use root argument
        vec![root.to_path_buf()]
    };

    // Automatically add Downloads folder to scan roots if not already covered
    // This ensures we find duplicates in the most likely place even if scanning a different root
    if let Ok(profile) = std::env::var("USERPROFILE") {
        let downloads = PathBuf::from(profile).join("Downloads");
        if downloads.exists() {
            // Check if downloads is already covered by existing roots
            let already_covered = scan_roots.iter().any(|r| downloads.starts_with(r));
            if !already_covered {
                scan_roots.push(downloads);
            }
        }
    }

    // Get config values for performance optimization
    let memmap_threshold = config
        .map(|c| c.memmap_threshold_bytes)
        .unwrap_or(10 * 1024 * 1024); // Default 10MB
    let buffer_size = config
        .map(|c| c.buffer_size_bytes)
        .unwrap_or(8 * 1024 * 1024); // Default 8MB

    // Step 1: Group files by size (using parallel directory traversal)
    let size_groups: HashMap<u64, Vec<PathBuf>> = {
        use std::sync::Mutex;
        let groups: Mutex<HashMap<u64, Vec<PathBuf>>> = Mutex::new(HashMap::new());

        // Clone config for thread-safe access (jwalk requires 'static)
        let config_arc = Arc::new(global_config.clone());

        for dir in scan_roots {
            if !dir.exists() {
                continue;
            }

            // Use jwalk for parallel directory traversal (2-4x faster than walkdir)
            const MAX_DEPTH: usize = 20;
            let config_clone = Arc::clone(&config_arc);

            let reporter_for_walk = reporter.as_ref().map(Arc::clone);

            WalkDir::new(&dir)
                .max_depth(MAX_DEPTH)
                .follow_links(false) // CRITICAL: Prevents infinite loops on Windows junctions/reparse points
                .parallelism(jwalk::Parallelism::RayonDefaultPool {
                    busy_timeout: std::time::Duration::from_secs(1),
                })
                .process_read_dir(move |_depth, _path, _read_dir_state, children| {
                    // Filter out directories we don't want to descend into
                    children.retain(|entry| {
                        if let Ok(ref e) = entry {
                            let path = e.path();

                            // Skip symlinks and Windows reparse points (junctions, OneDrive placeholders)
                            // This prevents infinite loops on Windows systems with OneDrive folders
                            if utils::should_skip_entry(&path) {
                                return false;
                            }

                            if e.file_type().is_dir() {
                                // Skip system/build directories
                                if let Some(name) = path.file_name() {
                                    let name_lower = name.to_string_lossy().to_lowercase();
                                    if matches!(
                                        name_lower.as_str(),
                                        "node_modules"
                                            | ".git"
                                            | ".hg"
                                            | ".svn"
                                            | "target"
                                            | ".gradle"
                                            | "__pycache__"
                                            | ".venv"
                                            | "venv"
                                            | ".next"
                                            | ".nuxt"
                                            | "$recycle.bin"
                                            | "system volume information"
                                            | "appdata"
                                            | "programdata"
                                    ) {
                                        return false;
                                    }
                                }

                                // Check user exclusions
                                if config_clone.is_excluded(&path) {
                                    return false;
                                }
                            }
                        }
                        true
                    });
                })
                .into_iter()
                .filter_map(|e| e.ok())
                .for_each(|entry| {
                    let path = entry.path();
                    if let Some(ref reporter) = reporter_for_walk {
                        reporter.emit_path(&path);
                    }

                    // Only process files
                    if !entry.file_type().is_file() {
                        return;
                    }

                    // Skip hidden files
                    if utils::is_hidden(&path) {
                        return;
                    }

                    // Skip system paths
                    if utils::is_system_path(&path) {
                        return;
                    }

                    // Get file size from cached metadata
                    if let Ok(metadata) = entry.metadata() {
                        let size = metadata.len();
                        if size > 0 {
                            let mut groups = groups.lock().unwrap();
                            groups.entry(size).or_default().push(path);
                        }
                    }
                });
        }

        groups.into_inner().unwrap()
    };

    // Step 2: For files with same size, compute partial hash (PARALLELIZED)
    let mut partial_hash_groups: HashMap<String, Vec<PathBuf>> = HashMap::new();

    // Collect all paths that need partial hashing
    let paths_to_hash: Vec<(u64, Vec<PathBuf>)> = size_groups
        .into_iter()
        .filter(|(_, paths)| paths.len() >= 2)
        .collect();

    // Parallelize partial hash computation
    let reporter_for_partial = reporter.as_ref().map(Arc::clone);
    let partial_hash_results: Vec<(String, PathBuf)> = paths_to_hash
        .par_iter()
        .flat_map(|(_size, paths)| {
            paths
                .par_iter()
                .filter_map(|path| {
                    if let Some(ref reporter) = reporter_for_partial {
                        reporter.emit_path(path);
                    }
                    compute_partial_hash(path, buffer_size)
                        .ok()
                        .map(|hash| (hash, path.clone()))
                })
                .collect::<Vec<_>>()
        })
        .collect();

    // Group by partial hash
    for (partial_hash, path) in partial_hash_results {
        partial_hash_groups
            .entry(partial_hash)
            .or_default()
            .push(path);
    }

    // Step 3: For partial hash matches, compute full hash (PARALLELIZED)
    let mut full_hash_groups: HashMap<String, Vec<PathBuf>> = HashMap::new();

    // Collect paths that need full hashing
    let paths_for_full_hash: Vec<Vec<PathBuf>> = partial_hash_groups
        .into_iter()
        .filter(|(_, paths)| paths.len() >= 2)
        .map(|(_, paths)| paths)
        .collect();

    // Parallelize full hash computation
    let memmap_threshold_clone = memmap_threshold;
    let buffer_size_clone = buffer_size;
    let reporter_for_full = reporter.as_ref().map(Arc::clone);
    let full_hash_results: Vec<(String, PathBuf)> = paths_for_full_hash
        .par_iter()
        .flat_map(|paths| {
            paths
                .par_iter()
                .filter_map(|path| {
                    if let Some(ref reporter) = reporter_for_full {
                        reporter.emit_path(path);
                    }
                    compute_full_hash(path, memmap_threshold_clone, buffer_size_clone)
                        .ok()
                        .map(|hash| (hash, path.clone()))
                })
                .collect::<Vec<_>>()
        })
        .collect();

    // Group by full hash
    for (full_hash, path) in full_hash_results {
        full_hash_groups.entry(full_hash).or_default().push(path);
    }

    // Build duplicate groups
    for (hash, paths) in full_hash_groups {
        // Only include groups with duplicates (2+ files)
        if paths.len() < 2 {
            continue;
        }

        // Get file size (all files in group have same size)
        let size = paths
            .first()
            .and_then(|p| std::fs::metadata(p).ok())
            .map(|m| m.len())
            .unwrap_or(0);

        // Calculate wasted space: (n-1) * size (keep one copy)
        let wasted = (paths.len() - 1) as u64 * size;

        result.groups.push(DuplicateGroup { hash, size, paths });

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

/// Compute partial hash (first 4KB) of a file
fn compute_partial_hash(path: &Path, _buffer_size: usize) -> Result<String> {
    let file =
        File::open(path).with_context(|| format!("Failed to open file: {}", path.display()))?;

    let mut reader = BufReader::new(file);
    let mut buffer = vec![0u8; PARTIAL_HASH_SIZE];

    let bytes_read = reader
        .read(&mut buffer)
        .with_context(|| format!("Failed to read file: {}", path.display()))?;

    buffer.truncate(bytes_read);

    let mut hasher = Hasher::new();
    hasher.update(&buffer);
    let hash = hasher.finalize();

    Ok(format!("{}", hash.to_hex()))
}

/// Compute full hash of a file
///
/// Uses memory mapping for large files (faster on NVMe SSDs) and buffered reads for smaller files
fn compute_full_hash(path: &Path, memmap_threshold: u64, buffer_size: usize) -> Result<String> {
    // Get file size to decide on read strategy
    let metadata = std::fs::metadata(path)
        .with_context(|| format!("Failed to get metadata: {}", path.display()))?;
    let file_size = metadata.len();

    // Use memory mapping for large files (typically faster on modern SSDs)
    if file_size >= memmap_threshold && file_size > 0 {
        return compute_full_hash_memmap(path, file_size);
    }

    // Use buffered reads for smaller files (memory mapping overhead not worth it)
    let file =
        File::open(path).with_context(|| format!("Failed to open file: {}", path.display()))?;

    let mut reader = BufReader::with_capacity(buffer_size, file);
    let mut hasher = Hasher::new();
    let mut buffer = vec![0u8; buffer_size];

    loop {
        let bytes_read = reader
            .read(&mut buffer)
            .with_context(|| format!("Failed to read file: {}", path.display()))?;

        if bytes_read == 0 {
            break;
        }

        hasher.update(&buffer[..bytes_read]);
    }

    let hash = hasher.finalize();
    Ok(format!("{}", hash.to_hex()))
}

/// Compute full hash using memory mapping (faster for large files)
fn compute_full_hash_memmap(path: &Path, _file_size: u64) -> Result<String> {
    let file =
        File::open(path).with_context(|| format!("Failed to open file: {}", path.display()))?;

    // Safety: We're only reading the file, not modifying it
    // The file is opened read-only and we're computing a hash
    let mmap = unsafe {
        MmapOptions::new()
            .map(&file)
            .with_context(|| format!("Failed to memory map file: {}", path.display()))?
    };

    let mut hasher = Hasher::new();
    hasher.update(&mmap[..]);
    let hash = hasher.finalize();

    Ok(format!("{}", hash.to_hex()))
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
                crate::trash_ops::delete(path)
                    .with_context(|| format!("Failed to delete: {}", path.display()))?;
            }
        }
    }
    Ok(())
}
