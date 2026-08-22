#![cfg(unix)]

//! Cross-process crash recovery acceptance fixtures for Unix daemon lifetimes.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use locron_store::{RunRecord, Store};

const STARTUP_TIMEOUT: Duration = Duration::from_secs(8);
const TARGET_DELAY: &str = "2";

struct DaemonChild(Child);

impl DaemonChild {
    fn kill(&mut self) {
        self.0.kill().expect("send SIGKILL to daemon");
        self.0.wait().expect("reap daemon");
    }
}

impl Drop for DaemonChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn locron(state: &tempfile::TempDir) -> Command {
    let mut command = Command::new(assert_cmd::cargo::cargo_bin!("locron"));
    command.arg("--state-dir").arg(state.path());
    command
}

fn timestamp_after(duration: Duration) -> String {
    let micros = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time after epoch")
        .saturating_add(duration)
        .as_micros()
        .min(i64::MAX as u128) as i64;
    locron_core::Timestamp::from_epoch_micros(micros).to_string()
}

fn add_due_one_time_job(state: &tempfile::TempDir, name: &str, marker: &Path, script: &str) {
    let at = timestamp_after(Duration::from_secs(2));
    let marker = format!("MARKER={}", marker.display());
    let output = locron(state)
        .args([
            "add",
            name,
            "--at",
            &at,
            "--retries",
            "2",
            "--retry-delay",
            "1s",
            "--env",
            &marker,
            "--shell",
            script,
        ])
        .output()
        .expect("register one-time job");
    assert!(
        output.status.success(),
        "job registration failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    thread::sleep(Duration::from_millis(2_200));
}

fn start_daemon(state: &tempfile::TempDir, boundary: Option<&str>) -> DaemonChild {
    let mut command = locron(state);
    command
        .args(["daemon", "run"])
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(boundary) = boundary {
        command
            .env("LOCRON_TEST_CRASH_POINT", boundary)
            .env("LOCRON_TEST_CRASH_READY", ready_path(state, boundary));
    }
    DaemonChild(command.spawn().expect("spawn daemon"))
}

fn ready_path(state: &tempfile::TempDir, boundary: &str) -> PathBuf {
    state.path().join(format!("{boundary}.ready"))
}

fn wait_for_path(path: &Path, description: &str) {
    let deadline = Instant::now() + STARTUP_TIMEOUT;
    while !path.exists() {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {description}"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn history(state: &tempfile::TempDir, name: &str) -> Vec<RunRecord> {
    Store::open_read_only(&state.path().join("state.db"))
        .expect("open durable state read-only")
        .history(Some(name), 10)
        .expect("read durable history")
}

fn wait_for_state(state: &tempfile::TempDir, name: &str, expected: &str) -> RunRecord {
    let deadline = Instant::now() + STARTUP_TIMEOUT;
    loop {
        if let Some(run) = history(state, name)
            .into_iter()
            .find(|run| run.state == expected)
        {
            return run;
        }
        assert!(
            Instant::now() < deadline,
            "{name} never entered {expected}: {:?}",
            history(state, name)
                .iter()
                .map(|run| run.state.as_str())
                .collect::<Vec<_>>()
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn marker_lines(marker: &Path) -> Vec<String> {
    std::fs::read_to_string(marker)
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

fn assert_unknown_stays_unique_without_retry(state: &tempfile::TempDir, name: &str) -> DaemonChild {
    let daemon = start_daemon(state, None);
    let recovered = wait_for_state(state, name, "interrupted_unknown");
    assert_eq!(recovered.trigger, "catch_up");
    thread::sleep(Duration::from_millis(1_250));
    let runs = history(state, name);
    assert_eq!(runs.len(), 1, "one-time occurrence was duplicated");
    assert_eq!(runs[0].id, recovered.id);
    assert_eq!(runs[0].state, "interrupted_unknown");
    daemon
}

#[test]
fn death_before_spawn_never_executes_or_retries_one_time_work() {
    let state = tempfile::tempdir().expect("temporary state");
    let marker = state.path().join("effect.log");
    add_due_one_time_job(
        &state,
        "before-spawn",
        &marker,
        "printf 'effect\\n' >> \"$MARKER\"",
    );

    let mut daemon = start_daemon(&state, Some("before-spawn"));
    wait_for_path(&ready_path(&state, "before-spawn"), "pre-spawn boundary");
    assert_eq!(
        wait_for_state(&state, "before-spawn", "starting").state,
        "starting"
    );
    assert!(!marker.exists());
    daemon.kill();

    let _restarted = assert_unknown_stays_unique_without_retry(&state, "before-spawn");
    assert!(!marker.exists(), "unknown pre-spawn work was executed");
}

#[test]
fn death_after_spawn_recovers_unknown_without_a_second_side_effect() {
    let state = tempfile::tempdir().expect("temporary state");
    let marker = state.path().join("effect.log");
    add_due_one_time_job(
        &state,
        "after-spawn",
        &marker,
        &format!(
            "printf 'started\\n' >> \"$MARKER\"; sleep {TARGET_DELAY}; printf 'survived\\n' >> \"$MARKER\""
        ),
    );

    let mut daemon = start_daemon(&state, Some("after-spawn"));
    wait_for_path(&ready_path(&state, "after-spawn"), "post-spawn boundary");
    wait_for_path(&marker, "spawned target side effect");
    assert_eq!(
        wait_for_state(&state, "after-spawn", "running").state,
        "running"
    );
    daemon.kill();

    let _restarted = assert_unknown_stays_unique_without_retry(&state, "after-spawn");
    wait_for_lines(&marker, 2, "spawned target survival");
    assert_eq!(marker_lines(&marker), ["started", "survived"]);
}

#[test]
fn restart_does_not_signal_a_stale_process_identity() {
    let state = tempfile::tempdir().expect("temporary state");
    let marker = state.path().join("effect.log");
    add_due_one_time_job(
        &state,
        "while-running",
        &marker,
        &format!(
            "printf 'started\\n' >> \"$MARKER\"; sleep {TARGET_DELAY}; printf 'survived\\n' >> \"$MARKER\""
        ),
    );

    let mut daemon = start_daemon(&state, None);
    wait_for_state(&state, "while-running", "running");
    wait_for_path(&marker, "running target side effect");
    daemon.kill();

    let _restarted = assert_unknown_stays_unique_without_retry(&state, "while-running");
    wait_for_lines(&marker, 2, "stale target completion");
    assert_eq!(marker_lines(&marker), ["started", "survived"]);
}

#[test]
fn death_after_target_exit_before_final_commit_never_reexecutes() {
    let state = tempfile::tempdir().expect("temporary state");
    let marker = state.path().join("effect.log");
    add_due_one_time_job(
        &state,
        "after-target-exit",
        &marker,
        "printf 'effect\\n' >> \"$MARKER\"",
    );

    let mut daemon = start_daemon(&state, Some("after-target-exit"));
    wait_for_path(
        &ready_path(&state, "after-target-exit"),
        "post-target pre-commit boundary",
    );
    assert_eq!(marker_lines(&marker), ["effect"]);
    assert_eq!(
        wait_for_state(&state, "after-target-exit", "running").state,
        "running"
    );
    daemon.kill();

    let _restarted = assert_unknown_stays_unique_without_retry(&state, "after-target-exit");
    assert_eq!(marker_lines(&marker), ["effect"]);
}

fn wait_for_lines(marker: &Path, count: usize, description: &str) {
    let deadline = Instant::now() + STARTUP_TIMEOUT;
    while marker_lines(marker).len() < count {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {description}"
        );
        thread::sleep(Duration::from_millis(10));
    }
}
