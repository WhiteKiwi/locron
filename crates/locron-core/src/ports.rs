//! Runtime-neutral ports used by orchestration layers.

use crate::{CoreError, Timestamp};

/// Injected wall-clock source for deterministic scheduling tests.
pub trait Clock: Send + Sync {
    /// Current durable UTC instant.
    fn now(&self) -> Timestamp;
}

/// Minimal persistence health operation shared by applications.
pub trait HealthPort: Send + Sync {
    /// Validates durable state and returns an operator-facing report.
    fn check(&self) -> Result<String, CoreError>;
}
