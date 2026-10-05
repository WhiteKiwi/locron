//! Private, instance-local qualification; no hook or metric exists in production.

use std::mem::size_of;
use std::sync::mpsc::{Receiver, SyncSender};
use std::time::{Duration, Instant};

use super::{CreateJob, RunRecord, StatePaths, Store, StoreError, StoreResult};
use rusqlite::params;

trait PrivateResult<T> {
    fn fixed(self) -> T;
}

impl<T, E> PrivateResult<T> for Result<T, E> {
    fn fixed(self) -> T {
        match self {
            Ok(value) => value,
            Err(_) => panic!("private Store qualification refused"),
        }
    }
}

#[derive(Default)]
pub(super) struct Observation {
    barrier: Option<CountBarrier>,
    representation: Representation,
}

struct CountBarrier {
    reached: SyncSender<()>,
    release: Receiver<()>,
    deadline: Instant,
}

#[derive(Clone, Default)]
struct Representation {
    rows: usize,
    lowercase_calls: usize,
    max_vec_capacity: usize,
    max_retained: usize,
    max_current: usize,
    max_job_name: usize,
    normalized: usize,
    max_lowercase: usize,
    max_observed: usize,
    current_site: usize,
}

pub(super) fn after_count(store: &Store) -> StoreResult<()> {
    let barrier = store
        .history_observation
        .lock()
        .map_err(|_| StoreError::Conflict("qualification observer poisoned".into()))?
        .barrier
        .take();
    if let Some(barrier) = barrier {
        barrier
            .reached
            .send(())
            .map_err(|_| StoreError::Conflict("qualification peer closed".into()))?;
        let remaining = barrier.deadline.saturating_duration_since(Instant::now());
        barrier
            .release
            .recv_timeout(remaining)
            .map_err(|_| StoreError::Conflict("qualification release unavailable".into()))?;
    }
    Ok(())
}

fn strings(run: &RunRecord) -> usize {
    run.id.capacity()
        + run.job_id.capacity()
        + run.trigger.capacity()
        + run.state.capacity()
        + run.snapshot_json.capacity()
        + run.reason.as_ref().map_or(0, String::capacity)
}

fn retained(runs: &Vec<RunRecord>) -> usize {
    runs.capacity() * size_of::<RunRecord>() + runs.iter().map(strings).sum::<usize>()
}

pub(super) fn start_search(store: &Store, normalized: &String) {
    store.history_observation.lock().fixed().representation = Representation {
        normalized: size_of::<String>() + normalized.capacity(),
        ..Representation::default()
    };
}

pub(super) fn observe_row(
    store: &Store,
    runs: &Vec<RunRecord>,
    run: &RunRecord,
    job_name: &String,
    normalized: &String,
) {
    let mut observer = store.history_observation.lock().fixed();
    let metric = &mut observer.representation;
    let page = retained(runs);
    let current = size_of::<RunRecord>() + strings(run);
    let job = size_of::<String>() + job_name.capacity();
    metric.rows += 1;
    metric.max_vec_capacity = metric.max_vec_capacity.max(runs.capacity());
    metric.max_retained = metric.max_retained.max(page);
    metric.max_current = metric.max_current.max(current);
    metric.max_job_name = metric.max_job_name.max(job);
    metric.current_site = page + current + job + size_of::<String>() + normalized.capacity();
    metric.max_observed = metric.max_observed.max(metric.current_site);
}

pub(super) fn observe_lowercase(store: &Store, actual: String) -> String {
    let mut observer = store.history_observation.lock().fixed();
    let metric = &mut observer.representation;
    let bytes = size_of::<String>() + actual.capacity();
    metric.lowercase_calls += 1;
    metric.max_lowercase = metric.max_lowercase.max(bytes);
    metric.max_observed = metric.max_observed.max(metric.current_site + bytes);
    actual
}

pub(super) fn observe_retained(store: &Store, runs: &Vec<RunRecord>) {
    let mut observer = store.history_observation.lock().fixed();
    let metric = &mut observer.representation;
    metric.max_vec_capacity = metric.max_vec_capacity.max(runs.capacity());
    metric.max_retained = metric.max_retained.max(retained(runs));
}

const A: &str = "00000000-0000-7000-8000-000000009001";
const B: &str = "00000000-0000-7000-8000-000000009002";
const C: &str = "00000000-0000-7000-8000-000000009003";
const UNKNOWN: &str = "00000000-0000-7000-8000-000000009099";
const ACTIVE: [&str; 4] = ["queued", "starting", "running", "retry_wait"];
const TERMINAL: [&str; 7] = [
    "succeeded",
    "failed",
    "timed_out",
    "cancelled",
    "skipped_overlap",
    "skipped_concurrency",
    "interrupted_unknown",
];

struct LedgerRow {
    id: String,
    job: &'static str,
    time: i64,
    state: &'static str,
}

struct Fixture {
    store: Store,
    ledger: Vec<LedgerRow>,
    deadline: Instant,
    _guard: locron_core::filesystem::DirectoryGuard,
    _temporary: tempfile::TempDir,
}

impl Fixture {
    fn new(terminal_count: usize, active_count: usize) -> Self {
        let deadline = Instant::now() + Duration::from_secs(180);
        let temporary = tempfile::tempdir().fixed();
        let guard =
            locron_core::filesystem::DirectoryGuard::private(&temporary.path().join("private"))
                .fixed();
        let store = Store::open(StatePaths::new(guard.normalized_path().into()), "test", 1).fixed();
        for (id, name) in [(A, "Ledger Ü %_"), (B, "distinct"), (C, "removed")] {
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
                .fixed();
        }
        let mut ledger = Vec::new();
        for index in 0..terminal_count {
            ledger.push(LedgerRow {
                id: run_id(index + 1),
                job: A,
                time: if index < 150 {
                    10000 + (index / 2) as i64
                } else {
                    5000 + ((index - 150) / 2) as i64
                },
                state: TERMINAL[index % TERMINAL.len()],
            });
        }
        for index in 0..active_count {
            ledger.push(LedgerRow {
                id: run_id(terminal_count + index + 1),
                job: A,
                time: 100 + (index / 2) as i64,
                state: ACTIVE[index % ACTIVE.len()],
            });
        }
        for (index, job) in [(7001, B), (7002, B), (7003, C)] {
            ledger.push(LedgerRow {
                id: run_id(index),
                job,
                time: 50,
                state: "succeeded",
            });
        }
        {
            let mut connection = store.conn().fixed();
            let transaction = connection.transaction().fixed();
            for (sequence, row) in ledger.iter().enumerate() {
                transaction.execute(
                    "INSERT INTO runs(id,job_id,revision,trigger,requested_at_us,eligible_at_us,queue_sequence,snapshot_json,state,reason,finished_at_us) VALUES(?1,?2,1,'manual',?3,?3,?4,'{}',?5,?6,?7)",
                    params![row.id, row.job, row.time, (sequence + 1) as i64, row.state,
                        (row.state == "running").then_some("termination_unconfirmed"),
                        TERMINAL.contains(&row.state).then_some(row.time + 1)],
                ).fixed();
            }
            transaction.commit().fixed();
        }
        store.remove_job("removed", 2).fixed();
        let fixture = Self {
            store,
            ledger,
            deadline,
            _guard: guard,
            _temporary: temporary,
        };
        fixture.check();
        fixture
    }

    fn check(&self) {
        assert!(
            Instant::now() + Duration::from_secs(10) < self.deadline,
            "qualification exhausted its preparation/operation horizon"
        );
    }

    fn finish(self) {
        let Self {
            store,
            ledger,
            deadline,
            _guard,
            _temporary,
        } = self;
        drop(store);
        drop(ledger);
        drop(_guard);
        _temporary.close().fixed();
        assert!(
            Instant::now() < deadline,
            "qualification teardown exceeded one horizon"
        );
    }

    fn expected(
        &self,
        job: Option<&str>,
        active: bool,
        offset: usize,
        limit: usize,
    ) -> Vec<String> {
        let mut rows: Vec<_> = self
            .ledger
            .iter()
            .filter(|row| {
                job.is_none_or(|id| row.job == id) && (!active || ACTIVE.contains(&row.state))
            })
            .collect();
        rows.sort_by(|left, right| {
            right
                .time
                .cmp(&left.time)
                .then_with(|| right.id.cmp(&left.id))
        });
        rows.into_iter()
            .skip(offset)
            .take(limit.min(100))
            .map(|row| row.id.clone())
            .collect()
    }
}

fn run_id(index: usize) -> String {
    format!("00000000-0000-7000-8000-{index:012}")
}

fn ids(runs: &[RunRecord]) -> Vec<String> {
    runs.iter().map(|run| run.id.clone()).collect()
}

fn checked<T>(fixture: &Fixture, operation: impl FnOnce() -> T) -> T {
    fixture.check();
    let started = Instant::now();
    let value = operation();
    assert!(
        started.elapsed() <= Duration::from_secs(60),
        "qualification operation exceeded 60s"
    );
    fixture.check();
    value
}

#[test]
fn history_qualification_deep_pages_and_active_ledger() {
    let fixture = Fixture::new(1205, 114);
    for reference in [None, Some(A), Some("Ledger Ü %_")] {
        let job = reference.map(|_| A);
        let total = if job.is_some() { 1319 } else { 1322 };
        for offset in [
            0,
            990,
            1000,
            1100,
            1310,
            total - 1,
            total,
            total + 1,
            usize::MAX,
        ] {
            for limit in [0, 20, 100, 101] {
                let page = checked(&fixture, || {
                    fixture.store.history_page(reference, limit, offset).fixed()
                });
                assert_eq!(page.total, total);
                assert!(
                    ids(&page.runs) == fixture.expected(job, false, offset, limit),
                    "deep page ledger mismatch"
                );
            }
        }
    }
    assert_eq!(
        fixture.store.history(Some(A), usize::MAX).fixed().len(),
        1000
    );
    for reference in [UNKNOWN, C, "removed", "missing"] {
        assert!(matches!(
            fixture.store.history_page(Some(reference), 20, 0),
            Err(StoreError::NotFound(_))
        ));
    }
    for limit in [0, 1, 20, 100, 101, usize::MAX] {
        let (count, sample) = checked(&fixture, || {
            fixture.store.active_runs_for_job(A, limit).fixed()
        });
        assert_eq!(count, 114);
        assert!(
            ids(&sample) == fixture.expected(Some(A), true, 0, limit),
            "active sample ledger mismatch"
        );
    }
    assert_eq!(fixture.store.active_runs_for_job(UNKNOWN, 100).fixed().0, 0);
    assert!(fixture.store.active_runs_for_job("not-a-uuid", 0).is_err());
    fixture.finish();
}

#[test]
fn history_qualification_search_saturation_and_empty_pages() {
    let fixture = Fixture::new(1205, 114);
    for offset in [0, 990, 1000, 1100, 1319, usize::MAX] {
        for limit in [0, 20, 100, 101] {
            let searched = checked(&fixture, || {
                fixture
                    .store
                    .search_history("  LEDGER ü %_  ", limit, offset)
                    .fixed()
            });
            assert_eq!(searched.total, 1319);
            assert!(
                ids(&searched.runs) == fixture.expected(Some(A), false, offset, limit),
                "literal search ledger mismatch"
            );
            let empty = fixture.store.search_history(" \t ", limit, offset).fixed();
            assert_eq!(empty.total, 1322);
            assert!(
                ids(&empty.runs) == fixture.expected(None, false, offset, limit),
                "empty search ledger mismatch"
            );
        }
    }
    let last = fixture.store.search_history(&run_id(1319), 100, 0).fixed();
    assert_eq!(last.total, 1);
    assert!(
        ids(&last.runs) == vec![run_id(1319)],
        "late id was not searched"
    );
    fixture.finish();
}

#[test]
fn history_qualification_late_decode_errors_propagate() {
    let fixture = Fixture::new(1205, 114);
    {
        let connection = fixture.store.conn().fixed();
        connection
            .execute(
                "UPDATE runs SET snapshot_json=CAST(x'FF' AS TEXT) WHERE id=?1",
                [run_id(7003)],
            )
            .fixed();
        let observed: (String, String) = connection
            .query_row(
                "SELECT typeof(snapshot_json),hex(snapshot_json) FROM runs WHERE id=?1",
                [run_id(7003)],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .fixed();
        assert_eq!(observed, ("text".into(), "FF".into()));
    }
    for (limit, offset) in [(20, 0), (0, 0), (100, usize::MAX)] {
        assert!(
            checked(&fixture, || fixture
                .store
                .search_history("ledger", limit, offset))
            .is_err(),
            "late real TEXT decode failure was hidden"
        );
    }
    assert!(
        fixture
            .store
            .history_page(None, 0, 0)
            .fixed()
            .runs
            .is_empty()
    );
    fixture
        .store
        .conn()
        .fixed()
        .execute(
            "UPDATE runs SET snapshot_json=CAST(x'FF' AS TEXT) WHERE id=?1",
            [run_id(1319)],
        )
        .fixed();
    assert_eq!(fixture.store.active_runs_for_job(A, 0).fixed().0, 114);
    assert!(fixture.store.active_runs_for_job(A, 100).is_err());
    fixture.finish();
}

fn snapshot_barrier(active: bool) {
    let fixture = Fixture::new(1205, 114);
    for suffix in ["-wal", "-shm"] {
        let path = fixture.store.paths().root.join(format!("state.db{suffix}"));
        assert!(
            locron_core::filesystem::is_private(&path, false).fixed(),
            "WAL mode was not admitted"
        );
    }
    let reader = Store::open_read_only(&fixture.store.paths().database).fixed();
    assert!(reader.conn().fixed().is_readonly(rusqlite::MAIN_DB).fixed());
    let (reached_sender, reached_receiver) = std::sync::mpsc::sync_channel(1);
    let (release_sender, release_receiver) = std::sync::mpsc::sync_channel(1);
    let deadline =
        (Instant::now() + Duration::from_secs(60)).min(fixture.deadline - Duration::from_secs(10));
    reader.history_observation.lock().fixed().barrier = Some(CountBarrier {
        reached: reached_sender,
        release: release_receiver,
        deadline,
    });
    std::thread::scope(|scope| {
        let handle = scope.spawn(move || {
            if active {
                reader
                    .active_runs_for_job(A, 100)
                    .map(|(total, runs)| super::RunHistoryPage { total, runs })
            } else {
                reader.history_page(Some(A), 100, 0)
            }
        });
        reached_receiver
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .fixed();
        let new_id = run_id(8000);
        fixture.store.conn().fixed().execute(
            "INSERT INTO runs(id,job_id,revision,trigger,requested_at_us,eligible_at_us,queue_sequence,snapshot_json,state) VALUES(?1,?2,1,'manual',999999,999999,999999,'{}','queued')",
            params![new_id, A],
        ).fixed();
        release_sender.send(()).fixed();
        let page = handle.join().fixed().fixed();
        assert_eq!(page.total, if active { 114 } else { 1319 });
        assert!(
            ids(&page.runs) == fixture.expected(Some(A), active, 0, 100),
            "count/sample used different WAL snapshots"
        );
    });
    assert_eq!(
        fixture.store.history_page(Some(A), 1, 0).fixed().total,
        1320
    );
    assert_eq!(fixture.store.active_runs_for_job(A, 1).fixed().0, 115);
    fixture.check();
    fixture.finish();
}

#[test]
fn history_qualification_count_and_page_share_wal_snapshot() {
    snapshot_barrier(false);
}

#[test]
fn history_qualification_count_and_sample_share_wal_snapshot() {
    snapshot_barrier(true);
}

#[test]
fn history_qualification_retained_representation_is_independent_of_scan_length() {
    let mut metrics = Vec::new(); // Two measurements, never one metric per scanned row.
    for terminal_count in [150, 1205] {
        let fixture = Fixture::new(terminal_count, 114);
        let page = checked(&fixture, || {
            fixture.store.search_history("ledger", 20, 0).fixed()
        });
        assert_eq!(page.total, terminal_count + 114);
        assert!(
            ids(&page.runs) == fixture.expected(Some(A), false, 0, 20),
            "measured result ledger mismatch"
        );
        let metric = fixture
            .store
            .history_observation
            .lock()
            .fixed()
            .representation
            .clone();
        assert_eq!(metric.rows, terminal_count + 117);
        assert!(metric.lowercase_calls >= metric.rows);
        assert_eq!(metric.max_vec_capacity, page.runs.capacity());
        assert_eq!(metric.max_retained, retained(&page.runs));
        assert!(metric.max_current > size_of::<RunRecord>());
        assert!(metric.max_job_name > size_of::<String>());
        assert!(metric.normalized > size_of::<String>());
        assert!(metric.max_lowercase > size_of::<String>());
        assert!(
            metric.max_observed
                <= metric.max_retained
                    + metric.max_current
                    + metric.max_job_name
                    + metric.normalized
                    + metric.max_lowercase
        );
        metrics.push(metric);
        let exact = fixture.store.search_history(&run_id(1), 20, 0).fixed();
        assert_eq!(exact.total, 1);
        assert!(
            ids(&exact.runs) == vec![run_id(1)],
            "id short-circuit control mismatch"
        );
        let id_metric = fixture
            .store
            .history_observation
            .lock()
            .fixed()
            .representation
            .clone();
        assert_eq!(id_metric.lowercase_calls, id_metric.rows * 2 - 1);
        fixture.finish();
    }
    assert!(metrics[1].rows > metrics[0].rows * 4);
    assert_eq!(metrics[0].max_vec_capacity, metrics[1].max_vec_capacity);
    assert_eq!(metrics[0].max_retained, metrics[1].max_retained);
    assert_eq!(metrics[0].max_current, metrics[1].max_current);
    assert_eq!(metrics[0].max_job_name, metrics[1].max_job_name);
    assert_eq!(metrics[0].normalized, metrics[1].normalized);
    assert_eq!(metrics[0].max_lowercase, metrics[1].max_lowercase);
    assert_eq!(metrics[0].max_observed, metrics[1].max_observed);
}
