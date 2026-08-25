//! `locron self-update` contract tests against a local HTTP fixture.
//!
//! `LOCRON_UPDATE_API_BASE` and `LOCRON_UPDATE_ASSET_BASE` point the binary at
//! the fixture, which serves a fake `releases/latest` document, checksums, and
//! tarballs. Replacement tests run a *copy* of the real binary from a temp
//! directory so the build artifact itself is never replaced.

use std::fmt::Write as _;
use std::fs;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::process::Command as StdCommand;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use assert_cmd::Command;
use flate2::Compression;
use flate2::write::GzEncoder;
use predicates::prelude::*;
use serde_json::Value;
use sha2::{Digest, Sha256};

const NEW_TAG: &str = "v9.9.9";
const NEW_VERSION: &str = "9.9.9";
/// The fixture's replacement binary: a tiny shell script that reports its
/// version and records post-replace `service install`, `dashboard status`,
/// and `dashboard enable` invocations in `$LOCRON_FIXTURE_SERVICE_LOG` when
/// that environment is set. `$LOCRON_FIXTURE_DASHBOARD_REGISTERED` makes
/// `dashboard status --json` report a registered dashboard.
const NEW_BINARY: &[u8] = b"#!/bin/sh\n\
if [ \"${1:-}\" = \"service\" ] && [ \"${2:-}\" = \"install\" ]; then\n\
    if [ -n \"${LOCRON_FIXTURE_SERVICE_LOG:-}\" ]; then\n\
        echo \"service install\" >> \"$LOCRON_FIXTURE_SERVICE_LOG\"\n\
    fi\n\
    exit 0\n\
fi\n\
if [ \"${1:-}\" = \"dashboard\" ] && [ \"${2:-}\" = \"status\" ]; then\n\
    if [ -n \"${LOCRON_FIXTURE_EXPECT_STATE_DIR:-}\" ] && { [ \"${3:-}\" != \"--state-dir\" ] || [ \"${4:-}\" != \"$LOCRON_FIXTURE_EXPECT_STATE_DIR\" ]; }; then\n\
        exit 9\n\
    fi\n\
    if [ -n \"${LOCRON_FIXTURE_SERVICE_LOG:-}\" ]; then\n\
        echo \"dashboard status\" >> \"$LOCRON_FIXTURE_SERVICE_LOG\"\n\
    fi\n\
    if [ \"${LOCRON_FIXTURE_DASHBOARD_REGISTERED:-0}\" = \"malformed\" ]; then\n\
        echo '{\"schema\":\"locron.cli/v1\",\"ok\":true,\"command\":\"dashboard status\",\"data\":{\"registered\":\"yes\"},\"warnings\":[]}'\n\
    elif [ \"${LOCRON_FIXTURE_DASHBOARD_REGISTERED:-0}\" = \"missing\" ]; then\n\
        echo '{\"schema\":\"locron.cli/v1\",\"ok\":true,\"command\":\"dashboard status\",\"data\":{},\"warnings\":[]}'\n\
    elif [ \"${LOCRON_FIXTURE_DASHBOARD_REGISTERED:-0}\" = \"1\" ]; then\n\
        echo '{\"schema\":\"locron.cli/v1\",\"ok\":true,\"command\":\"dashboard status\",\"data\":{\"registered\":true,\"loaded\":true,\"enabled\":true,\"domain\":null,\"pid\":null,\"executable\":null,\"session_available\":false,\"service_name\":\"dev.locron.dashboard\",\"access_url\":\"http://127.0.0.1:10824/\",\"token\":{\"present\":true,\"permissions\":\"owner_only\"}},\"warnings\":[]}'\n\
    else\n\
        echo '{\"schema\":\"locron.cli/v1\",\"ok\":true,\"command\":\"dashboard status\",\"data\":{\"registered\":false,\"loaded\":false,\"enabled\":null,\"domain\":null,\"pid\":null,\"executable\":null,\"session_available\":false,\"service_name\":\"dev.locron.dashboard\",\"access_url\":\"http://127.0.0.1:10824/\",\"token\":{\"present\":false,\"permissions\":\"missing\"}},\"warnings\":[]}'\n\
    fi\n\
    exit 0\n\
fi\n\
if [ \"${1:-}\" = \"dashboard\" ] && [ \"${2:-}\" = \"enable\" ]; then\n\
    if [ -n \"${LOCRON_FIXTURE_EXPECT_STATE_DIR:-}\" ] && { [ \"${3:-}\" != \"--state-dir\" ] || [ \"${4:-}\" != \"$LOCRON_FIXTURE_EXPECT_STATE_DIR\" ]; }; then\n\
        exit 9\n\
    fi\n\
    if [ -n \"${LOCRON_FIXTURE_SERVICE_LOG:-}\" ]; then\n\
        echo \"dashboard enable\" >> \"$LOCRON_FIXTURE_SERVICE_LOG\"\n\
    fi\n\
    exit 0\n\
fi\n\
echo fixture-locron 9.9.9\n";

type Response = (u16, Vec<(String, String)>, Vec<u8>);
type Handler = Box<dyn Fn(&str) -> Response + Send>;

/// A tiny one-request-per-connection HTTP fixture.
struct Fixture {
    address: SocketAddr,
    requests: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl Fixture {
    fn start(handler: Handler) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        // The accept loop signals readiness; clients connect only after the
        // socket is live and being served.
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<()>();
        let handle = thread::spawn({
            let requests = Arc::clone(&requests);
            let stop = Arc::clone(&stop);
            move || {
                let _ = ready_tx.send(());
                while !stop.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            // A panicking connection must never kill the
                            // listener: keep accepting subsequent requests.
                            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                serve(&handler, &requests, &mut stream);
                            }));
                        }
                        // Transient accept errors (e.g. ECONNABORTED under
                        // parallel load) must not close the listener either:
                        // dropping it makes later connects fail with
                        // ECONNREFUSED. Keep serving until `stop` is set.
                        Err(_) => thread::sleep(Duration::from_millis(10)),
                    }
                }
            }
        });
        ready_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("fixture server must start accepting within 5s");
        Self {
            address,
            requests,
            stop,
            handle: Some(handle),
        }
    }

    fn requests(&self) -> Vec<String> {
        self.requests
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn serve(handler: &Handler, requests: &Mutex<Vec<String>>, stream: &mut TcpStream) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    // The accepted stream inherits O_NONBLOCK from the listener, so reads and
    // writes can fail fast with WouldBlock before the client's bytes have
    // arrived. Poll with a deadline instead of giving up on the first
    // WouldBlock (which used to close the connection mid-request under load).
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut buffer = [0_u8; 4096];
    let mut used = 0;
    let header_end = loop {
        if used >= buffer.len() || Instant::now() >= deadline {
            break None;
        }
        match stream.read(&mut buffer[used..]) {
            Ok(0) => break None,
            Ok(count) => {
                used += count;
                if buffer[..used]
                    .windows(4)
                    .any(|window| window == b"\r\n\r\n")
                {
                    break Some(used);
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                thread::sleep(Duration::from_millis(5));
            }
            Err(_) => break None,
        }
    };
    let Some(used) = header_end else { return };
    let head = String::from_utf8_lossy(&buffer[..used]);
    let path = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/")
        .to_owned();
    requests
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(path.clone());
    let (status, headers, body) = handler(&path);
    let mut response = format!("HTTP/1.1 {} {}\r\n", status, reason_phrase(status));
    for (name, value) in &headers {
        let _ = write!(response, "{name}: {value}\r\n");
    }
    let _ = write!(
        response,
        "Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    write_all_polling(stream, response.as_bytes(), deadline);
    write_all_polling(stream, &body, deadline);
}

/// Writes `bytes`, retrying WouldBlock/TimedOut until the deadline.
fn write_all_polling(stream: &mut TcpStream, mut bytes: &[u8], deadline: Instant) {
    while !bytes.is_empty() {
        match stream.write(bytes) {
            Ok(0) => return,
            Ok(count) => bytes = &bytes[count..],
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                if Instant::now() >= deadline {
                    return;
                }
                thread::sleep(Duration::from_millis(5));
            }
            Err(_) => return,
        }
    }
}

fn reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        403 => "Forbidden",
        404 => "Not Found",
        _ => "OK",
    }
}

fn json_response(body: &str) -> Response {
    (
        200,
        vec![("Content-Type".to_owned(), "application/json".to_owned())],
        body.as_bytes().to_vec(),
    )
}

fn text_response(body: &str) -> Response {
    (
        200,
        vec![("Content-Type".to_owned(), "text/plain".to_owned())],
        body.as_bytes().to_vec(),
    )
}

fn bytes_response(body: Vec<u8>) -> Response {
    (
        200,
        vec![("Content-Type".to_owned(), "application/gzip".to_owned())],
        body,
    )
}

fn latest_document(tag: &str, target: &str) -> String {
    format!(
        r#"{{"tag_name":"{tag}","name":"{tag}","assets":[{{"name":"locron-{tag}-{target}.tar.gz"}},{{"name":"SHA256SUMS.txt"}}]}}"#
    )
}

/// A gzip tarball containing `locron-{tag}-{target}/locron` with `NEW_BINARY`.
fn fixture_tarball(target: &str) -> Vec<u8> {
    let mut tar = Vec::new();
    {
        // `append_data` (unlike `append`) computes the header checksum.
        let mut builder = tar::Builder::new(&mut tar);
        let mut directory = tar::Header::new_gnu();
        directory.set_mode(0o755);
        directory.set_entry_type(tar::EntryType::Directory);
        directory.set_size(0);
        builder
            .append_data(
                &mut directory,
                format!("locron-{NEW_TAG}-{target}"),
                std::io::empty(),
            )
            .unwrap();

        let mut file = tar::Header::new_gnu();
        file.set_mode(0o755);
        file.set_entry_type(tar::EntryType::Regular);
        file.set_size(NEW_BINARY.len() as u64);
        builder
            .append_data(
                &mut file,
                format!("locron-{NEW_TAG}-{target}/locron"),
                NEW_BINARY,
            )
            .unwrap();
        builder.finish().unwrap();
    }
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&tar).unwrap();
    encoder.finish().unwrap()
}

fn valid_sums(target: &str) -> String {
    let hash = sha256_hex(&fixture_tarball(target));
    format!("{hash}  locron-{NEW_TAG}-{target}.tar.gz\n")
}

/// A fixture serving the latest document, checksums, and the target tarball.
fn update_fixture(target: &str, sums: &str) -> Fixture {
    let latest = latest_document(NEW_TAG, target);
    let sums_path = format!("/releases/download/{NEW_TAG}/SHA256SUMS.txt");
    let tarball_path = format!("/releases/download/{NEW_TAG}/locron-{NEW_TAG}-{target}.tar.gz");
    let tarball = fixture_tarball(target);
    let sums = sums.to_owned();
    Fixture::start(Box::new(move |path: &str| {
        if path == "/repos/WhiteKiwi/locron/releases/latest" {
            json_response(&latest)
        } else if path == sums_path {
            text_response(&sums)
        } else if path == tarball_path {
            bytes_response(tarball.clone())
        } else {
            (404, Vec::new(), b"not found".to_vec())
        }
    }))
}

/// The published target triple for the running test platform.
fn expected_target() -> String {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "aarch64-apple-darwin".to_owned(),
        ("macos", "x86_64") => "x86_64-apple-darwin".to_owned(),
        ("linux", "aarch64") => "aarch64-unknown-linux-gnu".to_owned(),
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu".to_owned(),
        (os, arch) => panic!("unsupported test platform: {os}/{arch}"),
    }
}

/// Serializes the suite's tests. Every test forks real child processes
/// against build artifacts, and cargo runs tests in parallel; CI showed
/// cross-test interference under that parallelism (ETXTBSY on spawn under
/// Linux, fixture connection refused under macOS). Each test must call this
/// first and hold the guard for its whole body.
fn serialized() -> std::sync::MutexGuard<'static, ()> {
    static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());
    SERIAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// A writable copy of the real binary at `dir/fake/locron`, so the build
/// artifact is never replaced and the package-manager marker path
/// (`dir/lib/.disable-self-update`) stays per-test.
fn fake_binary(dir: &Path) -> std::path::PathBuf {
    let fake_dir = dir.join("fake");
    fs::create_dir_all(&fake_dir).unwrap();
    let fake = fake_dir.join("locron");
    fs::copy(assert_cmd::cargo::cargo_bin("locron"), &fake).unwrap();
    fs::write(
        fake_dir.join(".locron-install-receipt-v1"),
        "locron.install/v1\nstandalone\n",
    )
    .unwrap();
    fake
}

/// A command running the fake binary against the fixture. The post-replace
/// `service install` invocation records itself in `service_log` (a fixture
/// log path inherited from the update process's environment).
fn locron_command(fake: &Path, fixture: &Fixture, service_log: &Path) -> Command {
    let mut command = Command::new(fake);
    command
        .env(
            "LOCRON_UPDATE_API_BASE",
            format!("http://{}", fixture.address),
        )
        .env(
            "LOCRON_UPDATE_ASSET_BASE",
            format!("http://{}", fixture.address),
        )
        .env("LOCRON_FIXTURE_SERVICE_LOG", service_log);
    command
}

/// The recorded post-replace `service install` invocations, or none when the
/// fixture log was never written.
fn service_log_entries(path: &Path) -> Vec<String> {
    fs::read_to_string(path)
        .map(|text| text.lines().map(str::to_owned).collect())
        .unwrap_or_default()
}

#[test]
fn self_update_installs_the_latest_release() {
    let _serial = serialized();
    let target = expected_target();
    let fixture = update_fixture(&target, &valid_sums(&target));
    let dir = tempfile::tempdir().unwrap();
    let fake = fake_binary(dir.path());
    let service_log = dir.path().join("service.log");

    locron_command(&fake, &fixture, &service_log)
        .args(["--json", "self-update"])
        .assert()
        .success()
        .stdout(predicate::function(|stdout: &[u8]| {
            let envelope: Value = serde_json::from_slice(stdout).unwrap();
            assert_eq!(envelope["schema"], "locron.cli/v1");
            assert_eq!(envelope["ok"], true);
            assert_eq!(envelope["command"], "self-update");
            assert_eq!(
                envelope["data"]["current_version"],
                env!("CARGO_PKG_VERSION")
            );
            assert_eq!(envelope["data"]["new_version"], NEW_VERSION);
            assert_eq!(envelope["data"]["updated"], true);
            assert_eq!(envelope["warnings"], serde_json::json!([]));
            true
        }));

    assert_eq!(
        fs::read(&fake).unwrap(),
        NEW_BINARY,
        "the executable must contain the new binary"
    );
    let output = StdCommand::new(fake.as_path()).arg("-V").output().unwrap();
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("fixture-locron 9.9.9"),
        "the next invocation must run the new binary"
    );
    let requests = fixture.requests();
    assert!(requests.contains(&"/repos/WhiteKiwi/locron/releases/latest".to_owned()));
    assert!(requests.contains(&format!("/releases/download/{NEW_TAG}/SHA256SUMS.txt")));
    assert!(requests.contains(&format!(
        "/releases/download/{NEW_TAG}/locron-{NEW_TAG}-{target}.tar.gz"
    )));
    assert_eq!(
        service_log_entries(&service_log),
        ["service install", "dashboard status"],
        "a successful replace must run the post-replace service registration once \
         and probe the dashboard registration"
    );
}

#[test]
fn self_update_refreshes_a_registered_dashboard_exactly_once_after_replace() {
    let _serial = serialized();
    let target = expected_target();
    let fixture = update_fixture(&target, &valid_sums(&target));
    let dir = tempfile::tempdir().unwrap();
    let fake = fake_binary(dir.path());
    let service_log = dir.path().join("service.log");
    let state_dir = dir.path().join("custom-state");

    locron_command(&fake, &fixture, &service_log)
        .env("LOCRON_FIXTURE_DASHBOARD_REGISTERED", "1")
        .env("LOCRON_FIXTURE_EXPECT_STATE_DIR", &state_dir)
        .arg("--state-dir")
        .arg(&state_dir)
        .args(["--json", "self-update"])
        .assert()
        .success()
        .stdout(predicate::function(|stdout: &[u8]| {
            let envelope: Value = serde_json::from_slice(stdout).unwrap();
            assert_eq!(envelope["data"]["updated"], true);
            assert_eq!(envelope["warnings"], serde_json::json!([]));
            true
        }));

    assert_eq!(
        fs::read(&fake).unwrap(),
        NEW_BINARY,
        "the executable must contain the new binary"
    );
    assert_eq!(
        service_log_entries(&service_log),
        ["service install", "dashboard status", "dashboard enable"],
        "a successful replace must refresh a registered dashboard exactly once"
    );
}

#[test]
fn self_update_leaves_an_absent_dashboard_untouched() {
    let _serial = serialized();
    let target = expected_target();
    let fixture = update_fixture(&target, &valid_sums(&target));
    let dir = tempfile::tempdir().unwrap();
    let fake = fake_binary(dir.path());
    let service_log = dir.path().join("service.log");

    locron_command(&fake, &fixture, &service_log)
        .env("LOCRON_FIXTURE_DASHBOARD_REGISTERED", "0")
        .args(["--json", "self-update"])
        .assert()
        .success()
        .stdout(predicate::function(|stdout: &[u8]| {
            let envelope: Value = serde_json::from_slice(stdout).unwrap();
            assert_eq!(envelope["data"]["updated"], true);
            assert_eq!(envelope["warnings"], serde_json::json!([]));
            true
        }));

    assert_eq!(
        service_log_entries(&service_log),
        ["service install", "dashboard status"],
        "a dashboard that was never enabled must not be registered by the update"
    );
}

#[test]
fn self_update_warns_and_does_not_mutate_on_malformed_dashboard_status() {
    let _serial = serialized();
    let target = expected_target();
    for malformed in ["malformed", "missing"] {
        let fixture = update_fixture(&target, &valid_sums(&target));
        let dir = tempfile::tempdir().unwrap();
        let fake = fake_binary(dir.path());
        let service_log = dir.path().join("service.log");

        locron_command(&fake, &fixture, &service_log)
            .env("LOCRON_FIXTURE_DASHBOARD_REGISTERED", malformed)
            .args(["--json", "self-update"])
            .assert()
            .success()
            .stdout(predicate::function(|stdout: &[u8]| {
                let envelope: Value = serde_json::from_slice(stdout).unwrap();
                assert_eq!(envelope["data"]["updated"], true);
                assert!(
                    envelope["warnings"][0].as_str().is_some_and(
                        |warning| warning.contains("data.registered must be a boolean")
                    ),
                    "warning: {}",
                    envelope["warnings"]
                );
                true
            }));

        assert_eq!(
            service_log_entries(&service_log),
            ["service install", "dashboard status"],
            "malformed status must never register or refresh the dashboard"
        );
    }
}

#[test]
fn human_output_reports_current_and_new_version() {
    let _serial = serialized();
    let target = expected_target();
    let fixture = update_fixture(&target, &valid_sums(&target));
    let dir = tempfile::tempdir().unwrap();
    let fake = fake_binary(dir.path());
    let service_log = dir.path().join("service.log");

    locron_command(&fake, &fixture, &service_log)
        .arg("self-update")
        .assert()
        .success()
        .stdout(predicate::str::contains(format!(
            "Current version: {}",
            env!("CARGO_PKG_VERSION")
        )))
        .stdout(predicate::str::contains(format!(
            "New version: {NEW_VERSION}"
        )))
        .stdout(predicate::str::contains("Updated: yes"))
        .stdout(predicate::function(|stdout: &[u8]| {
            serde_json::from_slice::<Value>(stdout).is_err()
        }));
}

#[test]
fn self_update_reports_already_up_to_date_without_downloading() {
    let _serial = serialized();
    let target = expected_target();
    let current = env!("CARGO_PKG_VERSION");
    let latest = latest_document(&format!("v{current}"), &target);
    let fixture = Fixture::start(Box::new(move |path: &str| {
        if path == "/repos/WhiteKiwi/locron/releases/latest" {
            json_response(&latest)
        } else {
            (404, Vec::new(), b"not found".to_vec())
        }
    }));
    let dir = tempfile::tempdir().unwrap();
    let fake = fake_binary(dir.path());
    let service_log = dir.path().join("service.log");

    locron_command(&fake, &fixture, &service_log)
        .args(["--json", "self-update"])
        .assert()
        .success()
        .stdout(predicate::function(|stdout: &[u8]| {
            let envelope: Value = serde_json::from_slice(stdout).unwrap();
            assert_eq!(envelope["command"], "self-update");
            assert_eq!(envelope["ok"], true);
            assert_eq!(envelope["data"]["current_version"], current);
            assert_eq!(envelope["data"]["new_version"], current);
            assert_eq!(envelope["data"]["updated"], false);
            true
        }));

    assert!(
        fixture
            .requests()
            .iter()
            .all(|path| !path.contains("/releases/download/")),
        "already up to date must not download assets"
    );
    assert_ne!(fs::read(&fake).unwrap(), NEW_BINARY);
    assert_eq!(
        service_log_entries(&service_log),
        Vec::<String>::new(),
        "an update that replaces nothing must not register a service"
    );
}

#[test]
fn checksum_mismatch_leaves_the_old_binary_untouched() {
    let _serial = serialized();
    let target = expected_target();
    let wrong_sums = format!("{}  locron-{NEW_TAG}-{target}.tar.gz\n", "0".repeat(64));
    let fixture = update_fixture(&target, &wrong_sums);
    let dir = tempfile::tempdir().unwrap();
    let fake = fake_binary(dir.path());
    let before = fs::read(&fake).unwrap();
    let service_log = dir.path().join("service.log");

    locron_command(&fake, &fixture, &service_log)
        .args(["--json", "self-update"])
        .assert()
        .failure()
        .code(5)
        .stdout(predicate::function(|stdout: &[u8]| {
            let envelope: Value = serde_json::from_slice(stdout).unwrap();
            assert_eq!(envelope["ok"], false);
            assert_eq!(envelope["command"], "self-update");
            assert_eq!(envelope["error"]["code"], "update_checksum_mismatch");
            assert!(
                envelope["error"]["message"]
                    .as_str()
                    .unwrap()
                    .contains("checksum mismatch")
            );
            true
        }));

    assert_eq!(
        fs::read(&fake).unwrap(),
        before,
        "a failed update must leave the old binary untouched"
    );
    assert_eq!(
        service_log_entries(&service_log),
        Vec::<String>::new(),
        "a failed update must not register a service"
    );
}

#[test]
fn atomic_replace_keeps_a_running_process_on_the_old_binary() {
    let _serial = serialized();
    let target = expected_target();
    let fixture = update_fixture(&target, &valid_sums(&target));
    let dir = tempfile::tempdir().unwrap();
    let fake = fake_binary(dir.path());
    let service_log = dir.path().join("service.log");

    // A process running the old binary (mcp waits on stdin) stays alive across
    // the replacement and keeps its old inode.
    let mut running = StdCommand::new(fake.as_path())
        .arg("mcp")
        .env("LOCRON_STATE_DIR", dir.path().join("state"))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();

    locron_command(&fake, &fixture, &service_log)
        .arg("self-update")
        .assert()
        .success();

    assert!(
        running.try_wait().unwrap().is_none(),
        "the pre-update process must keep running after the replacement"
    );
    assert_eq!(fs::read(&fake).unwrap(), NEW_BINARY);
    assert_eq!(
        service_log_entries(&service_log),
        ["service install", "dashboard status"],
        "the replace must run the post-replace service registration and the \
         dashboard probe"
    );

    drop(running.stdin.take());
    let status = running.wait().unwrap();
    assert!(status.success(), "the pre-update process exits cleanly");
}

#[test]
fn marker_file_refusal_directs_to_brew_upgrade() {
    let _serial = serialized();
    let dir = tempfile::tempdir().unwrap();
    let fake = fake_binary(dir.path());
    let marker_dir = dir.path().join("lib");
    fs::create_dir_all(&marker_dir).unwrap();
    fs::write(marker_dir.join(".disable-self-update"), "").unwrap();

    // The marker is checked before any network access.
    let mut command = Command::new(&fake);
    command
        .env("LOCRON_UPDATE_API_BASE", "http://127.0.0.1:1")
        .env("LOCRON_UPDATE_ASSET_BASE", "http://127.0.0.1:1");
    command
        .args(["--json", "self-update"])
        .assert()
        .failure()
        .code(3)
        .stdout(predicate::function(|stdout: &[u8]| {
            let envelope: Value = serde_json::from_slice(stdout).unwrap();
            assert_eq!(envelope["ok"], false);
            assert_eq!(envelope["error"]["code"], "update_managed_install");
            assert!(
                envelope["error"]["message"]
                    .as_str()
                    .unwrap()
                    .contains("brew upgrade locron")
            );
            true
        }));
}

#[test]
fn missing_receipt_refuses_before_network_access() {
    let _serial = serialized();
    let dir = tempfile::tempdir().unwrap();
    let fake = fake_binary(dir.path());
    fs::remove_file(fake.parent().unwrap().join(".locron-install-receipt-v1")).unwrap();

    let mut command = Command::new(&fake);
    command
        .env("LOCRON_UPDATE_API_BASE", "http://127.0.0.1:1")
        .env("LOCRON_UPDATE_ASSET_BASE", "http://127.0.0.1:1");
    command
        .args(["--json", "self-update"])
        .assert()
        .failure()
        .code(3)
        .stdout(predicate::function(|stdout: &[u8]| {
            let envelope: Value = serde_json::from_slice(stdout).unwrap();
            assert_eq!(envelope["error"]["code"], "update_unowned_install");
            let message = envelope["error"]["message"].as_str().unwrap();
            assert!(message.contains("cargo install --locked locron"));
            assert!(message.contains("rerun the standalone installer"));
            true
        }));
}

#[test]
fn malformed_receipts_refuse_self_update() {
    let _serial = serialized();
    for payload in [
        "",
        "locron.install/v1\nstandalone",
        "locron.install/v2\nstandalone\n",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let fake = fake_binary(dir.path());
        fs::write(
            fake.parent().unwrap().join(".locron-install-receipt-v1"),
            payload,
        )
        .unwrap();
        Command::new(&fake)
            .args(["--json", "self-update"])
            .assert()
            .failure()
            .code(3)
            .stdout(predicate::str::contains("update_unowned_install"));
    }
}

#[cfg(unix)]
#[test]
fn symlink_receipt_refuses_self_update() {
    use std::os::unix::fs::symlink;

    let _serial = serialized();
    let dir = tempfile::tempdir().unwrap();
    let fake = fake_binary(dir.path());
    let receipt = fake.parent().unwrap().join(".locron-install-receipt-v1");
    let payload = dir.path().join("receipt-payload");
    fs::write(&payload, "locron.install/v1\nstandalone\n").unwrap();
    fs::remove_file(&receipt).unwrap();
    symlink(&payload, &receipt).unwrap();

    Command::new(&fake)
        .args(["--json", "self-update"])
        .assert()
        .failure()
        .code(3)
        .stdout(predicate::str::contains("update_unowned_install"));
}

#[test]
fn rate_limited_api_response_maps_to_a_stable_error() {
    let _serial = serialized();
    let fixture = Fixture::start(Box::new(|path: &str| {
        if path == "/repos/WhiteKiwi/locron/releases/latest" {
            (
                403,
                vec![
                    ("x-ratelimit-remaining".to_owned(), "0".to_owned()),
                    ("Content-Type".to_owned(), "application/json".to_owned()),
                ],
                b"{\"message\":\"API rate limit exceeded for 1.2.3.4\"}".to_vec(),
            )
        } else {
            (404, Vec::new(), b"not found".to_vec())
        }
    }));
    let dir = tempfile::tempdir().unwrap();
    let fake = fake_binary(dir.path());
    let service_log = dir.path().join("service.log");

    locron_command(&fake, &fixture, &service_log)
        .args(["--json", "self-update"])
        .assert()
        .failure()
        .code(5)
        .stdout(predicate::function(|stdout: &[u8]| {
            let envelope: Value = serde_json::from_slice(stdout).unwrap();
            assert_eq!(envelope["ok"], false);
            assert_eq!(envelope["error"]["code"], "update_rate_limited");
            true
        }));
}

#[test]
fn missing_published_asset_is_a_release_metadata_error() {
    let _serial = serialized();
    let latest = format!(r#"{{"tag_name":"{NEW_TAG}","assets":[{{"name":"SHA256SUMS.txt"}}]}}"#);
    let fixture = Fixture::start(Box::new(move |path: &str| {
        if path == "/repos/WhiteKiwi/locron/releases/latest" {
            json_response(&latest)
        } else {
            (404, Vec::new(), b"not found".to_vec())
        }
    }));
    let dir = tempfile::tempdir().unwrap();
    let fake = fake_binary(dir.path());
    let before = fs::read(&fake).unwrap();
    let service_log = dir.path().join("service.log");

    locron_command(&fake, &fixture, &service_log)
        .args(["--json", "self-update"])
        .assert()
        .failure()
        .code(5)
        .stdout(predicate::function(|stdout: &[u8]| {
            let envelope: Value = serde_json::from_slice(stdout).unwrap();
            assert_eq!(envelope["error"]["code"], "update_release_metadata");
            true
        }));

    assert_eq!(fs::read(&fake).unwrap(), before);
}

#[test]
fn malformed_checksum_entry_is_a_release_metadata_error() {
    let _serial = serialized();
    let target = expected_target();
    let malformed = format!("not-hex  locron-{NEW_TAG}-{target}.tar.gz\n");
    let fixture = update_fixture(&target, &malformed);
    let dir = tempfile::tempdir().unwrap();
    let fake = fake_binary(dir.path());
    let service_log = dir.path().join("service.log");

    locron_command(&fake, &fixture, &service_log)
        .args(["--json", "self-update"])
        .assert()
        .failure()
        .code(5)
        .stdout(predicate::function(|stdout: &[u8]| {
            let envelope: Value = serde_json::from_slice(stdout).unwrap();
            assert_eq!(envelope["error"]["code"], "update_release_metadata");
            true
        }));
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
