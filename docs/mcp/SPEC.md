# locron MCP Specification

## Purpose

This document defines the specification for the Model Context Protocol (MCP) interface in `locron`. The MCP interface enables AI assistants and agents (such as Claude Desktop, Antigravity, Cursor, Windsurf, and custom AI tooling) to discover, inspect, schedule, trigger, and diagnose local jobs through standard JSON-RPC 2.0 over Standard I/O (`stdio`).

---

## 1. Goals and Observable Completion Criteria

### Goals
1. Enable AI assistants to manage local scheduling seamlessly via the open Model Context Protocol standard.
2. Maintain strict safety, atomicity, validation, and privacy parity with the core `locron` CLI.
3. Keep the runtime lightweight and self-contained within the single `locron` binary via `locron mcp`.

### Observable Completion Criteria
1. Running `locron mcp` enters the standard MCP stdio protocol loop, listening for JSON-RPC 2.0 requests on `stdin` and emitting JSON-RPC 2.0 responses on `stdout`.
2. All diagnostics, logs, and tracing are strictly routed to `stderr`, guaranteeing zero corruption of the JSON-RPC `stdout` stream.
3. AI clients can discover and execute a comprehensive suite of tools (`locron_list_jobs`, `locron_get_job`, `locron_add_job`, `locron_update_job`, `locron_enable_job`, `locron_disable_job`, `locron_remove_job`, `locron_run_job`, `locron_cancel_run`, `locron_get_logs`, `locron_why`, `locron_preview_schedule`, `locron_doctor`).
4. AI clients can inspect dynamic resources via `locron://jobs`, `locron://jobs/{id_or_name}`, `locron://history/{run_id}`, `locron://logs/{run_id}`, and `locron://doctor`.
5. AI clients can access standard prompt templates (`schedule_task`, `diagnose_failure`).
6. All mutating tools support a `dry_run: bool` parameter allowing safe pre-execution simulation without altering durable state.
7. End-to-end integration tests prove client-server negotiation, schema validity, tool execution, and error handling across mock AI client sessions.

---

## 2. In Scope vs Out of Scope

### In Scope
- Single binary entrypoint: `locron mcp` subcommand.
- Standard I/O (`stdio`) JSON-RPC 2.0 transport.
- Tools for complete Job CRUD, manual execution, cancellation, output streaming/history, why diagnostics, and schedule previews.
- Read-only resources for system state and job inspection.
- Guided prompt templates for scheduling and troubleshooting.
- Reusing existing `locron-core` command validation and `locron-store` SQLite transactions.
- Environment variable and secret redaction identical to `locron --json`.

### Out of Scope
- HTTP/SSE or WebSocket remote network transports (reserved for future remote web viewer phase).
- Multi-user authentication or authorization tokens (the MCP server runs locally within the user's OS permission boundary).
- Direct arbitrary code execution bypassing the scheduler.

---

## 3. Protocol and Transport Contracts

- **Transport**: Standard Input (`stdin`) and Standard Output (`stdout`) carrying UTF-8 JSON-RPC 2.0 messages delimited by newlines.
- **Logging**: All `tracing` subscribers in `locron mcp` MUST write to `std::io::stderr`.
- **Protocol Version**: Compatible with Model Context Protocol specification `2024-11-05` (and newer revisions).
- **Server Info**:
  - `name`: `"locron"`
  - `version`: current workspace version (e.g. `"0.1.0"`)

---

## 4. MCP Tools Specification

### 1. `locron_list_jobs`
- **Description**: List scheduled jobs with optional status and tag filtering.
- **Parameters**:
  - `enabled_only` (optional, `boolean`): Filter only enabled jobs.
  - `tag` (optional, `string`): Filter jobs containing the specified tag.
- **Output**: JSON array of job summaries including `id`, `name`, `schedule`, `target_summary`, `enabled`, `last_run_outcome`, and `next_occurrence`.

### 2. `locron_get_job`
- **Description**: Get full definition, metadata, schedule, policies, and recent runs for a job.
- **Parameters**:
  - `job` (required, `string`): Job UUID or unique job name.
- **Output**: Detailed job object including schedule details, overlap/missed-run/retry policies, target configuration, and last 5 execution summaries.

### 3. `locron_add_job`
- **Description**: Register a new scheduled job (cron, interval, or one-time).
- **Parameters**:
  - `name` (required, `string`): Unique job identifier name.
  - `schedule_type` (required, `string` enum: `"cron"`, `"interval"`, `"at"`): Schedule mode.
  - `schedule_expr` (required, `string`): Cron expression (e.g. `"0 3 * * *"`), interval duration (e.g. `"15m"`, `"30s"`), or ISO 8601 offset timestamp (e.g. `"2026-09-01T09:00:00+09:00"`).
  - `timezone` (optional, `string`): IANA time zone (e.g. `"Asia/Seoul"`, default: `"Local"`).
  - `target_type` (required, `string` enum: `"process"`, `"shell"`, `"http"`): Target execution type.
  - `command` (optional, `array[string]`): Process argv list (required if `target_type == "process"`).
  - `shell_script` (optional, `string`): Shell script string (required if `target_type == "shell"`).
  - `http_url` (optional, `string`): URL for HTTP target (required if `target_type == "http"`).
  - `http_method` (optional, `string`): HTTP method (default: `"GET"`).
  - `overlap_policy` (optional, `string` enum: `"skip"`, `"replace"`, `"allow"`): Default `"skip"`.
  - `missed_run_policy` (optional, `string` enum: `"skip"`, `"latest"`, `"all"`): Default `"skip"`.
  - `max_retries` (optional, `integer`): Maximum failure retries (0..10).
  - `timeout_seconds` (optional, `integer`): Execution timeout in seconds.
  - `description` (optional, `string`): Human-readable job description.
  - `tags` (optional, `array[string]`): List of tags.
  - `dry_run` (optional, `boolean`): If true, simulates validation without saving.
- **Output**: Created job object or dry-run validation report.

### 4. `locron_update_job`
- **Description**: Update an existing job's schedule, policies, target, or metadata.
- **Parameters**:
  - `job` (required, `string`): Job UUID or name to update.
  - (Same optional fields as `add_job`)
  - `dry_run` (optional, `boolean`): Validate without mutating state.
- **Output**: Updated job details or diff.

### 5. `locron_enable_job` / `locron_disable_job` / `locron_remove_job`
- **Description**: Lifecycle control for jobs.
- **Parameters**:
  - `job` (required, `string`): Job UUID or name.
- **Output**: Status confirmation.

### 6. `locron_run_job`
- **Description**: Trigger an immediate manual run of a job.
- **Parameters**:
  - `job` (required, `string`): Job UUID or name.
  - `wait` (optional, `boolean`): Whether to wait for execution completion (default: `false`).
  - `timeout_seconds` (optional, `integer`): Max wait duration.
  - `dry_run` (optional, `boolean`): Simulate run admission without executing.
- **Output**: Enqueued run ID, initial status, and execution outcome if `wait: true`.

### 7. `locron_cancel_run`
- **Description**: Request cancellation of an active or queued run.
- **Parameters**:
  - `run_id` (required, `string`): Run UUID.
- **Output**: Cancellation acceptance status.

### 8. `locron_get_logs`
- **Description**: Retrieve captured stdout/stderr/HTTP output for a run.
- **Parameters**:
  - `job` (optional, `string`): Job UUID or name (retrieves latest run if `run_id` omitted).
  - `run_id` (optional, `string`): Specific run UUID.
  - `tail_lines` (optional, `integer`): Number of latest lines to retrieve (default: 100).
- **Output**: Text logs with stdout/stderr interleaved ordering preserved.

### 9. `locron_why`
- **Description**: Explain the causal diagnosis of a job's current state or skipped occurrences.
- **Parameters**:
  - `job` (required, `string`): Job UUID or name.
- **Output**: Structured diagnosis explaining scheduler state, skip reasons, quarantine status, or concurrency barriers.

### 10. `locron_preview_schedule`
- **Description**: Preview future occurrence timestamps for a schedule expression without creating a job.
- **Parameters**:
  - `schedule_type` (required, `string` enum: `"cron"`, `"interval"`).
  - `schedule_expr` (required, `string`).
  - `timezone` (optional, `string`).
  - `count` (optional, `integer`, default: 5, max: 20).
- **Output**: Array of next calculated RFC 3339 timestamps.

### 11. `locron_doctor`
- **Description**: Perform health and environment diagnosis of the local scheduler daemon and database.
- **Output**: System health, active daemon status, state directory path, SQLite connectivity, and migration status.

---

## 5. MCP Resources Specification

- `locron://jobs`: JSON array of all registered jobs and current schedules.
- `locron://jobs/{job_id_or_name}`: Detailed JSON descriptor of a specific job.
- `locron://history/{run_id}`: Detailed JSON outcome, attempt breakdown, timestamps, and exit code.
- `locron://logs/{run_id}`: Raw captured output stream.
- `locron://doctor`: System diagnostic report.

---

## 6. MCP Prompts Specification

- **`schedule_task`**: Interactive prompt assisting users in creating reliable schedules, choosing between cron vs interval, configuring overlap policies (`skip` vs `replace`), and setting failure retries.
- **`diagnose_failure`**: Automated troubleshooting prompt taking a failed job or run ID, fetching the captured stderr/HTTP output and why-diagnostics, and generating remediation advice.

---

## 7. Security and Redaction Guarantees

1. **Transactional Parity**: Every mutation goes through `locron-core` command validation and `locron-store` immediate transactions.
2. **Redaction**: Inline secrets and sensitive environment variables match the existing redaction boundary.
3. **No Stale Process Signalling**: Process cancellation strictly uses durable process groups supervised by the daemon.
