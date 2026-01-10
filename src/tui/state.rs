//! Application state management for TUI

use crate::output::ScanResults;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

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
    CategoryHeader { group_idx: usize },
    FolderHeader { group_idx: usize, folder_idx: usize },
    Item { item_idx: usize },
    Spacer,
}

/// A single row in the Confirm screen.
///
/// Now matches ResultsRow with folder grouping support for consistent behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmRow {
    CategoryHeader { cat_idx: usize },
    FolderHeader { cat_idx: usize, folder_idx: usize },
    Item { item_idx: usize },
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
}

/// Result of a restore operation
#[derive(Debug, Clone)]
pub struct RestoreResult {
    pub restored: usize,
    pub restored_bytes: u64,
    pub errors: usize,
    pub not_found: usize,
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
}

/// A single result item for display in the table
#[derive(Debug, Clone)]
pub struct ResultItem {
    pub path: PathBuf,
    pub size_bytes: u64,
    pub age_days: Option<u64>,
    pub category: String,
    pub safe: bool, // true for cache/temp/trash, false for large/old/duplicates
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

        let all_category_names = vec![
            "Package cache",
            "Application cache",
            "Temp",
            "Trash",
            "Build",
            "Downloads",
            "Large",
            "Old",
            "Browser",
            "System",
            "Empty",
            "Duplicates",
        ];

        // If config specifies default_enabled, use those; otherwise use hardcoded defaults
        let use_config_defaults = !default_enabled.is_empty();

        let categories = all_category_names
            .iter()
            .map(|name| {
                let enabled = if use_config_defaults {
                    default_enabled.contains(&name.to_lowercase().replace(" ", "_"))
                } else {
                    // Hardcoded defaults: Package cache, Application cache, Temp, Trash, Build enabled by default
                    matches!(
                        *name,
                        "Package cache" | "Application cache" | "Temp" | "Trash" | "Build"
                    )
                };

                let description = match *name {
                    "Package cache" => "Package manager caches (npm, pip, nuget, etc.)".to_string(),
                    "Application cache" => {
                        "Application caches (Discord, VS Code, Slack, etc.)".to_string()
                    }
                    "Temp" => "System temp folders".to_string(),
                    "Trash" => "Recycle Bin contents".to_string(),
                    "Build" => "node_modules, target, .next".to_string(),
                    "Downloads" => "Old files in Downloads".to_string(),
                    "Large" => format!("Files over {}MB", config.thresholds.min_size_mb),
                    "Old" => format!(
                        "Files not accessed in {} days",
                        config.thresholds.min_age_days
                    ),
                    "Browser" => "Browser caches".to_string(),
                    "System" => "Windows system caches".to_string(),
                    "Empty" => "Empty folders".to_string(),
                    "Duplicates" => "Duplicate files".to_string(),
                    _ => "".to_string(),
                };

                CategorySelection {
                    name: name.to_string(),
                    enabled,
                    description: description.to_string(),
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
        }
    }

    /// Reset config editor UI state (selection, edit buffer, messages).
    pub fn reset_config_editor(&mut self) {
        self.config_editor = ConfigEditorState::default();
    }

    /// Apply relevant config values to the live app state (scan path + descriptions).
    pub fn apply_config_to_state(&mut self) {
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

        // Update category descriptions that depend on thresholds.
        for cat in &mut self.categories {
            match cat.name.as_str() {
                "Large" => {
                    cat.description = format!("Files over {}MB", self.config.thresholds.min_size_mb)
                }
                "Old" => {
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
                    // Calculate age if possible
                    let age_days = std::fs::metadata(path)
                        .ok()
                        .and_then(|m| m.accessed().ok())
                        .map(|accessed| {
                            accessed.elapsed().map(|d| d.as_secs() / 86400).unwrap_or(0)
                        });

                    let item_size = std::fs::metadata(path)
                        .ok()
                        .map(|m| m.len())
                        .unwrap_or_else(|| size_bytes / paths.len().max(1) as u64);

                    total_size += item_size;

                    self.all_items.push(ResultItem {
                        path: path.clone(),
                        size_bytes: item_size,
                        age_days,
                        category: category.to_string(),
                        safe,
                    });
                }

                // Create category group if there are items
                if !paths.is_empty() {
                    let items: Vec<usize> = (start_idx..self.all_items.len()).collect();

                    // Special handling for Build category: group by project folder only
                    let grouped_by_folder = true;
                    let folder_groups = if category == "Build" {
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
                            return;
                        }

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
                    };

                    self.category_groups.push(CategoryGroup {
                        name: category.to_string(),
                        items: Vec::new(),
                        folder_groups,
                        total_size,
                        expanded: true, // Start expanded
                        safe,
                        grouped_by_folder,
                    });
                }
            };

            add_category(
                &results.cache.paths,
                results.cache.size_bytes,
                "Package cache",
                true,
            );
            add_category(
                &results.app_cache.paths,
                results.app_cache.size_bytes,
                "Application cache",
                true,
            );
            add_category(
                &results.temp.paths,
                results.temp.size_bytes,
                "Temp Files",
                true,
            );
            add_category(
                &results.trash.paths,
                results.trash.size_bytes,
                "Trash",
                true,
            );
            add_category(
                &results.build.paths,
                results.build.size_bytes,
                "Build Artifacts",
                true,
            );
            add_category(
                &results.downloads.paths,
                results.downloads.size_bytes,
                "Old Downloads",
                false,
            );
            add_category(
                &results.large.paths,
                results.large.size_bytes,
                "Large Files",
                false,
            );
            add_category(
                &results.old.paths,
                results.old.size_bytes,
                "Old Files",
                false,
            );
            add_category(
                &results.browser.paths,
                results.browser.size_bytes,
                "Browser Cache",
                true,
            );
            add_category(
                &results.system.paths,
                results.system.size_bytes,
                "System Cache",
                true,
            );
            add_category(
                &results.empty.paths,
                results.empty.size_bytes,
                "Empty Folders",
                true,
            );
            add_category(
                &results.duplicates.paths,
                results.duplicates.size_bytes,
                "Duplicates",
                false,
            );

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
                            rows.push(ResultsRow::Item { item_idx });
                        }
                    } else {
                        // Always show folder headers (they're visible even when collapsed)
                        for (folder_idx, folder_group) in group.folder_groups.iter().enumerate() {
                            rows.push(ResultsRow::FolderHeader {
                                group_idx,
                                folder_idx,
                            });
                            if folder_group.expanded {
                                for &item_idx in &folder_group.items {
                                    rows.push(ResultsRow::Item { item_idx });
                                }
                            }
                            // Add spacer between folder groups, but not after the last one
                            if folder_idx < group.folder_groups.len() - 1 {
                                rows.push(ResultsRow::Spacer);
                            }
                        }
                    }
                } else {
                    for &item_idx in &group.items {
                        rows.push(ResultsRow::Item { item_idx });
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
        if self.search_query.is_empty() {
            return self.results_rows();
        }

        let query = self.search_query.to_lowercase();
        let mut filtered = Vec::new();
        let skip_category_header = self.category_groups.len() == 1;

        // Helper to check if an item matches the query
        let item_matches = |item_idx: usize| -> bool {
            if let Some(item) = self.all_items.get(item_idx) {
                let path_str = item.path.display().to_string().to_lowercase();
                path_str.contains(&query)
            } else {
                false
            }
        };

        for (group_idx, group) in self.category_groups.iter().enumerate() {
            let mut has_matching_items = false;
            let mut matching_rows = Vec::new();

            // When skipping category header, always check content (treat as expanded)
            // This ensures we don't show an empty screen when there's only one category
            let check_content = if skip_category_header {
                true // Always check content when header is skipped
            } else {
                group.expanded // Use actual expansion state when header is shown
            };

            if check_content {
                if group.grouped_by_folder {
                    // Check each folder group
                    for (folder_idx, folder_group) in group.folder_groups.iter().enumerate() {
                        let folder_has_matches =
                            folder_group.items.iter().any(|&idx| item_matches(idx));

                        if folder_has_matches {
                            has_matching_items = true;
                            matching_rows.push(ResultsRow::FolderHeader {
                                group_idx,
                                folder_idx,
                            });

                            // Add matching items from this folder
                            if folder_group.expanded {
                                for &item_idx in &folder_group.items {
                                    if item_matches(item_idx) {
                                        matching_rows.push(ResultsRow::Item { item_idx });
                                    }
                                }
                            }

                            // Add spacer between folder groups, but not after the last matching one
                            if folder_idx < group.folder_groups.len() - 1 {
                                // Check if there are more matching folders after this one
                                let has_more_matches = group
                                    .folder_groups
                                    .iter()
                                    .enumerate()
                                    .skip(folder_idx + 1)
                                    .any(|(_, fg)| fg.items.iter().any(|&idx| item_matches(idx)));
                                if has_more_matches {
                                    matching_rows.push(ResultsRow::Spacer);
                                }
                            }
                        }
                    }
                } else {
                    // Check items directly in category
                    for &item_idx in &group.items {
                        if item_matches(item_idx) {
                            has_matching_items = true;
                            matching_rows.push(ResultsRow::Item { item_idx });
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
                    for (folder_idx, folder_group) in group.folder_groups.iter().enumerate() {
                        rows.push(ConfirmRow::FolderHeader {
                            cat_idx,
                            folder_idx,
                        });
                        if folder_group.expanded {
                            for &item_idx in &folder_group.items {
                                rows.push(ConfirmRow::Item { item_idx });
                            }
                        }
                    }
                } else {
                    for &item_idx in &group.items {
                        rows.push(ConfirmRow::Item { item_idx });
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
