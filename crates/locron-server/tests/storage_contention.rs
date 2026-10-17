//! Real authenticated HTTP mutation/import requests against an external SQLite writer.

#[path = "../../locron-cli/tests/support/storage_contention.rs"]
mod contention;

use std::time::{Duration, Instant};

use locron_server::{AppState, router};
use locron_store::{StatePaths, Store};
use serde_json::Value;

struct Server {
    base: String,
    client: reqwest::Client,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn server(paths: &StatePaths) -> Server {
    let token = locron_server::token::ensure(paths).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let state = AppState {
        paths: paths.clone(),
        token: token.clone(),
        bound_port: port,
    };
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::AUTHORIZATION,
        format!("token {token}").parse().unwrap(),
    );
    Server {
        base: format!("http://127.0.0.1:{port}"),
        client: reqwest::Client::builder()
            .no_proxy()
            .default_headers(headers)
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap(),
        task: tokio::spawn(async move {
            axum::serve(listener, router(state)).await.unwrap();
        }),
    }
}

async fn assert_failure(response: reqwest::Response, started: Instant) {
    assert_eq!(
        response.status(),
        reqwest::StatusCode::INTERNAL_SERVER_ERROR
    );
    assert!(
        started.elapsed() < Duration::from_secs(8),
        "stacked wait budget"
    );
    let bytes = response.bytes().await.unwrap();
    contention::assert_private(&bytes);
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["schema"], "locron.api/v1");
    assert_eq!(value["ok"], false);
    contention::assert_busy(&value["error"]);
}

#[tokio::test]
async fn http_held_writer_removal_is_state_error_and_retry_succeeds() {
    let temporary = tempfile::tempdir().unwrap();
    let paths = StatePaths::new(temporary.path().join("private"));
    contention::seed(&paths);
    let server = server(&paths).await;
    let writer = contention::hold_writer(&paths);
    let url = format!("{}/api/v1/jobs/private-process", server.base);
    let started = Instant::now();
    assert_failure(server.client.delete(&url).send().await.unwrap(), started).await;
    writer.execute_batch("ROLLBACK").unwrap();
    drop(writer);
    let store = Store::open(paths.clone(), env!("CARGO_PKG_VERSION"), 1).unwrap();
    assert!(!store.job("private-process").unwrap().enabled);
    drop(store);
    contention::reject_private_mutations(&paths);
    let rejected = server.client.delete(&url).send().await.unwrap();
    assert_eq!(
        rejected.status(),
        reqwest::StatusCode::INTERNAL_SERVER_ERROR
    );
    let bytes = rejected.bytes().await.unwrap();
    contention::assert_private(&bytes);
    contention::assert_rejected(&serde_json::from_slice::<Value>(&bytes).unwrap()["error"]);
    contention::allow_private_mutations(&paths);
    assert!(
        server
            .client
            .delete(&url)
            .send()
            .await
            .unwrap()
            .status()
            .is_success()
    );
    assert!(
        Store::open(paths, env!("CARGO_PKG_VERSION"), 1)
            .unwrap()
            .job("private-process")
            .is_err()
    );
}

#[tokio::test]
async fn http_import_held_writer_preserves_safe_storage_projection() {
    let temporary = tempfile::tempdir().unwrap();
    let paths = StatePaths::new(temporary.path().join("private"));
    contention::seed(&paths);
    let server = server(&paths).await;
    let mut document: Value = server
        .client
        .get(format!(
            "{}/api/v1/export?include-values=true&acknowledge-plaintext=true",
            server.base,
        ))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    document["jobs"][0]["description"] = Value::String("changed import".into());
    let writer = contention::hold_writer(&paths);
    let url = format!("{}/api/v1/import?accept-plaintext-values=true", server.base);
    let started = Instant::now();
    assert_failure(
        server
            .client
            .post(&url)
            .json(&document)
            .send()
            .await
            .unwrap(),
        started,
    )
    .await;
    writer.execute_batch("ROLLBACK").unwrap();
    drop(writer);
    let store = Store::open(paths, env!("CARGO_PKG_VERSION"), 1).unwrap();
    assert!(
        store
            .job(document["jobs"][0]["id"].as_str().unwrap())
            .unwrap()
            .description
            .is_none()
    );
    drop(store);
    assert!(
        server
            .client
            .post(&url)
            .json(&document)
            .send()
            .await
            .unwrap()
            .status()
            .is_success()
    );
}
