use crate::output::CategoryResult;
use crate::project;
use crate::utils;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Build artifact directories to detect
const BUILD_ARTIFACTS: &[&str] = &[
    "node_modules",
    "target",
    "bin",
    "obj",
    "dist",
    "build",
    ".next",
    ".nuxt",
    ".output",
    "__pycache__",
    ".pytest_cache",
    ".mypy_cache",
    ".venv",
    "venv",
    ".gradle",
    ".parcel-cache",
    ".turbo",
    ".angular",
    ".svelte-kit",
    "coverage",
    ".nyc_output",
];

/// Scan for build artifacts in inactive projects
/// 
/// Uses shared calculate_dir_size for consistent size calculation.
/// Sorts results by size for better UX.
pub fn scan(root: &Path, project_age_days: u64) -> Result<CategoryResult> {
    let mut result = CategoryResult::default();
    
    // Find all project roots
    let project_roots = project::find_project_roots(root);
    
    // Collect artifacts with sizes for sorting
    let mut artifacts_with_sizes: Vec<(PathBuf, u64)> = Vec::new();
    
    // Filter to inactive projects only
    for project_root in project_roots {
        match project::is_project_active(&project_root, project_age_days) {
            Ok(false) => {
                // Project is inactive - find its build artifacts
                let artifacts = find_build_artifacts(&project_root);
                for artifact_path in artifacts {
                    if artifact_path.exists() {
                        let size = utils::calculate_dir_size(&artifact_path);
                        if size > 0 {
                            artifacts_with_sizes.push((artifact_path, size));
                        }
                    }
                }
            }
            Ok(true) => {
                // Project is active - skip it
            }
            Err(_) => {
                // Error checking activity - skip silently
            }
        }
    }
    
    // Sort by size descending (biggest first)
    artifacts_with_sizes.sort_by(|a, b| b.1.cmp(&a.1));
    
    // Build result
    for (path, size) in artifacts_with_sizes {
        result.items += 1;
        result.size_bytes += size;
        result.paths.push(path);
    }
    
    Ok(result)
}

/// Find build artifact directories in a project
fn find_build_artifacts(project_path: &Path) -> Vec<PathBuf> {
    let mut artifacts = Vec::new();
    
    for artifact_name in BUILD_ARTIFACTS {
        let artifact_path = project_path.join(artifact_name);
        if artifact_path.exists() && artifact_path.is_dir() {
            artifacts.push(artifact_path);
        }
    }
    
    artifacts
}

/// Clean (delete) a build artifact directory by moving it to the Recycle Bin
pub fn clean(path: &Path) -> Result<()> {
    trash::delete(path)
        .with_context(|| format!("Failed to delete build artifact: {}", path.display()))?;
    Ok(())
}
