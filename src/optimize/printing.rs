//! Optimization output formatting feature.

use crate::output::OutputMode;
use crate::theme::Theme;
use super::result::OptimizeResult;

pub(crate) fn print_operation_start(message: &str, output_mode: OutputMode) {
    if output_mode != OutputMode::Quiet {
        print!("  {} ", Theme::muted("→"));
        print!("{}", message);
        std::io::Write::flush(&mut std::io::stdout()).ok();
    }
}

pub(crate) fn print_operation_result(result: &OptimizeResult, output_mode: OutputMode) {
    if output_mode == OutputMode::Quiet {
        return;
    }

    // Clear the line and print result
    print!("\r");

    if result.success {
        if result.message.starts_with("Skipped:") {
            println!(
                "  {} {} - {}",
                Theme::muted("○"),
                result.action,
                Theme::muted(&result.message)
            );
        } else {
            println!(
                "  {} {} - {}",
                Theme::success("✓"),
                result.action,
                Theme::success(&result.message)
            );
        }
    } else {
        println!(
            "  {} {} - {}",
            Theme::error("✗"),
            result.action,
            Theme::error(&result.message)
        );
    }
}

/// Print summary of optimization results
pub fn print_summary(results: &[OptimizeResult], output_mode: OutputMode) {
    if output_mode == OutputMode::Quiet {
        return;
    }

    let total = results.len();
    let success = results
        .iter()
        .filter(|r| r.success && !r.message.starts_with("Skipped:"))
        .count();
    let skipped = results
        .iter()
        .filter(|r| r.message.starts_with("Skipped:"))
        .count();
    let failed = results.iter().filter(|r| !r.success).count();

    println!();
    println!("{}", Theme::divider(60));
    println!(
        "{}",
        Theme::primary(&format!(
            "Summary: {} total, {} succeeded, {} skipped, {} failed",
            total, success, skipped, failed
        ))
    );

    // Show restart hint if network was reset
    if results.iter().any(|r| {
        r.action == "Reset Network Stack" && r.success && !r.message.starts_with("Skipped:")
    }) {
        println!();
        println!(
            "{}",
            Theme::warning("Note: A system restart is recommended after network reset.")
        );
    }
}
