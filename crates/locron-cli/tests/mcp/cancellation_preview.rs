//! Real MCP requests and migrated Store fixtures; no target process is launched.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use locron_core::filesystem::DirectoryGuard;
use locron_store::{AttemptCompletion, RetryPlan, StatePaths, Store};
use serde_json::{Value, json};

use super::{McpClient, add_job_arguments, tool_json};

const RUN: &str = "00000000-0000-7000-8000-000000000001";
const LIFETIME: &str = "00000000-0000-7000-8000-000000000002";

struct Fixture {
    client: McpClient,
    paths: StatePaths,
    _guard: DirectoryGuard,
    _temporary: tempfile::TempDir,
}

impl Fixture {
    fn new(state: &str, requested: bool) -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let guard = DirectoryGuard::private(&temporary.path().join("private")).unwrap();
        let paths = StatePaths::new(guard.normalized_path().to_path_buf());
        let mut client = McpClient::spawn(&paths.root);
        client.call_tool("locron_add_job", add_job_arguments("seed"));
        let store = Store::open(paths.clone(), env!("CARGO_PKG_VERSION"), 10).unwrap();
        store
            .begin_lifetime(LIFETIME, 10, env!("CARGO_PKG_VERSION"))
            .unwrap();
        store.enqueue_manual("seed", RUN, 20).unwrap();
        if state != "queued" {
            let admitted = store.admit(LIFETIME, 30, 1).unwrap();
            assert_eq!(admitted.attempts.len(), 1);
            assert_eq!(admitted.attempts[0].run_id, RUN);
            if state != "starting" {
                store.mark_attempt_running(RUN, 1, 40).unwrap();
            }
            if requested {
                store.cancel(RUN, 45).unwrap();
            }
            if !["starting", "running"].contains(&state) {
                let final_state = match state {
                    "quarantine" => "termination_unconfirmed",
                    "retry_wait" => "failed",
                    other => other,
                };
                store
                    .complete_attempt(&AttemptCompletion {
                        run_id: RUN.into(),
                        attempt_number: 1,
                        now_us: 50,
                        duration_us: 10,
                        state: final_state.into(),
                        exit_code: None,
                        http_status: None,
                        http_content_type: None,
                        reason: "test-owned completion".into(),
                        retry: (state == "retry_wait").then(|| RetryPlan {
                            not_before_us: 1000,
                            classification: "known_failure".into(),
                        }),
                    })
                    .unwrap();
            }
        } else {
            assert!(!requested, "public queued fixture has no prior request");
        }
        let run = store.run(RUN).unwrap();
        assert_eq!(
            run.state,
            if state == "quarantine" {
                "running"
            } else {
                state
            }
        );
        // Cancelled completion itself records cancellation, even when the
        // fixture did not issue an earlier starting/running request.
        assert_eq!(
            store.cancellation_requested(RUN).unwrap(),
            requested || state == "cancelled",
            "unexpected setup request state: {state}/{requested}"
        );
        drop(store); // End the fixture writer before checking immutable preview bytes.
        Self {
            client,
            paths,
            _guard: guard,
            _temporary: temporary,
        }
    }

    fn call(&mut self, dry_run: bool, acknowledge: bool) -> Value {
        self.client.request(
            "tools/call",
            json!({"name":"locron_cancel_run", "arguments":{
                "run_id":RUN, "dry_run":dry_run, "acknowledge_unconfirmed":acknowledge
            }}),
        )
    }
}

fn files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn collect(root: &Path, directory: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in std::fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            let metadata = std::fs::symlink_metadata(&path).unwrap();
            assert!(!metadata.file_type().is_symlink());
            if metadata.is_dir() {
                collect(root, &path, out);
            } else {
                assert!(metadata.is_file());
                out.insert(
                    path.strip_prefix(root).unwrap().to_owned(),
                    std::fs::read(&path).unwrap(),
                );
            }
        }
    }
    let mut out = BTreeMap::new();
    collect(root, root, &mut out);
    out
}

fn data(response: &Value) -> Value {
    assert!(response.get("error").is_none(), "{response}");
    let decoded = tool_json(response);
    assert!(decoded.is_object(), "unexpected tool object: {response}");
    decoded
}

fn compare_case(state: &str, requested: bool, acknowledge: bool) {
    let mut preview = Fixture::new(state, requested);
    let mut live = Fixture::new(state, requested);
    let before = files(&preview.paths.root);
    assert!(!before.is_empty());
    let observed = preview.call(true, acknowledge);
    assert_eq!(
        files(&preview.paths.root),
        before,
        "preview changed durable files: {state}/{requested}/{acknowledge}"
    );
    let executed = live.call(false, acknowledge);
    if executed["result"]["isError"] == true {
        assert_eq!(observed["result"]["isError"], true, "{observed}");
        assert_eq!(observed["result"]["content"], executed["result"]["content"]);
        return;
    }
    let observed = data(&observed);
    let executed = data(&executed);
    assert_eq!(observed["dry_run"], true);
    assert_eq!(observed["run_id"], RUN);
    assert_eq!(observed["already_requested"], requested);
    let before_execution = ["queued", "retry_wait"].contains(&state);
    let risk = state == "quarantine";
    assert_eq!(observed["would_cancel_before_execution"], before_execution);
    assert_eq!(observed["would_acknowledge_unconfirmed"], risk);
    assert_eq!(
        observed["would_request_cancellation"],
        before_execution || (!risk && !requested)
    );
    let decision = if risk {
        "acknowledged_unconfirmed"
    } else if before_execution {
        "cancelled_before_execution"
    } else if requested {
        "already_requested"
    } else {
        "cancellation_requested"
    };
    assert_eq!(observed["decision"], decision);
    if risk {
        assert_eq!(
            executed["acknowledged_unconfirmed"], true,
            "live acknowledgement: {state}/{requested}/{acknowledge}: {executed}"
        );
        assert!(executed.get("requested").is_none(), "{executed}");
    } else {
        assert_eq!(
            executed["requested"], true,
            "live request: {state}/{requested}/{acknowledge}: {executed}"
        );
        if before_execution {
            assert_eq!(executed["cancelled"], true, "{executed}");
            assert_eq!(executed["before_execution"], true, "{executed}");
        }
    }
    let actual = Store::open_read_only(&live.paths.database)
        .unwrap()
        .run(RUN)
        .unwrap_or_else(|error| panic!("live row: {state}/{requested}/{acknowledge}: {error}; response={executed}"));
    assert_eq!(observed["resulting_state"], actual.state);
}

#[test]
fn ordinary_state_previews_match_independent_live_store_outcomes() {
    for state in ["queued", "starting", "running", "retry_wait"] {
        for requested in [false, true] {
            if state == "queued" && requested {
                continue;
            }
            for acknowledge in [false, true] {
                compare_case(state, requested, acknowledge);
            }
        }
    }
}

#[test]
fn quarantine_acknowledgement_is_not_a_fresh_cancellation_or_process_death_claim() {
    for requested in [false, true] {
        for acknowledge in [false, true] {
            compare_case("quarantine", requested, acknowledge);
        }
    }
}

#[test]
fn terminal_refusals_match_live_and_never_create_preview_side_effects() {
    for state in ["succeeded", "failed", "cancelled"] {
        for requested in [false, true] {
            for acknowledge in [false, true] {
                compare_case(state, requested, acknowledge);
            }
        }
    }
}

#[test]
fn changed_state_after_preview_is_rechecked_by_the_live_transaction() {
    let mut fixture = Fixture::new("queued", false);
    assert_eq!(
        data(&fixture.call(true, false))["decision"],
        "cancelled_before_execution"
    );
    let store = Store::open(fixture.paths.clone(), env!("CARGO_PKG_VERSION"), 30).unwrap();
    assert_eq!(store.admit(LIFETIME, 30, 1).unwrap().attempts.len(), 1);
    store.mark_attempt_running(RUN, 1, 40).unwrap();
    store
        .complete_attempt(&AttemptCompletion {
            run_id: RUN.into(),
            attempt_number: 1,
            now_us: 50,
            duration_us: 10,
            state: "termination_unconfirmed".into(),
            exit_code: None,
            http_status: None,
            http_content_type: None,
            reason: "test-owned quarantine".into(),
            retry: None,
        })
        .unwrap();
    assert_eq!(store.run(RUN).unwrap().state, "running");
    drop(store);
    let refused = fixture.call(false, false);
    assert_eq!(refused["result"]["isError"], true, "{refused}");
    assert!(
        refused["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("--acknowledge-unconfirmed"),
        "{refused}"
    );
    let store = Store::open_read_only(&fixture.paths.database).unwrap();
    let run = store.run(RUN).unwrap_or_else(|error| {
        panic!("quarantined row after refused cancellation: {error}; response={refused}")
    });
    assert_eq!(run.state, "running");
    assert_eq!(run.reason.as_deref(), Some("termination_unconfirmed"));
}

#[test]
fn missing_run_preview_is_a_tool_error_without_file_changes() {
    let mut fixture = Fixture::new("queued", false);
    let before = files(&fixture.paths.root);
    let response = fixture.client.request(
        "tools/call",
        json!({"name":"locron_cancel_run", "arguments":{
            "run_id":"00000000-0000-7000-8000-000000000099", "dry_run":true
        }}),
    );
    assert_eq!(response["result"]["isError"], true);
    assert!(
        response["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("not found")
    );
    assert_eq!(files(&fixture.paths.root), before);
}
