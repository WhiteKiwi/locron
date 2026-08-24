//! Contract tests for the `locron dashboard` command family.
//!
//! The service-management flows (`enable`/`disable`/`status`) are covered by
//! `service.rs` against the deterministic fake backend; this suite covers the
//! command surface that drives real processes: foreground serving (startup URL
//! and token output, bind refusal, port strictness, fallback, service-mode
//! fixed port), the `token` display command, and the doctor exposure facts.

use std::fs;
use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use assert_cmd::cargo::cargo_bin;
use serde_json::Value;

const DEFAULT_PORT: u16 = 10824;

fn locron() -> Command {
    Command::new(cargo_bin("locron"))
}

fn envelope(output: &std::process::Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("locron.cli/v1 envelope on stdout")
}

/// A currently-free port, released immediately (small race window, standard
/// practice for port tests).
fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// A port held on both loopback families for the test's lifetime.
struct HeldPort {
    port: u16,
    _listeners: Vec<TcpListener>,
}

/// Binds `port` on both loopback families, keeping whatever succeeds. A bind
/// that fails because something else already holds the address is equally
/// fine — the port stays occupied from the server's point of view.
fn occupy(port: u16) -> Vec<TcpListener> {
    let mut held = Vec::new();
    if let Ok(listener) = TcpListener::bind(("127.0.0.1", port)) {
        held.push(listener);
    }
    if let Ok(listener) = TcpListener::bind(("::1", port)) {
        held.push(listener);
    }
    held
}

/// Picks a free port and holds it on both loopback families.
///
/// The server binds both families and serves as long as either succeeds, so
/// a single-family hold would not stop it. The probe port is released
/// between the pick and the hold and another test's server may claim it in
/// that window; retrying on a fresh port closes the race.
fn hold_port() -> HeldPort {
    for _ in 0..10 {
        let port = free_port();
        let listeners = occupy(port);
        if listeners.len() == 2 {
            return HeldPort {
                port,
                _listeners: listeners,
            };
        }
    }
    panic!("could not hold a free port on both loopback families");
}

/// Holds a fixed port on both loopback families, retrying while only one
/// family could be held (a transient holder may release the other family
/// before the server under test binds). An empty result means something else
/// holds both families, which blocks the server just as well.
fn hold_fixed(port: u16) -> Vec<TcpListener> {
    for _ in 0..10 {
        let listeners = occupy(port);
        match listeners.len() {
            2 => return listeners,
            0 => return listeners,
            _ => drop(listeners),
        }
    }
    panic!("could not hold {port} on both loopback families");
}

/// Reads `stdout` line by line on a thread; `next` waits for the next line
/// with a deadline, so a child that never prints fails the test instead of
/// hanging it.
fn line_reader(
    stdout: std::process::ChildStdout,
) -> (mpsc::Receiver<String>, thread::JoinHandle<()>) {
    let (sender, receiver) = mpsc::channel();
    let reader = thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            if sender.send(line.unwrap()).is_err() {
                break;
            }
        }
    });
    (receiver, reader)
}

fn next_line(receiver: &mpsc::Receiver<String>, what: &str) -> String {
    receiver
        .recv_timeout(Duration::from_secs(15))
        .unwrap_or_else(|_| panic!("timed out waiting for: {what}"))
}

/// Kills the child and waits for it, so no server process outlives a test.
fn kill(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// Owns a spawned serve child — directly or wrapped in `script` — and
/// guarantees the server process dies when the test ends, including when an
/// assertion panics. On macOS, killing the `script` parent does not deliver
/// SIGHUP to its PTY child, so the serve child is additionally located by its
/// unique `--state-dir` command-line argument and killed.
struct ServeGuard {
    child: Child,
    state_dir: Option<String>,
}

impl Drop for ServeGuard {
    fn drop(&mut self) {
        kill(&mut self.child);
        if let Some(state_dir) = &self.state_dir {
            kill_serve_child(state_dir);
        }
    }
}

/// Kills every process whose command line names `locron` and `state_dir`.
///
/// On macOS, killing the `script` parent does not deliver SIGHUP to its PTY
/// child, so the serve child would otherwise survive the test and keep the
/// stdout pipe open. The `--state-dir <dir>` argument is unique to this
/// test's child, which makes the scan unambiguous.
fn kill_serve_child(state_dir: &str) {
    use std::process::Command;
    let Ok(ps) = Command::new("ps").arg("-axo").arg("pid=,command=").output() else {
        eprintln!("SKIPPED: `ps` is unavailable; the serve child must be reaped manually");
        return;
    };
    let text = String::from_utf8_lossy(&ps.stdout);
    for line in text.lines() {
        let mut parts = line.trim_start().splitn(2, char::is_whitespace);
        let Some(pid) = parts.next().and_then(|pid| pid.parse::<u32>().ok()) else {
            continue;
        };
        if parts
            .next()
            .is_some_and(|command| command.contains("locron") && command.contains(state_dir))
        {
            let _ = Command::new("kill").arg(pid.to_string()).status();
        }
    }
}

/// Runs a command to completion with a hard deadline, killing the child when
/// it outlives the timeout — so an accidentally-started server fails the test
/// instead of hanging the suite.
fn run_with_timeout(command: &mut Command, timeout: Duration) -> std::process::Output {
    use std::io::Read;
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("command did not exit within {timeout:?}");
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let mut output = std::process::Output {
        status,
        stdout: Vec::new(),
        stderr: Vec::new(),
    };
    child
        .stdout
        .take()
        .unwrap()
        .read_to_end(&mut output.stdout)
        .unwrap();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_end(&mut output.stderr)
        .unwrap();
    output
}

/// A raw HTTP GET over the loopback port, returning the status code.
fn http_status(port: u16) -> Option<u16> {
    use std::io::{Read, Write};
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).ok()?;
    write!(
        stream,
        "GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n"
    )
    .ok()?;
    let mut response = Vec::new();
    stream.read_to_end(&mut response).ok()?;
    let text = String::from_utf8_lossy(&response);
    text.lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
}

#[test]
fn foreground_serve_prints_the_url_and_token_then_serves() {
    let dir = tempfile::tempdir().unwrap();
    let port = free_port();
    let mut child = locron()
        .arg("--state-dir")
        .arg(dir.path())
        .args(["dashboard", "--port", &port.to_string()])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let (receiver, reader) = line_reader(child.stdout.take().unwrap());
    let guard = ServeGuard {
        child,
        state_dir: None,
    };

    assert_eq!(
        next_line(&receiver, "the access URL"),
        format!("Dashboard URL: http://127.0.0.1:{port}/"),
        "the startup line must name the exact access URL"
    );
    let token_line = next_line(&receiver, "the first-run access token");
    let token = token_line
        .rsplit_once(": ")
        .map(|(_, token)| token)
        .expect("the token line names the token");
    assert_eq!(token.len(), 64, "the token is 64 hex characters");
    assert!(
        token.chars().all(|c| c.is_ascii_hexdigit()),
        "the token is hex"
    );
    assert!(
        token_line.starts_with("Access token (newly generated"),
        "a fresh state directory must report a newly generated token"
    );

    assert_eq!(
        http_status(port),
        Some(200),
        "the server must serve the entry page"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("dashboard.token")).unwrap(),
        token,
        "the printed token is the stored token"
    );
    drop(guard);
    let _ = reader.join();
}

#[test]
fn foreground_serve_machine_envelope_reports_facts_not_the_token() {
    let dir = tempfile::tempdir().unwrap();
    let port = free_port();
    let mut child = locron()
        .arg("--state-dir")
        .arg(dir.path())
        .args(["dashboard", "--port", &port.to_string(), "--json"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let (receiver, reader) = line_reader(child.stdout.take().unwrap());
    let guard = ServeGuard {
        child,
        state_dir: None,
    };

    let line = next_line(&receiver, "the machine envelope");
    let envelope: Value = serde_json::from_str(&line).expect("envelope on the first line");
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["command"], "dashboard");
    assert_eq!(
        envelope["data"]["access_url"],
        format!("http://127.0.0.1:{port}/")
    );
    assert_eq!(envelope["data"]["token"]["present"], true);
    assert_eq!(envelope["data"]["token"]["permissions"], "owner_only");
    assert_eq!(envelope["data"]["token"]["generated"], true);
    let envelope_text = format!("{envelope}");
    let stored_token = fs::read_to_string(dir.path().join("dashboard.token")).unwrap();
    assert!(
        !envelope_text.contains(&stored_token),
        "the machine envelope must never contain the token value"
    );
    drop(guard);
    let _ = reader.join();
}

#[test]
fn non_loopback_bind_is_refused() {
    let output = locron()
        .args(["dashboard", "--bind", "0.0.0.0"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2), "refusal is a usage error");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("refused 0.0.0.0") && stderr.contains("127.0.0.1"),
        "the refusal names the refused address and the allowed ones: {stderr}"
    );

    let output = locron()
        .args(["--json", "dashboard", "--bind", "localhost"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(envelope(&output)["error"]["code"], "invalid_request");
}

#[test]
fn explicit_port_is_strict_when_occupied() {
    let held = hold_port();
    let port = held.port;
    let dir = tempfile::tempdir().unwrap();
    let mut command = locron();
    command.arg("--state-dir").arg(dir.path());
    command.args(["dashboard", "--port", &port.to_string(), "--json"]);
    let output = run_with_timeout(&mut command, Duration::from_secs(10));
    assert_eq!(
        output.status.code(),
        Some(5),
        "an occupied explicit port is strict"
    );
    let error = &envelope(&output)["error"];
    assert_eq!(error["code"], "service_io");
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains(&port.to_string()),
        "the error names the occupied port"
    );
}

#[test]
fn service_mode_keeps_the_default_port_fixed_when_occupied() {
    // Only the hidden marker carried by launchd/systemd registration selects
    // the fixed policy: an occupied 10824 is an error, never a silent fallback.
    let _fixed = hold_fixed(DEFAULT_PORT);
    let dir = tempfile::tempdir().unwrap();
    let mut command = locron();
    command
        .stdin(Stdio::null())
        .arg("--state-dir")
        .arg(dir.path());
    command.args(["dashboard", "serve", "--service-mode", "--json"]);
    let output = run_with_timeout(&mut command, Duration::from_secs(10));
    assert_eq!(
        output.status.code(),
        Some(5),
        "service mode must fail on an occupied default port"
    );
    let error = &envelope(&output)["error"];
    assert_eq!(error["code"], "service_io");
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains(&DEFAULT_PORT.to_string()),
        "the error names the fixed port"
    );
}

#[test]
fn redirected_bare_serve_still_uses_foreground_fallback() {
    let _fixed = hold_fixed(DEFAULT_PORT);
    let dir = tempfile::tempdir().unwrap();
    let mut child = locron()
        .stdin(Stdio::null())
        .arg("--state-dir")
        .arg(dir.path())
        .arg("dashboard")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let (receiver, reader) = line_reader(child.stdout.take().unwrap());
    let guard = ServeGuard {
        child,
        state_dir: None,
    };
    let line = next_line(&receiver, "the redirected fallback URL");
    let port = line
        .strip_prefix("Dashboard URL: http://127.0.0.1:")
        .and_then(|rest| rest.strip_suffix('/'))
        .and_then(|port| port.parse::<u16>().ok())
        .unwrap_or_else(|| panic!("unexpected startup line: {line:?}"));
    assert_ne!(
        port, DEFAULT_PORT,
        "redirected foreground serve must fall back"
    );
    assert_eq!(http_status(port), Some(200));
    drop(guard);
    let _ = reader.join();
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn foreground_serve_falls_back_when_the_default_port_is_occupied() {
    // A real PTY is another foreground context. The child dies with SIGHUP
    // when the PTY closes.
    if Command::new("script").arg("-V").output().is_err() {
        eprintln!("SKIPPED: `script` is unavailable in this environment");
        return;
    }
    let _fixed = hold_fixed(DEFAULT_PORT);
    let dir = tempfile::tempdir().unwrap();
    let binary = cargo_bin("locron");
    let binary = binary.display().to_string();
    let state_dir = dir.path().display().to_string();
    let mut command = Command::new("script");
    if cfg!(target_os = "macos") {
        command.args([
            "-q",
            "/dev/null",
            &binary,
            "--state-dir",
            &state_dir,
            "dashboard",
        ]);
    } else {
        let invocation = format!("{binary:?} --state-dir {state_dir:?} dashboard");
        command.args(["-q", "-c", &invocation, "/dev/null"]);
    }
    let child = command.stdout(Stdio::piped()).spawn().unwrap();
    let mut guard = ServeGuard {
        child,
        state_dir: Some(state_dir),
    };
    let (receiver, reader) = line_reader(guard.child.stdout.take().unwrap());

    // macOS `script` prefixes the PTY output with terminal control bytes, so
    // locate the startup line by its marker instead of assuming it is first.
    let line = loop {
        let line = next_line(&receiver, "the fallback URL");
        let line = line.trim_end_matches('\r').to_owned();
        if line.contains("Dashboard URL: http://127.0.0.1:") {
            break line;
        }
    };
    let port = line
        .find("Dashboard URL: http://127.0.0.1:")
        .map(|start| line[start + "Dashboard URL: http://127.0.0.1:".len()..].to_owned())
        .and_then(|rest| rest.strip_suffix('/').map(str::to_owned))
        .and_then(|port| port.parse::<u16>().ok())
        .unwrap_or_else(|| panic!("unexpected startup line: {line:?}"));
    assert_ne!(
        port, DEFAULT_PORT,
        "an occupied default port must fall back (startup line: {line:?})"
    );
    assert_eq!(
        http_status(port),
        Some(200),
        "the fallback server must serve"
    );

    // Closing the PTY delivers SIGHUP on Linux; on macOS the serve child
    // survives the `script` parent, so the guard additionally kills it by its
    // unique `--state-dir` argument. The port poll below catches a failed
    // cleanup instead of leaking a server.
    drop(guard);
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
        assert!(
            std::time::Instant::now() < deadline,
            "the fallback server must stop when its terminal closes"
        );
        std::thread::sleep(Duration::from_millis(200));
    }
    // The child is dead, so its end of the stdout pipe is closed and the
    // reader thread has finished.
    let _ = reader.join();
}

#[test]
fn port_and_bind_are_refused_on_service_subcommands() {
    let output = locron()
        .args(["dashboard", "--port", "9000", "status", "--json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let error = envelope(&output)["error"].clone();
    assert_eq!(error["code"], "invalid_request");
    let message = error["message"].as_str().unwrap().to_owned();
    assert!(
        message.contains("foreground serving"),
        "the usage error points at the foreground form: {message}"
    );
}

#[test]
fn dashboard_token_displays_the_stored_token() {
    let dir = tempfile::tempdir().unwrap();
    let token = "0123456789abcdef".repeat(4);
    fs::write(dir.path().join("dashboard.token"), &token).unwrap();

    let output = locron()
        .arg("--state-dir")
        .arg(dir.path())
        .args(["dashboard", "token", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let data = &envelope(&output)["data"];
    assert_eq!(data["token"], token);
    assert_eq!(
        data["access_url"],
        format!("http://127.0.0.1:{DEFAULT_PORT}/")
    );

    let output = locron()
        .arg("--state-dir")
        .arg(dir.path())
        .args(["dashboard", "token"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stdout).contains(&token),
        "the human form shows the token value"
    );
}

#[test]
fn dashboard_token_generates_a_missing_token() {
    let dir = tempfile::tempdir().unwrap();
    let output = locron()
        .arg("--state-dir")
        .arg(dir.path())
        .args(["dashboard", "token", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let token = envelope(&output)["data"]["token"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(token.len(), 64);
    assert_eq!(
        fs::read_to_string(dir.path().join("dashboard.token")).unwrap(),
        token,
        "the displayed token is persisted"
    );
}

#[test]
fn doctor_reports_the_dashboard_exposure_facts() {
    let tmp = tempfile::tempdir().unwrap();
    let state_dir = tmp.path().join("state");
    fs::create_dir_all(&state_dir).unwrap();
    let token_path = state_dir.join("dashboard.token");
    fs::write(&token_path, "c".repeat(64)).unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&token_path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    let fake_state = tmp.path().join("state.json");
    let fake_log = tmp.path().join("calls.log");
    fs::write(
        &fake_state,
        "{\"session\":true,\"loaded\":true,\"enabled\":true,\"registered\":true}",
    )
    .unwrap();
    let fake_doctor = |json: bool| {
        let mut command = locron();
        command
            .env("LOCRON_SERVICE_BACKEND", "fake")
            .env("LOCRON_SERVICE_FAKE_STATE", &fake_state)
            .env("LOCRON_SERVICE_FAKE_LOG", &fake_log)
            .arg("--state-dir")
            .arg(&state_dir)
            .arg("doctor");
        if json {
            command.arg("--json");
        }
        command
    };

    let output = fake_doctor(true).output().unwrap();
    assert!(output.status.success());
    let dashboard = &envelope(&output)["data"]["dashboard"];
    assert_eq!(dashboard["token"]["present"], true);
    assert_eq!(dashboard["token"]["permissions"], "owner_only");
    assert_eq!(dashboard["registered"], true);
    assert_eq!(dashboard["loaded"], true);
    assert_eq!(
        dashboard["access_url"],
        format!("http://127.0.0.1:{DEFAULT_PORT}/")
    );
    assert!(
        !dashboard["token"]
            .as_object()
            .unwrap()
            .contains_key("value"),
        "doctor must never report the token value"
    );

    let output = fake_doctor(false).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("ok   dashboard service: registered"),
        "human doctor output reports the registration: {stdout}"
    );
    assert!(stdout.contains("ok   dashboard token: present (owner only)"));
}
