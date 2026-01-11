# Scan Cache Robustness Verification Prompt

## Task
Verify that the scan cache implementation (`src/scan_cache/`) is robust, uncorruptible, and handles edge cases gracefully. Focus on the recent robustness improvements made in commit `71b0d7f`.

## Key Areas to Verify

### 1. Database Corruption Recovery
- **Check**: `src/scan_cache/database.rs::open()` method
- **Verify**:
  - If schema initialization fails, does it attempt automatic recovery?
  - Does it backup corrupted databases before recreating?
  - Does recovery handle edge cases (permissions, disk full, etc.)?
  - Can the application continue functioning if cache is corrupted?

### 2. Transaction Safety
- **Check**: All batch operations and migrations
- **Verify**:
  - Are `upsert_files_batch()` and `cleanup_stale()` wrapped in transactions?
  - Are schema migrations atomic (all-or-nothing)?
  - Do transactions properly rollback on errors?
  - Are there any operations that should be transactional but aren't?

### 3. Integer Overflow Protection
- **Check**: File size handling in `upsert_file()` and `upsert_files_batch()`
- **Verify**:
  - Are `u64` file sizes properly converted to `i64` for SQLite storage?
  - Is there protection against files >9PB (i64::MAX)?
  - Does the code handle overflow gracefully without panicking?

### 4. Path Normalization
- **Check**: `normalize_path()` function and all path storage/lookup operations
- **Verify**:
  - Are paths normalized consistently before database storage?
  - Does Windows use case-insensitive normalization?
  - Does Unix preserve case sensitivity?
  - Are path lookups using normalized paths?
  - Test with: Unicode paths, paths with spaces, very long paths, network paths

### 5. Concurrency and Performance
- **Check**: Database connection setup in `open()`
- **Verify**:
  - Is WAL (Write-Ahead Logging) mode enabled?
  - Is busy timeout configured (should be 30 seconds)?
  - Can multiple readers access cache simultaneously?
  - Are there any race conditions in concurrent access scenarios?

### 6. Error Handling (Non-Fatal)
- **Check**: `src/scanner.rs` - cache integration points
- **Verify**:
  - Do cache failures allow scans to continue?
  - Are cache errors logged but not fatal?
  - Does `update_cache_with_results()` handle errors gracefully?
  - Does `finish_scan()` handle errors without breaking the scan?

### 7. Resource Management
- **Check**: Database statement lifecycle and transaction management
- **Verify**:
  - Are database statements properly scoped to avoid borrow checker issues?
  - Are transactions always committed or rolled back?
  - Are there any memory leaks or resource leaks?
  - Do queries properly handle iterator lifetimes?

### 8. Edge Case Handling
- **Check**: All database operations
- **Verify**:
  - Empty result sets (no files, no categories)
  - Missing file metadata (permission errors, deleted files)
  - Very large file counts (millions of files)
  - Concurrent scan sessions
  - Database locked scenarios
  - Disk full scenarios
  - Invalid UTF-8 in paths
  - Paths longer than SQLite TEXT limits

### 9. Query Safety
- **Check**: All SQL queries
- **Verify**:
  - Are all queries parameterized (no SQL injection)?
  - Are indexes used for performance-critical queries?
  - Are batch operations efficient (not N+1 queries)?
  - Do queries handle NULL values correctly?

### 10. Data Integrity
- **Check**: File existence and metadata validation
- **Verify**:
  - Does code verify file existence before caching?
  - Are permission errors handled gracefully?
  - Does stale cleanup verify files exist before deletion?
  - Are file signatures computed correctly even for edge cases (0-byte files, symlinks, etc.)?

## Testing Checklist

### Unit Tests
- [ ] Test database corruption recovery
- [ ] Test transaction rollback on errors
- [ ] Test integer overflow handling
- [ ] Test path normalization (Windows vs Unix)
- [ ] Test concurrent access scenarios
- [ ] Test empty result sets
- [ ] Test very large batches (>10,000 files)

### Integration Tests
- [ ] Test full scan with cache enabled
- [ ] Test incremental scan with cache
- [ ] Test cache invalidation
- [ ] Test cache with corrupted database
- [ ] Test cache with disk full scenario
- [ ] Test cache with permission errors

### Manual Verification
- [ ] Run `cargo test` - all tests pass
- [ ] Run `cargo clippy` - no warnings
- [ ] Run `cargo build --release` - compiles successfully
- [ ] Test with actual file system (create/delete/modify files)
- [ ] Test with very large number of files (>100k)
- [ ] Test concurrent scans (if possible)

## Code Review Checklist

### Safety
- [ ] No `unwrap()` calls that could panic
- [ ] All errors are handled with `Result` types
- [ ] No unsafe code blocks (except documented memory mapping)
- [ ] No integer overflows or underflows
- [ ] No use-after-free or double-free issues

### Correctness
- [ ] Database schema matches code expectations
- [ ] Migrations are idempotent (can run multiple times)
- [ ] Cache invalidation works correctly
- [ ] File signatures are computed correctly
- [ ] Path comparisons are correct (case sensitivity)

### Performance
- [ ] Batch operations are used where appropriate
- [ ] Indexes are used for lookups
- [ ] No N+1 query problems
- [ ] Transactions are used efficiently

### Maintainability
- [ ] Code is well-documented
- [ ] Error messages are helpful
- [ ] Functions have single responsibilities
- [ ] No code duplication

## Specific Code Locations to Review

1. **`src/scan_cache/database.rs`**:
   - `open()` - corruption recovery
   - `init_schema()` - schema initialization
   - `migrate_schema()` - transaction safety
   - `upsert_file()` / `upsert_files_batch()` - overflow protection, transactions
   - `check_file()` / `check_files_batch()` - path normalization
   - `cleanup_stale()` - transaction safety, resource management
   - `normalize_path()` - path normalization logic

2. **`src/scanner.rs`**:
   - `try_incremental_scan()` - error handling
   - `update_cache_with_results()` - non-fatal error handling
   - Cache integration points - graceful degradation

3. **`src/scan_cache/signature.rs`**:
   - `from_path()` - edge cases (0-byte files, permission errors)
   - `compute_hash()` - memory mapping safety

## Expected Outcomes

After verification, provide:
1. **Summary**: Overall assessment of robustness (Strong/Moderate/Weak)
2. **Issues Found**: List any bugs, edge cases, or potential improvements
3. **Test Results**: Results of running tests
4. **Recommendations**: Any additional improvements needed

## Success Criteria

The implementation is considered robust if:
- ✅ Handles database corruption gracefully
- ✅ All operations are transaction-safe
- ✅ No integer overflows possible
- ✅ Path normalization works correctly
- ✅ Cache failures don't break scans
- ✅ All edge cases are handled
- ✅ No resource leaks
- ✅ All tests pass
- ✅ Code compiles without warnings
