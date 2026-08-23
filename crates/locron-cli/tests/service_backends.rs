//! Real-backend tests for `locron service install|uninstall|status`.
//!
//! These tests drive the actual service manager of the running platform and
//! therefore register and unregister a real service on the machine that runs
//! them. Every test first detects whether the environment can host the leg it
//! needs and reports exactly what it could and could not run when it skips:
//!
//! - macOS: launchd in the `gui/<uid>` domain with a `user/<uid>` fallback.
//! - Linux: a real systemd user manager, started under `dbus-run-session`
//!   when no user session exists yet.
//!
//! The registration always uses the default state directory (the plist/unit
//! template is frozen), so the tests refuse to run when a manual daemon
//! already holds that state lock or when the service is already registered.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use assert_cmd::cargo::cargo_bin;
use serde_json::Value;

/// Serializes this suite's tests: they share the real service manager.
fn serialized() -> std::sync::MutexGuard<'static, ()> {
    static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());
    SERIAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn locron() -> PathBuf {
    cargo_bin("locron")
}

fn run_json(binary: &Path, args: &[&str]) -> (i32, Value) {
    let output = Command::new(binary)
        .args(args)
        .arg("--json")
        .output()
        .expect("locron must run");
    let value: Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|_| panic!("locron.cli/v1 envelope on stdout for {args:?}"));
    (output.status.code().unwrap_or(-1), value)
}

fn home() -> PathBuf {
    PathBuf::from(
        std::env::var_os("HOME").unwrap_or_else(|| panic!("HOME must be set for the real tests")),
    )
}

fn uid() -> String {
    let output = Command::new("id").arg("-u").output().expect("id -u");
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

#[cfg(target_os = "macos")]
fn launchctl_ok(args: &[&str]) -> bool {
    Command::new("launchctl")
        .args(args)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// The default-state daemon lock, so a manual daemon's ownership is respected.
fn default_daemon_lock_held() -> bool {
    let state_dir = home().join("Library/Application Support/locron");
    let lock_path = state_dir.join("daemon.lock");
    if !lock_path.exists() {
        return false;
    }
    let Ok(file) = fs::OpenOptions::new().read(true).open(&lock_path) else {
        return false;
    };
    file.try_lock().is_err()
}

/// Read the daemon lock diagnostic (pid and lifetime id) of the running daemon.
#[cfg(target_os = "macos")]
fn daemon_lock_pid() -> Option<u32> {
    let state_dir = home().join("Library/Application Support/locron");
    let text = fs::read_to_string(state_dir.join("daemon.lock")).ok()?;
    let value: Value = serde_json::from_str(text.lines().next()?).ok()?;
    value.get("pid")?.as_u64().map(|pid| pid as u32)
}

/// Poll a condition with a deadline, sleeping between attempts.
#[cfg(target_os = "macos")]
fn wait_until(mut condition: impl FnMut() -> bool, timeout: Duration, what: &str) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if condition() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    eprintln!("timed out waiting for: {what}");
    false
}

/// Best-effort uninstall on drop, so a failing test never leaves a real
/// service registered behind.
struct ServiceCleanup {
    binary: PathBuf,
}

impl Drop for ServiceCleanup {
    fn drop(&mut self) {
        let _ = Command::new(&self.binary)
            .arg("service")
            .arg("uninstall")
            .output();
        let _ = Command::new("launchctl")
            .args(["bootout", &format!("gui/{}/dev.locron.daemon", uid())])
            .output();
    }
}

// ---------------------------------------------------------------------------
// macOS: real launchd backend
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
#[test]
fn macos_launchd_backend_registers_restarts_and_unregisters() {
    let _serial = serialized();
    let binary = locron();
    let domain = format!("gui/{}", uid());
    let target = format!("{domain}/dev.locron.daemon");

    // Detect what this environment can run.
    if Command::new("launchctl").arg("print").output().is_err() {
        eprintln!("SKIPPED: launchctl is unavailable in this environment");
        return;
    }
    if !launchctl_ok(&["print", &domain]) && !launchctl_ok(&["print", &format!("user/{}", uid())]) {
        eprintln!(
            "SKIPPED: no launchd user domain is reachable here (gui/{} and user/{} both failed)",
            uid(),
            uid()
        );
        return;
    }
    if launchctl_ok(&["print", &target]) {
        eprintln!("SKIPPED: dev.locron.daemon is already registered in this environment");
        return;
    }
    if default_daemon_lock_held() {
        eprintln!("SKIPPED: a manual locron daemon holds the default-state lock");
        return;
    }

    let _cleanup = ServiceCleanup {
        binary: binary.clone(),
    };

    // Install: plist written, service bootstrapped and running.
    let (code, data) = run_json(&binary, &["service", "install"]);
    assert_eq!(code, 0, "service install must succeed");
    assert_eq!(data["data"]["registered"], true);
    let plist = home().join("Library/LaunchAgents/dev.locron.daemon.plist");
    assert!(
        plist.exists(),
        "the plist must be written to ~/Library/LaunchAgents"
    );
    assert!(
        home().join("Library/Logs/locron").is_dir(),
        "the log directory must be created"
    );

    let (code, status) = run_json(&binary, &["service", "status"]);
    assert_eq!(code, 0);
    assert_eq!(status["data"]["registered"], true);
    assert!(
        status["data"]["domain"].is_string(),
        "status reports the loaded domain"
    );

    // The service may legitimately defer its start when a manual daemon
    // appeared in the meantime; report and skip the restart observation then.
    if data["data"]["deferred"] == true {
        eprintln!(
            "SKIPPED restart observation: a manual daemon holds the state lock; \
             registration is complete and the service will start when it exits"
        );
        let (code, data) = run_json(&binary, &["service", "uninstall"]);
        assert_eq!(code, 0);
        assert_eq!(data["data"]["removed"], true);
        assert!(!plist.exists(), "uninstall removes the plist");
        return;
    }
    assert_eq!(status["data"]["loaded"], true, "the daemon must be running");
    // The daemon starts asynchronously after bootstrap, so the lock ownership
    // appears a moment later; poll for it.
    let first_pid = wait_until(
        || daemon_lock_pid().is_some_and(|pid| pid > 0),
        Duration::from_secs(30),
        "the daemon to acquire the state lock",
    )
    .then(|| daemon_lock_pid().unwrap())
    .expect("the daemon must hold the state lock");
    assert!(first_pid > 0);

    // Refresh: a repeat install restarts the running daemon onto the current
    // binary; launchd's KeepAlive relaunches it with a fresh process and a
    // fresh lock lifetime (the marker-process observation).
    let (code, data) = run_json(&binary, &["service", "install"]);
    assert_eq!(code, 0);
    assert_eq!(
        data["data"]["restarted"], true,
        "repeat install restarts the service"
    );
    assert!(
        wait_until(
            || {
                daemon_lock_pid().is_some_and(|pid| pid != first_pid)
                    && !std::process::Command::new("kill")
                        .arg("-0")
                        .arg(first_pid.to_string())
                        .stderr(std::process::Stdio::null())
                        .status()
                        .map(|status| status.success())
                        .unwrap_or(false)
            },
            Duration::from_secs(60),
            "the old daemon pid to exit and a new pid to take the state lock",
        ),
        "SIGTERM + KeepAlive must relaunch the daemon under a new pid"
    );

    // Uninstall: signal first, wait for the job to leave the domain, then
    // bootout and remove the plist.
    let (code, data) = run_json(&binary, &["service", "uninstall"]);
    assert_eq!(code, 0);
    assert_eq!(data["data"]["stopped"], true);
    assert_eq!(data["data"]["removed"], true);
    assert!(
        wait_until(
            || !launchctl_ok(&["print", &target]),
            Duration::from_secs(30),
            "the job to leave the launchd domain",
        ),
        "uninstall must unload the job from launchd"
    );
    assert!(!plist.exists(), "uninstall removes the plist");
    let (code, status) = run_json(&binary, &["service", "status"]);
    assert_eq!(code, 0);
    assert_eq!(status["data"]["registered"], false);
    assert_eq!(status["data"]["loaded"], false);
}

#[cfg(not(target_os = "macos"))]
#[test]
fn macos_launchd_backend_is_not_run_outside_macos() {
    eprintln!("SKIPPED: the real launchd backend runs on the macOS CI leg only");
}

// ---------------------------------------------------------------------------
// Linux: real systemd user manager (direct session or dbus-run-session)
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
fn has_on_path(binary: &str) -> bool {
    Command::new(binary).arg("--version").output().is_ok()
        || Command::new(binary).arg("--help").output().is_ok()
}

#[cfg(target_os = "linux")]
#[test]
fn linux_systemd_backend_registers_restarts_and_unregisters() {
    let _serial = serialized();
    let binary = locron();
    let unit_path = home().join(".config/systemd/user/locron.service");

    // A direct user manager: reachable bus in the current session.
    let direct_session = Command::new("systemctl")
        .args(["--user", "show-environment"])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);

    if !direct_session {
        if !has_on_path("dbus-run-session") || !has_on_path("systemd") {
            eprintln!(
                "SKIPPED: no systemd user session and no dbus-run-session/systemd to \
                 start one; the Linux leg runs on the Linux CI leg"
            );
            return;
        }
        // Start a private user manager under dbus-run-session and run the
        // whole service flow inside it, capturing the JSON envelopes.
        let runtime_dir =
            std::env::temp_dir().join(format!("locron-runtime-{}", std::process::id()));
        fs::create_dir_all(&runtime_dir).unwrap();
        let script = format!(
            r#"
set -eu
export XDG_RUNTIME_DIR="{runtime_dir}"
systemd --user >/dev/null 2>&1 &
for _ in $(seq 1 50); do
  if systemctl --user show-environment >/dev/null 2>&1; then break; fi
  sleep 0.2
done
"{binary}" service install --json > install.json
"{binary}" service status --json > status.json
"{binary}" service install --json > refresh.json
"{binary}" service uninstall --json > uninstall.json
"#,
            runtime_dir = runtime_dir.display(),
            binary = binary.display(),
        );
        let output = Command::new("dbus-run-session")
            .args(["--", "sh", "-c", &script])
            .output()
            .expect("dbus-run-session must run");
        assert!(
            output.status.success(),
            "the flow must exit zero: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let read = |name: &str| -> Value {
            let text = fs::read_to_string(name).unwrap_or_else(|_| panic!("{name} must exist"));
            serde_json::from_str(&text).unwrap_or_else(|_| panic!("{name} must be JSON"))
        };
        let install = read("install.json");
        assert_eq!(install["data"]["registered"], true);
        let status = read("status.json");
        assert_eq!(status["data"]["loaded"], true, "the unit must be active");
        let refresh = read("refresh.json");
        assert_eq!(
            refresh["data"]["restarted"], true,
            "repeat install restarts the unit"
        );
        let uninstall = read("uninstall.json");
        assert_eq!(uninstall["data"]["removed"], true);
        assert_eq!(uninstall["data"]["stopped"], true);
        let _ = fs::remove_dir_all(&runtime_dir);
        return;
    }

    // Direct user manager: run the flow in the current session.
    if std::fs::metadata(&unit_path).is_ok() {
        eprintln!("SKIPPED: locron.service is already registered in this environment");
        return;
    }
    if default_daemon_lock_held() {
        eprintln!("SKIPPED: a manual locron daemon holds the default-state lock");
        return;
    }
    let _cleanup = ServiceCleanup {
        binary: binary.clone(),
    };
    let (code, data) = run_json(&binary, &["service", "install"]);
    assert_eq!(code, 0);
    assert_eq!(data["data"]["registered"], true);
    assert!(unit_path.exists(), "the unit must be written");
    let (code, status) = run_json(&binary, &["service", "status"]);
    assert_eq!(code, 0);
    assert_eq!(status["data"]["loaded"], true);
    let (code, data) = run_json(&binary, &["service", "install"]);
    assert_eq!(code, 0);
    assert_eq!(data["data"]["restarted"], true);
    let (code, data) = run_json(&binary, &["service", "uninstall"]);
    assert_eq!(code, 0);
    assert_eq!(data["data"]["stopped"], true);
    assert_eq!(data["data"]["removed"], true);
    assert!(!unit_path.exists(), "uninstall removes the unit");
}

#[cfg(target_os = "linux")]
#[test]
fn no_session_environment_prints_guidance_and_exits_zero() {
    let _serial = serialized();
    // Stripped environment: no XDG_RUNTIME_DIR, so the user manager cannot be
    // reached even when systemctl exists.
    let output = Command::new(locron())
        .env_remove("XDG_RUNTIME_DIR")
        .args(["service", "install", "--json"])
        .output()
        .expect("locron must run");
    assert!(output.status.success(), "no-session install must exit zero");
    let value: Value = serde_json::from_slice(&output.stdout).expect("envelope on stdout");
    assert_eq!(value["data"]["registered"], false);
    assert!(
        value["data"]["guidance"]
            .as_str()
            .is_some_and(|text| text.contains("locron service install")),
        "guidance must point at service install"
    );
}

#[cfg(not(target_os = "linux"))]
#[test]
fn linux_systemd_leg_reports_what_cannot_run_here() {
    eprintln!(
        "SKIPPED: this is {}/{}; systemctl, dbus-run-session, and systemd are not \
         available, so the real systemd backend and the stripped-environment \
         guidance test run on the Linux CI leg only",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
}
