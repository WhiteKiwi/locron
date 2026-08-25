# locron Architecture

## Status and document map

This document is the durable architecture contract for locron milestone 1. The repository now
contains a partial implementation of this contract; `docs/STATUS.md` records verified behavior and
remaining gaps without weakening these invariants.

- `docs/SPEC.md` is the frozen authority for product behavior and scope.
- `docs/ARCHITECTURE.md` owns durable system boundaries, component responsibilities, dependency direction, runtime topology, and persistence invariants.
- `docs/IMPLEMENTATION.md` owns milestone-specific implementation choices, trade-offs, edge cases, change order, and verification strategy.
- `docs/CLI.md` owns the reviewed command, diagnostic, machine-output, and exit-code contract.
- `docs/STORAGE.md` owns the reviewed state layout, logical schema, migrations, and output protocol.
- `docs/TODO.md` tracks progress and evidence.
- `docs/FINDINGS.md` preserves research evidence; it does not override the frozen specification.

Future durable architectural decisions that need their own rationale and supersession history belong under `docs/decisions/`. Do not create an ADR until there is an actual reviewed decision to record.

## System boundary

locron is one per-user, per-machine scheduler on macOS and Linux. It owns job schedules, durable occurrences, execution, recovery, history, and retention. It does not translate jobs into cron, launchd, or systemd entries. An operating-system service manager may keep the locron daemon running, but it is not a job-level scheduling backend.

Milestone 1 has no distributed worker, multi-user authority layer, workflow graph, desktop application, or service-supervision role. The MCP server ships inside `locron-cli` (`locron mcp`) over the application boundary. The HTTP management and viewer surface (`locron-server`, roadmap phase 1, `docs/dashboard/SPEC.md`) is a separate library crate that must reuse the same application and engine boundaries rather than bypassing them to manipulate durable tables, and it never owns the daemon: it holds no scheduler lifetime, runs no scheduling loop, and supervises no targets. Any future surface follows the same rule.

## Supported foundation

The accepted implementation foundation is:

- Rust edition 2024 with Cargo resolver 3.
- MSRV Rust 1.94, continuously checked alongside latest stable Rust.
- macOS 14 or newer on `aarch64` and `x86_64`.
- Linux kernel 5.14 or newer with glibc 2.34 or newer on `aarch64` and `x86_64`.

Windows, 32-bit targets, and official musl/Alpine support are deferred. Unsupported builds may happen to work, but they have no v1 compatibility or release-verification promise.

## Workspace and dependency direction

The virtual Cargo workspace has five crates and one distributable binary. Milestone 1 shipped the first four; `locron-server` is the roadmap-phase-1 web administration surface (`docs/dashboard/SPEC.md`).

| Crate | Durable responsibility | Forbidden coupling |
|---|---|---|
| `locron-core` | Domain identities and values, schedules and policies, validation, state transitions, application commands/results, persistence/clock/executor ports, and the shared redaction boundary over serialized documents | SQLite, CLI parsing/rendering, HTTP handling, operating-system service setup |
| `locron-store` | SQLite connections and migrations, repositories, transactions, durable uniqueness, lifetime and retention records, and implementations of core persistence ports | CLI presentation, process spawning, HTTP execution |
| `locron-engine` | Complete daemon runtime: lifetime/lock ownership, reconciliation loop, overlap/concurrency admission, retry timing, process/shell/HTTP runners, cancellation and recovery, maintenance, signals, and graceful shutdown | SQLite implementation details and CLI presentation |
| `locron-server` | The loopback HTTP management and viewer surface: middleware security stack (Host/Origin/CSRF/token), `locron.api/v1` envelope, route handlers over the store's durable application commands, SSE live output, and the embedded single-page viewer | CLI parsing/rendering, daemon ownership, any scheduler loop or runner lifecycle, SQLite table access outside the store boundary |
| `locron-cli` | Thin composition and command entrypoint for the `locron` binary: parsing, human/machine rendering, bootstrap/configuration, wiring store to engine, `locron daemon run`, and the `locron dashboard` family | Reimplemented domain policy, scheduler loops, runner lifecycle, or daemon behavior |

```text
                          locron-cli
            /            /          \            \
           v            v            v            v
   locron-server  locron-engine   locron-store  locron-core
         |  \            \            /            /
         |   \            v          v            /
         |    \         locron-core               /
         |     \_________________________________/
         v
   locron-core   (locron-store depends on locron-core only)
```

- `locron-core` has no dependency on another workspace crate.
- `locron-store` and `locron-engine` each depend on `locron-core`.
- `locron-engine` receives persistence through core ports and does not depend on `locron-store`.
- `locron-server` depends on `locron-core` and `locron-store` only; it never depends on `locron-cli` and never owns daemon behavior.
- `locron-cli` is the composition root and depends on the other four crates.
- No library depends on `locron-cli`.

There is no `locron-daemon` crate and no `locrond` binary in v1. `locron daemon run` constructs the dependencies and enters the daemon runtime owned by `locron-engine`. `locron dashboard` constructs the store and starts the `locron-server` library. Future desktop crates are added only when that milestone begins.

## Runtime topology and data flow

The single binary has a short-lived command role, one long-lived daemon command, and (when enabled) a separate long-lived dashboard server process. All of them use the same domain and durable state. The dashboard server is a process separate from the scheduler daemon: it reads and writes the same SQLite state through the store boundary, sends the same best-effort wake hint after durable mutations, works while the daemon is offline, and never acquires the daemon lock or a scheduler lifetime. Its restarts never affect scheduling, and daemon restarts do not stop it.

```text
single locron binary (locron-cli)
        |
        +-- short-lived command
        |       |
        |       v
        |   validate/normalize --> persist job revision or durable run request
        |
        +-- locron daemon run
                |
                v
        locron-engine daemon runtime
        lifetime/lock + signals + graceful shutdown
                |
clock/wake ---> reconciliation loop ---> durable queued runs
                |                              |
                v                              v
          overlap decision             concurrency admission
                                               |
                                               v
                                    durable attempt identity
                                               |
                                               v
                                 process/shell/HTTP runner
                                               |
                       output capture <--------+
                               |
                               v
                   result/retry/terminal state
                               |
                               v
                     bounded maintenance

Both roles --> locron-core ports <-- locron-store SQLite implementation
```

Job mutation does not require daemon restart. A committed revision or run request becomes visible to the engine on reconciliation. Manual submission receives a durable run identity even while no daemon is active. Wait/follow observes that same run and does not create another execution mode.

### Durable control and wakeup

SQLite is the sole correctness path for job mutations, manual runs, cancellation intent, and global configuration changes. A client commits the application command first and only then sends a best-effort wake notification. No durable command or policy value exists only in an IPC message.

The daemon binds an owner-only Unix datagram socket after acquiring scheduler ownership. Datagram content is only a versioned wake hint: the daemon distrusts it, coalesces notifications, and rereads durable state. Missing, full, stale, or path-length-incompatible sockets affect latency but never correctness. The ownership holder alone may replace a stale socket artifact.

The engine waits for the earliest calculated schedule/retry deadline, wake notification, process signal, or a 30-second safety reconciliation. Every wake rereads wall time and durable cursors. This permits prompt ordinary updates while still recovering from socket loss, suspend/resume, and clock adjustment.

Wait/follow clients observe the committed run and append-only output artifact directly with bounded polling. Their connection lifetime does not own or cancel execution. A future HTTP, MCP, or desktop surface must invoke the same durable application commands rather than treating the wake socket as a management protocol.

### Admission fairness and replacement

Global capacity is shared through durable round-robin admission across eligible jobs. Each pass admits at most one attempt per job, begins after the last durably admitted job, and repeats only while capacity remains. All target and trigger kinds have equal weight; v1 has no priority or weighted scheduling. This prevents a large catch-up or allow-overlap workload from taking every newly available slot before another eligible job is considered.

Within a job, durable eligibility time and queue sequence provide stable ordering. A missed-run `all` batch is its own ordered lane: only its oldest non-terminal member advances, including through retry, before the next batch member. Normal occurrences may still interact with that active batch exactly as the job's overlap policy requires.

A `replace` job retains at most one pending replacement candidate. A newer normal occurrence atomically terminalizes the previous candidate as `skipped_overlap` with a supersession reason and becomes the candidate. Termination intent is durable and signalling is not duplicated. No candidate is admitted until prior active termination is confirmed. Inability to confirm termination atomically terminalizes the newest candidate with an explicit failure reason and leaves the unconfirmed predecessor in a durable active-blocking quarantine; restart preserves the block without signalling its stale PID/PGID. Only an explicit operator acknowledgement of that exact run atomically terminalizes it as `interrupted_unknown` and releases the block. Members already selected into one missed-run `all` batch are not replacement candidates for one another.

## Durable model

SQLite is the authoritative local state. The names below are conceptual domain records, not frozen table names.

- **Job**: stable identity, unique live name, description, tags, enabled/removed state, and current revision.
- **Job revision**: immutable normalized schedule, target, environment references, policies, concurrency, and execution configuration.
- **Schedule cursor**: the durable reconciliation boundary plus interval anchor and an optional
  disabled-since boundary needed to classify elapsed time without inferring disablement from generic
  job metadata timestamps.
- **Run**: durable scheduled occurrence or manual request, trigger, nominal time, immutable execution snapshot, optional catch-up position, lifecycle, and final summary.
- **Attempt**: ordered execution try, owning scheduler lifetime, timing, selected executable or HTTP summary, result, and state.
- **Retry intent**: a known retryable failure plus durable not-before time belonging to the same run.
- **Event**: bounded audit fact such as mutation, missed-range summary, skip, recovery, truncation, or pruning.
- **Output metadata**: retained byte counts, truncation/discard facts, storage reference, and prune time.
- **Scheduler lifetime**: unique daemon lifetime and liveness facts used for exclusive ownership and crash classification.
- **Settings and schema metadata**: global execution settings, retention settings, migration version, and maintenance facts.

### Identity and time representation

- Stable job identities, run identities, and scheduler-lifetime identities are UUIDv7 values. User-editable job names are lookup aliases, not durable identity.
- A job revision is identified by its job identity plus a monotonically increasing revision number. An attempt is identified by its run identity plus a monotonically increasing attempt number.
- Events use a database-local increasing integer identity so event consumers have a stable local cursor.
- UUIDs are persisted and exposed as lowercase canonical strings. Their embedded millisecond field is not an authoritative creation timestamp and is not used alone for semantic ordering.
- Durable instants are signed 64-bit Unix epoch microseconds in UTC. Human and machine interfaces render them as RFC 3339 UTC values and accept explicit-offset input where the product contract permits it.
- Schedule timezone and source values are retained separately from normalized UTC occurrence instants so future occurrence calculation remains reproducible.
- Elapsed durations and timeout accounting use a monotonic clock while the process is alive, then persist the measured duration in integer microseconds. Wall-clock movement cannot produce a negative duration.
- Stable ordering uses an explicit timestamp followed by the record identity; UUID ordering never substitutes for a domain timestamp.

### Output representation

Attempt output is stored outside SQLite in one versioned, length-delimited framed file per attempt. Frames preserve observed stream order, identify stdout, stderr, or HTTP body data, accept arbitrary bytes, and carry sequence and monotonic elapsed-time information. SQLite remains authoritative for the logical output key, lifecycle, retained payload and file byte counts, discarded byte counts, truncation, and pruning facts.

An active attempt writes a partial file through one serializing writer. Successful finalization closes and atomically renames it on the same filesystem before recording finalized metadata. A crash leaves a recoverable partial artifact: startup recovery accepts only complete frames, discards an incomplete tail, reconciles byte metadata, and retains the owning attempt's `interrupted_unknown` semantics.

The per-run capture allowance is shared by every attempt in the run. Reaching a run or global storage bound stops persistence, not pipe draining or target execution. A truncation marker is rendered from metadata rather than inserted as target-authored bytes. Global pressure first prunes the oldest eligible terminal output; when no eligible output can be reclaimed, new capture is discarded and counted.

Pruning is restartable: durable intent precedes file removal, and completion metadata follows it. Verified unreferenced artifacts are deleted only after a safety grace period. Output directories and files are private to the owning user, output paths are derived from trusted identities rather than user input, and readers never follow user-controlled symbolic links.

The exact frame binary layout and table layout remain milestone implementation decisions.

### SQLite and scheduler ownership

The state database uses bundled SQLite in WAL mode with full synchronous durability, foreign-key enforcement, normal locking mode, a bounded busy timeout, and untrusted schema features disabled. locron supports its state directory only on a local filesystem; network filesystem behavior is outside the official contract.

The daemon serializes writes through one writer lane and uses a small bounded set of read connections. Short-lived CLI processes use one connection. Write transactions acquire intent before reading data they will mutate, remain short, and never span target execution, HTTP, output I/O, process signalling, or waits.

Exactly one daemon owns a state directory. Before opening the scheduler lifetime, it takes a non-blocking OS exclusive lock on a permanent lock file and holds that file descriptor for its full lifetime without allowing child inheritance. The lock file is never removed during ordinary shutdown. Diagnostic owner text is advisory; PID data is never used to kill a process or infer ownership. SQLite lifetime records remain the durable explanation of recovery.

The daemon acquires ownership before applying migrations. A CLI may migrate an older schema only after proving that no daemon holds the ownership lock; otherwise it requires a daemon restart. A database newer than the running binary is rejected. Schema version checks and migration steps occur under transactional revalidation so concurrent initializers cannot apply the same step twice.

## Persistence and lifecycle invariants

The following invariants hold regardless of the final schema or library selection:

1. A scheduled occurrence is durably unique by job identity, schedule revision, and scheduled instant. Manual runs have independent identities.
2. A run and its immutable execution snapshot exist before external execution begins. Later job edits, disablement, removal, or name reuse do not rewrite that snapshot or its history.
3. Schedule edits create a new occurrence namespace. Interval anchors and reconciliation cursors survive daemon restart and non-schedule edits.
4. SQLite transactions are short. Process execution, HTTP requests, signal waits, output streaming, and filesystem cleanup never occur inside a write transaction.
5. Reconciliation atomically rechecks the revision/cursor, materializes unique runs or bounded summary facts, resolves one-time state by disabling the current job revision, and advances the cursor. A conflict retries from fresh durable state. Manual submission never resolves or disables a one-time schedule. Explicit disable records disabled-since on the current cursor; re-enable preserves it until the first successful reconciliation clears it in the same cursor/materialization commit.
6. Manual submission atomically snapshots the job and creates its durable run; daemon availability is not a commit prerequisite.
7. Admission atomically rechecks active runs, catch-up ordering, policy, cancellation, capacity, and run state before creating a durable attempt and moving the run toward execution. Capacity is not reserved only in memory.
8. Attempt completion atomically records a known result and either a durable retry intent or a terminal run transition.
9. Cancellation is resolved atomically against the current run state. Queued and retry-wait runs
   become terminal `cancelled` immediately, clear any retry intent, and need no engine signal;
   starting and running runs retain durable intent before signalling. A replacement is not admitted
   until prior termination is confirmed.
10. A new daemon lifetime classifies stale non-terminal attempts from an older lifetime as `interrupted_unknown`. It neither infers their result nor signals a recorded stale PID/PGID. A termination-unconfirmed quarantine is the exception to ordinary run terminalization: its attempt is unknown, but the predecessor run remains active-blocking across lifetimes until an operator explicitly acknowledges that exact run. The acknowledgement transaction verifies the quarantine state, records an audit event, terminalizes the run as `interrupted_unknown`, and never inspects or signals stored process identities. Ordinary cancellation, an acknowledgement against another state, and repeated acknowledgement cannot release it.
11. Cleanup selects only eligible terminal data in bounded batches. It never prunes active runs and retains an explanation when output is truncated or removed.
12. Legal stored states, foreign-key ownership, scheduled occurrence uniqueness, attempt ordering, and unique live job names are defended by database constraints where SQLite can express them and by domain validation for cross-record rules.
13. External execution never starts until its attempt and logical output identity are durable. Output creation failure before spawn is a known execution failure; it cannot create an untracked child.
    The final starting-to-running commit rechecks cancellation and can terminalize before spawn;
    only its explicit ready decision authorizes the runner.
14. A daemon that cannot durably commit a required transition stops new admission and retries the transition; it never continues scheduling from memory-only state.
15. A wake notification is never evidence that a command exists or succeeded. Only committed durable state can cause the engine to act.
16. Admission cursor movement and attempt creation commit together. A crash cannot advance fairness without admitting the corresponding attempt or admit an attempt without advancing the cursor.
17. At most one normal replacement candidate exists for a job. Superseding it and installing its successor is one durable transition.
18. Once admission creates an attempt, every target-resolution or runtime-configuration result is
    completed durably. A known pre-execution failure finalizes an empty bounded output artifact and
    terminalizes the attempt and run as failed; adapter error propagation cannot orphan a running
    record.
19. A fixed 64-attempt in-process permit ceiling is only a hard safety guard. The admission
    transaction rereads durable global concurrency and counts durable starting/running attempts,
    then caps selection by both the setting and available hard-guard permits. A concurrent setting
    change serializes before or after that transaction rather than splitting its capacity decision.
    A decrease below the active count admits zero and does not signal active work; an increase is
    usable on the next pass without daemon restart.

These boundaries allow crash ambiguity to be represented honestly without promising exactly-once external side effects. A target that requires stronger duplicate protection can use the durable run identity as an idempotency key.

## Daemon ownership and shutdown

`locron-engine` is the sole owner of long-lived daemon behavior for one state directory:

- acquire and release scheduler lifetime/lock ownership;
- classify stale prior-lifetime work before ordinary admission;
- reconcile clocks, job changes, durable requests, and retry times;
- enforce per-job and global admission;
- supervise all target runners and output capture;
- perform bounded retention and orphan-temp maintenance;
- receive termination signals, stop new admission, allow the configured natural-completion window, terminate remaining process groups, and persist confirmed outcomes.

The CLI layer does not reproduce or coordinate these loops. Its daemon subcommand only validates startup configuration, constructs the accepted store implementation and engine dependencies, invokes the runtime, and renders startup/fatal errors.

Only one daemon may own a state directory at a time. The engine uses the permanent OS-locked file and scheduler-lifetime records described above; SQLite connection ownership alone is not the daemon lock.

## Compatibility boundaries

The domain vocabulary, durable migrations, export format, and machine-readable command results become compatibility surfaces at the first stable tag. Future HTTP, MCP, and desktop clients should depend on application commands/results and stable identifiers, not SQLite layout or CLI prose.

The service-manager process remains outside job semantics. Packaging milestones may install launchd/systemd definitions that invoke `locron daemon run`, but they must not introduce another scheduler implementation or executable.
