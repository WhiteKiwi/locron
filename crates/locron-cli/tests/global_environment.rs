#![cfg(unix)]

//! End-to-end global environment and executable-resolution contracts.

use std::os::unix::fs::PermissionsExt;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use locron_store::{DaemonLock, Store};

fn locron(state: &tempfile::TempDir) -> Command {
    let mut command = Command::new(assert_cmd::cargo::cargo_bin!("locron"));
    command.arg("--state-dir").arg(state.path());
    command
}

fn successful_output(command: &mut Command) -> std::process::Output {
    let output = command.output().expect("run locron");
    assert!(
        output.status.success(),
        "locron failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn start_daemon(state: &tempfile::TempDir) -> Child {
    let mut child = locron(state)
        .args(["daemon", "run"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("start daemon");
    let deadline = Instant::now() + Duration::from_secs(5);
    while DaemonLock::try_prove_free(&state.path().join("daemon.lock")).is_ok() {
        assert!(Instant::now() < deadline, "daemon did not acquire its lock");
        assert!(child.try_wait().unwrap().is_none(), "daemon exited early");
        thread::sleep(Duration::from_millis(20));
    }
    child
}

#[test]
fn config_environment_redacts_values_and_dry_run_is_read_only() {
    let state = tempfile::tempdir().unwrap();
    let secret = "global-secret-value";
    let dry_run = successful_output(locron(&state).args([
        "--json",
        "config",
        "set",
        "environment.TOKEN",
        secret,
        "--dry-run",
    ]));
    let dry_run = String::from_utf8(dry_run.stdout).unwrap();
    assert!(dry_run.contains("\"action\":\"created\""));
    assert!(!dry_run.contains(secret));
    assert!(!state.path().join("state.db").exists());

    let set = successful_output(locron(&state).args([
        "--json",
        "config",
        "set",
        "environment.TOKEN",
        secret,
    ]));
    let set = String::from_utf8(set.stdout).unwrap();
    assert!(set.contains("\"value_redacted\":true"));
    assert!(!set.contains(secret));

    let ordinary_set = successful_output(locron(&state).args([
        "--json",
        "config",
        "set",
        "global_concurrency",
        "17",
    ]));
    assert!(
        !String::from_utf8(ordinary_set.stdout)
            .unwrap()
            .contains(secret)
    );

    let doctor = successful_output(locron(&state).args(["--json", "doctor"]));
    assert!(!String::from_utf8(doctor.stdout).unwrap().contains(secret));

    let get = successful_output(locron(&state).args(["--json", "config", "get"]));
    let get = String::from_utf8(get.stdout).unwrap();
    assert!(get.contains("TOKEN"));
    assert!(!get.contains(secret));

    let one =
        successful_output(locron(&state).args(["--json", "config", "get", "environment.TOKEN"]));
    let one = String::from_utf8(one.stdout).unwrap();
    assert!(one.contains("\"configured\":true"));
    assert!(!one.contains(secret));

    let unset =
        successful_output(locron(&state).args(["--json", "config", "unset", "environment.TOKEN"]));
    assert!(
        String::from_utf8(unset.stdout)
            .unwrap()
            .contains("\"action\":\"removed\"")
    );
    let unchanged =
        successful_output(locron(&state).args(["--json", "config", "unset", "environment.TOKEN"]));
    assert!(
        String::from_utf8(unchanged.stdout)
            .unwrap()
            .contains("\"action\":\"unchanged\"")
    );

    let reserved = locron(&state)
        .args(["config", "set", "environment.LOCRON_RUN_ID", secret])
        .output()
        .unwrap();
    assert_eq!(reserved.status.code(), Some(2));
    assert!(!String::from_utf8_lossy(&reserved.stdout).contains(secret));
    assert!(!String::from_utf8_lossy(&reserved.stderr).contains(secret));
}

#[test]
fn plaintext_export_import_round_trips_global_environment_only_with_acknowledgement() {
    let source = tempfile::tempdir().unwrap();
    let destination = tempfile::tempdir().unwrap();
    let secret = "exported-global-secret";
    successful_output(locron(&source).args(["config", "set", "environment.API_TOKEN", secret]));

    let redacted = successful_output(locron(&source).arg("export"));
    let redacted = String::from_utf8(redacted.stdout).unwrap();
    assert!(!redacted.contains(secret));
    assert!(redacted.contains("settings.environment.API_TOKEN"));

    let plaintext = successful_output(locron(&source).args([
        "export",
        "--include-values",
        "--acknowledge-plaintext",
    ]));
    let plaintext = String::from_utf8(plaintext.stdout).unwrap();
    assert!(plaintext.contains(secret));
    let document = source.path().join("plaintext.json");
    std::fs::write(&document, plaintext).unwrap();

    successful_output(
        locron(&destination)
            .arg("import")
            .arg(&document)
            .arg("--accept-plaintext-values"),
    );
    let round_trip = successful_output(locron(&destination).args([
        "export",
        "--include-values",
        "--acknowledge-plaintext",
    ]));
    assert!(
        String::from_utf8(round_trip.stdout)
            .unwrap()
            .contains(secret)
    );
}

#[test]
fn execution_layers_global_environment_and_persists_the_resolved_executable() {
    let state = tempfile::tempdir().unwrap();
    let bin = state.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    let executable = bin.join("layer-probe");
    std::fs::write(
        &executable,
        "#!/bin/sh\nprintf '%s|%s|%s|%s\\n' \"$LAYER\" \"$GLOBAL_ONLY\" \"$FILE_ONLY\" \"$LOCRON_RUN_ID\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    let env_file = state.path().join("job.env");
    std::fs::write(&env_file, "LAYER=file\nFILE_ONLY=from-file\n").unwrap();

    successful_output(locron(&state).args([
        "config",
        "set",
        "environment.PATH",
        state.path().join("global-path-must-lose").to_str().unwrap(),
    ]));
    successful_output(locron(&state).args(["config", "set", "environment.LAYER", "global"]));
    successful_output(locron(&state).args([
        "config",
        "set",
        "environment.GLOBAL_ONLY",
        "from-global",
    ]));
    successful_output(
        locron(&state)
            .args(["add", "layered", "--every", "1h", "--env-file"])
            .arg(&env_file)
            .args(["--env", "LAYER=inline", "--path"])
            .arg(&bin)
            .args(["--", "layer-probe"]),
    );
    let run = successful_output(locron(&state).args(["--json", "run", "layered"]));
    let envelope: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    let run_id = envelope["data"]["run_id"].as_str().unwrap().to_owned();

    successful_output(
        locron(&state)
            .args(["add", "absolute", "--every", "1h", "--"])
            .arg(&executable),
    );
    let absolute_run = successful_output(locron(&state).args(["--json", "run", "absolute"]));
    let absolute_run: serde_json::Value = serde_json::from_slice(&absolute_run.stdout).unwrap();
    let absolute_run = absolute_run["data"]["run_id"].as_str().unwrap().to_owned();

    successful_output(locron(&state).args(["add", "shell", "--every", "1h", "--shell", "true"]));
    let shell_run = successful_output(locron(&state).args(["--json", "run", "shell"]));
    let shell_run: serde_json::Value = serde_json::from_slice(&shell_run.stdout).unwrap();
    let shell_run = shell_run["data"]["run_id"].as_str().unwrap().to_owned();

    successful_output(locron(&state).args([
        "add",
        "missing",
        "--every",
        "1h",
        "--",
        "definitely-not-a-locron-executable",
    ]));
    let missing_run = successful_output(locron(&state).args(["--json", "run", "missing"]));
    let missing_run: serde_json::Value = serde_json::from_slice(&missing_run.stdout).unwrap();
    let missing_run = missing_run["data"]["run_id"].as_str().unwrap().to_owned();

    let mut daemon = start_daemon(&state);
    let store = Store::open_read_only(&state.path().join("state.db")).unwrap();
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let run = store.run(&run_id).unwrap();
        if run.state == "succeeded" {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "run did not succeed: {}",
            run.state
        );
        thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(
        store
            .attempt_resolved_executable(&run_id, 1)
            .unwrap()
            .as_deref(),
        Some(executable.to_str().unwrap())
    );
    let logs = successful_output(locron(&state).args(["logs", &run_id]));
    let logs = String::from_utf8(logs.stdout).unwrap();
    assert!(logs.contains(&format!("inline|from-global|from-file|{run_id}")));

    wait_for_terminal(&store, &absolute_run, "succeeded");
    assert_eq!(
        store
            .attempt_resolved_executable(&absolute_run, 1)
            .unwrap()
            .as_deref(),
        Some(executable.to_str().unwrap())
    );

    wait_for_terminal(&store, &shell_run, "succeeded");
    assert_eq!(
        store
            .attempt_resolved_executable(&shell_run, 1)
            .unwrap()
            .as_deref(),
        Some("/bin/sh")
    );

    wait_for_terminal(&store, &missing_run, "failed");
    assert_eq!(
        store.attempt_resolved_executable(&missing_run, 1).unwrap(),
        None
    );
    assert!(
        state
            .path()
            .join("outputs")
            .join(&missing_run)
            .join("1.log")
            .is_file()
    );

    daemon.kill().unwrap();
    daemon.wait().unwrap();
}

fn wait_for_terminal(store: &Store, run_id: &str, expected: &str) {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let run = store.run(run_id).unwrap();
        if run.state == expected {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "run did not reach {expected}: {}",
            run.state
        );
        thread::sleep(Duration::from_millis(20));
    }
}
