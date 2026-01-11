//! Interactive menu feature.
//!
//! This module owns the CLI interactive menu display.

use crate::theme::Theme;
use super::Cli;

impl Cli {
    /// Show interactive menu when no command is provided
    pub fn show_interactive_menu() {
        println!();
        println!("{}", Theme::header("Wole - Reclaim Disk Space on Windows"));
        println!("{}", Theme::divider_bold(60));
        println!();
        println!("{}", Theme::primary("Available Commands:"));
        println!();
        println!(
            "  {}  {}  {}",
            Theme::command("scan"),
            Theme::muted("or"),
            Theme::command("s"),
        );
        println!(
            "     {} Find cleanable files (safe, dry-run)",
            Theme::muted("→")
        );
        println!();
        println!(
            "  {}  {}  {}",
            Theme::command("clean"),
            Theme::muted("or"),
            Theme::command("c"),
        );
        println!("     {} Delete files found by scan", Theme::muted("→"));
        println!();
        println!(
            "  {}  {}  {}",
            Theme::command("analyze"),
            Theme::muted("or"),
            Theme::command("a"),
        );
        println!(
            "     {} Show detailed analysis with file lists",
            Theme::muted("→")
        );
        println!();
        println!("  {}", Theme::command("config"));
        println!("     {} View or modify configuration", Theme::muted("→"));
        println!();
        println!(
            "  {}  {}  {}",
            Theme::command("restore"),
            Theme::muted("or"),
            Theme::command("r"),
        );
        println!(
            "     {} Restore files from last deletion",
            Theme::muted("→")
        );
        println!();
        println!("  {}", Theme::command("remove"));
        println!("     {} Uninstall wole from your system", Theme::muted("→"));
        println!();
        println!("  {}", Theme::command("update"));
        println!("     {} Check for and install updates", Theme::muted("→"));
        println!();
        println!(
            "  {}  {}  {}",
            Theme::command("optimize"),
            Theme::muted("or"),
            Theme::command("o"),
        );
        println!(
            "     {} Optimize Windows system performance",
            Theme::muted("→")
        );
        println!();
        println!(
            "  {}  {}  {}",
            Theme::command("status"),
            Theme::muted("or"),
            Theme::command("st"),
        );
        println!(
            "     {} Show real-time system status dashboard",
            Theme::muted("→")
        );
        println!();
        println!("{}", Theme::divider(60));
        println!();
        println!("{}", Theme::primary("Quick Examples:"));
        println!();
        println!("  {} Launch interactive TUI mode", Theme::command("wole"));
        println!(
            "  {} Scan all categories",
            Theme::command("wole scan --all")
        );
        println!(
            "  {} Scan specific categories",
            Theme::command("wole scan --cache --temp")
        );
        println!(
            "  {} Clean all files",
            Theme::command("wole clean --all -y")
        );
        println!(
            "  {} Find large files",
            Theme::command("wole scan --large --min-size 500MB")
        );
        println!(
            "  {} Interactive disk insights",
            Theme::command("wole analyze --interactive")
        );
        println!(
            "  {} Restore last deletion",
            Theme::command("wole restore --last")
        );
        println!(
            "  {} Run all system optimizations",
            Theme::command("wole optimize --all")
        );
        println!("  {} Show system status", Theme::command("wole status"));
        println!();
        println!(
            "{}",
            Theme::muted("Tip: Use --help with any command for detailed options")
        );
        println!();
    }
}
