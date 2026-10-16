//! Field-level validation failures and the durable error vocabulary.

use thiserror::Error;

/// Safe, bounded facts about one persistence operation, without user data or SQL.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct PersistenceDiagnostic {
    /// Static name of the operation.
    pub operation: &'static str,
    /// Static name of its last observed stage.
    pub stage: &'static str,
    /// Failure classification, or `success` for a completed observation.
    pub kind: &'static str,
    /// Total measured elapsed microseconds.
    pub elapsed_us: u64,
    /// Time waiting for cooperative intent and writer admission.
    pub admission_wait_us: u64,
    /// Time waiting for the connection mutex.
    pub connection_wait_us: u64,
    /// Time waiting for SQLite before a transaction began.
    pub sqlite_wait_us: u64,
    /// Measured duration of write ownership inside SQLite.
    pub transaction_us: u64,
    /// Measured commit call duration.
    pub commit_us: u64,
    /// Number of safe contention retries.
    pub retries: u32,
    /// Original numeric SQLite primary code when supplied by SQLite.
    pub sqlite_primary_code: Option<i32>,
    /// Original numeric SQLite extended code when supplied by SQLite.
    pub sqlite_extended_code: Option<i32>,
    /// Original numeric operating-system code when supplied by the OS.
    pub os_error_code: Option<i32>,
    /// Whether a caller may retry the complete request after this refusal.
    pub retryable: bool,
    /// `not_committed`, `unknown`, or `committed` for a successful observation.
    pub durable_outcome: &'static str,
    /// `unknown`: advisory lock contention does not authenticate a holder.
    pub lock_owner: &'static str,
}

impl std::fmt::Display for PersistenceDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "storage {} failed at {}: {}; elapsed {} ms, admission wait {} ms, connection wait {} ms, SQLite wait {} ms, retries {}",
            self.operation,
            self.stage,
            self.kind,
            self.elapsed_us / 1_000,
            self.admission_wait_us / 1_000,
            self.connection_wait_us / 1_000,
            self.sqlite_wait_us / 1_000,
            self.retries
        )?;
        if let Some(code) = self.sqlite_primary_code {
            write!(f, "; SQLite primary {code}")?;
        }
        if let Some(code) = self.sqlite_extended_code {
            write!(f, ", extended {code}")?;
        }
        if let Some(code) = self.os_error_code {
            write!(f, "; OS error {code}")?;
        }
        write!(
            f,
            "; transaction {} us, commit {} us",
            self.transaction_us, self.commit_us
        )?;
        write!(
            f,
            "; durable outcome {}, lock owner unknown; {}",
            self.durable_outcome,
            if self.retryable {
                "retry the request after competing work finishes"
            } else if self.kind == "locked" {
                "a resource is locked; close conflicting readers or restart the affected client before retrying"
            } else {
                "inspect state and storage diagnostics before retrying"
            }
        )
    }
}

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
    /// Structured safe adapter facts retained across the application boundary.
    #[error("{0}")]
    PersistenceDiagnostic(Box<PersistenceDiagnostic>),
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

impl CoreError {
    /// Returns structured persistence facts without exposing adapter inputs.
    #[must_use]
    pub fn persistence_diagnostic(&self) -> Option<&PersistenceDiagnostic> {
        match self {
            Self::PersistenceDiagnostic(diagnostic) => Some(diagnostic),
            _ => None,
        }
    }
}

/// Crate-wide result type carrying [`CoreError`].
pub type Result<T> = std::result::Result<T, CoreError>;
