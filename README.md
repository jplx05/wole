# Sweep

A Windows-first developer cleanup tool that safely removes build artifacts and caches from inactive projects.

## Why Sweep?

Existing tools (kondo, npkill, BleachBit) lack **project activity awareness**. Sweep only cleans artifacts from projects you're not actively working on.

## Features

- **Git-aware** — Skips projects with recent commits or uncommitted changes
- **Multi-language** — Node, Rust, .NET, Python, Java
- **Windows-native** — Handles NuGet, npm/yarn/pnpm caches, VS artifacts
- **Safe by default** — Dry-run mode, Recycle Bin deletion, JSON manifests

## Installation

### Windows (PowerShell)

```powershell
# Download and install from GitHub releases
irm https://raw.githubusercontent.com/jpaulpoliquit/sweeper/main/install.ps1 | iex
```

Or download `install.ps1` and run:
```powershell
.\install.ps1
```

### Windows (Batch)

```cmd
# Download install.bat and run
install.bat
```

### Windows (Git Bash / MINGW64)

```bash
# Download install.sh and run
./install.sh
```

### Manual Installation

1. Download the latest release from [GitHub Releases](https://github.com/jpaulpoliquit/sweeper/releases)
2. Extract `sweeper.exe` to a directory in your PATH (e.g., `%LOCALAPPDATA%\sweeper\bin`)
3. Add that directory to your PATH environment variable

## Building from Source

### Prerequisites

- [Rust](https://rustup.rs/) (latest stable)
- Visual Studio Build Tools with "C++ build tools" workload (for Windows MSVC target)

### Build Instructions

**PowerShell (Recommended):**
```powershell
# Debug build
.\build\build.ps1

# Release build
.\build\build.ps1 -Release
```

**Command Prompt:**
```cmd
cargo build
cargo build --release
```

**Git Bash:**
```bash
# Use the build script (recommended - handles PATH automatically)
./build/build.sh

# Or manually fix PATH first, then build
export PATH=$(echo "$PATH" | tr ':' '\n' | grep -v "Git/usr/bin" | grep -v "Git/cmd" | grep -v "Git/mingw64/bin" | tr '\n' ':' | sed 's/:$//')
unalias link 2>/dev/null || true
cargo build
```

**⚠️ Important:** If you get `link: extra operand` errors in Git Bash, use PowerShell instead:
```powershell
.\build\build.ps1
```

**Note:** If you encounter linker errors in Git Bash (`link: extra operand`), use PowerShell or CMD instead. See [build/BUILD_TROUBLESHOOTING.md](build/BUILD_TROUBLESHOOTING.md) for details.

### Build Output

- Debug: `target\debug\sweeper.exe`
- Release: `target\release\sweeper.exe`

## Quick Start

```bash
# Scan for reclaimable space
sweeper scan --all

# Scan specific categories
sweeper scan --build --cache --temp

# Preview what would be deleted
sweeper scan --build --cache

# Detailed analysis with file lists
sweeper analyze --all

# Clean inactive projects (with confirmation)
sweeper clean --build --cache

# Clean without confirmation
sweeper clean --build --cache -y

# Dry run (preview without deleting)
sweeper clean --all --dry-run

# Permanent delete (bypass Recycle Bin)
sweeper clean --temp --permanent -y

# Exclude specific paths
sweeper scan --all --exclude "**/important-project/**"
```

## Commands

| Command | Description |
|---------|-------------|
| `scan` | Find cleanable files (safe, dry-run by default) |
| `clean` | Delete files found by scan (with confirmation) |
| `analyze` | Show detailed breakdown with file lists and statistics |
| `config` | View or modify configuration settings |

## Categories

| Flag | Targets |
|------|---------|
| `--build` | Build artifacts from inactive projects (`node_modules`, `target/`, `bin/obj`, `dist/`, `__pycache__`, etc.) |
| `--cache` | npm/yarn/pnpm, NuGet, Cargo, pip caches |
| `--temp` | Windows temp directories (files older than 1 day) |
| `--trash` | Recycle Bin contents |
| `--downloads` | Old files in Downloads folder (default: 30+ days) |
| `--large` | Files over size threshold (default: 100MB) |
| `--old` | Files not accessed in N days (default: 30+ days) |

## Options

### Common Options

- `--all`, `-a` - Enable all scan categories at once
- `--exclude <PATTERN>` - Exclude paths matching pattern (repeatable)
- `--path <PATH>` - Root path to scan (default: home directory)
- `--json` - Output results as JSON for scripting
- `-v`, `-vv` - Increase verbosity
- `-q` - Quiet mode (errors only)

### Scan Options

- `--project-age <DAYS>` - Project inactivity threshold (default: 14 days)
- `--min-age <DAYS>` - Minimum file age for downloads/old (default: 30 days)
- `--min-size <SIZE>` - Minimum file size for large files (default: 100MB)

### Clean Options

- `-y`, `--yes` - Skip confirmation prompt
- `--permanent` - Permanently delete (bypass Recycle Bin)
- `--dry-run` - Preview only, don't delete

## Configuration

Config file: `%APPDATA%\sweeper\config.toml`

```toml
[thresholds]
project_age_days = 14
min_age_days = 30
min_size_mb = 100

[exclusions]
patterns = [
    "**/important-project/**",
    "**/backup/**"
]
```

View or modify configuration:

```bash
# Show current configuration
sweeper config --show

# Reset to defaults
sweeper config --reset

# Open config file in editor
sweeper config --edit
```

## Troubleshooting

If you encounter build issues, see [build/BUILD_TROUBLESHOOTING.md](build/BUILD_TROUBLESHOOTING.md) for common solutions.

## License

[MIT](LICENSE)
