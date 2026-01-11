//! Windows system optimization module
//!
//! Provides Windows equivalents to macOS optimization operations:
//! - DNS cache flush
//! - Thumbnail cache clearing
//! - Icon cache rebuild
//! - Browser database optimization (VACUUM)
//! - Font cache service restart
//! - Standby memory clearing
//! - Network stack reset
//! - Bluetooth service restart
//! - Windows Search service restart
//! - Explorer restart

mod admin_check;
mod operations;
mod printing;
mod result;
mod run;

pub use admin_check::is_admin;
pub use operations::{
    clear_standby_memory, clear_thumbnail_cache, flush_dns_cache, rebuild_icon_cache,
    reset_network_stack, restart_bluetooth_service, restart_explorer, restart_font_cache_service,
    restart_windows_search, vacuum_browser_databases,
};
pub use printing::print_summary;
pub use result::OptimizeResult;
pub use run::run_optimizations;
