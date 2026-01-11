//! Status command feature.
//!
//! This module owns and handles the "wole status" command behavior.

pub(crate) fn handle_status(json: bool, _watch: bool) -> anyhow::Result<()> {
    if json {
        // JSON output mode - use text output
        use sysinfo::System;

        let mut system = System::new();
        system.refresh_all();

        match crate::status::gather_status(&mut system) {
            Ok(status) => {
                let json_output = serde_json::to_string_pretty(&status)?;
                println!("{}", json_output);
                Ok(())
            }
            Err(e) => Err(anyhow::anyhow!("Failed to gather system status: {}", e)),
        }
    } else {
        // Launch interactive TUI for real-time status dashboard
        // Ignore watch flag - TUI always auto-refreshes
        use crate::status::gather_status;
        use sysinfo::System;

        // Don't call refresh_all() - gather_status will refresh what it needs
        // This avoids blocking on expensive full system refresh
        let mut system = System::new();

        match gather_status(&mut system) {
            Ok(status) => {
                let mut app_state = crate::tui::state::AppState::new();
                app_state.screen = crate::tui::state::Screen::Status {
                    status: Box::new(status),
                    last_refresh: std::time::Instant::now(),
                    status_receiver: None,
                };
                crate::tui::run(Some(app_state))?;
                Ok(())
            }
            Err(e) => Err(anyhow::anyhow!("Failed to gather system status: {}", e)),
        }
    }
}
