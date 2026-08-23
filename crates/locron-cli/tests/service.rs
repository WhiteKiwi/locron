//! Contract tests for `locron service install|uninstall|status` against the
//! deterministic fake service manager (`LOCRON_SERVICE_BACKEND=fake`).
//!
//! The fake never touches a real service manager: `LOCRON_SERVICE_FAKE_STATE`
//! seeds the manager state and `LOCRON_SERVICE_FAKE_LOG` records every port
//! call in order, so the tests assert the exact flow the real backends share.

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use serde_json::Value;

/// Serializes the suite's tests: every test forks real child processes, and
/// cargo runs tests in parallel.
fn serialized() -> std::sync::MutexGuard<'static, ()> {
    static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());
    SERIAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// A command running the real binary against the fake backend.
fn fake_command(state: &Path, log: &Path) -> Command {
    let mut command = Command::cargo_bin("locron").unwrap();
    command
        .env("LOCRON_SERVICE_BACKEND", "fake")
        .env("LOCRON_SERVICE_FAKE_STATE", state)
        .env("LOCRON_SERVICE_FAKE_LOG", log);
    command
}

/// Seed the fake manager state and return its state and log file paths.
fn seeded(dir: &Path, body: &str) -> (PathBuf, PathBuf) {
    let state = dir.join("state.json");
    let log = dir.join("calls.log");
    fs::write(&state, body).unwrap();
    (state, log)
}

fn envelope(output: &std::process::Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("locron.cli/v1 envelope on stdout")
}

fn calls(log: &Path) -> Vec<String> {
    fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

/// The name the service carries in the current platform's manager.
fn native_service_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "dev.locron.daemon"
    } else {
        "locron.service"
    }
}

/// The name the dashboard service carries in the current platform's manager.
fn native_dashboard_service_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "dev.locron.dashboard"
    } else {
        "locron-dashboard.service"
    }
}

/// A command running the real binary against the fake backend with a
/// dedicated state directory (the dashboard flows write the access token).
fn fake_dashboard_command(state: &Path, log: &Path, state_dir: &Path) -> Command {
    let mut command = fake_command(state, log);
    command.arg("--state-dir").arg(state_dir).arg("dashboard");
    command
}

#[test]
fn install_registers_and_starts_in_write_reload_probe_enable_start_order() {
    let _serial = serialized();
    let tmp = tempfile::tempdir().unwrap();
    let (state, log) = seeded(
        tmp.path(),
        "{\"session\":true,\"loaded\":false,\"enabled\":false,\"registered\":false}",
    );
    let output = fake_command(&state, &log)
        .arg("service")
        .arg("install")
        .arg("--json")
        .output()
        .unwrap();
    assert!(output.status.success());
    let data = &envelope(&output)["data"];
    assert_eq!(data["registered"], true);
    assert_eq!(data["restarted"], false);
    assert_eq!(data["deferred"], false);
    assert_eq!(data["service_name"], native_service_name());
    assert_eq!(data["domain"], "fake/domain");
    assert_eq!(
        calls(&log),
        [
            "session_available",
            "write_registration",
            "reload",
            "is_loaded",
            "enable",
            "start",
        ]
    );
}

#[test]
fn install_refreshes_and_restarts_a_loaded_service() {
    let _serial = serialized();
    let tmp = tempfile::tempdir().unwrap();
    let (state, log) = seeded(
        tmp.path(),
        "{\"session\":true,\"loaded\":true,\"enabled\":true,\"registered\":true}",
    );
    let output = fake_command(&state, &log)
        .arg("service")
        .arg("install")
        .arg("--json")
        .output()
        .unwrap();
    assert!(output.status.success());
    let data = &envelope(&output)["data"];
    assert_eq!(data["registered"], true);
    assert_eq!(data["restarted"], true);
    assert_eq!(data["deferred"], false);
    assert_eq!(
        calls(&log),
        [
            "session_available",
            "write_registration",
            "reload",
            "is_loaded",
            "restart",
            "status",
        ]
    );
}

#[test]
fn install_defers_the_start_when_a_manual_daemon_holds_the_state_lock() {
    let _serial = serialized();
    let tmp = tempfile::tempdir().unwrap();
    let state_dir = tmp.path().join("state");
    fs::create_dir_all(&state_dir).unwrap();
    // Hold the state lock from this process, exactly as a manual
    // `locron daemon run` would.
    let lock_path = state_dir.join("daemon.lock");
    let lock_file = fs::File::create(&lock_path).unwrap();
    lock_file.lock().unwrap();
    let (state, log) = seeded(
        tmp.path(),
        "{\"session\":true,\"loaded\":false,\"enabled\":false,\"registered\":false}",
    );
    let output = fake_command(&state, &log)
        .arg("--state-dir")
        .arg(&state_dir)
        .arg("service")
        .arg("install")
        .arg("--json")
        .output()
        .unwrap();
    assert!(output.status.success());
    let data = &envelope(&output)["data"];
    assert_eq!(data["registered"], true);
    assert_eq!(data["deferred"], true);
    assert!(data["guidance"].is_string());
    let calls = calls(&log);
    assert_eq!(calls.last().map(String::as_str), Some("enable"));
    assert!(!calls.contains(&"start".to_owned()));
}

#[test]
fn uninstall_stops_before_unloading_and_removes_the_registration() {
    let _serial = serialized();
    let tmp = tempfile::tempdir().unwrap();
    let (state, log) = seeded(
        tmp.path(),
        "{\"session\":true,\"loaded\":true,\"enabled\":true,\"registered\":true}",
    );
    let output = fake_command(&state, &log)
        .arg("service")
        .arg("uninstall")
        .arg("--json")
        .output()
        .unwrap();
    assert!(output.status.success());
    let data = &envelope(&output)["data"];
    assert_eq!(data["removed"], true);
    assert_eq!(data["stopped"], true);
    let calls = calls(&log);
    assert_eq!(
        calls,
        [
            "session_available",
            "is_loaded",
            "status",
            "stop",
            "status",
            "unload",
            "remove_registration",
            "reload",
        ]
    );
    let stop = calls.iter().position(|call| call == "stop").unwrap();
    let unload = calls.iter().position(|call| call == "unload").unwrap();
    let remove = calls
        .iter()
        .position(|call| call == "remove_registration")
        .unwrap();
    assert!(
        stop < unload && unload < remove,
        "signal before bootout before removal"
    );
}

#[test]
fn status_reports_the_manager_state_fields() {
    let _serial = serialized();
    let tmp = tempfile::tempdir().unwrap();
    let (state, log) = seeded(
        tmp.path(),
        "{\"session\":true,\"loaded\":true,\"enabled\":true,\"registered\":true}",
    );
    let output = fake_command(&state, &log)
        .arg("service")
        .arg("status")
        .arg("--json")
        .output()
        .unwrap();
    assert!(output.status.success());
    let data = &envelope(&output)["data"];
    assert_eq!(data["registered"], true);
    assert_eq!(data["loaded"], true);
    assert_eq!(data["enabled"], true);
    assert_eq!(data["session_available"], true);
    assert_eq!(data["service_name"], native_service_name());
    assert!(data["domain"].is_string());
}

#[test]
fn status_without_a_state_directory_is_a_stable_diagnostic() {
    let _serial = serialized();
    let tmp = tempfile::tempdir().unwrap();
    let missing = tmp.path().join("does-not-exist");
    let (state, log) = seeded(
        tmp.path(),
        "{\"session\":true,\"loaded\":false,\"enabled\":false,\"registered\":false}",
    );
    let output = fake_command(&state, &log)
        .arg("--state-dir")
        .arg(&missing)
        .arg("service")
        .arg("status")
        .arg("--json")
        .output()
        .unwrap();
    assert!(output.status.success());
    let data = &envelope(&output)["data"];
    assert_eq!(data["registered"], false);
    assert_eq!(data["loaded"], false);
}

#[test]
fn no_session_install_completes_with_guidance_and_exits_zero() {
    let _serial = serialized();
    let tmp = tempfile::tempdir().unwrap();
    let (state, log) = seeded(
        tmp.path(),
        "{\"session\":false,\"loaded\":false,\"enabled\":false,\"registered\":false}",
    );
    let output = fake_command(&state, &log)
        .arg("service")
        .arg("install")
        .arg("--json")
        .output()
        .unwrap();
    assert!(output.status.success());
    let data = &envelope(&output)["data"];
    assert_eq!(data["registered"], false);
    let guidance = data["guidance"]
        .as_str()
        .expect("guidance on no-session install");
    assert!(guidance.contains("locron service install"));
    assert!(guidance.contains("loginctl enable-linger"));
    assert_eq!(calls(&log), ["session_available"]);
}

#[test]
fn no_session_install_human_mode_prints_the_guidance_to_stderr() {
    let _serial = serialized();
    let tmp = tempfile::tempdir().unwrap();
    let (state, log) = seeded(
        tmp.path(),
        "{\"session\":false,\"loaded\":false,\"enabled\":false,\"registered\":false}",
    );
    let output = fake_command(&state, &log)
        .arg("service")
        .arg("install")
        .output()
        .unwrap();
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("locron service install"));
    // Human output is still the pretty JSON data on stdout.
    let data: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(data["registered"], false);
}

#[test]
fn marker_refusal_directs_to_brew_services() {
    let _serial = serialized();
    let tmp = tempfile::tempdir().unwrap();
    let fake_dir = tmp.path().join("fake");
    fs::create_dir_all(&fake_dir).unwrap();
    let fake = fake_dir.join("locron");
    fs::copy(assert_cmd::cargo::cargo_bin("locron"), &fake).unwrap();
    let marker_dir = tmp.path().join("lib");
    fs::create_dir_all(&marker_dir).unwrap();
    fs::write(marker_dir.join(".disable-self-update"), "").unwrap();
    let (state, log) = seeded(
        tmp.path(),
        "{\"session\":true,\"loaded\":false,\"enabled\":false,\"registered\":false}",
    );
    let output = Command::new(&fake)
        .env("LOCRON_SERVICE_BACKEND", "fake")
        .env("LOCRON_SERVICE_FAKE_STATE", &state)
        .env("LOCRON_SERVICE_FAKE_LOG", &log)
        .args(["service", "install", "--json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(3));
    let envelope = envelope(&output);
    assert_eq!(envelope["ok"], false);
    assert_eq!(envelope["error"]["code"], "service_managed_install");
    let message = envelope["error"]["message"].as_str().unwrap();
    assert!(message.contains("brew services start locron"));
    assert_eq!(calls(&log), Vec::<String>::new());
}

#[test]
fn forcing_an_unsupported_backend_is_a_stable_platform_error() {
    let _serial = serialized();
    let tmp = tempfile::tempdir().unwrap();
    let (state, log) = seeded(
        tmp.path(),
        "{\"session\":true,\"loaded\":false,\"enabled\":false,\"registered\":false}",
    );
    let forced = if cfg!(target_os = "macos") {
        "systemd"
    } else {
        "launchd"
    };
    let output = Command::cargo_bin("locron")
        .unwrap()
        .env("LOCRON_SERVICE_BACKEND", forced)
        .env("LOCRON_SERVICE_FAKE_STATE", &state)
        .env("LOCRON_SERVICE_FAKE_LOG", &log)
        .args(["service", "status", "--json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let envelope = envelope(&output);
    assert_eq!(envelope["error"]["code"], "service_unsupported_platform");
}

#[test]
fn every_service_subcommand_has_help_text() {
    let _serial = serialized();
    for subcommand in ["install", "uninstall", "status"] {
        let output = Command::cargo_bin("locron")
            .unwrap()
            .args(["service", subcommand, "--help"])
            .output()
            .unwrap();
        assert!(output.status.success(), "{subcommand} --help must succeed");
        let help = String::from_utf8_lossy(&output.stdout);
        assert!(
            help.contains("Usage: locron service"),
            "{subcommand} help usage"
        );
        assert!(help.contains("Examples:"), "{subcommand} help examples");
    }
}

// ---------------------------------------------------------------------------
// Dashboard target: the same deterministic fake drives the second
// registration target; the flows add the access-token steps around the
// daemon registration's verified ordering.
// ---------------------------------------------------------------------------

#[test]
fn dashboard_enable_generates_the_token_and_registers_in_the_verified_order() {
    let _serial = serialized();
    let tmp = tempfile::tempdir().unwrap();
    let state_dir = tmp.path().join("state");
    let (state, log) = seeded(
        tmp.path(),
        "{\"session\":true,\"loaded\":false,\"enabled\":false,\"registered\":false}",
    );
    let output = fake_dashboard_command(&state, &log, &state_dir)
        .arg("enable")
        .arg("--json")
        .output()
        .unwrap();
    assert!(output.status.success());
    let data = &envelope(&output)["data"];
    assert_eq!(data["registered"], true);
    assert_eq!(data["restarted"], false);
    assert_eq!(data["deferred"], false);
    assert_eq!(data["service_name"], native_dashboard_service_name());
    assert_eq!(data["domain"], "fake/domain");
    let token = fs::read_to_string(state_dir.join("dashboard.token")).unwrap();
    assert_eq!(token.len(), 64, "enable generates a 64-hex access token");
    assert_eq!(
        calls(&log),
        [
            "session_available",
            "write_registration",
            "reload",
            "is_loaded",
            "enable",
            "start",
        ],
        "the dashboard flow keeps the verified write-reload-probe-enable-start order \
         and never probes the daemon lock"
    );
}

#[test]
fn dashboard_enable_is_idempotent_and_reset_regenerates_then_restarts() {
    let _serial = serialized();
    let tmp = tempfile::tempdir().unwrap();
    let state_dir = tmp.path().join("state");
    let (state, log) = seeded(
        tmp.path(),
        "{\"session\":true,\"loaded\":true,\"enabled\":true,\"registered\":true}",
    );
    // Prime the token the way a first enable would.
    let output = fake_dashboard_command(&state, &log, &state_dir)
        .arg("enable")
        .arg("--json")
        .output()
        .unwrap();
    assert!(output.status.success());
    let data = &envelope(&output)["data"];
    assert_eq!(
        data["restarted"], true,
        "repeated enable refreshes and restarts"
    );
    assert_eq!(
        calls(&log),
        [
            "session_available",
            "write_registration",
            "reload",
            "is_loaded",
            "restart",
            "status",
        ]
    );
    let first = fs::read_to_string(state_dir.join("dashboard.token")).unwrap();

    // --reset regenerates the token before the refresh-and-restart.
    fs::write(&log, "").unwrap();
    let output = fake_dashboard_command(&state, &log, &state_dir)
        .arg("enable")
        .arg("--reset")
        .arg("--json")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(envelope(&output)["data"]["restarted"], true);
    let second = fs::read_to_string(state_dir.join("dashboard.token")).unwrap();
    assert_ne!(first, second, "--reset must regenerate the access token");
    assert_eq!(second.len(), 64);
    let order = calls(&log);
    let restart_at = order.iter().position(|call| call == "restart").unwrap();
    let first_call = order.first().map(String::as_str);
    assert_eq!(first_call, Some("session_available"));
    let _ = restart_at;
}

#[test]
fn dashboard_disable_unregisters_then_removes_the_token() {
    let _serial = serialized();
    let tmp = tempfile::tempdir().unwrap();
    let state_dir = tmp.path().join("state");
    fs::create_dir_all(&state_dir).unwrap();
    fs::write(state_dir.join("dashboard.token"), "a".repeat(64)).unwrap();
    let (state, log) = seeded(
        tmp.path(),
        "{\"session\":true,\"loaded\":true,\"enabled\":true,\"registered\":true}",
    );
    let output = fake_dashboard_command(&state, &log, &state_dir)
        .arg("disable")
        .arg("--json")
        .output()
        .unwrap();
    assert!(output.status.success());
    let data = &envelope(&output)["data"];
    assert_eq!(data["removed"], true);
    assert_eq!(data["stopped"], true);
    assert_eq!(data["token_removed"], true);
    assert_eq!(data["service_name"], native_dashboard_service_name());
    assert!(
        !state_dir.join("dashboard.token").exists(),
        "disable must remove the access token"
    );
    let order = calls(&log);
    assert_eq!(
        order,
        [
            "session_available",
            "is_loaded",
            "status",
            "stop",
            "status",
            "unload",
            "remove_registration",
            "reload",
        ]
    );
}

#[test]
fn dashboard_status_reports_service_state_url_and_token_facts() {
    let _serial = serialized();
    let tmp = tempfile::tempdir().unwrap();
    let state_dir = tmp.path().join("state");
    fs::create_dir_all(&state_dir).unwrap();
    fs::write(state_dir.join("dashboard.token"), "b".repeat(64)).unwrap();
    let (state, log) = seeded(
        tmp.path(),
        "{\"session\":true,\"loaded\":true,\"enabled\":true,\"registered\":true}",
    );
    // The real flow writes the token with 0600; mirror that here so the
    // posture fact is observable (fs::write would use the umask).
    let token_path = state_dir.join("dashboard.token");
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&token_path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    let output = fake_dashboard_command(&state, &log, &state_dir)
        .arg("status")
        .arg("--json")
        .output()
        .unwrap();
    assert!(output.status.success());
    let data = &envelope(&output)["data"];
    assert_eq!(data["registered"], true);
    assert_eq!(data["loaded"], true);
    assert_eq!(data["enabled"], true);
    assert_eq!(data["service_name"], native_dashboard_service_name());
    assert_eq!(data["access_url"], "http://127.0.0.1:10824/");
    assert_eq!(data["token"]["present"], true);
    assert_eq!(data["token"]["permissions"], "owner_only");
    assert!(
        !data["token"].as_object().unwrap().contains_key("value"),
        "status must never report the token value"
    );
    assert!(
        !format!("{data}").contains("bbbbbbbb"),
        "no token material leaks"
    );
}

#[test]
fn dashboard_status_reports_a_missing_token_without_generating_one() {
    let _serial = serialized();
    let tmp = tempfile::tempdir().unwrap();
    let state_dir = tmp.path().join("state");
    let (state, log) = seeded(
        tmp.path(),
        "{\"session\":true,\"loaded\":false,\"enabled\":false,\"registered\":false}",
    );
    let output = fake_dashboard_command(&state, &log, &state_dir)
        .arg("status")
        .arg("--json")
        .output()
        .unwrap();
    assert!(output.status.success());
    let data = &envelope(&output)["data"];
    assert_eq!(data["token"]["present"], false);
    assert_eq!(data["token"]["permissions"], "missing");
    assert!(
        !state_dir.join("dashboard.token").exists(),
        "status is read-only and must not generate a token"
    );
}

#[test]
fn dashboard_enable_refuses_managed_binaries_before_generating_the_token() {
    let _serial = serialized();
    let tmp = tempfile::tempdir().unwrap();
    let fake_dir = tmp.path().join("fake");
    fs::create_dir_all(&fake_dir).unwrap();
    let fake = fake_dir.join("locron");
    fs::copy(assert_cmd::cargo::cargo_bin("locron"), &fake).unwrap();
    let marker_dir = tmp.path().join("lib");
    fs::create_dir_all(&marker_dir).unwrap();
    fs::write(marker_dir.join(".disable-self-update"), "").unwrap();
    let state_dir = tmp.path().join("state");
    let (state, log) = seeded(
        tmp.path(),
        "{\"session\":true,\"loaded\":false,\"enabled\":false,\"registered\":false}",
    );
    let output = Command::new(&fake)
        .env("LOCRON_SERVICE_BACKEND", "fake")
        .env("LOCRON_SERVICE_FAKE_STATE", &state)
        .env("LOCRON_SERVICE_FAKE_LOG", &log)
        .arg("--state-dir")
        .arg(&state_dir)
        .args(["dashboard", "enable", "--json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(3));
    let envelope = envelope(&output);
    assert_eq!(envelope["error"]["code"], "service_managed_install");
    assert!(
        envelope["error"]["message"]
            .as_str()
            .unwrap()
            .contains("brew services"),
        "guidance must point at the package manager"
    );
    assert!(
        !state_dir.join("dashboard.token").exists(),
        "refusal must happen before the token is generated"
    );
    assert_eq!(calls(&log), Vec::<String>::new());
}

#[test]
fn every_dashboard_subcommand_has_help_text() {
    let _serial = serialized();
    for subcommand in ["enable", "disable", "status"] {
        let output = Command::cargo_bin("locron")
            .unwrap()
            .args(["dashboard", subcommand, "--help"])
            .output()
            .unwrap();
        assert!(output.status.success(), "{subcommand} --help must succeed");
        let help = String::from_utf8_lossy(&output.stdout);
        assert!(
            help.contains("Usage: locron dashboard"),
            "{subcommand} help usage"
        );
        assert!(help.contains("Examples:"), "{subcommand} help examples");
    }
}
