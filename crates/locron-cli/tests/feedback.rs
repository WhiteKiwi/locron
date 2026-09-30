//! Isolated regression contracts for the first user feedback triage.
use locron_store::{StatePaths, Store};
use serde_json::Value;
use std::process::Command;

fn output(state: &tempfile::TempDir, args: &[&str]) -> std::process::Output {
    let out = Command::new(assert_cmd::cargo::cargo_bin!("locron"))
        .arg("--state-dir")
        .arg(state.path())
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    out
}
fn json(state: &tempfile::TempDir, args: &[&str]) -> Value {
    let mut arguments = vec!["--json"];
    arguments.extend(args);
    serde_json::from_slice(&output(state, &arguments).stdout).unwrap()
}

#[test]
fn registration_warnings_are_advisory_and_dry_runs_are_read_only() {
    let state = tempfile::tempdir().unwrap();
    let args = [
        "add",
        "probe",
        "--every",
        "10m",
        "--dry-run",
        "--",
        "locron-feedback-missing-binary",
    ];
    let dry = json(&state, &args);
    assert!(
        dry["warnings"][0]
            .as_str()
            .unwrap()
            .contains("could not be resolved")
    );
    assert_eq!(std::fs::read_dir(state.path()).unwrap().count(), 0);
    let live = json(
        &state,
        &[
            "add",
            "probe",
            "--every",
            "10m",
            "--",
            "locron-feedback-missing-binary",
        ],
    );
    assert_eq!(live["warnings"], dry["warnings"]);
    let before = json(&state, &["show", "probe"]);
    let dry_update = json(
        &state,
        &["update", "probe", "--description", "updated", "--dry-run"],
    );
    assert_eq!(dry_update["warnings"], dry["warnings"]);
    assert_eq!(before, json(&state, &["show", "probe"]));
    let live_update = output(&state, &["update", "probe", "--description", "updated"]);
    assert!(String::from_utf8_lossy(&live_update.stderr).contains("warning: process executable"));
    assert!(String::from_utf8_lossy(&live_update.stdout).contains("job updated: probe"));
    let resolved = json(
        &state,
        &["add", "resolved", "--every", "1h", "--", "/usr/bin/true"],
    );
    assert_eq!(resolved["warnings"], serde_json::json!([]));
}

#[test]
fn http_hint_is_narrow_and_never_repeats_urls_or_environment_errors() {
    let state = tempfile::tempdir().unwrap();
    // Explicit empty PATH makes this independent of installed http clients.
    let base = [
        "add",
        "http-probe",
        "--every",
        "1h",
        "--path",
        "",
        "--dry-run",
        "--",
        "http",
    ];
    for (method, url, hint) in [
        (
            "POST",
            "https://user:secret@example.com/private?token=hidden",
            true,
        ),
        ("post", "https://example.com", false),
        ("OPTIONS", "https://example.com", false),
        ("POST", "file:///private", false),
        ("POST", "not-a-url", false),
    ] {
        let mut args = base.to_vec();
        args.extend([method, url]);
        let result = json(&state, &args);
        let warnings = result["warnings"].to_string();
        assert_eq!(warnings.contains("did you mean --http"), hint);
        assert!(!warnings.contains(url));
        assert!(!warnings.contains("secret"));
    }
    let file = state.path().join("malformed.env");
    std::fs::write(&file, "secret-value-is-not-an-environment-line").unwrap();
    let result = json(
        &state,
        &[
            "add",
            "bad-env",
            "--every",
            "1h",
            "--env-file",
            file.to_str().unwrap(),
            "--path",
            "",
            "--dry-run",
            "--",
            "http",
            "POST",
            "https://example.com",
        ],
    );
    let text = result.to_string();
    assert!(text.contains("effective job environment unavailable"));
    assert!(!text.contains("secret-value"));
    assert!(!result["warnings"].to_string().contains("did you mean"));
    assert!(!StatePaths::new(state.path().into()).database.exists());
}

#[test]
fn advisory_resolution_uses_environment_precedence_and_cwd() {
    let state = tempfile::tempdir().unwrap();
    let bin = state.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    std::fs::write(bin.join("http"), "fixture").unwrap();
    let env_file = state.path().join("job.env");
    std::fs::write(&env_file, "PATH=bin\n").unwrap();
    let result = json(
        &state,
        &[
            "add",
            "relative",
            "--every",
            "1h",
            "--cwd",
            state.path().to_str().unwrap(),
            "--path",
            "",
            "--env-file",
            env_file.to_str().unwrap(),
            "--dry-run",
            "--",
            "http",
            "POST",
            "https://example.com",
        ],
    );
    assert!(!result["warnings"].to_string().contains("process"));
    assert!(!result["warnings"].to_string().contains("did you mean"));
    let overridden = json(
        &state,
        &[
            "add",
            "override",
            "--every",
            "1h",
            "--cwd",
            state.path().to_str().unwrap(),
            "--env-file",
            env_file.to_str().unwrap(),
            "--env",
            "PATH=",
            "--dry-run",
            "--",
            "http",
        ],
    );
    assert!(
        overridden["warnings"]
            .to_string()
            .contains("could not be resolved")
    );
    json(
        &state,
        &["config", "set", "environment.PATH", bin.to_str().unwrap()],
    );
    let global = json(&state, &["add", "global", "--every", "1h", "--", "http"]);
    assert_eq!(global["warnings"], serde_json::json!([]));
    let doctor = json(&state, &["doctor"]);
    assert_eq!(
        doctor["data"]["process_resolution"][0]["status"],
        "resolved"
    );
}

#[test]
fn list_reports_latest_identity_and_state_without_changing_show() {
    let state = tempfile::tempdir().unwrap();
    json(
        &state,
        &["add", "probe", "--every", "1h", "--", "/usr/bin/true"],
    );
    assert_eq!(
        json(&state, &["list"])["data"][0]["latest_run"],
        Value::Null
    );
    let run = json(&state, &["run", "probe"]);
    let id = run["data"]["run_id"].as_str().unwrap();
    let list = json(&state, &["list"]);
    assert_eq!(
        list["data"][0]["latest_run"],
        serde_json::json!({"id":id,"state":"queued"})
    );
    let table = output(&state, &["list"]);
    let table = String::from_utf8(table.stdout).unwrap();
    assert!(table.contains("LAST RUN"));
    assert!(table.contains("yes     queued"));
    assert!(
        json(&state, &["show", "probe"])["data"]
            .get("latest_run")
            .is_none()
    );
    let store = Store::open(StatePaths::new(state.path().into()), "test", 1).unwrap();
    store.cancel(id, 2).unwrap();
    assert_eq!(
        json(&state, &["list"])["data"][0]["latest_run"]["state"],
        "cancelled"
    );
}
