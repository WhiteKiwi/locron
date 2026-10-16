//! One admission order and deadline for every writable Store boundary.

use std::cell::{Cell, RefCell};
use std::fs::File;
use std::sync::{MutexGuard, TryLockError};
use std::time::{Duration, Instant};

use locron_core::PersistenceDiagnostic;
use locron_core::filesystem::{GuardedFile, open_private_or_create};
use rusqlite::{Connection, Transaction, TransactionBehavior};

use super::{Store, StoreError, StoreResult};

const ALLOWANCE: Duration = Duration::from_secs(5);
const BACKOFF: Duration = Duration::from_millis(10);
thread_local! {
    static BACKGROUND: Cell<Option<(usize, Instant)>> = const { Cell::new(None) };
}
#[cfg(test)]
thread_local! {
    static BUSY_ACK: RefCell<Option<std::sync::mpsc::Sender<()>>> = const { RefCell::new(None) };
}

/// Original adapter failure retained for explicit inspection, never formatted in a chain.
/// In particular, SQLite messages may contain SQL or configured values.
pub struct ObservedFailure {
    pub(super) diagnostic: Box<PersistenceDiagnostic>,
    cause: Box<StoreError>,
}
impl std::fmt::Display for ObservedFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.diagnostic.fmt(f)
    }
}
impl std::fmt::Debug for ObservedFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.diagnostic.fmt(f)
    }
}
impl std::error::Error for ObservedFailure {}

impl StoreError {
    /// Safe scalar persistence observations, suitable for every transport/verbosity.
    #[must_use]
    pub fn diagnostic(&self) -> Option<&PersistenceDiagnostic> {
        match self {
            Self::Observed(failure) => Some(&failure.diagnostic),
            _ => None,
        }
    }

    /// Original failure for explicit adapter inspection; never render this value.
    #[must_use]
    pub fn original_error(&self) -> &Self {
        match self {
            Self::Observed(failure) => failure.cause.original_error(),
            _ => self,
        }
    }
}

pub(crate) struct Operation {
    started: Instant,
    deadline: Instant,
    background: bool,
    facts: RefCell<PersistenceDiagnostic>,
    transaction_started: Cell<Option<Instant>>,
    commit_failed: Cell<bool>,
    committed_steps: Cell<u32>,
}

impl Operation {
    pub(crate) fn new(name: &'static str, background: Option<Instant>) -> Self {
        let started = Instant::now();
        Self {
            started,
            deadline: background.unwrap_or(started + ALLOWANCE),
            background: background.is_some(),
            facts: RefCell::new(PersistenceDiagnostic {
                operation: name,
                stage: "intent",
                kind: "success",
                elapsed_us: 0,
                admission_wait_us: 0,
                connection_wait_us: 0,
                sqlite_wait_us: 0,
                transaction_us: 0,
                commit_us: 0,
                retries: 0,
                sqlite_primary_code: None,
                sqlite_extended_code: None,
                os_error_code: None,
                retryable: false,
                durable_outcome: "not_committed",
                lock_owner: "unknown",
            }),
            transaction_started: Cell::new(None),
            commit_failed: Cell::new(false),
            committed_steps: Cell::new(0),
        }
    }

    #[cfg(windows)]
    pub(crate) fn deadline(&self) -> Instant {
        self.deadline
    }
    pub(crate) fn stage(&self, stage: &'static str) {
        self.facts.borrow_mut().stage = stage;
    }

    fn pause(&self) -> StoreResult<()> {
        if self.background {
            return Err(StoreError::MaintenanceDeferred);
        }
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(StoreError::AdmissionTimeout);
        }
        std::thread::sleep(BACKOFF.min(remaining));
        self.facts.borrow_mut().retries += 1;
        self.check_deadline()?;
        Ok(())
    }

    fn check_deadline(&self) -> StoreResult<()> {
        if Instant::now() >= self.deadline {
            return Err(if self.background {
                StoreError::MaintenanceDeferred
            } else {
                StoreError::AdmissionTimeout
            });
        }
        Ok(())
    }

    pub(crate) fn retry_begin<'a>(
        &self,
        connection: &'a Connection,
    ) -> StoreResult<Transaction<'a>> {
        self.stage("begin");
        let started = Instant::now();
        loop {
            self.check_deadline()?;
            match Transaction::new_unchecked(connection, TransactionBehavior::Immediate) {
                Ok(tx) => {
                    self.facts.borrow_mut().sqlite_wait_us += micros(started.elapsed());
                    self.transaction_started.set(Some(Instant::now()));
                    self.stage("execute");
                    return Ok(tx);
                }
                Err(error) if is_busy(&error) => {
                    #[cfg(test)]
                    BUSY_ACK.with(|slot| {
                        if let Some(sender) = slot.borrow_mut().take() {
                            let _ = sender.send(());
                        }
                    });
                    self.sqlite_codes(&error);
                    if self.background {
                        self.facts.borrow_mut().sqlite_wait_us += micros(started.elapsed());
                        return Err(StoreError::MaintenanceDeferred);
                    }
                    if self.pause().is_err() {
                        self.facts.borrow_mut().sqlite_wait_us += micros(started.elapsed());
                        return Err(StoreError::Sqlite(error));
                    }
                }
                Err(error) => {
                    self.facts.borrow_mut().sqlite_wait_us += micros(started.elapsed());
                    return Err(StoreError::Sqlite(error));
                }
            }
        }
    }

    pub(crate) fn begin<'a>(&self, connection: &'a mut Connection) -> StoreResult<Transaction<'a>> {
        self.retry_begin(connection)
    }

    pub(crate) fn commit(&self, tx: Transaction<'_>) -> StoreResult<()> {
        self.stage("commit");
        let started = Instant::now();
        let result = tx.commit();
        self.facts.borrow_mut().commit_us += micros(started.elapsed());
        self.end_transaction();
        self.commit_failed.set(result.is_err());
        if result.is_ok() {
            self.committed_steps
                .set(self.committed_steps.get().saturating_add(1));
            self.facts.borrow_mut().durable_outcome = "committed";
        }
        result.map_err(StoreError::from)
    }

    fn end_transaction(&self) {
        if let Some(started) = self.transaction_started.take() {
            self.facts.borrow_mut().transaction_us += micros(started.elapsed());
        }
    }

    pub(crate) fn sqlite_codes(&self, error: &rusqlite::Error) {
        if let Some(code) = error.sqlite_error() {
            let mut facts = self.facts.borrow_mut();
            facts.sqlite_primary_code = Some(code.extended_code & 0xff);
            facts.sqlite_extended_code = Some(code.extended_code);
        }
    }

    #[cfg(windows)]
    pub(crate) fn note_busy(&self, error: &rusqlite::Error) {
        self.sqlite_codes(error);
        self.facts.borrow_mut().retries += 1;
    }

    #[cfg(windows)]
    pub(crate) fn sqlite_stage<T>(
        &self,
        action: impl FnOnce() -> StoreResult<T>,
    ) -> StoreResult<T> {
        let started = Instant::now();
        let result = action();
        self.facts.borrow_mut().sqlite_wait_us += micros(started.elapsed());
        result
    }

    #[cfg(not(windows))]
    pub(crate) fn retry_sqlite<T>(
        &self,
        stage: &'static str,
        mut action: impl FnMut() -> rusqlite::Result<T>,
    ) -> StoreResult<T> {
        self.stage(stage);
        let started = Instant::now();
        let result = loop {
            self.check_deadline()?;
            match action() {
                Ok(value) => break Ok(value),
                Err(error) if is_busy(&error) => {
                    self.sqlite_codes(&error);
                    if self.background {
                        break Err(StoreError::MaintenanceDeferred);
                    }
                    if self.pause().is_err() {
                        break Err(StoreError::Sqlite(error));
                    }
                }
                Err(error) => break Err(StoreError::Sqlite(error)),
            }
        };
        self.facts.borrow_mut().sqlite_wait_us += micros(started.elapsed());
        result
    }

    pub(crate) fn finish<T>(
        &self,
        result: &StoreResult<T>,
        autocommit: bool,
    ) -> PersistenceDiagnostic {
        self.end_transaction();
        let mut facts = self.facts.borrow_mut();
        facts.elapsed_us = micros(self.started.elapsed());
        if let Err(error) = result {
            // A recovered BEGIN BUSY is not the cause of a later JSON/I/O/readback
            // failure. Project codes from the returned error, never an earlier retry.
            facts.sqlite_primary_code = None;
            facts.sqlite_extended_code = None;
            facts.os_error_code = None;
            if self.commit_failed.get()
                || !autocommit
                || (facts.operation == "open" && self.committed_steps.get() > 0)
            {
                facts.durable_outcome = "unknown";
            } else if facts.durable_outcome != "committed" {
                facts.durable_outcome = "not_committed";
            }
            match error.original_error() {
                StoreError::Sqlite(error) => {
                    if let Some(code) = error.sqlite_error() {
                        facts.sqlite_primary_code = Some(code.extended_code & 0xff);
                        facts.sqlite_extended_code = Some(code.extended_code);
                    }
                    facts.kind = match error.sqlite_error_code() {
                        Some(rusqlite::ErrorCode::DatabaseBusy) => "busy",
                        Some(rusqlite::ErrorCode::DatabaseLocked) => "locked",
                        _ => "sqlite",
                    };
                }
                StoreError::Io(error) => {
                    facts.kind = "io";
                    facts.os_error_code = error.raw_os_error();
                }
                StoreError::Json(_) => facts.kind = "serialization",
                StoreError::AdmissionTimeout => facts.kind = "admission_timeout",
                StoreError::MaintenanceDeferred => facts.kind = "deferred",
                _ => facts.kind = "state",
            }
            facts.retryable = matches!(facts.kind, "busy" | "admission_timeout")
                && facts.durable_outcome == "not_committed";
        }
        facts.clone()
    }

    pub(crate) fn observed<T>(&self, result: StoreResult<T>, autocommit: bool) -> StoreResult<T> {
        let diagnostic = self.finish(&result, autocommit);
        result.map_err(|error| observe(error, diagnostic))
    }
}

fn observe(error: StoreError, diagnostic: PersistenceDiagnostic) -> StoreError {
    // Preserve established semantic conflict/not-found/migration categories.
    match error {
        StoreError::Sqlite(_)
        | StoreError::Io(_)
        | StoreError::Json(_)
        | StoreError::AdmissionTimeout => StoreError::Observed(ObservedFailure {
            diagnostic: Box::new(diagnostic),
            cause: Box::new(error),
        }),
        _ => error,
    }
}

pub(crate) fn is_busy(error: &rusqlite::Error) -> bool {
    error.sqlite_error_code() == Some(rusqlite::ErrorCode::DatabaseBusy)
}
fn micros(duration: Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
}

struct LockedFile(GuardedFile);
impl Drop for LockedFile {
    fn drop(&mut self) {
        let _ = File::unlock(&self.0);
    }
}

pub(crate) struct Admission {
    // Writer is dropped before the independent intent guard, including on errors.
    _writer: LockedFile,
    intent: Option<LockedFile>,
}
impl Admission {
    pub(crate) fn acquire(store: &Store, operation: &Operation) -> StoreResult<Self> {
        Self::for_paths(&store.paths, operation)
    }
    pub(crate) fn for_paths(paths: &crate::StatePaths, operation: &Operation) -> StoreResult<Self> {
        let started = Instant::now();
        let result = (|| {
            operation.stage("intent");
            operation.check_deadline()?;
            let intent = open_private_or_create(&paths.root.join("writer-intent.lock"))?;
            lock(&intent, !operation.background, operation)?;
            let intent = LockedFile(intent);
            operation.stage("writer_admission");
            let writer = open_private_or_create(&paths.root.join("writer.lock"))?;
            lock(&writer, false, operation)?;
            Ok(Self {
                _writer: LockedFile(writer),
                intent: Some(intent),
            })
        })();
        operation.facts.borrow_mut().admission_wait_us += micros(started.elapsed());
        result
    }
    pub(crate) fn admitted(&mut self) {
        self.intent.take();
    }
}
fn lock(file: &File, shared: bool, operation: &Operation) -> StoreResult<()> {
    loop {
        operation.check_deadline()?;
        let result = if shared {
            file.try_lock_shared()
        } else {
            file.try_lock()
        };
        match result {
            Ok(()) => return Ok(()),
            Err(std::fs::TryLockError::WouldBlock) => operation.pause()?,
            Err(std::fs::TryLockError::Error(error)) => return Err(error.into()),
        }
    }
}

impl Store {
    pub(super) fn background_deadline(&self) -> Option<Instant> {
        BACKGROUND.with(|slot| {
            slot.get()
                .filter(|(key, _)| *key == std::ptr::from_ref(self) as usize)
                .map(|(_, deadline)| deadline)
        })
    }

    /// Runs synchronous automatic maintenance with nonwaiting writer priority.
    /// The scoped mode applies only to this Store on the calling thread.
    pub fn with_background_budget<T>(&self, deadline: Instant, action: impl FnOnce() -> T) -> T {
        struct Restore(Option<(usize, Instant)>);
        impl Drop for Restore {
            fn drop(&mut self) {
                BACKGROUND.with(|slot| slot.set(self.0));
            }
        }
        let _restore = Restore(
            BACKGROUND
                .with(|slot| slot.replace(Some((std::ptr::from_ref(self) as usize, deadline)))),
        );
        action()
    }

    /// Whether automatic maintenance should yield before its next recoverable unit.
    /// Foreground requests and read-only callers do not probe/create admission files.
    pub fn maintenance_should_yield(&self) -> StoreResult<bool> {
        let Some(deadline) = self.background_deadline() else {
            return Ok(false);
        };
        if Instant::now() >= deadline {
            return Ok(true);
        }
        if !self.writable {
            return Ok(true);
        }
        let file = open_private_or_create(&self.paths.root.join("writer-intent.lock"))?;
        match file.try_lock() {
            Ok(()) => {
                File::unlock(&file)?;
                Ok(false)
            }
            Err(std::fs::TryLockError::WouldBlock) => Ok(true),
            Err(std::fs::TryLockError::Error(error)) => Err(error.into()),
        }
    }

    /// Returns at most the latest write observation; no payloads or history are retained.
    #[must_use]
    pub fn last_write_observation(&self) -> Option<PersistenceDiagnostic> {
        self.write_observation
            .lock()
            .ok()
            .and_then(|observation| observation.clone())
    }

    fn record_write<T>(&self, operation: &Operation, result: StoreResult<T>) -> StoreResult<T> {
        let diagnostic = result
            .as_ref()
            .err()
            .and_then(StoreError::diagnostic)
            .cloned()
            .unwrap_or_else(|| operation.finish(&result, true));
        if let Ok(mut observation) = self.write_observation.lock() {
            *observation = Some(diagnostic);
        }
        result
    }

    pub(crate) fn write<T>(
        &self,
        name: &'static str,
        action: impl FnOnce(&mut Connection, &Operation) -> StoreResult<T>,
    ) -> StoreResult<T> {
        let operation = Operation::new(name, self.background_deadline());
        self.record_write(&operation, self.write_inner(&operation, action))
    }

    pub(crate) fn write_readback<T, R>(
        &self,
        name: &'static str,
        action: impl FnOnce(&mut Connection, &Operation) -> StoreResult<T>,
        readback: impl FnOnce(&Connection, T) -> StoreResult<R>,
    ) -> StoreResult<R> {
        let operation = Operation::new(name, self.background_deadline());
        let result = self.write_inner(&operation, action);
        self.record_write(
            &operation,
            self.read_after_write(&operation, result, readback),
        )
    }

    pub(crate) fn write_prepared<P, T>(
        &self,
        name: &'static str,
        prepare: impl FnOnce(&Connection) -> StoreResult<P>,
        action: impl FnOnce(P, &mut Connection, &Operation) -> StoreResult<T>,
    ) -> StoreResult<T> {
        let operation = Operation::new(name, self.background_deadline());
        self.record_write(
            &operation,
            self.prepare_and_write(&operation, prepare, action),
        )
    }

    pub(crate) fn write_prepared_readback<P, T, R>(
        &self,
        name: &'static str,
        prepare: impl FnOnce(&Connection) -> StoreResult<P>,
        action: impl FnOnce(P, &mut Connection, &Operation) -> StoreResult<T>,
        readback: impl FnOnce(&Connection, T) -> StoreResult<R>,
    ) -> StoreResult<R> {
        let operation = Operation::new(name, self.background_deadline());
        let result = self.prepare_and_write(&operation, prepare, action);
        self.record_write(
            &operation,
            self.read_after_write(&operation, result, readback),
        )
    }

    fn prepare_and_write<P, T>(
        &self,
        operation: &Operation,
        prepare: impl FnOnce(&Connection) -> StoreResult<P>,
        action: impl FnOnce(P, &mut Connection, &Operation) -> StoreResult<T>,
    ) -> StoreResult<T> {
        operation.stage("prepare");
        let mut autocommit = true;
        let prepared = (|| {
            let connection = self.connection_until(operation)?;
            autocommit = connection.is_autocommit();
            operation.check_deadline()?;
            connection.busy_timeout(Duration::ZERO)?;
            let result = prepare(&connection);
            autocommit = connection.is_autocommit();
            let restored = connection.busy_timeout(ALLOWANCE);
            match result {
                Err(error) => Err(error),
                Ok(value) => {
                    restored?;
                    Ok(value)
                }
            }
        })();
        match prepared {
            Ok(prepared) => {
                #[cfg(test)]
                super::transaction_qualification::prepared(operation.facts.borrow().operation);
                self.write_inner(operation, |connection, operation| {
                    action(prepared, connection, operation)
                })
            }
            Err(error) => operation.observed(Err(error), autocommit),
        }
    }

    fn read_after_write<T, R>(
        &self,
        operation: &Operation,
        result: StoreResult<T>,
        readback: impl FnOnce(&Connection, T) -> StoreResult<R>,
    ) -> StoreResult<R> {
        let written = result?;
        operation.stage("readback");
        // write_inner returned, so both its writer and connection guards have closed.
        #[cfg(test)]
        super::readback_qualification::before_readback(operation.facts.borrow().operation);
        let result = (|| {
            let connection = self.connection_until(operation)?;
            connection.busy_timeout(Duration::ZERO)?;
            let result = readback(&connection, written);
            let restored = connection.busy_timeout(ALLOWANCE);
            match result {
                Err(error) => Err(error),
                Ok(value) => {
                    restored?;
                    Ok(value)
                }
            }
        })();
        operation.observed(result, true)
    }

    pub(crate) fn write_transaction<T>(
        &self,
        name: &'static str,
        action: impl FnOnce(&Transaction<'_>) -> StoreResult<T>,
    ) -> StoreResult<T> {
        self.write(name, |connection, operation| {
            let tx = operation.begin(connection)?;
            let value = action(&tx)?;
            operation.commit(tx)?;
            Ok(value)
        })
    }

    fn write_inner<T>(
        &self,
        operation: &Operation,
        action: impl FnOnce(&mut Connection, &Operation) -> StoreResult<T>,
    ) -> StoreResult<T> {
        if !self.writable {
            return operation.observed(
                Err(StoreError::Sqlite(rusqlite::Error::SqliteFailure(
                    rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_READONLY),
                    None,
                ))),
                true,
            );
        }
        let mut admission = match Admission::acquire(self, operation) {
            Ok(admission) => admission,
            Err(error) => return operation.observed(Err(error), true),
        };
        operation.stage("connection");
        let mut connection = match self.connection_until(operation) {
            Ok(connection) => connection,
            Err(error) => return operation.observed(Err(error), true),
        };
        admission.admitted();
        let result = (|| {
            if !connection.is_autocommit() {
                return Err(StoreError::Conflict(
                    "connection still owns an unfinished transaction".into(),
                ));
            }
            connection.busy_timeout(Duration::ZERO)?;
            action(&mut connection, operation)
        })();
        let result = operation.observed(result, connection.is_autocommit());
        // Only reads use SQLite's built-in bounded timeout; writers always reset to zero.
        let restored = connection.busy_timeout(ALLOWANCE);
        match result {
            Err(error) => Err(error),
            Ok(value) => {
                operation.stage("connection_restore");
                operation.observed(
                    restored.map(|()| value).map_err(StoreError::from),
                    connection.is_autocommit(),
                )
            }
        }
    }

    pub(super) fn connection_until(
        &self,
        operation: &Operation,
    ) -> StoreResult<MutexGuard<'_, Connection>> {
        let started = Instant::now();
        let result = loop {
            if let Err(error) = operation.check_deadline() {
                break Err(error);
            }
            match self.connection.try_lock() {
                Ok(connection) => break Ok(connection),
                Err(TryLockError::WouldBlock) => {
                    if let Err(error) = operation.pause() {
                        break Err(error);
                    }
                }
                Err(TryLockError::Poisoned(_)) => {
                    break Err(StoreError::Conflict("store mutex poisoned".into()));
                }
            }
        };
        operation.facts.borrow_mut().connection_wait_us += micros(started.elapsed());
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::process::{Child, Command, Stdio};
    use std::sync::Arc;

    fn fixture() -> (tempfile::TempDir, Arc<Store>) {
        let temporary = tempfile::tempdir().unwrap();
        let paths = crate::StatePaths::new(temporary.path().join("private"));
        let store = Arc::new(Store::open(paths, "test", 1).unwrap());
        store
            .create_job(&super::super::CreateJob {
                id: uuid::Uuid::from_u128(180).to_string(),
                name: "disabled".into(),
                description: None,
                tags_json: "[]".into(),
                enabled: false,
                definition_json: "{}".into(),
                now_us: 1,
                cursor_us: 1,
            })
            .unwrap();
        (temporary, store)
    }

    #[test]
    fn admission_connection_and_sqlite_share_one_deadline_and_preserve_removal() {
        let (_temporary, store) = fixture();
        let external = Connection::open(&store.paths.database).unwrap();
        external.execute_batch("BEGIN IMMEDIATE").unwrap();
        let writer = open_private_or_create(&store.paths.root.join("writer.lock")).unwrap();
        writer.try_lock().unwrap();
        let connection = store.conn().unwrap();
        let copied = store.clone();
        let started = Instant::now();
        let removal = std::thread::spawn(move || copied.remove_job("disabled", 2));
        let intent = open_private_or_create(&store.paths.root.join("writer-intent.lock")).unwrap();
        wait_until(|| match intent.try_lock() {
            Ok(()) => {
                File::unlock(&intent).unwrap();
                false
            }
            Err(std::fs::TryLockError::WouldBlock) => true,
            Err(error) => panic!("intent barrier: {error}"),
        });
        std::thread::sleep(Duration::from_millis(250));
        File::unlock(&writer).unwrap();
        wait_until(|| match writer.try_lock() {
            Ok(()) => {
                File::unlock(&writer).unwrap();
                false
            }
            Err(std::fs::TryLockError::WouldBlock) => true,
            Err(error) => panic!("writer barrier: {error}"),
        });
        std::thread::sleep(Duration::from_millis(250));
        drop(connection);
        let error = removal.join().unwrap().unwrap_err();
        assert!(started.elapsed() >= Duration::from_secs(4));
        assert!(started.elapsed() < Duration::from_secs(7));
        let facts = error.diagnostic().unwrap();
        assert_eq!(
            (facts.operation, facts.stage, facts.kind),
            ("remove_job", "begin", "busy")
        );
        assert_eq!(facts.sqlite_primary_code, Some(5));
        assert_eq!(facts.sqlite_extended_code, Some(5));
        assert!(facts.admission_wait_us >= 150_000);
        assert!(facts.connection_wait_us >= 150_000);
        assert!(facts.sqlite_wait_us >= 3_500_000);
        assert!(facts.elapsed_us < 6_500_000);
        assert_eq!(facts.transaction_us, 0);
        assert_eq!(facts.durable_outcome, "not_committed");
        assert!(facts.retryable);
        assert!(facts.retries > 0);
        assert_eq!(store.job("disabled").unwrap().removed_at_us, None);
        assert!(!store.job("disabled").unwrap().enabled);
        external.execute_batch("ROLLBACK").unwrap();
        store.remove_job("disabled", 3).unwrap();
        assert!(matches!(
            store.job("disabled"),
            Err(StoreError::NotFound(_))
        ));
    }

    #[test]
    fn held_then_released_external_writer_records_successful_wait_and_hold() {
        let (_temporary, store) = fixture();
        let external = Connection::open(&store.paths.database).unwrap();
        external.execute_batch("BEGIN IMMEDIATE").unwrap();
        let copied = store.clone();
        let (sender, receiver) = std::sync::mpsc::channel();
        let removal = std::thread::spawn(move || {
            BUSY_ACK.with(|slot| *slot.borrow_mut() = Some(sender));
            copied.remove_job("disabled", 2)
        });
        receiver.recv_timeout(Duration::from_secs(3)).unwrap();
        std::thread::sleep(Duration::from_millis(150));
        external.execute_batch("ROLLBACK").unwrap();
        removal.join().unwrap().unwrap();
        let observation = store.last_write_observation().unwrap();
        assert_eq!(observation.kind, "success");
        assert!(observation.sqlite_wait_us >= 75_000);
        assert!(observation.transaction_us > 0);
        assert!(observation.commit_us > 0);
        assert_eq!(observation.durable_outcome, "committed");
    }

    #[test]
    fn uuid_shaped_job_name_cannot_remove_two_resolved_jobs() {
        let (_temporary, store) = fixture();
        let reference = uuid::Uuid::from_u128(180).to_string();
        let other_id = uuid::Uuid::from_u128(181).to_string();
        store
            .create_job(&super::super::CreateJob {
                id: other_id.clone(),
                name: reference.clone(),
                description: None,
                tags_json: "[]".into(),
                enabled: false,
                definition_json: "{}".into(),
                now_us: 1,
                cursor_us: 1,
            })
            .unwrap();
        let selected_id = store.job(&reference).unwrap().id;
        store.remove_job(&reference, 2).unwrap();
        let connection = store.conn().unwrap();
        let rows = connection
            .prepare("SELECT id,removed_at_us FROM jobs ORDER BY id")
            .unwrap()
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Option<i64>>(1)?))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows.iter().filter(|(_, removed)| removed.is_some()).count(),
            1
        );
        for (id, removed) in rows {
            assert_eq!(removed, (id == selected_id).then_some(2));
        }
    }

    #[test]
    fn locked_sqlite_resource_is_not_retried_or_misclassified_as_busy() {
        let (_temporary, store) = fixture();
        store
            .conn()
            .unwrap()
            .execute_batch(
                "CREATE TABLE locked_control(value INTEGER); INSERT INTO locked_control VALUES(1)",
            )
            .unwrap();
        let error = store
            .write("locked_control", |connection, operation| {
                let tx = operation.begin(connection)?;
                let mut statement = tx.prepare("SELECT value FROM locked_control")?;
                let mut rows = statement.query([])?;
                assert!(rows.next()?.is_some());
                tx.execute_batch("DROP TABLE locked_control")?;
                Ok(())
            })
            .unwrap_err();
        let facts = error.diagnostic().unwrap();
        assert_eq!(facts.kind, "locked");
        assert_eq!(facts.sqlite_primary_code, Some(6));
        assert_eq!(facts.retries, 0);
        assert!(!facts.retryable);
        assert_eq!(facts.durable_outcome, "not_committed");
        assert!(store.job("disabled").is_ok());
    }

    #[test]
    fn actual_failed_commit_is_unknown_and_never_replays_the_body() {
        let (_temporary, store) = fixture();
        let calls = Cell::new(0);
        let error = store.write("commit_control", |connection, operation| {
            let tx = operation.begin(connection)?;
            calls.set(calls.get() + 1);
            tx.pragma_update(None, "defer_foreign_keys", true)?;
            tx.execute("UPDATE jobs SET description='body executed once'", [])?;
            tx.execute("INSERT INTO job_revisions(job_id,revision,definition_json,created_at_us,created_by) VALUES('missing-parent',1,'{}',1,'add')", [])?;
            operation.commit(tx)
        }).unwrap_err();
        let facts = error.diagnostic().unwrap();
        assert_eq!(
            (
                facts.stage,
                facts.sqlite_primary_code,
                facts.sqlite_extended_code
            ),
            ("commit", Some(19), Some(787))
        );
        assert_eq!(facts.durable_outcome, "unknown");
        assert!(!facts.retryable);
        assert_eq!(facts.retries, 0);
        assert_eq!(calls.get(), 1);
        assert_eq!(store.job("disabled").unwrap().description, None);
        let connection = store.conn().unwrap();
        assert!(connection.is_autocommit());
        assert_eq!(
            connection
                .query_row(
                    "SELECT count(*) FROM job_revisions WHERE job_id='missing-parent'",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            0
        );
    }

    #[test]
    fn adapter_codes_are_safe_in_display_debug_and_explicitly_inspectable() {
        let operation = Operation::new("redaction_control", None);
        let error = operation
            .observed::<()>(
                Err(StoreError::Io(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "SECRET_ARGUMENT_AND_CREDENTIAL",
                ))),
                true,
            )
            .unwrap_err();
        assert_eq!(error.diagnostic().unwrap().kind, "io");
        assert!(!error.diagnostic().unwrap().retryable);
        for rendered in [
            format!("{error}"),
            format!("{error:#}"),
            format!("{error:?}"),
            format!("{error:#?}"),
        ] {
            assert!(!rendered.contains("SECRET_ARGUMENT_AND_CREDENTIAL"));
        }
        assert!(
            error
                .original_error()
                .to_string()
                .contains("SECRET_ARGUMENT_AND_CREDENTIAL")
        );
        let port = super::super::map_store_port_error(super::super::StorePortError::Store(error));
        assert_eq!(port.persistence_diagnostic().unwrap().kind, "io");
        assert!(!format!("{port:?}").contains("SECRET_ARGUMENT_AND_CREDENTIAL"));
        let code = Operation::new("io_code", None)
            .observed::<()>(
                Err(StoreError::Io(std::io::Error::from_raw_os_error(5))),
                true,
            )
            .unwrap_err();
        assert_eq!(code.diagnostic().unwrap().os_error_code, Some(5));
    }

    #[test]
    fn invalid_database_and_real_admission_io_are_not_retryable_contention() {
        let (_temporary, store) = fixture();
        let writer_path = store.paths.root.join("writer.lock");
        std::fs::remove_file(&writer_path).unwrap();
        std::fs::create_dir(&writer_path).unwrap();
        let error = store.remove_job("disabled", 2).unwrap_err();
        let facts = error.diagnostic().unwrap();
        assert_eq!((facts.stage, facts.kind), ("writer_admission", "io"));
        assert!(!facts.retryable);
        assert_eq!(facts.retries, 0);
        assert_eq!(store.job("disabled").unwrap().removed_at_us, None);

        let temporary = tempfile::tempdir().unwrap();
        let paths = crate::StatePaths::new(temporary.path().join("private"));
        paths.ensure().unwrap();
        let mut database = locron_core::filesystem::create_private_new(&paths.database).unwrap();
        database
            .write_all(b"SECRET_CORRUPT_DATABASE_PAYLOAD")
            .unwrap();
        drop(database);
        let Err(error) = Store::open(paths, "test", 1) else {
            panic!("an invalid database must refuse");
        };
        let facts = error.diagnostic().unwrap();
        assert_eq!(facts.kind, "sqlite");
        assert_eq!(facts.sqlite_primary_code, Some(26));
        assert!(!facts.retryable);
        assert_eq!(facts.retries, 0);
        assert!(!format!("{error:#?}").contains("SECRET_CORRUPT_DATABASE_PAYLOAD"));
    }

    #[test]
    fn read_only_access_does_not_create_coordination_files_even_on_write_refusal() {
        let (_temporary, store) = fixture();
        let other = tempfile::tempdir().unwrap();
        let paths = crate::StatePaths::new(other.path().join("private"));
        paths.ensure().unwrap();
        {
            let connection = store.conn().unwrap();
            connection
                .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
                .unwrap();
        }
        std::fs::copy(&store.paths.database, &paths.database).unwrap();
        crate::paths::set_owner_only(&paths.database, false).unwrap();
        let readonly = Store::open_read_only(&paths.database).unwrap();
        assert!(readonly.job("disabled").is_ok());
        let error = readonly.remove_job("disabled", 2).unwrap_err();
        assert_eq!(error.diagnostic().unwrap().sqlite_primary_code, Some(8));
        assert!(!paths.root.join("writer.lock").exists());
        assert!(!paths.root.join("writer-intent.lock").exists());
        let absent = other.path().join("absent");
        assert!(Store::open_read_only(&absent.join("state.db")).is_err());
        assert!(!absent.exists());
    }

    struct Actor(Child);
    impl Drop for Actor {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    fn actor(root: &std::path::Path, role: &str) -> Actor {
        Actor(
            Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "store::contention::tests::process_actor",
                    "--ignored",
                    "--nocapture",
                ])
                .env("LOCRON_CONTENTION_TEST_ROOT", root)
                .env("LOCRON_CONTENTION_TEST_ROLE", role)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        )
    }
    fn wait_until(mut ready: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !ready() {
            assert!(Instant::now() < deadline, "contention barrier expired");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn separate_process_foreground_intent_defers_background_and_crash_releases_writer() {
        let (_temporary, store) = fixture();
        let mut holder = actor(&store.paths.root, "hold");
        wait_until(|| store.paths.root.join("actor-ready").exists());
        let mut foreground = actor(&store.paths.root, "remove");
        let intent = open_private_or_create(&store.paths.root.join("writer-intent.lock")).unwrap();
        wait_until(|| match intent.try_lock() {
            Ok(()) => {
                File::unlock(&intent).unwrap();
                false
            }
            Err(std::fs::TryLockError::WouldBlock) => true,
            Err(error) => panic!("intent probe: {error}"),
        });
        let started = Instant::now();
        let result =
            store.with_background_budget(Instant::now() + Duration::from_millis(250), || {
                assert!(store.maintenance_should_yield().unwrap());
                store.set_setting("output_limit_bytes", "0", 2)
            });
        assert!(matches!(result, Err(StoreError::MaintenanceDeferred)));
        assert!(started.elapsed() < Duration::from_secs(1));
        holder.0.kill().unwrap();
        holder.0.wait().unwrap();
        wait_until(|| foreground.0.try_wait().unwrap().is_some());
        assert!(foreground.0.wait().unwrap().success());
        assert!(matches!(
            store.job("disabled"),
            Err(StoreError::NotFound(_))
        ));
        // A new foreground writer can also proceed after both independent handles close.
        store.set_setting("output_limit_bytes", "1", 3).unwrap();
    }

    #[test]
    fn background_busy_and_expired_units_defer_without_waiting() {
        let (_temporary, store) = fixture();
        let external = Connection::open(&store.paths.database).unwrap();
        external.execute_batch("BEGIN IMMEDIATE").unwrap();
        let started = Instant::now();
        let result = store
            .with_background_budget(Instant::now() + Duration::from_millis(250), || {
                store.remove_job("disabled", 2)
            });
        assert!(matches!(result, Err(StoreError::MaintenanceDeferred)));
        assert!(started.elapsed() < Duration::from_millis(500));
        let result =
            store.with_background_budget(Instant::now(), || store.remove_job("disabled", 2));
        assert!(matches!(result, Err(StoreError::MaintenanceDeferred)));
        external.execute_batch("ROLLBACK").unwrap();
        store.remove_job("disabled", 3).unwrap();
    }

    #[test]
    #[ignore = "private subprocess actor invoked only by the coordinated test"]
    fn process_actor() {
        let root = std::env::var_os("LOCRON_CONTENTION_TEST_ROOT").unwrap();
        let paths = crate::StatePaths::new(root.into());
        match std::env::var("LOCRON_CONTENTION_TEST_ROLE")
            .unwrap()
            .as_str()
        {
            "hold" => {
                let writer = open_private_or_create(&paths.root.join("writer.lock")).unwrap();
                writer.try_lock().unwrap();
                let mut ready =
                    locron_core::filesystem::create_private_new(&paths.root.join("actor-ready"))
                        .unwrap();
                ready.write_all(b"ready").unwrap();
                std::thread::sleep(Duration::from_secs(15));
                drop(writer);
            }
            "remove" => Store::open(paths, "test", 2)
                .unwrap()
                .remove_job("disabled", 3)
                .unwrap(),
            _ => panic!("unknown test actor"),
        }
    }
}
