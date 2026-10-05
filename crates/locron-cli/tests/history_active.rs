//! Real CLI and HTTP fixed-ledger qualification. No target or daemon is executed.

#[path = "support/history_logical.rs"]
mod logical;

use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use locron_core::command::{CompletionAction, JobDefinition};
use locron_core::filesystem::{DirectoryGuard, create_private_new, open_private};
use locron_core::policy::{ExecutionPolicy, OverlapPolicy};
use locron_core::schedule::Schedule;
use locron_core::target::{Environment, Target};
use locron_core::{DurationMicros, Timestamp};
use locron_store::{CreateJob, StatePaths, Store};
use logical::{Cell, Logical, checked};
use rusqlite::{Connection, OpenFlags, params};
use serde_json::{Value, json};

const A: &str = "00000000-0000-7000-8000-000000009001";
const B: &str = "00000000-0000-7000-8000-000000009002";
const C: &str = "00000000-0000-7000-8000-000000009003";
const CANARY: &str = "history-secret-canary";
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

struct Row {
    id: String,
    job: &'static str,
    time: i64,
    state: &'static str,
}

struct Fixture {
    paths: StatePaths,
    ledger: Vec<Row>,
    before: Logical,
    deadline: Instant,
    token: String,
    capture: DirectoryGuard,
    _guard: DirectoryGuard,
    temporary: tempfile::TempDir,
}

fn definition(root: &std::path::Path, overlap: OverlapPolicy) -> JobDefinition {
    let policy = ExecutionPolicy {
        overlap,
        per_job_concurrency: if overlap == OverlapPolicy::Allow {
            16
        } else {
            1
        },
        ..ExecutionPolicy::default()
    };
    let value = JobDefinition {
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
        cwd: root.into(),
        environment: Environment {
            file: None,
            values: BTreeMap::from([("PRIVATE_TEST".into(), CANARY.into())]),
            path: None,
        },
        policy,
        completion_action: CompletionAction::Retain,
    };
    checked(value.validate(16));
    value
}

impl Fixture {
    fn new(overlap: OverlapPolicy, active: bool) -> Self {
        let deadline = Instant::now() + Duration::from_secs(180);
        let temporary = checked(tempfile::tempdir());
        let guard = checked(DirectoryGuard::private(&temporary.path().join("private")));
        let capture = checked(DirectoryGuard::private(&temporary.path().join("capture")));
        let paths = StatePaths::new(guard.normalized_path().to_path_buf());
        let writer = checked(Store::open(paths.clone(), env!("CARGO_PKG_VERSION"), 1));
        let definition_json = checked(serde_json::to_string(&definition(&paths.root, overlap)));
        for (id, name) in [(A, "Ledger Ü %_"), (B, "distinct"), (C, "removed")] {
            checked(writer.create_job(&CreateJob {
                id: id.into(),
                name: name.into(),
                description: None,
                tags_json: "[]".into(),
                enabled: true,
                definition_json: definition_json.clone(),
                now_us: 1,
                cursor_us: 1,
            }));
        }
        let mut ledger = Vec::new();
        for index in 0..1205 {
            ledger.push(Row {
                id: run_id(index + 1),
                job: A,
                time: if index < 150 {
                    10000 + (index / 2) as i64
                } else {
                    5000 + ((index - 150) / 2) as i64
                },
                state: TERMINAL[index % 7],
            });
        }
        if active {
            for index in 0..114 {
                ledger.push(Row {
                    id: run_id(index + 1206),
                    job: A,
                    time: 100 + (index / 2) as i64,
                    state: ACTIVE[index % 4],
                });
            }
        }
        for (index, job) in [(7001, B), (7002, B), (7003, C)] {
            ledger.push(Row {
                id: run_id(index),
                job,
                time: 50,
                state: "succeeded",
            });
        }
        {
            let guard = checked(open_private(&paths.database, OpenOptions::new().read(true)));
            let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW;
            #[cfg(windows)]
            let mut connection = checked(Connection::open_with_flags_and_vfs(
                guard.normalized_path(),
                flags,
                "win32-longpath",
            ));
            #[cfg(unix)]
            let mut connection =
                checked(Connection::open_with_flags(guard.normalized_path(), flags));
            assert!(
                !checked(connection.is_readonly(rusqlite::MAIN_DB)),
                "seed writer unexpectedly readonly"
            );
            let transaction = checked(connection.transaction());
            for (sequence, row) in ledger.iter().enumerate() {
                checked(transaction.execute("INSERT INTO runs(id,job_id,revision,trigger,requested_at_us,eligible_at_us,queue_sequence,snapshot_json,state,reason,finished_at_us) VALUES(?1,?2,1,'manual',?3,?3,?4,?5,?6,?7,?8)",
                    params![row.id, row.job, row.time, (sequence + 1) as i64, definition_json, row.state, (row.state == "running").then_some("termination_unconfirmed"), TERMINAL.contains(&row.state).then_some(row.time + 1)]));
            }
            // Independent enrichment control on a real deep-page failed row.
            let deep = ordered(&ledger, Some(A), false)
                .into_iter()
                .skip(1000)
                .take(20)
                .find(|row| row.state == "failed")
                .unwrap();
            checked(transaction.execute("INSERT INTO scheduler_lifetimes(id,pid,binary_version,started_at_us,heartbeat_at_us) VALUES('00000000-0000-7000-8000-000000009050',1,'fixture',1,1)", []));
            checked(transaction.execute("INSERT INTO attempts(run_id,attempt_number,lifetime_id,state,started_at_us,running_at_us,finished_at_us,duration_us) VALUES(?1,1,'00000000-0000-7000-8000-000000009050','failed',?2,?2,?3,1)", params![deep.id, deep.time, deep.time + 1]));
            checked(transaction.commit());
            drop(connection);
            drop(guard);
        }
        checked(writer.remove_job("removed", 2));
        let token = checked(locron_server::token::ensure(&paths));
        let output_guard = checked(DirectoryGuard::private(
            &paths.outputs.join("qualification"),
        ));
        for path in [
            paths.root.join("foreign.keep"),
            output_guard.normalized_path().join("sentinel.log"),
        ] {
            let mut file = checked(create_private_new(&path));
            checked(std::io::Write::write_all(
                &mut *file,
                b"test-owned physical canary",
            ));
            checked(file.sync_all());
        }
        drop(output_guard);
        let before = logical::logical(&paths); // Readers close while the normal setup writer still lives.
        drop(writer);
        let mut calibration = before.clone();
        calibration.tables.get_mut("runs").unwrap().set(
            "id",
            &Cell::text(&run_id(1)),
            "state",
            Cell::text("oracle-calibration"),
        );
        assert!(
            calibration != before,
            "logical oracle missed an altered typed value"
        );
        calibration = before.clone();
        calibration
            .tables
            .get_mut("runs")
            .unwrap()
            .remove("id", &Cell::text(&run_id(1)));
        assert!(
            calibration != before,
            "logical oracle missed row multiplicity"
        );
        assert!(
            before.tables["admission_state"].single("singleton", &Cell::Integer(1))[2].integer()
                > 0
        );
        let fixture = Self {
            paths,
            ledger,
            before,
            deadline,
            token,
            capture,
            _guard: guard,
            temporary,
        };
        fixture.check();
        fixture
    }

    fn check(&self) {
        assert!(
            Instant::now() + Duration::from_secs(10) < self.deadline,
            "qualification horizon exhausted"
        );
    }
    fn expected(
        &self,
        job: Option<&str>,
        active: bool,
        offset: usize,
        limit: usize,
    ) -> Vec<String> {
        ordered(&self.ledger, job, active)
            .into_iter()
            .skip(offset)
            .take(limit.min(100))
            .map(|row| row.id.clone())
            .collect()
    }
    fn unchanged(&self) {
        assert!(
            self.before == logical::logical(&self.paths),
            "public observation changed logical durable state"
        );
        self.check();
    }
    fn finish(self) {
        let deadline = self.deadline;
        drop(self.capture);
        drop(self._guard);
        checked(self.temporary.close());
        assert!(
            Instant::now() < deadline,
            "qualification cleanup exceeded horizon"
        );
    }
}

fn ordered<'a>(ledger: &'a [Row], job: Option<&str>, active: bool) -> Vec<&'a Row> {
    let mut rows: Vec<_> = ledger
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
    rows
}
fn run_id(index: usize) -> String {
    format!("00000000-0000-7000-8000-{index:012}")
}
fn ids(rows: &Value) -> Vec<String> {
    rows.as_array()
        .expect("rows array")
        .iter()
        .map(|row| row["id"].as_str().expect("row id").to_owned())
        .collect()
}

struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn cli(fixture: &Fixture, args: &[&str], capture_index: usize) -> Value {
    fixture.check();
    let output_path = fixture
        .capture
        .normalized_path()
        .join(format!("{capture_index}.out"));
    let error_path = fixture
        .capture
        .normalized_path()
        .join(format!("{capture_index}.err"));
    let output = checked(create_private_new(&output_path));
    let errors = checked(create_private_new(&error_path));
    let output_id = logical::identity(&output);
    let error_id = logical::identity(&errors);
    let deadline = (Instant::now() + Duration::from_secs(60)).min(
        fixture
            .deadline
            .checked_sub(Duration::from_secs(10))
            .expect("qualification horizon exhausted"),
    );
    let mut child = OwnedChild(checked(
        Command::new(assert_cmd::cargo::cargo_bin!("locron"))
            .arg("--state-dir")
            .arg(&fixture.paths.root)
            .arg("--json")
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::from(checked(output.try_clone())))
            .stderr(Stdio::from(checked(errors.try_clone())))
            .spawn(),
    ));
    let status = loop {
        if let Some(status) = checked(child.0.try_wait()) {
            break status;
        }
        assert!(Instant::now() < deadline, "owned CLI operation expired");
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(status.success(), "owned CLI operation refused");
    drop(child);
    drop(output);
    drop(errors);
    let mut output = checked(open_private(&output_path, OpenOptions::new().read(true)));
    let mut errors = checked(open_private(&error_path, OpenOptions::new().read(true)));
    assert!(
        logical::identity(&output) == output_id && logical::identity(&errors) == error_id,
        "capture identity changed"
    );
    let mut bytes = Vec::new();
    checked((&mut *output).take(512 * 1024 + 1).read_to_end(&mut bytes));
    let mut diagnostic = Vec::new();
    checked(
        (&mut *errors)
            .take(64 * 1024 + 1)
            .read_to_end(&mut diagnostic),
    );
    assert!(
        bytes.len() <= 512 * 1024 && diagnostic.len() <= 64 * 1024,
        "capture cap exceeded"
    );
    assert!(
        !bytes
            .windows(CANARY.len())
            .any(|part| part == CANARY.as_bytes())
            && !diagnostic
                .windows(CANARY.len())
                .any(|part| part == CANARY.as_bytes()),
        "capture reflected secret canary"
    );
    let value: Value = checked(serde_json::from_slice(&bytes));
    assert_eq!(value["ok"], true);
    fixture.check();
    value["data"].clone()
}

#[test]
fn history_active_cli_dry_why_explain() {
    for (index, policy) in [
        OverlapPolicy::Skip,
        OverlapPolicy::Replace,
        OverlapPolicy::Allow,
    ]
    .into_iter()
    .enumerate()
    {
        for active in [false, true] {
            let fixture = Fixture::new(policy, active);
            let physical_before = logical::physical(&fixture.paths);
            let run = cli(&fixture, &["run", A, "--dry-run"], index * 10);
            let physical_after = logical::physical(&fixture.paths);
            logical::unchanged_physical(&physical_before, &physical_after);
            assert_eq!(run["dry_run"], true);
            assert_eq!(run["durable"], false);
            assert_eq!(run["capacity_reserved"], false);
            let decision = if !active {
                "eligible"
            } else {
                match policy {
                    OverlapPolicy::Skip => "would_skip_overlap",
                    OverlapPolicy::Replace => "would_replace",
                    OverlapPolicy::Allow => "eligible_subject_to_capacity",
                }
            };
            assert_eq!(run["decision"], decision);
            fixture.unchanged();
            let why = cli(&fixture, &["why", A], index * 10 + 1);
            assert!(
                ids(&why["active_runs"]) == fixture.expected(Some(A), true, 0, 100),
                "CLI active sample mismatch"
            );
            fixture.unchanged();
            let explain = cli(&fixture, &["explain", A], index * 10 + 2);
            assert_eq!(
                explain["current_status"]["active_runs"],
                if active { 114 } else { 0 }
            );
            fixture.unchanged();
            fixture.finish();
        }
    }
}

struct Http {
    base: String,
    client: reqwest::Client,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    task: Option<tokio::task::JoinHandle<std::io::Result<()>>>,
}
impl Http {
    async fn start(fixture: &Fixture) -> Self {
        let bound = checked(
            locron_server::bind(&locron_server::Config {
                bind: vec!["127.0.0.1".into()],
                port: Some(0),
                port_policy: locron_server::PortPolicy::Fixed,
                ..locron_server::Config::default()
            })
            .await,
        );
        let base = format!("http://127.0.0.1:{}", bound.port);
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let paths = fixture.paths.clone();
        let task = tokio::spawn(locron_server::serve_until(bound, paths, async move {
            let _ = stopped.await;
        }));
        Self {
            base,
            client: checked(
                reqwest::Client::builder()
                    .no_proxy()
                    .timeout(Duration::from_secs(60))
                    .build(),
            ),
            stop: Some(stop),
            task: Some(task),
        }
    }
    async fn request(&self, fixture: &Fixture, path: &str, body: Option<Value>) -> (u16, Value) {
        fixture.check();
        let deadline = (Instant::now() + Duration::from_secs(60)).min(
            fixture
                .deadline
                .checked_sub(Duration::from_secs(10))
                .expect("qualification horizon exhausted"),
        );
        let response = checked(
            tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), async {
                let mut request = if body.is_some() {
                    self.client.post(format!("{}{path}", self.base))
                } else {
                    self.client.get(format!("{}{path}", self.base))
                };
                request = request.header(
                    reqwest::header::AUTHORIZATION,
                    format!("token {}", fixture.token),
                );
                if let Some(body) = body {
                    request = request.json(&body);
                }
                let mut response = checked(request.send().await);
                let status = response.status().as_u16();
                let mut bytes = Vec::new();
                while let Some(chunk) = checked(response.chunk().await) {
                    assert!(
                        bytes.len() + chunk.len() <= 512 * 1024,
                        "HTTP body cap exceeded"
                    );
                    bytes.extend_from_slice(&chunk);
                }
                assert!(
                    !bytes
                        .windows(CANARY.len())
                        .any(|part| part == CANARY.as_bytes()),
                    "HTTP reflected secret canary"
                );
                (status, checked(serde_json::from_slice::<Value>(&bytes)))
            })
            .await,
        );
        fixture.check();
        response
    }
    async fn finish(mut self, fixture: &Fixture) {
        checked(self.stop.take().unwrap().send(()));
        let deadline = fixture
            .deadline
            .min(Instant::now() + Duration::from_secs(10));
        let result = checked(
            tokio::time::timeout_at(
                tokio::time::Instant::from_std(deadline),
                self.task.take().unwrap(),
            )
            .await,
        );
        checked(checked(result));
    }
}
impl Drop for Http {
    fn drop(&mut self) {
        self.stop.take();
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

fn http_data(body: &Value) -> &Value {
    assert_eq!(body["schema"], "locron.api/v1");
    assert_eq!(body["ok"], true);
    assert!(
        body.get("warnings").is_some(),
        "missing HTTP warnings field"
    );
    &body["data"]
}

#[tokio::test]
async fn history_active_http_deep_page_contract() {
    let fixture = Fixture::new(OverlapPolicy::Skip, true);
    let http = Http::start(&fixture).await;
    let mut enriched = false;
    for reference in [None, Some(A), Some("Ledger%20%C3%9C%20%25_")] {
        let job = reference.map(|_| A);
        let total = if job.is_some() { 1319 } else { 1322 };
        for offset in [990, 1000, 1100, total - 1, total, total + 1, usize::MAX] {
            for limit in [1, 20, 100] {
                let path = format!(
                    "/api/v1/runs?limit={limit}&offset={offset}{}",
                    reference.map_or(String::new(), |reference| format!("&job={reference}"))
                );
                let (status, body) = http.request(&fixture, &path, None).await;
                assert_eq!(status, 200);
                let data = http_data(&body);
                assert_eq!(data["total"], total);
                assert_eq!(data["limit"], limit);
                assert_eq!(data["offset"], offset);
                assert!(
                    ids(&data["runs"]) == fixture.expected(job, false, offset, limit),
                    "HTTP deep page ledger mismatch"
                );
                for run in data["runs"].as_array().unwrap() {
                    assert!(
                        run.get("source").is_some()
                            && run.get("outcome").is_some()
                            && run.get("duration_us").is_some(),
                        "HTTP observable fields missing"
                    );
                    let attempts = run["attempts"]
                        .as_array()
                        .expect("attempt enrichment array");
                    if !attempts.is_empty() {
                        enriched = true;
                        assert_eq!(attempts.len(), 1);
                        assert_eq!(run["duration_us"], 1);
                        assert_eq!(run["actual_started_at_us"], run["requested_at_us"]);
                    }
                }
            }
        }
    }
    assert!(enriched, "HTTP omitted the seeded deep-page attempt");
    for path in [
        "/api/v1/runs?limit=0",
        "/api/v1/runs?limit=101",
        "/api/v1/runs?job=missing&q=",
        "/api/v1/runs?job=missing&q=ledger",
    ] {
        let (status, body) = http.request(&fixture, path, None).await;
        assert_eq!(status, 400);
        assert_eq!(body["ok"], false);
    }
    for reference in ["missing", C, "00000000-0000-7000-8000-000000009099"] {
        let (status, body) = http
            .request(&fixture, &format!("/api/v1/runs?job={reference}"), None)
            .await;
        assert_eq!(status, 404);
        assert_eq!(body["ok"], false);
    }
    let (status, body) = http
        .request(
            &fixture,
            "/api/v1/runs?q=%20LEDGER%20%C3%BC%20%25_%20&offset=1100&limit=100",
            None,
        )
        .await;
    assert_eq!(status, 200);
    assert_eq!(http_data(&body)["total"], 1319);
    assert!(
        ids(&http_data(&body)["runs"]) == fixture.expected(Some(A), false, 1100, 100),
        "HTTP literal search mismatch"
    );
    fixture.unchanged();
    http.finish(&fixture).await;
    fixture.finish();
}

#[tokio::test]
async fn history_active_http_dry_why_contract() {
    for policy in [
        OverlapPolicy::Skip,
        OverlapPolicy::Replace,
        OverlapPolicy::Allow,
    ] {
        for active in [false, true] {
            let fixture = Fixture::new(policy, active);
            let http = Http::start(&fixture).await;
            let physical_before = logical::physical(&fixture.paths);
            let (status, body) = http
                .request(
                    &fixture,
                    &format!("/api/v1/jobs/{A}/run?dry-run=true"),
                    Some(json!({})),
                )
                .await;
            let physical_immediately_after = logical::physical(&fixture.paths);
            logical::unchanged_physical(&physical_before, &physical_immediately_after);
            assert_eq!(status, 200);
            let data = http_data(&body);
            assert_eq!(data["dry_run"], true);
            assert_eq!(data["durable"], false);
            assert_eq!(data["capacity_reserved"], false);
            let expected = if !active {
                "eligible"
            } else {
                match policy {
                    OverlapPolicy::Skip => "would_skip_overlap",
                    OverlapPolicy::Replace => "would_replace",
                    OverlapPolicy::Allow => "eligible_subject_to_capacity",
                }
            };
            assert_eq!(data["decision"], expected);
            fixture.unchanged();
            let (status, body) = http
                .request(&fixture, &format!("/api/v1/jobs/{A}/why"), None)
                .await;
            assert_eq!(status, 200);
            assert!(
                ids(&http_data(&body)["active_runs"]) == fixture.expected(Some(A), true, 0, 100),
                "HTTP active sample mismatch"
            );
            fixture.unchanged();
            http.finish(&fixture).await;
            fixture.finish();
        }
    }
}
