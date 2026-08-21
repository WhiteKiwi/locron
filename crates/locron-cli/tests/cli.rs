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
