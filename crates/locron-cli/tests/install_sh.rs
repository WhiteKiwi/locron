//! `install.sh` contract tests against a local file:// release fixture.
//!
//! The fixture serves a fake release: a gzip tarball whose `locron` binary is
//! a tiny shell script that reports its version and records a `service install`
//! invocation. `LOCRON_UPDATE_ASSET_BASE=file://...` keeps the installer fully
//! offline, and `LOCRON_FIXTURE_MODE` steers the recorded registration between
//! success, the no-session guidance exit, and a hard failure.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;

use flate2::Compression;
use flate2::write::GzEncoder;
use sha2::{Digest, Sha256};

const NEW_TAG: &str = "v9.9.9";

/// The fixture's replacement binary. Reports its version to `-V`, and for
/// `service install` either records the invocation, prints no-session guidance
/// and exits zero, or fails, according to `$LOCRON_FIXTURE_MODE`.
const FIXTURE_BINARY: &[u8] = b"#!/bin/sh\n\
if [ \"${1:-}\" = \"-V\" ]; then\n\
    echo fixture-locron 9.9.9\n\
    exit 0\n\
fi\n\
if [ \"${1:-}\" = \"service\" ] && [ \"${2:-}\" = \"install\" ]; then\n\
    case \"${LOCRON_FIXTURE_MODE:-register}\" in\n\
        no-session)\n\
            echo \"locron: no login session; run 'locron service install' in a session\" >&2\n\
            exit 0\n\
            ;;\n\
        fail)\n\
            exit 5\n\
            ;;\n\
        *)\n\
            echo \"service install\" >> \"${LOCRON_FIXTURE_SERVICE_LOG:?}\"\n\
            exit 0\n\
            ;;\n\
    esac\n\
fi\n\
exit 0\n";

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

/// A gzip tarball containing `locron-{tag}-{target}/locron` with `FIXTURE_BINARY`.
fn fixture_tarball(tag: &str, target: &str) -> Vec<u8> {
    let mut tar = Vec::new();
    {
        let mut builder = tar::Builder::new(&mut tar);
        let mut directory = tar::Header::new_gnu();
        directory.set_mode(0o755);
        directory.set_entry_type(tar::EntryType::Directory);
        directory.set_size(0);
        builder
            .append_data(
                &mut directory,
                format!("locron-{tag}-{target}"),
                std::io::empty(),
            )
            .unwrap();

        let mut file = tar::Header::new_gnu();
        file.set_mode(0o755);
        file.set_entry_type(tar::EntryType::Regular);
        file.set_size(FIXTURE_BINARY.len() as u64);
        builder
            .append_data(
                &mut file,
                format!("locron-{tag}-{target}/locron"),
                FIXTURE_BINARY,
            )
            .unwrap();
        builder.finish().unwrap();
    }
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&tar).unwrap();
    encoder.finish().unwrap()
}

/// A `file://` release host: `releases/download/{tag}/` with the checksums file
/// and the target tarball.
struct ReleaseHost {
    root: PathBuf,
}

fn release_host(dir: &Path) -> ReleaseHost {
    let target = expected_target();
    let download_dir = dir.join("releases/download").join(NEW_TAG);
    fs::create_dir_all(&download_dir).unwrap();
    let tarball_name = format!("locron-{NEW_TAG}-{target}.tar.gz");
    let tarball = fixture_tarball(NEW_TAG, &target);
    fs::write(download_dir.join(&tarball_name), &tarball).unwrap();
    let checksum = format!("{:x}", Sha256::digest(&tarball));
    fs::write(
        download_dir.join("SHA256SUMS.txt"),
        format!("{checksum}  {tarball_name}\n"),
    )
    .unwrap();
    ReleaseHost {
        root: dir.to_path_buf(),
    }
}

/// Serializes the suite's tests: every test forks shell processes and writes
/// to shared temp areas under parallel load.
fn serialized() -> std::sync::MutexGuard<'static, ()> {
    static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());
    SERIAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The installer script under test.
fn installer() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../install.sh")
        .canonicalize()
        .expect("install.sh must exist at the repository root")
}

/// Run the installer against the fixture with the given extra environment.
fn run_installer(
    dir: &Path,
    host: &ReleaseHost,
    mode: &str,
    no_service: bool,
) -> std::process::Output {
    let install_path = dir.join("inst/bin/locron");
    let service_log = dir.join("service.log");
    let mut command = StdCommand::new("/bin/sh");
    command.arg(installer());
    command
        .env("LOCRON_INSTALL_DIR", &install_path)
        .env("LOCRON_VERSION", NEW_TAG)
        .env(
            "LOCRON_UPDATE_ASSET_BASE",
            format!("file://{}", host.root.display()),
        )
        .env("LOCRON_FIXTURE_MODE", mode)
        .env("LOCRON_FIXTURE_SERVICE_LOG", &service_log);
    if no_service {
        command.env("LOCRON_NO_SERVICE", "1");
    }
    command.output().expect("install.sh must run")
}

fn installed_binary(dir: &Path) -> PathBuf {
    dir.join("inst/bin/locron")
}

fn service_log_entries(dir: &Path) -> Vec<String> {
    fs::read_to_string(dir.join("service.log"))
        .map(|text| text.lines().map(str::to_owned).collect())
        .unwrap_or_default()
}

#[test]
fn install_sh_registers_a_login_service_after_the_replace() {
    let _serial = serialized();
    let dir = tempfile::tempdir().unwrap();
    let host = release_host(dir.path());
    let output = run_installer(dir.path(), &host, "register", false);

    assert!(
        output.status.success(),
        "install.sh must succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Installed locron v9.9.9 to"),
        "the report must name the installed version: {stdout}"
    );
    assert!(
        installed_binary(dir.path()).exists(),
        "the binary must be installed"
    );
    assert_eq!(
        service_log_entries(dir.path()),
        ["service install"],
        "the installer must attempt service registration after the replace"
    );
}

#[test]
fn install_sh_locron_no_service_skips_registration() {
    let _serial = serialized();
    let dir = tempfile::tempdir().unwrap();
    let host = release_host(dir.path());
    let output = run_installer(dir.path(), &host, "register", true);

    assert!(
        output.status.success(),
        "install.sh must succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        installed_binary(dir.path()).exists(),
        "the binary must be installed"
    );
    assert_eq!(
        service_log_entries(dir.path()),
        Vec::<String>::new(),
        "LOCRON_NO_SERVICE=1 must skip the registration attempt"
    );
}

#[test]
fn install_sh_tolerates_a_guidance_exit_from_registration() {
    let _serial = serialized();
    let dir = tempfile::tempdir().unwrap();
    let host = release_host(dir.path());
    let output = run_installer(dir.path(), &host, "no-session", false);

    assert!(
        output.status.success(),
        "the no-session guidance exit must leave the install successful: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("no login session"),
        "the guidance output must pass through: {stderr}"
    );
    assert!(
        !stderr.contains("warning: could not register"),
        "a guidance exit zero must not warn: {stderr}"
    );
}

#[test]
fn install_sh_warns_and_continues_when_registration_fails() {
    let _serial = serialized();
    let dir = tempfile::tempdir().unwrap();
    let host = release_host(dir.path());
    let output = run_installer(dir.path(), &host, "fail", false);

    assert!(
        output.status.success(),
        "a failed registration must leave the install successful: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("warning: could not register locron as a login service (exit 5)"),
        "the failure must warn and name the exit: {stderr}"
    );
    assert!(
        stderr.contains("locron service install"),
        "the warning must point at the retry command: {stderr}"
    );
}
