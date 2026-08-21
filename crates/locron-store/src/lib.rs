//! Durable SQLite and output-artifact storage for locron.
//!
//! The public requests intentionally carry normalized, versioned JSON snapshots.
//! Validation and normalization belong to `locron-core`; this crate owns atomicity,
//! persistence constraints, state paths, and filesystem consistency.

mod lock;
mod migration;
mod output;
mod paths;
mod store;

pub use lock::{DaemonLock, LockMetadata};
pub use migration::{APPLICATION_ID, LATEST_SCHEMA_VERSION};
pub use output::{
    FRAME_HEADER_LEN, Frame, FrameChannel, FrameReader, FrameWriter, MAX_FRAME_PAYLOAD,
    OutputRepair, repair_partial,
};
pub use paths::StatePaths;
pub use store::{
    Admission, AdmitAttempt, AttemptCompletion, CreateJob, CursorUpdate, ImportBatch, ImportJob,
    ImportResolution, ImportSummary, JobIdentity, JobRecord, MaterializedRun, NewScheduledRun,
    OutputRecord, RetentionCandidate, RetryPlan, RunRecord, SettingsRecord, Store, StoreError,
    StoreResult, UpdateJob,
};
