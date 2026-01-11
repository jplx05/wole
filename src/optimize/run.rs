//! Optimization run orchestration feature.

use super::admin_check::is_admin;
use super::operations::{
    clear_standby_memory, clear_thumbnail_cache, flush_dns_cache, rebuild_icon_cache,
    reset_network_stack, restart_bluetooth_service, restart_explorer, restart_font_cache_service,
    restart_windows_search, vacuum_browser_databases,
};
use super::printing::{print_operation_result, print_operation_start};
use super::result::OptimizeResult;
use crate::output::OutputMode;
use crate::theme::Theme;

/// Run all optimizations
#[allow(clippy::too_many_arguments)]
pub fn run_optimizations(
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
    _yes: bool,
    output_mode: OutputMode,
) -> Vec<OptimizeResult> {
    let mut results = Vec::new();

    // Determine which optimizations to run
    let run_dns = all || dns;
    let run_thumbnails = all || thumbnails;
    let run_icons = all || icons;
    let run_databases = all || databases;
    let mut run_fonts = all || fonts;
    let mut run_memory = all || memory;
    let mut run_network = all || network;
    let mut run_bluetooth = all || bluetooth;
    let mut run_search = all || search;
    let run_explorer = all || explorer;

    // Check if any admin operations are requested
    let needs_admin = run_fonts || run_memory || run_network || run_bluetooth || run_search;
    let is_admin_user = is_admin();

    // If admin operations are needed and we're not running as admin, skip them automatically
    if needs_admin && !is_admin_user && !dry_run {
        if output_mode != OutputMode::Quiet {
            println!();
            println!(
                "{}",
                Theme::warning(
                    "Running non-admin optimizations only (not running as Administrator)."
                )
            );
            println!();
        }
        // Skip admin operations if not running as admin
        run_fonts = false;
        run_memory = false;
        run_network = false;
        run_bluetooth = false;
        run_search = false;
    }

    // Run non-admin operations first
    if run_dns {
        print_operation_start("Flushing DNS cache...", output_mode);
        let result = flush_dns_cache(dry_run);
        print_operation_result(&result, output_mode);
        results.push(result);
    }

    if run_thumbnails {
        print_operation_start("Clearing thumbnail cache...", output_mode);
        let result = clear_thumbnail_cache(dry_run);
        print_operation_result(&result, output_mode);
        results.push(result);
    }

    if run_icons {
        print_operation_start("Rebuilding icon cache...", output_mode);
        // Don't restart explorer if we're going to do it separately
        let result = rebuild_icon_cache(dry_run, !run_explorer);
        print_operation_result(&result, output_mode);
        results.push(result);
    }

    if run_databases {
        print_operation_start("Optimizing browser databases...", output_mode);
        let result = vacuum_browser_databases(dry_run);
        print_operation_result(&result, output_mode);
        results.push(result);
    }

    // Admin operations
    if run_fonts {
        print_operation_start("Restarting font cache service...", output_mode);
        let result = restart_font_cache_service(dry_run);
        print_operation_result(&result, output_mode);
        results.push(result);
    }

    if run_memory {
        print_operation_start("Clearing standby memory...", output_mode);
        let result = clear_standby_memory(dry_run);
        print_operation_result(&result, output_mode);
        results.push(result);
    }

    if run_network {
        // Check if we already skipped it
        let already_skipped = results.iter().any(|r| r.action == "Reset Network Stack");
        if !already_skipped {
            print_operation_start("Resetting network stack...", output_mode);
            let result = reset_network_stack(dry_run);
            print_operation_result(&result, output_mode);
            results.push(result);
        }
    }

    if run_bluetooth {
        print_operation_start("Restarting Bluetooth service...", output_mode);
        let result = restart_bluetooth_service(dry_run);
        print_operation_result(&result, output_mode);
        results.push(result);
    }

    if run_search {
        print_operation_start("Restarting Windows Search...", output_mode);
        let result = restart_windows_search(dry_run);
        print_operation_result(&result, output_mode);
        results.push(result);
    }

    // Explorer should be last as it refreshes the shell
    if run_explorer {
        print_operation_start("Restarting Explorer...", output_mode);
        let result = restart_explorer(dry_run);
        print_operation_result(&result, output_mode);
        results.push(result);
    }

    // If we skipped admin operations, show helpful message
    if needs_admin && !is_admin_user && !dry_run && output_mode != OutputMode::Quiet {
        let skipped_flags: Vec<&str> = [
            (all || fonts, "--fonts"),
            (all || memory, "--memory"),
            (all || network, "--network"),
            (all || bluetooth, "--bluetooth"),
            (all || search, "--search"),
        ]
        .iter()
        .filter(|(requested, _)| *requested)
        .map(|(_, flag)| *flag)
        .collect();

        if !skipped_flags.is_empty() {
            println!();
            println!("{}", Theme::divider(60));
            println!();
            println!(
                "{}",
                Theme::warning("Skipped admin-required optimizations:")
            );
            for flag in &skipped_flags {
                println!("  • {}", Theme::muted(flag));
            }
            println!();
            println!(
                "{}",
                Theme::primary("To run these, restart as Administrator:")
            );

            // Build the command with specific flags
            let flags_arg = if all {
                "--all".to_string()
            } else {
                skipped_flags.join("','")
            };

            println!(
                "  {}",
                Theme::command(&format!(
                    "Start-Process wole -ArgumentList 'optimize','{}' -Verb RunAs",
                    flags_arg
                ))
            );
            println!();
            println!("{}", Theme::muted("(Run the above command in PowerShell)"));
        }
    }

    results
}
