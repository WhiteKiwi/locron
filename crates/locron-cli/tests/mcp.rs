//! MCP stdio protocol integration tests.
//!
//! Spawns the real `locron mcp` binary with piped stdin/stdout/stderr and
//! drives a JSON-RPC 2.0 client session over the pipe, verifying the
//! handshake, tool/resource/prompt surface, error codes, and the guarantee
//! that stdout carries only JSON-RPC frames while diagnostics go to stderr.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStderr, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

/// A minimal JSON-RPC client attached to one `locron mcp` subprocess.
struct McpClient {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout_lines: mpsc::Receiver<String>,
    stderr: Option<ChildStderr>,
    next_id: u64,
    received: Vec<String>,
}

impl McpClient {
    fn spawn(state: &Path) -> Self {
        let mut child = Command::new(assert_cmd::cargo::cargo_bin!("locron"))
            .arg("--state-dir")
            .arg(state)
            .arg("-v")
            .arg("mcp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn locron mcp");
        let stdin = child.stdin.take().expect("stdin pipe");
        let stdout = child.stdout.take().expect("stdout pipe");
        let stderr = child.stderr.take().expect("stderr pipe");
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                let Ok(line) = line else { break };
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            stdin: Some(stdin),
            stdout_lines: receiver,
            stderr: Some(stderr),
            next_id: 1,
            received: Vec::new(),
        }
    }

    fn send_line(&mut self, line: &str) {
        let stdin = self.stdin.as_mut().expect("stdin open");
        writeln!(stdin, "{line}").expect("write frame");
        stdin.flush().expect("flush frame");
    }

    /// Sends a request and waits for its response, asserting the frame
    /// parses as JSON-RPC 2.0 with the matching id.
    fn request(&mut self, method: &str, params: impl Into<Value>) -> Value {
        let params = params.into();
        let id = self.next_id;
        self.next_id += 1;
        self.send_line(
            &json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}).to_string(),
        );
        let line = self
            .stdout_lines
            .recv_timeout(Duration::from_secs(15))
            .expect("response timeout");
        self.received.push(line.clone());
        let value: Value = serde_json::from_str(&line).expect("stdout line is JSON");
        assert_eq!(value["jsonrpc"], "2.0", "frame: {line}");
        assert_eq!(value["id"], json!(id), "frame: {line}");
        value
    }

    /// Sends a notification (no id) and asserts the server sends no frame.
    fn notify(&mut self, method: &str, params: impl Into<Value>) {
        let params = params.into();
        self.send_line(&json!({"jsonrpc":"2.0","method":method,"params":params}).to_string());
        match self.stdout_lines.recv_timeout(Duration::from_millis(400)) {
            Err(mpsc::RecvTimeoutError::Timeout | mpsc::RecvTimeoutError::Disconnected) => {}
            Ok(line) => panic!("notification must not be answered, got: {line}"),
        }
    }

    /// Sends a request that is expected to fail with a JSON-RPC error.
    fn request_error(&mut self, method: &str, params: Value) -> Value {
        let response = self.request(method, params);
        assert!(
            response.get("error").is_some(),
            "expected JSON-RPC error: {response}"
        );
        response
    }

    /// Calls a tool and returns its parsed result object, panicking on
    /// tool-level errors.
    fn call_tool(&mut self, name: &str, arguments: impl Into<Value>) -> Value {
        let arguments = arguments.into();
        let response = self.request("tools/call", json!({"name": name, "arguments": arguments}));
        let result = response["result"].clone();
        if result.get("isError").and_then(Value::as_bool) == Some(true) {
            let text = result["content"][0]["text"]
                .as_str()
                .unwrap_or("(no text)")
                .to_owned();
            panic!("tool {name} returned isError: {text}");
        }
        let text = result["content"][0]["text"].as_str().expect("text content");
        serde_json::from_str(text).expect("tool result is JSON")
    }

    /// Closes stdin, signalling EOF to the server.
    fn close_stdin(&mut self) {
        self.stdin.take();
    }

    fn wait_exit(&mut self) -> ExitStatus {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if let Some(status) = self.child.try_wait().expect("try_wait") {
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "locron mcp did not exit after stdin close"
            );
            thread::sleep(Duration::from_millis(20));
        }
    }

    /// Drains any remaining stdout frames after the server has exited.
    fn drain_stdout(&mut self) -> Vec<String> {
        let mut lines = Vec::new();
        while let Ok(line) = self.stdout_lines.recv_timeout(Duration::from_millis(300)) {
            self.received.push(line.clone());
            lines.push(line);
        }
        lines
    }

    fn stderr_text(&mut self) -> String {
        let mut text = String::new();
        self.stderr
            .take()
            .expect("stderr pipe")
            .read_to_string(&mut text)
            .expect("read stderr");
        text
    }

    fn assert_clean_streams(&mut self) {
        let drained = self.drain_stdout();
        for line in drained {
            let value: Value = serde_json::from_str(&line).expect("stdout carries only JSON-RPC");
            assert_eq!(value["jsonrpc"], "2.0", "frame: {line}");
            assert!(
                value.get("id").is_some() || value.get("method").is_some(),
                "frame has id or method: {line}"
            );
        }
        let stderr = self.stderr_text();
        assert!(
            stderr.contains("starting locron MCP stdio server"),
            "diagnostics must appear on stderr, got: {stderr}"
        );
    }
}

fn spawn_mcp() -> (tempfile::TempDir, McpClient) {
    let state = tempfile::tempdir().expect("tempdir");
    let client = McpClient::spawn(state.path());
    (state, client)
}

fn add_job_arguments(name: &str) -> Value {
    json!({
        "name": name,
        "schedule_type": "interval",
        "schedule_expr": "15m",
        "target_type": "process",
        "command": ["/bin/echo", "hello"],
        "description": "integration test job",
        "tags": ["test"]
    })
}

#[test]
fn initialize_handshake_ping_and_clean_eof_exit() {
    let (_state, mut client) = spawn_mcp();

    let response = client.request("initialize", json!({}));
    let result = &response["result"];
    assert_eq!(result["protocolVersion"], "2024-11-05");
    assert_eq!(result["serverInfo"]["name"], "locron");
    assert_eq!(result["serverInfo"]["version"], env!("CARGO_PKG_VERSION"));
    assert!(result["capabilities"]["tools"].is_object());
    assert!(result["capabilities"]["resources"].is_object());
    assert!(result["capabilities"]["prompts"].is_object());

    client.notify("notifications/initialized", json!({}));
    assert_eq!(client.request("ping", json!({}))["result"], json!({}));

    client.close_stdin();
    let status = client.wait_exit();
    assert!(status.success(), "clean exit on EOF");
    client.assert_clean_streams();
}

#[test]
fn protocol_error_codes() {
    let (_state, mut client) = spawn_mcp();
    client.request("initialize", json!({}));

    let response = client.request_error("no_such_method", json!({}));
    assert_eq!(response["error"]["code"], -32601);

    let response = client.request_error("tools/call", json!({"arguments": {}}));
    assert_eq!(response["error"]["code"], -32602);

    let response = client.request_error("resources/read", json!({}));
    assert_eq!(response["error"]["code"], -32602);

    let response = client.request_error("prompts/get", json!({}));
    assert_eq!(response["error"]["code"], -32602);

    let response = client.request_error("tools/call", json!({"name": "locron_mystery"}));
    assert_eq!(response["error"]["code"], -32601);

    // Parse error: raw non-JSON input gets a -32700 frame with null id.
    client.send_line("{not json");
    let line = client
        .stdout_lines
        .recv_timeout(Duration::from_secs(5))
        .expect("parse error response");
    client.received.push(line.clone());
    let value: Value = serde_json::from_str(&line).expect("stdout line is JSON");
    assert_eq!(value["error"]["code"], -32700);
    assert_eq!(value["id"], Value::Null);

    client.close_stdin();
    assert!(client.wait_exit().success());
    client.assert_clean_streams();
}

#[test]
fn tools_list_advertises_thirteen_tools() {
    let (_state, mut client) = spawn_mcp();
    client.request("initialize", json!({}));

    let response = client.request("tools/list", json!({}));
    let tools = response["result"]["tools"].as_array().expect("tools array");
    assert_eq!(tools.len(), 13);
    for tool in tools {
        assert!(tool["name"].is_string());
        assert!(tool["description"].is_string());
        assert_eq!(tool["inputSchema"]["type"], "object");
        assert!(tool["inputSchema"]["properties"].is_object());
    }

    client.close_stdin();
    assert!(client.wait_exit().success());
    client.assert_clean_streams();
}

#[test]
fn full_tool_and_resource_flow() {
    let (_state, mut client) = spawn_mcp();
    client.request("initialize", json!({}));
    client.notify("notifications/initialized", json!({}));

    // add_job dry run must not persist.
    let mut dry_args = add_job_arguments("backup");
    dry_args["dry_run"] = json!(true);
    let result = client.call_tool("locron_add_job", dry_args);
    assert_eq!(result["dry_run"], true);
    assert_eq!(result["name"], "backup");

    let listed = client.call_tool("locron_list_jobs", json!({}));
    assert!(
        listed.as_array().expect("jobs").is_empty(),
        "dry run must not create"
    );

    // Real add.
    let created = client.call_tool("locron_add_job", add_job_arguments("backup"));
    assert_eq!(created["name"], "backup");
    assert_eq!(created["enabled"], true);
    let job_id = created["id"].as_str().expect("job id").to_owned();

    let listed = client.call_tool("locron_list_jobs", json!({}));
    let jobs = listed.as_array().expect("jobs array");
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0]["name"], "backup");
    assert!(jobs[0]["next_occurrence"].is_string());

    let filtered = client.call_tool("locron_list_jobs", json!({"tag": "test"}));
    assert_eq!(filtered.as_array().expect("jobs").len(), 1);
    let filtered = client.call_tool("locron_list_jobs", json!({"enabled_only": true}));
    assert_eq!(filtered.as_array().expect("jobs").len(), 1);

    let got = client.call_tool("locron_get_job", json!({"job": "backup"}));
    assert_eq!(got["job"]["id"], job_id);

    let preview = client.call_tool(
        "locron_preview_schedule",
        json!({"schedule_type": "cron", "schedule_expr": "0 3 * * *", "timezone": "UTC", "count": 3}),
    );
    assert_eq!(preview.as_array().expect("occurrences").len(), 3);

    // run_job dry run, then a real enqueue and cancel.
    let dry_run = client.call_tool("locron_run_job", json!({"job": "backup", "dry_run": true}));
    assert_eq!(dry_run["dry_run"], true);
    assert_eq!(dry_run["decision"], "eligible");

    let run = client.call_tool("locron_run_job", json!({"job": "backup"}));
    let run_id = run["run_id"].as_str().expect("run id").to_owned();
    assert_eq!(run["state"], "queued");

    let cancel_dry = client.call_tool(
        "locron_cancel_run",
        json!({"run_id": run_id, "dry_run": true}),
    );
    assert_eq!(cancel_dry["would_request_cancellation"], true);

    let cancelled = client.call_tool("locron_cancel_run", json!({"run_id": run_id}));
    assert_eq!(cancelled["requested"], true);
    assert_eq!(cancelled["cancelled"], true);

    let why = client.call_tool("locron_why", json!({"job": "backup"}));
    assert_eq!(why["job"]["name"], "backup");
    let why_run = client.call_tool("locron_why", json!({"run_id": run_id}));
    assert_eq!(why_run["run"]["id"], run_id);
    assert!(why_run["events"].is_array());

    let logs = client.request(
        "tools/call",
        json!({"name": "locron_get_logs", "arguments": {"run_id": run_id}}),
    );
    let logs_text = logs["result"]["content"][0]["text"]
        .as_str()
        .expect("logs text");
    assert!(logs_text.contains("No captured logs"));

    let doctor = client.call_tool("locron_doctor", json!({}));
    assert!(doctor["checks"].is_array());
    assert!(doctor["database"].is_string());

    let updated = client.call_tool(
        "locron_update_job",
        json!({"job": "backup", "description": "updated via mcp", "max_retries": 2}),
    );
    assert_eq!(updated["description"], "updated via mcp");

    // Resources.
    let resources = client.request("resources/list", json!({}));
    let names = resources["result"]["resources"]
        .as_array()
        .expect("resources")
        .iter()
        .map(|r| r["uri"].as_str().expect("uri"))
        .collect::<Vec<_>>();
    assert_eq!(names, ["locron://jobs", "locron://doctor"]);
    let templates = resources["result"]["resourceTemplates"]
        .as_array()
        .expect("templates")
        .iter()
        .map(|r| r["uriTemplate"].as_str().expect("template"))
        .collect::<Vec<_>>();
    assert_eq!(
        templates,
        [
            "locron://jobs/{job_id_or_name}",
            "locron://history/{run_id}",
            "locron://logs/{run_id}"
        ]
    );

    let read = client.request("resources/read", json!({"uri": "locron://jobs"}));
    assert_eq!(
        read["result"]["contents"][0]["mimeType"],
        "application/json"
    );
    let read = client.request("resources/read", json!({"uri": "locron://jobs/backup"}));
    let text = read["result"]["contents"][0]["text"]
        .as_str()
        .expect("text");
    let job: Value = serde_json::from_str(text).expect("job json");
    assert_eq!(job["job"]["name"], "backup");
    let read = client.request(
        "resources/read",
        json!({"uri": format!("locron://history/{run_id}")}),
    );
    let text = read["result"]["contents"][0]["text"]
        .as_str()
        .expect("text");
    let history: Value = serde_json::from_str(text).expect("history json");
    assert_eq!(history["run"]["id"], run_id);
    let read = client.request(
        "resources/read",
        json!({"uri": format!("locron://logs/{run_id}")}),
    );
    assert_eq!(read["result"]["contents"][0]["mimeType"], "text/plain");
    let read = client.request("resources/read", json!({"uri": "locron://doctor"}));
    assert_eq!(
        read["result"]["contents"][0]["mimeType"],
        "application/json"
    );

    // Prompts.
    let prompts = client.request("prompts/list", json!({}));
    let names = prompts["result"]["prompts"]
        .as_array()
        .expect("prompts")
        .iter()
        .map(|p| p["name"].as_str().expect("name"))
        .collect::<Vec<_>>();
    assert_eq!(names, ["schedule_task", "diagnose_failure"]);
    let prompt = client.request(
        "prompts/get",
        json!({"name": "schedule_task", "arguments": {"task_description": "backup data"}}),
    );
    assert!(
        prompt["result"]["messages"][0]["content"]["text"]
            .as_str()
            .expect("prompt text")
            .contains("backup data")
    );

    // Lifecycle: disable/enable/remove, including dry-run variants.
    let disabled = client.call_tool("locron_disable_job", json!({"job": "backup"}));
    assert_eq!(disabled["enabled"], false);
    let enabled = client.call_tool("locron_enable_job", json!({"job": "backup"}));
    assert_eq!(enabled["enabled"], true);
    let remove_dry = client.call_tool(
        "locron_remove_job",
        json!({"job": "backup", "dry_run": true}),
    );
    assert_eq!(remove_dry["would_remove"], true);
    let removed = client.call_tool("locron_remove_job", json!({"job": "backup"}));
    assert_eq!(removed["removed"], true);

    // Domain error: removed job is a tool error, not a JSON-RPC error.
    let response = client.request(
        "tools/call",
        json!({"name": "locron_get_job", "arguments": {"job": "backup"}}),
    );
    assert_eq!(
        response["result"]["isError"].as_bool(),
        Some(true),
        "missing job must be a tool error: {response}"
    );

    client.close_stdin();
    assert!(client.wait_exit().success());
    client.assert_clean_streams();
}

#[test]
fn terminates_cleanly_on_sigterm() {
    let (_state, mut client) = spawn_mcp();
    client.request("initialize", json!({}));

    let pid = client.child.id();
    let status = Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .status()
        .expect("run kill");
    assert!(status.success(), "kill -TERM delivered");

    let status = client.wait_exit();
    assert!(status.success(), "clean exit on SIGTERM");
    client.assert_clean_streams();
}
