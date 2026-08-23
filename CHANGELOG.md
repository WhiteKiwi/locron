# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html) as described in
[`docs/RELEASE.md`](docs/RELEASE.md).

Entries cover user-visible change: behavior, CLI surface, output contracts, packaging, and
documentation. CI and test-only changes are omitted — the commit history is the record for those.

## [Unreleased]

### Fixed

- Daemon admission latency is now bounded, and attempts that conflict permanently are no longer
  retried indefinitely.
- Output recovery no longer treats a live attempt's captured output as orphaned.

## [0.2.0] - 2026-08-23

### Added

- `-V`/`--version` honors `--format json`, bringing it in line with the machine-output contract used
  by the rest of the CLI.

### Documentation

- Install instructions cover the `brew trust` step that newer Homebrew requires for third-party taps.

## [0.1.1] - 2026-08-22

### Added

- `locron mcp` starts a Model Context Protocol server over stdio, exposing 13 tools, 5 resources, and
  2 prompts so an assistant can schedule, inspect, and diagnose jobs directly. Every mutating tool
  accepts `"dry_run": true`, and domain validation failures are returned as tool errors with
  `isError: true` rather than protocol errors.

### Documentation

- Per-option help text completed across every CLI command.

## [0.1.0] - 2026-08-22

Initial release.

### Added

- **Scheduling** — standard 5-field cron expressions, fixed intervals at second resolution, and
  one-time executions, with IANA timezone support, DST duplicate safety, durable cursors, and
  one-time uniqueness.
- **Execution targets** — direct argv execution, explicit shell execution, and native HTTP requests
  with header management and retry handling.
- **Policies** — explicit per-job overlap policies (`skip`, `replace`, `allow`) and missed-run
  policies (`skip`, `latest`, `all`), with bounded newest-window catch-up and compact omission
  summaries.
- **Process supervision** — process group supervision with configurable timeouts, SIGTERM/SIGKILL
  grace periods, and bounded output capture.
- **Durable state** — a private per-user state directory backed by bundled SQLite with WAL
  transactions, atomic migrations, and restartable crash recovery, including durable retry,
  concurrency, replacement, cancellation, and quarantine acknowledgement.
- **Job management** — normalized job CRUD, schedule preview, enable/disable/remove, manual run,
  cancellation, execution history, log streaming and follow, typed configuration, and atomic
  export/import.
- **Observability** — `locron why` explains why a job ran, was skipped, or failed, and
  `locron doctor` diagnoses daemon and state directory health.
- **Output contracts** — versioned human, `locron.cli/v1`, and `locron.stream/v1` output, with
  retry-aware wait/follow and stable target outcome exit codes.
- **Distribution** — Homebrew tap, `.deb` and `.rpm` packages, and pre-built tarballs for macOS and
  Linux on both x86_64 and aarch64.

[Unreleased]: https://github.com/WhiteKiwi/locron/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/WhiteKiwi/locron/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/WhiteKiwi/locron/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/WhiteKiwi/locron/releases/tag/v0.1.0
