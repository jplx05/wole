use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;

use crate::spinner;

/// Create a spinner for indeterminate progress
pub fn create_spinner(msg: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .tick_chars(spinner::spinner_chars())
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    pb.set_message(msg.to_string());
    pb.enable_steady_tick(Duration::from_millis(80));
    pb
}

/// Create a progress bar for determinate progress
pub fn create_progress_bar(total: u64, msg: &str) -> ProgressBar {
    let pb = ProgressBar::new(total);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("█▓░"),
    );
    pb.set_message(msg.to_string());
    pb.enable_steady_tick(Duration::from_millis(100));
    pb
}

/// Create a progress bar with ETA display
///
/// Shows: spinner, progress bar, position/total, items per second, ETA, and message
pub fn create_progress_bar_with_eta(total: u64, msg: &str) -> ProgressBar {
    let pb = ProgressBar::new(total);
    pb.set_style(
        ProgressStyle::default_bar()
            .template(
                "{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} ({per_sec}) ETA: {eta} {msg}",
            )
            .unwrap()
            .progress_chars("█▓░"),
    );
    pb.set_message(msg.to_string());
    pb.enable_steady_tick(Duration::from_millis(100));
    pb
}

/// Create a bytes-based progress bar with ETA (for cleaning operations)
///
/// Shows: spinner, progress bar, bytes/total_bytes, throughput, ETA, and message
pub fn create_bytes_progress_bar(total_bytes: u64, msg: &str) -> ProgressBar {
    let pb = ProgressBar::new(total_bytes);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}) ETA: {eta} {msg}")
            .unwrap()
            .progress_chars("█▓░")
    );
    pb.set_message(msg.to_string());
    pb.enable_steady_tick(Duration::from_millis(100));
    pb
}

/// Create a scanning progress bar (for multi-category scans)
///
/// Shows: spinner, progress bar, position/total categories, elapsed time, message
pub fn create_scan_progress_bar(total: u64, msg: &str) -> ProgressBar {
    let pb = ProgressBar::new(total);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} ({elapsed_precise}) {msg}")
            .unwrap()
            .progress_chars("█▓░"),
    );
    pb.set_message(msg.to_string());
    pb.enable_steady_tick(Duration::from_millis(100));
    pb
}

/// Finish progress bar with a success message
pub fn finish_with_message(pb: &ProgressBar, msg: &str) {
    pb.finish_with_message(msg.to_string());
}

/// Finish and clear progress bar
pub fn finish_and_clear(pb: &ProgressBar) {
    pb.finish_and_clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_spinner() {
        let pb = create_spinner("Test spinner");
        assert!(!pb.is_finished());
        pb.finish();
        assert!(pb.is_finished());
    }

    #[test]
    fn test_create_progress_bar() {
        let pb = create_progress_bar(100, "Test progress");
        assert_eq!(pb.length(), Some(100));
        assert_eq!(pb.position(), 0);
        pb.inc(50);
        assert_eq!(pb.position(), 50);
        pb.finish();
    }

    #[test]
    fn test_create_progress_bar_with_eta() {
        let pb = create_progress_bar_with_eta(100, "Test ETA progress");
        assert_eq!(pb.length(), Some(100));
        pb.inc(25);
        assert_eq!(pb.position(), 25);
        pb.finish();
    }

    #[test]
    fn test_create_bytes_progress_bar() {
        let pb = create_bytes_progress_bar(1024 * 1024, "Test bytes progress");
        assert_eq!(pb.length(), Some(1024 * 1024));
        pb.inc(512 * 1024);
        assert_eq!(pb.position(), 512 * 1024);
        pb.finish();
    }

    #[test]
    fn test_create_scan_progress_bar() {
        let pb = create_scan_progress_bar(10, "Test scan progress");
        assert_eq!(pb.length(), Some(10));
        pb.inc(5);
        assert_eq!(pb.position(), 5);
        pb.finish();
    }
}
