use anyhow::Result;
use chrono::{DateTime, Utc};
use git2::Repository;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

// ============================================================================
// Git Root Cache
// ============================================================================
// 
// Finding git roots requires walking up the directory tree, which is O(depth).
// When scanning thousands of files, this becomes O(files × depth) = very slow.
// 
// Solution: Cache git root lookups. For any directory, we cache whether it has
// a git root and what that root is. This makes repeated lookups O(1).
// ============================================================================

lazy_static::lazy_static! {
    /// Cache of directory -> git root mappings
    /// None value means "we checked and there's no git root"
    static ref GIT_ROOT_CACHE: RwLock<HashMap<PathBuf, Option<PathBuf>>> = 
        RwLock::new(HashMap::with_capacity(1000));
}

/// Clear the git root cache
/// Call this before a new scan session to ensure fresh results
pub fn clear_cache() {
    if let Ok(mut cache) = GIT_ROOT_CACHE.write() {
        cache.clear();
    }
}

/// Find the git root directory with caching
/// 
/// This is the preferred method - uses cache for O(1) repeated lookups
pub fn find_git_root_cached(path: &Path) -> Option<PathBuf> {
    // Normalize to parent directory if path is a file
    let dir = if path.is_file() {
        path.parent().unwrap_or(path)
    } else {
        path
    };
    
    // Check cache first (read lock)
    {
        if let Ok(cache) = GIT_ROOT_CACHE.read() {
            if let Some(result) = cache.get(dir) {
                return result.clone();
            }
        }
    }
    
    // Not in cache, compute it
    let result = find_git_root(dir);
    
    // Store in cache (write lock)
    {
        if let Ok(mut cache) = GIT_ROOT_CACHE.write() {
            cache.insert(dir.to_path_buf(), result.clone());
        }
    }
    
    result
}

/// Find the git root directory by walking up from the given path
/// 
/// Prefer find_git_root_cached() for performance in scan loops
pub fn find_git_root(path: &Path) -> Option<PathBuf> {
    let mut current = path.to_path_buf();
    
    loop {
        let git_dir = current.join(".git");
        if git_dir.exists() {
            return Some(current);
        }
        
        if !current.pop() {
            break;
        }
    }
    
    None
}

/// Check if a git repository has uncommitted changes (dirty)
pub fn is_dirty(repo_path: &Path) -> Result<bool> {
    let repo = match Repository::open(repo_path) {
        Ok(repo) => repo,
        Err(_) => return Ok(false), // Not a git repo or can't open - not dirty
    };
    
    let mut status_options = git2::StatusOptions::new();
    status_options.include_ignored(false);
    status_options.include_untracked(true);
    
    let statuses = match repo.statuses(Some(&mut status_options)) {
        Ok(statuses) => statuses,
        Err(_) => return Ok(false), // Can't get status - assume not dirty
    };
    
    // If there are any status entries, the repo is dirty
    Ok(!statuses.is_empty())
}

/// Get the date of the last commit in a git repository
pub fn last_commit_date(repo_path: &Path) -> Result<Option<DateTime<Utc>>> {
    let repo = match Repository::open(repo_path) {
        Ok(repo) => repo,
        Err(_) => return Ok(None), // Not a git repo
    };
    
    let head = match repo.head() {
        Ok(head) => head,
        Err(_) => return Ok(None), // No HEAD
    };
    
    let commit = match head.peel_to_commit() {
        Ok(commit) => commit,
        Err(_) => return Ok(None), // Can't get commit
    };
    
    let time = commit.time();
    let datetime = DateTime::from_timestamp(time.seconds(), 0)
        .unwrap_or_else(|| Utc::now());
    
    Ok(Some(datetime))
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
    fn test_find_git_root_no_git() {
        let temp_dir = create_test_dir();
        assert_eq!(find_git_root(temp_dir.path()), None);
    }
    
    #[test]
    fn test_find_git_root_cached() {
        let temp_dir = create_test_dir();
        let git_dir = temp_dir.path().join(".git");
        fs::create_dir_all(&git_dir).unwrap();
        
        // First call should find it
        let result1 = find_git_root_cached(temp_dir.path());
        assert_eq!(result1, Some(temp_dir.path().to_path_buf()));
        
        // Second call should use cache
        let result2 = find_git_root_cached(temp_dir.path());
        assert_eq!(result2, Some(temp_dir.path().to_path_buf()));
    }
    
    #[test]
    fn test_clear_cache() {
        let temp_dir = create_test_dir();
        let git_dir = temp_dir.path().join(".git");
        fs::create_dir_all(&git_dir).unwrap();
        
        // Populate cache
        find_git_root_cached(temp_dir.path());
        
        // Clear cache
        clear_cache();
        
        // Cache should be empty (but we can't directly test that)
        // The function should still work after clearing
        let result = find_git_root_cached(temp_dir.path());
        assert_eq!(result, Some(temp_dir.path().to_path_buf()));
    }
    
    #[test]
    fn test_is_dirty_no_repo() {
        let temp_dir = create_test_dir();
        // No git repo, should return Ok(false)
        assert_eq!(is_dirty(temp_dir.path()).unwrap(), false);
    }
}
