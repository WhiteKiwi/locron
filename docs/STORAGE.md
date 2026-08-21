# locron Storage Contract

## Status and boundary

This document owns the reviewed milestone-1 state layout, logical schema, migration rules, and output consistency protocol. SQLite is private implementation state, not a supported external API. Future clients use application commands and versioned machine results.

## State discovery and layout

Precedence is `--state-dir`, then `LOCRON_STATE_DIR`, then the platform default:

- Linux: `$XDG_STATE_HOME/locron`, falling back to `$HOME/.local/state/locron`.
- macOS: `$HOME/Library/Application Support/locron`.

Milestone 1 has no separate configuration file. Global settings, including the locron-owned execution path, live in SQLite. The state directory is owner-only and contains:

```text
state.db
daemon.lock
wake.sock
outputs/<run-uuid>/<attempt-number>.partial
outputs/<run-uuid>/<attempt-number>.log
tmp/
```

Database, lock, and output files are owner-readable/writable only. Paths derived from identifiers use validated canonical components. locron does not traverse symlinks during output creation, repair, or pruning. Only local filesystems are supported.

## Database operation

The bundled SQLite database uses WAL, full synchronous durability, foreign keys, normal locking mode, a five-second busy timeout, disabled trusted-schema behavior, and STRICT tables. The SQLite application ID identifies a locron database. All timestamps are signed epoch-microsecond integers and UUIDs are lowercase canonical text.

The daemon serializes writes through one connection and uses at most three read connections. A CLI process uses one connection. Read-modify-write operations begin with write intent. No transaction spans process/HTTP activity, output I/O, signalling, or waiting.

## Logical schema

Exact SQL is migration-owned, but the initial schema has these records and keys:

| Record | Primary identity | Purpose |
|---|---|---|
| schema migration | version integer | Ordered migration name, checksum, binary version, application time |
| settings | singleton | Concurrency, retention, execution PATH, maintenance defaults |
| jobs | UUIDv7 | Stable metadata, exact live name, enabled and soft-delete facts, current revision |
| job revisions | job UUID + revision number | Immutable normalized definition JSON and creation facts |
| schedule cursors | job UUID + revision number | Reconciliation boundary, interval anchor, one-time resolution |
| runs | UUIDv7 | Trigger, nominal/request/eligibility times, queue sequence, snapshot, lifecycle and reason |
| attempts | run UUID + attempt number | Lifetime owner, timing, process/HTTP result, resolved executable and state |
| retry intents | run UUID | Prior attempt, durable not-before time and retry classification |
| events | increasing integer | Bounded audit and explanation cursor |
| output artifacts | run UUID + attempt number | Logical key, lifecycle, byte/truncation/discard/prune facts |
| scheduler lifetimes | UUIDv7 | PID, version, start/heartbeat/end and exit classification |
| admission state | singleton | Last admitted job and next durable queue sequence |

Complex job definitions and immutable run snapshots are canonical versioned JSON validated by `locron-core`. Frequently selected state, identity, ordering, and retention columns remain relational and indexed.

## Required constraints and indexes

- One current revision belongs to each live job.
- Live job names are unique with binary, case-sensitive comparison; a soft-deleted name may be reused without reusing identity.
- Revisions and attempts are positive and monotonically ordered within their parent.
- A non-manual scheduled occurrence is unique by job identity, revision, and nominal scheduled instant.
- Queue sequence is globally unique and increasing.
- At most one pending normal replacement candidate exists per job.
- At most one retry intent exists per run and it references the immediately preceding known attempt.
- Legal run, attempt, output, trigger, and reason vocabulary is constrained.
- Foreign keys are immediate unless one documented atomic transition requires deferral.
- Event IDs never reuse a deleted cursor value.
- Retention and admission queries have indexes beginning with terminal/eligible state and their time/order key.

## Transaction boundaries

Dedicated store operations atomically perform job revision changes, manual snapshot/enqueue, reconciliation cursor plus run materialization, overlap/replacement decisions, admission plus fairness cursor movement, attempt completion plus retry intent, cancellation intent, stale-lifetime recovery, bounded retention selection, and whole-document settings/job import.

Every operation revalidates the expected revision/state inside the transaction. A conflict returns a typed retry or conflict result; callers do not continue using stale in-memory decisions.

Import plans identify the expected destination job/revision and whether the schedule changed. One
immediate transaction rechecks all source-ID/live-name mappings before applying any row. Creates may
reuse a source UUID only when no live or removed job owns it. Updates append at most one immutable
revision and preserve the previous cursor unless the schedule changed; settings and every job action
roll back together on any conflict.

## Output file protocol

Each attempt uses one versioned binary framed stream. The header contains magic and format version. Each frame contains channel, sequence, monotonic elapsed microseconds, payload length, payload bytes, and corruption-detection checksum. Readers cap lengths before allocation and accept only complete valid frames.

The attempt and logical output key commit before file creation and target spawn. Active data uses `.partial`; successful close and sync is followed by same-filesystem rename to `.log`, then finalized metadata. Recovery scans a referenced partial file to its last valid frame, removes an incomplete tail, reconciles counts, and preserves the attempt's unknown-outcome classification.

The per-run payload allowance is shared across attempts. Physical file bytes determine the global allowance. Writers continue draining after capture stops and record saturating discarded counts. Pruning marks intent durably, removes the file, then commits completion. Missing, pending, pruned, and orphaned states remain distinguishable.

## Ownership and migration

One permanent lock file is opened without replacement and held with a non-blocking exclusive OS lock by the daemon. Its descriptor is not inherited by children. Lock text is diagnostic only; the SQLite lifetime is the durable recovery record.

The daemon locks before migrating. A CLI may migrate an older database only after proving the daemon lock is free. Each migration rechecks the version within its transaction and records its checksum. A database newer than the binary is rejected without mutation. Failed migrations roll back and leave actionable diagnostics.

## Backup and direct access

Copying only `state.db` while WAL activity exists is not a supported backup. Versioned export is the portable backup/migration surface. Diagnostic commands may run SQLite integrity and foreign-key checks, but users are not expected to edit the database directly.
