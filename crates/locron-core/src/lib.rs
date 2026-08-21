//! Storage- and runtime-independent domain behavior for locron.
//!
//! This crate owns normalized values, validation, schedule enumeration,
//! lifecycle transitions and the ports used by the store and engine crates.
//! It intentionally exposes no SQLite, operating-system, CLI, or async-runtime
//! types.

pub mod command;
pub mod error;
pub mod id;
pub mod lifecycle;
pub mod policy;
pub mod ports;
pub mod schedule;
pub mod target;
pub mod time;

pub use error::{CoreError, Result, ValidationError};
pub use id::{JobId, RunId, SchedulerLifetimeId};
pub use schedule::{
    CompiledSchedule, ElapsedKind, OmittedRange, OmittedRangeKind, ScheduleReconciliation,
    SelectedOccurrence,
};
pub use time::{DurationMicros, Timestamp};
