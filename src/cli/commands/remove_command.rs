//! Remove command feature.
//!
//! This module owns and handles the "wole remove" command behavior.

use crate::output::OutputMode;
use crate::theme::Theme;
use crate::uninstall;
use std::io::{self, Write};

/// Read a line from stdin, handling terminal focus loss issues on Windows.
/// This function ensures stdin is properly synchronized and clears any stale input
/// before reading, which fixes issues when the terminal loses and regains focus.
///
/// On Windows, when a terminal loses focus and regains it, stdin can be in a
/// problematic state. This function ensures we get a fresh stdin handle each time,
/// which helps resolve focus-related input issues.
fn read_line_from_stdin() -> io::Result<String> {
    // Flush stdout to ensure prompt is visible before reading
    io::stdout().flush()?;

    // Always get a fresh stdin handle to avoid issues with stale locks
    // This is especially important on Windows when the terminal loses focus
    let mut input = String::new();

    // Use BufRead for better control and proper buffering
    use std::io::BufRead;

    // Get a fresh stdin handle each time (don't reuse a locked handle)
    // This ensures we're reading from the current terminal state
    let stdin = io::stdin();
    let mut handle = stdin.lock();

    // Read a line - this will block until the user types and presses Enter
    // On Windows, getting a fresh handle helps when the terminal has lost focus
    handle.read_line(&mut input)?;

    Ok(input)
}

pub(crate) fn handle_remove(
    config: bool,
    data: bool,
    yes: bool,
    quiet: bool,
    verbose: u8,
) -> anyhow::Result<()> {
    let output_mode = if quiet {
        OutputMode::Quiet
    } else if verbose >= 2 {
        OutputMode::VeryVerbose
    } else if verbose == 1 {
        OutputMode::Verbose
    } else {
        OutputMode::Normal
    };

    // Confirm unless --yes flag is provided
    if !yes {
        println!();
        println!(
            "{}",
            Theme::warning("Warning: This will uninstall wole from your system.")
        );
        println!();
        println!("This will:");
        println!("  • Remove the wole executable");
        println!("  • Remove wole from your PATH");
        if config {
            println!(
                "  • Remove config directory ({})",
                uninstall::get_config_dir()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| "%APPDATA%\\wole".to_string())
            );
        }
        if data {
            println!(
                "  • Remove data directory ({})",
                uninstall::get_data_dir()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| "%LOCALAPPDATA%\\wole".to_string())
            );
        }
        println!();
        print!("Are you sure you want to continue? [y/N]: ");
        io::stdout().flush().ok();
        let input = match read_line_from_stdin() {
            Ok(line) => line.trim().to_lowercase(),
            Err(_) => {
                // If reading fails (e.g., stdin is not available), default to "no"
                println!("\nUninstall cancelled (failed to read input).");
                return Ok(());
            }
        };
        if input != "y" && input != "yes" {
            println!("Uninstall cancelled.");
            return Ok(());
        }
    }

    uninstall::uninstall(config, data, output_mode)?;
    Ok(())
}
