//! Startup command feature.
//!
//! This module owns and handles the "wole startup" command behavior.

use crate::theme::Theme;

pub(crate) fn handle_startup(
    _list: bool,
    disable: Option<String>,
    enable: Option<String>,
    json: bool,
) -> anyhow::Result<()> {
    use crate::categories::startup;

    if let Some(name) = disable {
        let programs = startup::list_startup_programs()?;
        if let Some(program) = programs.iter().find(|p| p.name == name) {
            startup::disable_startup_program(program)?;
            if !json {
                println!(
                    "{} Disabled startup program: {}",
                    Theme::success("✓"),
                    Theme::value(&name)
                );
            }
        } else {
            return Err(anyhow::anyhow!("Startup program not found: {}", name));
        }
    } else if let Some(name) = enable {
        let programs = startup::list_startup_programs()?;
        if let Some(program) = programs.iter().find(|p| p.name == name) {
            startup::enable_startup_program(program)?;
            if !json {
                println!(
                    "{} Enabled startup program: {}",
                    Theme::success("✓"),
                    Theme::value(&name)
                );
            }
        } else {
            return Err(anyhow::anyhow!("Startup program not found: {}", name));
        }
    } else {
        // List all startup programs
        let programs = startup::list_startup_programs()?;

        if json {
            println!("{}", serde_json::to_string_pretty(&programs)?);
        } else {
            println!();
            println!("{}", Theme::header("Windows Startup Programs"));
            println!("{}", Theme::divider_bold(60));
            println!();

            if programs.is_empty() {
                println!("{}", Theme::muted("No startup programs found."));
            } else {
                println!(
                    "{:<30} {:<50} {:<15} {:<10}",
                    Theme::primary("Name"),
                    Theme::primary("Command"),
                    Theme::primary("Location"),
                    Theme::primary("Impact")
                );
                println!("{}", Theme::divider(60));

                for program in &programs {
                    let impact_str = program.impact.as_str();
                    let location_short = if program.location.len() > 45 {
                        format!("{}...", &program.location[..42])
                    } else {
                        program.location.clone()
                    };
                    println!(
                        "{:<30} {:<50} {:<15} {:<10}",
                        Theme::value(&program.name),
                        Theme::muted(&program.command),
                        Theme::muted(&location_short),
                        Theme::category(impact_str)
                    );
                }

                println!();
                println!(
                    "{} Use {} to disable or {} to enable a program",
                    Theme::muted("→"),
                    Theme::command("wole startup --disable <name>"),
                    Theme::command("wole startup --enable <name>")
                );
            }
            println!();
        }
    }

    Ok(())
}
