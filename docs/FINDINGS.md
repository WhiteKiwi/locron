# locron v1 Planning Findings

## Scope and Method

This document resolves the open questions in `SPEC.md` for the first program milestone only: a per-user local scheduler for macOS and Linux.

The evidence base favors operating-system and project documentation, language/runtime documentation, and direct competitor repositories. Exact numeric defaults that cannot be derived from an external standard are identified as product judgments and should be validated with load and recovery tests.

## 1. Overlap Policies

### Evidence

- Kubernetes CronJob exposes `Allow`, `Forbid`, and `Replace`. `Forbid` skips an occurrence when the preceding job is still running; `Replace` cancels the current job and starts the new occurrence. This demonstrates that skip and replace are established, independently useful semantics rather than variations of one boolean. Source: https://kubernetes.io/docs/reference/kubernetes-api/batch/cron-job-v1/
- RunWisp exposes `queue`, `skip`, and `terminate` for local scheduled tasks. Source: https://github.com/runwisp/runwisp
- Cronicle separates concurrency limits from optional queuing and adds an explicit queue limit to prevent an uncontrolled backlog. Source: https://github.com/jhuckaby/Cronicle/blob/master/docs/WebUI.md#allow-queued-jobs
- The current cockpit scheduler supports only skip and allow. That is a workable minimum, but it cannot express “run once after the active run” or “only the latest invocation matters.”

### Recommendation

Include all four policies in the first stable interface:

- `skip`: record the due occurrence as `skipped_overlap`; default.
- `queue-one`: keep at most one pending occurrence for the job. When more occurrences arrive, retain the latest scheduled time and record the displaced occurrences as skipped/coalesced.
- `replace`: request termination of the active process tree, wait through the normal termination grace period, then start the newest occurrence. If the old process tree cannot be confirmed stopped, do not start the replacement concurrently; fail the replacement with an explainable state.
- `allow`: permit parallel runs, still subject to per-job and global concurrency limits.

Use the name `queue-one`, not `queue`, because the bound is part of the contract. The policy applies to scheduled and catch-up occurrences. A normal manual trigger should obey it; a separate explicit force mechanism may bypass it later if a concrete use case justifies that risk.

### Alternatives and Trade-offs

- `skip|allow` is easier to implement, but adding queueing or replace after a stable release changes both the durable state model and cancellation contract. Those concepts are already required by the completion criteria, so deferring them saves little foundational work.
- An unbounded `queue` preserves every occurrence but is unsafe on laptops and overlaps with missed-run `all`. It should not be offered.
- `replace` is operationally harder than skip because process-tree termination must be confirmed. It is nevertheless valuable for refresh/index/poll work where stale execution is worse than cancellation.

## 2. Missed-run Policies

### Evidence

- Quartz CronTrigger distinguishes `DoNothing` from `FireOnceNow`; its default smart policy maps to fire once now. Source: https://www.quartz-scheduler.org/documentation/quartz-2.3.0/tutorials/tutorial-lesson-06.html
- Both launchd `StartInterval` and systemd calendar timers coalesce multiple expirations during sleep into one activation after wake. Sources: https://github.com/apple-oss-distributions/launchd/blob/main/man/launchd.plist.5 and https://github.com/systemd/systemd/blob/main/man/systemd.timer.xml
- RunWisp directly exposes `latest`, `all`, and `skip` catch-up policies. Source: https://github.com/runwisp/runwisp
- Cronicle can replay every missed scheduled time, but its documentation also provides queue limits because catch-up can otherwise grow indefinitely. Source: https://github.com/jhuckaby/Cronicle/blob/master/docs/WebUI.md#run-all-mode
- Kubernetes refuses to start a CronJob after more than 100 missed schedules rather than creating an unlimited storm. Source: https://kubernetes.io/docs/concepts/workloads/controllers/cron-jobs/

### Recommendation

Include `skip`, `latest`, and bounded `all` in v1:

- `skip`: do not execute missed occurrences; default for cron and fixed interval schedules.
- `latest`: create one catch-up run representing the latest eligible missed scheduled time; default for one-time schedules.
- `all`: create each eligible occurrence in chronological order, capped by `max-catchup`.

Use two independent bounds:

- `start-deadline`: maximum lateness for any occurrence. Default is unlimited for one-time schedules and disabled/no catch-up for recurring schedules through their default `skip` policy.
- `max-catchup`: maximum occurrences materialized by one reconciliation. Default `100`, allowed range `1..1000` when policy is `all`.

If more than the limit are eligible, keep the newest bounded window and persist one summary event with the omitted count and time range. `skip` and `latest` also use a missed-range summary instead of materializing an unbounded row per elapsed timestamp. This favors useful recent work over replaying the oldest backlog first and bounds database writes during recovery. A unique durable occurrence key must prevent the same `(job identity, schedule revision, scheduled time)` from being created twice.

### Alternatives and Trade-offs

- `skip|latest` is the safest small interface, but it cannot serve explicitly count-sensitive local work such as hourly export partitions or ledger ingestion.
- Unbounded `all` is rejected: sleep, clock repair, or a disabled daemon can generate an execution storm.
- A single “persistent” boolean, as found in OS schedulers, hides too many decisions and does not distinguish coalescing from replay.

## 3. Manual Trigger Return Behavior

### Evidence

- `systemd-run` starts a transient service asynchronously and returns after execution has begun by default; `--wait` waits for termination and returns its status, while `--pipe` connects command output. This establishes a useful CLI split between submission and observation. Source: https://man7.org/linux/man-pages/man1/systemd-run.1.html
- Cronicle’s “Run Now” creates an on-demand job immediately and exposes the resulting job through its active/completed job views. Source: https://github.com/jhuckaby/Cronicle/blob/master/docs/WebUI.md#run-now
- The specification requires durable identity before execution. Returning that identity makes scripts robust even when the actual work outlives the invoking terminal.

### Recommendation

Default `locron run NAME` to return after the run is durably enqueued and print the run ID. Add `--wait` to follow output through completion and return the job’s outcome as the CLI exit status. Machine-readable output must expose the same run ID and enqueue state.

`--wait` changes only caller attachment. It does not create a different durable execution mode, and disconnecting the caller must not cancel the run. Do not persist a `fire-and-forget` policy.

### Alternatives and Trade-offs

- Waiting by default is convenient for interactive debugging but makes automation unexpectedly block for long jobs.
- Returning before durable enqueue is not acceptable because the caller would have no reliable evidence that the request exists.

## 4. Fixed-interval Anchor

### Evidence

- systemd deliberately provides both activation-anchored (`OnUnitActiveSec`) and completion/inactive-anchored (`OnUnitInactiveSec`) timers, showing that these are different schedule types. Source: https://github.com/systemd/systemd/blob/main/man/systemd.timer.xml
- Scheduled-time anchoring produces stable nominal occurrence times. Completion anchoring makes the next due time depend on execution duration and prevents meaningful enumeration of missed occurrences while the scheduler is down.

### Recommendation

Anchor `--every` to scheduled time, starting from a persisted schedule anchor. Advance the next scheduled time by whole interval multiples, never by `now + interval` and never by completion time. Missed-run and overlap policies decide what to do with elapsed anchors.

Persist the anchor explicitly so a restart or job edit cannot silently reset phase. A schedule-changing update creates a new schedule revision and anchor; non-schedule metadata edits do not.

### Alternatives and Trade-offs

- Completion anchoring is useful for “wait N minutes after the previous run finishes,” but it is a distinct delay-after-completion schedule. Defer it rather than overloading `--every`.
- Anchoring to daemon startup is simple but produces drift on every restart and violates predictability.

## 5. Global Concurrency

### Evidence

- Cronicle supports both per-event and category-level concurrency limits, demonstrating that local/global admission control is separate from per-job overlap policy. Source: https://github.com/jhuckaby/Cronicle/blob/master/docs/WebUI.md#event-concurrency
- `cronn` defaults restart/resume work to one concurrent execution, favoring conservative recovery. Source: https://github.com/umputun/cronn
- GoCron goes further and allows only one command globally, skipping other work while it runs. This is safe but too restrictive for independent local jobs. Source: https://github.com/flohoss/gocron#failure-semantics
- No external standard supplies a correct laptop-wide number: shell jobs may be CPU-heavy, while HTTP jobs may spend almost all their time waiting.

### Recommendation

Set global concurrency to `4` by default, configurable from `1` through a hard maximum of `64`. Set per-job concurrency default to `1`; it may be raised only for `allow` and may not exceed the global limit.

The fixed default is a product judgment: four permits independent network and process work without tying behavior to CPU count, which is a poor proxy for mixed workloads. The hard maximum protects against configuration mistakes and descriptor/process storms. Verify these values with stress tests on the oldest supported macOS and a low-resource Linux VM before freezing the stable interface.

During crash catch-up, ordinary global admission control still applies; recovery must not get a separate concurrency pool.

### Alternatives and Trade-offs

- `1` is maximally conservative but creates unrelated-job head-of-line blocking.
- `available_parallelism()` looks adaptive but implies CPU capacity is the limiting resource and makes the same configuration behave differently by machine.
- An unlimited setting conflicts with the safe-by-default principle. If a real local use case needs more than 64, the maximum can be raised in a later compatibility-preserving release.

## 6. Executable Path Resolution

### Evidence

- POSIX `execvp` searches `PATH` when the executable has no slash. Source: https://pubs.opengroup.org/onlinepubs/9799919799/functions/exec.html
- Rust `std::process::Command` likewise searches `PATH` in an OS-defined way for a non-absolute program and documents that Unix uses the child environment’s `PATH`. Source: https://doc.rust-lang.org/std/process/struct.Command.html
- systemd offers `ExecSearchPath` as an explicit execution-environment setting, rather than resolving every executable when a unit is authored. Source: https://github.com/systemd/systemd/blob/main/man/systemd.exec.xml
- Registration-time absolute paths become stale after package upgrades, environment-manager changes, or moving an imported database to another machine. Runtime-only lookup without recording the result, however, makes failures difficult to explain.

### Recommendation

Store the command exactly as an argv vector and resolve a bare executable name at run time against the job’s effective, explicit `PATH`. Do not silently use the CLI process’s current `PATH` as permanent configuration.

- If argv[0] contains `/`, normalize a relative path against the job working directory at registration and persist the resulting absolute path.
- If argv[0] is bare, validate that it resolves at registration when possible, but keep the bare name.
- Persist the resolved absolute executable path in every run snapshot before spawn.
- Define and display the daemon baseline `PATH`; allow jobs to override `PATH` through their non-secret environment.
- `doctor` must show the effective `PATH` and current resolution result.

This provides package-upgrade friendliness while retaining per-run observability.

### Alternatives and Trade-offs

- Persisting only the registration-time absolute path is maximally deterministic but breaks common version-manager and package-manager upgrade flows.
- Resolving from an implicit launchd/systemd environment at run time recreates the classic cron “works in my shell” failure. The effective path must therefore be a locron-owned, inspectable value.

## 7. Inline Environment Values

### Evidence

- GoCron supports global and per-job key/value environments. Source: https://github.com/flohoss/gocron#job-defaults
- RunWisp distinguishes ordinary `env`, which is visible in the UI, from `secrets_file`, which is never returned by its API/UI. Source: https://github.com/runwisp/runwisp
- systemd’s credential design exists because public configuration and ordinary environment configuration are unsuitable places for secret material; credentials are passed separately via restricted runtime files. Source: https://github.com/systemd/systemd/blob/main/docs/CREDENTIALS.md
- Environment is essential for locale, tool configuration, and deterministic `PATH`, even when secret management is explicitly out of scope.

### Recommendation

Support inline non-secret environment values in v1 with `--env KEY=VALUE`, plus `--env-file PATH` as a runtime file reference. Treat both as execution configuration, but with different persistence:

- Inline values are stored in the per-user database and all values are redacted in normal list/show/log/diagnostic output. Only key names are shown.
- An env-file path is stored, but its contents are read at run time and never copied to the database, run snapshots, or locron logs.
- Export omits inline values by default and requires an explicit `--include-env-values` acknowledgement to include them.
- Documentation and CLI help must state that inline values are plaintext local configuration, not a secret store. Recommend an env file with user-only permissions or an external secret-injection command for credentials.
- Reject invalid names, NUL bytes, and attempts to override reserved `LOCRON_*` variables.

### Alternatives and Trade-offs

- Deferring all environment support would make direct execution impractical and leave `PATH` under-specified.
- Supporting only env files avoids database plaintext but harms simple portable jobs and import/export. The split model supports both without claiming secret management.
- Encrypting inline values would require key management and is explicitly deferred.

## 8. Language and Repository Shape

### Evidence

- The user selected Rust.
- Rust publishes and continuously tests host tools for the relevant macOS and Linux targets. Source: https://doc.rust-lang.org/rustc/platform-support.html
- The standard library exposes explicit argv/environment process construction and Unix process-group configuration. Sources: https://doc.rust-lang.org/std/process/struct.Command.html and https://doc.rust-lang.org/std/os/unix/process/trait.CommandExt.html
- Cargo workspaces share one lockfile, output directory, dependency declarations, metadata, profiles, and lint configuration while allowing package boundaries and workspace-wide checks. Source: https://doc.rust-lang.org/cargo/reference/workspaces.html

### Recommendation

Use Rust and start with a small virtual Cargo workspace using resolver `3` and exactly four crates:

- `locron-core`: domain types, schedule/policy vocabulary, validation, and ports/traits; no SQLite, CLI, or OS service knowledge.
- `locron-store`: SQLite schema, migrations, transactions, occurrence uniqueness, repositories, and retention.
- `locron-engine`: reconciliation, admission control, retries, execution, cancellation, recovery, and HTTP/process runners.
- `locron-cli`: the `locron` binary, command parsing, human/JSON rendering, and daemon entrypoint.

Keep one distributable binary in v1 even though its code is split into libraries. Do not create empty crates for later HTTP management, MCP, or desktop surfaces; add those only when their milestones begin. Enforce the following dependency shape: `locron-cli` composes `locron-engine` and `locron-store`; both depend on `locron-core`; `locron-engine` does not depend on the SQLite implementation; and no library depends back on the CLI.

### Alternatives and Trade-offs

- One crate minimizes manifests but makes it easier for CLI, persistence, and scheduling concerns to become mutually dependent. The future surfaces already justify a small reusable core boundary.
- More granular crates for every runner or policy would optimize prematurely and slow early refactoring.
- Go remains a credible single-binary alternative, but the language choice is resolved and should not remain open.

## 9. Crash Recovery

### Evidence

- SQLite provides ACID transactions and UNIQUE constraints, which can atomically create a durable occurrence before execution and reject duplicate occurrence identities. Sources: https://www.sqlite.org/fullsql.html and https://www.sqlite.org/lang_createtable.html
- GoCron marks database rows left in `Running` as `Canceled` on next startup after a hard kill. This is explainable but overstates knowledge: an orphaned subprocess may have completed after its parent died. Source: https://github.com/flohoss/gocron#graceful-shutdown
- launchd normally kills remaining processes with the managed job’s process-group ID when the job dies, unless `AbandonProcessGroup` is enabled. A locron child placed in its own per-run process group does not share that ID, so this behavior cannot by itself guarantee child cleanup. Source: https://github.com/apple-oss-distributions/launchd/blob/main/man/launchd.plist.5
- systemd tracks service processes through control groups and terminates remaining processes according to `KillMode`; this is stronger than portable PID tracking but is Linux/service-manager-specific. Sources: https://github.com/systemd/systemd/blob/main/man/systemd.xml and https://github.com/systemd/systemd/blob/main/man/systemd.service.xml
- Rust can place children in a Unix process group, enabling normal timeout and cancellation of a process tree on both target operating systems. Source: https://doc.rust-lang.org/std/os/unix/process/trait.CommandExt.html

### Recommendation

Promise the following minimum across both platforms:

1. The scheduler transactionally creates a unique durable occurrence/run record before spawn.
2. After successful spawn it records the attempt and process metadata. Normal timeout, cancellation, and graceful daemon shutdown signal the whole job process group, wait a configurable grace period, then force-kill it.
3. On startup, every non-terminal attempt owned by a previous daemon lifetime becomes `interrupted_unknown`, with an audit event identifying crash recovery. It is never reported as success, failure, or cancellation without evidence.
4. `interrupted_unknown` is not automatically retried by default. An explicit job policy may later permit unknown-outcome retry, but not in v1.
5. Reconciliation may create genuinely missed occurrences according to policy, but the unique occurrence key prevents recreation of the interrupted scheduled occurrence. This makes one-time schedules restart-safe without claiming exactly-once side effects.
6. The daemon does not attempt to reattach to or kill an old PID/PGID on startup: PID reuse and cross-platform ownership checks make that unsafe. systemd cgroup cleanup is useful defense in depth; launchd’s same-process-group cleanup must not be treated as equivalent because per-run children use distinct process groups.

Test this with forced daemon `SIGKILL` at transaction-before-spawn, spawn-before-running-update, running, and child-exit-before-final-update boundaries. Assertions should cover durable identity, classification, no implicit retry, no duplicate one-time run, and service-manager process cleanup separately on macOS and Linux.

### Alternatives and Trade-offs

- Claiming all stale rows are `cancelled` is simple but asserts an outcome the scheduler does not know.
- Reattaching to an orphan would require a dedicated child supervisor or IPC protocol and is not reliably portable.
- Detached/untracked execution would undermine timeout, cancellation, and observability and is deferred.

## 10. History and Log Retention

### Evidence

- GoCron defaults to deleting run history and logs after seven days. Source: https://github.com/flohoss/gocron#log-retention
- Cronicle retains up to 10,000 completed-job rows by default and allows a per-job log-expiry override. Source: https://github.com/jhuckaby/Cronicle/blob/master/docs/WebUI.md#completed-jobs-tab
- RunWisp’s example retains 30 runs for a task and provides per-task log rotation. Source: https://github.com/runwisp/runwisp
- A time-only bound is insufficient for a second-level scheduler: one noisy or frequent job can consume unbounded disk before its age threshold is reached.

### Recommendation

Separate lightweight run metadata from captured byte output:

- Run metadata: keep terminal runs for `90 days`, with a secondary cap of `1,000 per job` and `10,000 globally`.
- Captured output: keep for `30 days`, cap each run at `10 MiB` combined stdout/stderr, and cap all retained output at `256 MiB` globally.
- On per-run overflow, continue the process but truncate capture with an explicit marker and byte counts.
- On global overflow, evict oldest terminal-run output first. Keep the run summary and an `output_pruned_at` marker until metadata itself expires.
- Never prune active runs. Removal of a job is a soft delete and follows normal history retention.
- Run cleanup at startup and periodically, in small batches. Expose current usage, configured bounds, last cleanup, and truncation/pruning through diagnostics.

These values are product judgments between competitors’ seven-day and 10,000-row defaults. They must be covered by deterministic cleanup tests and a disk-pressure test before the interface freezes.

### Alternatives and Trade-offs

- Per-job count alone is predictable but does not bound bytes or global database growth.
- Time alone is friendly to users but unsafe for high-frequency jobs.
- Keeping failed logs longer than successful logs adds policy complexity. Start with one rule; users can export important logs before expiry.

## Initial Research Policy Matrix (Superseded Where Noted)

This table records the research sub-session's initial recommendation, not the accepted product contract. Interactive review subsequently removed `queue-one`, changed global concurrency to 16, and refined other details in `docs/SPEC.md`. The specification always wins.

| Concern | Stable values | Default | Bound or important rule |
|---|---|---|---|
| Overlap | `skip`, `replace`, `allow` | `skip` | no general overlap queue; replace retains only the newest candidate |
| Missed recurring run | `skip`, `latest`, `all` | `skip` | `all` defaults to 100, maximum 1,000 per reconciliation |
| Missed one-time run | `skip`, `latest` | `latest` | unique durable occurrence prevents restart duplication |
| Start lateness | duration or unlimited | recurring: not applicable under `skip`; one-time: unlimited | independently filters catch-up eligibility |
| Manual run | enqueue, `--wait` | enqueue | return only after durable creation; always print run ID |
| Fixed interval | scheduled-time anchored | scheduled time | persisted anchor and schedule revision |
| Retry | explicit eligible outcomes and backoff | `0` retries | never auto-retry `interrupted_unknown` in v1 |
| Per-job concurrency | integer | `1` | greater than 1 requires overlap `allow` |
| Global concurrency | integer | `16` | range `1..64` |
| Executable lookup | absolute/relative-with-slash or bare name | runtime resolution for bare name | effective `PATH` is locron-owned; resolved path captured per run |
| Environment | inline non-secret values, env-file reference | empty job override | values redacted; env-file contents never persisted |
| Crash recovery | `interrupted_unknown` | no retry | do not reattach/kill stale PID on startup |
| Run history | time plus count caps | 90 days | 1,000/job and 10,000 global |
| Output retention | time plus byte caps | 30 days | 10 MiB/run and 256 MiB global |

## Explicitly Deferred Features

The following should not be added while implementing the first program milestone:

- Unbounded overlap queues or unbounded missed-run replay.
- Delay-after-completion/restart-delay schedules; `--every` remains scheduled-time anchored.
- A persisted `fire-and-forget`, detached, untracked, or daemon-surviving execution mode.
- Reattaching to orphaned processes after daemon restart.
- Automatic retry of unknown crash outcomes.
- Built-in encryption, key management, secret storage, or secret-provider integrations.
- CPU-, memory-, battery-, network-, or idle-state admission policies.
- Randomized delay/jitter, business calendars, holiday calendars, and natural-language parsing.
- DAGs, dependencies, chaining, distributed workers, and long-running service supervision.
- HTTP management/viewer server, MCP server, desktop application, and their Cargo crates.
- Package-manager publication and launchd/systemd installation automation.
- More than the four initial Cargo crates without a demonstrated dependency boundary.

## Remaining Validation, Not Product Questions

The ten specification questions are resolved by the recommendations above. Before the specification is frozen, implementation planning should turn the following into executable verification steps rather than reopen them as undefined policy:

- Stress-test global concurrency `16` and the hard maximum `64` on a low-resource Linux VM and the oldest supported macOS target.
- Simulate clock movement, sleep, and downtime to verify scheduled-time anchoring and bounded catch-up.
- Verify process-group cancellation for direct and explicit-shell commands, including grandchildren, on both operating systems.
- Inject crashes at every durable-run/spawn state boundary and assert `interrupted_unknown`, occurrence uniqueness, and one-time restart safety.
- Fill output beyond per-run and global bounds and verify truncation, eviction order, retained summaries, and bounded disk use.

## Implementation Dependency Follow-up

Research on 2026-08-21 checked the current primary documentation for the selected Rust building blocks:

- Tokio exposes the timers, process, signal, Unix networking, synchronization, and deterministic test-time facilities needed by the engine without forcing a framework into the domain model. Source: https://docs.rs/tokio/latest/tokio/
- `rusqlite` recommends its bundled feature for applications that control their own database, avoiding missing or old system SQLite versions. Source: https://docs.rs/crate/rusqlite/latest
- Reqwest supports Rustls, streaming bodies, and disabled/custom redirect behavior. locron disables automatic redirects and owns the redirect loop so sensitive-header policy remains explicit. Source: https://docs.rs/reqwest/latest/reqwest/
- Jiff exposes system and IANA time zones plus explicit gap/fold ambiguity, allowing locron to skip nonexistent civil minutes and select the first repeated minute. Source: https://docs.rs/jiff/latest/jiff/tz/index.html
- Rust standard file locking has been stable since Rust 1.89, below locron's Rust 1.94 MSRV. Source: https://doc.rust-lang.org/stable/std/fs/struct.File.html

Croner 3.0.1 was evaluated and rejected as the scheduler authority. Its documented fixed-time DST-gap behavior moves execution to the first valid time after a gap, and its wildcard DST-fold behavior may execute both repeated matches. Both conflict with the frozen locron rule: skip a nonexistent wall time and produce one occurrence for a repeated wall time. Croner also accepts extensions beyond the exact v1 grammar. Source: https://docs.rs/crate/croner/latest/source/README.md

The accepted implementation therefore owns five-field parsing and civil-minute enumeration in `locron-core` and uses Jiff only for calendar/timezone primitives and explicit ambiguity classification. This is more code than adopting a cron iterator, but it keeps the most safety-sensitive behavior directly testable and prevents dependency defaults from silently changing product semantics.
