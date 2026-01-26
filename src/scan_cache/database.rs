//! SQLite database operations for scan cache

use crate::scan_cache::session::{ScanSession, ScanStats};
use crate::scan_cache::signature::{FileSignature, FileStatus};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, ErrorCode};
use serde_json;
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const SCHEMA_VERSION: i32 = 3;
const DB_BUSY_TIMEOUT_SECS: u64 = 30;

/// Scan cache database
pub struct ScanCache {
    db: Connection,
    current_scan_id: Option<i64>,
}

impl ScanCache {
    /// Open or create the scan cache database
    pub fn open() -> Result<Self> {
        let cache_dir = get_cache_dir()?;
        std::fs::create_dir_all(&cache_dir).with_context(|| {
            format!("Failed to create cache directory: {}", cache_dir.display())
        })?;

        let db_path = cache_dir.join("scan_cache.db");

        let db = match Self::open_connection(&db_path) {
            Ok(db) => db,
            Err(e) => {
                return Self::recover_database(&db_path, e);
            }
        };

        let mut cache = Self {
            db,
            current_scan_id: None,
        };

        // Initialize schema - if it fails due to corruption, try to recover
        if let Err(e) = cache.init_schema() {
            eprintln!(
                "Warning: Failed to initialize cache schema: {}. Attempting recovery...",
                e
            );
            drop(cache);
            return Self::recover_database(&db_path, e);
        }

        Ok(cache)
    }

    fn open_connection(db_path: &Path) -> Result<Connection> {
        let db = Connection::open(db_path)
            .with_context(|| format!("Failed to open database: {}", db_path.display()))?;

        // Enable WAL mode (Write-Ahead Logging) for better concurrency
        // This allows multiple readers while one writer is active
        db.pragma_update(None, "journal_mode", "WAL")
            .with_context(|| "Failed to enable WAL mode")?;

        // Set busy timeout to handle concurrent access gracefully
        db.busy_timeout(Duration::from_secs(DB_BUSY_TIMEOUT_SECS))
            .with_context(|| "Failed to set busy timeout")?;

        // Performance optimizations for rebuildable cache
        // NORMAL synchronous mode: faster than FULL, still safe (WAL provides durability)
        db.pragma_update(None, "synchronous", "NORMAL")
            .with_context(|| "Failed to set synchronous mode")?;

        // Store temporary tables/indexes in memory for faster operations
        db.pragma_update(None, "temp_store", "MEMORY")
            .with_context(|| "Failed to set temp_store")?;

        // Increase cache size to 64MB for better performance (default is 2MB)
        // This is safe for a rebuildable cache
        db.pragma_update(None, "cache_size", "-16384") // Negative value = KB, so -16384 = 16MB
            .with_context(|| "Failed to set cache_size")?;

        Ok(db)
    }

    fn recover_database(db_path: &Path, error: anyhow::Error) -> Result<Self> {
        eprintln!(
            "Warning: Scan cache is unavailable: {}. Attempting recovery...",
            error
        );

        if db_path.exists() {
            let backup_path = db_path.with_extension("db.backup");
            if let Err(err) = std::fs::rename(db_path, &backup_path) {
                eprintln!(
                    "Warning: Failed to move corrupted cache database to {}: {}",
                    backup_path.display(),
                    err
                );
                if let Err(err) = std::fs::copy(db_path, &backup_path) {
                    eprintln!(
                        "Warning: Failed to backup corrupted cache database to {}: {}",
                        backup_path.display(),
                        err
                    );
                } else if let Err(err) = std::fs::remove_file(db_path) {
                    eprintln!(
                        "Warning: Failed to remove corrupted cache database: {}",
                        err
                    );
                }
            }

            let _ = std::fs::remove_file(db_path.with_extension("db-wal"));
            let _ = std::fs::remove_file(db_path.with_extension("db-shm"));
        }

        let db = Self::open_connection(db_path)
            .with_context(|| format!("Failed to recreate database: {}", db_path.display()))?;
        let mut cache = Self {
            db,
            current_scan_id: None,
        };
        cache
            .init_schema()
            .with_context(|| "Failed to initialize schema after recovery")?;
        Ok(cache)
    }

    /// Initialize database schema
    fn init_schema(&mut self) -> Result<()> {
        // Check if schema_version table exists
        let version: i32 = self
            .db
            .query_row("SELECT version FROM schema_version LIMIT 1", [], |row| {
                row.get(0)
            })
            .or_else(|_| {
                // Table doesn't exist, create it and return version 0
                self.db.execute(
                    "CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL)",
                    [],
                )?;
                self.db
                    .execute("INSERT INTO schema_version (version) VALUES (0)", [])?;
                Ok::<i32, rusqlite::Error>(0)
            })?;

        if version < SCHEMA_VERSION {
            self.migrate_schema(version)?;
        }

        Ok(())
    }

    /// Migrate schema to current version
    fn migrate_schema(&mut self, from_version: i32) -> Result<()> {
        // Use transaction to ensure atomic migration
        let tx = self
            .db
            .transaction()
            .with_context(|| "Failed to start migration transaction")?;

        if from_version < 1 {
            // Initial schema
            tx.execute(
                "CREATE TABLE IF NOT EXISTS file_records (
                    path TEXT PRIMARY KEY,
                    size INTEGER NOT NULL,
                    mtime_secs INTEGER NOT NULL,
                    mtime_nsecs INTEGER NOT NULL,
                    content_hash TEXT,
                    category TEXT NOT NULL,
                    last_scan_id INTEGER NOT NULL,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL
                )",
                [],
            )
            .with_context(|| "Failed to create file_records table")?;

            tx.execute(
                "CREATE TABLE IF NOT EXISTS scan_sessions (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    started_at INTEGER NOT NULL,
                    finished_at INTEGER,
                    scan_type TEXT NOT NULL,
                    categories TEXT NOT NULL,
                    total_files INTEGER,
                    new_files INTEGER,
                    changed_files INTEGER,
                    removed_files INTEGER
                )",
                [],
            )
            .with_context(|| "Failed to create scan_sessions table")?;

            // Create indexes
            tx.execute(
                "CREATE INDEX IF NOT EXISTS idx_category ON file_records(category)",
                [],
            )
            .with_context(|| "Failed to create category index")?;
            tx.execute(
                "CREATE INDEX IF NOT EXISTS idx_scan_id ON file_records(last_scan_id)",
                [],
            )
            .with_context(|| "Failed to create scan_id index")?;
            tx.execute(
                "CREATE INDEX IF NOT EXISTS idx_size ON file_records(size)",
                [],
            )
            .with_context(|| "Failed to create size index")?;

            // Update schema version
            tx.execute("UPDATE schema_version SET version = ?1", [1])
                .with_context(|| "Failed to update schema version")?;
        }

        if from_version < 2 {
            // Migration to version 2: Add per-category scan IDs
            // Create category_scans table to track scan IDs per category
            tx.execute(
                "CREATE TABLE IF NOT EXISTS category_scans (
                    category TEXT PRIMARY KEY,
                    scan_id INTEGER NOT NULL DEFAULT 0,
                    last_scan_session_id INTEGER,
                    last_updated INTEGER NOT NULL
                )",
                [],
            )
            .with_context(|| "Failed to create category_scans table")?;

            // Migrate existing data: create category scan IDs from existing file_records
            // For each category, find the max last_scan_id used and set that as the category scan_id
            tx.execute(
                "INSERT INTO category_scans (category, scan_id, last_updated)
                 SELECT category, MAX(last_scan_id), MAX(updated_at)
                 FROM file_records
                 GROUP BY category
                 ON CONFLICT(category) DO NOTHING",
                [],
            )
            .with_context(|| "Failed to migrate category scan IDs")?;

            // Create index for category_scans
            tx.execute(
                "CREATE INDEX IF NOT EXISTS idx_category_scans_category ON category_scans(category)",
                [],
            )
            .with_context(|| "Failed to create category_scans index")?;

            // Update schema version
            tx.execute("UPDATE schema_version SET version = ?1", [2])
                .with_context(|| "Failed to update schema version")?;
        }

        if from_version < 3 {
            // Migration to version 3: Add file_categories junction table to support multiple categories per file
            // Create file_categories table to track which categories each file belongs to
            tx.execute(
                "CREATE TABLE IF NOT EXISTS file_categories (
                    path TEXT NOT NULL,
                    category TEXT NOT NULL,
                    category_scan_id INTEGER NOT NULL,
                    PRIMARY KEY (path, category),
                    FOREIGN KEY (path) REFERENCES file_records(path) ON DELETE CASCADE
                )",
                [],
            )
            .with_context(|| "Failed to create file_categories table")?;

            // Migrate existing data: create file_categories entries from file_records
            // Use the last_scan_id from file_records as the category_scan_id
            // Note: This assumes files were stored with per-category scan IDs (from version 2)
            // For files that might have been stored before version 2, we'll use the last_scan_id
            tx.execute(
                "INSERT INTO file_categories (path, category, category_scan_id)
                 SELECT path, category, last_scan_id
                 FROM file_records
                 WHERE category IS NOT NULL
                 ON CONFLICT(path, category) DO NOTHING",
                [],
            )
            .with_context(|| "Failed to migrate file_categories data")?;

            // Create indexes for file_categories
            tx.execute(
                "CREATE INDEX IF NOT EXISTS idx_file_categories_category ON file_categories(category, category_scan_id)",
                [],
            )
            .with_context(|| "Failed to create file_categories category index")?;
            tx.execute(
                "CREATE INDEX IF NOT EXISTS idx_file_categories_path ON file_categories(path)",
                [],
            )
            .with_context(|| "Failed to create file_categories path index")?;

            // Update schema version
            tx.execute("UPDATE schema_version SET version = ?1", [SCHEMA_VERSION])
                .with_context(|| "Failed to update schema version")?;
        }

        // Commit migration transaction
        tx.commit()
            .with_context(|| "Failed to commit migration transaction")?;

        Ok(())
    }

    /// Start a new scan session
    pub fn start_scan(&mut self, scan_type: &str, categories: &[&str]) -> Result<i64> {
        let started_at = Utc::now().timestamp();
        let categories_json = serde_json::to_string(categories)?;

        self.db.execute(
            "INSERT INTO scan_sessions (started_at, scan_type, categories) VALUES (?1, ?2, ?3)",
            params![started_at, scan_type, categories_json],
        )?;

        let scan_id = self.db.last_insert_rowid();
        self.current_scan_id = Some(scan_id);
        Ok(scan_id)
    }

    /// Check if a file needs rescanning
    pub fn check_file(&self, path: &Path) -> Result<FileStatus> {
        let path_str = normalize_path(path);

        let result: Option<(i64, i64, i64)> = self
            .db
            .query_row(
                "SELECT size, mtime_secs, mtime_nsecs FROM file_records WHERE path = ?1",
                [&path_str],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .ok();

        let Some((cached_size, mtime_secs, mtime_nsecs)) = result else {
            return Ok(FileStatus::New);
        };

        // Get current file metadata
        let metadata = match std::fs::metadata(path) {
            Ok(m) => m,
            Err(err) => {
                if err.kind() == io::ErrorKind::NotFound {
                    // File doesn't exist at original location - check if it's in recycle bin
                    // If it's in recycle bin, it was cleaned (mark as InRecycleBin)
                    // If not in recycle bin, it's truly deleted
                    if is_in_recycle_bin(path) {
                        return Ok(FileStatus::InRecycleBin);
                    } else {
                        return Ok(FileStatus::Deleted);
                    }
                } else {
                    return Ok(FileStatus::Modified);
                }
            }
        };

        let current_size = clamp_size_to_i64(metadata.len());
        let current_mtime = match metadata.modified() {
            Ok(mtime) => mtime,
            Err(_) => return Ok(FileStatus::Modified),
        };

        let (current_secs, current_nsecs) = system_time_to_secs_nsecs(current_mtime);

        // Compare signatures
        if current_size != cached_size || current_secs != mtime_secs || current_nsecs != mtime_nsecs
        {
            Ok(FileStatus::Modified)
        } else {
            Ok(FileStatus::Unchanged)
        }
    }

    /// Batch check multiple files (more efficient)
    pub fn check_files_batch(&self, paths: &[PathBuf]) -> Result<HashMap<PathBuf, FileStatus>> {
        let mut result = HashMap::new();

        if paths.is_empty() {
            return Ok(result);
        }

        let mut cached: HashMap<String, (i64, i64, i64)> = HashMap::new();
        const SQLITE_MAX_VARS: usize = 900;

        // Get all cached records for these paths (normalize for consistent lookup)
        for chunk in paths.chunks(SQLITE_MAX_VARS) {
            let path_strs: Vec<String> = chunk.iter().map(|p| normalize_path(p)).collect();
            let placeholders = path_strs.iter().map(|_| "?").collect::<Vec<_>>().join(",");

            let mut stmt = self.db.prepare(&format!(
                "SELECT path, size, mtime_secs, mtime_nsecs FROM file_records WHERE path IN ({})",
                placeholders
            ))?;

            // Build params vector manually
            let mut query_params: Vec<&dyn rusqlite::ToSql> = Vec::new();
            for s in &path_strs {
                query_params.push(s);
            }

            let rows = stmt.query_map(rusqlite::params_from_iter(query_params), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    (
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                    ),
                ))
            })?;

            for row in rows {
                let (path, values) = row?;
                cached.insert(path, values);
            }
        }

        // Check each file
        for path in paths {
            let path_str = normalize_path(path);

            if let Some((cached_size, mtime_secs, mtime_nsecs)) = cached.get(&path_str) {
                // File is in cache, check if it changed
                match std::fs::metadata(path) {
                    Ok(metadata) => {
                        let current_size = clamp_size_to_i64(metadata.len());
                        let current_mtime = metadata.modified().ok();

                        if let Some(mtime) = current_mtime {
                            let (current_secs, current_nsecs) = system_time_to_secs_nsecs(mtime);

                            if current_size != *cached_size
                                || current_secs != *mtime_secs
                                || current_nsecs != *mtime_nsecs
                            {
                                result.insert(path.clone(), FileStatus::Modified);
                            } else {
                                result.insert(path.clone(), FileStatus::Unchanged);
                            }
                        } else {
                            result.insert(path.clone(), FileStatus::Modified);
                        }
                    }
                    Err(err) => {
                        if err.kind() == io::ErrorKind::NotFound {
                            // File doesn't exist at original location - check recycle bin
                            // If in recycle bin, mark as InRecycleBin; if not, mark as Deleted
                            if is_in_recycle_bin(path) {
                                result.insert(path.clone(), FileStatus::InRecycleBin);
                            } else {
                                result.insert(path.clone(), FileStatus::Deleted);
                            }
                        } else {
                            result.insert(path.clone(), FileStatus::Modified);
                        }
                    }
                }
            } else {
                // File not in cache
                match std::fs::metadata(path) {
                    Ok(_) => {
                        result.insert(path.clone(), FileStatus::New);
                    }
                    Err(err) => {
                        if err.kind() == io::ErrorKind::NotFound {
                            // File not in cache and doesn't exist - check recycle bin
                            // If in recycle bin, mark as InRecycleBin; if not, mark as Deleted
                            if is_in_recycle_bin(path) {
                                result.insert(path.clone(), FileStatus::InRecycleBin);
                            } else {
                                result.insert(path.clone(), FileStatus::Deleted);
                            }
                        } else {
                            result.insert(path.clone(), FileStatus::Modified);
                        }
                    }
                }
            }
        }

        Ok(result)
    }

    /// Update/insert file record after scanning
    pub fn upsert_file(&mut self, sig: &FileSignature, category: &str, scan_id: i64) -> Result<()> {
        let path_str = normalize_path(&sig.path);
        let (mtime_secs, mtime_nsecs) = system_time_to_secs_nsecs(sig.mtime);
        let now = Utc::now().timestamp();

        // Handle potential integer overflow for very large files (>9PB)
        // SQLite INTEGER can store up to 8 bytes, so i64::MAX is safe
        let size_i64 = clamp_size_to_i64(sig.size);

        // Insert/update file record (file metadata) - don't overwrite category on conflict
        self.db.execute(
            "INSERT INTO file_records (path, size, mtime_secs, mtime_nsecs, content_hash, category, last_scan_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(path) DO UPDATE SET
                size = ?2,
                mtime_secs = ?3,
                mtime_nsecs = ?4,
                content_hash = ?5,
                last_scan_id = ?7,
                updated_at = ?9",
            params![
                path_str,
                size_i64,
                mtime_secs,
                mtime_nsecs,
                sig.content_hash,
                category,
                scan_id,
                now,
                now
            ],
        )?;

        // Insert/update file_categories junction table (category membership)
        // This allows files to belong to multiple categories
        self.db.execute(
            "INSERT INTO file_categories (path, category, category_scan_id)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(path, category) DO UPDATE SET
                category_scan_id = ?3",
            params![path_str, category, scan_id],
        )?;

        Ok(())
    }

    /// Batch upsert for efficiency
    pub fn upsert_files_batch(
        &mut self,
        records: &[(FileSignature, String)],
        scan_id: i64,
    ) -> Result<()> {
        if records.is_empty() {
            return Ok(());
        }

        let tx = self
            .db
            .transaction()
            .with_context(|| "Failed to start transaction")?;

        // Calculate timestamp once outside the loop
        let now = Utc::now().timestamp();

        // Prepare statements once per batch for better performance
        {
            // Statement for file_records - don't overwrite category on conflict to preserve multi-category support
            let mut file_stmt = tx.prepare_cached(
                "INSERT INTO file_records (path, size, mtime_secs, mtime_nsecs, content_hash, category, last_scan_id, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(path) DO UPDATE SET
                    size = ?2,
                    mtime_secs = ?3,
                    mtime_nsecs = ?4,
                    content_hash = ?5,
                    last_scan_id = ?7,
                    updated_at = ?9"
            )?;

            // Statement for file_categories - tracks which categories each file belongs to
            let mut category_stmt = tx.prepare_cached(
                "INSERT INTO file_categories (path, category, category_scan_id)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(path, category) DO UPDATE SET
                    category_scan_id = ?3",
            )?;

            for (sig, category) in records {
                let path_str = normalize_path(&sig.path);
                let (mtime_secs, mtime_nsecs) = system_time_to_secs_nsecs(sig.mtime);

                // Handle potential integer overflow for very large files
                let size_i64 = clamp_size_to_i64(sig.size);

                // Upsert file record (metadata only, category not overwritten on conflict)
                if let Err(e) = file_stmt.execute(params![
                    path_str,
                    size_i64,
                    mtime_secs,
                    mtime_nsecs,
                    sig.content_hash,
                    category,
                    scan_id,
                    now,
                    now
                ]) {
                    // Transaction will be rolled back automatically on drop
                    return Err(anyhow::anyhow!("Failed to upsert file record: {}", e));
                }

                // Upsert file_categories to track this file belongs to this category
                if let Err(e) = category_stmt.execute(params![path_str, category, scan_id]) {
                    // Transaction will be rolled back automatically on drop
                    return Err(anyhow::anyhow!("Failed to upsert file category: {}", e));
                }
            }
        } // Drop stmts here so tx can be moved

        tx.commit()
            .with_context(|| "Failed to commit transaction")?;
        Ok(())
    }

    /// Get cached results for a category (unchanged files from previous scan)
    /// Uses category-specific scan ID
    pub fn get_cached_category(
        &self,
        category: &str,
        category_scan_id: i64,
    ) -> Result<Vec<PathBuf>> {
        // Query from file_categories junction table (schema v3+)
        // Falls back to file_records for backward compatibility
        let mut stmt = self.db.prepare(
            "SELECT fc.path FROM file_categories fc
             WHERE fc.category = ?1 AND fc.category_scan_id = ?2
             UNION
             SELECT fr.path FROM file_records fr
             WHERE fr.category = ?1 AND fr.last_scan_id = ?2
             AND NOT EXISTS (
                 SELECT 1 FROM file_categories WHERE path = fr.path AND category = ?1
             )",
        )?;

        let mut paths = Vec::new();
        let rows = stmt.query_map(
            params![category, category_scan_id, category, category_scan_id],
            |row| Ok(decode_path(&row.get::<_, String>(0)?)),
        )?;

        for row in rows {
            paths.push(row?);
        }

        Ok(paths)
    }

    /// Remove entries for deleted files (files that were in cache but no longer exist)
    /// With per-category scan IDs, we check each category's previous scan
    pub fn cleanup_stale(&mut self, _current_scan_session_id: i64) -> Result<usize> {
        // Get all categories that have been scanned
        let categories: Vec<String> = {
            let mut stmt = self.db.prepare("SELECT category FROM category_scans")?;
            let mut categories = Vec::new();
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            for row in rows {
                categories.push(row?);
            }
            categories
        };

        if categories.is_empty() {
            return Ok(0);
        }

        let mut total_deleted = 0;

        // For each category, check files from its previous scan
        for category in categories {
            // Get the current category scan ID
            let current_category_scan_id: Option<i64> = self
                .db
                .query_row(
                    "SELECT scan_id FROM category_scans WHERE category = ?1",
                    [&category],
                    |row| row.get(0),
                )
                .ok();

            if let Some(current_id) = current_category_scan_id {
                if current_id <= 1 {
                    continue; // First scan, nothing to clean up
                }

                let previous_category_scan_id = current_id - 1;

                // Get paths from previous category scan using file_categories table
                let paths: Vec<String> = {
                    let mut stmt = self
                        .db
                        .prepare("SELECT path FROM file_categories WHERE category = ?1 AND category_scan_id = ?2")?;

                    let mut paths = Vec::new();
                    let rows = stmt
                        .query_map(params![&category, previous_category_scan_id], |row| {
                            row.get::<_, String>(0)
                        })?;

                    for row in rows {
                        paths.push(row?);
                    }
                    paths
                };

                if paths.is_empty() {
                    continue;
                }

                // Check which paths are deleted
                // Files that are in the recycle bin should be excluded from scan results
                // but kept in cache (they were cleaned, not truly deleted)
                // Files that are not in recycle bin and don't exist should be removed from cache
                let mut deleted_paths = Vec::new();
                for path_str in paths {
                    let path = decode_path(&path_str);
                    match std::fs::metadata(&path) {
                        Ok(_) => {
                            // File exists at original location - keep it
                        }
                        Err(err) => {
                            if err.kind() == io::ErrorKind::NotFound {
                                // File doesn't exist - check if it's in recycle bin
                                if is_in_recycle_bin(&path) {
                                    // File is in recycle bin - it was cleaned, so exclude from results
                                    // but don't remove from cache yet (user might restore it)
                                    // We'll mark it as deleted in the category but keep the record
                                    deleted_paths.push(path_str);
                                } else {
                                    // File is truly deleted (not in recycle bin) - remove from cache
                                    deleted_paths.push(path_str);
                                }
                            }
                        }
                    }
                }

                if !deleted_paths.is_empty() {
                    // Batch delete in a transaction
                    let tx = self
                        .db
                        .transaction()
                        .with_context(|| "Failed to start cleanup transaction")?;

                    // Delete from file_categories for this category
                    let mut delete_category_stmt = tx
                        .prepare("DELETE FROM file_categories WHERE path = ?1 AND category = ?2")?;
                    for path_str in &deleted_paths {
                        if let Err(e) = delete_category_stmt.execute([path_str, &category]) {
                            return Err(anyhow::anyhow!(
                                "Failed to delete stale category record: {}",
                                e
                            ));
                        }
                    }
                    drop(delete_category_stmt);

                    // Delete from file_records only if file has no remaining categories
                    // (file might be in multiple categories, so only delete if it's not in any category)
                    let mut delete_file_stmt = tx.prepare(
                        "DELETE FROM file_records 
                         WHERE path = ?1 
                         AND NOT EXISTS (SELECT 1 FROM file_categories WHERE file_categories.path = file_records.path)"
                    )?;
                    for path_str in &deleted_paths {
                        if let Err(e) = delete_file_stmt.execute([path_str]) {
                            return Err(anyhow::anyhow!(
                                "Failed to delete stale file record: {}",
                                e
                            ));
                        }
                    }
                    drop(delete_file_stmt);

                    total_deleted += deleted_paths.len();
                    tx.commit()
                        .with_context(|| "Failed to commit cleanup transaction")?;
                }
            }
        }

        Ok(total_deleted)
    }

    /// Finish scan session and cleanup
    pub fn finish_scan(&mut self, scan_id: i64, stats: ScanStats) -> Result<()> {
        let finished_at = Utc::now().timestamp();

        self.db.execute(
            "UPDATE scan_sessions SET
                finished_at = ?1,
                total_files = ?2,
                new_files = ?3,
                changed_files = ?4,
                removed_files = ?5
             WHERE id = ?6",
            params![
                finished_at,
                stats.total_files as i64,
                stats.new_files as i64,
                stats.changed_files as i64,
                stats.removed_files as i64,
                scan_id
            ],
        )?;

        self.current_scan_id = None;
        Ok(())
    }

    /// Best-effort non-blocking finish to avoid UI stalls.
    /// Returns true if finished synchronously, false if deferred.
    pub fn finish_scan_nonblocking(&mut self, scan_id: i64, stats: ScanStats) -> Result<bool> {
        // Avoid blocking on database locks at the tail end of a scan.
        let _ = self.db.busy_timeout(Duration::from_millis(0));
        let result = self.finish_scan(scan_id, stats);
        let _ = self
            .db
            .busy_timeout(Duration::from_secs(DB_BUSY_TIMEOUT_SECS));

        match result {
            Ok(()) => Ok(true),
            Err(e) => {
                if is_busy_error(&e) {
                    Ok(false)
                } else {
                    Err(e)
                }
            }
        }
    }

    /// Force full rescan (clear cache for categories)
    pub fn invalidate(&mut self, categories: Option<&[&str]>) -> Result<()> {
        if let Some(cats) = categories {
            if !cats.is_empty() {
                let placeholders = cats.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                // Build params vector manually
                let mut query_params: Vec<&dyn rusqlite::ToSql> = Vec::new();
                for cat in cats {
                    query_params.push(cat);
                }
                self.db.execute(
                    &format!(
                        "DELETE FROM file_records WHERE category IN ({})",
                        placeholders
                    ),
                    rusqlite::params_from_iter(query_params),
                )?;
            }
        } else {
            self.db.execute("DELETE FROM file_records", [])?;
        }
        Ok(())
    }

    /// Completely clear scan cache *and* scan history.
    ///
    /// This resets first-scan detection (get_previous_scan_id() becomes None)
    /// so the next scan will behave like a true first scan again.
    pub fn clear_all(&mut self) -> Result<()> {
        // File signatures
        self.db.execute("DELETE FROM file_records", [])?;
        // Scan history (used by get_previous_scan_id)
        self.db.execute("DELETE FROM scan_sessions", [])?;
        self.current_scan_id = None;
        Ok(())
    }

    /// Get the previous scan ID (for getting cached results)
    pub fn get_previous_scan_id(&self) -> Result<Option<i64>> {
        let result: Option<i64> = self
            .db
            .query_row(
                "SELECT MAX(id) FROM scan_sessions WHERE finished_at IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .ok();
        Ok(result)
    }

    /// Get the current scan ID for a category (increments if needed)
    /// Returns the scan ID to use for this category scan
    pub fn get_category_scan_id(&mut self, category: &str, scan_session_id: i64) -> Result<i64> {
        let now = Utc::now().timestamp();

        // Try to get existing category scan ID
        let current_scan_id: Option<i64> = self
            .db
            .query_row(
                "SELECT scan_id FROM category_scans WHERE category = ?1",
                [category],
                |row| row.get(0),
            )
            .ok();

        let category_scan_id = if let Some(id) = current_scan_id {
            // Category exists, increment scan ID
            let new_id = id + 1;
            self.db.execute(
                "UPDATE category_scans 
                 SET scan_id = ?1, last_scan_session_id = ?2, last_updated = ?3 
                 WHERE category = ?4",
                params![new_id, scan_session_id, now, category],
            )?;
            new_id
        } else {
            // First scan of this category, start at 1
            self.db.execute(
                "INSERT INTO category_scans (category, scan_id, last_scan_session_id, last_updated)
                 VALUES (?1, 1, ?2, ?3)",
                params![category, scan_session_id, now],
            )?;
            1
        };

        Ok(category_scan_id)
    }

    /// Get the previous scan ID for a category (without incrementing)
    /// Returns None if the category was never scanned before
    pub fn get_previous_category_scan_id(&self, category: &str) -> Result<Option<i64>> {
        let result: Option<i64> = self
            .db
            .query_row(
                "SELECT scan_id FROM category_scans WHERE category = ?1",
                [category],
                |row| row.get(0),
            )
            .ok();
        Ok(result)
    }

    /// Get last scan info
    pub fn get_last_scan(&self) -> Result<Option<ScanSession>> {
        let result: Option<ScanSession> = self.db.query_row(
            "SELECT id, started_at, finished_at, scan_type, categories, total_files, new_files, changed_files, removed_files
             FROM scan_sessions
             ORDER BY id DESC LIMIT 1",
            [],
            |row| {
                let id: i64 = row.get(0)?;
                let started_at: i64 = row.get(1)?;
                let finished_at: Option<i64> = row.get(2)?;
                let scan_type: String = row.get(3)?;
                let categories_json: String = row.get(4)?;
                let total_files: Option<i64> = row.get(5)?;
                let new_files: Option<i64> = row.get(6)?;
                let changed_files: Option<i64> = row.get(7)?;
                let removed_files: Option<i64> = row.get(8)?;

                let categories: Vec<String> = serde_json::from_str(&categories_json).unwrap_or_default();

                Ok(ScanSession {
                    id,
                    started_at: DateTime::from_timestamp(started_at, 0)
                        .unwrap_or_else(Utc::now),
                    finished_at: finished_at.map(|ts| {
                        DateTime::from_timestamp(ts, 0).unwrap_or_else(Utc::now)
                    }),
                    scan_type,
                    categories,
                    stats: ScanStats {
                        total_files: total_files.unwrap_or(0) as usize,
                        new_files: new_files.unwrap_or(0) as usize,
                        changed_files: changed_files.unwrap_or(0) as usize,
                        removed_files: removed_files.unwrap_or(0) as usize,
                    },
                })
            },
        ).ok();

        Ok(result)
    }

    /// Get current scan ID
    pub fn current_scan_id(&self) -> Option<i64> {
        self.current_scan_id
    }

    /// Get cache statistics: total files and total storage scanned
    pub fn get_cache_stats(&self) -> Result<(usize, u64)> {
        let total_files: i64 =
            self.db
                .query_row("SELECT COUNT(*) FROM file_records", [], |row| row.get(0))?;

        let total_storage: i64 = self.db.query_row(
            "SELECT COALESCE(SUM(size), 0) FROM file_records",
            [],
            |row| row.get(0),
        )?;

        Ok((total_files as usize, total_storage as u64))
    }
}

fn is_busy_error(err: &anyhow::Error) -> bool {
    match err.downcast_ref::<rusqlite::Error>() {
        Some(rusqlite::Error::SqliteFailure(code, _))
            if matches!(
                code.code,
                ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked
            ) =>
        {
            true
        }
        _ => false,
    }
}

/// Get cache directory path
fn get_cache_dir() -> Result<PathBuf> {
    let base_dir = if cfg!(windows) {
        std::env::var("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                std::env::var("USERPROFILE")
                    .map(|p| PathBuf::from(p).join("AppData").join("Local"))
                    .unwrap_or_else(|_| PathBuf::from("."))
            })
    } else {
        std::env::var("HOME")
            .map(|h| PathBuf::from(h).join(".local").join("share"))
            .unwrap_or_else(|_| PathBuf::from("."))
    };

    Ok(base_dir.join("wole").join("cache"))
}

/// Normalize path for consistent storage and lookup
/// On Windows, converts to lowercase for case-insensitive matching
/// On Unix, preserves case
fn normalize_path(path: &Path) -> String {
    #[cfg(windows)]
    {
        // Single-pass normalizer: lowercase and convert \ to / in one pass.
        //
        // NOTE: use Unicode lowercasing (not ASCII-only) to preserve previous behavior.
        let path_str = path.to_string_lossy();
        let mut result = String::with_capacity(path_str.len());
        for ch in path_str.chars() {
            if ch == '\\' {
                result.push('/');
            } else {
                // `to_lowercase()` can expand into multiple chars (e.g. ß -> ss).
                result.extend(ch.to_lowercase());
            }
        }
        result
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::ffi::OsStrExt;

        let bytes = path.as_os_str().as_bytes();
        match std::str::from_utf8(bytes) {
            Ok(path_str) => path_str.to_string(),
            Err(_) => {
                let mut encoded = String::with_capacity(1 + bytes.len() * 2);
                encoded.push('\0');
                for byte in bytes {
                    use std::fmt::Write;
                    let _ = write!(&mut encoded, "{:02x}", byte);
                }
                encoded
            }
        }
    }
}

fn decode_path(value: &str) -> PathBuf {
    #[cfg(windows)]
    {
        PathBuf::from(value)
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::ffi::OsStringExt;

        if !value.starts_with('\0') {
            return PathBuf::from(value);
        }

        let hex = &value[1..];
        if hex.len() % 2 != 0 {
            return PathBuf::from(value);
        }

        let mut bytes = Vec::with_capacity(hex.len() / 2);
        for pair in hex.as_bytes().chunks(2) {
            let pair_str = match std::str::from_utf8(pair) {
                Ok(s) => s,
                Err(_) => return PathBuf::from(value),
            };
            let byte = match u8::from_str_radix(pair_str, 16) {
                Ok(b) => b,
                Err(_) => return PathBuf::from(value),
            };
            bytes.push(byte);
        }

        PathBuf::from(std::ffi::OsString::from_vec(bytes))
    }
}

/// Convert SystemTime to (seconds, nanoseconds) tuple
fn system_time_to_secs_nsecs(time: SystemTime) -> (i64, i64) {
    match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => {
            let secs = duration.as_secs() as i64;
            let nsecs = duration.subsec_nanos() as i64;
            (secs, nsecs)
        }
        Err(_) => (0, 0), // Shouldn't happen for mtime, but handle gracefully
    }
}

fn clamp_size_to_i64(size: u64) -> i64 {
    if size > i64::MAX as u64 {
        i64::MAX
    } else {
        size as i64
    }
}

/// Check if a file path is in the recycle bin
/// Returns true if the file exists in the recycle bin, false otherwise
fn is_in_recycle_bin(path: &Path) -> bool {
    // Only check recycle bin on Windows (where trash_ops::list works)
    #[cfg(windows)]
    {
        use crate::restore::normalize_path_for_comparison;

        // Try to get recycle bin contents (non-fatal if it fails)
        if let Ok(recycle_bin_items) = crate::trash_ops::list() {
            let path_str = path.display().to_string();
            let normalized_path = normalize_path_for_comparison(&path_str);

            // Check if any recycle bin item matches this path
            for item in recycle_bin_items {
                let original_path = item.original_parent.join(&item.name);
                let normalized_original =
                    normalize_path_for_comparison(&original_path.display().to_string());

                // Exact match
                if normalized_original == normalized_path {
                    return true;
                }

                // Check if path is inside a deleted directory
                // Windows Recycle Bin stores individual files when directories are deleted
                let normalized_original_with_sep = if normalized_original.ends_with('/') {
                    normalized_original.clone()
                } else {
                    format!("{}/", normalized_original)
                };

                if normalized_path.starts_with(&normalized_original_with_sep) {
                    return true;
                }
            }
        }
    }

    #[cfg(not(windows))]
    {
        // On non-Windows, check using trash crate if available
        if let Ok(recycle_bin_items) = crate::trash_ops::list() {
            let path_str = path.display().to_string();

            for item in recycle_bin_items {
                let original_path = item.original_parent.join(&item.name);
                if original_path == path {
                    return true;
                }
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_test_cache() -> (TempDir, ScanCache) {
        let temp_dir = TempDir::new().unwrap();
        std::env::set_var("LOCALAPPDATA", temp_dir.path());
        let cache = ScanCache::open().unwrap();
        (temp_dir, cache)
    }

    #[test]
    fn test_open_cache() {
        let (_temp_dir, _cache) = setup_test_cache();
    }

    #[test]
    fn test_start_scan() {
        let (_temp_dir, mut cache) = setup_test_cache();
        let scan_id = cache.start_scan("full", &["cache", "temp"]).unwrap();
        assert!(scan_id > 0);
    }

    #[test]
    fn test_upsert_file() {
        let (temp_dir, mut cache) = setup_test_cache();
        let scan_id = cache.start_scan("full", &["cache"]).unwrap();

        let test_file = temp_dir.path().join("test.txt");
        fs::write(&test_file, "hello").unwrap();

        let sig = FileSignature::from_path(&test_file, false).unwrap();
        cache.upsert_file(&sig, "cache", scan_id).unwrap();

        // Check that file is in cache
        let status = cache.check_file(&test_file).unwrap();
        assert!(matches!(status, FileStatus::Unchanged));
    }

    #[test]
    fn test_check_file_new() {
        let (temp_dir, cache) = setup_test_cache();

        let test_file = temp_dir.path().join("new_file.txt");
        fs::write(&test_file, "new content").unwrap();

        let status = cache.check_file(&test_file).unwrap();
        assert!(matches!(status, FileStatus::New));
    }

    #[test]
    fn test_check_file_modified() {
        let (temp_dir, mut cache) = setup_test_cache();
        let scan_id = cache.start_scan("full", &["cache"]).unwrap();

        let test_file = temp_dir.path().join("test.txt");
        fs::write(&test_file, "original").unwrap();

        let sig = FileSignature::from_path(&test_file, false).unwrap();
        cache.upsert_file(&sig, "cache", scan_id).unwrap();

        // Modify file
        std::thread::sleep(std::time::Duration::from_millis(10));
        fs::write(&test_file, "modified").unwrap();

        let status = cache.check_file(&test_file).unwrap();
        assert!(matches!(status, FileStatus::Modified));
    }

    #[test]
    fn test_invalidate() {
        let (temp_dir, mut cache) = setup_test_cache();
        let scan_id = cache.start_scan("full", &["cache"]).unwrap();

        let test_file = temp_dir.path().join("test.txt");
        fs::write(&test_file, "hello").unwrap();

        let sig = FileSignature::from_path(&test_file, false).unwrap();
        cache.upsert_file(&sig, "cache", scan_id).unwrap();

        // Invalidate cache
        cache.invalidate(Some(&["cache"])).unwrap();

        // File should be gone from cache
        let status = cache.check_file(&test_file).unwrap();
        assert!(matches!(status, FileStatus::New));
    }
}
