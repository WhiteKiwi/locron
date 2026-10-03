//! Native role ownership/control checks against the separate CLI executable.
#![cfg(windows)]

use futures_util::StreamExt;
use locron_store::{DaemonLock, RoleLockMetadata, StatePaths, Store};
use std::io::{Read, Seek, SeekFrom};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;

fn cli_executable() -> PathBuf {
    std::env::var_os("LOCRON_TEST_BINARY").map_or_else(
        || PathBuf::from(assert_cmd::cargo::cargo_bin!("locron")),
        |override_path| {
            let path = PathBuf::from(override_path);
            assert!(
                path.is_absolute() && path.is_file(),
                "LOCRON_TEST_BINARY must identify the absolute same-revision CLI artifact"
            );
            path
        },
    )
}

struct Fixture {
    paths: StatePaths,
    _temporary: tempfile::TempDir,
}

impl Fixture {
    fn absent() -> Self {
        let temporary = tempfile::tempdir().expect("temporary parent");
        let paths = StatePaths::new(temporary.path().join("private native 生命周期"));
        assert!(!paths.root.exists());
        Self {
            paths,
            _temporary: temporary,
        }
    }

    fn new() -> Self {
        let mut fixture = Self::absent();
        let guard = locron_core::filesystem::DirectoryGuard::private(&fixture.paths.root)
            .expect("private fixture state");
        fixture.paths = StatePaths::new(guard.normalized_path().to_path_buf());
        drop(Store::open(fixture.paths.clone(), "test", 1).expect("initialized fixture store"));
        // The test binary and CLI must derive identical versioned SID/file identities.
        locron_core::notification::instance_identity(&fixture.paths.root)
            .expect("native state identity");
        fixture
    }

    fn daemon(&self, registered: bool) -> NativeProcess {
        let mut command = Command::new(cli_executable());
        command
            .arg("--state-dir")
            .arg(&self.paths.root)
            .args(["daemon", "run"]);
        if registered {
            command.arg("--service-mode");
        }
        NativeProcess::start(command)
    }

    fn supervised_daemon(&self, supervisor: &str, worker: &str) -> NativeProcess {
        let mut command = Command::new(cli_executable());
        command.arg("--state-dir").arg(&self.paths.root).args([
            "daemon",
            "run",
            "--service-mode",
            "--supervisor-lifetime",
            supervisor,
            "--worker-lifetime",
            worker,
        ]);
        NativeProcess::start(command)
    }

    fn parent_activation(&self, lifetime: &str) -> DaemonLock {
        DaemonLock::acquire_role(
            &self.paths.daemon_activation_lock,
            &locron_store::LockMetadata {
                pid: std::process::id(),
                lifetime_id: lifetime.to_owned(),
                started_at_us: 1,
                binary_version: "test".into(),
            },
            true,
        )
        .expect("owned held supervisor fixture lease")
    }

    fn dashboard(&self) -> NativeProcess {
        let mut command = Command::new(cli_executable());
        command.arg("--state-dir").arg(&self.paths.root).args([
            "--json",
            "dashboard",
            "--port",
            "0",
            "--bind",
            "127.0.0.1",
            "serve",
            "--service-mode",
        ]);
        NativeProcess::start(command)
    }

    fn queue_marker_run(&self, name: &str, gate: Option<&Path>) -> String {
        let marker = self.paths.root.join("native-completed.txt");
        let mut command = Command::new(cli_executable());
        command
            .arg("--state-dir")
            .arg(&self.paths.root)
            .args([
                "--json",
                "add",
                name,
                "--every",
                "1h",
                "--disabled",
                "--cwd",
            ])
            .arg(&self.paths.root)
            .args([
                "--env",
                &format!("WINDOWS_NATIVE_MARKER={}", marker.display()),
            ]);
        if let Some(gate) = gate {
            command.args(["--env", &format!("WINDOWS_NATIVE_GATE={}", gate.display())]);
        }
        let added = command
            .arg("--")
            .arg(std::env::current_exe().expect("native fixture executable"))
            .args([
                "--exact",
                "native_process_target",
                "--nocapture",
                "--test-threads=1",
            ])
            .output()
            .expect("add native target");
        assert!(
            added.status.success(),
            "native add failed: {}",
            String::from_utf8_lossy(&added.stderr)
        );
        let run = Command::new(cli_executable())
            .arg("--state-dir")
            .arg(&self.paths.root)
            .args(["--json", "run", name])
            .output()
            .expect("queue native target");
        assert!(
            run.status.success(),
            "native run failed: {}",
            String::from_utf8_lossy(&run.stderr)
        );
        let envelope: serde_json::Value =
            serde_json::from_slice(&run.stdout).expect("run envelope");
        envelope["data"]["run_id"]
            .as_str()
            .expect("durable run id")
            .to_owned()
    }

    fn wait_running(&self, run_id: &str) -> u64 {
        let store = Store::open(self.paths.clone(), "test", 1).expect("observe running target");
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let run = store.run(run_id).expect("durable run fact");
            if run.state == "running"
                && let Some(heartbeat) = self.heartbeat()
            {
                return heartbeat;
            }
            assert!(
                matches!(run.state.as_str(), "queued" | "starting" | "running"),
                "gated target ended as {}: {:?}",
                run.state,
                run.reason
            );
            assert!(Instant::now() < deadline, "gated target did not start");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn heartbeat(&self) -> Option<u64> {
        std::fs::read_to_string(self.paths.root.join("native-completed.heartbeat"))
            .ok()
            .and_then(|value| value.parse().ok())
    }

    fn assert_target_continues(&self, run_id: &str, previous_heartbeat: u64) {
        let store = Store::open(self.paths.clone(), "test", 1).expect("preserved durable target");
        let run = store.run(run_id).expect("preserved running state");
        assert_eq!(run.state, "running");
        assert_eq!(run.finished_at_us, None);
        assert!(
            !store
                .cancellation_requested(run_id)
                .expect("cancellation fact")
        );
        let deadline = Instant::now() + Duration::from_secs(3);
        let baseline = loop {
            if let Some(current) = self.heartbeat() {
                break current.max(previous_heartbeat);
            }
            assert!(
                Instant::now() < deadline,
                "native heartbeat was not readable after exit"
            );
            std::thread::sleep(Duration::from_millis(20));
        };
        loop {
            if self.heartbeat().is_some_and(|current| current > baseline) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "native target stopped making progress"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn wait_marker_run(&self, run_id: &str) {
        let store = Store::open(self.paths.clone(), "test", 1).expect("observe actual run");
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let run = store.run(run_id).expect("durable run fact");
            if run.state == "succeeded" {
                assert_eq!(
                    std::fs::read(self.paths.root.join("native-completed.txt")).unwrap(),
                    b"done"
                );
                return;
            }
            assert!(
                matches!(run.state.as_str(), "queued" | "starting" | "running"),
                "native target ended as {}: {:?}",
                run.state,
                run.reason
            );
            assert!(
                Instant::now() < deadline,
                "native target did not complete: {}",
                run.state
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn wait_initialized_store(&self, process: &mut NativeProcess) {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if self.paths.database.is_file()
                && let Ok(store) = Store::open_read_only(&self.paths.database)
                && let Ok(settings) = store.settings()
            {
                assert_eq!(
                    settings.execution_path,
                    locron_core::execution::default_execution_path()
                );
                return;
            }
            assert!(
                process
                    .child
                    .try_wait()
                    .expect("first-run daemon process")
                    .is_none(),
                "daemon exited during first-run store initialization: {}",
                process.stderr()
            );
            assert!(
                Instant::now() < deadline,
                "first-run store was not initialized: {}",
                process.stderr()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

struct NativeProcess {
    child: Child,
    output: tempfile::NamedTempFile,
    diagnostics: tempfile::NamedTempFile,
}

impl NativeProcess {
    fn start(mut command: Command) -> Self {
        let output = tempfile::NamedTempFile::new().expect("startup output file");
        let diagnostics = tempfile::NamedTempFile::new().expect("diagnostic file");
        let child = command
            .stdout(Stdio::from(output.reopen().expect("independent stdout handle")))
            .stderr(Stdio::from(diagnostics.reopen().expect("independent stderr handle")))
            .creation_flags(0x0800_0000) // Exercise headless roles without a console Ctrl-C path.
            .spawn()
            .expect("native CLI process");
        Self {
            child,
            output,
            diagnostics,
        }
    }

    fn stderr(&mut self) -> String {
        Self::read_capture(&mut self.diagnostics)
    }

    fn read_capture(capture: &mut tempfile::NamedTempFile) -> String {
        capture.seek(SeekFrom::Start(0)).expect("rewind capture");
        let mut bytes = Vec::new();
        capture
            .take(64 * 1024)
            .read_to_end(&mut bytes)
            .expect("read bounded capture");
        String::from_utf8_lossy(&bytes).into_owned()
    }

    fn wait_dashboard(&mut self, paths: &StatePaths) -> (RoleLockMetadata, String) {
        let owner = self.wait_owner(&paths.dashboard_lock);
        assert!(owner.service_mode);
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let output = Self::read_capture(&mut self.output);
            for line in output.lines() {
                if let Ok(envelope) = serde_json::from_str::<serde_json::Value>(line)
                    && envelope["ok"] == true
                    && let Some(url) = envelope["data"]["access_url"].as_str()
                {
                    let url = reqwest::Url::parse(url).expect("dashboard startup URL");
                    assert_eq!(url.host_str(), Some("127.0.0.1"));
                    assert!(url.port().is_some_and(|port| port != 0));
                    return (owner, url.to_string());
                }
            }
            assert!(
                self.child
                    .try_wait()
                    .expect("dashboard startup state")
                    .is_none(),
                "dashboard exited before startup JSON: {}",
                self.stderr()
            );
            assert!(
                Instant::now() < deadline,
                "dashboard startup JSON missing: {}",
                self.stderr()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn wait_owner(&mut self, lock: &Path) -> RoleLockMetadata {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if let Some(status) = self.child.try_wait().expect("startup process state") {
                panic!(
                    "CLI exited before role ownership ({status}): {}",
                    self.stderr()
                );
            }
            // This readiness hint cannot create the first-run state being observed.
            // The guarded read and expected child PID remain the ownership check.
            if DaemonLock::owner_sidecar(lock).is_file()
                && let Some(owner) = DaemonLock::read_role_metadata(lock)
                    .ok()
                    .flatten()
                    .filter(|owner| owner.metadata.pid == self.child.id())
            {
                return owner;
            }
            assert!(
                Instant::now() < deadline,
                "role owner was not published: {}",
                self.stderr()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn wait_exit(&mut self, deadline: Instant) -> ExitStatus {
        loop {
            if let Some(status) = self.child.try_wait().expect("native process state") {
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "actual CLI process did not exit: {}",
                self.stderr()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn hard_stop(&mut self) {
        self.child.kill().expect("terminate owned test process");
        self.child.wait().expect("reap owned test process");
    }

    fn stop_role(
        &mut self,
        paths: &StatePaths,
        role: &str,
        owner: &RoleLockMetadata,
    ) -> ExitStatus {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            match locron_core::notification::request_shutdown(
                &paths.root,
                role,
                &owner.metadata.lifetime_id,
            ) {
                Ok(()) => break,
                Err(error) => {
                    assert!(
                        self.child
                            .try_wait()
                            .expect("control process state")
                            .is_none(),
                        "CLI exited before control acceptance: {}",
                        self.stderr()
                    );
                    assert!(
                        Instant::now() < deadline,
                        "{role} control unavailable: {error}"
                    );
                    std::thread::sleep(Duration::from_millis(20));
                }
            }
        }
        let status = self.wait_exit(deadline);
        let lock = match role {
            "daemon-activation" => &paths.daemon_activation_lock,
            "daemon-worker" => &paths.daemon_worker_activation_lock,
            "dashboard" => &paths.dashboard_lock,
            _ => panic!("unsupported fixture role"),
        };
        loop {
            // A passive expected-PID observer has already confirmed this owned process exited.
            if DaemonLock::try_prove_free(lock).is_ok() {
                assert!(
                    Instant::now() < deadline,
                    "actual {role} exit exceeded the control budget"
                );
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "{role} ownership remained after process exit"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn stop_activation(&mut self, paths: &StatePaths, owner: &RoleLockMetadata) {
        let status = self.stop_role(paths, "daemon-activation", owner);
        assert!(
            status.success(),
            "cooperative CLI exit failed ({status}): {}",
            self.stderr()
        );
        DaemonLock::try_prove_free(&paths.daemon_activation_lock)
            .expect("actual activation ownership exited");
    }
}

impl Drop for NativeProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn native_process_target() {
    let Some(marker) = std::env::var_os("WINDOWS_NATIVE_MARKER") else {
        return;
    };
    let marker = PathBuf::from(marker);
    if let Some(gate) = std::env::var_os("WINDOWS_NATIVE_GATE") {
        let gate = PathBuf::from(gate);
        let deadline = Instant::now() + Duration::from_secs(60);
        let mut heartbeat = 0_u64;
        while !gate.exists() {
            assert!(Instant::now() < deadline, "native target gate did not open");
            std::fs::write(marker.with_extension("heartbeat"), heartbeat.to_string())
                .expect("native progress fact");
            heartbeat += 1;
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    std::fs::write(marker, b"done").expect("native target side effect");
    std::process::exit(0);
}

#[test]
fn cancelling_a_waiting_activation_preserves_the_manual_daemon() {
    let fixture = Fixture::new();
    let mut manual = fixture.daemon(false);
    let manual_owner = manual.wait_owner(&fixture.paths.daemon_lock);
    assert!(!manual_owner.service_mode);
    let mut registered = fixture.daemon(true);
    let activation = registered.wait_owner(&fixture.paths.daemon_activation_lock);
    assert!(activation.service_mode);
    let actual = DaemonLock::read_role_metadata(&fixture.paths.daemon_lock)
        .expect("actual owner metadata")
        .expect("manual owner");
    assert_eq!(
        actual.metadata.lifetime_id,
        manual_owner.metadata.lifetime_id
    );

    registered.stop_activation(&fixture.paths, &activation);
    assert!(
        manual
            .child
            .try_wait()
            .expect("manual process state")
            .is_none()
    );
    assert!(DaemonLock::try_prove_free(&fixture.paths.daemon_lock).is_err());
    let actual = DaemonLock::read_role_metadata(&fixture.paths.daemon_lock)
        .expect("preserved owner metadata")
        .expect("manual owner remains");
    assert_eq!(actual.metadata, manual_owner.metadata);
}

#[test]
fn activation_automatically_takes_ownership_after_the_manual_daemon_exits() {
    let fixture = Fixture::new();
    let mut manual = fixture.daemon(false);
    manual.wait_owner(&fixture.paths.daemon_lock);
    let mut registered = fixture.daemon(true);
    let activation = registered.wait_owner(&fixture.paths.daemon_activation_lock);
    manual.hard_stop();

    let daemon_owner = registered.wait_owner(&fixture.paths.daemon_lock);
    assert!(daemon_owner.service_mode);
    let still_owned = DaemonLock::read_role_metadata(&fixture.paths.daemon_activation_lock)
        .expect("continuous activation metadata")
        .expect("activation remains live");
    assert_eq!(still_owned.metadata, activation.metadata);
    assert!(DaemonLock::try_prove_free(&fixture.paths.daemon_activation_lock).is_err());
    let run_id = fixture.queue_marker_run("activation-target", None);
    fixture.wait_marker_run(&run_id);
    registered.stop_activation(&fixture.paths, &activation);
    DaemonLock::try_prove_free(&fixture.paths.daemon_lock).expect("actual daemon ownership exited");
    assert!(locron_core::notification::send_wake(&fixture.paths.root).is_err());
}

#[test]
fn duplicate_registered_activation_and_stale_control_lifetime_are_refused() {
    let fixture = Fixture::new();
    let mut registered = fixture.daemon(true);
    let activation = registered.wait_owner(&fixture.paths.daemon_activation_lock);
    registered.wait_owner(&fixture.paths.daemon_lock);
    let mut duplicate = fixture.daemon(true);
    let status = duplicate.wait_exit(Instant::now() + Duration::from_secs(30));
    assert!(
        !status.success(),
        "duplicate registered activation must fail closed"
    );
    assert!(
        locron_core::notification::request_shutdown(
            &fixture.paths.root,
            "daemon-activation",
            &uuid::Uuid::now_v7().to_string(),
        )
        .is_err()
    );
    assert!(
        registered
            .child
            .try_wait()
            .expect("original process state")
            .is_none()
    );
    let actual = DaemonLock::read_role_metadata(&fixture.paths.daemon_activation_lock)
        .expect("original activation metadata")
        .expect("original activation survives");
    assert_eq!(actual.metadata, activation.metadata);
    registered.stop_activation(&fixture.paths, &activation);
    DaemonLock::try_prove_free(&fixture.paths.daemon_lock).expect("original daemon exited");
}

#[test]
fn invalid_supervised_daemon_arguments_never_create_child_state() {
    let supervisor = uuid::Uuid::now_v7().to_string();
    let worker = uuid::Uuid::now_v7().to_string();
    for arguments in [
        vec!["--supervisor-lifetime", supervisor.as_str()],
        vec![
            "--service-mode",
            "--supervisor-lifetime",
            supervisor.as_str(),
        ],
        vec!["--service-mode", "--worker-lifetime", worker.as_str()],
        vec![
            "--supervisor-lifetime",
            supervisor.as_str(),
            "--worker-lifetime",
            worker.as_str(),
        ],
        vec![
            "--service-mode",
            "--supervisor-lifetime",
            "not-a-uuid",
            "--worker-lifetime",
            worker.as_str(),
        ],
        vec![
            "--service-mode",
            "--supervisor-lifetime",
            supervisor.as_str(),
            "--worker-lifetime",
            supervisor.as_str(),
        ],
        // Valid UUIDs still require an existing held parent before any child writes.
        vec![
            "--service-mode",
            "--supervisor-lifetime",
            supervisor.as_str(),
            "--worker-lifetime",
            worker.as_str(),
        ],
    ] {
        let fixture = Fixture::absent();
        let mut command = Command::new(cli_executable());
        command
            .arg("--state-dir")
            .arg(&fixture.paths.root)
            .args(["daemon", "run"])
            .args(arguments);
        let mut child = NativeProcess::start(command);
        assert!(
            !child
                .wait_exit(Instant::now() + Duration::from_secs(30))
                .success()
        );
        assert!(!fixture.paths.root.exists());
    }
}

#[test]
fn stale_supervisor_uuid_cannot_create_a_daemon_worker_lease() {
    let fixture = Fixture::new();
    let supervisor = uuid::Uuid::now_v7().to_string();
    let _parent = fixture.parent_activation(&supervisor);
    let before = DaemonLock::read_role_metadata(&fixture.paths.daemon_activation_lock)
        .unwrap()
        .unwrap();
    let mut child = fixture.supervised_daemon(
        &uuid::Uuid::now_v7().to_string(),
        &uuid::Uuid::now_v7().to_string(),
    );
    assert!(
        !child
            .wait_exit(Instant::now() + Duration::from_secs(30))
            .success()
    );
    assert!(!fixture.paths.daemon_worker_activation_lock.exists());
    assert!(!fixture.paths.daemon_lock.exists());
    assert_eq!(
        DaemonLock::probe_existing(&fixture.paths.daemon_activation_lock).unwrap(),
        locron_store::LockProbe::Held
    );
    assert_eq!(
        DaemonLock::read_role_metadata(&fixture.paths.daemon_activation_lock)
            .unwrap()
            .unwrap(),
        before
    );
}

#[test]
fn exact_daemon_worker_cancellation_preserves_the_parent_and_manual_scheduler() {
    let fixture = Fixture::new();
    let mut manual = fixture.daemon(false);
    let manual_owner = manual.wait_owner(&fixture.paths.daemon_lock);
    let supervisor = uuid::Uuid::now_v7().to_string();
    let _parent = fixture.parent_activation(&supervisor);
    let worker = uuid::Uuid::now_v7().to_string();
    let mut child = fixture.supervised_daemon(&supervisor, &worker);
    let activation = child.wait_owner(&fixture.paths.daemon_worker_activation_lock);
    assert_eq!(activation.metadata.lifetime_id, worker);
    assert!(activation.service_mode);
    assert!(
        locron_core::notification::request_shutdown(
            &fixture.paths.root,
            "daemon-worker",
            &uuid::Uuid::now_v7().to_string(),
        )
        .is_err()
    );
    assert!(
        child
            .stop_role(&fixture.paths, "daemon-worker", &activation)
            .success()
    );
    assert_manual_owner_is_preserved(&mut manual, &fixture.paths, &manual_owner);
    assert_eq!(
        DaemonLock::probe_existing(&fixture.paths.daemon_activation_lock).unwrap(),
        locron_store::LockProbe::Held
    );
    assert!(
        locron_core::notification::request_shutdown(&fixture.paths.root, "daemon-worker", &worker)
            .is_err()
    );
}

#[test]
fn supervised_daemon_activates_with_its_worker_uuid_after_the_manual_owner_exits() {
    let fixture = Fixture::new();
    let mut manual = fixture.daemon(false);
    manual.wait_owner(&fixture.paths.daemon_lock);
    let supervisor = uuid::Uuid::now_v7().to_string();
    let _parent = fixture.parent_activation(&supervisor);
    let worker = uuid::Uuid::now_v7().to_string();
    let mut child = fixture.supervised_daemon(&supervisor, &worker);
    let activation = child.wait_owner(&fixture.paths.daemon_worker_activation_lock);
    manual.hard_stop();
    let role = child.wait_owner(&fixture.paths.daemon_lock);
    assert!(role.service_mode);
    assert_eq!(role.metadata.lifetime_id, worker);
    assert_eq!(
        DaemonLock::read_role_metadata(&fixture.paths.daemon_worker_activation_lock)
            .unwrap()
            .unwrap(),
        activation
    );
    let run_id = fixture.queue_marker_run("supervised-worker-target", None);
    fixture.wait_marker_run(&run_id);
    assert!(
        child
            .stop_role(&fixture.paths, "daemon-worker", &activation)
            .success()
    );
    DaemonLock::try_prove_free(&fixture.paths.daemon_lock).expect("actual owned daemon exit");
    assert!(locron_core::notification::send_wake(&fixture.paths.root).is_err());
    assert_eq!(
        DaemonLock::probe_existing(&fixture.paths.daemon_activation_lock).unwrap(),
        locron_store::LockProbe::Held
    );
}

fn dashboard_client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(30))
        .build()
        .expect("native loopback client")
}

fn assert_manual_owner_is_preserved(
    manual: &mut NativeProcess,
    paths: &StatePaths,
    owner: &RoleLockMetadata,
) {
    assert!(
        manual
            .child
            .try_wait()
            .expect("manual daemon process")
            .is_none()
    );
    assert!(DaemonLock::try_prove_free(&paths.daemon_lock).is_err());
    let actual = DaemonLock::read_role_metadata(&paths.daemon_lock)
        .expect("manual ownership metadata")
        .expect("manual owner remains");
    assert_eq!(&actual, owner);
}

fn assert_dashboard_is_gone(paths: &StatePaths, owner: &RoleLockMetadata) {
    DaemonLock::try_prove_free(&paths.dashboard_lock).expect("actual dashboard ownership exited");
    assert!(
        DaemonLock::read_role_metadata(&paths.dashboard_lock)
            .expect("dashboard owner sidecar")
            .is_none()
    );
    assert!(
        locron_core::notification::request_shutdown(
            &paths.root,
            "dashboard",
            &owner.metadata.lifetime_id,
        )
        .is_err()
    );
}

#[test]
fn first_run_registered_daemon_explicitly_creates_private_state_before_control() {
    let fixture = Fixture::absent();
    assert_eq!(
        locron_core::notification::instance_identity(&fixture.paths.root)
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::NotFound
    );
    assert!(!fixture.paths.root.exists());
    let mut registered = fixture.daemon(true);
    let activation = registered.wait_owner(&fixture.paths.daemon_activation_lock);
    let owner = registered.wait_owner(&fixture.paths.daemon_lock);
    assert!(owner.service_mode);
    assert!(locron_core::filesystem::is_private(&fixture.paths.root, true).unwrap());
    fixture.wait_initialized_store(&mut registered);
    let run_id = fixture.queue_marker_run("first-run-target", None);
    fixture.wait_marker_run(&run_id);
    registered.stop_activation(&fixture.paths, &activation);
    DaemonLock::try_prove_free(&fixture.paths.daemon_lock).expect("first-run daemon exited");
}

#[tokio::test(flavor = "multi_thread")]
async fn first_run_registered_dashboard_explicitly_creates_private_state_before_control() {
    let fixture = Fixture::absent();
    assert!(
        locron_core::notification::request_shutdown(
            &fixture.paths.root,
            "dashboard",
            &uuid::Uuid::now_v7().to_string(),
        )
        .is_err()
    );
    assert!(!fixture.paths.root.exists());
    let mut dashboard = fixture.dashboard();
    let (owner, url) = dashboard.wait_dashboard(&fixture.paths);
    assert!(locron_core::filesystem::is_private(&fixture.paths.root, true).unwrap());
    let token = locron_server::token::ensure(&fixture.paths).expect("first-run private token");
    let response = dashboard_client()
        .get(format!("{url}api/v1/session"))
        .header("authorization", format!("token {token}"))
        .send()
        .await
        .expect("first-run dashboard listener");
    assert!(response.status().is_success());
    let status = dashboard.stop_role(&fixture.paths, "dashboard", &owner);
    assert!(
        status.success(),
        "first-run dashboard exit failed: {}",
        dashboard.stderr()
    );
    assert_dashboard_is_gone(&fixture.paths, &owner);
}

#[tokio::test(flavor = "multi_thread")]
async fn actual_dashboard_exit_closes_active_sse_while_a_native_job_continues() {
    let fixture = Fixture::new();
    let mut manual = fixture.daemon(false);
    let manual_owner = manual.wait_owner(&fixture.paths.daemon_lock);
    let mut dashboard = fixture.dashboard();
    let (dashboard_owner, url) = dashboard.wait_dashboard(&fixture.paths);
    let token = locron_server::token::ensure(&fixture.paths).expect("private dashboard token");
    assert!(!NativeProcess::read_capture(&mut dashboard.output).contains(&token));
    let gate = fixture.paths.root.join("native-release.gate");
    let run_id = fixture.queue_marker_run("sse-native-target", Some(&gate));
    let heartbeat = fixture.wait_running(&run_id);

    let response = dashboard_client()
        .get(format!("{url}api/v1/runs/{run_id}/stream"))
        .header("authorization", format!("token {token}"))
        .send()
        .await
        .expect("active native SSE request");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let mut stream = response.bytes_stream();
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut bytes = Vec::new();
        while !String::from_utf8_lossy(&bytes).contains("event: run") {
            bytes.extend_from_slice(
                &stream
                    .next()
                    .await
                    .expect("initial run event")
                    .expect("SSE bytes"),
            );
            assert!(bytes.len() < 64 * 1024, "initial SSE event was not bounded");
        }
    })
    .await
    .expect("SSE was active before role shutdown");

    let status = dashboard.stop_role(&fixture.paths, "dashboard", &dashboard_owner);
    assert!(
        status.success(),
        "active SSE prevented graceful CLI exit ({status}): {}",
        dashboard.stderr()
    );
    assert_dashboard_is_gone(&fixture.paths, &dashboard_owner);
    tokio::time::timeout(Duration::from_secs(3), async {
        while let Some(chunk) = stream.next().await {
            chunk.expect("finite SSE completion");
        }
    })
    .await
    .expect("SSE transport ended after actual dashboard process exit");

    assert_manual_owner_is_preserved(&mut manual, &fixture.paths, &manual_owner);
    fixture.assert_target_continues(&run_id, heartbeat);
    std::fs::write(gate, b"release").expect("release fixture target");
    fixture.wait_marker_run(&run_id);
}

#[tokio::test(flavor = "multi_thread")]
async fn actual_dashboard_exit_is_bounded_with_idle_http_clients() {
    let fixture = Fixture::new();
    let mut manual = fixture.daemon(false);
    let manual_owner = manual.wait_owner(&fixture.paths.daemon_lock);
    let mut dashboard = fixture.dashboard();
    let (dashboard_owner, url) = dashboard.wait_dashboard(&fixture.paths);
    let token = locron_server::token::ensure(&fixture.paths).expect("private dashboard token");
    let parsed = reqwest::Url::parse(&url).expect("startup address");
    let address = ("127.0.0.1", parsed.port().expect("ephemeral port"));
    let gate = fixture.paths.root.join("native-release.gate");
    let run_id = fixture.queue_marker_run("idle-native-target", Some(&gate));
    let heartbeat = fixture.wait_running(&run_id);

    let mut headers = tokio::net::TcpStream::connect(address)
        .await
        .expect("idle headers client");
    headers
        .write_all(b"GET /api/v1/jobs HTTP/1.1\r\nHost: 127.0.0.1\r\n")
        .await
        .expect("incomplete HTTP headers");
    let mut body = tokio::net::TcpStream::connect(address)
        .await
        .expect("idle body client");
    body.write_all(format!(
        "POST /api/v1/jobs HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nAuthorization: token {token}\r\nContent-Type: application/json\r\nContent-Length: 10000\r\n\r\n{{",
        address.1,
    ).as_bytes()).await.expect("incomplete HTTP body");
    let response = dashboard_client()
        .get(format!("{url}api/v1/session"))
        .header("authorization", format!("token {token}"))
        .send()
        .await
        .expect("dashboard remains responsive");
    assert!(response.status().is_success());
    tokio::time::sleep(Duration::from_millis(50)).await;

    let status = dashboard.stop_role(&fixture.paths, "dashboard", &dashboard_owner);
    if !status.success() {
        assert_eq!(
            status.code(),
            Some(5),
            "unexpected CLI exit: {}",
            dashboard.stderr()
        );
        let output = NativeProcess::read_capture(&mut dashboard.output);
        let failure = output
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .find(|envelope| envelope["ok"] == false)
            .expect("bounded drain error envelope");
        assert_eq!(failure["error"]["code"], "service_io");
        assert!(
            failure["error"]["message"]
                .as_str()
                .expect("drain failure message")
                .contains("dashboard connection drain exceeded its deadline")
        );
    }
    assert_dashboard_is_gone(&fixture.paths, &dashboard_owner);
    assert_manual_owner_is_preserved(&mut manual, &fixture.paths, &manual_owner);
    fixture.assert_target_continues(&run_id, heartbeat);
    // Both malicious connections remain held until actual process and role-lock exit are proven.
    drop((headers, body));
    std::fs::write(gate, b"release").expect("release fixture target");
    fixture.wait_marker_run(&run_id);
}
