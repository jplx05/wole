# Disk Insights Cache Implementation Plan

## Decision: Option 1 - Simple File-Based Cache

### Rationale
- **Different use case**: Disk insights caches folder tree structures, while scan_cache tracks individual files
- **Simplicity**: Just serialize/deserialize `DiskInsights` - no database schema changes
- **Performance**: File-based cache is faster for read-heavy operations
- **Independence**: Won't affect existing scan_cache functionality
- **Easy invalidation**: Check root directory mtime

### Implementation Steps

1. **Add Serialization Support**
   - Add `Serialize` and `Deserialize` derives to `DiskInsights`, `FolderNode`, and `FileInfo`
   - Use `serde_json` for human-readable format (or `bincode` for compact binary)

2. **Create Cache Module** (`src/disk_usage_cache.rs`)
   - Cache key: `{normalized_path}_{depth}_{root_mtime_secs}`
   - Cache file: `{cache_dir}/disk_insights/{hash}.json`
   - Functions:
     - `get_cache_key(path: &Path, depth: u8) -> Result<String>`
     - `load_cached_insights(key: &str) -> Result<Option<DiskInsights>>`
     - `save_cached_insights(key: &str, insights: &DiskInsights) -> Result<()>`
     - `invalidate_cache(path: &Path) -> Result<()>` (optional cleanup)

3. **Integration Points**
   - Modify `scan_directory_with_progress()` to check cache first
   - Save results after successful scan
   - Invalidate if root directory mtime changed

4. **Cache Invalidation Strategy**
   - Check root directory mtime before loading cache
   - If mtime changed, skip cache and rescan
   - Optional: TTL-based expiration (e.g., 24 hours)

### Cache Key Format
```
{normalized_path}_{depth}_{mtime_secs}
Example: "C__Users_jhppo_Documents_3_1234567890"
```

### File Structure
```
LOCALAPPDATA/wole/cache/disk_insights/
  ├── {hash1}.json
  ├── {hash2}.json
  └── ...
```

### Benefits
- ✅ Fast cache hits (just deserialize JSON)
- ✅ Simple to implement and maintain
- ✅ Easy to debug (human-readable JSON)
- ✅ Independent from scan_cache
- ✅ No database overhead

### Future Enhancements
- Add TTL expiration
- Cache cleanup (remove old/stale entries)
- Compression for large cache files
- Cache statistics/metrics
