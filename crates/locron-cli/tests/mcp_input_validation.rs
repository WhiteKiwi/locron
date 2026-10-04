//! Real MCP dispatch regressions, sharing the existing subprocess client.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::{McpClient, add_job_arguments};

fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, directory: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in std::fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            let metadata = std::fs::symlink_metadata(&path).unwrap();
            assert!(!metadata.file_type().is_symlink());
            if metadata.is_dir() {
                visit(root, &path, files);
            } else {
                assert!(metadata.is_file());
                files.insert(path.strip_prefix(root).unwrap().to_owned(), std::fs::read(path).unwrap());
            }
        }
    }
    let mut files = BTreeMap::new();
    if root.exists() {
        visit(root, root, &mut files);
    }
    files
}

fn refused(client: &mut McpClient, name: &str, arguments: Value) -> String {
    let response = client.request("tools/call", json!({"name":name,"arguments":arguments}));
    assert!(response.get("error").is_none(), "preserve the tool-error envelope: {response}");
    assert_eq!(response["result"]["isError"], true, "{name}: {response}");
    let text = response["result"]["content"][0]["text"].as_str().unwrap();
    assert!(!text.contains("secret-canary"), "input reflected: {text}");
    text.to_owned()
}

fn mutation_cases(run_id: &str) -> Vec<(&'static str, Value)> {
    vec![
        ("locron_add_job", add_job_arguments("must-not-create")),
        ("locron_update_job", json!({"job":"seed", "name":"must-not-rename"})),
        ("locron_enable_job", json!({"job":"seed"})),
        ("locron_disable_job", json!({"job":"seed"})),
        ("locron_remove_job", json!({"job":"seed"})),
        ("locron_run_job", json!({"job":"seed"})),
        ("locron_cancel_run", json!({"run_id":run_id})),
    ]
}

fn malformed_flags(client: &mut McpClient, run_id: &str) {
    for (name, original) in mutation_cases(run_id) {
        for bad in [json!(null), json!("secret-canary"), json!("true"), json!(0), json!(1), json!([]), json!({})] {
            let mut arguments = original.clone();
            arguments["dry_run"] = bad;
            assert!(refused(client, name, arguments).contains("dry_run"));
        }
        for bad in [json!(null), json!([]), json!(true), json!(42), json!("secret-canary")] {
            assert!(refused(client, name, bad).contains("object"));
        }
    }
}

#[test]
fn malformed_mutation_arguments_leave_absent_state_absent() {
    let temporary = tempfile::tempdir().unwrap();
    let parent = locron_core::filesystem::DirectoryGuard::private(&temporary.path().join("private")).unwrap();
    let state = parent.normalized_path().join("absent");
    let mut client = McpClient::spawn(&state);
    malformed_flags(&mut client, "00000000-0000-7000-8000-000000000001");
    assert!(!state.exists());
    let response = client.request("ping", json!({}));
    assert_eq!(response["result"], json!({}));
}

#[test]
fn malformed_mutations_preserve_existing_database_and_all_fixture_files() {
    let temporary = tempfile::tempdir().unwrap();
    let root = locron_core::filesystem::DirectoryGuard::private(&temporary.path().join("private")).unwrap();
    let state = root.normalized_path();
    let mut client = McpClient::spawn(state);
    client.call_tool("locron_add_job", add_job_arguments("seed"));
    let run = client.call_tool("locron_run_job", json!({"job":"seed"}));
    let before = snapshot(state);
    assert!(!before.is_empty());
    malformed_flags(&mut client, run["run_id"].as_str().unwrap());
    assert_eq!(snapshot(state), before, "invalid calls changed durable state");
}

#[test]
fn malformed_metadata_and_arrays_are_not_silently_dropped() {
    let temporary = tempfile::tempdir().unwrap();
    let root = locron_core::filesystem::DirectoryGuard::private(&temporary.path().join("private")).unwrap();
    let state = root.normalized_path();
    let mut client = McpClient::spawn(state);
    client.call_tool("locron_add_job", add_job_arguments("seed"));
    let before = snapshot(state);
    for name in ["locron_add_job", "locron_update_job"] {
        let base = if name == "locron_add_job" { add_job_arguments("new") } else { json!({"job":"seed"}) };
        for field in ["name", "schedule_type", "schedule_expr", "timezone", "target_type", "shell_script", "http_url", "http_method", "overlap_policy", "missed_run_policy", "description"] {
            let mut arguments = base.clone();
            arguments[field] = json!({"secret-canary":true});
            assert!(refused(&mut client, name, arguments).contains(field));
        }
        for field in ["tags", "command"] {
            for bad in [json!(null), json!("secret-canary"), json!(["secret-canary", 9]), json!([null])] {
                let mut arguments = base.clone();
                arguments[field] = bad;
                assert!(refused(&mut client, name, arguments).contains(field));
            }
        }
        for field in ["max_retries", "timeout_seconds"] {
            for bad in [json!(null), json!(-1), json!(1.5), json!("secret-canary"), json!({}), json!([])] {
                let mut arguments = base.clone();
                arguments[field] = bad;
                assert!(refused(&mut client, name, arguments).contains(field));
            }
        }
        let mut arguments = base.clone();
        arguments["max_retries"] = json!(11);
        assert!(refused(&mut client, name, arguments).contains("max_retries"));
    }
    for bad in [json!(null), json!("secret-canary"), json!(0), json!([]), json!({})] {
        assert!(refused(&mut client, "locron_update_job", json!({"job":"seed", "enabled":bad})).contains("enabled"));
        assert!(refused(&mut client, "locron_cancel_run", json!({"run_id":"00000000-0000-7000-8000-000000000001", "acknowledge_unconfirmed":bad})).contains("acknowledge_unconfirmed"));
        assert!(refused(&mut client, "locron_run_job", json!({"job":"seed", "wait":bad})).contains("wait"));
    }
    assert_eq!(snapshot(state), before);
}

#[test]
fn invalid_wait_deadlines_refuse_before_enqueue_in_every_mode() {
    let temporary = tempfile::tempdir().unwrap();
    let root = locron_core::filesystem::DirectoryGuard::private(&temporary.path().join("private")).unwrap();
    let state = root.normalized_path();
    let mut client = McpClient::spawn(state);
    client.call_tool("locron_add_job", add_job_arguments("seed"));
    let before = snapshot(state);
    for wait in [false, true] {
        for dry_run in [false, true] {
            for bad in [json!(0), json!(-1), json!(1.5), json!("30"), json!(null), json!(u64::MAX)] {
                assert!(refused(&mut client, "locron_run_job", json!({"job":"seed", "wait":wait, "dry_run":dry_run, "timeout_seconds":bad})).contains("timeout_seconds"));
            }
        }
    }
    assert_eq!(snapshot(state), before);
    assert_eq!(client.request("ping", json!({}))["result"], json!({}));
}

#[test]
fn valid_previews_defaults_live_flags_and_description_clearing_remain_supported() {
    let temporary = tempfile::tempdir().unwrap();
    let parent = locron_core::filesystem::DirectoryGuard::private(&temporary.path().join("private")).unwrap();
    let state = parent.normalized_path().join("state");
    let mut client = McpClient::spawn(&state);
    let mut arguments = add_job_arguments("seed");
    arguments["dry_run"] = json!(true);
    arguments["description"] = Value::Null;
    arguments["tags"] = json!([]);
    assert_eq!(client.call_tool("locron_add_job", arguments.clone())["dry_run"], true);
    assert!(!state.exists());
    arguments["dry_run"] = json!(false);
    let created = client.call_tool("locron_add_job", arguments);
    assert_eq!(created["name"], "seed");
    let updated = client.call_tool("locron_update_job", json!({"job":"seed", "description":"set me", "enabled":false, "tags":["kept"]}));
    assert_eq!(updated["enabled"], false);
    let preview = client.call_tool("locron_update_job", json!({"job":"seed", "description":null, "tags":[], "dry_run":true}));
    assert!(preview["updated"]["description"].is_null());
    let updated = client.call_tool("locron_update_job", json!({"job":"seed", "description":null, "enabled":true}));
    assert!(updated["description"].is_null());
    assert_eq!(updated["enabled"], true);
    let catalogue = client.request("tools/list", json!({}));
    for tool in catalogue["result"]["tools"].as_array().unwrap() {
        if ["locron_add_job", "locron_update_job"].contains(&tool["name"].as_str().unwrap()) {
            assert_eq!(tool["inputSchema"]["properties"]["description"]["type"], json!(["string", "null"]));
        }
    }
}

#[test]
fn wait_expiry_observes_a_durable_run_without_cancelling_it() {
    let temporary = tempfile::tempdir().unwrap();
    let root = locron_core::filesystem::DirectoryGuard::private(&temporary.path().join("private")).unwrap();
    let mut client = McpClient::spawn(root.normalized_path());
    client.call_tool("locron_add_job", add_job_arguments("seed"));
    let run = client.call_tool("locron_run_job", json!({"job":"seed", "wait":true, "timeout_seconds":1}));
    assert_eq!(run["completed"], false);
    assert_eq!(run["state"], "queued");
    let cancelled = client.call_tool("locron_cancel_run", json!({"run_id":run["run_id"], "dry_run":false}));
    assert_eq!(cancelled["cancelled"], true);
    assert_eq!(cancelled["before_execution"], true);
}
