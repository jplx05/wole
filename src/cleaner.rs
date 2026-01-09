use crate::categories;
use crate::output::{OutputMode, ScanResults};
use crate::progress;
use anyhow::{Context, Result};
use colored::*;
use std::io::{self, Write};
use std::path::Path;

/// Clean all categories based on scan results
/// 
/// Handles confirmation prompts, error tracking, and provides progress feedback
pub fn clean_all(
    results: &ScanResults, 
    skip_confirm: bool, 
    mode: OutputMode,
    permanent: bool,
    dry_run: bool,
) -> Result<()> {
    let total_items = results.cache.items
        + results.temp.items
        + results.trash.items
        + results.build.items
        + results.downloads.items
        + results.large.items
        + results.old.items
        + results.browser.items
        + results.system.items
        + results.empty.items
        + results.duplicates.items;
    let total_bytes = results.cache.size_bytes
        + results.temp.size_bytes
        + results.trash.size_bytes
        + results.build.size_bytes
        + results.downloads.size_bytes
        + results.large.size_bytes
        + results.old.size_bytes
        + results.browser.size_bytes
        + results.system.size_bytes
        + results.empty.size_bytes
        + results.duplicates.size_bytes;
    
    if total_items == 0 {
        if mode != OutputMode::Quiet {
            println!("{}", "✨ Nothing to clean.".green());
        }
        return Ok(());
    }
    
    if dry_run {
        if mode != OutputMode::Quiet {
            println!("{}", "🔍 DRY RUN MODE - No files will be deleted".yellow().bold());
            println!();
        }
    }
    
    if permanent && mode != OutputMode::Quiet {
        println!("{}", "⚠️  PERMANENT DELETE MODE - Files will bypass Recycle Bin".red().bold());
    }
    
    if !skip_confirm && !dry_run {
        print!(
            "🗑️  Delete {} items ({})? [y/N]: ",
            total_items.to_string().cyan().bold(),
            bytesize::to_string(total_bytes, true).yellow()
        );
        io::stdout().flush()?;
        
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("{}", "Cancelled.".dimmed());
            return Ok(());
        }
    }
    
    // Create progress bar
    let progress = if mode != OutputMode::Quiet {
        Some(progress::create_progress_bar(total_items as u64, "Cleaning..."))
    } else {
        None
    };
    
    let mut cleaned = 0u64;
    let mut cleaned_bytes = 0u64;
    let mut errors = 0;
    
    // Clean cache
    if results.cache.items > 0 {
        if let Some(ref pb) = progress {
            pb.set_message("Cleaning cache...");
        }
        for path in &results.cache.paths {
            if dry_run {
                cleaned += 1;
                if let Some(ref pb) = progress { pb.inc(1); }
            } else {
                match clean_path(path, permanent) {
                    Ok(()) => {
                        cleaned += 1;
                        if let Some(ref pb) = progress { pb.inc(1); }
                    }
                    Err(e) => {
                        errors += 1;
                        if mode != OutputMode::Quiet {
                            eprintln!("{} Failed to clean {}: {}", "⚠".yellow(), path.display(), e);
                        }
                    }
                }
            }
        }
        cleaned_bytes += results.cache.size_bytes;
    }
    
    // Clean temp
    if results.temp.items > 0 {
        if let Some(ref pb) = progress {
            pb.set_message("Cleaning temp files...");
        }
        for path in &results.temp.paths {
            if dry_run {
                cleaned += 1;
                if let Some(ref pb) = progress { pb.inc(1); }
            } else {
                match clean_path(path, permanent) {
                    Ok(()) => {
                        cleaned += 1;
                        if let Some(ref pb) = progress { pb.inc(1); }
                    }
                    Err(e) => {
                        errors += 1;
                        if mode != OutputMode::Quiet {
                            eprintln!("{} Failed to clean {}: {}", "⚠".yellow(), path.display(), e);
                        }
                    }
                }
            }
        }
        cleaned_bytes += results.temp.size_bytes;
    }
    
    // Clean trash
    if results.trash.items > 0 {
        if let Some(ref pb) = progress {
            pb.set_message("Emptying Recycle Bin...");
        }
        if dry_run {
            cleaned += results.trash.items as u64;
            if let Some(ref pb) = progress { pb.inc(results.trash.items as u64); }
            cleaned_bytes += results.trash.size_bytes;
        } else {
            match categories::trash::clean() {
                Ok(()) => {
                    cleaned += results.trash.items as u64;
                    if let Some(ref pb) = progress { pb.inc(results.trash.items as u64); }
                    cleaned_bytes += results.trash.size_bytes;
                }
                Err(e) => {
                    errors += 1;
                    if mode != OutputMode::Quiet {
                        eprintln!("{} Failed to empty Recycle Bin: {}", "⚠".yellow(), e);
                    }
                }
            }
        }
    }
    
    // Clean build artifacts
    if results.build.items > 0 {
        if let Some(ref pb) = progress {
            pb.set_message("Cleaning build artifacts...");
        }
        for path in &results.build.paths {
            if dry_run {
                cleaned += 1;
                if let Some(ref pb) = progress { pb.inc(1); }
            } else {
                match clean_path(path, permanent) {
                    Ok(()) => {
                        cleaned += 1;
                        if let Some(ref pb) = progress { pb.inc(1); }
                    }
                    Err(e) => {
                        errors += 1;
                        if mode != OutputMode::Quiet {
                            eprintln!("{} Failed to clean {}: {}", "⚠".yellow(), path.display(), e);
                        }
                    }
                }
            }
        }
        cleaned_bytes += results.build.size_bytes;
    }
    
    // Clean downloads
    if results.downloads.items > 0 {
        if let Some(ref pb) = progress {
            pb.set_message("Cleaning old downloads...");
        }
        for path in &results.downloads.paths {
            if dry_run {
                cleaned += 1;
                if let Some(ref pb) = progress { pb.inc(1); }
            } else {
                match clean_path(path, permanent) {
                    Ok(()) => {
                        cleaned += 1;
                        if let Some(ref pb) = progress { pb.inc(1); }
                    }
                    Err(e) => {
                        errors += 1;
                        if mode != OutputMode::Quiet {
                            eprintln!("{} Failed to clean {}: {}", "⚠".yellow(), path.display(), e);
                        }
                    }
                }
            }
        }
        cleaned_bytes += results.downloads.size_bytes;
    }
    
    // Clean large files
    if results.large.items > 0 {
        if let Some(ref pb) = progress {
            pb.set_message("Cleaning large files...");
        }
        for path in &results.large.paths {
            if dry_run {
                cleaned += 1;
                if let Some(ref pb) = progress { pb.inc(1); }
            } else {
                match clean_path(path, permanent) {
                    Ok(()) => {
                        cleaned += 1;
                        if let Some(ref pb) = progress { pb.inc(1); }
                    }
                    Err(e) => {
                        errors += 1;
                        if mode != OutputMode::Quiet {
                            eprintln!("{} Failed to clean {}: {}", "⚠".yellow(), path.display(), e);
                        }
                    }
                }
            }
        }
        cleaned_bytes += results.large.size_bytes;
    }
    
    // Clean old files
    if results.old.items > 0 {
        if let Some(ref pb) = progress {
            pb.set_message("Cleaning old files...");
        }
        for path in &results.old.paths {
            if dry_run {
                cleaned += 1;
                if let Some(ref pb) = progress { pb.inc(1); }
            } else {
                match clean_path(path, permanent) {
                    Ok(()) => {
                        cleaned += 1;
                        if let Some(ref pb) = progress { pb.inc(1); }
                    }
                    Err(e) => {
                        errors += 1;
                        if mode != OutputMode::Quiet {
                            eprintln!("{} Failed to clean {}: {}", "⚠".yellow(), path.display(), e);
                        }
                    }
                }
            }
        }
        cleaned_bytes += results.old.size_bytes;
    }
    
    // Clean browser caches
    if results.browser.items > 0 {
        if let Some(ref pb) = progress {
            pb.set_message("Cleaning browser caches...");
        }
        for path in &results.browser.paths {
            if dry_run {
                cleaned += 1;
                if let Some(ref pb) = progress { pb.inc(1); }
            } else {
                match categories::browser::clean(path) {
                    Ok(()) => {
                        cleaned += 1;
                        if let Some(ref pb) = progress { pb.inc(1); }
                    }
                    Err(e) => {
                        errors += 1;
                        if mode != OutputMode::Quiet {
                            eprintln!("{} Failed to clean {}: {}", "⚠".yellow(), path.display(), e);
                        }
                    }
                }
            }
        }
        cleaned_bytes += results.browser.size_bytes;
    }
    
    // Clean system caches
    if results.system.items > 0 {
        if let Some(ref pb) = progress {
            pb.set_message("Cleaning system caches...");
        }
        for path in &results.system.paths {
            if dry_run {
                cleaned += 1;
                if let Some(ref pb) = progress { pb.inc(1); }
            } else {
                match categories::system::clean(path) {
                    Ok(()) => {
                        cleaned += 1;
                        if let Some(ref pb) = progress { pb.inc(1); }
                    }
                    Err(e) => {
                        errors += 1;
                        if mode != OutputMode::Quiet {
                            eprintln!("{} Failed to clean {}: {}", "⚠".yellow(), path.display(), e);
                        }
                    }
                }
            }
        }
        cleaned_bytes += results.system.size_bytes;
    }
    
    // Clean empty folders
    if results.empty.items > 0 {
        if let Some(ref pb) = progress {
            pb.set_message("Cleaning empty folders...");
        }
        for path in &results.empty.paths {
            if dry_run {
                cleaned += 1;
                if let Some(ref pb) = progress { pb.inc(1); }
            } else {
                match categories::empty::clean(path) {
                    Ok(()) => {
                        cleaned += 1;
                        if let Some(ref pb) = progress { pb.inc(1); }
                    }
                    Err(e) => {
                        errors += 1;
                        if mode != OutputMode::Quiet {
                            eprintln!("{} Failed to clean {}: {}", "⚠".yellow(), path.display(), e);
                        }
                    }
                }
            }
        }
        cleaned_bytes += results.empty.size_bytes;
    }
    
    // Clean duplicate files
    if results.duplicates.items > 0 {
        if let Some(ref pb) = progress {
            pb.set_message("Cleaning duplicate files...");
        }
        for path in &results.duplicates.paths {
            if dry_run {
                cleaned += 1;
                if let Some(ref pb) = progress { pb.inc(1); }
            } else {
                match clean_path(path, permanent) {
                    Ok(()) => {
                        cleaned += 1;
                        if let Some(ref pb) = progress { pb.inc(1); }
                    }
                    Err(e) => {
                        errors += 1;
                        if mode != OutputMode::Quiet {
                            eprintln!("{} Failed to clean {}: {}", "⚠".yellow(), path.display(), e);
                        }
                    }
                }
            }
        }
        cleaned_bytes += results.duplicates.size_bytes;
    }
    
    // Finish progress bar
    if let Some(pb) = progress {
        pb.finish_and_clear();
    }
    
    // Print summary
    if mode != OutputMode::Quiet {
        println!();
        if dry_run {
            println!(
                "{} Dry run complete: {} items would be cleaned ({}), {} errors",
                "🔍".yellow(),
                cleaned.to_string().cyan().bold(),
                bytesize::to_string(cleaned_bytes, true).cyan(),
                errors.to_string().red()
            );
        } else if errors > 0 {
            println!(
                "{} Cleanup complete: {} items cleaned ({}), {} errors",
                "⚠".yellow(),
                cleaned.to_string().green().bold(),
                bytesize::to_string(cleaned_bytes, true).green(),
                errors.to_string().red()
            );
        } else {
            println!(
                "{} Cleanup complete: {} items cleaned, {} freed!",
                "✓".green().bold(),
                cleaned.to_string().green().bold(),
                bytesize::to_string(cleaned_bytes, true).green().bold()
            );
        }
    }
    
    Ok(())
}

/// Clean a single path, optionally permanently
fn clean_path(path: &Path, permanent: bool) -> Result<()> {
    if permanent {
        // Permanent delete - bypass Recycle Bin
        if path.is_dir() {
            std::fs::remove_dir_all(path)
                .with_context(|| format!("Failed to permanently delete directory: {}", path.display()))?;
        } else {
            std::fs::remove_file(path)
                .with_context(|| format!("Failed to permanently delete file: {}", path.display()))?;
        }
    } else {
        // Move to Recycle Bin
        trash::delete(path)
            .with_context(|| format!("Failed to delete: {}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::ScanResults;
    use std::fs;
    use tempfile::TempDir;
    
    fn create_test_dir() -> TempDir {
        tempfile::tempdir().unwrap()
    }
    
    #[test]
    fn test_clean_all_empty_results() {
        let results = ScanResults::default();
        
        // Should return Ok without doing anything
        let result = clean_all(&results, true, OutputMode::Normal, false, false);
        assert!(result.is_ok());
    }
    
    #[test]
    fn test_clean_all_dry_run() {
        let temp_dir = create_test_dir();
        let file = temp_dir.path().join("test.txt");
        fs::write(&file, "test content").unwrap();
        
        let mut results = ScanResults::default();
        results.cache.paths.push(file.clone());
        results.cache.items = 1;
        results.cache.size_bytes = 12;
        
        // Dry run should not delete the file
        let result = clean_all(&results, true, OutputMode::Normal, false, true);
        assert!(result.is_ok());
        assert!(file.exists()); // File should still exist
    }
}
