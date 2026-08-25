# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html) as described in
[`docs/RELEASE.md`](docs/RELEASE.md).

Entries cover user-visible change: behavior, CLI surface, output contracts, packaging, and
documentation. CI and test-only changes are omitted — the commit history is the record for those.

## [Unreleased]

## [0.9.2] - 2026-08-25

### Fixed

- Service and dashboard lifecycle commands, `dashboard token`, and successful self-updates now
  render labeled terminal reports in default human mode instead of pretty-printed JSON. Machine
  envelopes remain unchanged, and dashboard tokens stay copyable without appearing in status output.
- Stale dashboard links now show complete job- or run-not-found pages with a direct route back to
  the applicable collection instead of displaying only a raw missing identifier.

## [0.9.1] - 2026-08-25

### Changed

- Human `locron history` output now fits narrow terminals by truncating only the trigger column.
  Redirected and machine-readable output remain complete, and `--no-trunc` preserves the full
  human table when requested.

## [0.9.0] - 2026-08-25

### Added

- Rust users can install and update Locron from crates.io with
  `cargo install --locked locron`. The five workspace packages publish together at one exact
  version, and release automation verifies the registry installation before completing a release.

### Changed

- Built-in self-update now requires the ownership receipt written by the standalone installer.
  Cargo, Homebrew, source, manually copied, and system-package installations remain owned by their
  respective package managers and receive actionable update guidance instead.
- Completed checklist history now lives in a dedicated archive, while deferred product ideas are
  tracked separately from the active implementation checklist.

## [0.8.0] - 2026-08-25

### Added

- `locron dashboard` provides an optional local web interface over the same durable jobs, runs,
  settings, diagnostics, validation, dry-run, and redaction boundaries as the CLI. Foreground,
  enable, disable, status, and token commands cover temporary use and per-user service lifecycle.
- The authenticated `locron.api/v1` management API exposes job and run operations, schedule
  preview, settings, import/export, pruning, diagnostics, and live updates for local clients.
- The responsive dashboard includes light, dark, and system themes; accessible job and run tables;
  whole-row detail navigation; searchable history; structured forms for durations, output sizes,
  and schedules; aggregate settings review; and a redacted, pretty-formatted JSON viewer.

### Security

- Dashboard servers refuse non-loopback binds and protect state with an owner-only random token,
  authenticated browser sessions, Host and Origin validation, double-submit CSRF checks, and a
  no-referrer policy. Tokens are pasted into the entry page or sent in an authorization header and
  never accepted in URLs, logs, diagnostics, or API responses.

### Changed

- Job and run searches match partial names, descriptions, tags, and identifiers after a short
  typing debounce. Enabled and disabled jobs remain discoverable under the corresponding state
  filters, and empty results preserve table context with clear recovery actions.
- The primary documentation now gives a copyable path from installation to the loopback dashboard
  and explains foreground versus persistent service operation without changing the CLI-first
  scheduler model.

## [0.7.0] - 2026-08-24

### Added

- One-time jobs can opt into automatic cleanup with `locron add --at ... --delete-after-run`.
  Once the scheduled run reaches a final outcome, locron soft-removes the job definition while
  retaining its run history and captured output under the normal retention policy. History labels
  removed jobs explicitly and remains queryable by their retained identity.

## [0.6.0] - 2026-08-24

### Added

- `locron explain NAME_OR_ID` provides one redacted summary of a job's schedule, next occurrence,
  current scheduling posture, latest run, and latest anomalous terminal run. Human and versioned
  JSON output carry the same canonical run identities and durable reasons, while `why --run`
  remains the detailed attempt and event trace.

### Documentation

- Repositioned locron as “Cron that explains itself.” and reorganized the README around
  explainability, real-world reliability, and agent integration. Examples now distinguish job
  explanations from run explanations and use canonical run IDs for captured logs.
- Added AI-assisted installation guidance and a copy-paste Homebrew install command using the
  fully qualified tap name.

## [0.5.0] - 2026-08-24

### Added

- The human `list` table fits the terminal width: on a terminal, when the table would overflow, the
  `TARGET` column truncates to the remaining width with a trailing `…` and `--no-trunc` prints full
  values. Piped or redirected output always prints full values, and machine output is
  byte-identical. Truncation follows character display width, so CJK and emoji targets fit
  correctly.

## [0.4.1] - 2026-08-24

### Added

- Human-readable output for every command (`docs/SPEC.md` Human Output Contract, issue #4): a docker-style `history` table, confirmation lines for `add`/`update`/`enable`/`disable`/`remove`/`run`/`cancel`/`config`/`import`/`prune`, labeled report sections for `show`/`why`/`doctor`, and one-line occurrence lists for `preview`. Machine-readable output is byte-identical, and every human form honors the existing redaction rules. The demo screencast no longer pipes commands through `jq`.

### Fixed

- The shutdown-drain acceptance test no longer depends on launchd zombie-reap timing: a pure-builtin keep-alive loop makes the test process group single-member, so leader reap equals group absence and the two macOS CI legs stop flaking.

## [0.4.0] - 2026-08-24

### Added

- Export selection: `locron export --jobs`/`--tag` exports an exact subset, and a bare export in an
  interactive terminal shows a job multi-select picker (all jobs initially selected) while standard
  output still carries only the export document. Non-interactive contexts — pipes, redirection,
  `CI`, JSON mode — export everything as before.
- URL import: `locron import https://…` fetches and applies an export document with mandatory TLS
  verification, bounded redirect/size/timeout limits, and the same validation, dry-run, and
  atomicity as a file import. Importing a document registers executable schedules, with the same
  trust boundary as installing a script from that URL.
- `ls`/`rm` visible aliases for `list`/`remove`, and a docker-style aligned human `list` table
  (NAME, SCHEDULE, TARGET, ENABLED). Machine output is unchanged.

## [0.3.1] - 2026-08-23

### Fixed

- Linux: `locron self-update` no longer skips the post-update service registration. The update flow
  captured the running executable through `/proc/self/exe`, which resolves to the deleted inode
  after an atomic self-replace, so the daemon service silently stayed on the old binary; the flow
  now captures the executable path before the replace and re-registers the service onto the new
  version.

### Added

- Maintainer tooling: `scripts/usage.sh` prints a distribution-channel usage snapshot (GitHub
  release downloads, stars, Homebrew installs, crates.io downloads, and owner-only traffic) in
  human or flat-JSON form.

## [0.3.0] - 2026-08-23

### Added

- One-line installer (`install.sh`): downloads and verifies the platform archive against the release
  `SHA256SUMS.txt`, installs atomically to `~/.local/bin` (override with `LOCRON_INSTALL_DIR`, pin
  with `LOCRON_VERSION`), and best-effort registers a per-user daemon service
  (`LOCRON_NO_SERVICE=1` skips it). The script never edits shell configuration.
- `locron self-update`: checksum-verified, atomic self-replace; refuses package-manager-managed
  installs.
- Daemon service lifecycle: `locron service install|uninstall|status` (LaunchAgent on macOS,
  systemd user unit on Linux), a Homebrew `service` block so `brew services start locron` works,
  and postinst/postin guidance for `.deb`/`.rpm` packages. Installation never starts the daemon
  automatically.

### Fixed

- Daemon admission latency is bounded; attempts that conflict permanently stop retrying instead of
  staying `running` forever.
- Output recovery no longer treats a live attempt's captured output as orphaned.
- Self-replace closes the write handle before the rename, so the update cannot fail on a busy file.
- Installer version parsing handles quoted suffix expansions.

### Documentation

- Added contributor, security, and conduct guides plus issue and pull-request templates.
- `CHANGELOG.md` is now maintained in Keep a Changelog form, generated with git-cliff from commit
  types and curated per release; GitHub release notes are published from it.
- `docs/RELEASE.md` now covers the dependency audit workflow (cargo-deny) and changelog maintenance
  policy.
- README restructured to one install line per channel, a quick start, and a consolidated MCP
  section.

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

[Unreleased]: https://github.com/WhiteKiwi/locron/compare/v0.9.2...HEAD
[0.9.2]: https://github.com/WhiteKiwi/locron/compare/v0.9.1...v0.9.2
[0.9.1]: https://github.com/WhiteKiwi/locron/compare/v0.9.0...v0.9.1
[0.9.0]: https://github.com/WhiteKiwi/locron/compare/v0.8.0...v0.9.0
[0.8.0]: https://github.com/WhiteKiwi/locron/compare/v0.7.0...v0.8.0
[0.7.0]: https://github.com/WhiteKiwi/locron/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/WhiteKiwi/locron/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/WhiteKiwi/locron/compare/v0.4.1...v0.5.0
[0.4.1]: https://github.com/WhiteKiwi/locron/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/WhiteKiwi/locron/compare/v0.3.1...v0.4.0
[0.3.1]: https://github.com/WhiteKiwi/locron/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/WhiteKiwi/locron/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/WhiteKiwi/locron/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/WhiteKiwi/locron/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/WhiteKiwi/locron/releases/tag/v0.1.0
