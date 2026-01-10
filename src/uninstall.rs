use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
use std::env;

use crate::theme::Theme;
use crate::output::OutputMode;

/// Get the installation directory where wole.exe is located
pub fn get_install_dir() -> Result<PathBuf> {
    let localappdata = env::var("LOCALAPPDATA")
        .context("LOCALAPPDATA environment variable not set")?;
    Ok(PathBuf::from(localappdata).join("wole").join("bin"))
}

/// Get the executable path
pub fn get_executable_path() -> Result<PathBuf> {
    Ok(get_install_dir()?.join("wole.exe"))
}

/// Get the config directory path
pub fn get_config_dir() -> Result<PathBuf> {
    let appdata = env::var("APPDATA")
        .context("APPDATA environment variable not set")?;
    Ok(PathBuf::from(appdata).join("wole"))
}

/// Get the data directory path (contains history, etc.)
pub fn get_data_dir() -> Result<PathBuf> {
    let localappdata = env::var("LOCALAPPDATA")
        .context("LOCALAPPDATA environment variable not set")?;
    Ok(PathBuf::from(localappdata).join("wole"))
}

/// Remove wole from PATH
fn remove_from_path(output_mode: OutputMode) -> Result<()> {
    let install_dir = get_install_dir()?;
    
    // Normalize the path
    let install_dir_normalized = install_dir
        .canonicalize()
        .unwrap_or_else(|_| install_dir.clone())
        .to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_string();

    // Update user PATH in registry
    #[cfg(windows)]
    {
        use std::process::Command;
        
        // Use PowerShell to update the PATH in the registry
        let ps_script = format!(
            r#"
            $installDir = [System.IO.Path]::GetFullPath('{}').TrimEnd('\', '/');
            $currentPath = [Environment]::GetEnvironmentVariable('Path', 'User');
            if (-not [string]::IsNullOrWhiteSpace($currentPath)) {{
                $pathEntries = $currentPath -split ';' | Where-Object {{ -not [string]::IsNullOrWhiteSpace($_) }};
                $filteredEntries = @();
                foreach ($entry in $pathEntries) {{
                    try {{
                        $normalizedEntry = [System.IO.Path]::GetFullPath($entry.Trim()).TrimEnd('\', '/');
                        if ($normalizedEntry -ne $installDir) {{
                            $filteredEntries += $entry;
                        }}
                    }} catch {{
                        $normalizedEntry = $entry.Trim().TrimEnd('\', '/');
                        if ($normalizedEntry -ne $installDir) {{
                            $filteredEntries += $entry;
                        }}
                    }}
                }};
                $newPath = $filteredEntries -join ';';
                [Environment]::SetEnvironmentVariable('Path', $newPath, 'User');
                Write-Host 'Removed from PATH' -ForegroundColor Green;
            }} else {{
                Write-Host 'PATH was empty' -ForegroundColor Gray;
            }}
            "#,
            install_dir_normalized.replace('\\', "\\\\")
        );

        let output = Command::new("powershell")
            .args(&["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &ps_script])
            .output()
            .context("Failed to execute PowerShell to update PATH")?;

        if output.status.success() {
            if output_mode != OutputMode::Quiet {
                println!("{} Removed wole from PATH", Theme::success("OK"));
            }
        } else {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Failed to remove from PATH: {}", error));
        }
    }

    Ok(())
}

/// Uninstall wole
pub fn uninstall(
    remove_config: bool,
    remove_data: bool,
    output_mode: OutputMode,
) -> Result<()> {
    // Check if executable exists
    let exe_path = get_executable_path()?;
    let install_dir = get_install_dir()?;
    
    let exe_existed = exe_path.exists();
    
    if !exe_existed {
        if output_mode != OutputMode::Quiet {
            eprintln!("{} wole executable not found at {}", 
                Theme::warning("Warning"), exe_path.display());
            eprintln!("It may have already been removed or installed in a different location.");
            eprintln!("Continuing with PATH cleanup and optional directory removal...");
        }
    } else {
        // Create spinner for uninstall operation
        let spinner = if output_mode != OutputMode::Quiet {
            Some(crate::progress::create_spinner("Uninstalling wole..."))
        } else {
            None
        };

        if output_mode != OutputMode::Quiet {
            println!("  Executable: {}", exe_path.display());
        }

        // Remove executable
        fs::remove_file(&exe_path)
            .with_context(|| format!("Failed to remove executable: {}", exe_path.display()))?;

        // Clear spinner
        if let Some(sp) = spinner {
            crate::progress::finish_and_clear(&sp);
        }

        if output_mode != OutputMode::Quiet {
            println!("{} Removed executable", Theme::success("OK"));
        }

        // Remove bin directory if empty
        if install_dir.exists() {
            match fs::read_dir(&install_dir) {
                Ok(mut entries) => {
                    if entries.next().is_none() {
                        // Directory is empty, remove it
                        fs::remove_dir(&install_dir)
                            .with_context(|| format!("Failed to remove directory: {}", install_dir.display()))?;
                        if output_mode != OutputMode::Quiet {
                            println!("{} Removed empty bin directory", Theme::success("OK"));
                        }
                    }
                }
                Err(_) => {
                    // Can't read directory, skip
                }
            }
        }
    }

    // Remove from PATH (always attempt, even if exe doesn't exist)
    remove_from_path(output_mode)?;

    // Remove config directory if requested
    if remove_config {
        let config_dir = get_config_dir()?;
        if config_dir.exists() {
            fs::remove_dir_all(&config_dir)
                .with_context(|| format!("Failed to remove config directory: {}", config_dir.display()))?;
            if output_mode != OutputMode::Quiet {
                println!("{} Removed config directory", Theme::success("OK"));
            }
        }
    }

    // Remove data directory if requested
    if remove_data {
        let data_dir = get_data_dir()?;
        if data_dir.exists() {
            fs::remove_dir_all(&data_dir)
                .with_context(|| format!("Failed to remove data directory: {}", data_dir.display()))?;
            if output_mode != OutputMode::Quiet {
                println!("{} Removed data directory (including history)", Theme::success("OK"));
            }
        }
    }

    if output_mode != OutputMode::Quiet {
        println!();
        if exe_existed {
            println!("{} wole has been uninstalled successfully!", Theme::success("✓"));
        } else {
            println!("{} Cleanup completed!", Theme::success("✓"));
        }
        if !remove_config && !remove_data {
            println!("Note: Config and data directories were preserved.");
            println!("Use --config and --data flags to remove them as well.");
        }
    }

    Ok(())
}
