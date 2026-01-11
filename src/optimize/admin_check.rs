//! Administrator privilege detection feature.

use std::env;
use std::fs;
use std::path::PathBuf;

/// Check if the current process is running with administrator privileges
pub fn is_admin() -> bool {
    // Try to access a protected path that requires admin
    // This is a simple heuristic - not 100% accurate but good enough
    let system_root = env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
    let test_path = PathBuf::from(&system_root).join("System32\\config\\system");

    // Try to open the file - if we can, we likely have admin rights
    fs::metadata(&test_path).is_ok()
}
