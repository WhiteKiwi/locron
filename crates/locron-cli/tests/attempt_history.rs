//! End-to-end run and attempt observability contracts.

#![cfg(unix)]

use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use locron_store::{LockMetadata, Store};

const SECRET: &str = "attempt-history-secret-value";

fn locron(state: &tempfile::TempDir) -> Command {
    let mut command = Command::new(assert_cmd::cargo::cargo_bin!("locron"));
    command.arg("--state-dir").arg(state.path());
    command
}

fn successful_output(command: &mut Command) -> std::process::Output {
    let output = command.output().expect("run locron");
    assert!(
        output.status.success(),
        "locron failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn json_output(command: &mut Command) -> (serde_json::Value, std::process::Output) {
    let output = successful_output(command);
    let value = serde_json::from_slice(&output.stdout).expect("valid JSON envelope");
    (value, output)
}

struct Daemon(Child);

fn lock_metadata_matches_pid(path: &std::path::Path, pid: u32) -> bool {
    let Ok(file) = std::fs::File::open(path) else {
        return false;
    };
    let mut bytes = Vec::new();
    if file.take(16 * 1024 + 1).read_to_end(&mut bytes).is_err() || bytes.len() > 16 * 1024 {
        return false;
    }
    serde_json::from_slice::<LockMetadata>(&bytes).is_ok_and(|metadata| metadata.pid == pid)
}

fn startup_diagnostic(stderr: &tempfile::NamedTempFile) -> String {
    const LIMIT: usize = 8192;
    let mut bytes = Vec::new();
    stderr
        .reopen()
        .unwrap()
        .take((LIMIT + 1) as u64)
        .read_to_end(&mut bytes)
        .unwrap();
    let truncated = bytes.len() > LIMIT;
    bytes.truncate(LIMIT);
    let mut diagnostic = String::from_utf8_lossy(&bytes).into_owned();
    if truncated {
        diagnostic.push_str("\n[stderr truncated]");
    }
    diagnostic
}

impl Daemon {
    fn start(state: &tempfile::TempDir) -> Self {
        let stderr = tempfile::NamedTempFile::new().unwrap();
        let child = locron(state)
            .args(["daemon", "run"])
            .stdout(Stdio::null())
            .stderr(stderr.reopen().unwrap())
            .spawn()
            .expect("start daemon");
        let mut daemon = Self(child);
        let lock_path = state.path().join("daemon.lock");
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = daemon.0.try_wait().unwrap() {
                panic!(
                    "daemon exited during startup ({status}): {}",
                    startup_diagnostic(&stderr)
                );
            }
            if lock_metadata_matches_pid(&lock_path, daemon.0.id())
                && daemon.0.try_wait().unwrap().is_none()
            {
                return daemon;
            }
            assert!(
                Instant::now() < deadline,
                "daemon did not publish its metadata: {}",
                startup_diagnostic(&stderr)
            );
            thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        if self.0.try_wait().is_ok_and(|status| status.is_some()) {
            return;
        }
        let _ = self.0.kill();
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if self.0.try_wait().is_ok_and(|status| status.is_some()) {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}

fn assert_no_secret(output: &std::process::Output) {
    assert!(!String::from_utf8_lossy(&output.stdout).contains(SECRET));
    assert!(!String::from_utf8_lossy(&output.stderr).contains(SECRET));
}

fn assert_attempt_timing(attempt: &serde_json::Value) {
    let started = attempt["started_at_us"].as_i64().expect("admission time");
    let running = attempt["running_at_us"]
        .as_i64()
        .expect("actual start time");
    let finished = attempt["finished_at_us"].as_i64().expect("finish time");
    let duration = attempt["duration_us"].as_i64().expect("duration");
    assert!(started <= running, "running must follow admission");
    assert!(running <= finished, "finish must follow actual start");
    assert!(duration >= 0);
}

fn assert_output_facts(attempt: &serde_json::Value) {
    let output = &attempt["output"];
    assert_eq!(output["state"], "finalized");
    assert!(output["retained_payload_bytes"].as_i64().unwrap() > 0);
    assert!(output["physical_bytes"].as_i64().unwrap() > 0);
    assert_eq!(output["discarded_bytes"], 0);
    assert_eq!(output["truncated"], false);
    assert!(output["truncated_at_us"].is_null());
    assert!(output["finalized_at_us"].as_i64().is_some());
    assert!(output["prune_started_at_us"].is_null());
    assert!(output["pruned_at_us"].is_null());
}

#[test]
fn history_and_why_expose_complete_ordered_retry_attempts_without_secrets() {
    let state = tempfile::tempdir().unwrap();
    successful_output(locron(&state).args([
        "add",
        "observable-retry",
        "--every",
        "1h",
        "--retries",
        "1",
        "--backoff",
        "fixed",
        "--retry-delay",
        "1s",
        "--env",
        &format!("TOKEN={SECRET}"),
        "--shell",
        "printf 'attempt-%s\\n' \"$LOCRON_ATTEMPT\"; [ \"$LOCRON_ATTEMPT\" -eq 2 ]",
    ]));

    let run = successful_output(locron(&state).args(["--json", "run", "observable-retry"]));
    assert_no_secret(&run);
    let run: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    let run_id = run["data"]["run_id"].as_str().unwrap().to_owned();

    let first_daemon = Daemon::start(&state);
    let store = Store::open_read_only(&state.path().join("state.db")).unwrap();
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let run = store.run(&run_id).unwrap();
        if run.state == "retry_wait" {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "first attempt did not enter retry_wait: {} ({:?})",
            run.state,
            run.reason
        );
        thread::sleep(Duration::from_millis(20));
    }
    drop(first_daemon);
    thread::sleep(Duration::from_millis(1_100));

    let _second_daemon = Daemon::start(&state);
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let run = store.run(&run_id).unwrap();
        if run.state == "succeeded" {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "retry run did not succeed: {} ({:?})",
            run.state,
            run.reason
        );
        thread::sleep(Duration::from_millis(20));
    }

    let (history, history_output) =
        json_output(locron(&state).args(["--json", "history", "observable-retry", "--limit", "1"]));
    assert_no_secret(&history_output);
    assert_eq!(history["schema"], "locron.cli/v1");
    assert_eq!(history["command"], "history");
    let run = &history["data"][0];
    assert_eq!(run["id"], run_id);
    assert_eq!(run["source"], "manual", "history run: {run}");
    assert_eq!(run["trigger"], "manual");
    assert_eq!(run["state"], "succeeded");
    assert_eq!(run["outcome"], "succeeded");
    assert!(run["nominal_us"].is_null());

    let attempts = run["attempts"].as_array().expect("attempt history");
    assert_eq!(attempts.len(), 2);
    assert_eq!(attempts[0]["attempt_number"], 1);
    assert_eq!(attempts[1]["attempt_number"], 2);
    for attempt in attempts {
        assert_eq!(attempt["run_id"], run_id);
        assert_attempt_timing(attempt);
        assert!(attempt["resolved_executable"].as_str().is_some());
        assert_output_facts(attempt);
        assert!(attempt["http_status"].is_null());
        assert!(attempt["http_content_type"].is_null());
    }
    assert_eq!(attempts[0]["state"], "failed");
    assert_eq!(attempts[0]["outcome"], "failed");
    assert_eq!(attempts[0]["exit_code"], 1);
    assert!(attempts[0]["error"].as_str().is_some());
    assert_eq!(attempts[0]["reason"], attempts[0]["error"]);
    assert_eq!(attempts[1]["state"], "succeeded");
    assert_eq!(attempts[1]["outcome"], "succeeded");
    assert_eq!(attempts[1]["exit_code"], 0);

    let actual_started = run["actual_started_at_us"].as_i64().unwrap();
    let finished = run["finished_at_us"].as_i64().unwrap();
    assert_eq!(actual_started, attempts[0]["running_at_us"]);
    assert_eq!(finished, attempts[1]["finished_at_us"]);
    assert_eq!(run["duration_us"], finished - actual_started);
    assert!(actual_started < run["eligible_at_us"].as_i64().unwrap());

    let (why, why_output) = json_output(locron(&state).args(["--json", "why", "--run", &run_id]));
    assert_no_secret(&why_output);
    assert_eq!(why["schema"], "locron.cli/v1");
    assert_eq!(why["command"], "why");
    assert_eq!(&why["data"]["run"], run);
    assert!(why["data"]["events"].as_array().is_some());
}

#[test]
fn history_persists_final_http_status_and_content_type() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let fixture = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 1024];
        let _ = stream.read(&mut request).unwrap();
        stream
            .write_all(
                b"HTTP/1.1 204 No Content\r\nContent-Type: application/vnd.locron.test+json; charset=utf-8\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )
            .unwrap();
    });
    let state = tempfile::tempdir().unwrap();
    successful_output(locron(&state).args([
        "add",
        "http-history",
        "--every",
        "1h",
        "--http",
        "GET",
        &format!("http://{address}/status"),
    ]));
    let run = successful_output(locron(&state).args(["--json", "run", "http-history"]));
    let run: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    let run_id = run["data"]["run_id"].as_str().unwrap().to_owned();

    let daemon = Daemon::start(&state);
    let store = Store::open_read_only(&state.path().join("state.db")).unwrap();
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let run = store.run(&run_id).unwrap();
        if run.state == "succeeded" {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "HTTP run did not succeed: {} ({:?})",
            run.state,
            run.reason
        );
        thread::sleep(Duration::from_millis(20));
    }
    fixture.join().unwrap();
    drop(daemon);

    let (history, _) =
        json_output(locron(&state).args(["--json", "history", "http-history", "--limit", "1"]));
    let attempt = &history["data"][0]["attempts"][0];
    assert_eq!(attempt["http_status"], 204);
    assert_eq!(
        attempt["http_content_type"],
        "application/vnd.locron.test+json; charset=utf-8"
    );
    assert!(attempt["duration_us"].as_i64().unwrap() > 0);
}
