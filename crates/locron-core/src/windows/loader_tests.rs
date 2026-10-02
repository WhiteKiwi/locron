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

struct FixtureChild(Child);

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

fn bounded_text(path: &Path) -> String {
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
    let mut child = FixtureChild(
        Command::new(std::env::current_exe().unwrap())
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
            .stderr(File::create(&stderr).unwrap())
            .spawn()
            .unwrap(),
    );
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
