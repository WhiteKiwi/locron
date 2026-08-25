# locron MCP Implementation Plan

## Architecture and Approach

This document describes the implementation architecture for `locron mcp` based on the frozen requirements in `SPEC.md` (this directory).

---

## 1. Component Architecture

The MCP server is implemented directly inside `locron` (`crates/locron-cli/src/mcp.rs`) to maintain the single-binary distribution contract and directly reuse existing `locron-core` domain commands and `locron-store` SQLite transactions.

```text
┌─────────────────────────────────────────────────────────────┐
│                       AI Assistant                          │
│         (Claude Desktop / Antigravity / Cursor)             │
└──────────────────────────────┬──────────────────────────────┘
                               │ JSON-RPC 2.0 (stdio)
                               ▼
┌─────────────────────────────────────────────────────────────┐
│                    locron mcp (CLI Root)                    │
│  ┌───────────────────────────────────────────────────────┐  │
│  │               MCP JSON-RPC Router                     │  │
│  │   - Handshake (initialize, ping)                      │  │
│  │   - Tools Dispatch (locron_list_jobs, add_job, etc.)  │  │
│  │   - Resources Dispatch (locron://jobs, doctor, etc.)  │  │
│  │   - Prompts Dispatch (schedule_task, diagnose)        │  │
│  └───────────────────────────┬───────────────────────────┘  │
│                              │                              │
│                 std::io::stderr Only Logs                   │
│                              │                              │
│                              ▼                              │
│                 locron-core / locron-store                  │
│       (Application Commands, Validation, SQLite WAL)        │
└─────────────────────────────────────────────────────────────┘
```

---

## 2. JSON-RPC 2.0 and MCP Handshake Engine

### Handshake (`initialize`)
- Client sends `initialize` with protocol version and capabilities.
- Server returns:
  ```json
  {
    "protocolVersion": "2024-11-05",
    "capabilities": {
      "tools": { "listChanged": false },
      "resources": { "subscribe": false, "listChanged": false },
      "prompts": { "listChanged": false }
    },
    "serverInfo": {
      "name": "locron",
      "version": "0.1.0"
    }
  }
  ```
- Client sends `notifications/initialized`.

### Stdio Transport Loop
- Reads lines from `tokio::io::BufReader(tokio::io::stdin())`.
- Serializes responses to `tokio::io::stdout()`.
- Flushes stdout after each JSON-RPC frame.
- Guarantees clean exit on EOF (`stdin` close) or SIGINT/SIGTERM.

---

## 3. Tool and Resource Execution Engine

Each MCP tool parses parameters and constructs the corresponding `locron-core` Command:
- `locron_add_job` -> `CreateJob` command -> `store.create_job(cmd)`
- `locron_update_job` -> `UpdateJob` command -> `store.update_job(cmd)`
- `locron_run_job` -> `store.enqueue_manual(job_id, run_id, now_us())` (dry run uses `open_read_only`)
- `locron_cancel_run` -> `store.cancel_with_acknowledgement(run_id, now_us(), acknowledge_unconfirmed)`
- `locron_why` -> `store.run(run_id)` + `store.events_for_run(run_id)` (run facts) or `store.job(name)` +
  `store.history(name, count)` + `store.cancellation_requested(run_id)` (job facts)
- `locron_preview_schedule` -> `Schedule::next(after, count)`

### Error Handling
- Protocol errors return standard JSON-RPC error codes (`-32600` Invalid Request, `-32601` Method Not Found, `-32602` Invalid Params).
- Domain validation failures (e.g. invalid cron expression, duplicate job name) return tool call errors (`isError: true`) with clear, actionable error descriptions for the LLM.

---

## 4. Verification Plan

1. **Unit Tests**:
   - JSON-RPC request and response serialization/deserialization.
   - Tool schema generation and parameter validation.
2. **Integration Tests**:
   - `crates/locron-cli/tests/mcp.rs`: Spawns `locron mcp` sub-process with piped stdin/stdout.
   - Tests `initialize` handshake, `tools/list`, `tools/call` for job creation, preview, listing, manual run, cancellation, and why diagnosis.
   - Verifies `stdout` contains only valid JSON-RPC frames while debug logs appear exclusively on `stderr`.
