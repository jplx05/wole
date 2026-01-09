//! Plain text theme - no colors, no emojis
//! Simple, clean text-only output

/// Plain text formatting utilities
pub struct Theme;

impl Theme {
    /// Plain text (no styling)
    pub fn primary(text: &str) -> String {
        text.to_string()
    }
    
    /// Plain text (no styling)
    pub fn secondary(text: &str) -> String {
        text.to_string()
    }
    
    /// Plain text (no styling)
    pub fn success(text: &str) -> String {
        text.to_string()
    }
    
    /// Plain text (no styling)
    pub fn warning(text: &str) -> String {
        text.to_string()
    }
    
    /// Plain text (no styling)
    pub fn muted(text: &str) -> String {
        text.to_string()
    }
    
    /// Plain text (no styling)
    pub fn subtle(text: &str) -> String {
        text.to_string()
    }
    
    /// Plain text (no styling)
    pub fn accent(text: &str) -> String {
        text.to_string()
    }
    
    /// Plain divider line
    pub fn divider(width: usize) -> String {
        "-".repeat(width)
    }
    
    /// Plain double divider
    pub fn divider_bold(width: usize) -> String {
        "=".repeat(width)
    }
    
    /// Plain text (no styling)
    pub fn category(text: &str) -> String {
        text.to_string()
    }
    
    /// Plain text (no styling)
    pub fn value(text: &str) -> String {
        text.to_string()
    }
    
    /// Plain text (no styling)
    pub fn size(text: &str) -> String {
        text.to_string()
    }
    
    /// Plain text (no styling)
    pub fn status_safe(text: &str) -> String {
        text.to_string()
    }
    
    /// Plain text (no styling)
    pub fn status_review(text: &str) -> String {
        text.to_string()
    }
    
    /// Plain text (no styling)
    pub fn header(text: &str) -> String {
        text.to_string()
    }
    
    /// Plain text (no styling)
    pub fn command(text: &str) -> String {
        text.to_string()
    }
    
    /// Plain text (no emoji)
    pub fn emoji(_text: &str) -> String {
        String::new()
    }
    
    /// Plain text (no styling)
    pub fn error(text: &str) -> String {
        text.to_string()
    }
    
    /// Plain text (no styling)
    pub fn warning_msg(text: &str) -> String {
        text.to_string()
    }
}
