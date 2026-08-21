//! End-to-end command contract tests.

use std::io::{BufRead, BufReader};
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
