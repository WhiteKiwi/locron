//! Real adapter failures after a committed mutation must retain that outcome.

use super::*;
use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

struct ReadbackHook {
    operation: &'static str,
    action: Box<dyn FnOnce()>,
}

thread_local! {
    static HOOK: RefCell<Option<ReadbackHook>> = const { RefCell::new(None) };
}

pub(super) fn before_readback(operation: &str) {
    let hook = HOOK.with(|slot| {
        let mut hook = slot.borrow_mut();
        if hook
            .as_ref()
            .is_some_and(|hook| hook.operation == operation)
        {
            hook.take()
        } else {
            None
        }
    });
    if let Some(hook) = hook {
        (hook.action)();
    }
}

struct ResetHook;

impl Drop for ResetHook {
    fn drop(&mut self) {
        HOOK.with(|slot| slot.borrow_mut().take());
    }
}

fn inject(
    operation: &'static str,
    store: &Arc<Store>,
    fault: impl FnOnce(&Connection) + 'static,
) -> (ResetHook, Arc<AtomicUsize>) {
    let store = Arc::clone(store);
    let calls = Arc::new(AtomicUsize::new(0));
    let observed_calls = Arc::clone(&calls);
    HOOK.with(|slot| {
        assert!(slot.borrow().is_none());
        *slot.borrow_mut() = Some(ReadbackHook {
            operation,
            action: Box::new(move || {
                observed_calls.fetch_add(1, Ordering::SeqCst);
                let writer = locron_core::filesystem::open_private(
                    &store.paths().root.join("writer.lock"),
                    std::fs::OpenOptions::new().read(true).write(true),
                )
                .unwrap();
                writer
                    .try_lock()
                    .expect("result readback still owns the writer lock");
                std::fs::File::unlock(&writer).unwrap();
                let external = Connection::open(&store.paths().database).unwrap();
                external.busy_timeout(std::time::Duration::ZERO).unwrap();
                fault(&external);
            }),
        });
    });
    (ResetHook, calls)
}

struct Fixture {
    store: Arc<Store>,
    _guard: locron_core::filesystem::DirectoryGuard,
    _temporary: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let paths = StatePaths::new(temporary.path().join("private"));
        let guard = locron_core::filesystem::DirectoryGuard::private(&paths.root).unwrap();
        let store = Arc::new(Store::open(paths, "readback-qualification", 1).unwrap());
        Self {
            store,
            _guard: guard,
            _temporary: temporary,
        }
    }

    fn create() -> CreateJob {
        CreateJob {
            id: Uuid::from_u128(1).to_string(),
            name: "readback-job".into(),
            description: None,
            tags_json: "[]".into(),
            enabled: true,
            definition_json: "{}".into(),
            now_us: 2,
            cursor_us: 1,
        }
    }
}

fn assert_committed_readback(
    store: &Store,
    error: &StoreError,
    operation: &'static str,
    calls: &AtomicUsize,
) {
    let diagnostic = error
        .diagnostic()
        .expect("readback lost its persistence diagnostic");
    assert_eq!(diagnostic.operation, operation);
    assert_eq!(diagnostic.stage, "readback");
    assert_eq!(diagnostic.durable_outcome, "committed");
    assert_eq!(diagnostic.retries, 0);
    assert!(!diagnostic.retryable);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(store.last_write_observation().as_ref(), Some(diagnostic));
    assert!(!format!("{error:#?}").contains("readback-private-sentinel"));
    assert!(!format!("{error:#}").contains("readback-private-sentinel"));
}

#[test]
fn committed_create_readback_decode_failure_never_replays_creation() {
    let fixture = Fixture::new();
    let job = Fixture::create();
    let id = job.id.clone();
    let (_reset, calls) = inject("create_job", &fixture.store, move |external| {
        let committed: (i64, i64) = external.query_row(
            "SELECT current_revision,(SELECT count(*) FROM events WHERE job_id=jobs.id AND kind='job_added') FROM jobs WHERE id=?1",
            [&id], |row| Ok((row.get(0)?, row.get(1)?)),
        ).unwrap();
        assert_eq!(committed, (1, 1));
        external
            .execute(
                "UPDATE jobs SET tags_json=CAST(x'FF' AS TEXT) WHERE id=?1",
                [&id],
            )
            .unwrap();
    });
    let error = fixture.store.create_job(&job).unwrap_err();
    assert_committed_readback(&fixture.store, &error, "create_job", &calls);
    assert!(matches!(error.original_error(), StoreError::Sqlite(_)));
    let conn = fixture.store.conn().unwrap();
    let ledger: (i64, i64, i64) = conn.query_row(
        "SELECT (SELECT count(*) FROM jobs),(SELECT count(*) FROM job_revisions),(SELECT count(*) FROM events WHERE kind='job_added')",
        [], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?)),
    ).unwrap();
    assert_eq!(ledger, (1, 1, 1));
}

#[test]
fn committed_update_readback_decode_failure_preserves_exact_revision() {
    let fixture = Fixture::new();
    let initial = Fixture::create();
    fixture.store.create_job(&initial).unwrap();
    let id = initial.id.clone();
    let (_reset, calls) = inject("update_job", &fixture.store, move |external| {
        let committed: (i64, String) = external
            .query_row(
                "SELECT current_revision,name FROM jobs WHERE id=?1",
                [&id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(committed, (2, "updated-readback-job".into()));
        external
            .execute(
                "UPDATE jobs SET tags_json=CAST(x'FF' AS TEXT) WHERE id=?1",
                [&id],
            )
            .unwrap();
    });
    let error = fixture
        .store
        .update_job(&UpdateJob {
            id: initial.id.clone(),
            expected_revision: 1,
            name: "updated-readback-job".into(),
            description: None,
            tags_json: "[]".into(),
            enabled: true,
            definition_json: "{}".into(),
            now_us: 3,
            cursor_us: 2,
        })
        .unwrap_err();
    assert_committed_readback(&fixture.store, &error, "update_job", &calls);
    assert!(matches!(error.original_error(), StoreError::Sqlite(_)));
    let conn = fixture.store.conn().unwrap();
    let ledger: (i64, i64, i64) = conn.query_row(
        "SELECT current_revision,(SELECT count(*) FROM job_revisions WHERE job_id=jobs.id),(SELECT count(*) FROM events WHERE job_id=jobs.id AND kind='job_updated') FROM jobs WHERE id=?1",
        [&initial.id], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?)),
    ).unwrap();
    assert_eq!(ledger, (2, 2, 1));
}

#[test]
fn committed_manual_enqueue_readback_decode_failure_keeps_one_run() {
    let fixture = Fixture::new();
    let job = Fixture::create();
    fixture.store.create_job(&job).unwrap();
    let run_id = Uuid::from_u128(2).to_string();
    let id = run_id.clone();
    let (_reset, calls) = inject("enqueue_manual", &fixture.store, move |external| {
        let state: String = external
            .query_row("SELECT state FROM runs WHERE id=?1", [&id], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(state, "queued");
        external
            .execute(
                "UPDATE runs SET snapshot_json=CAST(x'FF' AS TEXT) WHERE id=?1",
                [&id],
            )
            .unwrap();
    });
    let error = fixture
        .store
        .enqueue_manual(&job.id, &run_id, 3)
        .unwrap_err();
    assert_committed_readback(&fixture.store, &error, "enqueue_manual", &calls);
    assert!(matches!(error.original_error(), StoreError::Sqlite(_)));
    let conn = fixture.store.conn().unwrap();
    let ledger: (i64, i64, i64) = conn.query_row(
        "SELECT (SELECT count(*) FROM runs),(SELECT count(*) FROM events WHERE kind='manual_enqueued'),(SELECT count(*) FROM attempts)",
        [], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?)),
    ).unwrap();
    assert_eq!(ledger, (1, 1, 0));
}

#[test]
fn committed_environment_readback_json_failure_keeps_the_committed_outcome() {
    let fixture = Fixture::new();
    let (_reset, calls) = inject("set_environment", &fixture.store, |external| {
        let committed: (String, i64) = external
            .query_row(
                "SELECT environment_json,updated_at_us FROM settings WHERE singleton=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        let environment: serde_json::Value = serde_json::from_str(&committed.0).unwrap();
        assert_eq!(environment["READBACK_TEST"], "readback-private-sentinel");
        assert_eq!(committed.1, 3);
        external
            .execute(
                "UPDATE settings SET environment_json=?1 WHERE singleton=1",
                ["{\"readback-private-sentinel\":}"],
            )
            .unwrap();
    });
    let error = fixture
        .store
        .set_environment("READBACK_TEST", Some("readback-private-sentinel"), 3)
        .unwrap_err();
    assert_committed_readback(&fixture.store, &error, "set_environment", &calls);
    assert!(matches!(error.original_error(), StoreError::Json(_)));
    let conn = fixture.store.conn().unwrap();
    let ledger: (i64, i64) = conn
        .query_row(
            "SELECT count(*),max(updated_at_us) FROM settings",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(ledger, (1, 3));
}

fn assert_raw_adapter(error: rusqlite::Error, kind: &str, primary: i32) {
    let extended = error.sqlite_error().unwrap().extended_code;
    assert_eq!(extended & 0xff, primary);
    let mapped = map_store_port_error(StorePortError::Store(StoreError::Sqlite(error)));
    let CoreError::PersistenceDiagnostic(diagnostic) = mapped else {
        panic!("raw SQLite adapter fallback discarded structured diagnostic");
    };
    assert_eq!(diagnostic.operation, "persistence_port");
    assert_eq!(diagnostic.stage, "adapter");
    assert_eq!(diagnostic.kind, kind);
    assert_eq!(diagnostic.sqlite_primary_code, Some(primary));
    assert_eq!(diagnostic.sqlite_extended_code, Some(extended));
    assert_eq!(diagnostic.durable_outcome, "unknown");
    assert!(!diagnostic.retryable);
    assert_eq!(diagnostic.retries, 0);
}

#[test]
fn raw_core_fallback_retains_actual_busy_and_locked_codes_and_unknown_outcome() {
    let fixture = Fixture::new();
    let holder = Connection::open(&fixture.store.paths().database).unwrap();
    holder.execute_batch("BEGIN IMMEDIATE").unwrap();
    let blocked = Connection::open(&fixture.store.paths().database).unwrap();
    blocked.busy_timeout(std::time::Duration::ZERO).unwrap();
    let busy = blocked
        .execute("UPDATE settings SET updated_at_us=4 WHERE singleton=1", [])
        .unwrap_err();
    assert_raw_adapter(busy, "busy", rusqlite::ffi::SQLITE_BUSY);
    holder.execute_batch("ROLLBACK").unwrap();

    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch("CREATE TABLE source(value INTEGER); INSERT INTO source VALUES(1),(2)")
        .unwrap();
    let mut statement = connection.prepare("SELECT value FROM source").unwrap();
    let mut rows = statement.query([]).unwrap();
    assert!(rows.next().unwrap().is_some());
    let locked = connection.execute("DROP TABLE source", []).unwrap_err();
    assert_raw_adapter(locked, "locked", rusqlite::ffi::SQLITE_LOCKED);
}

#[test]
fn both_json_adapter_paths_are_structured_and_redacted() {
    for standalone in [false, true] {
        let error = serde_json::from_str::<u64>("\"adapter-private-sentinel\"").unwrap_err();
        assert!(error.to_string().contains("adapter-private-sentinel"));
        let port_error = if standalone {
            StorePortError::Json(error)
        } else {
            StorePortError::Store(StoreError::Json(error))
        };
        let mapped = map_store_port_error(port_error);
        assert!(!format!("{mapped:#}").contains("adapter-private-sentinel"));
        assert!(!format!("{mapped:#?}").contains("adapter-private-sentinel"));
        let CoreError::PersistenceDiagnostic(diagnostic) = mapped else {
            panic!("JSON adapter fallback discarded structured diagnostic");
        };
        assert_eq!(diagnostic.operation, "persistence_port");
        assert_eq!(diagnostic.stage, "adapter");
        assert_eq!(diagnostic.kind, "serialization");
        assert_eq!(diagnostic.durable_outcome, "unknown");
        assert!(!diagnostic.retryable);
        assert_eq!(diagnostic.sqlite_primary_code, None);
        assert_eq!(diagnostic.sqlite_extended_code, None);
        assert!(
            !serde_json::to_string(&diagnostic)
                .unwrap()
                .contains("adapter-private-sentinel")
        );
    }
}
