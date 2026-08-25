# Milestone 1 Implementation Status

## Purpose

This is the evidence report for the current implementation. It does not weaken the frozen
requirements in `SPEC.md`. `TODO.md` remains the progress checklist and `ACCEPTANCE.md` maps every
completion criterion to exact automated evidence.

## Verified implementation

The repository provides one Rust binary from the `locron` package, composed with four internal
library packages. All five exact-version packages are published at 0.9.0 for the secondary
crates.io source-install channel, and future releases are restricted to the repository's OIDC
trusted-publishing path. The implementation provides the complete milestone-1 surface on macOS
arm64:

- normalized job CRUD, schedule preview, enable/disable/remove, manual run, cancellation, history,
  logs/follow, `why`, diagnostics, typed configuration, export/import, and recursive CLI help;
- versioned human, `locron.cli/v1`, and `locron.stream/v1` output, including retry-aware wait/follow
  and stable target outcome exit codes;
- cron, interval, and one-time reconciliation with DST duplicate safety, bounded newest-window
  catch-up, exact compact omission summaries, durable cursors, and one-time uniqueness;
- durable overlap, concurrency, retry, replacement, cancellation, quarantine acknowledgement, and
  crash-recovery state transitions with no unknown-outcome retry;
- direct argv, explicit shell, layered environment, executable resolution, process groups,
  TERM/KILL escalation, graceful service shutdown, HTTP/TLS/redirect behavior, and bounded output;
- SQLite schema v5 with checksummed migrations, complete ordered attempts, final HTTP status and
  content type, resolved executable, resumable output/metadata retention, and global environment;
- startup and periodic maintenance for partial repair, missing/finalized reconciliation, pending
  prune recovery, age/count/byte retention, and canonical unreferenced output cleanup;
- output-storage failure boundaries that distinguish failure before execution from an unknown
  post-execution outcome, terminate spawned process groups, and retry only durable persistence.

## Verification evidence

Fresh-target local validation completed on 2026-08-22 with Rust 1.94.0 on Darwin arm64:

| Command | Result |
|---|---:|
| `cargo fmt --all --check` | pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| `cargo test --workspace --all-targets` | 194 tests pass |

The suite includes deterministic clock/time-zone tests, property tests, real temporary SQLite
databases, concurrent writers, local HTTP/TLS fixtures, real subprocess trees, SIGTERM service
lifetime checks, cross-process daemon death at four lifecycle boundaries, output/storage failure
injection, retention recovery, and complete CLI acceptance scenarios.

## Milestone evidence

GitHub Actions run
[32506527959](https://github.com/WhiteKiwi/locron/actions/runs/32506527959) passed all eight official
jobs: Linux x86_64, Linux arm64, macOS x86_64, and macOS arm64, each on Rust 1.94 and stable. Every
job recorded `uname -a` and `rustc -vV`, then passed format, Clippy with `-D warnings`, and all 194
tests, including the process-tree, service-lifetime, and cross-process crash fixtures. GitGuardian
also passed. This completes all 16 milestone-1 criteria mapped in `ACCEPTANCE.md`.

The loopback dashboard/API and MCP surface have shipped as optional clients of the same application
boundary. Release run
[32811493858](https://github.com/WhiteKiwi/locron/actions/runs/32811493858) published v0.9.0 through
the all-present crates.io gate, verified `cargo install --locked locron`, created the GitHub Release,
and completed the Homebrew update. The temporary bootstrap token was revoked, and all five packages
now require trusted publishing through `WhiteKiwi/locron`, `release.yml`, and the `crates-io`
environment.

The first push CI run after that release,
[32811491695](https://github.com/WhiteKiwi/locron/actions/runs/32811491695), exposed a false archive
inspection failure: `tar -tf | grep -q` can return failure under the hosted runner's `pipefail` even
after finding the server asset. The corrective tree inspects the already materialized archive
listing; local workflow and archive checks pass, while the complete hosted rerun remains pending
publication by the parent session. Desktop and Mac App Store ideas are deferred in
`docs/BACKLOG.md` and require reviewed specifications before implementation.
