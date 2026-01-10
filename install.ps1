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
        Write-Host "  Location: $TARGET_PATH" -ForegroundColor Gray
        
        # Verify file size to ensure download wasn't corrupted
        $fileSize = (Get-Item $TARGET_PATH).Length
        if ($fileSize -lt 100000) {
            Write-Host "⚠ Warning: Executable seems too small ($fileSize bytes) - download may have failed" -ForegroundColor Yellow
        }
        
        # Try to actually run wole to verify it works
        $woleWorks = $false
        $woleOutput = ""
        $woleError = ""
        try {
            $processInfo = New-Object System.Diagnostics.ProcessStartInfo
            $processInfo.FileName = $TARGET_PATH
            $processInfo.Arguments = "--version"
            $processInfo.RedirectStandardOutput = $true
            $processInfo.RedirectStandardError = $true
            $processInfo.UseShellExecute = $false
            $processInfo.CreateNoWindow = $true
            
            $process = New-Object System.Diagnostics.Process
            $process.StartInfo = $processInfo
            $process.Start() | Out-Null
            $woleOutput = $process.StandardOutput.ReadToEnd()
            $woleError = $process.StandardError.ReadToEnd()
            $process.WaitForExit(5000) | Out-Null
            
            if ($process.ExitCode -eq 0) {
                $woleWorks = $true
            }
        } catch {
            $woleError = $_.Exception.Message
        }
        
        if ($woleWorks) {
            Write-Host "✓ wole is ready to use!" -ForegroundColor Green
            if ($woleOutput) {
                Write-Host "  Version: $($woleOutput.Trim())" -ForegroundColor Gray
            }
            Write-Host ""
            Write-Host "Quick start:" -ForegroundColor Cyan
            Write-Host "  wole scan     - Scan for cleanable files" -ForegroundColor White
            Write-Host "  wole clean    - Clean files interactively" -ForegroundColor White
            Write-Host "  wole status   - Show system status" -ForegroundColor White
            Write-Host ""
            Write-Host "Run 'wole --help' for all commands." -ForegroundColor Gray
        } else {
            Write-Host "⚠ wole installed but may have issues running" -ForegroundColor Yellow
            if ($woleError) {
                Write-Host "  Error: $woleError" -ForegroundColor Red
            }

            # If we hit the common "vcruntime140.dll was not found" case, try to install VC++ runtime automatically
            $missingVCRuntime = $false
            if ($woleError -match '(?i)vcruntime140\.dll' -or $woleError -match '(?i)msvcp140\.dll' -or $woleError -match '(?i)api-ms-win-crt') {
                $missingVCRuntime = $true
            }

            if ($missingVCRuntime) {
                Write-Host "" 
                Write-Host "Missing Microsoft Visual C++ Runtime detected." -ForegroundColor Yellow
                Write-Host "Attempting to install it now (may prompt for admin approval)..." -ForegroundColor Yellow

                # Choose the correct redistributable for the current arch
                $vcArch = "x64"
                if ($ARCH -eq "arm64") { $vcArch = "arm64" }
                elseif ($ARCH -eq "i686") { $vcArch = "x86" }

                $vcRedistUrl = "https://aka.ms/vs/17/release/vc_redist.${vcArch}.exe"
                $vcRedistPath = Join-Path $TEMP_DIR "vc_redist.${vcArch}.exe"

                try {
                    Write-Host "Downloading VC++ Runtime from $vcRedistUrl..." -ForegroundColor Gray
                    Invoke-WebRequest -Uri $vcRedistUrl -OutFile $vcRedistPath -UseBasicParsing

                    # Silent install; will trigger UAC if needed
                    $proc = Start-Process -FilePath $vcRedistPath -ArgumentList "/install", "/quiet", "/norestart" -Wait -PassThru
                    $vcExit = $proc.ExitCode

                    if ($vcExit -eq 0 -or $vcExit -eq 3010) {
                        Write-Host "✓ VC++ Runtime installed." -ForegroundColor Green
                        if ($vcExit -eq 3010) {
                            Write-Host "⚠ A restart may be required to complete installation." -ForegroundColor Yellow
                        }

                        # Re-test wole
                        $woleOutput = ""
                        $woleError = ""
                        try {
                            $processInfo = New-Object System.Diagnostics.ProcessStartInfo
                            $processInfo.FileName = $TARGET_PATH
                            $processInfo.Arguments = "--version"
                            $processInfo.RedirectStandardOutput = $true
                            $processInfo.RedirectStandardError = $true
                            $processInfo.UseShellExecute = $false
                            $processInfo.CreateNoWindow = $true

                            $process = New-Object System.Diagnostics.Process
                            $process.StartInfo = $processInfo
                            $process.Start() | Out-Null
                            $woleOutput = $process.StandardOutput.ReadToEnd()
                            $woleError = $process.StandardError.ReadToEnd()
                            $process.WaitForExit(5000) | Out-Null

                            if ($process.ExitCode -eq 0) {
                                $woleWorks = $true
                            }
                        } catch {
                            $woleError = $_.Exception.Message
                        }
                    } else {
                        Write-Host "⚠ VC++ Runtime installer exited with code $vcExit." -ForegroundColor Yellow
                    }
                } catch {
                    Write-Host "⚠ Failed to install VC++ Runtime automatically." -ForegroundColor Yellow
                    Write-Host "  You can install it manually by running:" -ForegroundColor Yellow
                    Write-Host "  $vcRedistPath" -ForegroundColor White
                    Write-Host "  (or download: $vcRedistUrl)" -ForegroundColor Gray
                }

                if ($woleWorks) {
                    Write-Host ""
                    Write-Host "✓ wole is ready to use!" -ForegroundColor Green
                    if ($woleOutput) {
                        Write-Host "  Version: $($woleOutput.Trim())" -ForegroundColor Gray
                    }
                    Write-Host ""
                    Write-Host "Run 'wole --help' to get started." -ForegroundColor Cyan
                    Write-Host ""
                    return
                }
            }

            Write-Host ""
            Write-Host "Try running directly:" -ForegroundColor Yellow
            Write-Host "  & `"$TARGET_PATH`" --help" -ForegroundColor White
            Write-Host ""
            Write-Host "If that fails, possible causes:" -ForegroundColor Gray
            Write-Host "  - Missing Visual C++ Runtime (will show vcruntime140.dll not found)" -ForegroundColor Gray
            Write-Host "  - Windows Defender blocking (check Security settings)" -ForegroundColor Gray
            Write-Host "  - Wrong architecture (ARM vs x64)" -ForegroundColor Gray
        }
    } else {
        Write-Host "✗ Installation failed - executable not found at $TARGET_PATH" -ForegroundColor Red
    }
    
} finally {
    # Cleanup
    Remove-Item -Path $TEMP_DIR -Recurse -Force -ErrorAction SilentlyContinue
}
