<p align="center">
  <img src="assets/banner.jpg" alt="locron banner" width="800">
</p>

<h1 align="center">locron</h1>

<p align="center">
  <strong>Cron that explains itself.</strong>
</p>

<p align="center">
  <a href="https://github.com/WhiteKiwi/locron/actions/workflows/ci.yml"><img src="https://github.com/WhiteKiwi/locron/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/WhiteKiwi/locron/releases"><img src="https://img.shields.io/github/v/release/WhiteKiwi/locron?color=blue" alt="Latest release"></a>
  <a href="#license"><img src="https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-blue.svg" alt="License"></a>
</p>

---

Cron runs jobs. When one does not run, figuring out why is usually your problem.

`locron` is a local-first scheduler for **macOS** and **Linux**. It keeps durable run history and
captured output, makes missed-run and overlap behavior explicit, and explains the facts behind a
job's current eligibility or a run's terminal outcome.

## A 10-second tour

Add a job, preview its schedule, and ask what the scheduler currently knows. IDs and timestamps
will differ; the command output below uses the current CLI's human format.

```console
$ locron add backup --every 1h -- /bin/echo backup-complete
job added: backup (01a0330e-8ca5-77b1-baa5-eaeebe71b2b2)
schedule: every 1h
target: run /bin/echo backup-complete

$ locron preview backup --count 2
schedule: every 1h
2026-08-24T10:16:26.66083Z
2026-08-24T11:16:26.66083Z

$ locron why backup
```

Selected `why` output (the complete report includes additional job, schedule, and policy fields):

```text
ELIGIBILITY
  active runs: 0
  decision: eligible
  global concurrency: 16
POLICIES
  overlap: skip
  missed run: skip
DAEMON
  daemon running: yes
```

After runs exist, `locron history backup` lists their recorded outcomes. `why NAME` explains the
scheduler's current durable view; downtime explanations are based on schedule cursors and
reconciliation facts, not inferred sleep telemetry. For a durable run's terminal outcome, use
`history` and `why --run RUN_ID` to inspect that specific run's recorded outcome and reason.

## Explainability first

- `locron preview backup` shows upcoming occurrences before you rely on a schedule.
- `locron history backup` shows past runs, triggers, states, and durations.
- `locron why backup` explains current job eligibility, policies, schedule cursor, and daemon
  availability.
- `locron why --run <RUN_ID>` explains one durable run, including attempts, recorded events, and
  its terminal reason.
- `locron logs <RUN_ID>` reads that run's captured output; add `--follow` while it is active.
- `locron doctor` checks the daemon, state directory, database, execution path, and job target
  resolution.

Manual runs print their canonical run ID. History's machine-readable records expose it as well:

```sh
locron run backup
locron history backup --format json
locron why --run 018f47a2-4a12-7c35-b9d8-0123456789ab
locron logs 018f47a2-4a12-7c35-b9d8-0123456789ab
```

## Reliability for machines that stop and restart

Local machines sleep, reboot, lose network access, and sometimes start a new occurrence before the
previous one has finished. locron turns those cases into named policies instead of hidden behavior.

- **Missed runs:** choose `skip`, `latest`, or bounded `all` catch-up behavior.
- **Overlaps:** choose `skip`, `replace`, or `allow`, subject to concurrency limits.
- **Durable occurrences:** schedule revisions, nominal times, run identities, attempts, and events
  survive daemon restarts in the local state store.
- **Recovery:** startup reconciliation classifies interrupted work without blindly repeating an
  external side effect whose outcome is unknown.
- **Supervision:** process groups, timeouts, cancellation, termination grace periods, retries, and
  bounded output capture are part of the scheduler's execution model.
- **Targets:** run direct processes, explicit shell commands, or HTTP requests.

Declare the behavior with the job:

```sh
locron add sync --every 15m \
  --missed-run latest \
  --overlap skip \
  --timeout 10m \
  -- /usr/local/bin/sync-data
```

The durable state is stored in bundled SQLite with WAL transactions and atomic migrations. Those
are implementation details, but they are what let the observable scheduling facts survive a
process crash or restart.

## Agent-friendly by design

CLI commands support versioned `locron.cli/v1` machine output. Job creation and updates, manual-run
admission, imports, and pruning offer dry-run paths so scripts and coding agents can validate intent
before changing durable state.

```sh
locron why backup --format json
locron history backup --format json
locron add test-job --cron "0 12 * * *" --dry-run -- /usr/bin/true
```

`locron mcp` serves the [Model Context Protocol](https://modelcontextprotocol.io) over stdio, so
Claude Desktop, Cursor, and other MCP clients can inspect and manage the scheduler through the same
validation, redaction, and durable application boundary as the CLI.

It exposes **13 tools**, **5 resources**, and **2 prompts**. Every mutating tool accepts
`"dry_run": true`, and domain failures are returned as tool errors the assistant can inspect.

Register it with any MCP client using the same entry — `claude_desktop_config.json` for Claude
Desktop, or `.cursor/mcp.json` / Cursor Settings → MCP for Cursor:

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

If `locron` is not on the client's `PATH`, use an absolute path. See the
[MCP Specification](docs/mcp/SPEC.md) for the full tool, resource, and prompt reference.

## Installation

**Homebrew (macOS and Linux):**

```sh
brew install whitekiwi/tap/locron
```

**Install script (macOS and Linux):**

```sh
curl -fsSL https://locron.whitekiwi.link/install.sh | sh
```

The short URL redirects to the canonical
`https://github.com/WhiteKiwi/locron/releases/latest/download/install.sh` release asset.

**Or ask Claude Code:**

```sh
claude "Install locron: https://github.com/WhiteKiwi/locron"
```

Review and approve each command it proposes. Every channel — packages, tarballs, building from
source, updating, and uninstalling — is covered in the [Installation Guide](docs/INSTALL.md).

## Start scheduling

First, check the daemon:

```sh
locron service status
```

The install script registers the daemon as a login service and starts it for you. Homebrew installs
never auto-start; run `brew services start locron` once instead. For manual control,
`locron daemon run` runs it in the foreground. See
[service setup](docs/OPERATOR.md#run-the-daemon-as-a-service) for the complete lifecycle.

Then add any supported target:

```sh
# Direct process every 15 minutes
locron add fetch-repo --every 15m -- git -C ~/projects/app fetch

# Explicit shell command nightly at 03:00 Seoul time
locron add nightly-backup --cron "0 3 * * *" --timezone Asia/Seoul --shell "./scripts/backup.sh"

# HTTP request every 5 minutes
locron add health-check --every 5m --http GET https://example.com/health

# One-time execution at a specific instant
locron add deploy-task --at 2026-09-01T09:00:00+09:00 -- /usr/local/bin/deploy
```

State lives in `~/.local/share/locron` (or `$XDG_DATA_HOME/locron`) and can be overridden with
`--state-dir` or `LOCRON_STATE_DIR`.

## Documentation

- **[Installation Guide](docs/INSTALL.md)** — install channels, updates, and uninstalling.
- **[Operator Guide](docs/OPERATOR.md)** — daily operations, policies, and troubleshooting.
- **[CLI Reference](docs/CLI.md)** — every command, option, and output contract.
- **[Architecture](docs/ARCHITECTURE.md)** — system design, invariants, and durable state.
- **[MCP Specification](docs/mcp/SPEC.md)** — tools, resources, and prompts.
- **[Release Policy](docs/RELEASE.md)** — versioning, packaging, and release automation.
- **[Changelog](CHANGELOG.md)** — notable changes in each release.

## Contributing

Contributions are welcome. `locron` is developed **documentation-first** — planning documents
change before code — so please read [CONTRIBUTING.md](CONTRIBUTING.md) before opening a pull
request.

- [Report a bug or propose a feature](https://github.com/WhiteKiwi/locron/issues/new/choose)
- [Report a security vulnerability privately](https://github.com/WhiteKiwi/locron/security/advisories/new)
  — never in a public issue ([SECURITY.md](SECURITY.md))

This project follows the [Contributor Covenant](CODE_OF_CONDUCT.md) code of conduct.

## License

Dual-licensed under either of:

- MIT License ([LICENSE-MIT](LICENSE-MIT))
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in
the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.
