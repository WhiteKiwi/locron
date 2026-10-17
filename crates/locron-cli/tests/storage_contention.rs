//! Actual external writer qualification for the CLI error and stream contracts.

#[path = "support/storage_contention.rs"]
mod contention;
#[path = "support/private_state.rs"]
mod private_state;

use std::process::{Command, Output};
use std::time::{Duration, Instant};

use locron_store::{StatePaths, Store};
use private_state::{PrivateState, private_state_fixture};
use serde_json::Value;

fn invoke(state: &PrivateState, args: &[&str]) -> Output {
    Command::new(assert_cmd::cargo::cargo_bin!("locron"))
        .arg("--state-dir")
        .arg(state.path())
        .arg("-vv")
        .args(args)
        .output()
        .unwrap()
}

fn sustained(args: &[&str]) -> Output {
    let state = private_state_fixture();
    let paths = StatePaths::new(state.path().to_path_buf());
    contention::seed(&paths);
    let writer = contention::hold_writer(&paths);
    let started = Instant::now();
    let output = invoke(&state, args);
    assert_eq!(output.status.code(), Some(5));
    assert!(started.elapsed() >= Duration::from_millis(4800));
    assert!(
        started.elapsed() < Duration::from_secs(8),
        "stacked wait budget"
    );
    contention::assert_private(&output.stdout);
    contention::assert_private(&output.stderr);
    writer.execute_batch("ROLLBACK").unwrap();
    drop(writer);
    let store = Store::open(paths, env!("CARGO_PKG_VERSION"), 1).unwrap();
    let job = store.job("private-process").unwrap();
    assert!(!job.enabled, "failed mutation changed disabled job");
    drop(store);
    let retry = invoke(&state, &["remove", "private-process"]);
    assert!(
        retry.status.success(),
        "same removal must succeed after release"
    );
    output
}

#[test]
fn cli_json_held_writer_preserves_codes_and_safe_details() {
    let output = sustained(&["--json", "remove", "private-process"]);
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema"], "locron.cli/v1");
    assert_eq!(value["ok"], false);
    contention::assert_busy(&value["error"]);
}

#[test]
fn cli_human_held_writer_reports_stage_wait_codes_and_guidance() {
    let output = sustained(&["remove", "private-process"]);
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("failed at begin"));
    assert!(stderr.contains("SQLite wait"));
    assert!(stderr.contains("SQLite primary 5"));
    assert!(stderr.contains("retry the request"));
    assert!(stderr.contains("lock owner unknown"));
}

#[test]
fn cli_stream_held_writer_has_terminal_storage_error() {
    let output = sustained(&["--json", "run", "private-http", "--wait"]);
    let text = String::from_utf8(output.stdout).unwrap();
    let records: Vec<Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let terminal = records.last().unwrap();
    assert_eq!(terminal["schema"], "locron.stream/v1");
    assert_eq!(terminal["terminal"], true);
    assert_eq!(terminal["ok"], false);
    contention::assert_busy(&terminal["error"]);
}

#[test]
fn cli_short_external_writer_release_succeeds_without_daemon() {
    let state = private_state_fixture();
    let paths = StatePaths::new(state.path().to_path_buf());
    contention::seed(&paths);
    let writer = contention::hold_writer(&paths);
    let release = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(250));
        writer.execute_batch("ROLLBACK").unwrap();
    });
    let started = Instant::now();
    let output = invoke(&state, &["--json", "remove", "private-process"]);
    release.join().unwrap();
    assert!(output.status.success());
    assert!(started.elapsed() >= Duration::from_millis(200));
    assert!(started.elapsed() < Duration::from_secs(3));
    contention::assert_private(&output.stdout);
    contention::assert_private(&output.stderr);
}

#[test]
fn cli_sqlite_execution_failure_redacts_raw_secret_message_without_retry() {
    let state = private_state_fixture();
    let paths = StatePaths::new(state.path().to_path_buf());
    contention::seed(&paths);
    contention::reject_private_mutations(&paths);
    for args in [
        &["--json", "remove", "private-process"][..],
        &["--format=human", "remove", "private-process"][..],
        &["--json", "run", "private-http", "--wait"][..],
    ] {
        let output = invoke(&state, args);
        assert_eq!(output.status.code(), Some(5));
        contention::assert_private(&output.stdout);
        contention::assert_private(&output.stderr);
        if args[0] == "--json" {
            let value: Value = serde_json::from_slice(&output.stdout).unwrap();
            contention::assert_rejected(&value["error"]);
        }
    }
    contention::allow_private_mutations(&paths);
    assert!(
        !Store::open(paths, env!("CARGO_PKG_VERSION"), 1)
            .unwrap()
            .job("private-process")
            .unwrap()
            .enabled
    );
}
