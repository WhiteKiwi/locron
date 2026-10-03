//! Native GUI image and actual console controls for the optional internal launcher.

#![cfg(windows)]

use std::ffi::OsStr;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom};
use std::os::windows::{fs::OpenOptionsExt, process::CommandExt};
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Deserializer};

#[path = "../src/windows_launcher.rs"]
mod windows_launcher;

const CREATE_NEW_CONSOLE: u32 = 0x10;
const DETACHED_PROCESS: u32 = 0x08;
const OUTPUT_LIMIT: u64 = 5 * 1024;
const FIXTURE_MODE: &str = "LOCRON_GUI_CONSOLE_FIXTURE";

struct OwnedProbe(Child);

impl OwnedProbe {
    fn wait_until(&mut self, deadline: Instant) -> io::Result<ExitStatus> {
        loop {
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "probe root did not exit",
                ));
            }
            if let Some(status) = self.0.try_wait()? {
                if Instant::now() >= deadline {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "late probe root exit",
                    ));
                }
                return Ok(status);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Drop for OwnedProbe {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(Some(_))) {
            return;
        }
        // Signal the retained exact child handle, never a looked-up PID. These compiled
        // read-only helpers have no child-dispatch path. Drop is containment, not success.
        let _ = self.0.kill();
        let _ = self.wait_until(Instant::now() + Duration::from_secs(3));
    }
}

fn capture(mut command: Command) -> io::Result<Output> {
    let entered = Instant::now();
    let deadline = entered + Duration::from_secs(30);
    let temporary = tempfile::tempdir()?;
    // Only public probe/test-harness facts are captured; the GUI itself never discovers state.
    let mut stdout = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(temporary.path().join("stdout"))?;
    let mut stderr = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(temporary.path().join("stderr"))?;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout.try_clone()?))
        .stderr(Stdio::from(stderr.try_clone()?));
    let mut child = OwnedProbe(command.spawn()?);
    let status = child.wait_until(deadline)?;
    drop(child);
    stdout.seek(SeekFrom::Start(0))?;
    stderr.seek(SeekFrom::Start(0))?;
    let mut out = Vec::new();
    let mut err = Vec::new();
    stdout.take(OUTPUT_LIMIT + 1).read_to_end(&mut out)?;
    stderr.take(OUTPUT_LIMIT + 1).read_to_end(&mut err)?;
    if out.len() as u64 > OUTPUT_LIMIT || err.len() as u64 > OUTPUT_LIMIT {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "oversized probe output",
        ));
    }
    Ok(Output {
        status,
        stdout: out,
        stderr: err,
    })
}

fn locked_image(path: &Path) -> File {
    // Cargo's exact fixture image is trusted source, not a managed-state root. Retain it
    // without write/delete sharing through header inspection and actual process execution.
    OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(path)
        .unwrap()
}

fn subsystem(image: &mut File) -> u16 {
    let mut dos = [0; 64];
    image.read_exact(&mut dos).unwrap();
    assert_eq!(&dos[..2], b"MZ");
    let offset = u32::from_le_bytes(dos[60..64].try_into().unwrap());
    assert!(u64::from(offset) + 94 <= image.metadata().unwrap().len());
    image.seek(SeekFrom::Start(u64::from(offset))).unwrap();
    let mut header = [0; 94];
    image.read_exact(&mut header).unwrap();
    assert_eq!(&header[..4], b"PE\0\0");
    let machine = u16::from_le_bytes(header[4..6].try_into().unwrap());
    let expected = match std::env::consts::ARCH {
        "x86_64" => 0x8664,
        "aarch64" => 0xaa64,
        architecture => panic!("unsupported qualification architecture: {architecture}"),
    };
    assert_eq!(machine, expected);
    assert_eq!(
        u16::from_le_bytes(header[24..26].try_into().unwrap()),
        0x20b
    );
    assert!(u16::from_le_bytes(header[20..22].try_into().unwrap()) >= 70);
    u16::from_le_bytes(header[92..94].try_into().unwrap())
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Probe {
    schema: String,
    version: String,
    target: String,
    launcher_abi: String,
    initial_conout_opened: bool,
    #[serde(deserialize_with = "required_console_error")]
    initial_conout_error: Option<i32>,
}

fn required_console_error<'de, D: Deserializer<'de>>(decoder: D) -> Result<Option<i32>, D::Error> {
    Option::<i32>::deserialize(decoder)
}

#[test]
fn probe_requires_all_six_fields_even_when_error_is_null() {
    let complete = serde_json::json!({
        "schema": "locron.windows-launcher-probe/v1",
        "version": env!("CARGO_PKG_VERSION"),
        "target": "x86_64-pc-windows-msvc",
        "launcher_abi": "native-gui-v1",
        "initial_conout_opened": true,
        "initial_conout_error": null,
    });
    assert!(serde_json::from_value::<Probe>(complete.clone()).is_ok());
    for field in [
        "schema",
        "version",
        "target",
        "launcher_abi",
        "initial_conout_opened",
        "initial_conout_error",
    ] {
        let mut missing = complete.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<Probe>(missing).is_err(), "{field}");
    }
}

fn decode(output: &[u8]) -> Probe {
    assert!(output.len() <= 4 * 1024);
    assert_eq!(output.first(), Some(&b'{'));
    let mut decoder = serde_json::Deserializer::from_slice(output).into_iter::<Probe>();
    let probe = decoder.next().unwrap().unwrap();
    assert_eq!(decoder.byte_offset(), output.len(), "trailing probe bytes");
    assert!(decoder.next().is_none());
    assert_eq!(probe.schema, "locron.windows-launcher-probe/v1");
    assert_eq!(probe.version, env!("CARGO_PKG_VERSION"));
    assert_eq!(probe.launcher_abi, "native-gui-v1");
    assert_eq!(
        probe.target,
        format!("{}-pc-windows-msvc", std::env::consts::ARCH)
    );
    match (probe.initial_conout_opened, probe.initial_conout_error) {
        (true, None) => {}
        (false, Some(error)) => assert_ne!(error, 0),
        facts => panic!("inconsistent raw console facts: {facts:?}"),
    }
    probe
}

fn launcher() -> Command {
    Command::new(env!("CARGO_BIN_EXE_locron-service-launcher"))
}

#[test]
fn native_console_fixture_child() {
    if std::env::var_os(FIXTURE_MODE).as_deref() == Some(OsStr::new("raw-conout")) {
        let entered = Instant::now();
        std::process::exit(windows_launcher::run(
            entered,
            vec!["--identity-probe".into()],
        ));
    }
}

fn console_control(flags: u32) -> Probe {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "native_console_fixture_child", "--nocapture"])
        .env(FIXTURE_MODE, "raw-conout")
        .creation_flags(flags);
    let output = capture(command).unwrap();
    assert!(output.status.success(), "{:?}", output);
    assert!(output.stderr.is_empty());
    // Only the independent Rust test harness has this fixed prelude; the real GUI output
    // below is decoded in full without trimming or accepting extra bytes.
    let start = output.stdout.iter().position(|byte| *byte == b'{').unwrap();
    let prelude = std::str::from_utf8(&output.stdout[..start]).unwrap();
    assert!(prelude.contains("running 1 test"));
    decode(&output.stdout[start..])
}

#[test]
fn native_gui_and_actual_console_controls_preserve_raw_first_entry_facts() {
    let mut gui_image = locked_image(Path::new(env!("CARGO_BIN_EXE_locron-service-launcher")));
    let mut console_image = locked_image(Path::new(env!("CARGO_BIN_EXE_locron")));
    let mut helper_image = locked_image(&std::env::current_exe().unwrap());
    assert_eq!(subsystem(&mut gui_image), 2);
    assert_eq!(subsystem(&mut console_image), 3);
    assert_eq!(subsystem(&mut helper_image), 3);
    let mut command = launcher();
    command.arg("--identity-probe");
    let output = capture(command).unwrap();
    assert!(output.status.success(), "{:?}", output);
    assert!(output.stderr.is_empty());
    let gui = decode(&output.stdout);
    let detached = console_control(DETACHED_PROCESS);
    let attached = console_control(CREATE_NEW_CONSOLE);
    assert!(!gui.initial_conout_opened);
    assert!(!detached.initial_conout_opened);
    assert_eq!(gui.initial_conout_error, detached.initial_conout_error);
    assert!(!matches!(gui.initial_conout_error, Some(3 | 5 | 32)));
    assert!(attached.initial_conout_opened);
    assert_eq!(attached.initial_conout_error, None);
}

#[test]
fn native_version_and_secret_safe_refusals_leave_missing_state_absent() {
    let temporary = tempfile::tempdir().unwrap();
    let missing = temporary.path().join("missing private 状態");
    let mut command = launcher();
    command.arg("--version").env("LOCRON_STATE_DIR", &missing);
    let output = capture(command).unwrap();
    assert!(output.status.success());
    assert_eq!(
        output.stdout,
        format!("locron-service-launcher {}\n", env!("CARGO_PKG_VERSION")).as_bytes()
    );
    assert!(output.stderr.is_empty());
    for extra in ["SECRET-CAPABILITY", "--version"] {
        let mut command = launcher();
        command
            .args(["--identity-probe", extra])
            .env("LOCRON_STATE_DIR", &missing);
        let output = capture(command).unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());
    }
    let mut command = launcher();
    command.arg("--state-dir").arg(&missing).args([
        "--role",
        "daemon",
        "--run-capability",
        "v1:SECRET-CAPABILITY",
    ]);
    let output = capture(command).unwrap();
    assert_eq!(output.status.code(), Some(70));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
    assert!(!missing.exists());
}
