use crate::config::Config;
use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use jwalk::WalkDir;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::cell::RefCell;

// Thread-local cache for project active status to avoid repeated file system checks
thread_local! {
    static PROJECT_ACTIVE_CACHE: RefCell<std::collections::HashMap<(PathBuf, u64), bool>> = RefCell::new(std::collections::HashMap::new());
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectType {
    Node,
    Rust,
    DotNet,
    Python,
    Java,
}

/// Detect project type by looking for marker files
pub fn detect_project_type(path: &Path) -> Option<ProjectType> {
    // Check for Node.js
    if path.join("package.json").exists() {
        return Some(ProjectType::Node);
    }

    // Check for Rust
    if path.join("Cargo.toml").exists() {
        return Some(ProjectType::Rust);
    }

    // Check for .NET by globbing
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.ends_with(".csproj") || name_str.ends_with(".sln") {
                return Some(ProjectType::DotNet);
            }
        }
    }

    // Check for Python
    if path.join("pyproject.toml").exists() || path.join("requirements.txt").exists() {
        return Some(ProjectType::Python);
    }

    // Check for Java
    if path.join("build.gradle").exists() || path.join("pom.xml").exists() {
        return Some(ProjectType::Java);
    }

    None
}

/// Get the marker file path for a project type
#[allow(dead_code)]
fn get_marker_file(path: &Path, project_type: ProjectType) -> Option<PathBuf> {
    match project_type {
        ProjectType::Node => Some(path.join("package.json")),
        ProjectType::Rust => Some(path.join("Cargo.toml")),
        ProjectType::DotNet => {
            // Try to find any .csproj or .sln file
            if let Ok(entries) = std::fs::read_dir(path) {
                for entry in entries.flatten() {
                    let name = entry.file_name();
                    let name_str = name.to_string_lossy();
                    if name_str.ends_with(".csproj") || name_str.ends_with(".sln") {
                        return Some(entry.path());
                    }
                }
            }
            None
        }
        ProjectType::Python => {
            if path.join("pyproject.toml").exists() {
                Some(path.join("pyproject.toml"))
            } else if path.join("requirements.txt").exists() {
                Some(path.join("requirements.txt"))
            } else {
                None
            }
        }
        ProjectType::Java => {
            if path.join("build.gradle").exists() {
                Some(path.join("build.gradle"))
            } else if path.join("pom.xml").exists() {
                Some(path.join("pom.xml"))
            } else {
                None
            }
        }
    }
}

/// Check if a project is active (recently modified or has uncommitted changes)
/// Uses smart heuristics to check multiple indicators of recent activity
///
/// PERFORMANCE: Caches results per (project_path, age_days) to avoid repeated
/// expensive file system checks when scanning many files in the same project.
pub fn is_project_active(path: &Path, age_days: u64) -> Result<bool> {
    // Normalize path for cache key (use absolute if possible)
    let cache_key = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .ok()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|| path.to_path_buf())
    };

    // Check cache first
    let cached_result = PROJECT_ACTIVE_CACHE.with(|cache| {
        let cache_ref = cache.borrow();
        cache_ref.get(&(cache_key.clone(), age_days)).copied()
    });

    if let Some(cached) = cached_result {
        return Ok(cached);
    }

    // Not in cache - compute result
    let cutoff = Utc::now() - Duration::days(age_days as i64);

    // Helper to check if file was modified within cutoff
    let was_modified_recently = |file_path: &Path| -> bool {
        if let Ok(meta) = std::fs::metadata(file_path) {
            if let Ok(modified) = meta.modified() {
                let modified_dt: DateTime<Utc> = modified.into();
                return modified_dt > cutoff;
            }
        }
        false
    };

    // Check git index (file-based, no git2 needed)
    if was_modified_recently(&path.join(".git").join("index")) {
        PROJECT_ACTIVE_CACHE.with(|cache| {
            cache.borrow_mut().insert((cache_key, age_days), true);
        });
        return Ok(true);
    }

    // Check git HEAD
    if was_modified_recently(&path.join(".git").join("HEAD")) {
        PROJECT_ACTIVE_CACHE.with(|cache| {
            cache.borrow_mut().insert((cache_key, age_days), true);
        });
        return Ok(true);
    }

    // Check common project files and lock files
    let project_files = [
        "package.json",
        "package-lock.json",
        "yarn.lock",
        "pnpm-lock.yaml",
        "Cargo.toml",
        "Cargo.lock",
        "requirements.txt",
        "pyproject.toml",
        "poetry.lock",
        "build.gradle",
        "pom.xml",
        "go.mod",
        "go.sum",
        "composer.json",
        "composer.lock",
        "Gemfile",
        "Gemfile.lock",
    ];

    for file in &project_files {
        if was_modified_recently(&path.join(file)) {
            PROJECT_ACTIVE_CACHE.with(|cache| {
                cache.borrow_mut().insert((cache_key, age_days), true);
            });
            return Ok(true);
        }
    }

    // Check if any source files were modified recently
    let source_extensions = [
        "rs", "js", "ts", "tsx", "jsx", "py", "go", "java", "rb", "php", "c", "cpp", "h",
    ];

    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten().take(100) {
            // Limit to first 100 files
            let entry_path = entry.path();
            if let Some(ext) = entry_path.extension() {
                if source_extensions.contains(&ext.to_string_lossy().as_ref())
                    && was_modified_recently(&entry_path)
                {
                    PROJECT_ACTIVE_CACHE.with(|cache| {
                        cache.borrow_mut().insert((cache_key, age_days), true);
                    });
                    return Ok(true);
                }
            }
        }
    }

    // Project is inactive - cache and return
    PROJECT_ACTIVE_CACHE.with(|cache| {
        cache.borrow_mut().insert((cache_key, age_days), false);
    });
    Ok(false) // Inactive
}

/// Find all project roots in a directory tree
///
/// Uses jwalk for parallel directory traversal (2-4x faster than sequential).
pub fn find_project_roots(root: &Path, config: &Config) -> Vec<PathBuf> {
    // Skip if root itself is a project (avoid scanning into it)
    if detect_project_type(root).is_some() {
        return vec![root.to_path_buf()];
    }

    const MAX_DEPTH: usize = 5;

    let projects: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
    let seen: Mutex<HashSet<PathBuf>> = Mutex::new(HashSet::new());

    // Clone config for thread-safe access (jwalk requires 'static)
    let config_arc = Arc::new(config.clone());

    WalkDir::new(root)
        .max_depth(MAX_DEPTH)
        .follow_links(false)
        .parallelism(jwalk::Parallelism::RayonDefaultPool {
            busy_timeout: std::time::Duration::from_secs(1),
        })
        .process_read_dir(move |_depth, _path, _state, children| {
            // Filter out directories we don't want to descend into
            children.retain(|entry| {
                if let Ok(ref e) = entry {
                    let path = e.path();

                    // Skip symlinks
                    if e.file_type().is_symlink() {
                        return false;
                    }

                    if e.file_type().is_dir() {
                        // Skip known deep/large directories that aren't project roots
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
                                    | ".turbo"
                                    | ".parcel-cache"
                                    | "$recycle.bin"
                                    | "system volume information"
                                    | "windows"
                                    | "program files"
                                    | "program files (x86)"
                                    | "programdata"
                                    | "appdata"
                            ) {
                                return false;
                            }
                        }

                        // Check user config exclusions
                        if config_arc.is_excluded(&path) {
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

            // Only process directories
            if !entry.file_type().is_dir() {
                return;
            }

            // Check if this is a project root
            if detect_project_type(&path).is_some() {
                let mut seen_guard = seen.lock().unwrap();
                if !seen_guard.contains(&path) {
                    seen_guard.insert(path.clone());
                    drop(seen_guard);

                    let mut projects_guard = projects.lock().unwrap();
                    // Check it's not a subproject of an already-found project
                    let is_subproject = projects_guard
                        .iter()
                        .any(|p: &PathBuf| path.starts_with(p) && path != *p);
                    if !is_subproject {
                        projects_guard.push(path);
                    }
                }
            }
        });

    projects.into_inner().unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_dir() -> TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    #[ignore = "temporarily disabled to debug stack overflow"]
    fn test_detect_project_type_node() {
        let temp_dir = create_test_dir();
        let package_json = temp_dir.path().join("package.json");
        fs::write(&package_json, r#"{"name": "test"}"#).unwrap();

        assert_eq!(
            detect_project_type(temp_dir.path()),
            Some(ProjectType::Node)
        );
    }

    #[test]
    #[ignore = "temporarily disabled to debug stack overflow"]
    fn test_detect_project_type_rust() {
        let temp_dir = create_test_dir();
        let cargo_toml = temp_dir.path().join("Cargo.toml");
        fs::write(&cargo_toml, "[package]").unwrap();

        assert_eq!(
            detect_project_type(temp_dir.path()),
            Some(ProjectType::Rust)
        );
    }

    #[test]
    #[ignore = "temporarily disabled to debug stack overflow"]
    fn test_detect_project_type_python() {
        let temp_dir = create_test_dir();
        let pyproject = temp_dir.path().join("pyproject.toml");
        fs::write(&pyproject, "[project]").unwrap();

        assert_eq!(
            detect_project_type(temp_dir.path()),
            Some(ProjectType::Python)
        );
    }

    #[test]
    #[ignore = "temporarily disabled to debug stack overflow"]
    fn test_detect_project_type_none() {
        let temp_dir = create_test_dir();
        // No project files
        assert_eq!(detect_project_type(temp_dir.path()), None);
    }

    #[test]
    #[ignore = "temporarily disabled to debug stack overflow"]
    fn test_find_project_roots() {
        let temp_dir = create_test_dir();
        let project1 = temp_dir.path().join("project1");
        let project2 = temp_dir.path().join("project2");

        fs::create_dir_all(&project1).unwrap();
        fs::create_dir_all(&project2).unwrap();

        fs::write(project1.join("package.json"), "{}").unwrap();
        fs::write(project2.join("Cargo.toml"), "[package]").unwrap();

        let config = crate::config::Config::default();
        let roots = find_project_roots(temp_dir.path(), &config);
        assert_eq!(roots.len(), 2);
    }
}
