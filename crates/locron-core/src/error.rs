use thiserror::Error;

/// A field-level validation failure suitable for CLI and machine rendering.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidationError {
    pub field: &'static str,
    pub code: &'static str,
    pub message: String,
}

impl ValidationError {
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

#[derive(Debug, Error)]
pub enum CoreError {
    #[error(transparent)]
    Validation(#[from] ValidationError),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("durable conflict: {0}")]
    Conflict(String),
    #[error("state is unavailable: {0}")]
    Unavailable(String),
    #[error("persistence failed: {0}")]
    Persistence(String),
    #[error("execution failed: {0}")]
    Execution(String),
    #[error("invalid lifecycle transition: {from} -> {to}")]
    InvalidTransition { from: String, to: String },
}

pub type Result<T> = std::result::Result<T, CoreError>;
