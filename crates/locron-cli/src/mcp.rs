//! Model Context Protocol (MCP) server implementation for locron.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use locron_core::command::JobDefinition;
use locron_core::policy::{ExecutionPolicy, MissedRunPolicy, OverlapPolicy};
use locron_core::schedule::{Schedule, ScheduleTimeZone};
use locron_core::target::{Environment, HttpMethod, HttpTarget, Target};
use locron_core::{JobId, Timestamp};
use locron_store::{AdmitAttempt, CancelOutcome, CreateJob, StatePaths, UpdateJob};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use uuid::Uuid;

use crate::{
    configured_global_concurrency, daemon_lock_free, engine_target, environment_warnings, now_us,
    open, open_read_only, redact_definition, redacted_job, redacted_observable_run, redacted_run,
    send_wake, terminal_run_state, validate_metadata,
};

#[derive(Debug, Deserialize)]
pub(crate) struct JsonRpcRequest {
    pub jsonrpc: String,
    #[serde(default)]
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Option<Value>,
}

#[derive(Debug, Serialize, PartialEq)]
pub(crate) struct JsonRpcResponse {
    pub jsonrpc: &'static str,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize, PartialEq)]
pub(crate) struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl JsonRpcResponse {
    pub fn success(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: Value, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }
}

pub async fn run_mcp_server(paths: StatePaths) -> Result<()> {
    tracing::info!("starting locron MCP stdio server");
    let stdin = tokio::io::stdin();
    let mut reader = tokio::io::BufReader::new(stdin).lines();
    let mut stdout = tokio::io::stdout();

    // Registered once so the handlers stay armed across the whole session
    // (both underlying futures are cancel-safe).
    let mut termination = Box::pin(async {
        #[cfg(unix)]
        {
            if let Ok(mut terminate) =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                && terminate.recv().await.is_some()
            {
                return;
            }
        }
        let _ = tokio::signal::ctrl_c().await;
    });

    loop {
        // One JSON-RPC frame per newline-delimited line. The loop ends on
        // stdin EOF, or exits directly on an interrupt/termination signal so
        // the process never hangs on runtime shutdown with stdin still open.
        let line = tokio::select! {
            line = reader.next_line() => match line {
                Ok(Some(line)) => line,
                Ok(None) => break,
                Err(error) => return Err(error.into()),
            },
            () = &mut termination => {
                tracing::info!("received termination signal; shutting down locron MCP stdio server");
                let _ = stdout.flush().await;
                std::process::exit(0);
            }
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let response = match serde_json::from_str::<JsonRpcRequest>(trimmed) {
            Ok(req) => handle_request(&paths, req).await,
            Err(err) => Some(JsonRpcResponse::error(
                Value::Null,
                -32700,
                format!("Parse error: {err}"),
            )),
        };

        if let Some(resp) = response {
            let serialized = serde_json::to_string(&resp)?;
            stdout.write_all(serialized.as_bytes()).await?;
            stdout.write_all(b"\n").await?;
            stdout.flush().await?;
        }
    }

    tracing::info!("locron MCP stdio server connection closed");
    Ok(())
}

pub(crate) async fn handle_request(
    paths: &StatePaths,
    req: JsonRpcRequest,
) -> Option<JsonRpcResponse> {
    if req.jsonrpc != "2.0" {
        return req
            .id
            .map(|id| JsonRpcResponse::error(id, -32600, "Invalid Request: expected jsonrpc 2.0"));
    }

    // Handle notifications (no id)
    if req.id.is_none() {
        tracing::debug!(method = %req.method, "received MCP notification");
        return None;
    }

    let id = req.id.unwrap();
    let params = req.params.unwrap_or(Value::Null);

    let response = match req.method.as_str() {
        "initialize" => handle_initialize(id),
        "notifications/initialized" | "initialized" => JsonRpcResponse::success(id, json!({})),
        "ping" => JsonRpcResponse::success(id, json!({})),
        "tools/list" => JsonRpcResponse::success(id, handle_tools_list()),
        "tools/call" => handle_tools_call(paths, id, params).await,
        "resources/list" => JsonRpcResponse::success(id, handle_resources_list()),
        "resources/read" => handle_resources_read(paths, id, &params),
        "prompts/list" => JsonRpcResponse::success(id, handle_prompts_list()),
        "prompts/get" => handle_prompts_get(id, &params),
        other => JsonRpcResponse::error(id, -32601, format!("Method not found: {other}")),
    };

    Some(response)
}

fn handle_initialize(id: Value) -> JsonRpcResponse {
    JsonRpcResponse::success(
        id,
        json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": { "listChanged": false },
                "resources": { "subscribe": false, "listChanged": false },
                "prompts": { "listChanged": false }
            },
            "serverInfo": {
                "name": "locron",
                "version": env!("CARGO_PKG_VERSION")
            }
        }),
    )
}

fn handle_tools_list() -> Value {
    json!({
        "tools": [
            {
                "name": "locron_list_jobs",
                "description": "List scheduled jobs with optional status and tag filtering.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "enabled_only": {
                            "type": "boolean",
                            "description": "Filter only enabled jobs."
                        },
                        "tag": {
                            "type": "string",
                            "description": "Filter jobs containing the specified tag."
                        }
                    }
                }
            },
            {
                "name": "locron_get_job",
                "description": "Get full definition, metadata, schedule, policies, and recent runs for a job.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "job": {
                            "type": "string",
                            "description": "Job UUID or unique job name."
                        }
                    },
                    "required": ["job"]
                }
            },
            {
                "name": "locron_add_job",
                "description": "Register a new scheduled job (cron, interval, or one-time).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "Unique job identifier name."
                        },
                        "schedule_type": {
                            "type": "string",
                            "enum": ["cron", "interval", "at"],
                            "description": "Schedule mode."
                        },
                        "schedule_expr": {
                            "type": "string",
                            "description": "Cron expression (e.g. '0 3 * * *'), interval duration (e.g. '15m', '30s'), or ISO 8601 offset timestamp (e.g. '2026-09-01T09:00:00+09:00')."
                        },
                        "timezone": {
                            "type": "string",
                            "description": "IANA time zone (e.g. 'Asia/Seoul', 'UTC', default: 'Local')."
                        },
                        "target_type": {
                            "type": "string",
                            "enum": ["process", "shell", "http"],
                            "description": "Target execution type."
                        },
                        "command": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Process argv list (required if target_type == 'process')."
                        },
                        "shell_script": {
                            "type": "string",
                            "description": "Shell script string (required if target_type == 'shell')."
                        },
                        "http_url": {
                            "type": "string",
                            "description": "URL for HTTP target (required if target_type == 'http')."
                        },
                        "http_method": {
                            "type": "string",
                            "description": "HTTP method (default: 'GET')."
                        },
                        "overlap_policy": {
                            "type": "string",
                            "enum": ["skip", "replace", "allow"],
                            "description": "Overlap policy (default: 'skip')."
                        },
                        "missed_run_policy": {
                            "type": "string",
                            "enum": ["skip", "latest", "all"],
                            "description": "Missed run policy (default: 'skip')."
                        },
                        "max_retries": {
                            "type": "integer",
                            "minimum": 0,
                            "maximum": 10,
                            "description": "Maximum failure retries (0..10)."
                        },
                        "timeout_seconds": {
                            "type": "integer",
                            "minimum": 1,
                            "description": "Execution timeout in seconds."
                        },
                        "description": {
                            "type": "string",
                            "description": "Human-readable job description."
                        },
                        "tags": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "List of tags."
                        },
                        "dry_run": {
                            "type": "boolean",
                            "description": "If true, simulates validation without saving."
                        }
                    },
                    "required": ["name", "schedule_type", "schedule_expr", "target_type"]
                }
            },
            {
                "name": "locron_update_job",
                "description": "Update an existing job's schedule, policies, target, or metadata.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "job": {
                            "type": "string",
                            "description": "Job UUID or name to update."
                        },
                        "name": {
                            "type": "string",
                            "description": "New job name (rename)."
                        },
                        "schedule_type": {
                            "type": "string",
                            "enum": ["cron", "interval", "at"],
                            "description": "New schedule mode."
                        },
                        "schedule_expr": {
                            "type": "string",
                            "description": "New schedule expression."
                        },
                        "timezone": {
                            "type": "string",
                            "description": "New IANA time zone for cron."
                        },
                        "target_type": {
                            "type": "string",
                            "enum": ["process", "shell", "http"],
                            "description": "New target type."
                        },
                        "command": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Process argv list."
                        },
                        "shell_script": {
                            "type": "string",
                            "description": "Shell script string."
                        },
                        "http_url": {
                            "type": "string",
                            "description": "HTTP target URL."
                        },
                        "http_method": {
                            "type": "string",
                            "description": "HTTP method."
                        },
                        "overlap_policy": {
                            "type": "string",
                            "enum": ["skip", "replace", "allow"],
                            "description": "Overlap policy."
                        },
                        "missed_run_policy": {
                            "type": "string",
                            "enum": ["skip", "latest", "all"],
                            "description": "Missed run policy."
                        },
                        "max_retries": {
                            "type": "integer",
                            "minimum": 0,
                            "maximum": 10,
                            "description": "Max failure retries."
                        },
                        "timeout_seconds": {
                            "type": "integer",
                            "minimum": 1,
                            "description": "Execution timeout in seconds."
                        },
                        "description": {
                            "type": "string",
                            "description": "Job description."
                        },
                        "tags": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "List of tags."
                        },
                        "enabled": {
                            "type": "boolean",
                            "description": "Enable or disable the job."
                        },
                        "dry_run": {
                            "type": "boolean",
                            "description": "Validate without mutating state."
                        }
                    },
                    "required": ["job"]
                }
            },
            {
                "name": "locron_enable_job",
                "description": "Enable a scheduled job.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "job": {
                            "type": "string",
                            "description": "Job UUID or name."
                        },
                        "dry_run": {
                            "type": "boolean",
                            "description": "Report the planned change without mutating state."
                        }
                    },
                    "required": ["job"]
                }
            },
            {
                "name": "locron_disable_job",
                "description": "Disable a scheduled job.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "job": {
                            "type": "string",
                            "description": "Job UUID or name."
                        },
                        "dry_run": {
                            "type": "boolean",
                            "description": "Report the planned change without mutating state."
                        }
                    },
                    "required": ["job"]
                }
            },
            {
                "name": "locron_remove_job",
                "description": "Soft-remove a scheduled job.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "job": {
                            "type": "string",
                            "description": "Job UUID or name."
                        },
                        "dry_run": {
                            "type": "boolean",
                            "description": "Report the planned removal without mutating state."
                        }
                    },
                    "required": ["job"]
                }
            },
            {
                "name": "locron_run_job",
                "description": "Trigger an immediate manual run of a job.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "job": {
                            "type": "string",
                            "description": "Job UUID or name."
                        },
                        "wait": {
                            "type": "boolean",
                            "description": "Whether to wait for execution completion (default: false)."
                        },
                        "timeout_seconds": {
                            "type": "integer",
                            "minimum": 1,
                            "description": "Max wait duration in seconds (default: 30)."
                        },
                        "dry_run": {
                            "type": "boolean",
                            "description": "Simulate run admission without executing."
                        }
                    },
                    "required": ["job"]
                }
            },
            {
                "name": "locron_cancel_run",
                "description": "Request cancellation of an active or queued run.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "run_id": {
                            "type": "string",
                            "description": "Run UUID."
                        },
                        "acknowledge_unconfirmed": {
                            "type": "boolean",
                            "description": "Acknowledge unconfirmed quarantine if applicable."
                        },
                        "dry_run": {
                            "type": "boolean",
                            "description": "Report whether cancellation would be requested without mutating state."
                        }
                    },
                    "required": ["run_id"]
                }
            },
            {
                "name": "locron_get_logs",
                "description": "Retrieve captured stdout/stderr/HTTP output for a run.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "job": {
                            "type": "string",
                            "description": "Job UUID or name (retrieves latest run if run_id omitted)."
                        },
                        "run_id": {
                            "type": "string",
                            "description": "Specific run UUID."
                        },
                        "tail_lines": {
                            "type": "integer",
                            "minimum": 1,
                            "description": "Number of latest lines to retrieve (default: 100)."
                        }
                    }
                }
            },
            {
                "name": "locron_why",
                "description": "Explain the causal diagnosis of a job's current state or skipped occurrences.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "job": {
                            "type": "string",
                            "description": "Job UUID or name."
                        },
                        "run_id": {
                            "type": "string",
                            "description": "Run UUID."
                        }
                    }
                }
            },
            {
                "name": "locron_preview_schedule",
                "description": "Preview future occurrence timestamps for a schedule expression without creating a job.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "schedule_type": {
                            "type": "string",
                            "enum": ["cron", "interval"],
                            "description": "Schedule mode."
                        },
                        "schedule_expr": {
                            "type": "string",
                            "description": "Cron expression or interval duration."
                        },
                        "timezone": {
                            "type": "string",
                            "description": "IANA timezone (optional)."
                        },
                        "count": {
                            "type": "integer",
                            "minimum": 1,
                            "maximum": 20,
                            "description": "Number of occurrences to calculate (default: 5, max: 20)."
                        }
                    },
                    "required": ["schedule_type", "schedule_expr"]
                }
            },
            {
                "name": "locron_doctor",
                "description": "Perform health and environment diagnosis of the local scheduler daemon and database.",
                "inputSchema": {
                    "type": "object",
                    "properties": {}
                }
            }
        ]
    })
}

async fn handle_tools_call(paths: &StatePaths, id: Value, params: Value) -> JsonRpcResponse {
    let Some(name) = params.get("name").and_then(Value::as_str) else {
        return JsonRpcResponse::error(id, -32602, "Invalid params: missing tool name");
    };

    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);

    let result = match name {
        "locron_list_jobs" => {
            tool_list_jobs(paths, &arguments).map(|v| serde_json::to_string_pretty(&v).unwrap())
        }
        "locron_get_job" => {
            tool_get_job(paths, &arguments).map(|v| serde_json::to_string_pretty(&v).unwrap())
        }
        "locron_add_job" => {
            tool_add_job(paths, &arguments).map(|v| serde_json::to_string_pretty(&v).unwrap())
        }
        "locron_update_job" => {
            tool_update_job(paths, &arguments).map(|v| serde_json::to_string_pretty(&v).unwrap())
        }
        "locron_enable_job" => {
            tool_enable_job(paths, &arguments).map(|v| serde_json::to_string_pretty(&v).unwrap())
        }
        "locron_disable_job" => {
            tool_disable_job(paths, &arguments).map(|v| serde_json::to_string_pretty(&v).unwrap())
        }
        "locron_remove_job" => {
            tool_remove_job(paths, &arguments).map(|v| serde_json::to_string_pretty(&v).unwrap())
        }
        "locron_run_job" => tool_run_job(paths, arguments)
            .await
            .map(|v| serde_json::to_string_pretty(&v).unwrap()),
        "locron_cancel_run" => {
            tool_cancel_run(paths, &arguments).map(|v| serde_json::to_string_pretty(&v).unwrap())
        }
        "locron_get_logs" => tool_get_logs(paths, &arguments),
        "locron_why" => {
            tool_why(paths, &arguments).map(|v| serde_json::to_string_pretty(&v).unwrap())
        }
        "locron_preview_schedule" => {
            tool_preview_schedule(&arguments).map(|v| serde_json::to_string_pretty(&v).unwrap())
        }
        "locron_doctor" => tool_doctor(paths).map(|v| serde_json::to_string_pretty(&v).unwrap()),
        unknown => return JsonRpcResponse::error(id, -32601, format!("Unknown tool: {unknown}")),
    };

    match result {
        Ok(text) => JsonRpcResponse::success(
            id,
            json!({
                "content": [
                    {
                        "type": "text",
                        "text": text
                    }
                ]
            }),
        ),
        Err(err) => JsonRpcResponse::success(
            id,
            json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!("Error: {err:#}")
                    }
                ],
                "isError": true
            }),
        ),
    }
}

fn format_schedule(schedule: &Schedule) -> String {
    match schedule {
        Schedule::Cron {
            expression,
            timezone,
        } => match timezone {
            ScheduleTimeZone::Local => format!("cron: {expression} (local)"),
            ScheduleTimeZone::Iana(tz) => format!("cron: {expression} ({tz})"),
        },
        Schedule::Every { interval, .. } => format!("every: {interval:?}"),
        Schedule::At { at } => format!("at: {at}"),
    }
}

fn format_target(target: &Target) -> String {
    match target {
        Target::Process { executable, args } => {
            if args.is_empty() {
                format!("process: {executable}")
            } else {
                format!("process: {executable} {}", args.join(" "))
            }
        }
        Target::Shell { command, .. } => format!("shell: {command}"),
        Target::Http(http) => format!("http: {} {}", http.method.as_str(), http.url),
    }
}

fn tool_list_jobs(paths: &StatePaths, args: &Value) -> Result<Value> {
    let store = open(paths)?;
    let enabled_only = args
        .get("enabled_only")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let filter_tag = args.get("tag").and_then(Value::as_str);

    let all_jobs = store.list_jobs(true)?;
    let now = Timestamp::from_epoch_micros(now_us());
    let mut results = Vec::new();

    for job in all_jobs {
        if enabled_only && !job.enabled {
            continue;
        }
        let tags: Vec<String> = serde_json::from_str(&job.tags_json).unwrap_or_default();
        if let Some(tag) = filter_tag
            && !tags.iter().any(|t| t == tag)
        {
            continue;
        }
        let definition: JobDefinition = serde_json::from_str(&job.definition_json)?;
        let schedule_summary = format_schedule(&definition.schedule);
        let target_summary = format_target(&definition.target);
        let next_occurrence = if job.enabled {
            definition
                .schedule
                .next(now, 1)
                .ok()
                .and_then(|v| v.first().map(ToString::to_string))
        } else {
            None
        };
        let last_run_outcome = store
            .history(Some(&job.name), 1)
            .ok()
            .and_then(|runs| runs.first().map(|r| r.state.clone()));

        results.push(json!({
            "id": job.id,
            "name": job.name,
            "description": job.description,
            "tags": tags,
            "enabled": job.enabled,
            "schedule": schedule_summary,
            "target_summary": target_summary,
            "last_run_outcome": last_run_outcome,
            "next_occurrence": next_occurrence,
        }));
    }

    Ok(json!(results))
}

fn tool_get_job(paths: &StatePaths, args: &Value) -> Result<Value> {
    let job_name_or_id = args
        .get("job")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing required parameter: job"))?;
    let store = open(paths)?;
    let job = store.job(job_name_or_id)?;
    let definition: JobDefinition = serde_json::from_str(&job.definition_json)?;
    let now = Timestamp::from_epoch_micros(now_us());
    let next_occurrence = if job.enabled {
        definition
            .schedule
            .next(now, 1)
            .ok()
            .and_then(|v| v.first().map(ToString::to_string))
    } else {
        None
    };
    let recent_runs = store
        .history(Some(&job.name), 5)?
        .into_iter()
        .map(|r| redacted_observable_run(&store, r))
        .collect::<Result<Vec<_>>>()?;

    let redacted_job_rec = redacted_job(job)?;
    Ok(json!({
        "job": redacted_job_rec,
        "next_occurrence": next_occurrence,
        "recent_runs": recent_runs,
    }))
}

fn parse_schedule(
    schedule_type: &str,
    schedule_expr: &str,
    timezone: Option<&str>,
    now: Timestamp,
) -> Result<Schedule> {
    match schedule_type {
        "cron" => {
            let tz = match timezone.unwrap_or("local") {
                "local" => ScheduleTimeZone::Local,
                name => ScheduleTimeZone::Iana(name.into()),
            };
            Ok(Schedule::Cron {
                expression: schedule_expr.to_string(),
                timezone: tz,
            })
        }
        "interval" => {
            let interval = schedule_expr
                .parse()
                .context("invalid interval duration (e.g. '15m', '1h', '30s')")?;
            Ok(Schedule::Every {
                interval,
                anchor: now,
            })
        }
        "at" => {
            let at = schedule_expr
                .parse()
                .context("invalid 'at' timestamp (e.g. RFC 3339 format)")?;
            Ok(Schedule::At { at })
        }
        other => Err(anyhow!(
            "unsupported schedule_type: '{other}', expected 'cron', 'interval', or 'at'"
        )),
    }
}

fn parse_http_method(value: &str) -> Result<HttpMethod> {
    match value.to_uppercase().as_str() {
        "GET" => Ok(HttpMethod::Get),
        "POST" => Ok(HttpMethod::Post),
        "PUT" => Ok(HttpMethod::Put),
        "PATCH" => Ok(HttpMethod::Patch),
        "DELETE" => Ok(HttpMethod::Delete),
        "HEAD" => Ok(HttpMethod::Head),
        _ => Err(anyhow!("unsupported HTTP method: {value}")),
    }
}

fn parse_retry_count(value: Option<&Value>) -> Result<u8> {
    match value {
        None => Ok(0),
        Some(Value::Number(number)) => {
            let count = number
                .as_u64()
                .ok_or_else(|| anyhow!("max_retries must be a non-negative integer"))?;
            if count > 10 {
                return Err(anyhow!("max_retries must be from 0 through 10"));
            }
            Ok(u8::try_from(count).expect("0..=10 fits in u8"))
        }
        Some(_) => Err(anyhow!("max_retries must be a non-negative integer")),
    }
}

fn parse_timeout_seconds(value: Option<&Value>) -> Result<Option<u64>> {
    match value {
        None => Ok(None),
        Some(Value::Number(number)) => {
            let seconds = number
                .as_u64()
                .ok_or_else(|| anyhow!("timeout_seconds must be a positive integer"))?;
            if seconds == 0 {
                return Err(anyhow!("timeout_seconds must be positive"));
            }
            Ok(Some(seconds))
        }
        Some(_) => Err(anyhow!("timeout_seconds must be a positive integer")),
    }
}

fn parse_target(
    target_type: &str,
    command: Option<&[Value]>,
    shell_script: Option<&str>,
    http_url: Option<&str>,
    http_method: Option<&str>,
) -> Result<Target> {
    match target_type {
        "process" => {
            let cmd_array = command.ok_or_else(|| {
                anyhow!("'command' array is required when target_type is 'process'")
            })?;
            if cmd_array.is_empty() {
                return Err(anyhow!("'command' array must not be empty"));
            }
            let strings: Vec<String> = cmd_array
                .iter()
                .filter_map(Value::as_str)
                .map(String::from)
                .collect();
            if strings.len() != cmd_array.len() {
                return Err(anyhow!("all elements in 'command' must be strings"));
            }
            Ok(Target::Process {
                executable: strings[0].clone(),
                args: strings[1..].to_vec(),
            })
        }
        "shell" => {
            let script = shell_script
                .ok_or_else(|| anyhow!("'shell_script' is required when target_type is 'shell'"))?;
            if script.trim().is_empty() {
                return Err(anyhow!("'shell_script' must not be empty"));
            }
            Ok(Target::Shell {
                command: script.to_string(),
                shell: PathBuf::from("/bin/sh"),
            })
        }
        "http" => {
            let url_str = http_url
                .ok_or_else(|| anyhow!("'http_url' is required when target_type is 'http'"))?;
            let method = parse_http_method(http_method.unwrap_or("GET"))?;
            let url = url_str.to_string();
            Ok(Target::Http(HttpTarget {
                method,
                url,
                headers: BTreeMap::new(),
                body: None,
                body_file: None,
                success_statuses: Vec::new(),
                follow_redirects: true,
            }))
        }
        other => Err(anyhow!(
            "unsupported target_type: '{other}', expected 'process', 'shell', or 'http'"
        )),
    }
}

fn tool_add_job(paths: &StatePaths, args: &Value) -> Result<Value> {
    let name = args
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing required parameter: name"))?;
    let schedule_type = args
        .get("schedule_type")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing required parameter: schedule_type"))?;
    let schedule_expr = args
        .get("schedule_expr")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing required parameter: schedule_expr"))?;
    let timezone = args.get("timezone").and_then(Value::as_str);
    let target_type = args
        .get("target_type")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing required parameter: target_type"))?;
    let command = args.get("command").and_then(Value::as_array);
    let shell_script = args.get("shell_script").and_then(Value::as_str);
    let http_url = args.get("http_url").and_then(Value::as_str);
    let http_method = args.get("http_method").and_then(Value::as_str);

    let overlap_policy = match args.get("overlap_policy").and_then(Value::as_str) {
        Some("replace") => OverlapPolicy::Replace,
        Some("allow") => OverlapPolicy::Allow,
        Some("skip") => OverlapPolicy::Skip,
        Some(other) => {
            return Err(anyhow!(
                "invalid overlap_policy: '{other}', expected 'skip', 'replace', or 'allow'"
            ));
        }
        None => OverlapPolicy::Skip,
    };
    let missed_run_policy = match args.get("missed_run_policy").and_then(Value::as_str) {
        Some("latest") => MissedRunPolicy::Latest,
        Some("all") => MissedRunPolicy::All,
        Some("skip") => MissedRunPolicy::Skip,
        Some(other) => {
            return Err(anyhow!(
                "invalid missed_run_policy: '{other}', expected 'skip', 'latest', or 'all'"
            ));
        }
        None => MissedRunPolicy::Skip,
    };
    let max_retries = parse_retry_count(args.get("max_retries"))?;
    let timeout_seconds = parse_timeout_seconds(args.get("timeout_seconds"))?;
    let description = args
        .get("description")
        .and_then(Value::as_str)
        .map(String::from);
    let tags: Vec<String> = args
        .get("tags")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();
    let dry_run = args
        .get("dry_run")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let now = Timestamp::from_epoch_micros(now_us());
    let schedule = parse_schedule(schedule_type, schedule_expr, timezone, now)?;
    let target = parse_target(
        target_type,
        command.map(Vec::as_slice),
        shell_script,
        http_url,
        http_method,
    )?;

    // Start from the core defaults so unexposed policy fields (retry delays,
    // backoff mode, termination grace, timeout) match CLI-created jobs, then
    // apply only the documented overrides.
    let mut policy = ExecutionPolicy {
        overlap: overlap_policy,
        missed_run: missed_run_policy,
        retries: max_retries,
        ..ExecutionPolicy::default()
    };
    if let Some(seconds) = timeout_seconds {
        policy.timeout = Some(Duration::from_secs(seconds).into());
    }
    if policy.overlap == OverlapPolicy::Allow {
        policy.per_job_concurrency = 2;
    }
    if matches!(schedule, Schedule::At { .. }) && args.get("missed_run_policy").is_none() {
        policy.missed_run = MissedRunPolicy::Latest;
    }

    validate_metadata(name, description.as_deref(), &tags)?;

    let definition = JobDefinition {
        schedule,
        target,
        cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")),
        environment: Environment::default(),
        policy,
    };

    let global_concurrency = configured_global_concurrency(paths)?;
    definition.validate(global_concurrency)?;

    if dry_run {
        return Ok(json!({
            "dry_run": true,
            "name": name,
            "description": description,
            "tags": tags,
            "definition": redact_definition(serde_json::to_value(&definition)?),
            "warnings": environment_warnings(&definition.environment)
        }));
    }

    let store = open(paths)?;
    let record = store.create_job(&CreateJob {
        id: JobId::new().to_string(),
        name: name.to_string(),
        description,
        tags_json: serde_json::to_string(&tags)?,
        enabled: true,
        definition_json: serde_json::to_string(&definition)?,
        now_us: now.epoch_micros(),
        cursor_us: now.epoch_micros(),
    })?;

    send_wake(paths);
    redacted_job(record)
}

fn tool_update_job(paths: &StatePaths, args: &Value) -> Result<Value> {
    let job_name_or_id = args
        .get("job")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing required parameter: job"))?;
    let dry_run = args
        .get("dry_run")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let store = if dry_run {
        open_read_only(paths)?
    } else {
        open(paths)?
    };
    let existing = store.job(job_name_or_id)?;
    let mut def: JobDefinition = serde_json::from_str(&existing.definition_json)?;
    let now = Timestamp::from_epoch_micros(now_us());
    let mut schedule_changed = false;

    // Schedule updates
    if let Some(schedule_type) = args.get("schedule_type").and_then(Value::as_str) {
        let schedule_expr = args
            .get("schedule_expr")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("'schedule_expr' is required when updating 'schedule_type'"))?;
        let timezone = args.get("timezone").and_then(Value::as_str);
        def.schedule = parse_schedule(schedule_type, schedule_expr, timezone, now)?;
        schedule_changed = true;
    } else if let Some(schedule_expr) = args.get("schedule_expr").and_then(Value::as_str) {
        let timezone = args.get("timezone").and_then(Value::as_str);
        match &def.schedule {
            Schedule::Cron { .. } => {
                def.schedule = parse_schedule("cron", schedule_expr, timezone, now)?;
            }
            Schedule::Every { .. } => {
                def.schedule = parse_schedule("interval", schedule_expr, timezone, now)?;
            }
            Schedule::At { .. } => {
                def.schedule = parse_schedule("at", schedule_expr, timezone, now)?;
            }
        }
        schedule_changed = true;
    }

    // Target updates
    if let Some(target_type) = args.get("target_type").and_then(Value::as_str) {
        let command = args.get("command").and_then(Value::as_array);
        let shell_script = args.get("shell_script").and_then(Value::as_str);
        let http_url = args.get("http_url").and_then(Value::as_str);
        let http_method = args.get("http_method").and_then(Value::as_str);
        def.target = parse_target(
            target_type,
            command.map(Vec::as_slice),
            shell_script,
            http_url,
            http_method,
        )?;
    }

    // Policy updates
    if let Some(overlap_str) = args.get("overlap_policy").and_then(Value::as_str) {
        let normalized = match overlap_str {
            "replace" => OverlapPolicy::Replace,
            "allow" => OverlapPolicy::Allow,
            "skip" => OverlapPolicy::Skip,
            other => {
                return Err(anyhow!(
                    "invalid overlap_policy: '{other}', expected 'skip', 'replace', or 'allow'"
                ));
            }
        };
        if normalized != def.policy.overlap {
            def.policy.overlap = normalized;
            def.policy.per_job_concurrency = if normalized == OverlapPolicy::Allow {
                2
            } else {
                1
            };
        }
    }
    if let Some(missed_str) = args.get("missed_run_policy").and_then(Value::as_str) {
        def.policy.missed_run = match missed_str {
            "latest" => MissedRunPolicy::Latest,
            "all" => MissedRunPolicy::All,
            "skip" => MissedRunPolicy::Skip,
            other => {
                return Err(anyhow!(
                    "invalid missed_run_policy: '{other}', expected 'skip', 'latest', or 'all'"
                ));
            }
        };
    }
    if let Some(retries) = args.get("max_retries") {
        def.policy.retries = parse_retry_count(Some(retries))?;
    }
    if let Some(timeout_seconds) = args.get("timeout_seconds") {
        def.policy.timeout =
            parse_timeout_seconds(Some(timeout_seconds))?.map(|s| Duration::from_secs(s).into());
    }

    // Metadata updates
    let new_name = args
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or(&existing.name)
        .to_string();
    let new_description = match args.get("description") {
        Some(Value::String(s)) => Some(s.clone()),
        Some(Value::Null) => None,
        None => existing.description.clone(),
        _ => existing.description.clone(),
    };
    let new_tags: Vec<String> = match args.get("tags").and_then(Value::as_array) {
        Some(arr) => arr
            .iter()
            .filter_map(Value::as_str)
            .map(String::from)
            .collect(),
        None => serde_json::from_str(&existing.tags_json).unwrap_or_default(),
    };
    let new_enabled = args
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(existing.enabled);

    validate_metadata(&new_name, new_description.as_deref(), &new_tags)?;
    let global_concurrency = configured_global_concurrency(paths)?;
    def.validate(global_concurrency)?;

    if dry_run {
        return Ok(json!({
            "dry_run": true,
            "job": existing.name,
            "updated": {
                "name": new_name,
                "description": new_description,
                "tags": new_tags,
                "enabled": new_enabled,
                "definition": redact_definition(serde_json::to_value(&def)?)
            }
        }));
    }

    let updated = store.update_job(&UpdateJob {
        id: existing.id.clone(),
        expected_revision: existing.current_revision,
        name: new_name,
        description: new_description,
        tags_json: serde_json::to_string(&new_tags)?,
        enabled: new_enabled,
        definition_json: serde_json::to_string(&def)?,
        now_us: now.epoch_micros(),
        cursor_us: if schedule_changed {
            now.epoch_micros()
        } else {
            existing.cursor_us
        },
    })?;

    send_wake(paths);
    redacted_job(updated)
}

fn tool_enable_job(paths: &StatePaths, args: &Value) -> Result<Value> {
    let job = args
        .get("job")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing required parameter: job"))?;
    let dry_run = args
        .get("dry_run")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let store = if dry_run {
        open_read_only(paths)?
    } else {
        open(paths)?
    };
    let record = store.job(job)?;
    if dry_run {
        return Ok(json!({
            "dry_run": true,
            "job": job,
            "enabled": true,
            "changed": !record.enabled
        }));
    }
    store.set_enabled(job, true, now_us())?;
    send_wake(paths);
    Ok(json!({ "job": job, "enabled": true }))
}

fn tool_disable_job(paths: &StatePaths, args: &Value) -> Result<Value> {
    let job = args
        .get("job")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing required parameter: job"))?;
    let dry_run = args
        .get("dry_run")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let store = if dry_run {
        open_read_only(paths)?
    } else {
        open(paths)?
    };
    let record = store.job(job)?;
    if dry_run {
        return Ok(json!({
            "dry_run": true,
            "job": job,
            "enabled": false,
            "changed": record.enabled
        }));
    }
    store.set_enabled(job, false, now_us())?;
    send_wake(paths);
    Ok(json!({ "job": job, "enabled": false }))
}

fn tool_remove_job(paths: &StatePaths, args: &Value) -> Result<Value> {
    let job = args
        .get("job")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing required parameter: job"))?;
    let dry_run = args
        .get("dry_run")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let store = if dry_run {
        open_read_only(paths)?
    } else {
        open(paths)?
    };
    store.job(job)?;
    if dry_run {
        return Ok(json!({ "dry_run": true, "job": job, "would_remove": true }));
    }
    store.remove_job(job, now_us())?;
    send_wake(paths);
    Ok(json!({ "job": job, "removed": true }))
}

async fn tool_run_job(paths: &StatePaths, args: Value) -> Result<Value> {
    let job = args
        .get("job")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing required parameter: job"))?;
    let wait = args.get("wait").and_then(Value::as_bool).unwrap_or(false);
    let timeout_seconds = args
        .get("timeout_seconds")
        .and_then(Value::as_u64)
        .unwrap_or(30);
    let dry_run = args
        .get("dry_run")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let store = if dry_run {
        open_read_only(paths)?
    } else {
        open(paths)?
    };
    let job_rec = store.job(job)?;

    if dry_run {
        let def: JobDefinition = serde_json::from_str(&job_rec.definition_json)?;
        let active = store
            .history(Some(&job_rec.name), 100)?
            .into_iter()
            .filter(|r| {
                matches!(
                    r.state.as_str(),
                    "queued" | "starting" | "running" | "retry_wait"
                )
            })
            .count();
        let decision = if active == 0 {
            "eligible"
        } else {
            match def.policy.overlap {
                OverlapPolicy::Skip => "would_skip_overlap",
                OverlapPolicy::Replace => "would_replace",
                OverlapPolicy::Allow => "eligible_subject_to_capacity",
            }
        };
        return Ok(json!({
            "dry_run": true,
            "eligible": active == 0 || def.policy.overlap != OverlapPolicy::Skip,
            "decision": decision,
        }));
    }

    let run_id = Uuid::now_v7().to_string();
    let run = store.enqueue_manual(job, &run_id, now_us())?;
    send_wake(paths);

    if wait {
        let deadline = Instant::now() + Duration::from_secs(timeout_seconds);
        let mut current_run = run;
        while Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(50)).await;
            current_run = store.run(&run_id)?;
            if terminal_run_state(&current_run.state) {
                break;
            }
        }
        let completed = terminal_run_state(&current_run.state);
        Ok(json!({
            "run_id": current_run.id,
            "state": current_run.state,
            "reason": current_run.reason,
            "completed": completed,
        }))
    } else {
        Ok(json!({
            "run_id": run.id,
            "state": run.state,
        }))
    }
}

fn tool_cancel_run(paths: &StatePaths, args: &Value) -> Result<Value> {
    let run_id = args
        .get("run_id")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing required parameter: run_id"))?;
    let acknowledge_unconfirmed = args
        .get("acknowledge_unconfirmed")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let dry_run = args
        .get("dry_run")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Uuid::parse_str(run_id).context("invalid run UUID")?;

    let store = if dry_run {
        open_read_only(paths)?
    } else {
        open(paths)?
    };
    if dry_run {
        let run = store.run(run_id)?;
        let active = matches!(
            run.state.as_str(),
            "queued" | "starting" | "running" | "retry_wait"
        );
        let already_requested = active && store.cancellation_requested(run_id)?;
        return Ok(json!({
            "dry_run": true,
            "run_id": run_id,
            "state": run.state,
            "would_request_cancellation": active && !already_requested
        }));
    }
    let outcome = store.cancel_with_acknowledgement(run_id, now_us(), acknowledge_unconfirmed)?;
    send_wake(paths);

    let data = match outcome {
        CancelOutcome::CancelledBeforeExecution => {
            json!({"run_id": run_id, "requested": true, "cancelled": true, "before_execution": true})
        }
        CancelOutcome::CancellationRequested => {
            json!({"run_id": run_id, "requested": true})
        }
        CancelOutcome::AcknowledgedUnconfirmed => {
            json!({"run_id": run_id, "acknowledged_unconfirmed": true, "state": "interrupted_unknown"})
        }
    };
    Ok(data)
}

fn tool_get_logs(paths: &StatePaths, args: &Value) -> Result<String> {
    let store = open(paths)?;
    let job_arg = args.get("job").and_then(Value::as_str);
    let run_arg = args.get("run_id").and_then(Value::as_str);
    let tail_lines = args
        .get("tail_lines")
        .and_then(Value::as_u64)
        .map_or(100, |n| n as usize);

    let run_id = if let Some(r) = run_arg {
        r.to_string()
    } else if let Some(j) = job_arg {
        let runs = store.history(Some(j), 1)?;
        runs.first()
            .ok_or_else(|| anyhow!("no runs found for job '{j}'"))?
            .id
            .clone()
    } else {
        return Err(anyhow!("provide either 'job' or 'run_id'"));
    };

    let final_path = paths.final_output(&run_id, 1)?;
    let partial_path = paths.partial_output(&run_id, 1)?;
    let frames = match locron_engine::read_frames(&final_path) {
        Ok(frames) => frames,
        Err(_) => locron_engine::read_frames(&partial_path).unwrap_or_default(),
    };

    if frames.is_empty() {
        return Ok(format!("[No captured logs found for run {run_id}]"));
    }

    let mut full_output = String::new();
    for frame in frames {
        full_output.push_str(&String::from_utf8_lossy(&frame.payload));
    }

    let lines: Vec<&str> = full_output.lines().collect();
    let result = if lines.len() > tail_lines {
        lines[lines.len() - tail_lines..].join("\n")
    } else {
        full_output
    };

    Ok(result)
}

fn tool_why(paths: &StatePaths, args: &Value) -> Result<Value> {
    let store = open(paths)?;
    let job_arg = args.get("job").and_then(Value::as_str);
    let run_arg = args.get("run_id").and_then(Value::as_str);

    if let Some(run_id) = run_arg {
        let events = store.events_for_run(run_id)?;
        let run = redacted_observable_run(&store, store.run(run_id)?)?;
        return Ok(json!({
            "run": run,
            "events": events,
            "daemon_running": !daemon_lock_free(paths),
            "explanation": "terminal reason, immutable snapshot, ordered attempts, and audit events are durable facts"
        }));
    }

    if let Some(target) = job_arg {
        let job = store.job(target)?;
        let definition: JobDefinition = serde_json::from_str(&job.definition_json)?;
        let next = definition
            .schedule
            .next(Timestamp::from_epoch_micros(now_us()), 1)?
            .first()
            .map(ToString::to_string);
        let active = store
            .history(Some(&job.name), 100)?
            .into_iter()
            .filter(|run| {
                matches!(
                    run.state.as_str(),
                    "queued" | "starting" | "running" | "retry_wait"
                )
            })
            .map(redacted_run)
            .collect::<Result<Vec<_>>>()?;
        let job_val = redacted_job(job)?;
        return Ok(json!({
            "job": job_val,
            "next_occurrence": next,
            "active_runs": active,
            "overlap": definition.policy.overlap,
            "daemon_running": !daemon_lock_free(paths),
            "explanation": "facts are read from durable state; unknown execution facts are not inferred"
        }));
    }

    Err(anyhow!("provide either 'job' or 'run_id'"))
}

fn tool_preview_schedule(args: &Value) -> Result<Value> {
    let schedule_type = args
        .get("schedule_type")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing required parameter: schedule_type"))?;
    let schedule_expr = args
        .get("schedule_expr")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing required parameter: schedule_expr"))?;
    let timezone = args.get("timezone").and_then(Value::as_str);
    let count = args
        .get("count")
        .and_then(Value::as_u64)
        .map_or(5, |n| (n as usize).clamp(1, 20));

    let now = Timestamp::from_epoch_micros(now_us());
    let schedule = parse_schedule(schedule_type, schedule_expr, timezone, now)?;
    let occurrences = schedule.next(now, count)?;
    let strings: Vec<String> = occurrences.into_iter().map(|t| t.to_string()).collect();
    Ok(json!(strings))
}

fn tool_doctor(paths: &StatePaths) -> Result<Value> {
    let store = open(paths)?;
    let settings = store.settings()?;
    let mut resolutions = Vec::new();
    for job in store.list_jobs(true)? {
        let definition: JobDefinition = serde_json::from_str(&job.definition_json)?;
        let requested = match &definition.target {
            Target::Process { executable, .. } => executable.clone(),
            Target::Shell { shell, .. } => shell.display().to_string(),
            Target::Http(_) => continue,
        };
        let diagnostic_attempt = AdmitAttempt {
            run_id: Uuid::nil().to_string(),
            job_id: job.id.clone(),
            attempt_number: 1,
            trigger: "diagnostic".into(),
            nominal_us: None,
            snapshot_json: String::new(),
        };
        match engine_target(&definition, &diagnostic_attempt, &settings) {
            Ok(locron_engine::TargetSpec::Process(process)) => resolutions.push(json!({
                "job_id": job.id,
                "job_name": job.name,
                "requested_executable": requested,
                "effective_path": process.env.get("PATH"),
                "resolved_executable": process.executable,
                "status": "resolved"
            })),
            Ok(locron_engine::TargetSpec::Http(_)) => unreachable!(),
            Err(error) => resolutions.push(json!({
                "job_id": job.id,
                "job_name": job.name,
                "requested_executable": requested,
                "status": "unresolved",
                "error": error
            })),
        }
    }
    Ok(json!({
        "state_dir": paths.root,
        "database": paths.database,
        "daemon_running": !daemon_lock_free(paths),
        "wake_socket": paths.wake_socket.exists(),
        "execution_path": settings.execution_path,
        "global_environment_names": settings.environment.keys().collect::<Vec<_>>(),
        "process_resolution": resolutions,
        "checks": store.integrity_check()?
    }))
}

fn handle_resources_list() -> Value {
    json!({
        "resources": [
            {
                "uri": "locron://jobs",
                "name": "All Jobs",
                "description": "JSON array of all registered jobs and current schedules",
                "mimeType": "application/json"
            },
            {
                "uri": "locron://doctor",
                "name": "System Health and Diagnostics",
                "description": "System health, daemon status, and database diagnostics",
                "mimeType": "application/json"
            }
        ],
        "resourceTemplates": [
            {
                "uriTemplate": "locron://jobs/{job_id_or_name}",
                "name": "Job Details",
                "description": "Detailed JSON descriptor of a specific job",
                "mimeType": "application/json"
            },
            {
                "uriTemplate": "locron://history/{run_id}",
                "name": "Run History",
                "description": "Detailed JSON outcome, attempt breakdown, timestamps, and exit code",
                "mimeType": "application/json"
            },
            {
                "uriTemplate": "locron://logs/{run_id}",
                "name": "Run Logs",
                "description": "Raw captured stdout/stderr output stream",
                "mimeType": "text/plain"
            }
        ]
    })
}

fn handle_resources_read(paths: &StatePaths, id: Value, params: &Value) -> JsonRpcResponse {
    let Some(uri) = params.get("uri").and_then(Value::as_str) else {
        return JsonRpcResponse::error(id, -32602, "Invalid params: missing 'uri'");
    };

    let result = if uri == "locron://jobs" {
        tool_list_jobs(paths, &json!({})).and_then(|v| {
            serde_json::to_string_pretty(&v)
                .map(|text| ("application/json", text))
                .map_err(Into::into)
        })
    } else if uri == "locron://doctor" {
        tool_doctor(paths).and_then(|v| {
            serde_json::to_string_pretty(&v)
                .map(|text| ("application/json", text))
                .map_err(Into::into)
        })
    } else if let Some(job_name_or_id) = uri.strip_prefix("locron://jobs/") {
        tool_get_job(paths, &json!({ "job": job_name_or_id })).and_then(|v| {
            serde_json::to_string_pretty(&v)
                .map(|text| ("application/json", text))
                .map_err(Into::into)
        })
    } else if let Some(run_id) = uri.strip_prefix("locron://history/") {
        (|| -> Result<(&'static str, String)> {
            let store = open(paths)?;
            let run = redacted_observable_run(&store, store.run(run_id)?)?;
            let events = store.events_for_run(run_id)?;
            let val = json!({ "run": run, "events": events });
            Ok(("application/json", serde_json::to_string_pretty(&val)?))
        })()
    } else if let Some(run_id) = uri.strip_prefix("locron://logs/") {
        tool_get_logs(paths, &json!({ "run_id": run_id, "tail_lines": 1000 }))
            .map(|text| ("text/plain", text))
    } else {
        return JsonRpcResponse::error(id, -32602, format!("Resource not found: {uri}"));
    };

    match result {
        Ok((mime, text)) => JsonRpcResponse::success(
            id,
            json!({
                "contents": [
                    {
                        "uri": uri,
                        "mimeType": mime,
                        "text": text
                    }
                ]
            }),
        ),
        Err(err) => JsonRpcResponse::error(id, -32603, format!("Failed to read resource: {err:#}")),
    }
}

fn handle_prompts_list() -> Value {
    json!({
        "prompts": [
            {
                "name": "schedule_task",
                "description": "Interactive prompt assisting users in creating reliable scheduled jobs in locron",
                "arguments": [
                    {
                        "name": "task_description",
                        "description": "Description of what command, script, or HTTP endpoint needs to be scheduled",
                        "required": true
                    },
                    {
                        "name": "frequency",
                        "description": "Intended schedule frequency or timing (e.g., 'every 15 minutes', 'daily at 3am', 'cron 0 3 * * *')",
                        "required": false
                    }
                ]
            },
            {
                "name": "diagnose_failure",
                "description": "Troubleshoot a failed job or run using execution logs, error output, and causal diagnostics",
                "arguments": [
                    {
                        "name": "job_or_run_id",
                        "description": "Job name or run UUID to diagnose",
                        "required": true
                    }
                ]
            }
        ]
    })
}

fn handle_prompts_get(id: Value, params: &Value) -> JsonRpcResponse {
    let Some(name) = params.get("name").and_then(Value::as_str) else {
        return JsonRpcResponse::error(id, -32602, "Invalid params: missing prompt name");
    };
    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);

    match name {
        "schedule_task" => {
            let task_desc = arguments
                .get("task_description")
                .and_then(Value::as_str)
                .unwrap_or("");
            let frequency = arguments
                .get("frequency")
                .and_then(Value::as_str)
                .unwrap_or("unspecified");
            let prompt_text = format!(
                "Please help configure and register a local scheduled job using locron.\n\n\
                Task Description: {task_desc}\n\
                Target Frequency / Schedule: {frequency}\n\n\
                Guidelines for locron:\n\
                1. Choose an appropriate schedule type:\n   \
                   - 'cron' for calendar-aligned schedules (e.g. '0 3 * * *', with optional timezone like 'UTC' or 'Asia/Seoul').\n   \
                   - 'interval' for recurring intervals (e.g. '15m', '1h', '30s').\n   \
                   - 'at' for one-time execution at a future ISO 8601 timestamp.\n\
                2. Choose target type: 'process' (with command array), 'shell' (with shell_script), or 'http' (with http_url and http_method).\n\
                3. Consider overlap policy ('skip' default, 'replace' if newest should cancel active, 'allow' for concurrent).\n\
                4. You can preview schedules using `locron_preview_schedule` and register the job using `locron_add_job`."
            );
            JsonRpcResponse::success(
                id,
                json!({
                    "description": "Prompt to assist in creating a scheduled job in locron",
                    "messages": [
                        {
                            "role": "user",
                            "content": {
                                "type": "text",
                                "text": prompt_text
                            }
                        }
                    ]
                }),
            )
        }
        "diagnose_failure" => {
            let target = arguments
                .get("job_or_run_id")
                .and_then(Value::as_str)
                .unwrap_or("");
            let prompt_text = format!(
                "Please diagnose the failure for locron job or run '{target}'.\n\n\
                Troubleshooting Steps:\n\
                1. Use `locron_why` with job or run_id to inspect the durable state, terminal reason, and lifecycle events.\n\
                2. Use `locron_get_logs` to inspect the stderr output, stdout, or HTTP response body for the failed run.\n\
                3. Analyze the root cause (e.g. non-zero exit code, command not found, timeout, HTTP error, permission denied) and explain why it failed.\n\
                4. Recommend concrete steps to resolve the issue."
            );
            JsonRpcResponse::success(
                id,
                json!({
                    "description": "Prompt to troubleshoot a failed job or run in locron",
                    "messages": [
                        {
                            "role": "user",
                            "content": {
                                "type": "text",
                                "text": prompt_text
                            }
                        }
                    ]
                }),
            )
        }
        unknown => JsonRpcResponse::error(id, -32601, format!("Unknown prompt: {unknown}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_paths() -> (tempfile::TempDir, StatePaths) {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = StatePaths::new(dir.path().to_path_buf());
        (dir, paths)
    }

    fn request(id: Option<u64>, method: &str, params: Value) -> JsonRpcRequest {
        JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: id.map(|id| json!(id)),
            method: method.into(),
            params: Some(params),
        }
    }

    fn call(params: Value) -> JsonRpcRequest {
        request(Some(1), "tools/call", params)
    }

    /// Returns the tool result object embedded in a tools/call response.
    fn tool_result(response: JsonRpcResponse) -> Value {
        let result = response.result.expect("tool call succeeded");
        let text = result["content"][0]["text"].as_str().expect("text content");
        serde_json::from_str(text).expect("tool result is JSON")
    }

    fn tool_is_error(response: &JsonRpcResponse) -> bool {
        response
            .result
            .as_ref()
            .and_then(|result| result.get("isError"))
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    async fn request_tool(paths: &StatePaths, tool: &str, arguments: Value) -> Value {
        let response = handle_request(paths, call(json!({"name": tool, "arguments": arguments})))
            .await
            .expect("response");
        assert!(!tool_is_error(&response), "tool {tool} failed");
        tool_result(response)
    }

    fn valid_add_args(name: &str) -> Value {
        json!({
            "name": name,
            "schedule_type": "interval",
            "schedule_expr": "15m",
            "target_type": "process",
            "command": ["/bin/echo", "hello"],
            "description": "test job",
            "tags": ["test", "ops"]
        })
    }

    async fn add_job(paths: &StatePaths, name: &str) -> Value {
        request_tool(paths, "locron_add_job", valid_add_args(name)).await
    }

    #[tokio::test]
    async fn jsonrpc_request_deserialization() {
        let request: JsonRpcRequest =
            serde_json::from_str(r#"{"jsonrpc":"2.0","id":7,"method":"ping","params":{}}"#)
                .expect("valid request parses");
        assert_eq!(request.jsonrpc, "2.0");
        assert_eq!(request.id, Some(json!(7)));
        assert_eq!(request.method, "ping");

        let notification: JsonRpcRequest =
            serde_json::from_str(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#)
                .expect("notification parses");
        assert_eq!(notification.id, None);

        let no_params: JsonRpcRequest =
            serde_json::from_str(r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#)
                .expect("omitted params parses");
        assert_eq!(no_params.params, None);
    }

    #[test]
    fn jsonrpc_response_serialization() {
        let success = JsonRpcResponse::success(json!(1), json!({"ok": true}));
        let success_json = serde_json::to_value(&success).expect("serializes");
        assert_eq!(success_json["jsonrpc"], "2.0");
        assert_eq!(success_json["id"], 1);
        assert_eq!(success_json["result"]["ok"], true);
        assert!(success_json.get("error").is_none());

        let error = JsonRpcResponse::error(json!(2), -32601, "Method not found: nope");
        let error_json = serde_json::to_value(&error).expect("serializes");
        assert_eq!(error_json["id"], 2);
        assert_eq!(error_json["error"]["code"], -32601);
        assert_eq!(error_json["error"]["message"], "Method not found: nope");
        assert!(error_json.get("result").is_none());
    }

    #[tokio::test]
    async fn initialize_handshake() {
        let (_dir, paths) = fresh_paths();
        let response = handle_request(&paths, request(Some(1), "initialize", json!({})))
            .await
            .expect("response");
        let result = response.result.expect("initialize result");
        assert_eq!(result["protocolVersion"], "2024-11-05");
        assert_eq!(result["serverInfo"]["name"], "locron");
        assert_eq!(result["serverInfo"]["version"], env!("CARGO_PKG_VERSION"));
        assert!(result["capabilities"]["tools"].is_object());
        assert!(result["capabilities"]["resources"].is_object());
        assert!(result["capabilities"]["prompts"].is_object());
    }

    #[tokio::test]
    async fn ping_responds_empty() {
        let (_dir, paths) = fresh_paths();
        let response = handle_request(&paths, request(Some(1), "ping", json!({})))
            .await
            .expect("response");
        assert_eq!(response.result.expect("ping result"), json!({}));
    }

    #[tokio::test]
    async fn unknown_method_returns_32601() {
        let (_dir, paths) = fresh_paths();
        let response = handle_request(&paths, request(Some(1), "frobnicate", json!({})))
            .await
            .expect("response");
        let error = response.error.expect("error");
        assert_eq!(error.code, -32601);
    }

    #[tokio::test]
    async fn invalid_jsonrpc_returns_32600() {
        let (_dir, paths) = fresh_paths();
        let mut invalid = request(Some(1), "ping", json!({}));
        invalid.jsonrpc = "1.0".into();
        let response = handle_request(&paths, invalid).await.expect("response");
        assert_eq!(response.error.expect("error").code, -32600);
    }

    #[tokio::test]
    async fn notifications_are_not_answered() {
        let (_dir, paths) = fresh_paths();
        let response = handle_request(
            &paths,
            request(None, "notifications/initialized", json!({})),
        )
        .await;
        assert!(response.is_none());
    }

    #[tokio::test]
    async fn tools_list_exposes_all_thirteen_tools() {
        let (_dir, paths) = fresh_paths();
        let response = handle_request(&paths, request(Some(1), "tools/list", json!({})))
            .await
            .expect("response");
        let result = response.result.expect("result");
        let tools = result["tools"].as_array().expect("tools array");
        let names = tools
            .iter()
            .map(|tool| tool["name"].as_str().expect("name"))
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            [
                "locron_list_jobs",
                "locron_get_job",
                "locron_add_job",
                "locron_update_job",
                "locron_enable_job",
                "locron_disable_job",
                "locron_remove_job",
                "locron_run_job",
                "locron_cancel_run",
                "locron_get_logs",
                "locron_why",
                "locron_preview_schedule",
                "locron_doctor",
            ]
        );
        for tool in tools {
            let schema = &tool["inputSchema"];
            assert_eq!(schema["type"], "object", "schema for {}", tool["name"]);
            assert!(schema["properties"].is_object());
        }
        let add = &tools[2];
        let required = add["inputSchema"]["required"].as_array().expect("required");
        let required = required
            .iter()
            .map(|value| value.as_str().expect("required string"))
            .collect::<Vec<_>>();
        assert_eq!(
            required,
            ["name", "schedule_type", "schedule_expr", "target_type"]
        );
        let preview = &tools[11];
        assert_eq!(
            preview["inputSchema"]["properties"]["schedule_type"]["enum"],
            json!(["cron", "interval"])
        );
        assert_eq!(preview["inputSchema"]["properties"]["count"]["maximum"], 20);
        let mutating = [
            "locron_enable_job",
            "locron_disable_job",
            "locron_remove_job",
        ];
        for tool in tools {
            if mutating.contains(&tool["name"].as_str().expect("name")) {
                assert!(
                    tool["inputSchema"]["properties"].get("dry_run").is_some(),
                    "{} must advertise dry_run",
                    tool["name"]
                );
            }
        }
    }

    #[tokio::test]
    async fn preview_schedule_clamps_count() {
        let (_dir, paths) = fresh_paths();
        let result = request_tool(
            &paths,
            "locron_preview_schedule",
            json!({"schedule_type": "interval", "schedule_expr": "1h", "count": 100}),
        )
        .await;
        assert_eq!(result.as_array().expect("array").len(), 20);

        let result = request_tool(
            &paths,
            "locron_preview_schedule",
            json!({"schedule_type": "interval", "schedule_expr": "1h", "count": 0}),
        )
        .await;
        assert_eq!(result.as_array().expect("array").len(), 1);

        let result = request_tool(
            &paths,
            "locron_preview_schedule",
            json!({"schedule_type": "cron", "schedule_expr": "0 3 * * *", "timezone": "UTC"}),
        )
        .await;
        let occurrences = result.as_array().expect("array");
        assert!(!occurrences.is_empty());
        assert!(occurrences[0].as_str().expect("timestamp").contains('T'));
    }

    #[tokio::test]
    async fn preview_schedule_rejects_invalid_expression() {
        let (_dir, paths) = fresh_paths();
        let response = handle_request(
            &paths,
            call(json!({
                "name": "locron_preview_schedule",
                "arguments": {"schedule_type": "cron", "schedule_expr": "not a cron"}
            })),
        )
        .await
        .expect("response");
        assert!(tool_is_error(&response), "invalid cron must fail");
    }

    #[tokio::test]
    async fn add_job_dry_run_does_not_persist() {
        let (_dir, paths) = fresh_paths();
        let mut args = valid_add_args("backup");
        args["dry_run"] = json!(true);
        let result = request_tool(&paths, "locron_add_job", args).await;
        assert_eq!(result["dry_run"], true);
        assert_eq!(result["name"], "backup");

        let store = open(&paths).expect("store");
        assert!(
            store.job("backup").is_err(),
            "dry run must not create the job"
        );
    }

    #[tokio::test]
    async fn add_job_persists_and_is_listed() {
        let (_dir, paths) = fresh_paths();
        let result = add_job(&paths, "backup").await;
        assert_eq!(result["name"], "backup");
        assert_eq!(result["enabled"], true);
        let id = result["id"].as_str().expect("job id").to_owned();
        assert!(Uuid::parse_str(&id).is_ok());

        let listed = request_tool(&paths, "locron_list_jobs", json!({})).await;
        let jobs = listed.as_array().expect("jobs array");
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0]["name"], "backup");
        assert!(jobs[0]["next_occurrence"].is_string());

        let filtered =
            request_tool(&paths, "locron_list_jobs", json!({"tag": "missing-tag"})).await;
        assert!(filtered.as_array().expect("array").is_empty());

        let got = request_tool(&paths, "locron_get_job", json!({"job": "backup"})).await;
        assert_eq!(got["job"]["id"], id);
    }

    #[tokio::test]
    async fn add_job_rejects_invalid_parameters() {
        let (_dir, paths) = fresh_paths();
        let mut retries = valid_add_args("backup");
        retries["max_retries"] = json!(11);
        let response = handle_request(
            &paths,
            call(json!({"name": "locron_add_job", "arguments": retries})),
        )
        .await
        .expect("response");
        assert!(tool_is_error(&response));
        let text = response.result.unwrap()["content"][0]["text"].clone();
        assert!(text.as_str().unwrap().contains("0 through 10"));

        let mut overlap = valid_add_args("backup");
        overlap["overlap_policy"] = json!("bogus");
        let response = handle_request(
            &paths,
            call(json!({"name": "locron_add_job", "arguments": overlap})),
        )
        .await
        .expect("response");
        assert!(tool_is_error(&response));

        let mut no_command = valid_add_args("backup");
        no_command["command"] = json!([]);
        let response = handle_request(
            &paths,
            call(json!({"name": "locron_add_job", "arguments": no_command})),
        )
        .await
        .expect("response");
        assert!(tool_is_error(&response));
    }

    #[tokio::test]
    async fn add_job_missing_required_parameter_is_error() {
        let (_dir, paths) = fresh_paths();
        let response = handle_request(
            &paths,
            call(json!({"name": "locron_add_job", "arguments": {"name": "x"}})),
        )
        .await
        .expect("response");
        assert!(tool_is_error(&response));
    }

    #[tokio::test]
    async fn at_schedule_defaults_to_latest_missed_run() {
        let (_dir, paths) = fresh_paths();
        let mut args = json!({
            "name": "one-shot",
            "schedule_type": "at",
            "schedule_expr": "2099-01-01T09:00:00+09:00",
            "target_type": "shell",
            "shell_script": "echo one"
        });
        let result = request_tool(&paths, "locron_add_job", args.clone()).await;
        let definition: JobDefinition =
            serde_json::from_str(result["definition_json"].as_str().expect("definition")).unwrap();
        assert_eq!(definition.policy.missed_run, MissedRunPolicy::Latest);

        args["name"] = json!("one-shot-explicit");
        args["missed_run_policy"] = json!("skip");
        let result = request_tool(&paths, "locron_add_job", args).await;
        let definition: JobDefinition =
            serde_json::from_str(result["definition_json"].as_str().expect("definition")).unwrap();
        assert_eq!(definition.policy.missed_run, MissedRunPolicy::Skip);
    }

    #[tokio::test]
    async fn overlap_allow_sets_per_job_concurrency() {
        let (_dir, paths) = fresh_paths();
        let mut args = valid_add_args("concurrent");
        args["overlap_policy"] = json!("allow");
        let result = request_tool(&paths, "locron_add_job", args).await;
        let definition: JobDefinition =
            serde_json::from_str(result["definition_json"].as_str().expect("definition")).unwrap();
        assert_eq!(definition.policy.overlap, OverlapPolicy::Allow);
        assert_eq!(definition.policy.per_job_concurrency, 2);
    }

    #[tokio::test]
    async fn update_job_applies_fields_and_dry_run() {
        let (_dir, paths) = fresh_paths();
        add_job(&paths, "backup").await;

        let args = json!({"job": "backup", "description": "updated", "dry_run": true});
        let result = request_tool(&paths, "locron_update_job", args).await;
        assert_eq!(result["dry_run"], true);
        assert_eq!(result["updated"]["description"], "updated");

        let store = open(&paths).expect("store");
        let job = store.job("backup").expect("job");
        assert_eq!(
            job.description.as_deref(),
            Some("test job"),
            "dry run must not persist"
        );

        let args = json!({
            "job": "backup",
            "description": "updated",
            "rename": null,
            "overlap_policy": "allow",
            "max_retries": 3,
            "timeout_seconds": 90
        });
        let result = request_tool(&paths, "locron_update_job", args).await;
        assert_eq!(result["description"], "updated");
        let definition: JobDefinition =
            serde_json::from_str(result["definition_json"].as_str().expect("definition")).unwrap();
        assert_eq!(definition.policy.retries, 3);
        assert_eq!(definition.policy.overlap, OverlapPolicy::Allow);
        assert_eq!(definition.policy.per_job_concurrency, 2);
        assert_eq!(
            definition.policy.timeout,
            Some(Duration::from_secs(90).into())
        );
    }

    #[tokio::test]
    async fn lifecycle_tools_dry_run_then_apply() {
        let (_dir, paths) = fresh_paths();
        add_job(&paths, "backup").await;

        let result = request_tool(
            &paths,
            "locron_disable_job",
            json!({"job": "backup", "dry_run": true}),
        )
        .await;
        assert_eq!(result["dry_run"], true);
        assert_eq!(result["changed"], true);
        let store = open(&paths).expect("store");
        assert!(
            store.job("backup").expect("job").enabled,
            "dry run must not disable"
        );

        let result = request_tool(&paths, "locron_disable_job", json!({"job": "backup"})).await;
        assert_eq!(result["enabled"], false);
        assert!(!store.job("backup").expect("job").enabled);

        let result = request_tool(
            &paths,
            "locron_enable_job",
            json!({"job": "backup", "dry_run": true}),
        )
        .await;
        assert_eq!(result["changed"], true);
        let result = request_tool(&paths, "locron_enable_job", json!({"job": "backup"})).await;
        assert_eq!(result["enabled"], true);

        let result = request_tool(
            &paths,
            "locron_remove_job",
            json!({"job": "backup", "dry_run": true}),
        )
        .await;
        assert_eq!(result["would_remove"], true);
        let result = request_tool(&paths, "locron_remove_job", json!({"job": "backup"})).await;
        assert_eq!(result["removed"], true);
        assert!(store.job("backup").is_err(), "job must be removed");
    }

    #[tokio::test]
    async fn run_and_cancel_with_dry_run() {
        let (_dir, paths) = fresh_paths();
        add_job(&paths, "backup").await;

        let result = request_tool(
            &paths,
            "locron_run_job",
            json!({"job": "backup", "dry_run": true}),
        )
        .await;
        assert_eq!(result["dry_run"], true);
        assert_eq!(result["decision"], "eligible");

        let store = open(&paths).expect("store");
        assert!(
            store
                .history(Some("backup"), 10)
                .expect("history")
                .is_empty(),
            "dry run must not enqueue"
        );

        let result = request_tool(&paths, "locron_run_job", json!({"job": "backup"})).await;
        let run_id = result["run_id"].as_str().expect("run id").to_owned();
        assert_eq!(result["state"], "queued");

        let result = request_tool(
            &paths,
            "locron_cancel_run",
            json!({"run_id": run_id, "dry_run": true}),
        )
        .await;
        assert_eq!(result["would_request_cancellation"], true);
        assert_eq!(result["state"], "queued");

        let result = request_tool(&paths, "locron_cancel_run", json!({"run_id": run_id})).await;
        assert_eq!(result["requested"], true);
        assert_eq!(result["cancelled"], true);
        let run = store.run(&run_id).expect("run");
        assert_eq!(run.state, "cancelled");
    }

    #[tokio::test]
    async fn cancel_rejects_invalid_uuid() {
        let (_dir, paths) = fresh_paths();
        let response = handle_request(
            &paths,
            call(json!({"name": "locron_cancel_run", "arguments": {"run_id": "nope"}})),
        )
        .await
        .expect("response");
        assert!(tool_is_error(&response));
    }

    #[tokio::test]
    async fn why_reports_job_and_run_facts() {
        let (_dir, paths) = fresh_paths();
        add_job(&paths, "backup").await;
        let result = request_tool(&paths, "locron_run_job", json!({"job": "backup"})).await;
        let run_id = result["run_id"].as_str().expect("run id").to_owned();

        let why_job = request_tool(&paths, "locron_why", json!({"job": "backup"})).await;
        assert_eq!(why_job["job"]["name"], "backup");
        assert!(why_job["explanation"].is_string());

        let why_run = request_tool(&paths, "locron_why", json!({"run_id": run_id})).await;
        assert_eq!(why_run["run"]["id"], run_id);
        assert!(why_run["events"].is_array());
    }

    #[tokio::test]
    async fn get_logs_requires_run_context() {
        let (_dir, paths) = fresh_paths();
        let response = handle_request(
            &paths,
            call(json!({"name": "locron_get_logs", "arguments": {}})),
        )
        .await
        .expect("response");
        assert!(tool_is_error(&response));

        add_job(&paths, "backup").await;
        let result = request_tool(&paths, "locron_run_job", json!({"job": "backup"})).await;
        let run_id = result["run_id"].as_str().expect("run id").to_owned();
        let response = handle_request(
            &paths,
            call(json!({"name": "locron_get_logs", "arguments": {"run_id": run_id}})),
        )
        .await
        .expect("response");
        assert!(!tool_is_error(&response));
        let result = response.result.expect("result");
        let text = result["content"][0]["text"].as_str().expect("text logs");
        assert!(text.contains("No captured logs"));
    }

    #[tokio::test]
    async fn doctor_reports_state_and_checks() {
        let (_dir, paths) = fresh_paths();
        add_job(&paths, "backup").await;
        let result = request_tool(&paths, "locron_doctor", json!({})).await;
        assert!(result["checks"].is_array());
        assert!(result["state_dir"].is_string());
        assert!(result["database"].is_string());
        assert!(result["daemon_running"].is_boolean());
    }

    #[tokio::test]
    async fn unknown_tool_returns_32601() {
        let (_dir, paths) = fresh_paths();
        let response = handle_request(
            &paths,
            call(json!({"name": "locron_frobnicate", "arguments": {}})),
        )
        .await
        .expect("response");
        let error = response.error.expect("error");
        assert_eq!(error.code, -32601);
    }

    #[tokio::test]
    async fn tools_call_without_name_returns_32602() {
        let (_dir, paths) = fresh_paths();
        let response = handle_request(&paths, call(json!({"arguments": {}})))
            .await
            .expect("response");
        assert_eq!(response.error.expect("error").code, -32602);
    }

    #[tokio::test]
    async fn resources_list_and_read() {
        let (_dir, paths) = fresh_paths();
        add_job(&paths, "backup").await;
        let result = request_tool(&paths, "locron_run_job", json!({"job": "backup"})).await;
        let run_id = result["run_id"].as_str().expect("run id").to_owned();

        let response = handle_request(&paths, request(Some(1), "resources/list", json!({})))
            .await
            .expect("response");
        let result = response.result.expect("result");
        let names = result["resources"]
            .as_array()
            .expect("resources")
            .iter()
            .map(|r| r["uri"].as_str().expect("uri"))
            .collect::<Vec<_>>();
        assert_eq!(names, ["locron://jobs", "locron://doctor"]);
        let templates = result["resourceTemplates"]
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

        let read = |uri: &str| {
            handle_request(
                &paths,
                request(Some(1), "resources/read", json!({"uri": uri})),
            )
        };

        let response = read("locron://jobs").await.expect("response");
        let result = response.result.expect("result");
        let contents = result["contents"].as_array().expect("contents");
        assert_eq!(contents.len(), 1);
        assert_eq!(contents[0]["uri"], "locron://jobs");
        assert_eq!(contents[0]["mimeType"], "application/json");

        let response = read("locron://jobs/backup").await.expect("response");
        let result = response.result.expect("result");
        let text = result["contents"][0]["text"].as_str().expect("text");
        let job: Value = serde_json::from_str(text).expect("job json");
        assert_eq!(job["job"]["name"], "backup");

        let response = read(&format!("locron://history/{run_id}"))
            .await
            .expect("response");
        let result = response.result.expect("result");
        let text = result["contents"][0]["text"].as_str().expect("text");
        let history: Value = serde_json::from_str(text).expect("history json");
        assert_eq!(history["run"]["id"], run_id);

        let response = read(&format!("locron://logs/{run_id}"))
            .await
            .expect("response");
        assert_eq!(
            response.result.expect("result")["contents"][0]["mimeType"],
            "text/plain"
        );

        let response = read("locron://doctor").await.expect("response");
        assert_eq!(
            response.result.expect("result")["contents"][0]["mimeType"],
            "application/json"
        );

        let response = read("locron://unknown").await.expect("response");
        assert_eq!(response.error.expect("error").code, -32602);

        let response = handle_request(&paths, request(Some(1), "resources/read", json!({})))
            .await
            .expect("response");
        assert_eq!(response.error.expect("error").code, -32602);
    }

    #[tokio::test]
    async fn prompts_list_and_get() {
        let (_dir, paths) = fresh_paths();
        let response = handle_request(&paths, request(Some(1), "prompts/list", json!({})))
            .await
            .expect("response");
        let result = response.result.expect("result");
        let prompts = result["prompts"].as_array().expect("prompts");
        let names = prompts
            .iter()
            .map(|p| p["name"].as_str().expect("name"))
            .collect::<Vec<_>>();
        assert_eq!(names, ["schedule_task", "diagnose_failure"]);

        let response = handle_request(
            &paths,
            request(
                Some(1),
                "prompts/get",
                json!({"name": "schedule_task", "arguments": {"task_description": "backup"}}),
            ),
        )
        .await
        .expect("response");
        let result = response.result.expect("result");
        let messages = result["messages"].as_array().expect("messages");
        assert_eq!(messages.len(), 1);
        assert!(
            messages[0]["content"]["text"]
                .as_str()
                .unwrap()
                .contains("backup")
        );

        let response = handle_request(
            &paths,
            request(
                Some(1),
                "prompts/get",
                json!({"name": "diagnose_failure", "arguments": {"job_or_run_id": "backup"}}),
            ),
        )
        .await
        .expect("response");
        assert!(
            response.result.expect("result")["messages"][0]["content"]["text"]
                .as_str()
                .unwrap()
                .contains("backup")
        );

        let response = handle_request(
            &paths,
            request(Some(1), "prompts/get", json!({"name": "nope"})),
        )
        .await
        .expect("response");
        assert_eq!(response.error.expect("error").code, -32601);
    }
}
