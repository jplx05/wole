# Security Audit Report

**Status:** PASSED | **Risk Level:** LOW | **Version:** 0.1.0 (2026-01-10)

---

## Audit Overview


| Attribute        | Details                                            |
| ---------------- | -------------------------------------------------- |
| Audit Date       | January 10, 2026                                   |
| Audit Conclusion | **PASSED**                                         |
| Sweep Version    | v0.1.0                                             |
| Audited Branch   | `main` (HEAD)                                      |
| Scope            | Rust binaries, PowerShell installer, Configuration |
| Methodology      | Static analysis, Threat modeling, Code review      |
| Review Cycle     | Every 6 months or after major feature additions    |
| Next Review      | July 2026                                          |


**Key Findings:**

- Recycle Bin deletion by default ensures recoverability for most operations.
- Project activity detection prevents accidental deletion of active development files.
- Symlink and junction protection prevents infinite loops and system traversal.
- Long path support (>260 chars) handles deep `node_modules` safely.
- Dry-run mode allows safe preview of all operations.
- Comprehensive deletion logging provides audit trails.
- No admin privileges required for standard operations.

---

## Security Philosophy

**Core Principle: "Safe by Default"**

Sweep is built with a **defensive-first** architecture for filesystem operations. Every deletion defaults to the Recycle Bin, requiring explicit opt-in for permanent deletion.

**Guiding Priorities:**

1. **Recoverability First** - All deletions go to Recycle Bin by default; permanent delete requires `--permanent` flag.
2. **Project Awareness** - Build artifacts only cleaned from inactive projects (14+ days with no commits/activity).
3. **Fail Safe** - Skip locked files, permission errors, and symlinks instead of crashing.
4. **Full Transparency** - Deletion history logged with timestamps, paths, sizes, and categories.

---

## Threat Model

### Attack Vectors & Mitigations


| Threat                        | Risk Level | Mitigation                                               | Status      |
| ----------------------------- | ---------- | -------------------------------------------------------- | ----------- |
| Accidental User File Deletion | Critical   | Recycle Bin default, dry-run mode, confirmation prompts  | ✅ Mitigated |
| Active Project Deletion       | High       | 14-day project activity detection, git index monitoring  | ✅ Mitigated |
| Symlink/Junction Following    | High       | `should_skip_entry()` checks symlinks & reparse points   | ✅ Mitigated |
| Long Path Failures            | Medium     | `\\?\` prefix support via `to_long_path()` utility       | ✅ Mitigated |
| Locked File Crashes           | Medium     | `is_file_locked()` pre-flight check on Windows           | ✅ Mitigated |
| System Directory Deletion     | High       | `SYSTEM_DIRS` blocklist + `is_system_path()` validation  | ✅ Mitigated |
| Infinite Directory Loops      | High       | `MAX_DEPTH` limits (10-20) + symlink detection           | ✅ Mitigated |
| Race Conditions               | Medium     | Atomic operations, file existence checks before deletion | ✅ Mitigated |
| Command Injection             | Low        | No shell execution; Rust-native filesystem APIs          | ✅ Mitigated |
| Privilege Escalation          | Low        | No admin required; user-scoped operations only           | ✅ Mitigated |
| False Positive Deletion       | Medium     | Category-specific targeting, config exclusions           | ✅ Mitigated |


---

## Defense Architecture

### Multi-Layered Protection System

Sweep implements defense-in-depth with multiple validation layers:

#### Layer 1: System Path Protection

Hardcoded system directories are **unconditionally blocked** from traversal:

```rust
// src/utils.rs - SYSTEM_DIRS constant
pub const SYSTEM_DIRS: &[&str] = &[
    "Windows",
    "Program Files",
    "Program Files (x86)",
    "ProgramData",
    "$Recycle.Bin",
    "System Volume Information",
    "Recovery",
    "MSOCache",
];
```

**Enforcement:** `is_system_path()` checks all path components against this blocklist before any operation.

**Code:** `src/utils.rs:491-516`

#### Layer 2: Symlink & Junction Detection

Windows junction points and symlinks are detected and skipped to prevent:

- Infinite traversal loops
- Unintended system directory access
- OneDrive placeholder issues

```rust
// src/utils.rs - should_skip_entry()
pub fn should_skip_entry(path: &Path) -> bool {
    // Check for symlink via symlink_metadata
    if let Ok(meta) = std::fs::symlink_metadata(path) {
        if meta.file_type().is_symlink() {
            return true;
        }
    }
    // Check for Windows reparse points (junctions, OneDrive placeholders)
    is_windows_reparse_point(path)
}
```

**Code:** `src/utils.rs:130-163`

#### Layer 3: Project Activity Detection

Build artifacts are only cleaned from **inactive projects** to prevent deleting dependencies from active development:


| Check        | Indicator                               | Threshold |
| ------------ | --------------------------------------- | --------- |
| Git Index    | `.git/index` modification time          | 14 days   |
| Git HEAD     | `.git/HEAD` modification time           | 14 days   |
| Lock Files   | `package-lock.json`, `Cargo.lock`, etc. | 14 days   |
| Source Files | `.rs`, `.js`, `.ts`, `.py` modification | 14 days   |


**Code:** `src/project.rs:96-166`

#### Layer 4: Locked File Detection

On Windows, files locked by other processes are detected and skipped:

```rust
// src/cleaner.rs - is_file_locked()
fn is_file_locked(path: &Path) -> bool {
    match OpenOptions::new().read(true).write(true).open(path) {
        Ok(_) => false,
        Err(e) if e.raw_os_error() == Some(32) => true, // ERROR_SHARING_VIOLATION
        Err(_) => false,
    }
}
```

**Code:** `src/cleaner.rs:16-35`

#### Layer 5: Long Path Support

Windows long paths (>260 characters) are handled via the `\\?\` prefix:

```rust
// src/utils.rs - to_long_path()
pub fn to_long_path(path: &Path) -> PathBuf {
    // Add \\?\ prefix for extended-length paths
    PathBuf::from(format!(r"\\?\{}", absolute_path))
}
```

**Code:** `src/utils.rs:8-42`

---

## Safety Mechanisms

### Recycle Bin as Default

All deletions use the `trash` crate to move files to the Recycle Bin:


| Operation   | Default Behavior | Permanent Deletion          |
| ----------- | ---------------- | --------------------------- |
| CLI Clean   | Recycle Bin      | `--permanent` flag required |
| TUI Clean   | Recycle Bin      | `[P]` key required          |
| Batch Clean | Recycle Bin      | Explicit parameter          |


**Code:** `src/cleaner.rs:718-742`

### Confirmation Requirements


| Scenario           | Confirmation Required         |
| ------------------ | ----------------------------- |
| Any deletion       | Yes (unless `-y` flag)        |
| Permanent deletion | Double confirmation in TUI    |
| Large operations   | Item count and size displayed |


**Code:** `src/cleaner.rs:94-109`

### Conservative Scanning Logic

#### Project Inactivity Rule (14 Days)

Build artifacts (`node_modules`, `target/`, etc.) are only cleaned when:

1. **Project marker exists** - `package.json`, `Cargo.toml`, etc.
2. **No recent activity** - Git index, lock files, and source files unchanged for 14+ days
3. **No uncommitted changes** - Git status is clean (when git2 available)

**Code:** `src/categories/build.rs:69-130`

#### File Age Thresholds


| Category    | Default Threshold | Purpose                          |
| ----------- | ----------------- | -------------------------------- |
| Downloads   | 30 days           | Old files in Downloads folder    |
| Old Files   | 30 days           | Unused files in user directories |
| Large Files | 100 MB            | Files over size threshold        |
| Temp Files  | 1 day             | System temp directories          |


**Code:** `src/config.rs:337-345`

### Skip Lists

Categories skip known important directories during traversal:

```rust
// Skipped during traversal in all scanners
"node_modules", ".git", ".hg", ".svn", "target", ".gradle",
"__pycache__", ".venv", "venv", ".next", ".nuxt", ".turbo",
".parcel-cache", "$recycle.bin", "system volume information",
"windows", "program files", "program files (x86)", "appdata", "programdata"
```

**Code:** Multiple category files (e.g., `src/categories/large.rs:113-136`)

---

## User Controls

### Dry-Run Mode

**Command:** `wole clean --dry-run` | `wole scan --all`

**Behavior:**

- `scan` command is always dry-run (never deletes)
- `clean --dry-run` simulates deletion without modifying files
- Shows exact files that would be deleted with sizes

### Custom Exclusions

**Config File:** `%APPDATA%\wole\config.toml`

```toml
[exclusions]
patterns = [
    "**/important-project/**",
    "**/backup/**",
    "**/client-work/**"
]
```

**Features:**

- Glob pattern support (`**` for recursive, `*` for wildcard)
- Case-insensitive matching on Windows
- Applied during traversal (not post-processing) for efficiency

**Code:** `src/config.rs:460-481`

### Safety Settings

```toml
[safety]
always_confirm = false      # Require confirmation even with -y
default_permanent = false   # Use Recycle Bin by default
max_no_confirm = 10         # Max items without confirmation
max_size_no_confirm_mb = 100  # Max size (MB) without confirmation
skip_locked_files = true    # Skip files in use
dry_run_default = false     # Run in preview mode by default
```

**Code:** `src/config.rs:87-112`

### Restore Functionality

Files deleted to Recycle Bin can be restored:

```bash
wole restore --last      # Restore from last deletion session
wole restore --path "C:\path\to\file"  # Restore specific file
wole restore --from log.json  # Restore from specific log
```

**Code:** `src/restore.rs`

---

## Audit Trail & Logging

### Deletion History

Every deletion operation is logged:

```json
{
  "session_start": 1736524800,
  "records": [
    {
      "timestamp": 1736524801,
      "path": "C:\\Users\\name\\Downloads\\old-file.zip",
      "size_bytes": 104857600,
      "category": "downloads",
      "permanent": false,
      "success": true,
      "error": null
    }
  ],
  "total_bytes_cleaned": 104857600,
  "total_items": 1,
  "errors": 0
}
```

**Location:** `%LOCALAPPDATA%\wole\history\cleanup_YYYYMMDD_HHMMSS.json`

**Code:** `src/history.rs`

---

## Testing & Compliance

### Test Coverage

Sweep uses Rust's built-in test framework with integration tests.


| Test Category      | Key Tests                                                                   |
| ------------------ | --------------------------------------------------------------------------- |
| Long Path Support  | `test_long_path_conversion`, `test_safe_metadata_on_regular_file`           |
| Symlink Protection | `test_should_skip_entry_regular_dir`, `test_should_skip_entry_regular_file` |
| History Logging    | `test_deletion_log_creation`, `test_deletion_log_add_success/failure`       |
| Config Exclusions  | `test_config_exclusion_filtering`, `test_exclusion_patterns`                |
| Directory Size     | `test_calculate_dir_size_empty`, `test_calculate_dir_size_with_files`       |
| Scanner            | `test_scan_all_no_categories`, `test_scan_empty_directory`                  |


**Test Execution:**

```bash
cargo test                    # Run all tests
cargo test --test integration_tests  # Run integration tests
```

**Code:** `tests/integration_tests.rs`, unit tests in each module

### Static Analysis


| Tool           | Purpose                        | Status           |
| -------------- | ------------------------------ | ---------------- |
| `cargo clippy` | Lint checks with `-D warnings` | ✅ Enforced in CI |
| `cargo fmt`    | Code formatting                | ✅ Enforced in CI |
| `cargo check`  | Type checking                  | ✅ Enforced in CI |


**Code:** `.github/workflows/build-release.yml:46-53`

### Compliance Standards


| Standard                | Implementation                                      |
| ----------------------- | --------------------------------------------------- |
| CWE-22 (Path Traversal) | Symlink/junction detection, system path blocklist   |
| CWE-59 (Link Following) | `should_skip_entry()` before traversal              |
| CWE-367 (TOCTOU Race)   | Atomic operations, existence checks before deletion |
| CWE-732 (Permissions)   | User-scoped operations only, no admin required      |


---

## Dependencies

### Rust Crate Dependencies

All dependencies are pinned in `Cargo.lock` and vetted for security:


| Crate     | Version | Purpose                                 | License        |
| --------- | ------- | --------------------------------------- | -------------- |
| `trash`   | 5.0     | Recycle Bin operations                  | MIT            |
| `walkdir` | 2.4     | Directory traversal                     | MIT/Unlicense  |
| `jwalk`   | 0.8     | Parallel directory traversal            | MIT            |
| `blake3`  | 1.5     | Fast cryptographic hashing (duplicates) | CC0/Apache-2.0 |
| `clap`    | 4.5     | CLI argument parsing                    | MIT/Apache-2.0 |
| `ratatui` | 0.29    | Terminal UI framework                   | MIT            |
| `chrono`  | 0.4     | Date/time handling                      | MIT/Apache-2.0 |
| `serde`   | 1.0     | Serialization                           | MIT/Apache-2.0 |
| `globset` | 0.4     | Glob pattern matching                   | MIT/Unlicense  |
| `memmap2` | 0.9     | Memory-mapped file I/O                  | MIT/Apache-2.0 |
| `rayon`   | 1.10    | Parallel iteration                      | MIT/Apache-2.0 |


**Supply Chain Security:**

- All dependencies pinned to specific versions in `Cargo.lock`
- No pre-compiled binaries in repository
- Automated builds via GitHub Actions
- Multi-architecture support (x86_64, ARM64, i686)

**Code:** `Cargo.toml:15-38`

### Known Limitations


| Limitation                    | Impact                    | Mitigation                          |
| ----------------------------- | ------------------------- | ----------------------------------- |
| git2 removed                  | No git dirty detection    | File-based git index checking       |
| No undo for permanent delete  | Unrecoverable             | Clear warnings, Recycle Bin default |
| 14-day rule may delay cleanup | Orphaned artifacts remain | Manual `--project-age 0` override   |
| Windows-focused               | Limited Unix support      | Cross-platform utilities            |


---

## Installer Security

### PowerShell Installer (`install.ps1`)

**Security Features:**

1. **HTTPS Download** - Uses GitHub releases API over HTTPS
2. **User Scope** - Installs to `%LOCALAPPDATA%\wole\bin` (no admin)
3. **PATH Modification** - User PATH only, not system PATH
4. **Cleanup** - Temporary files removed after installation
5. **Verification** - Binary existence verified after install

**Code:** `install.ps1`

---

## Intentionally Out of Scope (Safety)

The following are **never** targeted for deletion:

- ❌ User documents (Documents folder contents)
- ❌ System files (`C:\Windows\*`)
- ❌ Program installations (`C:\Program Files\*`)
- ❌ Browser history or cookies (only cache files)
- ❌ Git repositories (only build artifacts inside them)
- ❌ Encryption keys or password managers
- ❌ Active project dependencies

---

## Recommendations

### For Users

1. **Always run `scan` first** to preview what will be cleaned
2. **Use `--exclude` patterns** for important directories
3. **Keep `default_permanent = false**` for Recycle Bin safety
4. **Review deletion logs** in `%LOCALAPPDATA%\wole\history\`

### For Contributors

1. **Add new categories carefully** - ensure system path checks
2. **Test on fresh Windows installs** - avoid environment-specific assumptions
3. **Maintain symlink protection** - always use `should_skip_entry()`
4. **Document threshold changes** - update security audit when defaults change

---

**Commitment:** This audit certifies that Sweep implements defense-in-depth security practices and prioritizes user data safety above all else. We default to non-destructive operations and require explicit user action for permanent deletions.

*For security concerns or vulnerability reports, please open an issue at [https://github.com/jpaulpoliquit/wole/issues*](https://github.com/jpaulpoliquit/wole/issues)

---

*Last Updated: January , 202509*