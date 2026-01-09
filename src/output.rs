use serde::Serialize;
use std::path::PathBuf;
use std::collections::HashMap;
use crate::utils;
use crate::theme::Theme;

/// Output verbosity mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    Quiet,       // Only errors
    Normal,      // Standard output
    Verbose,     // More details
    VeryVerbose, // All details including file paths
}

#[derive(Default, Debug, Clone)]
pub struct ScanResults {
    pub cache: CategoryResult,
    pub temp: CategoryResult,
    pub trash: CategoryResult,
    pub build: CategoryResult,
    pub downloads: CategoryResult,
    pub large: CategoryResult,
    pub old: CategoryResult,
    pub browser: CategoryResult,
    pub system: CategoryResult,
    pub empty: CategoryResult,
    pub duplicates: CategoryResult,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct CategoryResult {
    pub items: usize,
    pub size_bytes: u64,
    pub paths: Vec<PathBuf>,
}

impl CategoryResult {
    pub fn size_human(&self) -> String {
        bytesize::to_string(self.size_bytes, true)
    }
}

#[derive(Serialize)]
struct JsonResults {
    version: String,
    timestamp: String,
    categories: JsonCategories,
    summary: JsonSummary,
}

#[derive(Serialize)]
struct JsonCategories {
    cache: JsonCategory,
    temp: JsonCategory,
    trash: JsonCategory,
    build: JsonCategory,
    downloads: JsonCategory,
    large: JsonCategory,
    old: JsonCategory,
    browser: JsonCategory,
    system: JsonCategory,
    empty: JsonCategory,
    duplicates: JsonCategory,
}

#[derive(Serialize)]
struct JsonCategory {
    items: usize,
    size_bytes: u64,
    size_human: String,
    paths: Vec<String>,
}

#[derive(Serialize)]
struct JsonSummary {
    total_items: usize,
    total_bytes: u64,
    total_human: String,
}

pub fn print_human(results: &ScanResults, mode: OutputMode) {
    if mode == OutputMode::Quiet {
        return;
    }
    
    println!();
    println!("{}", Theme::header("Sweeper Scan Results"));
    println!("{}", Theme::divider_bold(60));
    println!();
    println!("{:<15} {:>8} {:>12} {:>20}", 
        Theme::primary("Category"), 
        Theme::primary("Items"), 
        Theme::primary("Size"), 
        Theme::primary("Status"));
    println!("{}", Theme::divider(60));
    
    let categories = [
        ("Cache", &results.cache, "[OK] Safe to clean"),
        ("Temp", &results.temp, "[OK] Safe to clean"),
        ("Trash", &results.trash, "[OK] Safe to clean"),
        ("Build", &results.build, "[OK] Inactive projects"),
        ("Downloads", &results.downloads, "[OK] Old files"),
        ("Large", &results.large, "[!] Review suggested"),
        ("Old", &results.old, "[!] Review suggested"),
        ("Browser", &results.browser, "[OK] Safe to clean"),
        ("System", &results.system, "[OK] Safe to clean"),
        ("Empty", &results.empty, "[OK] Safe to clean"),
        ("Duplicates", &results.duplicates, "[!] Review suggested"),
    ];
    
    for (name, result, status) in categories {
        if result.items > 0 {
            let status_colored = if status.starts_with("[OK]") { 
                Theme::status_safe(status)
            } else { 
                Theme::status_review(status)
            };
            println!(
                "{:<15} {:>8} {:>12} {:>20}",
                Theme::category(name),
                Theme::value(&result.items.to_string()),
                Theme::size(&result.size_human()),
                status_colored
            );
            
            // In verbose mode, show first few paths
            if mode == OutputMode::Verbose && !result.paths.is_empty() {
                let show_count = std::cmp::min(3, result.paths.len());
                for path in result.paths.iter().take(show_count) {
                    println!("  {}", Theme::muted(&path.display().to_string()));
                }
                if result.paths.len() > show_count {
                    println!("  {} ... and {} more", Theme::muted(""), Theme::muted(&(result.paths.len() - show_count).to_string()));
                }
            }
            
            // In very verbose mode, show all paths
            if mode == OutputMode::VeryVerbose {
                for path in &result.paths {
                    println!("  {}", Theme::muted(&path.display().to_string()));
                }
            }
        }
    }
    
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
    
    println!("{}", Theme::divider(60));
    
    if total_items == 0 {
        println!("{}", Theme::success("Your system is clean! No reclaimable space found."));
    } else {
        println!(
            "{:<15} {:>8} {:>12} {:>20}",
            Theme::header("Total"),
            Theme::value(&total_items.to_string()),
            Theme::size(&bytesize::to_string(total_bytes, true)),
            Theme::success("Reclaimable")
        );
        println!();
        println!("Run {} to remove these files.", Theme::command("sweeper clean --all"));
    }
    println!();
}

pub fn print_json(results: &ScanResults) -> anyhow::Result<()> {
    let json_results = JsonResults {
        version: "1.0".to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        categories: JsonCategories {
            cache: JsonCategory {
                items: results.cache.items,
                size_bytes: results.cache.size_bytes,
                size_human: results.cache.size_human(),
                paths: results.cache.paths.iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect(),
            },
            temp: JsonCategory {
                items: results.temp.items,
                size_bytes: results.temp.size_bytes,
                size_human: results.temp.size_human(),
                paths: results.temp.paths.iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect(),
            },
            trash: JsonCategory {
                items: results.trash.items,
                size_bytes: results.trash.size_bytes,
                size_human: results.trash.size_human(),
                paths: results.trash.paths.iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect(),
            },
            build: JsonCategory {
                items: results.build.items,
                size_bytes: results.build.size_bytes,
                size_human: results.build.size_human(),
                paths: results.build.paths.iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect(),
            },
            downloads: JsonCategory {
                items: results.downloads.items,
                size_bytes: results.downloads.size_bytes,
                size_human: results.downloads.size_human(),
                paths: results.downloads.paths.iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect(),
            },
            large: JsonCategory {
                items: results.large.items,
                size_bytes: results.large.size_bytes,
                size_human: results.large.size_human(),
                paths: results.large.paths.iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect(),
            },
            old: JsonCategory {
                items: results.old.items,
                size_bytes: results.old.size_bytes,
                size_human: results.old.size_human(),
                paths: results.old.paths.iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect(),
            },
            browser: JsonCategory {
                items: results.browser.items,
                size_bytes: results.browser.size_bytes,
                size_human: results.browser.size_human(),
                paths: results.browser.paths.iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect(),
            },
            system: JsonCategory {
                items: results.system.items,
                size_bytes: results.system.size_bytes,
                size_human: results.system.size_human(),
                paths: results.system.paths.iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect(),
            },
            empty: JsonCategory {
                items: results.empty.items,
                size_bytes: results.empty.size_bytes,
                size_human: results.empty.size_human(),
                paths: results.empty.paths.iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect(),
            },
            duplicates: JsonCategory {
                items: results.duplicates.items,
                size_bytes: results.duplicates.size_bytes,
                size_human: results.duplicates.size_human(),
                paths: results.duplicates.paths.iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect(),
            },
        },
        summary: JsonSummary {
            total_items: results.cache.items
                + results.temp.items
                + results.trash.items
                + results.build.items
                + results.downloads.items
                + results.large.items
                + results.old.items
                + results.browser.items
                + results.system.items
                + results.empty.items
                + results.duplicates.items,
            total_bytes: results.cache.size_bytes
                + results.temp.size_bytes
                + results.trash.size_bytes
                + results.build.size_bytes
                + results.downloads.size_bytes
                + results.large.size_bytes
                + results.old.size_bytes
                + results.browser.size_bytes
                + results.system.size_bytes
                + results.empty.size_bytes
                + results.duplicates.size_bytes,
            total_human: bytesize::to_string(
                results.cache.size_bytes
                    + results.temp.size_bytes
                    + results.trash.size_bytes
                    + results.build.size_bytes
                    + results.downloads.size_bytes
                    + results.large.size_bytes
                    + results.old.size_bytes
                    + results.browser.size_bytes
                    + results.system.size_bytes
                    + results.empty.size_bytes
                    + results.duplicates.size_bytes,
                true,
            ),
        },
    };
    
    println!("{}", serde_json::to_string_pretty(&json_results)?);
    Ok(())
}

pub fn print_analyze(results: &ScanResults, mode: OutputMode) {
    if mode == OutputMode::Quiet {
        return;
    }
    
    println!();
    println!("{}", Theme::header("Detailed Analysis"));
    println!("{}", Theme::divider_bold(60));
    println!();
    
    let categories = [
        ("Cache", &results.cache),
        ("Temp", &results.temp),
        ("Trash", &results.trash),
        ("Build", &results.build),
        ("Downloads", &results.downloads),
        ("Large", &results.large),
        ("Old", &results.old),
        ("Browser", &results.browser),
        ("System", &results.system),
        ("Empty", &results.empty),
        ("Duplicates", &results.duplicates),
    ];
    
    for (name, result) in categories {
        if result.items > 0 {
            println!("{}", format!("{} ({})", Theme::category(name), Theme::size(&result.size_human())));
            println!("{}", Theme::divider(60));
            
            // Show all paths with sizes
            let mut paths_with_sizes: Vec<(PathBuf, u64)> = result.paths.iter()
                .filter_map(|p| {
                    std::fs::metadata(p).ok()
                        .map(|m| (p.clone(), m.len()))
                })
                .collect();
            
            // Sort by size descending
            paths_with_sizes.sort_by(|a, b| b.1.cmp(&a.1));
            
            // Show top 10 or all if less than 10
            let show_count = std::cmp::min(10, paths_with_sizes.len());
            for (path, size) in paths_with_sizes.iter().take(show_count) {
                println!("  {}  {}", 
                    bytesize::to_string(*size, true),
                    path.display().to_string()
                );
            }
            
            if paths_with_sizes.len() > show_count {
                println!("  {} ... and {} more files", 
                    "", 
                    paths_with_sizes.len() - show_count
                );
            }
            
            // Special handling for large files: show file type breakdown
            if name == "Large" && !result.paths.is_empty() {
                println!();
                println!("  File type breakdown:");
                let mut type_counts: HashMap<&str, (usize, u64)> = HashMap::new();
                
                for path in &result.paths {
                    let file_type = utils::detect_file_type(path);
                    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
                    let entry = type_counts.entry(file_type.as_str()).or_insert((0, 0));
                    entry.0 += 1;
                    entry.1 += size;
                }
                
                let mut type_vec: Vec<(&str, usize, u64)> = type_counts.iter()
                    .map(|(k, (count, size))| (*k, *count, *size))
                    .collect();
                type_vec.sort_by(|a, b| b.2.cmp(&a.2));
                
                for (file_type, count, size) in type_vec.iter().take(5) {
                    println!("    {}: {} files ({})", 
                        Theme::secondary(file_type),
                        Theme::value(&count.to_string()),
                        Theme::muted(&bytesize::to_string(*size, true))
                    );
                }
            }
            
            // Special handling for downloads: show extension breakdown
            if name == "Downloads" && !result.paths.is_empty() {
                println!();
                println!("  Extension breakdown:");
                let mut ext_counts: HashMap<String, (usize, u64)> = HashMap::new();
                
                for path in &result.paths {
                    if let Some(ext) = path.extension() {
                        let ext_str = ext.to_string_lossy().to_lowercase();
                        let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
                        let entry = ext_counts.entry(ext_str).or_insert((0, 0));
                        entry.0 += 1;
                        entry.1 += size;
                    } else {
                        let entry = ext_counts.entry("(no extension)".to_string()).or_insert((0, 0));
                        entry.0 += 1;
                        entry.1 += std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
                    }
                }
                
                let mut ext_vec: Vec<(String, usize, u64)> = ext_counts.iter()
                    .map(|(k, (count, size))| (k.clone(), *count, *size))
                    .collect();
                ext_vec.sort_by(|a, b| b.2.cmp(&a.2));
                
                for (ext, count, size) in ext_vec.iter().take(5) {
                    println!("    .{}: {} files ({})", 
                        Theme::secondary(ext),
                        Theme::value(&count.to_string()),
                        Theme::muted(&bytesize::to_string(*size, true))
                    );
                }
            }
            
            // Special handling for old files: show age info
            if name == "Old" && !result.paths.is_empty() {
                println!();
                println!("  Age breakdown:");
                let mut age_groups: HashMap<String, (usize, u64)> = HashMap::new();
                
                for path in &result.paths {
                    if let Ok(metadata) = std::fs::metadata(path) {
                        if let Ok(accessed) = metadata.accessed() {
                            let age_days = accessed.elapsed()
                                .map(|d| d.as_secs() / 86400)
                                .unwrap_or(0);
                            
                            let age_group = if age_days < 90 {
                                "< 90 days".to_string()
                            } else if age_days < 180 {
                                "90-180 days".to_string()
                            } else if age_days < 365 {
                                "180-365 days".to_string()
                            } else {
                                "> 1 year".to_string()
                            };
                            
                            let size = metadata.len();
                            let entry = age_groups.entry(age_group).or_insert((0, 0));
                            entry.0 += 1;
                            entry.1 += size;
                        }
                    }
                }
                
                let mut age_vec: Vec<(String, usize, u64)> = age_groups.iter()
                    .map(|(k, (count, size))| (k.clone(), *count, *size))
                    .collect();
                age_vec.sort_by(|a, b| b.2.cmp(&a.2));
                
                for (age, count, size) in &age_vec {
                    println!("    {}: {} files ({})", 
                        Theme::secondary(age),
                        Theme::value(&count.to_string()),
                        Theme::muted(&bytesize::to_string(*size, true))
                    );
                }
            }
            
            println!();
        }
    }
    
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
    
    println!("{}", Theme::divider_bold(60));
    println!(
        "Total: {} items, {} reclaimable",
        Theme::value(&total_items.to_string()),
        Theme::success(&bytesize::to_string(total_bytes, true))
    );
    println!();
}
