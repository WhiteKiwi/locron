//! Runtime-neutral ports used by orchestration layers.

use jiff::tz::TimeZone;

use crate::{CoreError, Timestamp};

/// Injected wall-clock source for deterministic scheduling tests.
pub trait Clock: Send + Sync {
    /// Current durable UTC instant.
    fn now(&self) -> Timestamp;
    /// Process-local monotonic elapsed time for wall-clock discontinuity
    /// detection between reconciliation samples.
    fn monotonic_micros(&self) -> u64;
}

/// Resolves the symbolic system-local timezone once per engine pass.
pub trait TimeZoneResolver: Send + Sync {
    /// Current system-local timezone snapshot.
    fn local_timezone(&self) -> Result<TimeZone, CoreError>;
}

/// Minimal persistence health operation shared by applications.
pub trait HealthPort: Send + Sync {
    /// Validates durable state and returns an operator-facing report.
    fn check(&self) -> Result<String, CoreError>;
}
