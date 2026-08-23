//! Black-box milestone acceptance scenarios for the existing CLI.

#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use locron_store::DaemonLock;
use serde_json::Value;

const COMMAND_TIMEOUT: Duration = Duration::from_secs(12);
const LIVE_UPDATE_TIMEOUT: Duration = Duration::from_secs(40);
/// The retry-rendering test observes the transient `retry_wait` state and then
/// crosses a daemon restart. An idle daemon only admits late-enqueued runs at
/// its 30-second safety reconciliation, so under CPU contention (slow enqueue)
/// the whole timeline needs a budget above that cycle.
const RETRY_RENDERING_TIMEOUT: Duration = Duration::from_secs(75);

fn locron(state: &tempfile::TempDir) -> Command {
    let mut command = Command::new(assert_cmd::cargo::cargo_bin!("locron"));
    command.arg("--state-dir").arg(state.path());
    command
}

fn invoke(state: &tempfile::TempDir, args: &[&str]) -> Output {
    let output = locron(state).args(args).output().unwrap();
    assert!(
        output.status.success(),
        "locron {args:?} failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn invoke_json(state: &tempfile::TempDir, args: &[&str]) -> Value {
    let output = invoke(state, args);
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema"], "locron.cli/v1");
    assert_eq!(value["ok"], true);
    value
}

fn json_lines(output: &Output) -> Vec<Value> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
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

fn queue_manual(state: &tempfile::TempDir, name: &str) -> String {
    invoke_json(state, &["--json", "run", name])["data"]["run_id"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn history(state: &tempfile::TempDir, name: &str) -> Vec<Value> {
    invoke_json(state, &["--json", "history", name])["data"]
        .as_array()
        .unwrap()
        .clone()
}

fn wait_for_run_state(
    state: &tempfile::TempDir,
    name: &str,
    expected: &str,
    timeout: Duration,
) -> Value {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(run) = history(state, name)
            .into_iter()
            .find(|run| run["state"] == expected)
        {
            return run;
        }
        assert!(
            Instant::now() < deadline,
            "run for {name} did not enter {expected} before {timeout:?}"
        );
        thread::sleep(Duration::from_millis(25));
    }
}

fn wait_for_child(mut child: Child, timeout: Duration) -> Output {
    let deadline = Instant::now() + timeout;
    loop {
        if child.try_wait().unwrap().is_some() {
            return child.wait_with_output().unwrap();
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let output = child.wait_with_output().unwrap();
            panic!(
                "child did not finish before {timeout:?}: stdout={} stderr={}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        thread::sleep(Duration::from_millis(25));
    }
}

struct Daemon {
    child: Option<Child>,
}

impl Daemon {
    fn start(state: &tempfile::TempDir) -> Self {
        let mut child = locron(state)
            .args(["daemon", "run"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let lock_path = state.path().join("daemon.lock");
        let deadline = Instant::now() + COMMAND_TIMEOUT;
        while DaemonLock::try_prove_free(&lock_path).is_ok() && Instant::now() < deadline {
            assert_eq!(
                child.try_wait().unwrap(),
                None,
                "daemon exited during startup"
            );
            thread::sleep(Duration::from_millis(20));
        }
        assert!(
            DaemonLock::try_prove_free(&lock_path).is_err(),
            "daemon did not acquire its durable lock"
        );
        Self { child: Some(child) }
    }

    fn id(&self) -> u32 {
        self.child.as_ref().unwrap().id()
    }

    fn assert_running(&mut self) {
        assert_eq!(self.child.as_mut().unwrap().try_wait().unwrap(), None);
    }

    fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            if child.try_wait().unwrap().is_none() {
                child.kill().unwrap();
            }
            child.wait().unwrap();
        }
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        self.stop();
    }
}

fn log_payload(state: &tempfile::TempDir, run_id: &str, attempt: u16) -> Vec<u8> {
    let attempt = attempt.to_string();
    let output = invoke(state, &["--json", "logs", run_id, "--attempt", &attempt]);
    json_lines(&output)
        .into_iter()
        .flat_map(|record| {
            base64::engine::general_purpose::STANDARD
                .decode(record["data"]["bytes"].as_str().unwrap())
                .unwrap()
        })
        .collect()
}

#[derive(Debug, Eq, PartialEq)]
struct SchedulerSnapshot {
    crontab: Option<(Option<i32>, Vec<u8>, Vec<u8>)>,
    persistent_files: Vec<(String, Vec<u8>)>,
}

fn append_tree(root: &Path, path: &Path, output: &mut Vec<(String, Vec<u8>)>) {
    let relative = path.strip_prefix(root).unwrap_or(path);
    let label = relative.to_string_lossy().into_owned();
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) => {
            output.push((label, format!("error:{:?}", error.kind()).into_bytes()));
            return;
        }
    };
    if metadata.file_type().is_symlink() {
        let target = fs::read_link(path).unwrap_or_else(|_| PathBuf::from("<unreadable>"));
        output.push((label, format!("symlink:{}", target.display()).into_bytes()));
    } else if metadata.is_file() {
        output.push((
            label,
            fs::read(path).unwrap_or_else(|error| format!("error:{:?}", error.kind()).into_bytes()),
        ));
    } else if metadata.is_dir() {
        output.push((label, b"directory".to_vec()));
        let mut entries = fs::read_dir(path)
            .map(|entries| entries.filter_map(Result::ok).collect::<Vec<_>>())
            .unwrap_or_default();
        entries.sort_by_key(fs::DirEntry::file_name);
        for entry in entries {
            append_tree(root, &entry.path(), output);
        }
    }
}

fn scheduler_snapshot() -> SchedulerSnapshot {
    let crontab = Command::new("crontab")
        .arg("-l")
        .output()
        .ok()
        .map(|output| (output.status.code(), output.stdout, output.stderr));
    let mut roots = vec![PathBuf::from("/etc/crontab"), PathBuf::from("/etc/cron.d")];
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        if cfg!(target_os = "macos") {
            roots.push(home.join("Library/LaunchAgents"));
        }
        if cfg!(target_os = "linux") {
            roots.push(home.join(".config/systemd/user"));
        }
    }
    let mut persistent_files = Vec::new();
    for root in roots {
        if root.exists() {
            append_tree(&root, &root, &mut persistent_files);
        } else {
            persistent_files.push((root.display().to_string(), b"missing".to_vec()));
        }
    }
    SchedulerSnapshot {
        crontab,
        persistent_files,
    }
}

#[test]
fn cron_every_and_at_jobs_register_and_execute_without_os_scheduler_mutation() {
    let state = tempfile::tempdir().unwrap();
    let before = scheduler_snapshot();
    let at = timestamp_after(Duration::from_secs(3_600));
    let cases = [
        ("calendar", ["--cron", "*/5 * * * *"]),
        ("interval", ["--every", "1h"]),
        ("one-time", ["--at", at.as_str()]),
    ];

    let mut runs = Vec::new();
    for (name, schedule) in cases {
        let added = invoke_json(
            &state,
            &[
                "--json",
                "add",
                name,
                schedule[0],
                schedule[1],
                "--",
                "/usr/bin/printf",
                name,
            ],
        );
        assert_eq!(added["data"]["name"], name);
        runs.push((name, queue_manual(&state, name)));
    }

    let mut daemon = Daemon::start(&state);
    for (name, run_id) in &runs {
        let run = wait_for_run_state(&state, name, "succeeded", COMMAND_TIMEOUT);
        assert_eq!(run["id"], *run_id);
        assert_eq!(run["trigger"], "manual");
        assert_eq!(log_payload(&state, run_id, 1), name.as_bytes());
    }
    daemon.stop();

    assert_eq!(
        scheduler_snapshot(),
        before,
        "locron must not install cron, launchd, or systemd job definitions"
    );
}

#[test]
fn running_daemon_recognizes_a_job_schedule_and_target_update_without_restart() {
    let state = tempfile::tempdir().unwrap();
    let original_at = timestamp_after(Duration::from_secs(3_600));
    invoke(
        &state,
        &[
            "add",
            "live-update",
            "--at",
            &original_at,
            "--shell",
            "printf stale",
        ],
    );
    let mut daemon = Daemon::start(&state);
    let daemon_id = daemon.id();
    let updated_at = timestamp_after(Duration::from_secs(2));

    let updated = invoke_json(
        &state,
        &[
            "--json",
            "update",
            "live-update",
            "--at",
            &updated_at,
            "--shell",
            "printf updated",
        ],
    );
    assert_eq!(updated["data"]["current_revision"], 2);

    let run = wait_for_run_state(&state, "live-update", "succeeded", LIVE_UPDATE_TIMEOUT);
    daemon.assert_running();
    assert_eq!(daemon.id(), daemon_id);
    assert_eq!(run["revision"], 2);
    assert_eq!(run["trigger"], "scheduled");
    assert!(run["nominal_us"].as_i64().is_some());
    assert_eq!(
        log_payload(&state, run["id"].as_str().unwrap(), 1),
        b"updated"
    );
}

#[test]
fn cli_covers_preview_and_the_complete_job_lifecycle() {
    let state = tempfile::tempdir().unwrap();
    invoke(
        &state,
        &[
            "add",
            "lifecycle",
            "--every",
            "1h",
            "--disabled",
            "--",
            "/usr/bin/printf",
            "initial",
        ],
    );

    let preview = invoke_json(&state, &["--json", "preview", "lifecycle", "--count", "3"]);
    assert_eq!(preview["data"]["occurrences"].as_array().unwrap().len(), 3);
    assert_eq!(
        invoke_json(&state, &["--json", "show", "lifecycle"])["data"]["enabled"],
        false
    );
    assert_eq!(
        invoke_json(&state, &["--json", "list", "--all"])["data"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    let updated = invoke_json(
        &state,
        &[
            "--json",
            "update",
            "lifecycle",
            "--rename",
            "lifecycle-renamed",
            "--description",
            "updated definition",
            "--tag",
            "acceptance",
            "--shell",
            "printf lifecycle",
        ],
    );
    assert_eq!(updated["data"]["name"], "lifecycle-renamed");
    assert_eq!(updated["data"]["current_revision"], 2);

    invoke(&state, &["enable", "lifecycle-renamed"]);
    assert_eq!(
        invoke_json(&state, &["--json", "show", "lifecycle-renamed"])["data"]["enabled"],
        true
    );
    assert_eq!(
        invoke_json(&state, &["--json", "list"])["data"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    invoke(&state, &["disable", "lifecycle-renamed"]);
    let shown = invoke_json(&state, &["--json", "show", "lifecycle-renamed"]);
    assert_eq!(shown["data"]["enabled"], false);
    assert_eq!(shown["data"]["description"], "updated definition");

    let run_id = queue_manual(&state, "lifecycle-renamed");
    let mut daemon = Daemon::start(&state);
    wait_for_run_state(&state, "lifecycle-renamed", "succeeded", COMMAND_TIMEOUT);
    assert_eq!(log_payload(&state, &run_id, 1), b"lifecycle");
    daemon.stop();

    invoke(&state, &["remove", "lifecycle-renamed"]);
    assert!(
        invoke_json(&state, &["--json", "list"])["data"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let all = invoke_json(&state, &["--json", "list", "--all"]);
    assert!(all["data"].as_array().unwrap().is_empty());
    let removed_show = locron(&state)
        .args(["--json", "show", "lifecycle-renamed"])
        .output()
        .unwrap();
    assert!(!removed_show.status.success());
    let retained_run = invoke_json(&state, &["--json", "why", "--run", &run_id]);
    assert_eq!(retained_run["data"]["run"]["state"], "succeeded");
}

#[test]
fn scheduled_history_show_why_and_logs_render_available_run_facts() {
    let state = tempfile::tempdir().unwrap();
    let at = timestamp_after(Duration::from_secs(2));
    invoke(
        &state,
        &[
            "add",
            "observable",
            "--at",
            &at,
            "--shell",
            "printf observable; sleep 0.05",
        ],
    );
    thread::sleep(Duration::from_millis(2_200));
    let _daemon = Daemon::start(&state);
    let run = wait_for_run_state(&state, "observable", "succeeded", COMMAND_TIMEOUT);

    let nominal = run["nominal_us"].as_i64().expect("scheduled time");
    let admitted = run["eligible_at_us"]
        .as_i64()
        .expect("admission eligibility time");
    let finished = run["finished_at_us"].as_i64().expect("finish time");
    assert!(admitted >= nominal);
    assert!(finished >= admitted);
    // `eligible_at_us` is not the attempt's actual start time and cannot close criterion 7.
    assert_eq!(run["trigger"], "catch_up");
    assert_eq!(run["state"], "succeeded");
    assert!(run["reason"].as_str().is_some());

    let shown = invoke_json(&state, &["--json", "show", "observable"]);
    let definition: Value =
        serde_json::from_str(shown["data"]["definition_json"].as_str().unwrap()).unwrap();
    assert_eq!(definition["target"]["kind"], "shell");
    assert_eq!(definition["schedule"]["kind"], "at");

    let run_id = run["id"].as_str().unwrap();
    let why = invoke_json(&state, &["--json", "why", "--run", run_id]);
    assert_eq!(why["data"]["run"]["id"], run_id);
    assert_eq!(why["data"]["run"]["trigger"], "catch_up");
    assert!(why["data"]["events"].as_array().is_some());
    assert!(why["data"]["explanation"].as_str().is_some());
    assert_eq!(log_payload(&state, run_id, 1), b"observable");
}

#[test]
fn run_wait_renders_ordered_retry_output_frames_for_one_durable_run() {
    let state = tempfile::tempdir().unwrap();
    invoke(
        &state,
        &[
            "add",
            "retry-rendering",
            "--every",
            "1h",
            "--retries",
            "1",
            "--backoff",
            "fixed",
            "--retry-delay",
            // Wide window: `retry_wait` is only observable for the retry
            // delay, and the fork-per-poll state check can miss a short
            // window under CPU contention.
            "5s",
            "--shell",
            "printf 'attempt-%s' \"$LOCRON_ATTEMPT\"; [ \"$LOCRON_ATTEMPT\" -gt 1 ]",
        ],
    );
    let waiter = locron(&state)
        .args(["--json", "run", "retry-rendering", "--wait"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let mut first_daemon = Daemon::start(&state);
    let waiting = wait_for_run_state(
        &state,
        "retry-rendering",
        "retry_wait",
        RETRY_RENDERING_TIMEOUT,
    );
    let run_id = waiting["id"].as_str().unwrap().to_owned();
    first_daemon.stop();
    // Outlive the 5s retry deadline so the second daemon finds an expired
    // retry_wait. Starting early is still correct: retry-wait recovery waits
    // for the deadline durably.
    thread::sleep(Duration::from_millis(5_100));
    let _second_daemon = Daemon::start(&state);

    let output = wait_for_child(waiter, RETRY_RENDERING_TIMEOUT);
    assert!(
        output.status.success(),
        "wait failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let records = json_lines(&output);
    let frames = records
        .iter()
        .filter(|record| record["record"] == "frame")
        .collect::<Vec<_>>();
    assert_eq!(
        frames
            .iter()
            .map(|frame| frame["data"]["attempt"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        [1, 2]
    );
    assert_eq!(
        frames
            .iter()
            .map(|frame| {
                base64::engine::general_purpose::STANDARD
                    .decode(frame["data"]["bytes"].as_str().unwrap())
                    .unwrap()
            })
            .collect::<Vec<_>>(),
        [b"attempt-1".to_vec(), b"attempt-2".to_vec()]
    );
    let terminal = records.last().unwrap();
    assert_eq!(terminal["schema"], "locron.stream/v1");
    assert_eq!(terminal["record"], "result");
    assert_eq!(terminal["terminal"], true);
    assert_eq!(terminal["ok"], true);
    assert_eq!(terminal["data"]["run_id"], run_id);
    assert_eq!(
        wait_for_run_state(
            &state,
            "retry-rendering",
            "succeeded",
            RETRY_RENDERING_TIMEOUT
        )["id"],
        run_id
    );
}

#[test]
fn manual_run_dry_run_reports_eligibility_without_durable_mutation() {
    let state = tempfile::tempdir().unwrap();
    invoke(
        &state,
        &[
            "add",
            "manual-dry-run",
            "--every",
            "1h",
            "--",
            "/usr/bin/true",
        ],
    );
    let before = invoke_json(&state, &["--json", "show", "manual-dry-run"]);
    assert!(history(&state, "manual-dry-run").is_empty());

    let simulated = invoke_json(&state, &["--json", "run", "manual-dry-run", "--dry-run"]);
    assert_eq!(simulated["data"]["dry_run"], true);
    assert_eq!(simulated["data"]["durable"], false);
    assert_eq!(simulated["data"]["decision"], "eligible");
    assert_eq!(simulated["data"]["capacity_reserved"], false);

    assert!(history(&state, "manual-dry-run").is_empty());
    let after = invoke_json(&state, &["--json", "show", "manual-dry-run"]);
    assert_eq!(after["data"], before["data"]);
    assert!(
        fs::read_dir(state.path().join("outputs"))
            .unwrap()
            .next()
            .is_none(),
        "manual dry-run must not create an output artifact"
    );
}
