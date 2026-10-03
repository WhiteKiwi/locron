//! Admit the fixed WAL transition without retaining a failed statement's read lock.

use std::time::{Duration, Instant};

use rusqlite::Connection;

use crate::windows_open::{OpenTrace, Stage};
use crate::{StoreError, StoreResult};

const CONFIGURE_ALLOWANCE: Duration = Duration::from_secs(5);
const CONTENTION_YIELD: Duration = Duration::from_millis(10);

pub(crate) fn configure(connection: &Connection, trace: &OpenTrace) -> StoreResult<()> {
    configure_until(
        connection,
        trace,
        Instant::now()
            .checked_add(CONFIGURE_ALLOWANCE)
            .ok_or_else(deadline_error)?,
        |_| {},
    )
}

fn configure_until(
    connection: &Connection,
    trace: &OpenTrace,
    deadline: Instant,
    on_busy: impl FnMut(&rusqlite::Error),
) -> StoreResult<()> {
    trace.store(
        Stage::ConfigureWal,
        admit_wal_until(connection, deadline, on_busy),
    )?;
    trace.store(
        Stage::ConfigureSettings,
        configure_remaining(connection, deadline),
    )
}

fn deadline_error() -> StoreError {
    std::io::Error::new(
        std::io::ErrorKind::TimedOut,
        "SQLite configuration deadline elapsed",
    )
    .into()
}

fn check_deadline(deadline: Instant) -> StoreResult<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|remaining| !remaining.is_zero())
        .ok_or_else(deadline_error)
}

fn is_exact_busy(error: &rusqlite::Error) -> bool {
    matches!(error, rusqlite::Error::SqliteFailure(code, _) if code.extended_code == 5)
}

fn wal_attempt(connection: &Connection) -> rusqlite::Result<String> {
    // query_one steps through DONE rather than accepting only SQLITE_ROW. Finalize
    // explicitly even when stepping failed; no prepared statement escapes this call.
    let mut statement = connection.prepare("PRAGMA main.journal_mode=WAL;")?;
    let stepped = statement.query_one([], |row| row.get(0));
    let finalized = statement.finalize();
    match (stepped, finalized) {
        (Ok(mode), Ok(())) => Ok(mode),
        (Err(step), Err(finalize)) if is_exact_busy(&step) && !is_exact_busy(&finalize) => {
            Err(finalize)
        }
        (Err(step), _) => Err(step),
        (Ok(_), Err(finalize)) => Err(finalize),
    }
}

fn admit_wal_until(
    connection: &Connection,
    deadline: Instant,
    mut on_busy: impl FnMut(&rusqlite::Error),
) -> StoreResult<()> {
    // Refuse the caller's explicit transaction before changing its timeout.
    if !connection.is_autocommit() {
        return Err(StoreError::Conflict(
            "WAL admission requires an idle autocommit connection".into(),
        ));
    }
    check_deadline(deadline)?;
    // Internal busy handlers wait per locking event. The outer, finalized-statement
    // loop owns the one admission allowance instead of multiplying those waits.
    connection.busy_timeout(Duration::ZERO)?;
    check_deadline(deadline)?;
    let mut last_busy = None;
    loop {
        if check_deadline(deadline).is_err() {
            return Err(last_busy.map_or_else(deadline_error, StoreError::Sqlite));
        }
        match wal_attempt(connection) {
            Ok(mode) => {
                check_deadline(deadline)?;
                if mode != "wal" {
                    return Err(StoreError::Conflict(
                        "SQLite did not accept WAL journal mode".into(),
                    ));
                }
                return Ok(());
            }
            Err(error) if is_exact_busy(&error) => {
                if !connection.is_autocommit() {
                    return Err(error.into());
                }
                // Only tests supply an observer; production supplies a no-op. It sees
                // the genuine native BUSY only after the statement has finalized.
                on_busy(&error);
                let Ok(remaining) = check_deadline(deadline) else {
                    return Err(error.into());
                };
                last_busy = Some(error);
                std::thread::sleep(remaining.min(CONTENTION_YIELD));
            }
            Err(error) => return Err(error.into()),
        }
    }
}

fn configure_remaining(connection: &Connection, deadline: Instant) -> StoreResult<()> {
    check_deadline(deadline)?;
    connection.execute_batch("PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON; PRAGMA locking_mode=NORMAL; PRAGMA trusted_schema=OFF;")?;
    check_deadline(deadline)?;
    connection.busy_timeout(CONFIGURE_ALLOWANCE)?;
    check_deadline(deadline)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    use locron_core::filesystem::{DirectoryGuard, GuardedFile, create_private_new, is_private};
    use rusqlite::{Connection, OpenFlags, TransactionState};

    use super::{CONFIGURE_ALLOWANCE, configure, configure_until, is_exact_busy, wal_attempt};
    use crate::StoreError;
    use crate::windows_open::OpenTrace;

    struct Fixture {
        database: PathBuf,
        leaves: Vec<GuardedFile>,
        _guard: DirectoryGuard,
        _temporary: tempfile::TempDir,
    }

    impl Fixture {
        fn new() -> Self {
            let temporary = tempfile::tempdir().unwrap();
            let root = temporary.path().join("private");
            let guard = DirectoryGuard::private(&root).unwrap();
            let database = guard.normalized_path().join("configure.db");
            let leaves = ["configure.db", "configure.db-wal", "configure.db-shm"]
                .map(|name| create_private_new(&guard.normalized_path().join(name)).unwrap())
                .into();
            let fixture = Self {
                database,
                leaves,
                _guard: guard,
                _temporary: temporary,
            };
            let connection = fixture.open(OpenFlags::SQLITE_OPEN_READ_WRITE);
            connection
                .execute_batch(
                    "CREATE TABLE facts(value INTEGER NOT NULL); INSERT INTO facts VALUES(1);",
                )
                .unwrap();
            assert_eq!(
                connection
                    .query_one::<String, _, _>("PRAGMA journal_mode;", [], |row| row.get(0))
                    .unwrap(),
                "delete"
            );
            fixture
        }

        fn open(&self, flags: OpenFlags) -> Connection {
            Connection::open_with_flags_and_vfs(&self.database, flags, "win32-longpath").unwrap()
        }

        fn release_preparation(&mut self) {
            self.leaves.clear();
        }

        fn prepare_reopen(&mut self) {
            self.leaves = ["configure.db-wal", "configure.db-shm"]
                .map(|name| create_private_new(&self.database.with_file_name(name)).unwrap())
                .into();
        }

        fn assert_no_journals(&self) {
            for name in [
                "configure.db-wal",
                "configure.db-shm",
                "configure.db-journal",
            ] {
                assert!(!self.database.with_file_name(name).exists(), "{name}");
            }
        }
    }

    fn assert_settings(connection: &Connection) {
        for (name, expected) in [
            ("synchronous", 2),
            ("foreign_keys", 1),
            ("trusted_schema", 0),
            ("busy_timeout", 5_000),
        ] {
            assert_eq!(
                connection
                    .pragma_query_value(None, name, |row| row.get::<_, i64>(0))
                    .unwrap(),
                expected
            );
        }
        for (name, expected) in [("journal_mode", "wal"), ("locking_mode", "normal")] {
            assert_eq!(
                connection
                    .pragma_query_value(None, name, |row| row.get::<_, String>(0))
                    .unwrap(),
                expected
            );
        }
        assert_eq!(
            connection
                .transaction_state(Some(rusqlite::MAIN_DB))
                .unwrap(),
            TransactionState::None
        );
    }

    #[test]
    fn actual_busy_finalization_releases_read_lock_then_admits_wal_within_original_deadline() {
        let mut fixture = Fixture::new();
        let blocker = fixture.open(OpenFlags::SQLITE_OPEN_READ_WRITE);
        blocker.busy_timeout(Duration::ZERO).unwrap();
        blocker
            .execute_batch("BEGIN IMMEDIATE; INSERT INTO facts VALUES(2);")
            .unwrap();
        let connection = fixture.open(OpenFlags::SQLITE_OPEN_READ_WRITE);
        connection.busy_timeout(Duration::ZERO).unwrap();
        let error = wal_attempt(&connection).unwrap_err();
        assert!(is_exact_busy(&error));
        assert!(connection.is_autocommit());
        assert_eq!(
            connection
                .transaction_state(Some(rusqlite::MAIN_DB))
                .unwrap(),
            TransactionState::None
        );
        // With timeout zero, this actual commit can only succeed if the failed WAL
        // statement released its competing shared read lock before returning.
        blocker.execute_batch("COMMIT;").unwrap();
        blocker
            .execute_batch("BEGIN IMMEDIATE; INSERT INTO facts VALUES(3);")
            .unwrap();

        let (busy_tx, busy_rx) = mpsc::sync_channel(1);
        let worker = std::thread::spawn(move || {
            let deadline = Instant::now().checked_add(CONFIGURE_ALLOWANCE).unwrap();
            let mut observed = false;
            configure_until(&connection, &OpenTrace::new(), deadline, |error| {
                assert!(is_exact_busy(error));
                if !observed {
                    busy_tx.send(()).unwrap();
                    observed = true;
                }
            })
            .unwrap();
            assert!(
                observed,
                "the holder must cause a genuine completed BUSY attempt"
            );
            assert!(Instant::now() < deadline);
            assert_settings(&connection);
            connection
        });
        busy_rx.recv_timeout(CONFIGURE_ALLOWANCE).unwrap();
        blocker.execute_batch("COMMIT;").unwrap();
        drop(blocker);
        let connection = worker.join().unwrap();
        assert_eq!(
            connection
                .query_one::<i64, _, _>("SELECT COUNT(*) FROM facts;", [], |row| row.get(0))
                .unwrap(),
            3
        );
        connection
            .execute("INSERT INTO facts VALUES(4);", [])
            .unwrap();
        for name in ["configure.db", "configure.db-wal", "configure.db-shm"] {
            assert!(is_private(&fixture.database.with_file_name(name), false).unwrap());
        }
        fixture.release_preparation();
        drop(connection);
        fixture.assert_no_journals();
        fixture.prepare_reopen();
        let reopened = fixture.open(OpenFlags::SQLITE_OPEN_READ_WRITE);
        configure(&reopened, &OpenTrace::new()).unwrap();
        assert_settings(&reopened);
        assert_eq!(
            reopened
                .query_one::<i64, _, _>("SELECT COUNT(*) FROM facts;", [], |row| row.get(0))
                .unwrap(),
            4
        );
        fixture.release_preparation();
        drop(reopened);
        fixture.assert_no_journals();
    }

    #[test]
    fn persistent_native_busy_returns_original_code_with_one_short_allowance() {
        let fixture = Fixture::new();
        let blocker = fixture.open(OpenFlags::SQLITE_OPEN_READ_WRITE);
        blocker.busy_timeout(Duration::ZERO).unwrap();
        blocker.execute_batch("BEGIN IMMEDIATE;").unwrap();
        let connection = fixture.open(OpenFlags::SQLITE_OPEN_READ_WRITE);
        let entered = Instant::now();
        let deadline = entered.checked_add(Duration::from_millis(100)).unwrap();
        let mut attempts = 0;
        let error = configure_until(&connection, &OpenTrace::new(), deadline, |_| {
            attempts += 1;
        })
        .unwrap_err();
        assert!(matches!(error, StoreError::Sqlite(error) if is_exact_busy(&error)));
        assert!(
            attempts > 1,
            "real contention must exercise finalized reattempts"
        );
        assert!(Instant::now() >= deadline);
        assert!(entered.elapsed() < Duration::from_secs(1));
        assert!(connection.is_autocommit());
        assert_eq!(
            connection
                .transaction_state(Some(rusqlite::MAIN_DB))
                .unwrap(),
            TransactionState::None
        );
        blocker.execute_batch("COMMIT;").unwrap();
        assert_eq!(
            connection
                .query_one::<String, _, _>("PRAGMA journal_mode;", [], |row| row.get(0))
                .unwrap(),
            "delete"
        );
    }

    #[test]
    fn expired_and_explicit_transaction_entries_refuse_before_timeout_changes() {
        let fixture = Fixture::new();
        let connection = fixture.open(OpenFlags::SQLITE_OPEN_READ_WRITE);
        connection.busy_timeout(Duration::from_millis(321)).unwrap();
        assert!(matches!(
            configure_until(&connection, &OpenTrace::new(), Instant::now(), |_| {
                panic!("expired entry must not attempt WAL")
            }),
            Err(StoreError::Io(error)) if error.kind() == std::io::ErrorKind::TimedOut
        ));
        connection.execute_batch("BEGIN;").unwrap();
        assert!(matches!(
            configure(&connection, &OpenTrace::new()),
            Err(StoreError::Conflict(_))
        ));
        assert!(!connection.is_autocommit());
        assert_eq!(
            connection
                .pragma_query_value(None, "busy_timeout", |row| row.get::<_, i64>(0))
                .unwrap(),
            321
        );
        connection.execute_batch("ROLLBACK;").unwrap();
        assert_eq!(
            connection
                .query_one::<String, _, _>("PRAGMA journal_mode;", [], |row| row.get(0))
                .unwrap(),
            "delete"
        );
    }

    #[test]
    fn non_wal_result_and_real_readonly_error_never_enter_contention_retry() {
        let memory = Connection::open_in_memory().unwrap();
        assert!(matches!(
            configure_until(
                &memory,
                &OpenTrace::new(),
                Instant::now().checked_add(CONFIGURE_ALLOWANCE).unwrap(),
                |_| panic!("an unchanged in-memory mode is not BUSY")
            ),
            Err(StoreError::Conflict(_))
        ));
        assert_eq!(
            memory
                .query_one::<String, _, _>("PRAGMA journal_mode;", [], |row| row.get(0))
                .unwrap(),
            "memory"
        );
        let fixture = Fixture::new();
        let readonly = fixture.open(OpenFlags::SQLITE_OPEN_READ_ONLY);
        let expected = wal_attempt(&readonly).unwrap_err();
        assert!(
            matches!(&expected, rusqlite::Error::SqliteFailure(code, _) if code.code == rusqlite::ErrorCode::ReadOnly)
        );
        let error = configure_until(
            &readonly,
            &OpenTrace::new(),
            Instant::now().checked_add(CONFIGURE_ALLOWANCE).unwrap(),
            |_| panic!("a genuine readonly error must not become BUSY retry"),
        )
        .unwrap_err();
        let StoreError::Sqlite(actual) = error else {
            panic!("the original SQLite error must be preserved");
        };
        assert_eq!(actual, expected);
        assert!(readonly.is_autocommit());
    }
}
