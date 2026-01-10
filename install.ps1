# wole Windows Installer
# Downloads and installs wole from GitHub releases

$ErrorActionPreference = "Stop"

$REPO = "jpaulpoliquit/wole"

# Detect architecture
# Check PROCESSOR_ARCHITECTURE environment variable first
$ARCH = $env:PROCESSOR_ARCHITECTURE

# Also check PROCESSOR_ARCHITEW6432 for 32-bit processes on 64-bit systems
if ([string]::IsNullOrEmpty($ARCH) -or $ARCH -eq "x86") {
    $ARCH = $env:PROCESSOR_ARCHITEW6432
}

# Determine architecture
if ([string]::IsNullOrEmpty($ARCH)) {
    # Fallback: check if system is 64-bit
    if ([System.Environment]::Is64BitOperatingSystem) {
        # Check if ARM64 (try RuntimeInformation first, fallback to env var)
        $isArm64 = $false
        try {
            # Try to use RuntimeInformation if available (.NET Core/.NET 5+ or .NET Framework 4.7.1+)
            $procArch = [System.Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture
            if ($procArch -eq [System.Runtime.InteropServices.Architecture]::Arm64) {
                $isArm64 = $true
            }
        } catch {
            # RuntimeInformation not available (older .NET), check env var
            if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64" -or $env:PROCESSOR_ARCHITEW6432 -eq "ARM64") {
                $isArm64 = $true
            }
        }
        
        if ($isArm64) {
            $ARCH = "arm64"
        } else {
            $ARCH = "x86_64"
        }
    } else {
        # 32-bit system
        $ARCH = "i686"
    }
} elseif ($ARCH -eq "AMD64" -or $ARCH -eq "x64") {
    $ARCH = "x86_64"
} elseif ($ARCH -eq "ARM64" -or $ARCH -eq "arm64") {
    $ARCH = "arm64"
} elseif ($ARCH -eq "x86" -or $ARCH -eq "X86") {
    # Could be 32-bit system or 32-bit process on 64-bit system
    # Check if running on 64-bit OS
    if ([System.Environment]::Is64BitOperatingSystem) {
        # 32-bit process on 64-bit system - check actual architecture
        $isArm64 = $false
        try {
            # Try to use RuntimeInformation if available
            $procArch = [System.Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture
            if ($procArch -eq [System.Runtime.InteropServices.Architecture]::Arm64) {
                $isArm64 = $true
            }
        } catch {
            # RuntimeInformation not available, check env var
            if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64" -or $env:PROCESSOR_ARCHITEW6432 -eq "ARM64") {
                $isArm64 = $true
            }
        }
        
        if ($isArm64) {
            $ARCH = "arm64"
        } else {
            $ARCH = "x86_64"
        }
    } else {
        # True 32-bit system
        $ARCH = "i686"
    }
} else {
    Write-Warning "Unknown architecture '$ARCH', defaulting to x86_64"
    $ARCH = "x86_64"
}

$ASSET = "wole-windows-${ARCH}.zip"
$URL = "https://github.com/${REPO}/releases/latest/download/${ASSET}"

Write-Host "Downloading wole for Windows-${ARCH}..." -ForegroundColor Cyan

# Create temp directory
$TEMP_DIR = Join-Path $env:TEMP "wole-install"
New-Item -ItemType Directory -Force -Path $TEMP_DIR | Out-Null

try {
    # Download the release
    $ZIP_PATH = Join-Path $TEMP_DIR "wole.zip"
    Write-Host "Downloading from $URL..." -ForegroundColor Gray
    Invoke-WebRequest -Uri $URL -OutFile $ZIP_PATH -UseBasicParsing
    
    # Extract
    Write-Host "Extracting..." -ForegroundColor Gray
    Expand-Archive -Path $ZIP_PATH -DestinationPath $TEMP_DIR -Force
    
    # Find the executable (could be wole.exe or wole-windows-x86_64.exe)
    $EXE_NAME = "wole.exe"
    $EXE_PATH = Join-Path $TEMP_DIR $EXE_NAME
    
    # If not found, look for any .exe in the extracted folder
    if (-not (Test-Path $EXE_PATH)) {
        $EXE_PATH = Get-ChildItem -Path $TEMP_DIR -Filter "*.exe" -Recurse | Select-Object -First 1 -ExpandProperty FullName
        if (-not $EXE_PATH) {
            Write-Error "Could not find wole.exe in downloaded archive"
            exit 1
        }
    }
    
    # Determine install location
    # Use user directory by default (no admin required)
    $INSTALL_DIR = Join-Path $env:LOCALAPPDATA "wole\bin"
    
    # Create install directory
    New-Item -ItemType Directory -Force -Path $INSTALL_DIR | Out-Null
    
    # Copy executable
    $TARGET_PATH = Join-Path $INSTALL_DIR $EXE_NAME
    Copy-Item -Path $EXE_PATH -Destination $TARGET_PATH -Force
    
    Write-Host "Installed to $TARGET_PATH" -ForegroundColor Green
    
    # Add to PATH
    try {
        $INSTALL_DIR_NORMALIZED = [System.IO.Path]::GetFullPath($INSTALL_DIR).TrimEnd('\', '/')
    } catch {
        $INSTALL_DIR_NORMALIZED = $INSTALL_DIR.TrimEnd('\', '/')
    }
    $CURRENT_PATH = [Environment]::GetEnvironmentVariable("Path", "User")
    
    # Check if already in PATH (case-insensitive, handle trailing slashes)
    $pathAlreadyAdded = $false
    if (-not [string]::IsNullOrWhiteSpace($CURRENT_PATH)) {
        $pathEntries = $CURRENT_PATH -split ';'
        foreach ($entry in $pathEntries) {
            $entryStr = [string]$entry
            if ([string]::IsNullOrWhiteSpace($entryStr)) {
                continue
            }
            try {
                $normalizedEntry = [System.IO.Path]::GetFullPath($entryStr.Trim()).TrimEnd('\', '/')
            } catch {
                # If GetFullPath fails (e.g., contains env vars), do simple comparison
                $normalizedEntry = $entryStr.Trim().TrimEnd('\', '/')
            }
            if ($normalizedEntry -eq $INSTALL_DIR_NORMALIZED) {
                $pathAlreadyAdded = $true
                break
            }
        }
    }
    
    if (-not $pathAlreadyAdded) {
        Write-Host "Adding to PATH..." -ForegroundColor Gray
        # Add to user PATH (no admin required)
        if ([string]::IsNullOrWhiteSpace($CURRENT_PATH)) {
            $NEW_PATH = $INSTALL_DIR_NORMALIZED
        } else {
            $NEW_PATH = "$CURRENT_PATH;$INSTALL_DIR_NORMALIZED"
        }
        [Environment]::SetEnvironmentVariable("Path", $NEW_PATH, "User")
        Write-Host "Added $INSTALL_DIR_NORMALIZED to user PATH" -ForegroundColor Green
    } else {
        Write-Host "Already in PATH" -ForegroundColor Gray
    }
    
    # Refresh PATH from registry first
    $machinePath = [System.Environment]::GetEnvironmentVariable("Path", "Machine")
    $userPath = [System.Environment]::GetEnvironmentVariable("Path", "User")
    
    $registryPathParts = @()
    if (-not [string]::IsNullOrWhiteSpace($machinePath)) {
        $registryPathParts += $machinePath
    }
    if (-not [string]::IsNullOrWhiteSpace($userPath)) {
        $registryPathParts += $userPath
    }
    
    if ($registryPathParts.Count -gt 0) {
        $registryPath = $registryPathParts -join ';'
        $env:Path = $registryPath
    }
    
    # Always ensure install directory is in current session PATH (double-check and add if needed)
    $currentSessionPath = $env:Path
    $inSessionPath = $false
    
    if (-not [string]::IsNullOrWhiteSpace($currentSessionPath)) {
        $sessionPathEntries = $currentSessionPath -split ';'
        foreach ($entry in $sessionPathEntries) {
            $entryStr = [string]$entry
            if ([string]::IsNullOrWhiteSpace($entryStr)) {
                continue
            }
            try {
                $normalizedEntry = [System.IO.Path]::GetFullPath($entryStr.Trim()).TrimEnd('\', '/')
            } catch {
                # If GetFullPath fails (e.g., contains env vars), do simple comparison
                $normalizedEntry = $entryStr.Trim().TrimEnd('\', '/')
            }
            if ($normalizedEntry -eq $INSTALL_DIR_NORMALIZED) {
                $inSessionPath = $true
                break
            }
        }
    }
    
    if (-not $inSessionPath) {
        # Add to current session PATH (guarantees immediate availability)
        if ([string]::IsNullOrWhiteSpace($currentSessionPath)) {
            $env:Path = $INSTALL_DIR_NORMALIZED
        } else {
            $env:Path = "$currentSessionPath;$INSTALL_DIR_NORMALIZED"
        }
    }
    
    # Verify installation
    $exeExists = Test-Path $TARGET_PATH
    $pathContainsDir = $null
    try {
        $pathEntries = $env:Path -split ';'
        foreach ($pathEntry in $pathEntries) {
            # Convert to string and validate
            $entryStr = [string]$pathEntry
            if ([string]::IsNullOrWhiteSpace($entryStr)) {
                continue
            }
            
            try {
                $trimmedEntry = $entryStr.Trim()
                $normalized = [System.IO.Path]::GetFullPath($trimmedEntry).TrimEnd('\', '/')
                if ($normalized -eq $INSTALL_DIR_NORMALIZED) {
                    $pathContainsDir = $true
                    break
                }
            } catch {
                # If GetFullPath fails, try simple string comparison
                try {
                    $trimmedEntry = $entryStr.Trim().TrimEnd('\', '/')
                    if ($trimmedEntry -eq $INSTALL_DIR_NORMALIZED) {
                        $pathContainsDir = $true
                        break
                    }
                } catch {
                    # Skip this entry if we can't process it
                    continue
                }
            }
        }
    } catch {
        # If verification fails, assume not in path
        $pathContainsDir = $null
    }
    
    Write-Host ""
    if ($exeExists) {
        Write-Host "✓ wole installed successfully!" -ForegroundColor Green
        
        # Try to actually run wole to verify it works
        $woleWorks = $false
        try {
            $null = & $TARGET_PATH --version 2>&1
            if ($LASTEXITCODE -eq 0) {
                $woleWorks = $true
            }
        } catch {
            $woleWorks = $false
        }
        
        if ($woleWorks) {
            Write-Host "✓ wole is ready to use!" -ForegroundColor Green
            Write-Host ""
            Write-Host "Run 'wole --help' to get started." -ForegroundColor Cyan
            Write-Host ""
            # Show a quick demo
            Write-Host "Quick start:" -ForegroundColor White
            Write-Host "  wole scan     - Scan for cleanable files" -ForegroundColor Gray
            Write-Host "  wole clean    - Clean files interactively" -ForegroundColor Gray
            Write-Host "  wole status   - Show system status" -ForegroundColor Gray
        } else {
            Write-Host "✓ PATH updated for future terminal sessions" -ForegroundColor Green
            Write-Host ""
            Write-Host "To use wole now, either:" -ForegroundColor Yellow
            Write-Host "  1. Open a NEW terminal window, then run: wole --help" -ForegroundColor White
            Write-Host "  2. Or run directly: & `"$TARGET_PATH`" --help" -ForegroundColor White
        }
    } else {
        Write-Host "✗ Installation may have failed - executable not found" -ForegroundColor Red
    }
    
} finally {
    # Cleanup
    Remove-Item -Path $TEMP_DIR -Recurse -Force -ErrorAction SilentlyContinue
}
