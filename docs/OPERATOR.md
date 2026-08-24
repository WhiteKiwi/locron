# locron Operator Guide

This guide turns the milestone-1 contracts in [SPEC.md](SPEC.md) and [CLI.md](CLI.md) into a
day-to-day operating reference. The repository is still an active milestone implementation; use
[STATUS.md](STATUS.md) and [ACCEPTANCE.md](ACCEPTANCE.md) to distinguish implemented behavior from
pending acceptance evidence.

## Start and inspect the scheduler

locron is one per-user scheduler for one local state directory. Run one daemon for that directory:

```sh
locron daemon run
```

Use a service manager only to keep this command alive. Do not create parallel cron, launchd, or
systemd entries for individual locron jobs. Confirm ownership and durable state with:

```sh
locron doctor
locron config get
locron list --all
```

## Run the daemon as a service

`locron service install` registers and starts the daemon with the platform's per-user service
manager, without administrative privileges:

```sh
locron service install
locron service status
```

- **macOS** installs a per-user LaunchAgent (`dev.locron.daemon`) that keeps the daemon alive
  (`KeepAlive`), loads it at login (`RunAtLoad`), and writes the daemon log to
  `~/Library/Logs/locron/daemon.log`.
- **Linux** installs a systemd user unit (`locron.service`) that restarts the daemon on failure.
  The daemon runs inside your login session: it stops at logout and starts again at the next
  login. Work missed while it was stopped is reconciled under each job's missed-run policy when
  it next starts. To keep the daemon running after logout and across boots, enable lingering for
  your user (self-lingering needs no administrator authentication):

  ```sh
  loginctl enable-linger "$USER"
  ```

- Repeating `locron service install` is safe: it refreshes the registration onto the current
  binary (important after a binary move) and restarts a loaded service under ordinary
  graceful-shutdown rules, so active work completes or retries per the normal policies.
- If a manually started daemon holds the state lock, the service is registered and enabled but
  its start is deferred until that daemon exits; `service install` reports the deferral. The
  registered service never creates a second scheduler for one state directory.
- `locron service uninstall` stops the daemon, unloads the service, and removes the
  registration file; the binary stays in place.
- The install script (`install.sh`) registers the service automatically after installing or
  replacing the binary; set `LOCRON_NO_SERVICE=1` to decline. Registration is best-effort: a
  failed attempt warns and leaves the installation successful, and `locron service install`
  retries it.

Package-managed installs never register a service automatically. **Homebrew** ships the formula's
`service` block so the package manager supervises the daemon: start it with `brew services start
locron`, and use `brew services restart locron` after an upgrade (an upgrade never restarts a
running service on its own). **Debian/RPM** installs print the registration guidance during
installation; on Linux with a systemd user session, `locron service install` still works for a
manually supervised setup.

## Updating locron

- **Install script / tarball installs** update with `locron self-update`: it verifies the downloaded archive against the release's `SHA256SUMS.txt` and replaces the binary atomically (one temp file plus rename in the executable's directory), so a failed or interrupted update leaves the old binary working. After the replace it refreshes a service registration and restarts the daemon, so a registered service runs the new version immediately; a manual `locron daemon run` keeps the old code until restarted.
- **Homebrew** installs update with `brew upgrade locron`; the formula marks the install (`lib/.disable-self-update`) so `locron self-update` refuses and points to Homebrew. A running `brew services` service keeps the old code until `brew services restart locron`.
- **Debian / RPM** installs update by replacing the package; a service-managed daemon registered with `locron service install` keeps the old code until the registration is refreshed with `locron service install`.
- A running `locron daemon run` process keeps the old code until it is restarted: self-update replaces the file on disk and never signals running processes. After updating, restart the daemon (and any long-lived `locron` MCP/`run --wait` clients) to run the new version. Durable state is versioned, so downgrades and upgrades across revisions are handled by the store migration path; schedule and history data survive the restart.

## Schedules

Every job has exactly one schedule. Preview before registration or enablement when timing matters:

```sh
locron preview --cron "0 3 * * *" --timezone Asia/Seoul --count 5
locron add heartbeat --every 30s -- /usr/bin/printf 'ok\n'
locron add nightly --cron "0 3 * * *" --timezone Asia/Seoul --shell './backup.sh'
locron add deploy-once --at 2026-09-01T09:00:00+09:00 -- /usr/local/bin/deploy
locron add health-check --every 5m --http GET https://example.test/health
```

- Calendar schedules use five fields. DST gaps are skipped and a repeated wall time produces one
  occurrence.
- Fixed intervals advance from their durable anchor, not from the previous completion time.
- One-time schedules disable after their scheduled occurrence is resolved. Manual runs do not
  consume the scheduled occurrence.
- Schedule updates affect only future occurrences. Disable/re-enable and manual runs do not move an
  interval anchor.

## Admission policies

Set policies at `add` time or change them with `update`.

| Concern | Choices | Operator effect |
|---|---|---|
| Overlap | `skip` (default), `replace`, `allow` | Skip records a terminal explanation; replace terminates confirmed prior work before starting the newest candidate; allow uses bounded concurrency. |
| Missed run | `skip`, `latest`, `all` | Latest creates one catch-up run; all creates an oldest-first bounded batch; `--start-deadline` excludes stale work. |
| Retry | disabled by default; up to 10 retries | A retry is another attempt of the same run. Known configured failures are eligible; cancellation and unknown crash outcomes are not. |
| Concurrency | global 16 by default; per-job policy limit | Global pressure keeps eligible work queued. Same-job pressure is bounded; catch-up `all` is the explicit bounded queue. |

Examples:

```sh
locron update nightly --overlap replace --missed-run latest
locron update health-check --retries 3 --backoff exponential --retry-delay 10s --retry-cap 5m
locron update heartbeat --overlap allow --per-job-concurrency 4
locron config set global_concurrency 32
```

Reducing global concurrency does not terminate active attempts. The new value applies to the next
admission decision; no new attempt starts while the active count is at or above it.

## Timeout, cancellation, and crash recovery

Attempts time out after 60 seconds by default. Configure `--timeout DURATION` or explicitly remove
the timeout with `--no-timeout`. Process and shell targets run in a process group. Timeout,
cancellation, and replacement send graceful termination, wait the configured grace period, then
force termination. A deliberately detached descendant is outside the portable guarantee.

```sh
locron update nightly --timeout 20m --termination-grace 10s
locron history nightly --limit 20
locron cancel RUN_ID
locron why --run RUN_ID
```

After an unclean daemon exit, a stale active attempt becomes `interrupted_unknown`; locron neither
infers success nor retries or signals its recorded stale process identity. If termination could not
be confirmed, the run remains an active-blocking quarantine. Inspect it first, verify the external
process or side effect independently, then acknowledge only when accepting that uncertainty:

```sh
locron why --run RUN_ID
locron cancel RUN_ID --acknowledge-unconfirmed
locron why JOB_NAME
```

Acknowledgement is not ordinary cancellation. It is valid only for the exact quarantined run,
records an audit fact, ends the run as `interrupted_unknown`, and releases same-job admission.

## Retention

The milestone contract retains terminal run metadata for 90 days, bounded by 1,000 runs per job and
10,000 globally. Output defaults to 30 days, 10 MiB per run, and 256 MiB globally. Active runs are
never pruned; output pruning keeps metadata that explains truncation or removal.

Inspect settings and preview explicit cleanup before applying it:

```sh
locron config get
locron prune --dry-run
locron prune
locron doctor
```

The current build implements bounded explicit output pruning. Automatic startup/periodic cleanup,
startup artifact repair, and complete metadata age/count retention remain pending acceptance work;
see [STATUS.md](STATUS.md). Do not treat configured bounds as proof that every cleanup path has run.

## Plaintext and exactly-once boundaries

Inline environment values, process arguments, shell strings, inline HTTP headers, and inline bodies
are plaintext user-local configuration. Normal inspection and diagnostics redact configured values,
but locron cannot stop a target from printing a secret into captured output. Use runtime files with
owner-only permissions or an external secret provider when values must not be stored in the locron
database.

Plaintext export requires explicit acknowledgement on both export and import:

```sh
locron export --include-values --acknowledge-plaintext > locron-export.json
locron import locron-export.json --accept-plaintext-values --dry-run
locron import locron-export.json --accept-plaintext-values
```

Importing a document registers work that may run on this machine, whether it arrives as a file or a
URL, and carries the same trust boundary as installing a script obtained from the same source: the
URL's owner can schedule arbitrary processes here. Import fetches over HTTPS with mandatory TLS
verification and validates the complete document before any write, but no fetch can prove the source
is trustworthy. Prefer a trusted origin, and review the document with `--dry-run` before a first
import:

```sh
locron import https://example.com/locron-export.json --dry-run
locron import https://example.com/locron-export.json
```

locron prevents duplicate creation of one durable scheduled occurrence. It cannot promise
exactly-once effects for arbitrary processes or HTTP endpoints across a crash. Targets requiring
that property must use `LOCRON_RUN_ID` as an idempotency key and make the external operation
idempotent.

## Diagnostics and recovery checklist

1. Run `locron doctor` to check paths, database integrity, daemon ownership, and wake-socket state.
2. Run `locron why JOB_NAME` for eligibility, next occurrence, policy, and capacity blockers.
3. Run `locron why --run RUN_ID` for attempts, retries, replacement, cancellation, truncation,
   pruning, and the terminal reason.
4. Read `locron history JOB_NAME` and `locron logs RUN_ID --attempt N`; use `--channel` to isolate
   `stdout`, `stderr`, or HTTP `body`.
5. If the daemon is absent, restart `locron daemon run`. Durable queued work remains queued; a wake
   notification is only a hint, never the source of truth.
6. For `interrupted_unknown`, verify the external system before manually rerunning. A new manual run
   has a new run ID and may repeat an already-applied side effect.

`-v` and `--debug` add redacted stderr diagnostics. They do not change JSON stdout and are not a
replacement for `why`.

## Web dashboard

`locron dashboard` starts the loopback-only web administration surface: a browser viewer plus an
HTTP API over the same durable application commands as the CLI (see
[`docs/dashboard/SPEC.md`](docs/dashboard/SPEC.md) for the full contract). It runs as a process
separate from the scheduler daemon, is off by default, and serves only loopback addresses.

- **Foreground:** `locron dashboard` (identical to `locron dashboard serve`) binds, prints the
  exact access URL, and serves until interrupted. An occupied default port falls back to the next
  free port in foreground mode; an explicit `--port` is always strict, and `--bind` accepts only
  the loopback literals `127.0.0.1` and `::1`.
- **Service:** `locron dashboard enable` registers the dashboard as a per-user service — a
  `dev.locron.dashboard` LaunchAgent on macOS, a `locron-dashboard.service` systemd user unit on
  Linux — that starts immediately and again at login. In service mode the port is fixed, so a
  bookmarked address never moves. `locron dashboard status` reports the registered service,
  whether it is loaded, and token facts; `locron dashboard disable` unregisters the service and
  removes the token.
- On Linux the dashboard service stops at logout exactly like the daemon; the same optional
  `loginctl enable-linger "$USER"` step keeps both running after logout and across boots.
- `locron dashboard token` re-displays the stored token. `locron doctor` reports the dashboard
  exposure facts (token posture and registration) without the token value.

### Access token

- On first use the dashboard generates a 64-character random token, writes it owner-only (`0600`)
  into the state directory as `dashboard.token`, and prints it in the foreground startup output.
  Later starts reuse it.
- The token is accepted by the entry-page paste box — which then sets a same-site session cookie,
  so later visits need no token — or through an `Authorization: token` header for scripts and
  automation. It never appears in a URL, in logs, or in diagnostics.
- `locron dashboard token` re-displays the stored token; `locron dashboard enable --reset`
  regenerates it and restarts the service; `locron dashboard disable` removes it. Removing the
  token file yourself causes regeneration on the next start.

### What loopback does and does not protect

Binding to `127.0.0.1`/`::1` answers only on your machine's loopback interface: no other machine
can reach the dashboard, and any non-loopback bind address is refused outright rather than merely
warned about. Loopback is not a boundary between processes, though — any process running on your
machine can connect to the port. The access token, the Host/Origin validation, and the anti-CSRF
checks are what stop another local process or page from reading your scheduler or mutating it
through the API. Do not proxy or tunnel the port to other machines: the surface is explicitly not
remote access.

## Model Context Protocol (MCP) server

`locron mcp` serves the Model Context Protocol over stdio for AI assistants. It reuses the same
application boundary as every other command: `locron-core` validation, `locron-store` transactions,
and the same redaction rules as CLI output.

Run it directly to smoke-test a session (the process exits on stdin EOF):

```sh
printf '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}\n' | locron mcp
```

Transport contract:

- Newline-delimited JSON-RPC 2.0 frames on **stdout only**. Diagnostics and tracing go to stderr;
  nothing else ever writes to stdout while the server is running.
- Standard error codes: `-32700` parse error, `-32600` invalid request, `-32601` method not found,
  `-32602` invalid params.
- Domain failures (duplicate job name, invalid cron, missing job or run) return tool results with
  `isError: true` and an actionable message, so the assistant can correct itself.
- Clean exit on stdin EOF, SIGINT, or SIGTERM.

Surface:

- **Tools** — `locron_list_jobs`, `locron_get_job`, `locron_add_job`, `locron_update_job`,
  `locron_enable_job`, `locron_disable_job`, `locron_remove_job`, `locron_run_job`,
  `locron_cancel_run`, `locron_get_logs`, `locron_why`, `locron_preview_schedule`,
  `locron_doctor`. Every mutating tool accepts `dry_run: true` and reports what would change
  without persisting anything.
- **Resources** — `locron://jobs`, `locron://jobs/{id_or_name}`, `locron://history/{run_id}`,
  `locron://logs/{run_id}`, `locron://doctor`.
- **Prompts** — `schedule_task` (compose a validated job from a plain-language request) and
  `diagnose_failure` (explain why a job or run is in its current state).

### Claude Desktop

Edit `claude_desktop_config.json` (Claude → Settings → Developer → Edit Config):

```json
{
  "mcpServers": {
    "locron": {
      "command": "locron",
      "args": ["mcp"]
    }
  }
}
```

### Cursor

Add a server in Settings → MCP, or create `.cursor/mcp.json` in the project:

```json
{
  "mcpServers": {
    "locron": {
      "command": "locron",
      "args": ["mcp"]
    }
  }
}
```

If the binary is not on the MCP client's PATH, replace `"command"` with the absolute path to the
`locron` binary (for example `/usr/local/bin/locron` or `$HOME/.cargo/bin/locron`).

## Output examples

Current human output is readable JSON plus warnings on stderr:

```text
$ locron run nightly
{
  "run_id": "0198f000-0000-7000-8000-000000000001",
  "state": "queued"
}
warning: daemon is not running; run remains durably queued
```

Machine mode emits one `locron.cli/v1` document on stdout:

```text
$ locron --json run nightly
{"schema":"locron.cli/v1","ok":true,"command":"run","data":{"run_id":"0198f000-0000-7000-8000-000000000001","state":"queued"},"warnings":["daemon is not running; run remains durably queued"]}
```

The reviewed streaming contract uses newline-delimited `locron.stream/v1` records and ends with a
terminal result. Live partial-file follow and that envelope are not implemented yet, so the
following is a contract-shape example, not output the current build can produce:

```text
{"schema":"locron.stream/v1","record":"output record (shape pending implementation)"}
{"schema":"locron.stream/v1","record":"terminal result (shape pending implementation)"}
```

Until streaming acceptance is complete, use terminal `locron logs RUN_ID` output and a separate
`locron why --run RUN_ID` query. Do not build automation around the current `--follow --json`
behavior.
