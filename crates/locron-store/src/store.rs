use std::collections::{BTreeMap, VecDeque};
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use thiserror::Error;
#[cfg(test)]
use uuid::Uuid;

use crate::migration::migrate;
use crate::{DaemonLock, LockMetadata, StatePaths};

type AdmissionRow = (String, String, String, i64, String, Option<i64>);

pub type StoreResult<T> = Result<T, StoreError>;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("another locron daemon owns this state directory")]
    DaemonAlreadyRunning,
    #[error("database migration requires the running daemon to restart")]
    MigrationRequiresDaemonRestart,
    #[error("database schema {found} is newer than supported schema {supported}")]
    SchemaTooNew { found: i64, supported: i64 },
    #[error("database application id {0:#x} does not identify locron state")]
    NotLocronDatabase(i32),
    #[error("migration {version} checksum mismatch: expected {expected}, found {found}")]
    MigrationChecksumMismatch {
        version: i64,
        expected: String,
        found: String,
    },
    #[error("migration record {0} is missing")]
    MissingMigration(i64),
    #[error("migration raced another initializer")]
    MigrationConflict,
    #[error("state directory cannot be discovered")]
    StateDirectoryUnavailable,
    #[error("unsafe managed path: {0}")]
    UnsafePath(std::path::PathBuf),
    #[error("invalid identity: {0}")]
    InvalidIdentity(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("durable conflict: {0}")]
    Conflict(String),
}

#[derive(Clone, Debug)]
pub struct CreateJob {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub tags_json: String,
    pub enabled: bool,
    pub definition_json: String,
    pub now_us: i64,
    pub cursor_us: i64,
}

#[derive(Clone, Debug)]
pub struct UpdateJob {
    pub id: String,
    pub expected_revision: i64,
    pub name: String,
    pub description: Option<String>,
    pub tags_json: String,
    pub enabled: bool,
    pub definition_json: String,
    pub now_us: i64,
    pub cursor_us: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JobRecord {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub tags_json: String,
    pub enabled: bool,
    pub removed_at_us: Option<i64>,
    pub current_revision: i64,
    pub definition_json: String,
    pub cursor_us: i64,
    pub updated_at_us: i64,
    pub cursor_updated_at_us: i64,
    pub disabled_since_us: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct NewScheduledRun {
    pub id: String,
    pub job_id: String,
    pub revision: i64,
    pub trigger: String,
    pub nominal_us: i64,
    pub requested_at_us: i64,
    pub eligible_at_us: i64,
    pub snapshot_json: String,
}

#[derive(Clone, Debug)]
pub struct CursorUpdate {
    pub expected_revision: i64,
    pub expected_cursor_us: i64,
    pub new_cursor_us: i64,
    pub resolve_one_time: bool,
}

#[derive(Clone, Debug, Default)]
pub struct MaterializedRun {
    pub inserted: usize,
    pub duplicates: usize,
}

#[derive(Clone, Debug)]
pub struct ReconciliationSummary {
    pub kind: String,
    pub count: u64,
    pub first_nominal_us: i64,
    pub last_nominal_us: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EventRecord {
    pub id: i64,
    pub occurred_at_us: i64,
    pub kind: String,
    pub job_id: Option<String>,
    pub run_id: Option<String>,
    pub details_json: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunRecord {
    pub id: String,
    pub job_id: String,
    pub revision: i64,
    pub trigger: String,
    pub nominal_us: Option<i64>,
    pub requested_at_us: i64,
    pub eligible_at_us: i64,
    pub state: String,
    pub reason: Option<String>,
    pub snapshot_json: String,
    pub finished_at_us: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct AdmitAttempt {
    pub run_id: String,
    pub job_id: String,
    pub attempt_number: i64,
    pub trigger: String,
    pub nominal_us: Option<i64>,
    pub snapshot_json: String,
}
#[derive(Clone, Debug, Default)]
pub struct Admission {
    pub attempts: Vec<AdmitAttempt>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StartDecision {
    Ready,
    CancelledBeforeSpawn,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CancelOutcome {
    CancelledBeforeExecution,
    CancellationRequested,
    AcknowledgedUnconfirmed,
}

#[derive(Clone, Debug)]
pub struct RetryPlan {
    pub not_before_us: i64,
    pub classification: String,
}

#[derive(Clone, Debug)]
pub struct AttemptCompletion {
    pub run_id: String,
    pub attempt_number: i64,
    pub now_us: i64,
    pub duration_us: i64,
    pub state: String,
    pub exit_code: Option<i32>,
    pub http_status: Option<u16>,
    pub reason: String,
    pub retry: Option<RetryPlan>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OutputRecord {
    pub run_id: String,
    pub attempt_number: i64,
    pub relative_path: String,
    pub state: String,
    pub retained_payload_bytes: i64,
    pub physical_bytes: i64,
    pub discarded_bytes: i64,
    pub truncated: bool,
}

#[derive(Clone, Debug)]
pub struct RetentionCandidate {
    pub run_id: String,
    pub attempt_number: i64,
    pub relative_path: String,
    pub physical_bytes: i64,
    pub finalized_at_us: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SettingsRecord {
    pub global_concurrency: i64,
    pub execution_path: String,
    pub run_retention_count: i64,
    pub run_retention_age_us: Option<i64>,
    pub output_limit_bytes: i64,
    pub per_run_output_limit_bytes: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JobIdentity {
    pub id: String,
    pub name: String,
    pub removed: bool,
}

#[derive(Clone, Debug)]
pub struct ImportResolution {
    pub source_id: String,
    pub source_name: String,
    pub expected_id_destination: Option<String>,
    pub expected_name_destination: Option<String>,
}

#[derive(Clone, Debug)]
pub enum ImportJob {
    Create {
        job: CreateJob,
        resolution: ImportResolution,
    },
    Update {
        job: UpdateJob,
        resolution: ImportResolution,
    },
    Verify {
        job: UpdateJob,
        resolution: ImportResolution,
    },
}

#[derive(Clone, Debug)]
pub struct ImportBatch {
    pub settings: SettingsRecord,
    pub jobs: Vec<ImportJob>,
    pub now_us: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ImportSummary {
    pub created: usize,
    pub updated: usize,
}

/// Thread-safe serialized SQLite store.
pub struct Store {
    paths: StatePaths,
    connection: Mutex<Connection>,
}

impl Store {
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

    #[must_use]
    pub fn paths(&self) -> &StatePaths {
        &self.paths
    }
    fn conn(&self) -> StoreResult<MutexGuard<'_, Connection>> {
        self.connection
            .lock()
            .map_err(|_| StoreError::Conflict("store mutex poisoned".into()))
    }

    pub fn acquire_daemon_lock(&self, metadata: &LockMetadata) -> StoreResult<DaemonLock> {
        DaemonLock::acquire(&self.paths.daemon_lock, metadata)
    }

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

    pub fn job(&self, reference: &str) -> StoreResult<JobRecord> {
        let conn = self.conn()?;
        conn.query_row(
            "SELECT j.id,j.name,j.description,j.tags_json,j.enabled,j.removed_at_us,j.current_revision,r.definition_json,c.cursor_us,j.updated_at_us,c.updated_at_us,c.disabled_since_us FROM jobs j JOIN job_revisions r ON r.job_id=j.id AND r.revision=j.current_revision JOIN schedule_cursors c ON c.job_id=j.id AND c.revision=j.current_revision WHERE (j.id=?1 OR j.name=?1) AND j.removed_at_us IS NULL",
            [reference], map_job,
        ).optional()?.ok_or_else(|| StoreError::NotFound(reference.into()))
    }

    pub fn list_jobs(&self, all: bool) -> StoreResult<Vec<JobRecord>> {
        let conn = self.conn()?;
        let mut statement = conn.prepare("SELECT j.id,j.name,j.description,j.tags_json,j.enabled,j.removed_at_us,j.current_revision,r.definition_json,c.cursor_us,j.updated_at_us,c.updated_at_us,c.disabled_since_us FROM jobs j JOIN job_revisions r ON r.job_id=j.id AND r.revision=j.current_revision JOIN schedule_cursors c ON c.job_id=j.id AND c.revision=j.current_revision WHERE j.removed_at_us IS NULL AND (?1 OR j.enabled=1) ORDER BY j.name COLLATE BINARY")?;
        statement
            .query_map([all], map_job)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

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
        tx.execute(
            "UPDATE settings SET global_concurrency=?1,execution_path=?2,run_retention_count=?3,run_retention_age_us=?4,output_limit_bytes=?5,per_run_output_limit_bytes=?6,updated_at_us=?7 WHERE singleton=1",
            params![batch.settings.global_concurrency,batch.settings.execution_path,batch.settings.run_retention_count,batch.settings.run_retention_age_us,batch.settings.output_limit_bytes,batch.settings.per_run_output_limit_bytes,batch.now_us],
        )?;
        event(&tx, batch.now_us, "import_applied", None, None, "{}")?;
        tx.commit()?;
        Ok(summary)
    }

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

    pub fn remove_job(&self, reference: &str, now_us: i64) -> StoreResult<()> {
        let job = self.job(reference)?;
        let conn = self.conn()?;
        conn.execute(
            "UPDATE jobs SET enabled=0,removed_at_us=?2,updated_at_us=?2 WHERE id=?1",
            params![job.id, now_us],
        )?;
        Ok(())
    }

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

    pub fn materialize(
        &self,
        job_id: &str,
        cursor: CursorUpdate,
        runs: &[NewScheduledRun],
        now_us: i64,
    ) -> StoreResult<MaterializedRun> {
        self.materialize_with_summaries(job_id, cursor, runs, &[], now_us)
    }

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

    pub fn run(&self, id: &str) -> StoreResult<RunRecord> {
        let conn = self.conn()?;
        conn.query_row("SELECT id,job_id,revision,trigger,nominal_us,requested_at_us,eligible_at_us,state,reason,snapshot_json,finished_at_us FROM runs WHERE id=?1", [id], map_run).optional()?.ok_or_else(|| StoreError::NotFound(id.into()))
    }

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

    pub fn cancel(&self, id: &str, now_us: i64) -> StoreResult<CancelOutcome> {
        self.cancel_with_acknowledgement(id, now_us, false)
    }

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

    pub fn end_lifetime(&self, id: &str, now_us: i64) -> StoreResult<()> {
        self.conn()?.execute("UPDATE scheduler_lifetimes SET ended_at_us=?2,heartbeat_at_us=?2,exit_class='clean' WHERE id=?1 AND ended_at_us IS NULL", params![id,now_us])?;
        Ok(())
    }

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

    pub fn mark_attempt_running(
        &self,
        run_id: &str,
        attempt_number: i64,
        now_us: i64,
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
        let attempt = tx.execute(
            "UPDATE attempts SET state='running',running_at_us=?3 WHERE run_id=?1 AND attempt_number=?2 AND state='starting'",
            params![run_id, attempt_number, now_us],
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
        let changed = tx.execute("UPDATE attempts SET state=?3,finished_at_us=?4,duration_us=?5,exit_code=?6,http_status=?7,result_class=?3,error_message=?8 WHERE run_id=?1 AND attempt_number=?2 AND state IN ('starting','running')", params![completion.run_id,completion.attempt_number,completion.state,completion.now_us,completion.duration_us,completion.exit_code,completion.http_status,completion.reason])?;
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

    pub fn finalize_output(&self, output: &OutputRecord, now_us: i64) -> StoreResult<()> {
        let relative_path = format!("{}/{}.log", output.run_id, output.attempt_number);
        let changed = self.conn()?.execute(
            "UPDATE output_artifacts SET relative_path=?3,state='finalized',retained_payload_bytes=?4,physical_bytes=?5,discarded_bytes=?6,truncated=?7,truncated_at_us=CASE WHEN ?7 THEN ?8 ELSE NULL END,finalized_at_us=?8 WHERE run_id=?1 AND attempt_number=?2",
            params![output.run_id,output.attempt_number,relative_path,output.retained_payload_bytes,output.physical_bytes,output.discarded_bytes,output.truncated,now_us],
        )?;
        if changed != 1 {
            return Err(StoreError::Conflict("output artifact is missing".into()));
        }
        Ok(())
    }

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

    pub fn settings(&self) -> StoreResult<SettingsRecord> {
        self.conn()?.query_row(
            "SELECT global_concurrency,execution_path,run_retention_count,run_retention_age_us,output_limit_bytes,per_run_output_limit_bytes FROM settings WHERE singleton=1",
            [],
            |row| Ok(SettingsRecord { global_concurrency:row.get(0)?,execution_path:row.get(1)?,run_retention_count:row.get(2)?,run_retention_age_us:row.get(3)?,output_limit_bytes:row.get(4)?,per_run_output_limit_bytes:row.get(5)? }),
        ).map_err(Into::into)
    }

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
            "run_retention_count" | "output_limit_bytes" | "per_run_output_limit_bytes" => {
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

    pub fn output_retention_candidates(
        &self,
        limit: usize,
    ) -> StoreResult<Vec<RetentionCandidate>> {
        let conn = self.conn()?;
        let mut statement = conn.prepare(
            "SELECT o.run_id,o.attempt_number,o.relative_path,o.physical_bytes,o.finalized_at_us FROM output_artifacts o JOIN runs r ON r.id=o.run_id WHERE o.state='finalized' AND r.state IN ('succeeded','failed','timed_out','cancelled','skipped_overlap','skipped_concurrency','interrupted_unknown') ORDER BY o.finalized_at_us,o.run_id,o.attempt_number LIMIT ?1",
        )?;
        statement
            .query_map([limit.min(1000) as i64], |row| {
                Ok(RetentionCandidate {
                    run_id: row.get(0)?,
                    attempt_number: row.get(1)?,
                    relative_path: row.get(2)?,
                    physical_bytes: row.get(3)?,
                    finalized_at_us: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn retained_output_bytes(&self) -> StoreResult<i64> {
        self.conn()?.query_row(
            "SELECT COALESCE(sum(physical_bytes),0) FROM output_artifacts WHERE state='finalized'",
            [],
            |row| row.get(0),
        ).map_err(Into::into)
    }

    pub fn retained_run_output_bytes(&self, run_id: &str) -> StoreResult<i64> {
        self.conn()?.query_row(
            "SELECT COALESCE(sum(retained_payload_bytes),0) FROM output_artifacts WHERE run_id=?1 AND state IN ('active','finalized','prune_pending')",
            [run_id],
            |row| row.get(0),
        ).map_err(Into::into)
    }

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

    pub fn finish_output_prune(
        &self,
        candidate: &RetentionCandidate,
        now_us: i64,
    ) -> StoreResult<()> {
        self.conn()?.execute("UPDATE output_artifacts SET state='pruned',physical_bytes=0,pruned_at_us=?3 WHERE run_id=?1 AND attempt_number=?2 AND state='prune_pending'",params![candidate.run_id,candidate.attempt_number,now_us])?;
        Ok(())
    }
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
        String,
        Option<String>,
        i64,
        Option<i64>,
    );
    let current: Option<CompletionFacts> = tx
        .query_row(
            "SELECT a.state,a.finished_at_us,a.duration_us,a.exit_code,a.http_status,a.error_message,r.state,r.reason,r.eligible_at_us,r.finished_at_us FROM attempts a JOIN runs r ON r.id=a.run_id WHERE a.run_id=?1 AND a.attempt_number=?2",
            params![completion.run_id, completion.attempt_number],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?, row.get(7)?, row.get(8)?, row.get(9)?)),
        )
        .optional()?;
    let Some((
        attempt_state,
        finished,
        duration,
        exit_code,
        http_status,
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
            http_status: None,
            reason: "known result".into(),
            retry: None,
        };
        store.complete_attempt(&completion).unwrap();
        store.complete_attempt(&completion).unwrap();
        let mut mismatched = completion;
        mismatched.reason = "different result".into();
        assert!(matches!(
            store.complete_attempt(&mismatched),
            Err(StoreError::Conflict(_))
        ));
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
                    reason: "test completion".into(),
                    retry: None,
                })
                .unwrap();
        }
        store.set_setting("global_concurrency", "3", 9).unwrap();
        assert_eq!(store.admit(&lifetime, 9, 63).unwrap().attempts.len(), 1);
    }
}
