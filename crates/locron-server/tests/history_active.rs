//! Additional real Router/public-lifecycle regression: 1205 cancelled + 114 queued.
//! The CLI HTTP binary separately owns all4/all7 and independent full SQL qualification.

#[path = "../../locron-cli/tests/support/history_logical.rs"]
mod logical;

use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::time::{Duration, Instant};

use axum::body::{Body, to_bytes};
use axum::http::{Method, Request};
use locron_core::command::{CompletionAction, JobDefinition};
use locron_core::filesystem::{DirectoryGuard, open_private};
use locron_core::policy::ExecutionPolicy;
use locron_core::schedule::Schedule;
use locron_core::target::{Environment, Target};
use locron_core::{DurationMicros, Timestamp};
use locron_server::{AppState, router};
use locron_store::{CreateJob, StatePaths, Store};
use logical::Cell;
use rusqlite::{Connection, OpenFlags, params};
use serde_json::{Value, json};
use tower::ServiceExt;

const JOB: &str = "00000000-0000-7000-8000-000000009001";
const CANARY: &str = "history-secret-canary";

fn checked<T, E>(result: Result<T, E>) -> T {
    result.unwrap_or_else(|_| panic!("private HTTP qualification refused"))
}
fn run_id(index: usize) -> String {
    format!("00000000-0000-7000-8000-{index:012}")
}

async fn request(
    state: &AppState,
    deadline: Instant,
    path: &str,
    body: Option<Value>,
) -> (u16, Value) {
    assert!(
        Instant::now() + Duration::from_secs(10) < deadline,
        "HTTP horizon exhausted"
    );
    let operation_deadline = (Instant::now() + Duration::from_secs(60)).min(
        deadline
            .checked_sub(Duration::from_secs(10))
            .expect("HTTP horizon exhausted"),
    );
    let response = checked(
        tokio::time::timeout_at(tokio::time::Instant::from_std(operation_deadline), async {
            let mut builder = Request::builder()
                .method(if body.is_some() {
                    Method::POST
                } else {
                    Method::GET
                })
                .uri(path)
                .header("host", "127.0.0.1:10824")
                .header("authorization", format!("token {}", state.token));
            let body = match body {
                Some(body) => {
                    builder = builder.header("content-type", "application/json");
                    Body::from(checked(serde_json::to_vec(&body)))
                }
                None => Body::empty(),
            };
            let response = checked(
                router(state.clone())
                    .oneshot(checked(builder.body(body)))
                    .await,
            );
            let status = response.status().as_u16();
            let bytes = checked(to_bytes(response.into_body(), 512 * 1024).await);
            assert!(
                !bytes
                    .windows(CANARY.len())
                    .any(|value| value == CANARY.as_bytes()),
                "HTTP reflected canary"
            );
            (status, checked(serde_json::from_slice::<Value>(&bytes)))
        })
        .await,
    );
    assert!(
        Instant::now() + Duration::from_secs(10) < deadline,
        "HTTP operation exhausted horizon"
    );
    response
}

#[tokio::test]
async fn history_active_public_lifecycle_http() {
    let deadline = Instant::now() + Duration::from_secs(180);
    let temporary = checked(tempfile::tempdir());
    let guard = checked(DirectoryGuard::private(&temporary.path().join("private")));
    let paths = StatePaths::new(guard.normalized_path().to_path_buf());
    let writer = checked(Store::open(paths.clone(), env!("CARGO_PKG_VERSION"), 1));
    let definition = JobDefinition {
        schedule: Schedule::Every {
            interval: DurationMicros::new(900_000_000),
            anchor: Timestamp::from_epoch_micros(1),
        },
        target: Target::Process {
            executable: checked(std::env::current_exe())
                .to_string_lossy()
                .into_owned(),
            args: Vec::new(),
        },
        cwd: paths.root.clone(),
        environment: Environment {
            file: None,
            path: None,
            values: BTreeMap::from([("PRIVATE_TEST".into(), CANARY.into())]),
        },
        policy: ExecutionPolicy::default(),
        completion_action: CompletionAction::Retain,
    };
    checked(definition.validate(16));
    checked(writer.create_job(&CreateJob {
        id: JOB.into(),
        name: "lifecycle-ledger".into(),
        description: None,
        tags_json: "[]".into(),
        enabled: true,
        definition_json: checked(serde_json::to_string(&definition)),
        now_us: 1,
        cursor_us: 1,
    }));
    let mut ledger = Vec::new();
    for index in 0..1205 {
        assert!(
            Instant::now() + Duration::from_secs(10) < deadline,
            "lifecycle preparation expired"
        );
        let id = run_id(index + 1);
        let time = 10000 + (index / 2) as i64;
        checked(writer.enqueue_manual(JOB, &id, time));
        checked(writer.cancel(&id, time + 1));
        ledger.push((time, id));
    }
    let mut active = Vec::new();
    for index in 0..114 {
        let id = run_id(index + 1206);
        let time = 100 + (index / 2) as i64;
        checked(writer.enqueue_manual(JOB, &id, time));
        ledger.push((time, id.clone()));
        active.push((time, id));
    }
    // Normal manual admission remains Skip/per-job1. Only this query fixture
    // promotes the prepared manual rows; it does not demonstrate 114 admissions.
    let before_seed = logical::logical_with_writer(&paths, &writer);
    let original_runs = &before_seed.tables["runs"];
    let snapshot = Cell::text(&checked(serde_json::to_string(&definition)));
    let mut prepared = before_seed.clone();
    let mut unrelated_runs = original_runs.clone();
    assert!(
        before_seed.tables["admission_state"].single("singleton", &Cell::Integer(1))[2].integer()
            == 1320,
        "manual fixture queue sequence changed"
    );
    for (index, (time, id)) in active.iter().enumerate() {
        let key = Cell::text(id);
        let queued = index == 0;
        let expected_original = vec![
            key.clone(),
            Cell::text(JOB),
            Cell::Integer(1),
            Cell::text("manual"),
            Cell::Null,
            Cell::Integer(*time),
            Cell::Integer(*time),
            Cell::Integer(checked(i64::try_from(1206 + index))),
            snapshot.clone(),
            Cell::text(if queued { "queued" } else { "skipped_overlap" }),
            if queued {
                Cell::Null
            } else {
                Cell::text("active same-job work exists")
            },
            Cell::Null,
            Cell::Null,
            Cell::Integer(0),
            Cell::Null,
            Cell::Null,
            if queued {
                Cell::Null
            } else {
                Cell::Integer(*time)
            },
        ];
        assert!(
            original_runs.single("id", &key) == &expected_original,
            "manual fixture row did not match the original policy"
        );
        let events = &before_seed.tables["events"];
        assert!(
            events.single("run_id", &key)
                == &vec![
                    Cell::Integer(checked(i64::try_from(2412 + index))),
                    Cell::Integer(*time),
                    Cell::text("manual_enqueued"),
                    Cell::text(JOB),
                    key.clone(),
                    Cell::text("{}"),
                ],
            "manual fixture event relation changed"
        );
        unrelated_runs.remove("id", &key);
        for (field, value) in [
            ("state", Cell::text("queued")),
            ("reason", Cell::Null),
            ("finished_at_us", Cell::Null),
        ] {
            prepared
                .tables
                .get_mut("runs")
                .unwrap()
                .set("id", &key, field, value);
        }
    }
    assert!(
        unrelated_runs.rows.len() == 1205
            && unrelated_runs
                .rows
                .iter()
                .all(|row| row[original_runs.column("state")] == Cell::text("cancelled")),
        "query fixture changed the public cancellation ledger"
    );
    {
        let seed_guard = checked(open_private(&paths.database, OpenOptions::new().read(true)));
        let original_id = logical::identity(&seed_guard);
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW;
        #[cfg(windows)]
        let mut connection = checked(Connection::open_with_flags_and_vfs(
            seed_guard.normalized_path(),
            flags,
            "win32-longpath",
        ));
        #[cfg(unix)]
        let mut connection = checked(Connection::open_with_flags(
            seed_guard.normalized_path(),
            flags,
        ));
        assert!(
            !checked(connection.is_readonly(rusqlite::MAIN_DB)),
            "private query seed was readonly"
        );
        checked(connection.execute_batch("PRAGMA foreign_keys=ON; PRAGMA trusted_schema=OFF;"));
        let reported = connection.path().expect("SQLite reported a file");
        let reported_guard = checked(open_private(
            std::path::Path::new(reported),
            OpenOptions::new().read(true),
        ));
        assert!(
            logical::identity(&reported_guard) == original_id,
            "seed selected another database"
        );
        let transaction = checked(connection.transaction());
        for (_, id) in &active {
            assert!(
                checked(transaction.execute(
                    "UPDATE runs SET state='queued',reason=NULL,finished_at_us=NULL WHERE id=?1 AND job_id=?2",
                    params![id, JOB],
                )) == 1,
                "private query seed row was missing or ambiguous"
            );
        }
        checked(transaction.commit());
        assert!(
            logical::identity(&seed_guard) == original_id
                && logical::identity(&reported_guard) == original_id,
            "private query seed changed database identity"
        );
        drop(connection);
        drop(reported_guard);
        drop(seed_guard);
    }
    assert!(
        prepared == logical::logical_with_writer(&paths, &writer),
        "query seed changed an unselected typed logical value"
    );
    ledger.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| right.1.cmp(&left.1)));
    active.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| right.1.cmp(&left.1)));
    let state = AppState {
        paths: paths.clone(),
        token: checked(locron_server::token::ensure(&paths)),
        bound_port: 10824,
    };
    // Complete the permanent lock probe before the physical observation boundary.
    checked(locron_store::DaemonLock::try_prove_free(&paths.daemon_lock));
    assert_eq!(checked(writer.count_runs(Some(JOB))), 1319);
    assert_eq!(checked(writer.active_runs_for_job(JOB, 0)).0, 114);
    drop(writer);
    let logical_before_requests = logical::logical(&paths);
    let physical_before_requests = logical::physical(&paths);
    for offset in [990, 1000, 1100, 1318, 1319, usize::MAX] {
        for limit in [1, 20, 100] {
            let (status, body) = request(
                &state,
                deadline,
                &format!("/api/v1/runs?job={JOB}&limit={limit}&offset={offset}"),
                None,
            )
            .await;
            assert_eq!(status, 200);
            assert_eq!(body["schema"], "locron.api/v1");
            assert_eq!(body["ok"], true);
            assert_eq!(body["data"]["total"], 1319);
            let actual: Vec<_> = body["data"]["runs"]
                .as_array()
                .expect("runs array")
                .iter()
                .map(|row| row["id"].as_str().expect("run id").to_owned())
                .collect();
            let expected: Vec<_> = ledger
                .iter()
                .skip(offset)
                .take(limit)
                .map(|(_, id)| id.clone())
                .collect();
            assert!(actual == expected, "public lifecycle deep page mismatch");
            for row in body["data"]["runs"].as_array().unwrap() {
                assert!(
                    row["attempts"].is_array()
                        && row.get("source").is_some()
                        && row.get("outcome").is_some(),
                    "observable fields missing"
                );
            }
        }
    }
    for path in [
        "/api/v1/runs?limit=0",
        "/api/v1/runs?limit=101",
        "/api/v1/runs?job=missing&q=",
    ] {
        assert_eq!(request(&state, deadline, path, None).await.0, 400);
    }
    assert_eq!(
        request(&state, deadline, "/api/v1/runs?job=missing", None)
            .await
            .0,
        404
    );
    let (status, body) = request(
        &state,
        deadline,
        &format!("/api/v1/jobs/{JOB}/run?dry-run=true"),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["decision"], "would_skip_overlap");
    assert_eq!(body["data"]["durable"], false);
    assert_eq!(body["data"]["capacity_reserved"], false);
    let (status, body) = request(&state, deadline, &format!("/api/v1/jobs/{JOB}/why"), None).await;
    assert_eq!(status, 200);
    let actual: Vec<_> = body["data"]["active_runs"]
        .as_array()
        .expect("active sample")
        .iter()
        .map(|row| row["id"].as_str().expect("run id").to_owned())
        .collect();
    assert!(
        actual
            == active
                .into_iter()
                .take(100)
                .map(|(_, id)| id)
                .collect::<Vec<_>>(),
        "lifecycle active sample mismatch"
    );
    let physical_immediately_after_requests = logical::physical(&paths);
    logical::unchanged_physical(
        &physical_before_requests,
        &physical_immediately_after_requests,
    );
    assert!(
        logical_before_requests == logical::logical(&paths),
        "public lifecycle observation changed typed logical state"
    );
    let reader = checked(Store::open_read_only(&paths.database));
    assert_eq!(checked(reader.count_runs(Some(JOB))), 1319);
    assert_eq!(checked(reader.active_runs_for_job(JOB, 0)).0, 114);
    drop(reader);
    drop(state);
    assert!(
        Instant::now() + Duration::from_secs(10) < deadline,
        "HTTP cleanup reserve exhausted"
    );
    drop(guard);
    checked(temporary.close());
    assert!(Instant::now() < deadline, "HTTP cleanup exceeded horizon");
}
