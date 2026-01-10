//! Shared spinner animation frames for CLI and TUI
//!
//! Provides consistent spinner animation across both CLI progress indicators
//! and TUI screens.

/// Spinner animation frames (braille-style dots)
/// 
/// These are the same characters used by indicatif's default spinner.
/// Using a shared constant ensures consistency between CLI and TUI.
pub const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Get the spinner character for a given tick value
/// 
/// # Arguments
/// * `tick` - The current tick/frame number (increments over time)
/// 
/// # Returns
/// A reference to the spinner character string for the current frame
/// 
/// # Example
/// ```
/// use crate::spinner::get_spinner;
/// 
/// let frame0 = get_spinner(0);  // "⠋"
/// let frame1 = get_spinner(2);  // "⠙" (tick/2 for slower animation)
/// ```
pub fn get_spinner(tick: u64) -> &'static str {
    SPINNER_FRAMES[(tick as usize / 2) % SPINNER_FRAMES.len()]
}

/// Get spinner frames as a string (for indicatif ProgressBar)
/// 
/// Used by CLI progress indicators that use indicatif.
pub fn spinner_chars() -> &'static str {
    "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spinner_frames_count() {
        assert_eq!(SPINNER_FRAMES.len(), 10);
    }

    #[test]
    fn test_get_spinner_cycles() {
        // Test that spinner cycles through all frames
        let mut seen = std::collections::HashSet::new();
        for i in 0..20 {
            let frame = get_spinner(i);
            seen.insert(frame);
        }
        // Should have seen all frames
        assert_eq!(seen.len(), SPINNER_FRAMES.len());
    }

    #[test]
    fn test_spinner_chars_matches_frames() {
        let chars = spinner_chars();
        let expected: String = SPINNER_FRAMES.iter().copied().collect();
        assert_eq!(chars, expected);
    }
}
