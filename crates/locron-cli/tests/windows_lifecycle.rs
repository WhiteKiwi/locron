//! Native role ownership/control checks against the separate CLI executable.
#![cfg(windows)]

use locron_store::{DaemonLock, RoleLockMetadata, StatePaths, Store};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

struct Fixture {
    paths: StatePaths,
    _temporary: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let temporary = tempfile::tempdir().expect("temporary parent");
        let guard = locron_core::filesystem::DirectoryGuard::private(
            &temporary.path().join("private native 生命周期"),
        )
        .expect("private fixture state");
        let paths = StatePaths::new(guard.normalized_path().to_path_buf());
        drop(Store::open(paths.clone(), "test", 1).expect("initialized fixture store"));
        // The test binary and CLI must derive identical versioned SID/file identities.
        locron_core::notification::instance_identity(&paths.root).expect("native state identity");
        Self {
            paths,
            _temporary: temporary,
        }
    }

    fn daemon(&self, registered: bool) -> NativeProcess {
        let mut command = Command::new(assert_cmd::cargo::cargo_bin!("locron"));
        command
            .arg("--state-dir")
            .arg(&self.paths.root)
            .args(["daemon", "run"]);
        if registered {
            command.arg("--service-mode");
        }
        NativeProcess::start(command)
    }

    fn queue_marker_run(&self) -> String {
        let marker = self.paths.root.join("native-completed.txt");
        let added = Command::new(assert_cmd::cargo::cargo_bin!("locron"))
            .arg("--state-dir")
            .arg(&self.paths.root)
            .args([
                "--json",
                "add",
                "activation-target",
                "--every",
                "1h",
                "--disabled",
                "--cwd",
            ])
            .arg(&self.paths.root)
            .args([
                "--env",
                &format!("WINDOWS_NATIVE_MARKER={}", marker.display()),
                "--",
            ])
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
        let run = Command::new(assert_cmd::cargo::cargo_bin!("locron"))
            .arg("--state-dir")
            .arg(&self.paths.root)
            .args(["--json", "run", "activation-target", "--force"])
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
}

struct NativeProcess {
    child: Child,
    diagnostics: File,
}

impl NativeProcess {
    fn start(mut command: Command) -> Self {
        let diagnostics = tempfile::tempfile().expect("diagnostic file");
        let child = command
            .stdout(Stdio::null())
            .stderr(Stdio::from(diagnostics.try_clone().expect("diagnostic handle")))
            .creation_flags(0x0800_0000) // Exercise headless roles without a console Ctrl-C path.
            .spawn()
            .expect("native CLI process");
        Self { child, diagnostics }
    }

    fn stderr(&mut self) -> String {
        self.diagnostics
            .seek(SeekFrom::Start(0))
            .expect("rewind diagnostics");
        let mut bytes = Vec::new();
        (&mut self.diagnostics)
            .take(64 * 1024)
            .read_to_end(&mut bytes)
            .expect("read diagnostics");
        String::from_utf8_lossy(&bytes).into_owned()
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
            if let Some(owner) = DaemonLock::read_role_metadata(lock)
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

    fn stop_activation(&mut self, paths: &StatePaths, owner: &RoleLockMetadata) {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            match locron_core::notification::request_shutdown(
                &paths.root,
                "daemon-activation",
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
                        "activation control unavailable: {error}"
                    );
                    std::thread::sleep(Duration::from_millis(20));
                }
            }
        }
        let status = self.wait_exit(deadline);
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
    let run_id = fixture.queue_marker_run();
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
