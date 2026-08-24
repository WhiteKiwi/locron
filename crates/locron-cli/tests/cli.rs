//! End-to-end command contract tests.

use std::io::{BufRead, BufReader, Read};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use locron_engine::{Channel, OutputWriter};
use locron_store::{AttemptCompletion, DaemonLock, StatePaths, Store};
use predicates::prelude::*;
use uuid::Uuid;

fn locron(state: &tempfile::TempDir) -> Command {
    let mut command = Command::new(assert_cmd::cargo::cargo_bin!("locron"));
    command.arg("--state-dir").arg(state.path());
    command
}

fn invoke_json(state: &tempfile::TempDir, arguments: &[&str]) -> serde_json::Value {
    let output = locron(state)
        .arg("--json")
        .args(arguments)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "JSON command {arguments:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
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

fn start_daemon(state: &tempfile::TempDir) -> Child {
    let mut daemon = locron(state)
        .args(["daemon", "run"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let lock_path = state.path().join("daemon.lock");
    let deadline = Instant::now() + Duration::from_secs(5);
    while DaemonLock::try_prove_free(&lock_path).is_ok() && Instant::now() < deadline {
        assert_eq!(
            daemon.try_wait().unwrap(),
            None,
            "daemon exited during startup"
        );
        thread::sleep(Duration::from_millis(20));
    }
    assert!(
        DaemonLock::try_prove_free(&lock_path).is_err(),
        "daemon did not acquire its durable lock"
    );
    daemon
}

fn stream_records(output: &[u8]) -> Vec<serde_json::Value> {
    String::from_utf8_lossy(output)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn wait_for_run_state(state: &tempfile::TempDir, name: &str, expected: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let history = locron(state)
            .args(["--json", "history", name])
            .output()
            .unwrap();
        let envelope: serde_json::Value = serde_json::from_slice(&history.stdout).unwrap();
        if let Some(run) = envelope["data"]
            .as_array()
            .and_then(|runs| runs.first())
            .filter(|run| run["state"] == expected)
        {
            return run["id"].as_str().unwrap().to_owned();
        }
        assert!(
            Instant::now() < deadline,
            "run {name} never entered {expected}: {}",
            envelope["data"]
        );
        thread::sleep(Duration::from_millis(25));
    }
}

fn help_output(state: &tempfile::TempDir, arguments: &[String]) -> std::process::Output {
    locron(state).args(arguments).output().unwrap()
}

fn direct_subcommands(help: &str) -> Vec<String> {
    let Some((_, commands)) = help.split_once("Commands:\n") else {
        return Vec::new();
    };
    commands
        .lines()
        .take_while(|line| line.is_empty() || line.starts_with("  "))
        .filter_map(|line| line.split_whitespace().next())
        .filter(|name| *name != "help")
        .map(str::to_owned)
        .collect()
}

fn assert_help_contract(path: &[String], spelling: &str, output: std::process::Output) -> String {
    assert!(
        output.status.success(),
        "help failed for {path:?} via {spelling}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "successful help must not write stderr for {path:?} via {spelling}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    for expected in [
        "Usage:",
        "Options:",
        "Examples:\n  locron",
        "Navigation:\n  Run 'locron",
    ] {
        assert!(
            stdout.contains(expected),
            "help for {path:?} via {spelling} omitted {expected:?}:\n{stdout}"
        );
    }
    stdout
}

#[test]
fn complete_command_tree_has_consistent_help_surface() {
    let state = tempfile::tempdir().unwrap();
    let mut pending = vec![Vec::<String>::new()];
    let mut commands = Vec::new();

    while let Some(path) = pending.pop() {
        let mut arguments = path.clone();
        arguments.push("--help".into());
        let stdout = assert_help_contract(
            &path,
            "--help tree discovery",
            help_output(&state, &arguments),
        );
        let children = direct_subcommands(&stdout);
        if path.is_empty() {
            assert!(
                !children.is_empty(),
                "top-level help did not expose a command tree:\n{stdout}"
            );
        }
        for child in &children {
            let mut child_path = path.clone();
            child_path.push(child.clone());
            pending.push(child_path);
        }
        commands.push((path, !children.is_empty()));
    }

    assert!(
        commands
            .iter()
            .any(|(path, _)| path == &["config".to_owned(), "unset".to_owned()]),
        "recursive help discovery must include config unset"
    );

    for (path, has_subcommands) in &commands {
        for flag in ["-h", "--help"] {
            let mut arguments = path.clone();
            arguments.push(flag.into());
            assert_help_contract(path, flag, help_output(&state, &arguments));
        }

        let mut prefixed_help = vec!["help".into()];
        prefixed_help.extend(path.iter().cloned());
        assert_help_contract(path, "help <COMMAND>", help_output(&state, &prefixed_help));

        if *has_subcommands {
            let mut trailing_help = path.clone();
            trailing_help.push("help".into());
            assert_help_contract(path, "<COMMAND> help", help_output(&state, &trailing_help));
        }
    }

    assert!(
        !state.path().join("state.db").exists(),
        "help must not initialize durable state"
    );
}

#[test]
fn logs_follow_reads_partial_then_final_without_duplicate_frames() {
    let state = tempfile::tempdir().unwrap();
    let paths = StatePaths::new(state.path().to_path_buf());
    let run_id = Uuid::now_v7().to_string();
    let partial = paths.partial_output(&run_id, 1).unwrap();
    let final_path = paths.final_output(&run_id, 1).unwrap();
    let mut child = locron(&state)
        .args(["--json", "logs", &run_id, "--follow"])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let (sender, receiver) = mpsc::channel();
    let reader = thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            sender.send(line.unwrap()).unwrap();
        }
    });

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut writer = runtime
        .block_on(OutputWriter::create(&partial, 1_024))
        .unwrap();
    runtime
        .block_on(writer.write(Channel::Stdout, Duration::from_millis(1), b"live\n"))
        .unwrap();
    runtime.block_on(writer.flush()).unwrap();

    let frame: serde_json::Value = serde_json::from_str(
        &receiver
            .recv_timeout(Duration::from_secs(3))
            .expect("follow did not emit the flushed partial frame"),
    )
    .unwrap();
    assert_eq!(frame["schema"], "locron.stream/v1");
    assert_eq!(frame["record"], "frame");
    assert_eq!(frame["data"]["sequence"], 0);
    assert!(partial.is_file());
    assert!(!final_path.exists());

    runtime.block_on(writer.finalize(&final_path)).unwrap();
    let terminal: serde_json::Value = serde_json::from_str(
        &receiver
            .recv_timeout(Duration::from_secs(3))
            .expect("follow did not finish after output finalization"),
    )
    .unwrap();
    assert_eq!(terminal["schema"], "locron.stream/v1");
    assert_eq!(terminal["record"], "result");
    assert_eq!(terminal["terminal"], true);
    assert_eq!(terminal["ok"], true);
    assert!(child.wait().unwrap().success());
    reader.join().unwrap();
    assert!(
        receiver.try_recv().is_err(),
        "finalization re-emitted an already observed frame"
    );
}

#[test]
fn run_wait_streams_all_attempts_and_maps_target_outcomes() {
    let state = tempfile::tempdir().unwrap();
    let marker = state.path().join("retry-marker");
    let marker_environment = format!("MARKER={}", marker.display());
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args([
                "add",
                "wait-retry",
                "--every",
                "1h",
                "--retries",
                "1",
                "--retry-delay",
                "1s",
                "--env",
                &marker_environment,
                "--shell",
                "if [ -e \"$MARKER\" ]; then printf second; else : > \"$MARKER\"; printf first; exit 7; fi",
            ])
            .output()
            .unwrap(),
    )
    .success();
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args([
                "add",
                "wait-failure",
                "--every",
                "1h",
                "--shell",
                "printf failure; exit 9",
            ])
            .output()
            .unwrap(),
    )
    .success();
    let success_waiter = locron(&state)
        .args(["--json", "run", "wait-retry", "--wait"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    wait_for_run_state(&state, "wait-retry", "queued");
    let mut daemon = start_daemon(&state);
    wait_for_run_state(&state, "wait-retry", "retry_wait");
    let _ = daemon.kill();
    let _ = daemon.wait();
    thread::sleep(Duration::from_millis(1_100));
    daemon = start_daemon(&state);
    let success = success_waiter.wait_with_output().unwrap();
    assert!(
        success.status.success(),
        "{}",
        String::from_utf8_lossy(&success.stderr)
    );
    let records = stream_records(&success.stdout);
    assert!(
        records
            .iter()
            .all(|record| record["schema"] == "locron.stream/v1")
    );
    let attempts: Vec<u64> = records
        .iter()
        .filter(|record| record["record"] == "frame")
        .map(|record| record["data"]["attempt"].as_u64().unwrap())
        .collect();
    assert!(attempts.contains(&1), "first attempt output was omitted");
    assert!(attempts.contains(&2), "retry attempt output was omitted");
    assert_eq!(
        records
            .iter()
            .filter(|record| record["record"] == "result")
            .count(),
        1
    );
    let terminal = records.last().unwrap();
    assert_eq!(terminal["record"], "result");
    assert_eq!(terminal["terminal"], true);
    assert_eq!(terminal["ok"], true);
    assert_eq!(terminal["data"]["state"], "succeeded");

    let _ = daemon.kill();
    let _ = daemon.wait();
    let failure_waiter = locron(&state)
        .args(["--json", "run", "wait-failure", "--wait"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    wait_for_run_state(&state, "wait-failure", "queued");
    daemon = start_daemon(&state);
    let failure = failure_waiter.wait_with_output().unwrap();
    assert_eq!(failure.status.code(), Some(1));
    let records = stream_records(&failure.stdout);
    assert!(
        records
            .iter()
            .all(|record| record["schema"] == "locron.stream/v1")
    );
    assert_eq!(
        records
            .iter()
            .filter(|record| record["record"] == "result")
            .count(),
        1
    );
    let terminal = records.last().unwrap();
    assert_eq!(terminal["record"], "result");
    assert_eq!(terminal["terminal"], true);
    assert_eq!(terminal["ok"], false);
    assert_eq!(terminal["data"]["state"], "failed");
    assert_eq!(terminal["error"]["code"], "target_outcome");

    let _ = daemon.kill();
    let _ = daemon.wait();
}

#[test]
fn disconnecting_run_wait_does_not_cancel_the_durable_run() {
    let state = tempfile::tempdir().unwrap();
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args([
                "add",
                "wait-disconnect",
                "--every",
                "1h",
                "--shell",
                "sleep 1; printf survived",
            ])
            .output()
            .unwrap(),
    )
    .success();
    let mut waiter = locron(&state)
        .args(["--json", "run", "wait-disconnect", "--wait"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    wait_for_run_state(&state, "wait-disconnect", "queued");
    let mut daemon = start_daemon(&state);
    let run_id = wait_for_run_state(&state, "wait-disconnect", "running");
    waiter.kill().unwrap();
    let _ = waiter.wait();

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let history = locron(&state)
            .args(["--json", "history", "wait-disconnect"])
            .output()
            .unwrap();
        let envelope: serde_json::Value = serde_json::from_slice(&history.stdout).unwrap();
        let run = envelope["data"]
            .as_array()
            .unwrap()
            .iter()
            .find(|run| run["id"] == run_id)
            .unwrap();
        if run["state"] == "succeeded" {
            break;
        }
        assert_ne!(run["state"], "cancelled");
        assert!(
            Instant::now() < deadline,
            "durable run did not survive waiter disconnection: {run}"
        );
        thread::sleep(Duration::from_millis(25));
    }

    let _ = daemon.kill();
    let _ = daemon.wait();
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
fn quarantine_requires_explicit_acknowledgement_and_is_visible_in_why_and_history() {
    let state = tempfile::tempdir().unwrap();
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args([
                "add",
                "quarantine",
                "--every",
                "1h",
                "--overlap",
                "replace",
                "--",
                "/usr/bin/true",
            ])
            .output()
            .unwrap(),
    )
    .success();

    let store = Store::open(StatePaths::new(state.path().into()), "test", 0).unwrap();
    let run_id = Uuid::now_v7().to_string();
    store.enqueue_manual("quarantine", &run_id, 1).unwrap();
    let lifetime = Uuid::now_v7().to_string();
    store.begin_lifetime(&lifetime, 2, "test").unwrap();
    let attempt = store.admit(&lifetime, 3, 1).unwrap().attempts.remove(0);
    store
        .mark_attempt_running(&run_id, attempt.attempt_number, 4)
        .unwrap();
    store
        .complete_attempt(&AttemptCompletion {
            run_id: run_id.clone(),
            attempt_number: attempt.attempt_number,
            now_us: 5,
            duration_us: 1,
            state: "termination_unconfirmed".into(),
            exit_code: None,
            http_status: None,
            http_content_type: None,
            reason: "synthetic unconfirmed process group".into(),
            retry: None,
        })
        .unwrap();

    assert_cmd::assert::Assert::new(locron(&state).args(["cancel", &run_id]).output().unwrap())
        .failure()
        .stderr(predicate::str::contains("--acknowledge-unconfirmed"));

    assert_cmd::assert::Assert::new(
        locron(&state)
            .args(["--json", "cancel", &run_id, "--acknowledge-unconfirmed"])
            .output()
            .unwrap(),
    )
    .success()
    .stdout(predicate::str::contains(
        "\"acknowledged_unconfirmed\":true",
    ))
    .stdout(predicate::str::contains(
        "\"state\":\"interrupted_unknown\"",
    ));

    assert_cmd::assert::Assert::new(
        locron(&state)
            .args(["--json", "why", "--run", &run_id])
            .output()
            .unwrap(),
    )
    .success()
    .stdout(predicate::str::contains(
        "termination_unconfirmed_acknowledged",
    ))
    .stdout(predicate::str::contains("acknowledged by operator"));
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args(["--json", "history", "quarantine"])
            .output()
            .unwrap(),
    )
    .success()
    .stdout(predicate::str::contains(
        "\"state\":\"interrupted_unknown\"",
    ));
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args(["cancel", &run_id, "--acknowledge-unconfirmed"])
            .output()
            .unwrap(),
    )
    .failure()
    .stderr(predicate::str::contains("durable conflict"));
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
    .stdout(predicate::str::contains(
        "created 1, updated 0, unchanged 0",
    ));
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
    .stdout(predicate::str::contains(
        "created 0, updated 0, unchanged 1",
    ));
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
    .stdout(predicate::str::contains(
        "created 0, updated 1, unchanged 0",
    ));
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
    .stdout(predicate::str::contains(
        "created 1, updated 0, unchanged 0",
    ));
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
fn alias_ls_renders_identical_output_to_list() {
    let state = tempfile::tempdir().unwrap();
    for name in ["backup", "ping"] {
        assert_cmd::assert::Assert::new(
            locron(&state)
                .args(["add", name, "--every", "1h", "--", "/usr/bin/true"])
                .output()
                .unwrap(),
        )
        .success();
    }
    assert_cmd::assert::Assert::new(locron(&state).args(["disable", "ping"]).output().unwrap())
        .success();
    for (canonical, alias) in [
        (vec!["list"], vec!["ls"]),
        (vec!["list", "--all"], vec!["ls", "--all"]),
        (vec!["--json", "list"], vec!["--json", "ls"]),
    ] {
        let canonical_output = locron(&state).args(canonical.clone()).output().unwrap();
        let alias_output = locron(&state).args(alias.clone()).output().unwrap();
        assert!(
            canonical_output.status.success(),
            "canonical {canonical:?} failed"
        );
        assert!(alias_output.status.success(), "alias {alias:?} failed");
        assert_eq!(
            alias_output.stdout, canonical_output.stdout,
            "alias {alias:?} stdout must be byte-identical to {canonical:?} stdout"
        );
        assert_eq!(
            alias_output.stderr, canonical_output.stderr,
            "alias {alias:?} stderr must be byte-identical to {canonical:?} stderr"
        );
    }
}

#[test]
fn alias_ls_json_envelope_reports_the_canonical_list_command() {
    let state = tempfile::tempdir().unwrap();
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args(["add", "backup", "--every", "1h", "--", "/usr/bin/true"])
            .output()
            .unwrap(),
    )
    .success();
    let output = locron(&state).args(["--json", "ls"]).output().unwrap();
    assert!(output.status.success());
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(envelope["schema"], "locron.cli/v1");
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["command"], "list");
    assert_eq!(envelope["data"].as_array().map(Vec::len), Some(1));
    assert_eq!(envelope["warnings"], serde_json::json!([]));
}

#[test]
fn alias_rm_json_envelope_reports_the_canonical_remove_command() {
    let state = tempfile::tempdir().unwrap();
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args(["add", "backup", "--every", "1h", "--", "/usr/bin/true"])
            .output()
            .unwrap(),
    )
    .success();
    let output = locron(&state)
        .args(["--json", "rm", "backup"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(envelope["schema"], "locron.cli/v1");
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["command"], "remove");
    assert_eq!(envelope["data"]["name"], "backup");
    assert_eq!(envelope["data"]["removed"], true);
    assert_cmd::assert::Assert::new(locron(&state).args(["--json", "ls"]).output().unwrap())
        .success()
        .stdout(predicate::str::contains("backup").not());
}

#[test]
fn help_advertises_the_list_and_remove_aliases() {
    let state = tempfile::tempdir().unwrap();
    assert_cmd::assert::Assert::new(locron(&state).arg("--help").output().unwrap())
        .success()
        .stdout(predicate::str::contains("[alias: ls]"))
        .stdout(predicate::str::contains("[alias: rm]"));
}

#[test]
fn alias_help_exits_zero_with_the_canonical_surface() {
    let state = tempfile::tempdir().unwrap();
    for (alias, canonical) in [("ls", "list"), ("rm", "remove")] {
        let stdout = assert_help_contract(
            &[alias.into()],
            "alias -h",
            help_output(&state, &[alias.into(), "-h".into()]),
        );
        assert!(
            stdout.contains(&format!("Usage: locron {canonical}")),
            "alias {alias} help must show the canonical usage:\n{stdout}"
        );
    }
}

#[test]
fn empty_list_prints_the_header_only() {
    let state = tempfile::tempdir().unwrap();
    assert_cmd::assert::Assert::new(locron(&state).args(["list"]).output().unwrap())
        .success()
        .stdout("NAME SCHEDULE TARGET ENABLED\n")
        .stderr("");
}

#[test]
fn human_list_aligns_columns_across_name_widths() {
    let state = tempfile::tempdir().unwrap();
    for (name, expression) in [("a", "* * * * *"), ("longname", "0 9 * * MON-FRI")] {
        assert_cmd::assert::Assert::new(
            locron(&state)
                .args(["add", name, "--cron", expression, "--", "/usr/bin/true"])
                .output()
                .unwrap(),
        )
        .success();
    }
    assert_cmd::assert::Assert::new(locron(&state).args(["list"]).output().unwrap())
        .success()
        .stdout(
            "NAME     SCHEDULE               TARGET            ENABLED\n\
         a        cron '* * * * *'       run /usr/bin/true yes\n\
         longname cron '0 9 * * MON-FRI' run /usr/bin/true yes\n",
        );
}

#[test]
fn piped_human_list_prints_full_targets_byte_identically() {
    let state = tempfile::tempdir().unwrap();
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args([
                "add",
                "backup",
                "--every",
                "1h",
                "--shell",
                "echo run-a-very-long-backup-job-with-a-silly-name",
            ])
            .output()
            .unwrap(),
    )
    .success();
    // assert_cmd pipes stdout, so the terminal size lookup fails and the
    // long target prints in full — byte-identical to the pre-truncation
    // table.
    assert_cmd::assert::Assert::new(locron(&state).args(["ls"]).output().unwrap())
        .success()
        .stdout(
            "NAME   SCHEDULE TARGET                                                  ENABLED\n\
         backup every 1h shell echo run-a-very-long-backup-job-with-a-silly-name yes\n",
        )
        .stderr("");
}

#[test]
fn list_help_advertises_no_trunc_for_list_and_its_alias() {
    let state = tempfile::tempdir().unwrap();
    for path in ["list", "ls"] {
        let arguments = [path.to_owned(), "--help".to_owned()];
        let stdout = assert_help_contract(&arguments, "list help", help_output(&state, &arguments));
        assert!(
            stdout.contains("--no-trunc"),
            "list help must advertise --no-trunc:\n{stdout}"
        );
    }
}

#[test]
fn list_no_trunc_is_accepted_with_json_and_ignored() {
    let state = tempfile::tempdir().unwrap();
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args(["add", "backup", "--every", "1h", "--", "/usr/bin/true"])
            .output()
            .unwrap(),
    )
    .success();
    let plain = locron(&state)
        .args(["ls", "--format", "json"])
        .output()
        .unwrap();
    let flagged = locron(&state)
        .args(["ls", "--no-trunc", "--format", "json"])
        .output()
        .unwrap();
    assert!(plain.status.success());
    assert!(flagged.status.success());
    assert_eq!(
        flagged.stdout, plain.stdout,
        "--no-trunc must not change machine output"
    );
    assert_eq!(flagged.stderr, plain.stderr);
}

#[test]
fn human_list_all_marks_disabled_jobs_no() {
    let state = tempfile::tempdir().unwrap();
    for name in ["backup", "ping"] {
        assert_cmd::assert::Assert::new(
            locron(&state)
                .args(["add", name, "--every", "1h", "--", "/usr/bin/true"])
                .output()
                .unwrap(),
        )
        .success();
    }
    assert_cmd::assert::Assert::new(locron(&state).args(["disable", "ping"]).output().unwrap())
        .success();
    assert_cmd::assert::Assert::new(locron(&state).args(["list"]).output().unwrap())
        .success()
        .stdout(
            "NAME   SCHEDULE TARGET            ENABLED\nbackup every 1h run /usr/bin/true yes\n",
        );
    assert_cmd::assert::Assert::new(locron(&state).args(["list", "--all"]).output().unwrap())
        .success()
        .stdout(
            "NAME   SCHEDULE TARGET            ENABLED\n\
         backup every 1h run /usr/bin/true yes\n\
         ping   every 1h run /usr/bin/true no\n",
        );
}

#[test]
fn human_list_never_leaks_configured_values() {
    let state = tempfile::tempdir().unwrap();
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args([
                "add",
                "web",
                "--every",
                "1m",
                "--env",
                "TOKEN=should-not-leak",
                "--header",
                "X-Auth=secret-header",
                "--body",
                "secret-body",
                "--http",
                "POST",
                "https://example.com/hook",
            ])
            .output()
            .unwrap(),
    )
    .success()
    .stdout(predicate::str::contains("should-not-leak").not())
    .stdout(predicate::str::contains("secret-header").not())
    .stdout(predicate::str::contains("secret-body").not());
    assert_cmd::assert::Assert::new(locron(&state).args(["list"]).output().unwrap())
        .success()
        .stdout(predicate::str::contains(
            "http POST https://example.com/hook",
        ))
        .stdout(predicate::str::contains("should-not-leak").not())
        .stdout(predicate::str::contains("secret-header").not())
        .stdout(predicate::str::contains("secret-body").not());
}

#[test]
fn human_add_update_enable_disable_remove_print_outcome_lines() {
    let state = tempfile::tempdir().unwrap();
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args(["add", "backup", "--every", "1h", "--", "/usr/bin/true"])
            .output()
            .unwrap(),
    )
    .success()
    .stdout(predicate::str::contains("job added: backup ("))
    .stdout(predicate::str::contains("schedule: every 1h"))
    .stdout(predicate::str::contains("target: run /usr/bin/true"));
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args(["update", "backup", "--every", "2h", "--", "/usr/bin/true"])
            .output()
            .unwrap(),
    )
    .success()
    .stdout(predicate::str::contains("job updated: backup ("))
    .stdout(predicate::str::contains(", revision 2)"))
    .stdout(predicate::str::contains("schedule: every 2h"));
    assert_cmd::assert::Assert::new(locron(&state).args(["disable", "backup"]).output().unwrap())
        .success()
        .stdout("job disabled: backup\n");
    assert_cmd::assert::Assert::new(locron(&state).args(["enable", "backup"]).output().unwrap())
        .success()
        .stdout("job enabled: backup\n");
    assert_cmd::assert::Assert::new(locron(&state).args(["remove", "backup"]).output().unwrap())
        .success()
        .stdout("job removed: backup\n");

    // Dry runs print the outcome line and the summaries, write nothing, and
    // do not initialize state.
    let dry = tempfile::tempdir().unwrap();
    assert_cmd::assert::Assert::new(
        locron(&dry)
            .args([
                "add",
                "dry",
                "--every",
                "1h",
                "--dry-run",
                "--",
                "/usr/bin/true",
            ])
            .output()
            .unwrap(),
    )
    .success()
    .stdout(predicate::str::contains(
        "job added: dry (dry run; no changes made)",
    ))
    .stdout(predicate::str::contains("schedule: every 1h"))
    .stdout(predicate::str::contains("target: run /usr/bin/true"));
    assert!(!dry.path().join("state.db").exists());
    assert_cmd::assert::Assert::new(
        locron(&dry)
            .args(["add", "dry", "--every", "1h", "--", "/usr/bin/true"])
            .output()
            .unwrap(),
    )
    .success();
    assert_cmd::assert::Assert::new(
        locron(&dry)
            .args([
                "update",
                "dry",
                "--every",
                "2h",
                "--dry-run",
                "--",
                "/usr/bin/true",
            ])
            .output()
            .unwrap(),
    )
    .success()
    .stdout(predicate::str::contains(
        "job updated: dry (dry run; no changes made)",
    ))
    .stdout(predicate::str::contains("schedule: every 2h"));
    let show = locron(&dry)
        .args(["--json", "show", "dry"])
        .output()
        .unwrap();
    let show: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    assert_eq!(show["data"]["current_revision"], 1);
}

#[test]
fn human_history_prints_the_aligned_table_with_header_always() {
    let empty = tempfile::tempdir().unwrap();
    assert_cmd::assert::Assert::new(locron(&empty).args(["history"]).output().unwrap())
        .success()
        .stdout("TIME | JOB | TRIGGER | STATE | DURATION\n");

    let state = tempfile::tempdir().unwrap();
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args(["add", "backup", "--every", "1h", "--", "/usr/bin/true"])
            .output()
            .unwrap(),
    )
    .success();
    assert_cmd::assert::Assert::new(locron(&state).args(["run", "backup"]).output().unwrap())
        .success();
    let json = locron(&state).args(["--json", "history"]).output().unwrap();
    let json: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    let run_id = json["data"][0]["id"].as_str().unwrap().to_owned();
    let history = locron(&state).args(["history"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&history.stdout);
    let header = stdout.lines().next().expect("header must always print");
    for column in ["TIME", "JOB", "TRIGGER", "STATE", "DURATION"] {
        assert!(
            header.contains(column),
            "header must carry {column}:\n{stdout}"
        );
    }
    assert!(
        header.ends_with("| DURATION"),
        "header must end with DURATION:\n{stdout}"
    );
    let row = stdout.lines().nth(1).expect("row missing");
    for token in ["backup", "manual", "queued", "-"] {
        assert!(row.contains(token), "row missing {token}:\n{stdout}");
    }
    assert!(
        stdout.contains("Z | "),
        "TIME must be RFC 3339 UTC:\n{stdout}"
    );
    assert!(
        !stdout.contains(&run_id),
        "full run ID must not appear in the table:\n{stdout}"
    );
    assert_eq!(
        json["data"][0]["id"].as_str().unwrap(),
        run_id,
        "the JSON surface must keep the full run ID"
    );

    // A removed job's rows fall back to the abbreviated job ID.
    assert_cmd::assert::Assert::new(locron(&state).args(["remove", "backup"]).output().unwrap())
        .success();
    let history = locron(&state).args(["history"]).output().unwrap();
    let after = String::from_utf8_lossy(&history.stdout);
    assert!(
        !after.contains("| backup |"),
        "removed job name must not appear in history:\n{after}"
    );
    let row = after.lines().nth(1).expect("row must survive removal");
    assert!(
        row.contains("manual") && row.contains("queued"),
        "row must survive removal:\n{after}"
    );
}

#[test]
fn human_run_prints_the_dry_run_decision_and_queued_lines() {
    let state = tempfile::tempdir().unwrap();
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args([
                "add",
                "backup",
                "--every",
                "1h",
                "--overlap",
                "skip",
                "--",
                "/usr/bin/true",
            ])
            .output()
            .unwrap(),
    )
    .success();
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args(["run", "backup", "--dry-run"])
            .output()
            .unwrap(),
    )
    .success()
    .stdout("run eligible: backup\ndry run: no run created\n");
    let json = locron(&state)
        .args(["--json", "run", "backup", "--dry-run"])
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(json["data"]["decision"], "eligible");
    assert_eq!(json["data"]["dry_run"], true);

    // The queued run counts as active, so a later dry run reports the
    // overlap-skip admission decision.
    assert_cmd::assert::Assert::new(locron(&state).args(["run", "backup"]).output().unwrap())
        .success();
    let dry_run = locron(&state)
        .args(["run", "backup", "--dry-run"])
        .output()
        .unwrap();
    let dry = String::from_utf8_lossy(&dry_run.stdout);
    assert!(
        dry.contains("run would skip (overlap policy): backup\ndry run: no run created\n"),
        "overlap-skip decision missing:\n{dry}"
    );
    let run_output = locron(&state).args(["run", "backup"]).output().unwrap();
    let queued = String::from_utf8_lossy(&run_output.stdout);
    assert!(
        queued.starts_with("run queued: "),
        "queued line missing:\n{queued}"
    );
    assert!(
        queued.contains("(job backup)\n"),
        "queued line must name the job:\n{queued}"
    );
}

#[test]
fn human_cancel_prints_the_resolution_line() {
    let state = tempfile::tempdir().unwrap();
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args(["add", "backup", "--every", "1h", "--", "/usr/bin/true"])
            .output()
            .unwrap(),
    )
    .success();
    assert_cmd::assert::Assert::new(locron(&state).args(["run", "backup"]).output().unwrap())
        .success();
    let json = locron(&state).args(["--json", "history"]).output().unwrap();
    let json: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    let run_id = json["data"][0]["id"].as_str().unwrap().to_owned();
    assert_cmd::assert::Assert::new(locron(&state).args(["cancel", &run_id]).output().unwrap())
        .success()
        .stdout(predicate::str::contains("cancellation requested: "))
        .stdout(predicate::str::contains("(cancelled before execution)"));
    // Cancelling the same run again is a durable conflict.
    let json = locron(&state)
        .args(["--json", "cancel", &run_id])
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(json["ok"], false);
    assert_eq!(json["error"]["code"], "durable_conflict");
    // The JSON surface of a fresh cancellation keeps the stable envelope.
    assert_cmd::assert::Assert::new(locron(&state).args(["run", "backup"]).output().unwrap())
        .success();
    let json = locron(&state).args(["--json", "history"]).output().unwrap();
    let json: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    let second = json["data"][0]["id"].as_str().unwrap().to_owned();
    let json = locron(&state)
        .args(["--json", "cancel", &second])
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(json["data"]["requested"], true);
    assert_eq!(json["data"]["cancelled"], true);
    assert_eq!(json["data"]["before_execution"], true);
}

#[test]
fn human_show_prints_labeled_sections() {
    let state = tempfile::tempdir().unwrap();
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args([
                "add",
                "web",
                "--cron",
                "0 9 * * *",
                "--timezone",
                "UTC",
                "--tag",
                "daily",
                "--tag",
                "ops",
                "--shell",
                "printf ok",
            ])
            .output()
            .unwrap(),
    )
    .success();
    let show = locron(&state).args(["show", "web"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&show.stdout);
    for expected in [
        "JOB\n  name: web\n",
        "  id: ",
        "  enabled: yes\n",
        "  tags: ",
        "  revision: 1\n",
        "SCHEDULE\n  schedule: cron '0 9 * * *'\n  timezone: UTC\n",
        "TARGET\n  target: shell printf ok\n",
        "POLICIES\n  overlap: skip\n",
        "  missed run: skip\n",
        "  deadline: none\n",
        "  retries: 0\n",
        "  timeout: 1m\n",
        "  concurrency: 1\n",
    ] {
        assert!(
            stdout.contains(expected),
            "show omitted {expected:?}:\n{stdout}"
        );
    }
    assert!(stdout.contains("daily") && stdout.contains("ops"));
}

#[test]
fn human_show_why_and_list_never_leak_configured_values() {
    let state = tempfile::tempdir().unwrap();
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args([
                "add",
                "web",
                "--every",
                "1m",
                "--env",
                "TOKEN=show-secret",
                "--header",
                "X-Auth=show-header",
                "--body",
                "show-body",
                "--http",
                "POST",
                "https://example.com/hook",
            ])
            .output()
            .unwrap(),
    )
    .success()
    .stdout(predicate::str::contains("show-secret").not())
    .stdout(predicate::str::contains("show-header").not())
    .stdout(predicate::str::contains("show-body").not());
    for arguments in [
        vec!["show", "web"],
        vec!["why", "web"],
        vec!["list"],
        vec!["preview", "web"],
    ] {
        let output = locron(&state).args(&arguments).output().unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            !stdout.contains("show-secret"),
            "{arguments:?} leaked the environment value:\n{stdout}"
        );
        assert!(
            !stdout.contains("show-header"),
            "{arguments:?} leaked a header value:\n{stdout}"
        );
        assert!(
            !stdout.contains("show-body"),
            "{arguments:?} leaked the body:\n{stdout}"
        );
    }
}

#[test]
fn human_preview_prints_the_schedule_line_then_occurrences() {
    let state = tempfile::tempdir().unwrap();
    let output = locron(&state)
        .args(["preview", "--cron", "0 9 * * *", "--count", "2"])
        .output()
        .unwrap();
    assert_cmd::assert::Assert::new(output).success();
    let preview = locron(&state)
        .args(["preview", "--cron", "0 9 * * *", "--count", "2"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&preview.stdout);
    assert!(
        stdout.starts_with("schedule: cron '0 9 * * *'\n"),
        "preview must name the schedule first:\n{stdout}"
    );
    let occurrences: Vec<&str> = stdout.lines().skip(1).collect();
    assert_eq!(occurrences.len(), 2, "expected two occurrences:\n{stdout}");
    for occurrence in occurrences {
        assert!(
            occurrence.ends_with('Z') && occurrence.contains('T'),
            "occurrence must be RFC 3339 UTC: {occurrence:?}"
        );
    }
}

#[test]
fn human_why_job_prints_labeled_sections() {
    let state = tempfile::tempdir().unwrap();
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args(["add", "backup", "--every", "1h", "--", "/usr/bin/true"])
            .output()
            .unwrap(),
    )
    .success();
    let why = locron(&state).args(["why", "backup"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&why.stdout);
    for expected in [
        "JOB\n  name: backup\n",
        "  id: ",
        "  enabled: yes\n",
        "  revision: 1\n",
        "SCHEDULE\n  schedule: every 1h\n",
        "  cursor: ",
        "  next occurrence: ",
        "ELIGIBILITY\n  active runs: 0\n",
        "  decision: eligible\n",
        "  global concurrency: 16\n",
        "POLICIES\n  overlap: skip\n",
        "DAEMON\n  daemon running: no\n",
    ] {
        assert!(
            stdout.contains(expected),
            "why omitted {expected:?}:\n{stdout}"
        );
    }
}

#[test]
fn explain_no_history_is_explicit_redacted_and_human_json_equivalent() {
    let state = tempfile::tempdir().unwrap();
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args([
                "add",
                "quiet",
                "--every",
                "1h",
                "--disabled",
                "--env",
                "TOKEN=explain-secret-value",
                "--",
                "/usr/bin/true",
            ])
            .output()
            .unwrap(),
    )
    .success();

    let json = invoke_json(&state, &["explain", "quiet"]);
    assert_eq!(json["schema"], "locron.cli/v1");
    assert_eq!(json["command"], "explain");
    assert_eq!(json["data"]["job"]["name"], "quiet");
    assert_eq!(json["data"]["job"]["enabled"], false);
    assert_eq!(json["data"]["schedule"]["summary"], "every 1h");
    assert_eq!(
        json["data"]["schedule"]["next_occurrence"],
        serde_json::Value::Null
    );
    assert_eq!(json["data"]["current_status"]["eligibility"], "disabled");
    assert_eq!(
        json["data"]["current_status"]["overlap_decision"],
        "no_active_run"
    );
    assert_eq!(json["data"]["current_status"]["active_runs"], 0);
    assert_eq!(json["data"]["latest_run"], serde_json::Value::Null);
    assert_eq!(json["data"]["latest_anomaly"], serde_json::Value::Null);
    assert!(!json.to_string().contains("explain-secret-value"));

    let id = json["data"]["job"]["id"].as_str().unwrap();
    let human = locron(&state).args(["explain", id]).output().unwrap();
    assert_cmd::assert::Assert::new(human.clone()).success();
    let stdout = String::from_utf8_lossy(&human.stdout);
    for expected in [
        "JOB\n  name: quiet\n",
        &format!("  id: {id}\n"),
        "  enabled: no\n",
        "SCHEDULE\n  schedule: every 1h\n",
        "  timezone: none\n",
        "  next occurrence: none\n",
        "CURRENT STATUS\n  eligibility: disabled\n",
        "  overlap decision: no active run\n",
        "  active runs: 0\n",
        "  global concurrency limit: 16\n",
        "  daemon available: no\n",
        "LATEST RUN\n  none\n",
        "LATEST ANOMALY\n  none\n",
    ] {
        assert!(
            stdout.contains(expected),
            "explain omitted {expected:?}:\n{stdout}"
        );
    }
    assert!(!stdout.contains("explain-secret-value"));

    let why = locron(&state).args(["why", "quiet"]).output().unwrap();
    let why = String::from_utf8(why.stdout).unwrap();
    assert!(
        !why.contains("next occurrence: none"),
        "explain must not change the existing disabled-job why calculation:\n{why}"
    );
    assert!(
        why.contains("  decision: eligible\n"),
        "explain must not change the existing disabled-job why decision:\n{why}"
    );

    let help = locron(&state).args(["explain", "--help"]).output().unwrap();
    let help = String::from_utf8(help.stdout).unwrap();
    assert!(help.contains("locron explain backup"));
    assert!(help.contains("locron explain 018f47a2-4a12-7c35-b9d8-0123456789ab"));
}

#[test]
fn explain_distinguishes_success_only_history_from_an_active_latest_run() {
    let state = tempfile::tempdir().unwrap();
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args(["add", "work", "--every", "1h", "--", "/usr/bin/true"])
            .output()
            .unwrap(),
    )
    .success();
    let mut daemon = start_daemon(&state);
    let submitted = invoke_json(&state, &["run", "work"]);
    let succeeded_id = submitted["data"]["run_id"].as_str().unwrap().to_owned();
    assert_eq!(
        wait_for_run_state(&state, "work", "succeeded"),
        succeeded_id
    );

    let success_only = invoke_json(&state, &["explain", "work"]);
    assert_eq!(success_only["data"]["latest_run"]["id"], succeeded_id);
    assert_eq!(success_only["data"]["latest_run"]["state"], "succeeded");
    assert!(success_only["data"]["latest_run"]["requested_at"].is_string());
    assert!(success_only["data"]["latest_run"]["started_at"].is_string());
    assert!(success_only["data"]["latest_run"]["finished_at"].is_string());
    assert!(success_only["data"]["latest_run"]["duration_micros"].is_number());
    assert_eq!(
        success_only["data"]["latest_anomaly"],
        serde_json::Value::Null
    );

    let _ = daemon.kill();
    let _ = daemon.wait();
    let queued = invoke_json(&state, &["run", "work"]);
    let queued_id = queued["data"]["run_id"].as_str().unwrap();
    let active = invoke_json(&state, &["explain", "work"]);
    assert_eq!(active["data"]["latest_run"]["id"], queued_id);
    assert_eq!(active["data"]["latest_run"]["state"], "queued");
    assert_eq!(
        active["data"]["latest_run"]["started_at"],
        serde_json::Value::Null
    );
    assert_eq!(active["data"]["current_status"]["active_runs"], 1);
    assert_eq!(
        active["data"]["current_status"]["eligibility"],
        "subject_to_admission"
    );
    assert_eq!(
        active["data"]["current_status"]["overlap_decision"],
        "would_skip_overlap"
    );
    assert_eq!(active["data"]["latest_anomaly"], serde_json::Value::Null);
    let human = locron(&state).args(["explain", "work"]).output().unwrap();
    let human = String::from_utf8(human.stdout).unwrap();
    assert!(human.contains("  eligibility: subject to admission\n"));
    assert!(human.contains("  overlap decision: would skip (overlap policy)\n"));
    assert!(human.contains("  nominal time: none\n"));
    assert!(human.contains("  started: unknown\n"));
    assert!(human.contains("  finished: unknown\n"));
    assert!(human.contains("  duration: unknown\n"));
    assert!(human.contains("  reason: unknown\n"));
}

#[test]
fn explain_allows_latest_run_and_anomaly_to_match_then_retains_an_older_anomaly() {
    let state = tempfile::tempdir().unwrap();
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args(["add", "mixed", "--every", "1h", "--", "/usr/bin/true"])
            .output()
            .unwrap(),
    )
    .success();
    let queued = invoke_json(&state, &["run", "mixed"]);
    let cancelled_id = queued["data"]["run_id"].as_str().unwrap().to_owned();
    invoke_json(&state, &["cancel", &cancelled_id]);

    let same = invoke_json(&state, &["explain", "mixed"]);
    assert_eq!(same["data"]["latest_run"]["id"], cancelled_id);
    assert_eq!(same["data"]["latest_anomaly"]["id"], cancelled_id);
    assert_eq!(same["data"]["latest_anomaly"]["state"], "cancelled");
    assert!(same["data"]["latest_anomaly"]["reason"].is_string());

    let mut daemon = start_daemon(&state);
    let submitted = invoke_json(&state, &["run", "mixed"]);
    let succeeded_id = submitted["data"]["run_id"].as_str().unwrap().to_owned();
    assert_eq!(
        wait_for_run_state(&state, "mixed", "succeeded"),
        succeeded_id
    );
    let older = invoke_json(&state, &["explain", "mixed"]);
    assert_eq!(older["data"]["latest_run"]["id"], succeeded_id);
    assert_eq!(older["data"]["latest_run"]["state"], "succeeded");
    assert_eq!(older["data"]["latest_anomaly"]["id"], cancelled_id);
    assert_eq!(older["data"]["latest_anomaly"]["state"], "cancelled");
    let _ = daemon.kill();
    let _ = daemon.wait();
}

#[test]
fn explain_respects_removed_job_history_and_reused_name_identity() {
    let state = tempfile::tempdir().unwrap();
    let first = invoke_json(
        &state,
        &["add", "reused", "--every", "1h", "--", "/usr/bin/true"],
    );
    let first_job_id = first["data"]["id"].as_str().unwrap().to_owned();
    let queued = invoke_json(&state, &["run", "reused"]);
    let old_run_id = queued["data"]["run_id"].as_str().unwrap().to_owned();
    invoke_json(&state, &["remove", "reused"]);

    for reference in ["reused", first_job_id.as_str()] {
        assert_cmd::assert::Assert::new(
            locron(&state)
                .args(["explain", reference])
                .output()
                .unwrap(),
        )
        .failure()
        .code(3);
    }
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args(["why", "--run", &old_run_id])
            .output()
            .unwrap(),
    )
    .success()
    .stdout(predicate::str::contains(&old_run_id));

    let replacement = invoke_json(
        &state,
        &["add", "reused", "--every", "2h", "--", "/usr/bin/true"],
    );
    let replacement_id = replacement["data"]["id"].as_str().unwrap();
    assert_ne!(replacement_id, first_job_id);
    let explained = invoke_json(&state, &["explain", "reused"]);
    assert_eq!(explained["data"]["job"]["id"], replacement_id);
    assert_eq!(explained["data"]["schedule"]["summary"], "every 2h");
    assert_eq!(explained["data"]["latest_run"], serde_json::Value::Null);
    assert_eq!(explained["data"]["latest_anomaly"], serde_json::Value::Null);
}

#[test]
fn human_run_wait_streams_and_prints_the_terminal_outcome_line() {
    let state = tempfile::tempdir().unwrap();
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args(["add", "backup", "--every", "1h", "--", "/usr/bin/true"])
            .output()
            .unwrap(),
    )
    .success();
    let mut daemon = start_daemon(&state);
    let output = locron(&state)
        .args(["run", "backup", "--wait"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "run --wait failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.starts_with("run queued: "),
        "queued line missing:\n{stdout}"
    );
    assert!(
        stdout
            .lines()
            .any(|line| line.starts_with("run finished: ") && line.ends_with(" (succeeded)")),
        "terminal outcome line missing:\n{stdout}"
    );
    let _ = daemon.kill();
    let _ = daemon.wait();
}

#[test]
fn human_why_run_prints_immutable_run_facts() {
    let state = tempfile::tempdir().unwrap();
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args(["add", "backup", "--every", "1h", "--", "/usr/bin/true"])
            .output()
            .unwrap(),
    )
    .success();
    let mut daemon = start_daemon(&state);
    let output = locron(&state)
        .args(["run", "backup", "--wait"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let run_id = stdout
        .lines()
        .find_map(|line| line.strip_prefix("run finished: "))
        .and_then(|line| line.split_whitespace().next())
        .expect("terminal outcome line missing")
        .to_owned();
    let why = locron(&state)
        .args(["why", "--run", &run_id])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&why.stdout);
    for expected in [
        "RUN\n  run id: ",
        "  trigger: manual\n",
        "  nominal time: none\n",
        "  state: succeeded\n",
        "  outcome: succeeded\n",
        "ATTEMPTS\n  attempt 1: succeeded",
        "EVENTS\n",
        "TERMINAL REASON\n",
    ] {
        assert!(
            stdout.contains(expected),
            "why --run omitted {expected:?}:\n{stdout}"
        );
    }
    let _ = daemon.kill();
    let _ = daemon.wait();
}

#[test]
fn human_doctor_prints_one_level_line_per_check() {
    let state = tempfile::tempdir().unwrap();
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args(["add", "backup", "--every", "1h", "--", "/usr/bin/true"])
            .output()
            .unwrap(),
    )
    .success();
    let doctor = locron(&state).args(["doctor"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&doctor.stdout);
    for expected in [
        "ok   state dir: ",
        "ok   database: ",
        "warn daemon: not running\n",
        "ok   execution path: ",
        "ok   process resolution: backup -> ",
        "ok   integrity: database integrity verified\n",
        "ok   foreign key violations: 0\n",
    ] {
        assert!(
            stdout.contains(expected),
            "doctor omitted {expected:?}:\n{stdout}"
        );
    }
    for line in stdout.lines() {
        assert!(
            line.starts_with("ok   ") || line.starts_with("warn ") || line.starts_with("fail "),
            "doctor line lacks a level prefix: {line:?}"
        );
    }
}

#[test]
fn human_config_forms_print_key_value_and_action_lines() {
    let state = tempfile::tempdir().unwrap();
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args(["config", "get", "global_concurrency"])
            .output()
            .unwrap(),
    )
    .success()
    .stdout("global_concurrency=16\n");
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args(["config", "set", "global_concurrency", "8"])
            .output()
            .unwrap(),
    )
    .success()
    .stdout("global_concurrency: configured\n");
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args(["config", "set", "global_concurrency", "16", "--dry-run"])
            .output()
            .unwrap(),
    )
    .success()
    .stdout("global_concurrency: would be configured (dry run; no changes made)\n");
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args(["config", "set", "environment.API_TOKEN", "secret-value"])
            .output()
            .unwrap(),
    )
    .success()
    .stdout("environment.API_TOKEN: configured (value redacted)\n");
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args(["config", "get", "environment.API_TOKEN"])
            .output()
            .unwrap(),
    )
    .success()
    .stdout("environment.API_TOKEN: configured (value redacted)\n");
    // get-all prints one KEY=VALUE line per configured key with environment
    // values redacted.
    let all = locron(&state).args(["config", "get"]).output().unwrap();
    let all = String::from_utf8_lossy(&all.stdout);
    assert!(all.contains("global_concurrency=8\n"), "{all}");
    assert!(!all.contains("secret-value"), "value leaked: {all}");
    assert!(all.contains("environment.API_TOKEN=<redacted>\n"), "{all}");
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args(["config", "set", "environment.LATER", "x", "--dry-run"])
            .output()
            .unwrap(),
    )
    .success()
    .stdout("environment.LATER: configured (value redacted) (dry run; no changes made)\n");
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args(["config", "get", "environment.LATER"])
            .output()
            .unwrap(),
    )
    .success()
    .stdout("environment.LATER: not configured\n");
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args(["config", "unset", "environment.API_TOKEN", "--dry-run"])
            .output()
            .unwrap(),
    )
    .success()
    .stdout("environment.API_TOKEN: unset (dry run; no changes made)\n");
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args(["config", "unset", "environment.API_TOKEN"])
            .output()
            .unwrap(),
    )
    .success()
    .stdout("environment.API_TOKEN: unset\n");
    // The JSON surface keeps the stable envelope.
    let json = locron(&state)
        .args(["--json", "config", "get", "global_concurrency"])
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(json["data"]["value"], 8);
}

#[test]
fn human_import_prints_counts_then_action_lines() {
    let source = tempfile::tempdir().unwrap();
    assert_cmd::assert::Assert::new(
        locron(&source)
            .args(["add", "backup", "--every", "1h", "--", "/usr/bin/true"])
            .output()
            .unwrap(),
    )
    .success();
    let export = locron(&source).args(["export"]).output().unwrap();
    let path = source.path().join("backup.json");
    std::fs::write(&path, &export.stdout).unwrap();

    let destination = tempfile::tempdir().unwrap();
    assert_cmd::assert::Assert::new(
        locron(&destination)
            .arg("import")
            .arg(&path)
            .output()
            .unwrap(),
    )
    .success()
    .stdout(predicate::str::contains(
        "created 1, updated 0, unchanged 0\n",
    ))
    .stdout(predicate::str::contains("created: backup ("));
    assert_cmd::assert::Assert::new(
        locron(&destination)
            .arg("import")
            .arg(&path)
            .output()
            .unwrap(),
    )
    .success()
    .stdout(predicate::str::contains(
        "created 0, updated 0, unchanged 1\n",
    ))
    .stdout(predicate::str::contains("unchanged: backup ("));

    let dry = tempfile::tempdir().unwrap();
    assert_cmd::assert::Assert::new(
        locron(&dry)
            .arg("import")
            .arg(&path)
            .arg("--dry-run")
            .output()
            .unwrap(),
    )
    .success()
    .stdout(predicate::str::contains(
        "dry run: would create 1, update 0, unchanged 0; no changes made\n",
    ))
    .stdout(predicate::str::contains("created: backup ("));
    assert!(!dry.path().join("state.db").exists());
}

#[test]
fn human_prune_prints_the_pruned_counts() {
    let state = tempfile::tempdir().unwrap();
    assert_cmd::assert::Assert::new(locron(&state).args(["prune"]).output().unwrap())
        .success()
        .stdout("pruned: 0 runs, 0 outputs (0 bytes)\n");
    assert_cmd::assert::Assert::new(
        locron(&state)
            .args(["prune", "--dry-run"])
            .output()
            .unwrap(),
    )
    .success()
    .stdout("dry run: would prune 0 runs, 0 outputs (0 bytes)\n");
    let empty = tempfile::tempdir().unwrap();
    assert_cmd::assert::Assert::new(
        locron(&empty)
            .args(["prune", "--dry-run"])
            .output()
            .unwrap(),
    )
    .success()
    .stdout("dry run: would prune 0 runs, 0 outputs (0 bytes)\n");
    assert!(!empty.path().join("state.db").exists());
}

#[test]
fn human_forms_leave_the_json_envelope_untouched() {
    let state = tempfile::tempdir().unwrap();
    let json = locron(&state)
        .args([
            "--json",
            "add",
            "backup",
            "--every",
            "1h",
            "--",
            "/usr/bin/true",
        ])
        .output()
        .unwrap();
    let envelope: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(envelope["schema"], "locron.cli/v1");
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["command"], "add");
    assert_eq!(envelope["data"]["name"], "backup");
    let job_id = envelope["data"]["id"].as_str().unwrap().to_owned();
    let json = locron(&state)
        .args(["--json", "show", "backup"])
        .output()
        .unwrap();
    let envelope: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(envelope["command"], "show");
    assert_eq!(envelope["data"]["id"], job_id);
    let json = locron(&state)
        .args(["--json", "run", "backup", "--dry-run"])
        .output()
        .unwrap();
    let envelope: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(envelope["command"], "run");
    assert_eq!(envelope["data"]["decision"], "eligible");
    let json = locron(&state)
        .args(["--json", "preview", "--cron", "0 9 * * *", "--count", "1"])
        .output()
        .unwrap();
    let envelope: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(envelope["command"], "preview");
    assert_eq!(envelope["data"]["occurrences"].as_array().unwrap().len(), 1);
    let json = locron(&state)
        .args(["--json", "why", "backup"])
        .output()
        .unwrap();
    let envelope: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(envelope["command"], "why");
    assert!(envelope["data"]["job"]["id"].is_string());
    assert!(envelope["data"]["next_occurrence"].is_string());
    let json = locron(&state).args(["--json", "doctor"]).output().unwrap();
    let envelope: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(envelope["command"], "doctor");
    assert!(envelope["data"]["checks"].is_array());
    let json = locron(&state).args(["--json", "history"]).output().unwrap();
    let envelope: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(envelope["command"], "history");
    assert!(envelope["data"].is_array());
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

// --- Export selection and URL import (2026-08-24) ---

#[test]
fn export_selectors_union_dedup_and_no_match_rejected() {
    let state = tempfile::tempdir().unwrap();
    for (name, tag) in [
        ("alpha", "nightly"),
        ("beta", "nightly"),
        ("gamma", "backup"),
    ] {
        assert_cmd::assert::Assert::new(
            locron(&state)
                .args([
                    "add",
                    name,
                    "--every",
                    "1h",
                    "--tag",
                    tag,
                    "--",
                    "/usr/bin/true",
                ])
                .output()
                .unwrap(),
        )
        .success();
    }
    // Name/tag union, deduplicated by job identity, store order preserved.
    let output = locron(&state)
        .args(["export", "--jobs", "alpha,gamma", "--tag", "nightly"])
        .output()
        .unwrap();
    assert_cmd::assert::Assert::new(output)
        .success()
        .stdout(predicate::str::contains("\"schema\": \"locron.export/v1\""));
    let export = locron(&state)
        .args(["export", "--jobs", "alpha,gamma", "--tag", "nightly"])
        .output()
        .unwrap();
    let document: serde_json::Value = serde_json::from_slice(&export.stdout).unwrap();
    let names: Vec<&str> = document["jobs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|job| job["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["alpha", "beta", "gamma"]);

    // JSON mode honors selectors inside the machine envelope.
    let output = locron(&state)
        .args(["--json", "export", "--jobs", "alpha,beta"])
        .output()
        .unwrap();
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(envelope["ok"], true);
    let names: Vec<&str> = envelope["data"]["jobs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|job| job["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["alpha", "beta"]);

    // No-match is a validation error before any document output (human mode:
    // nothing on stdout; the error goes to stderr).
    let output = locron(&state)
        .args(["export", "--jobs", "ghost"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(
        output.stdout.is_empty(),
        "no-match must not produce document output"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("matched no job"),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // JSON mode reports the same validation failure in the machine envelope.
    let output = locron(&state)
        .args(["--json", "export", "--jobs", "ghost,alpha"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(envelope["ok"], false);
    assert_eq!(envelope["error"]["code"], "invalid_request");
    assert!(
        envelope["error"]["message"]
            .as_str()
            .unwrap()
            .contains("--jobs ghost"),
        "unexpected: {envelope}"
    );
}

#[test]
fn export_selectors_redaction_parity_with_full_export() {
    let state = tempfile::tempdir().unwrap();
    for (name, secret) in [("secret", "hunter2"), ("plain", "")] {
        assert_cmd::assert::Assert::new(
            locron(&state)
                .args([
                    "add",
                    name,
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
        let _ = secret;
    }
    let full = locron(&state).arg("export").output().unwrap();
    let full: serde_json::Value = serde_json::from_slice(&full.stdout).unwrap();
    let selected = locron(&state)
        .args(["export", "--jobs", "secret"])
        .output()
        .unwrap();
    let selected: serde_json::Value = serde_json::from_slice(&selected.stdout).unwrap();
    assert_eq!(selected["jobs"].as_array().unwrap().len(), 1);
    let full_secret = full["jobs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|job| job["name"] == "secret")
        .unwrap();
    // The selected export's job entry is byte-identical to the full export's
    // entry for the same job, redaction and omission accounting included.
    assert_eq!(selected["jobs"][0], *full_secret);
    assert_eq!(selected["settings"], full["settings"]);
    assert_eq!(selected["omitted_values"], full["omitted_values"]);
    let names: Vec<&str> = full["jobs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|job| job["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["plain", "secret"]);
}

#[test]
fn export_jobs_round_trip_import_reproduces_exactly_the_selected_jobs() {
    let source = tempfile::tempdir().unwrap();
    for (name, schedule) in [("alpha", "1h"), ("beta", "2h"), ("gamma", "30m")] {
        assert_cmd::assert::Assert::new(
            locron(&source)
                .args(["add", name, "--every", schedule, "--", "/usr/bin/true"])
                .output()
                .unwrap(),
        )
        .success();
    }
    let export = locron(&source)
        .args(["export", "--jobs", "alpha,beta"])
        .output()
        .unwrap();
    assert_cmd::assert::Assert::new(export).success();
    let export = locron(&source)
        .args(["export", "--jobs", "alpha,beta"])
        .output()
        .unwrap();
    let path = source.path().join("subset.json");
    std::fs::write(&path, &export.stdout).unwrap();

    let destination = tempfile::tempdir().unwrap();
    assert_cmd::assert::Assert::new(
        locron(&destination)
            .arg("import")
            .arg(&path)
            .output()
            .unwrap(),
    )
    .success()
    .stdout(predicate::str::contains(
        "created 2, updated 0, unchanged 0",
    ));
    for name in ["alpha", "beta"] {
        let source_show = locron(&source)
            .args(["--json", "show", name])
            .output()
            .unwrap();
        let destination_show = locron(&destination)
            .args(["--json", "show", name])
            .output()
            .unwrap();
        let source_show: serde_json::Value = serde_json::from_slice(&source_show.stdout).unwrap();
        let destination_show: serde_json::Value =
            serde_json::from_slice(&destination_show.stdout).unwrap();
        assert_eq!(
            destination_show["data"]["definition_json"], source_show["data"]["definition_json"],
            "{name} definition must survive the round trip"
        );
        assert_eq!(
            destination_show["data"]["enabled"],
            source_show["data"]["enabled"]
        );
    }
    assert_cmd::assert::Assert::new(
        locron(&destination)
            .args(["show", "gamma"])
            .output()
            .unwrap(),
    )
    .failure()
    .code(3);
}

#[test]
fn export_picker_hook_keeps_stdout_for_the_document_and_stderr_for_the_picker() {
    let state = tempfile::tempdir().unwrap();
    for name in ["alpha", "beta", "gamma"] {
        assert_cmd::assert::Assert::new(
            locron(&state)
                .args(["add", name, "--every", "1h", "--", "/usr/bin/true"])
                .output()
                .unwrap(),
        )
        .success();
    }
    // The hook drives the interactive branch without a PTY; stdout carries
    // only the export document while the picker prompt renders on stderr.
    let output = locron(&state)
        .arg("export")
        .env("LOCRON_TEST_EXPORT_PICKER", "alpha,gamma")
        .env_remove("CI")
        .output()
        .unwrap();
    assert_cmd::assert::Assert::new(output)
        .success()
        .stderr(predicate::str::contains("Select jobs to export"))
        .stderr(predicate::str::contains("picked 2 of 3 jobs"));
    let export = locron(&state)
        .arg("export")
        .env("LOCRON_TEST_EXPORT_PICKER", "alpha,gamma")
        .env_remove("CI")
        .output()
        .unwrap();
    let document: serde_json::Value = serde_json::from_slice(&export.stdout).unwrap();
    let names: Vec<&str> = document["jobs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|job| job["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["alpha", "gamma"]);

    // JSON mode never instantiates the picker: no stderr, full export.
    let output = locron(&state)
        .args(["--json", "export"])
        .env("LOCRON_TEST_EXPORT_PICKER", "alpha,gamma")
        .env_remove("CI")
        .output()
        .unwrap();
    assert!(
        output.stderr.is_empty(),
        "JSON mode must not render the picker: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(envelope["data"]["jobs"].as_array().unwrap().len(), 3);

    // CI wins over the hook: no picker, full export.
    let output = locron(&state)
        .arg("export")
        .env("LOCRON_TEST_EXPORT_PICKER", "alpha,gamma")
        .env("CI", "1")
        .output()
        .unwrap();
    assert!(
        output.stderr.is_empty(),
        "CI must suppress the picker: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let document: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(document["jobs"].as_array().unwrap().len(), 3);

    // An empty scripted selection exports settings only.
    let output = locron(&state)
        .arg("export")
        .env("LOCRON_TEST_EXPORT_PICKER", "")
        .env_remove("CI")
        .output()
        .unwrap();
    assert_cmd::assert::Assert::new(output)
        .success()
        .stderr(predicate::str::contains("picked 0 of 3 jobs"));
    let export = locron(&state)
        .arg("export")
        .env("LOCRON_TEST_EXPORT_PICKER", "")
        .env_remove("CI")
        .output()
        .unwrap();
    let document: serde_json::Value = serde_json::from_slice(&export.stdout).unwrap();
    assert!(document["jobs"].as_array().unwrap().is_empty());

    // A scripted selection naming no live job is a validation error.
    let output = locron(&state)
        .arg("export")
        .env("LOCRON_TEST_EXPORT_PICKER", "ghost")
        .env_remove("CI")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("matched no job"),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// --- URL import fixture ---

use std::fmt::Write as _;
use std::io::Write as IoWrite;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

type FixtureResponse = (u16, Vec<(String, String)>, Vec<u8>);

/// A tiny one-request-per-connection HTTP fixture for URL import tests.
struct ImportFixture {
    address: SocketAddr,
    requests: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl ImportFixture {
    fn start<F>(handler: F) -> Self
    where
        F: Fn(&str) -> FixtureResponse + Send + Sync + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let handler = Arc::new(handler);
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<()>();
        let handle = thread::spawn({
            let requests = Arc::clone(&requests);
            let stop = Arc::clone(&stop);
            let handler = Arc::clone(&handler);
            move || {
                let _ = ready_tx.send(());
                while !stop.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            // A panicking connection must never kill the
                            // listener: keep accepting subsequent requests.
                            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                serve_fixture(handler.as_ref(), &requests, &mut stream);
                            }));
                        }
                        // Transient accept errors must not close the listener.
                        Err(_) => thread::sleep(Duration::from_millis(10)),
                    }
                }
            }
        });
        ready_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("fixture server must start accepting within 5s");
        Self {
            address,
            requests,
            stop,
            handle: Some(handle),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{}", self.address.port(), path)
    }

    fn requests(&self) -> Vec<String> {
        self.requests
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

impl Drop for ImportFixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn serve_fixture(
    handler: &(dyn Fn(&str) -> FixtureResponse + Send + Sync),
    requests: &Mutex<Vec<String>>,
    stream: &mut TcpStream,
) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut buffer = [0_u8; 4096];
    let mut used = 0;
    let header_end = loop {
        if used >= buffer.len() || Instant::now() >= deadline {
            break None;
        }
        match stream.read(&mut buffer[used..]) {
            Ok(0) => break None,
            Ok(count) => {
                used += count;
                if buffer[..used]
                    .windows(4)
                    .any(|window| window == b"\r\n\r\n")
                {
                    break Some(used);
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                thread::sleep(Duration::from_millis(5));
            }
            Err(_) => break None,
        }
    };
    let Some(used) = header_end else { return };
    let head = String::from_utf8_lossy(&buffer[..used]);
    let path = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/")
        .to_owned();
    requests
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(path.clone());
    let (status, headers, body) = handler(&path);
    let reason = match status {
        200 => "OK",
        302 => "Found",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "Unknown",
    };
    let mut response = format!("HTTP/1.1 {status} {reason}\r\n");
    for (name, value) in &headers {
        let _ = write!(response, "{name}: {value}\r\n");
    }
    let _ = write!(
        response,
        "Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    write_all_polling(stream, response.as_bytes());
    write_all_polling(stream, &body);
}

/// Writes `bytes`, retrying WouldBlock/TimedOut until the deadline. The
/// accepted stream inherits O_NONBLOCK from the listener, so a large body
/// must be written in chunks rather than one failing `write_all`.
fn write_all_polling(stream: &mut TcpStream, mut bytes: &[u8]) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !bytes.is_empty() {
        match stream.write(bytes) {
            Ok(0) => break,
            Ok(count) => bytes = &bytes[count..],
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                if Instant::now() >= deadline {
                    break;
                }
                thread::sleep(Duration::from_millis(5));
            }
            Err(_) => break,
        }
    }
}

/// Serves one static response body at every path.
fn static_handler(status: u16, body: Vec<u8>) -> impl Fn(&str) -> FixtureResponse + Send + Sync {
    move |_| (status, Vec::new(), body.clone())
}

fn export_document_bytes(state: &tempfile::TempDir) -> Vec<u8> {
    locron(state).arg("export").output().unwrap().stdout
}

#[test]
fn import_url_matches_file_import_byte_for_byte() {
    let source = tempfile::tempdir().unwrap();
    assert_cmd::assert::Assert::new(
        locron(&source)
            .args([
                "add",
                "alpha",
                "--every",
                "1h",
                "--description",
                "from-url",
                "--",
                "/usr/bin/true",
            ])
            .output()
            .unwrap(),
    )
    .success();
    let document = export_document_bytes(&source);
    let fixture = ImportFixture::start(static_handler(200, document.clone()));

    let from_url = tempfile::tempdir().unwrap();
    assert_cmd::assert::Assert::new(
        locron(&from_url)
            .arg("import")
            .arg(fixture.url("/doc.json"))
            .output()
            .unwrap(),
    )
    .success()
    .stdout(predicate::str::contains(
        "created 1, updated 0, unchanged 0",
    ));

    let from_file = tempfile::tempdir().unwrap();
    let path = source.path().join("doc.json");
    std::fs::write(&path, &document).unwrap();
    assert_cmd::assert::Assert::new(
        locron(&from_file)
            .arg("import")
            .arg(&path)
            .output()
            .unwrap(),
    )
    .success()
    .stdout(predicate::str::contains(
        "created 1, updated 0, unchanged 0",
    ));

    // Both destinations hold the identical job definition; only the newly
    // allocated destination identity may differ.
    for destination in [&from_url, &from_file] {
        let show = locron(destination)
            .args(["--json", "show", "alpha"])
            .output()
            .unwrap();
        let show: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
        assert_eq!(show["data"]["description"], "from-url");
        assert_eq!(
            show["data"]["definition_json"],
            show["data"]["definition_json"]
        );
    }
    let url_show = locron(&from_url)
        .args(["--json", "show", "alpha"])
        .output()
        .unwrap();
    let file_show = locron(&from_file)
        .args(["--json", "show", "alpha"])
        .output()
        .unwrap();
    let url_show: serde_json::Value = serde_json::from_slice(&url_show.stdout).unwrap();
    let file_show: serde_json::Value = serde_json::from_slice(&file_show.stdout).unwrap();
    assert_eq!(
        url_show["data"]["definition_json"],
        file_show["data"]["definition_json"]
    );
}

#[test]
fn import_url_dry_run_does_not_mutate() {
    let source = tempfile::tempdir().unwrap();
    assert_cmd::assert::Assert::new(
        locron(&source)
            .args(["add", "alpha", "--every", "1h", "--", "/usr/bin/true"])
            .output()
            .unwrap(),
    )
    .success();
    let fixture = ImportFixture::start(static_handler(200, export_document_bytes(&source)));
    let destination = tempfile::tempdir().unwrap();
    assert_cmd::assert::Assert::new(
        locron(&destination)
            .args(["import", &fixture.url("/doc.json"), "--dry-run"])
            .output()
            .unwrap(),
    )
    .success()
    .stdout(predicate::str::contains("<non-durable").not());
    assert!(!destination.path().join("state.db").exists());
    assert_eq!(fixture.requests(), ["/doc.json"]);
}

#[test]
fn import_url_rejects_redacted_plaintext_and_malformed_bodies() {
    let source = tempfile::tempdir().unwrap();
    assert_cmd::assert::Assert::new(
        locron(&source)
            .args([
                "add",
                "secret",
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

    // A redacted document with omissions is rejected exactly like a file.
    let redacted = ImportFixture::start(static_handler(200, export_document_bytes(&source)));
    let destination = tempfile::tempdir().unwrap();
    let output = locron(&destination)
        .arg("import")
        .arg(redacted.url("/redacted.json"))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("cannot be imported faithfully"),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!destination.path().join("state.db").exists());

    // A plaintext document still requires the acknowledgement flag.
    let plaintext_document = locron(&source)
        .args(["export", "--include-values", "--acknowledge-plaintext"])
        .output()
        .unwrap()
        .stdout;
    let plaintext = ImportFixture::start(static_handler(200, plaintext_document));
    let output = locron(&destination)
        .arg("import")
        .arg(plaintext.url("/plaintext.json"))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("--accept-plaintext-values"),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!destination.path().join("state.db").exists());

    // Malformed bodies are document validation failures, not fetch failures.
    let malformed = ImportFixture::start(static_handler(200, b"not json".to_vec()));
    let output = locron(&destination)
        .arg("import")
        .arg(malformed.url("/bad.json"))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("invalid export document"),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!destination.path().join("state.db").exists());
}

#[test]
fn import_url_oversized_body_is_a_fetch_failure() {
    let oversized = vec![b'x'; 16 * 1024 * 1024 + 1];
    let fixture = ImportFixture::start(static_handler(200, oversized));
    let destination = tempfile::tempdir().unwrap();
    let output = locron(&destination)
        .arg("import")
        .arg(fixture.url("/huge.json"))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(5));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("exceeds the 16777216-byte limit"),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!destination.path().join("state.db").exists());
}

#[test]
fn import_url_redirect_chain_succeeds_and_redirect_loop_fails() {
    let source = tempfile::tempdir().unwrap();
    assert_cmd::assert::Assert::new(
        locron(&source)
            .args(["add", "alpha", "--every", "1h", "--", "/usr/bin/true"])
            .output()
            .unwrap(),
    )
    .success();
    let document = export_document_bytes(&source);
    let fixture = ImportFixture::start(move |path| match path {
        "/start.json" => (
            302,
            vec![("Location".into(), "/mid.json".into())],
            Vec::new(),
        ),
        "/mid.json" => (
            302,
            vec![("Location".into(), "/end.json".into())],
            Vec::new(),
        ),
        "/end.json" => (200, Vec::new(), document.clone()),
        "/loop.json" => (
            302,
            vec![("Location".into(), "/loop.json".into())],
            Vec::new(),
        ),
        _ => (404, Vec::new(), Vec::new()),
    });
    let destination = tempfile::tempdir().unwrap();
    assert_cmd::assert::Assert::new(
        locron(&destination)
            .arg("import")
            .arg(fixture.url("/start.json"))
            .output()
            .unwrap(),
    )
    .success()
    .stdout(predicate::str::contains(
        "created 1, updated 0, unchanged 0",
    ));
    assert_eq!(
        fixture.requests(),
        ["/start.json", "/mid.json", "/end.json"],
        "redirects must be followed in order"
    );

    let output = locron(&destination)
        .arg("import")
        .arg(fixture.url("/loop.json"))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(5));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("redirected more than 10"),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        fixture
            .requests()
            .iter()
            .filter(|path| path.as_str() == "/loop.json")
            .count()
            > 10,
        "redirect loop must actually be followed until the cap"
    );
}

#[test]
fn import_url_http_errors_map_to_category_5() {
    let fixture = ImportFixture::start(|path| match path {
        "/missing.json" => (404, Vec::new(), b"not found".to_vec()),
        "/broken.json" => (500, Vec::new(), b"boom".to_vec()),
        _ => (200, Vec::new(), b"{}".to_vec()),
    });
    for (path, status) in [("/missing.json", "HTTP 404"), ("/broken.json", "HTTP 500")] {
        let destination = tempfile::tempdir().unwrap();
        let output = locron(&destination)
            .arg("import")
            .arg(fixture.url(path))
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(5),
            "{path} must map to exit category 5"
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(status),
            "unexpected stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!destination.path().join("state.db").exists());
    }
}

#[test]
fn import_url_userinfo_and_unsupported_scheme_rejected_without_requests() {
    let fixture = ImportFixture::start(static_handler(200, b"{}".to_vec()));
    let destination = tempfile::tempdir().unwrap();

    // Userinfo is rejected at parse time as a validation error, before any
    // request is attempted.
    let userinfo_url = fixture
        .url("/doc.json")
        .replacen("http://", "http://user:pass@", 1);
    let output = locron(&destination)
        .arg("import")
        .arg(&userinfo_url)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("userinfo"),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // A non-HTTP scheme is a fetch-class failure.
    let output = locron(&destination)
        .arg("import")
        .arg("ftp://example.test/doc.json")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(5));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("scheme"),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(
        fixture.requests().is_empty(),
        "rejected URLs must never reach the network: {:?}",
        fixture.requests()
    );
}

#[test]
fn import_url_rolls_back_on_late_destination_conflict() {
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
    let fixture = ImportFixture::start(static_handler(200, serde_json::to_vec(&document).unwrap()));
    let output = locron(&state)
        .arg("import")
        .arg(fixture.url("/ambiguous.json"))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(3));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("resolve to different destination jobs"),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    for name in ["alpha", "beta"] {
        let show = locron(&state)
            .args(["--json", "show", name])
            .output()
            .unwrap();
        let show: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
        assert_eq!(
            show["data"]["current_revision"], 1,
            "{name} must not be mutated"
        );
    }
}
