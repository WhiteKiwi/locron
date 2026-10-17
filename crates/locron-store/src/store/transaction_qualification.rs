//! Qualification of immutable preparation and authoritative transaction rechecks.

use super::*;
use std::cell::RefCell;
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::time::{Duration, Instant};

struct PreparationBarrier {
    operation: &'static str,
    reached: SyncSender<()>,
    release: Receiver<()>,
}

thread_local! {
    static BARRIER: RefCell<Option<PreparationBarrier>> = const { RefCell::new(None) };
}

pub(super) fn prepared(operation: &str) {
    let barrier = BARRIER.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot
            .as_ref()
            .is_some_and(|barrier| barrier.operation == operation)
        {
            slot.take()
        } else {
            None
        }
    });
    if let Some(barrier) = barrier {
        barrier.reached.send(()).unwrap();
        barrier
            .release
            .recv_timeout(Duration::from_secs(10))
            .unwrap();
    }
}

fn race<T: Send>(
    operation: &'static str,
    action: impl FnOnce() -> T + Send,
    mutate: impl FnOnce(),
) -> T {
    let (reached_sender, reached_receiver) = sync_channel(1);
    let (release_sender, release_receiver) = sync_channel(1);
    std::thread::scope(|scope| {
        let thread = scope.spawn(move || {
            BARRIER.with(|cell| {
                *cell.borrow_mut() = Some(PreparationBarrier {
                    operation,
                    reached: reached_sender,
                    release: release_receiver,
                });
            });
            action()
        });
        reached_receiver
            .recv_timeout(Duration::from_secs(10))
            .unwrap();
        mutate();
        release_sender.send(()).unwrap();
        thread.join().unwrap()
    })
}

struct Fixture {
    store: Store,
    _guard: locron_core::filesystem::DirectoryGuard,
    _temporary: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let paths = StatePaths::new(temporary.path().join("private"));
        let guard = locron_core::filesystem::DirectoryGuard::private(&paths.root).unwrap();
        let store = Store::open(paths, "transaction-qualification", 1).unwrap();
        Self {
            store,
            _guard: guard,
            _temporary: temporary,
        }
    }

    fn job(&self, id: u128) -> String {
        let id = uuid(id);
        self.store
            .create_job(&CreateJob {
                id: id.clone(),
                name: id.clone(),
                description: None,
                tags_json: "[]".into(),
                enabled: true,
                definition_json: policy("allow", 64),
                now_us: 1,
                cursor_us: 1,
            })
            .unwrap();
        id
    }

    fn lifetime(&self) -> String {
        let id = uuid(900_000);
        self.store
            .begin_lifetime(&id, 2, "transaction-qualification")
            .unwrap();
        id
    }

    fn queued(&self, job: &str, id: u128) -> String {
        let id = uuid(id);
        self.store.enqueue_manual(job, &id, 2).unwrap();
        id
    }
}

fn uuid(id: u128) -> String {
    Uuid::from_u128(id).to_string()
}

fn policy(overlap: &str, concurrency: i64) -> String {
    serde_json::json!({"policy":{"overlap":overlap,"per_job_concurrency":concurrency}}).to_string()
}

fn cursor() -> CursorUpdate {
    CursorUpdate {
        expected_revision: 1,
        expected_cursor_us: 1,
        new_cursor_us: 3,
        resolve_one_time: false,
    }
}

fn scheduled(job: &str, id: u128, snapshot: String) -> NewScheduledRun {
    NewScheduledRun {
        id: uuid(id),
        job_id: job.into(),
        revision: 1,
        trigger: "scheduled".into(),
        nominal_us: i64::try_from(id).unwrap(),
        requested_at_us: 2,
        eligible_at_us: 2,
        snapshot_json: snapshot,
    }
}

#[test]
fn admission_rechecks_cancellation_and_defers_new_candidates() {
    let fixture = Fixture::new();
    let store = &fixture.store;
    let job = fixture.job(1);
    let cancelled = fixture.queued(&job, 100);
    let lifetime = fixture.lifetime();
    let result = race(
        "admit",
        || store.admit(&lifetime, 3, 2),
        || {
            store.cancel(&cancelled, 3).unwrap();
            fixture.queued(&job, 101);
        },
    )
    .unwrap();
    assert!(result.attempts.is_empty());
    assert_eq!(store.run(&cancelled).unwrap().state, "cancelled");
    let admitted = store.admit(&lifetime, 3, 2).unwrap();
    assert_eq!(admitted.attempts.len(), 1);
    assert_eq!(admitted.attempts[0].run_id, uuid(101));
    assert!(store.attempts_for_run(&cancelled).unwrap().is_empty());
}

#[test]
fn admission_rechecks_current_capacity_and_round_robin_cursor() {
    let fixture = Fixture::new();
    let store = &fixture.store;
    let first = fixture.job(1);
    let second = fixture.job(2);
    fixture.queued(&first, 100);
    fixture.queued(&second, 101);
    let lifetime = fixture.lifetime();
    let admitted = race(
        "admit",
        || store.admit(&lifetime, 3, 2),
        || {
            store.set_setting("global_concurrency", "1", 3).unwrap();
            store
                .conn()
                .unwrap()
                .execute(
                    "UPDATE admission_state SET last_admitted_job_id=?1 WHERE singleton=1",
                    [&first],
                )
                .unwrap();
        },
    )
    .unwrap();
    assert_eq!(admitted.attempts.len(), 1);
    assert_eq!(admitted.attempts[0].job_id, second);
    assert!(store.admit(&lifetime, 3, 64).unwrap().attempts.is_empty());
}

#[test]
fn admission_defers_snapshot_changes_and_uses_immutable_run_policy() {
    let fixture = Fixture::new();
    let store = &fixture.store;
    let job = fixture.job(1);
    let first = fixture.queued(&job, 100);
    let second = fixture.queued(&job, 101);
    let lifetime = fixture.lifetime();
    let admitted = race(
        "admit",
        || store.admit(&lifetime, 3, 2),
        || {
            store
                .conn()
                .unwrap()
                .execute(
                    "UPDATE runs SET snapshot_json=?2 WHERE id=?1",
                    params![first, policy("skip", 1)],
                )
                .unwrap();
        },
    )
    .unwrap();
    assert_eq!(admitted.attempts.len(), 1);
    assert_eq!(admitted.attempts[0].run_id, second);
    assert!(store.admit(&lifetime, 3, 2).unwrap().attempts.is_empty());
    assert_eq!(store.run(&first).unwrap().state, "queued");
}

#[test]
fn admission_preserves_lazy_decode_of_unexamined_candidates() {
    let fixture = Fixture::new();
    let store = &fixture.store;
    let job = fixture.job(1);
    let first = fixture.queued(&job, 100);
    let later = fixture.queued(&job, 101);
    store
        .conn()
        .unwrap()
        .execute(
            "UPDATE runs SET snapshot_json='not-json' WHERE id=?1",
            [&later],
        )
        .unwrap();
    let admitted = store.admit(&fixture.lifetime(), 3, 1).unwrap();
    assert_eq!(admitted.attempts[0].run_id, first);
    assert_eq!(store.run(&later).unwrap().state, "queued");
}

#[test]
fn concurrent_preparations_admit_each_run_once() {
    let fixture = Fixture::new();
    let store = &fixture.store;
    let job = fixture.job(1);
    fixture.queued(&job, 100);
    fixture.queued(&job, 101);
    let lifetime = fixture.lifetime();
    let start = std::sync::Barrier::new(3);
    let total = std::thread::scope(|scope| {
        let run = || {
            start.wait();
            store.admit(&lifetime, 3, 2).unwrap().attempts.len()
        };
        let first = scope.spawn(run);
        let second = scope.spawn(run);
        start.wait();
        first.join().unwrap() + second.join().unwrap()
    });
    assert_eq!(total, 2);
    for id in [100, 101] {
        let attempts = store.attempts_for_run(&uuid(id)).unwrap();
        assert_eq!(attempts.len(), 1);
        assert_eq!(attempts[0].attempt_number, 1);
    }
}

#[test]
fn manual_preparation_rejects_removed_or_changed_revision() {
    for change in ["removed", "revision", "snapshot"] {
        let fixture = Fixture::new();
        let store = &fixture.store;
        let job = fixture.job(1);
        let result = race(
            "enqueue_manual",
            || store.enqueue_manual(&job, &uuid(100), 3),
            || {
                if change == "removed" {
                    store.remove_job(&job, 3).unwrap();
                } else if change == "snapshot" {
                    store.conn().unwrap().execute(
                        "UPDATE job_revisions SET definition_json=?2 WHERE job_id=?1 AND revision=1",
                        params![job, policy("skip", 1)],
                    ).unwrap();
                } else {
                    store
                        .update_job(&UpdateJob {
                            id: job.clone(),
                            expected_revision: 1,
                            name: job.clone(),
                            description: None,
                            tags_json: "[]".into(),
                            enabled: true,
                            definition_json: policy("skip", 1),
                            now_us: 3,
                            cursor_us: 1,
                        })
                        .unwrap();
                }
            },
        );
        assert!(matches!(
            result,
            Err(StoreError::Conflict(_) | StoreError::NotFound(_))
        ));
        assert!(matches!(
            store.run(&uuid(100)),
            Err(StoreError::NotFound(_))
        ));
    }
}

#[test]
fn materialization_preparation_keeps_live_revision_and_cursor_authoritative() {
    for removed in [false, true] {
        let fixture = Fixture::new();
        let store = &fixture.store;
        let job = fixture.job(1);
        let runs = [scheduled(&job, 100, policy("allow", 64))];
        let summaries = [ReconciliationSummary {
            kind: "must_not_commit".into(),
            count: 2,
            first_nominal_us: 2,
            last_nominal_us: 3,
        }];
        let result = race(
            "materialize_with_summaries",
            || store.materialize_with_summaries(&job, cursor(), &runs, &summaries, 3),
            || {
                if removed {
                    store.remove_job(&job, 3).unwrap();
                } else {
                    store
                        .update_job(&UpdateJob {
                            id: job.clone(),
                            expected_revision: 1,
                            name: job.clone(),
                            description: None,
                            tags_json: "[]".into(),
                            enabled: true,
                            definition_json: policy("skip", 1),
                            now_us: 3,
                            cursor_us: 1,
                        })
                        .unwrap();
                }
            },
        );
        assert!(matches!(result, Err(StoreError::Conflict(_))));
        assert!(matches!(
            store.run(&uuid(100)),
            Err(StoreError::NotFound(_))
        ));
        assert!(
            store
                .events_for_job(&job)
                .unwrap()
                .iter()
                .all(|event| event.kind != "must_not_commit")
        );
    }
}

fn terminal_rows(store: &Store, jobs: &[String], count: usize) -> Vec<RunRetentionCandidate> {
    let mut conn = store.conn().unwrap();
    let tx = conn.transaction().unwrap();
    let mut rows = Vec::with_capacity(count);
    for index in 0..count {
        let candidate = RunRetentionCandidate {
            run_id: uuid(10_000 + index as u128),
            job_id: jobs[index % jobs.len()].clone(),
            finished_at_us: (index / 3) as i64,
        };
        tx.execute("INSERT INTO runs(id,job_id,revision,trigger,requested_at_us,eligible_at_us,queue_sequence,snapshot_json,state,finished_at_us) VALUES(?1,?2,1,'manual',1,1,?3,'{}','succeeded',?4)",
            params![candidate.run_id,candidate.job_id,index as i64 + 1,candidate.finished_at_us]).unwrap();
        rows.push(candidate);
    }
    tx.commit().unwrap();
    rows
}

#[test]
fn retention_thresholds_preserve_ties_nulls_zero_and_current_settings() {
    let fixture = Fixture::new();
    let store = &fixture.store;
    let job = fixture.job(1);
    store
        .set_setting("run_retention_age_us", "none", 2)
        .unwrap();
    let rows = terminal_rows(store, std::slice::from_ref(&job), 1_002);
    store.conn().unwrap().execute("INSERT INTO runs(id,job_id,revision,trigger,requested_at_us,eligible_at_us,queue_sequence,snapshot_json,state) VALUES(?1,?2,1,'manual',1,1,1003,'{}','succeeded')", params![uuid(99_999),job]).unwrap();
    for count in [0, 1, 1_000, 1_001, 10_000] {
        store
            .set_setting("run_retention_count", &count.to_string(), 3)
            .unwrap();
        let expected_count = (1_002_usize.saturating_sub(count)).clamp(2, 100);
        let candidates = store.run_retention_candidates(3, 100).unwrap();
        assert_eq!(candidates, rows[..expected_count]);
        for index in [0, 1, 2, 999, 1_000, 1_001] {
            let expected = 1_001 - index >= 1_000 || 1_001 - index >= count;
            let marked = store.mark_run_retention_pending(&rows[index], 3);
            assert_eq!(
                marked.is_ok(),
                expected,
                "threshold {count}, candidate {index}"
            );
        }
        store
            .conn()
            .unwrap()
            .execute("DELETE FROM run_retention_pending", [])
            .unwrap();
    }
    let fake_null = RunRetentionCandidate {
        run_id: uuid(99_999),
        job_id: job,
        finished_at_us: 0,
    };
    assert!(matches!(
        store.mark_run_retention_pending(&fake_null, 3),
        Err(StoreError::Conflict(_))
    ));

    let fresh = Fixture::new();
    let job = fresh.job(2);
    fresh
        .store
        .set_setting("run_retention_age_us", "none", 2)
        .unwrap();
    let rows = terminal_rows(&fresh.store, &[job], 2);
    fresh
        .store
        .set_setting("run_retention_count", "0", 2)
        .unwrap();
    assert_eq!(fresh.store.run_retention_candidates(3, 10).unwrap(), rows);
    fresh
        .store
        .set_setting("run_retention_count", "10000", 3)
        .unwrap();
    assert!(matches!(
        fresh.store.mark_run_retention_pending(&rows[0], 3),
        Err(StoreError::Conflict(_))
    ));
}

#[test]
#[ignore = "manual bundled-SQLite transaction-hold qualification; prints observations"]
fn bundled_materialization_transaction_hold() {
    for iteration in 0..20 {
        let fixture = Fixture::new();
        let job = fixture.job(1);
        let snapshot = serde_json::json!({"policy":{"overlap":"allow","per_job_concurrency":64},
            "target":{"kind":"process","argv":["/bin/echo", "x".repeat(16_384)]}})
        .to_string();
        let runs: Vec<_> = (0..128)
            .map(|index| scheduled(&job, 100 + index, snapshot.clone()))
            .collect();
        let started = Instant::now();
        let result = fixture
            .store
            .materialize_with_summaries(&job, cursor(), &runs, &[], 3)
            .unwrap();
        let elapsed = started.elapsed().as_micros();
        assert_eq!((result.inserted, result.duplicates), (128, 0));
        let conn = fixture.store.conn().unwrap();
        let ledger: (i64,i64,i64,i64) = conn.query_row(
            "SELECT count(*),sum(state='queued'),sum(state='skipped_concurrency'),sum(nominal_us) FROM runs",
            [], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).unwrap();
        assert_eq!(ledger, (128, 64, 64, 20_928));
        drop(conn);
        let hold = fixture
            .store
            .last_write_observation()
            .unwrap()
            .transaction_us;
        eprintln!(
            "bundled materialize iteration={iteration} elapsed_us={elapsed} transaction_us={hold} ledger={ledger:?}"
        );
    }
}

#[test]
fn retention_threshold_probe_uses_the_existing_bundled_sqlite_index() {
    let fixture = Fixture::new();
    let job = fixture.job(1);
    terminal_rows(&fixture.store, &[job], 1_002);
    let conn = fixture.store.conn().unwrap();
    for threshold in ["", "LIMIT 1000"] {
        let sql = format!(
            "EXPLAIN QUERY PLAN SELECT count(*) FROM (
             SELECT 1 FROM runs newer
             WHERE newer.state IN ('succeeded','failed','timed_out','cancelled','skipped_overlap','skipped_concurrency','interrupted_unknown')
               AND newer.finished_at_us IS NOT NULL
               AND (newer.finished_at_us > ?1 OR (newer.finished_at_us=?1 AND newer.id > ?2))
             {threshold})"
        );
        let plans: Vec<String> = conn
            .prepare(&sql)
            .unwrap()
            .query_map(params![0, uuid(10_000)], |row| row.get(3))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert!(
            plans
                .iter()
                .any(|detail| detail.contains("COVERING INDEX runs_retention"))
        );
        eprintln!("bundled retention threshold={threshold:?} plan={plans:?}");
    }
}

#[test]
#[ignore = "manual bundled-SQLite transaction-hold qualification; prints observations"]
fn bundled_retention_transaction_hold() {
    let fixture = Fixture::new();
    let jobs: Vec<_> = (1..=100).map(|id| fixture.job(id)).collect();
    let rows = terminal_rows(&fixture.store, &jobs, 60_000);
    fixture
        .store
        .set_setting("run_retention_age_us", "none", 2)
        .unwrap();
    fixture
        .store
        .set_setting("run_retention_count", "1000", 2)
        .unwrap();
    let sqlite: String = fixture
        .store
        .conn()
        .unwrap()
        .query_row("SELECT sqlite_version()", [], |row| row.get(0))
        .unwrap();
    for iteration in 0..20 {
        let started = Instant::now();
        fixture
            .store
            .mark_run_retention_pending(&rows[0], 3)
            .unwrap();
        let elapsed = started.elapsed().as_micros();
        let hold = fixture
            .store
            .last_write_observation()
            .unwrap()
            .transaction_us;
        assert_eq!(
            fixture.store.pending_run_retention(100).unwrap(),
            [rows[0].clone()]
        );
        let conn = fixture.store.conn().unwrap();
        let count: i64 = conn
            .query_row("SELECT count(*) FROM runs", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 60_000);
        conn.execute("DELETE FROM run_retention_pending", [])
            .unwrap();
        eprintln!(
            "bundled retention sqlite={sqlite} iteration={iteration} elapsed_us={elapsed} transaction_us={hold} rows={count} pending=1"
        );
    }
}
