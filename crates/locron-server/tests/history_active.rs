//! Additional real Router/public-lifecycle regression: 1205 cancelled + 114 queued.
//! The CLI HTTP binary separately owns all4/all7 and independent full SQL qualification.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use axum::body::{Body, to_bytes};
use axum::http::{Method, Request};
use locron_core::command::{CompletionAction, JobDefinition};
use locron_core::filesystem::DirectoryGuard;
use locron_core::policy::ExecutionPolicy;
use locron_core::schedule::Schedule;
use locron_core::target::{Environment, Target};
use locron_core::{DurationMicros, Timestamp};
use locron_server::{AppState, router};
use locron_store::{CreateJob, StatePaths, Store};
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
    ledger.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| right.1.cmp(&left.1)));
    active.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| right.1.cmp(&left.1)));
    let state = AppState {
        paths: paths.clone(),
        token: checked(locron_server::token::ensure(&paths)),
        bound_port: 10824,
    };
    assert_eq!(checked(writer.count_runs(Some(JOB))), 1319);
    assert_eq!(checked(writer.active_runs_for_job(JOB, 0)).0, 114);
    drop(writer);
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
