//! Isolated native ownership probes; these never warm the original cold state gate.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use serde_json::json;

use super::{ADAPTER_TIMEOUT, ADAPTER_WORKERS, prepare_adapter, run_adapter_worker};

pub(super) const POLICY_ASSERTION: &str = include_str!("loader_policy.ps1");

pub(super) fn restricted_child() -> bool {
    std::env::var("LOCRON_STOCK_LOADER_FIXTURE").is_ok_and(|mode| mode == "restricted-helper")
}

pub(super) struct FixtureChild(pub(super) Child);

impl Drop for FixtureChild {
    fn drop(&mut self) {
        if self.0.try_wait().is_ok_and(|status| status.is_some()) {
            return;
        }
        let _ = self.0.kill();
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if self.0.try_wait().is_ok_and(|status| status.is_some()) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

pub(super) fn bounded_text(path: &Path) -> String {
    let mut value = String::new();
    File::open(path)
        .unwrap()
        .take(32 * 1024)
        .read_to_string(&mut value)
        .unwrap();
    value
}

fn isolated(mode: &str, confirmation: &str) {
    let temporary = tempfile::tempdir().unwrap();
    let forged = temporary.path().join("forged modules");
    let module = forged.join("Microsoft.PowerShell.Utility");
    fs::create_dir_all(&module).unwrap();
    fs::write(
        module.join("Microsoft.PowerShell.Utility.psm1"),
        "[IO.File]::WriteAllText([IO.Path]::Combine([Environment]::CurrentDirectory, 'forged-module-ran'), 'loaded'); throw 'forged module loaded'",
    )
    .unwrap();
    fs::write(
        module.join("Microsoft.PowerShell.Utility.psd1"),
        "@{RootModule='Microsoft.PowerShell.Utility.psm1';ModuleVersion='1.0.0'}",
    )
    .unwrap();
    let stdout = temporary.path().join("stdout");
    let stderr = temporary.path().join("stderr");
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "windows::loader_tests::owned_loader_fixture_child",
            "--nocapture",
        ])
        .env("LOCRON_STOCK_LOADER_FIXTURE", mode)
        .env("PSModulePath", &forged)
        .current_dir(temporary.path())
        .creation_flags(0x0800_0000)
        .stdin(Stdio::null())
        .stdout(File::create(&stdout).unwrap())
        .stderr(File::create(&stderr).unwrap());
    if mode == "restricted-helper" {
        command.env("PSExecutionPolicyPreference", "Restricted");
    }
    let mut child = FixtureChild(command.spawn().unwrap());
    let deadline = Instant::now() + Duration::from_secs(45);
    let status = loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "isolated loader fixture timed out"
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(
        status.success(),
        "stdout={}\nstderr={}",
        bounded_text(&stdout),
        bounded_text(&stderr)
    );
    assert!(bounded_text(&stdout).contains(confirmation));
    assert!(!temporary.path().join("forged-module-ran").exists());
}

#[test]
fn native_guard_stall_keeps_ownership_after_bounded_driver_returns() {
    isolated("guard-stall", "native-guard-stall-confirmed");
}

#[test]
fn forged_module_path_and_cwd_cannot_replace_retained_json_commands() {
    isolated("forged", "forged-module-refusal-confirmed");
}

#[test]
fn owned_loader_fixture_child() {
    let Ok(mode) = std::env::var("LOCRON_STOCK_LOADER_FIXTURE") else {
        return;
    };
    match mode.as_str() {
        "restricted-driver" => isolated("restricted-helper", "restricted-child-policies-confirmed"),
        "restricted-helper" => restricted_policy(),
        "parent-exit-driver" => super::loader_crash::driver(),
        "parent-exit-host" => super::loader_crash::host(),
        "parent-exit-observer" => super::loader_crash::observer(),
        "native-heartbeat" => super::loader_crash::heartbeat(),
        "guard-stall" => guard_stall(),
        "forged" => {
            let value = super::run_script_json(
                "@{echo=[string]$request.echo} | & $locronToJson -Compress",
                &json!({"echo":"한국 日本語 % #"}),
            )
            .unwrap();
            assert_eq!(value["echo"], "한국 日本語 % #");
            assert!(!Path::new("forged-module-ran").exists());
            println!("forged-module-refusal-confirmed");
        }
        _ => panic!("unknown owned loader fixture"),
    }
}

fn restricted_policy() {
    assert_eq!(
        std::env::var("PSExecutionPolicyPreference").unwrap(),
        "Restricted"
    );
    let start = Instant::now();
    let deadline = start + ADAPTER_TIMEOUT;
    let temporary = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temporary.path())
        .unwrap()
        .join("private 日本語 % #");
    let fixed = std::thread::spawn(move || {
        let sid = super::current_user_sid().unwrap();
        super::remaining(deadline).unwrap();
        let guard = crate::filesystem::DirectoryGuard::private(&root).unwrap();
        let path = guard.normalized_path().join("secret 日本語 % #");
        let file = crate::filesystem::create_private_new(&path).unwrap();
        assert!(crate::filesystem::is_private(guard.normalized_path(), true).unwrap());
        assert!(crate::filesystem::is_private(&path, false).unwrap());
        assert!(sid.starts_with("S-1-"));
        let observation = loop {
            let observation = super::filesystem_worker::policy_observation();
            if observation.1 {
                break observation;
            }
            super::remaining(deadline).unwrap();
            std::thread::sleep(Duration::from_millis(5));
        };
        drop((file, guard));
        super::remaining(deadline).unwrap();
        observation
    });
    let request = prepare_adapter(
        "$scheduler = [Activator]::CreateInstance([Type]::GetTypeFromProgID('Schedule.Service', $true)); $scheduler.Connect(); $tasks = $scheduler.GetFolder('\\').GetTasks(0); @{echo=[string]$request.echo; count=[int]$tasks.Count} | & $locronToJson -Compress",
        &json!({"echo":"한국 日本語 % #"}),
    ).unwrap();
    let trace = Arc::clone(&request.trace);
    let permit = ADAPTER_WORKERS.acquire(deadline).unwrap();
    let result = run_adapter_worker(request, deadline, permit);
    // Always collect the separately owned fixed caller before asserting the generic outcome.
    let (fixed_pid, fixed_policy) = fixed.join().unwrap();
    let result = result.unwrap();
    assert_eq!(result["echo"], "한국 日本語 % #");
    assert!(result["count"].as_u64().is_some());
    assert!(trace.has_stage("policy-confirmed"));
    let generic_pid = trace.stage_value("spawn-complete").unwrap();
    assert!(fixed_policy && fixed_pid > 0 && generic_pid > 0 && fixed_pid != generic_pid);
    assert!(!Path::new("forged-module-ran").exists());
    assert!(start.elapsed() < ADAPTER_TIMEOUT);
    println!("restricted-child-policies-confirmed generic_pid={generic_pid} fixed_pid={fixed_pid}");
}

fn guard_stall() {
    assert_eq!(*ADAPTER_WORKERS.active.lock().unwrap(), 0);
    let (reader, mut release) = std::io::pipe().unwrap();
    let (entered, entry) = mpsc::sync_channel(1);
    let marker = std::env::current_dir().unwrap().join("late-spawn-marker");
    let mut request = prepare_adapter(
        "[IO.File]::WriteAllText([string]$request.marker, 'unexpected'); @{} | & $locronToJson -Compress",
        &json!({"marker": marker}),
    ).unwrap();
    request.guard_stall = Some(super::stock::GuardStall { reader, entered });
    let trace = Arc::clone(&request.trace);
    let start = Instant::now();
    let deadline = start + ADAPTER_TIMEOUT;
    let permit = ADAPTER_WORKERS.acquire(deadline).unwrap();
    let (reply, receiver) = mpsc::sync_channel(1);
    let driver = std::thread::spawn(move || {
        let result = run_adapter_worker(request, deadline, permit);
        let _ = reply.send(result);
    });
    entry
        .recv_timeout(ADAPTER_TIMEOUT)
        .expect("actual native guard read must enter");
    let error = receiver
        .recv_timeout((deadline + Duration::from_secs(4)).saturating_duration_since(Instant::now()))
        .expect("driver must return without joining the blocked native owner")
        .unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
    assert!(start.elapsed() < ADAPTER_TIMEOUT + Duration::from_secs(4));
    assert_eq!(*ADAPTER_WORKERS.active.lock().unwrap(), 1);
    // A read-only incompatible open proves the real stock leaf remains held; no stock writes.
    let blocked = OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(super::stock_powershell().unwrap())
        .unwrap_err();
    assert_eq!(blocked.raw_os_error(), Some(32));
    assert_eq!(
        ADAPTER_WORKERS
            .acquire(Instant::now() + Duration::from_millis(40))
            .err()
            .unwrap()
            .kind(),
        std::io::ErrorKind::TimedOut
    );
    release.write_all(&[1]).unwrap();
    drop(release);
    let released = Instant::now() + Duration::from_secs(3);
    while *ADAPTER_WORKERS.active.lock().unwrap() != 0 {
        assert!(
            Instant::now() < released,
            "late guard must refuse all further work"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!trace.has_stage("spawn-complete"));
    assert!(!trace.has_stage("input-written"));
    assert!(trace.child_phases().is_empty());
    assert!(!marker.exists());
    if driver.is_finished() {
        driver.join().unwrap();
    }
    println!("native-guard-stall-confirmed");
}
