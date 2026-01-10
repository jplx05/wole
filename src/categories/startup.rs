//! Windows Startup Program Manager
//!
//! Provides functionality to list, analyze, and manage Windows startup programs
//! from both Registry and Startup folder locations.

use anyhow::{Context, Result};
use serde::Serialize;
use std::path::PathBuf;
use std::process::Command;

#[cfg(windows)]
use winreg::enums::*;
#[cfg(windows)]
use winreg::{RegKey, HKEY};

/// Information about a startup program
#[derive(Debug, Clone, Serialize)]
pub struct StartupProgram {
    /// Name of the program
    pub name: String,
    /// Command/path to executable
    pub command: String,
    /// Location where it's registered (Registry path or Startup folder)
    pub location: String,
    /// Whether it's currently enabled
    pub enabled: bool,
    /// Estimated impact on boot time (Low, Medium, High)
    pub impact: StartupImpact,
    /// File size of the executable (if available)
    pub file_size: Option<u64>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum StartupImpact {
    Low,
    Medium,
    High,
    Unknown,
}

impl StartupImpact {
    pub fn as_str(&self) -> &'static str {
        match self {
            StartupImpact::Low => "Low",
            StartupImpact::Medium => "Medium",
            StartupImpact::High => "High",
            StartupImpact::Unknown => "Unknown",
        }
    }
}

/// List all startup programs from Registry and Startup folder
pub fn list_startup_programs() -> Result<Vec<StartupProgram>> {
    let mut programs = Vec::new();

    #[cfg(windows)]
    {
        // 1. Registry: HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Run
        programs.extend(list_registry_startup(
            HKEY_CURRENT_USER,
            "Software\\Microsoft\\Windows\\CurrentVersion\\Run",
        )?);

        // 2. Registry: HKEY_LOCAL_MACHINE\Software\Microsoft\Windows\CurrentVersion\Run
        programs.extend(list_registry_startup(
            HKEY_LOCAL_MACHINE,
            "Software\\Microsoft\\Windows\\CurrentVersion\\Run",
        )?);

        // 3. Registry: HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\RunOnce
        programs.extend(list_registry_startup(
            HKEY_CURRENT_USER,
            "Software\\Microsoft\\Windows\\CurrentVersion\\RunOnce",
        )?);

        // 4. Startup folder: %APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup
        programs.extend(list_startup_folder()?);
    }

    #[cfg(not(windows))]
    {
        // On non-Windows, return empty list
        return Ok(programs);
    }

    Ok(programs)
}

#[cfg(windows)]
fn list_registry_startup(hkey: HKEY, subkey: &str) -> Result<Vec<StartupProgram>> {
    let mut programs = Vec::new();

    let root = RegKey::predef(hkey);
    let run_key = root.open_subkey(subkey);

    let run_key = match run_key {
        Ok(key) => key,
        Err(_) => return Ok(programs), // Key doesn't exist, return empty
    };

    for (name, value) in run_key.enum_values().flatten() {
        let command = value.to_string();
        let location = format!("Registry: {}", subkey);
        let enabled = true; // Registry entries are enabled by default
        let impact = estimate_impact(&command);
        let file_size = get_executable_size(&command);

        programs.push(StartupProgram {
            name,
            command,
            location,
            enabled,
            impact,
            file_size,
        });
    }

    Ok(programs)
}

#[cfg(windows)]
fn list_startup_folder() -> Result<Vec<StartupProgram>> {
    let mut programs = Vec::new();

    let appdata = std::env::var("APPDATA")
        .ok()
        .map(PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("Could not find APPDATA"))?;

    let startup_folder = appdata
        .join("Microsoft")
        .join("Windows")
        .join("Start Menu")
        .join("Programs")
        .join("Startup");

    if !startup_folder.exists() {
        return Ok(programs);
    }

    if let Ok(entries) = std::fs::read_dir(&startup_folder) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path
                .file_stem()
                .and_then(|n| n.to_str())
                .unwrap_or("Unknown")
                .to_string();

            // Try to resolve shortcut target if it's a .lnk file
            let command = if path.extension().and_then(|e| e.to_str()) == Some("lnk") {
                resolve_shortcut(&path).unwrap_or_else(|| path.to_string_lossy().to_string())
            } else {
                path.to_string_lossy().to_string()
            };

            let location = format!("Startup Folder: {}", startup_folder.display());
            let enabled = true; // Files in startup folder are enabled
            let impact = estimate_impact(&command);
            let file_size = get_executable_size(&command);

            programs.push(StartupProgram {
                name,
                command,
                location,
                enabled,
                impact,
                file_size,
            });
        }
    }

    Ok(programs)
}

#[cfg(windows)]
fn resolve_shortcut(lnk_path: &PathBuf) -> Option<String> {
    // Try to use PowerShell to resolve shortcut target
    let script = format!(
        r#"$shell = New-Object -ComObject WScript.Shell; $shortcut = $shell.CreateShortcut('{}'); $shortcut.TargetPath"#,
        lnk_path.to_string_lossy().replace('\\', "\\\\")
    );

    let output = Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .output()
        .ok()?;

    if output.status.success() {
        let target = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !target.is_empty() {
            return Some(target);
        }
    }

    None
}

#[cfg(windows)]
fn get_executable_size(command: &str) -> Option<u64> {
    // Extract executable path from command (handle quoted paths and arguments)
    let exe_path = command
        .split_whitespace()
        .next()
        .map(|s| s.trim_matches('"'))
        .and_then(|s| {
            if s.ends_with(".exe") || s.ends_with(".bat") || s.ends_with(".cmd") {
                Some(s)
            } else {
                None
            }
        })?;

    let path = PathBuf::from(exe_path);
    if path.exists() {
        std::fs::metadata(&path).ok().map(|m| m.len())
    } else {
        None
    }
}

fn estimate_impact(command: &str) -> StartupImpact {
    let cmd_lower = command.to_lowercase();

    // High impact: Antivirus, security software, system utilities
    if cmd_lower.contains("antivirus")
        || cmd_lower.contains("avast")
        || cmd_lower.contains("norton")
        || cmd_lower.contains("mcafee")
        || cmd_lower.contains("kaspersky")
        || cmd_lower.contains("defender")
        || cmd_lower.contains("security")
    {
        return StartupImpact::High;
    }

    // Medium impact: Cloud sync, updaters, communication apps
    if cmd_lower.contains("onedrive")
        || cmd_lower.contains("dropbox")
        || cmd_lower.contains("google")
        || cmd_lower.contains("update")
        || cmd_lower.contains("discord")
        || cmd_lower.contains("slack")
        || cmd_lower.contains("teams")
    {
        return StartupImpact::Medium;
    }

    // Low impact: Small utilities, launchers
    if cmd_lower.contains("launcher") || cmd_lower.contains("tray") || cmd_lower.contains("helper")
    {
        return StartupImpact::Low;
    }

    StartupImpact::Unknown
}

/// Disable a startup program
///
/// For Registry entries, this renames the value to disable it.
/// For Startup folder entries, this moves the file to a disabled subfolder.
pub fn disable_startup_program(program: &StartupProgram) -> Result<()> {
    #[cfg(windows)]
    {
        if program.location.starts_with("Registry:") {
            disable_registry_startup(program)?;
        } else if program.location.starts_with("Startup Folder:") {
            disable_startup_folder_entry(program)?;
        }
    }

    #[cfg(not(windows))]
    {
        return Err(anyhow::anyhow!(
            "Startup management is only available on Windows"
        ));
    }

    Ok(())
}

#[cfg(windows)]
fn disable_registry_startup(program: &StartupProgram) -> Result<()> {
    // Extract registry path from location
    let location = program.location.strip_prefix("Registry: ").unwrap_or("");
    let parts: Vec<&str> = location.split('\\').collect();

    if parts.is_empty() {
        return Err(anyhow::anyhow!("Invalid registry location"));
    }

    // Determine root key
    let root_key = if location.contains("HKEY_CURRENT_USER") || location.contains("CurrentVersion")
    {
        HKEY_CURRENT_USER
    } else {
        HKEY_LOCAL_MACHINE
    };

    let subkey = parts.join("\\");
    let root = RegKey::predef(root_key);
    let run_key = root
        .open_subkey_with_flags(&subkey, KEY_WRITE)
        .with_context(|| format!("Failed to open registry key: {}", subkey))?;

    // Rename the value to disable it (add _Disabled suffix)
    let disabled_name = format!("{}_Disabled", program.name);

    // Delete old value and create new disabled one
    run_key
        .delete_value(&program.name)
        .with_context(|| format!("Failed to delete registry value: {}", program.name))?;

    run_key
        .set_value(&disabled_name, &program.command)
        .with_context(|| {
            format!(
                "Failed to create disabled registry value: {}",
                disabled_name
            )
        })?;

    Ok(())
}

#[cfg(windows)]
fn disable_startup_folder_entry(program: &StartupProgram) -> Result<()> {
    // Extract path from command or location
    let startup_folder = std::env::var("APPDATA")
        .ok()
        .map(PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("Could not find APPDATA"))?
        .join("Microsoft")
        .join("Windows")
        .join("Start Menu")
        .join("Programs")
        .join("Startup");

    // Find the file in startup folder
    let file_name = format!("{}.lnk", program.name);
    let file_path = startup_folder.join(&file_name);

    if !file_path.exists() {
        // Try without extension
        let file_path_no_ext = startup_folder.join(&program.name);
        if file_path_no_ext.exists() {
            // Move to disabled subfolder
            let disabled_folder = startup_folder.join("_Disabled");
            std::fs::create_dir_all(&disabled_folder)?;

            let disabled_path = disabled_folder.join(&program.name);
            std::fs::rename(&file_path_no_ext, &disabled_path)
                .with_context(|| format!("Failed to move startup entry: {}", program.name))?;
            return Ok(());
        }
        return Err(anyhow::anyhow!("Startup entry not found: {}", program.name));
    }

    // Move to disabled subfolder
    let disabled_folder = startup_folder.join("_Disabled");
    std::fs::create_dir_all(&disabled_folder)?;

    let disabled_path = disabled_folder.join(&file_name);
    std::fs::rename(&file_path, &disabled_path)
        .with_context(|| format!("Failed to move startup entry: {}", program.name))?;

    Ok(())
}

/// Enable a previously disabled startup program
pub fn enable_startup_program(program: &StartupProgram) -> Result<()> {
    #[cfg(windows)]
    {
        if program.location.starts_with("Registry:") {
            enable_registry_startup(program)?;
        } else if program.location.starts_with("Startup Folder:") {
            enable_startup_folder_entry(program)?;
        }
    }

    #[cfg(not(windows))]
    {
        return Err(anyhow::anyhow!(
            "Startup management is only available on Windows"
        ));
    }

    Ok(())
}

#[cfg(windows)]
fn enable_registry_startup(program: &StartupProgram) -> Result<()> {
    // Extract registry path from location
    let location = program.location.strip_prefix("Registry: ").unwrap_or("");
    let parts: Vec<&str> = location.split('\\').collect();

    if parts.is_empty() {
        return Err(anyhow::anyhow!("Invalid registry location"));
    }

    // Determine root key
    let root_key = if location.contains("HKEY_CURRENT_USER") || location.contains("CurrentVersion")
    {
        HKEY_CURRENT_USER
    } else {
        HKEY_LOCAL_MACHINE
    };

    let subkey = parts.join("\\");
    let root = RegKey::predef(root_key);
    let run_key = root
        .open_subkey_with_flags(&subkey, KEY_WRITE)
        .with_context(|| format!("Failed to open registry key: {}", subkey))?;

    // Check if there's a disabled version
    let disabled_name = format!("{}_Disabled", program.name);

    // Try to read disabled value
    if let Ok(command) = run_key.get_value::<String, _>(&disabled_name) {
        // Restore original name
        run_key.delete_value(&disabled_name)?;
        run_key.set_value(&program.name, &command)?;
    } else {
        // No disabled version found, create new entry
        run_key.set_value(&program.name, &program.command)?;
    }

    Ok(())
}

#[cfg(windows)]
fn enable_startup_folder_entry(program: &StartupProgram) -> Result<()> {
    let startup_folder = std::env::var("APPDATA")
        .ok()
        .map(PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("Could not find APPDATA"))?
        .join("Microsoft")
        .join("Windows")
        .join("Start Menu")
        .join("Programs")
        .join("Startup");

    let disabled_folder = startup_folder.join("_Disabled");
    let file_name = format!("{}.lnk", program.name);
    let disabled_path = disabled_folder.join(&file_name);

    if !disabled_path.exists() {
        // Try without extension
        let disabled_path_no_ext = disabled_folder.join(&program.name);
        if disabled_path_no_ext.exists() {
            let enabled_path = startup_folder.join(&program.name);
            std::fs::rename(&disabled_path_no_ext, &enabled_path)
                .with_context(|| format!("Failed to restore startup entry: {}", program.name))?;
            return Ok(());
        }
        return Err(anyhow::anyhow!(
            "Disabled startup entry not found: {}",
            program.name
        ));
    }

    let enabled_path = startup_folder.join(&file_name);
    std::fs::rename(&disabled_path, &enabled_path)
        .with_context(|| format!("Failed to restore startup entry: {}", program.name))?;

    Ok(())
}
