#[path = "../support/storage_contention.rs"]
mod contention;

use super::*;

#[test]
fn held_writer_tool_failure_keeps_text_and_adds_safe_storage_record() {
    let temporary = tempfile::tempdir().unwrap();
    let guard = locron_core::filesystem::DirectoryGuard::private(&temporary.path().join("private"))
        .unwrap();
    let paths = locron_store::StatePaths::new(guard.normalized_path().to_path_buf());
    drop(guard);
    contention::seed(&paths);
    let mut client = McpClient::spawn_with_verbosity(&paths.root, "-vv");
    client.request("initialize", json!({}));
    let writer = contention::hold_writer(&paths);
    let started = Instant::now();
    let response = client.request(
        "tools/call",
        json!({"name":"locron_remove_job","arguments":{"job":"private-process"}}),
    );
    assert!(
        started.elapsed() < Duration::from_secs(8),
        "stacked wait budget"
    );
    assert!(
        response.get("error").is_none(),
        "tool failure is not a protocol failure"
    );
    let result = &response["result"];
    assert_eq!(result["isError"], true);
    contention::assert_busy(&result["error"]);
    let text = result["content"][0]["text"].as_str().unwrap();
    assert!(text.starts_with("Error: storage "));
    assert!(text.contains("SQLite primary 5"));
    contention::assert_private(&serde_json::to_vec(&response).unwrap());
    writer.execute_batch("ROLLBACK").unwrap();
    drop(writer);
    let store = locron_store::Store::open(paths.clone(), env!("CARGO_PKG_VERSION"), 1).unwrap();
    assert!(!store.job("private-process").unwrap().enabled);
    drop(store);
    contention::reject_private_mutations(&paths);
    let rejected = client.request(
        "tools/call",
        json!({"name":"locron_remove_job","arguments":{"job":"private-process"}}),
    );
    assert_eq!(rejected["result"]["isError"], true);
    contention::assert_rejected(&rejected["result"]["error"]);
    contention::assert_private(&serde_json::to_vec(&rejected).unwrap());
    contention::allow_private_mutations(&paths);
    assert_eq!(
        client.call_tool("locron_remove_job", json!({"job":"private-process"}))["removed"],
        true,
    );
    client.close_stdin();
    assert!(client.wait_exit().success());
    for frame in &client.received {
        contention::assert_private(frame.as_bytes());
    }
    contention::assert_private(client.stderr_text().as_bytes());
}
