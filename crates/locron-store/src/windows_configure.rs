//! Admit the fixed WAL transition without retaining a failed statement's read lock.

use std::time::{Duration, Instant};

use rusqlite::Connection;

use crate::windows_open::{OpenTrace, Stage};
use crate::{StoreError, StoreResult};

#[cfg(debug_assertions)]
use diagnostics::{Gate, Snapshot, Stamp};

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
    #[cfg(debug_assertions)]
    let mut observations = Snapshot::new(deadline);
    let wal = admit_wal_until(
        connection,
        deadline,
        on_busy,
        #[cfg(debug_assertions)]
        &mut observations,
    );
    trace.configuration(
        Stage::ConfigureWal,
        wal,
        #[cfg(debug_assertions)]
        &observations,
    )?;
    let settings = configure_remaining(
        connection,
        deadline,
        #[cfg(debug_assertions)]
        &mut observations,
    );
    trace.configuration(
        Stage::ConfigureSettings,
        settings,
        #[cfg(debug_assertions)]
        &observations,
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

fn observed_deadline(
    deadline: Instant,
    #[cfg(debug_assertions)] observations: &mut Snapshot,
    #[cfg(debug_assertions)] gate: Gate,
) -> StoreResult<Duration> {
    let result = check_deadline(deadline);
    #[cfg(debug_assertions)]
    observations.gate(gate, &result);
    result
}

#[cfg(test)]
fn is_exact_busy(error: &rusqlite::Error) -> bool {
    matches!(error, rusqlite::Error::SqliteFailure(code, _) if code.extended_code == 5)
}

fn is_wal_contention(error: &rusqlite::Error) -> bool {
    matches!(error, rusqlite::Error::SqliteFailure(code, _) if matches!(code.extended_code, 5 | 261))
}

#[cfg(test)]
fn wal_attempt(connection: &Connection) -> rusqlite::Result<String> {
    #[cfg(debug_assertions)]
    let mut observations = Snapshot::new(Instant::now());
    wal_attempt_observed(
        connection,
        #[cfg(debug_assertions)]
        &mut observations,
    )
}

fn wal_attempt_observed(
    connection: &Connection,
    #[cfg(debug_assertions)] observations: &mut Snapshot,
) -> rusqlite::Result<String> {
    // query_one steps through DONE rather than accepting only SQLITE_ROW. Finalize
    // explicitly even when stepping failed; no prepared statement escapes this call.
    #[cfg(debug_assertions)]
    observations.mark(Stamp::PrepareEnter);
    let prepared = connection.prepare("PRAGMA main.journal_mode=WAL;");
    #[cfg(debug_assertions)]
    {
        observations.mark(Stamp::PrepareReturn);
        observations.sqlite_result(&prepared);
    }
    let mut statement = prepared?;
    #[cfg(debug_assertions)]
    observations.mark(Stamp::QueryEnter);
    let stepped = statement.query_one([], |row| {
        #[cfg(debug_assertions)]
        observations.mark(Stamp::RowEnter);
        let result = row.get(0);
        #[cfg(debug_assertions)]
        {
            observations.mark(Stamp::RowReturn);
            observations.sqlite_result(&result);
        }
        result
    });
    #[cfg(debug_assertions)]
    {
        observations.mark(Stamp::QueryReturn);
        observations.sqlite_result(&stepped);
        // Only successful query_one confirms its internal second step reached DONE.
        observations.query_returned(&stepped);
        observations.mark(Stamp::FinalizeEnter);
    }
    let finalized = statement.finalize();
    #[cfg(debug_assertions)]
    {
        observations.mark(Stamp::FinalizeReturn);
        observations.sqlite_result(&finalized);
    }
    match (stepped, finalized) {
        (Ok(mode), Ok(())) => Ok(mode),
        (Err(step), Err(finalize)) if is_wal_contention(&step) && !is_wal_contention(&finalize) => {
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
    #[cfg(debug_assertions)] observations: &mut Snapshot,
) -> StoreResult<()> {
    // Refuse the caller's explicit transaction before changing its timeout.
    let autocommit = connection.is_autocommit();
    #[cfg(debug_assertions)]
    observations.autocommit = Some(autocommit);
    if !autocommit {
        return Err(StoreError::Conflict(
            "WAL admission requires an idle autocommit connection".into(),
        ));
    }
    observed_deadline(
        deadline,
        #[cfg(debug_assertions)]
        observations,
        #[cfg(debug_assertions)]
        Gate::Entry,
    )?;
    // Internal busy handlers wait per locking event. The outer, finalized-statement
    // loop owns the one admission allowance instead of multiplying those waits.
    #[cfg(debug_assertions)]
    observations.phase("busy-zero-enter");
    let zero_timeout = connection.busy_timeout(Duration::ZERO);
    #[cfg(debug_assertions)]
    {
        observations.phase("busy-zero-return");
        observations.sqlite_result(&zero_timeout);
    }
    zero_timeout?;
    observed_deadline(
        deadline,
        #[cfg(debug_assertions)]
        observations,
        #[cfg(debug_assertions)]
        Gate::AfterZero,
    )?;
    let mut last_busy = None;
    loop {
        if observed_deadline(
            deadline,
            #[cfg(debug_assertions)]
            observations,
            #[cfg(debug_assertions)]
            Gate::BeforeAttempt,
        )
        .is_err()
        {
            return Err(last_busy.map_or_else(deadline_error, StoreError::Sqlite));
        }
        #[cfg(debug_assertions)]
        observations.begin_attempt();
        match wal_attempt_observed(
            connection,
            #[cfg(debug_assertions)]
            observations,
        ) {
            Ok(mode) => {
                observed_deadline(
                    deadline,
                    #[cfg(debug_assertions)]
                    observations,
                    #[cfg(debug_assertions)]
                    Gate::AfterWal,
                )?;
                if mode != "wal" {
                    return Err(StoreError::Conflict(
                        "SQLite did not accept WAL journal mode".into(),
                    ));
                }
                return Ok(());
            }
            Err(error) if is_wal_contention(&error) => {
                let autocommit = connection.is_autocommit();
                #[cfg(debug_assertions)]
                observations.autocommit = Some(autocommit);
                if !autocommit {
                    return Err(error.into());
                }
                // Only tests supply an observer; production supplies a no-op. It sees
                // the genuine native BUSY only after the statement has finalized.
                on_busy(&error);
                let Ok(remaining) = observed_deadline(
                    deadline,
                    #[cfg(debug_assertions)]
                    observations,
                    #[cfg(debug_assertions)]
                    Gate::AfterBusy,
                ) else {
                    return Err(error.into());
                };
                last_busy = Some(error);
                std::thread::sleep(remaining.min(CONTENTION_YIELD));
            }
            Err(error) => return Err(error.into()),
        }
    }
}

fn configure_remaining(
    connection: &Connection,
    deadline: Instant,
    #[cfg(debug_assertions)] observations: &mut Snapshot,
) -> StoreResult<()> {
    observed_deadline(
        deadline,
        #[cfg(debug_assertions)]
        observations,
        #[cfg(debug_assertions)]
        Gate::BeforeSettings,
    )?;
    #[cfg(debug_assertions)]
    observations.phase("settings-enter");
    let settings = connection.execute_batch("PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON; PRAGMA locking_mode=NORMAL; PRAGMA trusted_schema=OFF;");
    #[cfg(debug_assertions)]
    {
        observations.phase("settings-return");
        observations.sqlite_result(&settings);
    }
    settings?;
    observed_deadline(
        deadline,
        #[cfg(debug_assertions)]
        observations,
        #[cfg(debug_assertions)]
        Gate::AfterSettings,
    )?;
    #[cfg(debug_assertions)]
    observations.phase("timeout-restore-enter");
    let timeout = connection.busy_timeout(CONFIGURE_ALLOWANCE);
    #[cfg(debug_assertions)]
    {
        observations.phase("timeout-restore-return");
        observations.sqlite_result(&timeout);
    }
    timeout?;
    observed_deadline(
        deadline,
        #[cfg(debug_assertions)]
        observations,
        #[cfg(debug_assertions)]
        Gate::AfterRestore,
    )?;
    Ok(())
}

#[cfg(debug_assertions)]
pub(crate) mod diagnostics {
    use std::fmt::{self, Display};
    use std::time::{Duration, Instant};

    use crate::StoreResult;

    #[derive(Clone, Copy)]
    pub(super) enum Gate {
        Entry,
        AfterZero,
        BeforeAttempt,
        AfterWal,
        AfterBusy,
        BeforeSettings,
        AfterSettings,
        AfterRestore,
    }

    impl Gate {
        fn label(self) -> &'static str {
            match self {
                Self::Entry => "entry",
                Self::AfterZero => "after-zero",
                Self::BeforeAttempt => "before-attempt",
                Self::AfterWal => "after-wal",
                Self::AfterBusy => "after-busy",
                Self::BeforeSettings => "before-settings",
                Self::AfterSettings => "after-settings",
                Self::AfterRestore => "after-restore",
            }
        }
    }

    #[derive(Clone, Copy)]
    pub(super) enum Stamp {
        PrepareEnter,
        PrepareReturn,
        QueryEnter,
        RowEnter,
        RowReturn,
        QueryReturn,
        FinalizeEnter,
        FinalizeReturn,
    }

    impl Stamp {
        fn label(self) -> &'static str {
            match self {
                Self::PrepareEnter => "prepare-enter",
                Self::PrepareReturn => "prepare-return",
                Self::QueryEnter => "query-enter",
                Self::RowEnter => "row-enter",
                Self::RowReturn => "row-return",
                Self::QueryReturn => "query-return",
                Self::FinalizeEnter => "finalize-enter",
                Self::FinalizeReturn => "finalize-return",
            }
        }
    }

    #[derive(Clone, Copy)]
    enum Mode {
        Unobserved,
        Wal,
        Memory,
        Other,
    }

    impl Mode {
        fn label(self) -> &'static str {
            match self {
                Self::Unobserved => "unobserved",
                Self::Wal => "wal",
                Self::Memory => "memory",
                Self::Other => "other",
            }
        }
    }

    // Only the last attempt is retained. No strings from SQLite or native owners are stored.
    pub(crate) struct Snapshot {
        entered: Instant,
        entry_remaining_us: u64,
        gate: Option<Gate>,
        gate_us: Option<u64>,
        remaining_us: Option<u64>,
        phase: &'static str,
        phase_us: u64,
        attempts: u64,
        pub(super) autocommit: Option<bool>,
        mode: Mode,
        done: Option<bool>,
        observed_primary: Option<i32>,
        observed_extended: Option<i32>,
        stamps: [Option<u64>; 8],
    }

    fn micros(duration: Duration) -> u64 {
        u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
    }

    impl Snapshot {
        pub(super) fn new(deadline: Instant) -> Self {
            let entered = Instant::now();
            Self {
                entered,
                entry_remaining_us: micros(deadline.saturating_duration_since(entered)),
                gate: None,
                gate_us: None,
                remaining_us: None,
                phase: "entry",
                phase_us: 0,
                attempts: 0,
                autocommit: None,
                mode: Mode::Unobserved,
                done: None,
                observed_primary: None,
                observed_extended: None,
                stamps: [None; 8],
            }
        }

        pub(super) fn gate(&mut self, gate: Gate, result: &StoreResult<Duration>) {
            self.gate = Some(gate);
            self.gate_us = Some(micros(self.entered.elapsed()));
            // This is the original check's result, not a new native check after expiry.
            self.remaining_us = Some(result.as_ref().map_or(0, |remaining| micros(*remaining)));
        }

        pub(super) fn phase(&mut self, phase: &'static str) {
            self.phase = phase;
            self.phase_us = micros(self.entered.elapsed());
        }

        pub(super) fn mark(&mut self, stamp: Stamp) {
            self.phase(stamp.label());
            self.stamps[stamp as usize] = Some(self.phase_us);
        }

        pub(super) fn begin_attempt(&mut self) {
            self.attempts = self.attempts.saturating_add(1);
            self.stamps = [None; 8];
            self.mode = Mode::Unobserved;
            self.done = None;
        }

        pub(super) fn sqlite_result<T>(&mut self, result: &rusqlite::Result<T>) {
            if let Err(rusqlite::Error::SqliteFailure(code, _)) = result {
                self.observed_primary = Some(code.extended_code & 0xff);
                self.observed_extended = Some(code.extended_code);
            }
        }

        pub(super) fn query_returned(&mut self, result: &rusqlite::Result<String>) {
            if let Ok(mode) = result {
                self.done = Some(true);
                self.mode = match mode.as_str() {
                    "wal" => Mode::Wal,
                    "memory" => Mode::Memory,
                    _ => Mode::Other,
                };
            }
        }

        #[cfg(test)]
        pub(crate) fn maximum_fixture() -> Self {
            let mut snapshot = Self::new(Instant::now());
            snapshot.entry_remaining_us = u64::MAX;
            snapshot.gate = Some(Gate::BeforeSettings);
            snapshot.gate_us = Some(u64::MAX);
            snapshot.remaining_us = Some(u64::MAX);
            snapshot.phase = "timeout-restore-return";
            snapshot.phase_us = u64::MAX;
            snapshot.attempts = u64::MAX;
            snapshot.observed_primary = Some(255);
            snapshot.observed_extended = Some(i32::MIN);
            snapshot.stamps = [Some(u64::MAX); 8];
            snapshot
        }
    }

    struct Fact<T>(Option<T>);

    impl<T: Display> Display for Fact<T> {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            match &self.0 {
                Some(value) => value.fmt(formatter),
                None => formatter.write_str("unobserved"),
            }
        }
    }

    impl Display for Snapshot {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(
                formatter,
                " gate={} phase={} attempts={} entry_rem_us={} phase_us={} gate_us={} remaining_us={} autocommit={} mode={} done={} observed_primary={} observed_extended={} prepare_us={},{} query_us={},{} row_us={},{} finalize_us={},{}",
                self.gate.map_or("unobserved", Gate::label),
                self.phase,
                self.attempts,
                self.entry_remaining_us,
                self.phase_us,
                Fact(self.gate_us),
                Fact(self.remaining_us),
                Fact(self.autocommit),
                self.mode.label(),
                Fact(self.done),
                Fact(self.observed_primary),
                Fact(self.observed_extended),
                Fact(self.stamps[0]),
                Fact(self.stamps[1]),
                Fact(self.stamps[2]),
                Fact(self.stamps[5]),
                Fact(self.stamps[3]),
                Fact(self.stamps[4]),
                Fact(self.stamps[6]),
                Fact(self.stamps[7]),
            )
        }
    }

    #[cfg(test)]
    mod tests {
        use super::{Snapshot, Stamp};
        use std::time::Instant;

        #[test]
        fn unobserved_native_facts_and_done_are_not_invented_or_reused() {
            let mut snapshot = Snapshot::new(Instant::now());
            let initial = snapshot.to_string();
            assert!(initial.contains("gate=unobserved"));
            assert!(initial.contains("autocommit=unobserved mode=unobserved done=unobserved"));
            assert!(initial.contains("prepare_us=unobserved,unobserved"));
            snapshot.begin_attempt();
            snapshot.mark(Stamp::RowEnter);
            snapshot.mark(Stamp::RowReturn);
            snapshot.query_returned(&Ok("private-mode-must-not-render".to_owned()));
            let complete = snapshot.to_string();
            assert!(complete.contains("mode=other done=true"));
            assert!(!complete.contains("private-mode-must-not-render"));
            snapshot.begin_attempt();
            let next = snapshot.to_string();
            assert!(next.contains("attempts=2"));
            assert!(next.contains("mode=unobserved done=unobserved"));
            assert!(next.contains("row_us=unobserved,unobserved"));
            snapshot.attempts = u64::MAX;
            snapshot.begin_attempt();
            assert_eq!(snapshot.attempts, u64::MAX);
        }
    }
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
