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
4. `locron` composes those layers for short-lived commands and `locron daemon run`; it does not acquire daemon responsibilities.

This layering costs some domain/store mapping and trait design up front. It is justified by deterministic testing and by future viewer, MCP, and desktop surfaces needing the same behavior without depending on CLI parsing or SQLite layout.

## README product narrative (2026-08-24)

The README leads with **“Cron that explains itself.”** and earns that claim with shipped,
inspectable behavior. Its first screen moves from cron's silent-failure problem to locron's durable
history and explanations, then shows a short CLI path using the real `add`, `preview`, and `why`
syntax and current human-rendered output. Run-specific follow-up uses `history` and `why --run`;
captured output remains separate because the queued-run confirmation or `history --format json`
must supply the canonical run ID required by `locron logs <RUN_ID>`.

The capability story follows the specification's accepted order: explainability (`why`, `history`,
`logs`, `preview`, `doctor`), reliability (explicit missed-run and overlap policies, durable
occurrence identity, recovery), then agent integration (`--format json`, dry-run mutations, and
MCP over the same application boundary). SQLite, WAL, migrations, and process-group details appear
only as supporting evidence. Installation and service-start guidance stays unchanged in substance,
and all examples must be checked against the shipped help and isolated scratch-state output. The
README advertises the shipped `locron explain` consolidated summary while retaining `why` as the
detailed job/run diagnostic. It does not advertise richer event-derived decision traces or direct
machine sleep telemetry.

## README information architecture refresh (2026-08-25)

Keep the accepted “Cron that explains itself.” positioning, but make the first screen answer four
questions without relying on the banner or badges: locron is a local scheduler; it records and
explains scheduled execution; it is for developers and automation agents; and it supports macOS
and Linux. Add one restrained supporting line for the human-and-agent audience. Do not describe
locron as AI-powered, a cron wrapper, an MCP server, or a GUI for an operating-system scheduler.

Reorder the existing material into a short operational story. The opening tour uses only shipped
syntax and demonstrates create, preview, inspect, and explain; installation follows early enough
that a convinced reader does not need to cross the reliability and agent sections first. A compact
problem statement explains why laptop sleep, restarts, network loss, missed occurrences, and
overlaps require durable scheduling facts. The reliability section then maps those conditions to
the existing missed-run, overlap, retry, timeout, cancellation, supervision, output-retention, and
startup-reconciliation behavior without claiming direct sleep detection or exactly-once external
effects.

Describe human and machine interfaces as views over the same scheduler model. Human surfaces are
the readable CLI and optional loopback-only dashboard; automation surfaces are the versioned CLI
envelope, non-mutating dry runs, and MCP. A small Mermaid feedback loop may show plan, preview or
dry-run, mutate, execute, inspect, and adjust, but it must remain secondary to concrete commands.
MCP keeps its current configuration example and exact shipped inventory while no longer carrying
the entire agent story by itself.

Add two compact scope clarifications after the practical workflow. First, explain without a
winner-takes-all feature matrix that cron is a portable scheduling primitive and launchd/systemd
are native service managers, while locron owns a consistent cross-platform job/run/attempt model,
history, policies, and explanations. Second, show the actual component boundary: CLI, dashboard,
and MCP enter the shared application boundary; the engine schedules and supervises process, shell,
and HTTP targets; SQLite stores durable state. Link to the architecture document for invariants
instead of expanding storage and process details in the README.

Preserve installation-channel ownership guidance, dashboard token and loopback boundaries, target
and schedule examples, documentation links, contribution guidance, and licensing. Verify every
command against CLI help or contract tests, validate local Markdown links, and run Markdown/style
and diff checks available in the repository. No source, schema, API, or behavior change belongs in
this documentation-only refresh.

## Consolidated job explanation implementation (2026-08-24)

The new `locron explain NAME_OR_ID` command remains a thin CLI composition over existing durable
facts. One shared current-job explanation helper resolves the live job, deserializes its normalized
definition, calculates its next schedule occurrence at one sampled wall-clock instant, reads active
runs, checks daemon ownership, and loads global concurrency. Both `why NAME` and `explain` consume
these facts. The existing `why NAME` behavior remains unchanged, including its calculated next
occurrence and overlap-oriented eligibility for a disabled job; `explain` alone suppresses the next
occurrence and reports `disabled`, as required by its consolidated-summary contract. For an enabled
job, `explain` reports `subject_to_admission` rather than claiming capacity is currently available;
the overlap decision and configured global limit are separate facts. It does not duplicate the
store's transactional admission simulation or imply that reading the report reserves capacity.

A focused store read supplies the most recent run and latest anomalous terminal run with two bounded
queries in one read transaction, using the existing run mapping and the canonical
`requested_at_us DESC, id DESC` order. The anomaly predicate uses the persisted terminal-state
vocabulary rather than reason-text matching.
This avoids relying on the general history command's 1,000-row presentation cap when retention has
not yet pruned an unusually large burst. Live-job resolution happens before the read, which preserves
the soft-delete boundary and prevents a reused name from collecting the removed identity's history.

A dedicated redacted run-summary projection selects only canonical identity, trigger, nominal and
request times, current/final state, derived actual-start/duration facts, finish time, and durable
reason from the existing observable-run representation. It excludes the immutable target snapshot,
attempt details, and event details because `explain` is a summary; `why --run` remains their detailed
surface. Human output and the JSON `data` object are both rendered from one JSON-shaped report.
Human rendering distinguishes known absence (`none`: no run/anomaly, disabled next occurrence,
manual nominal time) from a fact that is not yet known (`unknown`: start, finish, duration, or
reason), and translates the machine eligibility codes into readable phrases. No schema migration,
new durable state, new dependency, or MCP surface is required; the added store operation is read-only
and uses existing indexes and mapping.

Edge cases are pinned as contracts: no history, only successful history, one anomalous run serving
as both latest and anomaly, an older anomaly behind a newer success, an active latest run, a removed
job whose run remains explainable by ID, a removed name reused by a new live job, and unchanged
`why NAME` output for a disabled job. Redaction tests put sensitive target configuration in the job
and assert it appears in neither human nor JSON output. The generic help-surface walk must discover
`explain` and its example.

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

Startup maintenance runs after scheduler ownership and stale-lifetime classification but before new
admission. It repairs every referenced partial artifact to its last valid frame, synchronizes and
renames that artifact to its final path, and then commits reconciled output metadata. A referenced
artifact with no safe regular file becomes `missing`. Existing `prune_pending` rows resume file
removal before their durable completion transition. Verified unreferenced regular files under the
managed output tree are removed only after a one-hour grace period; symbolic links and unexpected
filesystem objects are never followed or removed automatically.

The daemon also performs one bounded maintenance batch on startup and each safety reconciliation.
Output age/byte pruning completes before metadata deletion. Terminal run metadata is selected oldest
first when it exceeds the 90-day age bound, the fixed 1,000-per-job bound, or the configurable global
count bound (default 10,000). Active work is excluded. Filesystem deletion and SQLite transitions
remain separate restartable steps, and one pass considers at most 100 artifacts or runs. Maintenance
failure is reported as degraded diagnostics but does not authorize in-memory cleanup or discard
newly due work. The schema-v3 upgrade establishes the frozen 90-day default for databases created by
the earlier milestone schema, whose placeholder value was unlimited.

### Accepted: SQLite operation and daemon coordination

Bundle the tested SQLite library and configure each connection with WAL journal mode, `synchronous=FULL`, foreign keys enabled, normal locking mode, a five-second busy timeout, and untrusted schema features disabled. The durability cost is intentional because a pre-spawn run commit lost after power failure could permit duplicate external execution. State directories on network filesystems are unsupported.

The daemon has one serialized writer lane and at most three reader connections; each short-lived CLI process uses one connection. Writers use immediate transactions for read-modify-write operations. Transactions never span target execution or other external I/O. Checkpoint WAL periodically in passive mode and attempt a truncate checkpoint during graceful shutdown without waiting indefinitely for readers.

A CLI returns a stable database-busy error after its five-second deadline. The daemon retries required durable transitions instead of discarding them and stops new admission while persistence is degraded. It resumes only after the store becomes writable and the transition is committed. A permanent completion conflict — durable state that cannot accept the outcome after retry — is not retried forever: the engine logs it once and terminalizes the attempt as `interrupted_unknown` where the store permits, so a poisoned transition can never pin a run in `running` indefinitely. Completion idempotency checks compare durable identity fields (path, byte counts, truncation) and never the retry timestamp.

Use the Rust standard library's non-blocking exclusive file lock on a permanent owner-only lock file to enforce one daemon per state directory. Acquire it before migration and lifetime creation, retain the descriptor for the full daemon lifetime, mark it close-on-exec, and never remove the file on normal shutdown. Best-effort PID, lifetime UUID, start time, and binary version text aids diagnostics but never authorizes PID signalling or stale-lock breaking. OS lock release handles process death; durable scheduler-lifetime records explain prior work.

The daemon migrates after ownership acquisition. A CLI encountering an older schema may migrate only after it temporarily proves the daemon lock is free; otherwise it reports that a daemon restart is required. Revalidate the schema version inside the migration transaction. Reject databases newer than the binary. This prevents a new CLI from changing the schema beneath an older running daemon.

### Accepted: one-time automatic removal

`--delete-after-run` is represented by a `completion_action` in the immutable job definition and therefore in every run snapshot; absent values deserialize as `retain` for backward-compatible imports and existing state. CLI validation permits `delete` only when the effective schedule is `--at`. It is intentionally a definition-lifetime action, not a caller-attachment or output-retention setting.

When a scheduled one-time run with `completion_action=delete` reaches its final terminal transition, the store soft-removes its job in that same immediate transaction. Retry scheduling keeps the job live; manual runs never qualify. Pre-execution and runner-infrastructure terminal paths use the same predicate. The removal writes `removed_at_us`, disables the job, and leaves revisions, runs, attempts, events, and output artifacts referentially intact under normal retention. Atomicity prevents a crash from recording successful execution while leaving an auto-delete definition live, or from removing it before the terminal run exists.

History renders a removed job's retained name with a removed marker. A live name resolves normally; when no live job has that name, history may resolve the removed name. Once the name is reused, the old history remains addressable by its UUID.

### Accepted: durable CLI-to-daemon control

Commit job mutation, manual enqueue, cancellation intent, and global configuration changes to SQLite before attempting notification. Use an owner-only Unix datagram socket solely as a versioned best-effort wake hint. Do not send command content or treat the socket as a management API; on receipt the engine coalesces messages and rereads durable state.

The daemon creates the socket only after taking scheduler ownership. The owner may replace a verified stale socket on startup and removes its own socket on graceful shutdown. A missing endpoint, full socket buffer, send failure, or filesystem-socket path-length limit produces degraded latency rather than a failed committed command. State-path diagnostics expose whether wake acceleration is available.

The engine event loop waits for the earliest calculated schedule/retry deadline, datagram wake, attempt-completion wake, termination signal, or 30-second safety reconciliation. On every wake it samples wall time and reconciles cursors, covering suspend/resume and clock movement without turning the engine into a fixed 30-second scheduler. The earliest pending admission deadline (queued or retry-wait run) is read durably so an idle daemon admits eligible work at its deadline instead of at the next reconciliation boundary; an attempt completion notifies the loop so a freshly scheduled retry deadline is observed without a reconciliation delay.

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

Normalize add and update through one job-definition overlay/validation path. Add supplies required
schedule and target values plus defaults; update begins with the current typed definition and applies
only explicit tri-state changes. Compare normalized values before persistence, reject no-op updates,
and pass the current durable global concurrency into policy validation. Persist one revision and one
new cursor row in the same transaction; carry the prior cursor for a non-schedule edit and use commit
time for a changed schedule.

Represent HTTP headers as typed inline or effective-environment sources in the normalized domain.
Expand success ranges before persistence, normalize JSON bodies to bytes, and resolve header env
sources only at execution. Central redaction removes inline env/header/body payloads from normal
rendering and records explicit omission paths in safe exports.

Use typed `locron.export/v1` documents rather than reusing inspection JSON or serialized database
rows. The CLI validates schema, settings, IDs, names, definitions, omissions, plaintext acceptance,
and duplicate input before mutation. It computes deterministic destination actions, then one
immediate store transaction revalidates destination identity/name facts and applies settings plus all
job creates/updates. A failure rolls back the entire import. History/output import remains deferred
and an explicit requested history export fails rather than returning partial data.

## Scheduler and runner implementation

### Schedule reconciliation

Implement schedule evaluation as a pure operation over a normalized revision, durable cursor facts, previous events, and an injected clock. Cron uses the configured IANA or symbolic system-local zone; interval schedules advance by whole multiples from the durable anchor; one-time schedules yield at most one scheduled occurrence.

On engine startup, wake, normal tick, and observed job revision, reconcile `(cursor, now]`. Apply start deadline, then `skip`, `latest`, or bounded `all`, then persist unique runs and bounded range summaries with the cursor in one optimistic transaction. Resolving a due one-time schedule also marks its current job revision disabled in that transaction, whether the occurrence executes or is explained by missed-run policy; a manual run never performs this transition. On a conflict, recalculate from fresh state rather than committing a partial batch.

A backward wall-clock move does not rewind the durable cursor or recreate an occurrence. Re-enabling evaluates disabled elapsed time under missed-run policy and never resets an interval anchor. A new schedule revision begins at its creation/explicit anchor boundary and cannot backfill time before it existed.

Disabled elapsed time is a dedicated nullable cursor fact, never an `updated_at` comparison. Disable
sets it once, re-enable preserves it, and successful cursor/materialization clears it. Migration from
schema v1 leaves existing rows NULL rather than guessing historical disablement.

Trade-off: cursor-driven reconciliation requires more durable state than recomputing only the next time, but makes sleep, restart, timezone change, and missed ranges explainable and idempotent.

The bounded reconciliation implementation separates occurrence *selection* from range accounting.
Interval schedules derive first/last indexes and counts arithmetically. Calendar schedules compile
one 400-year Gregorian match cycle, reject a calendar expression with no possible matching civil
date, and use cycle-position binary search to jump directly between matching dates while retaining
at most `catch_up_limit + 1` matching civil times. Calendar range counts use the same cycle arithmetic
plus explicit timezone-gap correction, so summary counts do not create or visit every occurrence or
elapsed UTC minute. Gap correction is linear only in actual timezone transitions inside the range;
the supported Jiff civil domain (-9999 through 9999) gives this work a hard date bound and no tzdb
transition pattern is assumed to repeat every 400 years.
Compilation is distinct from reconciliation: one reconciliation pass shares exactly one compiled
calendar object across deadline accounting, eligible accounting, newest selection, and summary
boundary lookup. The daemon keeps a revision-keyed compiled-schedule cache, so unchanged jobs do not
rebuild their Gregorian cycle on every safety tick; a revision change gets a new cache entry and
symbolic local timezone resolution remains per pass rather than being cached. Tests compare a bounded
oracle matrix and instrument compilation count, while a repeated-reconciliation work-bound test
guards against elapsed-range-dependent compilation or allocation.
One-time schedules remain a single comparison. All three return the same pure reconciliation result:
the newest eligible window in chronological order and zero, one, or two compact range facts for
deadline exclusion and policy/limit omission.

The pure reconciliation input also carries an explicit missed/normal boundary derived from the
durable reason for the pass. Startup, disabled elapsed time, and detected suspend/downtime mark their
elapsed range missed; a steady-state schedule wake identifies the exact normal boundary it was
waiting for. No fixed wall-lateness grace or safety-tick duration is allowed to infer this policy,
because changing daemon latency must not change an occurrence from normal to missed. Start-deadline
comparison is inclusive at the cutoff: an occurrence exactly `deadline` old remains eligible.

The engine receives paired wall/monotonic clock samples plus a timezone resolver port; its
coordinator supplies the explicit durable elapsed-range classification described above. A material
wall/monotonic divergence is an explicit clock-jump or suspend reason classified as missed; it is
not nominal-time lateness and does not depend on the safety interval. A steady-state `Normal` input
splits by occurrence boundary: its newest due occurrence is the one normal wake boundary, while any
older elapsed prefix is explicitly reconciled as missed under the job policy. Recovery,
disabled, and suspend ranges are wholly `Missed`. This split retains at most
`catch_up_limit` missed occurrences plus one normal occurrence, records an exact compact summary for
the omitted prefix, and never silently drops work when a safety pass covers multiple boundaries.
One reconciliation pass samples
them once, then uses those immutable inputs for every job in that pass. A symbolic
`local` schedule therefore follows a resolver change on the next pass, while a fixed IANA schedule is
unaffected. Tests use mutable fake ports to model disable/re-enable, wall-clock jumps, suspend
detection, and local-zone replacement without sleeps or process-global timezone mutation.

### Overlap, concurrency, and admission

Queued and `retry_wait` runs count as active for same-job overlap. Normal scheduled and manual occurrences use exactly `skip`, `replace`, or `allow`:

- `skip` produces a terminal `skipped_overlap` explanation when another run is active.
- `replace` records cancellation intent and admits the newest replacement only after prior termination is confirmed.
- `allow` admits to the job bound and records `skipped_concurrency` for a new normal occurrence beyond it.

Members already materialized in one bounded `all` catch-up batch remain durable and run in scheduled-time order. Global capacity exhaustion leaves otherwise eligible work queued. The global default is 16 with range 1 through 64. `skip` and `replace` have effective concurrency one; `allow` defaults to two.

Admission rechecks state and capacity transactionally immediately before creating an attempt. The
daemon uses a fixed semaphore of 64 permits as the process-local hard ceiling and passes its current
available permits to the admission store operation. In the same immediate transaction that selects
attempts, the store rereads the durable global setting, counts durable starting/running attempts, and
caps selection to `min(hard_guard_available, configured_limit - active_attempts)`. The setting remains
validated to 1 through 64. This atomic recheck means a CLI setting writer serializes wholly before or
after admission; a stale pre-read cannot over-admit after a decrease or hide capacity after an
increase. Because one daemon serializes ticks, attempt tasks acquire their hard-guard permits before
another admission pass; a concurrent completion can only make the calculation conservatively small.
A decrease below current active work yields zero admission without cancellation, and a later
increase is visible on the next wake/pass without rebuilding the semaphore or restarting the daemon.

The acceptance matrix is deterministic store/engine testing rather than timing-based daemon tests.
For each overlap policy it crosses scheduled, manual, and catch-up work with zero/global/per-job
capacity, then repeats the relevant rows after a durable limit reduction. Retry-wait occupies the
same-job overlap set, and an eligible retry remains the same catch-up lane member and precedes the
next member. A retry already durably selected is not rechecked against its original occurrence's
start deadline; deadline filtering is complete before the run's first attempt.

### Retry and crash recovery

Classify an attempt result before scheduling retry. Retries default to zero, are capped at 10, and use fixed or capped exponential delay without jitter. A retry intent and not-before time are durable and continue as part of the same run. Timeout qualifies only when explicitly selected; cancellation, configuration error, replacement, and unknown crash outcome never do.

Resolve user cancellation in one immediate transaction after reading the current run state. A
queued or retry-wait run has no process to terminate, so mark it terminal `cancelled`, set its finish
time and reason, remove any retry intent, and append the cancellation event in that transaction. A
starting or running run keeps its state and receives durable cancellation intent for the engine to
observe and confirm through normal termination. Missing identities are not found; every terminal
state is a stable conflict rather than an apparently successful repeated request.

The final mark-running transaction rechecks cancellation intent before external execution. If a
user or replacement request arrived while the attempt was durably `starting`, the transaction
terminalizes the attempt/run and its not-yet-created output artifact without spawning; the daemon
treats this as a normal no-execute decision, not persistence degradation. A transient store error at
this boundary never releases the admitted attempt or authorizes execution from memory. The task keeps
its capacity permit and retries the same transaction with capped exponential delay; every retry
therefore rechecks durable cancellation. Retry waiting is interruptible by daemon shutdown, which
leaves the durable `starting` attempt for next-lifetime unknown recovery and still never spawns.
Persistent failure reports degraded persistence and blocks that task until shutdown or a durable
ready/no-execute decision.

Required transitions are idempotent under a commit-success/response-loss ambiguity. Repeating
mark-running for the same already-running attempt returns ready only after rechecking that exact
attempt and current cancellation; cancellation observed on the ambiguous retry terminalizes before
spawn. The runner produces a target outcome once, retains it in the task, and retries durable
completion with the same capped backoff without re-executing the target. The composition clock is
sampled exactly once when that outcome returns; the resulting completion instant, retry eligibility,
and immutable completion command are reused across every retry rather than recalculated by the store
adapter. Shutdown interrupts this
retry and leaves the active durable record for next-lifetime unknown recovery. Repeating an already
committed identical completion succeeds idempotently; a mismatched result remains a conflict.

For a running process, termination is a bounded process-group-liveness state machine rather than a
detached sleep task. Send TERM once and observe both the owned leader wait handle and process-group
existence during the grace interval. Leader exit alone is insufficient because an in-group descendant
may still be running. At the grace boundary send KILL only while the group still exists, then require
both leader reap and group absence within a second bounded grace. Signal errors other than an already-gone group and an
unconfirmed child become a typed non-retryable termination-confirmation result. Its completion
transaction marks the attempt `interrupted_unknown`, keeps the original run in an active-blocking
quarantine with no live runner ownership, and terminally fails the queued replacement candidate.
Startup recovery preserves that quarantine but never inspects or signals its recorded PID/PGID;
therefore neither the failed candidate nor later same-job work can overlap an unconfirmed process.
Quarantine is never cleared merely by a daemon restart. Admission hard-blocks the quarantined job
regardless of a later `skip`, `replace`, or
`allow` revision. `skip` submissions retain ordinary overlap explanations; a new `replace` submission
terminally fails in its enqueue transaction instead of waiting forever; `allow`, scheduled, and
catch-up submissions become explainable overlap terminals rather than a permanent queued backlog.
The existing cancel application command gains one explicit acknowledgement mode. Its immediate
transaction accepts only the exact active-blocking `termination_unconfirmed` run, writes a dedicated
audit event, and terminalizes that run as `interrupted_unknown`. Ordinary cancel returns an
actionable conflict for quarantine, while acknowledgement of a non-quarantine or a repeated
acknowledgement is a stable conflict. Neither path reads or signals a recorded process identity.
Never signal a recorded stale PID/PGID after lifetime recovery. A process
that deliberately escapes its inherited group remains outside v1 process-tree control.

At daemon startup, the engine creates its lifetime and marks stale non-terminal attempts from an older lifetime `interrupted_unknown` before normal admission. It does not inspect, attach to, signal, or automatically retry a stale recorded PID/PGID. Fault injection must cover every transaction/spawn/completion boundary because persistence cannot prove whether an arbitrary external side effect occurred.

Retry classification and timing are one pure decision over the immutable run snapshot, attempt
number, known outcome class, and injected completion instant. Fixed delay never doubles;
exponential delay is `base * 2^(attempt-1)` with saturating arithmetic and the configured cap.
Durable `retry_wait` is the only restart-resumable retry source. Completion rejects retry plans for
cancelled, configuration, replacement, or interrupted-unknown outcomes, and startup recovery deletes
any inconsistent retry intent attached to an unknown run rather than synthesizing an attempt.

Crash tests use store fault points at durable admission, spawn acknowledgement, and completion. A
committed attempt without a committed result is always recovered as `interrupted_unknown`, including
the target-exited/result-not-committed boundary. A queued one-time occurrence is never rematerialized
because occurrence uniqueness and its advanced cursor are committed together.

Acceptance fault tests inject failure before admission, after durable admission while starting,
after the running acknowledgement, and after a target outcome but before completion commit. They
assert no pre-admission attempt, no pre-spawn execution, unknown recovery without retry after an
ambiguous external side effect, completion retry without a second execution, and one-time occurrence
uniqueness across every recovery boundary. The 1,000 catch-up limit is exercised through the real
adapter and SQLite store: exactly the newest 1,000 explicit runs are materialized, one compact event
accounts for the omitted prefix, admission begins at the oldest retained nominal time, and duplicate
reconciliation adds neither runs nor per-occurrence omission events.

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

The version flag is owned by the CLI instead of clap's built-in version flag: the built-in flag exits during parsing and cannot honor `--format json`. `disable_version_flag = true` removes it, and a top-level, deliberately non-global `-V/--version` boolean is handled in `main` before tracing or state discovery, rendering `locron <version>` for human output and the standard `locron.cli/v1` envelope with `command` `version` and `data` `{"version": ...}` for JSON output, using `env!("CARGO_PKG_VERSION")`. Keeping the flag non-global preserves the existing rejection of `locron add -V` as an unexpected argument. The subcommand field becomes `Option<Command>`; when version is present the subcommand is ignored, and when it is absent the CLI reproduces clap's two existing failure surfaces byte-identically: a bare `locron` renders the full help to stderr with exit code 2 through the container-level `arg_required_else_help`, and an invocation with other arguments but no subcommand re-parses the original arguments with `subcommand_required(true)` via `try_get_matches_from` to emit the native `MissingSubcommand` error and subcommand list (`Command::error` alone renders a raw error without the command context, so it is not used). Because the field is optional, `override_usage = "locron [OPTIONS] <COMMAND>"` keeps the required-command spelling in every help and error path, and the version flag's `display_order` places it after the automatic help flag to preserve the baseline option order. The full parse completes before the version short-circuit, so invalid arguments such as `-V --format bogus` are rejected with exit code 2 rather than printing the version. Note that `required_unless_present` on the subcommand field is silently ignored by clap_derive 4.6 and must not be relied on.

`list` and `remove` carry Clap 4 visible subcommand aliases (`ls`, `rm`) on their command variants. The aliases are visible rather than hidden so `locron --help` advertises the shorthand. An alias resolves to the same enum variant, so dispatch, option handling, help, and the canonical command names hard-coded at each `render` call site are untouched: machine output for an aliased invocation is byte-identical to the canonical spelling. This adds no dependency and changes no product behavior, so the frozen specification is not amended.

Human `list` output renders a docker-style aligned table instead of the shared pretty-JSON fallback: a header line plus one row per live job, columns NAME, SCHEDULE, TARGET, and ENABLED derived from the redacted durable record only. Schedule summaries are `cron 'EXPR'`, `every DUR`, or `at RFC3339`; target summaries are `run EXE [ARGS...]`, `shell CMD`, or `http METHOD URL`; ENABLED is `yes` or `no`. Alignment is hand-rolled from the maximum column width with no new dependency — the workspace has repeatedly rejected non-essential crates — and values are never truncated. An empty result prints the header alone, matching `docker ps` with zero containers. Only the human `list` path changes: the `list` dispatch arm in `execute` branches on format (table for `Format::Human`, the unchanged shared `render` otherwise), so the JSON envelope, the canonical `command` field, and every other command's rendering are untouched. Human output is not a compatibility surface, but contract tests pin the table so it cannot regress accidentally. Other list-like commands (`history`) keep the pretty-JSON fallback until a reviewed decision extends table rendering.

Implementation deviations from the plan above, all confined to `locron`:

- The summaries parse the redacted `definition_json` as JSON values rather than deserializing into typed `JobDefinition`: a redacted inline body is the string `"<redacted>"`, which serde rejects for the typed `Vec<u8>` body field ("expected a sequence"). Value-level parsing still reads only the redacted record, so the redaction guarantee is unchanged.
- The table's renderers are named `list_schedule_summary`/`list_target_summary` because the export-selection work in the same file already owned the typed `schedule_summary(&Schedule)` name for its picker rows; both share `human_duration`.
- `every DUR` renders the largest whole unit (`s`, `m`, `h`, or `d`) that divides the stored microseconds, matching the CLI's input grammar; a sub-second value (which the grammar can never produce) falls back to the raw `{N}us` rendering rather than truncating.
- Rows never carry trailing whitespace: the last column is not padded.

`locron daemon run` loads configuration, constructs `locron-store` behind core ports, constructs `locron-engine`, and enters its daemon runtime. Signal loops, locks, reconciliation, runners, maintenance, and graceful shutdown remain inside the engine.

## Human rendering implementation (2026-08-24)

Implements the frozen human-output-contract amendment (issue #4). Each command's `Format::Human` branch calls a dedicated renderer instead of the shared pretty-JSON fallback; the `Format::Json` path and the `render` envelope are untouched, so machine output is byte-identical. Renderers consume only the redacted records the JSON path already uses, so redaction parity holds by construction.

Shared helpers live beside the existing list renderer: column alignment and human durations reuse `render_list_table` and `human_duration`; schedule and target summaries reuse `list_schedule_summary` and `list_target_summary`; run-state and trigger names render from the existing state vocabulary. No new dependency is added — every form is hand-rolled `println!` composition. `doctor` keeps its existing check evaluation and only changes presentation. `why` reuses its explanation facts and flattens them into labeled sections. `logs`, `run --wait` streams, `export`, `service`, `self-update`, `version`, and `mcp` are already conformant and are not changed.

Contract tests pin every command's human form for empty and populated states, dry-run wording, table-only ID abbreviation, and redaction, mirroring the existing help-surface walk so a new command cannot omit its human form silently. The README demo screencast (`assets/screencast.sh`) dropped its `jq` pipes as part of this change; the recording itself is regenerated separately.

Implementation deviations, all confined to `locron` and confined to human branches:

- The plan text above claims `run --wait` streams are "already conformant and are not changed". In fact the human wait stream is part of this work: after the queued line it now prints the terminal outcome line `run finished: {id} ({state})`, per the `docs/CLI.md` contract. The streamed progress lines themselves are unchanged.
- `why --run` also prints an EVENTS section (`  {RFC3339} {kind}` per durable event) beyond the contract's RUN/ATTEMPTS/terminal-reason sections. The events are already loaded to produce the terminal-reason text, so this costs no extra record access and the contract does not forbid it.
- The `why --run` RUN section omits a job-name line: the job is soft-deleted in the general case, so a name would require a best-effort extra lookup with an ambiguous fallback. The run id, trigger, and timestamps identify the run without it.
- The `history` table has no run-id column — the contract's "run ID may abbreviate in table only" permission is unused. When the job has been soft-deleted, the JOB column falls back to the abbreviated job id (first 8 hex characters) instead of the missing name.
- Dry-run wording choices: `{key}: would be configured (dry run; no changes made)` for `config set`, `dry run: would create N, update N, unchanged N; no changes made` for `import`, `dry run: would prune {runs} runs, {outputs} outputs ({bytes} bytes)` for `prune`, and `{decision}: {name}` plus `dry run: no run created` for `run --dry-run`.
- `prune` reports "N runs" as the count of distinct run ids: prune rows are per-attempt, and a run with several attempts is counted once.
- `import` no-op action lines print the import plan's pre-existing job name and id for the no-op entry (a no-op plan entry does not carry a resolved record id).

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
7. Implement thin `locron` commands and composition, including `locron daemon run`, human/versioned machine output, offline enqueue, wait/follow, import/export, prune, and doctor. Do not add another daemon crate or binary.
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

## Installer and self-update implementation (post-milestone delivery, 2026-08-23)

This section plans the installation-channels amendment to `docs/SPEC.md`: the Homebrew-independent one-line installer and the built-in self-update subcommand. Evidence and rejected alternatives are recorded in `docs/FINDINGS.md` §11.

### Accepted: evergreen install script

Ship one POSIX `sh` script, `install.sh` at the repository root, as the source of truth. The release pipeline attaches it to every GitHub Release, so the canonical one-liner is version-consistent with the artifacts it installs:

```
curl -fsSL https://github.com/WhiteKiwi/locron/releases/latest/download/install.sh | sh
```

The script resolves "latest" exclusively through `releases/latest/download/{asset}` redirects and never calls the GitHub REST API, avoiding the 60/hour unauthenticated limit documented in `docs/FINDINGS.md` §11. A pinned install uses `LOCRON_VERSION=vX.Y.Z`, switching the base to `releases/download/vX.Y.Z/`. Supported targets are exactly the four published release targets: `aarch64-apple-darwin`, `x86_64-apple-darwin`, `aarch64-unknown-linux-gnu`, `x86_64-unknown-linux-gnu`. Detection is `uname -s` (Darwin/Linux) and `uname -m` (arm64/aarch64, x86_64); musl detection via `ldd` must refuse with an actionable error because only glibc builds are published. Any other OS or architecture fails with an actionable unsupported-platform error before any download.

The script downloads `SHA256SUMS.txt` from the same release, selects the line for the chosen target, validates the entry as 64 hex characters, and verifies with `shasum -c` (or `sha256sum`) before extracting. Trust rests on HTTPS to the same origin as the artifacts — the same trust model as mise's pinned-version path. Extraction happens in `mktemp -d` with a cleanup trap; the binary is copied to a temp file inside the install directory, made executable, and atomically renamed over the target, correcting the `rm`+`mv` window present in mise.run's own script.

The default install path is `$HOME/.local/bin/locron`, overridden by `LOCRON_INSTALL_DIR` (a full file path; a directory value is an error, as in mise). No root is required and no shell configuration is modified: the script prints per-shell guidance for adding the directory to `PATH` when it is absent, detected from `$SHELL` like mise's `after_finish_help`. Re-running the same command downloads, verifies, and atomically replaces the binary — this is the update path for script-installed users, and no skip-if-exists option is added because re-running is cheap and deterministic. Missing `curl`/`wget`, download failures, checksum mismatches, extraction failures, and unwritable install directories each produce a specific actionable error and a non-zero exit.

A short custom domain serves the one-liner as `https://locron.whitekiwi.link/install.sh` (added 2026-08-24, completing the TODO follow-up). It is not a hosted script copy: a CloudFront viewer-request function (distribution `E2SNYXU6Z3ZE4N`, function `locron-redirect`, OAC `E2BUNP08WL3O60` in front of a private dummy S3 origin) 302-redirects `/install.sh` to the canonical release asset above and other paths to the repository. The served script is therefore always the version-consistent release asset with no release-pipeline change and no drift, the same trust level as the GitHub one-liner; the GitHub URL remains canonical in the documentation.

### Accepted: self-update subcommand

Add `locron self-update` to the CLI. It updates only to the latest stable release; pinning remains an installer function per the frozen specification.

Version resolution uses `GET https://api.github.com/repos/WhiteKiwi/locron/releases/latest`. This is an explicit user-triggered action, so the unauthenticated rate limit is acceptable; a rate-limit or network failure maps to the CLI's stable error categories with retry guidance, and no file is touched. The subcommand then downloads the matching tarball and the release's `SHA256SUMS.txt`, verifies the tarball hash (adding `sha2`, `tar`, and `flate2` as pure-Rust dependencies of `locron`, plus `reqwest` with rustls/stream/json for the API and asset downloads — reqwest is already a workspace crate, and the `self_update`/`self-replace` crates were rejected as unnecessary surface for one temp-file-plus-rename), and extracts in a temporary directory. The extracted binary is copied to a temp file in the same directory as the running executable and replaced with a single `fs::rename`, which is atomic on both platforms: the running process keeps its old inode, and the next invocation executes the new binary.

Package-manager refusal follows the mise pattern with a marker we control: the tap formula creates `lib/.disable-self-update` under the brew prefix at install time, and `self-update` refuses with a stable error directing the user to `brew upgrade locron` when the marker exists next to the canonicalized current executable. Script-installed and source-installed binaries have no marker and remain updatable. All verification and download failures occur before the rename, so a failed or interrupted update leaves the existing binary installed and working, as the specification requires. Human output reports the current and new version or "already up to date"; machine output uses the standard `locron.cli/v1` envelope with `command: "self-update"`.

Testability uses rustup's override seam: `LOCRON_UPDATE_API_BASE` and `LOCRON_UPDATE_ASSET_BASE` environment variables default to the production hosts and let contract tests point at a local HTTP fixture serving a fake `releases/latest` document, tarballs, and checksums. No automatic update check or auto-update behavior is added in this amendment.

### Accepted: tap formula marker and release pipeline

The formula template embedded in `.github/workflows/release.yml` gains one line that creates the self-update marker inside the prefix (`touch lib/.disable-self-update`), and the pipeline attaches `install.sh` to GitHub Releases so the canonical one-liner exists for every published version. `docs/CLI.md` documents the `self-update` command under the reviewed CLI contract, and the README installation section adds the one-liner plus the per-channel update story (re-run the script, `brew upgrade`, or `self-update`). *(The per-channel story later moved to `docs/INSTALL.md`, which the README installation section now links.)*

Implementation deviation, corrected 2026-08-24: the deployed template wrote the marker line as `FileUtils.touch` and carried an explicit `version` line, and the tap's `brew test-bot` failed on five consecutive bumps with the corresponding `brew style` and `brew audit` offenses (evidence in `docs/FINDINGS.md` §20). The template now writes `touch lib/".disable-self-update"` and renders the literal version into the four URL strings (`.../download/v${VERSION}/...`) with no `version` line — Homebrew scans the version from the literal URL token, the canonical pattern for GitHub-release binary formulas. Keeping `#{version}` placeholders without an explicit `version` line fails formula loading (`version (nil)`), so the template interpolates the version at generation time instead. Behavior is unchanged; the fix takes effect at the next tag because the workflow evaluates at the tagged commit.

### Edge cases to handle explicitly

- A machine has both a brew-installed and a script-installed locron; `PATH` order decides which runs, and each channel updates through itself.
- The daemon is running during self-update: replacement is atomic and the running daemon keeps the old code until its next restart; the operator documentation states this explicitly.
- The script or self-update runs on musl Linux, Windows, or an unknown architecture: refuse with the published-platform error, never guess.
- `LOCRON_INSTALL_DIR` is a directory or an unwritable path; `/tmp` is mounted noexec (mirror rustup's actionable message).
- A checksum file line for the target is missing or malformed; the tarball is truncated or corrupted mid-download.
- The GitHub API is rate-limited or unreachable during self-update; the tap formula is installed with an old marker layout.
- A pinned `LOCRON_VERSION` refers to a release whose checksum asset or tarball does not exist.

### Verification additions

- **Installer static checks:** `sh -n` and shellcheck pass in CI; the script contains no bashisms; a fixture-server test runs it end-to-end on macOS and Linux CI legs against a fake release layout (latest redirect, pinned version, checksum mismatch, unsupported arch, unwritable dir) with `LOCRON_VERSION` and the asset-base override, then executes the installed binary's `-V`.
- **Installer release check:** a pinned run against the real `v0.1.1` release into a temporary directory installs a working binary on both macOS architectures and Linux.
- **Self-update contract tests:** local HTTP fixture drives latest resolution, checksum verification, atomic replacement (the pre-update process keeps running while new invocations run the new binary), marker-file refusal with brew guidance, "already up to date", rate-limit error mapping, and JSON envelope output; failure injection proves the old binary is untouched after download/verify errors.
- **Formula marker:** the tap formula template contains the marker line, and a manual `brew reinstall` followed by `self-update` refusal is recorded as evidence at the next release.
- **Platform matrix:** the existing four-target CI runs the new suites; Windows, 32-bit, and musl results remain informational.

### Accepted: literal Homebrew formula rendering (2026-08-24 release follow-up)

The v0.6.0 release exposed a shell-expansion defect in the inline, unquoted formula heredoc: Ruby
documentation backticks were executed as shell command substitutions before the formula was written.
The release workflow therefore no longer owns an executable heredoc. A checked-in formula template
is plain data with explicit version and checksum tokens, and a small POSIX renderer validates the
release version and all four lowercase SHA-256 values before replacing only those tokens. The
workflow redirects the renderer's standard output into the cloned tap. Literal Ruby comments and
caveats never pass through shell evaluation, while release-derived values still render into the
four literal URLs and checksum fields Homebrew requires for version scanning.

The renderer fails if a value has the wrong shape or a template token remains. A deterministic
regression script renders fixed fixture values and asserts the complete package-manager guidance,
service-upgrade caveat, literal backticks, URLs, checksums, marker, and absence of trailing
whitespace. Push CI runs this check and shellchecks both scripts. This keeps the release-only path
executable before the next tag rather than relying on another publication to discover template
corruption.

The already-published v0.6.0 formula is repaired directly in `WhiteKiwi/homebrew-tap`: retain the
current v0.6.0 literal URLs and checksums, restore the guidance byte-for-byte from the style-clean
v0.5.0 formula, then require the tap's `brew test-bot --only-tap-syntax` workflow to succeed. No
locron release asset or product binary changes, and no v0.6.1 re-release is needed.

## Daemon service installation implementation (post-milestone delivery, 2026-08-23)

This section plans the daemon-service amendment to `docs/SPEC.md`: per-user registration and automatic startup of the daemon by the script installer, a Homebrew service definition for `brew services`, and refresh-and-restart behavior on updates. Evidence and rejected alternatives are recorded in `docs/FINDINGS.md` §12.

### Accepted: binary-owned service registration

`locron` owns a new `locron service install|uninstall|status` family behind a small service-manager port. The port has two real backends — launchd (macOS) and systemd user units (Linux) — and a deterministic fake for tests. `locron-engine` and the store are unchanged: the daemon already performs graceful SIGTERM shutdown, single-owner locking, and stale-attempt classification, which is everything a service manager requires of it. install.sh and self-update call the subcommand rather than shelling out to `launchctl`/`systemctl` themselves, keeping the POSIX script thin and the behavior unit-testable. No new dependencies: the backends run `launchctl`/`systemctl` as child processes.

Templates are embedded constants, not files shipped in archives. The macOS plist carries label `dev.locron.daemon`, `ProgramArguments` `[<current_exe>, "daemon", "run"]`, `KeepAlive` true, `RunAtLoad` true, and `StandardOutPath`/`StandardErrorPath` both at `~/Library/Logs/locron/daemon.log` (created at install; the Homebrew default-log-path convention). The Linux unit is `locron.service` at `~/.config/systemd/user/` with `ExecStart=<current_exe> daemon run`, `Restart=on-failure`, and `WantedBy=default.target`. Registration always uses the canonicalized absolute path of the running binary, so repeating it repairs a registration whose binary moved or was replaced.

### Accepted: macOS registration flow

Install writes the plist user-owned 0644, runs `launchctl enable gui/<uid>/<label>`, and consults `launchctl print` for the label. If the job is already loaded, install refreshes the plist and sends `SIGTERM` with `launchctl kill`; `KeepAlive` then restarts the job on the new binary, and the engine's ordinary graceful-shutdown sequence handles active work. If not loaded, install first checks the state-directory daemon lock with the store's existing lock probe and then bootstraps, so a manual daemon holding the lock is never shadowed by a restart loop. `bootstrap` into the `gui` domain can fail outside a GUI login session (for example over SSH); the backend falls back to the `user/<uid>` domain with an explanatory note, the path Homebrew itself uses. Because the termination semantics of `bootout` are undocumented (open question recorded in `docs/FINDINGS.md` §12), uninstall signals `SIGTERM` first, waits for the signaled process to exit, and runs `bootout` plus plist removal as cleanup; a live macOS test validates this ordering rather than relying on an assumed contract. Two launchd realities shape the uninstall wait: a KeepAlive job never leaves the domain until `bootout`, so the wait watches the process (its pid disappears, or a respawned pid replaces it) instead of the domain; and `launchctl kill`/`bootout` fail with exit 3 ("No process to signal"/already unloaded) whenever the job is between KeepAlive respawns, which is treated as the state the caller wants. Status reports the loaded domain, PID, and binary path from `launchctl print`.

### Accepted: Linux registration flow

The systemd backend first proves a usable user manager: `XDG_RUNTIME_DIR` set and the user bus reachable, probed with `systemctl --user show-environment`. Without one (SSH, containers, cron), `service install` prints the explicit guidance required by the specification and exits zero — installation remains successful. With a manager, install writes the unit, runs `systemctl --user daemon-reload`, then `enable --now`. When the unit is already active the refresh runs `stop` followed by `enable --now` — a bare `enable --now` would never restart a loaded daemon, because `systemctl start` on an already-active unit is a no-op; the `stop` signals SIGTERM and the subsequent start launches the new binary, the same graceful sequence as macOS. Uninstall runs `stop` and `disable`, removes the unit, and reloads. Status reports `is-active` and `is-enabled`. The unit stops with the login session by design; the operator guide documents `loginctl enable-linger` as the optional step for boot persistence (self-lingering requires no administrator authentication per `docs/FINDINGS.md` §12).

### Accepted: installer, self-update, and package integration

install.sh, after its atomic binary replace, runs `<installed> service install` unless `LOCRON_NO_SERVICE=1`, passing its output through. A zero exit with guidance output (no Linux session) leaves the install successful, exactly as the specification requires. Any other non-zero exit from the registration attempt also leaves the installation successful: the script warns and continues, because the binary replacement is the essential install and the registration attempt is best-effort by design (`LOCRON_NO_SERVICE` exists to decline it). The same tolerance is recorded in this backlog's step evidence.

self-update runs `service install` on the replaced executable after its own successful atomic replace (the child inherits the environment and its output is captured so the update envelope stays clean): if the daemon was service-managed, this refreshes and restarts it onto the new binary; if no registration existed, it performs a first registration; if the daemon was started manually, the registration is written and the lock check defers the start until the manual daemon stops. Registration is best-effort: a failed post-replace registration becomes a warning in the update envelope (and on stderr in human mode), never an update failure. The brew-managed refusal reuses the existing `lib/.disable-self-update` marker: `service install` and `service uninstall` refuse on a marker-bearing binary with a stable error directing to `brew services`.

The update flow must not re-resolve its own executable path after the replace: on Linux `/proc/self/exe` of a process that renamed its binary over itself resolves to the deleted old inode (`path (deleted)`), so the post-replace `fs::canonicalize(current_exe())` fails and the registration was silently skipped. The flow therefore captures the canonical executable path once before the atomic replace and threads it through `replace_binary` and `register_service` (macOS never showed the bug because `_NSGetExecutablePath` returns the exec-time path string without re-checking the filesystem).

The release.yml formula template gains a `service` block (`run [opt_bin/"locron", "daemon", "run"]`, `keep_alive true`, `run_at_load false`) and a caveats line pointing at `brew services start locron`; installation never starts the service. `brew upgrade` does not restart running services (`docs/FINDINGS.md` §12), so after an upgrade that caveat remains the documented restart path. The deb/rpm postinst prints the same guidance as the no-session Linux path; it never registers anything.

### Edge cases to handle explicitly

- `service install` while a manual `locron daemon run` holds the state lock: write and enable the registration, then report that it will start the daemon after the manual process stops, without bootstrapping.
- A service-loaded daemon exits because the lock is held elsewhere: launchd/systemd keep retrying at their throttle interval — safe (the engine's single-owner check never executes work twice) but visible in status output.
- macOS over SSH: `gui` bootstrap failure falls back to `user/<uid>` with a note.
- The binary is removed or moved after registration: restarts fail until `service install` re-registers; status surfaces the stale path.
- Linux self-update: the running process's `/proc/self/exe` points at the deleted inode after the replace; any post-replace path resolution must reuse the pre-replace capture.
- Two concurrent registrations: idempotent writes and enable calls; the last one wins.
- An update restart lands while jobs run: the engine's graceful-shutdown sequence applies unchanged, and interrupted attempts follow the existing recovery contract.
- Linux logout while jobs run: the session manager signals the daemon and the same graceful sequence runs.
- `service status` on an unsupported platform or without a state directory: a stable diagnostic, with no registration attempted.

### Verification additions

- **Fake-port contract tests:** template rendering (canonicalized path, label/unit name, KeepAlive/RunAtLoad, Restart=on-failure/WantedBy, log paths), enable/bootstrap/kill ordering, lock-held deferral, brew-marker refusal, no-session guidance, and machine-output envelopes, all without touching a real service manager. The envelope's `service_name` assertion uses the platform-native name (`dev.locron.daemon` on macOS, `locron.service` on Linux), never a hard-coded label.
- **Real-backend tests:** on the macOS CI leg, register/restart/unregister against the domain available on CI (`gui` when a GUI session exists, `user/<uid>` otherwise), asserting the plist, loaded state, and graceful SIGTERM restart with a marker process; on the Linux leg, run a real user manager under `dbus-run-session` to cover daemon-reload, enable --now, stop, and disable.
- **install.sh fixtures:** a default run attempts registration and tolerates the guidance exit; `LOCRON_NO_SERVICE=1` skips it entirely.
- **Release artifact checks:** the formula template contains the `service` block and the release attaches the updated script; a built .deb contains the postinst guidance.
- **Live evidence items:** `brew services start locron` starts the daemon, and `brew upgrade` leaves the old daemon running until `brew services restart` — recorded at the next release, like the self-update marker check.
- **Platform matrix:** the existing four-target CI runs the new suites; Windows, 32-bit, and musl results remain informational.

## Usage and installation measurement (maintainer tooling, 2026-08-23)

This section plans maintainer-facing measurement of locron's public distribution channels. It changes no product behavior, so the frozen `docs/SPEC.md` is not amended. Evidence and rejected alternatives are recorded in `docs/FINDINGS.md` §13.

### Accepted: dependency-free snapshot script

`scripts/usage.sh`, POSIX `sh`, depends only on `curl` and the optional `gh`, and prints one snapshot with these sections:

1. **GitHub Releases** — per-release asset download totals and a grand total from `GET /repos/WhiteKiwi/locron/releases`, paginating with `per_page=100` and following `Link` header pages with a sane page cap. Counts are cumulative and reset on asset re-upload, so the output labels them accordingly.
2. **Stars** — `stargazers_count` from the repository endpoint.
3. **Homebrew** — `whitekiwi/tap/locron` install counts for 30, 90, and 365 days from formulae.brew.sh; a missing entry renders as 0; output notes the anonymous/opt-out undercount.
4. **crates.io** — queries `/api/v1/crates/locron` with the descriptive User-Agent required by the data-access policy; prints `N/A (not published)` while unpublished and switches to the downloads endpoint automatically when the crate exists. The `/downloads` endpoint is a trailing-90-day series, so the published value is labeled as such; all-time totals live on the crate endpoint (recorded in `docs/FINDINGS.md` §13).
5. **GitHub traffic** — views and clones (14-day totals and uniques) via `gh api`; printed only when `gh` is present and authenticated, otherwise a one-line note explains how to enable it.
6. **Rate-limit awareness** — when the unauthenticated REST quota is exhausted, the GitHub sections print the limit message with retry guidance (`GITHUB_TOKEN` or `gh auth login`) instead of raw API errors.

`--json` emits the same snapshot as one flat JSON object (traffic keys present only when authenticated) for future automation. A per-section failure marks that section and lets the remaining sections print; the exit code reflects whether any section failed. The script parses JSON with only portable `sh` tooling (`grep`/`sed`/`awk`) — `jq` must not be a runtime requirement. Every heredoc is quoted; the repository incident memory requires it.

### Edge cases to handle explicitly

- Unauthenticated rate limit exhausted: GitHub sections degrade with actionable guidance, and the brew/crates.io sections still print.
- Tap formula with zero recorded installs: no analytics entry — render 0, never an error.
- Release list longer than one page: follow the `Link` header with a sane page cap.
- `gh` installed but unauthenticated or without owner access: traffic section omitted with a note.
- Network failure mid-run: later sections still print; non-zero exit.

### Verification additions

- **Static checks:** `sh -n` and shellcheck clean; the script contains no bashisms; all heredocs quoted.
- **CI smoke:** the existing `installer` job in `.github/workflows/ci.yml` gains a step that runs the script in `--json` mode against the live APIs (the authenticated `GITHUB_TOKEN` keeps the REST quota off the 60/hour limit) and asserts the JSON parses and each numeric field is a non-negative integer. The owner-only `/traffic/*` endpoints reject the Actions `GITHUB_TOKEN`, so the step additionally permits an exit confined to `traffic_error` while still failing on any other `*_error` key.
- **Local live check:** a real run prints all sections with numbers matching independently computed `jq` totals for the same day; the brew section renders 0 and crates.io renders `N/A` until their first real values.

## Export selection and URL import implementation (2026-08-24)

This section plans the 2026-08-24 `docs/SPEC.md` amendment: export job selection (interactive default on a TTY, deterministic filters, non-interactive full export) and import from a URL. Evidence and rejected alternatives are recorded in `docs/FINDINGS.md` §15. The change is confined to `locron`; `locron-core` and `locron-store` are unchanged, and the frozen dashboard spec's whole-document export download/import upload is unaffected (selection or URL support there would be its own dashboard spec change).

### Accepted: selection as a filter over the existing export path

Selection never reaches the store or domain crates. `locron` resolves the export subset from the same `list_jobs(true)` result the existing `export` function already reads, then hands the filtered list to the existing `export_job` mapping; the document shape, redaction, and omission accounting are untouched.

`--jobs NAME[,NAME...]` and `--tag TAG[,TAG...]` take exact names/tags, combine as a union, and deduplicate by job ID. Any selector value matching no job is a validation error before any output is produced (exit category 2), so a typo can never silently produce a smaller backup. Filters are valid with both human and JSON output and always suppress the picker. A zero-job state skips the picker entirely because there is nothing to select (export of settings only remains legal, as today).

### Accepted: interactive default on a TTY, deterministic everywhere else

Interactivity is decided once per invocation, before any output: stdin, stdout, and stderr must all be terminals (`std::io::IsTerminal`), the `CI` environment variable must be absent, output format must be human, and no `--jobs`/`--tag` selector may be present. Only that combination shows the picker; every other context exports the complete job set exactly as the current CLI does, so scripts, pipes, redirections, CI, and JSON consumers see no behavior change. This is the gh/OpenSpec/diagramkit convention from `docs/FINDINGS.md` §15, and the gh-gist piped-prompt bug is the counterexample this design rules out: non-TTY can never prompt.

stderr joins the decision because the picker renders there: dialoguer's stderr terminal refuses to render on a redirected stderr (`NotConnected`), so a TTY stdin/stdout with a redirected stderr must fall back to the deterministic full export rather than fail the command. This mirrors the both-terminals rule's intent: if the selection interface cannot render, the invocation is non-interactive.

The picker is a dialoguer 0.12 `MultiSelect` (MIT, rust-version 1.66, `default-features = false` — `editor`/`password` are unneeded) with the term target set to stderr, listing jobs by name with each item's schedule summary, every item initially selected, and Enter confirming. Rendering on stderr keeps the "human stdout is the bare export document" contract (`docs/CLI.md`) intact even while a picker is visible; the picker never writes to stdout. The picker interaction is wrapped behind a small selection port so contract tests drive a deterministic fake without a PTY.

For contract tests, a scripted picker substitutes for the TUI without a PTY: when `LOCRON_TEST_EXPORT_PICKER` (test-only hook, documented in the test file) is set to a comma-separated job name list, export treats the invocation as having three terminals, and the selection port returns exactly those job names (rendering its prompt line on stderr) instead of running dialoguer. The `CI`, format, and selector terms of the decision still apply, so the hook cannot make a scripted or JSON export interactive; it only replaces the TUI inside a context that already qualifies.

### Accepted: URL import reuses the whole-document import path

`locron import` accepts an absolute `http://` or `https://` URL in addition to a path. URL detection is an explicit scheme check (an `scheme://`-shaped input is parsed as a URL; anything else is a path — never a `Path::exists` guess). The CLI fetches the body with the existing reqwest/rustls client configuration (mandatory TLS verification), with a 16 MiB in-memory cap enforced while streaming, a 10-redirect cap, and a 30-second total timeout; URLs with a userinfo component are rejected at parse time as a validation error (exit category 2, like any bad argument — the category-5 set below covers what happens after the CLI commits to fetching). The fetched bytes then enter the existing `parse_import_document` → validate → plan → one-transaction apply path byte-for-byte unchanged, so redaction rejection, plaintext acknowledgement, deterministic resolution, dry-run, and rollback are identical for both sources. Fetch failures (DNS, TLS, timeout, cap, redirect excess, non-2xx, non-HTTP scheme) map to exit category 5 with an actionable message and retry guidance; document validation failures keep their existing categories. Import never prompts — `--dry-run` is the preview, and the post-import summary already reports create/update/no-op actions.

The trust boundary is documented, not coded around: `docs/CLI.md` and `docs/OPERATOR.md` state that an export document registers executable schedules and importing from a URL carries the same trust boundary as installing a script from that URL, with `--dry-run` recommended for first-time imports. No signature, pinning, or checksum scheme is added in this amendment; the existing redaction rules remain the value-protection mechanism.

### Edge cases to handle explicitly

- stdout is a TTY but stdin is not (input redirected): no picker — full export, matching the three-terminals rule.
- stdin and stdout are TTYs but stderr is redirected (`locron export 2>file`): no picker — the interface cannot render on a non-terminal stderr, and dialoguer would fail the command; the invocation falls back to the deterministic full export.
- `CI` is set while running in a real terminal (wrappers, `script -qec`): no picker — the environment marker wins, per the OpenSpec chain.
- Picker shown while the daemon edits jobs concurrently: selection resolves against the same `list_jobs` snapshot used for the document; a concurrently deleted job simply exports its last-read definition, and a concurrently created job appears in the next export.
- `--jobs` with a duplicate name and `--tag` with overlapping matches: union by job ID, one document entry per job.
- Zero registered jobs: no picker; settings-only export remains valid.
- URL import of a document with omitted values without `--accept-plaintext-values`: rejected exactly like a file import (existing rule).
- Fetch succeeds but the body is not valid UTF-8 JSON or exceeds 16 MiB: stable validation/protocol error before any write.
- URL import while the destination store is busy or migration-locked: existing exit category 4 behavior unchanged.
- Human mode with a URL: the bare document/plan renders on stdout exactly as for a file; fetch diagnostics and warnings go to stderr.

### Verification additions

- **Selection contract tests:** `--jobs`/`--tag` union and dedup, no-match validation error before output, JSON mode with and without selectors, redaction parity between a selected export and a full export of the same jobs, and round trips (`export --jobs` → `import`) reproducing exactly the selected jobs.
- **Interactivity tests:** the selection port is driven by a deterministic fake; the interactivity decision is a pure function tested across the TTY/CI/format/selector matrix; contract tests (via the `LOCRON_TEST_EXPORT_PICKER` hook, which drives the picker branch without a PTY) assert stdout carries only the document while the picker prompt renders on stderr, that an empty selection yields a settings-only export, that `CI` still wins over the hook, and that JSON mode never instantiates the picker.
- **URL import fixture tests:** a local HTTP fixture serves valid documents, redacted documents, malformed JSON, oversized bodies, redirect chains, and 404/500 responses; assertions cover successful atomic import, dry-run non-mutation, category-5 fetch failures, rollback on a late destination conflict, and identical behavior versus the same document as a file.
- **Platform matrix:** the existing four-target CI runs the new suites; no platform-specific code is introduced (TTY detection and stderr rendering are portable via dialoguer and `std::io::IsTerminal`).

## Shutdown-drain test determinism and CI lint consolidation (2026-08-24)

No product-behavior change — a test-harness script and the CI workflow only — so the frozen `docs/SPEC.md` is not amended. Failure evidence (run IDs, local reproduction) is recorded in `docs/TODO.md` "Shutdown-drain test determinism and CI lint consolidation backlog".

### Accepted: single-member process group makes drain-cancel confirmation event-driven

`daemon::tests::elapsed_shutdown_drain_cancels_runner_before_lifetime_end` failed on two macOS CI legs — run 32644652482 (`macos-aarch64` / Rust 1.94.0) and run 32644735243 (`macos-x86_64` / Rust stable), different platforms each time — with the outcome assertion receiving `TerminationUnconfirmed` instead of `Cancelled`.

Root cause: the test script's `while :; do sleep 1; done` puts a second process (`sleep`) in the run's process group. The runner confirms termination only when the owned leader wait handle has resolved and `kill(-pgid, 0)` reports the whole group absent (runner `wait` branch, `runner.rs:315`; group probe `observe_group_absence`, `runner.rs:767`). When the trapped `sh` exits, its orphaned `sleep` sibling is reaped by launchd/init on its own schedule; until then it remains a zombie that the group-liveness probe counts as alive, so the test's two 20 ms grace deadlines (TERM → KILL → confirm, `runner.rs:351`) can elapse before group absence becomes observable on a loaded macOS runner. The production path is unaffected in practice — its default graces are 5 s + 5 s, which the reap latency cannot plausibly exceed — so the flake is an artifact of the test shrinking the grace to 20 ms.

The fix makes confirmation event-driven rather than budget-driven. Replace the script with a pure-builtin loop (`while :; do :; done`) so the group has exactly one member: when the runner reaps the leader, group absence is true in the same poll, `termination_confirmed` is set without any deadline firing, and the outcome is `Cancelled` regardless of machine load, parallel test contention, or scheduler latency. The test also raises `termination_grace` from 20 ms to 1 s as belt-and-braces, so even a pathological TERM-to-trap delay cannot reach the KILL stage before the trap writes its marker file; the grace then gates only the escalation step, not the asserted outcome. `shutdown_drain` stays at 10 ms: it only decides when the daemon issues cancellation, and the attempt tracker cannot complete before that cancellation, so the drain always elapses. The trap still fires promptly in a builtin loop (the shell checks for pending traps between commands), and the loop burns one CPU core only from `ready` to the trap — tens of milliseconds, confined to one unit test.

Rejected alternatives:

- `serial_test` or `--test-threads=1`: reduces the CPU contention that amplifies the race, but keeps the fixed two-deadline budget against launchd reap latency, so the test would still flake on a loaded runner; the workspace has no other timing-sensitive test needing global serialization.
- Lengthening the graces alone: same residual race, just rarer — a fixed budget stays load-dependent.
- Redefining production confirmation as leader-reap-only (dropping the group-absence requirement): changes behavior the frozen documents record (SPEC: "Completion is not reported until termination is confirmed"; IMPLEMENTATION: "Leader exit alone is insufficient because an in-group descendant may still be running") to fix a test artifact. Not accepted.
- `cargo nextest`: its parallel runner and retry support are marginal here — the engine test binary finishes in under a second — and a retry wrapper would mask rather than remove the race.

### Accepted: dedicated lint job over OS × toolchain, tests over the full platform matrix

`cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings` run in all eight matrix legs of `.github/workflows/ci.yml`. Clippy findings are platform-sensitive only through OS-gated code (this repository's history contains two backlogs where macOS-only or Linux-only dead code failed the opposite OS legs), and the workspace contains no architecture-gated code, so the architecture duplication among the eight clippy compile passes carries no signal.

The workflow gains a `lint` job — matrix `linux-x86_64` and `macos-aarch64` × Rust 1.94.0 and stable, `fail-fast: false` — running fmt and clippy, and the `test` job's eight legs keep only `cargo test --workspace --all-targets`. This preserves exactly the coverage that has caught real bugs here (OS-gated dead code on both OSes, both toolchains' lint sets — formatting and lint rules have drifted between Rust versions before) while halving clippy compile work. rust-cache stays in the lint job so its clippy artifacts warm across runs.

### Edge cases to handle explicitly

- A toolchain update changes rustfmt or clippy output between Rust 1.94 and stable: both toolchains remain in the lint matrix because the CI contract requires clean results on both.
- macOS-only or Linux-only items that are dead code: still caught, because both operating systems remain in the lint matrix.
- The busy-loop script's CPU burn: bounded by the test's own cancellation path and confined to one unit test.
- The `termination_grace` change must not alter what the test proves: it still asserts `Cancelled` via the drain-elapsed → daemon-cancel → TERM-trap path, now with the outcome independent of the deadline values.

### Verification additions

- The changed test passes 100 consecutive local runs plus the macOS CI legs, and the sibling `shutdown_drain_allows_natural_completion_before_lifetime_end` test remains untouched and green.
- The `lint` job passes on both OS legs and both toolchains; the eight `test` job legs pass with `cargo test` only.

## Usage snapshot smoke relocation (2026-08-24)

No product-behavior change — CI placement only — so the frozen `docs/SPEC.md` is not amended. Evidence and rejected alternatives are recorded in `docs/FINDINGS.md` §18.

### Accepted: scheduled smoke workflow, hermetic push CI

CI run 32654895285 failed in `Installer / ubuntu-latest` at the "Usage snapshot smoke (live APIs)" step: `scripts/usage.sh --json` exited non-zero with a non-`traffic_error` key. The step never passed `GITHUB_TOKEN` through `env:`, so the script's GitHub REST calls ran unauthenticated against the shared-IP 60/hour quota even though the script supports the token (`usage.sh` line 80). The step's tolerance predicate (`has("traffic_error") and [all *_error keys == "traffic_error"]`) then rejected the failure, as designed.

New `.github/workflows/usage.yml`: `on: schedule: [cron: '0 3 * * 1']` (weekly, Monday 03:00 UTC) plus `workflow_dispatch`; one `usage` job on ubuntu-latest: checkout, run `sh scripts/usage.sh --json` with `env: GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}` and `permissions: contents: read`, then assert the JSON parses and every numeric field is a non-negative integer. The traffic-only tolerance from the removed step is carried over, for a reason the first plan draft missed: passing `GITHUB_TOKEN` also authenticates the preinstalled `gh`, and the owner-only `/traffic/*` endpoints then fail by design (the Actions token lacks push/admin access), so `traffic_error` is the *expected* state of an authenticated scheduled run, not a drift signal — the first `workflow_dispatch` (run 32655962445) failed on exactly this. Any other `*_error` key or an invalid snapshot still fails the run as a maintainer drift alert. No artifacts are published; the run log is the record.

`ci.yml`'s installer job loses the live-smoke step; the hermetic steps stay: shellcheck on `install.sh` and `scripts/usage.sh`, the fake-`uname`/`ldd` refusal tests, and the pinned `v0.2.0` install smoke (release-asset download through the CDN-backed redirect — no REST quota consumption — consistently green across the matrix runs, e.g. 32644269125 and 32654818613).

### Edge cases to handle explicitly

- Scheduled workflows run only on the default branch and auto-disable after 60 days of repository inactivity — acceptable for a maintainer measurement tool; `workflow_dispatch` covers manual runs.
- `gh` is not authenticated in the scheduled run: the traffic section prints its note and the script still exits 0 when every other section succeeds (existing script behavior, unchanged).
- formulae.brew.sh and crates.io have their own limits and are not authenticated: their sections can still fail a scheduled run — that is the drift alert working as intended, never a push gate.

### Verification additions

- `ci.yml` parses and the installer job's step list no longer includes the live smoke; `rg -n "usage.sh --json" .github/workflows` shows the smoke only inside `usage.yml`.
- The scheduled workflow cannot fire from a push (GitHub schedules run on the default branch with real cron timing); a manual `workflow_dispatch` run is recorded as evidence at first trigger.
- The next push CI run is green; run ID recorded in `docs/TODO.md`.

## Process-group cancellation confirmation on macOS (2026-08-25)

No product-scope change: cancellation still reports `Cancelled` only after the owned leader has
been reaped and its process group has received termination. The frozen `docs/SPEC.md` is unchanged.

The macOS x86_64 stable CI failure in [job 97478078694](https://github.com/WhiteKiwi/locron/actions/runs/32741897719/job/97478078694)
was a deterministic gap in the runner's confirmation rule, exposed intermittently by process
reaping latency. The cancellation test creates a TERM-ignoring grandchild. After SIGKILL succeeds,
that child can remain a zombie in the original process group until launchd reaps it. POSIX
`kill(-pgid, 0)` reports that zombie as present, even though SIGKILL has made further execution
impossible. The runner therefore incorrectly emitted `TerminationUnconfirmed` after its second
grace deadline.

The runner will retain group-absence probing before escalation: a TERM outcome remains confirmed
only after both the direct child is reaped and the process group is absent. Once SIGKILL was either
delivered successfully or found the group already absent (`ESRCH`), and the direct child is reaped,
the runner will classify the cancellation or timeout as confirmed without waiting for an
unreapable-by-locron zombie to disappear. Any SIGKILL error other than `ESRCH`, or a missing direct
child reap, remains `TerminationUnconfirmed`.

This is stronger than increasing a timeout: it removes a host-controlled zombie-reaping race while
retaining the safety boundary for a live descendant that did not receive SIGKILL.

### Verification additions

- Unit-test the SIGKILL delivery predicate for success, `ESRCH`, and a permission error.
- Run the live grandchild-cancellation regression repeatedly on macOS where available, then run
  the complete workspace test and lint battery.
- Confirm the next macOS x86_64 stable CI run reports the cancellation test as passed without a
  job retry.

## Terminal-width list table truncation (2026-08-24)

This section plans the 2026-08-24 `docs/SPEC.md` amendment (Human Output Contract: Table width). Evidence and rejected alternatives are recorded in `docs/FINDINGS.md` §19. The change is confined to `locron`; `locron-core`, `locron-store`, and `locron-engine` are unchanged.

### Accepted: TTY-only truncation of the table's final column

Width resolution is `console::Term::stdout().size_checked()` — the `TIOCGWINSZ` ioctl that docker and kubectl use. `console` 0.16 is already in the `locron` dependency graph through dialoguer 0.12 (verified with `cargo tree -p locron -i console`: console 0.16.4 → dialoguer 0.12.0 → locron), so declaring it as a direct dependency with `default-features = false` adds zero lockfile entries. A failed size lookup — stdout redirected, piped, or otherwise not a terminal — means no truncation, so the one mechanism is both the width source and the TTY gate. The width is sampled once per invocation; a mid-print window resize is not chased (docker and kubectl behave the same). The tuple is `(rows, cols)` — verified in the console 0.16.4 source (`unix_term.rs:53–67` returns `(winsize.ws_row, winsize.ws_col)`), so the width is the second element; a PTY check at real widths confirms the truncation budget tracks the column count.

Display width uses `unicode-width` 0.2, already locked transitively, declared directly on `locron`. The pure helper `truncate_display(&str, max_width) -> String` walks characters, sums display width, and appends the `…` marker (display width 1) only when the value actually shrinks; a value that fits is returned unchanged.

`render_list_table` gains a `width: Option<u16>` parameter, resolved once in the `list` dispatch arm for human format only. Column padding is unchanged; fitting is a separate step: the natural table width is `name_width + 1 + schedule_width + 1 + target_width + 1 + 7` (the final `ENABLED` column is unpadded), and when it exceeds the terminal width only `TARGET` — the table's final data column — absorbs the deficit. Earlier columns never truncate: `NAME` is the key for every other command, schedule summaries are inherently short, and truncating a middle column would misalign every column after it. When the deficit leaves less than one display column for `TARGET` (a pathological terminal width), no truncation occurs and the table wraps exactly as it does today — documented, not silently cut data.

`--no-trunc` is a clap boolean on `List`. It is a rendering flag, not a data flag: it restores full `TARGET` values on a terminal and is accepted with no effect in machine mode, whose envelope stays byte-identical either way. `show` is unchanged — it already prints the complete definition. The `history` table is unchanged in this amendment; applying the same rule there is a deferred follow-up when a long `TRIGGER` value demonstrates the need.

Testability needs no PTY: `truncate_display` and `render_list_table` are pure functions whose width is an injected parameter, so unit tests call them directly with widths of 40, 80, and `None`. Contract tests keep asserting piped `list` output — assert_cmd pipes stdout, the size lookup fails, and the output must be byte-identical to today's full-value table; the help-surface walk covers the new flag.

### Edge cases to handle explicitly

- A terminal narrower than `NAME + SCHEDULE + ENABLED` alone: no truncation, rows wrap as today.
- CJK or emoji in a target: fitting uses display width, never byte or character count; a truncation may split a grapheme cluster (acceptable in a summary table — the full value lives in `show`).
- `--no-trunc` with piped stdout: a no-op, because pipes already print full values.
- `--no-trunc` with `--format json`: accepted and ignored; the envelope is unchanged.
- An empty job list: header only, unchanged.
- Window resized after invocation start: the sampled width stands for the invocation.

### Verification additions

- **Unit tests:** `truncate_display` — ASCII fit/no-fit and exact-boundary cases, width-2 CJK, emoji, ellipsis appended only when truncation occurs, and zero/minimum widths; `render_list_table` with injected widths covering the truncating, fitting, and too-narrow fallback paths.
- **Contract tests:** piped human `ls` with a long target is byte-identical to the pre-change table; `--no-trunc` appears in `locron ls --help` and is accepted; `ls --no-trunc --format json` output is identical to `ls --format json`.
- **Workspace battery:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` pass on the installed toolchain; the four-target CI matrix stays green.

## Terminal-width history table truncation (2026-08-25)

This section implements the 2026-08-25 `docs/SPEC.md` amendment by extending the accepted list-table
mechanism to the human `history` table. The change remains confined to `locron`; storage, ordering,
redaction, and machine-readable history data are unchanged.

`History` gains a `--no-trunc` rendering flag. In the human dispatch path, stdout width is sampled
once with the existing `console::Term::stdout().size_checked()` lookup unless the flag is present.
The same failed-lookup behavior remains the TTY gate, so redirected and piped output receives no
width and therefore retains every value. Machine mode accepts and ignores the flag, preserving the
same envelope and data.

Refactor the history renderer into a printing wrapper and a pure table builder with an injected
optional width, matching the list renderer's test seam. The natural table width includes the five
maximum display-width columns and the four literal ` | ` separators. If it exceeds the sampled
terminal width, only `TRIGGER` receives the remaining budget after reserving the natural widths of
`TIME`, `JOB`, `STATE`, and `DURATION` plus separators. A budget of at least one display column uses
the existing Unicode display-width truncation helper and its trailing `…`; a smaller budget falls
back to the full table so no preserved column or separator is destroyed. Column padding is based on
display width so a wide trigger remains aligned and fits the same budget. Empty history remains the
unchanged header-only table.

Verification covers pure history-table rendering at fitting, truncating, absent-width, too-narrow,
and Unicode widths; CLI contracts for help, piped full output, and machine-output equivalence with
`--no-trunc`; formatting; warnings-denied all-target workspace Clippy; all workspace targets; and
`git diff --check`.

## Dashboard v0.8.0 release preparation (2026-08-25)

This section implements the corresponding `docs/SPEC.md` amendment without changing dashboard
runtime behavior. The feature implementation and its integrated browser QA are already complete;
this pass makes the shipped surface discoverable, gives it one release identity, and prepares the
reviewed branch for the repository publication workflow.

### Documentation and release-note structure

The README will introduce the dashboard immediately after installation and before the broader CLI
quick start. The shortest supported flow is foreground-first: start `locron dashboard`, open the
printed loopback URL, and paste the printed token into the entry page. A compact persistent-service
alternative will use `locron dashboard enable`, with `status` for the stable local URL and `token`
for intentional token re-display. The copy must say that the dashboard is optional, disabled by
default, loopback-only, and backed by the same durable jobs, runs, settings, and diagnostics as the
CLI. It must not place a token in a URL, imply remote access, or imply that the dashboard replaces or
owns the scheduler daemon.

The existing Operator and CLI references remain the detailed source of truth. Their dashboard
sections will be checked for stale paths or statements; only user-facing inconsistencies found in
that review will change. The README documentation index will point to the operational guide as well
as the design/specification material so a new user can move from quick start to lifecycle and
security detail.

`CHANGELOG.md` will be normalized into chronological Keep a Changelog order, because its current
header and Unreleased section sit below several released entries after an earlier merge. A curated
`0.8.0` entry will summarize the complete user-visible dashboard: lifecycle commands, loopback and
token boundary, authenticated management and diagnostics, responsive light/dark interface,
human-friendly schedule/size controls, debounced partial search, row navigation, and redacted
pretty JSON. Test-only and internal implementation details stay out of the entry. The comparison
links will advance from `v0.7.0` to `v0.8.0`, while Unreleased remains empty and ready for the next
change.

### Version synchronization

The workspace package version changes from `0.7.0` to `0.8.0`, a backward-compatible feature
release. Member manifests inherit the workspace version, so `Cargo.toml` is the only hand-edited
manifest. `cargo check --workspace` will perform Cargo's normal lockfile reconciliation; the five
workspace package records in `Cargo.lock` must all report `0.8.0`, with dependency versions
otherwise unchanged.

### Publication boundary and maintained skill

This repository preparation does not open or merge a pull request, create a tag, publish a GitHub
release, update Homebrew, or modify the separate `WhiteKiwi/skills` repository. Those external
mutations belong to the parent publication session after review. The parent will compare the
released dashboard command surface against the Locron skill before publication; if it changes the
skill, that repository's generated packages and validation must be synchronized independently.

### Verification additions

- Validate every relative README Markdown link and every shell fence used by the quick start; scan
  the new dashboard copy for token-in-URL examples and contradictory remote/off-by-default claims.
- Confirm `CHANGELOG.md` has one heading, strictly descending released versions, a `0.8.0` entry,
  and comparison links based on `v0.8.0` and `v0.7.0`.
- Run `cargo check --workspace --locked` after normal lockfile generation, and assert the workspace
  manifest plus all five Locron lockfile package records report `0.8.0`.
- Run `cargo fmt --all --check`, `git diff --check`, and inspect staged and unstaged changes so the
  release-preparation commit contains only the specification amendment, planning documents,
  user-facing documentation, changelog, and version metadata.

## Linux service cfg portability follow-up (2026-08-25)

This is a compilation-portability correction for the already-specified daemon and dashboard
service behavior. No root or dashboard specification changes: systemd receives the same target
service names, and the real-backend tests keep the same platform ownership.

The systemd `ServicePort` methods that select a daemon or dashboard unit must rename their existing
`_ctx` binding to `ctx`. There is no data-flow or command change; the correction merely makes the
identifier used in each existing `systemctl` argument available in the Linux compilation branch.
The adjacent context-independent methods retain `_ctx`, making the unused/used distinction
compiler-enforced.

The macOS-only dashboard cleanup type, its `Drop` implementation, and the default dashboard token
path helper will receive `#[cfg(target_os = "macos")]`. Scoping both the type and implementation is
necessary: scoping only the construction leaves the definition dead on Linux, while scoping only
the type leaves an invalid implementation. The common daemon cleanup remains available on Linux.

Verification proceeds from the narrowest platform seam outward:

1. Compile the systemd module on Linux and in unit-test builds, and add a no-command unit test that
   boxes `SystemdPort` as a `dyn ServicePort` trait object. A source contract additionally confirms
   only truly unused arguments retain `_ctx` and that the dashboard cleanup type, implementation,
   and token helper are under macOS cfg.
2. `cargo fmt --all --check`, warnings-denied workspace all-target Clippy, and the complete
   workspace all-target test suite run on the available macOS toolchain to detect cross-platform
   fallout.
3. Attempt the installed Linux Rust target check, record any missing cross-C-toolchain boundary, and
   require the parent-owned native CI rerun as the final Linux compile proof.

The follow-up commit contains only this implementation correction and its FINDINGS,
IMPLEMENTATION, and TODO evidence. The parent session owns push and PR workflow actions.

## Service template identity follow-up (2026-08-25)

No service behavior changes. This follow-up separates two concepts that were equivalent only on a
macOS host: the active platform manager's service identity and the launchd identity embedded in a
plist template.

Add `Target::launchd_label()` under `cfg(any(target_os = "macos", test))`, mapping daemon and
dashboard directly to `DAEMON_LABEL` and `DASHBOARD_LABEL`. The macOS branch of
`Target::service_name()` delegates to this accessor, retaining one launchd label source of truth.
Linux `service_name()` continues returning `DAEMON_UNIT`/`DASHBOARD_UNIT`, so all systemd manager
commands, unit paths, CLI output, and API output are unchanged.

`render_plist` uses `launchd_label()` instead of `service_name()`. That makes the template output a
property of the requested format, not of the host executing its test. Existing daemon/dashboard
plist assertions remain strict and become portable; the launchd constants become genuinely used in
Linux test builds. `render_unit` stays unchanged because it embeds no service-manager name.

Verification requires the two focused plist tests and the systemd compile seam on the local host,
a source contract proving `render_plist` uses `launchd_label` while unit-path/manager calls retain
`service_name`, warnings-denied all-target Clippy, full workspace all-target tests, fmt, and diff
checks. Native Linux remains parent-owned CI confirmation after publication of the scoped fix.

## Dashboard fixed-port test serialization follow-up (2026-08-25)

Runtime binding behavior remains unchanged: partial IPv4/IPv6 success is valid, foreground falls
back only when no configured family can bind the candidate port, and fixed mode errors under the
same all-family conflict condition.

Add a test-only `serialized_default_port()` helper in the dashboard CLI integration suite. A
process-static `Mutex<()>` returns a poison-tolerant guard so a failed test does not prevent later
cleanup or diagnostics. Acquire that guard before `hold_fixed(DEFAULT_PORT)` in exactly the three
tests that exercise default-port fixed/fallback behavior, and retain it through child cleanup by
ordinary lexical lifetime. Random explicit-port tests do not share the resource and stay parallel.

The existing `hold_fixed` dual-family helper and all assertions remain unchanged. In a clean CI
environment, each serialized test owns both loopback listeners. If an external process owns the
default port, the existing helper can still observe that stable conflict; the mutex specifically
eliminates unowned conflicts created by another test in this process.

Verification includes a source inventory proving every `hold_fixed(DEFAULT_PORT)` call acquires the
guard, repeated high-parallelism runs of the complete dashboard integration binary, focused fixed,
redirected, and PTY fallback tests, and the full fmt/warnings-denied Clippy/workspace all-target
battery. The review server stays running; native matrix confirmation remains parent-owned.
## crates.io source installation and trusted publication (2026-08-25)

The accepted distribution design follows the evidence in `docs/FINDINGS.md` §34. crates.io is a
secondary source-build channel for Rust users; the standalone installer and Homebrew remain the
general-user paths. The user-facing Cargo package is named `locron`, retains the single `locron`
binary target, and is published together with the four library packages required by its normal
dependency graph.

### Package graph and metadata

Rename the `locron` Cargo package to `locron` without moving its source directory or creating a
second binary. Centralize each internal dependency in `[workspace.dependencies]` as a local `path`
plus an exact registry version equal to the lockstep workspace release. Members inherit those
declarations so local builds use the workspace source while packaged manifests resolve the exact
crates.io release. Cargo's native workspace publisher then owns the dependency order:
`locron-core`; `locron-store` and `locron-engine`; `locron-server`; then `locron`.

All five packages inherit the repository, root README, authors, dual license, Rust version,
crates.io-only publication restriction, keywords, and command-line category. Package descriptions
remain role-specific. The internal libraries are published implementation packages required by the
binary, not a promise of an independently stable public API. Release version updates must change
the workspace version and every exact internal version in one reviewed commit.

### Installation ownership

Binary ownership and service ownership stay separate. `install.sh` writes an atomic owner-only
receipt named `.locron-install-receipt-v1` beside the installed executable after a verified atomic
replacement. Its exact two-line payload is `locron.install/v1` then `standalone`; the deterministic
sibling location binds it to the canonical executable directory, and moving the binary without the
receipt intentionally drops self-update authority. `locron self-update` accepts only a regular,
non-symlink receipt with that exact payload before downloading or replacing anything. An absent or
malformed receipt refuses with stable machine output and channel guidance. Cargo users receive
`cargo install --locked locron`; receipt-less older script users are told to rerun the standalone
installer once; other installations are told to use their installation channel. The existing
Homebrew marker remains the stronger Homebrew-specific message.

Cargo does not own launchd or systemd registration, so its receipt-less binary may still run
`locron service install` and `locron dashboard enable`. The Homebrew marker continues to block
those mutations because Homebrew does own their service lifecycle. Manual tarball/source and
deb/rpm installs likewise keep the existing service-registration behavior even though built-in
self-update refuses them.

### CI and release flow

Push/PR CI runs `cargo publish --workspace --dry-run --locked` on Rust 1.94 after the ordinary
workspace gate and inspects the package set, normalized manifests, bundled README/licenses, and
absence of repository-only or secret material. No check uses `--allow-dirty` or `--no-verify`.

The tag workflow adds a dedicated `publish-crates` job after the complete binary build matrix and
before GitHub Release/Homebrew publication. Only that job receives `contents: read` and
`id-token: write`, is bound to the protected `crates-io` environment, exchanges GitHub OIDC through
the official `rust-lang/crates-io-auth-action@v1`, and passes the short-lived token as
`CARGO_REGISTRY_TOKEN` to `cargo publish --workspace --locked`. The GitHub Release job alone keeps
`contents: write`.

The job first proves tag, workspace, lockfile, binary, and changelog version agreement, then queries
all five exact package versions with a descriptive user agent. None present permits publication;
all present is an idempotent rerun/bootstrap case and skips upload; a partial set fails with an
inventory and explicit recovery guidance. Workspace publication is not atomic, so downstream
publication never runs after a partial failure. After all versions become visible, install the
exact `locron` version into a temporary Cargo root with `--locked`, verify its version and
self-update refusal, and run non-mutating service/dashboard status checks.

crates.io requires one manual first publication per new package before trusted publishers can be
configured. The release guide therefore defines a one-time bootstrap from the exact clean release
commit using a newly created narrow API token, immediate token revocation, trusted-publisher
bindings for all five packages to `WhiteKiwi/locron` + `release.yml` + `crates-io`, and then the
ordinary immutable tag. The tag job observes all versions already present and performs the same
registry-install verification without trying to overwrite them. Later tags use OIDC only.

### Documentation and TODO compaction

README and installation/release documentation list the prebuilt installer and Homebrew before
`cargo install --locked locron`, explain the Rust 1.94 source-build requirement, distinguish Cargo
update/removal from Locron service registration, and document the one-time trusted-publisher
bootstrap and partial-publication recovery.

Keep `docs/TODO.md` as the live checklist: move fully completed top-level sections verbatim to
`docs/TODO-archive.md`; for mixed sections, archive the completed evidence and retain only the open
follow-up with enough context and its verification method. Verify the apparently stale unchecked
terminal-width planning item against the existing SPEC/CLI/IMPLEMENTATION/FINDINGS records before
marking and archiving it. Do not move any genuinely open checkbox, and preserve the current
`docs/BACKLOG.md` distinction between inactive ideas and committed work.

### Verification strategy

Before handoff, run formatting, warnings-denied workspace Clippy, all workspace targets, dependency
direction, shell syntax and shellcheck for changed scripts, workflow YAML/action lint, workspace
package and publish dry-runs on Rust 1.94, per-package file/archive inspection, exact-version Cargo
installation into a temporary root where possible, self-update ownership tests, CLI help/contract
tests, Markdown link/reference checks, and `git diff --check`. No real crates.io upload is part of
implementation verification.

### Source-package archive inspection follow-up (2026-08-25)

The first hosted source-package run proved that piping `tar -tf` directly into `grep -q` is unsafe
under the runner's `bash -o pipefail`: once `grep` finds the expected member and exits, `tar` can
receive a broken pipe and make the successful inspection fail. Archive listings are already
materialized into one file per package for the other checks, so the server asset assertion must
read that saved listing as well. This keeps the check exact, avoids suppressing genuine `tar`
failures, and makes local and hosted behavior deterministic. Verification reruns the failed hosted
job on the corrective commit and requires the source-package job plus the complete CI workflow to
pass.

## Dashboard lifecycle human output and stale detail recovery (2026-08-25)

The CLI output audit follows command dispatch rather than only searching the shared renderer. The
ordinary scheduler commands already select a command-specific human renderer before calling the
machine envelope renderer. The remaining accidental pretty-JSON paths are the three daemon service
commands, the four non-foreground dashboard lifecycle commands, and successful `self-update`.
Human `export` remains intentionally serialized because its stdout is the portable export document,
not a command result report.

Remove the shared renderer's pretty-JSON human fallback after the audit. Reaching it in human mode
is an internal contract violation, which prevents a later command from silently reintroducing this
class of omission; intentional serialized surfaces such as export continue to render explicitly.

Keep the existing JSON data shapes and error envelopes unchanged. In human mode, service and
dashboard lifecycle commands instead print a stable labeled report. Installation reports service
identity and registration/restart/defer facts; removal reports service identity and stop/removal
facts; status reports registration, running/enabled/session state and optional manager facts.
Dashboard status extends that report with the access URL and token presence/permission posture,
while dashboard disable reports token removal. `dashboard token` prints the access URL and an
`Access token:` label followed by the unmodified token on its own line so it remains immediately
copyable. Existing actionable guidance remains on stderr and no command other than token or the
first foreground startup line reveals the token value. Successful human self-update reports the
old and new versions and whether replacement occurred.

The frontend keeps successful-login deep links intact. Job and run detail loaders distinguish a
404 response from loading and from other request failures. A missing detail route renders its own
route header and card that name the resource category, explain that the durable resource may have
been removed or the link may be stale, and provide a direct collection link (`#/jobs` or `#/runs`).
The raw missing identifier is not used as the page-level explanation. Valid detail responses keep
the existing detail UI, and non-404 failures keep their ordinary request feedback so operational
errors are not mislabeled as absent data.

Implementation and verification order:

1. Add focused CLI contract assertions for all lifecycle human forms and successful self-update
   while retaining their existing JSON assertions.
2. Add job and run detail tests for 404, non-404, and valid response branches, then implement the
   explicit missing-resource states without changing hash routing.
3. Run the complete service/dashboard/self-update integration suites and frontend test/build
   checks, followed by workspace formatting, warnings-denied Clippy, relevant workspace tests, and
   diff checks.

The plan review confirms that no state, API, routing, authentication, token storage, or machine
schema change is required. The changes are limited to presentation selection and explicit 404
recovery, so the durable architecture remains unchanged.
