//! Restore command feature.
//!
//! This module owns and handles the "wole restore" command behavior.

use crate::history;
use crate::output::OutputMode;
use crate::restore;
use crate::theme::Theme;
use anyhow::Context;
use std::path::PathBuf;

pub(crate) fn handle_restore(
    last: bool,
    path: Option<PathBuf>,
    from: Option<PathBuf>,
    all: bool,
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

    if all {
        // Restore all contents of Recycle Bin in bulk
        match restore::restore_all_bin(output_mode, None) {
            Ok(result) => {
                if output_mode != OutputMode::Quiet {
                    println!();
                    println!("{} {}", Theme::success("OK"), Theme::success(&result.summary()));
                }
            }
            Err(e) => {
                return Err(anyhow::anyhow!("Failed to restore: {}", e));
            }
        }
    } else if last {
        // Restore from last deletion session
        match restore::restore_last(output_mode) {
            Ok(result) => {
                if output_mode != OutputMode::Quiet {
                    println!();
                    println!("{} {}", Theme::success("OK"), Theme::success(&result.summary()));
                }
            }
            Err(e) => {
                return Err(anyhow::anyhow!("Failed to restore: {}", e));
            }
        }
    } else if let Some(ref restore_path) = path {
        // Restore specific path
        match restore::restore_path(restore_path, output_mode) {
            Ok(result) => {
                if output_mode != OutputMode::Quiet {
                    println!();
                    println!("{} {}", Theme::success("OK"), Theme::success(&result.summary()));
                }
            }
            Err(e) => {
                return Err(anyhow::anyhow!("Failed to restore: {}", e));
            }
        }
    } else if let Some(ref log_path) = from {
        // Restore from specific log file
        let log = history::load_log(log_path)
            .with_context(|| format!("Failed to load log file: {}", log_path.display()))?;
        match restore::restore_from_log(&log, output_mode) {
            Ok(result) => {
                if output_mode != OutputMode::Quiet {
                    println!();
                    println!("{} {}", Theme::success("OK"), Theme::success(&result.summary()));
                }
            }
            Err(e) => {
                return Err(anyhow::anyhow!("Failed to restore: {}", e));
            }
        }
    } else {
        // Default: restore from last session
        match restore::restore_last(output_mode) {
            Ok(result) => {
                if output_mode != OutputMode::Quiet {
                    println!();
                    println!("{} {}", Theme::success("OK"), Theme::success(&result.summary()));
                }
            }
            Err(e) => {
                return Err(anyhow::anyhow!("Failed to restore: {}", e));
            }
        }
    }

    Ok(())
}
