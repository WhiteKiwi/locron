//! End-to-end command contract tests.

use std::process::Command;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use predicates::prelude::*;

fn locron(state: &tempfile::TempDir) -> Command {
    let mut command = Command::new(assert_cmd::cargo::cargo_bin!("locron"));
    command.arg("--state-dir").arg(state.path());
    command
}

fn timestamp_after(duration: Duration) -> String {
    let micros = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .saturating_add(duration)
        .as_micros()
        .min(i64::MAX as u128) as i64;
    locron_core::Timestamp::from_epoch_micros(micros).to_string()
}

#[test]
fn add_dry_run_is_non_mutating_and_machine_readable() {
    let state = tempfile::tempdir().unwrap();
    let mut command = locron(&state);
    command.args([
        "--json",
        "add",
        "sample",
        "--every",
        "1m",
        "--dry-run",
        "--",
        "/usr/bin/true",
    ]);
    assert_cmd::assert::Assert::new(command.output().unwrap())
        .success()
        .stdout(predicate::str::contains("\"schema\":\"locron.cli/v1\""))
        .stdout(predicate::str::contains("\"durable\":").not());
    assert!(!state.path().join("state.db").exists());
}

#[test]
fn config_and_prune_dry_runs_do_not_initialize_state() {
    let state = tempfile::tempdir().unwrap();
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args(["config", "set", "global_concurrency", "32", "--dry-run"])
            .output()
            .unwrap(),
    )
    .success();
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args(["prune", "--dry-run"])
            .output()
            .unwrap(),
    )
    .success();
    assert!(!state.path().join("state.db").exists());
}

#[test]
fn manual_run_is_durable_while_daemon_is_offline() {
    let state = tempfile::tempdir().unwrap();
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args(["add", "sample", "--every", "1m", "--", "/usr/bin/true"])
            .output()
            .unwrap(),
    )
    .success();
    let output = locron(&state)
        .args(["--json", "run", "sample"])
        .output()
        .unwrap();
    assert_cmd::assert::Assert::new(output)
        .success()
        .stdout(predicate::str::contains("\"state\":\"queued\""))
        .stdout(predicate::str::contains("daemon is not running"));
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args(["--json", "history", "sample"])
            .output()
            .unwrap(),
    )
    .success()
    .stdout(predicate::str::contains("\"trigger\":\"manual\""));
}

#[test]
fn offline_queued_run_can_be_cancelled_terminally() {
    let state = tempfile::tempdir().unwrap();
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args([
                "add",
                "offline-cancel",
                "--every",
                "1h",
                "--",
                "/usr/bin/true",
            ])
            .output()
            .unwrap(),
    )
    .success();
    let submission = locron(&state)
        .args(["--json", "run", "offline-cancel"])
        .output()
        .unwrap();
    let envelope: serde_json::Value = serde_json::from_slice(&submission.stdout).unwrap();
    let run_id = envelope["data"]["run_id"].as_str().unwrap();

    assert_cmd::assert::Assert::new(
        locron(&state)
            .args(["--json", "cancel", run_id])
            .output()
            .unwrap(),
    )
    .success()
    .stdout(predicate::str::contains("\"requested\":true"));
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args(["--json", "history", "offline-cancel"])
            .output()
            .unwrap(),
    )
    .success()
    .stdout(predicate::str::contains("\"state\":\"cancelled\""))
    .stdout(predicate::str::contains("\"finished_at_us\":null").not());
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args(["--json", "cancel", run_id])
            .output()
            .unwrap(),
    )
    .failure()
    .code(3)
    .stdout(predicate::str::contains("already terminal"));
}

#[test]
fn missing_runtime_env_file_terminalizes_admitted_attempt() {
    let state = tempfile::tempdir().unwrap();
    let env_file = state.path().join("runtime.env");
    std::fs::write(&env_file, "VALUE=present\n").unwrap();
    assert_cmd::assert::Assert::new(
        locron(&state)
            .arg("add")
            .arg("missing-env")
            .args(["--every", "1h", "--env-file"])
            .arg(&env_file)
            .args(["--", "/usr/bin/true"])
            .output()
            .unwrap(),
    )
    .success();
    let submission = locron(&state)
        .args(["--json", "run", "missing-env"])
        .output()
        .unwrap();
    let envelope: serde_json::Value = serde_json::from_slice(&submission.stdout).unwrap();
    let run_id = envelope["data"]["run_id"].as_str().unwrap().to_owned();
    std::fs::remove_file(&env_file).unwrap();

    let mut daemon = locron(&state)
        .args(["daemon", "run"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let history = locron(&state)
            .args(["--json", "history", "missing-env"])
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&history.stdout);
        if text.contains("\"state\":\"failed\"") {
            assert!(text.contains("environment file"));
            break;
        }
        assert!(
            Instant::now() < deadline,
            "run remained non-terminal: {text}"
        );
        thread::sleep(Duration::from_millis(25));
    }
    let _ = daemon.kill();
    let _ = daemon.wait();
    assert!(
        state
            .path()
            .join("outputs")
            .join(&run_id)
            .join("1.log")
            .is_file()
    );
    assert!(
        !state
            .path()
            .join("outputs")
            .join(&run_id)
            .join("1.partial")
            .exists()
    );
}

#[test]
fn future_one_time_job_stays_enabled_without_a_run() {
    let state = tempfile::tempdir().unwrap();
    let at = timestamp_after(Duration::from_secs(60));
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args(["add", "future-at", "--at", &at, "--", "/usr/bin/true"])
            .output()
            .unwrap(),
    )
    .success();
    let mut daemon = locron(&state)
        .args(["daemon", "run"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !state.path().join("wake.sock").exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    thread::sleep(Duration::from_millis(100));
    let show = locron(&state)
        .args(["--json", "show", "future-at"])
        .output()
        .unwrap();
    let show: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    assert_eq!(show["data"]["enabled"], true);
    let history = locron(&state)
        .args(["--json", "history", "future-at"])
        .output()
        .unwrap();
    let history: serde_json::Value = serde_json::from_slice(&history.stdout).unwrap();
    assert_eq!(history["data"].as_array().unwrap().len(), 0);
    let _ = daemon.kill();
    let _ = daemon.wait();
}

#[test]
fn manual_run_does_not_resolve_future_one_time_schedule() {
    let state = tempfile::tempdir().unwrap();
    let at = timestamp_after(Duration::from_secs(60));
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args(["add", "manual-at", "--at", &at, "--", "/usr/bin/true"])
            .output()
            .unwrap(),
    )
    .success();
    assert_cmd::assert::Assert::new(locron(&state).args(["run", "manual-at"]).output().unwrap())
        .success();
    let show = locron(&state)
        .args(["--json", "show", "manual-at"])
        .output()
        .unwrap();
    let show: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    assert_eq!(show["data"]["enabled"], true);
}

#[test]
fn due_one_time_job_catches_up_once_and_disables() {
    let state = tempfile::tempdir().unwrap();
    let at = timestamp_after(Duration::from_secs(2));
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args(["add", "due-at", "--at", &at, "--", "/usr/bin/true"])
            .output()
            .unwrap(),
    )
    .success();
    thread::sleep(Duration::from_millis(2_200));

    let mut daemon = locron(&state)
        .args(["daemon", "run"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let history = locron(&state)
            .args(["--json", "history", "due-at"])
            .output()
            .unwrap();
        if String::from_utf8_lossy(&history.stdout).contains("\"state\":\"succeeded\"") {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "one-time run did not complete: {}",
            String::from_utf8_lossy(&history.stdout)
        );
        thread::sleep(Duration::from_millis(25));
    }
    let _ = daemon.kill();
    let _ = daemon.wait();
    let show = locron(&state)
        .args(["--json", "show", "due-at"])
        .output()
        .unwrap();
    let show: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    assert_eq!(show["data"]["enabled"], false);

    let mut restarted = locron(&state)
        .args(["daemon", "run"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    thread::sleep(Duration::from_millis(200));
    let history = locron(&state)
        .args(["--json", "history", "due-at"])
        .output()
        .unwrap();
    let history: serde_json::Value = serde_json::from_slice(&history.stdout).unwrap();
    assert_eq!(history["data"].as_array().unwrap().len(), 1);
    let _ = restarted.kill();
    let _ = restarted.wait();
}

#[test]
fn normal_inspection_redacts_inline_environment_values() {
    let state = tempfile::tempdir().unwrap();
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args([
                "add",
                "secret",
                "--every",
                "1m",
                "--env",
                "TOKEN=should-not-leak",
                "--",
                "/usr/bin/true",
            ])
            .output()
            .unwrap(),
    )
    .success()
    .stdout(predicate::str::contains("should-not-leak").not());
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args(["--json", "show", "secret"])
            .output()
            .unwrap(),
    )
    .success()
    .stdout(predicate::str::contains("<redacted>"))
    .stdout(predicate::str::contains("should-not-leak").not());
}

#[test]
fn conflicting_schedule_selectors_fail_without_state() {
    let state = tempfile::tempdir().unwrap();
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args([
                "--json",
                "add",
                "bad",
                "--every",
                "1m",
                "--cron",
                "* * * * *",
                "--dry-run",
                "--",
                "/usr/bin/true",
            ])
            .output()
            .unwrap(),
    )
    .failure()
    .code(2)
    .stdout(predicate::str::contains("\"ok\":false"));
    assert!(!state.path().join("state.db").exists());
}

#[test]
fn wake_socket_makes_new_manual_run_promptly_visible_to_daemon() {
    let state = tempfile::tempdir().unwrap();
    let mut daemon = locron(&state)
        .args(["daemon", "run"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !state.path().join("wake.sock").exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    assert!(state.path().join("wake.sock").exists());
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args(["add", "wake", "--every", "1h", "--", "/usr/bin/true"])
            .output()
            .unwrap(),
    )
    .success();
    assert_cmd::assert::Assert::new(locron(&state).args(["run", "wake"]).output().unwrap())
        .success();
    let completed = loop {
        let output = locron(&state)
            .args(["--json", "history", "wake"])
            .output()
            .unwrap();
        let text = String::from_utf8(output.stdout).unwrap();
        if text.contains("\"state\":\"succeeded\"") {
            break true;
        }
        if Instant::now() >= deadline {
            break false;
        }
        thread::sleep(Duration::from_millis(25));
    };
    let _ = daemon.kill();
    let _ = daemon.wait();
    assert!(
        completed,
        "wake notification did not prompt daemon admission"
    );
}

#[test]
fn durable_cancel_terminates_a_running_process() {
    let state = tempfile::tempdir().unwrap();
    let mut daemon = locron(&state)
        .args(["daemon", "run"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(8);
    while !state.path().join("wake.sock").exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args(["add", "cancel", "--every", "1h", "--shell", "sleep 30"])
            .output()
            .unwrap(),
    )
    .success();
    let output = locron(&state)
        .args(["--json", "run", "cancel"])
        .output()
        .unwrap();
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let run_id = envelope["data"]["run_id"].as_str().unwrap();
    loop {
        let history = locron(&state)
            .args(["--json", "history", "cancel"])
            .output()
            .unwrap();
        if String::from_utf8_lossy(&history.stdout).contains("\"state\":\"running\"") {
            break;
        }
        assert!(Instant::now() < deadline, "run never entered running state");
        thread::sleep(Duration::from_millis(25));
    }
    assert_cmd::assert::Assert::new(locron(&state).args(["cancel", run_id]).output().unwrap())
        .success();
    let cancelled = loop {
        let history = locron(&state)
            .args(["--json", "history", "cancel"])
            .output()
            .unwrap();
        if String::from_utf8_lossy(&history.stdout).contains("\"state\":\"cancelled\"") {
            break true;
        }
        if Instant::now() >= deadline {
            break false;
        }
        thread::sleep(Duration::from_millis(25));
    };
    let _ = daemon.kill();
    let _ = daemon.wait();
    assert!(
        cancelled,
        "durable cancellation did not terminate the process"
    );
}

#[test]
fn update_dry_run_reports_sorted_changes_without_leaking_values() {
    let state = tempfile::tempdir().unwrap();
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args([
                "add",
                "api",
                "--every",
                "1h",
                "--env",
                "TOKEN=environment-secret",
                "--http",
                "POST",
                "https://example.test/hook",
                "--body",
                "body-secret",
                "--header",
                "Authorization=header-secret",
            ])
            .output()
            .unwrap(),
    )
    .success();

    let output = locron(&state)
        .args([
            "--json",
            "update",
            "api",
            "--description",
            "changed",
            "--unset-header",
            "Authorization",
            "--header-env",
            "X-Token=TOKEN",
            "--retries",
            "2",
            "--backoff",
            "fixed",
            "--dry-run",
        ])
        .output()
        .unwrap();
    assert_cmd::assert::Assert::new(output.clone())
        .success()
        .stdout(predicate::str::contains("environment-secret").not())
        .stdout(predicate::str::contains("header-secret").not())
        .stdout(predicate::str::contains("body-secret").not());
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let changed = envelope["data"]["changed_fields"].as_array().unwrap();
    let changed = changed
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect::<Vec<_>>();
    let mut sorted = changed.clone();
    sorted.sort_unstable();
    assert_eq!(changed, sorted);

    let show = locron(&state)
        .args(["--json", "show", "api"])
        .output()
        .unwrap();
    let show: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    assert_eq!(show["data"]["current_revision"], 1);
    assert_eq!(show["data"]["description"], serde_json::Value::Null);
}

#[test]
fn export_requires_plaintext_ack_and_omits_redacted_values() {
    let state = tempfile::tempdir().unwrap();
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args([
                "add",
                "secret",
                "--every",
                "1h",
                "--env",
                "TOKEN=environment-secret",
                "--http",
                "POST",
                "https://example.test/hook",
                "--body",
                "body-secret",
                "--header",
                "Authorization=header-secret",
            ])
            .output()
            .unwrap(),
    )
    .success();

    let redacted = locron(&state).arg("export").output().unwrap();
    assert_cmd::assert::Assert::new(redacted.clone())
        .success()
        .stdout(predicate::str::contains("environment-secret").not())
        .stdout(predicate::str::contains("header-secret").not())
        .stdout(predicate::str::contains("body-secret").not())
        .stdout(predicate::str::contains("<redacted>").not());
    let document: serde_json::Value = serde_json::from_slice(&redacted.stdout).unwrap();
    assert_eq!(document["schema"], "locron.export/v1");
    assert_eq!(document["values_mode"], "redacted");
    assert_eq!(
        document["jobs"][0]["definition"]["environment"]["values"],
        serde_json::json!({})
    );
    assert!(
        document["jobs"][0]["omitted_values"]
            .as_array()
            .unwrap()
            .len()
            >= 3
    );

    for arguments in [
        vec!["export", "--include-values"],
        vec!["export", "--acknowledge-plaintext"],
        vec!["export", "--include-history"],
    ] {
        assert_cmd::assert::Assert::new(locron(&state).args(arguments).output().unwrap())
            .failure()
            .code(2);
    }
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args(["export", "--include-values", "--acknowledge-plaintext"])
            .output()
            .unwrap(),
    )
    .success()
    .stdout(predicate::str::contains("environment-secret"))
    .stdout(predicate::str::contains("header-secret"))
    .stdout(predicate::str::contains("body-secret").not());
}

#[test]
fn plaintext_export_import_round_trips_and_second_import_is_no_op() {
    let source = tempfile::tempdir().unwrap();
    let destination = tempfile::tempdir().unwrap();
    let export_path = source.path().join("export.json");
    assert_cmd::assert::Assert::new(
        locron(&source)
            .args([
                "add",
                "roundtrip",
                "--cron",
                "15 3 * * MON",
                "--timezone",
                "Asia/Seoul",
                "--env",
                "TOKEN=roundtrip-secret",
                "--overlap",
                "allow",
                "--per-job-concurrency",
                "4",
                "--retries",
                "3",
                "--shell",
                "printf ok",
            ])
            .output()
            .unwrap(),
    )
    .success();
    let exported = locron(&source)
        .args(["export", "--include-values", "--acknowledge-plaintext"])
        .output()
        .unwrap();
    std::fs::write(&export_path, &exported.stdout).unwrap();

    assert_cmd::assert::Assert::new(
        locron(&destination)
            .arg("import")
            .arg(&export_path)
            .output()
            .unwrap(),
    )
    .failure()
    .code(2)
    .stderr(predicate::str::contains("--accept-plaintext-values"));
    assert!(!destination.path().join("state.db").exists());

    assert_cmd::assert::Assert::new(
        locron(&destination)
            .arg("import")
            .arg(&export_path)
            .arg("--accept-plaintext-values")
            .output()
            .unwrap(),
    )
    .success()
    .stdout(predicate::str::contains("\"created\": 1"));
    let destination_export = locron(&destination)
        .args(["export", "--include-values", "--acknowledge-plaintext"])
        .output()
        .unwrap();
    let source_document: serde_json::Value = serde_json::from_slice(&exported.stdout).unwrap();
    let destination_document: serde_json::Value =
        serde_json::from_slice(&destination_export.stdout).unwrap();
    assert_eq!(source_document, destination_document);

    assert_cmd::assert::Assert::new(
        locron(&destination)
            .arg("import")
            .arg(&export_path)
            .arg("--accept-plaintext-values")
            .output()
            .unwrap(),
    )
    .success()
    .stdout(predicate::str::contains("\"no_op\": 1"));
    let show = locron(&destination)
        .args(["--json", "show", "roundtrip"])
        .output()
        .unwrap();
    let show: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    assert_eq!(show["data"]["current_revision"], 1);
}

#[test]
fn import_dry_run_and_rejected_redacted_import_do_not_initialize_state() {
    let source = tempfile::tempdir().unwrap();
    let destination = tempfile::tempdir().unwrap();
    let export_path = source.path().join("export.json");
    assert_cmd::assert::Assert::new(
        locron(&source)
            .args(["add", "safe", "--every", "1h", "--", "/usr/bin/true"])
            .output()
            .unwrap(),
    )
    .success();
    let safe_export = locron(&source).arg("export").output().unwrap();
    std::fs::write(&export_path, &safe_export.stdout).unwrap();
    assert_cmd::assert::Assert::new(
        locron(&destination)
            .arg("import")
            .arg(&export_path)
            .arg("--dry-run")
            .output()
            .unwrap(),
    )
    .success()
    .stdout(predicate::str::contains("<non-durable").not());
    assert!(!destination.path().join("state.db").exists());

    let secret = tempfile::tempdir().unwrap();
    assert_cmd::assert::Assert::new(
        locron(&secret)
            .args([
                "add",
                "unsafe",
                "--every",
                "1h",
                "--env",
                "TOKEN=value",
                "--",
                "/usr/bin/true",
            ])
            .output()
            .unwrap(),
    )
    .success();
    let omitted_export = locron(&secret).arg("export").output().unwrap();
    std::fs::write(&export_path, &omitted_export.stdout).unwrap();
    assert_cmd::assert::Assert::new(
        locron(&destination)
            .arg("import")
            .arg(&export_path)
            .arg("--dry-run")
            .output()
            .unwrap(),
    )
    .failure()
    .code(2)
    .stderr(predicate::str::contains("cannot be imported faithfully"));
    assert!(!destination.path().join("state.db").exists());
}

#[test]
fn import_maps_by_live_name_and_reallocates_a_removed_id_collision() {
    let source = tempfile::tempdir().unwrap();
    let destination = tempfile::tempdir().unwrap();
    let export_path = source.path().join("mapping.json");
    assert_cmd::assert::Assert::new(
        locron(&source)
            .args([
                "add",
                "mapped",
                "--every",
                "2h",
                "--description",
                "source-definition",
                "--",
                "/usr/bin/true",
            ])
            .output()
            .unwrap(),
    )
    .success();
    let source_show = locron(&source)
        .args(["--json", "show", "mapped"])
        .output()
        .unwrap();
    let source_show: serde_json::Value = serde_json::from_slice(&source_show.stdout).unwrap();
    let source_id = source_show["data"]["id"].as_str().unwrap().to_owned();
    let export = locron(&source).arg("export").output().unwrap();
    std::fs::write(&export_path, export.stdout).unwrap();

    assert_cmd::assert::Assert::new(
        locron(&destination)
            .args(["add", "mapped", "--every", "1h", "--", "/usr/bin/false"])
            .output()
            .unwrap(),
    )
    .success();
    let before = locron(&destination)
        .args(["--json", "show", "mapped"])
        .output()
        .unwrap();
    let before: serde_json::Value = serde_json::from_slice(&before.stdout).unwrap();
    let local_id = before["data"]["id"].as_str().unwrap().to_owned();
    assert_ne!(local_id, source_id);
    assert_cmd::assert::Assert::new(
        locron(&destination)
            .arg("import")
            .arg(&export_path)
            .output()
            .unwrap(),
    )
    .success()
    .stdout(predicate::str::contains("\"updated\": 1"));
    let mapped = locron(&destination)
        .args(["--json", "show", "mapped"])
        .output()
        .unwrap();
    let mapped: serde_json::Value = serde_json::from_slice(&mapped.stdout).unwrap();
    assert_eq!(mapped["data"]["id"], local_id);
    assert_eq!(mapped["data"]["description"], "source-definition");

    let collision = tempfile::tempdir().unwrap();
    assert_cmd::assert::Assert::new(
        locron(&collision)
            .arg("import")
            .arg(&export_path)
            .output()
            .unwrap(),
    )
    .success();
    assert_cmd::assert::Assert::new(
        locron(&collision)
            .args(["remove", "mapped"])
            .output()
            .unwrap(),
    )
    .success();
    assert_cmd::assert::Assert::new(
        locron(&collision)
            .arg("import")
            .arg(&export_path)
            .output()
            .unwrap(),
    )
    .success()
    .stdout(predicate::str::contains("\"created\": 1"));
    let recreated = locron(&collision)
        .args(["--json", "show", "mapped"])
        .output()
        .unwrap();
    let recreated: serde_json::Value = serde_json::from_slice(&recreated.stdout).unwrap();
    assert_ne!(recreated["data"]["id"], source_id);
}

#[cfg(unix)]
#[test]
fn broad_env_file_permissions_warn_without_reading_or_mutating_on_dry_run() {
    use std::os::unix::fs::PermissionsExt as _;

    let state = tempfile::tempdir().unwrap();
    let env_file = state.path().join("broad.env");
    std::fs::write(&env_file, "SECRET=warning-must-not-read-this\n").unwrap();
    std::fs::set_permissions(&env_file, std::fs::Permissions::from_mode(0o644)).unwrap();
    let output = locron(&state)
        .arg("--json")
        .arg("add")
        .arg("permissions")
        .args(["--every", "1h", "--env-file"])
        .arg(&env_file)
        .args(["--dry-run", "--", "/usr/bin/true"])
        .output()
        .unwrap();
    assert_cmd::assert::Assert::new(output)
        .success()
        .stdout(predicate::str::contains(
            "readable or writable by group/others",
        ))
        .stdout(predicate::str::contains("warning-must-not-read-this").not());
    assert!(!state.path().join("state.db").exists());
}

#[test]
fn per_job_concurrency_uses_current_durable_global_limit() {
    let state = tempfile::tempdir().unwrap();
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args(["config", "set", "global_concurrency", "2"])
            .output()
            .unwrap(),
    )
    .success();
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args([
                "add",
                "too-wide",
                "--every",
                "1h",
                "--overlap",
                "allow",
                "--per-job-concurrency",
                "3",
                "--",
                "/usr/bin/true",
            ])
            .output()
            .unwrap(),
    )
    .failure()
    .code(2)
    .stderr(predicate::str::contains("global concurrency"));
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args(["--json", "list", "--all"])
            .output()
            .unwrap(),
    )
    .success()
    .stdout(predicate::str::contains("too-wide").not());
}

#[test]
fn partial_json_body_update_preserves_explicit_content_type() {
    let state = tempfile::tempdir().unwrap();
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args([
                "add",
                "content-type",
                "--every",
                "1h",
                "--http",
                "POST",
                "https://example.test/hook",
                "--header",
                "Content-Type=application/vnd.example+json",
                "--success-status",
                "201",
            ])
            .output()
            .unwrap(),
    )
    .success();
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args([
                "update",
                "content-type",
                "--clear-success-statuses",
                "--success-status",
                "202-203",
            ])
            .output()
            .unwrap(),
    )
    .success();
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args([
                "update",
                "content-type",
                "--json-body",
                "{\"updated\":true}",
            ])
            .output()
            .unwrap(),
    )
    .success();

    let export = locron(&state)
        .args(["export", "--include-values", "--acknowledge-plaintext"])
        .output()
        .unwrap();
    let document: serde_json::Value = serde_json::from_slice(&export.stdout).unwrap();
    assert_eq!(
        document["jobs"][0]["definition"]["target"]["headers"]["Content-Type"]["value"],
        "application/vnd.example+json"
    );
    assert_eq!(
        document["jobs"][0]["definition"]["target"]["success_statuses"],
        serde_json::json!([202, 203])
    );
}

#[test]
fn repeated_overlap_selector_preserves_custom_concurrency_and_is_a_no_op() {
    let state = tempfile::tempdir().unwrap();
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args([
                "add",
                "allow-wide",
                "--every",
                "1h",
                "--overlap",
                "allow",
                "--per-job-concurrency",
                "10",
                "--",
                "/usr/bin/true",
            ])
            .output()
            .unwrap(),
    )
    .success();
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args(["update", "allow-wide", "--overlap", "allow"])
            .output()
            .unwrap(),
    )
    .failure()
    .code(2)
    .stderr(predicate::str::contains("does not change any field"));
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args([
                "update",
                "allow-wide",
                "--overlap",
                "allow",
                "--description",
                "preserved",
            ])
            .output()
            .unwrap(),
    )
    .success();
    let show = locron(&state)
        .args(["--json", "show", "allow-wide"])
        .output()
        .unwrap();
    let show: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    let definition: serde_json::Value =
        serde_json::from_str(show["data"]["definition_json"].as_str().unwrap()).unwrap();
    assert_eq!(definition["policy"]["per_job_concurrency"], 10);
}

#[test]
fn selector_specific_schedule_options_are_rejected_without_state() {
    for arguments in [
        vec![
            "add",
            "bad",
            "--every",
            "1h",
            "--timezone",
            "UTC",
            "--dry-run",
            "--",
            "/usr/bin/true",
        ],
        vec![
            "add",
            "bad",
            "--cron",
            "* * * * *",
            "--anchor",
            "2026-01-01T00:00:00Z",
            "--dry-run",
            "--",
            "/usr/bin/true",
        ],
        vec![
            "add",
            "bad",
            "--at",
            "2026-01-01T00:00:00Z",
            "--timezone",
            "UTC",
            "--dry-run",
            "--",
            "/usr/bin/true",
        ],
    ] {
        let state = tempfile::tempdir().unwrap();
        assert_cmd::assert::Assert::new(locron(&state).args(arguments).output().unwrap())
            .failure()
            .code(2);
        assert!(!state.path().join("state.db").exists());
    }
}

#[test]
fn import_rejects_source_id_name_destination_ambiguity_as_durable_conflict() {
    let state = tempfile::tempdir().unwrap();
    for name in ["alpha", "beta"] {
        assert_cmd::assert::Assert::new(
            locron(&state)
                .args(["add", name, "--every", "1h", "--", "/usr/bin/true"])
                .output()
                .unwrap(),
        )
        .success();
    }
    let export = locron(&state).arg("export").output().unwrap();
    let mut document: serde_json::Value = serde_json::from_slice(&export.stdout).unwrap();
    let jobs = document["jobs"].as_array_mut().unwrap();
    let mut alpha = jobs
        .iter()
        .find(|job| job["name"] == "alpha")
        .unwrap()
        .clone();
    alpha["name"] = serde_json::Value::String("beta".into());
    *jobs = vec![alpha];
    let path = state.path().join("ambiguous.json");
    std::fs::write(&path, serde_json::to_vec(&document).unwrap()).unwrap();

    assert_cmd::assert::Assert::new(
        locron(&state)
            .arg("import")
            .arg(&path)
            .arg("--dry-run")
            .output()
            .unwrap(),
    )
    .failure()
    .code(3)
    .stderr(predicate::str::contains(
        "resolve to different destination jobs",
    ));
    for name in ["alpha", "beta"] {
        let show = locron(&state)
            .args(["--json", "show", name])
            .output()
            .unwrap();
        let show: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
        assert_eq!(show["data"]["current_revision"], 1);
    }
}

#[test]
fn duplicate_add_and_rename_are_stable_durable_conflicts() {
    let state = tempfile::tempdir().unwrap();
    for name in ["alpha", "beta"] {
        assert_cmd::assert::Assert::new(
            locron(&state)
                .args(["add", name, "--every", "1h", "--", "/usr/bin/true"])
                .output()
                .unwrap(),
        )
        .success();
    }
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args(["add", "alpha", "--every", "1h", "--", "/usr/bin/true"])
            .output()
            .unwrap(),
    )
    .failure()
    .code(3)
    .stderr(predicate::str::contains("durable conflict"));
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args(["update", "alpha", "--rename", "beta"])
            .output()
            .unwrap(),
    )
    .failure()
    .code(3)
    .stderr(predicate::str::contains("durable conflict"));
    let alpha = locron(&state)
        .args(["--json", "show", "alpha"])
        .output()
        .unwrap();
    let alpha: serde_json::Value = serde_json::from_slice(&alpha.stdout).unwrap();
    assert_eq!(alpha["data"]["current_revision"], 1);
}
