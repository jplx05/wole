# sweeper Windows Installer
# Downloads and installs sweeper from GitHub releases

$ErrorActionPreference = "Stop"

$REPO = "jpaulpoliquit/sweeper"

# Detect architecture
$ARCH = $env:PROCESSOR_ARCHITECTURE
if ([string]::IsNullOrEmpty($ARCH)) {
    # Fallback: check if system is 64-bit
    if ([System.Environment]::Is64BitOperatingSystem) {
        $ARCH = "x86_64"
    } else {
        Write-Error "Unsupported architecture: Unable to detect system architecture"
        exit 1
    }
} elseif ($ARCH -eq "AMD64" -or $ARCH -eq "x64") {
    $ARCH = "x86_64"
} elseif ($ARCH -eq "ARM64") {
    $ARCH = "arm64"
} else {
    Write-Warning "Unknown architecture '$ARCH', defaulting to x86_64"
    $ARCH = "x86_64"
}

# Windows only has x86_64 builds for now
if ($ARCH -ne "x86_64") {
    Write-Warning "Only x86_64 builds are available. Attempting x86_64..."
    $ARCH = "x86_64"
}

$ASSET = "sweeper-windows-${ARCH}.zip"
$URL = "https://github.com/${REPO}/releases/latest/download/${ASSET}"

Write-Host "Downloading sweeper for Windows-${ARCH}..." -ForegroundColor Cyan

# Create temp directory
$TEMP_DIR = Join-Path $env:TEMP "sweeper-install"
New-Item -ItemType Directory -Force -Path $TEMP_DIR | Out-Null

try {
    # Download the release
    $ZIP_PATH = Join-Path $TEMP_DIR "sweeper.zip"
    Write-Host "Downloading from $URL..." -ForegroundColor Gray
    Invoke-WebRequest -Uri $URL -OutFile $ZIP_PATH -UseBasicParsing
    
    # Extract
    Write-Host "Extracting..." -ForegroundColor Gray
    Expand-Archive -Path $ZIP_PATH -DestinationPath $TEMP_DIR -Force
    
    # Find the executable (could be sweeper.exe or sweeper-windows-x86_64.exe)
    $EXE_NAME = "sweeper.exe"
    $EXE_PATH = Join-Path $TEMP_DIR $EXE_NAME
    
    # If not found, look for any .exe in the extracted folder
    if (-not (Test-Path $EXE_PATH)) {
        $EXE_PATH = Get-ChildItem -Path $TEMP_DIR -Filter "*.exe" -Recurse | Select-Object -First 1 -ExpandProperty FullName
        if (-not $EXE_PATH) {
            Write-Error "Could not find sweeper.exe in downloaded archive"
            exit 1
        }
    }
    
    # Determine install location
    $INSTALL_DIR = ""
    $NEEDS_ADMIN = $false
    
    # Try Program Files first (requires admin)
    $PROGRAM_FILES = ${env:ProgramFiles}
    if (Test-Path $PROGRAM_FILES) {
        $INSTALL_DIR = Join-Path $PROGRAM_FILES "sweeper"
        $NEEDS_ADMIN = $true
    } else {
        # Fall back to user directory
        $INSTALL_DIR = Join-Path $env:LOCALAPPDATA "sweeper\bin"
    }
    
    # Create install directory
    New-Item -ItemType Directory -Force -Path $INSTALL_DIR | Out-Null
    
    # Copy executable
    $TARGET_PATH = Join-Path $INSTALL_DIR $EXE_NAME
    Copy-Item -Path $EXE_PATH -Destination $TARGET_PATH -Force
    
    Write-Host "Installed to $TARGET_PATH" -ForegroundColor Green
    
    # Add to PATH
    $CURRENT_PATH = [Environment]::GetEnvironmentVariable("Path", "User")
    $INSTALL_DIR_NORMALIZED = $INSTALL_DIR.TrimEnd('\')
    
    if ($CURRENT_PATH -notlike "*$INSTALL_DIR_NORMALIZED*") {
        Write-Host "Adding to PATH..." -ForegroundColor Gray
        
        if ($NEEDS_ADMIN) {
            # Try to add to system PATH (requires admin)
            $SYSTEM_PATH = [Environment]::GetEnvironmentVariable("Path", "Machine")
            if ($SYSTEM_PATH -notlike "*$INSTALL_DIR_NORMALIZED*") {
                Write-Host "Adding to system PATH requires administrator privileges." -ForegroundColor Yellow
                Write-Host "You can add it manually or run this script as administrator." -ForegroundColor Yellow
                Write-Host ""
                Write-Host "To add manually, run this in PowerShell as Administrator:" -ForegroundColor Cyan
                Write-Host "  [Environment]::SetEnvironmentVariable(`"Path`", `"$SYSTEM_PATH;$INSTALL_DIR_NORMALIZED`", `"Machine`")" -ForegroundColor White
            }
        } else {
            # Add to user PATH
            $NEW_PATH = "$CURRENT_PATH;$INSTALL_DIR_NORMALIZED"
            [Environment]::SetEnvironmentVariable("Path", $NEW_PATH, "User")
            Write-Host "Added $INSTALL_DIR_NORMALIZED to user PATH" -ForegroundColor Green
        }
    } else {
        Write-Host "Already in PATH" -ForegroundColor Gray
    }
    
    Write-Host ""
    Write-Host "✓ sweeper installed successfully!" -ForegroundColor Green
    Write-Host ""
    
    if ($CURRENT_PATH -notlike "*$INSTALL_DIR_NORMALIZED*") {
        Write-Host "Note: Restart your terminal or run this to use sweeper immediately:" -ForegroundColor Yellow
        Write-Host ('  $env:Path += ";' + $INSTALL_DIR_NORMALIZED + '"') -ForegroundColor White
        Write-Host ""
    }
    
    Write-Host "Run 'sweeper --help' to get started." -ForegroundColor Cyan
    
} finally {
    # Cleanup
    Remove-Item -Path $TEMP_DIR -Recurse -Force -ErrorAction SilentlyContinue
}
