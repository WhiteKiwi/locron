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
    pub expected_cursor_us: i64,
    pub new_cursor_us: i64,
    pub resolve_one_time: bool,
}

#[derive(Clone, Debug, Default)]
pub struct MaterializedRun {
    pub inserted: usize,
    pub duplicates: usize,
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SettingsRecord {
    pub global_concurrency: i64,
    pub execution_path: String,
    pub run_retention_count: i64,
    pub run_retention_age_us: Option<i64>,
    pub output_limit_bytes: i64,
    pub per_run_output_limit_bytes: i64,
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
        tx.execute("INSERT INTO jobs(id,name,description,tags_json,enabled,created_at_us,updated_at_us,current_revision) VALUES(?1,?2,?3,?4,?5,?6,?6,1)", params![job.id, job.name, job.description, job.tags_json, job.enabled, job.now_us])?;
        tx.execute("INSERT INTO job_revisions(job_id,revision,definition_json,created_at_us,created_by) VALUES(?1,1,?2,?3,'add')", params![job.id, job.definition_json, job.now_us])?;
        tx.execute("INSERT INTO schedule_cursors(job_id,revision,cursor_us,interval_anchor_us,updated_at_us) VALUES(?1,1,?2,NULL,?3)", params![job.id, job.cursor_us, job.now_us])?;
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
        let revision = current + 1;
        tx.execute("UPDATE jobs SET name=?2,description=?3,tags_json=?4,enabled=?5,updated_at_us=?6,current_revision=?7 WHERE id=?1", params![job.id,job.name,job.description,job.tags_json,job.enabled,job.now_us,revision])?;
        tx.execute(
            "INSERT INTO job_revisions VALUES(?1,?2,?3,?4,'update')",
            params![job.id, revision, job.definition_json, job.now_us],
        )?;
        tx.execute("INSERT INTO schedule_cursors(job_id,revision,cursor_us,interval_anchor_us,updated_at_us) VALUES(?1,?2,?3,NULL,?4)", params![job.id,revision,job.cursor_us,job.now_us])?;
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
            "SELECT j.id,j.name,j.description,j.tags_json,j.enabled,j.removed_at_us,j.current_revision,r.definition_json,c.cursor_us FROM jobs j JOIN job_revisions r ON r.job_id=j.id AND r.revision=j.current_revision JOIN schedule_cursors c ON c.job_id=j.id AND c.revision=j.current_revision WHERE (j.id=?1 OR j.name=?1) AND j.removed_at_us IS NULL",
            [reference], map_job,
        ).optional()?.ok_or_else(|| StoreError::NotFound(reference.into()))
    }

    pub fn list_jobs(&self, all: bool) -> StoreResult<Vec<JobRecord>> {
        let conn = self.conn()?;
        let mut statement = conn.prepare("SELECT j.id,j.name,j.description,j.tags_json,j.enabled,j.removed_at_us,j.current_revision,r.definition_json,c.cursor_us FROM jobs j JOIN job_revisions r ON r.job_id=j.id AND r.revision=j.current_revision JOIN schedule_cursors c ON c.job_id=j.id AND c.revision=j.current_revision WHERE j.removed_at_us IS NULL AND (?1 OR j.enabled=1) ORDER BY j.name COLLATE BINARY")?;
        statement
            .query_map([all], map_job)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn set_enabled(
        &self,
        reference: &str,
        enabled: bool,
        now_us: i64,
    ) -> StoreResult<JobRecord> {
        let job = self.job(reference)?;
        let conn = self.conn()?;
        conn.execute(
            "UPDATE jobs SET enabled=?2,updated_at_us=?3 WHERE id=?1",
            params![job.id, enabled, now_us],
        )?;
        drop(conn);
        self.job(&job.id)
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
        let mut state = "queued";
        let mut reason: Option<&str> = None;
        let mut replacement_candidate = false;
        if active_count > 0 {
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
                    tx.execute(
                        "UPDATE runs SET state=CASE WHEN state IN ('queued','retry_wait') THEN 'cancelled' ELSE state END,reason=CASE WHEN state IN ('queued','retry_wait') THEN 'superseded by newer replacement' ELSE reason END,finished_at_us=CASE WHEN state IN ('queued','retry_wait') THEN ?2 ELSE finished_at_us END,cancellation_requested_at_us=CASE WHEN state IN ('starting','running') THEN ?2 ELSE cancellation_requested_at_us END,cancellation_reason=CASE WHEN state IN ('starting','running') THEN 'replacement' ELSE cancellation_reason END,replacement_candidate=0 WHERE job_id=?1 AND state IN ('queued','starting','running','retry_wait')",
                        params![job.id, now_us],
                    )?;
                    replacement_candidate = true;
                }
                _ => {}
            }
        }
        let finished_at =
            matches!(state, "skipped_overlap" | "skipped_concurrency").then_some(now_us);
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
        let mut conn = self.conn()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = tx.execute("UPDATE schedule_cursors SET cursor_us=?3,one_time_resolved=CASE WHEN ?5 THEN 1 ELSE one_time_resolved END,updated_at_us=?4 WHERE job_id=?1 AND revision=(SELECT current_revision FROM jobs WHERE id=?1) AND cursor_us=?2", params![job_id,cursor.expected_cursor_us,cursor.new_cursor_us,now_us,cursor.resolve_one_time])?;
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
            let sequence = next_queue_sequence(&tx)?;
            let policy = snapshot_admission_policy(&run.snapshot_json)?;
            let active_count: i64 = tx.query_row(
                "SELECT count(*) FROM runs WHERE job_id=?1 AND state IN ('queued','starting','running','retry_wait')",
                [&run.job_id],
                |row| row.get(0),
            )?;
            let mut state = "queued";
            let mut reason: Option<&str> = None;
            let mut replacement_candidate = false;
            if run.trigger != "catch_up" && active_count > 0 {
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
                        tx.execute(
                            "UPDATE runs SET state=CASE WHEN state IN ('queued','retry_wait') THEN 'cancelled' ELSE state END,reason=CASE WHEN state IN ('queued','retry_wait') THEN 'superseded by newer replacement' ELSE reason END,finished_at_us=CASE WHEN state IN ('queued','retry_wait') THEN ?2 ELSE finished_at_us END,cancellation_requested_at_us=CASE WHEN state IN ('starting','running') THEN ?2 ELSE cancellation_requested_at_us END,cancellation_reason=CASE WHEN state IN ('starting','running') THEN 'replacement' ELSE cancellation_reason END,replacement_candidate=0 WHERE job_id=?1 AND state IN ('queued','starting','running','retry_wait')",
                            params![run.job_id, now_us],
                        )?;
                        replacement_candidate = true;
                    }
                    _ => {}
                }
            }
            let finished_at =
                matches!(state, "skipped_overlap" | "skipped_concurrency").then_some(now_us);
            let changed = tx.execute("INSERT OR IGNORE INTO runs(id,job_id,revision,trigger,nominal_us,requested_at_us,eligible_at_us,queue_sequence,snapshot_json,state,reason,replacement_candidate,finished_at_us) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)", params![run.id,run.job_id,run.revision,run.trigger,run.nominal_us,run.requested_at_us,run.eligible_at_us,sequence,run.snapshot_json,state,reason,replacement_candidate,finished_at])?;
            if changed == 1 {
                result.inserted += 1
            } else {
                result.duplicates += 1
            }
        }
        tx.commit()?;
        Ok(result)
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

    pub fn cancel(&self, id: &str, now_us: i64) -> StoreResult<()> {
        let mut conn = self.conn()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current: Option<(String, String, Option<i64>)> = tx
            .query_row(
                "SELECT state,job_id,cancellation_requested_at_us FROM runs WHERE id=?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let Some((state, job_id, cancellation_requested_at_us)) = current else {
            return Err(StoreError::NotFound(id.into()));
        };
        match state.as_str() {
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
            }
            terminal => {
                return Err(StoreError::Conflict(format!(
                    "run {id} is already terminal ({terminal})"
                )));
            }
        }
        tx.commit()?;
        Ok(())
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
        let stale = tx.execute("UPDATE attempts SET state='interrupted_unknown',finished_at_us=?1,error_message='scheduler lifetime ended without a durable result' WHERE state IN ('starting','running')", [now_us])?;
        tx.execute("UPDATE runs SET state='interrupted_unknown',finished_at_us=?1,reason='scheduler lifetime ended without a durable result' WHERE state IN ('starting','running')", [now_us])?;
        tx.execute("UPDATE scheduler_lifetimes SET ended_at_us=?1,exit_class='stale' WHERE ended_at_us IS NULL", [now_us])?;
        tx.execute("INSERT INTO scheduler_lifetimes(id,pid,binary_version,started_at_us,heartbeat_at_us) VALUES(?1,?2,?3,?4,?4)", params![id,std::process::id(),binary_version,now_us])?;
        tx.commit()?;
        Ok(stale)
    }

    pub fn end_lifetime(&self, id: &str, now_us: i64) -> StoreResult<()> {
        self.conn()?.execute("UPDATE scheduler_lifetimes SET ended_at_us=?2,heartbeat_at_us=?2,exit_class='clean' WHERE id=?1 AND ended_at_us IS NULL", params![id,now_us])?;
        Ok(())
    }

    pub fn admit(&self, lifetime_id: &str, now_us: i64, capacity: usize) -> StoreResult<Admission> {
        if capacity == 0 {
            return Ok(Admission::default());
        }
        let mut conn = self.conn()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut statement = tx.prepare("SELECT r.id,r.job_id,r.snapshot_json,COALESCE((SELECT MAX(attempt_number) FROM attempts a WHERE a.run_id=r.id),0)+1,r.trigger,r.nominal_us FROM runs r WHERE r.state IN ('queued','retry_wait') AND r.eligible_at_us<=?1 AND r.cancellation_requested_at_us IS NULL AND (r.replacement_candidate=0 OR NOT EXISTS(SELECT 1 FROM runs prior WHERE prior.job_id=r.job_id AND prior.id<>r.id AND prior.state IN ('starting','running','retry_wait'))) AND (r.trigger<>'catch_up' OR NOT EXISTS(SELECT 1 FROM runs earlier WHERE earlier.job_id=r.job_id AND earlier.queue_sequence<r.queue_sequence AND earlier.state IN ('queued','starting','running','retry_wait'))) ORDER BY r.eligible_at_us,r.queue_sequence LIMIT ?2")?;
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
        let mut selected = Vec::new();
        while selected.len() < capacity {
            let mut progressed = false;
            for job in &jobs {
                if let Some(row) = grouped.get_mut(job).and_then(VecDeque::pop_front) {
                    selected.push(row);
                    progressed = true;
                    if selected.len() == capacity {
                        break;
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
                "UPDATE runs SET state='running' WHERE id=?1 AND state IN ('queued','retry_wait')",
                [&run_id],
            )?;
            tx.execute("INSERT INTO attempts(run_id,attempt_number,lifetime_id,state,started_at_us,running_at_us) VALUES(?1,?2,?3,'running',?4,?4)", params![run_id,number,lifetime_id,now_us])?;
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

    pub fn complete_attempt(&self, completion: &AttemptCompletion) -> StoreResult<()> {
        let mut conn = self.conn()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute("UPDATE attempts SET state=?3,finished_at_us=?4,duration_us=?5,exit_code=?6,http_status=?7,result_class=?3,error_message=?8 WHERE run_id=?1 AND attempt_number=?2", params![completion.run_id,completion.attempt_number,completion.state,completion.now_us,completion.duration_us,completion.exit_code,completion.http_status,completion.reason])?;
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
    })
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
        assert_eq!(store.run(&first).unwrap().state, "cancelled");
        assert_eq!(store.run(&second).unwrap().state, "queued");
    }
}
