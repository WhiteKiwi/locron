# locron Specification

## Status

Frozen on 2026-08-21 after interactive product review. Changes to this document represent a change in product scope or behavior and must precede implementation changes.

Amended 2026-08-23: version reporting honors the machine-readable output contract.
Amended 2026-08-23: installation channels and self-update added; package-manager publication removed from out-of-scope items.
Amended 2026-08-23: daemon service installation and automatic startup added; operating-system service installation removed from out-of-scope items.
Amended 2026-08-24: export job selection and URL import added.
Amended 2026-08-24: human output contract added — human mode renders readable per-command forms instead of machine JSON (issue #4).
Amended 2026-08-24: the human table form fits the terminal width on a terminal; full values remain available through the detail report, machine output, and a no-truncation rendering flag.
Amended 2026-08-24: documentation-facing product positioning prioritizes explainability, real-world scheduling semantics, and safe automation surfaces.
Amended 2026-08-24: a consolidated job explanation reports current scheduling facts, the latest run, and the latest anomalous run.
Amended 2026-08-24: automated Homebrew publication preserves formula guidance literally and produces a style-clean formula.
Amended 2026-08-24: one-time jobs may opt into automatic soft deletion after their scheduled run reaches a terminal outcome.

## Goal

Build a local-first job scheduler that lets one user register, inspect, run, and manage scheduled work consistently on macOS and Linux.

The product should retain the simplicity of cron while making execution behavior observable and explicit. A user should be able to understand what was scheduled, when it was expected to run, whether it actually ran, and why it did not run without consulting operating-system-specific scheduler files or logs.

locron owns scheduling and execution semantics itself. It is not a management wrapper that translates individual jobs into cron, launchd, or systemd schedules. Operating-system service managers may keep locron available, but they do not become job-level scheduling backends.

## Product Positioning

The primary product message is **“Cron that explains itself.”** locron is an observable local scheduler, not merely a friendlier syntax for operating-system cron. Its leading user benefit is that a person can inspect the scheduler's durable facts and understand why work ran, did not run, or is not currently eligible.

Product documentation presents capabilities in this order:

1. Explainability: preview, history, captured output, job and run explanations, and health diagnostics.
2. Real-world reliability: explicit missed-run and overlap policies, durable occurrence identity, bounded catch-up, and restart or crash recovery.
3. Safe automation: machine-readable output, non-mutating dry runs, and optional agent-facing surfaces that reuse the same validation and durable application semantics.
4. Implementation evidence: local storage, transactions, process supervision, and similar internals support the reliability claim but do not lead the product story.

Documentation examples must reflect commands and human output that exist in the current release. They must not imply that locron directly observes machine sleep state when it only has durable schedule cursors, daemon lifetime facts, and reconciliation events, and they must not present planned diagnostic commands or richer decision traces as shipped features.

## Product Principles

- Local-first: schedules and execution history belong to one machine and one user.
- Predictable: missed schedules, overlapping runs, timeouts, retries, and manual triggers have documented behavior.
- Observable: every attempted, skipped, cancelled, or interrupted run can be explained.
- Safe by default: concurrency and retry behavior must not create accidental duplicate work.
- Portable: the same user-facing behavior is available on supported macOS and Linux systems.
- Independently useful: the command-line scheduler is complete without a web viewer, MCP integration, or desktop application.

## Completion Criteria

The first program milestone is complete when all of the following can be observed on both macOS and Linux:

1. A user can register exactly one of a calendar schedule, fixed interval, or one-time schedule for a job.
2. A user can register process and HTTP work without editing operating-system scheduler configuration.
3. New and updated jobs are recognized without restarting the scheduler.
4. A user can list, inspect, update, enable, disable, remove, and manually trigger jobs.
5. A user can preview future scheduled times before enabling a job.
6. Each scheduled occurrence receives a durable identity before execution begins.
7. Each run exposes its scheduled time, actual start and finish times, trigger source, outcome, duration, and captured output.
8. Long-running work is subject to a configurable timeout and cancellable as a process tree.
9. Overlapping and missed occurrences follow explicit per-job policies.
10. Failure retries occur only when explicitly configured and are visible as attempts of the same run.
11. One-time jobs cannot execute more than once merely because the scheduler restarts.
12. Restarting after an unclean shutdown produces an explainable terminal or recoverable state for previously active runs.
13. Invalid schedules and conflicting options are rejected before they can become active.
14. Stored state survives scheduler restarts and schema upgrades.
15. Automated tests cover time progression, sleep or downtime recovery, daylight-saving transitions, overlap, timeout, cancellation, retry, and unclean restart behavior.
16. A user can simulate a mutation or manual run without changing durable state, and can ask why a job or run is in its current state without enabling debug logs.
17. A user can request one consolidated explanation of a job's current scheduling state, latest run, and latest anomalous terminal run without assembling those facts from several commands.

## In Scope

- Per-user operation on macOS and Linux.
- One local machine and one user account per scheduler instance.
- Calendar schedules using standard five-field cron expressions.
- Fixed intervals with second-level duration units.
- One-time schedules using unambiguous ISO 8601 timestamps.
- Per-job time zones.
- Direct process execution and explicitly requested shell execution as distinct target modes.
- HTTP request execution.
- Job metadata including stable identity, unique name, description, and tags.
- Enable, disable, update, remove, manual trigger, history, log, and cancellation operations.
- Explicit overlap, missed-run, timeout, and retry policies.
- Global and per-job concurrency limits.
- Durable local state, execution history, and bounded log retention.
- Machine-readable command output for automation.
- Export and import for backup, migration, and command sharing, including selection of a job subset on export and import from a URL.
- Diagnostics that explain effective paths, environment, scheduler health, and invalid jobs.

## Out of Scope

- Distributed scheduling or remote workers.
- Multi-user or system-wide scheduling.
- Workflow graphs, job dependencies, and business calendars.
- Long-running service supervision.
- Container orchestration.
- Natural-language schedule parsing.
- Built-in secret management.
- Importing only a selected subset of jobs from an export document.
- A web viewer or HTTP management API.
- MCP integration.
- A desktop application.
- Operating-system service installation beyond the per-user registration and startup described in Daemon Service Installation, such as system-wide units, multi-user services, or container-supervisor integration.

The excluded delivery surfaces may influence compatibility boundaries, but they are not acceptance criteria for the first program milestone.

## Installation Channels

A user can install a working prebuilt locron on macOS and Linux without Homebrew, without any developer toolchain (no Xcode Command Line Tools, no Rust), and without administrative privileges. This channel exists so that a machine whose package manager or toolchain is unavailable or outdated can still install and update locron cleanly.

- A single shell command retrieves and executes an official installer that selects the correct published release archive for the machine's operating system and CPU architecture.
- The installer verifies the selected archive against the checksums published with the release before installing anything.
- The default install location is inside the user's home directory and requires no elevated privileges. An explicit alternative prefix is supported.
- After installing, the installer tells the user how to run locron, including any path adjustment the default location requires.
- The installer is repeatable: running the same command again replaces the binary with the latest published release. Installing a pinned version is also supported.
- The installer reports actionable errors for unsupported platforms, failed downloads, checksum mismatches, and unwritable install locations, and it does not require or modify a package manager.
- Homebrew remains a supported channel with its own update path. A script-installed and a Homebrew-installed locron may coexist on one machine; each channel updates through itself.
- Automated Homebrew publication must preserve the formula's package-manager marker guidance and service-upgrade caveats exactly as authored, without interpreting documentation text as shell commands, and the generated formula must pass the tap's syntax and style checks.
- A built-in self-update subcommand replaces the running locron with the latest stable release, selected and verified exactly as the installer verifies downloads, and reports the current and new version before replacing.
- Self-update manages only installations it can confirm are not owned by a package manager. When the running binary is package-manager-managed, self-update refuses with guidance to use that manager's update path.
- A failed or interrupted update must leave the existing binary installed and working. Update failures and permission problems produce actionable errors.
- Selecting a pinned version remains an installer function; self-update always moves to the latest stable release.

## Daemon Service Installation

A user can install locron and have the scheduler running without learning how their operating system supervises background services.

- Installing through the script installer registers and starts the daemon automatically, without administrative privileges, so schedules take effect immediately without a manual start step. An installation can explicitly decline service registration.
- The registration is a built-in locron operation that knows the installed binary's location. Repeating it refreshes or repairs the registration and is safe while a daemon is already running.
- On macOS the service is a per-user LaunchAgent. On Linux it is a systemd user unit inside the user's login session.
- The script installer attempts the Linux registration and, when the environment has no systemd user session, completes the installation successfully with explicit guidance for registering and starting the daemon. Linux package installations (deb/rpm) never register automatically and always print that guidance.
- Homebrew installation ships the service definition so the package manager's own service mechanism can start and supervise the daemon, and installation does not start it automatically. Package-manager-managed services follow the package manager's start, stop, and update behavior.
- On Linux the daemon stops at logout and starts again at the next login. Work missed while the daemon was unavailable is reconciled under the missed-run policy when it next starts. Keeping the daemon running after logout is documented as an optional operator step, not an installer behavior.
- Where locron manages the registration itself, a built-in removal operation unregisters the service without removing the binary.
- The service keeps the locron daemon itself available, restarting it when it stops unexpectedly. It never becomes a job-level scheduling backend.
- Starting the registered service never creates a second scheduler for one state directory; the existing single-owner guarantee still applies.
- When an update replaces the binary, a running daemon keeps the old code until it restarts. Where locron manages the registration itself, the update refreshes the registration and restarts the daemon under ordinary graceful-shutdown rules so the new version takes effect.

## Required Policy Vocabulary

The product must describe independent policies for:

- What happens when an occurrence becomes due while a previous run is active.
- What happens when an occurrence was missed while the scheduler or machine was unavailable.
- How late an occurrence may start and still be useful.
- Which failures are eligible for retry and how retry delays are calculated.
- Whether a manual trigger returns immediately or waits for completion.
- How active runs are classified after scheduler termination or machine restart.

The term “fire-and-forget” must not be used as a persisted execution policy because it conflates caller waiting, durable queuing, process supervision, and result tracking.

## Schedule Semantics

Each job has exactly one schedule:

- A standard five-field cron expression with minute-level resolution.
- A fixed interval with second-level duration units.
- An unambiguous one-time ISO 8601 timestamp that includes an offset.

Calendar schedules support standard cron aliases and use traditional cron matching when both day-of-month and day-of-week are restricted. Each calendar schedule has either a symbolic system-local time zone or a fixed IANA time zone. A system-local schedule follows changes to the machine time zone; a fixed IANA schedule does not.

During a daylight-saving transition, a nonexistent local wall-clock time is skipped and a repeated wall-clock time produces one occurrence. This favors duplicate safety over matching the same local clock value twice.

A fixed interval is calculated from a durable anchor, not from the previous run's completion time. The default anchor is the time at which the schedule is created. Disabling and re-enabling a job does not move the anchor. A manual trigger does not move any schedule or affect its next occurrence.

A one-time schedule becomes disabled after its scheduled occurrence is resolved. Its definition and history remain available, and manual triggers remain possible. Removing the definition is a separate explicit operation.

`locron add` and `locron update` accept `--delete-after-run` only with an `--at` schedule. It selects automatic removal of the job definition after the one scheduled run reaches a terminal outcome, including all configured retries. The job is soft-removed atomically with that terminal transition, so it no longer appears in `list --all`, cannot be enabled or manually run, and its name becomes reusable. Manual runs never consume this action. Disabling a job before its scheduled occurrence leaves it intact. `--delete-after-run` does not delete run metadata or captured output; normal retention remains the only mechanism that removes those records.

Editing a schedule affects future occurrences only. It does not mutate runs that have already received durable identities.

## Overlap Semantics

Each job selects one of three overlap policies:

- `skip`: while a run is active, each newly due occurrence is recorded as `skipped_overlap`. This is the default.
- `replace`: request termination of the active process tree, wait through the normal termination grace period, and start the newest occurrence only after termination is confirmed. If termination cannot be confirmed, the replacement does not run concurrently and ends in an explainable failure state.
- `allow`: make concurrent occurrences eligible for execution, subject to per-job and global concurrency limits.

Overlap policy applies to scheduled and normal manual occurrences, including a new normal occurrence that becomes due while catch-up work is active. Members already selected into the same bounded catch-up batch remain durable queued work and are not discarded merely because another member of that batch is active. The first stable interface does not provide a force option that silently bypasses the selected policy or concurrency admission.

Every due occurrence must remain explainable as executed, skipped, or otherwise terminal. No overlap queue is supported in the first stable interface.

## Missed-run Semantics

An occurrence is missed when its scheduled time passes without a run being created because the machine was powered off or asleep, the scheduler was stopped, or the job was disabled. Each job selects one missed-run policy:

- `skip`: do not execute missed occurrences. This is the default for calendar and fixed-interval schedules.
- `latest`: create one catch-up run representing the latest eligible missed scheduled time. This is the default for one-time schedules.
- `all`: create eligible missed occurrences in chronological order as a bounded catch-up batch.

Each job may define a start deadline that excludes occurrences older than the allowed lateness. One-time schedules have no default deadline. An `all` batch defaults to at most 100 occurrences and may be configured from 1 through 1,000. If more occurrences are eligible, the newest bounded window is retained and executed in chronological order. Omitted and skipped ranges are represented by summary events containing their count and time range instead of unbounded per-occurrence rows.

Missed-run selection happens before overlap admission. A selected catch-up batch is a durable queue and its members run in scheduled-time order. If a new normal occurrence becomes due while a catch-up member is active, the job's overlap policy controls that new occurrence: `skip` discards it, `replace` terminates the active catch-up run before starting it, and `allow` permits it subject to concurrency limits.

Every scheduled occurrence is uniquely identified by job identity, schedule revision, and scheduled time. Restart, reconciliation, or a backward clock adjustment must not create the same occurrence twice.

## Retry Semantics

A retry is another attempt of the same durable run and scheduled occurrence, not a new run. Each attempt retains its own timing, outcome, exit or HTTP status, and captured output. A run exposes both its final outcome and the complete ordered attempt history.

Automatic retry is disabled by default. A job may request up to 10 retries in addition to the initial attempt. Retry delay defaults to 10 seconds with exponential backoff capped at five minutes. Fixed backoff is also supported. Random jitter is not part of the first stable interface.

By default, only known execution failures are eligible:

- A process exits with a non-zero status.
- An HTTP request fails because of a connection, name-resolution, or transport error.
- An HTTP response has status 408, 429, or a 5xx status.

Cancellation, overlap replacement, invalid execution configuration, and an outcome made unknown by scheduler termination are never retried automatically. Timeout is not retryable by default but may be explicitly selected by the job.

Timeout applies independently to each attempt. Retry delays and repeated attempt time are not included in one total-run timeout. A run waiting for its next retry remains active for overlap purposes. A scheduler restart preserves a retry that was durably scheduled after a known failure, but it must not synthesize a retry for an attempt whose outcome is unknown.

The start deadline decides whether a newly due occurrence may begin its first attempt. Once a run has
started and a known retry has been durably scheduled, that retry remains eligible after its original
occurrence passes the start deadline. It is still subject to cancellation and ordinary global and
per-job admission. This prevents the meaning of an accepted run from changing while it waits for an
explicitly configured retry.

Retries do not move or recalculate the job's normal schedule.

## Manual-run Semantics

A normal manual trigger snapshots the current job definition, creates a durable manual run, prints its run identity, and returns after enqueue. Successful submission means the durable request exists; it does not mean the target has completed successfully. Submission remains possible while the scheduler is offline, and the result must clearly expose that the run is queued without an active scheduler.

An optional wait mode follows output through all attempts and returns a status representing the final target outcome. Wait mode changes only caller attachment and does not create a different execution policy. Terminal disconnection or interruption of the waiting client does not cancel the durable run; cancellation is a separate explicit operation.

A disabled job remains manually runnable. A manual trigger does not move the normal schedule and does not consume a one-time scheduled occurrence. Manual runs obey the job's overlap policy and ordinary concurrency admission. Human-readable and machine-readable submission output both include the run identity.

## Concurrency Semantics

The scheduler admits at most 16 active attempts globally by default. The configured limit may range from 1 through 64 and applies uniformly to process, shell, HTTP, retry, manual, and catch-up work. Recovery does not receive a separate execution pool. A running scheduler applies a durable limit change to its next admission decision without restart. Reducing the limit does not terminate attempts that are already running; while the active count is at or above the new limit no additional attempt is admitted. Increasing the limit makes the additional capacity available without restarting the scheduler.

Global capacity exhaustion leaves an otherwise eligible durable run queued rather than discarding it. A queued run counts as active for same-job overlap decisions so global pressure cannot create an unbounded same-job queue.

Jobs using `skip` or `replace` have an effective per-job concurrency of one. Jobs using `allow` default to two and may configure a value from two through the current global limit. Once that per-job limit is reached, a new normal occurrence is recorded as skipped for concurrency rather than queued without a bound. A bounded catch-up batch selected by missed-run `all` remains the explicit exception and is processed in scheduled-time order.

## Process and Shell Targets

Direct process execution is the default process target. The executable and each argument are stored as an explicit argument vector and are not interpreted for pipelines, redirection, globbing, variable substitution, command chaining, or home-directory expansion.

Shell execution is a distinct, explicitly requested target. It stores one command string and defaults to predictable POSIX shell execution rather than implicitly selecting the registering terminal's shell or interactive configuration. A job or global setting may select another absolute shell executable.

The default working directory is the absolute current directory at registration. An explicitly supplied working directory is expanded and normalized at registration. A missing working directory at execution is a non-retryable configuration failure.

An executable containing a path separator is normalized against the job working directory at registration and persisted as an absolute path. A bare executable name remains bare. It is validated at registration when possible and resolved at execution against a locron-owned, inspectable effective path. Initial setup seeds the global execution path from the invoking terminal, jobs may override it, and the daemon service manager's implicit environment is not the source of truth. Each run snapshot records the absolute executable actually selected before spawn. Diagnostics expose the effective path and current resolution results.

## Execution Environment

Execution does not inherit the daemon's complete environment. A minimal operating-system environment supplies home, user identity, locale, and temporary-directory values. The locron-owned global execution path is then applied, followed in order by global environment values, a job environment file, job inline values, and reserved run metadata. Later layers override earlier layers.

Inline non-secret values are supported and persisted as plaintext user-local configuration. Values are preserved in immutable run configuration but are redacted from normal list, inspection, log, diagnostic, and export output. Exporting values requires an explicit acknowledgement.

An environment-file target is normalized to an absolute path and read at execution. Its contents are never copied into job or run records. A missing, unreadable, or malformed file is a non-retryable configuration failure. Diagnostics warn when file permissions are broader than the current user. The runtime may retain a content hash for audit without retaining the content.

Reserved `LOCRON_*` names cannot be supplied or overridden by user configuration. Each attempt receives at least job identity, run identity, nominal scheduled time, trigger source, and attempt number through reserved values.

Built-in encryption, key management, and secret-provider integration are out of scope. Documentation must state that inline values, process arguments, and shell command strings are plaintext local configuration. locron does not emit environment values itself, but it cannot prevent a target from writing sensitive data to captured output.

## HTTP Targets

An HTTP target uses an absolute HTTP or HTTPS URL and one of GET, POST, PUT, PATCH, DELETE, or HEAD. Relative routes and an implicit base URL are not supported. A response status from 200 through 299 is successful by default; jobs may add explicit successful status values or ranges.

A request may use one inline body, one runtime body-file reference, or a JSON convenience form, and these forms are mutually exclusive. Inline bodies are plaintext job configuration. A body-file path is normalized to an absolute path and its content is read only at execution. Missing or unreadable request files are non-retryable configuration failures.

Headers may use inline non-secret values or source their value from the effective execution environment. Sensitive request values are redacted from normal output. Response headers are not retained by default. The run captures response status, content type, and response body under the ordinary output-retention limits.

Redirect following is disabled by default and may be explicitly enabled with a maximum of 10 redirects. Sensitive headers are not forwarded across origins. TLS certificate verification is mandatory in the first stable interface; local cleartext development targets may use loopback HTTP.

Attempt timeout covers the complete HTTP request. Transport and name-resolution errors, status 408, status 429, and 5xx statuses use the approved known-failure retry semantics. Other 4xx statuses fail without default retry. Oversized response output is truncated for storage without changing the success decision.

## Execution Lifecycle and Crash Recovery

Run lifecycle distinguishes queued, starting, running, and retry-wait states from terminal success, failure, timeout, cancellation, overlap skip, concurrency skip, and interrupted-unknown outcomes.

Each attempt has a 60-second timeout by default. A job may select another duration or explicitly remove the timeout. Process and shell attempts run in their own process group. Timeout, cancellation, and overlap replacement signal the process group for graceful termination, wait five seconds by default, and then force termination if necessary. Completion is not reported until termination is confirmed. A descendant that deliberately escapes the process group is outside the portable guarantee.

On normal scheduler termination, new admission stops and active attempts receive 30 seconds by default to finish naturally. Remaining process groups then follow the ordinary graceful and forced termination sequence. Confirmed termination is cancellation; an unconfirmed outcome is interrupted-unknown.

A run and its scheduled occurrence identity are durable before process or HTTP execution begins. After an unclean scheduler lifetime ends, every stale non-terminal attempt owned by that lifetime becomes `interrupted_unknown`. The scheduler does not infer success, failure, or cancellation; does not automatically retry the unknown outcome; and does not reattach to or signal a stale recorded process identity after restart.

If process-group termination cannot be confirmed, locron keeps the run in an active-blocking
quarantine because the target may still be executing. Ordinary cancellation cannot silently clear
that uncertainty. The operator may explicitly acknowledge the exact quarantined run identity and
accept that risk; this atomically records an acknowledgement event, finishes the run as
`interrupted_unknown`, and releases later same-job work without inspecting or signalling the
recorded PID or process group. Acknowledgement is valid only for that quarantine state. Omitting the
acknowledgement, acknowledging another state, or repeating an already completed acknowledgement is
an actionable stable conflict rather than a successful no-op.

Durable occurrence uniqueness prevents a restart from creating the same scheduled occurrence or one-time run again. This does not promise exactly-once external side effects. Targets may use the reserved run identity as an idempotency key when they need stronger protection.

## Inspection and Diagnostic Semantics

Mutating commands and manual execution support a dry-run mode. A dry run performs parsing, normalization, validation, schedule calculation, target/path resolution when safely possible, and a current admission simulation, but does not mutate durable state, create a run identity, signal a process, execute a command, or issue an HTTP request. Its result is explicitly a point-in-time simulation and does not reserve future capacity.

A dedicated explanation command reports why a job is or is not eligible, its next occurrence, applicable missed-run and overlap decisions, current concurrency blockers, daemon availability, and redacted target resolution. The same command can explain a run from its durable snapshot, attempts, events, supersession, and terminal reason. Explanations use durable facts and current calculations rather than requiring debug logging.

A broader job explanation is available for a live job by name or identity. It summarizes the job's schedule and next occurrence, its current eligibility and daemon availability, its most recent run of any state, and its most recent anomalous terminal run. An anomalous run is any terminal run whose final state is not successful, including failure, timeout, cancellation, overlap or concurrency skip, and interrupted-unknown outcomes. If the latest run is also the latest anomaly, both sections may identify the same durable run. Missing run history or missing anomaly history is stated explicitly.

The consolidated explanation orders runs by durable request time and identity, carries canonical run identities, trigger and nominal-time facts, timing and duration when known, final state, and a durable terminal reason when one exists. It does not infer machine sleep or another cause that was not recorded. It is a readable summary rather than a replacement for the detailed job and run explanation commands; the run identity lets a user request the full event and attempt trace when needed. Human and machine-readable forms expose the same redacted facts.

Verbose output adds user-facing decision context without changing command behavior. Debug output emits developer-oriented operational traces to standard error. Neither mode may reveal configured environment values, sensitive headers, body content, or other redacted values. Machine-readable standard output remains a single valid result independent of diagnostic verbosity.

The program reports its own version on request through the standard `-V` and `--version` flags, and version output honors the machine-readable output contract. Version reporting requires no state directory or daemon and succeeds without them.

## Human Output Contract

Machine-readable output is the compatibility surface; human output renders the same facts for reading in a terminal. With the human format selected, every command renders one of the following forms, never a machine serialization:

- **Table** — `list` (one row per live job) and `history` (one row per run) render an aligned table with a header line and one left-aligned row per record in the command's documented order. The header prints even when no record exists. Identifiers may be abbreviated inside the table only; copyable output always carries the full identity.
- **Table width** — on a terminal, when a table would exceed the terminal width, the table truncates the last data column whose values are unbounded in length, marks the truncation with a trailing ellipsis, and keeps every other column intact. When standard output is redirected or piped, no truncation occurs and every value prints in full, so scripts and automation always receive complete values. A rendering flag restores full-width table values on a terminal, and the dedicated detail report for a job always presents the complete definition. Column fitting uses character display width, never byte length. This contract changes the human `list` table only; machine-readable output is unaffected.
- **Confirmation lines** — commands that change state (`add`, `update`, `enable`, `disable`, `remove`, `run`, `cancel`, `config set`, `config unset`, `import`, `prune`) print one or more short lines naming the affected identity, the action taken, and the resulting facts that matter, such as the new run identity or the affected counts. A dry run states explicitly that nothing changed.
- **Report** — `show`, `why`, `explain`, and `doctor` render labeled sections: one field per line, grouped under short section headers, so an explanation reads top to bottom. Unknown facts are stated as unknown rather than inferred. The consolidated job explanation keeps current scheduling facts, latest-run facts, and latest-anomaly facts in distinct sections.
- **Value list** — `preview` prints a context line naming the schedule followed by one occurrence per line.
- **Bare document** — `export` keeps the existing bare export document, suitable for redirection.
- **Streams** — `logs`, `run --wait`, and the daemon keep their streamed output forms.

All human forms honor the existing redaction rules: no configured environment value, sensitive header, body content, or other redacted value appears in any rendering, at any verbosity. Human output for these commands never presents escaped JSON strings, nested objects, or arrays — those are machine forms. Verbose and debug context go to standard error and never change the facts standard output carries.

## Export and Import Semantics

Export produces one typed, versioned document containing global settings and the selected jobs' normalized current definitions. Import applies a complete document atomically under the validation, redaction, plaintext-acknowledgement, resolution, and rollback rules defined for the export and import commands.

A user can export the complete job set or a chosen subset:

- Without an explicit selection, export follows the invocation context. In an interactive terminal, a selection interface lists every job, initially all selected, and lets the user choose which jobs to include. In a non-interactive context — no terminal, output redirected or piped, or an environment that declares the invocation non-interactive — export includes every job without prompting.
- An explicit selection by job name or tag exports exactly the matching jobs and never prompts, in any context. A selection that matches no job is rejected before any output is produced.
- The selection interface renders outside standard output. In every mode standard output carries only the export document, so redirection and machine-readable output remain single valid results. Machine-readable output never presents a selection interface and, without an explicit selection, exports the complete job set.

Import accepts a local path or an absolute HTTP or HTTPS URL and treats both identically after the document is obtained. Import never prompts; a dry run reports exactly which jobs and settings an import would create, update, or leave unchanged without changing durable state. HTTPS fetches use mandatory TLS certificate verification.

An export document describes executable schedules. Importing a document registers work that may run on this machine, whether it arrives as a file or a URL, and carries the same trust boundary as installing a script obtained from the same source. The complete document is validated before any write, and no import can partially apply.

## Retention Semantics

Terminal run metadata is retained for 90 days by default, subject to a secondary cap of 1,000 runs per job and 10,000 runs globally. The first exceeded age or count bound evicts the oldest eligible metadata. Active runs are never pruned, and removing a job, including automatic `--delete-after-run` removal, does not bypass normal retention.

Captured process output and HTTP response bodies are retained for 30 days by default, capped at 10 MiB per run and 256 MiB globally. Per-run overflow does not terminate the target; capture stops with an explicit truncation marker and discarded-byte accounting. Global overflow removes the oldest terminal-run output first while preserving run metadata and the fact and time of pruning.

Cleanup occurs at scheduler startup and periodically in bounded batches. Retention bounds are configurable, important output can be exported, explicit pruning is supported, and diagnostics expose current usage, last cleanup, and truncation or pruning counts.

## Safety and Reliability Constraints

- Exactly-once side effects cannot be promised for arbitrary external commands or HTTP endpoints.
- Duplicate creation of the same scheduled occurrence must be prevented within local durable state.
- Unknown outcomes after a crash must not be retried automatically by default.
- Automatic retry defaults to disabled.
- Overlapping execution defaults to disabled.
- Catch-up behavior must be bounded so waking a machine cannot create an unlimited execution storm.
- Sensitive values must not appear in normal list, inspect, or log output.
- Removing a job must not silently erase its execution history.

## Open Questions

None. Implementation choices and their trade-offs are recorded separately from this frozen product specification.
