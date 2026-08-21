# locron Milestone 1 Implementation Plan

## Status and authority

This document plans the first program milestone against the frozen behavior in `docs/SPEC.md` and the durable structure in `docs/ARCHITECTURE.md`.

Accepted foundations are Rust edition 2024, Cargo resolver 3, Rust 1.94 MSRV, the official platform matrix, the four-crate dependency direction, one `locron` binary, and an engine-owned daemon entered through `locron daemon run`. Those decisions are not Draft.

> **Review state:** milestone-1 implementation choices are accepted. Update this document and `docs/TODO.md` before deviating in code. A change to observable behavior or scope updates `docs/SPEC.md` first; a change to durable component boundaries or invariants updates `docs/ARCHITECTURE.md` first. Reviewed CLI and storage contracts live in `docs/CLI.md` and `docs/STORAGE.md`.

`docs/FINDINGS.md` preserves the research path and does not override the frozen specification. In particular, v1 has no `queue-one` overlap policy and global concurrency defaults to 16, not 4.

## Milestone approach

Implement from the inside out: deterministic domain behavior, transactional storage, daemon orchestration and runners, then the thin CLI. This order makes time, crash, and concurrency policy testable before it is coupled to a real clock or terminal.

1. `locron-core` defines normalized commands, schedules, policies, state transitions, and testable ports.
2. `locron-store` implements the architecture's persistence invariants with real SQLite transactions and migrations.
3. `locron-engine` implements the complete daemon runtime, first against fake time/execution and then against process, shell, and HTTP runners.
4. `locron-cli` composes those layers for short-lived commands and `locron daemon run`; it does not acquire daemon responsibilities.

This layering costs some domain/store mapping and trait design up front. It is justified by deterministic testing and by future viewer, MCP, and desktop surfaces needing the same behavior without depending on CLI parsing or SQLite layout.

## Accepted implementation decisions

### Accepted: identifiers and timestamps

Use RFC 9562 UUIDv7 for stable job, run, and scheduler-lifetime identities. Use parent-scoped increasing numbers for job revisions and attempts, and a database-local increasing integer for the event cursor. Keep user-editable names separate from identity.

Persist UUIDs as lowercase canonical text rather than 16-byte blobs. This costs some index and database space but keeps a local SQLite store directly inspectable and makes CLI, future HTTP/MCP, export, and logs use the same representation. UUID time ordering helps locality, but UUID-embedded time is never authoritative and semantic ordering always uses an explicit timestamp plus identity.

Persist instants as signed 64-bit Unix epoch microseconds in UTC and render them externally as RFC 3339 UTC strings. Preserve the configured timezone and schedule source separately. Measure elapsed time with a monotonic clock and persist durations in integer microseconds so wall-clock adjustment cannot produce negative execution time.

### Accepted: output storage

Stream stdout, stderr, and HTTP response bodies through one serializer into a per-attempt framed file under the user state directory. The versioned, length-delimited format preserves observed channel order and arbitrary bytes and records channel, sequence, monotonic elapsed time, and payload. A file is partial while active and is closed and atomically renamed on the same filesystem when finalized. Compression is not part of v1.

SQLite stores the logical output key, lifecycle, retained payload and physical file sizes, discarded-byte count, truncation fact/time, and pruning state. The path is derived only from durable identities. Output directories and files use owner-only permissions, and maintenance and readers do not traverse user-controlled symbolic links.

The 10 MiB per-run allowance covers retained payload across all attempts of that run. Writers keep draining after a per-run or global limit is reached, discard further bytes with saturating accounting, and do not change the target result. The 256 MiB global bound uses physical retained size: reclaim the oldest eligible terminal output first, then discard new capture if nothing eligible can be reclaimed. Render truncation from metadata so it cannot be confused with target bytes.

An attempt and logical output identity are durable before output creation and spawn. Startup repairs referenced partial files to their last complete frame and reconciles metadata; the attempt still follows `interrupted_unknown` recovery. Pruning uses durable pending state, file removal, and a completion transaction so it can resume after a crash. Delete verified unreferenced files only after a grace period.

This adds a small framed-file protocol and filesystem/database recovery work, but avoids SQLite BLOB/WAL amplification, supports efficient live follow, preserves stream order, and returns disk space immediately when output is removed.

### Accepted: SQLite operation and daemon coordination

Bundle the tested SQLite library and configure each connection with WAL journal mode, `synchronous=FULL`, foreign keys enabled, normal locking mode, a five-second busy timeout, and untrusted schema features disabled. The durability cost is intentional because a pre-spawn run commit lost after power failure could permit duplicate external execution. State directories on network filesystems are unsupported.

The daemon has one serialized writer lane and at most three reader connections; each short-lived CLI process uses one connection. Writers use immediate transactions for read-modify-write operations. Transactions never span target execution or other external I/O. Checkpoint WAL periodically in passive mode and attempt a truncate checkpoint during graceful shutdown without waiting indefinitely for readers.

A CLI returns a stable database-busy error after its five-second deadline. The daemon retries required durable transitions instead of discarding them and stops new admission while persistence is degraded. It resumes only after the store becomes writable and the transition is committed.

Use the Rust standard library's non-blocking exclusive file lock on a permanent owner-only lock file to enforce one daemon per state directory. Acquire it before migration and lifetime creation, retain the descriptor for the full daemon lifetime, mark it close-on-exec, and never remove the file on normal shutdown. Best-effort PID, lifetime UUID, start time, and binary version text aids diagnostics but never authorizes PID signalling or stale-lock breaking. OS lock release handles process death; durable scheduler-lifetime records explain prior work.

The daemon migrates after ownership acquisition. A CLI encountering an older schema may migrate only after it temporarily proves the daemon lock is free; otherwise it reports that a daemon restart is required. Revalidate the schema version inside the migration transaction. Reject databases newer than the binary. This prevents a new CLI from changing the schema beneath an older running daemon.

### Accepted: durable CLI-to-daemon control

Commit job mutation, manual enqueue, cancellation intent, and global configuration changes to SQLite before attempting notification. Use an owner-only Unix datagram socket solely as a versioned best-effort wake hint. Do not send command content or treat the socket as a management API; on receipt the engine coalesces messages and rereads durable state.

The daemon creates the socket only after taking scheduler ownership. The owner may replace a verified stale socket on startup and removes its own socket on graceful shutdown. A missing endpoint, full socket buffer, send failure, or filesystem-socket path-length limit produces degraded latency rather than a failed committed command. State-path diagnostics expose whether wake acceleration is available.

The engine event loop waits for the earliest calculated schedule/retry deadline, datagram wake, termination signal, or 30-second safety reconciliation. On every wake it samples wall time and reconciles cursors, covering suspend/resume and clock movement without turning the engine into a fixed 30-second scheduler.

Wait/follow observes the same committed run with bounded SQLite polling and framed-file reads. Client disconnection never emits cancellation. This avoids a v1 request/response protocol, authentication and reconnection state, and a hidden management server while retaining offline correctness and prompt normal operation.

### Accepted: fairness and replacement coalescing

Use durable round-robin admission. In one pass consider each eligible job once, beginning after the last durably admitted job, and admit at most one attempt from that job. Repeat passes while global capacity remains. Commit attempt creation and the new cursor together. Process, shell, HTTP, scheduled, manual, and retry work receive no hidden priority or weight in v1.

Order a job's ordinary eligible work by durable eligibility time and queue sequence. Treat a missed-run `all` batch as an ordered lane whose oldest non-terminal member, including any retry, gates the next member. This preserves oldest-first catch-up while permitting normal occurrences to interact with the active batch under the frozen overlap policy.

For `replace`, store at most one pending normal replacement candidate per job. A newer occurrence atomically marks the previous candidate `skipped_overlap` with a supersession reason and successor identity, then becomes the candidate. Persist one termination intent and do not send duplicate termination requests. Admit the newest candidate only after prior termination is confirmed; otherwise terminalize it with an explicit replacement failure. Replace a queued or retry-wait run without signalling. Do not coalesce members already selected into the same missed-run `all` batch.

This may leave a slot unused for the short duration of a pass and gives no preferential latency to manual work, but it provides deterministic sharing, bounded replacement state, and no general overlap queue.

### Accepted: runtime and dependencies

Use Tokio 1.x with only the multi-thread runtime, macros, time, process, signal, Unix networking, synchronization, I/O utility, and filesystem features required by the engine. Use `tokio-util` cancellation tokens and task tracking for structured shutdown. Blocking `rusqlite` operations remain behind store interfaces and run off Tokio worker threads; no workspace crate exposes Tokio types as domain values.

Use `rusqlite` 0.40 with default features disabled and bundled SQLite enabled. Use `reqwest` 0.13 with default features disabled, Rustls TLS, streaming, and JSON only. Redirect handling is engine-owned with reqwest automatic redirects disabled, so cross-origin sensitive-header removal and the 10-hop cap exactly match the product contract.

Use Jiff 0.2 for instants, civil time, IANA zones, system-local discovery, ambiguity classification, and RFC 3339 conversion. Read the operating system IANA database on supported Unix platforms and make missing timezone data a validation/doctor error rather than silently using UTC.

Do not depend on a cron evaluator. The core implements the accepted five-field grammar as bounded bit sets and performs field-jumping civil-time enumeration. It maps each matching civil minute with Jiff: gaps are skipped and folds select only the earlier instant. This avoids third-party DST behavior that conflicts with the frozen specification and keeps cron, interval, and one-time enumeration under one injected-clock test model.

Use `uuid` 1.x (`v7`, `serde`), `serde`/`serde_json` 1.x, Clap 4.x derive, `thiserror` 2.x for typed library errors, and `anyhow` 1.x only at the CLI composition boundary. Use `nix` 0.31 signal/process features behind Unix adapters, Rust standard file locking, `tracing`/`tracing-subscriber` for redacted diagnostics, `crc32fast` for framed-output corruption detection, `blake3` for non-secret audit hashes, and `base64` for arbitrary-byte machine output.

Use property and integration testing with `proptest`, `tempfile`, `assert_cmd`, and `predicates`; local HTTP fixtures use Tokio networking rather than adding a production server framework. Exact patch versions are resolved into the committed lockfile and must build on Rust 1.94. Default crate features are disabled where they would add alternate TLS, native system dependencies, implicit proxy behavior, or unused protocols.

### Accepted: command, storage, and persisted compatibility

Use the command and diagnostic contract in `docs/CLI.md`, including non-mutating dry-run, durable-fact `why`, repeatable verbose output, redacted debug traces, versioned JSON/stream envelopes, export/import versions, and stable exit categories.

Use the state discovery, logical schema, output framing, migration, and ownership contract in `docs/STORAGE.md`. Milestone 1 has no separate configuration file; global configuration is typed durable state. Treat machine field names, policy vocabulary, export schema, frame version, SQLite migration order, and stable IDs as compatibility surfaces. Human prose and private table layout are not public APIs.

## Scheduler and runner implementation

### Schedule reconciliation

Implement schedule evaluation as a pure operation over a normalized revision, durable cursor facts, previous events, and an injected clock. Cron uses the configured IANA or symbolic system-local zone; interval schedules advance by whole multiples from the durable anchor; one-time schedules yield at most one scheduled occurrence.

On engine startup, wake, normal tick, and observed job revision, reconcile `(cursor, now]`. Apply start deadline, then `skip`, `latest`, or bounded `all`, then persist unique runs and bounded range summaries with the cursor in one optimistic transaction. Resolving a due one-time schedule also marks its current job revision disabled in that transaction, whether the occurrence executes or is explained by missed-run policy; a manual run never performs this transition. On a conflict, recalculate from fresh state rather than committing a partial batch.

A backward wall-clock move does not rewind the durable cursor or recreate an occurrence. Re-enabling evaluates disabled elapsed time under missed-run policy and never resets an interval anchor. A new schedule revision begins at its creation/explicit anchor boundary and cannot backfill time before it existed.

Trade-off: cursor-driven reconciliation requires more durable state than recomputing only the next time, but makes sleep, restart, timezone change, and missed ranges explainable and idempotent.

### Overlap, concurrency, and admission

Queued and `retry_wait` runs count as active for same-job overlap. Normal scheduled and manual occurrences use exactly `skip`, `replace`, or `allow`:

- `skip` produces a terminal `skipped_overlap` explanation when another run is active.
- `replace` records cancellation intent and admits the newest replacement only after prior termination is confirmed.
- `allow` admits to the job bound and records `skipped_concurrency` for a new normal occurrence beyond it.

Members already materialized in one bounded `all` catch-up batch remain durable and run in scheduled-time order. Global capacity exhaustion leaves otherwise eligible work queued. The global default is 16 with range 1 through 64. `skip` and `replace` have effective concurrency one; `allow` defaults to two.

Admission rechecks state and capacity transactionally immediately before creating an attempt. Decreasing the global limit never terminates attempts already running.

### Retry and crash recovery

Classify an attempt result before scheduling retry. Retries default to zero, are capped at 10, and use fixed or capped exponential delay without jitter. A retry intent and not-before time are durable and continue as part of the same run. Timeout qualifies only when explicitly selected; cancellation, configuration error, replacement, and unknown crash outcome never do.

Resolve user cancellation in one immediate transaction after reading the current run state. A
queued or retry-wait run has no process to terminate, so mark it terminal `cancelled`, set its finish
time and reason, remove any retry intent, and append the cancellation event in that transaction. A
starting or running run keeps its state and receives durable cancellation intent for the engine to
observe and confirm through normal termination. Missing identities are not found; every terminal
state is a stable conflict rather than an apparently successful repeated request.

At daemon startup, the engine creates its lifetime and marks stale non-terminal attempts from an older lifetime `interrupted_unknown` before normal admission. It does not inspect, attach to, signal, or automatically retry a stale recorded PID/PGID. Fault injection must cover every transaction/spawn/completion boundary because persistence cannot prove whether an arbitrary external side effect occurred.

### Process and shell runner

Resolve execution configuration immediately before an attempt. Direct execution preserves argv boundaries. Shell execution selects an explicit absolute shell and never loads interactive configuration implicitly. Resolve a bare executable from the locron-owned effective `PATH` and persist the selected absolute path before spawn.

Durable admission precedes runtime file reads and target construction. Resolve each admitted attempt
independently: if its immutable snapshot, environment file, body file, URL, path, or other runtime
configuration cannot be resolved, create and finalize its empty framed output artifact and commit a
known non-retryable failed attempt/run. Do not fail an admitted batch collection in a way that leaves
one or more rows running without an execution task.

Construct the environment in the order frozen by `docs/SPEC.md`, writing reserved `LOCRON_*` values last. Missing CWD/executable/env-file and malformed runtime configuration are known non-retryable failures.

Start process and shell targets in a new Unix process group. Timeout, cancellation, and replacement send `SIGTERM`, wait the configured grace, then send `SIGKILL`. Do not commit a confirmed cancellation until termination is observed. On normal daemon signal, engine admission stops, active work receives the natural-completion window, and remaining process groups follow the same termination path.

### HTTP runner

Validate method, absolute URL, body-source exclusivity, header sources, and success ranges before queueing. At execution, verify TLS, follow redirects only when explicitly configured, cap redirects at 10, and remove sensitive headers across origins. The attempt timeout covers the whole request.

Manual redirect handling follows conventional method semantics: `303` becomes `GET` except for
`HEAD`; `301` and `302` rewrite `POST` to `GET`; `307` and `308` preserve method and body. Whenever
the method becomes `GET`, discard the request body and entity headers. Sensitive authentication and
cookie headers remain stripped whenever the redirect crosses origins.

Map connection/name-resolution/transport failure, status 408/429, and 5xx to known retry-eligible failures. Other 4xx responses fail without default retry. Stream the response body through the same bounded capture mechanism as process output; truncation never changes HTTP success classification.

### Thin CLI composition

Short-lived commands parse input, create normalized application commands, invoke shared validation/storage operations, and render typed results. They do not duplicate schedule, admission, retry, or redaction policy. Manual submission commits before returning its run ID; wait/follow attaches to the same durable run and client disconnection does not cancel it.

`locron daemon run` loads configuration, constructs `locron-store` behind core ports, constructs `locron-engine`, and enters its daemon runtime. Signal loops, locks, reconciliation, runners, maintenance, and graceful shutdown remain inside the engine.

## Edge cases to handle explicitly

- Two daemon commands start against one state directory.
- A CLI mutation races engine reconciliation of an older revision.
- Manual enqueue succeeds with no daemon and becomes visible after daemon start.
- A job is disabled, re-enabled, edited, renamed, or soft-deleted while work is queued/running.
- A deleted name is reused without confusing historical identity.
- Cron aliases and day-of-month/day-of-week OR semantics.
- System-local timezone changes while disabled or while the daemon runs.
- DST spring gap, fall repetition, backward wall-clock movement, and a large forward jump.
- Long downtime summary calculation does not iterate an unbounded number of instants.
- `all` keeps the newest bounded window and executes it oldest-first.
- A normal occurrence arrives while catch-up is active under every overlap policy.
- Global capacity is exhausted or reduced below the current running count.
- Several replace occurrences arrive during graceful termination; failed termination never creates concurrency.
- A retry becomes due while another occurrence arrives, or the daemon restarts during `retry_wait`.
- The daemon dies before spawn, after spawn/before running commit, while running, or after target exit/before result commit.
- PID/PGID reuse is never used during recovery; a child may create grandchildren or escape its process group.
- CWD, executable, env file, or HTTP body file disappears after registration.
- PATH resolution changes after a package upgrade and remains auditable per attempt.
- Environment parsing rejects invalid names, NUL, and reserved values; every output mode redacts configured sensitive values.
- HTTP redirect crosses origin, response exceeds capture bounds, or timeout occurs during streaming.
- Output finalization or pruning is interrupted between filesystem and database operations.
- Age, count, and byte retention bounds are exceeded together while active runs remain protected.
- SQLite is busy, disk is full, migration fails, or state comes from a newer incompatible schema.
- An unsupported Windows, 32-bit, or musl/Alpine build is not accidentally advertised as an official v1 artifact.

## Change plan

The plan is restricted to this repository. Before an implementation deviation, update `docs/IMPLEMENTATION.md` and `docs/TODO.md`; update `docs/ARCHITECTURE.md` first when the durable structure or invariant changes.

1. Keep the reviewed decisions in this document, `docs/CLI.md`, and `docs/STORAGE.md` synchronized before implementation deviations.
2. Create the edition-2024, resolver-3 virtual workspace with the four accepted crates, `rust-version = "1.94"`, one `locron` binary, workspace lint/profile/dependency policy, and CI on Rust 1.94 plus latest stable.
3. Implement `locron-core` domain values, normalization, state transitions, pure schedule enumeration, policy validation, and fake clock/store/executor ports.
4. Implement versioned `locron-store` migrations and transactions for jobs/revisions, cursors, runs/attempts/retries/events, lifetimes, settings, output metadata, uniqueness, soft deletion, and bounded retention.
5. Implement the `locron-engine` daemon runtime: ownership, startup recovery, reconciliation, overlap/concurrency admission, retry, cancellation, maintenance, signals, and graceful shutdown.
6. Implement process, explicit-shell, and HTTP runners in `locron-engine`, including environment/path resolution, process groups, timeout/cancellation, and bounded output capture.
7. Implement thin `locron-cli` commands and composition, including `locron daemon run`, human/versioned machine output, offline enqueue, wait/follow, import/export, prune, and doctor. Do not add another daemon crate or binary.
8. Add deterministic unit, integration, fault-injection, retention/disk-pressure, and platform tests for macOS 14+ and Linux kernel 5.14+/glibc 2.34+ on `aarch64` and `x86_64`.
9. Complete user/operator documentation and map every `docs/SPEC.md` completion criterion to executable evidence without introducing deferred viewer, MCP, desktop, packaging, or service-installation work.

## Verification strategy

- **Architecture and workspace:** check edition 2024, resolver 3, `rust-version = "1.94"`, exactly four members, one `locron` binary, the documented dependency graph, and absence of a daemon crate/binary. Run formatting, all-target compilation, strict lints, tests, and docs/link checks on Rust 1.94 and latest stable.
- **Domain unit tests:** use table/property tests and injected time for cron/interval/at enumeration, schedule revisions, DST/timezone changes, duration overflow, policy validation, retries, and every legal/illegal state transition.
- **Store integration tests:** use temporary real SQLite databases for clean and upgrade migrations, constraints, transaction races, occurrence idempotency, cursor/run atomicity, lifetime recovery, busy handling, soft deletion, output consistency, and retention order.
- **Engine integration tests:** use fake time and fake executors for wake/downtime, disabled intervals, bounded catch-up, global/per-job admission, ordering, replacement, retries, cancellation, maintenance, signals, and restart without nondeterministic sleeps.
- **Runner tests:** use real process trees and local HTTP fixtures for argv/environment/CWD/PATH, output streaming, redirect/TLS policy, result classification, timeout, TERM/KILL escalation, and grandchildren.
- **Fault injection:** terminate the daemon at every transaction/spawn/completion boundary and assert durable identity, `interrupted_unknown`, no unknown-outcome retry, and no duplicate scheduled or one-time occurrence.
- **Retention/resource tests:** exceed per-run/global output, metadata age/count, catch-up, concurrency, and database-pressure bounds; assert bounded work, deterministic truncation/eviction, and active-run protection.
- **CLI contract tests:** assert human and machine results, IDs, redaction, error categories, offline enqueue, wait disconnect, import/export round trips, invalid option rejection, doctor output, and thin delegation of `locron daemon run` to the engine.
- **Platform verification:** run process-group, signal, filesystem permission, timezone, service-lifetime, crash, and global concurrency 16/64 tests on macOS 14+ and Linux kernel 5.14+/glibc 2.34+ across `aarch64` and `x86_64`. Windows, 32-bit, and musl/Alpine results are informational only.
- **Acceptance audit:** map all 16 `docs/SPEC.md` completion criteria to an automated test or a documented official-platform check. Milestone 1 is incomplete while any criterion lacks evidence.
