use std::collections::{BTreeMap, VecDeque};
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use locron_core::command::{
    AddJob as AddJobCommand, AddJobResult, CancelRun, CancelRunResult, CancellationDecision,
    Configuration, ConfigurationChange, ManualRun, ManualRunResult, RemoveJob, RemoveJobResult,
    SetJobEnabled, SetJobEnabledResult, UpdateConfiguration, UpdateJob as UpdateJobCommand,
    UpdateJobResult,
};
use locron_core::lifecycle::RunState;
use locron_core::ports::PersistencePort;
use locron_core::{CoreError, DurationMicros, JobId, RevisionNumber};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use thiserror::Error;
#[cfg(test)]
use uuid::Uuid;

use crate::migration::migrate;
use crate::{DaemonLock, LockMetadata, StatePaths};

type AdmissionRow = (String, String, String, i64, String, Option<i64>);
const MAINTENANCE_BATCH_LIMIT: usize = 100;

/// Result type returned by store operations, carrying a [`StoreError`] on failure.
pub type StoreResult<T> = Result<T, StoreError>;

/// Errors produced by store operations.
#[derive(Debug, Error)]
pub enum StoreError {
    /// Filesystem or OS-level I/O failure.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// SQLite driver failure, including busy and locked conditions.
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    /// Failure to serialize or deserialize JSON stored in the database.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// Another locron daemon owns this state directory.
    #[error("another locron daemon owns this state directory")]
    DaemonAlreadyRunning,
    /// The state directory is held by a daemon that must restart to apply a
    /// pending database migration.
    #[error("database migration requires the running daemon to restart")]
    MigrationRequiresDaemonRestart,
    /// The database schema is newer than the schema this binary supports.
    #[error("database schema {found} is newer than supported schema {supported}")]
    SchemaTooNew {
        /// Schema version recorded in the database.
        found: i64,
        /// Highest schema version this binary supports.
        supported: i64,
    },
    /// The database application id does not identify locron state.
    #[error("database application id {0:#x} does not identify locron state")]
    NotLocronDatabase(i32),
    /// A recorded migration checksum does not match the migration this binary
    /// ships with.
    #[error("migration {version} checksum mismatch: expected {expected}, found {found}")]
    MigrationChecksumMismatch {
        /// Migration version whose checksum mismatched.
        version: i64,
        /// Checksum this binary computed for the migration.
        expected: String,
        /// Checksum recorded in the database.
        found: String,
    },
    /// A migration record is missing for the given version.
    #[error("migration record {0} is missing")]
    MissingMigration(i64),
    /// Migration raced another concurrent initializer.
    #[error("migration raced another initializer")]
    MigrationConflict,
    /// The state directory cannot be discovered (no platform default).
    #[error("state directory cannot be discovered")]
    StateDirectoryUnavailable,
    /// A managed path is unsafe (symlink or unexpected file type).
    #[error("unsafe managed path: {0}")]
    UnsafePath(std::path::PathBuf),
    /// An identity (for example a UUID) is invalid.
    #[error("invalid identity: {0}")]
    InvalidIdentity(String),
    /// The referenced entity does not exist.
    #[error("not found: {0}")]
    NotFound(String),
    /// A durable conflict: optimistic revision mismatch, identity collision,
    /// or state that is no longer applicable.
    #[error("durable conflict: {0}")]
    Conflict(String),
}

/// Error produced while adapting a core application command to SQLite.
#[derive(Debug, Error)]
pub enum StorePortError {
    /// A core application command validation error.
    #[error(transparent)]
    Core(#[from] CoreError),
    /// A store-level failure.
    #[error(transparent)]
    Store(#[from] StoreError),
    /// A JSON serialization failure.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// Input for creating a new job at revision 1.
#[derive(Clone, Debug)]
pub struct CreateJob {
    /// Canonical lowercase UUID of the new job.
    pub id: String,
    /// Live job name, unique among non-removed jobs.
    pub name: String,
    /// Optional human-readable description.
    pub description: Option<String>,
    /// Job tags serialized as a JSON array.
    pub tags_json: String,
    /// Whether the job starts enabled.
    pub enabled: bool,
    /// Job definition (schedule, target, policy) serialized as JSON.
    pub definition_json: String,
    /// Wall-clock time in microseconds recorded as the creation timestamp.
    pub now_us: i64,
    /// Initial schedule cursor in microseconds.
    pub cursor_us: i64,
}

/// Input for replacing a job's fields, bumping its revision.
#[derive(Clone, Debug)]
pub struct UpdateJob {
    /// Canonical lowercase UUID of the job to update.
    pub id: String,
    /// Revision the job must currently hold; a mismatch returns
    /// [`StoreError::Conflict`].
    pub expected_revision: i64,
    /// Live job name, unique among non-removed jobs.
    pub name: String,
    /// Optional human-readable description.
    pub description: Option<String>,
    /// Job tags serialized as a JSON array.
    pub tags_json: String,
    /// Whether the job is enabled after the update.
    pub enabled: bool,
    /// Job definition (schedule, target, policy) serialized as JSON.
    pub definition_json: String,
    /// Wall-clock time in microseconds recorded as the update timestamp.
    pub now_us: i64,
    /// New schedule cursor in microseconds.
    pub cursor_us: i64,
}

/// A full snapshot of one job at its current revision.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JobRecord {
    /// Canonical UUID of the job.
    pub id: String,
    /// Live name of the job.
    pub name: String,
    /// Optional human-readable description.
    pub description: Option<String>,
    /// Job tags serialized as a JSON array.
    pub tags_json: String,
    /// Whether the job is currently enabled.
    pub enabled: bool,
    /// Wall-clock time in microseconds when the job was soft-deleted, or
    /// `None` while the job is live.
    pub removed_at_us: Option<i64>,
    /// Revision of the current definition, incremented on every update.
    pub current_revision: i64,
    /// Definition of the current revision, serialized as JSON.
    pub definition_json: String,
    /// Schedule cursor of the current revision in microseconds.
    pub cursor_us: i64,
    /// Wall-clock time in microseconds of the last job-row update.
    pub updated_at_us: i64,
    /// Wall-clock time in microseconds when the current cursor row was
    /// written.
    pub cursor_updated_at_us: i64,
    /// Wall-clock time in microseconds when the job entered its current
    /// disabled period; `None` while enabled or after a cursor update.
    pub disabled_since_us: Option<i64>,
}

/// Input for materializing one scheduled run occurrence.
#[derive(Clone, Debug)]
pub struct NewScheduledRun {
    /// Canonical UUID of the new run.
    pub id: String,
    /// UUID of the job the run belongs to.
    pub job_id: String,
    /// Job revision whose definition snapshot produced this run.
    pub revision: i64,
    /// Trigger kind: `scheduled` or `catch_up`.
    pub trigger: String,
    /// Nominal schedule time in microseconds.
    pub nominal_us: i64,
    /// Wall-clock time in microseconds when the run was requested.
    pub requested_at_us: i64,
    /// Wall-clock time in microseconds before which the run must not be
    /// admitted.
    pub eligible_at_us: i64,
    /// Job definition snapshot at this revision, serialized as JSON; the
    /// admission policy is read from it.
    pub snapshot_json: String,
}

/// Input for advancing a job's schedule cursor atomically with materializing
/// its runs.
#[derive(Clone, Debug)]
pub struct CursorUpdate {
    /// Job revision the cursor update applies to; must match the current
    /// revision.
    pub expected_revision: i64,
    /// Cursor value the update is based on; a mismatch returns
    /// [`StoreError::Conflict`].
    pub expected_cursor_us: i64,
    /// New cursor value in microseconds.
    pub new_cursor_us: i64,
    /// When `true`, the job's one-time schedule is resolved and the job is
    /// disabled atomically with the cursor update.
    pub resolve_one_time: bool,
}

/// Counts produced by one materialization pass.
#[derive(Clone, Debug, Default)]
pub struct MaterializedRun {
    /// Number of runs newly inserted.
    pub inserted: usize,
    /// Number of runs already present that were ignored.
    pub duplicates: usize,
}

/// A compact event recorded for a reconciliation range, atomically with the
/// cursor that caused it.
#[derive(Clone, Debug)]
pub struct ReconciliationSummary {
    /// Event kind recorded for the range (for example
    /// `missed_start_deadline`).
    pub kind: String,
    /// Number of occurrences reconciled in the range; must be positive.
    pub count: u64,
    /// First nominal schedule time in microseconds of the range.
    pub first_nominal_us: i64,
    /// Last nominal schedule time in microseconds of the range; must be at
    /// least `first_nominal_us`.
    pub last_nominal_us: i64,
}

/// One row of the append-only event log.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EventRecord {
    /// Monotonic event row id.
    pub id: i64,
    /// Wall-clock time in microseconds when the event occurred.
    pub occurred_at_us: i64,
    /// Event kind (for example `job_added`, `run_cancelled`).
    pub kind: String,
    /// Job the event concerns, when any.
    pub job_id: Option<String>,
    /// Run the event concerns, when any.
    pub run_id: Option<String>,
    /// Event-specific details serialized as JSON.
    pub details_json: String,
}

/// A full snapshot of one run.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunRecord {
    /// Canonical UUID of the run.
    pub id: String,
    /// UUID of the job the run belongs to.
    pub job_id: String,
    /// Job revision whose snapshot produced the run.
    pub revision: i64,
    /// Trigger kind: `manual`, `scheduled`, or `catch_up`.
    pub trigger: String,
    /// Nominal schedule time in microseconds; `None` for manual runs.
    pub nominal_us: Option<i64>,
    /// Wall-clock time in microseconds when the run was requested.
    pub requested_at_us: i64,
    /// Wall-clock time in microseconds before which the run must not be
    /// admitted.
    pub eligible_at_us: i64,
    /// Run state: `queued`, `starting`, `running`, `retry_wait`, or a terminal
    /// state (`succeeded`, `failed`, `timed_out`, `cancelled`,
    /// `skipped_overlap`, `skipped_concurrency`, `interrupted_unknown`).
    pub state: String,
    /// Human-readable reason for the state, when set.
    pub reason: Option<String>,
    /// Job definition snapshot at this revision, serialized as JSON.
    pub snapshot_json: String,
    /// Wall-clock time in microseconds when the run reached a terminal state;
    /// `None` while the run is active.
    pub finished_at_us: Option<i64>,
}

/// The output artifact row of one attempt.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AttemptOutputRecord {
    /// Artifact state: `pending`, `active`, `finalized`, `missing`,
    /// `prune_pending`, or `pruned`.
    pub state: String,
    /// Payload bytes retained after truncation.
    pub retained_payload_bytes: i64,
    /// Bytes physically written to disk.
    pub physical_bytes: i64,
    /// Bytes discarded by truncation.
    pub discarded_bytes: i64,
    /// Whether the output exceeded its limit and was truncated.
    pub truncated: bool,
    /// Wall-clock time in microseconds when truncation occurred.
    pub truncated_at_us: Option<i64>,
    /// Wall-clock time in microseconds when the artifact was finalized.
    pub finalized_at_us: Option<i64>,
    /// Wall-clock time in microseconds when pruning of the artifact started.
    pub prune_started_at_us: Option<i64>,
    /// Wall-clock time in microseconds when the artifact was pruned.
    pub pruned_at_us: Option<i64>,
}

/// A full snapshot of one attempt, with its output artifact when present.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AttemptRecord {
    /// UUID of the run the attempt belongs to.
    pub run_id: String,
    /// One-based attempt number within the run.
    pub attempt_number: i64,
    /// Wall-clock time in microseconds when the attempt was admitted.
    pub started_at_us: i64,
    /// Wall-clock time in microseconds when the attempt began executing;
    /// `None` until spawn.
    pub running_at_us: Option<i64>,
    /// Wall-clock time in microseconds when the attempt finished; `None`
    /// while active.
    pub finished_at_us: Option<i64>,
    /// Elapsed duration of the attempt in microseconds; `None` while active.
    pub duration_us: Option<i64>,
    /// Attempt state: `starting`, `running`, or a terminal state (`succeeded`,
    /// `failed`, `timed_out`, `cancelled`, `interrupted_unknown`).
    pub state: String,
    /// Result class recorded at completion (for example `succeeded`,
    /// `termination_unconfirmed`, `output_preparation_failed`).
    pub outcome: Option<String>,
    /// Process exit code, when the target exited.
    pub exit_code: Option<i32>,
    /// HTTP status code, when the target is an HTTP request.
    pub http_status: Option<u16>,
    /// Content type of the final HTTP response, when applicable.
    pub http_content_type: Option<String>,
    /// Absolute path of the executable the attempt was spawned with, once
    /// resolved.
    pub resolved_executable: Option<String>,
    /// Error message recorded at completion.
    pub error: Option<String>,
    /// Human-readable reason for the attempt state.
    pub reason: Option<String>,
    /// Output artifact of this attempt, when one exists.
    pub output: Option<AttemptOutputRecord>,
}

/// One attempt admitted by [`Store::admit`].
#[derive(Clone, Debug)]
pub struct AdmitAttempt {
    /// UUID of the run being admitted.
    pub run_id: String,
    /// UUID of the job the run belongs to.
    pub job_id: String,
    /// Attempt number assigned by admission (one past the run's highest).
    pub attempt_number: i64,
    /// Run trigger kind.
    pub trigger: String,
    /// Nominal schedule time in microseconds; `None` for manual runs.
    pub nominal_us: Option<i64>,
    /// Job definition snapshot serialized as JSON, needed to launch the
    /// attempt.
    pub snapshot_json: String,
}
/// The attempts admitted by one [`Store::admit`] pass.
#[derive(Clone, Debug, Default)]
pub struct Admission {
    /// Admitted attempts, in admission order.
    pub attempts: Vec<AdmitAttempt>,
}

/// Outcome of advancing an admitted attempt past the pre-spawn boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StartDecision {
    /// The attempt may spawn; the run advanced to `running`.
    Ready,
    /// The run was cancelled or superseded before spawn; the attempt is
    /// durably terminal.
    CancelledBeforeSpawn,
}

/// Outcome of cancelling a run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CancelOutcome {
    /// The run was queued or retry-waiting and is now cancelled.
    CancelledBeforeExecution,
    /// The run was starting or running; a durable cancellation request was
    /// recorded for the runner.
    CancellationRequested,
    /// A termination-unconfirmed quarantine was acknowledged by an operator;
    /// the run is durably `interrupted_unknown`.
    AcknowledgedUnconfirmed,
}

/// Retry plan attached to a failed or timed-out attempt completion.
#[derive(Clone, Debug)]
pub struct RetryPlan {
    /// Earliest wall-clock time in microseconds at which the retry may be
    /// admitted.
    pub not_before_us: i64,
    /// Retry classification recorded with the retry intent (for example
    /// `process_exit`, `known_failure`).
    pub classification: String,
}

/// Input for durably recording the outcome of one attempt.
#[derive(Clone, Debug)]
pub struct AttemptCompletion {
    /// UUID of the run the attempt belongs to.
    pub run_id: String,
    /// Attempt number completing.
    pub attempt_number: i64,
    /// Wall-clock time in microseconds of the completion; also the recorded
    /// finished timestamp.
    pub now_us: i64,
    /// Elapsed duration of the attempt in microseconds.
    pub duration_us: i64,
    /// Terminal state to record (`succeeded`, `failed`, `timed_out`,
    /// `cancelled`, `termination_unconfirmed`).
    pub state: String,
    /// Process exit code, when available.
    pub exit_code: Option<i32>,
    /// HTTP status code, when the target is an HTTP request.
    pub http_status: Option<u16>,
    /// Content type of the final HTTP response, when applicable.
    pub http_content_type: Option<String>,
    /// Human-readable reason for the outcome.
    pub reason: String,
    /// Retry plan when the attempt is retryable; `None` for a terminal
    /// no-retry outcome.
    pub retry: Option<RetryPlan>,
}

/// Output artifact facts reported by a completed run.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OutputRecord {
    /// UUID of the run the output belongs to.
    pub run_id: String,
    /// Attempt number the output belongs to.
    pub attempt_number: i64,
    /// Artifact path relative to the outputs directory.
    pub relative_path: String,
    /// Artifact state as observed by the runner; the store records the
    /// durable state itself.
    pub state: String,
    /// Payload bytes retained after truncation.
    pub retained_payload_bytes: i64,
    /// Bytes physically written to disk.
    pub physical_bytes: i64,
    /// Bytes discarded by truncation.
    pub discarded_bytes: i64,
    /// Whether the output exceeded its limit and was truncated.
    pub truncated: bool,
}

/// A finalized output artifact selected for pruning.
#[derive(Clone, Debug)]
pub struct RetentionCandidate {
    /// UUID of the run the artifact belongs to.
    pub run_id: String,
    /// Attempt number the artifact belongs to.
    pub attempt_number: i64,
    /// Artifact path relative to the outputs directory.
    pub relative_path: String,
    /// Bytes the artifact occupies on disk.
    pub physical_bytes: i64,
    /// Wall-clock time in microseconds when the artifact was finalized (or
    /// when its prune started, for re-listed pending prunes).
    pub finalized_at_us: i64,
}

/// A pending or active output artifact whose attempt is a recovery candidate
/// (terminal, or owned by a scheduler lifetime other than the live daemon's).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutputRecoveryCandidate {
    /// UUID of the run the artifact belongs to.
    pub run_id: String,
    /// Attempt number the artifact belongs to.
    pub attempt_number: i64,
    /// Artifact path relative to the outputs directory.
    pub relative_path: String,
    /// Artifact state observed (`pending` or `active`).
    pub state: String,
}

/// A terminal run whose metadata is eligible for retention deletion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunRetentionCandidate {
    /// UUID of the run.
    pub run_id: String,
    /// UUID of the job the run belongs to.
    pub job_id: String,
    /// Wall-clock time in microseconds when the run finished.
    pub finished_at_us: i64,
}

/// The global settings singleton.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SettingsRecord {
    /// Maximum number of attempts admitted at once, from 1 through 64.
    pub global_concurrency: i64,
    /// Directory in which scheduled processes are executed.
    pub execution_path: String,
    /// Maximum number of terminal runs kept globally; older runs become
    /// retention candidates.
    pub run_retention_count: i64,
    /// Maximum age in microseconds of terminal runs before they become
    /// retention candidates; `None` disables age-based retention.
    pub run_retention_age_us: Option<i64>,
    /// Global cap on retained output bytes across all finalized artifacts.
    pub output_limit_bytes: i64,
    /// Cap on retained payload bytes per run.
    pub per_run_output_limit_bytes: i64,
    /// Global environment variables exported to every scheduled process.
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
}

/// The identity facts of one job row.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JobIdentity {
    /// Canonical UUID of the job.
    pub id: String,
    /// Name of the job.
    pub name: String,
    /// Whether the job has been soft-deleted.
    pub removed: bool,
}

/// The expected mapping of one job between the exporting database and this
/// one.
#[derive(Clone, Debug)]
pub struct ImportResolution {
    /// Identity of the job in the exporting database.
    pub source_id: String,
    /// Name of the job in the exporting database.
    pub source_name: String,
    /// Job UUID the source id is expected to map to locally, when known.
    pub expected_id_destination: Option<String>,
    /// Job UUID the source name is expected to map to locally, when known.
    pub expected_name_destination: Option<String>,
}

/// One job operation of an import batch.
#[derive(Clone, Debug)]
pub enum ImportJob {
    /// Create a new job locally.
    Create {
        /// Job to create.
        job: CreateJob,
        /// Expected source-to-destination mapping.
        resolution: ImportResolution,
    },
    /// Update an existing local job with the exported revision.
    Update {
        /// Job fields to apply.
        job: UpdateJob,
        /// Expected source-to-destination mapping.
        resolution: ImportResolution,
    },
    /// Verify the destination still matches the exported revision; no changes
    /// are written.
    Verify {
        /// Job fields the destination must still match.
        job: UpdateJob,
        /// Expected source-to-destination mapping.
        resolution: ImportResolution,
    },
}

/// One atomic import of settings and jobs.
#[derive(Clone, Debug)]
pub struct ImportBatch {
    /// Global settings to apply with the import.
    pub settings: SettingsRecord,
    /// Jobs to create, update, or verify, in order.
    pub jobs: Vec<ImportJob>,
    /// Wall-clock time in microseconds used for all timestamps of the import.
    pub now_us: i64,
}

/// Counts produced by one import.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ImportSummary {
    /// Jobs created by the import.
    pub created: usize,
    /// Jobs updated by the import.
    pub updated: usize,
}

/// Thread-safe serialized SQLite store.
pub struct Store {
    paths: StatePaths,
    connection: Mutex<Connection>,
}

impl Store {
    /// Opens the store at `paths`, creating the state layout and running any
    /// pending schema migrations for `binary_version`, and returns a store
    /// backed by a configured WAL connection. The daemon lock is acquired
    /// separately via [`Store::acquire_daemon_lock`].
    pub fn open(paths: StatePaths, binary_version: &str, now_us: i64) -> StoreResult<Self> {
        paths.ensure()?;
        let mut connection = Connection::open(&paths.database)?;
        configure(&connection)?;
        migrate(&mut connection, binary_version, now_us)?;
        Ok(Self {
            paths,
            connection: Mutex::new(connection),
        })
    }

    /// Opens an existing `state.db` file read-only from `path`, without
    /// running migrations or touching the daemon lock. The store's state
    /// paths are derived from the database file's parent directory.
    pub fn open_read_only(path: &Path) -> StoreResult<Self> {
        let paths = StatePaths::new(path.parent().unwrap_or(Path::new(".")).to_path_buf());
        let connection =
            Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        configure_read_only(&connection)?;
        Ok(Self {
            paths,
            connection: Mutex::new(connection),
        })
    }

    /// Returns the state paths this store was opened with.
    #[must_use]
    pub fn paths(&self) -> &StatePaths {
        &self.paths
    }
    fn conn(&self) -> StoreResult<MutexGuard<'_, Connection>> {
        self.connection
            .lock()
            .map_err(|_| StoreError::Conflict("store mutex poisoned".into()))
    }

    /// Acquires the daemon lock for this state directory, writing `metadata`
    /// as the lock diagnostic. Returns [`StoreError::DaemonAlreadyRunning`]
    /// when another daemon holds the lock.
    pub fn acquire_daemon_lock(&self, metadata: &LockMetadata) -> StoreResult<DaemonLock> {
        DaemonLock::acquire(&self.paths.daemon_lock, metadata)
    }

    /// Creates a job at revision 1 in one immediate transaction and returns
    /// its record. The id must be a canonical lowercase UUID; an existing id
    /// or live name returns [`StoreError::Conflict`].
    pub fn create_job(&self, job: &CreateJob) -> StoreResult<JobRecord> {
        crate::paths::validate_uuid(&job.id)?;
        let mut conn = self.conn()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let identity_exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM jobs WHERE id=?1)",
            [&job.id],
            |row| row.get(0),
        )?;
        let live_name_exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM jobs WHERE name=?1 AND removed_at_us IS NULL)",
            [&job.name],
            |row| row.get(0),
        )?;
        if identity_exists || live_name_exists {
            return Err(StoreError::Conflict(format!(
                "job identity or live name already exists: {}",
                job.name
            )));
        }
        tx.execute("INSERT INTO jobs(id,name,description,tags_json,enabled,created_at_us,updated_at_us,current_revision) VALUES(?1,?2,?3,?4,?5,?6,?6,1)", params![job.id, job.name, job.description, job.tags_json, job.enabled, job.now_us])?;
        tx.execute("INSERT INTO job_revisions(job_id,revision,definition_json,created_at_us,created_by) VALUES(?1,1,?2,?3,'add')", params![job.id, job.definition_json, job.now_us])?;
        tx.execute("INSERT INTO schedule_cursors(job_id,revision,cursor_us,interval_anchor_us,updated_at_us,disabled_since_us) VALUES(?1,1,?2,NULL,?3,CASE WHEN ?4 THEN NULL ELSE ?3 END)", params![job.id, job.cursor_us, job.now_us, job.enabled])?;
        event(&tx, job.now_us, "job_added", Some(&job.id), None, "{}")?;
        tx.commit()?;
        drop(conn);
        self.job(&job.id)
    }

    /// Applies a full job update in one immediate transaction, bumping the
    /// revision by one. `expected_revision` must match the job's current
    /// revision or [`StoreError::Conflict`] is returned; an unknown reference
    /// returns [`StoreError::NotFound`].
    pub fn update_job(&self, job: &UpdateJob) -> StoreResult<JobRecord> {
        let mut conn = self.conn()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current: i64 = tx
            .query_row(
                "SELECT current_revision FROM jobs WHERE id=?1 AND removed_at_us IS NULL",
                [&job.id],
                |row| row.get(0),
            )
            .optional()?
            .ok_or_else(|| StoreError::NotFound(job.id.clone()))?;
        if current != job.expected_revision {
            return Err(StoreError::Conflict(format!(
                "expected revision {}, found {current}",
                job.expected_revision
            )));
        }
        let other_name: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM jobs WHERE name=?1 AND id<>?2 AND removed_at_us IS NULL)",
            params![job.name, job.id],
            |row| row.get(0),
        )?;
        if other_name {
            return Err(StoreError::Conflict(format!(
                "job live name already exists: {}",
                job.name
            )));
        }
        let revision = current + 1;
        tx.execute("UPDATE jobs SET name=?2,description=?3,tags_json=?4,enabled=?5,updated_at_us=?6,current_revision=?7 WHERE id=?1", params![job.id,job.name,job.description,job.tags_json,job.enabled,job.now_us,revision])?;
        tx.execute(
            "INSERT INTO job_revisions VALUES(?1,?2,?3,?4,'update')",
            params![job.id, revision, job.definition_json, job.now_us],
        )?;
        tx.execute("INSERT INTO schedule_cursors(job_id,revision,cursor_us,interval_anchor_us,updated_at_us,disabled_since_us) VALUES(?1,?2,?3,NULL,?4,CASE WHEN ?5 THEN NULL ELSE ?4 END)", params![job.id,revision,job.cursor_us,job.now_us,job.enabled])?;
        event(
            &tx,
            job.now_us,
            "job_updated",
            Some(&job.id),
            None,
            &format!("{{\"revision\":{revision}}}"),
        )?;
        tx.commit()?;
        drop(conn);
        self.job(&job.id)
    }

    /// Returns the live job matching `reference` by id or name, or
    /// [`StoreError::NotFound`] when no live job matches.
    pub fn job(&self, reference: &str) -> StoreResult<JobRecord> {
        let conn = self.conn()?;
        conn.query_row(
            "SELECT j.id,j.name,j.description,j.tags_json,j.enabled,j.removed_at_us,j.current_revision,r.definition_json,c.cursor_us,j.updated_at_us,c.updated_at_us,c.disabled_since_us FROM jobs j JOIN job_revisions r ON r.job_id=j.id AND r.revision=j.current_revision JOIN schedule_cursors c ON c.job_id=j.id AND c.revision=j.current_revision WHERE (j.id=?1 OR j.name=?1) AND j.removed_at_us IS NULL",
            [reference], map_job,
        ).optional()?.ok_or_else(|| StoreError::NotFound(reference.into()))
    }

    /// Lists live jobs ordered by name: always the enabled jobs, plus
    /// disabled ones only when `all` is true.
    pub fn list_jobs(&self, all: bool) -> StoreResult<Vec<JobRecord>> {
        let conn = self.conn()?;
        let mut statement = conn.prepare("SELECT j.id,j.name,j.description,j.tags_json,j.enabled,j.removed_at_us,j.current_revision,r.definition_json,c.cursor_us,j.updated_at_us,c.updated_at_us,c.disabled_since_us FROM jobs j JOIN job_revisions r ON r.job_id=j.id AND r.revision=j.current_revision JOIN schedule_cursors c ON c.job_id=j.id AND c.revision=j.current_revision WHERE j.removed_at_us IS NULL AND (?1 OR j.enabled=1) ORDER BY j.name COLLATE BINARY")?;
        statement
            .query_map([all], map_job)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    /// Lists the id, name, and removed flag of every job row (removed ones
    /// included), ordered by id.
    pub fn job_identities(&self) -> StoreResult<Vec<JobIdentity>> {
        let conn = self.conn()?;
        let mut statement = conn.prepare(
            "SELECT id,name,removed_at_us IS NOT NULL FROM jobs ORDER BY id COLLATE BINARY",
        )?;
        statement
            .query_map([], |row| {
                Ok(JobIdentity {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    removed: row.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    /// Applies a prevalidated import as one immediate transaction. Identity and
    /// optimistic-revision facts are rechecked inside the transaction.
    pub fn apply_import(&self, batch: &ImportBatch) -> StoreResult<ImportSummary> {
        validate_import_settings(&batch.settings)?;
        let mut conn = self.conn()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut summary = ImportSummary::default();
        for import in &batch.jobs {
            let resolution = match import {
                ImportJob::Create { resolution, .. }
                | ImportJob::Update { resolution, .. }
                | ImportJob::Verify { resolution, .. } => resolution,
            };
            validate_import_resolution(&tx, resolution)?;
            match import {
                ImportJob::Create { job, .. } => {
                    crate::paths::validate_uuid(&job.id)?;
                    let id_exists: bool = tx.query_row(
                        "SELECT EXISTS(SELECT 1 FROM jobs WHERE id=?1)",
                        [&job.id],
                        |row| row.get(0),
                    )?;
                    let name_exists: bool = tx.query_row(
                        "SELECT EXISTS(SELECT 1 FROM jobs WHERE name=?1 AND removed_at_us IS NULL)",
                        [&job.name],
                        |row| row.get(0),
                    )?;
                    if id_exists || name_exists {
                        return Err(StoreError::Conflict(format!(
                            "import create identity collision for {}",
                            job.name
                        )));
                    }
                }
                ImportJob::Update { job, .. } | ImportJob::Verify { job, .. } => {
                    let current = import_destination(&tx, &job.id)?;
                    if current.current_revision != job.expected_revision {
                        return Err(StoreError::Conflict(format!(
                            "expected revision {}, found {}",
                            job.expected_revision, current.current_revision
                        )));
                    }
                    let other_name: bool = tx.query_row(
                        "SELECT EXISTS(SELECT 1 FROM jobs WHERE name=?1 AND id<>?2 AND removed_at_us IS NULL)",
                        params![job.name,job.id],
                        |row| row.get(0),
                    )?;
                    if other_name {
                        return Err(StoreError::Conflict(format!(
                            "import update name collision for {}",
                            job.name
                        )));
                    }
                    if matches!(import, ImportJob::Verify { .. })
                        && !import_job_matches(&current, job)
                    {
                        return Err(StoreError::Conflict(format!(
                            "import no-op destination changed for {}",
                            resolution.source_name
                        )));
                    }
                }
            }
        }
        for job in &batch.jobs {
            match job {
                ImportJob::Create { job, .. } => {
                    tx.execute("INSERT INTO jobs(id,name,description,tags_json,enabled,created_at_us,updated_at_us,current_revision) VALUES(?1,?2,?3,?4,?5,?6,?6,1)", params![job.id,job.name,job.description,job.tags_json,job.enabled,batch.now_us])?;
                    tx.execute("INSERT INTO job_revisions(job_id,revision,definition_json,created_at_us,created_by) VALUES(?1,1,?2,?3,'import')", params![job.id,job.definition_json,batch.now_us])?;
                    tx.execute("INSERT INTO schedule_cursors(job_id,revision,cursor_us,interval_anchor_us,updated_at_us,disabled_since_us) VALUES(?1,1,?2,NULL,?3,CASE WHEN ?4 THEN NULL ELSE ?3 END)", params![job.id,job.cursor_us,batch.now_us,job.enabled])?;
                    event(
                        &tx,
                        batch.now_us,
                        "job_imported",
                        Some(&job.id),
                        None,
                        "{\"action\":\"create\"}",
                    )?;
                    summary.created += 1;
                }
                ImportJob::Update { job, .. } => {
                    let revision = job.expected_revision + 1;
                    tx.execute("UPDATE jobs SET name=?2,description=?3,tags_json=?4,enabled=?5,updated_at_us=?6,current_revision=?7 WHERE id=?1", params![job.id,job.name,job.description,job.tags_json,job.enabled,batch.now_us,revision])?;
                    tx.execute("INSERT INTO job_revisions(job_id,revision,definition_json,created_at_us,created_by) VALUES(?1,?2,?3,?4,'import')", params![job.id,revision,job.definition_json,batch.now_us])?;
                    tx.execute("INSERT INTO schedule_cursors(job_id,revision,cursor_us,interval_anchor_us,updated_at_us,disabled_since_us) VALUES(?1,?2,?3,NULL,?4,CASE WHEN ?5 THEN NULL ELSE ?4 END)", params![job.id,revision,job.cursor_us,batch.now_us,job.enabled])?;
                    event(
                        &tx,
                        batch.now_us,
                        "job_imported",
                        Some(&job.id),
                        None,
                        "{\"action\":\"update\"}",
                    )?;
                    summary.updated += 1;
                }
                ImportJob::Verify { .. } => {}
            }
        }
        let environment_json = serde_json::to_string(&batch.settings.environment)?;
        tx.execute(
            "UPDATE settings SET global_concurrency=?1,execution_path=?2,run_retention_count=?3,run_retention_age_us=?4,output_limit_bytes=?5,per_run_output_limit_bytes=?6,updated_at_us=?7,environment_json=?8 WHERE singleton=1",
            params![batch.settings.global_concurrency,batch.settings.execution_path,batch.settings.run_retention_count,batch.settings.run_retention_age_us,batch.settings.output_limit_bytes,batch.settings.per_run_output_limit_bytes,batch.now_us,environment_json],
        )?;
        event(&tx, batch.now_us, "import_applied", None, None, "{}")?;
        tx.commit()?;
        Ok(summary)
    }

    /// Sets the enabled flag of the job identified by `reference`, recording
    /// the durable disabled-since transition, and returns the updated record.
    /// An unknown reference returns [`StoreError::NotFound`].
    pub fn set_enabled(
        &self,
        reference: &str,
        enabled: bool,
        now_us: i64,
    ) -> StoreResult<JobRecord> {
        let mut conn = self.conn()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current: Option<(String, bool, i64)> = tx
            .query_row(
                "SELECT id,enabled,current_revision FROM jobs WHERE (id=?1 OR name=?1) AND removed_at_us IS NULL",
                [reference],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let Some((job_id, was_enabled, revision)) = current else {
            return Err(StoreError::NotFound(reference.into()));
        };
        tx.execute(
            "UPDATE jobs SET enabled=?2,updated_at_us=?3 WHERE id=?1",
            params![job_id, enabled, now_us],
        )?;
        if enabled && !was_enabled {
            tx.execute(
                "UPDATE schedule_cursors SET disabled_since_us=COALESCE(disabled_since_us,cursor_us) WHERE job_id=?1 AND revision=?2",
                params![job_id, revision],
            )?;
        } else if !enabled && was_enabled {
            tx.execute(
                "UPDATE schedule_cursors SET disabled_since_us=COALESCE(disabled_since_us,?3) WHERE job_id=?1 AND revision=?2",
                params![job_id, revision, now_us],
            )?;
        }
        tx.commit()?;
        drop(conn);
        self.job(&job_id)
    }

    /// Soft-deletes the job identified by `reference`: it is disabled and
    /// stamped with `removed_at_us`, after which its id and name no longer
    /// resolve via [`Store::job`].
    pub fn remove_job(&self, reference: &str, now_us: i64) -> StoreResult<()> {
        let job = self.job(reference)?;
        let conn = self.conn()?;
        conn.execute(
            "UPDATE jobs SET enabled=0,removed_at_us=?2,updated_at_us=?2 WHERE id=?1",
            params![job.id, now_us],
        )?;
        Ok(())
    }

    /// Queues a manual run for the job identified by `reference`, applying
    /// the job's overlap policy against active same-job work (possibly
    /// skipping the run or superseding its predecessors), and returns the run
    /// record. The run id must be a canonical lowercase UUID.
    pub fn enqueue_manual(
        &self,
        reference: &str,
        run_id: &str,
        now_us: i64,
    ) -> StoreResult<RunRecord> {
        crate::paths::validate_uuid(run_id)?;
        let job = self.job(reference)?;
        let mut conn = self.conn()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let sequence = next_queue_sequence(&tx)?;
        let policy = snapshot_admission_policy(&job.definition_json)?;
        let active_count: i64 = tx.query_row(
            "SELECT count(*) FROM runs WHERE job_id=?1 AND state IN ('queued','starting','running','retry_wait')",
            [&job.id],
            |row| row.get(0),
        )?;
        let quarantined: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM runs WHERE job_id=?1 AND state='running' AND reason='termination_unconfirmed')",
            [&job.id],
            |row| row.get(0),
        )?;
        let mut state = "queued";
        let mut reason: Option<&str> = None;
        let mut replacement_candidate = false;
        if quarantined {
            if policy.overlap == "replace" {
                state = "failed";
                reason = Some("replacement failed: predecessor termination unconfirmed");
            } else {
                state = "skipped_overlap";
                reason = Some("predecessor termination unconfirmed");
            }
        } else if active_count > 0 {
            match policy.overlap.as_str() {
                "skip" => {
                    state = "skipped_overlap";
                    reason = Some("active same-job work exists");
                }
                "allow" if active_count >= policy.per_job_concurrency => {
                    state = "skipped_concurrency";
                    reason = Some("per-job concurrency limit reached");
                }
                "replace" => {
                    supersede_for_replacement(&tx, &job.id, now_us, Some(run_id))?;
                    replacement_candidate = true;
                }
                _ => {}
            }
        }
        let finished_at =
            matches!(state, "skipped_overlap" | "skipped_concurrency" | "failed").then_some(now_us);
        tx.execute(
            "INSERT INTO runs(id,job_id,revision,trigger,nominal_us,requested_at_us,eligible_at_us,queue_sequence,snapshot_json,state,reason,replacement_candidate,finished_at_us) VALUES(?1,?2,?3,'manual',NULL,?4,?4,?5,?6,?7,?8,?9,?10)",
            params![run_id,job.id,job.current_revision,now_us,sequence,job.definition_json,state,reason,replacement_candidate,finished_at],
        )?;
        event(
            &tx,
            now_us,
            "manual_enqueued",
            Some(&job.id),
            Some(run_id),
            "{}",
        )?;
        tx.commit()?;
        drop(conn);
        self.run(run_id)
    }

    /// Materializes `runs` for the job while advancing its schedule cursor,
    /// without reconciliation summaries. See
    /// [`Store::materialize_with_summaries`].
    pub fn materialize(
        &self,
        job_id: &str,
        cursor: CursorUpdate,
        runs: &[NewScheduledRun],
        now_us: i64,
    ) -> StoreResult<MaterializedRun> {
        self.materialize_with_summaries(job_id, cursor, runs, &[], now_us)
    }

    /// Advances the schedule cursor of `job_id` in one immediate transaction
    /// and inserts `runs`, applying each run's snapshot overlap policy and
    /// counting duplicates that already exist. A cursor or revision mismatch
    /// returns [`StoreError::Conflict`]; `summaries` are recorded as events
    /// atomically with the cursor, and `resolve_one_time` disables the job.
    // cursor is consumed by value at call sites in locron-cli; signature is public API
    #[allow(clippy::needless_pass_by_value)]
    pub fn materialize_with_summaries(
        &self,
        job_id: &str,
        cursor: CursorUpdate,
        runs: &[NewScheduledRun],
        summaries: &[ReconciliationSummary],
        now_us: i64,
    ) -> StoreResult<MaterializedRun> {
        let mut conn = self.conn()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = tx.execute("UPDATE schedule_cursors SET cursor_us=?4,one_time_resolved=CASE WHEN ?6 THEN 1 ELSE one_time_resolved END,updated_at_us=?5,disabled_since_us=NULL WHERE job_id=?1 AND revision=?2 AND revision=(SELECT current_revision FROM jobs WHERE id=?1) AND cursor_us=?3", params![job_id,cursor.expected_revision,cursor.expected_cursor_us,cursor.new_cursor_us,now_us,cursor.resolve_one_time])?;
        if changed != 1 {
            return Err(StoreError::Conflict("schedule cursor changed".into()));
        }
        if cursor.resolve_one_time {
            tx.execute(
                "UPDATE jobs SET enabled=0,updated_at_us=?2 WHERE id=?1",
                params![job_id, now_us],
            )?;
            event(&tx, now_us, "one_time_resolved", Some(job_id), None, "{}")?;
        }
        let mut result = MaterializedRun::default();
        for run in runs {
            if run.job_id != job_id || run.revision != cursor.expected_revision {
                return Err(StoreError::Conflict(
                    "scheduled run does not match reconciled job revision".into(),
                ));
            }
            let sequence = next_queue_sequence(&tx)?;
            let policy = snapshot_admission_policy(&run.snapshot_json)?;
            let active_count: i64 = tx.query_row(
                "SELECT count(*) FROM runs WHERE job_id=?1 AND state IN ('queued','starting','running','retry_wait')",
                [&run.job_id],
                |row| row.get(0),
            )?;
            let quarantined: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM runs WHERE job_id=?1 AND state='running' AND reason='termination_unconfirmed')",
                [&run.job_id],
                |row| row.get(0),
            )?;
            let mut state = "queued";
            let mut reason: Option<&str> = None;
            let mut replacement_candidate = false;
            if quarantined {
                if policy.overlap == "replace" {
                    state = "failed";
                    reason = Some("replacement failed: predecessor termination unconfirmed");
                } else {
                    state = "skipped_overlap";
                    reason = Some("predecessor termination unconfirmed");
                }
            } else if run.trigger != "catch_up" && active_count > 0 {
                match policy.overlap.as_str() {
                    "skip" => {
                        state = "skipped_overlap";
                        reason = Some("active same-job work exists");
                    }
                    "allow" if active_count >= policy.per_job_concurrency => {
                        state = "skipped_concurrency";
                        reason = Some("per-job concurrency limit reached");
                    }
                    "replace" => {
                        supersede_for_replacement(&tx, &run.job_id, now_us, Some(&run.id))?;
                        replacement_candidate = true;
                    }
                    _ => {}
                }
            }
            let finished_at = matches!(state, "skipped_overlap" | "skipped_concurrency" | "failed")
                .then_some(now_us);
            let changed = tx.execute("INSERT OR IGNORE INTO runs(id,job_id,revision,trigger,nominal_us,requested_at_us,eligible_at_us,queue_sequence,snapshot_json,state,reason,replacement_candidate,finished_at_us) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)", params![run.id,run.job_id,run.revision,run.trigger,run.nominal_us,run.requested_at_us,run.eligible_at_us,sequence,run.snapshot_json,state,reason,replacement_candidate,finished_at])?;
            if changed == 1 {
                result.inserted += 1
            } else {
                result.duplicates += 1
            }
        }
        for summary in summaries {
            if summary.count == 0 || summary.first_nominal_us > summary.last_nominal_us {
                return Err(StoreError::Conflict(
                    "invalid reconciliation summary range".into(),
                ));
            }
            let details = serde_json::json!({
                "count": summary.count,
                "first_nominal_us": summary.first_nominal_us,
                "last_nominal_us": summary.last_nominal_us,
            });
            event(
                &tx,
                now_us,
                &summary.kind,
                Some(job_id),
                None,
                &serde_json::to_string(&details)?,
            )?;
        }
        tx.commit()?;
        Ok(result)
    }

    /// Returns all events recorded for the job, in insertion order.
    pub fn events_for_job(&self, job_id: &str) -> StoreResult<Vec<EventRecord>> {
        let conn = self.conn()?;
        let mut statement = conn.prepare(
            "SELECT id,occurred_at_us,kind,job_id,run_id,details_json FROM events WHERE job_id=?1 ORDER BY id",
        )?;
        statement
            .query_map([job_id], |row| {
                Ok(EventRecord {
                    id: row.get(0)?,
                    occurred_at_us: row.get(1)?,
                    kind: row.get(2)?,
                    job_id: row.get(3)?,
                    run_id: row.get(4)?,
                    details_json: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    /// Returns all events recorded for the run, in insertion order.
    pub fn events_for_run(&self, run_id: &str) -> StoreResult<Vec<EventRecord>> {
        let conn = self.conn()?;
        let mut statement = conn.prepare(
            "SELECT id,occurred_at_us,kind,job_id,run_id,details_json FROM events WHERE run_id=?1 ORDER BY id",
        )?;
        statement
            .query_map([run_id], |row| {
                Ok(EventRecord {
                    id: row.get(0)?,
                    occurred_at_us: row.get(1)?,
                    kind: row.get(2)?,
                    job_id: row.get(3)?,
                    run_id: row.get(4)?,
                    details_json: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    /// Returns the run with the given id, or [`StoreError::NotFound`].
    pub fn run(&self, id: &str) -> StoreResult<RunRecord> {
        let conn = self.conn()?;
        conn.query_row("SELECT id,job_id,revision,trigger,nominal_us,requested_at_us,eligible_at_us,state,reason,snapshot_json,finished_at_us FROM runs WHERE id=?1", [id], map_run).optional()?.ok_or_else(|| StoreError::NotFound(id.into()))
    }

    /// Returns the resolved executable recorded for the attempt, if any.
    /// Returns [`StoreError::NotFound`] when the attempt does not exist.
    pub fn attempt_resolved_executable(
        &self,
        run_id: &str,
        attempt_number: i64,
    ) -> StoreResult<Option<String>> {
        self.conn()?
            .query_row(
                "SELECT resolved_executable FROM attempts WHERE run_id=?1 AND attempt_number=?2",
                params![run_id, attempt_number],
                |row| row.get(0),
            )
            .optional()?
            .ok_or_else(|| StoreError::NotFound(format!("attempt {run_id}/{attempt_number}")))
    }

    /// Returns all attempts of the run in attempt-number order, each with its
    /// output artifact when one exists. Returns [`StoreError::NotFound`] when
    /// the run does not exist.
    pub fn attempts_for_run(&self, run_id: &str) -> StoreResult<Vec<AttemptRecord>> {
        let conn = self.conn()?;
        let exists = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM runs WHERE id=?1)",
            [run_id],
            |row| row.get::<_, bool>(0),
        )?;
        if !exists {
            return Err(StoreError::NotFound(run_id.into()));
        }
        let mut statement = conn.prepare(
            "SELECT a.run_id,a.attempt_number,a.started_at_us,a.running_at_us,a.finished_at_us,\
                    a.duration_us,a.state,a.result_class,a.exit_code,a.http_status,\
                    a.http_content_type,a.resolved_executable,a.error_message,\
                    o.state,o.retained_payload_bytes,o.physical_bytes,o.discarded_bytes,o.truncated,\
                    o.truncated_at_us,o.finalized_at_us,o.prune_started_at_us,o.pruned_at_us \
             FROM attempts a \
             LEFT JOIN output_artifacts o \
               ON o.run_id=a.run_id AND o.attempt_number=a.attempt_number \
             WHERE a.run_id=?1 \
             ORDER BY a.attempt_number",
        )?;
        statement
            .query_map([run_id], |row| {
                let output_state: Option<String> = row.get(13)?;
                let output = if let Some(state) = output_state {
                    Some(AttemptOutputRecord {
                        state,
                        retained_payload_bytes: row.get(14)?,
                        physical_bytes: row.get(15)?,
                        discarded_bytes: row.get(16)?,
                        truncated: row.get(17)?,
                        truncated_at_us: row.get(18)?,
                        finalized_at_us: row.get(19)?,
                        prune_started_at_us: row.get(20)?,
                        pruned_at_us: row.get(21)?,
                    })
                } else {
                    None
                };
                let error: Option<String> = row.get(12)?;
                Ok(AttemptRecord {
                    run_id: row.get(0)?,
                    attempt_number: row.get(1)?,
                    started_at_us: row.get(2)?,
                    running_at_us: row.get(3)?,
                    finished_at_us: row.get(4)?,
                    duration_us: row.get(5)?,
                    state: row.get(6)?,
                    outcome: row.get(7)?,
                    exit_code: row.get(8)?,
                    http_status: row.get(9)?,
                    http_content_type: row.get(10)?,
                    resolved_executable: row.get(11)?,
                    reason: error.clone(),
                    error,
                    output,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    /// Returns recent runs, newest first, optionally filtered to the job
    /// identified by name or id, up to `limit` (capped at 1000).
    pub fn history(&self, job: Option<&str>, limit: usize) -> StoreResult<Vec<RunRecord>> {
        let job_id = match job {
            Some(value) => Some(self.job(value)?.id),
            None => None,
        };
        let conn = self.conn()?;
        let mut statement = conn.prepare("SELECT id,job_id,revision,trigger,nominal_us,requested_at_us,eligible_at_us,state,reason,snapshot_json,finished_at_us FROM runs WHERE (?1 IS NULL OR job_id=?1) ORDER BY requested_at_us DESC,id DESC LIMIT ?2")?;
        statement
            .query_map(params![job_id, limit.min(1000) as i64], map_run)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    /// Returns the latest run and latest anomalous terminal run for one live
    /// job, both ordered by durable request time and canonical identity. The
    /// focused queries are not subject to the presentation cap of
    /// [`Store::history`].
    pub fn latest_and_anomalous_runs(
        &self,
        job: &str,
    ) -> StoreResult<(Option<RunRecord>, Option<RunRecord>)> {
        let job_id = self.job(job)?.id;
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        let latest = tx
            .query_row(
                "SELECT id,job_id,revision,trigger,nominal_us,requested_at_us,eligible_at_us,state,reason,snapshot_json,finished_at_us FROM runs WHERE job_id=?1 ORDER BY requested_at_us DESC,id DESC LIMIT 1",
                [&job_id],
                map_run,
            )
            .optional()?;
        let anomaly = tx
            .query_row(
                "SELECT id,job_id,revision,trigger,nominal_us,requested_at_us,eligible_at_us,state,reason,snapshot_json,finished_at_us FROM runs WHERE job_id=?1 AND state IN ('failed','timed_out','cancelled','skipped_overlap','skipped_concurrency','interrupted_unknown') ORDER BY requested_at_us DESC,id DESC LIMIT 1",
                [&job_id],
                map_run,
            )
            .optional()?;
        tx.commit()?;
        Ok((latest, anomaly))
    }

    /// Cancels the run without acknowledging an unconfirmed termination. See
    /// [`Store::cancel_with_acknowledgement`].
    pub fn cancel(&self, id: &str, now_us: i64) -> StoreResult<CancelOutcome> {
        self.cancel_with_acknowledgement(id, now_us, false)
    }

    /// Cancels the run in one immediate transaction: queued and retry-wait
    /// runs become `cancelled` immediately, starting and running runs get a
    /// durable cancellation request for the runner, and terminal runs return
    /// [`StoreError::Conflict`]. With `acknowledge_unconfirmed`, a
    /// termination-unconfirmed quarantine is released and recorded as
    /// `interrupted_unknown`; runs not in that quarantine return
    /// [`StoreError::Conflict`].
    pub fn cancel_with_acknowledgement(
        &self,
        id: &str,
        now_us: i64,
        acknowledge_unconfirmed: bool,
    ) -> StoreResult<CancelOutcome> {
        let mut conn = self.conn()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current: Option<(String, String, Option<i64>, Option<String>)> = tx
            .query_row(
                "SELECT state,job_id,cancellation_requested_at_us,reason FROM runs WHERE id=?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;
        let Some((state, job_id, cancellation_requested_at_us, reason)) = current else {
            return Err(StoreError::NotFound(id.into()));
        };
        let quarantined =
            state == "running" && reason.as_deref() == Some("termination_unconfirmed");
        if acknowledge_unconfirmed {
            if !quarantined {
                return Err(StoreError::Conflict(format!(
                    "run {id} is not an active termination-unconfirmed quarantine"
                )));
            }
            let changed = tx.execute(
                "UPDATE runs SET state='interrupted_unknown',reason='termination unconfirmed; risk acknowledged by operator',finished_at_us=?2,replacement_candidate=0 WHERE id=?1 AND state='running' AND reason='termination_unconfirmed'",
                params![id, now_us],
            )?;
            if changed != 1 {
                return Err(StoreError::Conflict(format!(
                    "run {id} quarantine changed before acknowledgement"
                )));
            }
            tx.execute("DELETE FROM retry_intents WHERE run_id=?1", [id])?;
            event(
                &tx,
                now_us,
                "termination_unconfirmed_acknowledged",
                Some(&job_id),
                Some(id),
                r#"{"source":"user","risk":"process_liveness_unconfirmed"}"#,
            )?;
            tx.commit()?;
            return Ok(CancelOutcome::AcknowledgedUnconfirmed);
        }
        if quarantined {
            return Err(StoreError::Conflict(format!(
                "run {id} termination is unconfirmed; repeat cancel with --acknowledge-unconfirmed to accept the risk and release the quarantine"
            )));
        }
        let outcome = match state.as_str() {
            "queued" | "retry_wait" => {
                tx.execute(
                    "UPDATE runs SET state='cancelled',reason='cancelled by user before execution',finished_at_us=?2,cancellation_requested_at_us=?2,cancellation_reason='user',replacement_candidate=0 WHERE id=?1",
                    params![id, now_us],
                )?;
                tx.execute("DELETE FROM retry_intents WHERE run_id=?1", [id])?;
                event(
                    &tx,
                    now_us,
                    "run_cancelled",
                    Some(&job_id),
                    Some(id),
                    r#"{"source":"user","before_execution":true}"#,
                )?;
                CancelOutcome::CancelledBeforeExecution
            }
            "starting" | "running" => {
                if cancellation_requested_at_us.is_none() {
                    tx.execute(
                        "UPDATE runs SET cancellation_requested_at_us=?2,cancellation_reason='user' WHERE id=?1",
                        params![id, now_us],
                    )?;
                    event(
                        &tx,
                        now_us,
                        "cancellation_requested",
                        Some(&job_id),
                        Some(id),
                        r#"{"source":"user"}"#,
                    )?;
                }
                CancelOutcome::CancellationRequested
            }
            terminal => {
                return Err(StoreError::Conflict(format!(
                    "run {id} is already terminal ({terminal})"
                )));
            }
        };
        tx.commit()?;
        Ok(outcome)
    }

    /// Returns whether a durable cancellation request has been recorded for
    /// the run.
    pub fn cancellation_requested(&self, id: &str) -> StoreResult<bool> {
        let requested: Option<Option<i64>> = self
            .conn()?
            .query_row(
                "SELECT cancellation_requested_at_us FROM runs WHERE id=?1",
                [id],
                |row| row.get(0),
            )
            .optional()?;
        Ok(requested.flatten().is_some())
    }

    /// Begins a new scheduler lifetime in one immediate transaction:
    /// attempts still starting or running under a previous lifetime are
    /// terminalized as `interrupted_unknown`, their retry intents are
    /// cleared, stale lifetime rows are closed, and this lifetime is
    /// inserted. Returns the number of attempts recovered.
    pub fn begin_lifetime(
        &self,
        id: &str,
        now_us: i64,
        binary_version: &str,
    ) -> StoreResult<usize> {
        let mut conn = self.conn()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "DELETE FROM retry_intents WHERE run_id IN (SELECT run_id FROM attempts WHERE state IN ('starting','running'))",
            [],
        )?;
        let stale = tx.execute("UPDATE attempts SET state='interrupted_unknown',finished_at_us=?1,error_message='scheduler lifetime ended without a durable result' WHERE state IN ('starting','running')", [now_us])?;
        tx.execute("UPDATE runs SET state='interrupted_unknown',finished_at_us=?1,reason='scheduler lifetime ended without a durable result' WHERE state IN ('starting','running') AND (reason IS NULL OR reason<>'termination_unconfirmed')", [now_us])?;
        tx.execute("UPDATE scheduler_lifetimes SET ended_at_us=?1,exit_class='stale' WHERE ended_at_us IS NULL", [now_us])?;
        tx.execute("INSERT INTO scheduler_lifetimes(id,pid,binary_version,started_at_us,heartbeat_at_us) VALUES(?1,?2,?3,?4,?4)", params![id,std::process::id(),binary_version,now_us])?;
        tx.commit()?;
        Ok(stale)
    }

    /// Records a clean end for the given scheduler lifetime, stamping
    /// `ended_at_us` and `exit_class='clean'`. A lifetime that is not open is
    /// left untouched.
    pub fn end_lifetime(&self, id: &str, now_us: i64) -> StoreResult<()> {
        self.conn()?.execute("UPDATE scheduler_lifetimes SET ended_at_us=?2,heartbeat_at_us=?2,exit_class='clean' WHERE id=?1 AND ended_at_us IS NULL", params![id,now_us])?;
        Ok(())
    }

    /// Admits up to `hard_guard_available` queued and retry-wait runs that
    /// are eligible at `now_us`, in one immediate transaction, respecting the
    /// configured global concurrency and each job's per-job overlap policy
    /// with round-robin job fairness, and returns the admitted attempts.
    /// Returns an empty `Admission` when nothing is eligible or capacity is
    /// exhausted. The run rows advance to `starting` and pending output
    /// artifacts are created for the admitted attempts.
    pub fn admit(
        &self,
        lifetime_id: &str,
        now_us: i64,
        hard_guard_available: usize,
    ) -> StoreResult<Admission> {
        if hard_guard_available == 0 {
            return Ok(Admission::default());
        }
        let mut conn = self.conn()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let configured_limit: i64 = tx.query_row(
            "SELECT global_concurrency FROM settings WHERE singleton=1",
            [],
            |row| row.get(0),
        )?;
        if !(1..=64).contains(&configured_limit) {
            return Err(StoreError::Conflict(
                "global_concurrency must be from 1 through 64".into(),
            ));
        }
        let active_attempts: i64 = tx.query_row(
            "SELECT count(*) FROM attempts WHERE state IN ('starting','running')",
            [],
            |row| row.get(0),
        )?;
        let durable_available = configured_limit.saturating_sub(active_attempts).max(0);
        let capacity = hard_guard_available.min(usize::try_from(durable_available).unwrap_or(0));
        if capacity == 0 {
            return Ok(Admission::default());
        }
        let mut statement = tx.prepare("SELECT r.id,r.job_id,r.snapshot_json,COALESCE((SELECT MAX(attempt_number) FROM attempts a WHERE a.run_id=r.id),0)+1,r.trigger,r.nominal_us FROM runs r WHERE r.state IN ('queued','retry_wait') AND r.eligible_at_us<=?1 AND r.cancellation_requested_at_us IS NULL AND NOT EXISTS(SELECT 1 FROM runs quarantine WHERE quarantine.job_id=r.job_id AND quarantine.state='running' AND quarantine.reason='termination_unconfirmed') AND (r.replacement_candidate=0 OR NOT EXISTS(SELECT 1 FROM runs prior WHERE prior.job_id=r.job_id AND prior.id<>r.id AND prior.state IN ('starting','running','retry_wait'))) AND (r.trigger<>'catch_up' OR NOT EXISTS(SELECT 1 FROM runs earlier WHERE earlier.job_id=r.job_id AND earlier.queue_sequence<r.queue_sequence AND earlier.state IN ('queued','starting','running','retry_wait'))) ORDER BY r.eligible_at_us,r.queue_sequence LIMIT ?2")?;
        let scan_limit = capacity.saturating_mul(64).max(capacity);
        let rows = statement
            .query_map(params![now_us, scan_limit as i64], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<i64>>(5)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        let last_job: Option<String> = tx.query_row(
            "SELECT last_admitted_job_id FROM admission_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )?;
        let mut grouped: BTreeMap<String, VecDeque<AdmissionRow>> = BTreeMap::new();
        for row in rows {
            grouped.entry(row.1.clone()).or_default().push_back(row);
        }
        let mut jobs = grouped.keys().cloned().collect::<Vec<_>>();
        if let Some(last) = last_job {
            let split = jobs.partition_point(|job| job <= &last);
            jobs.rotate_left(split);
        }
        let mut active_by_job = BTreeMap::<String, i64>::new();
        {
            let mut active = tx.prepare(
                "SELECT job_id,count(*) FROM runs WHERE state IN ('starting','running') GROUP BY job_id",
            )?;
            for row in active.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })? {
                let (job_id, count) = row?;
                active_by_job.insert(job_id, count);
            }
        }
        let mut selected_by_job = BTreeMap::<String, i64>::new();
        let mut selected = Vec::new();
        while selected.len() < capacity {
            let mut progressed = false;
            for job in &jobs {
                if let Some(row) = grouped.get_mut(job).and_then(VecDeque::pop_front) {
                    progressed = true;
                    let policy = snapshot_admission_policy(&row.2)?;
                    let limit = if policy.overlap == "allow" {
                        policy.per_job_concurrency.max(1)
                    } else {
                        1
                    };
                    let occupied = active_by_job
                        .get(job)
                        .copied()
                        .unwrap_or(0)
                        .saturating_add(selected_by_job.get(job).copied().unwrap_or(0));
                    if occupied < limit {
                        selected.push(row);
                        *selected_by_job.entry(job.clone()).or_default() += 1;
                        if selected.len() == capacity {
                            break;
                        }
                    }
                }
            }
            if !progressed {
                break;
            }
        }
        let mut attempts = Vec::new();
        for (run_id, job_id, snapshot_json, number, trigger, nominal_us) in selected {
            tx.execute(
                "UPDATE runs SET state='starting' WHERE id=?1 AND state IN ('queued','retry_wait')",
                [&run_id],
            )?;
            tx.execute("INSERT INTO attempts(run_id,attempt_number,lifetime_id,state,started_at_us,running_at_us) VALUES(?1,?2,?3,'starting',?4,NULL)", params![run_id,number,lifetime_id,now_us])?;
            let relative = format!("{run_id}/{number}.partial");
            tx.execute("INSERT INTO output_artifacts(run_id,attempt_number,relative_path,state) VALUES(?1,?2,?3,'pending')", params![run_id,number,relative])?;
            tx.execute(
                "UPDATE admission_state SET last_admitted_job_id=?1 WHERE singleton=1",
                [&job_id],
            )?;
            attempts.push(AdmitAttempt {
                run_id,
                job_id,
                attempt_number: number,
                trigger,
                nominal_us,
                snapshot_json,
            });
        }
        tx.commit()?;
        Ok(Admission { attempts })
    }

    /// Earliest `eligible_at_us` across runs pending admission (queued or
    /// retry-wait), or `None` when no run is pending. The daemon uses this to
    /// sleep until the earliest calculated schedule/retry deadline instead of
    /// the next safety reconciliation boundary.
    pub fn earliest_pending_eligible_at_us(&self) -> StoreResult<Option<i64>> {
        let conn = self.conn()?;
        conn.query_row(
            "SELECT MIN(eligible_at_us) FROM runs WHERE state IN ('queued','retry_wait')",
            [],
            |row| row.get(0),
        )
        .map_err(Into::into)
    }

    /// Advances an admitted attempt from `starting` to `running` after a
    /// successful spawn, unless the run was cancelled or superseded before
    /// spawn, in which case the attempt is durably terminal and
    /// [`StartDecision::CancelledBeforeSpawn`] is returned. Re-marking an
    /// already-running attempt returns [`StartDecision::Ready`] without
    /// changes; a run that is no longer at the pre-spawn boundary returns
    /// [`StoreError::Conflict`].
    pub fn mark_attempt_running(
        &self,
        run_id: &str,
        attempt_number: i64,
        now_us: i64,
    ) -> StoreResult<StartDecision> {
        self.mark_attempt_running_inner(run_id, attempt_number, now_us, None)
    }

    fn mark_attempt_running_inner(
        &self,
        run_id: &str,
        attempt_number: i64,
        now_us: i64,
        resolved_executable: Option<&Path>,
    ) -> StoreResult<StartDecision> {
        let mut conn = self.conn()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current: Option<(String, Option<String>, String)> = tx
            .query_row(
                "SELECT state,cancellation_reason,job_id FROM runs WHERE id=?1",
                [run_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let Some((state, cancellation_reason, job_id)) = current else {
            return Err(StoreError::NotFound(run_id.into()));
        };
        if !matches!(state.as_str(), "starting" | "running") {
            return Err(StoreError::Conflict(
                "admitted run is no longer at the pre-spawn boundary".into(),
            ));
        }
        if let Some(source) = cancellation_reason {
            let reason = if source == "replacement" {
                "replacement requested before spawn"
            } else {
                "cancelled by user before spawn"
            };
            let attempt = tx.execute(
                "UPDATE attempts SET state='cancelled',finished_at_us=?3,duration_us=0,result_class='cancelled',error_message=?4 WHERE run_id=?1 AND attempt_number=?2 AND state IN ('starting','running')",
                params![run_id, attempt_number, now_us, reason],
            )?;
            if attempt != 1 {
                return Err(StoreError::Conflict(
                    "admitted attempt is no longer starting".into(),
                ));
            }
            tx.execute(
                "UPDATE output_artifacts SET state='missing',finalized_at_us=?3 WHERE run_id=?1 AND attempt_number=?2 AND state='pending'",
                params![run_id, attempt_number, now_us],
            )?;
            tx.execute(
                "UPDATE runs SET state='cancelled',reason=?2,finished_at_us=?3,replacement_candidate=0 WHERE id=?1 AND state IN ('starting','running')",
                params![run_id, reason, now_us],
            )?;
            tx.execute("DELETE FROM retry_intents WHERE run_id=?1", [run_id])?;
            event(
                &tx,
                now_us,
                "cancelled_before_spawn",
                Some(&job_id),
                Some(run_id),
                &serde_json::to_string(&serde_json::json!({"source": source}))?,
            )?;
            tx.commit()?;
            return Ok(StartDecision::CancelledBeforeSpawn);
        }
        if state == "running" {
            let attempt_state: Option<String> = tx
                .query_row(
                    "SELECT state FROM attempts WHERE run_id=?1 AND attempt_number=?2",
                    params![run_id, attempt_number],
                    |row| row.get(0),
                )
                .optional()?;
            if attempt_state.as_deref() != Some("running") {
                return Err(StoreError::Conflict(
                    "running run does not match the admitted attempt".into(),
                ));
            }
            tx.commit()?;
            return Ok(StartDecision::Ready);
        }
        let resolved_executable =
            resolved_executable.map(|path| path.to_string_lossy().into_owned());
        let attempt = tx.execute(
            "UPDATE attempts SET state='running',running_at_us=?3,resolved_executable=?4 WHERE run_id=?1 AND attempt_number=?2 AND state='starting'",
            params![run_id, attempt_number, now_us, resolved_executable],
        )?;
        let run = tx.execute(
            "UPDATE runs SET state='running' WHERE id=?1 AND state='starting'",
            [run_id],
        )?;
        if attempt != 1 || run != 1 {
            return Err(StoreError::Conflict(
                "admitted attempt is no longer starting".into(),
            ));
        }
        tx.commit()?;
        Ok(StartDecision::Ready)
    }

    /// Durably records the outcome of an attempt in one immediate
    /// transaction, moving the run to its terminal state or to `retry_wait`
    /// per the retry plan. A `termination_unconfirmed` state quarantines the
    /// run and fails any replacement candidates. An identical re-completion
    /// is idempotent; a completion that does not match the durable state
    /// returns [`StoreError::Conflict`].
    pub fn complete_attempt(&self, completion: &AttemptCompletion) -> StoreResult<()> {
        let mut conn = self.conn()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if completion.state == "termination_unconfirmed" {
            if completion.retry.is_some() {
                return Err(StoreError::Conflict(
                    "termination-unconfirmed attempts cannot retry".into(),
                ));
            }
            let changed = tx.execute(
                "UPDATE attempts SET state='interrupted_unknown',finished_at_us=?3,duration_us=?4,result_class='termination_unconfirmed',error_message=?5 WHERE run_id=?1 AND attempt_number=?2 AND state IN ('starting','running')",
                params![completion.run_id, completion.attempt_number, completion.now_us, completion.duration_us, completion.reason],
            )?;
            if changed != 1 {
                if termination_completion_committed(&tx, completion)? {
                    return Ok(());
                }
                return Err(StoreError::Conflict(
                    "attempt is not active for quarantine".into(),
                ));
            }
            tx.execute(
                "DELETE FROM retry_intents WHERE run_id=?1",
                [&completion.run_id],
            )?;
            let (job_id, cancellation_reason): (String, Option<String>) = tx.query_row(
                "SELECT job_id,cancellation_reason FROM runs WHERE id=?1 AND state IN ('starting','running')",
                [&completion.run_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            tx.execute(
                "UPDATE runs SET state='running',reason='termination_unconfirmed',finished_at_us=NULL WHERE id=?1",
                [&completion.run_id],
            )?;
            if cancellation_reason.as_deref() == Some("replacement") {
                tx.execute(
                    "DELETE FROM retry_intents WHERE run_id IN (SELECT id FROM runs WHERE job_id=?1 AND replacement_candidate=1)",
                    [&job_id],
                )?;
                tx.execute(
                    "UPDATE runs SET state='failed',reason='replacement failed: predecessor termination unconfirmed',finished_at_us=?2,replacement_candidate=0 WHERE job_id=?1 AND replacement_candidate=1 AND state IN ('queued','retry_wait')",
                    params![job_id, completion.now_us],
                )?;
            }
            event(
                &tx,
                completion.now_us,
                "termination_unconfirmed",
                Some(&job_id),
                Some(&completion.run_id),
                &serde_json::to_string(&serde_json::json!({"detail": completion.reason}))?,
            )?;
            tx.commit()?;
            return Ok(());
        }
        if completion.retry.is_some()
            && !matches!(completion.state.as_str(), "failed" | "timed_out")
        {
            return Err(StoreError::Conflict(
                "retry intent requires a known failed or timed-out attempt".into(),
            ));
        }
        let changed = tx.execute("UPDATE attempts SET state=?3,finished_at_us=?4,duration_us=?5,exit_code=?6,http_status=?7,http_content_type=?8,result_class=?3,error_message=?9 WHERE run_id=?1 AND attempt_number=?2 AND state IN ('starting','running')", params![completion.run_id,completion.attempt_number,completion.state,completion.now_us,completion.duration_us,completion.exit_code,completion.http_status,completion.http_content_type,completion.reason])?;
        if changed != 1 {
            if completion_already_committed(&tx, completion)? {
                return Ok(());
            }
            return Err(StoreError::Conflict(
                "attempt is not active for completion".into(),
            ));
        }
        if let Some(retry) = &completion.retry {
            tx.execute(
                "UPDATE runs SET state='retry_wait',eligible_at_us=?2,reason=?3 WHERE id=?1",
                params![completion.run_id, retry.not_before_us, completion.reason],
            )?;
            tx.execute("INSERT OR REPLACE INTO retry_intents(run_id,prior_attempt_number,not_before_us,classification,created_at_us) VALUES(?1,?2,?3,?4,?5)",params![completion.run_id,completion.attempt_number,retry.not_before_us,retry.classification,completion.now_us])?;
        } else {
            tx.execute(
                "DELETE FROM retry_intents WHERE run_id=?1",
                [&completion.run_id],
            )?;
            tx.execute(
                "UPDATE runs SET state=?2,reason=?3,finished_at_us=?4 WHERE id=?1",
                params![
                    completion.run_id,
                    completion.state,
                    completion.reason,
                    completion.now_us
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Records a non-retryable failure that happened before execution, in one
    /// immediate transaction: the output artifact is finalized (when `output`
    /// is supplied and its identity matches the attempt) or marked missing,
    /// and the attempt and run are terminalized as `failed`. A state that is
    /// no longer applicable returns [`StoreError::Conflict`].
    pub fn complete_pre_execution_failure(
        &self,
        run_id: &str,
        attempt_number: i64,
        output: Option<&OutputRecord>,
        now_us: i64,
        reason: &str,
    ) -> StoreResult<()> {
        let mut conn = self.conn()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let artifact_changed = if let Some(output) = output {
            if output.run_id != run_id || output.attempt_number != attempt_number {
                return Err(StoreError::Conflict(
                    "output identity does not match admitted attempt".into(),
                ));
            }
            let relative_path = format!("{run_id}/{attempt_number}.log");
            tx.execute(
                "UPDATE output_artifacts SET relative_path=?3,state='finalized',retained_payload_bytes=?4,physical_bytes=?5,discarded_bytes=?6,truncated=?7,truncated_at_us=CASE WHEN ?7 THEN ?8 ELSE NULL END,finalized_at_us=?8 WHERE run_id=?1 AND attempt_number=?2 AND state='pending'",
                params![run_id,attempt_number,relative_path,output.retained_payload_bytes,output.physical_bytes,output.discarded_bytes,output.truncated,now_us],
            )?
        } else {
            tx.execute(
                "UPDATE output_artifacts SET state='missing',finalized_at_us=?3 WHERE run_id=?1 AND attempt_number=?2 AND state='pending'",
                params![run_id, attempt_number, now_us],
            )?
        };
        if artifact_changed != 1 {
            return Err(StoreError::Conflict(
                "admitted output artifact is not pending".into(),
            ));
        }
        let attempt_changed = tx.execute(
            "UPDATE attempts SET state='failed',finished_at_us=?3,duration_us=0,result_class='failed',error_message=?4 WHERE run_id=?1 AND attempt_number=?2 AND state IN ('starting','running')",
            params![run_id, attempt_number, now_us, reason],
        )?;
        if attempt_changed != 1 {
            return Err(StoreError::Conflict(
                "admitted attempt is not active".into(),
            ));
        }
        tx.execute("DELETE FROM retry_intents WHERE run_id=?1", [run_id])?;
        let job_id: String =
            tx.query_row("SELECT job_id FROM runs WHERE id=?1", [run_id], |row| {
                row.get(0)
            })?;
        tx.execute(
            "UPDATE runs SET state='failed',reason=?2,finished_at_us=?3 WHERE id=?1 AND state IN ('starting','running')",
            params![run_id, reason, now_us],
        )?;
        event(
            &tx,
            now_us,
            "attempt_configuration_failed",
            Some(&job_id),
            Some(run_id),
            r#"{"retryable":false}"#,
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Records a runner-side infrastructure failure in one immediate
    /// transaction: the output artifact is marked missing and the attempt and
    /// run become `interrupted_unknown` (with a duration computed from the
    /// start) when `execution_may_have_started`, or `failed` with result
    /// class `output_preparation_failed` otherwise. Identical durable facts
    /// are idempotent; other states return [`StoreError::Conflict`].
    pub fn complete_runner_failure(
        &self,
        run_id: &str,
        attempt_number: i64,
        now_us: i64,
        reason: &str,
        execution_may_have_started: bool,
    ) -> StoreResult<()> {
        let mut conn = self.conn()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let state = if execution_may_have_started {
            "interrupted_unknown"
        } else {
            "failed"
        };
        let result_class = if execution_may_have_started {
            "interrupted_unknown"
        } else {
            "output_preparation_failed"
        };
        let duration_us = if execution_may_have_started {
            tx.query_row(
                "SELECT max(0,?3-COALESCE(running_at_us,started_at_us)) FROM attempts WHERE run_id=?1 AND attempt_number=?2",
                params![run_id, attempt_number, now_us],
                |row| row.get::<_, i64>(0),
            )?
        } else {
            0
        };
        let output_changed = tx.execute(
            "UPDATE output_artifacts SET state='missing',retained_payload_bytes=0,physical_bytes=0,discarded_bytes=0,truncated=0,truncated_at_us=NULL,finalized_at_us=?3 WHERE run_id=?1 AND attempt_number=?2 AND state IN ('pending','active')",
            params![run_id, attempt_number, now_us],
        )?;
        let attempt_changed = tx.execute(
            "UPDATE attempts SET state=?3,finished_at_us=?4,duration_us=?5,result_class=?6,error_message=?7 WHERE run_id=?1 AND attempt_number=?2 AND state IN ('starting','running')",
            params![run_id, attempt_number, state, now_us, duration_us, result_class, reason],
        )?;
        let run_changed = tx.execute(
            "UPDATE runs SET state=?2,reason=?3,finished_at_us=?4,replacement_candidate=0 WHERE id=?1 AND state IN ('starting','running')",
            params![run_id, state, reason, now_us],
        )?;
        tx.execute("DELETE FROM retry_intents WHERE run_id=?1", [run_id])?;

        if output_changed == 1 && attempt_changed == 1 && run_changed == 1 {
            let job_id: String =
                tx.query_row("SELECT job_id FROM runs WHERE id=?1", [run_id], |row| {
                    row.get(0)
                })?;
            event(
                &tx,
                now_us,
                if execution_may_have_started {
                    "attempt_infrastructure_interrupted"
                } else {
                    "attempt_output_preparation_failed"
                },
                Some(&job_id),
                Some(run_id),
                &serde_json::to_string(&serde_json::json!({
                    "retryable": false,
                    "execution_may_have_started": execution_may_have_started,
                }))?,
            )?;
            tx.commit()?;
            return Ok(());
        }

        // Idempotency compares durable terminal identity (attempt/run state,
        // result class, reason, missing output) only, never timestamps or the
        // elapsed duration derived from them, so an identical runner failure
        // recompleted at a different instant is still recognized as committed.
        let already_committed: bool = tx.query_row(
            "SELECT EXISTS(
                SELECT 1
                FROM attempts a
                JOIN runs r ON r.id=a.run_id
                JOIN output_artifacts o
                  ON o.run_id=a.run_id AND o.attempt_number=a.attempt_number
                WHERE a.run_id=?1 AND a.attempt_number=?2
                  AND a.state=?3 AND a.result_class=?4 AND a.error_message=?5
                  AND r.state=?3 AND r.reason=?5
                  AND o.state='missing'
                  AND NOT EXISTS (SELECT 1 FROM retry_intents WHERE run_id=?1)
            )",
            params![run_id, attempt_number, state, result_class, reason],
            |row| row.get(0),
        )?;
        if already_committed {
            tx.commit()?;
            Ok(())
        } else {
            Err(StoreError::Conflict(
                "runner failure is not applicable to this attempt".into(),
            ))
        }
    }

    /// Finalizes the output artifact of an attempt from a completed run. See
    /// [`Store::reconcile_output_finalized`].
    pub fn finalize_output(&self, output: &OutputRecord, now_us: i64) -> StoreResult<()> {
        self.reconcile_output_finalized(output, now_us)
    }

    /// Lists pending/active output artifacts whose attempts are recovery
    /// candidates: terminal attempts, or attempts owned by a scheduler
    /// lifetime other than the live daemon's. An attempt that is `starting` or
    /// `running` under `lifetime_id` is deliberately excluded because its
    /// partial file legitimately does not exist yet; maintenance must never
    /// reconcile it as missing while the live daemon owns it.
    pub fn referenced_partial_artifacts(
        &self,
        limit: usize,
        lifetime_id: &str,
    ) -> StoreResult<Vec<OutputRecoveryCandidate>> {
        let conn = self.conn()?;
        let mut statement = conn.prepare(
            "SELECT o.run_id,o.attempt_number,o.relative_path,o.state
             FROM output_artifacts o
             JOIN attempts a ON a.run_id=o.run_id AND a.attempt_number=o.attempt_number
             WHERE o.state IN ('pending','active')
               AND (a.state NOT IN ('starting','running') OR a.lifetime_id <> ?2)
             ORDER BY o.run_id,o.attempt_number
             LIMIT ?1",
        )?;
        statement
            .query_map(params![maintenance_limit(limit), lifetime_id], |row| {
                Ok(OutputRecoveryCandidate {
                    run_id: row.get(0)?,
                    attempt_number: row.get(1)?,
                    relative_path: row.get(2)?,
                    state: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    /// Returns whether any output artifact row exists for the given run id
    /// and path relative to the outputs directory.
    pub fn output_artifact_references(
        &self,
        run_id: &str,
        relative_path: &str,
    ) -> StoreResult<bool> {
        self.conn()?
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM output_artifacts
                    WHERE run_id=?1 AND relative_path=?2
                    LIMIT 1
                )",
                params![run_id, relative_path],
                |row| row.get(0),
            )
            .map_err(Into::into)
    }

    /// Lists artifacts awaiting durable prune completion, oldest prune start
    /// first, up to `limit` (capped at the maintenance batch limit).
    pub fn pending_output_prunes(&self, limit: usize) -> StoreResult<Vec<RetentionCandidate>> {
        let conn = self.conn()?;
        let mut statement = conn.prepare(
            "SELECT run_id,attempt_number,relative_path,physical_bytes,COALESCE(finalized_at_us,prune_started_at_us,0) FROM output_artifacts WHERE state='prune_pending' ORDER BY prune_started_at_us,run_id,attempt_number LIMIT ?1",
        )?;
        statement
            .query_map([maintenance_limit(limit)], map_retention_candidate)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    /// Finalizes the output artifact of an attempt, renaming it to the
    /// conventional `{run_id}/{attempt}.log` path and recording byte counts
    /// and truncation. Finalizing an identical durable artifact again is a
    /// no-op; an artifact that cannot be recovered for finalization returns
    /// [`StoreError::Conflict`].
    pub fn reconcile_output_finalized(
        &self,
        output: &OutputRecord,
        now_us: i64,
    ) -> StoreResult<()> {
        let relative_path = format!("{}/{}.log", output.run_id, output.attempt_number);
        let conn = self.conn()?;
        let changed = conn.execute(
            "UPDATE output_artifacts SET relative_path=?3,state='finalized',retained_payload_bytes=?4,physical_bytes=?5,discarded_bytes=?6,truncated=?7,truncated_at_us=CASE WHEN ?7 THEN ?8 ELSE NULL END,finalized_at_us=?8 WHERE run_id=?1 AND attempt_number=?2 AND state IN ('pending','active')",
            params![output.run_id,output.attempt_number,relative_path,output.retained_payload_bytes,output.physical_bytes,output.discarded_bytes,output.truncated,now_us],
        )?;
        if changed == 1 {
            return Ok(());
        }
        // Idempotency compares durable identity fields (path, byte counts,
        // truncation) only, never the finalization timestamp, so an identical
        // completion retried at a different instant is still recognized as
        // already committed.
        let already_reconciled: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM output_artifacts WHERE run_id=?1 AND attempt_number=?2 AND state='finalized' AND relative_path=?3 AND retained_payload_bytes=?4 AND physical_bytes=?5 AND discarded_bytes=?6 AND truncated=?7)",
            params![output.run_id,output.attempt_number,relative_path,output.retained_payload_bytes,output.physical_bytes,output.discarded_bytes,output.truncated],
            |row| row.get(0),
        )?;
        if already_reconciled {
            Ok(())
        } else {
            Err(StoreError::Conflict(
                "output artifact is not recoverable for finalization".into(),
            ))
        }
    }

    /// Marks the output artifact of an attempt missing, zeroing its byte
    /// counts. An artifact already missing is a no-op; an artifact that
    /// cannot be recovered as missing returns [`StoreError::Conflict`].
    pub fn reconcile_output_missing(
        &self,
        run_id: &str,
        attempt_number: i64,
        now_us: i64,
    ) -> StoreResult<()> {
        let conn = self.conn()?;
        let changed = conn.execute(
            "UPDATE output_artifacts SET state='missing',retained_payload_bytes=0,physical_bytes=0,discarded_bytes=0,truncated=0,truncated_at_us=NULL,finalized_at_us=?3 WHERE run_id=?1 AND attempt_number=?2 AND state IN ('pending','active')",
            params![run_id, attempt_number, now_us],
        )?;
        if changed == 1 {
            return Ok(());
        }
        let already_missing: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM output_artifacts WHERE run_id=?1 AND attempt_number=?2 AND state='missing')",
            params![run_id, attempt_number],
            |row| row.get(0),
        )?;
        if already_missing {
            Ok(())
        } else {
            Err(StoreError::Conflict(
                "output artifact is not recoverable as missing".into(),
            ))
        }
    }

    /// Runs `PRAGMA integrity_check` and a foreign-key violation scan,
    /// returning one summary line per check.
    pub fn integrity_check(&self) -> StoreResult<Vec<String>> {
        let conn = self.conn()?;
        let integrity: String = conn.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        let foreign: i64 =
            conn.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })?;
        Ok(vec![
            format!("integrity: {integrity}"),
            format!("foreign_key_violations: {foreign}"),
        ])
    }

    /// Returns the current global settings record, reading columns that
    /// predate schema version 4 with their defaults.
    pub fn settings(&self) -> StoreResult<SettingsRecord> {
        let conn = self.conn()?;
        let version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
        let (
            global_concurrency,
            execution_path,
            run_retention_count,
            run_retention_age_us,
            output_limit_bytes,
            per_run_output_limit_bytes,
            environment_json,
        ): (i64, String, i64, Option<i64>, i64, i64, String) = if version >= 4 {
            conn.query_row(
                "SELECT global_concurrency,execution_path,run_retention_count,run_retention_age_us,output_limit_bytes,per_run_output_limit_bytes,environment_json FROM settings WHERE singleton=1",
                [],
                |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?)),
            )?
        } else {
            let values = conn.query_row(
                "SELECT global_concurrency,execution_path,run_retention_count,run_retention_age_us,output_limit_bytes,per_run_output_limit_bytes FROM settings WHERE singleton=1",
                [],
                |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?)),
            )?;
            (
                values.0,
                values.1,
                values.2,
                values.3,
                values.4,
                values.5,
                "{}".into(),
            )
        };
        let environment = parse_environment_json(&environment_json)?;
        Ok(SettingsRecord {
            global_concurrency,
            execution_path,
            run_retention_count,
            run_retention_age_us,
            output_limit_bytes,
            per_run_output_limit_bytes,
            environment,
        })
    }

    /// Sets (or removes, when `value` is `None`) one global environment
    /// variable in an immediate transaction, storing the map as canonical
    /// JSON, and returns the updated settings. Invalid or reserved names,
    /// NUL bytes in values, and non-canonical maps return
    /// [`StoreError::Conflict`].
    pub fn set_environment(
        &self,
        name: &str,
        value: Option<&str>,
        now_us: i64,
    ) -> StoreResult<SettingsRecord> {
        validate_environment_entry(name, value)?;
        let mut conn = self.conn()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let source: String = tx.query_row(
            "SELECT environment_json FROM settings WHERE singleton=1",
            [],
            |row| row.get(0),
        )?;
        let mut environment = parse_environment_json(&source)?;
        match value {
            Some(value) => {
                environment.insert(name.to_owned(), value.to_owned());
            }
            None => {
                environment.remove(name);
            }
        }
        let canonical = serde_json::to_string(&environment)?;
        tx.execute(
            "UPDATE settings SET environment_json=?1,updated_at_us=?2 WHERE singleton=1",
            params![canonical, now_us],
        )?;
        tx.commit()?;
        drop(conn);
        self.settings()
    }

    /// Like [`Store::mark_attempt_running`], additionally committing the
    /// resolved executable path at the pre-spawn boundary. The path must be
    /// absolute, or [`StoreError::Conflict`] is returned.
    pub fn mark_attempt_running_with_executable(
        &self,
        run_id: &str,
        attempt_number: i64,
        now_us: i64,
        resolved_executable: Option<&Path>,
    ) -> StoreResult<StartDecision> {
        if resolved_executable.is_some_and(|path| !path.is_absolute()) {
            return Err(StoreError::Conflict(
                "resolved executable must be absolute".into(),
            ));
        }
        self.mark_attempt_running_inner(run_id, attempt_number, now_us, resolved_executable)
    }

    /// Sets a single configuration key from its string form, validating the
    /// value (`global_concurrency` from 1 through 64, other integers
    /// non-negative), and returns the updated settings. Unknown keys return
    /// [`StoreError::NotFound`]; invalid values return
    /// [`StoreError::Conflict`].
    pub fn set_setting(&self, key: &str, value: &str, now_us: i64) -> StoreResult<SettingsRecord> {
        let (column, normalized) = match key {
            "global_concurrency" => {
                let parsed: i64 = value.parse().map_err(|_| {
                    StoreError::Conflict("global_concurrency must be an integer".into())
                })?;
                if !(1..=64).contains(&parsed) {
                    return Err(StoreError::Conflict(
                        "global_concurrency must be from 1 through 64".into(),
                    ));
                }
                (
                    "global_concurrency",
                    rusqlite::types::Value::Integer(parsed),
                )
            }
            "execution_path" => (
                "execution_path",
                rusqlite::types::Value::Text(value.to_owned()),
            ),
            "run_retention_count"
            | "run_retention_age_us"
            | "output_limit_bytes"
            | "per_run_output_limit_bytes" => {
                let parsed: i64 = value.parse().map_err(|_| {
                    StoreError::Conflict(format!("{key} must be a non-negative integer"))
                })?;
                if parsed < 0 {
                    return Err(StoreError::Conflict(format!("{key} must be non-negative")));
                }
                (key, rusqlite::types::Value::Integer(parsed))
            }
            _ => return Err(StoreError::NotFound(format!("configuration key {key}"))),
        };
        self.conn()?.execute(
            &format!("UPDATE settings SET {column}=?1,updated_at_us=?2 WHERE singleton=1"),
            params![normalized, now_us],
        )?;
        self.settings()
    }

    /// Lists the finalized output artifacts of terminal runs that are
    /// eligible for pruning, oldest finalization first, up to `limit` (capped
    /// at the maintenance batch limit).
    pub fn output_retention_candidates(
        &self,
        limit: usize,
    ) -> StoreResult<Vec<RetentionCandidate>> {
        let conn = self.conn()?;
        let mut statement = conn.prepare(
            "SELECT o.run_id,o.attempt_number,o.relative_path,o.physical_bytes,o.finalized_at_us FROM output_artifacts o JOIN runs r ON r.id=o.run_id WHERE o.state='finalized' AND r.state IN ('succeeded','failed','timed_out','cancelled','skipped_overlap','skipped_concurrency','interrupted_unknown') ORDER BY o.finalized_at_us,o.run_id,o.attempt_number LIMIT ?1",
        )?;
        statement
            .query_map([maintenance_limit(limit)], map_retention_candidate)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    /// Returns the total physical bytes of all finalized output artifacts.
    pub fn retained_output_bytes(&self) -> StoreResult<i64> {
        self.conn()?.query_row(
            "SELECT COALESCE(sum(physical_bytes),0) FROM output_artifacts WHERE state='finalized'",
            [],
            |row| row.get(0),
        ).map_err(Into::into)
    }

    /// Returns the retained payload bytes of one run's active, finalized,
    /// and prune-pending output artifacts.
    pub fn retained_run_output_bytes(&self, run_id: &str) -> StoreResult<i64> {
        self.conn()?.query_row(
            "SELECT COALESCE(sum(retained_payload_bytes),0) FROM output_artifacts WHERE run_id=?1 AND state IN ('active','finalized','prune_pending')",
            [run_id],
            |row| row.get(0),
        ).map_err(Into::into)
    }

    /// Lists terminal runs eligible for metadata retention deletion: older
    /// than the configured retention age, or beyond the per-job (1000) or
    /// global retention count. Oldest first, up to `limit` (capped at the
    /// maintenance batch limit), excluding runs already reserved for
    /// deletion.
    pub fn run_retention_candidates(
        &self,
        now_us: i64,
        limit: usize,
    ) -> StoreResult<Vec<RunRetentionCandidate>> {
        let conn = self.conn()?;
        let (retention_count, retention_age_us): (i64, Option<i64>) = conn.query_row(
            "SELECT run_retention_count,run_retention_age_us FROM settings WHERE singleton=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let age_cutoff = retention_age_us.map(|age| now_us.saturating_sub(age));
        let mut statement = conn.prepare(
            "WITH terminal AS (
                 SELECT id,job_id,finished_at_us,
                        row_number() OVER (PARTITION BY job_id ORDER BY finished_at_us DESC,id DESC) AS per_job_rank,
                        row_number() OVER (ORDER BY finished_at_us DESC,id DESC) AS global_rank
                 FROM runs
                 WHERE state IN ('succeeded','failed','timed_out','cancelled','skipped_overlap','skipped_concurrency','interrupted_unknown')
                   AND finished_at_us IS NOT NULL
             )
             SELECT id,job_id,finished_at_us
             FROM terminal
             WHERE NOT EXISTS (SELECT 1 FROM run_retention_pending p WHERE p.run_id=terminal.id)
               AND ((?1 IS NOT NULL AND finished_at_us < ?1) OR per_job_rank > 1000 OR global_rank > ?2)
             ORDER BY finished_at_us,id
             LIMIT ?3",
        )?;
        statement
            .query_map(
                params![age_cutoff, retention_count, maintenance_limit(limit)],
                map_run_retention_candidate,
            )?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    /// Reserves a retention candidate for metadata deletion, rechecking its
    /// eligibility (terminal state, age or rank bounds) inside the immediate
    /// transaction. A candidate that is no longer eligible returns
    /// [`StoreError::Conflict`].
    pub fn mark_run_retention_pending(
        &self,
        candidate: &RunRetentionCandidate,
        now_us: i64,
    ) -> StoreResult<()> {
        let mut conn = self.conn()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (retention_count, retention_age_us): (i64, Option<i64>) = tx.query_row(
            "SELECT run_retention_count,run_retention_age_us FROM settings WHERE singleton=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let age_cutoff = retention_age_us.map(|age| now_us.saturating_sub(age));
        let eligible: bool = tx.query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM runs candidate
                 WHERE candidate.id=?1 AND candidate.job_id=?2 AND candidate.finished_at_us=?3
                   AND candidate.state IN ('succeeded','failed','timed_out','cancelled','skipped_overlap','skipped_concurrency','interrupted_unknown')
                   AND (
                       (?4 IS NOT NULL AND candidate.finished_at_us < ?4)
                       OR (SELECT count(*) FROM runs newer
                           WHERE newer.job_id=candidate.job_id
                             AND newer.state IN ('succeeded','failed','timed_out','cancelled','skipped_overlap','skipped_concurrency','interrupted_unknown')
                             AND newer.finished_at_us IS NOT NULL
                             AND (newer.finished_at_us > candidate.finished_at_us OR (newer.finished_at_us=candidate.finished_at_us AND newer.id > candidate.id))) >= 1000
                       OR (SELECT count(*) FROM runs newer
                           WHERE newer.state IN ('succeeded','failed','timed_out','cancelled','skipped_overlap','skipped_concurrency','interrupted_unknown')
                             AND newer.finished_at_us IS NOT NULL
                             AND (newer.finished_at_us > candidate.finished_at_us OR (newer.finished_at_us=candidate.finished_at_us AND newer.id > candidate.id))) >= ?5
                   )
             )",
            params![candidate.run_id,candidate.job_id,candidate.finished_at_us,age_cutoff,retention_count],
            |row| row.get(0),
        )?;
        if !eligible {
            return Err(StoreError::Conflict(
                "run is no longer eligible for metadata retention".into(),
            ));
        }
        tx.execute(
            "INSERT INTO run_retention_pending(run_id,selected_at_us) VALUES(?1,?2) ON CONFLICT(run_id) DO NOTHING",
            params![candidate.run_id, now_us],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Lists runs reserved for metadata deletion, oldest selection first, up
    /// to `limit` (capped at the maintenance batch limit).
    pub fn pending_run_retention(&self, limit: usize) -> StoreResult<Vec<RunRetentionCandidate>> {
        let conn = self.conn()?;
        let mut statement = conn.prepare(
            "SELECT r.id,r.job_id,r.finished_at_us FROM run_retention_pending p JOIN runs r ON r.id=p.run_id ORDER BY p.selected_at_us,r.finished_at_us,r.id LIMIT ?1",
        )?;
        statement
            .query_map([maintenance_limit(limit)], map_run_retention_candidate)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    /// Deletes the metadata of a pending retention candidate in one immediate
    /// transaction, after verifying the run is still terminal with matching
    /// identity and that all of its output artifacts are pruned or missing.
    /// Deleting an already-deleted run is a no-op; other inapplicable states
    /// return [`StoreError::Conflict`].
    pub fn finish_run_retention(&self, candidate: &RunRetentionCandidate) -> StoreResult<()> {
        let mut conn = self.conn()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let pending: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM run_retention_pending WHERE run_id=?1)",
            [&candidate.run_id],
            |row| row.get(0),
        )?;
        if !pending {
            let exists: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM runs WHERE id=?1)",
                [&candidate.run_id],
                |row| row.get(0),
            )?;
            return if exists {
                Err(StoreError::Conflict(
                    "run metadata is not pending retention deletion".into(),
                ))
            } else {
                Ok(())
            };
        }
        let safe: bool = tx.query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM runs
                 WHERE id=?1 AND job_id=?2 AND finished_at_us=?3
                   AND state IN ('succeeded','failed','timed_out','cancelled','skipped_overlap','skipped_concurrency','interrupted_unknown')
                   AND NOT EXISTS (
                       SELECT 1 FROM output_artifacts
                       WHERE run_id=?1 AND state NOT IN ('pruned','missing')
                   )
             )",
            params![candidate.run_id, candidate.job_id, candidate.finished_at_us],
            |row| row.get(0),
        )?;
        if !safe {
            return Err(StoreError::Conflict(
                "run output must be pruned or missing before metadata deletion".into(),
            ));
        }
        tx.execute(
            "DELETE FROM retry_intents WHERE run_id=?1",
            [&candidate.run_id],
        )?;
        tx.execute(
            "DELETE FROM output_artifacts WHERE run_id=?1",
            [&candidate.run_id],
        )?;
        tx.execute("DELETE FROM attempts WHERE run_id=?1", [&candidate.run_id])?;
        tx.execute(
            "DELETE FROM run_retention_pending WHERE run_id=?1",
            [&candidate.run_id],
        )?;
        let changed = tx.execute("DELETE FROM runs WHERE id=?1", [&candidate.run_id])?;
        if changed != 1 {
            return Err(StoreError::Conflict(
                "run metadata disappeared during retention deletion".into(),
            ));
        }
        tx.commit()?;
        Ok(())
    }

    /// Marks a finalized output artifact `prune_pending` with the current
    /// time. An artifact that is no longer finalized returns
    /// [`StoreError::Conflict`].
    pub fn mark_output_prune_pending(
        &self,
        candidate: &RetentionCandidate,
        now_us: i64,
    ) -> StoreResult<()> {
        let changed=self.conn()?.execute("UPDATE output_artifacts SET state='prune_pending',prune_started_at_us=?3 WHERE run_id=?1 AND attempt_number=?2 AND state='finalized'",params![candidate.run_id,candidate.attempt_number,now_us])?;
        if changed != 1 {
            return Err(StoreError::Conflict(
                "output is no longer eligible for pruning".into(),
            ));
        }
        Ok(())
    }

    /// Durably records an output artifact as pruned with zero bytes.
    /// Pruning an already-pruned artifact is a no-op; an artifact not pending
    /// prune completion returns [`StoreError::Conflict`].
    pub fn finish_output_prune(
        &self,
        candidate: &RetentionCandidate,
        now_us: i64,
    ) -> StoreResult<()> {
        let conn = self.conn()?;
        let changed=conn.execute("UPDATE output_artifacts SET state='pruned',retained_payload_bytes=0,physical_bytes=0,pruned_at_us=?3 WHERE run_id=?1 AND attempt_number=?2 AND state='prune_pending'",params![candidate.run_id,candidate.attempt_number,now_us])?;
        if changed == 1 {
            return Ok(());
        }
        let already_pruned: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM output_artifacts WHERE run_id=?1 AND attempt_number=?2 AND state='pruned' AND physical_bytes=0)",
            params![candidate.run_id, candidate.attempt_number],
            |row| row.get(0),
        )?;
        if already_pruned {
            Ok(())
        } else {
            Err(StoreError::Conflict(
                "output is not pending durable prune completion".into(),
            ))
        }
    }
}

impl PersistencePort<AddJobCommand> for Store {
    type Error = StorePortError;

    fn apply(&self, command: AddJobCommand) -> Result<AddJobResult, Self::Error> {
        let settings = configuration_from_record(self.settings()?)?;
        command
            .validate(settings.global_concurrency)
            .map_err(CoreError::from)?;
        let record = self.create_job(&CreateJob {
            id: command.id.to_string(),
            name: command.name,
            description: command.description,
            tags_json: serde_json::to_string(&command.tags)?,
            enabled: command.enabled,
            definition_json: serde_json::to_string(&command.definition)?,
            now_us: command.created_at.epoch_micros(),
            cursor_us: command.cursor_at.epoch_micros(),
        })?;
        Ok(AddJobResult {
            job_id: parse_job_id(&record.id)?,
            revision: revision_number(record.current_revision)?,
        })
    }

    fn map_error(error: Self::Error) -> CoreError {
        map_store_port_error(error)
    }
}

impl PersistencePort<UpdateJobCommand> for Store {
    type Error = StorePortError;

    fn apply(&self, command: UpdateJobCommand) -> Result<UpdateJobResult, Self::Error> {
        let settings = configuration_from_record(self.settings()?)?;
        command
            .validate(settings.global_concurrency)
            .map_err(CoreError::from)?;
        let record = self.update_job(&UpdateJob {
            id: command.job_id.to_string(),
            expected_revision: sequence_i64(command.expected_revision.get(), "revision")?,
            name: command.name,
            description: command.description,
            tags_json: serde_json::to_string(&command.tags)?,
            enabled: command.enabled,
            definition_json: serde_json::to_string(&command.definition)?,
            now_us: command.updated_at.epoch_micros(),
            cursor_us: command.cursor_at.epoch_micros(),
        })?;
        Ok(UpdateJobResult {
            job_id: parse_job_id(&record.id)?,
            revision: revision_number(record.current_revision)?,
        })
    }

    fn map_error(error: Self::Error) -> CoreError {
        map_store_port_error(error)
    }
}

impl PersistencePort<SetJobEnabled> for Store {
    type Error = StorePortError;

    fn apply(&self, command: SetJobEnabled) -> Result<SetJobEnabledResult, Self::Error> {
        let record = self.set_enabled(
            &command.job_id.to_string(),
            command.enabled,
            command.changed_at.epoch_micros(),
        )?;
        Ok(SetJobEnabledResult {
            job_id: parse_job_id(&record.id)?,
            revision: revision_number(record.current_revision)?,
            enabled: record.enabled,
        })
    }

    fn map_error(error: Self::Error) -> CoreError {
        map_store_port_error(error)
    }
}

impl PersistencePort<RemoveJob> for Store {
    type Error = StorePortError;

    fn apply(&self, command: RemoveJob) -> Result<RemoveJobResult, Self::Error> {
        self.remove_job(
            &command.job_id.to_string(),
            command.removed_at.epoch_micros(),
        )?;
        Ok(RemoveJobResult {
            job_id: command.job_id,
        })
    }

    fn map_error(error: Self::Error) -> CoreError {
        map_store_port_error(error)
    }
}

impl PersistencePort<ManualRun> for Store {
    type Error = StorePortError;

    fn apply(&self, command: ManualRun) -> Result<ManualRunResult, Self::Error> {
        let record = self.enqueue_manual(
            &command.job_id.to_string(),
            &command.run_id.to_string(),
            command.requested_at.epoch_micros(),
        )?;
        Ok(ManualRunResult {
            run_id: command.run_id,
            state: parse_run_state(&record.state)?,
        })
    }

    fn map_error(error: Self::Error) -> CoreError {
        map_store_port_error(error)
    }
}

impl PersistencePort<CancelRun> for Store {
    type Error = StorePortError;

    fn apply(&self, command: CancelRun) -> Result<CancelRunResult, Self::Error> {
        let decision = match self.cancel_with_acknowledgement(
            &command.run_id.to_string(),
            command.requested_at.epoch_micros(),
            command.acknowledge_unconfirmed,
        )? {
            CancelOutcome::CancelledBeforeExecution => {
                CancellationDecision::CancelledBeforeExecution
            }
            CancelOutcome::CancellationRequested => CancellationDecision::CancellationRequested,
            CancelOutcome::AcknowledgedUnconfirmed => CancellationDecision::AcknowledgedUnconfirmed,
        };
        Ok(CancelRunResult {
            run_id: command.run_id,
            decision,
        })
    }

    fn map_error(error: Self::Error) -> CoreError {
        map_store_port_error(error)
    }
}

impl PersistencePort<UpdateConfiguration> for Store {
    type Error = StorePortError;

    fn apply(&self, command: UpdateConfiguration) -> Result<Configuration, Self::Error> {
        command.change.validate().map_err(CoreError::from)?;
        let now_us = command.changed_at.epoch_micros();
        let record = match command.change {
            ConfigurationChange::GlobalConcurrency(value) => {
                self.set_setting("global_concurrency", &value.to_string(), now_us)?
            }
            ConfigurationChange::ExecutionPath(value) => {
                self.set_setting("execution_path", &value, now_us)?
            }
            ConfigurationChange::RunRetentionCount(value) => self.set_setting(
                "run_retention_count",
                &storage_integer(value, "run_retention_count")?,
                now_us,
            )?,
            ConfigurationChange::RunRetentionAge(value) => self.set_setting(
                "run_retention_age_us",
                &storage_integer(value.get(), "run_retention_age")?,
                now_us,
            )?,
            ConfigurationChange::OutputLimitBytes(value) => self.set_setting(
                "output_limit_bytes",
                &storage_integer(value, "output_limit_bytes")?,
                now_us,
            )?,
            ConfigurationChange::PerRunOutputLimitBytes(value) => self.set_setting(
                "per_run_output_limit_bytes",
                &storage_integer(value, "per_run_output_limit_bytes")?,
                now_us,
            )?,
            ConfigurationChange::Environment { name, value } => {
                self.set_environment(&name, value.as_deref(), now_us)?
            }
        };
        configuration_from_record(record)
    }

    fn map_error(error: Self::Error) -> CoreError {
        map_store_port_error(error)
    }
}

fn map_store_port_error(error: StorePortError) -> CoreError {
    match error {
        StorePortError::Core(error) => error,
        StorePortError::Store(StoreError::NotFound(value)) => CoreError::NotFound(value),
        StorePortError::Store(StoreError::Conflict(value)) => CoreError::Conflict(value),
        StorePortError::Store(StoreError::DaemonAlreadyRunning) => {
            CoreError::Conflict("another locron daemon owns this state directory".into())
        }
        StorePortError::Store(StoreError::Sqlite(error))
            if matches!(
                error.sqlite_error_code(),
                Some(rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked)
            ) =>
        {
            CoreError::Unavailable("SQLite is busy".into())
        }
        StorePortError::Store(error) => CoreError::Persistence(error.to_string()),
        StorePortError::Json(error) => CoreError::Persistence(error.to_string()),
    }
}

fn parse_job_id(value: &str) -> Result<JobId, StorePortError> {
    value
        .parse::<JobId>()
        .map_err(CoreError::from)
        .map_err(Into::into)
}

fn revision_number(value: i64) -> Result<RevisionNumber, StorePortError> {
    let value = u64::try_from(value).map_err(|_| {
        CoreError::Persistence("stored revision is outside the domain range".into())
    })?;
    RevisionNumber::new(value)
        .map_err(CoreError::from)
        .map_err(Into::into)
}

fn sequence_i64(value: u64, field: &'static str) -> Result<i64, StorePortError> {
    i64::try_from(value).map_err(|_| {
        CoreError::Validation(locron_core::ValidationError::new(
            field,
            "out_of_range",
            "sequence exceeds the durable integer range",
        ))
        .into()
    })
}

fn storage_integer(value: u64, field: &'static str) -> Result<String, StorePortError> {
    sequence_i64(value, field).map(|value| value.to_string())
}

fn parse_run_state(value: &str) -> Result<RunState, StorePortError> {
    let state = match value {
        "queued" => RunState::Queued,
        "starting" => RunState::Starting,
        "running" => RunState::Running,
        "retry_wait" => RunState::RetryWait,
        "succeeded" => RunState::Succeeded,
        "failed" => RunState::Failed,
        "timed_out" => RunState::TimedOut,
        "cancelled" => RunState::Cancelled,
        "skipped_overlap" => RunState::SkippedOverlap,
        "skipped_concurrency" => RunState::SkippedConcurrency,
        "interrupted_unknown" => RunState::InterruptedUnknown,
        other => {
            return Err(CoreError::Persistence(format!(
                "stored run state is outside the domain vocabulary: {other}"
            ))
            .into());
        }
    };
    Ok(state)
}

fn configuration_from_record(record: SettingsRecord) -> Result<Configuration, StorePortError> {
    let global_concurrency = u8::try_from(record.global_concurrency).map_err(|_| {
        CoreError::Persistence("stored global concurrency is outside the domain range".into())
    })?;
    if !(1..=64).contains(&global_concurrency) {
        return Err(CoreError::Persistence(
            "stored global concurrency is outside the domain range".into(),
        )
        .into());
    }
    let unsigned = |value: i64, field: &str| {
        u64::try_from(value)
            .map_err(|_| CoreError::Persistence(format!("stored {field} is negative")))
    };
    Ok(Configuration {
        global_concurrency,
        execution_path: record.execution_path,
        run_retention_count: unsigned(record.run_retention_count, "run retention count")?,
        run_retention_age: record
            .run_retention_age_us
            .map(|value| unsigned(value, "run retention age").map(DurationMicros::new))
            .transpose()?,
        output_limit_bytes: unsigned(record.output_limit_bytes, "output limit")?,
        per_run_output_limit_bytes: unsigned(
            record.per_run_output_limit_bytes,
            "per-run output limit",
        )?,
        environment: record.environment,
    })
}

fn maintenance_limit(limit: usize) -> i64 {
    limit.min(MAINTENANCE_BATCH_LIMIT) as i64
}

fn map_retention_candidate(row: &rusqlite::Row<'_>) -> rusqlite::Result<RetentionCandidate> {
    Ok(RetentionCandidate {
        run_id: row.get(0)?,
        attempt_number: row.get(1)?,
        relative_path: row.get(2)?,
        physical_bytes: row.get(3)?,
        finalized_at_us: row.get(4)?,
    })
}

fn map_run_retention_candidate(row: &rusqlite::Row<'_>) -> rusqlite::Result<RunRetentionCandidate> {
    Ok(RunRetentionCandidate {
        run_id: row.get(0)?,
        job_id: row.get(1)?,
        finished_at_us: row.get(2)?,
    })
}

struct SnapshotAdmissionPolicy {
    overlap: String,
    per_job_concurrency: i64,
}

fn termination_completion_committed(
    tx: &Transaction<'_>,
    completion: &AttemptCompletion,
) -> StoreResult<bool> {
    type TerminationCompletionFacts = (
        String,
        Option<i64>,
        Option<i64>,
        Option<String>,
        String,
        Option<String>,
    );
    let current: Option<TerminationCompletionFacts> = tx
        .query_row(
            "SELECT a.state,a.finished_at_us,a.duration_us,a.error_message,r.state,r.reason FROM attempts a JOIN runs r ON r.id=a.run_id WHERE a.run_id=?1 AND a.attempt_number=?2",
            params![completion.run_id, completion.attempt_number],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
        )
        .optional()?;
    Ok(matches!(
        current,
        Some((attempt_state, finished, duration, error, run_state, reason))
            if attempt_state == "interrupted_unknown"
                && finished == Some(completion.now_us)
                && duration == Some(completion.duration_us)
                && error.as_deref() == Some(completion.reason.as_str())
                && run_state == "running"
                && reason.as_deref() == Some("termination_unconfirmed")
    ))
}

fn completion_already_committed(
    tx: &Transaction<'_>,
    completion: &AttemptCompletion,
) -> StoreResult<bool> {
    type CompletionFacts = (
        String,
        Option<i64>,
        Option<i64>,
        Option<i32>,
        Option<i64>,
        Option<String>,
        Option<String>,
        String,
        Option<String>,
        i64,
        Option<i64>,
    );
    let current: Option<CompletionFacts> = tx
        .query_row(
            "SELECT a.state,a.finished_at_us,a.duration_us,a.exit_code,a.http_status,a.http_content_type,a.error_message,r.state,r.reason,r.eligible_at_us,r.finished_at_us FROM attempts a JOIN runs r ON r.id=a.run_id WHERE a.run_id=?1 AND a.attempt_number=?2",
            params![completion.run_id, completion.attempt_number],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?, row.get(7)?, row.get(8)?, row.get(9)?, row.get(10)?)),
        )
        .optional()?;
    let Some((
        attempt_state,
        finished,
        duration,
        exit_code,
        http_status,
        http_content_type,
        error,
        run_state,
        reason,
        eligible_at,
        run_finished,
    )) = current
    else {
        return Ok(false);
    };
    if attempt_state != completion.state
        || finished != Some(completion.now_us)
        || duration != Some(completion.duration_us)
        || exit_code != completion.exit_code
        || http_status != completion.http_status.map(i64::from)
        || http_content_type.as_deref() != completion.http_content_type.as_deref()
        || error.as_deref() != Some(completion.reason.as_str())
        || reason.as_deref() != Some(completion.reason.as_str())
    {
        return Ok(false);
    }
    if let Some(retry) = &completion.retry {
        if run_state != "retry_wait" || eligible_at != retry.not_before_us || run_finished.is_some()
        {
            return Ok(false);
        }
        let retry_matches: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM retry_intents WHERE run_id=?1 AND prior_attempt_number=?2 AND not_before_us=?3 AND classification=?4 AND created_at_us=?5)",
            params![completion.run_id, completion.attempt_number, retry.not_before_us, retry.classification, completion.now_us],
            |row| row.get(0),
        )?;
        Ok(retry_matches)
    } else {
        let retry_exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM retry_intents WHERE run_id=?1)",
            [&completion.run_id],
            |row| row.get(0),
        )?;
        Ok(!retry_exists
            && run_state == completion.state
            && run_finished == Some(completion.now_us))
    }
}

fn snapshot_admission_policy(snapshot: &str) -> StoreResult<SnapshotAdmissionPolicy> {
    let value: serde_json::Value = serde_json::from_str(snapshot)?;
    let policy = value.get("policy");
    Ok(SnapshotAdmissionPolicy {
        overlap: policy
            .and_then(|value| value.get("overlap"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("skip")
            .to_owned(),
        per_job_concurrency: policy
            .and_then(|value| value.get("per_job_concurrency"))
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(1),
    })
}

fn configure(connection: &Connection) -> StoreResult<()> {
    connection.busy_timeout(std::time::Duration::from_secs(5))?;
    connection.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON; PRAGMA locking_mode=NORMAL; PRAGMA trusted_schema=OFF;")?;
    Ok(())
}

fn configure_read_only(connection: &Connection) -> StoreResult<()> {
    connection.busy_timeout(std::time::Duration::from_secs(5))?;
    connection.execute_batch("PRAGMA foreign_keys=ON; PRAGMA trusted_schema=OFF;")?;
    Ok(())
}

fn next_queue_sequence(tx: &Transaction<'_>) -> StoreResult<i64> {
    let value: i64 = tx.query_row("UPDATE admission_state SET next_queue_sequence=next_queue_sequence+1 WHERE singleton=1 RETURNING next_queue_sequence-1", [], |row| row.get(0))?;
    Ok(value)
}

fn supersede_for_replacement(
    tx: &Transaction<'_>,
    job_id: &str,
    now_us: i64,
    successor_id: Option<&str>,
) -> StoreResult<()> {
    let mut statement = tx.prepare(
        "SELECT id FROM runs WHERE job_id=?1 AND (state='retry_wait' OR (state='queued' AND trigger<>'catch_up')) ORDER BY queue_sequence",
    )?;
    let superseded = statement
        .query_map([job_id], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);
    for run_id in superseded {
        tx.execute("DELETE FROM retry_intents WHERE run_id=?1", [&run_id])?;
        tx.execute(
            "UPDATE runs SET state='skipped_overlap',reason='superseded by newer replacement',finished_at_us=?2,replacement_candidate=0 WHERE id=?1 AND state IN ('queued','retry_wait')",
            params![run_id, now_us],
        )?;
        let details = serde_json::json!({
            "reason": "superseded_by_newer_replacement",
            "successor_run_id": successor_id,
        });
        event(
            tx,
            now_us,
            "replacement_superseded",
            Some(job_id),
            Some(&run_id),
            &serde_json::to_string(&details)?,
        )?;
    }
    let active_changed = tx.execute(
        "UPDATE runs SET cancellation_requested_at_us=COALESCE(cancellation_requested_at_us,?2),cancellation_reason=COALESCE(cancellation_reason,'replacement'),replacement_candidate=0 WHERE job_id=?1 AND state IN ('starting','running')",
        params![job_id, now_us],
    )?;
    if active_changed > 0 {
        let details = serde_json::json!({
            "source": "replacement",
            "successor_run_id": successor_id,
        });
        event(
            tx,
            now_us,
            "replacement_requested",
            Some(job_id),
            None,
            &serde_json::to_string(&details)?,
        )?;
    }
    Ok(())
}

fn event(
    tx: &Transaction<'_>,
    at: i64,
    kind: &str,
    job: Option<&str>,
    run: Option<&str>,
    details: &str,
) -> StoreResult<()> {
    tx.execute(
        "INSERT INTO events(occurred_at_us,kind,job_id,run_id,details_json) VALUES(?1,?2,?3,?4,?5)",
        params![at, kind, job, run, details],
    )?;
    Ok(())
}

fn map_job(row: &rusqlite::Row<'_>) -> rusqlite::Result<JobRecord> {
    Ok(JobRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        tags_json: row.get(3)?,
        enabled: row.get(4)?,
        removed_at_us: row.get(5)?,
        current_revision: row.get(6)?,
        definition_json: row.get(7)?,
        cursor_us: row.get(8)?,
        updated_at_us: row.get(9)?,
        cursor_updated_at_us: row.get(10)?,
        disabled_since_us: row.get(11)?,
    })
}

fn validate_import_resolution(
    tx: &Transaction<'_>,
    resolution: &ImportResolution,
) -> StoreResult<()> {
    let by_id = tx
        .query_row(
            "SELECT id FROM jobs WHERE id=?1 AND removed_at_us IS NULL",
            [&resolution.source_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let by_name = tx
        .query_row(
            "SELECT id FROM jobs WHERE name=?1 AND removed_at_us IS NULL",
            [&resolution.source_name],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if by_id != resolution.expected_id_destination
        || by_name != resolution.expected_name_destination
    {
        return Err(StoreError::Conflict(format!(
            "import source mapping changed for {}",
            resolution.source_name
        )));
    }
    Ok(())
}

fn import_destination(tx: &Transaction<'_>, id: &str) -> StoreResult<JobRecord> {
    tx.query_row(
        "SELECT j.id,j.name,j.description,j.tags_json,j.enabled,j.removed_at_us,j.current_revision,r.definition_json,c.cursor_us,j.updated_at_us,c.updated_at_us,c.disabled_since_us FROM jobs j JOIN job_revisions r ON r.job_id=j.id AND r.revision=j.current_revision JOIN schedule_cursors c ON c.job_id=j.id AND c.revision=j.current_revision WHERE j.id=?1 AND j.removed_at_us IS NULL",
        [id],
        map_job,
    )
    .optional()?
    .ok_or_else(|| StoreError::NotFound(id.into()))
}

fn import_job_matches(current: &JobRecord, expected: &UpdateJob) -> bool {
    current.id == expected.id
        && current.current_revision == expected.expected_revision
        && current.name == expected.name
        && current.description == expected.description
        && current.tags_json == expected.tags_json
        && current.enabled == expected.enabled
        && current.definition_json == expected.definition_json
        && current.cursor_us == expected.cursor_us
}

fn validate_import_settings(settings: &SettingsRecord) -> StoreResult<()> {
    if !(1..=64).contains(&settings.global_concurrency) {
        return Err(StoreError::Conflict(
            "import global_concurrency must be from 1 through 64".into(),
        ));
    }
    if settings.run_retention_count < 0
        || settings.output_limit_bytes < 0
        || settings.per_run_output_limit_bytes < 0
        || settings.run_retention_age_us.is_some_and(|value| value < 0)
    {
        return Err(StoreError::Conflict(
            "import retention and output limits must be non-negative".into(),
        ));
    }
    for (name, value) in &settings.environment {
        validate_environment_entry(name, Some(value))?;
    }
    Ok(())
}

fn parse_environment_json(source: &str) -> StoreResult<BTreeMap<String, String>> {
    let environment: BTreeMap<String, String> = serde_json::from_str(source)?;
    for (name, value) in &environment {
        validate_environment_entry(name, Some(value))?;
    }
    if serde_json::to_string(&environment)? != source {
        return Err(StoreError::Conflict(
            "global environment is not canonical JSON".into(),
        ));
    }
    Ok(environment)
}

fn validate_environment_entry(name: &str, value: Option<&str>) -> StoreResult<()> {
    let mut bytes = name.bytes();
    let valid_start = bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_');
    let valid_rest = bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_');
    if !valid_start || !valid_rest || name.starts_with("LOCRON_") {
        return Err(StoreError::Conflict(format!(
            "invalid or reserved environment name {name}"
        )));
    }
    if value.is_some_and(|value| value.contains('\0')) {
        return Err(StoreError::Conflict(format!(
            "environment value for {name} contains NUL"
        )));
    }
    Ok(())
}
fn map_run(row: &rusqlite::Row<'_>) -> rusqlite::Result<RunRecord> {
    Ok(RunRecord {
        id: row.get(0)?,
        job_id: row.get(1)?,
        revision: row.get(2)?,
        trigger: row.get(3)?,
        nominal_us: row.get(4)?,
        requested_at_us: row.get(5)?,
        eligible_at_us: row.get(6)?,
        state: row.get(7)?,
        reason: row.get(8)?,
        snapshot_json: row.get(9)?,
        finished_at_us: row.get(10)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use locron_core::command::JobDefinition;
    use locron_core::policy::ExecutionPolicy;
    use locron_core::schedule::Schedule;
    use locron_core::target::{Environment, Target};
    use locron_core::{RunId, Timestamp};

    fn store() -> (tempfile::TempDir, Store) {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(StatePaths::new(temp.path().into()), "test", 1).unwrap();
        (temp, store)
    }
    fn create(store: &Store, id: &str, name: &str) {
        store
            .create_job(&CreateJob {
                id: id.into(),
                name: name.into(),
                description: None,
                tags_json: "[]".into(),
                enabled: true,
                definition_json: "{}".into(),
                now_us: 1,
                cursor_us: 1,
            })
            .unwrap();
    }

    fn create_with_policy(store: &Store, id: &str, name: &str, overlap: &str, limit: i64) {
        store
            .create_job(&CreateJob {
                id: id.into(),
                name: name.into(),
                description: None,
                tags_json: "[]".into(),
                enabled: true,
                definition_json: format!(
                    "{{\"policy\":{{\"overlap\":\"{overlap}\",\"per_job_concurrency\":{limit}}}}}"
                ),
                now_us: 1,
                cursor_us: 1,
            })
            .unwrap();
    }

    fn insert_terminal_runs(
        store: &Store,
        job_id: &str,
        first_identity: u128,
        first_sequence: i64,
        finished_at_us: &[i64],
    ) -> Vec<String> {
        let mut conn = store.conn().unwrap();
        let tx = conn.transaction().unwrap();
        let mut identities = Vec::with_capacity(finished_at_us.len());
        for (offset, finished_at_us) in finished_at_us.iter().copied().enumerate() {
            let run_id = Uuid::from_u128(first_identity + offset as u128).to_string();
            tx.execute(
                "INSERT INTO runs(id,job_id,revision,trigger,nominal_us,requested_at_us,eligible_at_us,queue_sequence,snapshot_json,state,reason,finished_at_us) VALUES(?1,?2,1,'manual',NULL,?3,?3,?4,'{}','succeeded','test',?3)",
                params![run_id, job_id, finished_at_us, first_sequence + offset as i64],
            )
            .unwrap();
            identities.push(run_id);
        }
        tx.execute(
            "UPDATE admission_state SET next_queue_sequence=(SELECT COALESCE(max(queue_sequence),0)+1 FROM runs) WHERE singleton=1",
            [],
        )
        .unwrap();
        tx.commit().unwrap();
        identities
    }

    fn import_resolution(
        source_id: &str,
        source_name: &str,
        destination: Option<&str>,
    ) -> ImportResolution {
        ImportResolution {
            source_id: source_id.into(),
            source_name: source_name.into(),
            expected_id_destination: destination.map(str::to_owned),
            expected_name_destination: destination.map(str::to_owned),
        }
    }

    fn application_definition(cwd: &Path) -> JobDefinition {
        JobDefinition {
            schedule: Schedule::Every {
                interval: DurationMicros::SECOND,
                anchor: Timestamp::UNIX_EPOCH,
            },
            target: Target::Process {
                executable: "/usr/bin/true".into(),
                args: Vec::new(),
            },
            cwd: cwd.into(),
            environment: Environment::default(),
            policy: ExecutionPolicy::default(),
        }
    }

    #[test]
    fn core_application_ports_drive_real_sqlite_mutations_with_typed_results() {
        let (temp, store) = store();
        let job_id = JobId::new();
        let added = PersistencePort::apply(
            &store,
            AddJobCommand {
                id: job_id,
                name: "ported".into(),
                description: Some("through core".into()),
                tags: vec!["adapter".into()],
                enabled: true,
                definition: application_definition(temp.path()),
                created_at: Timestamp::from_epoch_micros(10),
                cursor_at: Timestamp::from_epoch_micros(10),
            },
        )
        .unwrap();
        assert_eq!(added.job_id, job_id);
        assert_eq!(added.revision.get(), 1);

        let updated = PersistencePort::apply(
            &store,
            UpdateJobCommand {
                job_id,
                expected_revision: added.revision,
                name: "ported-renamed".into(),
                description: None,
                tags: vec!["typed".into()],
                enabled: true,
                definition: application_definition(temp.path()),
                updated_at: Timestamp::from_epoch_micros(11),
                cursor_at: Timestamp::from_epoch_micros(10),
            },
        )
        .unwrap();
        assert_eq!(updated.revision.get(), 2);

        let disabled = PersistencePort::apply(
            &store,
            SetJobEnabled {
                job_id,
                enabled: false,
                changed_at: Timestamp::from_epoch_micros(12),
            },
        )
        .unwrap();
        assert!(!disabled.enabled);
        assert_eq!(disabled.revision, updated.revision);

        let run_id = RunId::new();
        let manual = PersistencePort::apply(
            &store,
            ManualRun {
                run_id,
                job_id,
                requested_at: Timestamp::from_epoch_micros(13),
            },
        )
        .unwrap();
        assert_eq!(manual.run_id, run_id);
        assert_eq!(manual.state, RunState::Queued);

        let cancelled = PersistencePort::apply(
            &store,
            CancelRun {
                run_id,
                requested_at: Timestamp::from_epoch_micros(14),
                acknowledge_unconfirmed: false,
            },
        )
        .unwrap();
        assert_eq!(
            cancelled.decision,
            CancellationDecision::CancelledBeforeExecution
        );

        let configuration = PersistencePort::apply(
            &store,
            UpdateConfiguration {
                change: ConfigurationChange::Environment {
                    name: "PORT_TEST".into(),
                    value: Some("present".into()),
                },
                changed_at: Timestamp::from_epoch_micros(15),
            },
        )
        .unwrap();
        assert_eq!(
            configuration
                .environment
                .get("PORT_TEST")
                .map(String::as_str),
            Some("present")
        );

        let removed = PersistencePort::apply(
            &store,
            RemoveJob {
                job_id,
                removed_at: Timestamp::from_epoch_micros(16),
            },
        )
        .unwrap();
        assert_eq!(removed.job_id, job_id);
        assert!(matches!(
            store.job(&job_id.to_string()),
            Err(StoreError::NotFound(_))
        ));
    }

    #[test]
    fn core_application_port_maps_store_conflicts_without_sqlite_leakage() {
        let (temp, store) = store();
        let command = AddJobCommand {
            id: JobId::new(),
            name: "duplicate".into(),
            description: None,
            tags: Vec::new(),
            enabled: true,
            definition: application_definition(temp.path()),
            created_at: Timestamp::from_epoch_micros(10),
            cursor_at: Timestamp::from_epoch_micros(10),
        };
        PersistencePort::apply(&store, command.clone()).unwrap();
        let duplicate = PersistencePort::apply(&store, command).unwrap_err();
        let mapped = <Store as PersistencePort<AddJobCommand>>::map_error(duplicate);
        assert!(
            matches!(mapped, CoreError::Conflict(message) if message.contains("already exists"))
        );
    }

    #[test]
    fn duplicate_occurrence_is_idempotent() {
        let (_temp, store) = store();
        let job = Uuid::now_v7().to_string();
        create(&store, &job, "x");
        let run = |id: String| NewScheduledRun {
            id,
            job_id: job.clone(),
            revision: 1,
            trigger: "scheduled".into(),
            nominal_us: 10,
            requested_at_us: 10,
            eligible_at_us: 10,
            snapshot_json: "{}".into(),
        };
        let first = store
            .materialize(
                &job,
                CursorUpdate {
                    expected_revision: 1,
                    expected_cursor_us: 1,
                    new_cursor_us: 10,
                    resolve_one_time: false,
                },
                &[run(Uuid::now_v7().to_string())],
                10,
            )
            .unwrap();
        assert_eq!(first.inserted, 1);
        let second = store
            .materialize(
                &job,
                CursorUpdate {
                    expected_revision: 1,
                    expected_cursor_us: 10,
                    new_cursor_us: 20,
                    resolve_one_time: false,
                },
                &[run(Uuid::now_v7().to_string())],
                20,
            )
            .unwrap();
        assert_eq!(second.duplicates, 1);
    }

    #[test]
    fn reconciliation_rejects_same_cursor_on_a_new_revision() {
        let (_temp, store) = store();
        let job = Uuid::now_v7().to_string();
        create(&store, &job, "x");
        store
            .update_job(&UpdateJob {
                id: job.clone(),
                expected_revision: 1,
                name: "x".into(),
                description: None,
                tags_json: "[]".into(),
                enabled: true,
                definition_json: "{\"revision\":2}".into(),
                now_us: 2,
                cursor_us: 1,
            })
            .unwrap();
        let stale = NewScheduledRun {
            id: Uuid::now_v7().to_string(),
            job_id: job.clone(),
            revision: 1,
            trigger: "scheduled".into(),
            nominal_us: 10,
            requested_at_us: 10,
            eligible_at_us: 10,
            snapshot_json: "{}".into(),
        };
        assert!(matches!(
            store.materialize(
                &job,
                CursorUpdate {
                    expected_revision: 1,
                    expected_cursor_us: 1,
                    new_cursor_us: 10,
                    resolve_one_time: false,
                },
                &[stale],
                10,
            ),
            Err(StoreError::Conflict(_))
        ));
        assert_eq!(store.job("x").unwrap().current_revision, 2);
        assert_eq!(store.job("x").unwrap().cursor_us, 1);
        assert!(store.history(Some("x"), 10).unwrap().is_empty());
    }

    #[test]
    fn one_time_resolution_disables_atomically_but_manual_enqueue_does_not() {
        let (_temp, store) = store();
        let job = Uuid::now_v7().to_string();
        create(&store, &job, "once");
        let manual = Uuid::now_v7().to_string();
        store.enqueue_manual("once", &manual, 2).unwrap();
        assert!(store.job("once").unwrap().enabled);
        let scheduled = NewScheduledRun {
            id: Uuid::now_v7().to_string(),
            job_id: job.clone(),
            revision: 1,
            trigger: "catch_up".into(),
            nominal_us: 10,
            requested_at_us: 20,
            eligible_at_us: 20,
            snapshot_json: "{}".into(),
        };
        let result = store
            .materialize(
                &job,
                CursorUpdate {
                    expected_revision: 1,
                    expected_cursor_us: 1,
                    new_cursor_us: 20,
                    resolve_one_time: true,
                },
                std::slice::from_ref(&scheduled),
                20,
            )
            .unwrap();
        assert_eq!(result.inserted, 1);
        assert!(!store.job("once").unwrap().enabled);

        let duplicate = store
            .materialize(
                &job,
                CursorUpdate {
                    expected_revision: 1,
                    expected_cursor_us: 20,
                    new_cursor_us: 30,
                    resolve_one_time: true,
                },
                &[NewScheduledRun {
                    id: Uuid::now_v7().to_string(),
                    ..scheduled
                }],
                30,
            )
            .unwrap();
        assert_eq!(duplicate.duplicates, 1);
        assert_eq!(store.history(Some("once"), 10).unwrap().len(), 2);
    }

    #[test]
    fn manual_enqueue_survives_without_daemon() {
        let (_temp, store) = store();
        let job = Uuid::now_v7().to_string();
        create(&store, &job, "x");
        let run = Uuid::now_v7().to_string();
        assert_eq!(store.enqueue_manual("x", &run, 2).unwrap().state, "queued");
    }

    #[test]
    fn enable_transition_is_a_durable_fact_not_an_updated_at_heuristic() {
        let (_temp, store) = store();
        let job = Uuid::now_v7().to_string();
        create(&store, &job, "x");
        assert_eq!(
            store.set_enabled("x", true, 1).unwrap().disabled_since_us,
            None
        );

        assert_eq!(
            store.set_enabled("x", false, 10).unwrap().disabled_since_us,
            Some(10)
        );
        assert_eq!(
            store.set_enabled("x", false, 11).unwrap().disabled_since_us,
            Some(10)
        );
        assert_eq!(
            store.set_enabled("x", true, 11).unwrap().disabled_since_us,
            Some(10)
        );

        store
            .materialize(
                &job,
                CursorUpdate {
                    expected_revision: 1,
                    expected_cursor_us: 1,
                    new_cursor_us: 20,
                    resolve_one_time: false,
                },
                &[],
                20,
            )
            .unwrap();
        assert_eq!(store.job("x").unwrap().disabled_since_us, None);
        assert!(matches!(
            store.set_enabled("missing", false, 30),
            Err(StoreError::NotFound(_))
        ));
    }

    #[test]
    fn concurrent_enable_disable_transitions_serialize_in_sqlite() {
        let temp = tempfile::tempdir().unwrap();
        let paths = StatePaths::new(temp.path().into());
        let first = Store::open(paths.clone(), "test", 1).unwrap();
        let second = Store::open(paths, "test", 1).unwrap();
        let job = Uuid::now_v7().to_string();
        create(&first, &job, "x");
        let barrier = std::sync::Barrier::new(2);
        std::thread::scope(|scope| {
            scope.spawn(|| {
                barrier.wait();
                first.set_enabled("x", false, 10).unwrap();
            });
            scope.spawn(|| {
                barrier.wait();
                second.set_enabled("x", true, 11).unwrap();
            });
        });
        first.set_enabled("x", true, 12).unwrap();
        assert!(first.job("x").unwrap().disabled_since_us.is_some());
    }

    #[test]
    fn cancelling_queued_run_terminalizes_and_prevents_admission() {
        let (_temp, store) = store();
        let job = Uuid::now_v7().to_string();
        create(&store, &job, "x");
        let run = Uuid::now_v7().to_string();
        store.enqueue_manual("x", &run, 2).unwrap();

        store.cancel(&run, 3).unwrap();

        let cancelled = store.run(&run).unwrap();
        assert_eq!(cancelled.state, "cancelled");
        assert_eq!(cancelled.finished_at_us, Some(3));
        assert_eq!(
            cancelled.reason.as_deref(),
            Some("cancelled by user before execution")
        );
        let lifetime = Uuid::now_v7().to_string();
        store.begin_lifetime(&lifetime, 4, "test").unwrap();
        assert!(store.admit(&lifetime, 4, 1).unwrap().attempts.is_empty());
        let event_count: i64 = store
            .conn()
            .unwrap()
            .query_row(
                "SELECT count(*) FROM events WHERE run_id=?1 AND kind='run_cancelled'",
                [&run],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(event_count, 1);
        assert!(matches!(
            store.cancel(&run, 5),
            Err(StoreError::Conflict(_))
        ));
        assert!(matches!(
            store.cancel(&Uuid::now_v7().to_string(), 5),
            Err(StoreError::NotFound(_))
        ));
    }

    #[test]
    fn cancelling_retry_wait_clears_retry_intent() {
        let (_temp, store) = store();
        let job = Uuid::now_v7().to_string();
        create(&store, &job, "x");
        let run = Uuid::now_v7().to_string();
        store.enqueue_manual("x", &run, 2).unwrap();
        let lifetime = Uuid::now_v7().to_string();
        store.begin_lifetime(&lifetime, 3, "test").unwrap();
        let admitted = store.admit(&lifetime, 3, 1).unwrap();
        assert_eq!(admitted.attempts.len(), 1);
        store
            .complete_attempt(&AttemptCompletion {
                run_id: run.clone(),
                attempt_number: 1,
                now_us: 4,
                duration_us: 1,
                state: "failed".into(),
                exit_code: Some(1),
                http_status: None,
                http_content_type: None,
                reason: "retryable failure".into(),
                retry: Some(RetryPlan {
                    not_before_us: 100,
                    classification: "process_exit".into(),
                }),
            })
            .unwrap();
        assert_eq!(store.run(&run).unwrap().state, "retry_wait");

        store.cancel(&run, 5).unwrap();

        assert_eq!(store.run(&run).unwrap().state, "cancelled");
        let retry_count: i64 = store
            .conn()
            .unwrap()
            .query_row(
                "SELECT count(*) FROM retry_intents WHERE run_id=?1",
                [&run],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(retry_count, 0);
        assert!(store.admit(&lifetime, 100, 1).unwrap().attempts.is_empty());
    }

    #[test]
    fn import_rolls_back_jobs_and_settings_on_late_conflict() {
        let (_temp, store) = store();
        let existing = Uuid::now_v7().to_string();
        create(&store, &existing, "existing");
        let before_settings = store.settings().unwrap();
        let new_id = Uuid::now_v7().to_string();
        let batch = ImportBatch {
            settings: SettingsRecord {
                global_concurrency: 32,
                ..before_settings.clone()
            },
            jobs: vec![
                ImportJob::Create {
                    job: CreateJob {
                        id: new_id.clone(),
                        name: "created-before-conflict".into(),
                        description: None,
                        tags_json: "[]".into(),
                        enabled: true,
                        definition_json: "{}".into(),
                        now_us: 10,
                        cursor_us: 10,
                    },
                    resolution: import_resolution(&new_id, "created-before-conflict", None),
                },
                ImportJob::Update {
                    job: UpdateJob {
                        id: existing.clone(),
                        expected_revision: 99,
                        name: "existing".into(),
                        description: None,
                        tags_json: "[]".into(),
                        enabled: true,
                        definition_json: "{}".into(),
                        now_us: 10,
                        cursor_us: 10,
                    },
                    resolution: import_resolution(&existing, "existing", Some(&existing)),
                },
            ],
            now_us: 10,
        };

        assert!(matches!(
            store.apply_import(&batch),
            Err(StoreError::Conflict(_))
        ));
        assert!(matches!(store.job(&new_id), Err(StoreError::NotFound(_))));
        assert_eq!(store.job(&existing).unwrap().current_revision, 1);
        assert_eq!(store.settings().unwrap(), before_settings);
    }

    #[test]
    fn import_applies_settings_create_and_update_in_one_commit() {
        let (_temp, store) = store();
        let existing = Uuid::now_v7().to_string();
        create(&store, &existing, "existing");
        let created = Uuid::now_v7().to_string();
        let mut settings = store.settings().unwrap();
        settings.global_concurrency = 24;
        let summary = store
            .apply_import(&ImportBatch {
                settings: settings.clone(),
                jobs: vec![
                    ImportJob::Update {
                        job: UpdateJob {
                            id: existing.clone(),
                            expected_revision: 1,
                            name: "renamed".into(),
                            description: Some("updated".into()),
                            tags_json: "[\"tag\"]".into(),
                            enabled: false,
                            definition_json: "{\"version\":2}".into(),
                            now_us: 10,
                            cursor_us: 5,
                        },
                        resolution: ImportResolution {
                            source_id: existing.clone(),
                            source_name: "renamed".into(),
                            expected_id_destination: Some(existing.clone()),
                            expected_name_destination: None,
                        },
                    },
                    ImportJob::Create {
                        job: CreateJob {
                            id: created.clone(),
                            name: "created".into(),
                            description: None,
                            tags_json: "[]".into(),
                            enabled: true,
                            definition_json: "{}".into(),
                            now_us: 10,
                            cursor_us: 10,
                        },
                        resolution: import_resolution(&created, "created", None),
                    },
                ],
                now_us: 10,
            })
            .unwrap();

        assert_eq!(summary.created, 1);
        assert_eq!(summary.updated, 1);
        assert_eq!(store.settings().unwrap(), settings);
        assert_eq!(store.job(&existing).unwrap().current_revision, 2);
        assert_eq!(store.job(&created).unwrap().current_revision, 1);
    }

    #[test]
    fn import_rechecks_source_id_and_name_mapping_inside_transaction() {
        let (_temp, store) = store();
        let destination = Uuid::now_v7().to_string();
        create(&store, &destination, "mapped-name");
        let source_id = Uuid::now_v7().to_string();
        create(&store, &source_id, "racing-owner");
        let before_settings = store.settings().unwrap();
        let mut changed_settings = before_settings.clone();
        changed_settings.global_concurrency = 20;

        let result = store.apply_import(&ImportBatch {
            settings: changed_settings,
            jobs: vec![ImportJob::Update {
                job: UpdateJob {
                    id: destination.clone(),
                    expected_revision: 1,
                    name: "mapped-name".into(),
                    description: Some("must not apply".into()),
                    tags_json: "[]".into(),
                    enabled: true,
                    definition_json: "{}".into(),
                    now_us: 10,
                    cursor_us: 1,
                },
                resolution: ImportResolution {
                    source_id,
                    source_name: "mapped-name".into(),
                    expected_id_destination: None,
                    expected_name_destination: Some(destination.clone()),
                },
            }],
            now_us: 10,
        });

        assert!(matches!(result, Err(StoreError::Conflict(_))));
        assert_eq!(store.job(&destination).unwrap().description, None);
        assert_eq!(store.settings().unwrap(), before_settings);
    }

    #[test]
    fn soft_deleted_name_can_be_reused_and_history_survives() {
        let (_temp, store) = store();
        let first = Uuid::now_v7().to_string();
        create(&store, &first, "x");
        let run = Uuid::now_v7().to_string();
        store.enqueue_manual("x", &run, 2).unwrap();
        store.remove_job("x", 3).unwrap();
        let second = Uuid::now_v7().to_string();
        create(&store, &second, "x");
        assert_ne!(first, store.job("x").unwrap().id);
        assert_eq!(store.run(&run).unwrap().job_id, first);
    }

    #[test]
    fn focused_explanation_runs_ignore_history_cap_and_use_request_time_then_id() {
        let (_temp, store) = store();
        let job = Uuid::now_v7().to_string();
        create(&store, &job, "explain");
        let lower_anomaly = Uuid::from_u128(1).to_string();
        let higher_anomaly = Uuid::from_u128(2).to_string();
        {
            let conn = store.conn().unwrap();
            for (id, state, sequence) in [
                (&lower_anomaly, "cancelled", 1_i64),
                (&higher_anomaly, "failed", 2_i64),
            ] {
                conn.execute(
                    "INSERT INTO runs(id,job_id,revision,trigger,nominal_us,requested_at_us,eligible_at_us,queue_sequence,snapshot_json,state,reason,finished_at_us) VALUES(?1,?2,1,'manual',NULL,2,2,?3,'{}',?4,'test anomaly',2)",
                    params![id, job, sequence, state],
                )
                .unwrap();
            }
        }
        let success_times = (3_i64..=1_003).collect::<Vec<_>>();
        let successes = insert_terminal_runs(&store, &job, 10_000, 3, &success_times);

        let capped = store.history(Some("explain"), 1_000).unwrap();
        assert_eq!(capped.len(), 1_000);
        assert!(capped.iter().all(|run| run.state == "succeeded"));

        let (latest, anomaly) = store.latest_and_anomalous_runs("explain").unwrap();
        assert_eq!(latest.unwrap().id, *successes.last().unwrap());
        let anomaly = anomaly.unwrap();
        assert_eq!(anomaly.id, higher_anomaly);
        assert_eq!(anomaly.state, "failed");
    }

    #[test]
    fn skip_overlap_records_explainable_terminal_run() {
        let (_temp, store) = store();
        let job = Uuid::now_v7().to_string();
        create_with_policy(&store, &job, "x", "skip", 1);
        let first = Uuid::now_v7().to_string();
        let second = Uuid::now_v7().to_string();
        assert_eq!(
            store.enqueue_manual("x", &first, 2).unwrap().state,
            "queued"
        );
        let skipped = store.enqueue_manual("x", &second, 3).unwrap();
        assert_eq!(skipped.state, "skipped_overlap");
        assert!(skipped.reason.unwrap().contains("active"));
    }

    #[test]
    fn replace_coalesces_queued_candidate() {
        let (_temp, store) = store();
        let job = Uuid::now_v7().to_string();
        create_with_policy(&store, &job, "x", "replace", 1);
        let first = Uuid::now_v7().to_string();
        let second = Uuid::now_v7().to_string();
        store.enqueue_manual("x", &first, 2).unwrap();
        assert_eq!(
            store.enqueue_manual("x", &second, 3).unwrap().state,
            "queued"
        );
        assert_eq!(store.run(&first).unwrap().state, "skipped_overlap");
        assert_eq!(
            store.run(&first).unwrap().reason.as_deref(),
            Some("superseded by newer replacement")
        );
        assert_eq!(store.run(&second).unwrap().state, "queued");
    }

    #[test]
    fn reconciliation_summaries_are_compact_and_atomic_with_cursor() {
        let (_temp, store) = store();
        let job = Uuid::now_v7().to_string();
        create(&store, &job, "x");
        store
            .materialize_with_summaries(
                &job,
                CursorUpdate {
                    expected_revision: 1,
                    expected_cursor_us: 1,
                    new_cursor_us: 1_000_000,
                    resolve_one_time: false,
                },
                &[],
                &[
                    ReconciliationSummary {
                        kind: "missed_start_deadline".into(),
                        count: 99_000,
                        first_nominal_us: 2,
                        last_nominal_us: 99_001,
                    },
                    ReconciliationSummary {
                        kind: "catch_up_omitted".into(),
                        count: 900,
                        first_nominal_us: 99_002,
                        last_nominal_us: 999_000,
                    },
                ],
                1_000_000,
            )
            .unwrap();
        let events = store.events_for_job(&job).unwrap();
        let summaries = events
            .iter()
            .filter(|event| event.kind.contains("missed") || event.kind.contains("omitted"))
            .collect::<Vec<_>>();
        assert_eq!(summaries.len(), 2);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&summaries[0].details_json).unwrap()["count"],
            99_000
        );

        let conflict = store.materialize_with_summaries(
            &job,
            CursorUpdate {
                expected_revision: 1,
                expected_cursor_us: 1,
                new_cursor_us: 2_000_000,
                resolve_one_time: false,
            },
            &[],
            &[ReconciliationSummary {
                kind: "must_not_commit".into(),
                count: 1,
                first_nominal_us: 1,
                last_nominal_us: 1,
            }],
            2_000_000,
        );
        assert!(matches!(conflict, Err(StoreError::Conflict(_))));
        assert!(
            store
                .events_for_job(&job)
                .unwrap()
                .iter()
                .all(|event| event.kind != "must_not_commit")
        );
    }

    #[test]
    fn replace_waits_for_active_confirmation_and_keeps_only_newest_candidate() {
        let (_temp, store) = store();
        let job = Uuid::now_v7().to_string();
        create_with_policy(&store, &job, "x", "replace", 1);
        let first = Uuid::now_v7().to_string();
        store.enqueue_manual("x", &first, 2).unwrap();
        let lifetime = Uuid::now_v7().to_string();
        store.begin_lifetime(&lifetime, 3, "test").unwrap();
        let first_attempt = store.admit(&lifetime, 3, 1).unwrap().attempts.remove(0);
        assert_eq!(store.run(&first).unwrap().state, "starting");
        assert_eq!(
            store
                .mark_attempt_running(&first, first_attempt.attempt_number, 4)
                .unwrap(),
            StartDecision::Ready
        );
        assert_eq!(
            store
                .mark_attempt_running(&first, first_attempt.attempt_number, 4)
                .unwrap(),
            StartDecision::Ready
        );

        let second = Uuid::now_v7().to_string();
        let third = Uuid::now_v7().to_string();
        store.enqueue_manual("x", &second, 5).unwrap();
        store.enqueue_manual("x", &third, 6).unwrap();
        assert_eq!(store.run(&second).unwrap().state, "skipped_overlap");
        assert!(store.cancellation_requested(&first).unwrap());
        assert!(store.admit(&lifetime, 6, 1).unwrap().attempts.is_empty());

        store
            .complete_attempt(&AttemptCompletion {
                run_id: first.clone(),
                attempt_number: first_attempt.attempt_number,
                now_us: 8,
                duration_us: 4,
                state: "cancelled".into(),
                exit_code: None,
                http_status: None,
                http_content_type: None,
                reason: "replacement termination confirmed".into(),
                retry: None,
            })
            .unwrap();
        let replacement = store.admit(&lifetime, 9, 1).unwrap();
        assert_eq!(replacement.attempts.len(), 1);
        assert_eq!(replacement.attempts[0].run_id, third);
    }

    #[test]
    fn cancellation_requested_while_starting_prevents_spawn() {
        for replacement in [false, true] {
            let (_temp, store) = store();
            let job = Uuid::now_v7().to_string();
            create_with_policy(
                &store,
                &job,
                "x",
                if replacement { "replace" } else { "skip" },
                1,
            );
            let first = Uuid::now_v7().to_string();
            store.enqueue_manual("x", &first, 2).unwrap();
            let lifetime = Uuid::now_v7().to_string();
            store.begin_lifetime(&lifetime, 3, "test").unwrap();
            let attempt = store.admit(&lifetime, 3, 1).unwrap().attempts.remove(0);

            if replacement {
                let successor = Uuid::now_v7().to_string();
                store.enqueue_manual("x", &successor, 4).unwrap();
            } else {
                store.cancel(&first, 4).unwrap();
            }

            assert_eq!(
                store
                    .mark_attempt_running(&first, attempt.attempt_number, 5)
                    .unwrap(),
                StartDecision::CancelledBeforeSpawn
            );
            assert_eq!(store.run(&first).unwrap().state, "cancelled");
            assert!(
                store
                    .events_for_job(&job)
                    .unwrap()
                    .iter()
                    .any(|event| event.kind == "cancelled_before_spawn")
            );
        }
    }

    #[test]
    fn ambiguous_mark_running_retry_rechecks_cancellation_before_spawn() {
        let (_temp, store) = store();
        let job = Uuid::now_v7().to_string();
        create(&store, &job, "x");
        let run = Uuid::now_v7().to_string();
        store.enqueue_manual("x", &run, 2).unwrap();
        let lifetime = Uuid::now_v7().to_string();
        store.begin_lifetime(&lifetime, 3, "test").unwrap();
        let attempt = store.admit(&lifetime, 3, 1).unwrap().attempts.remove(0);
        assert_eq!(
            store
                .mark_attempt_running(&run, attempt.attempt_number, 4)
                .unwrap(),
            StartDecision::Ready
        );
        store.cancel(&run, 5).unwrap();
        assert_eq!(
            store
                .mark_attempt_running(&run, attempt.attempt_number, 6)
                .unwrap(),
            StartDecision::CancelledBeforeSpawn
        );
        assert_eq!(store.run(&run).unwrap().state, "cancelled");
    }

    #[test]
    fn identical_attempt_completion_is_idempotent_but_mismatch_conflicts() {
        let (_temp, store) = store();
        let job = Uuid::now_v7().to_string();
        create(&store, &job, "x");
        let run = Uuid::now_v7().to_string();
        store.enqueue_manual("x", &run, 2).unwrap();
        let lifetime = Uuid::now_v7().to_string();
        store.begin_lifetime(&lifetime, 3, "test").unwrap();
        let attempt = store.admit(&lifetime, 3, 1).unwrap().attempts.remove(0);
        store
            .mark_attempt_running(&run, attempt.attempt_number, 4)
            .unwrap();
        let completion = AttemptCompletion {
            run_id: run,
            attempt_number: attempt.attempt_number,
            now_us: 5,
            duration_us: 1,
            state: "succeeded".into(),
            exit_code: Some(0),
            http_status: Some(200),
            http_content_type: Some("application/json; charset=utf-8".into()),
            reason: "known result".into(),
            retry: None,
        };
        store.complete_attempt(&completion).unwrap();
        store.complete_attempt(&completion).unwrap();
        let persisted = store.attempts_for_run(&completion.run_id).unwrap();
        assert_eq!(persisted[0].http_status, Some(200));
        assert_eq!(
            persisted[0].http_content_type.as_deref(),
            Some("application/json; charset=utf-8")
        );
        let mut mismatched = completion;
        mismatched.reason = "different result".into();
        assert!(matches!(
            store.complete_attempt(&mismatched),
            Err(StoreError::Conflict(_))
        ));
    }

    #[test]
    fn earliest_pending_eligible_at_us_covers_queued_and_retry_wait_runs() {
        let (_temp, store) = store();
        create(&store, &Uuid::now_v7().to_string(), "a");
        create(&store, &Uuid::now_v7().to_string(), "b");
        create(&store, &Uuid::now_v7().to_string(), "c");

        // Empty: nothing is pending admission.
        assert_eq!(store.earliest_pending_eligible_at_us().unwrap(), None);

        // Queued-only.
        let early = Uuid::now_v7().to_string();
        store.enqueue_manual("a", &early, 100).unwrap();
        assert_eq!(store.earliest_pending_eligible_at_us().unwrap(), Some(100));

        // Retry-wait-only.
        let retry = Uuid::now_v7().to_string();
        store.enqueue_manual("b", &retry, 500).unwrap();
        store
            .conn()
            .unwrap()
            .execute("UPDATE runs SET state='retry_wait' WHERE id=?1", [&retry])
            .unwrap();
        assert_eq!(store.earliest_pending_eligible_at_us().unwrap(), Some(100));

        // Mixed: the earliest pending deadline wins.
        let earliest = Uuid::now_v7().to_string();
        store.enqueue_manual("c", &earliest, 50).unwrap();
        assert_eq!(store.earliest_pending_eligible_at_us().unwrap(), Some(50));

        // Terminal runs are excluded.
        store
            .conn()
            .unwrap()
            .execute(
                "UPDATE runs SET state='succeeded',finished_at_us=999 WHERE id=?1",
                [&early],
            )
            .unwrap();
        assert_eq!(store.earliest_pending_eligible_at_us().unwrap(), Some(50));
    }

    #[test]
    fn finalize_output_reconciliation_is_idempotent_across_timestamps() {
        let (_temp, store) = store();
        let job = Uuid::now_v7().to_string();
        create(&store, &job, "finalize-idempotent");
        let run = Uuid::now_v7().to_string();
        store
            .enqueue_manual("finalize-idempotent", &run, 2)
            .unwrap();
        let lifetime = Uuid::now_v7().to_string();
        store.begin_lifetime(&lifetime, 3, "test").unwrap();
        let attempt = store.admit(&lifetime, 3, 1).unwrap().attempts.remove(0);
        let output = OutputRecord {
            run_id: run.clone(),
            attempt_number: attempt.attempt_number,
            relative_path: String::new(),
            state: "finalized".into(),
            retained_payload_bytes: 3,
            physical_bytes: 3,
            discarded_bytes: 0,
            truncated: false,
        };
        store.finalize_output(&output, 100).unwrap();
        // An identical re-finalization at a different instant is a no-op
        // because identity excludes the finalization timestamp.
        store.finalize_output(&output, 200).unwrap();
        let artifact_state: String = store
            .conn()
            .unwrap()
            .query_row(
                "SELECT state FROM output_artifacts WHERE run_id=?1 AND attempt_number=?2",
                params![run, attempt.attempt_number],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(artifact_state, "finalized");
        // A mismatched durable identity still conflicts.
        let mut mismatched = output;
        mismatched.retained_payload_bytes = 99;
        assert!(matches!(
            store.finalize_output(&mismatched, 300),
            Err(StoreError::Conflict(_))
        ));
    }

    #[test]
    fn runner_failure_recompletion_is_idempotent_across_timestamps() {
        for execution_may_have_started in [false, true] {
            let (_temp, store) = store();
            let job = Uuid::now_v7().to_string();
            create(&store, &job, "idempotent-runner-failure");
            let run = Uuid::now_v7().to_string();
            store
                .enqueue_manual("idempotent-runner-failure", &run, 2)
                .unwrap();
            let lifetime = Uuid::now_v7().to_string();
            store.begin_lifetime(&lifetime, 3, "test").unwrap();
            let attempt = store.admit(&lifetime, 3, 1).unwrap().attempts.remove(0);
            assert_eq!(
                store
                    .mark_attempt_running(&run, attempt.attempt_number, 4)
                    .unwrap(),
                StartDecision::Ready
            );
            store
                .complete_runner_failure(
                    &run,
                    attempt.attempt_number,
                    10,
                    "output storage failed",
                    execution_may_have_started,
                )
                .unwrap();
            // An identical recompletion at a different instant is a no-op
            // because identity excludes timestamps and the duration derived
            // from them.
            store
                .complete_runner_failure(
                    &run,
                    attempt.attempt_number,
                    999,
                    "output storage failed",
                    execution_may_have_started,
                )
                .unwrap();
            let state: String = store
                .conn()
                .unwrap()
                .query_row("SELECT state FROM runs WHERE id=?1", [&run], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(
                state,
                if execution_may_have_started {
                    "interrupted_unknown"
                } else {
                    "failed"
                }
            );
        }
    }

    #[test]
    fn runner_failure_terminalizes_an_attempt_whose_output_was_already_missing() {
        let (_temp, store) = store();
        let job = Uuid::now_v7().to_string();
        create(&store, &job, "already-missing");
        let run = Uuid::now_v7().to_string();
        store.enqueue_manual("already-missing", &run, 2).unwrap();
        let lifetime = Uuid::now_v7().to_string();
        store.begin_lifetime(&lifetime, 3, "test").unwrap();
        let attempt = store.admit(&lifetime, 3, 1).unwrap().attempts.remove(0);
        assert_eq!(
            store
                .mark_attempt_running(&run, attempt.attempt_number, 4)
                .unwrap(),
            StartDecision::Ready
        );
        // Startup recovery already reconciled the pending artifact as missing
        // at a different instant than the eventual completion, so an ordinary
        // finalize completion would conflict forever. The daemon's
        // terminalization fallback must instead succeed and reach a terminal
        // state.
        store
            .reconcile_output_missing(&run, attempt.attempt_number, 50)
            .unwrap();
        store
            .complete_runner_failure(
                &run,
                attempt.attempt_number,
                100,
                "durable completion conflict after target outcome",
                true,
            )
            .unwrap();
        assert_eq!(store.run(&run).unwrap().state, "interrupted_unknown");
    }

    #[test]
    fn unconfirmed_replacement_termination_quarantines_predecessor_and_fails_candidate() {
        let (_temp, store) = store();
        let job = Uuid::now_v7().to_string();
        create_with_policy(&store, &job, "x", "replace", 1);
        let predecessor = Uuid::now_v7().to_string();
        store.enqueue_manual("x", &predecessor, 2).unwrap();
        let lifetime = Uuid::now_v7().to_string();
        store.begin_lifetime(&lifetime, 3, "test").unwrap();
        let attempt = store.admit(&lifetime, 3, 1).unwrap().attempts.remove(0);
        assert_eq!(
            store
                .mark_attempt_running(&predecessor, attempt.attempt_number, 4)
                .unwrap(),
            StartDecision::Ready
        );
        let candidate = Uuid::now_v7().to_string();
        store.enqueue_manual("x", &candidate, 5).unwrap();

        store
            .complete_attempt(&AttemptCompletion {
                run_id: predecessor.clone(),
                attempt_number: attempt.attempt_number,
                now_us: 6,
                duration_us: 3,
                state: "termination_unconfirmed".into(),
                exit_code: None,
                http_status: None,
                http_content_type: None,
                reason: "TERM and KILL confirmation deadlines elapsed".into(),
                retry: None,
            })
            .unwrap();
        assert_eq!(store.run(&predecessor).unwrap().state, "running");
        assert_eq!(
            store.run(&predecessor).unwrap().reason.as_deref(),
            Some("termination_unconfirmed")
        );
        assert_eq!(store.run(&candidate).unwrap().state, "failed");
        assert!(store.admit(&lifetime, 7, 1).unwrap().attempts.is_empty());

        let next_lifetime = Uuid::now_v7().to_string();
        store.begin_lifetime(&next_lifetime, 8, "next").unwrap();
        assert_eq!(store.run(&predecessor).unwrap().state, "running");
        assert!(
            store
                .admit(&next_lifetime, 9, 1)
                .unwrap()
                .attempts
                .is_empty()
        );
        let later_replacement = Uuid::now_v7().to_string();
        assert_eq!(
            store
                .enqueue_manual("x", &later_replacement, 10)
                .unwrap()
                .state,
            "failed"
        );
        let current = store.job("x").unwrap();
        store
            .update_job(&UpdateJob {
                id: current.id,
                expected_revision: current.current_revision,
                name: current.name,
                description: current.description,
                tags_json: current.tags_json,
                enabled: true,
                definition_json: "{\"policy\":{\"overlap\":\"allow\",\"per_job_concurrency\":2}}"
                    .into(),
                now_us: 11,
                cursor_us: current.cursor_us,
            })
            .unwrap();
        let allowed_snapshot = Uuid::now_v7().to_string();
        assert_eq!(
            store
                .enqueue_manual("x", &allowed_snapshot, 12)
                .unwrap()
                .state,
            "skipped_overlap"
        );
        let catch_up = Uuid::now_v7().to_string();
        store
            .materialize(
                &job,
                CursorUpdate {
                    expected_revision: 2,
                    expected_cursor_us: 1,
                    new_cursor_us: 20,
                    resolve_one_time: false,
                },
                &[NewScheduledRun {
                    id: catch_up.clone(),
                    job_id: job.clone(),
                    revision: 2,
                    trigger: "catch_up".into(),
                    nominal_us: 20,
                    requested_at_us: 20,
                    eligible_at_us: 20,
                    snapshot_json: "{\"policy\":{\"overlap\":\"allow\",\"per_job_concurrency\":2}}"
                        .into(),
                }],
                20,
            )
            .unwrap();
        assert_eq!(store.run(&catch_up).unwrap().state, "skipped_overlap");
        assert!(
            store
                .admit(&next_lifetime, 12, 2)
                .unwrap()
                .attempts
                .is_empty()
        );
        let ordinary_cancel = store.cancel(&predecessor, 21).unwrap_err();
        assert!(
            ordinary_cancel
                .to_string()
                .contains("--acknowledge-unconfirmed")
        );
        assert!(matches!(
            store.cancel_with_acknowledgement(&candidate, 22, true),
            Err(StoreError::Conflict(_))
        ));
        assert_eq!(
            store
                .cancel_with_acknowledgement(&predecessor, 23, true)
                .unwrap(),
            CancelOutcome::AcknowledgedUnconfirmed
        );
        let acknowledged = store.run(&predecessor).unwrap();
        assert_eq!(acknowledged.state, "interrupted_unknown");
        assert_eq!(acknowledged.finished_at_us, Some(23));
        assert!(
            acknowledged
                .reason
                .as_deref()
                .unwrap()
                .contains("acknowledged by operator")
        );
        let acknowledged_attempt_state: String = store
            .conn()
            .unwrap()
            .query_row(
                "SELECT state FROM attempts WHERE run_id=?1 AND attempt_number=?2",
                params![predecessor, attempt.attempt_number],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(acknowledged_attempt_state, "interrupted_unknown");
        let retry_intents: i64 = store
            .conn()
            .unwrap()
            .query_row(
                "SELECT count(*) FROM retry_intents WHERE run_id=?1",
                [&predecessor],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(retry_intents, 0);
        let active_runs: i64 = store
            .conn()
            .unwrap()
            .query_row(
                "SELECT count(*) FROM runs WHERE id=?1 AND state IN ('queued','starting','running','retry_wait')",
                [&predecessor],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(active_runs, 0);
        let acknowledgement_events = store.events_for_run(&predecessor).unwrap();
        assert!(acknowledgement_events.iter().any(|event| {
            event.kind == "termination_unconfirmed_acknowledged"
                && event.details_json.contains("process_liveness_unconfirmed")
        }));
        assert!(matches!(
            store.cancel_with_acknowledgement(&predecessor, 24, true),
            Err(StoreError::Conflict(_))
        ));
        let released = Uuid::now_v7().to_string();
        assert_eq!(
            store.enqueue_manual("x", &released, 25).unwrap().state,
            "queued"
        );
        assert_eq!(
            store
                .admit(&next_lifetime, 25, 2)
                .unwrap()
                .attempts
                .remove(0)
                .run_id,
            released
        );
        let attempt_state: String = store
            .conn()
            .unwrap()
            .query_row(
                "SELECT state FROM attempts WHERE run_id=?1 AND attempt_number=?2",
                params![predecessor, attempt.attempt_number],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(attempt_state, "interrupted_unknown");
    }

    #[test]
    fn crash_boundaries_recover_active_attempts_without_retry() {
        for mark_running in [false, true] {
            let (_temp, store) = store();
            let job = Uuid::now_v7().to_string();
            create(&store, &job, "x");
            let run = Uuid::now_v7().to_string();
            store.enqueue_manual("x", &run, 2).unwrap();
            let old_lifetime = Uuid::now_v7().to_string();
            store.begin_lifetime(&old_lifetime, 3, "old").unwrap();
            let attempt = store.admit(&old_lifetime, 4, 1).unwrap().attempts.remove(0);
            if mark_running {
                store
                    .mark_attempt_running(&run, attempt.attempt_number, 5)
                    .unwrap();
            }
            store
                .conn()
                .unwrap()
                .execute(
                    "INSERT INTO retry_intents(run_id,prior_attempt_number,not_before_us,classification,created_at_us) VALUES(?1,?2,100,'injected_stale',5)",
                    params![run, attempt.attempt_number],
                )
                .unwrap();

            let new_lifetime = Uuid::now_v7().to_string();
            assert_eq!(store.begin_lifetime(&new_lifetime, 6, "new").unwrap(), 1);
            assert_eq!(store.run(&run).unwrap().state, "interrupted_unknown");
            assert!(
                store
                    .admit(&new_lifetime, 100, 1)
                    .unwrap()
                    .attempts
                    .is_empty()
            );
            let retry_count: i64 = store
                .conn()
                .unwrap()
                .query_row(
                    "SELECT count(*) FROM retry_intents WHERE run_id=?1",
                    [&run],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(retry_count, 0);
        }
    }

    #[test]
    fn one_time_occurrence_stays_unique_across_lifecycle_fault_boundaries() {
        #[derive(Clone, Copy, Debug)]
        enum FaultBoundary {
            BeforeAdmission,
            StartingBeforeSpawn,
            RunningAfterSpawn,
            OutcomeBeforeCompletion,
        }

        for boundary in [
            FaultBoundary::BeforeAdmission,
            FaultBoundary::StartingBeforeSpawn,
            FaultBoundary::RunningAfterSpawn,
            FaultBoundary::OutcomeBeforeCompletion,
        ] {
            let (_temp, store) = store();
            let job = Uuid::now_v7().to_string();
            create(&store, &job, "once");
            let run = Uuid::now_v7().to_string();
            let scheduled = NewScheduledRun {
                id: run.clone(),
                job_id: job.clone(),
                revision: 1,
                trigger: "catch_up".into(),
                nominal_us: 10,
                requested_at_us: 20,
                eligible_at_us: 20,
                snapshot_json: "{}".into(),
            };
            assert_eq!(
                store
                    .materialize(
                        &job,
                        CursorUpdate {
                            expected_revision: 1,
                            expected_cursor_us: 1,
                            new_cursor_us: 20,
                            resolve_one_time: true,
                        },
                        std::slice::from_ref(&scheduled),
                        20,
                    )
                    .unwrap()
                    .inserted,
                1,
                "{boundary:?}"
            );

            if !matches!(boundary, FaultBoundary::BeforeAdmission) {
                let old_lifetime = Uuid::now_v7().to_string();
                store.begin_lifetime(&old_lifetime, 21, "old").unwrap();
                let attempt = store
                    .admit(&old_lifetime, 22, 1)
                    .unwrap()
                    .attempts
                    .remove(0);
                if matches!(
                    boundary,
                    FaultBoundary::RunningAfterSpawn | FaultBoundary::OutcomeBeforeCompletion
                ) {
                    store
                        .mark_attempt_running(&run, attempt.attempt_number, 23)
                        .unwrap();
                }
                store
                    .conn()
                    .unwrap()
                    .execute(
                        "INSERT INTO retry_intents(run_id,prior_attempt_number,not_before_us,classification,created_at_us) VALUES(?1,?2,100,'injected_stale',23)",
                        params![run, attempt.attempt_number],
                    )
                    .unwrap();
            }

            let new_lifetime = Uuid::now_v7().to_string();
            let recovered = store.begin_lifetime(&new_lifetime, 24, "new").unwrap();
            let expected_recovered =
                usize::from(!matches!(boundary, FaultBoundary::BeforeAdmission));
            assert_eq!(recovered, expected_recovered, "{boundary:?}");

            let duplicate = store
                .materialize(
                    &job,
                    CursorUpdate {
                        expected_revision: 1,
                        expected_cursor_us: 20,
                        new_cursor_us: 30,
                        resolve_one_time: true,
                    },
                    &[NewScheduledRun {
                        id: Uuid::now_v7().to_string(),
                        requested_at_us: 30,
                        eligible_at_us: 30,
                        ..scheduled
                    }],
                    30,
                )
                .unwrap();
            assert_eq!(duplicate.inserted, 0, "{boundary:?}");
            assert_eq!(duplicate.duplicates, 1, "{boundary:?}");

            let conn = store.conn().unwrap();
            let occurrence_count: i64 = conn
                .query_row(
                    "SELECT count(*) FROM runs WHERE job_id=?1 AND revision=1 AND nominal_us=10 AND trigger<>'manual'",
                    [&job],
                    |row| row.get(0),
                )
                .unwrap();
            let retry_count: i64 = conn
                .query_row(
                    "SELECT count(*) FROM retry_intents WHERE run_id=?1",
                    [&run],
                    |row| row.get(0),
                )
                .unwrap();
            drop(conn);
            assert_eq!(occurrence_count, 1, "{boundary:?}");
            assert_eq!(retry_count, 0, "{boundary:?}");
            assert!(!store.job("once").unwrap().enabled, "{boundary:?}");
            assert_eq!(
                store.run(&run).unwrap().state,
                if matches!(boundary, FaultBoundary::BeforeAdmission) {
                    "queued"
                } else {
                    "interrupted_unknown"
                },
                "{boundary:?}"
            );
        }
    }

    #[test]
    fn retry_wait_survives_lifetime_restart_and_respects_not_before() {
        let (_temp, store) = store();
        let job = Uuid::now_v7().to_string();
        create(&store, &job, "x");
        let run = Uuid::now_v7().to_string();
        store.enqueue_manual("x", &run, 2).unwrap();
        let first_lifetime = Uuid::now_v7().to_string();
        store.begin_lifetime(&first_lifetime, 3, "first").unwrap();
        let attempt = store
            .admit(&first_lifetime, 3, 1)
            .unwrap()
            .attempts
            .remove(0);
        store
            .complete_attempt(&AttemptCompletion {
                run_id: run.clone(),
                attempt_number: attempt.attempt_number,
                now_us: 4,
                duration_us: 1,
                state: "failed".into(),
                exit_code: Some(1),
                http_status: None,
                http_content_type: None,
                reason: "known failure".into(),
                retry: Some(RetryPlan {
                    not_before_us: 100,
                    classification: "known_failure".into(),
                }),
            })
            .unwrap();
        let second_lifetime = Uuid::now_v7().to_string();
        store.begin_lifetime(&second_lifetime, 5, "second").unwrap();
        assert!(
            store
                .admit(&second_lifetime, 99, 1)
                .unwrap()
                .attempts
                .is_empty()
        );
        let retry = store.admit(&second_lifetime, 100, 1).unwrap();
        assert_eq!(retry.attempts.len(), 1);
        assert_eq!(retry.attempts[0].attempt_number, 2);
    }

    #[test]
    fn admission_enforces_same_job_slots_across_normal_and_catch_up_lanes() {
        for (overlap, per_job_limit, expected_admitted) in
            [("skip", 1, 1), ("replace", 1, 1), ("allow", 2, 2)]
        {
            let (_temp, store) = store();
            let job = Uuid::now_v7().to_string();
            create_with_policy(&store, &job, "x", overlap, per_job_limit);
            let snapshot = store.job("x").unwrap().definition_json;
            let scheduled = Uuid::now_v7().to_string();
            let catch_up = Uuid::now_v7().to_string();
            store
                .materialize(
                    &job,
                    CursorUpdate {
                        expected_revision: 1,
                        expected_cursor_us: 1,
                        new_cursor_us: 3,
                        resolve_one_time: false,
                    },
                    &[
                        NewScheduledRun {
                            id: catch_up.clone(),
                            job_id: job.clone(),
                            revision: 1,
                            trigger: "catch_up".into(),
                            nominal_us: 2,
                            requested_at_us: 3,
                            eligible_at_us: 3,
                            snapshot_json: snapshot.clone(),
                        },
                        NewScheduledRun {
                            id: scheduled.clone(),
                            job_id: job.clone(),
                            revision: 1,
                            trigger: "scheduled".into(),
                            nominal_us: 3,
                            requested_at_us: 3,
                            eligible_at_us: 3,
                            snapshot_json: snapshot,
                        },
                    ],
                    3,
                )
                .unwrap();
            let lifetime = Uuid::now_v7().to_string();
            store.begin_lifetime(&lifetime, 4, "test").unwrap();
            let first = store.admit(&lifetime, 4, 16).unwrap();
            assert_eq!(first.attempts.len(), expected_admitted, "{overlap}");
            assert_eq!(first.attempts[0].run_id, catch_up, "{overlap}");
            assert_eq!(
                first
                    .attempts
                    .iter()
                    .filter(|attempt| attempt.job_id == job)
                    .count(),
                expected_admitted,
                "{overlap}"
            );
            if overlap == "replace" {
                assert_eq!(store.run(&scheduled).unwrap().state, "queued");
            }
        }
    }

    #[test]
    fn overlap_trigger_and_capacity_matrix_is_explainable_and_bounded() {
        for overlap in ["skip", "replace", "allow"] {
            for trigger in ["manual", "scheduled", "catch_up"] {
                let (_temp, store) = store();
                let job = Uuid::now_v7().to_string();
                create_with_policy(&store, &job, "x", overlap, 2);
                let predecessor = Uuid::now_v7().to_string();
                store.enqueue_manual("x", &predecessor, 2).unwrap();
                let candidate = Uuid::now_v7().to_string();
                let candidate_record = if trigger == "manual" {
                    store.enqueue_manual("x", &candidate, 3).unwrap()
                } else {
                    let snapshot = store.job("x").unwrap().definition_json;
                    store
                        .materialize(
                            &job,
                            CursorUpdate {
                                expected_revision: 1,
                                expected_cursor_us: 1,
                                new_cursor_us: 3,
                                resolve_one_time: false,
                            },
                            &[NewScheduledRun {
                                id: candidate.clone(),
                                job_id: job.clone(),
                                revision: 1,
                                trigger: trigger.into(),
                                nominal_us: 3,
                                requested_at_us: 3,
                                eligible_at_us: 3,
                                snapshot_json: snapshot,
                            }],
                            3,
                        )
                        .unwrap();
                    store.run(&candidate).unwrap()
                };
                let expected_state = match (overlap, trigger) {
                    (_, "catch_up") | ("replace" | "allow", _) => "queued",
                    ("skip", _) => "skipped_overlap",
                    _ => unreachable!(),
                };
                assert_eq!(
                    candidate_record.state, expected_state,
                    "{overlap}/{trigger}"
                );
                if overlap == "replace" && trigger != "catch_up" {
                    assert_eq!(
                        store.run(&predecessor).unwrap().state,
                        "skipped_overlap",
                        "{overlap}/{trigger}"
                    );
                }
                let lifetime = Uuid::now_v7().to_string();
                store.begin_lifetime(&lifetime, 4, "test").unwrap();
                assert!(store.admit(&lifetime, 4, 0).unwrap().attempts.is_empty());
                let admitted = store.admit(&lifetime, 4, 64).unwrap();
                let expected_count = usize::from(overlap == "allow" && trigger != "catch_up") + 1;
                assert_eq!(
                    admitted.attempts.len(),
                    expected_count,
                    "{overlap}/{trigger}"
                );
            }
        }

        let (_temp, store) = store();
        let job = Uuid::now_v7().to_string();
        create_with_policy(&store, &job, "allow", "allow", 2);
        let first = Uuid::now_v7().to_string();
        let second = Uuid::now_v7().to_string();
        let rejected = Uuid::now_v7().to_string();
        store.enqueue_manual("allow", &first, 2).unwrap();
        store.enqueue_manual("allow", &second, 3).unwrap();
        assert_eq!(
            store.enqueue_manual("allow", &rejected, 4).unwrap().state,
            "skipped_concurrency"
        );
        let catch_up = Uuid::now_v7().to_string();
        let snapshot = store.job("allow").unwrap().definition_json;
        store
            .materialize(
                &job,
                CursorUpdate {
                    expected_revision: 1,
                    expected_cursor_us: 1,
                    new_cursor_us: 5,
                    resolve_one_time: false,
                },
                &[NewScheduledRun {
                    id: catch_up.clone(),
                    job_id: job.clone(),
                    revision: 1,
                    trigger: "catch_up".into(),
                    nominal_us: 5,
                    requested_at_us: 5,
                    eligible_at_us: 5,
                    snapshot_json: snapshot,
                }],
                5,
            )
            .unwrap();
        assert_eq!(store.run(&catch_up).unwrap().state, "queued");
        let lifetime = Uuid::now_v7().to_string();
        store.begin_lifetime(&lifetime, 6, "test").unwrap();
        assert_eq!(store.admit(&lifetime, 6, 64).unwrap().attempts.len(), 2);
        assert_eq!(store.run(&catch_up).unwrap().state, "queued");
    }

    #[test]
    fn retry_wait_interacts_with_normal_occurrences_by_overlap_policy() {
        for overlap in ["skip", "replace", "allow"] {
            let (_temp, store) = store();
            let job = Uuid::now_v7().to_string();
            create_with_policy(&store, &job, "x", overlap, 2);
            let retried = Uuid::now_v7().to_string();
            store.enqueue_manual("x", &retried, 2).unwrap();
            let lifetime = Uuid::now_v7().to_string();
            store.begin_lifetime(&lifetime, 3, "test").unwrap();
            let attempt = store.admit(&lifetime, 3, 64).unwrap().attempts.remove(0);
            store
                .complete_attempt(&AttemptCompletion {
                    run_id: retried.clone(),
                    attempt_number: attempt.attempt_number,
                    now_us: 4,
                    duration_us: 1,
                    state: "failed".into(),
                    exit_code: Some(7),
                    http_status: None,
                    http_content_type: None,
                    reason: "known failure".into(),
                    retry: Some(RetryPlan {
                        not_before_us: 10,
                        classification: "known_failure".into(),
                    }),
                })
                .unwrap();
            let normal = Uuid::now_v7().to_string();
            let normal_record = store.enqueue_manual("x", &normal, 5).unwrap();
            match overlap {
                "skip" => {
                    assert_eq!(normal_record.state, "skipped_overlap");
                    assert_eq!(store.run(&retried).unwrap().state, "retry_wait");
                    let admitted = store.admit(&lifetime, 10, 64).unwrap();
                    assert_eq!(admitted.attempts.len(), 1);
                    assert_eq!(admitted.attempts[0].run_id, retried);
                }
                "replace" => {
                    assert_eq!(normal_record.state, "queued");
                    assert_eq!(store.run(&retried).unwrap().state, "skipped_overlap");
                    let admitted = store.admit(&lifetime, 10, 64).unwrap();
                    assert_eq!(admitted.attempts.len(), 1);
                    assert_eq!(admitted.attempts[0].run_id, normal);
                }
                "allow" => {
                    assert_eq!(normal_record.state, "queued");
                    assert_eq!(store.run(&retried).unwrap().state, "retry_wait");
                    let admitted = store.admit(&lifetime, 10, 64).unwrap();
                    assert_eq!(admitted.attempts.len(), 2);
                    assert!(admitted.attempts.iter().any(|item| item.run_id == retried));
                    assert!(admitted.attempts.iter().any(|item| item.run_id == normal));
                }
                _ => unreachable!(),
            }
        }
    }

    #[test]
    fn admission_atomically_rechecks_durable_limit_across_stale_increase_and_decrease_reads() {
        let (_temp, store) = store();
        for index in 0..4 {
            let job = Uuid::now_v7().to_string();
            let name = format!("job-{index}");
            create_with_policy(&store, &job, &name, "skip", 1);
            store
                .enqueue_manual(&name, &Uuid::now_v7().to_string(), 2)
                .unwrap();
        }
        let lifetime = Uuid::now_v7().to_string();
        store.begin_lifetime(&lifetime, 3, "test").unwrap();

        store.set_setting("global_concurrency", "3", 4).unwrap();
        let stale_high = store.settings().unwrap().global_concurrency;
        assert_eq!(stale_high, 3);
        store.set_setting("global_concurrency", "1", 5).unwrap();
        let first = store.admit(&lifetime, 5, 64).unwrap();
        assert_eq!(first.attempts.len(), 1);

        let stale_low = store.settings().unwrap().global_concurrency;
        assert_eq!(stale_low, 1);
        store.set_setting("global_concurrency", "3", 6).unwrap();
        let expanded = store.admit(&lifetime, 6, 63).unwrap();
        assert_eq!(expanded.attempts.len(), 2);

        store.set_setting("global_concurrency", "1", 7).unwrap();
        assert!(store.admit(&lifetime, 7, 61).unwrap().attempts.is_empty());

        for attempt in expanded.attempts {
            store
                .complete_attempt(&AttemptCompletion {
                    run_id: attempt.run_id,
                    attempt_number: attempt.attempt_number,
                    now_us: 8,
                    duration_us: 0,
                    state: "succeeded".into(),
                    exit_code: Some(0),
                    http_status: None,
                    http_content_type: None,
                    reason: "test completion".into(),
                    retry: None,
                })
                .unwrap();
        }
        store.set_setting("global_concurrency", "3", 9).unwrap();
        assert_eq!(store.admit(&lifetime, 9, 63).unwrap().attempts.len(), 1);
    }

    #[test]
    fn admission_stresses_default_and_maximum_global_concurrency() {
        for (configured_limit, expected_limit) in [(None, 16), (Some("64"), 64)] {
            let (_temp, store) = store();
            if let Some(limit) = configured_limit {
                store.set_setting("global_concurrency", limit, 2).unwrap();
            }
            assert_eq!(store.settings().unwrap().global_concurrency, expected_limit);

            for index in 0..=expected_limit {
                let job_id = Uuid::from_u128(1_000 + u128::try_from(index).unwrap()).to_string();
                let run_id = Uuid::from_u128(2_000 + u128::try_from(index).unwrap()).to_string();
                let name = format!("stress-{index}");
                create(&store, &job_id, &name);
                store.enqueue_manual(&name, &run_id, 3).unwrap();
            }

            let lifetime =
                Uuid::from_u128(3_000 + u128::try_from(expected_limit).unwrap()).to_string();
            store.begin_lifetime(&lifetime, 4, "test").unwrap();
            let admitted = store.admit(&lifetime, 4, 64).unwrap();
            assert_eq!(
                admitted.attempts.len(),
                usize::try_from(expected_limit).unwrap()
            );
            assert!(store.admit(&lifetime, 4, 64).unwrap().attempts.is_empty());

            let active: i64 = store
                .conn()
                .unwrap()
                .query_row(
                    "SELECT count(*) FROM attempts WHERE state IN ('starting','running')",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(active, expected_limit);
        }
    }

    #[test]
    fn output_retention_candidates_exclude_active_runs_and_preserve_eviction_order() {
        let (_temp, store) = store();
        let active_job = Uuid::from_u128(4_001).to_string();
        let terminal_job = Uuid::from_u128(4_002).to_string();
        let active_run = Uuid::from_u128(4_003).to_string();
        let terminal_run = Uuid::from_u128(4_004).to_string();
        create(&store, &active_job, "active");
        create(&store, &terminal_job, "terminal");
        store.enqueue_manual("active", &active_run, 2).unwrap();
        store.enqueue_manual("terminal", &terminal_run, 3).unwrap();
        let lifetime = Uuid::from_u128(4_005).to_string();
        store.begin_lifetime(&lifetime, 4, "test").unwrap();
        let admitted = store.admit(&lifetime, 4, 2).unwrap();
        assert_eq!(admitted.attempts.len(), 2);

        store
            .finalize_output(
                &OutputRecord {
                    run_id: active_run.clone(),
                    attempt_number: 1,
                    relative_path: String::new(),
                    state: "finalized".into(),
                    retained_payload_bytes: 10,
                    physical_bytes: 12,
                    discarded_bytes: 0,
                    truncated: false,
                },
                10,
            )
            .unwrap();
        store
            .finalize_output(
                &OutputRecord {
                    run_id: terminal_run.clone(),
                    attempt_number: 1,
                    relative_path: String::new(),
                    state: "finalized".into(),
                    retained_payload_bytes: 20,
                    physical_bytes: 24,
                    discarded_bytes: 0,
                    truncated: false,
                },
                20,
            )
            .unwrap();
        store
            .complete_attempt(&AttemptCompletion {
                run_id: terminal_run.clone(),
                attempt_number: 1,
                now_us: 21,
                duration_us: 17,
                state: "succeeded".into(),
                exit_code: Some(0),
                http_status: None,
                http_content_type: None,
                reason: "completed".into(),
                retry: None,
            })
            .unwrap();

        let candidates = store.output_retention_candidates(10).unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].run_id, terminal_run);

        store
            .complete_attempt(&AttemptCompletion {
                run_id: active_run.clone(),
                attempt_number: 1,
                now_us: 30,
                duration_us: 26,
                state: "succeeded".into(),
                exit_code: Some(0),
                http_status: None,
                http_content_type: None,
                reason: "completed".into(),
                retry: None,
            })
            .unwrap();
        let candidates = store.output_retention_candidates(10).unwrap();
        assert_eq!(
            candidates
                .iter()
                .map(|candidate| candidate.run_id.as_str())
                .collect::<Vec<_>>(),
            [active_run.as_str(), terminal_run.as_str()]
        );
    }

    #[test]
    fn run_retention_unions_age_per_job_and_global_bounds_oldest_first() {
        const DAY_US: i64 = 86_400_000_000;
        let (_temp, store) = store();
        let first_job = Uuid::from_u128(6_001).to_string();
        let second_job = Uuid::from_u128(6_002).to_string();
        create(&store, &first_job, "first-retention");
        create(&store, &second_job, "second-retention");

        store
            .set_setting("run_retention_count", "10000", 2)
            .unwrap();
        store
            .set_setting("run_retention_age_us", &i64::MAX.to_string(), 2)
            .unwrap();
        let per_job_runs = insert_terminal_runs(
            &store,
            &first_job,
            10_000,
            1,
            &(1..=1_002).collect::<Vec<_>>(),
        );
        let per_job = store.run_retention_candidates(2_000, usize::MAX).unwrap();
        assert_eq!(per_job.len(), 2);
        assert_eq!(per_job[0].run_id, per_job_runs[0]);
        assert_eq!(per_job[1].run_id, per_job_runs[1]);

        store.set_setting("run_retention_count", "2", 3).unwrap();
        let second_job_runs =
            insert_terminal_runs(&store, &second_job, 20_000, 2_000, &[2_000, 3_000]);
        let bounded = store.run_retention_candidates(4_000, usize::MAX).unwrap();
        assert_eq!(bounded.len(), MAINTENANCE_BATCH_LIMIT);
        assert_eq!(bounded[0].run_id, per_job_runs[0]);
        assert!(!bounded.iter().any(|item| item.run_id == second_job_runs[1]));

        store
            .set_setting("run_retention_count", "10000", 4)
            .unwrap();
        store
            .set_setting("run_retention_age_us", &(90 * DAY_US).to_string(), 4)
            .unwrap();
        let age_candidates = store
            .run_retention_candidates(100 * DAY_US, usize::MAX)
            .unwrap();
        assert_eq!(age_candidates.len(), MAINTENANCE_BATCH_LIMIT);
        assert_eq!(age_candidates[0].run_id, per_job_runs[0]);
        assert!(
            age_candidates
                .windows(2)
                .all(|pair| pair[0].finished_at_us <= pair[1].finished_at_us)
        );
    }

    #[test]
    fn run_retention_global_count_deduplicates_candidates_and_protects_active_runs() {
        let (_temp, store) = store();
        let first_job = Uuid::from_u128(7_001).to_string();
        let second_job = Uuid::from_u128(7_002).to_string();
        create(&store, &first_job, "global-first");
        create(&store, &second_job, "global-second");
        store.set_setting("run_retention_count", "2", 2).unwrap();
        store
            .set_setting("run_retention_age_us", &i64::MAX.to_string(), 2)
            .unwrap();
        let first = insert_terminal_runs(&store, &first_job, 30_000, 1, &[1, 3]);
        let second = insert_terminal_runs(&store, &second_job, 40_000, 3, &[2, 4]);
        let active_run = Uuid::from_u128(7_003).to_string();
        store
            .enqueue_manual("global-first", &active_run, 5)
            .unwrap();

        let candidates = store.run_retention_candidates(6, 100).unwrap();
        assert_eq!(
            candidates
                .iter()
                .map(|candidate| candidate.run_id.as_str())
                .collect::<Vec<_>>(),
            [first[0].as_str(), second[0].as_str()]
        );
        assert!(!candidates.iter().any(|item| item.run_id == active_run));
        assert!(matches!(
            store.mark_run_retention_pending(
                &RunRetentionCandidate {
                    run_id: active_run,
                    job_id: first_job,
                    finished_at_us: 0,
                },
                6,
            ),
            Err(StoreError::Conflict(_))
        ));
    }

    #[test]
    fn referenced_partial_finalization_and_missing_reconcile_after_reopen() {
        let temp = tempfile::tempdir().unwrap();
        let paths = StatePaths::new(temp.path().into());
        let store = Store::open(paths.clone(), "test", 1).unwrap();
        let job = Uuid::from_u128(8_001).to_string();
        create_with_policy(&store, &job, "recover-output", "allow", 2);
        let finalized_run = Uuid::from_u128(8_002).to_string();
        let missing_run = Uuid::from_u128(8_003).to_string();
        store
            .enqueue_manual("recover-output", &finalized_run, 2)
            .unwrap();
        store
            .enqueue_manual("recover-output", &missing_run, 3)
            .unwrap();
        let lifetime = Uuid::from_u128(8_004).to_string();
        store.begin_lifetime(&lifetime, 4, "test").unwrap();
        assert_eq!(store.admit(&lifetime, 4, 2).unwrap().attempts.len(), 2);
        store
            .conn()
            .unwrap()
            .execute(
                "UPDATE output_artifacts SET state='active' WHERE run_id=?1",
                [&missing_run],
            )
            .unwrap();
        drop(store);

        let reopened = Store::open(paths, "test", 5).unwrap();
        // A restarted daemon owns a fresh lifetime; stale attempts are no
        // longer protected and become recovery candidates.
        let reopened_lifetime = Uuid::from_u128(8_005).to_string();
        reopened
            .begin_lifetime(&reopened_lifetime, 5, "test")
            .unwrap();
        let partials = reopened
            .referenced_partial_artifacts(usize::MAX, &reopened_lifetime)
            .unwrap();
        assert_eq!(partials.len(), 2);
        assert!(partials.iter().any(|item| item.run_id == finalized_run));
        assert!(
            partials
                .iter()
                .any(|item| item.run_id == missing_run && item.state == "active")
        );
        let output = OutputRecord {
            run_id: finalized_run.clone(),
            attempt_number: 1,
            relative_path: format!("{finalized_run}/1.partial"),
            state: "active".into(),
            retained_payload_bytes: 12,
            physical_bytes: 20,
            discarded_bytes: 3,
            truncated: true,
        };
        reopened.reconcile_output_finalized(&output, 6).unwrap();
        reopened.reconcile_output_finalized(&output, 6).unwrap();
        reopened
            .reconcile_output_missing(&missing_run, 1, 7)
            .unwrap();
        reopened
            .reconcile_output_missing(&missing_run, 1, 8)
            .unwrap();
        assert!(
            reopened
                .referenced_partial_artifacts(100, &reopened_lifetime)
                .unwrap()
                .is_empty()
        );
        let states: Vec<String> = reopened
            .conn()
            .unwrap()
            .prepare("SELECT state FROM output_artifacts ORDER BY run_id")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(states, ["finalized", "missing"]);
    }

    #[test]
    fn recovery_candidates_exclude_live_lifetime_attempts() {
        let temp = tempfile::tempdir().unwrap();
        let paths = StatePaths::new(temp.path().into());
        let store = Store::open(paths, "test", 1).unwrap();
        let job = Uuid::from_u128(8_101).to_string();
        create_with_policy(&store, &job, "recover-lifetime", "allow", 2);
        let first_run = Uuid::from_u128(8_102).to_string();
        let second_run = Uuid::from_u128(8_103).to_string();
        store
            .enqueue_manual("recover-lifetime", &first_run, 2)
            .unwrap();
        store
            .enqueue_manual("recover-lifetime", &second_run, 3)
            .unwrap();
        let live_lifetime = Uuid::from_u128(8_104).to_string();
        store.begin_lifetime(&live_lifetime, 4, "test").unwrap();
        assert_eq!(store.admit(&live_lifetime, 4, 2).unwrap().attempts.len(), 2);

        // Regression: both output rows are 'pending' and their partial files
        // do not exist yet, but the attempts are 'starting' under the live
        // lifetime. A maintenance pass must never select them as recovery
        // candidates; doing so marks them 'missing' and permanently blocks
        // finalization when the runner completes.
        assert!(
            store
                .referenced_partial_artifacts(10, &live_lifetime)
                .unwrap()
                .is_empty(),
            "pending artifacts owned by the current lifetime must not be recovered"
        );

        // An attempt that has reached 'running' is equally protected.
        assert_eq!(
            store.mark_attempt_running(&first_run, 1, 5).unwrap(),
            StartDecision::Ready
        );
        assert!(
            store
                .referenced_partial_artifacts(10, &live_lifetime)
                .unwrap()
                .is_empty(),
            "running artifacts owned by the current lifetime must not be recovered"
        );

        // The same still-starting/running rows become recovery candidates for
        // a different lifetime, representing the daemon that replaced the one
        // that admitted them.
        let dead_lifetime = Uuid::from_u128(8_105).to_string();
        let candidates = store
            .referenced_partial_artifacts(10, &dead_lifetime)
            .unwrap();
        assert_eq!(candidates.len(), 2);
        assert!(
            candidates
                .iter()
                .any(|candidate| { candidate.run_id == first_run && candidate.state == "pending" })
        );
        assert!(
            candidates.iter().any(|candidate| {
                candidate.run_id == second_run && candidate.state == "pending"
            })
        );
    }

    #[test]
    fn metadata_retention_resumes_and_requires_output_prune_before_delete() {
        let temp = tempfile::tempdir().unwrap();
        let paths = StatePaths::new(temp.path().into());
        let store = Store::open(paths.clone(), "test", 1).unwrap();
        let job = Uuid::from_u128(9_001).to_string();
        let run = Uuid::from_u128(9_002).to_string();
        create(&store, &job, "metadata-prune");
        store.enqueue_manual("metadata-prune", &run, 2).unwrap();
        let lifetime = Uuid::from_u128(9_003).to_string();
        store.begin_lifetime(&lifetime, 3, "test").unwrap();
        store.admit(&lifetime, 3, 1).unwrap();
        store
            .finalize_output(
                &OutputRecord {
                    run_id: run.clone(),
                    attempt_number: 1,
                    relative_path: String::new(),
                    state: "finalized".into(),
                    retained_payload_bytes: 8,
                    physical_bytes: 16,
                    discarded_bytes: 0,
                    truncated: false,
                },
                4,
            )
            .unwrap();
        store
            .complete_attempt(&AttemptCompletion {
                run_id: run.clone(),
                attempt_number: 1,
                now_us: 5,
                duration_us: 2,
                state: "succeeded".into(),
                exit_code: Some(0),
                http_status: None,
                http_content_type: None,
                reason: "completed".into(),
                retry: None,
            })
            .unwrap();
        store.set_setting("run_retention_count", "0", 6).unwrap();
        let candidate = store.run_retention_candidates(6, 1).unwrap().remove(0);
        store.mark_run_retention_pending(&candidate, 6).unwrap();
        drop(store);

        let reopened = Store::open(paths, "test", 7).unwrap();
        assert_eq!(
            reopened.pending_run_retention(100).unwrap(),
            std::slice::from_ref(&candidate)
        );
        assert!(matches!(
            reopened.finish_run_retention(&candidate),
            Err(StoreError::Conflict(_))
        ));
        let output = reopened.output_retention_candidates(1).unwrap().remove(0);
        reopened.mark_output_prune_pending(&output, 8).unwrap();
        assert_eq!(reopened.pending_output_prunes(usize::MAX).unwrap().len(), 1);
        reopened.finish_output_prune(&output, 9).unwrap();
        reopened.finish_run_retention(&candidate).unwrap();
        reopened.finish_run_retention(&candidate).unwrap();
        assert!(matches!(reopened.run(&run), Err(StoreError::NotFound(_))));
        assert!(
            reopened
                .integrity_check()
                .unwrap()
                .iter()
                .all(|line| { line == "integrity: ok" || line == "foreign_key_violations: 0" })
        );
    }

    #[test]
    fn sqlite_writer_contention_returns_busy_and_recovers_after_release() {
        let temp = tempfile::tempdir().unwrap();
        let paths = StatePaths::new(temp.path().into());
        let first = Store::open(paths.clone(), "test", 1).unwrap();
        let second = Store::open(paths, "test", 1).unwrap();
        second
            .conn()
            .unwrap()
            .busy_timeout(std::time::Duration::ZERO)
            .unwrap();

        let first_connection = first.conn().unwrap();
        first_connection.execute_batch("BEGIN IMMEDIATE").unwrap();
        let error = second
            .set_setting("global_concurrency", "17", 2)
            .unwrap_err();
        let StoreError::Sqlite(rusqlite::Error::SqliteFailure(code, _)) = error else {
            panic!("expected SQLite busy failure, got {error}");
        };
        assert_eq!(code.code, rusqlite::ErrorCode::DatabaseBusy);
        assert_eq!(second.settings().unwrap().global_concurrency, 16);

        first_connection.execute_batch("ROLLBACK").unwrap();
        drop(first_connection);
        assert_eq!(
            second
                .set_setting("global_concurrency", "17", 3)
                .unwrap()
                .global_concurrency,
            17
        );
    }

    #[test]
    fn prune_pending_state_survives_reopen_and_known_candidate_can_finish() {
        let temp = tempfile::tempdir().unwrap();
        let paths = StatePaths::new(temp.path().into());
        let store = Store::open(paths.clone(), "test", 1).unwrap();
        let job = Uuid::from_u128(5_001).to_string();
        let run = Uuid::from_u128(5_002).to_string();
        create(&store, &job, "prune");
        store.enqueue_manual("prune", &run, 2).unwrap();
        let lifetime = Uuid::from_u128(5_003).to_string();
        store.begin_lifetime(&lifetime, 3, "test").unwrap();
        store.admit(&lifetime, 3, 1).unwrap();
        store
            .finalize_output(
                &OutputRecord {
                    run_id: run.clone(),
                    attempt_number: 1,
                    relative_path: String::new(),
                    state: "finalized".into(),
                    retained_payload_bytes: 30,
                    physical_bytes: 32,
                    discarded_bytes: 0,
                    truncated: false,
                },
                4,
            )
            .unwrap();
        store
            .complete_attempt(&AttemptCompletion {
                run_id: run.clone(),
                attempt_number: 1,
                now_us: 5,
                duration_us: 2,
                state: "succeeded".into(),
                exit_code: Some(0),
                http_status: None,
                http_content_type: None,
                reason: "completed".into(),
                retry: None,
            })
            .unwrap();
        let candidate = store.output_retention_candidates(1).unwrap().remove(0);
        store.mark_output_prune_pending(&candidate, 6).unwrap();
        assert!(store.output_retention_candidates(1).unwrap().is_empty());
        assert_eq!(store.retained_run_output_bytes(&run).unwrap(), 30);
        drop(store);

        let reopened = Store::open(paths, "test", 7).unwrap();
        let before_finish: (String, i64, Option<i64>) = reopened
            .conn()
            .unwrap()
            .query_row(
                "SELECT state,physical_bytes,prune_started_at_us FROM output_artifacts WHERE run_id=?1 AND attempt_number=1",
                [&run],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(before_finish, ("prune_pending".into(), 32, Some(6)));

        reopened.finish_output_prune(&candidate, 8).unwrap();
        let after_finish: (String, i64, Option<i64>) = reopened
            .conn()
            .unwrap()
            .query_row(
                "SELECT state,physical_bytes,pruned_at_us FROM output_artifacts WHERE run_id=?1 AND attempt_number=1",
                [&run],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(after_finish, ("pruned".into(), 0, Some(8)));
    }

    #[test]
    fn global_environment_is_validated_and_stored_as_canonical_json() {
        let (_temp, store) = store();

        store.set_environment("Z_TOKEN", Some("last"), 2).unwrap();
        store.set_environment("A_TOKEN", Some("first"), 3).unwrap();

        assert_eq!(
            store.settings().unwrap().environment,
            BTreeMap::from([
                ("A_TOKEN".into(), "first".into()),
                ("Z_TOKEN".into(), "last".into()),
            ])
        );
        let stored: String = store
            .conn()
            .unwrap()
            .query_row(
                "SELECT environment_json FROM settings WHERE singleton=1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stored, r#"{"A_TOKEN":"first","Z_TOKEN":"last"}"#);

        store.set_environment("A_TOKEN", None, 4).unwrap();
        assert_eq!(
            store.settings().unwrap().environment,
            BTreeMap::from([("Z_TOKEN".into(), "last".into())])
        );
        assert!(matches!(
            store.set_environment("LOCRON_RUN_ID", Some("spoof"), 5),
            Err(StoreError::Conflict(_))
        ));
        assert!(matches!(
            store.set_environment("BAD-NAME", Some("value"), 5),
            Err(StoreError::Conflict(_))
        ));
        assert!(matches!(
            store.set_environment("TOKEN", Some("bad\0value"), 5),
            Err(StoreError::Conflict(_))
        ));
    }

    #[test]
    fn runner_failures_atomically_mark_output_missing_and_never_retry() {
        for (execution_may_have_started, expected_state, expected_class) in [
            (false, "failed", "output_preparation_failed"),
            (true, "interrupted_unknown", "interrupted_unknown"),
        ] {
            let (_temp, store) = store();
            let job = Uuid::now_v7().to_string();
            let run = Uuid::now_v7().to_string();
            let lifetime = Uuid::now_v7().to_string();
            create(&store, &job, "runner-failure");
            store.enqueue_manual("runner-failure", &run, 2).unwrap();
            store.begin_lifetime(&lifetime, 3, "test").unwrap();
            let attempt = store.admit(&lifetime, 3, 1).unwrap().attempts.remove(0);
            assert_eq!(
                store
                    .mark_attempt_running(&run, attempt.attempt_number, 4)
                    .unwrap(),
                StartDecision::Ready
            );

            store
                .complete_runner_failure(
                    &run,
                    attempt.attempt_number,
                    10,
                    "output storage failed",
                    execution_may_have_started,
                )
                .unwrap();
            store
                .complete_runner_failure(
                    &run,
                    attempt.attempt_number,
                    10,
                    "output storage failed",
                    execution_may_have_started,
                )
                .unwrap();

            let facts: (String, String, String, String, i64) = store
                .conn()
                .unwrap()
                .query_row(
                    "SELECT a.state,a.result_class,r.state,o.state,count(ri.run_id)
                     FROM attempts a
                     JOIN runs r ON r.id=a.run_id
                     JOIN output_artifacts o
                       ON o.run_id=a.run_id AND o.attempt_number=a.attempt_number
                     LEFT JOIN retry_intents ri ON ri.run_id=a.run_id
                     WHERE a.run_id=?1 AND a.attempt_number=?2",
                    params![run, attempt.attempt_number],
                    |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                        ))
                    },
                )
                .unwrap();
            assert_eq!(
                facts,
                (
                    expected_state.into(),
                    expected_class.into(),
                    expected_state.into(),
                    "missing".into(),
                    0,
                )
            );
            assert!(
                store
                    .output_artifact_references(&run, &format!("{run}/1.partial"))
                    .unwrap()
            );
            assert!(
                !store
                    .output_artifact_references(&run, &format!("{run}/2.log"))
                    .unwrap()
            );
        }
    }

    #[test]
    fn resolved_executable_is_committed_at_the_cancellable_pre_spawn_boundary() {
        let (_temp, store) = store();
        let job = Uuid::from_u128(6_001).to_string();
        let run = Uuid::from_u128(6_002).to_string();
        create(&store, &job, "resolved");
        store.enqueue_manual("resolved", &run, 2).unwrap();
        let lifetime = Uuid::from_u128(6_003).to_string();
        store.begin_lifetime(&lifetime, 3, "test").unwrap();
        let attempt = store.admit(&lifetime, 3, 1).unwrap().attempts.remove(0);

        assert_eq!(
            store
                .mark_attempt_running_with_executable(
                    &run,
                    attempt.attempt_number,
                    4,
                    Some(Path::new("/usr/bin/true")),
                )
                .unwrap(),
            StartDecision::Ready
        );
        assert_eq!(
            store
                .attempt_resolved_executable(&run, attempt.attempt_number)
                .unwrap()
                .as_deref(),
            Some("/usr/bin/true")
        );

        assert!(matches!(
            store.mark_attempt_running_with_executable(
                &run,
                attempt.attempt_number,
                5,
                Some(Path::new("relative/bin")),
            ),
            Err(StoreError::Conflict(_))
        ));
    }
}
