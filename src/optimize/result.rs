//! Optimization result feature.

/// Result of an optimization operation
#[derive(Debug, Clone)]
pub struct OptimizeResult {
    /// Name of the action performed
    pub action: String,
    /// Whether the operation succeeded
    pub success: bool,
    /// Human-readable message about the result
    pub message: String,
    /// Whether this operation requires administrator privileges
    pub requires_admin: bool,
}

impl OptimizeResult {
    pub(crate) fn success(action: &str, message: &str, requires_admin: bool) -> Self {
        Self {
            action: action.to_string(),
            success: true,
            message: message.to_string(),
            requires_admin,
        }
    }

    pub(crate) fn failure(action: &str, message: &str, requires_admin: bool) -> Self {
        Self {
            action: action.to_string(),
            success: false,
            message: message.to_string(),
            requires_admin,
        }
    }

    pub(crate) fn skipped(action: &str, message: &str, requires_admin: bool) -> Self {
        Self {
            action: action.to_string(),
            success: true, // Skipped is considered "success" (not an error)
            message: format!("Skipped: {}", message),
            requires_admin,
        }
    }
}
