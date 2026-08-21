# Milestone 1 Implementation Status

## Purpose

This is an evidence and gap report for the current implementation. It does not weaken the frozen
requirements in `SPEC.md` or turn partially implemented behavior into an accepted product change.
`TODO.md` remains the progress checklist.

## Verified implementation

The repository currently provides one Rust binary assembled from the four approved crates. The
following slices run end to end on macOS arm64:

- job creation, listing, inspection, rename/description update, enable, disable, soft removal,
  schedule preview, history, logs, manual submission, cancellation, `why`, typed configuration,
  diagnostics, and source-level daemon startup;
- non-mutating add/update/run/config/import/prune dry-run paths, including tests proving that
  creation-oriented dry runs do not initialize state;
- durable offline manual enqueue followed by prompt wake-socket admission when the daemon starts;
- atomic offline queued/retry-wait cancellation with terminal finish facts, retry-intent cleanup,
  stable terminal conflicts, and no later admission;
- a single daemon owner enforced by the permanent OS-locked file, lifetime records, stale active
  attempt classification, durable cancellation polling, and natural-drain shutdown before forced
  cancellation;
- UUIDv7 identity, epoch-microsecond timestamps, whole-second interval anchors, one-time schedules
  that disable only when due resolution occurs, five-field cron aliases/names/day-OR behavior, and
  DST gap/fold duplicate-safety behavior;
- bundled SQLite WAL/FULL state with strict tables, foreign keys, scheduled-occurrence uniqueness,
  immutable revisions/snapshots, queue sequence, attempts, retry intents, events, output metadata,
  and resumable prune states;
- manual and normal scheduled overlap admission for `skip`, bounded `allow`, and coalescing
  `replace`; durable round-robin selection; ordered catch-up gating; default global concurrency 16;
- direct argv, explicit shell, process groups, timeout/cancellation TERM-to-KILL behavior, layered
  runtime environment, reserved `LOCRON_*` values, and runtime env/body files;
- HTTP transport using Rustls, conventional `301`/`302`/`303` method rewriting, `307`/`308`
  method/body preservation, cross-origin authorization removal, response streaming, default
  success/retry status classification, and attempt timeout;
- post-admission runtime configuration failures, including a disappearing environment file, become
  non-retryable terminal attempts with finalized framed output rather than orphaned running rows;
- checksummed framed binary output preserving channel order, bounded capture with discard accounting,
  atomic partial-to-final rename, partial-tail repair, metadata finalization, and bounded pruning;
- versioned `locron.cli/v1` non-streaming JSON envelopes and redaction of persisted inline
  environment/header values from normal inspection and export.

## Verification evidence

Local verification completed on 2026-08-21:

| Toolchain | Format | Clippy | Tests |
|---|---:|---:|---:|
| Rust 1.94.0 (MSRV) | pass | pass with `-D warnings` | 57 tests pass |
| Rust 1.98.0 (latest stable) | pass | pass with `-D warnings` | 57 tests pass |

The suite includes deterministic core tests, temporary real SQLite tests, real subprocess tests,
local TCP HTTP fixtures, CLI contract tests, wake-socket daemon execution, durable cancellation,
redaction, and non-mutating dry-run checks. GitHub Actions is configured for Rust 1.94/latest on
Ubuntu and macOS, but no remote CI run is claimed by this report.

## Remaining milestone gaps

These are required before milestone 1 can be called complete:

1. Finish the complete job-update syntax and atomic, ID-aware import application; support the
   reviewed explicit plaintext-value acknowledgement flow instead of only safe redacted export and
   import validation.
2. Replace the bounded calendar catch-up scan fallback with a mathematically bounded newest-window
   algorithm and persist omitted/skipped range summary events. Prove disabled/re-enabled and
   system-local timezone-change behavior with an injected engine clock.
3. Complete replacement failure classification and exercise every overlap/concurrency interaction,
   retry restart, fixed backoff, deadline, and one-time crash boundary in deterministic engine tests.
4. Wire startup output repair/orphan cleanup and automatic daemon maintenance; complete metadata
   age/count retention in addition to the implemented output byte/age prune path.
5. Expose the full reviewed HTTP/body/header/success-range and execution-environment CLI options,
   persist resolved executable/audit hashes, warn on broad env-file permissions, and add TLS,
   process-grandchild, disappearing HTTP body-file, streaming-timeout, and noisy-output fixtures.
6. Implement live partial-file follow and the reviewed `locron.stream/v1` terminal stream contract.
   Refine human rendering beyond the current readable JSON representation.
7. Add migration-upgrade, concurrent-writer/busy, rollback/disk-failure, crash injection, retention
   stress, concurrency 16/64, and catch-up 1,000 tests.
8. Run the official Linux/macOS architecture matrix and produce the 16-criterion completion matrix.

No HTTP management/viewer, MCP, desktop, package-manager publication, or service-installer code has
entered this milestone.
