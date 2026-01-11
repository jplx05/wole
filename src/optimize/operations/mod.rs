//! Optimization operation features.

pub mod clear_standby_memory;
pub mod clear_thumbnail_cache;
pub mod flush_dns_cache;
pub mod rebuild_icon_cache;
pub mod reset_network_stack;
pub mod restart_bluetooth_service;
pub mod restart_explorer;
pub mod restart_font_cache_service;
pub mod restart_windows_search;
pub mod vacuum_browser_databases;

pub use clear_standby_memory::clear_standby_memory;
pub use clear_thumbnail_cache::clear_thumbnail_cache;
pub use flush_dns_cache::flush_dns_cache;
pub use rebuild_icon_cache::rebuild_icon_cache;
pub use reset_network_stack::reset_network_stack;
pub use restart_bluetooth_service::restart_bluetooth_service;
pub use restart_explorer::restart_explorer;
pub use restart_font_cache_service::restart_font_cache_service;
pub use restart_windows_search::restart_windows_search;
pub use vacuum_browser_databases::vacuum_browser_databases;
