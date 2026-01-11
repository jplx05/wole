//! Optimize command feature.
//!
//! This module owns and handles the "wole optimize" command behavior.

use crate::optimize;
use crate::output::OutputMode;
use crate::theme::Theme;

pub(crate) fn handle_optimize(
    all: bool,
    dns: bool,
    thumbnails: bool,
    icons: bool,
    databases: bool,
    fonts: bool,
    memory: bool,
    network: bool,
    bluetooth: bool,
    search: bool,
    explorer: bool,
    dry_run: bool,
    yes: bool,
    output_mode: OutputMode,
) -> anyhow::Result<()> {
    // If no options specified, default to --all
    let all = if !all
        && !dns
        && !thumbnails
        && !icons
        && !databases
        && !fonts
        && !memory
        && !network
        && !bluetooth
        && !search
        && !explorer
    {
        if output_mode != OutputMode::Quiet {
            println!();
            println!(
                "{}",
                Theme::primary("No options specified, running all available optimizations...")
            );
            println!();
        }
        true
    } else {
        all
    };

    if output_mode != OutputMode::Quiet {
        println!();
        println!("{}", Theme::header("Windows System Optimization"));
        println!("{}", Theme::divider_bold(60));

        if dry_run {
            println!("{}", Theme::warning("DRY RUN MODE - No changes will be made"));
        }
        println!();
    }

    let results = optimize::run_optimizations(
        all,
        dns,
        thumbnails,
        icons,
        databases,
        fonts,
        memory,
        network,
        bluetooth,
        search,
        explorer,
        dry_run,
        yes,
        output_mode,
    );

    optimize::print_summary(&results, output_mode);
    Ok(())
}
