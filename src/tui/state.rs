//! Application state management for TUI

use crate::output::ScanResults;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::SystemTime;

#[derive(Debug, Clone)]
pub enum ConfigEditorMode {
    View,
    Editing { buffer: String },
}

#[derive(Debug, Clone)]
pub struct ConfigEditorState {
    /// Which field is selected in the Config screen
    pub selected: usize,
    pub mode: ConfigEditorMode,
    /// Temporary status message (e.g., Saved / Invalid value)
    pub message: Option<String>,
}

impl Default for ConfigEditorState {
    fn default() -> Self {
        Self {
            selected: 0,
            mode: ConfigEditorMode::View,
            message: None,
        }
    }
}

/// A single row in the Results screen.
///
/// We keep a flattened "row model" so cursor movement matches the rendered view
/// (category headers, folder headers, and items).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultsRow {
    CategoryHeader {
        group_idx: usize,
    },
    /// A folder header row within a grouped category.
    ///
    /// `depth` is the nesting depth within the category (0 = top-level folder).
    FolderHeader {
        group_idx: usize,
        folder_idx: usize,
        depth: usize,
    },
    /// An item row.
    ///
    /// `depth` is the nesting depth for rendering indentation (0 = top-level item).
    Item {
        item_idx: usize,
        depth: usize,
    },
    Spacer,
}

/// A single row in the Confirm screen.
///
/// Now matches ResultsRow with folder grouping support for consistent behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmRow {
    CategoryHeader {
        cat_idx: usize,
    },
    /// A folder header row within a grouped category.
    ///
    /// `depth` is the nesting depth within the category (0 = top-level folder).
    FolderHeader {
        cat_idx: usize,
        folder_idx: usize,
        depth: usize,
    },
    /// An item row.
    ///
    /// `depth` is the nesting depth for rendering indentation (0 = top-level item).
    Item {
        item_idx: usize,
        depth: usize,
    },
    Spacer,
}

/// Current screen being displayed
#[derive(Debug, Clone)]
pub enum Screen {
    Dashboard,
    Config,
    Scanning {
        progress: ScanProgress,
    },
    Results,
    Preview {
        index: usize,
    },
    Confirm {
        permanent: bool,
    },
    Cleaning {
        progress: CleanProgress,
    },
    Success {
        cleaned: u64,
        cleaned_bytes: u64,
        errors: usize,
        failed_temp_files: Vec<PathBuf>, // Track which temp files failed to delete
    },
    RestoreSelection {
        cursor: usize, // cursor for restore type selection
    },
    Restore {
        progress: Option<RestoreProgress>,
        result: Option<RestoreResult>,
        restore_all_bin: bool, // true = restore all bin, false = restore from last deletion
    },
    DiskInsights {
        insights: crate::disk_usage::DiskInsights,
        current_path: PathBuf,
        cursor: usize,
        sort_by: crate::disk_usage::SortBy,
    },
    Optimize {
        cursor: usize,
        selected: std::collections::HashSet<usize>,
        results: Vec<crate::optimize::OptimizeResult>,
        running: bool,
        message: Option<String>,
    },
    Status {
        status: Box<crate::status::SystemStatus>,
        last_refresh: std::time::Instant,
    },
}

/// Result of a restore operation
#[derive(Debug, Clone)]
pub struct RestoreResult {
    pub restored: usize,
    pub restored_bytes: u64,
    pub errors: usize,
    pub not_found: usize,
    pub error_reasons: Vec<String>, // Store error messages for display
}

/// Progress tracking for scanning
#[derive(Debug, Clone)]
pub struct ScanProgress {
    pub current_category: String,
    pub current_path: Option<PathBuf>,
    pub category_progress: Vec<CategoryProgress>,
    pub total_scanned: usize,
    pub total_found: usize,
    pub total_size: u64,
}

/// Progress for a single category during scan
#[derive(Debug, Clone)]
pub struct CategoryProgress {
    pub name: String,
    pub completed: bool,
    pub progress_pct: f32,
    pub size: Option<u64>,
}

/// Progress tracking for cleaning
#[derive(Debug, Clone)]
pub struct CleanProgress {
    pub current_category: String,
    pub current_path: Option<PathBuf>,
    pub cleaned: u64,
    pub total: u64,
    pub errors: usize,
}

/// Progress tracking for restoration
#[derive(Debug, Clone)]
pub struct RestoreProgress {
    pub current_path: Option<PathBuf>,
    pub restored: usize,
    pub total: usize,
    pub errors: usize,
    pub not_found: usize,
    pub restored_bytes: u64,
}

/// Category metadata for consistent naming across scan and clean
#[derive(Debug, Clone)]
pub struct CategoryDef {
    pub name: &'static str,        // Display name used everywhere
    pub scan_field: &'static str,  // Which ScanResults field to use
    pub safe: bool,                // Safe to auto-select
    pub default_enabled: bool,     // Enabled by default on dashboard
    pub description: &'static str, // Description for dashboard
}

/// Central category definitions - single source of truth for all category names
pub const CATEGORIES: &[CategoryDef] = &[
    CategoryDef {
        name: "System Cache",
        scan_field: "system",
        safe: true,
        default_enabled: true,
        description: "Windows system caches",
    },
    CategoryDef {
        name: "Browser Cache",
        scan_field: "browser",
        safe: true,
        default_enabled: true,
        description: "Browser caches",
    },
    CategoryDef {
        name: "Temp Files",
        scan_field: "temp",
        safe: true,
        default_enabled: true,
        description: "System temp folders",
    },
    CategoryDef {
        name: "Package Cache",
        scan_field: "cache",
        safe: true,
        default_enabled: true,
        description: "Package manager caches (npm, pip, nuget, etc.)",
    },
    CategoryDef {
        name: "Application Cache",
        scan_field: "app_cache",
        safe: true,
        default_enabled: true,
        description: "Application caches (Discord, VS Code, Slack, etc.)",
    },
    CategoryDef {
        name: "Build Artifacts",
        scan_field: "build",
        safe: true,
        default_enabled: true,
        description: "node_modules, target, .next",
    },
    CategoryDef {
        name: "Trash",
        scan_field: "trash",
        safe: true,
        default_enabled: true,
        description: "Recycle Bin contents",
    },
    CategoryDef {
        name: "Empty Folders",
        scan_field: "empty",
        safe: true,
        default_enabled: false,
        description: "Empty folders",
    },
    CategoryDef {
        name: "Old Downloads",
        scan_field: "downloads",
        safe: false,
        default_enabled: false,
        description: "Old files in Downloads",
    },
    CategoryDef {
        name: "Old Files",
        scan_field: "old",
        safe: false,
        default_enabled: false,
        description: "Files not accessed in X days",
    },
    CategoryDef {
        name: "Large Files",
        scan_field: "large",
        safe: false,
        default_enabled: false,
        description: "Files over size threshold",
    },
    CategoryDef {
        name: "Duplicates",
        scan_field: "duplicates",
        safe: false,
        default_enabled: false,
        description: "Duplicate files",
    },
    CategoryDef {
        name: "Installed Applications",
        scan_field: "applications",
        safe: false,
        default_enabled: false,
        description: "Installed applications",
    },
    CategoryDef {
        name: "Windows Update",
        scan_field: "windows_update",
        safe: false,
        default_enabled: false,
        description: "Windows Update files (requires admin)",
    },
    CategoryDef {
        name: "Event Logs",
        scan_field: "event_logs",
        safe: false,
        default_enabled: false,
        description: "Windows Event Log files (requires admin)",
    },
];

/// Category selection state
#[derive(Debug, Clone)]
pub struct CategorySelection {
    pub name: String,
    pub enabled: bool,
    pub description: String,
}

/// Pending action after scan completes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingAction {
    None,
    Clean,
    Analyze,
}

/// Folder group within a category (e.g., items grouped by parent project folder)
#[derive(Debug, Clone)]
pub struct FolderGroup {
    pub folder_name: String,
    pub items: Vec<usize>, // indices into all_items
    pub total_size: u64,
    pub expanded: bool,
}

/// Category group for results display
#[derive(Debug, Clone)]
pub struct CategoryGroup {
    pub name: String,
    pub items: Vec<usize>, // indices into all_items (for non-grouped categories)
    pub folder_groups: Vec<FolderGroup>, // folder groups (for grouped categories like Build)
    pub total_size: u64,
    pub expanded: bool,
    pub safe: bool,
    pub grouped_by_folder: bool, // true if items are grouped by folder
}

#[derive(Debug, Clone)]
struct FolderHierarchy {
    /// Root folder indices (no parent).
    roots: Vec<usize>,
    /// children[parent] = list of folder indices.
    children: Vec<Vec<usize>>,
}

fn normalize_folder_key(key: &str) -> String {
    key.replace('\\', "/").trim_end_matches('/').to_string()
}

fn is_ancestor_folder_key(ancestor: &str, child: &str) -> bool {
    if ancestor.is_empty() || child.is_empty() || ancestor == child {
        return false;
    }
    // Ensure we only match path boundaries (e.g. "Down" should not match "Downloads")
    child.starts_with(&format!("{}/", ancestor))
}

fn folder_key_for_display(scan_path: &PathBuf, folder_name: &str) -> String {
    if folder_name == "(root)" {
        return "(root)".to_string();
    }
    let folder_path = PathBuf::from(folder_name);
    crate::utils::to_relative_path(&folder_path, scan_path)
}

fn build_folder_hierarchy(
    scan_path: &PathBuf,
    group_name: &str,
    folder_groups: &[FolderGroup],
) -> FolderHierarchy {
    // Build artifacts uses non-path display labels (e.g. "project | Recent"),
    // so nesting-by-path doesn't apply there.
    let enable_path_nesting = group_name != "Build Artifacts";

    let keys: Vec<String> = folder_groups
        .iter()
        .map(|fg| folder_key_for_display(scan_path, &fg.folder_name))
        .collect();
    let norm_keys: Vec<String> = keys.iter().map(|k| normalize_folder_key(k)).collect();

    let n = folder_groups.len();
    let mut parent: Vec<Option<usize>> = vec![None; n];

    if enable_path_nesting {
        for child_idx in 0..n {
            if keys[child_idx] == "(root)" {
                continue;
            }
            let child_key = &norm_keys[child_idx];
            let mut best_parent: Option<(usize, usize)> = None; // (parent_idx, depth)

            for parent_idx in 0..n {
                if parent_idx == child_idx || keys[parent_idx] == "(root)" {
                    continue;
                }
                let parent_key = &norm_keys[parent_idx];
                if is_ancestor_folder_key(parent_key, child_key) {
                    let depth = parent_key.split('/').count();
                    if best_parent
                        .map(|(_, best_depth)| depth > best_depth)
                        .unwrap_or(true)
                    {
                        best_parent = Some((parent_idx, depth));
                    }
                }
            }

            parent[child_idx] = best_parent.map(|(idx, _)| idx);
        }
    }

    let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut roots: Vec<usize> = Vec::new();
    for idx in 0..n {
        if let Some(p) = parent[idx] {
            children[p].push(idx);
        } else {
            roots.push(idx);
        }
    }

    // Keep a stable, intuitive order by using the existing folder_groups order (folder_idx).
    roots.sort();
    for ch in &mut children {
        ch.sort();
    }

    FolderHierarchy { roots, children }
}

/// Main application state
pub struct AppState {
    pub screen: Screen,
    pub config: crate::config::Config,
    pub config_editor: ConfigEditorState,
    pub categories: Vec<CategorySelection>,
    pub scan_path: PathBuf,
    pub scan_results: Option<ScanResults>,
    pub selected_items: HashSet<usize>, // indices of selected items in flattened results
    pub cursor: usize,
    pub scroll_offset: usize,
    pub all_items: Vec<ResultItem>, // flattened list of all items for display
    pub category_groups: Vec<CategoryGroup>, // grouped results for display
    pub path_to_indices: HashMap<PathBuf, Vec<usize>>, // maps file paths to all indices in all_items (for cross-category sync)
    pub permanent_delete: bool, // flag for permanent deletion (bypass Recycle Bin)
    pub action_cursor: usize,   // cursor for action selection (0=Scan, 1=Clean, etc.)
    pub focus_actions: bool,    // true = actions panel focused, false = categories panel focused
    pub pending_action: PendingAction, // action to perform after scan completes
    pub tick: u64,              // animation tick counter
    pub visible_height: usize,  // cached visible height for scrolling calculations
    pub confirm_snapshot: HashSet<usize>, // snapshot of selected_items when entering confirm screen
    pub confirm_groups_cache: Vec<CategoryGroup>, // cached category groups for confirm screen (stable ordering)
    pub search_mode: bool,                        // whether search mode is active
    pub search_query: String,                     // current search query
    pub dashboard_message: Option<String>,        // temporary message for dashboard (e.g. warnings)
    pub last_scan_categories: Option<std::collections::HashSet<String>>, // categories enabled during last scan (for result reuse)
}

/// A single result item for display in the table
#[derive(Debug, Clone)]
pub struct ResultItem {
    pub path: PathBuf,
    pub size_bytes: u64,
    pub age_days: Option<u64>,
    pub last_opened: Option<SystemTime>, // currently only populated for Installed Applications
    pub category: String,
    pub safe: bool, // true for cache/temp/trash, false for large/old/duplicates
    pub display_name: Option<String>, // Optional display name (used for applications)
}

impl AppState {
    pub fn new() -> Self {
        // Load config to use its values (create default file if needed)
        let config = crate::config::Config::load_or_create();

        // Determine scan path from config or use defaults
        let scan_path = if let Some(ref config_path) = config.ui.default_scan_path {
            PathBuf::from(config_path)
        } else {
            // Auto-detect default scan path
            std::env::var("USERPROFILE")
                .ok()
                .map(|p| {
                    let base = PathBuf::from(&p);
                    let onedrive_docs = base.join("OneDrive").join("Documents");
                    if onedrive_docs.exists() {
                        onedrive_docs
                    } else if base.join("Documents").exists() {
                        base.join("Documents")
                    } else {
                        base
                    }
                })
                .unwrap_or_else(|| PathBuf::from("."))
        };

        // Build categories list with config defaults
        let default_enabled: std::collections::HashSet<String> = config
            .categories
            .default_enabled
            .iter()
            .map(|s| s.to_lowercase())
            .collect();

        // Categories organized logically:
        // 1. System & Browser Caches (safe, system-level)
        // 2. Application Caches (safe, app-level)
        // 3. Build Artifacts (safe for inactive projects)
        // 4. System Cleanup (safe)
        // 5. User Files (requires review)
        // 6. Applications (requires review - uninstalling apps)
        // Use central CATEGORIES constant as single source of truth

        // If config specifies default_enabled, use those; otherwise use hardcoded defaults
        let use_config_defaults = !default_enabled.is_empty();

        let categories = CATEGORIES
            .iter()
            .map(|cat_def| {
                let enabled = if use_config_defaults {
                    default_enabled.contains(&cat_def.name.to_lowercase().replace(" ", "_"))
                } else {
                    cat_def.default_enabled
                };

                // Handle dynamic descriptions that depend on config values
                let description = match cat_def.name {
                    "Old Files" => format!(
                        "Files not accessed in {} days",
                        config.thresholds.min_age_days
                    ),
                    "Large Files" => format!("Files over {}MB", config.thresholds.min_size_mb),
                    _ => cat_def.description.to_string(),
                };

                CategorySelection {
                    name: cat_def.name.to_string(),
                    enabled,
                    description,
                }
            })
            .collect();

        Self {
            screen: Screen::Dashboard,
            config,
            config_editor: ConfigEditorState::default(),
            categories,
            scan_path,
            scan_results: None,
            selected_items: HashSet::new(),
            cursor: 0,
            scroll_offset: 0,
            all_items: Vec::new(),
            category_groups: Vec::new(),
            path_to_indices: HashMap::new(),
            permanent_delete: false,
            action_cursor: 0,
            focus_actions: true, // Start with actions panel focused
            pending_action: PendingAction::None,
            tick: 0,
            visible_height: 20, // Default visible height, will be updated during rendering
            confirm_snapshot: HashSet::new(), // Empty initially, set when entering confirm screen
            confirm_groups_cache: Vec::new(), // Cached category groups for confirm screen
            search_mode: false,
            search_query: String::new(),
            dashboard_message: None,
            last_scan_categories: None, // No previous scan initially
        }
    }

    /// Reset config editor UI state (selection, edit buffer, messages).
    pub fn reset_config_editor(&mut self) {
        self.config_editor = ConfigEditorState::default();
    }

    /// Apply relevant config values to the live app state (scan path + descriptions).
    pub fn apply_config_to_state(&mut self) {
        // Store old scan path to detect changes
        let old_scan_path = self.scan_path.clone();

        // Update scan path from config (or auto-detect if not set)
        self.scan_path = if let Some(ref config_path) = self.config.ui.default_scan_path {
            PathBuf::from(config_path)
        } else {
            std::env::var("USERPROFILE")
                .ok()
                .map(|p| {
                    let base = PathBuf::from(&p);
                    let onedrive_docs = base.join("OneDrive").join("Documents");
                    if onedrive_docs.exists() {
                        onedrive_docs
                    } else if base.join("Documents").exists() {
                        base.join("Documents")
                    } else {
                        base
                    }
                })
                .unwrap_or_else(|| PathBuf::from("."))
        };

        // If scan path changed, clear scan results and category tracking
        if old_scan_path != self.scan_path {
            self.scan_results = None;
            self.last_scan_categories = None;
        }

        // Update category descriptions that depend on thresholds.
        for cat in &mut self.categories {
            match cat.name.as_str() {
                "Large Files" => {
                    cat.description = format!("Files over {}MB", self.config.thresholds.min_size_mb)
                }
                "Old Files" => {
                    cat.description = format!(
                        "Files not accessed in {} days",
                        self.config.thresholds.min_age_days
                    )
                }
                _ => {}
            }
        }
    }

    /// Flatten scan results into a single list for table display
    pub fn flatten_results(&mut self) {
        if let Some(ref results) = self.scan_results {
            self.all_items.clear();
            self.selected_items.clear();
            self.category_groups.clear();

            // Clone scan_path to avoid borrow checker issues with mut self later
            let scan_path = self.scan_path.clone();

            // Helper to extract parent folder path from a path
            // e.g. C:\Users\me\repo\target -> C:\Users\me\repo
            let extract_parent_folder = move |path: &PathBuf| -> Option<String> {
                path.parent()
                    .map(|p| crate::utils::to_relative_path(p, &scan_path))
            };

            // Helper to find project root for build artifacts
            // Walks up from artifact path to find project root
            let find_project_root = |artifact_path: &PathBuf| -> Option<(PathBuf, String, bool)> {
                let mut current = artifact_path.parent()?;

                // Walk up to find project root
                while let Some(parent) = current.parent() {
                    if crate::project::detect_project_type(current).is_some() {
                        let project_name = current
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("unknown")
                            .to_string();

                        // Check if project is active (use config's project_age_days)
                        let project_age_days = self.config.thresholds.project_age_days;
                        let is_active =
                            crate::project::is_project_active(current, project_age_days)
                                .unwrap_or(false);

                        return Some((current.to_path_buf(), project_name, is_active));
                    }
                    current = parent;
                }

                // Fallback: use parent folder as project
                artifact_path.parent().map(|p| {
                    let name = p
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown")
                        .to_string();
                    (p.to_path_buf(), name, false)
                })
            };

            // Helper to add items from a category
            let mut add_category = |paths: &[PathBuf],
                                    size_bytes: u64,
                                    category: &str,
                                    safe: bool| {
                let start_idx = self.all_items.len();
                let mut total_size = 0u64;

                for path in paths {
                    // "Age" is mostly used for Old/Large files. For Installed Applications, we'll
                    // treat age as "last opened" (best-effort).
                    let last_opened = if category == "Installed Applications" {
                        crate::categories::applications::get_app_last_opened(path)
                    } else {
                        None
                    };

                    let age_days = if category == "Installed Applications" {
                        last_opened
                            .and_then(|t| t.elapsed().ok())
                            .map(|d| d.as_secs() / 86400)
                    } else {
                        std::fs::metadata(path)
                            .ok()
                            .and_then(|m| m.accessed().ok())
                            .and_then(|t| t.elapsed().ok())
                            .map(|d| d.as_secs() / 86400)
                    };

                    // NOTE: `metadata.len()` on directories is NOT the folder's contents size.
                    // For Installed Applications we already computed real directory sizes during
                    // the scan (from registry EstimatedSize or a directory walk), so use that.
                    let item_size = if category == "Installed Applications" {
                        crate::categories::applications::get_app_size(path)
                            .unwrap_or_else(|| size_bytes / paths.len().max(1) as u64)
                    } else {
                        std::fs::metadata(path)
                            .ok()
                            .map(|m| m.len())
                            .unwrap_or_else(|| size_bytes / paths.len().max(1) as u64)
                    };

                    total_size += item_size;

                    // Get display name for applications
                    // Handle edge case: if lookup fails, try with canonicalized path
                    let display_name = if category == "Installed Applications" {
                        crate::categories::applications::get_app_display_name(path).or_else(|| {
                            // Fallback: try canonicalized path
                            path.canonicalize().ok().and_then(|canon_path| {
                                crate::categories::applications::get_app_display_name(&canon_path)
                            })
                        })
                    } else {
                        None
                    };

                    self.all_items.push(ResultItem {
                        path: path.clone(),
                        size_bytes: item_size,
                        age_days,
                        last_opened,
                        category: category.to_string(),
                        safe,
                        display_name,
                    });
                }

                // Create category group if there are items
                if paths.is_empty() {
                    return;
                }

                let items: Vec<usize> = (start_idx..self.all_items.len()).collect();

                // Special handling: Applications should not be grouped by folder
                // Build artifacts are grouped by project folder
                let grouped_by_folder = category != "Installed Applications";
                let folder_groups = if category == "Build Artifacts" {
                    // Group build artifacts by project root only (not by artifact type)
                    // All artifacts for a project are combined into one group
                    use std::collections::HashMap;
                    let mut project_map: HashMap<(PathBuf, String, bool), Vec<usize>> =
                        HashMap::new();
                    let mut ungrouped_items: Vec<usize> = Vec::new();

                    for &item_idx in &items {
                        if let Some(item) = self.all_items.get(item_idx) {
                            if let Some((project_path, project_name, is_active)) =
                                find_project_root(&item.path)
                            {
                                // Group by project only (combine all artifact types)
                                project_map
                                    .entry((project_path, project_name, is_active))
                                    .or_default()
                                    .push(item_idx);
                            } else {
                                // Fallback: use parent folder
                                if let Some(folder_name) = extract_parent_folder(&item.path) {
                                    let parent_path = item.path.parent().unwrap().to_path_buf();
                                    project_map
                                        .entry((parent_path, folder_name, false))
                                        .or_default()
                                        .push(item_idx);
                                } else {
                                    ungrouped_items.push(item_idx);
                                }
                            }
                        }
                    }

                    // Convert to FolderGroup vec, sorted by total size descending
                    let mut folder_groups: Vec<FolderGroup> = project_map
                        .into_iter()
                        .map(|((_, project_name, is_active), item_indices)| {
                            let group_size: u64 = item_indices
                                .iter()
                                .filter_map(|&idx| self.all_items.get(idx))
                                .map(|item| item.size_bytes)
                                .sum();

                            // Collect artifact types for display (e.g., "node_modules, .next")
                            let mut artifact_types: Vec<String> = item_indices
                                .iter()
                                .filter_map(|&idx| self.all_items.get(idx))
                                .filter_map(|item| item.path.file_name())
                                .filter_map(|n| n.to_str())
                                .map(|s| s.to_string())
                                .collect();
                            artifact_types.sort();
                            artifact_types.dedup();

                            // Create display name: "project-name" with artifact count
                            let display_name = if is_active {
                                format!("{} | Recent", project_name)
                            } else {
                                project_name
                            };

                            FolderGroup {
                                folder_name: display_name,
                                items: item_indices,
                                total_size: group_size,
                                expanded: false, // Collapsed by default - show only project summary
                            }
                        })
                        .collect();

                    folder_groups.sort_by(|a, b| b.total_size.cmp(&a.total_size));

                    // Add ungrouped items as a separate group if any exist
                    if !ungrouped_items.is_empty() {
                        let ungrouped_size: u64 = ungrouped_items
                            .iter()
                            .filter_map(|&idx| self.all_items.get(idx))
                            .map(|item| item.size_bytes)
                            .sum();

                        folder_groups.push(FolderGroup {
                            folder_name: "(root)".to_string(),
                            items: ungrouped_items,
                            total_size: ungrouped_size,
                            expanded: true,
                        });
                    }

                    folder_groups
                } else {
                    // For other categories, group by common parent directory
                    // Find the highest common parent and nest sub-folders under it
                    use std::collections::HashMap;

                    // Collect all item paths with their indices
                    let item_paths: Vec<(usize, &PathBuf)> = items
                        .iter()
                        .filter_map(|&item_idx| {
                            self.all_items
                                .get(item_idx)
                                .map(|item| (item_idx, &item.path))
                        })
                        .collect();

                    if item_paths.is_empty() {
                        Vec::new()
                    } else {
                        // Build a map: for each directory level, which items are under it
                        let mut dir_to_items: HashMap<PathBuf, Vec<usize>> = HashMap::new();

                        for (item_idx, path) in &item_paths {
                            // Add item to all its ancestor directories
                            let mut current = (*path).clone();
                            while let Some(parent) = current.parent() {
                                let parent_path = parent.to_path_buf();
                                dir_to_items
                                    .entry(parent_path.clone())
                                    .or_default()
                                    .push(*item_idx);
                                current = parent_path;
                            }
                        }

                        // Find the deepest common parent that contains a significant portion of items
                        // Look for a parent that contains at least 30% of items or at least 3 items
                        let total_items = item_paths.len();
                        let min_items_threshold = (total_items * 3 / 10).max(3.min(total_items));

                        let mut best_common_parent: Option<PathBuf> = None;
                        let mut best_common_parent_count = 0;

                        // Check each directory level to find the best common parent
                        for (parent_path, items_in_parent) in &dir_to_items {
                            if items_in_parent.len() >= min_items_threshold {
                                // Prefer deeper paths (longer paths), then prefer more items
                                let is_better = if let Some(ref current_best) = best_common_parent {
                                    let current_depth = current_best.components().count();
                                    let candidate_depth = parent_path.components().count();

                                    // First priority: deeper path
                                    if candidate_depth > current_depth {
                                        true
                                    } else if candidate_depth == current_depth {
                                        // Same depth: prefer more items
                                        items_in_parent.len() > best_common_parent_count
                                    } else {
                                        false
                                    }
                                } else {
                                    true
                                };

                                if is_better {
                                    best_common_parent = Some(parent_path.clone());
                                    best_common_parent_count = items_in_parent.len();
                                }
                            }
                        }

                        // Group items: use common parent if found, otherwise group by immediate parent
                        let mut folder_map: HashMap<String, Vec<usize>> = HashMap::new();
                        let mut ungrouped_items: Vec<usize> = Vec::new();

                        if let Some(ref common_parent) = best_common_parent {
                            // Group under common parent - separate direct items from sub-folder items
                            let mut direct_items: Vec<usize> = Vec::new();
                            let mut subfolder_map: HashMap<PathBuf, Vec<usize>> = HashMap::new();

                            for (item_idx, path) in &item_paths {
                                if let Some(item_parent) = path.parent() {
                                    if item_parent == *common_parent {
                                        // Item is directly in the common parent
                                        direct_items.push(*item_idx);
                                    } else if item_parent.starts_with(common_parent) {
                                        // Item is in a subdirectory of common parent
                                        // Find the immediate subdirectory
                                        let relative_path = item_parent
                                            .strip_prefix(common_parent)
                                            .unwrap_or(item_parent);

                                        // Get the first component (immediate subdirectory)
                                        let subdir = if let Some(first_component) =
                                            relative_path.components().next()
                                        {
                                            common_parent.join(first_component.as_os_str())
                                        } else {
                                            item_parent.to_path_buf()
                                        };

                                        subfolder_map.entry(subdir).or_default().push(*item_idx);
                                    } else {
                                        // Item is not under common parent - group separately
                                        let folder_name = item_parent.display().to_string();
                                        folder_map.entry(folder_name).or_default().push(*item_idx);
                                    }
                                } else {
                                    ungrouped_items.push(*item_idx);
                                }
                            }

                            // Add direct items as a group under common parent
                            if !direct_items.is_empty() {
                                let folder_name = common_parent.display().to_string();
                                folder_map
                                    .entry(folder_name)
                                    .or_default()
                                    .extend(direct_items);
                            }

                            // Add sub-folders as separate groups (they'll be displayed as children)
                            for (subdir_path, subdir_items) in subfolder_map {
                                let folder_name = subdir_path.display().to_string();
                                folder_map
                                    .entry(folder_name)
                                    .or_default()
                                    .extend(subdir_items);
                            }
                        } else {
                            // No common parent found - group by immediate parent, but detect similar prefixes
                            // First, collect items by their immediate parent
                            let mut parent_to_items: HashMap<PathBuf, Vec<usize>> = HashMap::new();
                            for (item_idx, path) in &item_paths {
                                if let Some(parent) = path.parent() {
                                    parent_to_items
                                        .entry(parent.to_path_buf())
                                        .or_default()
                                        .push(*item_idx);
                                } else {
                                    ungrouped_items.push(*item_idx);
                                }
                            }

                            // Detect folders with similar prefixes (e.g., scraper-output-*)
                            // Group them under a common parent
                            let mut prefix_to_groups: HashMap<String, Vec<(PathBuf, Vec<usize>)>> =
                                HashMap::new();
                            let mut standalone_parents: Vec<(PathBuf, Vec<usize>)> = Vec::new();

                            for (parent_path, items) in parent_to_items {
                                let parent_name = parent_path
                                    .file_name()
                                    .and_then(|n| n.to_str())
                                    .unwrap_or("")
                                    .to_string();

                                // Try to extract a prefix pattern (e.g., "scraper-output" from "scraper-output-120252-186")
                                // Look for the last separator before a numeric/timestamp suffix
                                let mut found_prefix = false;

                                // Try to find a separator followed by what looks like a suffix (numbers, timestamps)
                                // Common patterns: name-number, name-timestamp, name-id
                                if let Some(separator_pos) = parent_name.rfind('-') {
                                    if separator_pos > 0 && separator_pos < parent_name.len() - 1 {
                                        let potential_prefix = &parent_name[..separator_pos];
                                        let suffix = &parent_name[separator_pos + 1..];

                                        // Check if suffix looks like a number/timestamp/id (contains digits)
                                        if suffix.chars().any(|c| c.is_ascii_digit()) {
                                            prefix_to_groups
                                                .entry(potential_prefix.to_string())
                                                .or_default()
                                                .push((parent_path.clone(), items.clone()));
                                            found_prefix = true;
                                        }
                                    }
                                }

                                // Also try underscore separator
                                if !found_prefix {
                                    if let Some(separator_pos) = parent_name.rfind('_') {
                                        if separator_pos > 0
                                            && separator_pos < parent_name.len() - 1
                                        {
                                            let potential_prefix = &parent_name[..separator_pos];
                                            let suffix = &parent_name[separator_pos + 1..];

                                            if suffix.chars().any(|c| c.is_ascii_digit()) {
                                                prefix_to_groups
                                                    .entry(potential_prefix.to_string())
                                                    .or_default()
                                                    .push((parent_path.clone(), items.clone()));
                                                found_prefix = true;
                                            }
                                        }
                                    }
                                }

                                if !found_prefix {
                                    standalone_parents.push((parent_path, items));
                                }
                            }

                            // Process prefix groups: only keep groups with 2+ folders
                            for (prefix, group_items) in prefix_to_groups {
                                if group_items.len() >= 2 {
                                    // Group these folders under a common parent
                                    // Find the common parent path (parent of the first folder)
                                    if let Some((first_parent, _)) = group_items.first() {
                                        if let Some(common_parent) = first_parent.parent() {
                                            let common_parent_path = common_parent.to_path_buf();
                                            let group_folder_name = common_parent_path
                                                .join(&prefix)
                                                .display()
                                                .to_string();

                                            // Collect all items from this prefix group
                                            let mut all_prefix_items: Vec<usize> = Vec::new();
                                            for (_, items) in &group_items {
                                                all_prefix_items.extend(items);
                                            }

                                            folder_map
                                                .entry(group_folder_name)
                                                .or_default()
                                                .extend(all_prefix_items);
                                        } else {
                                            // No common parent, add as standalone
                                            for (parent_path, items) in group_items {
                                                let folder_name = parent_path.display().to_string();
                                                folder_map
                                                    .entry(folder_name)
                                                    .or_default()
                                                    .extend(items);
                                            }
                                        }
                                    } else {
                                        // Empty group, skip
                                    }
                                } else {
                                    // Less than 2 items, add as standalone
                                    for (parent_path, items) in group_items {
                                        let folder_name = parent_path.display().to_string();
                                        folder_map.entry(folder_name).or_default().extend(items);
                                    }
                                }
                            }

                            // Add standalone parents
                            for (parent_path, items) in standalone_parents {
                                let folder_name = parent_path.display().to_string();
                                folder_map.entry(folder_name).or_default().extend(items);
                            }
                        }

                        // Convert to FolderGroup vec, sorted by total size descending
                        let mut folder_groups: Vec<FolderGroup> = folder_map
                            .into_iter()
                            .map(|(folder_name, item_indices)| {
                                let group_size: u64 = item_indices
                                    .iter()
                                    .filter_map(|&idx| self.all_items.get(idx))
                                    .map(|item| item.size_bytes)
                                    .sum();

                                FolderGroup {
                                    folder_name,
                                    items: item_indices,
                                    total_size: group_size,
                                    expanded: true,
                                }
                            })
                            .collect();

                        // Sort folder groups: common parent first, then sub-folders, then others
                        if let Some(ref common_parent) = best_common_parent {
                            let common_parent_str = common_parent.display().to_string();
                            folder_groups.sort_by(|a, b| {
                                let a_is_common = a.folder_name == common_parent_str;
                                let b_is_common = b.folder_name == common_parent_str;
                                let a_is_subfolder = a.folder_name.starts_with(&common_parent_str)
                                    && a.folder_name != common_parent_str;
                                let b_is_subfolder = b.folder_name.starts_with(&common_parent_str)
                                    && b.folder_name != common_parent_str;

                                match (a_is_common, b_is_common) {
                                    (true, false) => std::cmp::Ordering::Less,
                                    (false, true) => std::cmp::Ordering::Greater,
                                    _ => match (a_is_subfolder, b_is_subfolder) {
                                        (true, false) => std::cmp::Ordering::Less,
                                        (false, true) => std::cmp::Ordering::Greater,
                                        _ => b.total_size.cmp(&a.total_size),
                                    },
                                }
                            });
                        } else {
                            folder_groups.sort_by(|a, b| b.total_size.cmp(&a.total_size));
                        }

                        // Add ungrouped items as a separate group if any exist
                        if !ungrouped_items.is_empty() {
                            let ungrouped_size: u64 = ungrouped_items
                                .iter()
                                .filter_map(|&idx| self.all_items.get(idx))
                                .map(|item| item.size_bytes)
                                .sum();

                            folder_groups.push(FolderGroup {
                                folder_name: "(root)".to_string(),
                                items: ungrouped_items,
                                total_size: ungrouped_size,
                                expanded: true,
                            });
                        }

                        folder_groups
                    }
                };

                self.category_groups.push(CategoryGroup {
                    name: category.to_string(),
                    items: if grouped_by_folder { Vec::new() } else { items },
                    folder_groups,
                    total_size,
                    expanded: true, // Start expanded
                    safe,
                    grouped_by_folder,
                });
            };

            // Helper to check if a category is currently enabled
            let is_category_enabled = |name: &str| -> bool {
                self.categories
                    .iter()
                    .any(|cat| cat.name == name && cat.enabled)
            };

            // Only add categories that are currently enabled
            // This allows reusing scan results when user disables some categories
            if is_category_enabled("Package Cache") {
                add_category(
                    &results.cache.paths,
                    results.cache.size_bytes,
                    "Package Cache",
                    true,
                );
            }
            if is_category_enabled("Application Cache") {
                add_category(
                    &results.app_cache.paths,
                    results.app_cache.size_bytes,
                    "Application Cache",
                    true,
                );
            }
            if is_category_enabled("Temp Files") {
                add_category(
                    &results.temp.paths,
                    results.temp.size_bytes,
                    "Temp Files",
                    true,
                );
            }
            if is_category_enabled("Trash") {
                add_category(
                    &results.trash.paths,
                    results.trash.size_bytes,
                    "Trash",
                    true,
                );
            }
            if is_category_enabled("Build Artifacts") {
                add_category(
                    &results.build.paths,
                    results.build.size_bytes,
                    "Build Artifacts",
                    true,
                );
            }
            if is_category_enabled("Old Downloads") {
                add_category(
                    &results.downloads.paths,
                    results.downloads.size_bytes,
                    "Old Downloads",
                    false,
                );
            }
            if is_category_enabled("Large Files") {
                add_category(
                    &results.large.paths,
                    results.large.size_bytes,
                    "Large Files",
                    false,
                );
            }
            if is_category_enabled("Old Files") {
                add_category(
                    &results.old.paths,
                    results.old.size_bytes,
                    "Old Files",
                    false,
                );
            }
            if is_category_enabled("Installed Applications") {
                add_category(
                    &results.applications.paths,
                    results.applications.size_bytes,
                    "Installed Applications",
                    false,
                );
            }
            if is_category_enabled("Browser Cache") {
                add_category(
                    &results.browser.paths,
                    results.browser.size_bytes,
                    "Browser Cache",
                    true,
                );
            }
            if is_category_enabled("System Cache") {
                add_category(
                    &results.system.paths,
                    results.system.size_bytes,
                    "System Cache",
                    true,
                );
            }
            if is_category_enabled("Empty Folders") {
                add_category(
                    &results.empty.paths,
                    results.empty.size_bytes,
                    "Empty Folders",
                    true,
                );
            }
            if is_category_enabled("Duplicates") {
                add_category(
                    &results.duplicates.paths,
                    results.duplicates.size_bytes,
                    "Duplicates",
                    false,
                );
            }
            if is_category_enabled("Windows Update") {
                add_category(
                    &results.windows_update.paths,
                    results.windows_update.size_bytes,
                    "Windows Update",
                    false,
                );
            }
            if is_category_enabled("Event Logs") {
                add_category(
                    &results.event_logs.paths,
                    results.event_logs.size_bytes,
                    "Event Logs",
                    false,
                );
            }

            // Sort category groups by size descending, then by name for stable ordering when sizes are equal
            self.category_groups.sort_by(|a, b| {
                let size_cmp = b.total_size.cmp(&a.total_size);
                if size_cmp == std::cmp::Ordering::Equal {
                    a.name.cmp(&b.name) // Secondary sort by name for stability
                } else {
                    size_cmp
                }
            });

            // Build path_to_indices mapping for cross-category selection sync
            // This allows selecting a file in one category to also select it in other categories
            self.path_to_indices.clear();
            for (idx, item) in self.all_items.iter().enumerate() {
                self.path_to_indices
                    .entry(item.path.clone())
                    .or_default()
                    .push(idx);
            }

            // Auto-select safe items
            for (i, item) in self.all_items.iter().enumerate() {
                if item.safe {
                    self.selected_items.insert(i);
                }
            }

            self.cursor = 0;
            self.scroll_offset = 0;
        }
    }

    /// Build a flattened list of rows for the Results screen.
    /// When there's only one category, skip the category header.
    pub fn results_rows(&self) -> Vec<ResultsRow> {
        let mut rows = Vec::new();
        let skip_category_header = self.category_groups.len() == 1;

        for (group_idx, group) in self.category_groups.iter().enumerate() {
            // Skip category header if there's only one category
            if !skip_category_header {
                rows.push(ResultsRow::CategoryHeader { group_idx });
            }

            // When skipping category header, always show content (treat as expanded)
            // This ensures we don't show an empty screen when there's only one category
            // For multiple categories, also ensure we show content if category is expanded
            let show_content = if skip_category_header {
                true // Always show content when header is skipped
            } else {
                group.expanded // Use actual expansion state when header is shown
            };

            if show_content {
                if group.grouped_by_folder {
                    // If folder_groups is empty, get items from category_item_indices as fallback
                    if group.folder_groups.is_empty() {
                        // Fallback: show items directly when folder grouping failed
                        let item_indices = self.category_item_indices(group_idx);
                        for &item_idx in &item_indices {
                            rows.push(ResultsRow::Item { item_idx, depth: 0 });
                        }
                    } else {
                        // Build a folder hierarchy so subfolders can nest under parents.
                        let hierarchy = build_folder_hierarchy(
                            &self.scan_path,
                            &group.name,
                            &group.folder_groups,
                        );

                        // Memoize subtree item sets so we can compute "direct items"
                        // (items not already covered by child folder groups) and avoid duplicates.
                        fn subtree_items(
                            folder_idx: usize,
                            group: &CategoryGroup,
                            children: &[Vec<usize>],
                            cache: &mut Vec<Option<HashSet<usize>>>,
                        ) -> HashSet<usize> {
                            if let Some(Some(cached)) = cache.get(folder_idx) {
                                return cached.clone();
                            }

                            let mut set: HashSet<usize> = group.folder_groups[folder_idx]
                                .items
                                .iter()
                                .copied()
                                .collect();
                            for &child in &children[folder_idx] {
                                set.extend(subtree_items(child, group, children, cache));
                            }

                            if let Some(slot) = cache.get_mut(folder_idx) {
                                *slot = Some(set.clone());
                            }
                            set
                        }

                        fn push_folder_rows(
                            rows: &mut Vec<ResultsRow>,
                            group_idx: usize,
                            group: &CategoryGroup,
                            folder_idx: usize,
                            depth: usize,
                            children: &[Vec<usize>],
                            cache: &mut Vec<Option<HashSet<usize>>>,
                        ) {
                            rows.push(ResultsRow::FolderHeader {
                                group_idx,
                                folder_idx,
                                depth,
                            });

                            let folder_group = &group.folder_groups[folder_idx];
                            if !folder_group.expanded {
                                return;
                            }

                            // Render subfolders first (tree-style).
                            for &child in &children[folder_idx] {
                                push_folder_rows(
                                    rows,
                                    group_idx,
                                    group,
                                    child,
                                    depth + 1,
                                    children,
                                    cache,
                                );
                            }

                            // Render items directly under this folder (exclude items owned by children).
                            let mut child_items: HashSet<usize> = HashSet::new();
                            for &child in &children[folder_idx] {
                                child_items.extend(subtree_items(child, group, children, cache));
                            }

                            for &item_idx in &folder_group.items {
                                if !child_items.contains(&item_idx) {
                                    rows.push(ResultsRow::Item {
                                        item_idx,
                                        depth: depth + 1,
                                    });
                                }
                            }
                        }

                        let mut cache: Vec<Option<HashSet<usize>>> =
                            vec![None; group.folder_groups.len()];

                        for (root_i, &root_folder_idx) in hierarchy.roots.iter().enumerate() {
                            push_folder_rows(
                                &mut rows,
                                group_idx,
                                group,
                                root_folder_idx,
                                0,
                                &hierarchy.children,
                                &mut cache,
                            );

                            // Spacer between top-level folders only.
                            if root_i < hierarchy.roots.len() - 1 {
                                rows.push(ResultsRow::Spacer);
                            }
                        }
                    }
                } else {
                    for &item_idx in &group.items {
                        rows.push(ResultsRow::Item { item_idx, depth: 0 });
                    }
                }
            }

            // Only add spacer if there are multiple categories
            if !skip_category_header {
                rows.push(ResultsRow::Spacer);
            }
        }

        // Final safeguard: if rows is empty but we have category groups,
        // ensure we at least show category headers (they might be collapsed)
        if rows.is_empty() && !self.category_groups.is_empty() && !skip_category_header {
            for (group_idx, _) in self.category_groups.iter().enumerate() {
                rows.push(ResultsRow::CategoryHeader { group_idx });
                if group_idx < self.category_groups.len() - 1 {
                    rows.push(ResultsRow::Spacer);
                }
            }
        }

        rows
    }

    /// Get results rows filtered by search query.
    /// Returns all rows if search_query is empty.
    /// Only shows category/folder headers if they contain matching items.
    pub fn filtered_results_rows(&self) -> Vec<ResultsRow> {
        let query = self.search_query.trim().to_lowercase();
        if query.is_empty() {
            return self.results_rows();
        }

        let mut filtered = Vec::new();
        let skip_category_header = self.category_groups.len() == 1;

        // Helper to check if an item matches the query
        let item_matches = |item_idx: usize| -> bool {
            if let Some(item) = self.all_items.get(item_idx) {
                let path_str = item.path.display().to_string().to_lowercase();
                if path_str.contains(&query) {
                    return true;
                }
                if let Some(display_name) = item.display_name.as_ref() {
                    return display_name.to_lowercase().contains(&query);
                }
                false
            } else {
                false
            }
        };

        for (group_idx, group) in self.category_groups.iter().enumerate() {
            let mut has_matching_items = false;
            let mut matching_rows = Vec::new();

            // Ignore expansion state while filtering so matches are always visible.
            let check_content = true;

            if check_content {
                if group.grouped_by_folder {
                    if !group.folder_groups.is_empty() {
                        let hierarchy = build_folder_hierarchy(
                            &self.scan_path,
                            &group.name,
                            &group.folder_groups,
                        );

                        fn subtree_items(
                            folder_idx: usize,
                            group: &CategoryGroup,
                            children: &[Vec<usize>],
                            cache: &mut Vec<Option<HashSet<usize>>>,
                        ) -> HashSet<usize> {
                            if let Some(Some(cached)) = cache.get(folder_idx) {
                                return cached.clone();
                            }

                            let mut set: HashSet<usize> = group.folder_groups[folder_idx]
                                .items
                                .iter()
                                .copied()
                                .collect();
                            for &child in &children[folder_idx] {
                                set.extend(subtree_items(child, group, children, cache));
                            }

                            if let Some(slot) = cache.get_mut(folder_idx) {
                                *slot = Some(set.clone());
                            }
                            set
                        }

                        fn subtree_has_match(
                            folder_idx: usize,
                            group: &CategoryGroup,
                            children: &[Vec<usize>],
                            item_matches: &dyn Fn(usize) -> bool,
                            cache: &mut Vec<Option<bool>>,
                        ) -> bool {
                            if let Some(Some(cached)) = cache.get(folder_idx) {
                                return *cached;
                            }

                            let mut has = group.folder_groups[folder_idx]
                                .items
                                .iter()
                                .copied()
                                .any(item_matches);
                            if !has {
                                for &child in &children[folder_idx] {
                                    if subtree_has_match(
                                        child,
                                        group,
                                        children,
                                        item_matches,
                                        cache,
                                    ) {
                                        has = true;
                                        break;
                                    }
                                }
                            }

                            if let Some(slot) = cache.get_mut(folder_idx) {
                                *slot = Some(has);
                            }
                            has
                        }

                        fn push_filtered_folder_rows(
                            rows: &mut Vec<ResultsRow>,
                            group_idx: usize,
                            group: &CategoryGroup,
                            folder_idx: usize,
                            depth: usize,
                            children: &[Vec<usize>],
                            item_matches: &dyn Fn(usize) -> bool,
                            match_cache: &mut Vec<Option<bool>>,
                            item_cache: &mut Vec<Option<HashSet<usize>>>,
                            force_expand: bool,
                        ) {
                            if !subtree_has_match(
                                folder_idx,
                                group,
                                children,
                                item_matches,
                                match_cache,
                            ) {
                                return;
                            }

                            rows.push(ResultsRow::FolderHeader {
                                group_idx,
                                folder_idx,
                                depth,
                            });

                            let folder_group = &group.folder_groups[folder_idx];
                            if !folder_group.expanded && !force_expand {
                                return;
                            }

                            // Subfolders first.
                            for &child in &children[folder_idx] {
                                push_filtered_folder_rows(
                                    rows,
                                    group_idx,
                                    group,
                                    child,
                                    depth + 1,
                                    children,
                                    item_matches,
                                    match_cache,
                                    item_cache,
                                    force_expand,
                                );
                            }

                            // Direct matching items for this folder.
                            let mut child_items: HashSet<usize> = HashSet::new();
                            for &child in &children[folder_idx] {
                                child_items
                                    .extend(subtree_items(child, group, children, item_cache));
                            }
                            for &item_idx in &folder_group.items {
                                if !child_items.contains(&item_idx) && item_matches(item_idx) {
                                    rows.push(ResultsRow::Item {
                                        item_idx,
                                        depth: depth + 1,
                                    });
                                }
                            }
                        }

                        let mut match_cache: Vec<Option<bool>> =
                            vec![None; group.folder_groups.len()];
                        let mut item_cache: Vec<Option<HashSet<usize>>> =
                            vec![None; group.folder_groups.len()];

                        let included_roots: Vec<usize> = hierarchy
                            .roots
                            .iter()
                            .copied()
                            .filter(|&root| {
                                subtree_has_match(
                                    root,
                                    group,
                                    &hierarchy.children,
                                    &item_matches,
                                    &mut match_cache,
                                )
                            })
                            .collect();

                        if !included_roots.is_empty() {
                            has_matching_items = true;
                        }

                        for (i, root) in included_roots.iter().enumerate() {
                            push_filtered_folder_rows(
                                &mut matching_rows,
                                group_idx,
                                group,
                                *root,
                                0,
                                &hierarchy.children,
                                &item_matches,
                                &mut match_cache,
                                &mut item_cache,
                                true,
                            );
                            if i < included_roots.len() - 1 {
                                matching_rows.push(ResultsRow::Spacer);
                            }
                        }
                    } else {
                        // Fallback: show items directly when folder grouping failed
                        for (item_idx, item) in self.all_items.iter().enumerate() {
                            if item.category == group.name && item_matches(item_idx) {
                                has_matching_items = true;
                                matching_rows.push(ResultsRow::Item { item_idx, depth: 0 });
                            }
                        }
                    }
                } else {
                    // Check items directly in category
                    for &item_idx in &group.items {
                        if item_matches(item_idx) {
                            has_matching_items = true;
                            matching_rows.push(ResultsRow::Item { item_idx, depth: 0 });
                        }
                    }
                }
            }

            // Only add category header and matching rows if there are matches
            if has_matching_items {
                if !skip_category_header {
                    filtered.push(ResultsRow::CategoryHeader { group_idx });
                }
                filtered.extend(matching_rows);
                if !skip_category_header {
                    filtered.push(ResultsRow::Spacer);
                }
            }
        }

        filtered
    }

    /// Build a flattened list of rows for the Confirm screen.
    /// Now includes folder grouping like results_rows() for consistent behavior.
    pub fn confirm_rows(&self) -> Vec<ConfirmRow> {
        let mut rows = Vec::new();

        // Get confirm category groups (already built and sorted)
        let confirm_groups = self.confirm_category_groups();
        let skip_category_header = confirm_groups.len() == 1;

        for (cat_idx, group) in confirm_groups.iter().enumerate() {
            // Skip category header if there's only one category
            if !skip_category_header {
                rows.push(ConfirmRow::CategoryHeader { cat_idx });
            }

            if group.expanded {
                if group.grouped_by_folder {
                    if group.folder_groups.is_empty() {
                        // Fallback: show items directly when folder grouping failed
                        for &item_idx in &group.items {
                            rows.push(ConfirmRow::Item { item_idx, depth: 0 });
                        }
                    } else {
                        let hierarchy = build_folder_hierarchy(
                            &self.scan_path,
                            &group.name,
                            &group.folder_groups,
                        );

                        fn subtree_items(
                            folder_idx: usize,
                            group: &CategoryGroup,
                            children: &[Vec<usize>],
                            cache: &mut Vec<Option<HashSet<usize>>>,
                        ) -> HashSet<usize> {
                            if let Some(Some(cached)) = cache.get(folder_idx) {
                                return cached.clone();
                            }

                            let mut set: HashSet<usize> = group.folder_groups[folder_idx]
                                .items
                                .iter()
                                .copied()
                                .collect();
                            for &child in &children[folder_idx] {
                                set.extend(subtree_items(child, group, children, cache));
                            }

                            if let Some(slot) = cache.get_mut(folder_idx) {
                                *slot = Some(set.clone());
                            }
                            set
                        }

                        fn push_folder_rows(
                            rows: &mut Vec<ConfirmRow>,
                            cat_idx: usize,
                            group: &CategoryGroup,
                            folder_idx: usize,
                            depth: usize,
                            children: &[Vec<usize>],
                            cache: &mut Vec<Option<HashSet<usize>>>,
                        ) {
                            rows.push(ConfirmRow::FolderHeader {
                                cat_idx,
                                folder_idx,
                                depth,
                            });

                            let folder_group = &group.folder_groups[folder_idx];
                            if !folder_group.expanded {
                                return;
                            }

                            // Subfolders first.
                            for &child in &children[folder_idx] {
                                push_folder_rows(
                                    rows,
                                    cat_idx,
                                    group,
                                    child,
                                    depth + 1,
                                    children,
                                    cache,
                                );
                            }

                            // Direct items for this folder (exclude child-owned items).
                            let mut child_items: HashSet<usize> = HashSet::new();
                            for &child in &children[folder_idx] {
                                child_items.extend(subtree_items(child, group, children, cache));
                            }
                            for &item_idx in &folder_group.items {
                                if !child_items.contains(&item_idx) {
                                    rows.push(ConfirmRow::Item {
                                        item_idx,
                                        depth: depth + 1,
                                    });
                                }
                            }
                        }

                        let mut cache: Vec<Option<HashSet<usize>>> =
                            vec![None; group.folder_groups.len()];

                        for &root in &hierarchy.roots {
                            push_folder_rows(
                                &mut rows,
                                cat_idx,
                                group,
                                root,
                                0,
                                &hierarchy.children,
                                &mut cache,
                            );
                        }
                    }
                } else {
                    for &item_idx in &group.items {
                        rows.push(ConfirmRow::Item { item_idx, depth: 0 });
                    }
                }
            }

            // Only add spacer if there are multiple categories
            if !skip_category_header && cat_idx < confirm_groups.len() - 1 {
                rows.push(ConfirmRow::Spacer);
            }
        }

        rows
    }

    /// Build category groups for the confirm screen from selected items.
    /// Uses confirm_snapshot to show all items that were selected when entering confirm screen,
    /// regardless of current selection state. Current selection state is used for checkbox display.
    /// Returns cached groups if available for stable ordering.
    pub fn confirm_category_groups(&self) -> Vec<CategoryGroup> {
        // Return cached groups if available (ensures stable ordering across renders)
        if !self.confirm_groups_cache.is_empty() {
            return self.confirm_groups_cache.clone();
        }

        self.build_confirm_category_groups()
    }

    /// Build and cache category groups for the confirm screen.
    /// Call this when entering the confirm screen to ensure stable ordering.
    pub fn cache_confirm_groups(&mut self) {
        self.confirm_groups_cache = self.build_confirm_category_groups();
    }

    /// Clear the confirm groups cache (call when leaving confirm screen).
    pub fn clear_confirm_cache(&mut self) {
        self.confirm_groups_cache.clear();
    }

    /// Internal method to build category groups for confirm screen.
    fn build_confirm_category_groups(&self) -> Vec<CategoryGroup> {
        use std::collections::HashMap;

        // Use confirm_snapshot if available (items that were selected when entering confirm),
        // otherwise fall back to selected_items for backward compatibility
        let items_to_show = if self.confirm_snapshot.is_empty() {
            &self.selected_items
        } else {
            &self.confirm_snapshot
        };

        // Group items by category (from snapshot, not current selection)
        let mut category_map: HashMap<String, Vec<usize>> = HashMap::new();
        for &item_idx in items_to_show {
            if let Some(item) = self.all_items.get(item_idx) {
                category_map
                    .entry(item.category.clone())
                    .or_default()
                    .push(item_idx);
            }
        }

        // Convert HashMap to Vec and sort by category name for stable processing order
        // This ensures consistent folder grouping even when categories have the same size
        let mut category_vec: Vec<(String, Vec<usize>)> = category_map.into_iter().collect();
        category_vec.sort_by(|a, b| a.0.cmp(&b.0)); // Sort by category name

        // Build category groups with folder grouping
        let mut groups: Vec<CategoryGroup> = Vec::new();

        for (category_name, item_indices) in category_vec {
            // Find the original category group to get its properties
            let original_group = self
                .category_groups
                .iter()
                .find(|g| g.name == category_name);
            let category_expanded = original_group.map(|g| g.expanded).unwrap_or(true);
            let safe = original_group.map(|g| g.safe).unwrap_or(false);
            let grouped_by_folder = original_group.map(|g| g.grouped_by_folder).unwrap_or(true);

            let total_size: u64 = item_indices
                .iter()
                .filter_map(|&idx| self.all_items.get(idx))
                .map(|item| item.size_bytes)
                .sum();

            // Build folder groups for this category
            // Use the same grouping logic as flatten_results() to ensure folder names match
            let folder_groups = if grouped_by_folder {
                if category_name == "Build Artifacts" {
                    // For Build category, use project root grouping (same as flatten_results)
                    let find_project_root =
                        |artifact_path: &PathBuf| -> Option<(PathBuf, String, bool)> {
                            let mut current = artifact_path.parent()?;

                            // Walk up to find project root
                            while let Some(parent) = current.parent() {
                                if crate::project::detect_project_type(current).is_some() {
                                    let project_name = current
                                        .file_name()
                                        .and_then(|n| n.to_str())
                                        .unwrap_or("unknown")
                                        .to_string();

                                    // Check if project is active (use config's project_age_days)
                                    let project_age_days = self.config.thresholds.project_age_days;
                                    let is_active = crate::project::is_project_active(
                                        current,
                                        project_age_days,
                                    )
                                    .unwrap_or(false);

                                    return Some((current.to_path_buf(), project_name, is_active));
                                }
                                current = parent;
                            }

                            // Fallback: use parent folder as project
                            artifact_path.parent().map(|p| {
                                let name = p
                                    .file_name()
                                    .and_then(|n| n.to_str())
                                    .unwrap_or("unknown")
                                    .to_string();
                                (p.to_path_buf(), name, false)
                            })
                        };

                    let mut project_map: HashMap<(PathBuf, String, bool), Vec<usize>> =
                        HashMap::new();
                    let mut ungrouped_items: Vec<usize> = Vec::new();

                    for &item_idx in &item_indices {
                        if let Some(item) = self.all_items.get(item_idx) {
                            if let Some((project_path, project_name, is_active)) =
                                find_project_root(&item.path)
                            {
                                project_map
                                    .entry((project_path, project_name, is_active))
                                    .or_default()
                                    .push(item_idx);
                            } else {
                                ungrouped_items.push(item_idx);
                            }
                        }
                    }

                    let mut folders: Vec<FolderGroup> = project_map
                        .into_iter()
                        .map(|((_, project_name, is_active), item_indices)| {
                            let folder_size: u64 = item_indices
                                .iter()
                                .filter_map(|&idx| self.all_items.get(idx))
                                .map(|item| item.size_bytes)
                                .sum();

                            // Create display name matching flatten_results() format
                            let display_name = if is_active {
                                format!("{} | Recent", project_name)
                            } else {
                                project_name
                            };

                            // Find original folder expansion state by matching display name
                            let folder_expanded = original_group
                                .and_then(|g| {
                                    g.folder_groups
                                        .iter()
                                        .find(|f| f.folder_name == display_name)
                                        .map(|f| f.expanded)
                                })
                                .unwrap_or(false); // Build folders default to collapsed

                            FolderGroup {
                                folder_name: display_name,
                                items: item_indices,
                                total_size: folder_size,
                                expanded: folder_expanded,
                            }
                        })
                        .collect();

                    // Sort by size descending, then by folder name for stable ordering when sizes are equal
                    folders.sort_by(|a, b| {
                        let size_cmp = b.total_size.cmp(&a.total_size);
                        if size_cmp == std::cmp::Ordering::Equal {
                            a.folder_name.cmp(&b.folder_name) // Secondary sort by name for stability
                        } else {
                            size_cmp
                        }
                    });

                    // Add ungrouped items if any
                    if !ungrouped_items.is_empty() {
                        let ungrouped_size: u64 = ungrouped_items
                            .iter()
                            .filter_map(|&idx| self.all_items.get(idx))
                            .map(|item| item.size_bytes)
                            .sum();

                        folders.push(FolderGroup {
                            folder_name: "(root)".to_string(),
                            items: ungrouped_items,
                            total_size: ungrouped_size,
                            expanded: true,
                        });
                    }

                    folders
                } else {
                    // For other categories, group by common parent directory (same as flatten_results)
                    // Collect all item paths
                    let item_paths: Vec<(usize, &PathBuf)> = item_indices
                        .iter()
                        .filter_map(|&item_idx| {
                            self.all_items
                                .get(item_idx)
                                .map(|item| (item_idx, &item.path))
                        })
                        .collect();

                    if item_paths.is_empty() {
                        Vec::new()
                    } else {
                        // Build a map: for each directory level, which items are under it
                        let mut dir_to_items: HashMap<PathBuf, Vec<usize>> = HashMap::new();

                        for (item_idx, path) in &item_paths {
                            // Add item to all its ancestor directories
                            let mut current = (*path).clone();
                            while let Some(parent) = current.parent() {
                                let parent_path = parent.to_path_buf();
                                dir_to_items
                                    .entry(parent_path.clone())
                                    .or_default()
                                    .push(*item_idx);
                                current = parent_path;
                            }
                        }

                        // Find the deepest common parent that contains a significant portion of items
                        // Look for a parent that contains at least 30% of items or at least 3 items
                        let total_items = item_paths.len();
                        let min_items_threshold = (total_items * 3 / 10).max(3.min(total_items));

                        let mut best_common_parent: Option<PathBuf> = None;
                        let mut best_common_parent_count = 0;

                        // Check each directory level to find the best common parent
                        for (parent_path, items_in_parent) in &dir_to_items {
                            if items_in_parent.len() >= min_items_threshold
                                && items_in_parent.len() > best_common_parent_count
                            {
                                // Check if this parent is deeper (longer path) than current best
                                let is_better = if let Some(ref current_best) = best_common_parent {
                                    // Prefer longer paths (deeper in hierarchy)
                                    parent_path.components().count()
                                        > current_best.components().count()
                                        || (parent_path.components().count()
                                            == current_best.components().count()
                                            && items_in_parent.len() > best_common_parent_count)
                                } else {
                                    true
                                };

                                if is_better {
                                    best_common_parent = Some(parent_path.clone());
                                    best_common_parent_count = items_in_parent.len();
                                }
                            }
                        }

                        // Group items: use common parent if found, otherwise group by immediate parent
                        let mut folder_map: HashMap<String, Vec<usize>> = HashMap::new();
                        let mut ungrouped_items: Vec<usize> = Vec::new();

                        if let Some(ref common_parent) = best_common_parent {
                            // Group under common parent - separate direct items from sub-folder items
                            let mut direct_items: Vec<usize> = Vec::new();
                            let mut subfolder_map: HashMap<PathBuf, Vec<usize>> = HashMap::new();

                            for (item_idx, path) in &item_paths {
                                if let Some(item_parent) = path.parent() {
                                    if item_parent == *common_parent {
                                        // Item is directly in the common parent
                                        direct_items.push(*item_idx);
                                    } else if item_parent.starts_with(common_parent) {
                                        // Item is in a subdirectory of common parent
                                        // Find the immediate subdirectory
                                        let relative_path = item_parent
                                            .strip_prefix(common_parent)
                                            .unwrap_or(item_parent);

                                        // Get the first component (immediate subdirectory)
                                        let subdir = if let Some(first_component) =
                                            relative_path.components().next()
                                        {
                                            common_parent.join(first_component.as_os_str())
                                        } else {
                                            item_parent.to_path_buf()
                                        };

                                        subfolder_map.entry(subdir).or_default().push(*item_idx);
                                    } else {
                                        // Item is not under common parent - group separately
                                        let folder_name = item_parent.display().to_string();
                                        folder_map.entry(folder_name).or_default().push(*item_idx);
                                    }
                                } else {
                                    ungrouped_items.push(*item_idx);
                                }
                            }

                            // Add direct items as a group under common parent
                            if !direct_items.is_empty() {
                                let folder_name = common_parent.display().to_string();
                                folder_map
                                    .entry(folder_name)
                                    .or_default()
                                    .extend(direct_items);
                            }

                            // Add sub-folders as separate groups (they'll be displayed as children)
                            for (subdir_path, subdir_items) in subfolder_map {
                                let folder_name = subdir_path.display().to_string();
                                folder_map
                                    .entry(folder_name)
                                    .or_default()
                                    .extend(subdir_items);
                            }
                        } else {
                            // No common parent found - group by immediate parent (original behavior)
                            for (item_idx, path) in &item_paths {
                                if let Some(parent) = path.parent() {
                                    let folder_name = parent.display().to_string();
                                    folder_map.entry(folder_name).or_default().push(*item_idx);
                                } else {
                                    ungrouped_items.push(*item_idx);
                                }
                            }
                        }

                        let mut folders: Vec<FolderGroup> = folder_map
                            .into_iter()
                            .map(|(folder_name, items)| {
                                let folder_size: u64 = items
                                    .iter()
                                    .filter_map(|&idx| self.all_items.get(idx))
                                    .map(|item| item.size_bytes)
                                    .sum();

                                // Find original folder expansion state - must match both category AND folder name
                                let folder_expanded = original_group
                                    .and_then(|g| {
                                        // Only match folders from the same category group
                                        g.folder_groups
                                            .iter()
                                            .find(|f| f.folder_name == folder_name)
                                            .map(|f| f.expanded)
                                    })
                                    .unwrap_or(true);

                                FolderGroup {
                                    folder_name,
                                    items,
                                    total_size: folder_size,
                                    expanded: folder_expanded,
                                }
                            })
                            .collect();

                        // Sort folder groups: common parent first, then sub-folders, then others
                        if let Some(ref common_parent) = best_common_parent {
                            let common_parent_str = common_parent.display().to_string();
                            folders.sort_by(|a, b| {
                                let a_is_common = a.folder_name == common_parent_str;
                                let b_is_common = b.folder_name == common_parent_str;
                                let a_is_subfolder = a.folder_name.starts_with(&common_parent_str)
                                    && a.folder_name != common_parent_str;
                                let b_is_subfolder = b.folder_name.starts_with(&common_parent_str)
                                    && b.folder_name != common_parent_str;

                                match (a_is_common, b_is_common) {
                                    (true, false) => std::cmp::Ordering::Less,
                                    (false, true) => std::cmp::Ordering::Greater,
                                    _ => {
                                        match (a_is_subfolder, b_is_subfolder) {
                                            (true, false) => std::cmp::Ordering::Less,
                                            (false, true) => std::cmp::Ordering::Greater,
                                            _ => {
                                                let size_cmp = b.total_size.cmp(&a.total_size);
                                                if size_cmp == std::cmp::Ordering::Equal {
                                                    a.folder_name.cmp(&b.folder_name)
                                                // Secondary sort by name for stability
                                                } else {
                                                    size_cmp
                                                }
                                            }
                                        }
                                    }
                                }
                            });
                        } else {
                            // Sort by size descending, then by folder name for stable ordering when sizes are equal
                            folders.sort_by(|a, b| {
                                let size_cmp = b.total_size.cmp(&a.total_size);
                                if size_cmp == std::cmp::Ordering::Equal {
                                    a.folder_name.cmp(&b.folder_name) // Secondary sort by name for stability
                                } else {
                                    size_cmp
                                }
                            });
                        }

                        // Add ungrouped items if any
                        if !ungrouped_items.is_empty() {
                            let ungrouped_size: u64 = ungrouped_items
                                .iter()
                                .filter_map(|&idx| self.all_items.get(idx))
                                .map(|item| item.size_bytes)
                                .sum();

                            folders.push(FolderGroup {
                                folder_name: "(root)".to_string(),
                                items: ungrouped_items,
                                total_size: ungrouped_size,
                                expanded: true,
                            });
                        }

                        folders
                    }
                }
            } else {
                Vec::new()
            };

            groups.push(CategoryGroup {
                name: category_name,
                items: if grouped_by_folder {
                    Vec::new()
                } else {
                    item_indices
                },
                folder_groups,
                total_size,
                expanded: category_expanded,
                safe,
                grouped_by_folder,
            });
        }

        // Sort by size descending, then by category name for stable ordering when sizes are equal
        groups.sort_by(|a, b| {
            let size_cmp = b.total_size.cmp(&a.total_size);
            if size_cmp == std::cmp::Ordering::Equal {
                a.name.cmp(&b.name) // Secondary sort by name for stability
            } else {
                size_cmp
            }
        });
        groups
    }

    /// Get the category index and expansion state for a category name in confirm screen.
    /// Returns (category_index_in_confirm_groups, is_expanded).
    pub fn confirm_category_state(&self, category_name: &str) -> Option<(usize, bool)> {
        use std::collections::HashMap;

        // Build the same structure as confirm_rows
        let mut category_map: HashMap<String, Vec<usize>> = HashMap::new();
        for &item_idx in &self.selected_items {
            if let Some(item) = self.all_items.get(item_idx) {
                category_map
                    .entry(item.category.clone())
                    .or_default()
                    .push(item_idx);
            }
        }

        let mut confirm_groups: Vec<(String, Vec<usize>, bool)> = category_map
            .into_iter()
            .map(|(cat, items)| {
                let expanded = self
                    .category_groups
                    .iter()
                    .find(|g| g.name == cat)
                    .map(|g| g.expanded)
                    .unwrap_or(true);
                (cat, items, expanded)
            })
            .collect();

        confirm_groups.sort_by(|a, b| {
            let size_a: u64 =
                a.1.iter()
                    .filter_map(|&idx| self.all_items.get(idx))
                    .map(|item| item.size_bytes)
                    .sum();
            let size_b: u64 =
                b.1.iter()
                    .filter_map(|&idx| self.all_items.get(idx))
                    .map(|item| item.size_bytes)
                    .sum();
            size_b.cmp(&size_a)
        });

        confirm_groups
            .iter()
            .position(|(cat, _, _)| cat == category_name)
            .map(|idx| (idx, confirm_groups[idx].2))
    }

    /// Toggle expansion for a category in the confirm screen.
    /// This updates the corresponding category_group's expansion state and the cache.
    pub fn toggle_confirm_category(&mut self, category_name: &str) {
        // Update the original category group
        if let Some(group) = self
            .category_groups
            .iter_mut()
            .find(|g| g.name == category_name)
        {
            group.expanded = !group.expanded;
        }

        // Update the cached groups to preserve ordering
        if let Some(cached_group) = self
            .confirm_groups_cache
            .iter_mut()
            .find(|g| g.name == category_name)
        {
            cached_group.expanded = !cached_group.expanded;
        }
    }

    /// Toggle expansion for a folder in the confirm screen.
    /// This updates the corresponding folder_group's expansion state and the cache.
    pub fn toggle_confirm_folder(&mut self, category_name: &str, folder_name: &str) {
        // Update the original category group
        if let Some(group) = self
            .category_groups
            .iter_mut()
            .find(|g| g.name == category_name)
        {
            if let Some(folder) = group
                .folder_groups
                .iter_mut()
                .find(|f| f.folder_name == folder_name)
            {
                folder.expanded = !folder.expanded;
            }
        }

        // Update the cached groups to preserve ordering
        if let Some(cached_group) = self
            .confirm_groups_cache
            .iter_mut()
            .find(|g| g.name == category_name)
        {
            if let Some(cached_folder) = cached_group
                .folder_groups
                .iter_mut()
                .find(|f| f.folder_name == folder_name)
            {
                cached_folder.expanded = !cached_folder.expanded;
            }
        }
    }

    /// Get all item indices for a given category name (from all_items, not just selected)
    pub fn category_items_by_name(&self, category_name: &str) -> Vec<usize> {
        self.all_items
            .iter()
            .enumerate()
            .filter_map(|(idx, item)| {
                if item.category == category_name {
                    Some(idx)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Toggle selection for a set of item indices.
    /// If all are selected, they are all deselected; otherwise, they are all selected.
    /// Also syncs selection state across categories for the same file path.
    pub fn toggle_items(&mut self, item_indices: impl IntoIterator<Item = usize>) {
        let indices: Vec<usize> = item_indices.into_iter().collect();
        if indices.is_empty() {
            return;
        }

        // Collect all related indices (same file path appearing in multiple categories)
        let mut all_related_indices: HashSet<usize> = HashSet::new();
        for &idx in &indices {
            if let Some(item) = self.all_items.get(idx) {
                if let Some(related) = self.path_to_indices.get(&item.path) {
                    all_related_indices.extend(related.iter().copied());
                } else {
                    all_related_indices.insert(idx);
                }
            }
        }

        // Check if all related indices are selected
        let all_selected = all_related_indices
            .iter()
            .all(|idx| self.selected_items.contains(idx));
        if all_selected {
            for idx in all_related_indices {
                self.selected_items.remove(&idx);
            }
        } else {
            for idx in all_related_indices {
                self.selected_items.insert(idx);
            }
        }
    }

    /// Get all item indices belonging to a given category group.
    pub fn category_item_indices(&self, group_idx: usize) -> Vec<usize> {
        let Some(group) = self.category_groups.get(group_idx) else {
            return Vec::new();
        };

        if group.grouped_by_folder {
            group
                .folder_groups
                .iter()
                .flat_map(|fg| fg.items.iter().copied())
                .collect()
        } else {
            group.items.clone()
        }
    }

    /// Get all item indices belonging to a folder group within a category group.
    pub fn folder_item_indices(&self, group_idx: usize, folder_idx: usize) -> Vec<usize> {
        let Some(group) = self.category_groups.get(group_idx) else {
            return Vec::new();
        };
        let Some(folder) = group.folder_groups.get(folder_idx) else {
            return Vec::new();
        };
        folder.items.clone()
    }

    /// Rebuild `category_groups` from the current `all_items` indices.
    ///
    /// This is useful after mutating `all_items` (e.g. excluding an item in Preview).
    pub fn rebuild_groups_from_all_items(&mut self) {
        use std::collections::HashMap;

        self.category_groups.clear();

        // Clone scan_path here too
        let scan_path = self.scan_path.clone();

        // Preserve the existing category order if we already have groups, otherwise derive from items.
        let mut by_category: HashMap<String, Vec<usize>> = HashMap::new();
        for (idx, item) in self.all_items.iter().enumerate() {
            by_category
                .entry(item.category.clone())
                .or_default()
                .push(idx);
        }

        for (category, indices) in by_category.into_iter() {
            let total_size: u64 = indices
                .iter()
                .filter_map(|&i| self.all_items.get(i))
                .map(|it| it.size_bytes)
                .sum();

            // Consider the category "safe" if all items are safe.
            let safe = indices
                .iter()
                .filter_map(|&i| self.all_items.get(i))
                .all(|it| it.safe);

            let grouped_by_folder = true;
            let folder_groups = if grouped_by_folder {
                // Group by common parent directory (same as flatten_results)
                // Collect all item paths
                let item_paths: Vec<(usize, &PathBuf)> = indices
                    .iter()
                    .filter_map(|&item_idx| {
                        self.all_items
                            .get(item_idx)
                            .map(|item| (item_idx, &item.path))
                    })
                    .collect();

                if item_paths.is_empty() {
                    Vec::new()
                } else {
                    // Build a map: for each directory level, which items are under it
                    let mut dir_to_items: HashMap<PathBuf, Vec<usize>> = HashMap::new();

                    for (item_idx, path) in &item_paths {
                        // Add item to all its ancestor directories
                        let mut current = (*path).clone();
                        while let Some(parent) = current.parent() {
                            let parent_path = parent.to_path_buf();
                            dir_to_items
                                .entry(parent_path.clone())
                                .or_default()
                                .push(*item_idx);
                            current = parent_path;
                        }
                    }

                    // Find the deepest common parent that contains a significant portion of items
                    // Look for a parent that contains at least 30% of items or at least 3 items
                    let total_items = item_paths.len();
                    let min_items_threshold = (total_items * 3 / 10).max(3.min(total_items));

                    let mut best_common_parent: Option<PathBuf> = None;
                    let mut best_common_parent_count = 0;

                    // Check each directory level to find the best common parent
                    for (parent_path, items_in_parent) in &dir_to_items {
                        if items_in_parent.len() >= min_items_threshold {
                            // Prefer deeper paths (longer paths), then prefer more items
                            let is_better = if let Some(ref current_best) = best_common_parent {
                                let current_depth = current_best.components().count();
                                let candidate_depth = parent_path.components().count();

                                // First priority: deeper path
                                if candidate_depth > current_depth {
                                    true
                                } else if candidate_depth == current_depth {
                                    // Same depth: prefer more items
                                    items_in_parent.len() > best_common_parent_count
                                } else {
                                    false
                                }
                            } else {
                                true
                            };

                            if is_better {
                                best_common_parent = Some(parent_path.clone());
                                best_common_parent_count = items_in_parent.len();
                            }
                        }
                    }

                    // Group items: use common parent if found, otherwise group by immediate parent
                    let mut folder_map: HashMap<String, Vec<usize>> = HashMap::new();
                    let mut ungrouped_items: Vec<usize> = Vec::new();

                    if let Some(ref common_parent) = best_common_parent {
                        // Group under common parent - separate direct items from sub-folder items
                        let mut direct_items: Vec<usize> = Vec::new();
                        let mut subfolder_map: HashMap<PathBuf, Vec<usize>> = HashMap::new();

                        for (item_idx, path) in &item_paths {
                            if let Some(item_parent) = path.parent() {
                                if item_parent == *common_parent {
                                    // Item is directly in the common parent
                                    direct_items.push(*item_idx);
                                } else if item_parent.starts_with(common_parent) {
                                    // Item is in a subdirectory of common parent
                                    // Find the immediate subdirectory
                                    let relative_path = item_parent
                                        .strip_prefix(common_parent)
                                        .unwrap_or(item_parent);

                                    // Get the first component (immediate subdirectory)
                                    let subdir = if let Some(first_component) =
                                        relative_path.components().next()
                                    {
                                        common_parent.join(first_component.as_os_str())
                                    } else {
                                        item_parent.to_path_buf()
                                    };

                                    subfolder_map.entry(subdir).or_default().push(*item_idx);
                                } else {
                                    // Item is not under common parent - group separately
                                    let folder_name =
                                        crate::utils::to_relative_path(item_parent, &scan_path);
                                    folder_map.entry(folder_name).or_default().push(*item_idx);
                                }
                            } else {
                                ungrouped_items.push(*item_idx);
                            }
                        }

                        // Add direct items as a group under common parent
                        if !direct_items.is_empty() {
                            let folder_name =
                                crate::utils::to_relative_path(common_parent, &scan_path);
                            folder_map
                                .entry(folder_name)
                                .or_default()
                                .extend(direct_items);
                        }

                        // Add sub-folders as separate groups (they'll be displayed as children)
                        for (subdir_path, subdir_items) in subfolder_map {
                            let folder_name =
                                crate::utils::to_relative_path(&subdir_path, &scan_path);
                            folder_map
                                .entry(folder_name)
                                .or_default()
                                .extend(subdir_items);
                        }
                    } else {
                        // No common parent found - group by immediate parent, but detect similar prefixes
                        // First, collect items by their immediate parent
                        let mut parent_to_items: HashMap<PathBuf, Vec<usize>> = HashMap::new();
                        for (item_idx, path) in &item_paths {
                            if let Some(parent) = path.parent() {
                                parent_to_items
                                    .entry(parent.to_path_buf())
                                    .or_default()
                                    .push(*item_idx);
                            } else {
                                ungrouped_items.push(*item_idx);
                            }
                        }

                        // Detect folders with similar prefixes (e.g., scraper-output-*)
                        // Group them under a common parent
                        let mut prefix_to_groups: HashMap<String, Vec<(PathBuf, Vec<usize>)>> =
                            HashMap::new();
                        let mut standalone_parents: Vec<(PathBuf, Vec<usize>)> = Vec::new();

                        for (parent_path, items) in parent_to_items {
                            let parent_name = parent_path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("")
                                .to_string();

                            // Try to extract a prefix pattern (e.g., "scraper-output" from "scraper-output-120252-186")
                            // Look for the last separator before a numeric/timestamp suffix
                            let mut found_prefix = false;

                            // Try to find a separator followed by what looks like a suffix (numbers, timestamps)
                            // Common patterns: name-number, name-timestamp, name-id
                            if let Some(separator_pos) = parent_name.rfind('-') {
                                if separator_pos > 0 && separator_pos < parent_name.len() - 1 {
                                    let potential_prefix = &parent_name[..separator_pos];
                                    let suffix = &parent_name[separator_pos + 1..];

                                    // Check if suffix looks like a number/timestamp/id (contains digits)
                                    if suffix.chars().any(|c| c.is_ascii_digit()) {
                                        prefix_to_groups
                                            .entry(potential_prefix.to_string())
                                            .or_default()
                                            .push((parent_path.clone(), items.clone()));
                                        found_prefix = true;
                                    }
                                }
                            }

                            // Also try underscore separator
                            if !found_prefix {
                                if let Some(separator_pos) = parent_name.rfind('_') {
                                    if separator_pos > 0 && separator_pos < parent_name.len() - 1 {
                                        let potential_prefix = &parent_name[..separator_pos];
                                        let suffix = &parent_name[separator_pos + 1..];

                                        if suffix.chars().any(|c| c.is_ascii_digit()) {
                                            prefix_to_groups
                                                .entry(potential_prefix.to_string())
                                                .or_default()
                                                .push((parent_path.clone(), items.clone()));
                                            found_prefix = true;
                                        }
                                    }
                                }
                            }

                            if !found_prefix {
                                standalone_parents.push((parent_path, items));
                            }
                        }

                        // Process prefix groups: only keep groups with 2+ folders
                        for (prefix, group_items) in prefix_to_groups {
                            if group_items.len() >= 2 {
                                // Group these folders under a common parent
                                // Find the common parent path (parent of the first folder)
                                if let Some((first_parent, _)) = group_items.first() {
                                    if let Some(common_parent) = first_parent.parent() {
                                        let common_parent_path = common_parent.to_path_buf();
                                        let group_folder_path = common_parent_path.join(&prefix);
                                        let group_folder_name = crate::utils::to_relative_path(
                                            &group_folder_path,
                                            &scan_path,
                                        );

                                        // Collect all items from this prefix group
                                        let mut all_prefix_items: Vec<usize> = Vec::new();
                                        for (_, items) in &group_items {
                                            all_prefix_items.extend(items);
                                        }

                                        folder_map
                                            .entry(group_folder_name)
                                            .or_default()
                                            .extend(all_prefix_items);
                                    } else {
                                        // No common parent, add as standalone
                                        for (parent_path, items) in group_items {
                                            let folder_name = crate::utils::to_relative_path(
                                                &parent_path,
                                                &scan_path,
                                            );
                                            folder_map
                                                .entry(folder_name)
                                                .or_default()
                                                .extend(items);
                                        }
                                    }
                                } else {
                                    // Empty group, skip
                                }
                            } else {
                                // Less than 2 items, add as standalone
                                for (parent_path, items) in group_items {
                                    let folder_name =
                                        crate::utils::to_relative_path(&parent_path, &scan_path);
                                    folder_map.entry(folder_name).or_default().extend(items);
                                }
                            }
                        }

                        // Add standalone parents
                        for (parent_path, items) in standalone_parents {
                            let folder_name =
                                crate::utils::to_relative_path(&parent_path, &scan_path);
                            folder_map.entry(folder_name).or_default().extend(items);
                        }
                    }

                    let mut folder_groups: Vec<FolderGroup> = folder_map
                        .into_iter()
                        .map(|(folder_name, item_indices)| {
                            let group_size: u64 = item_indices
                                .iter()
                                .filter_map(|&i| self.all_items.get(i))
                                .map(|it| it.size_bytes)
                                .sum();

                            FolderGroup {
                                folder_name,
                                items: item_indices,
                                total_size: group_size,
                                expanded: true,
                            }
                        })
                        .collect();

                    // Sort folder groups: common parent first, then sub-folders, then others
                    if let Some(ref common_parent) = best_common_parent {
                        let common_parent_str =
                            crate::utils::to_relative_path(common_parent, &scan_path);
                        folder_groups.sort_by(|a, b| {
                            let a_is_common = a.folder_name == common_parent_str;
                            let b_is_common = b.folder_name == common_parent_str;
                            let a_is_subfolder = a.folder_name.starts_with(&common_parent_str)
                                && a.folder_name != common_parent_str;
                            let b_is_subfolder = b.folder_name.starts_with(&common_parent_str)
                                && b.folder_name != common_parent_str;

                            match (a_is_common, b_is_common) {
                                (true, false) => std::cmp::Ordering::Less,
                                (false, true) => std::cmp::Ordering::Greater,
                                _ => match (a_is_subfolder, b_is_subfolder) {
                                    (true, false) => std::cmp::Ordering::Less,
                                    (false, true) => std::cmp::Ordering::Greater,
                                    _ => b.total_size.cmp(&a.total_size),
                                },
                            }
                        });
                    } else {
                        folder_groups.sort_by(|a, b| b.total_size.cmp(&a.total_size));
                    }

                    if !ungrouped_items.is_empty() {
                        let ungrouped_size: u64 = ungrouped_items
                            .iter()
                            .filter_map(|&i| self.all_items.get(i))
                            .map(|it| it.size_bytes)
                            .sum();

                        folder_groups.push(FolderGroup {
                            folder_name: "(root)".to_string(),
                            items: ungrouped_items,
                            total_size: ungrouped_size,
                            expanded: true,
                        });
                    }

                    folder_groups
                }
            } else {
                Vec::new()
            };

            self.category_groups.push(CategoryGroup {
                name: category,
                items: if grouped_by_folder {
                    Vec::new()
                } else {
                    indices
                },
                folder_groups,
                total_size,
                expanded: true,
                safe,
                grouped_by_folder,
            });
        }

        // Keep biggest categories at the top (matches initial scan behavior).
        // Sort by size descending, then by name for stable ordering when sizes are equal
        self.category_groups.sort_by(|a, b| {
            let size_cmp = b.total_size.cmp(&a.total_size);
            if size_cmp == std::cmp::Ordering::Equal {
                a.name.cmp(&b.name) // Secondary sort by name for stability
            } else {
                size_cmp
            }
        });

        // Rebuild path_to_indices mapping for cross-category selection sync
        self.path_to_indices.clear();
        for (idx, item) in self.all_items.iter().enumerate() {
            self.path_to_indices
                .entry(item.path.clone())
                .or_default()
                .push(idx);
        }
    }

    /// Get total size of selected items
    pub fn selected_size(&self) -> u64 {
        self.selected_items
            .iter()
            .filter_map(|&i| self.all_items.get(i))
            .map(|item| item.size_bytes)
            .sum()
    }

    /// Get count of selected items
    pub fn selected_count(&self) -> usize {
        self.selected_items.len()
    }

    /// Sync category selections from app state to config and save
    pub fn sync_categories_to_config(&mut self) {
        // Update config with current category enabled states
        self.config.categories.default_enabled = self
            .categories
            .iter()
            .filter(|cat| cat.enabled)
            .map(|cat| cat.name.clone())
            .collect();

        // Save config (ignore errors silently - this is best-effort)
        if let Err(e) = self.config.save() {
            eprintln!("Warning: Could not save category selections: {}", e);
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
