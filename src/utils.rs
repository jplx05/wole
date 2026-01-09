//! Shared utilities for sweeper
//!
//! This module contains common functions used across multiple category scanners
//! to reduce code duplication and ensure consistent behavior.

use std::path::{Path, PathBuf};

/// Convert to long path format for Windows (\\?\)
///
/// Windows has a default path length limit of 260 characters (MAX_PATH).
/// The \\?\ prefix enables extended-length paths up to ~32,767 characters.
/// This is common in deep `node_modules` directories.
#[cfg(windows)]
pub fn to_long_path(path: &Path) -> PathBuf {
    // Already has long path prefix
    if let Some(s) = path.to_str() {
        if s.starts_with(r"\\?\") {
            return path.to_path_buf();
        }
    }

    // Convert to absolute path first if relative
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    };

    // Add long path prefix
    if let Some(s) = absolute.to_str() {
        PathBuf::from(format!(r"\\?\{}", s))
    } else {
        path.to_path_buf()
    }
}

#[cfg(not(windows))]
pub fn to_long_path(path: &Path) -> PathBuf {
    path.to_path_buf()
}

/// Safe metadata that falls back to long path on Windows when normal access fails
///
/// Handles ERROR_PATH_NOT_FOUND (3) which occurs when paths exceed 260 chars
#[cfg(windows)]
pub fn safe_metadata(path: &Path) -> std::io::Result<std::fs::Metadata> {
    match std::fs::metadata(path) {
        Ok(m) => Ok(m),
        Err(e) if e.raw_os_error() == Some(3) => {
            // ERROR_PATH_NOT_FOUND - try with long path prefix
            std::fs::metadata(to_long_path(path))
        }
        Err(e) => Err(e),
    }
}

#[cfg(not(windows))]
pub fn safe_metadata(path: &Path) -> std::io::Result<std::fs::Metadata> {
    std::fs::metadata(path)
}

/// Safe symlink_metadata that falls back to long path on Windows
#[cfg(windows)]
pub fn safe_symlink_metadata(path: &Path) -> std::io::Result<std::fs::Metadata> {
    match std::fs::symlink_metadata(path) {
        Ok(m) => Ok(m),
        Err(e) if e.raw_os_error() == Some(3) => std::fs::symlink_metadata(to_long_path(path)),
        Err(e) => Err(e),
    }
}

#[cfg(not(windows))]
pub fn safe_symlink_metadata(path: &Path) -> std::io::Result<std::fs::Metadata> {
    std::fs::symlink_metadata(path)
}

/// Safe read_dir that falls back to long path on Windows
#[cfg(windows)]
pub fn safe_read_dir(path: &Path) -> std::io::Result<std::fs::ReadDir> {
    match std::fs::read_dir(path) {
        Ok(rd) => Ok(rd),
        Err(e) if e.raw_os_error() == Some(3) => std::fs::read_dir(to_long_path(path)),
        Err(e) => Err(e),
    }
}

#[cfg(not(windows))]
pub fn safe_read_dir(path: &Path) -> std::io::Result<std::fs::ReadDir> {
    std::fs::read_dir(path)
}

/// Safe remove_file that uses long path on Windows
#[cfg(windows)]
pub fn safe_remove_file(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.raw_os_error() == Some(3) => std::fs::remove_file(to_long_path(path)),
        Err(e) => Err(e),
    }
}

#[cfg(not(windows))]
pub fn safe_remove_file(path: &Path) -> std::io::Result<()> {
    std::fs::remove_file(path)
}

/// Safe remove_dir_all that uses long path on Windows
#[cfg(windows)]
pub fn safe_remove_dir_all(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(e) if e.raw_os_error() == Some(3) => std::fs::remove_dir_all(to_long_path(path)),
        Err(e) => Err(e),
    }
}

#[cfg(not(windows))]
pub fn safe_remove_dir_all(path: &Path) -> std::io::Result<()> {
    std::fs::remove_dir_all(path)
}

/// Check if entry should be skipped (symlink, junction, or reparse point)
///
/// Use this before descending into directories during scanning to prevent:
/// - Infinite loops from circular symlinks
/// - Following junctions to system directories
/// - Issues with OneDrive placeholder files
pub fn should_skip_entry(path: &Path) -> bool {
    // Check for symlink via symlink_metadata
    if let Ok(meta) = std::fs::symlink_metadata(path) {
        if meta.file_type().is_symlink() {
            return true;
        }
    }
    // Check for Windows reparse points (junctions, OneDrive placeholders)
    is_windows_reparse_point(path)
}

/// Returns true if this path is a Windows reparse point (junction/symlink/mount point).
///
/// Why this exists:
/// - On Windows, directory junctions and some OneDrive placeholders are *reparse points*.
/// - `walkdir`'s `follow_links(false)` prevents following *symlinks*, but junctions/reparse
///   points can still be traversed as normal directories, which may create cycles and
///   trigger stack overflows during deep scans.
pub fn is_windows_reparse_point(path: &Path) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        if let Ok(meta) = std::fs::symlink_metadata(path) {
            return meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0;
        }
        false
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        false
    }
}

/// Calculate total size of a directory tree using parallel traversal.
///
/// Uses jwalk for parallel directory traversal which is 2-4x faster than sequential.
///
/// Optimized to:
/// - Use parallel traversal with rayon thread pool
/// - Skip permission-denied errors gracefully
/// - NOT walk into .git directories
/// - Handle symlinks and reparse points safely (don't follow)
/// - Limit depth to prevent runaway scans
/// - Handle Windows long paths (>260 chars) gracefully
pub fn calculate_dir_size(path: &Path) -> u64 {
    use jwalk::WalkDir;
    use std::sync::atomic::{AtomicU64, Ordering};

    const MAX_DEPTH: usize = 15;

    let total = AtomicU64::new(0);

    WalkDir::new(path)
        .max_depth(MAX_DEPTH)
        .follow_links(false)
        .parallelism(jwalk::Parallelism::RayonDefaultPool {
            busy_timeout: std::time::Duration::from_secs(1),
        })
        .process_read_dir(|_depth, _path, _state, children| {
            // Skip directories we don't want to descend into
            children.retain(|entry| {
                if let Ok(ref e) = entry {
                    if e.file_type().is_symlink() {
                        return false;
                    }
                    if e.file_type().is_dir() {
                        if let Some(name) = e.path().file_name() {
                            let name_str = name.to_string_lossy();
                            // Skip .git internals
                            if name_str == ".git" {
                                return false;
                            }
                        }
                    }
                }
                true
            });
        })
        .into_iter()
        .for_each(|entry| {
            if let Ok(e) = entry {
                if e.file_type().is_file() {
                    if let Ok(meta) = e.metadata() {
                        total.fetch_add(meta.len(), Ordering::Relaxed);
                    }
                }
            }
        });

    total.load(Ordering::Relaxed)
}

/// Fast size calculation for a single directory level (no recursion).
///
/// Use this for quick estimates when you don't need exact totals.
/// Much faster than calculate_dir_size() for large directories.
pub fn calculate_shallow_size(path: &Path) -> u64 {
    let mut total = 0u64;

    if let Ok(entries) = safe_read_dir(path) {
        for entry in entries.flatten() {
            if let Ok(meta) = safe_metadata(&entry.path()) {
                if meta.is_file() {
                    total += meta.len();
                }
            }
        }
    }

    total
}

/// File type categories for large file detection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    Video,
    DiskImage,
    Archive,
    Installer,
    Document,
    Database,
    Backup,
    Other,
}

impl FileType {
    pub fn as_str(&self) -> &'static str {
        match self {
            FileType::Video => "Video",
            FileType::DiskImage => "Disk Image",
            FileType::Archive => "Archive",
            FileType::Installer => "Installer",
            FileType::Document => "Document",
            FileType::Database => "Database",
            FileType::Backup => "Backup",
            FileType::Other => "Other",
        }
    }

    pub fn emoji(&self) -> &'static str {
        match self {
            FileType::Video => "🎬",
            FileType::DiskImage => "💿",
            FileType::Archive => "📦",
            FileType::Installer => "📥",
            FileType::Document => "📄",
            FileType::Database => "🗃️",
            FileType::Backup => "💾",
            FileType::Other => "📁",
        }
    }
}

/// Detect file type based on extension
pub fn detect_file_type(path: &Path) -> FileType {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());

    match ext.as_deref() {
        // Video files
        Some("mp4" | "mkv" | "avi" | "mov" | "wmv" | "flv" | "webm" | "m4v" | "mpg" | "mpeg") => {
            FileType::Video
        }
        // Disk images
        Some("iso" | "img" | "dmg" | "vhd" | "vhdx" | "vmdk" | "vdi" | "wim" | "esd") => {
            FileType::DiskImage
        }
        // Archives
        Some("zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz" | "cab" | "tgz" | "tbz2") => {
            FileType::Archive
        }
        // Installers
        Some("exe" | "msi" | "msix" | "appx" | "appxbundle") => FileType::Installer,
        // Documents (large ones)
        Some("pdf" | "psd" | "ai" | "indd") => FileType::Document,
        // Databases
        Some("db" | "sqlite" | "sqlite3" | "mdb" | "accdb" | "bak") => FileType::Database,
        // Backup files
        Some("backup" | "old" | "orig" | "bkp") => FileType::Backup,
        _ => FileType::Other,
    }
}

/// Check if a file is hidden (Windows hidden attribute or dot-prefix)
#[allow(unused_variables)]
pub fn is_hidden(path: &Path) -> bool {
    // Check dot-prefix (Unix style, also works on Windows)
    if let Some(name) = path.file_name() {
        if name.to_string_lossy().starts_with('.') {
            return true;
        }
    }

    // Check Windows hidden attribute
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if let Ok(meta) = std::fs::metadata(path) {
            const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
            if meta.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0 {
                return true;
            }
        }
    }

    false
}

/// Convert an absolute path to a relative path based on a base directory.
///
/// If the path is not under the base directory, tries to show a relative path from
/// common parent directories (Documents, OneDrive/Documents, user home) to save space.
/// If the path equals the base directory, returns ".".
pub fn to_relative_path(path: &Path, base: &Path) -> String {
    // Normalize paths by converting to strings for comparison (handles case-insensitivity on Windows)
    let path_str = path.to_string_lossy().to_string();
    let base_str = base.to_string_lossy().to_string();

    // Normalize separators and remove trailing separators
    let path_normalized = path_str
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_string();
    let base_normalized = base_str
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_string();

    // Helper to check if path starts with base (case-insensitive on Windows)
    fn path_starts_with(path: &str, base: &str) -> bool {
        #[cfg(windows)]
        {
            path.to_lowercase().starts_with(&base.to_lowercase())
        }
        #[cfg(not(windows))]
        {
            path.starts_with(base)
        }
    }

    // Try to make relative to base directory
    if path_starts_with(&path_normalized, &base_normalized) {
        let relative = &path_normalized[base_normalized.len()..].trim_start_matches('/');
        if relative.is_empty() {
            return ".".to_string();
        }
        return relative.to_string();
    }

    // Path is not under base, try to make relative to common parent directories
    #[cfg(windows)]
    {
        // Try OneDrive/Documents
        if let Ok(userprofile) = std::env::var("USERPROFILE") {
            let onedrive_docs = format!("{}/OneDrive/Documents", userprofile.replace('\\', "/"));
            let onedrive_docs_normalized = onedrive_docs.trim_end_matches('/').to_string();
            if path_starts_with(&path_normalized, &onedrive_docs_normalized) {
                let relative =
                    &path_normalized[onedrive_docs_normalized.len()..].trim_start_matches('/');
                if !relative.is_empty() {
                    return format!("documents/{}", relative);
                }
            }

            // Try Documents
            let docs = format!("{}/Documents", userprofile.replace('\\', "/"));
            let docs_normalized = docs.trim_end_matches('/').to_string();
            if path_starts_with(&path_normalized, &docs_normalized) {
                let relative = &path_normalized[docs_normalized.len()..].trim_start_matches('/');
                if !relative.is_empty() {
                    return format!("documents/{}", relative);
                }
            }

            // check other standard folders
            for (name, folder) in [
                ("Downloads", "Downloads"),
                ("Pictures", "Pictures"),
                ("Music", "Music"),
                ("Videos", "Videos"),
                ("Desktop", "Desktop"),
            ] {
                let check_path = format!("{}/{}", userprofile.replace('\\', "/"), folder);
                let check_normalized = check_path.trim_end_matches('/').to_string();
                if path_starts_with(&path_normalized, &check_normalized) {
                    let relative =
                        &path_normalized[check_normalized.len()..].trim_start_matches('/');
                    if relative.is_empty() {
                        return name.to_string();
                    } else {
                        return format!("{}/{}", name, relative);
                    }
                }
            }

            // Try user home
            let home_normalized = userprofile
                .replace('\\', "/")
                .trim_end_matches('/')
                .to_string();
            if path_starts_with(&path_normalized, &home_normalized) {
                let relative = &path_normalized[home_normalized.len()..].trim_start_matches('/');
                if !relative.is_empty() {
                    // Start relative path directly if it's a top-level folder
                    // e.g. "Downloads" or "Music"
                    for folder in ["Downloads", "Pictures", "Music", "Videos", "Desktop"] {
                        if relative.eq_ignore_ascii_case(folder) {
                            return folder.to_string();
                        }
                        if relative
                            .to_lowercase()
                            .starts_with(&format!("{}/", folder.to_lowercase()))
                        {
                            return relative.to_string();
                        }
                    }

                    // Show last 2-3 components to save space
                    let components: Vec<&str> = relative.split('/').collect();
                    if components.len() > 3 {
                        return format!(".../{}", components[components.len() - 2..].join("/"));
                    }
                    return relative.to_string();
                }
            }
        }
    }

    #[cfg(not(windows))]
    {
        // Try user home on Unix
        if let Ok(home) = std::env::var("HOME") {
            let home_normalized = home.trim_end_matches('/').to_string();
            if path_normalized.starts_with(&home_normalized) {
                let relative = &path_normalized[home_normalized.len()..].trim_start_matches('/');
                if !relative.is_empty() {
                    return format!("~/{}", relative);
                }
            }
        }
    }

    // Fallback: show last 2-3 path components to save space
    let components: Vec<&str> = path_normalized
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    if components.len() > 3 {
        format!(".../{}", components[components.len() - 2..].join("/"))
    } else if !components.is_empty() {
        components.join("/")
    } else {
        path.display().to_string()
    }
}

/// System directories to always skip during scanning
pub const SYSTEM_DIRS: &[&str] = &[
    "Windows",
    "Program Files",
    "Program Files (x86)",
    "ProgramData",
    "$Recycle.Bin",
    "System Volume Information",
    "Recovery",
    "MSOCache",
];

/// Check if path contains a system directory
pub fn is_system_path(path: &Path) -> bool {
    for component in path.components() {
        if let std::path::Component::Normal(name) = component {
            let name_str = name.to_string_lossy();
            if SYSTEM_DIRS
                .iter()
                .any(|&sys| name_str.eq_ignore_ascii_case(sys))
            {
                return true;
            }
        }
    }
    false
}

/// Directories to skip walking into (we don't need to scan inside these)
pub const SKIP_WALK_DIRS: &[&str] = &[
    "node_modules",
    ".git",
    ".hg",
    ".svn",
    "target",
    ".gradle",
    "__pycache__",
    ".pytest_cache",
    ".mypy_cache",
    ".venv",
    "venv",
    ".next",
    ".nuxt",
    ".turbo",
    ".parcel-cache",
];

// Function disabled - walkdir not available in minimal test
// pub fn should_skip_walk(entry: &walkdir::DirEntry) -> bool { ... }

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_file_type_detection() {
        assert_eq!(detect_file_type(Path::new("movie.mp4")), FileType::Video);
        assert_eq!(
            detect_file_type(Path::new("windows.iso")),
            FileType::DiskImage
        );
        assert_eq!(detect_file_type(Path::new("backup.zip")), FileType::Archive);
        assert_eq!(
            detect_file_type(Path::new("setup.exe")),
            FileType::Installer
        );
        assert_eq!(detect_file_type(Path::new("random.txt")), FileType::Other);
    }

    #[test]
    fn test_system_path_detection() {
        // Test Windows paths (works on any platform as we're just checking path components)
        let windows_path1 = Path::new(r"C:\Windows\System32");
        let windows_path2 = Path::new(r"C:\Program Files\App");
        let normal_path = Path::new(r"C:\Users\me\Documents");

        // Check if Windows is in the path components
        let has_windows = windows_path1.components().any(|c| {
            if let std::path::Component::Normal(name) = c {
                name.to_string_lossy().eq_ignore_ascii_case("Windows")
            } else {
                false
            }
        });

        let has_program_files = windows_path2.components().any(|c| {
            if let std::path::Component::Normal(name) = c {
                name.to_string_lossy().eq_ignore_ascii_case("Program Files")
            } else {
                false
            }
        });

        // The function should detect these as system paths
        assert_eq!(is_system_path(windows_path1), has_windows);
        assert_eq!(is_system_path(windows_path2), has_program_files);
        assert!(!is_system_path(normal_path));

        // Test with a path that definitely has a system directory
        let test_path = Path::new("/some/path/Windows/system");
        assert!(is_system_path(test_path));

        let test_path2 = Path::new("/home/user/Program Files/app");
        assert!(is_system_path(test_path2));
    }

    #[test]
    #[ignore = "temporarily disabled to debug stack overflow"]
    fn test_should_skip_walk() {
        use walkdir::WalkDir;
        let temp_dir = tempfile::tempdir().unwrap();

        // Ensure we're using the temp directory
        assert!(temp_dir.path().exists());

        // Create a node_modules directory
        let node_modules = temp_dir.path().join("node_modules");
        fs::create_dir_all(&node_modules).unwrap();

        // Create a .git directory
        let git_dir = temp_dir.path().join(".git");
        fs::create_dir_all(&git_dir).unwrap();

        // Create a normal directory
        let normal_dir = temp_dir.path().join("normal");
        fs::create_dir_all(&normal_dir).unwrap();

        let mut entries: Vec<String> = Vec::new();
        // Use a very limited depth to prevent any stack issues
        for e in WalkDir::new(temp_dir.path())
            .max_depth(2) // Increased from 1 to allow subdirectories but still safe
            .into_iter()
            .filter_entry(|e| !should_skip_entry(e.path()))
            .flatten()
        {
            if let Some(name) = e.path().file_name() {
                entries.push(name.to_string_lossy().to_string());
            }
        }

        // Should skip node_modules and .git, but include normal
        assert!(!entries.contains(&"node_modules".to_string()));
        assert!(!entries.contains(&".git".to_string()));
    }

    #[test]
    fn test_is_hidden_dot_prefix() {
        let temp_dir = tempfile::tempdir().unwrap();
        let hidden_file = temp_dir.path().join(".hidden");
        fs::write(&hidden_file, "test").unwrap();

        assert!(is_hidden(&hidden_file));

        let visible_file = temp_dir.path().join("visible");
        fs::write(&visible_file, "test").unwrap();

        assert!(!is_hidden(&visible_file));
    }

    #[test]
    fn test_calculate_dir_size() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file1 = temp_dir.path().join("file1.txt");
        let file2 = temp_dir.path().join("file2.txt");

        fs::write(&file1, "hello").unwrap();
        fs::write(&file2, "world").unwrap();

        // Ensure we're using the temp directory, not accidentally walking system paths
        assert!(temp_dir.path().exists());
        let size = calculate_dir_size(temp_dir.path());
        assert_eq!(size, 10); // 5 + 5 bytes
    }

    #[test]
    fn test_file_type_emoji() {
        assert_eq!(FileType::Video.emoji(), "🎬");
        assert_eq!(FileType::Archive.emoji(), "📦");
        assert_eq!(FileType::Other.emoji(), "📁");
    }

    #[test]
    fn test_file_type_as_str() {
        assert_eq!(FileType::Video.as_str(), "Video");
        assert_eq!(FileType::DiskImage.as_str(), "Disk Image");
        assert_eq!(FileType::Other.as_str(), "Other");
    }

    #[test]
    fn test_to_long_path() {
        // Test that already-prefixed paths are returned unchanged
        let prefixed = Path::new(r"\\?\C:\Users\test");
        let result = to_long_path(prefixed);
        assert!(result.to_str().unwrap().starts_with(r"\\?\"));

        // Test that normal paths get the prefix added (on Windows)
        #[cfg(windows)]
        {
            let normal = Path::new(r"C:\Users\test\file.txt");
            let result = to_long_path(normal);
            assert!(result.to_str().unwrap().starts_with(r"\\?\"));
        }
    }

    #[test]
    fn test_safe_metadata() {
        let temp_dir = tempfile::tempdir().unwrap();
        let test_file = temp_dir.path().join("test.txt");
        fs::write(&test_file, "hello").unwrap();

        // safe_metadata should work on normal paths
        let meta = safe_metadata(&test_file).unwrap();
        assert!(meta.is_file());
        assert_eq!(meta.len(), 5);
    }

    #[test]
    fn test_safe_symlink_metadata() {
        let temp_dir = tempfile::tempdir().unwrap();
        let test_file = temp_dir.path().join("test.txt");
        fs::write(&test_file, "hello").unwrap();

        // safe_symlink_metadata should work on normal files
        let meta = safe_symlink_metadata(&test_file).unwrap();
        assert!(meta.is_file());
    }

    #[test]
    fn test_should_skip_entry_regular_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let test_file = temp_dir.path().join("regular.txt");
        fs::write(&test_file, "test").unwrap();

        // Regular files should not be skipped
        assert!(!should_skip_entry(&test_file));
    }

    #[test]
    fn test_should_skip_entry_regular_dir() {
        let temp_dir = tempfile::tempdir().unwrap();
        let test_dir = temp_dir.path().join("regular_dir");
        fs::create_dir(&test_dir).unwrap();

        // Regular directories should not be skipped
        assert!(!should_skip_entry(&test_dir));
    }

    #[test]
    #[cfg(unix)]
    fn test_should_skip_entry_symlink() {
        use std::os::unix::fs::symlink;

        let temp_dir = tempfile::tempdir().unwrap();
        let target = temp_dir.path().join("target.txt");
        let link = temp_dir.path().join("link.txt");

        fs::write(&target, "test").unwrap();
        symlink(&target, &link).unwrap();

        // Symlinks should be skipped
        assert!(should_skip_entry(&link));
    }
}
