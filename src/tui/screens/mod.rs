//! Screen rendering modules

pub mod config;
pub mod confirm;
pub mod dashboard;
pub mod disk_insights;
pub mod optimize;
pub mod preview;
pub mod restore;
pub mod restore_selection;
pub mod results;
pub mod scanning;
pub mod success;

use crate::tui::state::AppState;
use ratatui::Frame;

/// Main render function that dispatches to the appropriate screen
pub fn render(f: &mut Frame, app_state: &mut AppState) {
    match app_state.screen {
        crate::tui::state::Screen::Dashboard => dashboard::render(f, app_state),
        crate::tui::state::Screen::Config => config::render(f, app_state),
        crate::tui::state::Screen::Scanning { .. } => scanning::render(f, app_state),
        crate::tui::state::Screen::Results => results::render(f, app_state),
        crate::tui::state::Screen::Preview { .. } => preview::render(f, app_state),
        crate::tui::state::Screen::Confirm { .. } => confirm::render(f, app_state),
        crate::tui::state::Screen::Cleaning { .. } => scanning::render_cleaning(f, app_state),
        crate::tui::state::Screen::Success { .. } => success::render(f, app_state),
        crate::tui::state::Screen::RestoreSelection { .. } => restore_selection::render(f, app_state),
        crate::tui::state::Screen::Restore { .. } => restore::render(f, app_state),
        crate::tui::state::Screen::DiskInsights { .. } => disk_insights::render(f, app_state),
        crate::tui::state::Screen::Optimize { .. } => optimize::render(f, app_state),
    }
}
