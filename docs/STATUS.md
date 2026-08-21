# Milestone 1 Implementation Status

## Purpose

This is an evidence and gap report for the current implementation. It does not weaken the frozen
requirements in `SPEC.md` or turn partially implemented behavior into an accepted product change.
`TODO.md` remains the progress checklist.

## Verified implementation

The repository currently provides one Rust binary assembled from the four approved crates. The
following slices run end to end on macOS arm64:

- job creation, listing, inspection, complete normalized metadata/schedule/target/environment/policy
  updates, enable, disable, soft removal, schedule preview, history, logs, manual submission,
  cancellation, `why`, typed configuration, diagnostics, and source-level daemon startup;
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
  environment/header/body values from normal inspection and export;
- typed `locron.export/v1` redacted and explicitly acknowledged plaintext export, explicit history
  rejection, whole-document validation, deterministic ID/name import mapping, non-durable dry-run
  collision plans, fresh-state round trip, no-op preservation, and transactional settings/job
  application with mapping rechecks and rollback.

## Verification evidence

Local verification completed on 2026-08-21:

| Toolchain | Format | Clippy | Tests |
|---|---:|---:|---:|
| Rust 1.94.0 (MSRV) | pass | pass with `-D warnings` | 113 tests pass |
| Rust 1.98.0 (latest stable) | pass | pass with `-D warnings` | 113 tests pass |

The suite includes deterministic core tests, temporary real SQLite tests, real subprocess tests,
local TCP HTTP fixtures, CLI contract tests, wake-socket daemon execution, durable cancellation,
redaction, and non-mutating dry-run checks. GitHub Actions
[run 32486709802](https://github.com/whitekiwi/locron/actions/runs/32486709802) for commit
`af99df4` passed all four hosted jobs: Ubuntu with Rust 1.94, Ubuntu with stable, macOS 14 with Rust
1.94, and macOS 14 with stable. Every job passed formatting, Clippy with `-D warnings`, and all 78
tests. This is hosted-runner compatibility evidence; it does not replace the official architecture
and process-lifetime matrix still listed below.

The active correctness tranche now has a locally verified implementation of bounded exact
reconciliation, revision-cached compact Gregorian rank/select, durable disabled intervals and schema
v1-to-v2 migration, centralized retry timing, pre-spawn cancellation, replacement quarantine on
unconfirmed process-group termination, and transactional per-job admission slots. The working-tree
suite has 113 tests and passes format, all-target check, Clippy `-D warnings`, and workspace tests on
both Rust 1.94 and current stable. This is local evidence, not a replacement for a published hosted
run. The broad checklist items remain open until every clause in their larger verification matrices
is covered.

## Remaining milestone gaps

These are required before milestone 1 can be called complete:

1. Complete the exhaustive overlap/concurrency trigger matrix, retry/deadline interaction matrix,
   capacity-reduction fixtures, and deterministic fault points before admission, before spawn,
   after spawn, and after target exit. Exercise the 1,000-member catch-up boundary end to end.
2. Define and implement an explicit operator-resolution workflow for the safe
   `termination_unconfirmed` quarantine; until then it intentionally hard-blocks the job and
   terminally explains new submissions.
3. Wire startup output repair/orphan cleanup and automatic daemon maintenance; complete metadata
   age/count retention in addition to the implemented output byte/age prune path.
4. Persist resolved executable/audit hashes and add TLS, process-grandchild, disappearing HTTP
   body-file, streaming-timeout, and noisy-output fixtures.
5. Implement live partial-file follow and the reviewed `locron.stream/v1` terminal stream contract.
   Refine human rendering beyond the current readable JSON representation.
6. Add broader concurrent-writer/busy, disk-failure, retention stress, concurrency 16/64, and
   cross-process crash-injection tests beyond the deterministic migration/concurrency fixtures now
   present.
7. Run the official Linux/macOS architecture matrix and produce the 16-criterion completion matrix.

No HTTP management/viewer, MCP, desktop, package-manager publication, or service-installer code has
entered this milestone.
