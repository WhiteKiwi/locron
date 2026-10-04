//! Step-5 contract tests: the `/api/v1` route families over the durable
//! application commands, on an ephemeral port with a temp state directory.
//!
//! The suite exercises the real server (manually spawned `axum::serve` on a
//! port-0 listener, because `locron_server::serve` awaits ctrl_c), with a
//! private managed child and a token created through the production primitive. Every request is
//! token-authenticated (`Authorization: token <t>`), which is also the CSRF
//! exemption path; the envelope schema, CLI-category-to-status mapping, and
//! redaction parity are asserted throughout.

use std::net::SocketAddr;
use std::time::Duration;

use futures_util::StreamExt;
use reqwest::StatusCode;
use serde_json::{Value, json};
use tempfile::TempDir;

use locron_server::{AppState, router};
use locron_store::{FrameChannel, FrameWriter, StatePaths, Store};

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

struct TestServer {
    base: String,
    token: String,
    client: reqwest::Client,
    paths: StatePaths,
    task: tokio::task::JoinHandle<()>,
    _temp: TempDir,
}

fn spawn_server() -> TestServer {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = StatePaths::new(temp.path().join("private"));
    paths.ensure().expect("state layout");
    let token = locron_server::token::ensure(&paths).expect("private token file");
    let listener = std::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
        .expect("bind ephemeral port");
    listener.set_nonblocking(true).expect("nonblocking");
    let listener = tokio::net::TcpListener::from_std(listener).expect("tokio listener");
    let port = listener.local_addr().expect("local addr").port();
    let state = AppState {
        paths: paths.clone(),
        token: token.clone(),
        bound_port: port,
    };
    let app = router(state);
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("server task");
    });
    TestServer {
        base: format!("http://127.0.0.1:{port}"),
        token,
        client: reqwest::Client::builder()
            .no_proxy()
            .build()
            .expect("loopback client"),
        paths,
        task,
        _temp: temp,
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn fixture_executable() -> String {
    #[cfg(unix)]
    {
        "/bin/echo".into()
    }
    #[cfg(windows)]
    {
        std::env::current_exe()
            .expect("native executable")
            .to_str()
            .expect("Unicode fixture")
            .to_owned()
    }
}

fn resolved_fixture_executable() -> String {
    #[cfg(unix)]
    {
        fixture_executable()
    }
    #[cfg(windows)]
    {
        std::fs::canonicalize(fixture_executable())
            .expect("native executable identity")
            .to_str()
            .expect("Unicode fixture")
            .to_owned()
    }
}

fn fixture_cwd() -> std::path::PathBuf {
    #[cfg(unix)]
    {
        "/tmp".into()
    }
    #[cfg(windows)]
    {
        std::env::current_dir().expect("absolute native working directory")
    }
}

impl TestServer {
    /// Authenticated request; `body` is sent as JSON for mutation methods.
    async fn send(&self, method: &str, path: &str, body: Option<Value>) -> (StatusCode, Value) {
        let mut request = self
            .client
            .request(
                reqwest::Method::from_bytes(method.as_bytes()).expect("method"),
                format!("{}{}", self.base, path),
            )
            .header(
                reqwest::header::AUTHORIZATION,
                format!("token {}", self.token),
            );
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request
            .send()
            .await
            .unwrap_or_else(|error| panic!("{method} {path}: request failed: {error}"));
        let status = response.status();
        let text = response
            .text()
            .await
            .unwrap_or_else(|error| panic!("{method} {path}: no response body: {error}"));
        let value = serde_json::from_str(&text).unwrap_or_else(|error| {
            panic!("{method} {path}: non-JSON response {status}: {text:?} ({error})")
        });
        (status, value)
    }

    async fn get(&self, path: &str) -> (StatusCode, Value) {
        self.send("GET", path, None).await
    }

    async fn post(&self, path: &str, body: Option<Value>) -> (StatusCode, Value) {
        self.send("POST", path, body).await
    }

    async fn put(&self, path: &str, body: Option<Value>) -> (StatusCode, Value) {
        self.send("PUT", path, body).await
    }

    async fn delete(&self, path: &str) -> (StatusCode, Value) {
        self.send("DELETE", path, None).await
    }
}

/// Extracts the envelope `data` member after asserting the envelope shape.
fn data(body: &Value) -> &Value {
    assert_eq!(body["schema"], json!("locron.api/v1"), "schema: {body}");
    assert_eq!(body["ok"], json!(true), "ok: {body}");
    assert!(body.get("warnings").is_some(), "warnings: {body}");
    body.get("data").expect("data member")
}

/// Asserts an error envelope with the given CLI category and returns the message.
fn error(body: &Value, code: &str) -> String {
    assert_eq!(body["schema"], json!("locron.api/v1"), "schema: {body}");
    assert_eq!(body["ok"], json!(false), "ok: {body}");
    assert_eq!(body["error"]["code"], json!(code), "code: {body}");
    body["error"]["message"]
        .as_str()
        .expect("message")
        .to_owned()
}

fn body_text(body: &Value) -> String {
    serde_json::to_string(body).expect("serialize")
}

/// A minimal valid process-target definition, with optional secrets.
fn definition(executable: &str, env_token: bool, header_secret: bool, body_secret: bool) -> Value {
    let executable = if executable == "/bin/echo" {
        fixture_executable()
    } else {
        executable.to_owned()
    };
    let mut headers = serde_json::Map::new();
    if header_secret {
        headers.insert(
            "X-Api-Key".into(),
            json!({"source": "inline", "value": "header-secret"}),
        );
    }
    let mut environment = serde_json::Map::new();
    if env_token {
        environment.insert("TOKEN".into(), json!("super-secret-value"));
    }
    let mut target = serde_json::Map::new();
    target.insert("kind".into(), json!("process"));
    target.insert("executable".into(), json!(executable));
    target.insert("args".into(), json!([]));
    if body_secret {
        // Body bytes as the UTF-8 of "body-secret".
        target.insert(
            "body".into(),
            json!([98, 111, 100, 121, 45, 115, 101, 99, 114, 101, 116]),
        );
    }
    json!({
        "schedule": {"kind": "cron", "expression": "* * * * *", "timezone": {"mode": "local"}},
        "target": target,
        "cwd": fixture_cwd(),
        "environment": {"values": environment},
        "policy": {
            "overlap": "skip",
            "missed_run": "skip",
            "catch_up_limit": 10,
            "retries": 0,
            "retry_delay": 0,
            "retry_cap": 0,
            "backoff": "exponential",
            "retry_timeout": false,
            "timeout": null,
            "start_deadline": null,
            "termination_grace": 0,
            "per_job_concurrency": 1
        }
    })
}

fn create_body(name: &str, definition: &Value) -> Value {
    json!({"name": name, "description": "contract fixture", "tags": [], "enabled": true, "definition": definition})
}

fn http_definition(method: &str, body: &[u8]) -> Value {
    let mut definition = definition("/bin/echo", false, false, false);
    definition["target"] = json!({
        "kind": "http",
        "method": method,
        "url": "https://example.test/hook",
        "headers": {},
        "body": body,
        "body_file": null,
        "success_statuses": [200],
        "follow_redirects": true
    });
    definition
}

// ---------------------------------------------------------------------------
// Authentication
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn token_refusal() {
    let server = spawn_server();

    let response = server
        .client
        .get(format!("{}/api/v1/jobs", server.base))
        .send()
        .await
        .expect("request");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body: Value = response.json().await.expect("json");
    assert_eq!(
        error(&body, "unauthenticated"),
        "a valid access token or session cookie is required for /api/v1/jobs"
    );

    let response = server
        .client
        .get(format!("{}/api/v1/jobs", server.base))
        .header(
            reqwest::header::AUTHORIZATION,
            format!("token {}", "b".repeat(64)),
        )
        .send()
        .await
        .expect("request");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body: Value = response.json().await.expect("json");
    assert_eq!(
        error(&body, "unauthenticated"),
        "a valid access token or session cookie is required for /api/v1/jobs"
    );
}

// ---------------------------------------------------------------------------
// Job CRUD
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn job_crud_round_trip() {
    let server = spawn_server();

    // Create.
    let (status, body) = server
        .post(
            "/api/v1/jobs",
            Some(create_body(
                "alpha",
                &definition("/bin/echo", false, false, false),
            )),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let created = data(&body);
    let id = created["id"].as_str().expect("id").to_owned();
    assert_eq!(created["name"], json!("alpha"));
    assert!(created["enabled"].as_bool().expect("enabled"));
    let stored_definition: Value =
        serde_json::from_str(created["definition_json"].as_str().expect("definition"))
            .expect("stored definition");
    assert_eq!(
        stored_definition["target"]["executable"],
        json!(fixture_executable())
    );

    // List.
    let (status, body) = server.get("/api/v1/jobs").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let list = data(&body);
    assert_eq!(list.as_array().expect("array").len(), 1);
    assert_eq!(list[0]["id"], json!(id));

    // Show by name and by id.
    for reference in [&id, "alpha"] {
        let (status, body) = server.get(&format!("/api/v1/jobs/{reference}")).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(data(&body)["id"], json!(id));
    }

    // No-op update is a durable conflict, like the CLI.
    let (status, body) = server
        .put(&format!("/api/v1/jobs/{id}"), Some(json!({})))
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(
        error(&body, "durable_conflict"),
        "update does not change any field"
    );

    // Real update: name + description, revision bumps.
    let (status, body) = server
        .put(
            &format!("/api/v1/jobs/{id}"),
            Some(json!({"name": "beta", "description": "renamed"})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let updated = data(&body);
    assert_eq!(updated["name"], json!("beta"));
    assert_eq!(updated["current_revision"], json!(2));
    let (_, show) = server.get(&format!("/api/v1/jobs/{id}")).await;
    assert_eq!(data(&show)["current_revision"], json!(2));
    assert_eq!(data(&show)["description"], json!("renamed"));

    // Toggle.
    let (status, body) = server
        .post(&format!("/api/v1/jobs/{id}/disable"), None)
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["enabled"], json!(false));
    let (status, body) = server
        .post(&format!("/api/v1/jobs/{id}/enable"), None)
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["enabled"], json!(true));

    // Preview: RFC 3339 occurrences.
    let (status, body) = server.get(&format!("/api/v1/jobs/{id}/preview")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let preview = data(&body);
    let occurrences = preview["occurrences"].as_array().expect("occurrences");
    assert_eq!(occurrences.len(), 5);
    assert!(occurrences[0].as_str().expect("rfc3339").contains('T'));

    // Why: durable facts.
    let (status, body) = server.get(&format!("/api/v1/jobs/{id}/why")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let why = data(&body);
    assert_eq!(why["job"]["name"], json!("beta"));
    assert_eq!(why["overlap"], json!("skip"));
    assert_eq!(why["daemon_running"], json!(false));
    assert!(why["next_occurrence"].as_str().expect("next").contains('T'));

    // Remove; then not found.
    let (status, body) = server.delete(&format!("/api/v1/jobs/{id}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["removed"], json!(true));
    let (status, body) = server.get("/api/v1/jobs/beta").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(error(&body, "not_found"), "beta");
}

#[tokio::test(flavor = "multi_thread")]
async fn http_body_bytes_round_trip_through_create_update_and_dry_run() {
    let server = spawn_server();
    let first = "안녕 Locron".as_bytes();
    let (status, body) = server
        .post(
            "/api/v1/jobs",
            Some(create_body("webhook", &http_definition("POST", first))),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let id = data(&body)["id"].as_str().expect("id").to_owned();
    let document = server
        .export_document("?include-values=1&acknowledge-plaintext=1")
        .await;
    assert_eq!(
        document["jobs"][0]["definition"]["target"]["body"],
        json!(first)
    );

    let second = "수정된 본문".as_bytes();
    let (status, body) = server
        .put(
            &format!("/api/v1/jobs/{id}"),
            Some(json!({"definition": http_definition("PATCH", second)})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let document = server
        .export_document("?include-values=1&acknowledge-plaintext=1")
        .await;
    assert_eq!(
        document["jobs"][0]["definition"]["target"]["body"],
        json!(second)
    );

    let dry_run = "저장하지 않음".as_bytes();
    let (status, body) = server
        .put(
            &format!("/api/v1/jobs/{id}"),
            Some(json!({
                "definition": http_definition("PUT", dry_run),
                "dry_run": "1"
            })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["dry_run"], json!(true));
    let document = server
        .export_document("?include-values=1&acknowledge-plaintext=1")
        .await;
    assert_eq!(
        document["jobs"][0]["definition"]["target"]["body"],
        json!(second)
    );

    let response = server
        .client
        .post(format!("{}/api/v1/jobs", server.base))
        .header(
            reqwest::header::AUTHORIZATION,
            format!("token {}", server.token),
        )
        .json(&create_body(
            "unsupported",
            &http_definition("OPTIONS", b"body"),
        ))
        .send()
        .await
        .expect("unsupported method request");
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        response
            .text()
            .await
            .expect("error text")
            .contains("unknown variant `OPTIONS`")
    );
}

// ---------------------------------------------------------------------------
// Offline manual enqueue and cancellation
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn offline_manual_enqueue_and_cancel() {
    let server = spawn_server();
    let (_, body) = server
        .post(
            "/api/v1/jobs",
            Some(create_body(
                "alpha",
                &definition("/bin/echo", false, false, false),
            )),
        )
        .await;
    let id = data(&body)["id"].as_str().expect("id").to_owned();

    // Manual enqueue: durably queued with the daemon-absent warning.
    let (status, body) = server.post(&format!("/api/v1/jobs/{id}/run"), None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let run = data(&body);
    let run_id = run["run_id"].as_str().expect("run_id").to_owned();
    assert_eq!(run["state"], json!("queued"));
    assert_eq!(
        body["warnings"],
        json!(["daemon is not running; run remains durably queued"])
    );

    // Show: observable run document.
    let (status, body) = server.get(&format!("/api/v1/runs/{run_id}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let run_doc = data(&body);
    assert_eq!(run_doc["id"], json!(run_id));
    assert_eq!(run_doc["state"], json!("queued"));
    assert_eq!(run_doc["source"], json!("manual"));
    assert_eq!(run_doc["attempts"], json!([]));
    assert!(run_doc.get("outcome").is_some(), "outcome key present");

    // Cancel before execution: the CLI's first outcome shape.
    let (status, body) = server
        .post(&format!("/api/v1/runs/{run_id}/cancel"), None)
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        data(&body),
        &json!({"run_id": run_id, "requested": true, "cancelled": true, "before_execution": true})
    );

    // Run why: durable facts and audit events.
    let (status, body) = server.get(&format!("/api/v1/runs/{run_id}/why")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let why = data(&body);
    assert_eq!(why["run"]["id"], json!(run_id));
    assert_eq!(why["daemon_running"], json!(false));
    assert!(
        why["events"].as_array().expect("events").len() >= 2,
        "{why}"
    );

    // Cancelling the already-cancelled run: a durable conflict, like the CLI.
    let (status, body) = server
        .post(&format!("/api/v1/runs/{run_id}/cancel"), None)
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(
        error(&body, "durable_conflict"),
        format!("run {run_id} is already terminal (cancelled)")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn run_history_literal_search_is_complete_paginated_and_validated() {
    let server = spawn_server();
    let (_, body) = server
        .post(
            "/api/v1/jobs",
            Some(create_body(
                "Nightly-backup-백업%_One",
                &definition("/bin/echo", false, false, true),
            )),
        )
        .await;
    let job_id = data(&body)["id"].as_str().unwrap().to_owned();
    let store = Store::open(server.paths.clone(), "contract", 2).unwrap();
    let mut run_ids = Vec::new();
    for now in 1..=1_005 {
        let run_id = uuid::Uuid::now_v7().to_string();
        store.enqueue_manual(&job_id, &run_id, now).unwrap();
        run_ids.push(run_id);
    }

    let (status, body) = server.get("/api/v1/runs?q=BACK&limit=20&offset=20").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["total"], json!(1_005));
    assert_eq!(data(&body)["runs"].as_array().unwrap().len(), 20);
    assert!(!body_text(&body).contains("body-secret"));

    let (_, unicode) = server.get("/api/v1/runs?q=백업&limit=1").await;
    assert_eq!(data(&unicode)["total"], json!(1_005));
    let (_, literal) = server.get("/api/v1/runs?q=%25_&limit=1").await;
    assert_eq!(data(&literal)["total"], json!(1_005));
    let suffix = &run_ids[500][run_ids[500].len() - 6..];
    let (_, partial) = server
        .get(&format!("/api/v1/runs?q={suffix}&limit=100"))
        .await;
    assert!(
        data(&partial)["runs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|run| run["id"] == run_ids[500])
    );

    let (status, conflict) = server
        .get(&format!("/api/v1/runs?q=night&job={job_id}"))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        error(&conflict, "invalid_request"),
        "q and job are mutually exclusive"
    );
    let (status, invalid) = server.get("/api/v1/runs?limit=101").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        error(&invalid, "invalid_request"),
        "limit must be from 1 through 100"
    );

    let (_, _) = server
        .put(
            &format!("/api/v1/jobs/{job_id}"),
            Some(json!({"name": "Renamed current"})),
        )
        .await;
    let (_, old) = server.get("/api/v1/runs?q=nightly&limit=1").await;
    assert_eq!(data(&old)["total"], json!(0));
    let (_, current) = server.get("/api/v1/runs?q=RENAMED&limit=1").await;
    assert_eq!(data(&current)["total"], json!(1_005));
    let (_, _) = server.delete(&format!("/api/v1/jobs/{job_id}")).await;
    let (_, removed) = server.get("/api/v1/runs?q=renamed&limit=1").await;
    assert_eq!(data(&removed)["total"], json!(1_005));
}

// ---------------------------------------------------------------------------
// Dry-run non-mutation
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn dry_run_never_mutates() {
    let server = spawn_server();

    // Create dry-run with no database at all (body dry-run is a string flag,
    // parsed by the same flag parser as query parameters).
    let mut ghost = create_body("ghost", &definition("/bin/echo", false, false, false));
    ghost["dry_run"] = json!("1");
    let (status, body) = server.post("/api/v1/jobs", Some(ghost)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["dry_run"], json!(true));
    assert_eq!(data(&body)["id"], json!("<non-durable>"));
    let (_, list) = server.get("/api/v1/jobs").await;
    assert_eq!(data(&list).as_array().expect("array").len(), 0);

    // Live job for the remaining dry runs.
    let (_, body) = server
        .post(
            "/api/v1/jobs",
            Some(create_body(
                "alpha",
                &definition("/bin/echo", false, false, false),
            )),
        )
        .await;
    let id = data(&body)["id"].as_str().expect("id").to_owned();

    // Dry-run create against an existing state database: the create path must
    // branch on the request's dry_run flag, not on whether the database file
    // exists — an existing database must not fall into the live-create branch.
    let (status, body) = server
        .post(
            "/api/v1/jobs",
            Some({
                let mut ghost = create_body("ghost", &definition("/bin/echo", false, false, false));
                ghost["dry_run"] = json!("1");
                ghost
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["dry_run"], json!(true));
    assert_eq!(data(&body)["id"], json!("<non-durable>"));
    let (_, list) = server.get("/api/v1/jobs").await;
    assert_eq!(data(&list).as_array().expect("array").len(), 1);

    // Run dry-run: CLI shape, no durable run.
    let (status, body) = server
        .post(&format!("/api/v1/jobs/{id}/run?dry-run=1"), None)
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        data(&body),
        &json!({"dry_run": true, "durable": false, "decision": "eligible", "capacity_reserved": false})
    );
    let (_, history) = server.get("/api/v1/runs").await;
    assert_eq!(data(&history)["total"], json!(0));

    // Update dry-run: changed fields, no durable change.
    let (status, body) = server
        .put(
            &format!("/api/v1/jobs/{id}"),
            Some(json!({"description": "would change", "dry_run": "1"})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let preview_update = data(&body);
    assert_eq!(preview_update["dry_run"], json!(true));
    assert_eq!(preview_update["revision"], json!(2));
    assert!(
        preview_update["changed_fields"]
            .as_array()
            .expect("fields")
            .contains(&json!("description"))
    );
    let (_, show) = server.get(&format!("/api/v1/jobs/{id}")).await;
    assert_eq!(data(&show)["description"], json!("contract fixture"));

    // Prune dry-run: no candidates, no mutation.
    let (status, body) = server.post("/api/v1/prune?dry-run=1", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        data(&body),
        &json!({"dry_run": true, "candidate_count": 0, "bytes": 0})
    );

    // Settings dry-run: validated, not written (dry-run is a body field).
    let (status, body) = server
        .put(
            "/api/v1/settings/global_concurrency",
            Some(json!({"value": "4", "dry_run": "1"})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        data(&body),
        &json!({"key": "global_concurrency", "value": "4", "dry_run": true})
    );
    let (_, settings) = server.get("/api/v1/settings").await;
    assert_eq!(data(&settings)["global_concurrency"], json!(16));

    // Import dry-run: planned actions, no durable import.
    let export = server.export_document("").await;
    let (status, body) = server.post("/api/v1/import?dry-run=1", Some(export)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let plan = data(&body);
    assert_eq!(plan["dry_run"], json!(true));
    let actions = plan["actions"].as_array().expect("actions");
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0]["action"], json!("no_op"));
}

// ---------------------------------------------------------------------------
// Export and import
// ---------------------------------------------------------------------------

impl TestServer {
    async fn export_document(&self, query: &str) -> Value {
        let (status, body) = self.get(&format!("/api/v1/export{query}")).await;
        assert_eq!(status, StatusCode::OK, "export{query}: {body}");
        body
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn export_import_round_trip() {
    let origin = spawn_server();

    // Two jobs, one tagged and carrying an env secret; a global setting and an
    // environment value.
    let mut tagged = create_body("prod-ping", &definition("/bin/echo", true, false, false));
    tagged["tags"] = json!(["prod"]);
    let (_, body) = origin.post("/api/v1/jobs", Some(tagged)).await;
    let tagged_id = data(&body)["id"].as_str().expect("id").to_owned();
    let (_, body) = origin
        .post(
            "/api/v1/jobs",
            Some(create_body(
                "plain-ping",
                &definition("/bin/echo", false, false, false),
            )),
        )
        .await;
    let _plain_id = data(&body)["id"].as_str().expect("id").to_owned();
    let (_, body) = origin
        .put(
            "/api/v1/settings/global_concurrency",
            Some(json!({"value": "4"})),
        )
        .await;
    assert_eq!(data(&body)["global_concurrency"], json!(4));
    let (_, body) = origin
        .put(
            "/api/v1/settings/environment.FOO",
            Some(json!({"value": "bar"})),
        )
        .await;
    assert_eq!(data(&body)["action"], json!("created"));

    // Redacted export: no plaintext values anywhere, omitted paths listed.
    let document = origin.export_document("").await;
    assert_eq!(document["schema"], json!("locron.export/v1"));
    assert_eq!(document["values_mode"], json!("redacted"));
    let jobs = document["jobs"].as_array().expect("jobs");
    assert_eq!(jobs.len(), 2, "{document}");
    assert!(
        jobs[0]["definition"]["environment"]["values"]
            .as_object()
            .expect("values")
            .is_empty()
    );
    let omitted = document["omitted_values"].as_array().expect("omitted");
    assert!(
        omitted.contains(&json!("settings.environment.FOO")),
        "{document}"
    );
    let prod_ping = jobs
        .iter()
        .find(|job| job["name"] == json!("prod-ping"))
        .expect("prod-ping");
    assert!(
        prod_ping["omitted_values"]
            .as_array()
            .expect("omitted")
            .contains(&json!("definition.environment.values.TOKEN")),
        "{prod_ping}"
    );
    let raw = body_text(&document);
    assert!(!raw.contains("bar"), "secret in redacted export");
    assert!(
        !raw.contains("super-secret-value"),
        "job secret in redacted export"
    );

    // Plaintext export requires both flags.
    let (status, body) = origin.get("/api/v1/export?include-values=1").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(
        error(&body, "invalid_request"),
        "plaintext export requires both --include-values and --acknowledge-plaintext"
    );

    // Plaintext export with both flags carries the values verbatim.
    let (status, body) = origin
        .get("/api/v1/export?include-values=1&acknowledge-plaintext=1")
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["values_mode"], json!("plaintext"));
    assert!(
        body_text(&body).contains("bar"),
        "plaintext export lacks value"
    );

    // Content-Disposition attachment on the raw response.
    let response = origin
        .client
        .get(format!("{}/api/v1/export", origin.base))
        .header(
            reqwest::header::AUTHORIZATION,
            format!("token {}", origin.token),
        )
        .send()
        .await
        .expect("export");
    let disposition = response
        .headers()
        .get(reqwest::header::CONTENT_DISPOSITION)
        .expect("content-disposition")
        .to_str()
        .expect("ascii");
    assert_eq!(disposition, "attachment; filename=\"locron.export.json\"");

    // Importing the redacted document into a fresh database is refused,
    // mirroring the CLI's parse rule: an export that omitted values cannot be
    // imported faithfully.
    let target = spawn_server();
    let (status, body) = target.post("/api/v1/import", Some(document)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(
        error(&body, "invalid_request"),
        "redacted export contains omitted values and cannot be imported faithfully"
    );

    // The plaintext export is the faithful round-trip form; importing it
    // without the acknowledgement flag is refused…
    let (_, body) = origin
        .get("/api/v1/export?include-values=1&acknowledge-plaintext=1")
        .await;
    let plaintext = body;
    let (status, body) = target.post("/api/v1/import", Some(plaintext.clone())).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(
        error(&body, "invalid_request"),
        "plaintext values require --accept-plaintext-values"
    );

    // …and with the flag, everything is created and settings applied.
    let (status, body) = target
        .post(
            "/api/v1/import?accept-plaintext-values=1",
            Some(plaintext.clone()),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        data(&body),
        &json!({"created": 2, "updated": 0, "no_op": 0, "settings_changed": true})
    );
    let (_, list) = target.get("/api/v1/jobs").await;
    assert_eq!(data(&list).as_array().expect("array").len(), 2);
    let (_, settings) = target.get("/api/v1/settings").await;
    assert_eq!(data(&settings)["global_concurrency"], json!(4));
    assert_eq!(
        data(&settings)["environment"]["FOO"],
        json!({"configured": true, "value_redacted": true})
    );
    assert!(
        !body_text(&settings).contains("bar"),
        "secret leaked into settings"
    );

    // Re-import: everything is a no-op.
    let (status, body) = target
        .post("/api/v1/import?accept-plaintext-values=1", Some(plaintext))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        data(&body),
        &json!({"created": 0, "updated": 0, "no_op": 2, "settings_changed": false})
    );

    // The tagged job maps to its source id (destination id present in plan).
    let (_, body) = target
        .post(
            "/api/v1/import?accept-plaintext-values=1&dry-run=1",
            Some(
                origin
                    .export_document("?include-values=1&acknowledge-plaintext=1")
                    .await,
            ),
        )
        .await;
    let actions = data(&body)["actions"].as_array().expect("actions");
    let tagged_action = actions
        .iter()
        .find(|action| action["source_id"] == json!(tagged_id))
        .expect("tagged action");
    assert_eq!(tagged_action["action"], json!("no_op"));
}

#[tokio::test(flavor = "multi_thread")]
async fn export_selectors() {
    let server = spawn_server();
    let mut tagged = create_body("prod-ping", &definition("/bin/echo", false, false, false));
    tagged["tags"] = json!(["prod"]);
    let (_, body) = server.post("/api/v1/jobs", Some(tagged)).await;
    let tagged_id = data(&body)["id"].as_str().expect("id").to_owned();
    let (_, body) = server
        .post(
            "/api/v1/jobs",
            Some(create_body(
                "plain-ping",
                &definition("/bin/echo", false, false, false),
            )),
        )
        .await;
    let plain_id = data(&body)["id"].as_str().expect("id").to_owned();

    let document = server.export_document("?jobs=plain-ping").await;
    assert_eq!(document["jobs"].as_array().expect("jobs").len(), 1);
    assert_eq!(document["jobs"][0]["id"], json!(plain_id));

    let document = server.export_document("?tag=prod").await;
    assert_eq!(document["jobs"].as_array().expect("jobs").len(), 1);
    assert_eq!(document["jobs"][0]["id"], json!(tagged_id));

    // Comma-separated union.
    let document = server
        .export_document("?jobs=prod-ping&tag=prod,prod")
        .await;
    assert_eq!(document["jobs"].as_array().expect("jobs").len(), 1);

    // Strict no-match validation.
    let (status, body) = server.get("/api/v1/export?jobs=nope").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(error(&body, "invalid_request"), "no job matches nope");
    let (status, body) = server.get("/api/v1/export?tag=nope").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(error(&body, "invalid_request"), "no jobs match tag nope");
}

// ---------------------------------------------------------------------------
// URL import with the documented fetch bounds
// ---------------------------------------------------------------------------

/// Serves a fixed byte payload for one request, then exits.
async fn serve_once(body: Vec<u8>) -> u16 {
    let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .expect("bind");
    let port = listener.local_addr().expect("addr").port();
    tokio::spawn(async move {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let (mut socket, _) = listener.accept().await.expect("accept");
        let mut request = [0_u8; 4096];
        let _ = socket.read(&mut request).await;
        let header = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n",
            body.len()
        );
        socket.write_all(header.as_bytes()).await.expect("header");
        socket.write_all(&body).await.expect("body");
        socket.flush().await.expect("flush");
    });
    port
}

/// Serves an endless 302 chain, exercising the redirect cap.
async fn serve_redirect_loop() -> u16 {
    let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .expect("bind");
    let port = listener.local_addr().expect("addr").port();
    tokio::spawn(async move {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let mut request = [0_u8; 4096];
            let _ = socket.read(&mut request).await;
            let header = "HTTP/1.1 302 Found\r\nLocation: /next\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
            let _ = socket.write_all(header.as_bytes()).await;
        }
    });
    port
}

#[tokio::test(flavor = "multi_thread")]
async fn url_import() {
    let origin = spawn_server();
    let (_, _) = origin
        .post(
            "/api/v1/jobs",
            Some(create_body(
                "alpha",
                &definition("/bin/echo", false, false, false),
            )),
        )
        .await;
    let document = origin.export_document("").await;
    let document_bytes = serde_json::to_vec(&document).expect("doc bytes");

    // Local fixture URL import.
    let port = serve_once(document_bytes.clone()).await;
    let target = spawn_server();
    let (status, body) = target
        .post(
            "/api/v1/import",
            Some(json!({"url": format!("http://127.0.0.1:{port}/doc.json")})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["created"], json!(1));

    // Import body must be exactly {"url": …} to select URL mode.
    let port = serve_once(document_bytes.clone()).await;
    let (status, body) = target
        .post(
            "/api/v1/import",
            Some(json!({"url": format!("http://127.0.0.1:{port}/doc.json"), "extra": 1})),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(
        error(&body, "invalid_request"),
        "import body must be an export document or {\"url\": \"…\"}"
    );

    // Scheme and userinfo rejection.
    let (status, body) = target
        .post(
            "/api/v1/import",
            Some(json!({"url": "ftp://example.invalid/doc.json"})),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(
        error(&body, "invalid_request"),
        "import URL must be http or https"
    );
    let (status, body) = target
        .post(
            "/api/v1/import",
            Some(json!({"url": "http://user:pass@127.0.0.1:1/doc.json"})),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(
        error(&body, "invalid_request"),
        "import URL must not contain userinfo"
    );

    // 16 MiB streaming cap.
    let oversized = vec![b'x'; 16 * 1024 * 1024 + 64];
    let port = serve_once(oversized).await;
    let (status, body) = target
        .post(
            "/api/v1/import",
            Some(json!({"url": format!("http://127.0.0.1:{port}/big.json")})),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(
        error(&body, "invalid_request"),
        "import document exceeds the 16 MiB cap"
    );

    // Redirect cap: 10 redirects is a fetch failure, mapped to 502 state_error.
    let port = serve_redirect_loop().await;
    let (status, body) = target
        .post(
            "/api/v1/import",
            Some(json!({"url": format!("http://127.0.0.1:{port}/start")})),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "{body}");
    let message = error(&body, "state_error");
    assert!(
        message.starts_with("import fetch failed: error following redirect"),
        "{message}"
    );

    // Connection refused is also a 502 state_error.
    let (status, body) = target
        .post(
            "/api/v1/import",
            Some(json!({"url": "http://127.0.0.1:1/unreachable.json"})),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "{body}");
    let message = error(&body, "state_error");
    assert!(
        message.starts_with("import fetch failed: error sending request"),
        "{message}"
    );
}

// ---------------------------------------------------------------------------
// Error-category mapping matrix
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn error_mapping_matrix() {
    let server = spawn_server();
    let (_, body) = server
        .post(
            "/api/v1/jobs",
            Some(create_body(
                "alpha",
                &definition("/bin/echo", false, false, false),
            )),
        )
        .await;
    let id = data(&body)["id"].as_str().expect("id").to_owned();
    let (_, body) = server.post(&format!("/api/v1/jobs/{id}/run"), None).await;
    let run_id = data(&body)["run_id"].as_str().expect("run_id").to_owned();

    // NotFound -> 404 not_found (jobs and runs).
    let (status, body) = server.get("/api/v1/jobs/missing").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(error(&body, "not_found"), "missing");
    let phantom = uuid::Uuid::now_v7().to_string();
    let (status, body) = server.get(&format!("/api/v1/runs/{phantom}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(error(&body, "not_found"), phantom);
    let (status, body) = server.post("/api/v1/jobs/missing/run", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(error(&body, "not_found"), "missing");

    // Conflict -> 409 durable_conflict.
    let (status, body) = server
        .put(&format!("/api/v1/jobs/{id}"), Some(json!({})))
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(
        error(&body, "durable_conflict"),
        "update does not change any field"
    );

    // Validation -> 400 invalid_request.
    let (status, body) = server.post("/api/v1/runs/not-a-uuid/cancel", None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(error(&body, "invalid_request"), "invalid run UUID");
    // Live unknown key: the store's own not-found, exactly like the CLI's
    // live `config set` (the CLI validates only in dry-run mode).
    let (status, body) = server
        .put("/api/v1/settings/nope", Some(json!({"value": "1"})))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(error(&body, "not_found"), "configuration key nope");
    let (status, body) = server
        .put(
            "/api/v1/settings/nope",
            Some(json!({"value": "1", "dry_run": "1"})),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(error(&body, "invalid_request"), "unknown configuration key");
    // Live invalid values surface the store's durable conflict, exactly like
    // the CLI's live `config set`; dry-run validates first (400).
    let (status, body) = server
        .put(
            "/api/v1/settings/global_concurrency",
            Some(json!({"value": "0"})),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(
        error(&body, "durable_conflict"),
        "global_concurrency must be from 1 through 64"
    );
    let (status, body) = server
        .put(
            "/api/v1/settings/global_concurrency",
            Some(json!({"value": "abc"})),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(
        error(&body, "durable_conflict"),
        "global_concurrency must be an integer"
    );
    let (status, body) = server
        .put("/api/v1/settings/environment", Some(json!({"value": "x"})))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(
        error(&body, "invalid_request"),
        "environment requires a named environment.NAME key"
    );
    let (status, body) = server.delete("/api/v1/settings/execution_path").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(
        error(&body, "invalid_request"),
        "only environment.NAME settings can be unset"
    );
    let (status, body) = server
        .post("/api/v1/schedule/preview", Some(json!({})))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(
        error(&body, "invalid_request"),
        "provide a job name or a schedule"
    );
    let (status, body) = server
        .post(
            "/api/v1/schedule/preview",
            Some(json!({"job": "alpha", "schedule": {"kind": "cron", "expression": "* * * * *", "timezone": {"mode": "local"}}})),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(
        error(&body, "invalid_request"),
        "provide a job name or a schedule, not both"
    );
    let mut relative_cwd = create_body("bad-cwd", &definition("/bin/echo", false, false, false));
    relative_cwd["definition"]["cwd"] = json!("relative");
    let (status, body) = server.post("/api/v1/jobs", Some(relative_cwd)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(
        error(&body, "invalid_request"),
        "cwd: working directory must be absolute"
    );

    // State errors -> 500 state_error (invalid identity attempt zero).
    let (status, body) = server
        .get(&format!("/api/v1/runs/{run_id}/logs?attempt=0"))
        .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
    assert_eq!(
        error(&body, "state_error"),
        "invalid identity: attempt number must be positive"
    );
}

// ---------------------------------------------------------------------------
// Settings surface
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn settings_surface() {
    let server = spawn_server();

    // Defaults.
    let (status, body) = server.get("/api/v1/settings").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let settings = data(&body);
    assert_eq!(settings["global_concurrency"], json!(16));
    assert_eq!(
        settings["execution_path"],
        json!(locron_core::execution::default_execution_path())
    );
    assert_eq!(settings["environment"], json!({}));

    // Typed put returns the redacted settings document.
    let (status, body) = server
        .put(
            "/api/v1/settings/global_concurrency",
            Some(json!({"value": "4"})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["global_concurrency"], json!(4));
    let (status, body) = server
        .put(
            "/api/v1/settings/run_retention_count",
            Some(json!({"value": "7"})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["run_retention_count"], json!(7));
    let (status, body) = server
        .put(
            "/api/v1/settings/run_retention_age_us",
            Some(json!({"value": "3600000000", "dry_run": "1"})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["dry_run"], json!(true));
    let (status, body) = server
        .put(
            "/api/v1/settings/run_retention_age_us",
            Some(json!({"value": "3600000000"})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        data(&body)["run_retention_age_us"],
        json!(3_600_000_000_i64)
    );
    let (status, body) = server
        .put(
            "/api/v1/settings/run_retention_age_us",
            Some(json!({"value": "none", "dry_run": "1"})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = server
        .put(
            "/api/v1/settings/run_retention_age_us",
            Some(json!({"value": "none"})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["run_retention_age_us"], Value::Null);

    // Environment values: created then replaced, redacted on read.
    let (status, body) = server
        .put(
            "/api/v1/settings/environment.API_TOKEN",
            Some(json!({"value": "hunter2"})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        data(&body),
        &json!({"key": "environment.API_TOKEN", "action": "created", "configured": true, "value_redacted": true, "dry_run": false})
    );
    let (status, body) = server
        .put(
            "/api/v1/settings/environment.API_TOKEN",
            Some(json!({"value": "hunter3"})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["action"], json!("replaced"));
    let (_, settings) = server.get("/api/v1/settings").await;
    assert_eq!(
        data(&settings)["environment"]["API_TOKEN"],
        json!({"configured": true, "value_redacted": true})
    );
    assert!(!body_text(&settings).contains("hunter2"));
    assert!(!body_text(&settings).contains("hunter3"));

    // Reserved names are refused.
    let (status, body) = server
        .put(
            "/api/v1/settings/environment.LOCRON_X",
            Some(json!({"value": "x"})),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(
        error(&body, "invalid_request"),
        "invalid or reserved environment name LOCRON_X"
    );

    // Delete: removed, then unchanged.
    let (status, body) = server
        .delete("/api/v1/settings/environment.API_TOKEN")
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        data(&body),
        &json!({"key": "environment.API_TOKEN", "action": "removed", "configured": false, "value_redacted": true, "dry_run": false})
    );
    let (status, body) = server
        .delete("/api/v1/settings/environment.API_TOKEN")
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["action"], json!("unchanged"));
    let (_, settings) = server.get("/api/v1/settings").await;
    assert_eq!(data(&settings)["environment"], json!({}));
}

#[cfg(windows)]
#[tokio::test(flavor = "multi_thread")]
async fn windows_environment_case_rules_and_failures_preserve_redaction() {
    let server = spawn_server();
    let (status, body) = server
        .put(
            "/api/v1/settings/environment.fixture",
            Some(json!({"value":"private-first-value"})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["action"], "created");
    let (status, body) = server
        .put(
            "/api/v1/settings/environment.FIXTURE",
            Some(json!({"value":"private-replacement-value"})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["action"], "replaced");
    let (_, body) = server.get("/api/v1/settings").await;
    assert_eq!(
        data(&body)["environment"],
        json!({"FIXTURE":{"configured":true,"value_redacted":true}})
    );
    assert!(!body_text(&body).contains("private-first-value"));
    assert!(!body_text(&body).contains("private-replacement-value"));

    for values in [
        json!({"Token":"collision-secret-one","TOKEN":"collision-secret-two"}),
        json!({"lOcRoN_SECRET":"reserved-secret-value"}),
    ] {
        let mut definition = definition("/bin/echo", false, false, false);
        definition["environment"]["values"] = values;
        let (status, body) = server
            .post(
                "/api/v1/jobs",
                Some(create_body("invalid environment", &definition)),
            )
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(body["error"]["code"], "invalid_request");
        for secret in [
            "collision-secret-one",
            "collision-secret-two",
            "reserved-secret-value",
        ] {
            assert!(
                !body_text(&body).contains(secret),
                "failed validation leaked {secret}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// History pagination and the 1000-run cap warning
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn history_pagination_and_bounded_page_validation() {
    let server = spawn_server();
    for name in ["a", "b", "c"] {
        let (_, body) = server
            .post(
                "/api/v1/jobs",
                Some(create_body(
                    name,
                    &definition("/bin/echo", false, false, false),
                )),
            )
            .await;
        let id = data(&body)["id"].as_str().expect("id").to_owned();
        let (status, body) = server.post(&format!("/api/v1/jobs/{id}/run"), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }

    let (status, body) = server.get("/api/v1/runs").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let history = data(&body);
    assert_eq!(history["total"], json!(3));
    assert_eq!(history["limit"], json!(20));
    assert_eq!(history["offset"], json!(0));
    assert_eq!(history["runs"].as_array().expect("runs").len(), 3);
    assert_eq!(body["warnings"], json!([]));

    // Pagination: limit and offset slice the newest-first history.
    let (status, body) = server.get("/api/v1/runs?limit=2&offset=1").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let history = data(&body);
    assert_eq!(history["total"], json!(3));
    assert_eq!(history["runs"].as_array().expect("runs").len(), 2);

    // Per-job filter.
    let (_, body) = server.get("/api/v1/jobs").await;
    let first = data(&body)[0]["id"].as_str().expect("id").to_owned();
    let (status, body) = server.get(&format!("/api/v1/runs?job={first}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["total"], json!(1));

    // Pages are bounded; complete totals no longer depend on a 1000-row window.
    let (status, body) = server.get("/api/v1/runs?limit=1000&offset=100").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(
        error(&body, "invalid_request"),
        "limit must be from 1 through 100"
    );
}

// ---------------------------------------------------------------------------
// Logs from a manually written final output artifact
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn logs_frames() {
    let server = spawn_server();
    let (_, body) = server
        .post(
            "/api/v1/jobs",
            Some(create_body(
                "alpha",
                &definition("/bin/echo", false, false, false),
            )),
        )
        .await;
    let id = data(&body)["id"].as_str().expect("id").to_owned();
    let (_, body) = server.post(&format!("/api/v1/jobs/{id}/run"), None).await;
    let run_id = data(&body)["run_id"].as_str().expect("run_id").to_owned();

    // No daemon runs in tests: write the final output artifact directly.
    let output_dir = server.paths.outputs.join(&run_id);
    let _output_guard =
        locron_core::filesystem::DirectoryGuard::private(&output_dir).expect("private output dir");
    let mut writer = FrameWriter::create(&output_dir.join("1.log")).expect("create");
    writer
        .write(FrameChannel::Stdout, 100, b"hello")
        .expect("stdout frame");
    writer
        .write(FrameChannel::Stderr, 250, b"oops")
        .expect("stderr frame");
    drop(writer);

    let (status, body) = server.get(&format!("/api/v1/runs/{run_id}/logs")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let logs = data(&body);
    assert_eq!(logs["run_id"], json!(run_id));
    assert_eq!(logs["attempt"], json!(1));
    let frames = logs["frames"].as_array().expect("frames");
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0]["channel"], json!("stdout"));
    assert_eq!(frames[0]["sequence"], json!(0));
    assert_eq!(frames[0]["elapsed_micros"], json!(100));
    assert_eq!(frames[0]["bytes"], json!("aGVsbG8="));
    assert_eq!(frames[0]["encoding"], json!("base64"));
    assert_eq!(frames[1]["channel"], json!("stderr"));
    assert_eq!(frames[1]["bytes"], json!("b29wcw=="));

    // Channel filter.
    let (status, body) = server
        .get(&format!("/api/v1/runs/{run_id}/logs?channel=stderr"))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let frames = data(&body)["frames"].as_array().expect("frames");
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0]["channel"], json!("stderr"));

    // Missing attempt: 404 output not found.
    let (status, body) = server
        .get(&format!("/api/v1/runs/{run_id}/logs?attempt=2"))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(error(&body, "not_found"), "output not found");
}

// ---------------------------------------------------------------------------
// Diagnostics facts
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn diagnostics_facts() {
    let server = spawn_server();
    let (_, _) = server
        .post(
            "/api/v1/jobs",
            Some(create_body(
                "resolved",
                &definition("/bin/echo", false, false, false),
            )),
        )
        .await;
    let (_, _) = server
        .post(
            "/api/v1/jobs",
            Some(create_body(
                "missing",
                &definition("definitely-not-installed-locron-xyz", false, false, false),
            )),
        )
        .await;
    let (_, body) = server
        .put(
            "/api/v1/settings/environment.GREETING",
            Some(json!({"value": "hi"})),
        )
        .await;
    assert_eq!(data(&body)["action"], json!("created"));

    let (status, body) = server.get("/api/v1/diagnostics").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let facts = data(&body);
    assert_eq!(facts["daemon_running"], json!(false));
    assert!(
        facts["database"]
            .as_str()
            .expect("database")
            .ends_with("state.db")
    );
    assert!(facts["checks"].is_array(), "checks is an array: {facts}");
    assert_eq!(
        facts["execution_path"],
        json!(locron_core::execution::default_execution_path())
    );
    assert_eq!(facts["global_environment_names"], json!(["GREETING"]));
    let resolutions = facts["process_resolution"].as_array().expect("resolutions");
    assert_eq!(resolutions.len(), 2, "{facts}");
    let by_name = |name: &str| {
        resolutions
            .iter()
            .find(|entry| entry["job_name"] == json!(name))
            .expect(name)
    };
    assert_eq!(by_name("resolved")["status"], json!("resolved"));
    assert_eq!(
        by_name("resolved")["resolved_executable"],
        json!(resolved_fixture_executable())
    );
    assert_eq!(by_name("missing")["status"], json!("unresolved"));
    assert_eq!(
        by_name("missing")["error"],
        json!("executable not found in execution path")
    );
}

// ---------------------------------------------------------------------------
// Redaction parity: no secret material escapes through any surface
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn redaction_parity() {
    let server = spawn_server();

    // A job whose definition carries secrets in env, header, and body.
    let (_, created_body) = server
        .post(
            "/api/v1/jobs",
            Some(create_body(
                "secrets",
                &definition("/bin/echo", true, true, true),
            )),
        )
        .await;
    let id = data(&created_body)["id"].as_str().expect("id").to_owned();
    let definition_json = data(&created_body)["definition_json"]
        .as_str()
        .expect("definition");
    assert!(definition_json.contains("<redacted>"), "{definition_json}");
    assert!(!definition_json.contains("super-secret-value"));
    assert!(!definition_json.contains("header-secret"));

    // Every surface that serializes the definition: no plaintext anywhere.
    let secrets = ["super-secret-value", "header-secret", "body-secret"];
    let surfaces = [
        "/api/v1/jobs",
        &format!("/api/v1/jobs/{id}"),
        &format!("/api/v1/jobs/{id}/why"),
    ];
    for surface in surfaces {
        let (status, body) = server.get(surface).await;
        assert_eq!(status, StatusCode::OK, "{surface}");
        let raw = body_text(&body);
        for secret in secrets {
            assert!(!raw.contains(secret), "{secret} leaked through {surface}");
        }
    }

    // Run surfaces carry the redacted snapshot.
    let (_, body) = server.post(&format!("/api/v1/jobs/{id}/run"), None).await;
    let run_id = data(&body)["run_id"].as_str().expect("run_id").to_owned();
    for surface in [
        &format!("/api/v1/runs/{run_id}"),
        &format!("/api/v1/runs/{run_id}/why"),
        "/api/v1/runs",
    ] {
        let (status, body) = server.get(surface).await;
        assert_eq!(status, StatusCode::OK, "{surface}");
        let raw = body_text(&body);
        for secret in secrets {
            assert!(!raw.contains(secret), "{secret} leaked through {surface}");
        }
        assert!(raw.contains("<redacted>"), "marker missing from {surface}");
    }

    // The settings environment shape is the configured/redacted pair.
    let (_, body) = server
        .put(
            "/api/v1/settings/environment.API_TOKEN",
            Some(json!({"value": "hunter2"})),
        )
        .await;
    assert_eq!(data(&body)["action"], json!("created"));
    let (_, settings) = server.get("/api/v1/settings").await;
    assert_eq!(
        data(&settings)["environment"]["API_TOKEN"],
        json!({"configured": true, "value_redacted": true})
    );
    assert!(!body_text(&settings).contains("hunter2"));
}

// ---------------------------------------------------------------------------
// SSE run stream (step 6)
// ---------------------------------------------------------------------------

/// One parsed SSE event block: the `event:` name and the JSON `data` payload.
#[derive(Debug)]
struct SseEvent {
    name: String,
    data: Value,
}

/// Parses one SSE block (`event:`/`data:` lines); comment blocks (keepalive
/// pings) are ignored.
fn parse_sse_block(block: &str) -> Option<SseEvent> {
    let mut name = None;
    let mut data = None;
    for line in block.lines() {
        if let Some(value) = line.strip_prefix("event:") {
            name = Some(value.trim().to_owned());
        } else if let Some(value) = line.strip_prefix("data:") {
            data = Some(serde_json::from_str(value.trim()).expect("event data"));
        }
    }
    match (name, data) {
        (Some(name), Some(data)) => Some(SseEvent { name, data }),
        _ => None,
    }
}

/// Reads the response body as SSE events until the server closes the stream,
/// or until `stop_after` events have been parsed (`None` reads to the end).
/// Dropping the returned reader's in-flight chunk stream closes the
/// connection, which is how a client disconnects mid-stream.
async fn read_sse(
    response: reqwest::Response,
    stop_after: Option<usize>,
    limit: Duration,
) -> Vec<SseEvent> {
    let mut events = Vec::new();
    let mut buffer = String::new();
    let mut chunks = response.bytes_stream();
    loop {
        if stop_after.is_some_and(|wanted| events.len() >= wanted) {
            break;
        }
        let chunk = tokio::time::timeout(limit, chunks.next()).await;
        let Ok(Some(chunk)) = chunk else { break };
        let chunk = chunk.expect("stream chunk");
        buffer.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(offset) = buffer.find("\n\n") {
            let block = buffer[..offset].to_owned();
            buffer = buffer[offset + 2..].to_owned();
            if let Some(event) = parse_sse_block(&block) {
                events.push(event);
            }
            if stop_after.is_some_and(|wanted| events.len() >= wanted) {
                break;
            }
        }
    }
    events
}

/// Whether `wanted` appears as a (not necessarily contiguous) subsequence of
/// the event names.
fn contains_subsequence(events: &[SseEvent], wanted: &[&str]) -> bool {
    let mut cursor = 0;
    for event in events {
        if cursor < wanted.len() && event.name == wanted[cursor] {
            cursor += 1;
        }
    }
    cursor == wanted.len()
}

fn event_names(events: &[SseEvent]) -> Vec<&str> {
    events.iter().map(|event| event.name.as_str()).collect()
}

/// The session cookie the entry-page token paste sets. EventSource cannot
/// send an Authorization header, so the stream authenticates by cookie.
async fn session_cookie_for(server: &TestServer) -> String {
    let response = server
        .client
        .post(format!("{}/api/v1/session", server.base))
        .json(&json!({"token": server.token}))
        .send()
        .await
        .expect("session request");
    let mut session = None;
    for cookie in response.headers().get_all(reqwest::header::SET_COOKIE) {
        let raw = cookie.to_str().expect("cookie header");
        if let Some(value) = raw
            .split(';')
            .next()
            .and_then(|pair| pair.strip_prefix("locron_session="))
        {
            session = Some(value.to_owned());
        }
    }
    session.expect("session cookie set")
}

/// Wall-clock microseconds, as the durable API timestamps use.
fn test_now_us() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock precedes the epoch")
        .as_micros()
        .try_into()
        .expect("micros fit i64")
}

#[tokio::test(flavor = "multi_thread")]
async fn sse_stream_live_run_events() {
    let server = spawn_server();
    let (_, body) = server
        .post(
            "/api/v1/jobs",
            Some(create_body(
                "alpha",
                &definition("/bin/echo", false, false, false),
            )),
        )
        .await;
    let id = data(&body)["id"].as_str().expect("id").to_owned();
    let (_, body) = server.post(&format!("/api/v1/jobs/{id}/run"), None).await;
    let run_id = data(&body)["run_id"].as_str().expect("run_id").to_owned();

    // The stream authenticates through the session cookie alone.
    let session = session_cookie_for(&server).await;
    let response = server
        .client
        .get(format!("{}/api/v1/runs/{run_id}/stream", server.base))
        .header(reqwest::header::COOKIE, format!("locron_session={session}"))
        .send()
        .await
        .expect("stream request");
    assert_eq!(
        response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .expect("content type"),
        "text/event-stream"
    );

    // Let the first poll observe the durably queued run, then drive the
    // lifecycle through the store's public admission and completion APIs (no
    // daemon runs in tests), pausing between stages so the 200 ms stream
    // poll observes each transition.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let store = locron_store::Store::open(
        server.paths.clone(),
        env!("CARGO_PKG_VERSION"),
        test_now_us(),
    )
    .expect("test store");
    let lifetime = uuid::Uuid::now_v7().to_string();
    store
        .begin_lifetime(&lifetime, test_now_us(), env!("CARGO_PKG_VERSION"))
        .expect("lifetime");
    let admission = store.admit(&lifetime, test_now_us(), 64).expect("admit");
    assert_eq!(admission.attempts.len(), 1);
    assert_eq!(admission.attempts[0].run_id, run_id);
    assert_eq!(admission.attempts[0].attempt_number, 1);
    tokio::time::sleep(Duration::from_millis(600)).await;

    // Live frames arrive from the in-progress partial artifact.
    let output_dir = server.paths.outputs.join(&run_id);
    let _output_guard =
        locron_core::filesystem::DirectoryGuard::private(&output_dir).expect("private output dir");
    let mut writer = FrameWriter::create(&output_dir.join("1.partial")).expect("partial create");
    writer
        .write(FrameChannel::Stdout, 100, b"hello")
        .expect("stdout frame");
    writer
        .write(FrameChannel::Stderr, 250, b"oops")
        .expect("stderr frame");
    drop(writer);
    tokio::time::sleep(Duration::from_millis(600)).await;

    store
        .mark_attempt_running(&run_id, 1, test_now_us())
        .expect("mark running");
    tokio::time::sleep(Duration::from_millis(600)).await;

    store
        .complete_attempt(&locron_store::AttemptCompletion {
            run_id: run_id.clone(),
            attempt_number: 1,
            now_us: test_now_us(),
            duration_us: 1_000,
            state: "succeeded".to_owned(),
            exit_code: Some(0),
            http_status: None,
            http_content_type: None,
            reason: "completed by fixture".to_owned(),
            retry: None,
        })
        .expect("complete attempt");

    // The stream ends after the single terminal event.
    let events = read_sse(response, None, Duration::from_secs(10)).await;
    assert!(
        contains_subsequence(
            &events,
            &[
                "run",
                "run",
                "attempt",
                "output",
                "output",
                "run",
                "attempt",
                "run",
                "attempt",
                "termination",
            ],
        ),
        "event order: {:?}",
        event_names(&events)
    );

    // Connect catch-up: the durable state as of opening the stream.
    assert_eq!(events[0].name, "run");
    assert_eq!(events[0].data["state"], json!("queued"));
    assert!(events[0].data.get("reason").is_some(), "{events:?}");

    // Run transitions in order: queued, starting, running, succeeded.
    let run_states: Vec<&str> = events
        .iter()
        .filter(|event| event.name == "run")
        .map(|event| event.data["state"].as_str().expect("state"))
        .collect();
    assert_eq!(run_states, ["queued", "starting", "running", "succeeded"]);

    // Attempt transitions: attempt 1 starting, then running.
    let attempts: Vec<Value> = events
        .iter()
        .filter(|event| event.name == "attempt")
        .map(|event| {
            json!([
                event.data["attempt_number"].as_i64(),
                event.data["state"].as_str()
            ])
        })
        .collect();
    assert_eq!(
        Value::Array(attempts),
        json!([[1, "starting"], [1, "running"], [1, "succeeded"]])
    );

    // Output frames: ordered seqs with the CLI's payloads, base64-encoded.
    let outputs: Vec<&Value> = events
        .iter()
        .filter(|event| event.name == "output")
        .map(|event| &event.data)
        .collect();
    assert_eq!(outputs.len(), 2);
    assert_eq!(outputs[0]["attempt_number"], json!(1));
    assert_eq!(outputs[0]["channel"], json!("stdout"));
    assert_eq!(outputs[0]["seq"], json!(0));
    assert_eq!(outputs[0]["elapsed_us"], json!(100));
    assert_eq!(outputs[0]["data_b64"], json!("aGVsbG8="));
    assert_eq!(outputs[1]["channel"], json!("stderr"));
    assert_eq!(outputs[1]["seq"], json!(1));
    assert_eq!(outputs[1]["data_b64"], json!("b29wcw=="));

    // Exactly one terminal event, and it is the last one: the server closes
    // the stream right after finalization.
    let terminations: Vec<&SseEvent> = events
        .iter()
        .filter(|event| event.name == "termination")
        .collect();
    assert_eq!(terminations.len(), 1, "events: {:?}", event_names(&events));
    assert_eq!(events.last().expect("last").name, "termination");
    assert_eq!(terminations[0].data["state"], json!("succeeded"));
    assert_eq!(
        terminations[0].data["reason"],
        json!("completed by fixture")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn sse_stream_reconnect_idempotent() {
    let server = spawn_server();
    let (_, body) = server
        .post(
            "/api/v1/jobs",
            Some(create_body(
                "alpha",
                &definition("/bin/echo", false, false, false),
            )),
        )
        .await;
    let id = data(&body)["id"].as_str().expect("id").to_owned();
    let (_, body) = server.post(&format!("/api/v1/jobs/{id}/run"), None).await;
    let run_id = data(&body)["run_id"].as_str().expect("run_id").to_owned();

    // Terminal fixture: one admitted and successfully completed attempt with
    // a finalized output artifact.
    let store = locron_store::Store::open(
        server.paths.clone(),
        env!("CARGO_PKG_VERSION"),
        test_now_us(),
    )
    .expect("test store");
    let lifetime = uuid::Uuid::now_v7().to_string();
    store
        .begin_lifetime(&lifetime, test_now_us(), env!("CARGO_PKG_VERSION"))
        .expect("lifetime");
    let admission = store
        .admit(&lifetime, test_now_us(), 64)
        .expect("admission");
    assert_eq!(admission.attempts[0].attempt_number, 1);
    let output_dir = server.paths.outputs.join(&run_id);
    let _output_guard =
        locron_core::filesystem::DirectoryGuard::private(&output_dir).expect("private output dir");
    let mut writer = FrameWriter::create(&output_dir.join("1.log")).expect("create");
    writer
        .write(FrameChannel::Stdout, 42, b"final")
        .expect("frame");
    drop(writer);
    store
        .complete_attempt(&locron_store::AttemptCompletion {
            run_id: run_id.clone(),
            attempt_number: 1,
            now_us: test_now_us(),
            duration_us: 42,
            state: "succeeded".to_owned(),
            exit_code: Some(0),
            http_status: None,
            http_content_type: None,
            reason: "completed by fixture".to_owned(),
            retry: None,
        })
        .expect("complete attempt");

    // Each connect is idempotent: the catch-up run event, the retained
    // frames, and exactly one terminal event.
    for _ in 0..2 {
        let response = server
            .client
            .get(format!("{}/api/v1/runs/{run_id}/stream", server.base))
            .header(
                reqwest::header::AUTHORIZATION,
                format!("token {}", server.token),
            )
            .send()
            .await
            .expect("stream request");
        let events = read_sse(response, None, Duration::from_secs(5)).await;
        assert_eq!(
            event_names(&events),
            ["run", "attempt", "output", "termination"],
            "events: {events:?}"
        );
        assert_eq!(events[0].data["state"], json!("succeeded"));
        assert_eq!(events[1].data["attempt_number"], json!(1));
        assert_eq!(events[2].data["seq"], json!(0));
        assert_eq!(events[2].data["attempt_number"], json!(1));
        assert_eq!(events[2].data["data_b64"], json!("ZmluYWw="));
        assert_eq!(events[3].data["state"], json!("succeeded"));
        assert_eq!(events[3].data["reason"], json!("completed by fixture"));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn sse_output_keys_sequence_by_attempt() {
    let server = spawn_server();
    let (_, body) = server
        .post(
            "/api/v1/jobs",
            Some(create_body(
                "retrying",
                &definition("/bin/echo", false, false, false),
            )),
        )
        .await;
    let id = data(&body)["id"].as_str().expect("id").to_owned();
    let (_, body) = server.post(&format!("/api/v1/jobs/{id}/run"), None).await;
    let run_id = data(&body)["run_id"].as_str().expect("run_id").to_owned();
    let store = locron_store::Store::open(
        server.paths.clone(),
        env!("CARGO_PKG_VERSION"),
        test_now_us(),
    )
    .expect("test store");
    let lifetime = uuid::Uuid::now_v7().to_string();
    let now = test_now_us();
    store
        .begin_lifetime(&lifetime, now, env!("CARGO_PKG_VERSION"))
        .expect("lifetime");
    let first = store.admit(&lifetime, now, 64).expect("first admission");
    assert_eq!(first.attempts[0].attempt_number, 1);
    let output_dir = server.paths.outputs.join(&run_id);
    let _output_guard =
        locron_core::filesystem::DirectoryGuard::private(&output_dir).expect("private output dir");
    let mut first_output = FrameWriter::create(&output_dir.join("1.log")).expect("first output");
    first_output
        .write(FrameChannel::Stdout, 10, b"first")
        .expect("first frame");
    drop(first_output);
    store
        .complete_attempt(&locron_store::AttemptCompletion {
            run_id: run_id.clone(),
            attempt_number: 1,
            now_us: now + 1,
            duration_us: 1,
            state: "failed".to_owned(),
            exit_code: Some(1),
            http_status: None,
            http_content_type: None,
            reason: "retry fixture".to_owned(),
            retry: Some(locron_store::RetryPlan {
                not_before_us: now + 2,
                classification: "process_exit".to_owned(),
            }),
        })
        .expect("retry completion");
    let second = store
        .admit(&lifetime, now + 2, 64)
        .expect("second admission");
    assert_eq!(second.attempts[0].attempt_number, 2);
    let mut second_output = FrameWriter::create(&output_dir.join("2.log")).expect("second output");
    second_output
        .write(FrameChannel::Stdout, 20, b"second")
        .expect("second frame");
    drop(second_output);
    store
        .complete_attempt(&locron_store::AttemptCompletion {
            run_id: run_id.clone(),
            attempt_number: 2,
            now_us: now + 3,
            duration_us: 1,
            state: "succeeded".to_owned(),
            exit_code: Some(0),
            http_status: None,
            http_content_type: None,
            reason: "retry succeeded".to_owned(),
            retry: None,
        })
        .expect("terminal completion");

    let response = server
        .client
        .get(format!("{}/api/v1/runs/{run_id}/stream", server.base))
        .header(
            reqwest::header::AUTHORIZATION,
            format!("token {}", server.token),
        )
        .send()
        .await
        .expect("stream request");
    let events = read_sse(response, None, Duration::from_secs(5)).await;
    let output_keys = events
        .iter()
        .filter(|event| event.name == "output")
        .map(|event| {
            json!([
                event.data["attempt_number"].as_i64(),
                event.data["seq"].as_u64()
            ])
        })
        .collect::<Vec<_>>();
    assert_eq!(output_keys, [json!([1, 0]), json!([2, 0])]);
}

#[tokio::test(flavor = "multi_thread")]
async fn sse_stream_rejects_unknown_run() {
    let server = spawn_server();
    let (status, body) = server.get("/api/v1/runs/not-a-uuid/stream").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(error(&body, "invalid_request"), "invalid run UUID");

    let (status, body) = server
        .get("/api/v1/runs/00000000-0000-0000-0000-000000000000/stream")
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    // The CLI's category message is the raw run reference.
    assert_eq!(
        error(&body, "not_found"),
        "00000000-0000-0000-0000-000000000000"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn sse_stream_disconnect_never_cancels() {
    let server = spawn_server();
    let (_, body) = server
        .post(
            "/api/v1/jobs",
            Some(create_body(
                "alpha",
                &definition("/bin/echo", false, false, false),
            )),
        )
        .await;
    let id = data(&body)["id"].as_str().expect("id").to_owned();
    let (_, body) = server.post(&format!("/api/v1/jobs/{id}/run"), None).await;
    let run_id = data(&body)["run_id"].as_str().expect("run_id").to_owned();

    let response = server
        .client
        .get(format!("{}/api/v1/runs/{run_id}/stream", server.base))
        .header(
            reqwest::header::COOKIE,
            format!("locron_session={}", session_cookie_for(&server).await),
        )
        .send()
        .await
        .expect("stream request");

    // The client receives the connect catch-up, then goes away mid-stream:
    // dropping the chunk reader closes the connection.
    let events = read_sse(response, Some(1), Duration::from_secs(5)).await;
    assert_eq!(events.len(), 1, "events: {events:?}");
    assert_eq!(events[0].name, "run");
    assert_eq!(events[0].data["state"], json!("queued"));

    // Several stream polls pass with the connection gone. The run must
    // remain durably queued and still cancellable: following a run never
    // cancels it, and disconnecting does not either.
    tokio::time::sleep(Duration::from_millis(800)).await;
    let (status, body) = server.get(&format!("/api/v1/runs/{run_id}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["state"], json!("queued"));
    let (status, body) = server
        .post(&format!("/api/v1/runs/{run_id}/cancel"), None)
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["cancelled"], json!(true), "{body}");
}

// ---------------------------------------------------------------------------
// PR144: real HTTP body/query flags and closed-writer dry-run conservation
// ---------------------------------------------------------------------------

mod dry_run_http_qualification {
    use std::collections::BTreeMap;
    use std::fs;
    use std::io::Read;
    use std::net::SocketAddr;
    use std::path::{Path, PathBuf};

    use locron_core::filesystem;
    use locron_server::{AppState, router};
    use locron_store::{StatePaths, Store};
    use reqwest::StatusCode;
    use serde_json::{Value, json};
    use sha2::{Digest, Sha256};

    use super::{TestServer, body_text, create_body, data, definition, error, spawn_server};

    fn body_flags() -> [(&'static str, Option<Value>, bool); 9] {
        [
            ("boolean-true", Some(json!(true)), true),
            ("boolean-false", Some(json!(false)), false),
            ("text-empty", Some(json!("")), true),
            ("text-true", Some(json!("true")), true),
            ("text-one", Some(json!("1")), true),
            ("text-false", Some(json!("false")), false),
            ("text-zero", Some(json!("0")), false),
            ("null", Some(Value::Null), true),
            ("omitted", None, false),
        ]
    }

    fn invalid_body_flags() -> [(&'static str, Value); 6] {
        [
            ("number-zero", json!(0)),
            ("number-one", json!(1)),
            ("array", json!([])),
            ("object", json!({})),
            ("uppercase", json!("TRUE")),
            ("unsupported-text", json!("yes")),
        ]
    }

    fn with_body_flag(mut body: Value, flag: Option<&Value>) -> Value {
        if let Some(flag) = flag {
            body["dry_run"] = flag.clone();
        }
        body
    }

    fn query_flags() -> [(&'static str, Option<&'static str>, bool); 7] {
        [
            ("omitted", None, false),
            ("bare", Some(""), true),
            ("empty", Some("="), true),
            ("true", Some("=true"), true),
            ("one", Some("=1"), true),
            ("false", Some("=false"), false),
            ("zero", Some("=0"), false),
        ]
    }

    fn query_path(base: &str, key: &str, suffix: Option<&str>) -> String {
        match suffix {
            Some(suffix) => {
                let separator = if base.contains('?') { '&' } else { '?' };
                format!("{base}{separator}{key}{suffix}")
            }
            None => base.to_owned(),
        }
    }

    impl TestServer {
        async fn raw_response(
            &self,
            method: &str,
            path: &str,
            body: Option<&Value>,
        ) -> (StatusCode, String) {
            let mut request = self
                .client
                .request(
                    reqwest::Method::from_bytes(method.as_bytes()).expect("method"),
                    format!("{}{path}", self.base),
                )
                .header(
                    reqwest::header::AUTHORIZATION,
                    format!("token {}", self.token),
                );
            if let Some(body) = body {
                request = request.json(body);
            }
            let response = request.send().await.expect("selected HTTP request");
            let status = response.status();
            let text = response.text().await.expect("selected HTTP response");
            (status, text)
        }

        async fn stop_and_join(&mut self) {
            self.task.abort();
            if let Err(error) = (&mut self.task).await {
                assert!(error.is_cancelled(), "HTTP fixture server failed to join");
            }
        }
    }

    // Deliberately does not call ensure/token::ensure: this proves only the
    // handlers' absent-state behavior, not production bootstrap or token safety.
    fn spawn_missing_root() -> TestServer {
        let temp = tempfile::tempdir().expect("missing-state fixture parent");
        let paths = StatePaths::new(temp.path().join("absent"));
        assert!(!paths.root.exists());
        let token = "a".repeat(64);
        let listener = std::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
            .expect("bind missing-state fixture");
        listener.set_nonblocking(true).expect("nonblocking");
        let listener = tokio::net::TcpListener::from_std(listener).expect("tokio listener");
        let port = listener.local_addr().expect("local addr").port();
        let app = router(AppState {
            paths: paths.clone(),
            token: token.clone(),
            bound_port: port,
        });
        let task = tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("missing-state fixture server");
        });
        TestServer {
            base: format!("http://127.0.0.1:{port}"),
            token,
            client: reqwest::Client::builder()
                .no_proxy()
                .build()
                .expect("loopback client"),
            paths,
            task,
            _temp: temp,
        }
    }

    #[derive(PartialEq, Eq)]
    enum ManagedEntry {
        Directory,
        File { length: u64, sha256: [u8; 32] },
    }

    // Fixed fixture bounds, not an application retention or path policy.
    fn managed_files(root: &Path) -> BTreeMap<PathBuf, ManagedEntry> {
        const MAX_ENTRIES: usize = 64;
        const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
        const MAX_TOTAL_BYTES: u64 = 8 * 1024 * 1024;
        let mut entries = BTreeMap::new();
        let mut pending = vec![PathBuf::new()];
        let mut total = 0_u64;
        while let Some(relative) = pending.pop() {
            let path = root.join(&relative);
            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(error)
                    if error.kind() == std::io::ErrorKind::NotFound
                        && relative.as_os_str().is_empty() =>
                {
                    continue;
                }
                Err(error) => panic!("cannot inspect managed fixture entry: {:?}", error.kind()),
            };
            assert!(
                !metadata.file_type().is_symlink(),
                "unexpected managed symlink"
            );
            assert!(entries.len() < MAX_ENTRIES, "managed fixture entry cap");
            let entry = if metadata.is_dir() {
                for child in fs::read_dir(&path).expect("managed fixture directory") {
                    let child = child.expect("managed fixture directory entry");
                    pending.push(relative.join(child.file_name()));
                    assert!(
                        pending.len() + entries.len() <= MAX_ENTRIES,
                        "managed fixture traversal cap"
                    );
                }
                ManagedEntry::Directory
            } else {
                assert!(metadata.is_file(), "unexpected managed entry type");
                assert!(metadata.len() <= MAX_FILE_BYTES, "managed fixture file cap");
                let mut file = filesystem::open_private(&path, fs::OpenOptions::new().read(true))
                    .expect("managed fixture private read");
                let mut bytes = Vec::new();
                (&mut *file)
                    .take(MAX_FILE_BYTES + 1)
                    .read_to_end(&mut bytes)
                    .expect("managed fixture bounded read");
                let length = u64::try_from(bytes.len()).expect("fixture byte length");
                assert_eq!(
                    length,
                    metadata.len(),
                    "managed fixture changed during snapshot"
                );
                total += length;
                assert!(total <= MAX_TOTAL_BYTES, "managed fixture total cap");
                ManagedEntry::File {
                    length,
                    sha256: Sha256::digest(&bytes).into(),
                }
            };
            assert!(
                entries.insert(relative, entry).is_none(),
                "duplicate managed fixture entry"
            );
        }
        entries
    }

    struct DurableSnapshot {
        records: Value,
        files: BTreeMap<PathBuf, ManagedEntry>,
    }

    fn durable_snapshot(paths: &StatePaths) -> DurableSnapshot {
        // All setup/API writers have already returned and closed. This reader
        // also closes before byte hashing, and is never retained over a request.
        let records = {
            let store = Store::open_read_only(&paths.database).expect("read-only fixture state");
            json!({
                "settings": store.settings().expect("fixture settings"),
                "jobs": store.list_jobs(true).expect("fixture jobs and current revisions"),
                "history": store.history(None, 100).expect("fixture history"),
            })
        };
        DurableSnapshot {
            records,
            files: managed_files(&paths.root),
        }
    }

    fn assert_preserved(before: &DurableSnapshot, paths: &StatePaths, label: &str) {
        let after = durable_snapshot(paths);
        // Equality deliberately never renders plaintext settings, definitions,
        // tokens, managed paths or bytes in a failed assertion.
        assert!(
            before.records == after.records,
            "{label}: durable records changed"
        );
        assert!(
            before.files == after.files,
            "{label}: managed files changed"
        );
    }

    async fn seed_existing_state(server: &TestServer) {
        let (status, body) = server
            .post(
                "/api/v1/jobs",
                Some(create_body(
                    "original",
                    &definition("/bin/echo", true, false, false),
                )),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "fixture job creation");
        let id = data(&body)["id"].as_str().expect("fixture id");
        let (status, body) = server.post(&format!("/api/v1/jobs/{id}/run"), None).await;
        assert_eq!(status, StatusCode::OK, "fixture queued history");
        assert_eq!(data(&body)["state"], "queued");
        {
            let store =
                Store::open(server.paths.clone(), "contract", 2).expect("seed private store");
            store
                .set_environment("EXISTING", Some("original-private-value"), 3)
                .expect("seed existing environment");
            store
                .set_environment("UNCHANGED", Some("untouched-private-value"), 4)
                .expect("seed unrelated environment");
        }
    }

    fn assert_redacted(body: &Value) {
        let rendered = body_text(body);
        for value in [
            "original-private-value",
            "untouched-private-value",
            "replacement-private-value",
            "second-private-value",
            "super-secret-value",
        ] {
            assert!(
                !rendered.contains(value),
                "private value escaped selected response"
            );
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn accepted_json_body_flags_select_real_preview_or_live_mutation() {
        let mut server = spawn_server();
        seed_existing_state(&server).await;
        for (label, flag, preview) in body_flags() {
            let mut request = create_body(
                &format!("create-{label}"),
                &definition("/bin/echo", true, false, false),
            );
            request["enabled"] = json!(false);
            request["description"] = Value::Null;
            let before = durable_snapshot(&server.paths);
            let (status, body) = server
                .post("/api/v1/jobs", Some(with_body_flag(request, flag.as_ref())))
                .await;
            assert_eq!(status, StatusCode::OK, "create {label}");
            assert_redacted(&body);
            assert_eq!(data(&body)["enabled"], false);
            assert_eq!(data(&body)["description"], Value::Null);
            if preview {
                assert_eq!(data(&body)["dry_run"], true);
                assert_eq!(data(&body)["id"], "<non-durable>");
                assert_preserved(&before, &server.paths, label);
            } else {
                assert!(data(&body).get("dry_run").is_none());
                let store = Store::open_read_only(&server.paths.database).expect("created state");
                let job = store
                    .job(&format!("create-{label}"))
                    .expect("actual created job");
                assert_eq!(job.current_revision, 1);
                assert!(!job.enabled);
                assert!(job.description.is_none());
            }

            let update_name = format!("update-{label}");
            let (status, body) = server
                .post(
                    "/api/v1/jobs",
                    Some(create_body(
                        &update_name,
                        &definition("/bin/echo", true, false, false),
                    )),
                )
                .await;
            assert_eq!(status, StatusCode::OK, "update fixture {label}");
            let id = data(&body)["id"].as_str().expect("update id").to_owned();
            let mut request = json!({"name": format!("changed-{label}"), "enabled": false});
            match label {
                "boolean-true" | "boolean-false" | "omitted" => {}
                "text-empty" | "text-true" | "text-false" => request["description"] = Value::Null,
                _ => request["description"] = json!("changed description"),
            }
            let expected_description = request
                .get("description")
                .cloned()
                .unwrap_or_else(|| json!("contract fixture"));
            let before = durable_snapshot(&server.paths);
            let (status, body) = server
                .put(
                    &format!("/api/v1/jobs/{id}"),
                    Some(with_body_flag(request, flag.as_ref())),
                )
                .await;
            assert_eq!(status, StatusCode::OK, "update {label}");
            assert_redacted(&body);
            if preview {
                assert_eq!(data(&body)["dry_run"], true);
                assert_eq!(data(&body)["revision"], 2);
                assert_eq!(data(&body)["after"]["description"], expected_description);
                assert_eq!(data(&body)["after"]["enabled"], false);
                assert!(
                    data(&body)["changed_fields"]
                        .as_array()
                        .expect("changed fields")
                        .contains(&json!("name"))
                );
                assert_preserved(&before, &server.paths, label);
            } else {
                assert!(data(&body).get("dry_run").is_none());
                let store = Store::open_read_only(&server.paths.database).expect("updated state");
                let job = store.job(&id).expect("actual updated job");
                assert_eq!(job.name, format!("changed-{label}"));
                assert_eq!(job.current_revision, 2);
                assert!(!job.enabled);
                assert!(
                    serde_json::to_value(job.description).expect("description")
                        == expected_description,
                    "update description {label}"
                );
            }

            // Reset through a completed live request so every selected scalar
            // request would change the existing value, then close that writer.
            let (status, _) = server
                .put(
                    "/api/v1/settings/global_concurrency",
                    Some(json!({"value": "16"})),
                )
                .await;
            assert_eq!(status, StatusCode::OK);
            let before = durable_snapshot(&server.paths);
            let (status, body) = server
                .put(
                    "/api/v1/settings/global_concurrency",
                    Some(with_body_flag(json!({"value": "4"}), flag.as_ref())),
                )
                .await;
            assert_eq!(status, StatusCode::OK, "scalar {label}");
            if preview {
                assert_eq!(
                    data(&body),
                    &json!({"key": "global_concurrency", "value": "4", "dry_run": true})
                );
                assert_preserved(&before, &server.paths, label);
            } else {
                assert_eq!(data(&body)["global_concurrency"], 4);
                let store = Store::open_read_only(&server.paths.database).expect("scalar state");
                assert_eq!(
                    store
                        .settings()
                        .expect("actual scalar value")
                        .global_concurrency,
                    4
                );
            }
        }
        server.stop_and_join().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn environment_body_flags_preserve_complete_closed_state_or_persist_values() {
        let mut server = spawn_server();
        seed_existing_state(&server).await;
        for (label, flag, preview) in body_flags() {
            for name in ["EXISTING", "NEW_VALUE"] {
                {
                    let store = Store::open(server.paths.clone(), "contract", 5)
                        .expect("environment fixture reset");
                    store
                        .set_environment("EXISTING", Some("original-private-value"), 6)
                        .expect("restore original map");
                    store
                        .set_environment("NEW_VALUE", None, 7)
                        .expect("clear new fixture name");
                }
                let before = durable_snapshot(&server.paths);
                let (status, body) = server
                    .put(
                        &format!("/api/v1/settings/environment.{name}"),
                        Some(with_body_flag(
                            json!({"value": "replacement-private-value"}),
                            flag.as_ref(),
                        )),
                    )
                    .await;
                assert_eq!(status, StatusCode::OK, "environment {name} {label}");
                assert_eq!(
                    data(&body),
                    &json!({"key": format!("environment.{name}"), "action": if name == "EXISTING" { "replaced" } else { "created" }, "configured": true, "value_redacted": true, "dry_run": preview})
                );
                assert_redacted(&body);
                if preview {
                    assert_preserved(&before, &server.paths, label);
                } else {
                    let store =
                        Store::open_read_only(&server.paths.database).expect("environment state");
                    let mut expected = BTreeMap::from([
                        ("EXISTING".to_owned(), "original-private-value".to_owned()),
                        ("UNCHANGED".to_owned(), "untouched-private-value".to_owned()),
                    ]);
                    expected.insert(name.to_owned(), "replacement-private-value".to_owned());
                    assert!(
                        store.settings().expect("actual environment").environment == expected,
                        "live complete environment {label}"
                    );
                    drop(store);
                    let (status, body) = server
                        .put(
                            &format!("/api/v1/settings/environment.{name}"),
                            Some(with_body_flag(
                                json!({"value": "second-private-value"}),
                                flag.as_ref(),
                            )),
                        )
                        .await;
                    assert_eq!(status, StatusCode::OK, "live replacement {label}");
                    assert_eq!(data(&body)["action"], "replaced");
                    assert_eq!(data(&body)["dry_run"], false);
                    assert_redacted(&body);
                    expected.insert(name.to_owned(), "second-private-value".to_owned());
                    let store =
                        Store::open_read_only(&server.paths.database).expect("replacement state");
                    assert!(
                        store.settings().expect("actual replacement").environment == expected,
                        "replacement complete environment {label}"
                    );
                }
            }
        }
        server.stop_and_join().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn invalid_json_flags_and_other_body_types_refuse_before_durable_changes() {
        let mut server = spawn_server();
        seed_existing_state(&server).await;
        for (label, flag) in invalid_body_flags() {
            for (method, path, request) in [
                (
                    "POST",
                    "/api/v1/jobs",
                    create_body(
                        "invalid-create",
                        &definition("/bin/echo", false, false, false),
                    ),
                ),
                (
                    "PUT",
                    "/api/v1/jobs/original",
                    json!({"description": "a real change"}),
                ),
                (
                    "PUT",
                    "/api/v1/settings/global_concurrency",
                    json!({"value": "4"}),
                ),
                (
                    "PUT",
                    "/api/v1/settings/environment.EXISTING",
                    json!({"value": "replacement-private-value"}),
                ),
            ] {
                let before = durable_snapshot(&server.paths);
                let request = with_body_flag(request, Some(&flag));
                let (status, text) = server.raw_response(method, path, Some(&request)).await;
                assert_eq!(
                    status,
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "{label} extractor refusal"
                );
                assert!(
                    !text.contains("replacement-private-value"),
                    "extractor leaked value"
                );
                assert_preserved(&before, &server.paths, label);
            }
        }
        let mut bad_enabled = create_body(
            "invalid-enabled",
            &definition("/bin/echo", false, false, false),
        );
        bad_enabled["enabled"] = json!("true");
        for (method, path, request) in [
            ("POST", "/api/v1/jobs", bad_enabled),
            ("PUT", "/api/v1/jobs/original", json!({"enabled": "false"})),
            ("PUT", "/api/v1/jobs/original", json!({"description": 1})),
            (
                "PUT",
                "/api/v1/settings/global_concurrency",
                json!({"value": 4}),
            ),
            (
                "PUT",
                "/api/v1/settings/environment.EXISTING",
                json!({"value": true}),
            ),
        ] {
            let before = durable_snapshot(&server.paths);
            let (status, _) = server.raw_response(method, path, Some(&request)).await;
            assert_eq!(
                status,
                StatusCode::UNPROCESSABLE_ENTITY,
                "nonflag body type refusal"
            );
            assert_preserved(&before, &server.paths, "nonflag body type");
        }
        for (key, value, message) in [
            (
                "environment.1INVALID",
                "replacement-private-value",
                "invalid or reserved environment name 1INVALID",
            ),
            (
                "environment.LOCRON_X",
                "replacement-private-value",
                "invalid or reserved environment name LOCRON_X",
            ),
            (
                "environment.EXISTING",
                "contains\0nul",
                "environment value for EXISTING contains NUL",
            ),
        ] {
            for preview in [true, false] {
                let before = durable_snapshot(&server.paths);
                let (status, body) = server
                    .put(
                        &format!("/api/v1/settings/{key}"),
                        Some(json!({"value": value, "dry_run": preview})),
                    )
                    .await;
                assert_eq!(status, StatusCode::BAD_REQUEST);
                assert_eq!(error(&body, "invalid_request"), message);
                assert!(
                    !body_text(&body).contains(value),
                    "invalid environment value leaked"
                );
                assert_preserved(&before, &server.paths, "invalid environment");
            }
        }
        server.stop_and_join().await;
    }

    async fn assert_absent_previews(server: &TestServer) {
        assert!(!server.paths.database.exists());
        for (label, flag, preview) in body_flags() {
            if !preview {
                continue;
            }
            for (method, path, request) in [
                (
                    "POST",
                    "/api/v1/jobs",
                    create_body(
                        "absent-preview",
                        &definition("/bin/echo", true, false, false),
                    ),
                ),
                (
                    "PUT",
                    "/api/v1/settings/global_concurrency",
                    json!({"value": "4"}),
                ),
                (
                    "PUT",
                    "/api/v1/settings/environment.NEW_VALUE",
                    json!({"value": "replacement-private-value"}),
                ),
            ] {
                let before = managed_files(&server.paths.root);
                let (status, body) = server
                    .send(method, path, Some(with_body_flag(request, flag.as_ref())))
                    .await;
                assert_eq!(status, StatusCode::OK, "absent {label}");
                assert_eq!(data(&body)["dry_run"], true);
                assert_redacted(&body);
                assert!(
                    before == managed_files(&server.paths.root),
                    "absent preview changed managed entries"
                );
                assert!(!server.paths.database.exists());
            }
            let before = managed_files(&server.paths.root);
            let (status, body) = server
                .put(
                    "/api/v1/jobs/missing",
                    Some(with_body_flag(
                        json!({"description": "changed"}),
                        flag.as_ref(),
                    )),
                )
                .await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "absent update {label}");
            assert_eq!(
                error(&body, "invalid_request"),
                "state database does not exist"
            );
            assert!(
                before == managed_files(&server.paths.root),
                "absent update changed managed entries"
            );
            assert!(!server.paths.database.exists());
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn all_true_body_forms_leave_an_absent_database_uninitialized() {
        let mut server = spawn_server();
        assert_absent_previews(&server).await;
        server.stop_and_join().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn all_true_body_forms_leave_an_absent_state_root_uncreated() {
        let mut server = spawn_missing_root();
        assert_absent_previews(&server).await;
        assert!(!server.paths.root.exists());
        server.stop_and_join().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn body_dry_run_is_independent_of_the_query_spelling() {
        let mut server = spawn_server();
        seed_existing_state(&server).await;
        let before = durable_snapshot(&server.paths);
        let request = with_body_flag(
            create_body(
                "body-preview",
                &definition("/bin/echo", false, false, false),
            ),
            Some(&json!(true)),
        );
        let (status, body) = server
            .post("/api/v1/jobs?dry-run=false", Some(request))
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(data(&body)["id"], "<non-durable>");
        assert_preserved(&before, &server.paths, "body takes its own dry_run");
        let (status, body) = server
            .post(
                "/api/v1/jobs?dry-run=true",
                Some(create_body(
                    "query-does-not-preview",
                    &definition("/bin/echo", false, false, false),
                )),
            )
            .await;
        assert_eq!(status, StatusCode::OK);
        assert!(data(&body).get("dry_run").is_none());
        {
            let store =
                Store::open_read_only(&server.paths.database).expect("body versus query state");
            assert_eq!(
                store
                    .job("query-does-not-preview")
                    .expect("live body mutation")
                    .current_revision,
                1
            );
        }
        server.stop_and_join().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn query_flags_reach_jobs_run_cancel_and_prune_handlers() {
        let mut server = spawn_server();
        seed_existing_state(&server).await;
        let mut disabled = create_body("disabled", &definition("/bin/echo", false, false, false));
        disabled["enabled"] = json!(false);
        let (status, _) = server.post("/api/v1/jobs", Some(disabled)).await;
        assert_eq!(status, StatusCode::OK);
        for (label, suffix, selected) in query_flags() {
            let (status, body) = server.get(&query_path("/api/v1/jobs", "all", suffix)).await;
            assert_eq!(status, StatusCode::OK, "jobs all {label}");
            let names: Vec<&str> = data(&body)
                .as_array()
                .expect("jobs")
                .iter()
                .map(|job| job["name"].as_str().expect("job name"))
                .collect();
            let expected = if selected {
                vec!["disabled", "original"]
            } else {
                vec!["original"]
            };
            assert_eq!(names, expected, "jobs all {label}");

            // All wait=true syntax stays in preview; no live wait, runnable
            // child, daemon or new outbound network fixture is admitted.
            let before = durable_snapshot(&server.paths);
            let (status, body) = server
                .post(
                    &query_path("/api/v1/jobs/original/run?dry-run=1", "wait", suffix),
                    None,
                )
                .await;
            assert_eq!(status, StatusCode::OK, "run wait {label}");
            assert_eq!(
                data(&body),
                &json!({"dry_run": true, "durable": false, "decision": "would_skip_overlap", "capacity_reserved": false})
            );
            assert_preserved(&before, &server.paths, label);

            // Valid syntax reaches the UUID validator. This does not claim
            // acknowledgement of a running/unconfirmed process.
            let (status, body) = server
                .post(
                    &query_path(
                        "/api/v1/runs/not-a-uuid/cancel",
                        "acknowledge-unconfirmed",
                        suffix,
                    ),
                    None,
                )
                .await;
            assert_eq!(status, StatusCode::BAD_REQUEST);
            assert_eq!(error(&body, "invalid_request"), "invalid run UUID");
            assert_preserved(&before, &server.paths, label);

            let (status, body) = server
                .post(&query_path("/api/v1/prune", "dry-run", suffix), None)
                .await;
            assert_eq!(status, StatusCode::OK, "prune {label}");
            assert_eq!(
                data(&body),
                &json!({"dry_run": selected, "candidate_count": 0, "bytes": 0})
            );
            if selected {
                assert_preserved(&before, &server.paths, label);
            }
        }
        for (label, suffix, preview) in query_flags() {
            let reference = format!("run-{label}");
            let (status, _) = server
                .post(
                    "/api/v1/jobs",
                    Some(create_body(
                        &reference,
                        &definition("/bin/echo", true, false, false),
                    )),
                )
                .await;
            assert_eq!(status, StatusCode::OK, "run fixture {label}");
            let before = durable_snapshot(&server.paths);
            let count = before.records["history"].as_array().expect("history").len();
            let (status, body) = server
                .post(
                    &query_path(&format!("/api/v1/jobs/{reference}/run"), "dry-run", suffix),
                    None,
                )
                .await;
            assert_eq!(status, StatusCode::OK, "run dry-run {label}");
            if preview {
                assert_eq!(data(&body)["dry_run"], true);
                assert_eq!(data(&body)["durable"], false);
                assert_eq!(data(&body)["capacity_reserved"], false);
                assert_preserved(&before, &server.paths, label);
            } else {
                assert_eq!(data(&body)["state"], "queued");
                let run_id = data(&body)["run_id"]
                    .as_str()
                    .expect("live queued identity");
                let store = Store::open_read_only(&server.paths.database).expect("queued state");
                assert_eq!(
                    store.count_runs(None).expect("actual count"),
                    i64::try_from(count + 1).expect("count")
                );
                assert_eq!(
                    store.run(run_id).expect("actual queued run").state,
                    "queued"
                );
                assert!(
                    store
                        .attempts_for_run(run_id)
                        .expect("not executed")
                        .is_empty()
                );
            }
        }
        server.stop_and_join().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn export_and_import_query_forms_enforce_plaintext_acknowledgement_and_preview() {
        let mut server = spawn_server();
        seed_existing_state(&server).await;
        for key in ["include-values", "acknowledge-plaintext"] {
            let other = if key == "include-values" {
                "acknowledge-plaintext"
            } else {
                "include-values"
            };
            for (label, suffix, selected) in query_flags() {
                let (status, body) = server
                    .get(&query_path(
                        &format!("/api/v1/export?{other}=1"),
                        key,
                        suffix,
                    ))
                    .await;
                if selected {
                    assert_eq!(status, StatusCode::OK, "export {key} {label}");
                    assert_eq!(body["schema"], "locron.export/v1");
                    assert_eq!(body["values_mode"], "plaintext");
                    assert!(
                        body["settings"]["environment"]["EXISTING"] == "original-private-value",
                        "explicit plaintext export value"
                    );
                } else {
                    assert_eq!(status, StatusCode::BAD_REQUEST);
                    assert_eq!(
                        error(&body, "invalid_request"),
                        "plaintext export requires both --include-values and --acknowledge-plaintext"
                    );
                    assert_redacted(&body);
                }
            }
        }
        for query in ["", "?include-values=false&acknowledge-plaintext=0"] {
            let document = server.export_document(query).await;
            assert_eq!(document["values_mode"], "redacted");
            assert_redacted(&document);
        }
        let document = server
            .export_document("?include-values=1&acknowledge-plaintext=1")
            .await;
        for (label, suffix, accepted) in query_flags() {
            let before = durable_snapshot(&server.paths);
            let (status, body) = server
                .post(
                    &query_path(
                        "/api/v1/import?dry-run=1",
                        "accept-plaintext-values",
                        suffix,
                    ),
                    Some(document.clone()),
                )
                .await;
            if accepted {
                assert_eq!(status, StatusCode::OK, "import acknowledgement {label}");
                assert_eq!(data(&body)["dry_run"], true);
                assert_eq!(data(&body)["actions"][0]["action"], "no_op");
            } else {
                assert_eq!(status, StatusCode::BAD_REQUEST);
                assert_eq!(
                    error(&body, "invalid_request"),
                    "plaintext values require --accept-plaintext-values"
                );
            }
            assert_preserved(&before, &server.paths, label);
        }
        for (label, suffix, preview) in query_flags() {
            let mut request = document.clone();
            request["jobs"][0]["id"] = json!(uuid::Uuid::now_v7().to_string());
            request["jobs"][0]["name"] = json!(format!("import-{label}"));
            let before = durable_snapshot(&server.paths);
            let count = before.records["jobs"].as_array().expect("jobs").len();
            let (status, body) = server
                .post(
                    &query_path(
                        "/api/v1/import?accept-plaintext-values=1",
                        "dry-run",
                        suffix,
                    ),
                    Some(request),
                )
                .await;
            assert_eq!(status, StatusCode::OK, "import dry-run {label}");
            if preview {
                assert_eq!(data(&body)["dry_run"], true);
                assert_eq!(data(&body)["actions"][0]["action"], "create");
                assert_preserved(&before, &server.paths, label);
            } else {
                assert_eq!(data(&body)["created"], 1);
                let store = Store::open_read_only(&server.paths.database).expect("imported state");
                assert_eq!(
                    store.list_jobs(true).expect("actual imported jobs").len(),
                    count + 1
                );
                assert_eq!(
                    store
                        .job(&format!("import-{label}"))
                        .expect("actual imported job")
                        .current_revision,
                    1
                );
            }
            assert_redacted(&body);
        }
        server.stop_and_join().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn invalid_query_text_refuses_all_nine_fields_before_handler_mutation() {
        let mut server = spawn_server();
        seed_existing_state(&server).await;
        let document = server
            .export_document("?include-values=1&acknowledge-plaintext=1")
            .await;
        for (method, base, key, request) in [
            ("GET", "/api/v1/jobs", "all", None),
            ("POST", "/api/v1/jobs/original/run?dry-run=1", "wait", None),
            ("POST", "/api/v1/jobs/original/run", "dry-run", None),
            (
                "POST",
                "/api/v1/runs/not-a-uuid/cancel",
                "acknowledge-unconfirmed",
                None,
            ),
            ("GET", "/api/v1/export", "include-values", None),
            ("GET", "/api/v1/export", "acknowledge-plaintext", None),
            (
                "POST",
                "/api/v1/import?dry-run=1",
                "accept-plaintext-values",
                Some(&document),
            ),
            (
                "POST",
                "/api/v1/import?accept-plaintext-values=1",
                "dry-run",
                Some(&document),
            ),
            ("POST", "/api/v1/prune", "dry-run", None),
        ] {
            for text in ["TRUE", "yes", "2"] {
                let before = durable_snapshot(&server.paths);
                let path = query_path(base, key, Some(&format!("={text}")));
                let (status, raw) = server.raw_response(method, &path, request).await;
                assert_eq!(
                    status,
                    StatusCode::BAD_REQUEST,
                    "query extractor {key} {text}"
                );
                assert!(
                    serde_json::from_str::<Value>(&raw).is_err(),
                    "invalid query unexpectedly reached managed envelope"
                );
                assert!(
                    !raw.contains("original-private-value"),
                    "query refusal leaked value"
                );
                assert_preserved(&before, &server.paths, "invalid query text");
            }
        }
        server.stop_and_join().await;
    }
}
