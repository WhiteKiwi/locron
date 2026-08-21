# locron

A local-first scheduler for macOS and Linux. locron keeps schedules, durable run identity,
history, and bounded output in one private per-user state directory instead of translating
jobs into operating-system cron entries.

> This repository is an active milestone-1 implementation. The core CLI/daemon path works,
> but the complete frozen contract in `docs/SPEC.md` is not yet release-ready. See
> `docs/TODO.md` for verified progress and remaining edge cases.

## Build from source

Rust 1.94 or newer is required.

```sh
cargo build --release -p locron-cli
./target/release/locron --help
```

The v1 workspace deliberately produces one binary. Start its long-lived scheduler with:

```sh
locron daemon run
```

Packaging and service installation are later milestones. For development, run the daemon
from a terminal or configure your own process manager to invoke that command.

## Quick start

```sh
# Direct argv execution every 15 minutes.
locron add repository-fetch --every 15m -- git -C /path/to/repo fetch

# Explicit shell execution at 03:00 in a fixed IANA time zone.
locron add backup --cron "0 3 * * *" --timezone Asia/Seoul \
  --shell "./backup.sh"

# One-time request. The ISO timestamp must carry an offset.
locron add reminder --at 2026-08-22T09:00:00+09:00 \
  --shell "printf 'meeting starts now\\n'"

locron list
locron preview backup --count 5
locron run backup
locron history backup
locron why backup
locron doctor
```

`run` durably enqueues and returns its run ID. It still succeeds while the daemon is offline
and warns that the run remains queued. `run --wait` attaches to that durable run; disconnecting
the client does not request cancellation.

## Safe inspection

Mutating command families that support `--dry-run` validate and normalize without initializing
or changing state:

```sh
locron add sample --every 5m --dry-run -- /usr/bin/true
locron run sample --dry-run
locron config set global_concurrency 32 --dry-run
locron prune --dry-run
```

`why` explains durable job/run facts. Repeat `-v` for operator context or use `--debug` for
redacted developer traces on stderr. `--json` returns a versioned `locron.cli/v1` envelope on
stdout, keeping diagnostics separate.

## State and safety boundary

Use `--state-dir PATH` or `LOCRON_STATE_DIR` to override the platform default. locron stores a
bundled SQLite database, a permanent daemon lock, a best-effort wake socket, and framed output
artifacts there. State directories and files are owner-only and local filesystems are the
supported boundary.

Inline environment values, process arguments, shell commands, and HTTP headers are plaintext
local configuration. Normal output and diagnostics redact configured values, but a target can
still print secrets into captured output. locron is not a secret manager.

locron prevents duplicate durable occurrence creation, but cannot promise exactly-once external
side effects across a machine crash. Targets needing that guarantee should treat `LOCRON_RUN_ID`
as an idempotency key.

## Development verification

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
```

The documents under `docs/` are the source of truth. Start with `docs/SPEC.md`,
`docs/ARCHITECTURE.md`, `docs/CLI.md`, and `docs/STORAGE.md`.
