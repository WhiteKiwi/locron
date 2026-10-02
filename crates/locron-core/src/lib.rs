//! Storage- and runtime-independent domain behavior for locron.
//!
//! This crate owns normalized values, validation, schedule enumeration,
//! lifecycle transitions and the ports used by the store and engine crates.
//! It exposes no SQLite, CLI or public async-runtime types. Narrow shared
//! filesystem/environment adapters preserve the five-crate dependency direction.

pub mod command;
pub mod error;
pub mod execution;
pub mod filesystem;
pub mod id;
pub mod lifecycle;
pub mod notification;
pub mod policy;
pub mod ports;
pub mod redact;
pub mod schedule;
pub mod target;
pub mod time;
#[cfg(windows)]
pub mod windows;

pub use error::{CoreError, Result, ValidationError};
pub use id::{AttemptNumber, EventId, JobId, RevisionNumber, RunId, SchedulerLifetimeId};
pub use schedule::{
    CompiledSchedule, ElapsedKind, OmittedRange, OmittedRangeKind, ScheduleReconciliation,
    SelectedOccurrence,
};
pub use time::{DurationMicros, Timestamp};
