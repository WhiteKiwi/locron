//! Version flag contract tests.
//!
//! `-V/--version` must print `locron <version>` (or the machine-readable
//! envelope under `--format json` / `--json`) on stdout, exit 0, and never
//! touch the state directory. The flag is top-level only: subcommand
//! invocations reject it, and a bare or subcommand-less invocation keeps
//! clap's native help and missing-subcommand behavior.

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;

fn locron() -> Command {
    Command::cargo_bin("locron").unwrap()
}

fn expected_version() -> String {
    format!("locron {}\n", env!("CARGO_PKG_VERSION"))
}

/// Assert the standard `locron.cli/v1` envelope for the version command.
fn assert_version_envelope(output: &[u8]) {
    let envelope: Value = serde_json::from_slice(output).expect("stdout must be one JSON document");
    assert_eq!(envelope["schema"], "locron.cli/v1");
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["command"], "version");
    assert_eq!(envelope["data"]["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(
        envelope["warnings"],
        serde_json::json!([]),
        "version output must carry no warnings"
    );
}

#[test]
fn short_version_prints_plain_text() {
    locron()
        .arg("-V")
        .assert()
        .success()
        .stdout(expected_version())
        .stderr("");
}

#[test]
fn long_version_prints_plain_text() {
    locron()
        .arg("--version")
        .assert()
        .success()
        .stdout(expected_version())
        .stderr("");
}

#[test]
fn version_short_circuits_a_command() {
    locron()
        .args(["-V", "list"])
        .assert()
        .success()
        .stdout(expected_version())
        .stderr("");
}

#[test]
fn version_honors_format_json_flag_first() {
    locron()
        .args(["--format", "json", "--version"])
        .assert()
        .success()
        .stderr("")
        .stdout(predicate::function(|stdout: &[u8]| {
            assert_version_envelope(stdout);
            true
        }));
}

#[test]
fn version_honors_format_json_flag_last() {
    locron()
        .args(["-V", "--format", "json"])
        .assert()
        .success()
        .stderr("")
        .stdout(predicate::function(|stdout: &[u8]| {
            assert_version_envelope(stdout);
            true
        }));
}

#[test]
fn version_honors_json_alias() {
    locron()
        .args(["--json", "-V"])
        .assert()
        .success()
        .stderr("")
        .stdout(predicate::function(|stdout: &[u8]| {
            assert_version_envelope(stdout);
            true
        }));
}

#[test]
fn version_never_accesses_the_state_directory() {
    let base = tempfile::tempdir().unwrap();
    let state = base.path().join("state");
    assert!(!state.exists());

    locron()
        .env("LOCRON_STATE_DIR", &state)
        .arg("-V")
        .assert()
        .success()
        .stdout(expected_version())
        .stderr("");

    assert!(
        !state.exists(),
        "version must not initialize or discover the state directory"
    );

    // Also in machine-readable mode.
    locron()
        .env("LOCRON_STATE_DIR", &state)
        .args(["--json", "-V"])
        .assert()
        .success()
        .stdout(predicate::function(|stdout: &[u8]| {
            assert_version_envelope(stdout);
            true
        }));

    assert!(
        !state.exists(),
        "JSON version output must not initialize or discover the state directory"
    );
}

#[test]
fn bare_invocation_prints_full_help_on_stderr() {
    locron()
        .assert()
        .failure()
        .code(2)
        .stdout("")
        .stderr(predicate::str::contains(
            "Usage: locron [OPTIONS] <COMMAND>",
        ));
}

#[test]
fn subcommandless_invocation_reports_missing_subcommand() {
    locron()
        .arg("-v")
        .assert()
        .failure()
        .code(2)
        .stdout("")
        .stderr(predicate::str::contains("requires a subcommand"));
}

#[test]
fn version_flag_is_rejected_under_a_subcommand() {
    locron()
        .args(["add", "-V"])
        .assert()
        .failure()
        .code(2)
        .stdout("")
        .stderr(predicate::str::contains("unexpected argument '-V'"));
}
