//! Real MCP requests and migrated Store fixtures; no target process is launched.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use locron_core::filesystem::DirectoryGuard;
use locron_store::{AttemptCompletion, RetryPlan, StatePaths, Store};
use serde_json::{Value, json};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[path = "../support/history_logical.rs"]
mod logical;
use logical::{Cell, Logical, checked};

use super::{McpClient, add_job_arguments};

const RUN: &str = "00000000-0000-7000-8000-000000000001";
const LIFETIME: &str = "00000000-0000-7000-8000-000000000002";

struct Fixture {
    client: McpClient,
    paths: StatePaths,
    logical_before: Logical,
    deadline: Instant,
    _guard: DirectoryGuard,
    _temporary: tempfile::TempDir,
}

impl Fixture {
    fn new(state: &str, requested: bool) -> Self {
        let deadline = Instant::now() + Duration::from_secs(180);
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
        if state == "queued" {
            assert!(!requested, "public queued fixture has no prior request");
        } else {
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
        // Completion preserves the prior request flag; only the fixture's
        // explicit starting/running cancellation sets it.
        assert_eq!(
            store.cancellation_requested(RUN).unwrap(),
            requested,
            "unexpected setup request state: {state}/{requested}"
        );
        // Real foreign/output canaries make physical preservation non-vacuous.
        let output_guard = checked(DirectoryGuard::private(
            &paths.outputs.join("qualification"),
        ));
        for path in [
            paths.root.join("foreign.keep"),
            output_guard.normalized_path().join("sentinel.log"),
        ] {
            let mut file = checked(locron_core::filesystem::create_private_new(&path));
            checked(std::io::Write::write_all(
                &mut *file,
                b"test-owned physical canary",
            ));
            checked(file.sync_all());
        }
        drop(output_guard);
        let logical_before = logical::logical(&paths);
        drop(store); // Close every independent reader before the original final writer close.
        assert!(
            Instant::now() + Duration::from_secs(10) < deadline,
            "fixture preparation exhausted horizon"
        );
        Self {
            client,
            paths,
            logical_before,
            deadline,
            _guard: guard,
            _temporary: temporary,
        }
    }

    fn call(&mut self, dry_run: bool, acknowledge: bool) -> Value {
        assert!(
            Instant::now() + Duration::from_secs(10) < self.deadline,
            "fixture operation exhausted horizon"
        );
        let started = Instant::now();
        let response = self.client.request(
            "tools/call",
            json!({"name":"locron_cancel_run", "arguments":{
                "run_id":RUN, "dry_run":dry_run, "acknowledge_unconfirmed":acknowledge
            }}),
        );
        assert!(
            started.elapsed() <= Duration::from_secs(60),
            "fixture operation exceeded 60s"
        );
        assert!(
            Instant::now() + Duration::from_secs(10) < self.deadline,
            "fixture operation exhausted horizon"
        );
        response
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
    assert_ne!(response["result"]["isError"], true, "{response}");
    let text = response["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("missing tool text: {response}"));
    let decoded: Value = serde_json::from_str(text)
        .unwrap_or_else(|error| panic!("invalid tool JSON: {error}; response={response}"));
    assert!(decoded.is_object(), "unexpected tool object: {response}");
    decoded
}

fn compare_case(state: &str, requested: bool, acknowledge: bool) {
    let mut preview = Fixture::new(state, requested);
    let mut live = Fixture::new(state, requested);
    let before = files(&preview.paths.root);
    assert!(!before.is_empty());
    let physical_before = logical::physical(&preview.paths);
    let observed = preview.call(true, acknowledge);
    let physical_immediately_after = logical::physical(&preview.paths);
    let _raw_immediately_after = files(&preview.paths.root);
    logical::unchanged_physical(&physical_before, &physical_immediately_after);
    let logical_after = logical::logical(&preview.paths);
    let _physical_after_oracle = logical::physical(&preview.paths);
    assert!(
        preview.logical_before == logical_after,
        "preview changed logical durable state"
    );
    let interval_start = now_us();
    let executed = live.call(false, acknowledge);
    let interval_end = now_us();
    let live_after = logical::logical(&live.paths);
    let expected_refusal = ["succeeded", "failed", "cancelled"].contains(&state)
        || acknowledge != (state == "quarantine");
    assert_eq!(
        observed["result"]["isError"] == true,
        expected_refusal,
        "preview admission disagreed with the fixed fixture contract"
    );
    assert_eq!(
        executed["result"]["isError"] == true,
        expected_refusal,
        "live admission disagreed with the fixed fixture contract"
    );
    if executed["result"]["isError"] == true {
        assert_eq!(observed["result"]["isError"], true, "{observed}");
        assert_eq!(observed["result"]["content"], executed["result"]["content"]);
        assert!(
            live.logical_before == live_after,
            "refused live cancellation changed logical state"
        );
        return;
    }
    cancellation_delta(
        &live.logical_before,
        &live_after,
        RUN,
        state,
        requested,
        acknowledge,
        (interval_start, interval_end),
    );
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
        .unwrap_or_else(|error| {
            panic!("live row: {state}/{requested}/{acknowledge}: {error}; response={executed}")
        });
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
    assert!(!before.is_empty());
    let physical_before = logical::physical(&fixture.paths);
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
    let physical_immediately_after = logical::physical(&fixture.paths);
    let _raw_immediately_after = files(&fixture.paths.root);
    logical::unchanged_physical(&physical_before, &physical_immediately_after);
    let logical_after = logical::logical(&fixture.paths);
    let _physical_after_oracle = logical::physical(&fixture.paths);
    assert!(
        fixture.logical_before == logical_after,
        "missing-run preview changed logical state"
    );
}

fn now_us() -> i64 {
    let micros = checked(SystemTime::now().duration_since(UNIX_EPOCH)).as_micros();
    checked(i64::try_from(micros))
}

fn cancellation_delta(
    before: &Logical,
    after: &Logical,
    run_id: &str,
    state: &str,
    requested: bool,
    acknowledged: bool,
    interval: (i64, i64),
) {
    let mut expected = before.clone();
    let key = Cell::text(run_id);
    let runs = &before.tables["runs"];
    let old = runs.single("id", &key);
    let job = old[runs.column("job_id")].clone();
    let immediately_cancelled = state == "queued" || state == "retry_wait";
    let risk = state == "quarantine" && acknowledged;
    let mut event = None;
    if immediately_cancelled || risk || !requested {
        let actual = after.tables["runs"].single("id", &key);
        let timestamp = actual[after.tables["runs"].column(if immediately_cancelled || risk {
            "finished_at_us"
        } else {
            "cancellation_requested_at_us"
        })]
        .integer();
        assert!(
            interval.0 <= timestamp && timestamp <= interval.1,
            "live timestamp escaped observed call interval"
        );
        if risk {
            expected.tables.get_mut("runs").unwrap().set(
                "id",
                &key,
                "state",
                Cell::text("interrupted_unknown"),
            );
            expected.tables.get_mut("runs").unwrap().set(
                "id",
                &key,
                "reason",
                Cell::text("termination unconfirmed; risk acknowledged by operator"),
            );
            event = Some((
                timestamp,
                "termination_unconfirmed_acknowledged",
                "{\"source\":\"user\",\"risk\":\"process_liveness_unconfirmed\"}",
            ));
        } else if immediately_cancelled {
            expected.tables.get_mut("runs").unwrap().set(
                "id",
                &key,
                "state",
                Cell::text("cancelled"),
            );
            expected.tables.get_mut("runs").unwrap().set(
                "id",
                &key,
                "reason",
                Cell::text("cancelled by user before execution"),
            );
            event = Some((
                timestamp,
                "run_cancelled",
                "{\"source\":\"user\",\"before_execution\":true}",
            ));
        } else {
            event = Some((timestamp, "cancellation_requested", "{\"source\":\"user\"}"));
        }
        if immediately_cancelled || risk {
            expected.tables.get_mut("runs").unwrap().set(
                "id",
                &key,
                "finished_at_us",
                Cell::Integer(timestamp),
            );
            expected.tables.get_mut("runs").unwrap().set(
                "id",
                &key,
                "replacement_candidate",
                Cell::Integer(0),
            );
            expected
                .tables
                .get_mut("retry_intents")
                .unwrap()
                .remove("run_id", &key);
        }
        if !risk {
            expected.tables.get_mut("runs").unwrap().set(
                "id",
                &key,
                "cancellation_requested_at_us",
                Cell::Integer(timestamp),
            );
            expected.tables.get_mut("runs").unwrap().set(
                "id",
                &key,
                "cancellation_reason",
                Cell::text("user"),
            );
        }
    }
    if let Some((timestamp, kind, details)) = event {
        let sequence = before.tables["sqlite_sequence"].single("name", &Cell::text("events"));
        let next = sequence[before.tables["sqlite_sequence"].column("seq")].integer() + 1;
        let events = expected.tables.get_mut("events").unwrap();
        assert!(
            events.columns.iter().map(String::as_str).eq([
                "id",
                "occurred_at_us",
                "kind",
                "job_id",
                "run_id",
                "details_json"
            ]),
            "event layout changed"
        );
        events.rows.push(vec![
            Cell::Integer(next),
            Cell::Integer(timestamp),
            Cell::text(kind),
            job,
            key,
            Cell::text(details),
        ]);
        events.rows.sort();
        expected.tables.get_mut("sqlite_sequence").unwrap().set(
            "name",
            &Cell::text("events"),
            "seq",
            Cell::Integer(next),
        );
    }
    assert!(
        expected == *after,
        "live cancellation changed an unselected logical value"
    );
}

#[test]
fn history_active_mcp_dry_why_preserve_full_state() {
    use locron_core::filesystem::open_private;
    use rusqlite::{Connection, OpenFlags, params};
    use std::fs::OpenOptions;

    const TERMINAL: [&str; 7] = [
        "succeeded",
        "failed",
        "timed_out",
        "cancelled",
        "skipped_overlap",
        "skipped_concurrency",
        "interrupted_unknown",
    ];
    const ACTIVE: [&str; 4] = ["queued", "starting", "running", "retry_wait"];
    for policy in ["skip", "replace", "allow"] {
        for active in [false, true] {
            let deadline = Instant::now() + Duration::from_secs(180);
            let temporary = checked(tempfile::tempdir());
            let guard = checked(DirectoryGuard::private(&temporary.path().join("private")));
            let paths = StatePaths::new(guard.normalized_path().to_path_buf());
            let mut client = McpClient::spawn(&paths.root);
            let mut args = add_job_arguments("ledger");
            args["overlap_policy"] = json!(policy);
            let created = client.call_tool("locron_add_job", args);
            let job_id = created["id"].as_str().expect("created id").to_owned();
            let writer = checked(Store::open(paths.clone(), env!("CARGO_PKG_VERSION"), 1));
            let definition = checked(writer.job(&job_id)).definition_json;
            let mut expected = Vec::new();
            {
                let leaf = checked(open_private(&paths.database, OpenOptions::new().read(true)));
                let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW;
                #[cfg(windows)]
                let mut connection = checked(Connection::open_with_flags_and_vfs(
                    leaf.normalized_path(),
                    flags,
                    "win32-longpath",
                ));
                #[cfg(unix)]
                let mut connection =
                    checked(Connection::open_with_flags(leaf.normalized_path(), flags));
                assert!(
                    !checked(connection.is_readonly(rusqlite::MAIN_DB)),
                    "seed writer fell back to readonly"
                );
                let transaction = checked(connection.transaction());
                for index in 0..(1205 + if active { 114 } else { 0 }) {
                    let id = format!("00000000-0000-7000-8000-{:012}", index + 1);
                    let state = if index < 1205 {
                        TERMINAL[index % 7]
                    } else {
                        ACTIVE[(index - 1205) % 4]
                    };
                    let time = if index < 150 {
                        10000 + (index / 2) as i64
                    } else if index < 1205 {
                        5000 + ((index - 150) / 2) as i64
                    } else {
                        100 + ((index - 1205) / 2) as i64
                    };
                    checked(transaction.execute("INSERT INTO runs(id,job_id,revision,trigger,requested_at_us,eligible_at_us,queue_sequence,snapshot_json,state,reason,finished_at_us) VALUES(?1,?2,1,'manual',?3,?3,?4,?5,?6,?7,?8)", params![id, job_id, time, (index + 1) as i64, definition, state, (state == "running").then_some("termination_unconfirmed"), TERMINAL.contains(&state).then_some(time + 1)]));
                    if index >= 1205 {
                        expected.push((time, id));
                    }
                }
                checked(transaction.commit());
                drop(connection);
                drop(leaf);
            }
            expected.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| right.1.cmp(&left.1)));
            let expected: Vec<_> = expected.into_iter().take(100).map(|(_, id)| id).collect();
            let before = logical::logical(&paths);
            drop(writer);
            assert!(
                Instant::now() + Duration::from_secs(10) < deadline,
                "MCP preparation exhausted horizon"
            );
            let physical_before = logical::physical(&paths);
            let started = Instant::now();
            let run = client.call_tool("locron_run_job", json!({"job":job_id,"dry_run":true}));
            let physical_immediately_after = logical::physical(&paths);
            logical::unchanged_physical(&physical_before, &physical_immediately_after);
            assert!(
                started.elapsed() <= Duration::from_secs(60),
                "MCP operation exceeded 60s"
            );
            assert_eq!(run["dry_run"], true);
            assert_eq!(run["eligible"], !active || policy != "skip");
            let decision = if !active {
                "eligible"
            } else {
                match policy {
                    "skip" => "would_skip_overlap",
                    "replace" => "would_replace",
                    _ => "eligible_subject_to_capacity",
                }
            };
            assert_eq!(run["decision"], decision);
            assert!(
                before == logical::logical(&paths),
                "MCP dry run changed full logical state"
            );
            let started = Instant::now();
            let why = client.call_tool("locron_why", json!({"job":job_id}));
            assert!(
                started.elapsed() <= Duration::from_secs(60),
                "MCP why exceeded 60s"
            );
            let ids: Vec<_> = why["active_runs"]
                .as_array()
                .expect("active sample")
                .iter()
                .map(|row| row["id"].as_str().expect("run id").to_owned())
                .collect();
            assert!(ids == expected, "MCP active ledger mismatch");
            assert!(
                before == logical::logical(&paths),
                "MCP why changed full logical state"
            );
            assert!(
                Instant::now() + Duration::from_secs(10) < deadline,
                "MCP operation exhausted horizon"
            );
            client.close_stdin();
            assert!(client.wait_exit().success(), "MCP did not close cleanly");
            drop(client);
            drop(guard);
            checked(temporary.close());
            assert!(Instant::now() < deadline, "MCP cleanup exceeded horizon");
        }
    }
}

#[test]
fn history_active_mcp_update_releases_readonly_owner_in_both_modes() {
    use locron_core::filesystem::open_private;
    use std::fs::OpenOptions;

    for populated_wal in [false, true] {
        let deadline = Instant::now() + Duration::from_secs(180);
        let temporary = checked(tempfile::tempdir());
        let guard = checked(DirectoryGuard::private(&temporary.path().join("private")));
        let paths = StatePaths::new(guard.normalized_path().to_path_buf());
        let mut client = McpClient::spawn(&paths.root);
        client.call_tool("locron_add_job", add_job_arguments("owner"));
        let writer = populated_wal
            .then(|| checked(Store::open(paths.clone(), env!("CARGO_PKG_VERSION"), 1)));
        for suffix in ["-wal", "-shm"] {
            let path = paths.root.join(format!("state.db{suffix}"));
            assert_eq!(
                path.exists(),
                populated_wal,
                "update cell had wrong actual journal mode"
            );
            if populated_wal {
                assert!(
                    checked(locron_core::filesystem::is_private(&path, false)),
                    "WAL cell was not private"
                );
            }
        }
        let leaf = checked(open_private(&paths.database, OpenOptions::new().read(true)));
        let original_id = logical::identity(&leaf);
        let started = Instant::now();
        let updated = client.call_tool(
            "locron_update_job",
            json!({"job":"owner","description":"owner-release"}),
        );
        assert!(
            started.elapsed() <= Duration::from_secs(60),
            "legitimate update exceeded 60s"
        );
        assert_eq!(updated["description"], "owner-release");
        assert_eq!(updated["current_revision"], 2);
        assert!(
            logical::identity(&leaf) == original_id,
            "legitimate update replaced the database"
        );
        drop(leaf);
        let before = logical::logical(&paths);
        let physical_before = logical::physical(&paths);
        let preview = client.call_tool(
            "locron_update_job",
            json!({"job":"owner","description":null,"dry_run":true}),
        );
        let physical_immediately_after = logical::physical(&paths);
        logical::unchanged_physical(&physical_before, &physical_immediately_after);
        assert_eq!(preview["dry_run"], true);
        assert!(preview["updated"]["description"].is_null());
        assert!(
            before == logical::logical(&paths),
            "update preview changed logical state"
        );
        let cleared = client.call_tool(
            "locron_update_job",
            json!({"job":"owner","description":null}),
        );
        assert!(cleared["description"].is_null());
        assert_eq!(cleared["current_revision"], 3);
        // The public Store's original optimistic-revision admission remains independent.
        let stale_before = logical::logical(&paths);
        let store = checked(Store::open(paths.clone(), env!("CARGO_PKG_VERSION"), 1));
        let job = checked(store.job("owner"));
        let refused = store.update_job(&locron_store::UpdateJob {
            id: job.id,
            expected_revision: 2,
            name: job.name,
            description: job.description,
            tags_json: job.tags_json,
            enabled: job.enabled,
            definition_json: job.definition_json,
            now_us: 2,
            cursor_us: job.cursor_us,
        });
        assert!(
            matches!(refused, Err(locron_store::StoreError::Conflict(_))),
            "stale revision did not refuse"
        );
        assert!(
            stale_before == logical::logical(&paths),
            "stale revision changed logical state"
        );
        drop(store);
        drop(writer);
        assert!(
            Instant::now() + Duration::from_secs(10) < deadline,
            "update operation exhausted horizon"
        );
        client.close_stdin();
        assert!(
            client.wait_exit().success(),
            "update MCP did not close cleanly"
        );
        drop(client);
        drop(guard);
        checked(temporary.close());
        assert!(Instant::now() < deadline, "update cleanup exceeded horizon");
    }
}
