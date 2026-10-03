//! Isolated real-kernel proof of stock adapters after their owning parent exits.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::json;

use super::loader_tests::{FixtureChild, bounded_text};
use super::{ADAPTER_TIMEOUT, ADAPTER_WORKERS, prepare_adapter, run_adapter_worker};

const HOST: &str = include_str!("loader_crash_host.ps1");
const OBSERVER: &str = include_str!("loader_crash_observer.ps1");

struct Helper {
    child: FixtureChild,
    stdout: PathBuf,
    stderr: PathBuf,
}

impl Helper {
    fn spawn(mode: &str, directory: &Path) -> Self {
        let stdout = directory.join(format!("{mode}.stdout"));
        let stderr = directory.join(format!("{mode}.stderr"));
        let child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "windows::loader_tests::owned_loader_fixture_child",
                "--nocapture",
            ])
            .env("LOCRON_STOCK_LOADER_FIXTURE", mode)
            .env("LOCRON_LOADER_CRASH_DIRECTORY", directory)
            .current_dir(directory)
            .creation_flags(0x0800_0000)
            .stdin(Stdio::null())
            .stdout(File::create(&stdout).unwrap())
            .stderr(File::create(&stderr).unwrap())
            .spawn()
            .unwrap();
        Self {
            child: FixtureChild(child),
            stdout,
            stderr,
        }
    }

    fn live(&mut self) {
        let status = self.child.0.try_wait().unwrap();
        assert!(
            status.is_none(),
            "helper exited {status:?}: stdout={} stderr={}",
            bounded_text(&self.stdout),
            bounded_text(&self.stderr)
        );
    }

    fn exit_until(&mut self, deadline: Instant) -> ExitStatus {
        loop {
            super::remaining(deadline).unwrap_or_else(|error| {
                panic!(
                    "{error}: stdout={} stderr={}",
                    bounded_text(&self.stdout),
                    bounded_text(&self.stderr)
                )
            });
            if let Some(status) = self.child.0.try_wait().unwrap() {
                super::remaining(deadline).unwrap();
                return status;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

fn directory() -> PathBuf {
    std::env::var_os("LOCRON_LOADER_CRASH_DIRECTORY")
        .map(PathBuf::from)
        .unwrap()
}

fn publish(directory: &Path, name: &str, bytes: &[u8]) {
    let pending = directory.join(format!("{name}.pending"));
    fs::write(&pending, bytes).unwrap();
    fs::rename(pending, directory.join(name)).unwrap();
}

fn wait_artifact(path: &Path, host: Option<&mut Helper>, observer: &mut Helper, deadline: Instant) {
    let mut host = host;
    while !path.is_file() {
        super::remaining(deadline).unwrap();
        if let Some(host) = host.as_mut() {
            host.live();
        }
        observer.live();
        std::thread::sleep(Duration::from_millis(5));
    }
    super::remaining(deadline).unwrap();
}

fn beats(directory: &Path, deadline: Instant) -> (u64, u64) {
    let read = |name: &str| loop {
        super::remaining(deadline).unwrap();
        match fs::read_to_string(directory.join(name)) {
            Ok(value) => {
                if let Ok(value) = value.parse::<u64>() {
                    break value;
                }
            }
            Err(error) => assert_eq!(error.kind(), io::ErrorKind::NotFound),
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    (read("generic-heartbeat"), read("native-heartbeat"))
}

fn stock_is_held() {
    let error = OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(super::stock_powershell().unwrap())
        .unwrap_err();
    assert_eq!(error.raw_os_error(), Some(32));
}

pub(super) fn driver() {
    // This fresh orchestrator deliberately performs no SID, stock guard or adapter request.
    let temporary = tempfile::tempdir().unwrap();
    let directory = fs::canonicalize(temporary.path()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(45);
    let mut host = Helper::spawn("parent-exit-host", &directory);
    let mut observer = Helper::spawn("parent-exit-observer", &directory);
    wait_artifact(
        &directory.join("handles-bound"),
        Some(&mut host),
        &mut observer,
        deadline,
    );
    assert_eq!(
        bounded_text(&directory.join("handles-bound")),
        "three-live-handles"
    );
    let before = beats(&directory, deadline);
    std::thread::sleep(Duration::from_millis(200));
    let live = beats(&directory, deadline);
    assert!(live.0 > before.0 && live.1 > before.1);
    host.live();
    observer.live();
    stock_is_held();
    // Terminate only this exact retained fixture parent. Its kernel Job handles close abruptly.
    host.child.0.kill().unwrap();
    let status = host.exit_until(deadline);
    assert!(!status.success());
    wait_artifact(
        &directory.join("exits-confirmed"),
        None,
        &mut observer,
        deadline,
    );
    assert_eq!(
        bounded_text(&directory.join("exits-confirmed")),
        "three-associated-exits"
    );
    observer.live();
    stock_is_held();
    let stopped = beats(&directory, deadline);
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(beats(&directory, deadline), stopped);
    publish(&directory, "observer-release", b"release");
    let status = observer.exit_until(deadline);
    assert!(
        status.success(),
        "stdout={} stderr={}",
        bounded_text(&observer.stdout),
        bounded_text(&observer.stderr)
    );
    assert!(bounded_text(&observer.stdout).contains("observer-root-tree-pipes-confirmed"));
    // Both helpers have actually exited; the observer already confirmed all three target handles.
    let released = OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(super::stock_powershell().unwrap())
        .unwrap();
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(beats(&directory, deadline), stopped);
    drop(released);
    super::remaining(deadline).unwrap();
    println!("parent-exit-targets-handles-heartbeats-guards-confirmed");
}

pub(super) fn host() {
    let directory = directory();
    let fixed_directory = directory.clone();
    let fixed = std::thread::spawn(move || {
        let sid = super::current_user_sid().unwrap();
        assert!(sid.starts_with("S-1-"));
        let pid = super::filesystem_worker::observed_pid();
        assert!(pid > 0);
        publish(&fixed_directory, "fixed-pid", pid.to_string().as_bytes());
    });
    let deadline = Instant::now() + ADAPTER_TIMEOUT;
    let request = prepare_adapter(
        HOST,
        &json!({"directory":directory,"executable":std::env::current_exe().unwrap()}),
    )
    .unwrap();
    let trace = Arc::clone(&request.trace);
    let generic = std::thread::spawn(move || {
        let permit = ADAPTER_WORKERS.acquire(deadline).unwrap();
        run_adapter_worker(request, deadline, permit)
    });
    let pid = loop {
        super::remaining(deadline).unwrap();
        if let Some(pid) = trace.stage_value("spawn-complete") {
            assert!(pid > 0);
            break pid;
        }
        assert!(
            !generic.is_finished(),
            "generic adapter exited before actual spawn"
        );
        std::thread::sleep(Duration::from_millis(5));
    };
    publish(&directory, "generic-spawn-pid", pid.to_string().as_bytes());
    fixed.join().unwrap();
    let result = generic.join().unwrap();
    panic!(
        "crash host unexpectedly survived: result={result:?}, spawn={:?}",
        trace.stage_value("spawn-complete")
    );
}

pub(super) fn observer() {
    let request = prepare_adapter(OBSERVER, &json!({"directory":directory()})).unwrap();
    let trace = Arc::clone(&request.trace);
    let deadline = Instant::now() + ADAPTER_TIMEOUT;
    let permit = ADAPTER_WORKERS.acquire(deadline).unwrap();
    let result = run_adapter_worker(request, deadline, permit).unwrap();
    assert_eq!(result["exits"], 3);
    assert!(trace.has_stage("root-completed"));
    assert!(
        trace
            .stage_value("spawn-complete")
            .is_some_and(|pid| pid > 0)
    );
    println!("observer-root-tree-pipes-confirmed");
}

pub(super) fn heartbeat() {
    let path = directory().join("native-heartbeat");
    let deadline = Instant::now() + ADAPTER_TIMEOUT;
    let mut count = 0_u64;
    while Instant::now() < deadline {
        count += 1;
        fs::write(&path, count.to_string()).unwrap();
        std::thread::sleep(Duration::from_millis(25));
    }
    panic!("native heartbeat descendant was not contained by parent Job closure");
}
