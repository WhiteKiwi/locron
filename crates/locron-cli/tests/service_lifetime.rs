#![cfg(unix)]

//! Cross-process service-lifetime acceptance fixtures for Unix daemon shutdown.

use std::fs;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use locron_store::{DaemonLock, LockMetadata, RunRecord, Store};

const STARTUP_TIMEOUT: Duration = Duration::from_secs(8);
const SHORT_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);
const FORCED_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(50);

struct DaemonChild {
    child: Option<Child>,
}

impl DaemonChild {
    fn start(state: &tempfile::TempDir) -> Self {
        let child = locron(state)
            .args(["daemon", "run"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn daemon");
        let daemon = Self { child: Some(child) };
        wait_until("daemon lock acquisition", STARTUP_TIMEOUT, || {
            DaemonLock::try_prove_free(&state.path().join("daemon.lock")).is_err()
        });
        wait_until("daemon wake socket", STARTUP_TIMEOUT, || {
            state.path().join("wake.sock").exists()
        });
        daemon
    }

    fn pid(&self) -> u32 {
        self.child.as_ref().expect("daemon child available").id()
    }

    fn send_sigterm(&self) {
        let output = Command::new("/bin/kill")
            .args(["-TERM", &self.pid().to_string()])
            .output()
            .expect("invoke /bin/kill");
        assert!(
            output.status.success(),
            "send SIGTERM to daemon: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn wait(mut self, timeout: Duration) -> CapturedExit {
        let deadline = Instant::now() + timeout;
        let child = self.child.as_mut().expect("daemon child available");
        let status = loop {
            if let Some(status) = child.try_wait().expect("poll daemon status") {
                break status;
            }
            assert!(
                Instant::now() < deadline,
                "daemon did not exit within {timeout:?}"
            );
            thread::sleep(Duration::from_millis(20));
        };
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        child
            .stdout
            .take()
            .expect("captured daemon stdout")
            .read_to_end(&mut stdout)
            .expect("read daemon stdout");
        child
            .stderr
            .take()
            .expect("captured daemon stderr")
            .read_to_end(&mut stderr)
            .expect("read daemon stderr");
        self.child = None;
        CapturedExit {
            status,
            stdout,
            stderr,
        }
    }
}

impl Drop for DaemonChild {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

struct CapturedExit {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

impl CapturedExit {
    fn assert_success(&self) {
        assert!(
            self.status.success(),
            "daemon exited with {}\nstdout:\n{}\nstderr:\n{}",
            self.status,
            String::from_utf8_lossy(&self.stdout),
            String::from_utf8_lossy(&self.stderr)
        );
    }
}

struct ProcessTreeCleanup {
    identities: Vec<(PathBuf, PathBuf)>,
}

impl ProcessTreeCleanup {
    fn new(identities: Vec<(PathBuf, PathBuf)>) -> Self {
        Self { identities }
    }
}

impl Drop for ProcessTreeCleanup {
    fn drop(&mut self) {
        for (pid_path, command_path) in &self.identities {
            let Some(pid) = read_pid(pid_path) else {
                continue;
            };
            if original_process_is_live(pid, command_path) {
                let _ = Command::new("/bin/kill")
                    .args(["-KILL", &pid.to_string()])
                    .status();
            }
        }
    }
}

fn locron(state: &tempfile::TempDir) -> Command {
    let mut command = Command::new(assert_cmd::cargo::cargo_bin!("locron"));
    command.arg("--state-dir").arg(state.path());
    command
}

fn invoke(state: &tempfile::TempDir, arguments: &[&str]) -> std::process::Output {
    let output = locron(state)
        .args(arguments)
        .output()
        .expect("invoke locron");
    assert!(
        output.status.success(),
        "locron {arguments:?} failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn add_process_job(state: &tempfile::TempDir, name: &str, executable: &Path, arguments: &[&Path]) {
    let mut command = locron(state);
    command.args([
        "add",
        name,
        "--every",
        "1h",
        "--disabled",
        "--no-timeout",
        "--",
    ]);
    command.arg(executable).args(arguments);
    let output = command.output().expect("add process job");
    assert!(
        output.status.success(),
        "add {name} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn enqueue_manual(state: &tempfile::TempDir, name: &str) -> String {
    let output = locron(state)
        .args(["--json", "run", name])
        .output()
        .expect("enqueue manual run");
    assert!(
        output.status.success(),
        "enqueue {name} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let envelope: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("parse run result");
    envelope["data"]["run_id"]
        .as_str()
        .expect("run result includes run_id")
        .to_owned()
}

fn wait_for_run_state(state: &tempfile::TempDir, run_id: &str, expected: &str) -> RunRecord {
    let database = state.path().join("state.db");
    let deadline = Instant::now() + STARTUP_TIMEOUT;
    loop {
        if let Ok(store) = Store::open_read_only(&database)
            && let Ok(run) = store.run(run_id)
            && run.state == expected
        {
            return run;
        }
        assert!(
            Instant::now() < deadline,
            "run {run_id} did not enter {expected}"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

fn wait_until(description: &str, timeout: Duration, mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + timeout;
    while !predicate() {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {description}"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

fn lock_lifetime_id(state: &tempfile::TempDir) -> String {
    let bytes = fs::read(state.path().join("daemon.lock")).expect("read daemon lock metadata");
    serde_json::from_slice::<LockMetadata>(&bytes)
        .expect("parse daemon lock metadata")
        .lifetime_id
}

fn assert_clean_lifetime(state: &tempfile::TempDir, lifetime_id: &str) {
    let query = format!(
        "SELECT COALESCE(exit_class, '') || '|' || CASE WHEN ended_at_us IS NULL THEN 'open' ELSE 'ended' END FROM scheduler_lifetimes WHERE id='{lifetime_id}'"
    );
    let output = Command::new("sqlite3")
        .arg("-batch")
        .arg("-noheader")
        .arg(state.path().join("state.db"))
        .arg(query)
        .output()
        .expect("invoke sqlite3 for lifetime evidence");
    assert!(
        output.status.success(),
        "query scheduler lifetime: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "clean|ended"
    );
}

fn assert_lock_released(state: &tempfile::TempDir) {
    DaemonLock::try_prove_free(&state.path().join("daemon.lock"))
        .expect("daemon lock released after clean shutdown");
}

fn write_script(path: &Path, body: &str) {
    fs::write(path, body).expect("write process fixture");
}

fn read_pid(path: &Path) -> Option<u32> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

fn original_process_is_live(pid: u32, command_path: &Path) -> bool {
    let output = Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "stat=", "-o", "command="])
        .output()
        .expect("inspect process identity with ps");
    if !output.status.success() {
        return false;
    }
    let line = String::from_utf8_lossy(&output.stdout);
    let mut fields = line.split_whitespace();
    let Some(status) = fields.next() else {
        return false;
    };
    !status.starts_with('Z') && line.contains(command_path.to_string_lossy().as_ref())
}

#[test]
fn sigterm_stops_admission_while_short_target_finishes_during_natural_drain() {
    let state = tempfile::tempdir().expect("temporary state");
    invoke(&state, &["config", "set", "global_concurrency", "1"]);

    let short_script = state.path().join("short-target.sh");
    let short_started = state.path().join("short.started");
    let short_finished = state.path().join("short.finished");
    write_script(
        &short_script,
        "#!/bin/sh\nset -eu\nprintf 'started\\n' > \"$1\"\nsleep 3\nprintf 'finished\\n' > \"$2\"\n",
    );
    add_process_job(
        &state,
        "short-drain",
        Path::new("/bin/sh"),
        &[&short_script, &short_started, &short_finished],
    );

    let late_script = state.path().join("late-target.sh");
    let late_started = state.path().join("late.started");
    write_script(
        &late_script,
        "#!/bin/sh\nset -eu\nprintf 'admitted\\n' > \"$1\"\n",
    );
    add_process_job(
        &state,
        "queued-after-signal",
        Path::new("/bin/sh"),
        &[&late_script, &late_started],
    );

    let daemon = DaemonChild::start(&state);
    let lifetime_id = lock_lifetime_id(&state);
    let short_run = enqueue_manual(&state, "short-drain");
    wait_until("short target rendezvous", STARTUP_TIMEOUT, || {
        short_started.exists()
    });
    wait_for_run_state(&state, &short_run, "running");

    let queued_run = enqueue_manual(&state, "queued-after-signal");
    wait_for_run_state(&state, &queued_run, "queued");
    daemon.send_sigterm();
    let exit = daemon.wait(SHORT_SHUTDOWN_TIMEOUT);
    exit.assert_success();

    assert!(
        short_finished.is_file(),
        "short target did not finish naturally"
    );
    let completed = wait_for_run_state(&state, &short_run, "succeeded");
    assert!(completed.finished_at_us.is_some());
    let queued = wait_for_run_state(&state, &queued_run, "queued");
    assert!(queued.finished_at_us.is_none());
    assert!(
        !late_started.exists(),
        "daemon admitted queued work after SIGTERM"
    );
    assert_clean_lifetime(&state, &lifetime_id);
    assert_lock_released(&state);
}

#[test]
fn sigterm_forces_long_process_tree_to_cancel_then_closes_lifetime_and_lock() {
    let state = tempfile::tempdir().expect("temporary state");
    let leader_script = state.path().join("long-leader.sh");
    let grandchild_script = state.path().join("long-grandchild.sh");
    let leader_pid_path = state.path().join("leader.pid");
    let grandchild_pid_path = state.path().join("grandchild.pid");
    write_script(
        &leader_script,
        "#!/bin/sh\nset -eu\nprintf '%s\\n' \"$$\" > \"$1\"\ntrap '' TERM\n/bin/sh \"$2\" \"$3\" &\nwait \"$!\"\n",
    );
    write_script(
        &grandchild_script,
        "#!/bin/sh\nset -eu\nprintf '%s\\n' \"$$\" > \"$1\"\ntrap '' TERM\nwhile :; do sleep 1; done\n",
    );
    let _cleanup = ProcessTreeCleanup::new(vec![
        (leader_pid_path.clone(), leader_script.clone()),
        (grandchild_pid_path.clone(), grandchild_script.clone()),
    ]);

    add_process_job(
        &state,
        "forced-tree",
        Path::new("/bin/sh"),
        &[
            &leader_script,
            &leader_pid_path,
            &grandchild_script,
            &grandchild_pid_path,
        ],
    );
    let daemon = DaemonChild::start(&state);
    let lifetime_id = lock_lifetime_id(&state);
    let run_id = enqueue_manual(&state, "forced-tree");
    wait_until("leader PID rendezvous", STARTUP_TIMEOUT, || {
        leader_pid_path.exists()
    });
    wait_until("grandchild PID rendezvous", STARTUP_TIMEOUT, || {
        grandchild_pid_path.exists()
    });
    wait_for_run_state(&state, &run_id, "running");

    let leader_pid = read_pid(&leader_pid_path).expect("leader PID");
    let grandchild_pid = read_pid(&grandchild_pid_path).expect("grandchild PID");
    assert!(original_process_is_live(leader_pid, &leader_script));
    assert!(original_process_is_live(grandchild_pid, &grandchild_script));

    daemon.send_sigterm();
    let exit = daemon.wait(FORCED_SHUTDOWN_TIMEOUT);
    exit.assert_success();

    wait_until("leader process-group member exit", STARTUP_TIMEOUT, || {
        !original_process_is_live(leader_pid, &leader_script)
    });
    wait_until(
        "grandchild process-group member exit",
        STARTUP_TIMEOUT,
        || !original_process_is_live(grandchild_pid, &grandchild_script),
    );
    let cancelled = wait_for_run_state(&state, &run_id, "cancelled");
    assert!(cancelled.finished_at_us.is_some());
    assert_eq!(cancelled.reason.as_deref(), Some("attempt was cancelled"));
    assert_clean_lifetime(&state, &lifetime_id);
    assert_lock_released(&state);
}
