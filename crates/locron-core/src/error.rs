//! Field-level validation failures and the durable error vocabulary.

use thiserror::Error;

/// A field-level validation failure suitable for CLI and machine rendering.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidationError {
    /// Field or argument the validation failure applies to.
    pub field: &'static str,
    /// Stable machine-readable error code.
    pub code: &'static str,
    /// Human-readable explanation of the failure.
    pub message: String,
}

impl ValidationError {
    /// Builds a field-level validation failure with a stable code and message.
    #[must_use]
    pub fn new(field: &'static str, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            field,
            code,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.field, self.message)
    }
}

impl std::error::Error for ValidationError {}

/// Stable error vocabulary used across the application boundary.
#[derive(Debug, Error)]
pub enum CoreError {
    /// A field-level validation failure.
    #[error(transparent)]
    Validation(#[from] ValidationError),
    /// A durable resource was not found.
    #[error("not found: {0}")]
    NotFound(String),
    /// A durable optimistic-concurrency or state conflict.
    #[error("durable conflict: {0}")]
    Conflict(String),
    /// Required state or service is unavailable.
    #[error("state is unavailable: {0}")]
    Unavailable(String),
    /// Persistence failed with an adapter error.
    #[error("persistence failed: {0}")]
    Persistence(String),
    /// Target execution failed with an adapter error.
    #[error("execution failed: {0}")]
    Execution(String),
    /// The requested lifecycle transition is illegal.
    #[error("invalid lifecycle transition: {from} -> {to}")]
    InvalidTransition {
        /// The state being transitioned from.
        from: String,
        /// The state being transitioned to.
        to: String,
    },
}

/// Crate-wide result type carrying [`CoreError`].
pub type Result<T> = std::result::Result<T, CoreError>;
