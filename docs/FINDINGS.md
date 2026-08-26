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
- `locron`: the `locron` binary, command parsing, human/JSON rendering, and daemon entrypoint.

Keep one distributable binary in v1 even though its code is split into libraries. Do not create empty crates for later HTTP management, MCP, or desktop surfaces; add those only when their milestones begin. Enforce the following dependency shape: `locron` composes `locron-engine` and `locron-store`; both depend on `locron-core`; `locron-engine` does not depend on the SQLite implementation; and no library depends back on the CLI.

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

## 11. One-line Installer and Self-update (2026-08-23)

### Evidence

- The mise.run standalone installer is a ~370-line POSIX `sh` script that never calls the GitHub API. The current version and its checksums are baked into the script at release time; a pinned `MISE_VERSION` downloads `https://github.com/jdx/mise/releases/download/v${version}/SHASUMS256.txt`, greps the target line, regex-validates it as hex, and verifies with `shasum -c`. Default install path is `${MISE_INSTALL_PATH:-$HOME/.local/bin/mise}` — no root, and no shell-config modification (it prints per-shell `activate` guidance to stderr instead). Its replace step is `rm -f` then `mv` — explicitly non-atomic. Sources: https://mise.run, https://github.com/jdx/mise/blob/main/packaging/standalone/install.envsubst
- mise's `self-update` resolves latest through the `self_update` crate's GitHub backend at `https://api.github.com/repos/{owner}/{repo}/releases/latest` (unauthenticated limit 60 requests/hour per IP, verified live), verifies the archive against a zipsign signature with the public key compiled into the binary, and replaces the running binary via the `self-replace` crate: create a temp file in the same directory as the executable, copy the new binary in, preserve permissions, then one `fs::rename` — atomic on macOS and Linux. In-place writes are impossible (`ETXTBSY`), but rename over an open executable works; a live experiment confirmed the old process keeps its inode and the next invocation runs the new content. Source: https://github.com/mitsuhiko/self-replace/blob/main/src/unix.rs
- mise detects a package-manager-managed install not by path sniffing but with a marker file: the homebrew-core formula touches `lib/.disable-self-update` at install time, and `self-update` refuses with "mise is installed via a package manager, cannot update" when the marker exists. All download/verify failures happen before any file move, so the old binary keeps working. Source: https://github.com/jdx/mise/blob/main/src/cli/self_update.rs, https://github.com/Homebrew/homebrew-core/blob/master/Formula/m/mise.rb
- rustup's one-liner (`rustup-init.sh`) does no sha256 check of the bootstrap binary and instead hardens transport (TLS 1.2+, pinned cipher suites), installs to `$HOME/.cargo/bin` without root, is safe to re-run as a repair/update, and supports `RUSTUP_VERSION` plus `RUSTUP_UPDATE_ROOT`-style override variables — the latter is the established test seam for installer pipelines. Source: https://sh.rustup.rs
- GitHub's `releases/latest` REST endpoint is rate-limited (60/hour unauthenticated), which breaks curl|sh installers behind shared NATs. The API-free alternative is the static redirect `https://github.com/{owner}/{repo}/releases/latest/download/{asset}`, which 302s to a signed asset URL with no API involvement — this is what starship's install.sh uses for `VERSION=latest`. Sources: https://docs.github.com/en/rest/using-the-rest-api/rate-limits-for-the-rest-api, https://github.com/starship/starship/blob/master/install/install.sh
- Checksum practice among shipping one-liners is split: mise verifies (baked-in or `SHASUMS256.txt`), starship and the rustup bootstrap do not. The SHA256SUMS file itself is unsigned in every case; trust rests on HTTPS to the same origin. Source: table in research report §5.
- macOS rename preserves the source file's xattrs, so a quarantine xattr would carry onto the installed binary — in practice curl/wget do not set quarantine. SIP protects `/System` and `/usr` but not `/usr/local`. Source: research report §6.

### Recommendation

- Ship an evergreen POSIX `sh` installer that resolves "latest" through `releases/latest/download/{asset}` redirects (no API, no rate limit), fetches `SHA256SUMS.txt` from the same release, greps the target line, validates the hex form, and verifies before installing. Trust level equals mise's pinned-version path; no per-release script regeneration is needed, unlike mise's baked-checksum approach.
- Default install to `$HOME/.local/bin`, `LOCRON_INSTALL_DIR` override, no shell-config modification (print PATH guidance). Re-running replaces the binary with the latest release — the update path. `LOCRON_VERSION` pins.
- Make the replace step atomic (temp file in the install directory + `rename`), correcting mise.run's `rm`+`mv` weakness.
- Serve the one-liner as a release asset (`releases/latest/download/install.sh`) so the script and binaries are version-consistent; the repo copy is the source of truth and the release pipeline attaches it.
- Implement `locron self-update` in the CLI against the GitHub API (explicit user action; the 60/hour limit is acceptable and must produce an actionable error), with sha256 verification, atomic self-replace via standard-library `rename`, and package-manager refusal through the mise-style marker file that our own tap formula creates. Add `LOCRON_UPDATE_*` base-URL overrides as the rustup-style test seam.

### Alternatives and Trade-offs

- Baking the current version and checksums into the script at release time (mise) would add an envsubst/regeneration step to the release pipeline and make the `main`-hosted copy stale between releases, for no trust gain — the script and the checksums both come from github.com over HTTPS either way.
- Using the GitHub API inside the installer was rejected: 60 requests/hour unauthenticated makes the happy path fail in CI and corporate NATs, while the static redirect serves the same need without a limit.
- The `self_update`/`self-replace` crates were considered and rejected: the required mechanics are one temp-file-plus-`fs::rename` on a Unix-only platform set, and three pure-Rust deps (`tar`, `flate2`, `sha2`) for extraction and hashing cost less surface than the update crates.
- Path sniffing (`Cellar` component) to detect brew installs was rejected in favor of the marker file: the marker is explicit and packager-controlled, and path heuristics break under relocated prefixes.

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

## 12. Daemon Service Installation (2026-08-23)

### Evidence

**The canonical home for per-user launchd agents is `~/Library/LaunchAgents` — launchd.plist(5) lists it in FILES as "Per-user agents provided by the user"**, alongside `/Library/LaunchAgents` (administrator-provided per-user agents) and the LaunchDaemons directories. Source: https://github.com/apple-oss-distributions/launchd/blob/main/man/launchd.plist.5

**The modern launchctl interface is `bootstrap`/`bootout`, and `load`/`unload` are legacy.** The current launchctl(1) groups load/unload under a "LEGACY SUBCOMMANDS" heading with "Recommended alternative subcommands: bootstrap | bootout | enable | disable", and notes load/unload "will only return a non-zero exit code due to improper usage. Otherwise, zero is always returned" — meaning scripted load/unload cannot detect real failure. Bootstrap and bootout take a domain target, where `gui/<uid>` targets "the user-login domain by user rather than audit session ID" and is "generally more convenient"; a single service is addressed as `gui/<uid>/<label>`. Source: https://keith.github.io/xcode-man-pages/launchctl.1.html

**Homebrew's `brew services` implementation is a working reference for the canonical registration sequence.** Start writes the plist into the user service directory and then runs `launchctl enable gui/<uid>/<label>` followed by `launchctl bootstrap gui/<uid> <plist>`; stop unregisters with `launchctl bootout gui/<uid>/<label>`; on Linux it drives `systemctl start`/`enable`/`disable`. Sources: https://github.com/Homebrew/brew/blob/master/Library/Homebrew/services/cli.rb and https://github.com/Homebrew/brew/blob/master/Library/Homebrew/services/system.rb

**LaunchAgent plists a user loads must be owned by that user and must not be group- or world-writable** — stated under the legacy load/unload section: "per-user configuration files (LaunchAgents) must be owned by root (if they are located in `/Library/LaunchAgents`) or the user loading them (if they are located in `$HOME/Library/LaunchAgents`)", and configuration files "must disallow group and world writes". The same man page does not restate these rules for `bootstrap`, so an installer should satisfy them regardless. Source: https://keith.github.io/xcode-man-pages/launchctl.1.html

**KeepAlive and RunAtLoad both default to false.** KeepAlive "controls whether your job is to be kept continuously running or to let demand and conditions control the invocation"; RunAtLoad "is used to control whether your job is launched once at the time the job is loaded". A supervised daemon therefore needs both, and launchd also throttles restarts: "by default, jobs will not be spawned more than once every 10 seconds" (ThrottleInterval default). Source: https://github.com/apple-oss-distributions/launchd/blob/main/man/launchd.plist.5

**StandardOutPath and StandardErrorPath name the files that receive the job's stdout and stderr.** The man page documents no permissions or security note for these keys. Source: https://github.com/apple-oss-distributions/launchd/blob/main/man/launchd.plist.5

**What happens when the ProgramArguments binary is atomically replaced while the job runs is not documented in launchd.plist(5)** — the running process keeps its old inode and the next launch executes the new file is the general Unix behavior already established in §11 (self-replace live experiment) and cross-referenced by the installer section of `docs/IMPLEMENTATION.md`; no launchd-specific source exists, so this is cross-referenced rather than re-sourced here. Sources: §11 above and https://github.com/mitsuhiko/self-replace/blob/main/src/unix.rs

**User systemd units live at `~/.config/systemd/user`, which systemd.unit(5) lists as "User configuration ($XDG_CONFIG_HOME is used if set, ~/.config otherwise)"** in the user unit search path. Source: https://man7.org/linux/man-pages/man5/systemd.unit.5.html

**`default.target` is the correct [Install] target for a user unit: it is "the main target of the user service manager, started by default when the service manager is invoked"** — unlike the system instance's default.target (a boot alias), the user one is a real unit. Source: https://man7.org/linux/man-pages/man7/systemd.special.7.html

**For a long-running daemon, `Restart=on-failure` is what systemd.service(5) itself calls "the recommended choice for long-running services"**; RestartSec "Defaults to 100ms". Source: https://man7.org/linux/man-pages/man5/systemd.service.5.html

**`systemctl enable` alone does not start the unit**: it "will create a set of symlinks, as encoded in the [Install] sections", and "this does not have the effect of also starting any of the units being enabled. If this is desired, combine this command with the --now switch"; `daemon-reload` is needed after writing a new unit file ("reload all unit files, and recreate the entire dependency tree"). The established sequence is therefore write unit, `systemctl --user daemon-reload`, `systemctl --user enable --now`. Sources: https://man7.org/linux/man-pages/man1/systemctl.1.html and https://man.archlinux.org/man/systemctl.1.en

**The user manager and all user units stop at logout.** pam_systemd(8) documents that "An instance of the system service user@.service, which runs the systemd user manager instance, is started" with the first concurrent session, and "If the last concurrent session of a user ends, the user's systemd instance will be terminated too"; XDG_RUNTIME_DIR is "automatically created the first time a user logs in and removed on the user's final logout". Lingering is the documented exception: loginctl(1) enable-linger means "a user manager is spawned for the user at boot and kept around after logouts" and "allows users who are not logged in to run long-running services". Sources: https://man7.org/linux/man-pages/man8/pam_systemd.8.html and https://man7.org/linux/man-pages/man1/loginctl.1.html

**`systemctl --user` connectivity depends on session-scoped variables, which is the correct detection hook.** sd_bus_open_user(3): "If the $DBUS_SESSION_BUS_ADDRESS environment variable is set, it will be used as the address of the user bus. If this variable is not set, a suitable default for the default user D-Bus instance will be used", and the connection fails with -ENOMEDIUM "when the user session bus is not available because $XDG_RUNTIME_DIR is not set". So a shell installer detects a usable user manager by a non-empty XDG_RUNTIME_DIR with a reachable user bus, and contexts without a login session (SSH non-interactive, cron, containers) fail the `systemctl --user` path. Sources: https://man.archlinux.org/man/sd_bus_open_user.3.en and https://man7.org/linux/man-pages/man8/pam_systemd.8.html

**A user may enable lingering for themselves without authentication.** systemd's login1 polkit policy grants `org.freedesktop.login1.set-self-linger` (description "Allow non-logged-in user to run programs") with default allow_any/allow_inactive/allow_active all `yes`, while `org.freedesktop.login1.set-user-linger` requires `auth_admin_keep` in all contexts; logind-dbus.c selects between them with `uid == auth_uid ? "org.freedesktop.login1.set-self-linger" : "org.freedesktop.login1.set-user-linger"`. Plain `loginctl enable-linger` therefore needs no password for one's own user; enabling it for another user needs administrator authentication. Sources: https://github.com/systemd/systemd/blob/main/src/login/org.freedesktop.login1.policy and https://github.com/systemd/systemd/blob/main/src/login/logind-dbus.c

**Homebrew's service contract, as documented in the brew man page:** `brew services start` "Start[s] the service formula immediately and register[s] it to launch at login (or boot)"; `run` runs it "without registering to launch at login (or boot)"; `stop` stops and unregisters "unless --keep is specified"; and non-root operation "operate[s] on `~/Library/LaunchAgents` or `~/.config/systemd/user` (started at login)". Source: https://github.com/Homebrew/brew/blob/master/docs/Manpage.md

**A formula declares a service with a `service` block: "There are two ways to add `launchd` plists and `systemd` services to a formula, so that `brew services` can pick them up"** (ship a package-provided file via `name`, or generate one from block options); "The `run` or `name` field must be defined", and the block "instructs Homebrew to create a service description file using options set in the block" — `keep_alive` (default false), `working_dir`, `log_path`, `error_log_path`, `require_root`, `environment_variables`, `restart_delay`, `throttle_interval`, `process_type`, and `run_at_load` among the supported keys. Source: https://docs.brew.sh/Formula-Cookbook

**`brew upgrade` does not restart running services.** Source inspection of Homebrew/brew shows no service-restart logic in the upgrade path: `upgrade.rb` and `install.rb` contain no reference to services, launchd, or systemd, and `formula_installer.rb` only writes refreshed service files at install/upgrade time (`install_service`, invoked from `finish`) without stopping or restarting anything; the man page is consistent ("Changes take effect on the next `brew services restart` and persist across upgrades", of per-service env files). A running service therefore keeps executing the old binary until the user runs `brew services restart` — the package-manager equivalent of "old code until restart". Sources: https://github.com/Homebrew/brew/blob/master/Library/Homebrew/upgrade.rb, https://github.com/Homebrew/brew/blob/master/Library/Homebrew/install.rb, https://github.com/Homebrew/brew/blob/master/Library/Homebrew/formula_installer.rb

**The Homebrew services implementation names launchd services `homebrew.mxcl.<name>` and systemd units `homebrew.<name>.service`** ("Check if a launchd service is running, given its label (e.g. `homebrew.mxcl.foo`)"; the CLI label regex is `homebrew(?>\.mxcl)?\.([\w+-.@]+)` with a `.service` suffix stripped for systemd). Sources: https://github.com/Homebrew/brew/blob/master/Library/Homebrew/services/system.rb and https://github.com/Homebrew/brew/blob/master/Library/Homebrew/services/cli.rb

**The caveats convention "To start X now and restart at login: brew services start X" / "Or, if you don't want/need a background service you can just run: ..." is the historical Homebrew phrasing** that the man page semantics above replaced in practice: current homebrew-core formulae with `service` blocks (postgresql@17, redis, nginx, mysql, rabbitmq, elasticsearch) no longer print it, and the current Formula Cookbook does not prescribe it, but third-party tooling still emits the exact text. Source: https://github.com/act3-ai/hops/blob/v0.1.0-beta.4/internal/pretty/caveats.go

**The formula template embedded in `.github/workflows/release.yml` currently contains no `service` block** — only class/desc/version, per-arch url+sha256, `bin.install "locron"`, and a `--version` test — so Homebrew service support requires adding a `service do` block (with per-platform `run` and `keep_alive`) to that generated formula, separately from the already-planned `touch lib/.disable-self-update` marker line. Source: https://github.com/WhiteKiwi/locron/blob/main/.github/workflows/release.yml

**Ollama's Linux installer does not create a systemd user unit: it creates a system (root) unit and a dedicated system user.** `scripts/install.sh` runs `useradd -r` for an `ollama` user and `$SUDO tee /etc/systemd/system/ollama.service` with `ExecStart=$BINDIR/ollama serve`, `User=ollama`, `Restart=always`, `RestartSec=3`, and `[Install] WantedBy=default.target`; when `systemctl is-system-running` reports `running|degraded` it prints "Enabling and starting ollama service...", runs `$SUDO systemctl daemon-reload` and `$SUDO systemctl enable ollama`, and traps `$SUDO systemctl restart ollama` on exit. On macOS it runs `open -a Ollama --args hidden` unless `OLLAMA_NO_START`. No user units, no lingering, no login items anywhere in the script. Source: https://github.com/ollama/ollama/blob/main/scripts/install.sh

**Ollama's Linux docs likewise present only a root-run system service** ("Adding Ollama as a startup service (recommended)"): a unit at `/etc/systemd/system/ollama.service` plus `sudo systemctl daemon-reload`, `sudo systemctl enable ollama`, `sudo systemctl start ollama`; the docs contain no user-session or lingering guidance. Source: https://github.com/ollama/ollama/blob/main/docs/linux.mdx

**The Debian convention is that package upgrades restart the running service.** dh_installsystemd(1): "Do not stop the unit file until after the package upgrade has been completed. This is the default behaviour in compat 10" (`--restart-after-upgrade`); `--no-restart-after-upgrade` "will cause the service to be stopped in the _prerm_ script and started again in the _postinst_ script"; `-r`/`--no-stop-on-upgrade` "Do not stop service on upgrade. This has the side-effect of not restarting the service as a part of the upgrade". Units are also enabled on install by default. Sources: https://man7.org/linux/man-pages/man1/dh_installsystemd.1.html

**The maintainer-script mechanism is `deb-systemd-invoke`, "a wrapper around systemctl, respecting policy-rc.d"** that "asks /usr/sbin/policy-rc.d before performing a systemctl call" and "is intended to be used from maintscripts to manage systemd unit files". Source: https://manpages.debian.org/trixie/init-system-helpers/deb-systemd-invoke.1p.en.html

### Recommendation

- macOS registration: write a user-owned plist (not group- or world-writable) to `~/Library/LaunchAgents`, then `launchctl enable gui/<uid>/<label>` and `launchctl bootstrap gui/<uid> <plist>`; unregistration is `launchctl bootout gui/<uid>/<label>`. Set RunAtLoad and KeepAlive both true; point StandardOutPath/StandardErrorPath at locron-owned log files. Never use the legacy `load`/`unload` in new code, because they cannot signal real failure.
- Linux registration: write the unit to `~/.config/systemd/user`, then `systemctl --user daemon-reload` and `systemctl --user enable --now`. Use `Restart=on-failure` with a short `RestartSec` and `[Install] WantedBy=default.target`. This stops at logout by design (pam_systemd terminates the user manager with the last session); the guidance fallback must therefore appear whenever XDG_RUNTIME_DIR or the user bus is unavailable (SSH/cron/container contexts), and the optional logout-survival operator step is the user's own unauthenticated `loginctl enable-linger`.
- Updates: re-writing the same plist/unit refreshes registration; the daemon restart after a binary update uses the ordinary graceful-shutdown path (`bootout`+`bootstrap` on macOS, `systemctl --user restart` on Linux). The old binary keeps running until then — already established in §11 and `docs/IMPLEMENTATION.md`'s installer section.
- Homebrew: add a `service do` block (run + keep_alive, optionally log_path/error_log_path) to the formula template in `.github/workflows/release.yml`; installation never starts it (`brew services start` is the user's action), and `brew upgrade` refreshes the definition without restarting, so post-upgrade guidance ("restart via `brew services restart`") belongs in the formula caveats.
- deb/rpm: never register (per the specification) and always print the guidance; the Debian auto-restart-on-upgrade convention is moot when nothing is registered, and no auto-start is possible at package-install time anyway because the user session that would own the service does not exist yet.

### Alternatives and Trade-offs

- The ollama model — a root-owned systemd system unit with a dedicated system user, enabled and started by the installer — is rejected: it requires root, creates a system user, and runs the service outside any user session. Ollama's docs treat "install" and "start at boot" as separate root steps, which is exactly the split the locron specification avoids.
- Using `launchctl load`/`unload` for registration is rejected: the man page demotes them to legacy and documents that they cannot fail on improper usage, making idempotent registration/removal untestable.
- Auto-enabling lingering at install time is rejected: the specification makes stop-at-logout the default and lingering the documented optional operator step, and a user needs no authentication to enable it themselves.
- Debian-style auto-registration plus restart-on-upgrade for deb/rpm was rejected in the specification (guidance only); the Debian policy mechanism itself (deb-systemd-invoke with policy-rc.d) is what a later, root-capable package could adopt, not this milestone.
- Homebrew caveats carrying the full "To start X now and restart at login" text is not needed because the `service` block makes `brew services` discover the service; guidance on the package-manager update flow remains useful.

## 13. Usage and Installation Measurement (2026-08-23)

### Evidence

**GitHub REST exposes a cumulative `download_count` per release asset**, summed over the releases list endpoint (`GET /repos/{owner}/{repo}/releases?per_page=100`). Observed 2026-08-23: nine assets per release and per-asset counts present; the grand total moved from 22 to 32 during the day as v0.3.0 was published and v0.2.0's assets were re-uploaded (which resets that release's counts). The count resets when an asset is deleted and re-uploaded, and deleted releases disappear from the list, so the number is a floor rather than an exact ledger. Rate limits are 60 requests/hour unauthenticated and 5,000 authenticated — already established in §11. Source: https://docs.github.com/en/rest/releases/releases

**GitHub traffic metrics (`/repos/{owner}/{repo}/traffic/views` and `/clones`) are owner-only and cover the last 14 days.** `gh api` works without extra scopes because the maintainer account owns the repository; observed 2026-08-23: 77 views/5 uniques, 187 clones/39 uniques. Source: https://docs.github.com/en/rest/metrics/traffic

**locron is not published on crates.io** — the API answers `crate locron does not exist` (checked 2026-08-23). The check itself is policy-relevant: crates.io's data-access policy requires a descriptive User-Agent, and the default curl UA is refused (observed: a data-access-policy error without one, the authoritative answer with one). If the crate is ever published, `/api/v1/crates/{crate}` carries the all-time `downloads` total, while `/api/v1/crates/{crate}/downloads` is a trailing-90-day per-day and per-version series (verified against a published crate on 2026-08-23). Sources: https://crates.io/data-access and https://crates.io/api/v1/crates/locron

**Homebrew analytics are public JSON on formulae.brew.sh and include tap formulae under their fully-qualified name.** The install endpoints (`/api/analytics/install/30d.json`, also 90d/365d) list third-party taps as `user/tap/formula` — observed samples include `hashicorp/tap/terraform` and `wix/brew/applesimutils` — so locron's entry is `whitekiwi/tap/locron` (tap repository `whitekiwi/homebrew-tap`, installed with `brew tap whitekiwi/tap`). As of 2026-08-23 no `whitekiwi/tap/locron` entry exists in the 30/90/365-day data: a formula with zero recorded installs simply has no entry, which the tooling must render as zero. Analytics are anonymous and opt-out (`HOMEBREW_NO_ANALYTICS=1`), so any count understates real installs. Source: https://formulae.brew.sh/api/analytics/install/30d.json

**The repository endpoint carries the star count** (`GET /repos/{owner}/{repo}` → `stargazers_count`), public without authentication. Source: https://docs.github.com/en/rest/repos/repos

### Recommendation

A maintainer-facing POSIX `sh` script, `scripts/usage.sh`, that prints one snapshot per invocation: per-release and total GitHub asset downloads, stars, Homebrew 30/90/365-day install counts (absent entry rendered as zero), crates.io downloads when published (`N/A` while unpublished), and — only when `gh` is present and authenticated — repository views/clones for the last 14 days. A `--json` flag emits the same snapshot as one flat JSON object so a later scheduled snapshot job is a drop-in. The script only reads; no data leaves the machine and no telemetry is added anywhere.

### Alternatives and Trade-offs

- **A scheduled GitHub Action snapshotting numbers into the repository** is deferred: daily commits add repository noise and another write credential for marginal value at the current scale; the `--json` mode keeps it a drop-in when measurement history becomes worth that noise.
- **Publishing to crates.io for measurement purposes** is rejected as a reason to publish: registry metadata and a publishing workflow are real surface, and the existing GitHub Releases, Homebrew, and deb/rpm channels already cover distribution.
- **An in-product `locron stats` command** aggregating durable run history is a product-behavior
  change and therefore requires its own SPEC amendment; it is preserved in `docs/BACKLOG.md`
  instead of being folded into this tooling.

## 14. Web Administration (2026-08-23)

### Evidence

**Jupyter Server binds loopback by default and treats a generated token as the authentication boundary, printing the token in the access URL.** The public-server docs state "By default, Jupyter Server runs locally at 127.0.0.1:8888 and is accessible only from `localhost`", remote access requires changing the bind (`--ip=0.0.0.0` / `c.ServerApp.ip = '*'`), and the security docs show the startup log printing `http://localhost:8888/?token=...`. The token is generated as `binascii.hexlify(os.urandom(24))` when none is configured, and the docs document three ways to present it: "in the `Authorization` header, e.g.: `Authorization: token abcdef...`", "In a URL parameter", and "In the password field of the login form"; "Once you have visited this URL, a cookie will be set in your browser and you won't need to use the token again, unless you switch browsers, clear your cookies, or start a Jupyter server on a new port." Sources: https://jupyter-server.readthedocs.io/en/latest/operators/public-server.html, https://jupyter-server.readthedocs.io/en/latest/operators/security.html, https://github.com/jupyter-server/jupyter_server/blob/main/jupyter_server/auth/identity.py

**Jupyter persists server facts — including the plaintext token — in a runtime JSON file, and validates the Host header against loopback names, explicitly naming DNS rebinding as the threat.** `write_server_info_file` writes `server_info()` (url, port, token, pid) to `jpserver-<pid>.json` with `secure_write`; the `allow_remote_access` option documents: "By default, requests get a 403 forbidden response if the 'Host' header shows that the browser thinks it's on a non-local domain... This protects against 'DNS rebinding' attacks... Local IP addresses (such as 127.0.0.1 and ::1) are allowed as local, along with hostnames configured in local_hostnames", whose default is `["localhost"]`. Source: https://github.com/jupyter-server/jupyter_server/blob/main/jupyter_server/serverapp.py

**Caddy's admin API defaults to localhost:2019, has no authentication, and defends with Host/Origin validation.** The `AdminConfig` documentation: Listen — "Default: the value of the `CADDY_ADMIN` environment variable, or `localhost:2019` otherwise"; EnforceOrigin — "If true, CORS headers will be emitted, and requests to the API will be rejected if their `Host` and `Origin` headers do not match the expected value(s)"; Origins — "If not set, the listener address will be the default value. If set but empty, no origins will be allowed"; enforcement is "only on local (plaintext) endpoint". Sources: https://pkg.go.dev/github.com/caddyserver/caddy/v2#AdminConfig, https://caddyserver.com/docs/json/admin/, https://caddy.community/t/2-5-0-enforce-origin-bug/15764/8

**Host-header allowlisting is the canonical DNS-rebinding defense for plain-HTTP local services; Origin checks are complementary, not a substitute.** host-validation-middleware: "Middleware for validating host headers in requests to protect against DNS rebinding attacks", 403 for a Host outside the allowlist, always allowing `localhost` and subdomains of it plus "Any IPv4 or IPv6 address (e.g., `127.0.0.1`, `[::1]`)", and "DNS rebinding attacks are not effective against HTTPS sites" — i.e. plain-HTTP local services are the target. A real-world case: dbhub's missing Host validation enabled a full DNS-rebinding exploit (CVE-2025-66414). Sources: https://github.com/sapphi-red/host-validation-middleware, https://github.com/bytebase/dbhub/issues/304

**Jenkins persists a random initial admin password in a secrets file and requires a double-submit "crumb" for POSTs, exempting API-token-authenticated requests.** Official install docs print the password via `cat /var/lib/jenkins/secrets/initialAdminPassword` (Docker: `/var/jenkins_home/secrets/initialAdminPassword`); the CSRF doc: "Requests sent using the POST method are subject to CSRF protection in Jenkins and generally need to provide a crumb", while "Requests authenticating with an API token are exempt from CSRF protection in Jenkins." Sources: https://www.jenkins.io/doc/book/installing/macos/, https://www.jenkins.io/doc/book/security/csrf-protection/

**Django — healthchecks.io's framework — uses the double-submit CSRF cookie on all unsafe methods with SameSite=Lax cookies; healthchecks' live pages poll rather than stream.** Django: "For all incoming requests that are not using HTTP GET, HEAD, OPTIONS or TRACE, a CSRF cookie must be present, and the 'csrfmiddlewaretoken' field must be present and correct"; `CSRF_COOKIE_SAMESITE` default `'Lax'`; healthchecks' log page refreshes via `adaptiveSetInterval(fetchNewEvents, false)` — an unconditional 60-second poll with adaptive 3-second bursts while the tab is visible — and `SITE_ROOT` defaults to `http://localhost:8000` with `ALLOWED_HOSTS` derived from it. Sources: https://docs.djangoproject.com/en/5.1/ref/csrf/, https://docs.djangoproject.com/en/5.1/ref/settings/, https://github.com/healthchecks/healthchecks/blob/master/static/js/adaptive-setinterval.js, https://github.com/healthchecks/healthchecks/blob/master/static/js/log.js, https://github.com/healthchecks/healthchecks/blob/master/hc/settings.py

**Tornado's XSRF — the mechanism Jupyter uses — is the same double-submit model: an `_xsrf` cookie whose value must be echoed in a form field or `X-XSRFToken` header on POST/PUT/DELETE.** "the Tornado web application will set the `_xsrf` cookie for all users" and "reject all `POST`, `PUT`, and `DELETE` requests that do not contain a correct `_xsrf` value"; "if you support both cookie and non-cookie-based authentication, it is important that XSRF protection be used whenever the current request is authenticated with a cookie." Source: https://www.tornadoweb.org/en/stable/guide/security.html

**The unix-socket transport model (Docker, Podman) is the strongest "no network exposure without explicit opt-in" precedent.** Docker: "By default, a unix domain socket (or IPC socket) is created at /var/run/docker.sock", "Changing the default docker daemon binding to a TCP port or Unix docker user group introduces security risks", and "It is conventional to use port 2375 for un-encrypted, and port 2376 for encrypted communication with the daemon." Podman `system service`: default endpoints are `unix:///run/podman/podman.sock` (rootful) and `unix://$XDG_RUNTIME_DIR/podman/podman.sock` (rootless), TCP requires an explicit endpoint URI, "the API grants full access to all Podman functionality... and thus allows arbitrary code execution as the user running the API", and "We *strongly* recommend against making the API socket available via the network... without enabling mutual TLS to authenticate the client". Sources: https://docs.docker.com/reference/cli/dockerd/, https://docs.podman.io/en/latest/markdown/podman-system-service.1.html

**Default binds among local web tools split into loopback-first and all-interfaces-with-auth-or-warnings.** Loopback-first: Jupyter 127.0.0.1:8888; crontab-ui `app.set('host', process.env.HOST || '127.0.0.1')` on port 8000 with no authentication by default (optional `BASIC_AUTH_USER`/`BASIC_AUTH_PWD`); FileBrowser `--address` default `127.0.0.1` port 8080 plus README warning "Do not expose it directly to the internet"; code-server `--bind-addr` default `127.0.0.1:8080` with `auth: password` and a random password stored in `~/.config/code-server/config.yaml`; VS Code `code serve-web` reported to default to localhost:8000 with the connection token in the printed URL (`?tkn=`) — consistent across the microsoft/vscode CLI PR and secondary sources, but not re-verified from primary source this session; Vite `host` default `localhost`. All-interfaces: Grafana binds 0.0.0.0:3000 ("An empty value is equivalent to setting the value to `0.0.0.0`, which means the Grafana service binds to all interfaces") with default `admin`/`admin` login; Jenkins binds 0.0.0.0:8080 (packaged config: "The default is 0.0.0.0 which means it is listening on all available interfaces"); Transmission `rpc_bind_address` default "0.0.0.0" on 9091 with `rpc_authentication_required` default false but a loopback-only `rpc_whitelist` ("127.0.0.1") enabled by default; OliveTin's sample config binds 0.0.0.0:1337 with guest access by default (`authRequireGuestsToLogin: false`); Cronicle's docs use port 3012; Uptime Kuma "is now running on all network interfaces" on 3001. No surveyed tool refuses a non-loopback bind; the strictest norms are loopback-default-with-opt-in and warning-laden all-interfaces. Sources: https://github.com/jupyter-server/jupyter_server/blob/main/jupyter_server/serverapp.py, https://github.com/alseambusher/crontab-ui/blob/main/app.js, https://github.com/alseambusher/crontab-ui/blob/main/README.md, https://github.com/filebrowser/filebrowser/blob/master/cmd/root.go, https://github.com/filebrowser/filebrowser/blob/master/README.md, https://linuxcommandlibrary.com/man/code-server, https://coder.com/docs/code-server/FAQ, https://github.com/microsoft/vscode/pull/207932, https://vite.dev/config/server-options.html, https://grafana.com/docs/grafana/latest/setup-grafana/configure-grafana/, https://build.opensuse.org/projects/devel:tools:building/packages/jenkins/files/jenkins.sysconfig, https://github.com/transmission/transmission/blob/main/docs/Editing-Configuration-Files.md, https://github.com/OliveTin/OliveTin/blob/main/config.yaml, https://github.com/jhuckaby/Cronicle/blob/master/docs/Configuration.md, https://github.com/louislam/uptime-kuma/blob/master/README.md

**One-way output streaming has a documented SSE precedent; tools with bidirectional or multiplexed needs chose WebSocket.** MDN: SSE "is a one-way connection, so you can't send events from a client to a server", uses the `text/event-stream` media type, and "if the connection between the client and server closes, the connection is restarted" (EventSource auto-reconnect with a `retry` field); OpenAI's streaming API is explicitly "HTTP streaming (`stream=true`) over server-sent events (SSE)" with typed lifecycle events and a terminal completion event. WebSocket adopters all had a second direction or multiplexing: Jupyter multiplexes all kernel channels "into one WebSocket", Grafana Live "sends data to clients over persistent WebSocket connections" on a Pub/Sub model (and "checks the Origin request header" against hijacking, capped at 100 connections by default), Uptime Kuma's stated design goal is "Try to use WebSocket with SPA instead of a REST API", and Cronicle's configuration exposes `socket_io_transports`/`web_socket_use_hostnames`. Sources: https://developer.mozilla.org/en-US/docs/Web/API/Server-sent_events/Using_server-sent_events, https://developers.openai.com/api/docs/guides/streaming-responses, https://jupyter-server.readthedocs.io/en/latest/developers/websocket-protocols.html, https://grafana.com/docs/grafana/latest/setup-grafana/set-up-grafana-live/, https://github.com/louislam/uptime-kuma/blob/master/README.md, https://github.com/jhuckaby/Cronicle/blob/master/docs/Configuration.md

**Next-free-port fallback is an established local-tool convention.** Vite: "Note if the port is already being used, Vite will automatically try the next available port", with `server.strictPort` ("Set to `true` to exit if port is already in use, instead of automatically trying the next available port") as the opt-out; Jupyter's `port_retries` defaults to 50 ("The number of additional ports to try if the specified port is not available") and its `random_ports` helper tries sequential candidates first (`port + i` for the first five) and then randomized nearby candidates. Sources: https://vite.dev/config/server-options.html, https://github.com/jupyter-server/jupyter_server/blob/main/jupyter_server/serverapp.py

**The axum stack compiles below locron's MSRV 1.94 — no blocker.** Verified 2026-08-23 from crates.io metadata: axum 0.8.9 `rust-version: 1.80` (docs.rs: "axum's MSRV is 1.80"), tower-http 0.7.0 `rust-version: 1.65`, tokio-stream 0.1.19 `rust-version: 1.71`, hyper 1.11.0 `rust-version: 1.63`. axum's SSE support is built in (`axum::response::sse` with `Sse`, `Event`, `KeepAlive`), so no separate SSE crate is needed. Sources: https://docs.rs/crate/axum/latest, https://docs.rs/axum/latest/axum/response/sse/index.html, https://crates.io/crates/axum, https://crates.io/crates/tower-http, https://crates.io/crates/tokio-stream (rust-version fields verified with `cargo info` on 2026-08-23)

**The accepted default port 10824 is IANA-unassigned and has no observed unofficial use; the earlier candidate 45123 is also unassigned but carries one documented hostile endpoint.** Verified 2026-08-24 by downloading the live IANA service-names registry CSV: neither 10824 nor 45123 appears in it. 10824 sits in the unassigned gap 10811–10859 (nearest assignments 10805 `lpdg` and 10860 `helix`); 45123 sits in the unassigned gap 45055–45184 (nearest assignments 45054 `invision-ag` and 45185 `witsnet`). Both are in the registered range 1024–49151, avoiding both the privileged range and the OS-ephemeral 49152+ range. SpeedGuide documents no service or vulnerability for 10824, and the Dr.Web "Trojan.…10824" database entries use 10824 as an internal record ID with network activity on ports 80 and 1688 — not port usage. For 45123 the only concrete use found is a 2024-05 malware distribution endpoint (`37.55.154.130:45123/bin.sh`, a UPX-packed MIPS ELF, since sinkholed); the "Trojan.…45123" Dr.Web entries are likewise database IDs, but the observed endpoint was enough to prefer 10824. Sources: https://www.iana.org/assignments/service-names-port-numbers/service-names-port-numbers.csv, https://www.speedguide.net/port.php?port=10824, https://www.speedguide.net/port.php?port=45123, https://urlquery.net/report/180059c2-c9e2-4e0d-b38c-20c963c441be

### Recommendation

- **Bind and port.** Bind loopback interfaces only (`127.0.0.1`, `::1`) and refuse any other bind address at startup, as `docs/dashboard/SPEC.md` requires. This is stricter than every surveyed HTTP tool (the norm is loopback-default-with-opt-in, or all-interfaces-with-warnings), but it is the single-user contract, and the refusal is the same principle Docker/Podman apply at the transport level: non-default exposure requires an explicit opt-in, never a silent default. Use fixed default port **10824** — verified unassigned in the IANA registry on 2026-08-24, and preferred over the earlier candidate 45123 which has one documented hostile-use observation (evidence below) — with next-free-port fallback when occupied in foreground mode (Vite/Jupyter precedent), a fixed port in service mode so the bookmarked address never silently moves, and the chosen URL always printed or queryable. Implementation note: Host validation must compare the hostname and ignore the port, because the port can be the fallback value.
- **Token and transport.** Generate a 32-byte random token (hex-encoded, ~64 chars) on first use, store it in an owner-only (0600) file in the state directory, reuse it afterwards, and support explicit regeneration — the durable-token-file precedents are Jupyter's runtime JSON, Jenkins' `secrets/initialAdminPassword`, and code-server's config.yaml. Accept the token in the `Authorization` header (Jupyter's `token` scheme) and through a one-time paste at the entry page, which issues a `SameSite=Lax` session cookie so later visits need no token. The token **never appears in a URL** — the product review rejected the Jupyter/VS Code serve-web token-in-URL transport (recommended above in the earlier draft) because locron's token is durable rather than per-server-start, so a token URL would persist in browser history until an explicit reset; `Referrer-Policy: no-referrer` remains as defense in depth. Never log the token; diagnostics report only presence and file-permission facts, as the spec requires.
- **DNS rebinding and CSRF.** Reject any request whose Host header is not `localhost`, `127.0.0.1`, or `[::1]` (any port) with 403 before routing — the Jupyter/Caddy/host-validation-middleware defense, and the only effective browser-side defense for plain-HTTP local services, since it works even for requests carrying no Origin header. Apply the Origin check on unsafe methods only (Django model). Use the double-submit CSRF pattern on mutations: a server-set `csrf_token` cookie (SameSite=Lax, not HttpOnly) whose value must be echoed in an `X-CSRF-Token` header or form field (Tornado `_xsrf` / Django `csrfmiddlewaretoken`), required whenever the request is cookie-authenticated; requests authenticated solely by the bearer token in the Authorization header are exempt, because a cross-site page cannot attach that header — matching Jenkins' documented API-token crumb exemption.
- **Streaming.** SSE, as the spec mandates: one-way, `text/event-stream`, EventSource auto-reconnect, no upgrade handshake, and OpenAI's production precedent for exactly this stream shape (text deltas, typed lifecycle events, terminal event). axum provides `Sse`/`Event`/`KeepAlive` in `axum::response::sse`. Implementation note: browsers cannot set an Authorization header on EventSource, so the SSE endpoint authenticates via the session cookie only — the frozen spec never puts the token in a URL.
- **Framework.** axum 0.8.9 (MSRV 1.80 — below locron's 1.94, no blocker) with tokio-stream 0.1.19 for stream adapters; hyper 1.11.0 underneath. Nothing beyond that stack is needed for this surface. (The tower-http-for-static-assets half is superseded by the frozen plan: §17 verified rust-embed's custom-handler integration replaces it.)
- **UI.** Bundled static assets, no CDN. The convention that repeats across healthchecks.io, cronitor, Jenkins, and GitHub Actions is: a status chip per job in the list, a run timeline in the detail view, and a monospace, timestamped, follow/auto-scroll console log for active output — adopt exactly that.

### Alternatives and Trade-offs

- WebSocket over SSE: rejected — every WS adopter (Jupyter kernels, Grafana Live, Uptime Kuma, Cronicle) needed bidirectional or multiplexed channels; locron's follow stream is strictly server-to-client, and SSE adds automatic reconnection without an upgrade handshake.
- Polling (healthchecks' model): simplest, but 3–60-second latency on live output contradicts the spec's "streams as it is written" criterion and wastes requests on long-running jobs.
- No-auth loopback-only (Caddy admin model): Caddy's admin surface is configuration-only (no user state) and even it added Host/Origin enforcement; locron's API mutates durable job state, so token plus CSRF is warranted.
- Random-port-only: unfriendly to bookmarks and scripts for no gain, when fixed-port-with-fallback is the Vite/Jupyter convention.
- 0.0.0.0-with-warning (FileBrowser, Transmission, OliveTin, Grafana, Jenkins model): rejected — violates the single-user local contract; refusal keeps the failure early and explicit.
- Unix-socket-only (Docker/Podman): the strongest exposure control, but browsers cannot reach unix sockets and locron's surface is a browser UI.
- actix-web, rocket, or warp instead of axum: heavier API surface, proc-macro heaviness, or stagnated maintenance, with no capability this surface needs; axum's declared MSRV is also the lowest-risk fit for the 1.94 toolchain.
- Requiring the double-submit token on bearer-token-only requests: rejected — Jenkins' documented exemption is the precedent, and it avoids breaking non-browser clients (curl, scripts) that attach the token header anyway.

## 15. Export Selection and URL Import (2026-08-24)

### Evidence

**The mainstream CLI convention is interactive-by-default on a TTY with automatic non-interactive fallback, not an explicit `--interactive` opt-in flag.** gh configures prompting with a `prompt` setting whose default is `enabled`, and in non-TTY contexts it skips prompting by design, failing with an actionable message instead (`must provide --title and --body when not running interactively`); scripts additionally use `GH_FORCE_TTY=false` and `PAGER=cat`. OpenSpec implements a documented priority chain: explicit flags, then an environment override, then CI detection (`CI` env), then a TTY check. aptu proposed removing its `--yes` flag entirely because non-TTY contexts should auto-apply without confirmation. diagramkit's documented convention is "auto mode": prompt only when no positional/flag arguments were supplied AND a TTY is attached. Sources: https://cli.github.com/manual/gh_config, https://github.com/oocx/tfplan2md/blob/67ba90dd3fabccc07f89e7400725c15c7170a5a8/.github/gh-cli-instructions.md, https://deepwiki.com/Fission-AI/OpenSpec/8.11-interactive-mode-system, https://github.com/clouatre-labs/aptu/issues/579, https://raw.githubusercontent.com/sujeet-pro/diagramkit/3eefccc30eb35596ffaa78ae929a77b7bca40a89/.agents/skills/prj-add-cli-flag/references/cli-conventions.md

**The known failure mode of TTY-detected prompting is prompting anyway when piped.** `gh gist view`/`gh gist edit` still display an interactive selector even when output is piped, breaking pipelines; the documented ideal (modeled on `gh run view`) is to detect non-TTY at runtime and fall back to a clear error or deterministic behavior instead of prompting. kubectl-ai handles piped stdin by opening `/dev/tty` directly for user input rather than consuming the redirected stream. Sources: https://blog.gitcode.com/4aab31ffebc94be7d4f3ef95b3f51220.html, https://deepwiki.com/GoogleCloudPlatform/kubectl-ai/6.2-mcp-server-mode

**A selection interface must never own standard output when stdout already carries a document contract.** Redirecting or piping stdout automatically makes the context non-TTY, which is precisely what keeps `export > backup.json` deterministic under the interactive-default convention; the picker renders to stderr (or `/dev/tty`) instead.

**Prior art for cron export/import confirms the typed-document decision and the trust boundary.** The crontab model (`crontab -l > file` / `crontab file`) is plain-text and whole-file, with machine-migration gotchas (absolute paths, minimal PATH, `%` escaping, system-vs-user crontab format) that locron's registration-time normalization already removes. crontab-ui/cron-gui export a database dump as a downloadable file and validate entries on import, automatically creating a backup before every import. Cronicle exports its entire configuration as one JSON file, but its import is destructive, requires the scheduler to be fully stopped, and the maintainers recommend per-event API updates over full import for bulk changes — the exact failure class locron's whole-document validation plus atomic transaction avoids. Healthchecks.io has no built-in export/import (feature request #834); its workaround is Django `dumpdata`/`loaddata`, an ORM serializer format that breaks across schema changes — supporting locron's typed versioned document over a database dump. Sources: https://stackoverflow.com/feeds/question/15767834, https://deepwiki.com/alseambusher/crontab-ui/5.2-backup-and-restore-api, https://mintlify.wiki/pixlcore/xyops/migration, https://github.com/healthchecks/healthchecks/issues/834

**Importing an export document from a URL registers executable schedules, the same trust class as `curl | sh`.** locron's own install one-liner already uses this trust model (HTTPS to the same origin as the artifacts, checksum verification, no API trust): an import URL is an explicit operator action whose document is validated in full before any write, with dry-run as the preview mechanism instead of an interactive confirmation. Source: `docs/FINDINGS.md` §11, `install.sh`

**dialoguer 0.12.0 meets the workspace dependency policy for the picker.** Verified 2026-08-24 with `cargo info`: version 0.12.0, MIT, `rust-version: 1.66` (below the 1.94 MSRV), default features limited to `editor` and `password` — both unnecessary, so `default-features = false` suffices for `MultiSelect`, and the terminal target can be set to stderr. inquire 0.9.4 (MSRV 1.80) was considered and rejected as unnecessary surface.

### Recommendation

- **Export selection.** Interactive-by-default on a TTY: bare `locron export` in an interactive terminal shows a multi-select picker on stderr with every job initially selected (confirming the initial selection exports everything); non-TTY contexts (pipe, redirection, no terminal, `CI` set) export the complete job set unchanged; `--jobs`/`--tag` select exact matches deterministically and never prompt; JSON mode never prompts and exports everything without an explicit selection.
- **Picker mechanics.** dialoguer 0.12 `MultiSelect` with `default-features = false` and the term target set to stderr; selection input requires both stdin and stdout to be terminals; a zero-job state skips the picker.
- **URL import.** `locron import URL` fetches with the existing reqwest/rustls stack (mandatory TLS verification), bounded redirects (10) and size (16 MiB), a total timeout, then reuses the existing whole-document validation/atomic-apply path unchanged; fetch failures map to the existing I/O/protocol error category; import never prompts, and `--dry-run` is the preview.
- **Trust documentation.** State in the CLI contract and operator guide that an export document registers executable schedules and a URL import carries the same trust boundary as installing a script from that URL.

### Alternatives and Trade-offs

- Explicit `--interactive` opt-in: rejected — the surveyed mainstream is TTY-detected interactive default with non-TTY fallback; an opt-in flag hides the feature from the interactive user it is for, while TTY detection keeps every script and pipeline unchanged.
- Interactive confirmation on URL import: rejected — a one-command share must complete without a TUI session; dry-run preview plus post-import action summary provides the safety without breaking the goal.
- fzf-style fuzzy selector: rejected — adds an external or fuzzy-matcher dependency for a job list that is small; a plain multi-select covers it.
- Whole-database dump export (crontab-ui/Cronicle model): rejected — already decided for the existing `locron.export/v1`; Healthchecks' ORM-dump fragility is the counter-evidence.
- Partial import (`--jobs` on import): explicitly deferred in `docs/SPEC.md` Out of Scope; the amendment covers selective export only.

## 16. Dashboard UI and API Design (2026-08-24)

### Evidence

**healthchecks.io's information architecture is a two-level drill-down — a checks list page and a per-check detail/log page — with no separate "Live"/timeline page in current master.** Verified from `hc/front/urls.py` (project routes: `checks/`, `checks/add/`, `checks/status/`, `integrations/`, `badges/`; per-check routes: `details/`, `log/`, `log_events/`, `status/`, `pause/`, `resume/`, `remove/`, `clear_events/`, `pings/<n>/`, `pings/<n>/body/`) and the `templates/front/` set. The checks list page (`checks.html` + `checks_table.html`) has seven columns: status icon (CSS class from `check.cached_status`, spinner when a run is in progress), name with tag labels, ping URL, integrations letters, Period/Grace (renders `{{ check.timeout|hc_duration }}` for simple checks or a `.cron-expression` div with `{{ check.schedule }}` plus a grace subline), Last Ping (relative text via Django `naturaltime` plus absolute instant in a `data-dt` attribute), and Actions (pause, details). Controls: a search text input, a "Filter by status" dropdown (New, Paused, Started, Up, Late, Down), and tag filter buttons; the table itself is not paginated. Empty states: "The project {{ project }} does not have any checks yet." with an Add Check button, and "no matching checks found" when filters hide everything. Sources: https://raw.githubusercontent.com/healthchecks/healthchecks/master/hc/front/urls.py, https://raw.githubusercontent.com/healthchecks/healthchecks/master/templates/front/checks.html, https://raw.githubusercontent.com/healthchecks/healthchecks/master/templates/front/checks_table.html, https://raw.githubusercontent.com/healthchecks/healthchecks/master/templates/front/last_ping_cell.html

**The healthchecks detail page is single-column blocks plus an embedded events log; the log page adds age-range and event-kind filtering.** Detail page (`details.html`) sections: "Description", "How To Ping", "Current Status", "Schedule" ("Period" with subtitle "(The expected time between pings)", "Grace Time" with "(When a check is late, how long to wait to send an alert)", "Change Schedule…"), "Notification Groups"/"Notification Methods" ("No notification methods set up yet."), "Danger Zone" ("Copy, transfer, or permanently remove this check."), and "Events" with a "Show More…" link to the full log and empty text "This check has not received any pings yet." plus "You will see a live-updating log of received pings here." Status text variants (`log_status_text.html`): "This check is down. Last ping was {{ check.last_ping|naturaltime }}." / "up" / "late", "This check is paused.", "This check has never received a ping.", and "Currently running, started {{ check.last_start|hms }} ago." The log page (`log.html`) filters by a "Show events older than:" age-range slider and kind checkboxes (Success, Failure, Started, Log, Ignored ping, Status change, Downtime alert), shows "Showing N matching events.", has a timezone switcher, and polls `log_events` via `log.js`/`adaptive-setinterval.js` (60 s poll, adaptive 3 s bursts — see §14). Log row badges (`log_row.html`): exactly "Ignored", "Status {{ exitstatus }}", "Failure", "Started", "Log", and "OK", plus status-flip rows "Status: {{ old }} ➔ {{ new }}" and missing-ping rows with an alert icon; every row carries `data-dt` with the absolute timestamp. Sources: https://raw.githubusercontent.com/healthchecks/healthchecks/master/templates/front/details.html, https://raw.githubusercontent.com/healthchecks/healthchecks/master/templates/front/details_events.html, https://raw.githubusercontent.com/healthchecks/healthchecks/master/templates/front/log.html, https://raw.githubusercontent.com/healthchecks/healthchecks/master/templates/front/log_row.html, https://raw.githubusercontent.com/healthchecks/healthchecks/master/templates/front/log_status_text.html, https://raw.githubusercontent.com/healthchecks/healthchecks/master/static/js/dates.js

**The repeated time convention is dual rendering: relative text visible, absolute instant machine-available.** healthchecks renders `naturaltime` server-side ("3 minutes ago") with the absolute value in `data-dt`; `dates.js` is a `DateFormatter` class of `Intl.DateTimeFormat` instances for the chosen timezone ("Jan 15, 2025, 14:03") — so relative display, absolute-on-hover/click, and a timezone switcher coexist. The status-page template (healthchecks' `dashboard.html`) shows the same convention in plain JS: `timeSince` producing "5 min ago", with colors green/yellow/red for up/grace/down. Jenkins and GitHub Actions use the same relative-plus-absolute pattern on run lists. Sources: https://raw.githubusercontent.com/healthchecks/healthchecks/master/templates/front/last_ping_cell.html, https://raw.githubusercontent.com/healthchecks/healthchecks/master/static/js/dates.js, https://raw.githubusercontent.com/healthchecks/healthchecks/master/templates/front/dashboard.html

**Run-history visualization in current tools is either an event log list (healthchecks), a status-colored grid (Jenkins Stage View), or a timeline (cronitor's marketing); GitHub Actions adds an explicit attempt selector.** Jenkins Pipeline Stage View renders "one row per build" with "Checkout, Build, and Test columns" whose cells are status-colored and carry progress bars ("indicate how long each stage is taking"), plus per-cell Logs buttons — the closest precedent for status-colored segments per unit of work. GitHub Actions re-runs create attempts and the run summary shows a "Latest" dropdown where you "select the Latest dropdown menu and click a previous run attempt", capped at "a maximum of 50 times". Cronitor (closed source; docs subpages under `cronitor.io/docs/` returned 404 this session, so only product pages are citable) advertises a "Timeline view of job schedules and activity" whose purpose is to "Track job progress, visualize your cron schedules and find surprise hotspots". healthchecks has no run grid — its history is the event list above (verified by absence of any live/timeline route or template in master). Sources: https://plugins.jenkins.io/pipeline-stage-view/, https://docs.github.com/en/actions/managing-workflow-runs/re-running-workflows-and-jobs, https://cronitor.io/cron-monitoring, https://raw.githubusercontent.com/healthchecks/healthchecks/master/hc/front/urls.py

**Log viewer conventions: Jenkins tails by default and upgrades to progressive polling while the build runs; GitHub Actions logs are searchable, per-line permalinkable, and downloadable; healthchecks caps captured bodies at 100 kB with truncation at capture time.** Jenkins' console page (`console.jelly` + `console-log.jelly`) renders output into a `<pre id="out">` with no line numbers, defaults to the "last 150KB of output, configurable through the `hudson.consoleTailKB` system property" with a full-width "skipSome" link to `consoleFull` when the log is longer, and while the build runs switches to a `progressiveText` component polling `logText/progressiveHtml` from `startOffset` and signaling completion via `onFinishEvent="jenkins:consoleFinished"`; page actions are Download (links to `consoleText`), Copy, and "View as plain text". GitHub Actions' documented log page has a "Search logs" search box ("In the upper-right corner of the log output, in the Search logs search box, type a search query") with the caveat "When you search logs, only expanded steps are included in the results", line-number permalinks ("you can copy a permalink to a specific line in the log file to share with your team", "click on the step's line number"), and "Download log archive" via the gear dropdown. healthchecks' "Attaching Logs" docs: "Healthchecks.io stores the first 100 kB (100,000 bytes) of the request body" per ping, with a `Ping-Body-Limit` response header; issue #939 records the maintainer's change to truncate the beginning and keep the end ("when a script crashes, the error is usually at the end"); the ping details dialog shows the body in a `<pre>` with a "Download Original" link and a "The request body data is not yet available, please check back later." notice — truncation is silent at capture, no inline marker in the UI. Sources: https://raw.githubusercontent.com/jenkinsci/jenkins/master/core/src/main/resources/hudson/model/Run/console.jelly, https://raw.githubusercontent.com/jenkinsci/jenkins/master/core/src/main/resources/hudson/model/Run/console-log.jelly, https://docs.github.com/en/actions/monitoring-and-troubleshooting-workflows/using-workflow-run-logs, https://healthchecks.io/docs/attaching_logs/, https://github.com/healthchecks/healthchecks/issues/939, https://raw.githubusercontent.com/healthchecks/healthchecks/master/templates/front/ping_details.html

**REST conventions: healthchecks v3 is resource-per-URL with list envelopes and status-code errors; Jenkins is `/api`-suffix per resource with token Basic auth; Slack's `{ok, error, warning}` envelope is the closest precedent to locron's `{ok, data, warnings}`; cursor pagination is the Slack/GitHub-style norm for histories, offset-range the Jenkins norm.** healthchecks API v3: `GET/POST /api/v3/checks/`, `GET/POST /api/v3/checks/<uuid>`, `POST /checks/<uuid>/pause|resume`, `DELETE /checks/<uuid>`, `GET /checks/<uuid>/pings/` (most recent first), auth via `X-Api-Key` header; list responses wrap in a named object (`{"checks": [...]}`, `{"pings": [...]}`) while single objects are bare; errors are HTTP status codes and "may contain a JSON document with additional data"; there is no pagination — pings are plan-capped ("100 for free accounts, 1000 for paid accounts") with a documented rate limit ("Avoid making more than 100 API requests per minute"). Jenkins Remote Access API: resources expose ".../api/" suffixes (top level, job, build), triggering is "perform an HTTP POST on `JENKINS_URL/job/JOBNAME/build`", auth is "HTTP BASIC authentication" with API tokens ("API tokens are preferred instead of crumbs"), and responses are filtered by `depth`/`xpath`; the `tree` parameter selects fields (`tree=builds[number,timestamp,result]`) and array properties support `{M,N}` range pagination (CloudBees' write-up; the `builds` property is limited to the latest 100 without the hidden `allBuilds` element; JENKINS-39391 documents that out-of-range "from" indices throw rather than return empty pages, and the API exposes no total count). Slack Web API: "a top-level boolean property `ok` that indicates success or failure", "For failure results, the `error` property will contain a short machine-readable error code", and warnings ride along on success (`{"ok": true, "warning": "something_problematic", ...}`) — structurally identical to locron's `{ok, data, warnings}`; pagination is cursor-based: "Cursors are like pointers", "Paginated responses include a top-level `response_metadata` object that includes a `next_cursor`", and "An empty, null, or non-existent `next_cursor` in the response indicates no further results." Telegram Bot API uses the same family of envelope: every response "always has a Boolean field 'ok'", payload "can be found in the 'result' field", failures carry `error_code` plus a human "description". Sources: https://healthchecks.io/docs/api/, https://www.jenkins.io/doc/book/using/remote-access-api/, https://www.cloudbees.com/blog/taming-jenkins-json-api-depth-and-tree, https://github.com/jenkinsci/jenkins/issues/17648, https://docs.slack.dev/apis/web-api/, https://docs.slack.dev/apis/web-api/pagination/, https://core.telegram.org/bots/api

**A no-build vanilla-JS SPA is precedented by the surveyed tools themselves: EventSource reconnects by default, hash navigation is a browser-native event, and healthchecks' entire dashboard JS is hand-written framework-free code.** MDN: "By default, if the connection between the client and server closes, the connection is restarted"; the `retry` field sets "the reconnection time" in milliseconds; comment lines can "prevent connections from timing out"; and over HTTP/1.1 a browser caps SSE at about 6 open connections per domain (100 over HTTP/2). Hash routing is native: "The `hashchange` event is fired when the fragment identifier of the URL has changed (the part of the URL beginning with and following the `#` symbol)" (the event does not fire on `pushState`/`replaceState`). healthchecks' status page parses its URL hash directly (`#API_KEY=Label&theme=dark`) and fetches `/api/v3/checks/` with `fetch` + `X-Api-Key`, all in one hand-written HTML/JS file; its main dashboard JS (`checks.js`, `log.js`, `dates.js`, `adaptive-setinterval.js`) is likewise plain JavaScript over a static asset pipeline with no build step. crontab-ui is the minimal single-page extreme: one job-management view with per-job error logs, no history page at all (Express + static UI, port 8000, optional BASIC auth). Sources: https://developer.mozilla.org/en-US/docs/Web/API/Server-sent_events/Using_server-sent_events, https://developer.mozilla.org/en-US/docs/Web/API/Window/hashchange_event, https://raw.githubusercontent.com/healthchecks/healthchecks/master/templates/front/dashboard.html, https://github.com/alseambusher/crontab-ui/blob/main/README.md

**Empty/edge-state conventions in the surveyed dashboards: explicit onboarding copy with the primary action attached, a distinct filtered-empty message, an availability indicator, and placeholder text for never-seen values.** healthchecks: "The project {{ project }} does not have any checks yet." next to the Add Check button; "no matching checks found" for a filtered-empty list; "This check has never received a ping." and a bare "Never" in the last-ping cell; "No notification methods set up yet."; Jenkins' job page shows an empty "No builds" state and a console page with the tail-skip link as the long-log marker; Grafana's empty dashboard routes to the "Add an empty panel" workflow, and panels without data show a "No data" state rather than a blank pane. A daemon-offline banner has no strong documented precedent among the surveyed tools (Jenkins errors on an unreachable controller; healthchecks' "site down" states describe monitored sites, not the app itself), so locron's spec criterion 8 (web diagnostics page reporting the same facts as the CLI) is the design anchor; the observed universal pattern is a persistent, always-visible status line rather than a modal. Sources: https://raw.githubusercontent.com/healthchecks/healthchecks/master/templates/front/checks.html, https://raw.githubusercontent.com/healthchecks/healthchecks/master/templates/front/details_events.html, https://raw.githubusercontent.com/healthchecks/healthchecks/master/templates/front/last_ping_cell.html, https://archive.grafana.com/docs/grafana/v8.3/panels/add-a-panel/, https://raw.githubusercontent.com/jenkinsci/jenkins/master/core/src/main/resources/hudson/model/Run/console-log.jelly

### Recommendation

- **Information architecture.** One chrome shell with a persistent top navigation (Jobs, Run history, Diagnostics) and five hash-routed views: `#/jobs` (landing; job list), `#/jobs/:id` (job detail: definition, policies, why, recent runs), `#/jobs/new` and `#/jobs/:id/edit` (create/edit form with schedule preview and dry-run), `#/runs` (run history with per-run attempt breakdown), `#/runs/:id` (run detail: attempts, events, log viewer), `#/diagnostics` (scheduler health, paths, daemon availability, exposure facts, settings). This is the healthchecks two-level drill-down (list → detail/log), which every surveyed tool repeats; a cronitor-style "single pane of glass" dashboard is unnecessary for one user. The header carries a daemon-availability indicator fed by the diagnostics endpoint, per spec criterion 8.
- **Job list row.** Status chip (enabled/disabled plus last-outcome color, spinner while a run is active), name with tags, schedule summary (humanized interval/cron plus timezone; raw expression as secondary text), next occurrence (relative text with absolute RFC 3339 instant in a `data-*` attribute — the healthchecks dual-rendering convention), last outcome (outcome label, duration, time), and row actions (run now, show, enable/disable, remove). A search box plus an enabled/disabled status filter sits above the table (healthchecks model); no pagination — the list is small and healthchecks' full-table rendering is the precedent. Fields map 1:1 to spec §5's job list: name, schedule summary, enabled state, next occurrence, last outcome.
- **Run history and attempt visualization.** `#/runs` lists one row per run: trigger, nominal time, outcome chip, timing, attempt count, and a horizontal strip of status-colored segments — one segment per attempt, outcome-colored, width proportional to duration — with each segment linking to that attempt in the log viewer. This is the minimal standard for "attempts of one run with outcomes": Jenkins Stage View's status-colored cells with per-cell drill-down and GHA's step dots and attempt dropdown. Skip/supersession/acknowledgement events render as annotation rows with distinct badges (healthchecks' event-kind badge set: OK / Failure / Started / Log / Ignored / Status change, plus "Status: old ➔ new" flips). Pagination: `limit` + `offset` with a `total` count in the envelope (see API), since retention is bounded and the dataset is single-user local.
- **Log viewer.** A `<pre>`-style monospace pane: line numbers with click-to-copy permalinks (GHA precedent), per-line timestamps shown by default with a toggle (GHA stores them hidden by default; locron is a scheduler — time is the primary axis), tail-first open (Jenkins' 150 KB `consoleTailKB` precedent) with "load older" paging, a search box filtering displayed lines (GHA precedent; locron's bounded retention keeps this feasible client-side), and follow mode = pinned-to-bottom auto-scroll over the SSE stream with an explicit "stream ended" notice and a follow toggle that stops auto-scroll on manual scroll-up (Jenkins' progressive mode with `consoleFinished` termination event is the shape). Truncation and discard markers render at the point of truncation as inline markers, as the spec requires — stronger than healthchecks (silent at capture) and Jenkins (single skip-ahead link), and locron already owns the marker data. ANSI: preserve raw bytes in the API and render with a small hand-written ANSI→span parser bundled in the viewer; GitHub's community forum reports ANSI control codes breaking the Actions viewer (secondary source, unverified from primary).
- **API route families (1:1 with `docs/CLI.md` commands, under `/api/v1/`).** `add` → `POST /api/v1/jobs`; `update` → `PUT /api/v1/jobs/{name|uuid}` (immutable-revision semantics); `list` → `GET /api/v1/jobs`; `show` → `GET /api/v1/jobs/{name|uuid}`; `enable`/`disable` → `POST /api/v1/jobs/{id}/enable` and `/disable`; `remove` → `DELETE /api/v1/jobs/{id}`; `preview` → `POST /api/v1/schedule/preview` (schedule selector) and `GET /api/v1/jobs/{id}/preview` (live job, `--count` as `?count=`); `run` → `POST /api/v1/jobs/{id}/run` (`?wait`, `?dry-run`); `cancel` → `POST /api/v1/runs/{id}/cancel` (`acknowledge_unconfirmed` flag); `history` → `GET /api/v1/runs` (`?job=`, `?limit=`, `?offset=`); `logs` → `GET /api/v1/runs/{id}/logs` (`?attempt=`, `?channel=`); `why` → `GET /api/v1/jobs/{id}/why` and `GET /api/v1/runs/{id}/why`; `config get|set|unset` → `GET /api/v1/settings`, `PUT /api/v1/settings/{key}`, `DELETE /api/v1/settings/{key}` (global environment keys keep the `environment.NAME` grammar and redaction); `export` → `GET /api/v1/export` (explicit `--include-values`+`--acknowledge-plaintext` become required query flags per the CLI contract); `import` → `POST /api/v1/import` (multipart upload, `?accept-plaintext-values`, `?dry-run`); `prune` → `POST /api/v1/prune` (`?dry-run`); `doctor` → `GET /api/v1/diagnostics`. Not mirrored: `daemon run` (process supervision), `service` (registration; the dashboard family has its own), `self-update` (binary replacement) — their facts surface through diagnostics, which reports the same facts as the CLI per spec §9.
- **Envelope, errors, pagination, SSE.** Every response uses the `locron.api/v1` envelope mirroring the CLI's machine output: `{"schema":"locron.api/v1","ok":true,"data":{...},"warnings":[]}`, and errors `{"ok":false,"error":{"code":...,"message":...,"details":...}}` with the stable CLI error codes carried verbatim — Slack's `ok`/`error`/`warning` and Telegram's `ok`/`result`/`error_code` are the external precedents, and the CLI contract already fixes the field semantics. HTTP mapping of the CLI exit categories: validation → 400, not-found → 404, conflict → 409, busy/locked → 409, state unavailable or daemon-required → 503, internal → 500 (documented in IMPLEMENTATION.md per spec §6). Pagination is `limit`/`offset` + `total` in `data` — Slack's cursor is the norm for shared histories, but locron is single-user with bounded retention, where SQLite `COUNT` is cheap and offsets stay stable; Jenkins' range-without-total style is rejected for missing the total. SSE: `GET /api/v1/runs/{id}/stream` as typed named events (`output`, `attempt`, `run`, `termination` — OpenAI-style naming, §14) carrying the `locron.stream/v1` frames the CLI follows; the browser's EventSource auto-reconnect handles reconnection, the terminal event is idempotent, and keep-alive comments prevent proxy timeouts (MDN). Cookie-authenticated only, because EventSource cannot attach an Authorization header (spec §7, §14).
- **SPA structure sketch (no build toolchain).** `index.html` shell + hand-written JS split into small files served from the bundled static dir: `router.js` (hash parsing + `hashchange` dispatch to view renderers; ~50 lines, no library), `api.js` (fetch wrapper: adds `X-CSRF-Token` on cookie-authenticated mutations, unwraps the envelope, maps `error.code` to messages), `views/` (one render function per route), `components.js` (status chips, dual-rendered times, attempt segments, tables), `sse.js` (EventSource wrapper with reconnecting state and close-on-termination). No History API routing (needs server fallbacks), no framework, no CDN — matching the spec's self-contained constraint and the healthchecks hand-written precedent.
- **Empty/edge states.** `#/jobs` with zero jobs shows an onboarding card ("No jobs yet" + Create job action — healthchecks' "does not have any checks yet" + Add Check); filtered-empty shows "no matching jobs found"; the daemon-offline state is a persistent header banner fed by `GET /api/v1/diagnostics` with the exact CLI facts; redacted values render as the CLI's literal `<redacted>`/`value redacted` markers, never a value or a synthesized sentinel (CLI redaction convention in `docs/CLI.md`), keeping web and CLI output semantically identical.

### Alternatives and Trade-offs

- React/Vue/Svelte or a router library (react-router, etc.): rejected — the no-build-toolchain constraint is absolute for this surface, and the total feature set (5 views, one stream) fits ~1k lines of hand-written JS; healthchecks is the working precedent.
- History API (`pushState`) routing: rejected — deep links and refresh would need server-side fallback rewrites; hash routing survives refresh and bookmarking with zero server work (MDN `hashchange` is the browser-native mechanism).
- Server-rendered pages (Django/Jelly model of healthchecks/Jenkins): rejected — the frozen spec requires a fully self-contained bundled viewer, and SSR buys nothing for a single user on loopback.
- Cursor pagination for run history (Slack model): rejected — cursors protect against concurrent mutation of shared collections; locron is single-user with bounded retention, so `limit`/`offset` + `total` is simpler and Jenkins' no-total ranges were rejected for the missing total.
- Healthchecks-style polling for the log page (60 s adaptive): rejected — contradicts the spec's "streams as it is written" criterion; SSE follow covers it (§14).
- Per-attempt sub-pages instead of in-page segments: rejected — an attempt strip in the run row/detail keeps one view per run; GHA's separate attempt pages were judged more navigation for the same information.
- Strip ANSI entirely: rejected — loses coloring the CLI captures; a ~100-line parser bundled in the viewer keeps parity, and GitHub's viewer breakage reports argue for doing it deliberately.
- Timestamps hidden by default (GHA model): rejected — locron is a scheduler; nominal time and timestamps are the primary axis of every view, so they default on with a toggle off.
- Client-side-only log search: accepted for v1 — retention bounds keep loaded lines small; a server-side search endpoint is the deferred fallback if logs grow.
- Cronitor-style aggregate dashboard / "single pane of glass": rejected — one user, one scheduler; the job list is the dashboard, with diagnostics one click away.

## 17. Dashboard Implementation Stack (2026-08-24)

### Evidence

**rust-embed 8.12.0 is current, active, and MSRV-compatible: `rust-version: 1.80` (below the workspace MSRV 1.94), MIT, released 2026-07-08 on a steady 8.x cadence (8.7.2 May 2025, 8.8.0 Oct 2025, 8.9.0 Nov 2025, 8.10.0/8.11.0 Jan 2026), ~47.7M downloads.** The crate has no default features; the needed surface is `mime-guess` only (`mime-guess = [rust-embed-utils/mime-guess]`), which fills `Metadata.mimetype` (`pub fn mimetype(&self) -> &str`, gated `#[cfg(feature = "mime-guess")]` per the rust-embed-utils 8.12.0 source); `Metadata.sha256_hash() -> [u8; 32]` and `last_modified() -> Option<u64>` are always available. Embedding semantics: "loads files into the rust binary at compile time during release and loads the file from the fs during dev" — in debug builds (without the `debug-embed` feature) the folder is read from the filesystem, so asset edits do not require recompiles. Sources: https://crates.io/crates/rust-embed, https://docs.rs/rust-embed/latest/rust_embed/ (cargo info, 2026-08-24), https://docs.rs/rust-embed-utils/latest/rust_embed_utils/struct.Metadata.html, https://docs.rs/crate/rust-embed-utils/8.12.0/source/src/lib.rs

**rust-embed's documented axum integration is a custom handler reading `RustEmbed::get`, not a tower-http service.** The crate ships `examples/axum.rs` (run via the `axum-ex` feature) whose `serve_asset` does `match Asset::get(path) { Some(content) => ([(header::CONTENT_TYPE, content.metadata.mimetype())], content.data).into_response(), None => (StatusCode::NOT_FOUND, "404 Not Found").into_response() }` — `get` returns `Option<EmbeddedFile>` with `data: Cow<'static, [u8]>` and `metadata: Metadata`; the `Content-Type` header is only emitted under `mime-guess`; the example sets no cache/ETag headers (the embedded sha256 hash is available for an ETag). The `axum` feature (`axum = [dep:axum]`) exists only to compile the example. Sources: https://docs.rs/crate/rust-embed/8.12.0/source/examples/axum.rs, https://docs.rs/rust-embed/latest/rust_embed/struct.EmbeddedFile.html

**include_dir is stale and feature-poor by comparison: 0.7.4 (MSRV 1.64) is the latest, released 2024-06-17 — no release in over two years as of 2026-08-24 — with no MIME-type support and no framework integration; files are exposed as `&'static [u8]` via a `const`-based macro.** Sources: https://crates.io/crates/include_dir (cargo info and crates.io API, 2026-08-24)

**axum 0.8.9 and tokio-stream 0.1.19 remain current and MSRV-compatible (re-verified 2026-08-24): axum `rust-version: 1.80`, tokio-stream `rust-version: 1.71`.** Sources: https://crates.io/crates/axum, https://crates.io/crates/tokio-stream (cargo info, 2026-08-24)

**The conventional cookie handling for axum 0.8 is the `axum-extra` `CookieJar` extractor behind the `cookie` feature; axum-extra 0.12.6 is the axum-0.8 series (depends on axum ^0.8), MSRV 1.80, released 2026-04-14.** `CookieJar` is "Available on crate feature `cookie` only", implements `FromRequestParts` with an infallible rejection, and "must be returned from the handler as part of the response for the changes to be propagated" — `add`/`remove` consume and return the jar, and values are percent-encoded automatically. Signing/encryption are opt-in features (`cookie-signed`, `cookie-private`, `cookie-key-expansion`) mapped to the `cookie` crate's jars. Sources: https://docs.rs/axum-extra/latest/axum_extra/extract/cookie/struct.CookieJar.html, https://crates.io/crates/axum-extra (cargo info, 2026-08-24)

**The underlying `cookie` crate (0.18.2, MSRV 1.56, MIT OR Apache-2.0, released 2026-08-08 — very active) supports every attribute the plan needs through the `CookieBuilder` chain (`Cookie::build(("name", "value")).path("/").secure(true).http_only(true)`).** Documented SameSite semantics: `Strict` — "the cookie is never sent in cross-site requests"; `Lax` — "the cookie is only sent in cross-site requests with 'safe' HTTP methods" (GET, HEAD, OPTIONS, TRACE); `None` — sent in all cross-site requests "if the 'Secure' flag is also set, otherwise the cookie is ignored", and the crate "automatically sets the 'Secure' flag on cookies when `same_site` is set to `SameSite::None` as long as `secure` is not explicitly set to `false`". `max_age` takes the `time` crate's `Duration` (re-exported as `cookie::time::Duration`, e.g. `Duration::days(90)`), not `std::time::Duration` — an implementation note for the 90-day session lifetime. Sources: https://docs.rs/cookie/latest/cookie/enum.SameSite.html, https://docs.rs/cookie/latest/cookie/struct.Cookie.html, https://docs.rs/cookie/latest/cookie/struct.CookieBuilder.html, https://crates.io/crates/cookie (cargo info, 2026-08-24)

**A `Secure` flag on plain-HTTP loopback is both unnecessary and actively problematic: per MDN, "Insecure sites (`http:`) cannot set cookies with the `Secure` attribute", and a Secure cookie is sent "only when a request is made with the `https:` scheme (except on localhost)".** The plan's "no `Secure` flag on plain-HTTP loopback" is therefore correct; SameSite is the CSRF-relevant attribute ("provides some protection against certain cross-site attacks, including cross-site request forgery (CSRF) attacks"), and HttpOnly "forbids JavaScript from accessing the cookie". Sources: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Set-Cookie

**A token-as-cookie-value session needs none of the signing/encryption cookie features: the cookie's value is the server-held 32-byte token compared on every request, so nothing is stored in the cookie that must be authenticated or decrypted later; the plain (unsigned) `CookieJar` suffices and no cookie-signing key enters the system.**

**axum 0.8's documented middleware pattern is `middleware::from_fn` (and `from_fn_with_state` for state access), with ordering controlled by where the layer is added: "When you add middleware with `Router::layer` (or similar) all previously added routes will be wrapped in the middleware. Generally speaking, this results in middleware being executed from bottom to top."** `Router::layer` — "the middleware is only applied to existing routes. So you have to first add your routes (and / or fallback) and then call `layer` afterwards. Additional routes added after `layer` is called will not have the middleware added"; "Middleware added with this method will run _after_ routing and thus cannot be used to rewrite the request URI". `Router::route_layer` — "the middleware will only run if the request matches a route... useful for middleware that return early (such as authorization) which might otherwise convert a `404 Not Found` into a `401 Unauthorized`". State flows to middleware via `from_fn_with_state`; middleware-to-handler values via request extensions (`Extension`). Sources: https://docs.rs/axum/latest/axum/middleware/index.html, https://docs.rs/axum/latest/axum/routing/struct.Router.html

**SQLite WAL is the documented concurrency answer for the two-process design (daemon writer + server): "WAL provides more concurrency as readers do not block writers and a writer does not block readers", "there can only be one writer at a time", "many concurrent overlapping readers" are supported, and "a single read transaction only sees the database content as it existed at a single point in time" — consistent snapshots for the server's reads.** Checkpoints run automatically at the 1000-page WAL threshold and "when the last database connection on a database file closes". Sources: https://sqlite.org/wal.html

**rusqlite's `Connection` is `Send` but not `Sync` ("Rusqlite enforces thread-safety at compile time, so additional locking is not needed"), newly created connections already default to a 5000 ms busy timeout, and SQLite's threading guidance is that a connection must not be "used in two or more threads at the same time".** Connection-per-request on the Tokio blocking pool therefore needs no lock and no pool crate: each `spawn_blocking` task gets its own connection, uses it, and drops it. Sources: https://docs.rs/rusqlite/latest/rusqlite/struct.Connection.html, https://sqlite.org/threadsafe.html

**The store already implements exactly this connection setup and the dashboard should reuse it: `connection.busy_timeout(std::time::Duration::from_secs(5))` followed by `PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON; PRAGMA locking_mode=NORMAL; PRAGMA trusted_schema=OFF;` (crates/locron-store/src/store.rs:2977-2978).** The daemon and the dashboard therefore share a WAL database by construction; the server adds unlimited non-blocking readers and occasional writer connections serialized by the busy timeout. Source: crates/locron-store/src/store.rs (repo, 2026-08-24)

**Server-side URL import needs no new dependency and no new reqwest features: the CLI's `fetch_import_url` (crates/locron-cli/src/main.rs:2556-2600) already implements the exact plan bounds with the workspace reqwest — `Client::builder().redirect(reqwest::redirect::Policy::limited(10)).timeout(30 s)`, mandatory TLS verification via the `rustls` feature, `response.bytes_stream()` via the `stream` feature, and a 16 MiB in-memory cap (`IMPORT_MAX_BYTES = 16 * 1024 * 1024`) enforced while streaming, plus userinfo rejection.** reqwest documents `Policy::none()` — "Create a `Policy` that does not follow any redirect" — and `Policy::limited(n)` — "Create a `Policy` with a maximum number of redirects"; the CLI uses the `limited` form. The 16 MiB cap is applied to the accumulated stream, not a Content-Length pre-check — the safer pattern, since Content-Length can lie. Sources: crates/locron-cli/src/main.rs (repo, 2026-08-24), https://docs.rs/reqwest/latest/reqwest/redirect/struct.Policy.html

**getrandom 0.4.3 (MSRV 1.85, MIT OR Apache-2.0, released 2026-06-17) is the right direct dependency for the token's 32 bytes, and it is already in the lockfile: uuid 1.24.1 depends on getrandom 0.4.3, so a `getrandom = "0.4"` workspace entry resolves to the locked version with no new crate.** Documented pitfalls are irrelevant on macOS/Linux: macOS uses `getentropy` (no caveats), and the Linux early-boot behavior is that the crate "always choose[s] to block" rather than return low-entropy bytes; the wasm-only caveats do not apply. (getrandom 0.3.4 is in the tree only as a proptest dev-dependency via rand.) Sources: https://docs.rs/getrandom/latest/getrandom/, https://crates.io/crates/getrandom (cargo info and cargo tree, 2026-08-24)

### Recommendation

- **Asset embedding.** rust-embed 8.12 with the `mime-guess` feature only (default features are already empty). Active (8.12.0, 2026-07-08), MSRV 1.80, and its documented axum example is exactly the custom-handler pattern this surface needs. include_dir is stale (no release since 2024-06-17) and lacks MIME handling.
- **Static serving.** One custom handler: route `/` and `/{*path}`, `Asset::get`, respond with `Content-Type` from `content.metadata.mimetype()`, body `content.data`, 404 on miss — the crate's own axum example. Set `Cache-Control: no-cache` on asset responses (or an `ETag` from `metadata.sha256_hash()`); the example sets no cache headers, but a bundled viewer's files change only with the binary, so revalidation semantics are the honest choice. No tower-http, no ServeDir — the plan's exclusion is supported.
- **Cookie crate.** `axum-extra` with only the `cookie` feature for the `CookieJar` extractor; plain (unsigned) jar — no `cookie-signed`/`cookie-private`/`cookie-key-expansion`. Session cookie attributes via the cookie crate builder: `HttpOnly`, `SameSite::Lax`, `Path=/`, `Max-Age` 90 days (via `cookie::time::Duration::days(90)` — not `std::time::Duration`), no `Secure` (MDN: insecure sites cannot set Secure cookies; localhost is exempted, but the flag buys nothing on loopback).
- **Middleware pattern.** `axum::middleware::from_fn` functions composed in one chain (Host allowlist → Origin check on unsafe methods → token auth → CSRF → Referrer-Policy), applied with `Router::layer` after all routes and the fallback are registered, so the stack wraps every request including the entry-page fallback and unmapped paths; token and origin facts flow via `from_fn_with_state`.
- **SQLite connections.** Connection-per-request inside `tokio::task::spawn_blocking`, opened through the store's existing connection setup (WAL + `busy_timeout(5 s)` + the store's pragma set at store.rs:2977). WAL gives the server unlimited non-blocking readers alongside the daemon writer; writer contention is resolved by the busy timeout. No pool crate for this traffic level.
- **URL import.** Reuse the CLI's `fetch_import_url` bounds verbatim: `Policy::limited(10)`, 30-second total timeout, streaming 16 MiB cap, rustls verification, userinfo rejection. The workspace reqwest features (`rustls`, `stream`, `json`) already cover it.
- **Token RNG.** Add `getrandom = "0.4"` to `workspace.dependencies`; `getrandom::fill` into a 32-byte buffer, then hex-encode. MSRV 1.85, already locked at 0.4.3 via uuid — zero new dependencies.

### Alternatives and Trade-offs

- include_dir over rust-embed: rejected — stale (no release since 2024-06-17), no MIME support, no axum integration; rust-embed's `mime-guess` metadata is the documented content-type mechanism.
- Plain `include_bytes!` for the ~6 files: the minimal fallback, but hand-maintained path→(bytes, mime) map, no dev-time filesystem reading, no hash metadata for an ETag; not worth it when rust-embed is a single well-maintained proc-macro dependency.
- tower-http `ServeDir`: rejected — serves from disk, not from embedded bytes; the accepted plan already excludes tower-http, and §14's Recommendation line "with tower-http 0.7.0 for bundled static assets" is superseded by the accepted plan (this research supports the plan).
- `cookie` crate directly (bypassing axum-extra): viable, but `CookieJar` is the conventional axum integration (infallible extractor, automatic response propagation) for one small feature flag.
- Signed/private cookie jars: rejected — the session cookie's value is the server-held token, compared on each request; signing/encryption would add a cookie-key-management surface for data the server never needs to recover from the cookie.
- A rusqlite pool (deadpool-sqlite / r2d2): rejected — single-user loopback traffic with short requests; per-request connections on the blocking pool match SQLite's per-thread connection model and the store's existing pattern. A pool is the migration path only if request volume grows.
- `Router::route_layer` for the whole security stack: rejected — route_layer "will only run if the request matches a route", so a hostile Host on an unmapped path would reach the fallback instead of the 403; `Router::layer` covers every request.
- getrandom 0.3.x as the direct dependency: rejected — 0.3.4 is only in the tree as a proptest dev-dependency; 0.4.3 is already the production version (via uuid), MSRV 1.85, and its Linux/macOS behavior has no caveats for this surface.

## 18. Live External API Calls in CI (2026-08-24)

**The established convention is that blocking push CI must be hermetic; checks that depend on live third-party APIs belong on a schedule or as non-blocking.** Google's testing guidance makes hermeticity the definition of a usable test ("All tests should strive to be hermetic"; small tests "aren't allowed to access the network") and documents that a live backend makes presubmit flaky — the Assistant case study: non-hermetic presubmit "would routinely fail," and after hermeticization runtime dropped 14x with "virtually no flakiness." Martin Fowler's non-determinism essay is explicit that a live system "may not be stable enough to provide deterministic responses" and that "automated tests are useless if they are non-deterministic." Google's own live-dependent ("hermetic server") tests run "automatically at a regular frequency," not per changelist. Sources: https://abseil.io/resources/swe-book/html/ch11.html, https://abseil.io/resources/swe-book/html/ch23.html, https://martinfowler.com/articles/nonDeterminism.html, https://testing.googleblog.com/2012/10/hermetic-servers.html.

**Real projects converge on a three-tier placement**: hermetic push CI; `continue-on-error: true` for live checks that must ride pushes (forgejo's lychee job: "the workflow should now only fail if something goes wrong, not if there are missing links"); and `schedule:` cron plus `workflow_dispatch` for authoritative live/drift checks (wickra: weekly-cron links workflow, rationale "external sites flake… a transient external outage must never block"; cam-project: a weekly live-test tier marked `@pytest.mark.live` and excluded from normal runs; Ultralytics: daily link checks that deliberately accept 429s, exclude flaky domains, and retry; Google's Dart tools: a Sunday cron live smoke). Sources: https://github.com/wickra-lib/wickra/commit/57e52c67b9e7a24a9851028447a1dc18e8fa57b2, https://codeberg.org/forgejo/website/pulls/530, https://github.com/rogermyung/cam-project/pull/55, https://raw.githubusercontent.com/ultralytics/ultralytics/d86cb966893e2b96345e596a66d1ca8656de6590/.github/workflows/links.yml, https://dart.googlesource.com/tools/+/84ac705a7df1d6aa25f75e508b2b2b0bf408a57e/.github/workflows/markdown_flutter.yaml, https://stevekinney.com/courses/self-testing-ai-agents/nightly-verification-loops.

**Unauthenticated GitHub REST from Actions is the exact failure locron hit.** The unauthenticated REST quota is 60 requests/hour per originating IP, and GitHub-hosted runners use dynamically assigned shared IPs, so that budget is shared across tenants and frequently exhausted before a job starts; GitHub documents this and does not recommend IP-based trust. freenet-core hit the identical intermittent 403 in a nightly smoke ("The unauthenticated GitHub API limit is 60 req/hr per IP" … "GitHub-hosted runner IPs are shared across tenants" … "an intermittent flake that will keep recurring") and fixed it by wiring `GITHUB_TOKEN` into the API calls (1,000/hour per repository). locron's failing step even carried the comment "GITHUB_TOKEN raises the REST quota" without passing the token through `env:`. Sources: https://docs.github.com/en/rest/using-the-rest-api/rate-limits-for-the-rest-api, https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/about-githubs-ip-addresses, https://github.com/freenet/freenet-core/pull/4600, https://stackoverflow.com/questions/64793785/do-the-github-api-rate-limits-apply-to-github-actions.

**Accepted resolution**: remove the live usage-snapshot smoke from blocking push CI and move it to a weekly scheduled workflow with `workflow_dispatch`, authenticated with `GITHUB_TOKEN` (`contents: read`) per the freenet-core precedent — a failure there is a maintainer drift alert, not a blocked push. The hermetic parts (shellcheck on `scripts/usage.sh` and `install.sh`, the fake-`uname` refusal tests) stay in push CI. The pinned-release install smoke also stays: it downloads a release asset through the CDN-backed redirect (no REST quota consumption, unlike API endpoints) and has been consistently green.

## 19. Terminal-width List Table Truncation (2026-08-24)

**The established convention is that terminal tables truncate to the terminal width only when standard output is a terminal; redirected or piped output prints complete values.** `docker ps`/`docker container ls` truncates columns to fit the terminal and documents `--no-trunc` as "don't truncate output"; when stdout is not a TTY the output is not truncated, because scripting consumers must never parse a truncated value. `kubectl get` likewise fits columns to the terminal width (truncating with no ellipsis), with `-o wide` adding columns and `kubectl describe` as the full detail view. `ps` fits columns to the TTY width with `+`/unlimited-width `ww` modes, and `git log` supports `%<(N,trunc)` formatting while full detail lives in `git show`. The shared rule across all four: the summary table is a fit-to-terminal view, a detail command carries complete values, and anything piped receives full data by default. Sources: https://docs.docker.com/reference/cli/docker/container/ls/, https://docs.docker.com/reference/cli/docker/container/ls/#no-trunc, https://kubernetes.io/docs/reference/kubectl/generated/kubectl_get/, https://man7.org/linux/man-pages/man1/ps.1.html, https://git-scm.com/docs/git-log#Documentation/git-log.txt-emltltNtruncemgt.

**The terminal width mechanism is the `TIOCGWINSZ` ioctl** — what docker and kubectl both use — not `$COLUMNS`. `$COLUMNS` is a shell variable that is unset under `script`, in many CI/automation contexts, and does not track window resizes; a TTY width query fails on a pipe, so the single ioctl serves both as the width source and as the TTY gate. The `locron` dependency graph already contains a crate implementing exactly this ioctl: `console` 0.16.4 (dialoguer 0.12's dependency) whose `Term::size_checked() -> Option<(u16, u16)>` performs `TIOCGWINSZ` and returns `None` when the stream is not a terminal. Verified with `cargo tree -p locron -i console` (console 0.16.4 → dialoguer 0.12.0 → locron) and the crate source in the local registry (size_checked at `term.rs:424`). Declaring `console` as a direct `locron` dependency therefore adds zero new lockfile entries, consistent with the workspace's repeated rejection of non-essential crates (`docs/IMPLEMENTATION.md` "Thin CLI composition").

**Column fitting must use character display width, not byte or character count.** East Asian wide characters (CJK, full-width forms) and many emoji occupy two columns, so `str::len()` or a character count would under-truncate or overflow by a factor of two on the very values this feature targets (Korean, Japanese, and emoji-bearing shell commands). `unicode-width` 0.2 is already in the workspace lockfile (transitively via console/clap), so the display-width calculation is also dependency-free; `console` itself gates it behind a default feature, which is why it is declared directly on `locron` rather than relied on through console.

**Accepted resolution**: truncate only the table's final data column (TARGET in `list`) to the terminal width when stdout is a TTY, marking the cut with a trailing `…`; print full values when redirected or piped; add a `--no-trunc` rendering flag restoring full values on a terminal; keep `show` as the complete detail view; leave machine output byte-identical. Truncating only the final column preserves alignment for every earlier column and keeps NAME (the key for every other command) copyable. Middle-column truncation and a new dedicated `terminal_size` crate were rejected; applying the same rule to the `history` table is deferred until a long TRIGGER value demonstrates the need.

## 20. Homebrew Formula Audit and Style Offenses (2026-08-24)

**The tap's `brew test-bot` failed on five consecutive formula bumps** (0.4.0 through 0.5.0, runs 32623542870, 32645284198, 32654919434, 32656598223, 32684723747) with the same two offenses, and the root cause is the formula template embedded in `release.yml`, so every future bump reproduces them until the template is fixed:

1. `brew style` — `FormulaAudit/Miscellaneous: No need for FileUtils. before touch` (`Formula/locron.rb:33`). The formula DSL exposes a `touch` helper, so the `FileUtils.` prefix is an offense. The template deviated from the plan text, which documents the marker line as `touch lib/.disable-self-update` (`docs/IMPLEMENTATION.md` "Accepted: tap formula marker and release pipeline").
2. `brew audit` — `Stable: version X.Y.Z is redundant with version scanned from URL`. When the stable URL contains the version marker, Homebrew detects the version from the URL, so an explicit `version` line is redundant. Source: https://docs.brew.sh/Versions.

The first fix attempt (run 32685506683) dropped the `version` line while keeping the `#{version}` placeholders in the URL template — and `brew readall --os=all --arch=all` rejected the formula on every macOS alias with `invalid attribute for formula 'whitekiwi/tap/locron': version (nil)`. The self-referential placeholder interpolates to nil during class-body evaluation, so no literal version token exists in the URL for Homebrew to scan. The canonical pattern — the dominant one for GitHub-release binary formulas in homebrew-core (e.g. `zoxide`) — is a URL that carries the **literal** version string (`.../download/v0.5.0/locron-v0.5.0-....tar.gz`) with no `version` line at all; Homebrew scans the version from the literal URL token. The generated formula can do exactly this because the template renders the version at generation time.

**Accepted resolution**: the `release.yml` template interpolates `${VERSION}` literally into all four URL strings, carries no `version` line, and writes the marker as `touch lib/".disable-self-update"` (the formula DSL helper, no `FileUtils.` prefix); the tap's current `Formula/locron.rb` receives the identical fix so the running `brew test-bot` goes green now. The behavior is unchanged — the marker file, the service block, and the caveats stay exactly as documented in `docs/RELEASE.md` §4.2. Note that the template is evaluated at the tagged commit, so this fix takes effect for the next tag, not retroactively for v0.5.0.

## 21. Homebrew 6 Tap Trust One-Line Install (2026-08-24)

**Homebrew 6.0 requires explicit trust before loading third-party tap content, and the documented escape hatch is the fully-qualified install: `brew install user/repo/formula` (or the `--cask` variant) auto-taps and records item-level trust, with no separate `brew tap` or `brew trust` step.** The official Tap-Trust docs present the fully-qualified one-liner as a self-contained step ("Installing a fully qualified formula or cask name trusts only that item") and recommend item trust over whole-tap trust ("Prefer trusting the specific formula, cask or command you need"), because whole-tap trust loads every current and future formula, cask, and external command from that tap. Prior art follows it: chattymin/poke-token-bar documents `brew install --cask chattymin/tap/poke-token-bar` with no tap or trust step anywhere in the README. Sources: https://docs.brew.sh/Tap-Trust, https://github.com/chattymin/poke-token-bar

**Trust is persistent item-level state, so short names keep working after the one-liner.** Verified against the local Homebrew 6.0.18 source: `cmd/install.rb` calls `Homebrew::Trust.trust_fully_qualified_items!` during install, and `trust.rb` writes the entries to `~/.homebrew/trust.json` (falling back to `$XDG_CONFIG_HOME/trust.json`) under a store lock. Subsequent short-name operations — `brew upgrade locron`, `brew services start locron`, `brew uninstall locron` — therefore resolve the formula from the tap without a trust prompt. Source: `/opt/homebrew/Library/Homebrew/cmd/install.rb` and `/opt/homebrew/Library/Homebrew/trust.rb` (Homebrew 6.0.18, 2026-08-24)

**Accepted resolution**: the README and install guide replace `brew tap whitekiwi/tap && brew trust whitekiwi/tap && brew install locron` with the one-liner `brew install whitekiwi/tap/locron`. locron is a CLI formula, so no `--cask` flag; the trust explanation moves into the install guide, and the whole-tap trust previously granted by `brew trust whitekiwi/tap` is replaced by formula-only trust, matching the official recommendation.

## 22. Locron Brand and Dashboard Visual Direction (2026-08-24)

### Source verification

The user-supplied references were checked against their official site or repository rather than
treated as a single prescriptive style. They divide into technique libraries, inspiration
galleries, and repeatable design-system workflows:

| Reference | Verification and useful evidence |
|---|---|
| CloudAI-X/threejs-skills | The public repository exists and covers Three.js fundamentals, geometry, materials, lighting, animation, shaders, post-processing, interaction, and performance. It is useful when a product needs a real 3D scene, but supplies no evidence that an operations dashboard benefits from one. Source: https://github.com/CloudAI-X/threejs-skills |
| greensock/gsap-skills | GreenSock's official repository exists and covers core tweens, timelines, ScrollTrigger, framework lifecycle cleanup, and performance. It supports deliberate choreography when native CSS is insufficient; it is not itself a reason to add animation or a runtime dependency. Source: https://github.com/greensock/gsap-skills |
| LottieFiles/motion-design-skill | The official repository exists. Its workflow chooses purpose, properties, timing, easing, emotional intent, and brand personality before implementation, and includes accessibility/performance adaptation and state-feedback patterns. Source: https://github.com/LottieFiles/motion-design-skill |
| AThevon/genjutsu | The public repository exists. Its `paint` workflow establishes a visual and interaction thesis, persists a design system, then audits motion, accessibility, responsive behavior, color consistency, and performance. Its `cast` workflow similarly audits reduced motion and interaction performance after focused motion work. Source: https://github.com/AThevon/genjutsu |
| Design Spells | The official gallery exists and categorizes small interface details by device, interaction, motion, animation, 404, skeuomorphism, and other themes. Its value is selective delight at a meaningful moment, not wholesale adoption of every showcased effect. Source: https://www.designspells.com/ |
| recent.design | The official site exists and is a daily curation spanning interface, branding, product, typography, motion, illustration, 3D, editorial, print, and packaging. It is a broad mood-board source, not a product or accessibility standard. Source: https://recent.design/ |
| wwit | `wwit.design` exists as What Was IT and organizes Korean product references by industry, user-flow pattern, and component type, including onboarding, loading, success, lists, settings, inputs, tabs, popups, and charts. That taxonomy is useful for comparing one Locron flow at a time rather than copying an entire app. Source: https://wwit.design/ |
| film.ai | The supplied domain currently resolves to a domain-sale page, not a searchable film-frame reference. It is unavailable as a current design source and no prior design is inferred from archived or secondary descriptions. Source: https://film.ai/ |
| post.design | The official URL responded but exposed no verifiable page content in this research session. Search results conflate a current social-post curation description with the older Post.Design/Tangle platform, so neither is used as evidence for Locron. Source checked: https://post.design/ |
| MengTo/Skills | Meng To's public repository exists. Its stated philosophy is to treat prompts as versioned assets, prefer specifications and references over vague direction, iterate by changing a small number of variables, and include defaults, pitfalls, demos, and acceptance checks. Its design-first workflow structures direction as goal, format, layout, type, color, and constraints. Source: https://github.com/MengTo/Skills |
| Vercel `DESIGN.md` | No first-party `DESIGN.md` file was found in the official Vercel repositories checked; community reconstructions must not be presented as Vercel-authored. The first-party substitutes are the Geist design system and Vercel Web Interface Guidelines, which document accessible color, typography, icons, grid, interaction, focus, hit targets, borders, radii, and layered elevation. Sources: https://vercel.com/geist/stack, https://vercel.com/design/guidelines |
| nextlevelbuilder/ui-ux-pro-max-skill | The public repository exists. The strongest transferable parts are persistent master rules with page-level overrides, product-specific anti-patterns, resilient text wrapping, status meaning beyond color alone, visible focus, correct semantics after interrupted motion, and reduced-motion support. The large catalog of fashionable styles is reference data, not a mandate to mix styles. Source: https://github.com/nextlevelbuilder/ui-ux-pro-max-skill |

Two established brand guides add what the technique repositories do not. GitHub's official Brand
Toolkit treats logo, typography, color, illustration, mascots, accessibility, and distinct
marketing/product systems as one identity; it also asks whether a moment is expressive or
functional before choosing the treatment. That is a direct precedent for keeping Roki and the
hand-drawn language while making the authenticated dashboard denser and quieter. Sources:
https://brand.github.com/, https://brand.github.com/guides/getting-started.

Mailchimp's public content guide separates stable voice from situation-dependent tone, prioritizes
clarity over entertainment, and reserves humor for moments where it will not obstruct the user's
work. This is the useful model for a friendly mascot-bearing developer product: onboarding may be
warm, while a failed, interrupted, or destructive state must be direct and calm. Sources:
https://styleguide.mailchimp.com/voice-and-tone/,
https://styleguide.mailchimp.com/tldr/.

### Existing Locron identity

The repository's `assets/banner.jpg` is already a stronger source of truth than a trend gallery.
It establishes:

- a warm cream canvas, near-black rounded display lettering, graphite secondary text, restrained
  gray surfaces, and sunny yellow as the recognitional accent;
- Roki as a small friendly robot with yellow eyes and antenna, accompanied by hand-drawn marks,
  a sleepy cat, books, a laptop, and handwritten encouragement;
- a product promise that is local, easy to manage, private, secure, and built for developers; and
- a friendly, optimistic tone that remains simple rather than loud.

The banner is expressive marketing art. Its motifs should enter the product as controlled accents,
not make every table, form, status, or log pane look hand-drawn. GitHub's expressive-versus-
functional distinction supports this split.

### Recommendation: Locron brand system

Define the brand promise as **calm local control that explains itself**. Four attributes should
govern every choice: **warm, precise, capable, and reassuring**. The warm and reassuring half comes
from the cream/yellow/Roki banner; precise and capable come from durable run facts, local ownership,
and the existing “Cron that explains itself” positioning.

- **Voice.** Plainspoken, concise, developer-to-developer, and specific about what happened, why,
  and what to do next. Use active verbs and sentence case. Friendly microcopy belongs in entry,
  onboarding, success, and empty states. Errors, security, cancellation, interrupted outcomes, and
  destructive confirmations use neutral factual language with no joke or mascot dialogue.
- **Logo and character.** Preserve the `Locron` name as the primary identifier and Roki as a
  supporting character, never as a replacement for the wordmark. Keep clear space around either,
  do not distort or recolor Roki arbitrarily, and use the character at high-empathy moments such as
  login, first-job onboarding, and a truly empty dashboard. Avoid repeating Roki in dense lists or
  placing playful art beside dangerous actions.
- **Color roles.** Warm cream is the application canvas, light neutral is the working surface,
  charcoal is primary ink and strong action, graphite is secondary text, and sunny yellow is the
  brand accent/focus highlight. Success, warning, failure, running, and unknown outcomes retain
  separate accessible semantic colors plus text/icon labels. Yellow must not mean both “Locron” and
  “warning,” and no status may rely on color alone.
- **Typography.** Use a rounded, confident display treatment only for the Locron identity and
  occasional empty-state headline. Use the system sans-serif stack for operational reading and a
  system monospace stack for schedules, IDs, timestamps, commands, and logs. Maintain a small,
  explicit scale and tabular numerals where values must compare vertically; do not add an external
  font or CDN to the bundled local dashboard.
- **Shape and illustration.** Favor soft but disciplined radii, crisp charcoal or neutral borders,
  minimal layered shadow, and small hand-drawn yellow strokes as signatures. Icons use one coherent
  outline weight. Cream paper texture, doodles, cat/books, and Roki are occasional editorial
  elements, not repeated card decoration.
- **Layout.** Use a stable grid and spacing scale, large calm outer margins, and compact internal
  data rhythm. The first visual scan of any dashboard view should answer: current state, next
  occurrence, latest anomaly, then available action. One filled primary action per decision area is
  enough; secondary and destructive actions recede until needed.
- **Components and state.** Treat jobs, runs, diagnostics, forms, status chips, notices, empty
  states, destructive confirmations, and the log console as a documented family with the same
  type, spacing, radii, focus, hover, disabled, loading, error, and empty-state rules. Long names,
  IDs, URLs, tags, and translated copy must wrap or expose their full value without clipping.
- **Motion personality.** Use “snappy-gentle” motion for feedback and hierarchy: short hover/press,
  route/state transition, disclosure, and fresh-data acknowledgement. Prefer opacity and transform,
  let the final semantic state remain correct when interrupted, and honor `prefers-reduced-motion`.
  Do not add ambient loops, scroll spectacle, cursor trails, parallax, 3D, or bouncing failure
  states to an operational dashboard. The current plain HTML/CSS/JavaScript viewer can express the
  required motion without GSAP, Lottie, or Three.js.
- **Accessibility and responsive behavior.** Preserve visible focus, full keyboard operation,
  useful landmarks and labels, 44 px touch targets where the interface becomes touch-oriented,
  16 px mobile input text, sufficient text/status contrast, and a layout that works under narrow
  widths, zoom, text scaling, and reduced motion. Decorative handwriting and yellow accents never
  carry essential information.

The durable brand guide should record the promise and attributes; voice/tone; wordmark and Roki
usage; palette roles and tested contrast pairs; type scale; icon/illustration rules; grid, spacing,
radii, border, and elevation tokens; component states; motion rules; accessibility; and concrete
do/don't examples. A small rendered specimen of real dashboard components is more useful than a
palette page alone, matching Meng To's preference for references plus acceptance checks and
Genjutsu's preview-and-audit approach.

### Dashboard application and rejected excess

The dashboard should therefore feel like the banner grew into an operations tool: a cream shell,
charcoal hierarchy, quiet neutral work surfaces, one yellow recognitional accent, and sparse Roki or
hand-drawn moments around onboarding. Data tables, forms, diagnostics, and logs remain restrained
and highly legible. The dark log console is the intentional technical counter-surface, linked back
to the brand through typography, focus, and a small yellow live indicator rather than decorative
effects.

Reject the following despite their presence in inspiration catalogs: generic purple/pink “AI”
gradients, glassmorphism over dense data, neumorphism with weak boundaries, decorative 3D/WebGL,
cinematic scroll narratives, continuous ambient animation, heavy blur, excessive shadows, a card
around every value, rainbow status palettes, yellow-as-warning ambiguity, and copying Vercel or any
gallery example as a visual identity. The goal is an original Locron system derived from its own
banner and product promise, using external references only as decision frameworks and quality
checks.

## 23. Restrained glass and peer-theme evolution (2026-08-25)

The Locron banner remains the identity source, but an authored peer dark scheme can carry the same
cream/charcoal/yellow recognition through semantic roles rather than merely inverting the light
palette. Glass is useful only as restrained shell hierarchy: a translucent header, navigation,
theme/filter controls, and transient overlays over a solid fallback. Dense data, forms, notices,
destructive actions, and the console need opaque surfaces. Local Geist variable fonts provide a
precise product voice without a runtime request; Korean and unavailable-font fallbacks remain
system-native. Reduced transparency must remove blur and luminosity just as reduced motion removes
nonessential transitions.

## 24. Run search, settings IA, and exact input grammar (2026-08-25)

SQLite's default case folding is ASCII-focused and LIKE treats `%` and `_` as wildcards. The
accepted literal, Unicode-aware behavior therefore reads runs joined to durable job rows in one
read transaction, applies Rust Unicode lowercase plus literal `contains`, then computes total and
the requested page. This keeps removed jobs searchable because their rows are retained. The run
snapshot contains no historical job name, so a renamed job is searchable by its current durable
name; pre-rename names are explicitly out of scope. Stable order is `requested_at_us DESC, id
DESC`, and neither filtering nor total is capped at 1000.

Settings deserves its own route because durable execution/retention/environment mutation and
browser-local appearance are operational configuration, not diagnostics. Diagnostics remains a
read-only explanation surface. Native radio/checkbox/select elements preserve keyboard and form
semantics; styling them is safer than recreating listboxes. Decimal duration conversion must be
string-and-BigInt based: at most six decimal places, one optional s/m/h/d suffix, exact
microseconds, a safe JSON-number boundary, and field-specific empty/zero rules.

## 25. Operator-facing inputs, supplied visual references, and typed frontend (2026-08-25)

An inventory of every editable control found four remaining storage encodings in primary
workflows: byte counts, epoch microseconds, colon-delimited paths, and several comma/line mini
grammars. It also found policy inputs displayed while inapplicable, placeholder-only list filters,
bottom-only validation, internal setting keys in success copy, and terse choices without their
operational consequence. The accepted rule is that the ordinary workflow speaks in operator
concepts and the wire representation remains an advanced detail.

Kubernetes quantity documentation supplies the strongest size precedent: byte quantities use
fixed-point parsing, distinguish decimal M from binary Mi, and explicitly avoid floating point
(https://kubernetes.io/docs/reference/kubernetes-api/definitions/quantity-resource/ and
https://kubernetes.io/docs/concepts/configuration/manage-resources-containers/). Locron therefore
uses a magnitude plus B/KiB/MiB/GiB unit, accepts a pasted suffix, converts with decimal strings and
BigInt, and shows an exact byte equivalent. The two labels become `Total retained output` and
`Output retained per run`. Zero is not `Unlimited`: total zero makes completed output eligible for
pruning, while per-run zero drains but retains no payload and records truncation. Those effects
must be stated before saving.

Retention and policy controls need consequence-aware disclosure. GitHub exposes artifact/log
retention as days with a documented range and default
(https://docs.github.com/en/enterprise-cloud@latest/organizations/managing-organization-settings/configuring-the-retention-period-for-github-actions-artifacts-and-logs-in-your-organization),
and AWS CloudWatch distinguishes finite retention from never expire
(https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/Working-with-log-groups-and-streams.html).
Locron's nullable age contract therefore needs an explicit `No age limit`; zero remains immediate
expiry rather than off. Per-job concurrency is relevant only for overlap Allow, catch-up count only
for missed All, and delay/backoff/cap only after retries are enabled. Retry copy should state that
zero retries still means one initial attempt and preview the sequence.

Raw instants are also removed from the primary schedule flow. MDN notes that `datetime-local`
provides no timezone itself (https://developer.mozilla.org/en-US/docs/Web/HTML/Reference/Elements/input/datetime-local),
so Locron pairs local date/time with its explicit timezone choice and an absolute-time preview.
AWS Scheduler separates rate, cron, and one-time schedules and documents IANA timezone/DST behavior
(https://docs.aws.amazon.com/scheduler/latest/UserGuide/schedule-types.html). Cron gets next-run
feedback; Every is described as elapsed time; epoch microseconds remain an advanced diagnostic
value only.

Grafana Saga reinforces the form system rather than a particular dark palette. Its Field requires
labels, descriptions, and adjacent validation (https://grafana.com/developers/saga/components/field),
while its form pattern recommends one-column grouping, smart defaults, progressive disclosure,
and separating destructive actions (https://grafana.com/developers/saga/patterns/forms/). Placeholder
text is not a substitute for a label (https://grafana.com/developers/saga/components/input/).
GOV.UK similarly links the summary to the affected field and keeps a matching inline error
(https://design-system.service.gov.uk/components/error-summary/). The dashboard therefore uses a
shared Field contract, focuses the first invalid control, preserves values, replaces alerts with
inline retryable feedback, and keeps advanced/raw values collapsed.

The supplied visual references were inspected in a real browser. `portfolio.whitekiwi.link` uses a
near-black canvas, oversized condensed display type, restrained amber focus, faint oversized
background lettering, thin dividers, and one dominant terminal workbench. The public unauthenticated
`platform.deepseek.com/usage` surface uses a quiet split layout, soft cool artwork on one side,
near-black form surface on the other, generous whitespace, rounded outlined fields, and a single
high-contrast primary action; its authenticated usage view was not accessible. Grafana contributes
dense operational hierarchy, persistent toolbars, and restrained panels. Locron adopts the shared
principles—strong type hierarchy, quiet surfaces, one dominant workbench, disciplined spacing, and
sparse accent—without copying logos, imagery, or product palettes. Glass remains shell-only.

The existing browser code is 2,827 lines and the job view alone is 1,146 lines. This has crossed
the point where page-sized string templates and manual DOM state remain easier than components.
The accepted stack is Vite, React, and strict TypeScript. React's component/state guidance favors a
component hierarchy aligned to the data model and one owner for each state
(https://react.dev/learn/thinking-in-react and
https://react.dev/learn/sharing-state-between-components/). Vite officially supports the React
TypeScript template and produces static production assets
(https://vite.dev/guide/ and https://vite.dev/guide/build.html); TypeScript is checked separately
because Vite transpilation is not the type gate
(https://vite.dev/guide/features.html#typescript and
https://www.typescriptlang.org/tsconfig/noEmit.html).

Node remains a development and CI tool, never an install or runtime dependency. Exact package and
Node versions are pinned, `npm ci` uses the committed lock, strict type checking and component tests
run before build, and a clean build must exactly reproduce the committed production tree. Rust
embeds only that tree. Cargo must not invoke npm, `cargo package` must contain it, and a packaged
install must serve successfully without Node. Initial production dependencies are limited to React
and React DOM; native fetch, EventSource, hash routing, form controls, and CSS remain sufficient.
No router, query cache, general UI kit, Redux, Axios, CDN, or runtime font request is justified.

Browser chrome completes the identity. A local, original charcoal/amber SVG favicon must remain
recognizable at small sizes. Separate light/dark `theme-color` metadata follows the resolved scheme
as described by MDN (https://developer.mozilla.org/en-US/docs/Web/HTML/Reference/Elements/meta/name/theme-color),
and document titles use concise route-first names such as `Run history · Locron` without job names,
tokens, or other mutable operator data.

## 26. Modern operator cockpit, accessible primitives, and exact visual tokens (2026-08-25)

### Evidence boundary and reference translation

The supplied portfolio and public DeepSeek shell were already inspected directly in §25. A second
research pass could not obtain semantic content from the portfolio, while DeepSeek's authenticated
Usage route remained access-controlled; no private chart or billing layout is inferred. The accepted
translation is therefore conservative:

- the portfolio contributes one dominant workbench, confident type, thin dividers, and sparse amber
  emphasis, but not giant background lettering or marketing-scale type behind operational content;
- DeepSeek contributes quiet near-solid surfaces, generous outer space, aligned outlined controls,
  and one high-contrast primary action, but no copied navigation, palette, or authenticated layout;
- Grafana Saga contributes the operational model. Its navigation guidance separates global IA from
  page-local structure, its table template places actions above comparison data, its object-list
  guidance supports a purpose-built narrow layout, and its forms use left-aligned one-column groups,
  progressive disclosure, 16 px component rhythm, and roughly 40 px between major sections.
  Locron's four destinations need a simple persistent rail rather than Grafana's megamenu. Sources:
  https://grafana.com/developers/saga/patterns/navigation/,
  https://grafana.com/developers/saga/templates/table/,
  https://grafana.com/developers/saga/templates/lists-of-objects/,
  https://grafana.com/developers/saga/patterns/forms/, and
  https://grafana.com/developers/saga/foundations/design-principles/.

### Accessible headless primitives

Adopt the individual MIT-licensed Radix React Select, DropdownMenu, Dialog, and Tooltip packages,
not Radix Themes or another full UI kit. Locron continues to own its CSS, semantic tokens, markup
wrappers, and component names. The packages' current peer ranges include React 19; Select must be
pinned at 2.3.2 or later because that release records the React 19 infinite-render correction.
Sources: https://github.com/radix-ui/primitives,
https://github.com/radix-ui/primitives/blob/main/LICENSE, and
https://github.com/radix-ui/primitives/blob/main/packages/react/select/CHANGELOG.md.

- **Select** is for fixed enumerations such as units, HTTP method, and compact policy values. It
  supplies managed focus, arrow keys, typeahead, disabled items, collision positioning, and portal
  rendering. Radios remain better for two to four consequential choices that benefit from immediate
  comparison. A searchable timezone chooser is deferred: WAI-ARIA distinguishes the substantially
  larger editable-combobox contract. Sources: https://www.radix-ui.com/primitives/docs/components/select
  and https://www.w3.org/WAI/ARIA/apg/patterns/combobox/.
- **DropdownMenu** is for commands, especially stable row action triggers, never a persisted form
  value. It must expose a named real button, verb-labelled items, a separated destructive group,
  Escape/outside dismissal, and logical focus return. No nested submenu is needed initially.
  Sources: https://www.radix-ui.com/primitives/docs/components/dropdown-menu and
  https://www.w3.org/WAI/ARIA/apg/patterns/menu-button/.
- **Dialog** is for short blocking decisions such as remove, cancel acknowledgement, and pruning
  review, never the long job form. Title/description, focus trapping, cyclic Tab, Escape, visible
  close/cancel, and focus return are required; irreversible dialogs initially focus the least
  destructive action. Sources: https://www.radix-ui.com/primitives/docs/components/dialog and
  https://www.w3.org/WAI/ARIA/apg/patterns/dialog-modal/.
- **Tooltip** supplements named icon buttons after a 600 ms first-open delay and 300 ms skip delay.
  It never contains interactivity or replaces visible validation, consequence, status, or
  instructions. Source: https://www.radix-ui.com/primitives/docs/components/tooltip and
  https://www.w3.org/WAI/ARIA/apg/patterns/tooltip/.

All layers portal into one application-owned root adjacent to the React root. Tokens therefore live
on `:root`; the fixed depth scale is sticky 10, menu/tooltip 30, overlay 40, dialog 50. Routes close
open layers. Popup tests cover viewport bounds, nested scroll, outside pointer, Escape, background
inertness, and trigger focus restoration. Portals prevent clipping in tables and sticky containers,
but their inheritance, stacking, scroll-lock, and focus behavior remain explicit regression scope.

Adopt pinned `lucide-react` with direct named imports only. Lucide supplies optimized inline SVG,
TypeScript support, tree shaking, and a coherent outline language without a runtime request. Use
16 px icons normally, 18 px in primary navigation, 20 px only in empty states, 1.75 stroke, and
`currentColor`; decorative icons are hidden, icon-only buttons retain accessible names, and status
always keeps text. Roki, the wordmark, and favicon remain original assets. Sources:
https://lucide.dev/guide/react and https://github.com/lucide-icons/lucide/blob/main/LICENSE.

### Exact visual system

Amber is brand focus and selection, not warning. The accepted core tokens are:

| Token | Light | Dark | Role |
|---|---:|---:|---|
| `canvas` | `#F7F5EF` | `#151512` | application background |
| `surface` | `#FCFBF7` | `#1C1C18` | main workbench |
| `raised` | `#FFFFFF` | `#24231E` | menus and dialogs |
| `border` | `#D9D5CA` | `#3A3931` | passive divider |
| `border-control` | `#8D887E` | `#747164` | interactive boundary |
| `text` | `#211F1A` | `#F3F0E8` | primary foreground |
| `muted` | `#6A655B` | `#AAA69B` | secondary foreground |
| `accent` | `#E3A91D` | `#E4AD2B` | brand marker/selection |
| `accent-text` | `#7A4A00` | `#F0BD4C` | amber-associated text |
| `accent-soft` | `#FFF0C2` | `#3A2C0D` | active background |
| `on-accent` | `#241A00` | `#201800` | foreground on amber |
| `focus` | `#B87500` | `#E4AD2B` | focus ring |
| `primary` | `#211F1A` | `#F3F0E8` | primary button |
| `on-primary` | `#FFFFFF` | `#151512` | primary button text |

Key calculated ratios are light text/canvas 15.1:1, muted/canvas 5.31:1,
on-accent/accent 8.14:1; dark text/canvas 16.06:1, muted/canvas 7.52:1, and
on-accent/accent 8.65:1. Control borders exceed 3:1 against their ordinary surface. Status pairs
are success `#176B4C/#E7F5EE` and `#70D4A7/#193329`; warning
`#795000/#FFF3CC` and `#F0BD4C/#382B0D`; danger `#A73531/#FCECEA` and
`#F07872/#3D211F`; info/running `#245E8C/#EAF3FC` and `#83B9EB/#1D2E3D`.
Every status includes text. Sources: https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html
and https://www.w3.org/WAI/WCAG22/Understanding/non-text-contrast.html.

Use spacing `4, 8, 12, 16, 24, 32, 40, 48, 64 px`; radii `4 px` for status,
`6 px` controls, `8 px` menus/sections, and `12 px` dialogs. Pills are limited to status and compact
filters. Workbenches have no shadow, sticky chrome has only a divider, popups use
`0 12px 30px rgb(0 0 0 / 0.14)`, and dialogs use `0 20px 56px rgb(0 0 0 / 0.22)`.
Desktop rail and work surfaces are opaque. Only mobile sticky chrome may use a 92–94% surface and
at most 10 px blur, with an opaque reduced-transparency fallback.

Geist Sans remains local with `Pretendard`, `Apple SD Gothic Neo`, `Noto Sans KR`, and system
fallbacks. Do not add a condensed display font. Page titles are 24/30 at weight 650, section titles
18/24 at 650, subsections 15/22 at 600, body/controls 14/20, labels/buttons 13/18 at 600, dense
table/meta 12/16 or 13/18, and code/IDs 13/20 mono. No body text is below 12 px and comparable
numbers use tabular figures.

Compact controls are 36 px, normal form controls 40 px, multiline fields at least 96 px, desktop
icon buttons 32 px, table headers 36 px, rows 44 px, and all touch controls at least 44 px. Motion
uses an 80 ms press, 120 ms hover/focus, 160 ms disclosure/menu, and 200 ms dialog/state duration
with `cubic-bezier(.2, 0, 0, 1)`, animating opacity/transform only. Reduced motion removes transform
and uses immediate or at most 80 ms opacity changes.

### Application blueprint

At 1024 px and wider, use an opaque 224 px left rail, 64 px brand/header block, 40 px nav items,
and a bottom utility region for named daemon health and Settings/theme access. Route headers are at
least 64 px; content padding is 32 px, reduced to 24 px below 1280. The workbench may reach 1440 px,
while forms remain bounded. At 768–1023 px the rail is 64 px with labelled tooltips and a persistent
active marker.

Below 768 px, use a 56 px top bar and four equal labelled bottom destinations plus the safe-area
inset. Content padding is 16 px and primary actions remain labelled in the header; no hamburger-only
navigation or floating icon-only action is needed. Dialog width becomes `calc(100vw - 32px)` and
200% zoom naturally enters the narrow composition. This supports WCAG reflow without two-dimensional
page scrolling: https://www.w3.org/WAI/WCAG22/Understanding/reflow.html.

Desktop Jobs and Run history use a wrapping toolbar, a 240–420 px growing search, following filters,
live result state, and the primary action at the far right. Rows use aligned tabular values and a
stable overflow-menu column. Below a 760 px container width, render semantic object rows—not hidden
columns or a horizontally scrolling page—with name/status, schedule/next occurrence, latest
run/trigger time, a named action menu, and a detail link. Dividers, not floating cards, separate rows.

The job form uses a 720 px primary measure and a 176 px sticky section rail on wide screens. Its
sections are Identity, Schedule, Target, Environment, Policy, and Review, separated by 40 px, with
20 px field gaps and 8 px label-to-control gaps. Paired controls use two columns only when both keep
at least 240 px. A solid sticky action bar is at least 64 px, exposes review/save, cancel, dirty, and
saving state, and reserves enough page padding not to obscure the final field. Settings reuses this
system, visually separates browser-local Appearance from durable policy, and puts pruning/destructive
old→new consequences in a short review dialog.

### Adopted and rejected

Adopt selective Radix primitives, direct-import Lucide icons, a 224/64 px adaptive rail, labelled
four-item mobile bottom navigation, mobile object rows, one-column long forms, and sticky actions.
Reject a full themes/UI kit, custom combobox/listbox, icon fonts/CDNs, glass workbenches, authenticated
marketing heroes, ambient gradients/glow/texture, a card around every field/value, amber warnings,
tooltip-only instructions, icon-only core actions, hover-only row actions, nested dialogs/submenus,
parallax, ambient loops, and long decorative animation. The final matrix covers both themes at
desktop, compact rail, mobile, and 200% zoom across entry, every route, tables/object rows, Select,
menu, tooltip, dialog, validation, loading, empty, error, destructive review, keyboard-only use,
reduced motion, and empty developer logs.

## 27. Finish quality: code, type, material, rows, and perceptual color (2026-08-25)

### Changed decisions and reference boundary

This amendment refines §25–26. JSON becomes a purpose-built read-only viewer instead of a generic
monospace block. Glass expands from mobile chrome to genuinely overlapping sticky and transient
layers in both themes, while persistent rails and content remain opaque. Job and Run title links
remain the semantic destinations, but the noninteractive row surface becomes a pointer target; the
row never becomes a duplicate keyboard widget.

Public AI guidance is useful as a state-audit method, not as an aesthetic preset. Microsoft HAX and
Google PAIR emphasize expectations, explanation, recovery, and action-oriented testable patterns;
Locron transfers that method without presenting itself as an AI product. Vercel's current interface
guide adds directly relevant native semantics, visible focus, inline help before tooltips, tabular
numbers, resilient content, labelled status, and explicit motion properties. Sources:
https://www.microsoft.com/en-us/haxtoolkit/ai-guidelines/,
https://pair.withgoogle.com/guidebook-v2/, and https://vercel.com/design/guidelines.

The supplied skills remain process references. MengTo/Skills supports explicit reference boundaries,
constraints, pitfalls, and acceptance checks; Genjutsu supports an interaction thesis and state
audit but not cinematic effects; LottieFiles requires purpose, timing, easing, accessibility, and
performance before motion but does not justify a Lottie dependency; UI UX Pro Max contributes focus,
responsive, status, and reduced-motion checks but its style catalog is not authority to mix trends.
Design Spells, recent.design, and wwit remain technique and flow galleries rather than accessibility
or product-identity standards. Sources: https://github.com/MengTo/Skills,
https://github.com/AThevon/genjutsu, https://github.com/LottieFiles/motion-design-skill,
https://github.com/nextlevelbuilder/ui-ux-pro-max-skill, https://www.designspells.com/,
https://recent.design/, and https://wwit.design/.

### Dependency-free exact JSON viewer

Do not add Monaco, CodeMirror, or Shiki for this read-only JSON requirement. Monaco is an editor with
workers and editor focus behavior and does not support mobile browsers. CodeMirror itself recommends
avoiding `EditorView` when only syntax presentation is required. Shiki is accurate, but its presets
and grammar/theme/regex-or-WASM lifecycle are disproportionate for one language. CodeMirror becomes
the future candidate only if search, fold navigation, diffing, or editing becomes a real product
need. Sources: https://github.com/microsoft/monaco-editor,
https://codemirror.net/examples/readonly/, and https://shiki.style/guide/bundles.

Implement a small RFC 8259 lexer over the original string; never parse and reserialize, which can
change whitespace, line endings, key order, escapes, duplicate keys, and numeric spelling. Tokenize
strings/escapes, numbers, `true`/`false`/`null`, punctuation, and whitespace; a string is a key only
when the next non-whitespace token is a colon. Render React text nodes inside `<pre><code>`, never
HTML injection. Syntax roles use color plus weight/opacity, so plain text remains intelligible with
color removed. Source: https://www.rfc-editor.org/rfc/rfc8259.

The compact opaque header exposes `JSON`, exact Copy with visible status, and a Wrap toggle. Content
uses local Geist Mono 13/20, weight 430, zero tracking, disabled ligatures, and tabular numbers. Line
numbers are hidden from assistive technology; AT receives one continuous code value. Copy writes the
untouched source. Invalid JSON shows `Invalid JSON`, preserves the exact plain source, and remains
copyable. Above 200 lines or 64 KiB, initially show the first 80 complete lines with `Show all N
lines`; full source remains in memory for copy. Expanded content is internally bounded to 480 px
desktop and 360 px narrow. Wrap defaults off and persists locally. Tests cover escapes, Unicode,
CRLF, duplicate keys, exponents, invalid input, literal markup, exact copy, themes, zoom, and limits.

### Application typography

Geist's typography system defines each role with size, line-height, tracking, and weight, separates
multiline Copy from single-line Label, and uses tabular figures for data. Geist Sans/Mono remain the
locally licensed bundled fonts; Korean uses native fallbacks rather than another assumed or newly
bundled font. Sources: https://vercel.com/geist/typography,
https://github.com/vercel/geist-font, and https://github.com/vercel/geist-font/blob/main/LICENSE.txt.

Use `"Geist Sans", -apple-system, BlinkMacSystemFont, "Apple SD Gothic Neo", "Noto Sans KR",
"Malgun Gothic", system-ui, sans-serif` and `"Geist Mono", ui-monospace, SFMono-Regular, Menlo,
Consolas, monospace`. Set optical sizing, normal kerning, no synthetic faces, normal ligatures, and
tabular numeric columns. The exact application roles are:

| Role | Size / line / weight / tracking | Mixed Korean/Latin tracking |
|---|---|---|
| empty/display | 32 / 40 / 650 / `-.025em` | `-.012em` |
| page title | 24 / 32 / 650 / `-.018em` | `-.010em` |
| section title | 18 / 26 / 620 / `-.012em` | `-.006em` |
| subsection | 15 / 22 / 600 / `-.006em` | `0` |
| nav/menu/control | 14 / 20 / 540 / `-.006em` | `0` |
| body/copy | 14 / 21 / 420 / `-.003em` | `0` |
| field label | 13 / 18 / 560 / `-.004em` | `0` |
| table primary | 13 / 18 / 500 / `-.003em` | `0` |
| metadata/caption | 12 / 17 / 450 / `+.005em` | `0` |
| JSON/code | 13 / 20 / 430 / `0` | `0` |
| mobile input | 16 / 22 / 420–500 / `0` | `0` |

Use variable weights rather than a coarse 400/700 rhythm. Never apply all-caps or negative tracking
to Korean operational labels. Align icon/text by baseline with at most a one-pixel optical
adjustment rather than loose padding.

### Tooltip, material, and whole-row interaction

Labelled desktop and mobile navigation render no Tooltip and no duplicate `title`. Hover changes
only surface, text/icon contrast, and the active marker. Tooltips remain permitted for the 64 px
icon-only rail and genuinely icon-only supplemental controls; the trigger owns an independent
accessible name. APG keeps focus on the trigger, Escape dismisses, and no focusable content appears
inside. WCAG also requires custom hover/focus content to be dismissible, hoverable, and persistent.
Sources: https://www.w3.org/WAI/ARIA/apg/patterns/tooltip/,
https://www.w3.org/WAI/WCAG22/Understanding/content-on-hover-or-focus.html, and
https://vercel.com/design/guidelines.

Apple reserves material for functional controls/navigation and warns against content-layer overuse;
Microsoft reserves Acrylic for transient light-dismiss surfaces and opaque Mica-like bases for
long-lived content; Vercel distinguishes page surfaces from tooltip/menu/modal elevation and avoids
stacking material. Therefore desktop rail, workbench, table, form, code, notice, and status surfaces
remain opaque. Glass is limited to sticky chrome with content scrolling behind it, mobile bottom
navigation, and transient menus/popovers/tooltips. Dialog content remains opaque over smoke. Sources:
https://developer.apple.com/design/human-interface-guidelines/materials,
https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/materials, and
https://vercel.com/geist/materials.

| Material | Light | Dark |
|---|---|---|
| sticky chrome | `rgb(252 251 247 / .86)`, blur 14px, saturate 108% | `rgb(28 28 24 / .82)`, blur 14px, saturate 108% |
| transient | `rgb(255 255 255 / .92)`, blur 16px, saturate 110% | `rgb(36 35 30 / .90)`, blur 16px, saturate 110% |
| hairline | inset `0 1px rgb(255 255 255 / .72)`, outer `rgb(33 31 26 / .10)` | inset `0 1px rgb(255 255 255 / .10)`, outer `rgb(255 255 255 / .12)` |
| local shadow | `0 10px 28px rgb(33 31 26 / .10)` | `0 12px 32px rgb(0 0 0 / .32)` |
| solid fallback | `#FCFBF7` / `#FFFFFF` | `#1C1C18` / `#24231E` |
| modal smoke | `rgb(21 21 18 / .38)` | `rgb(0 0 0 / .58)` |

Set the solid value first and enhance under `@supports`. Forced colors, increased contrast, reduced
transparency, and an app-level solid-material hook remove blur. Blur never exceeds 16 px, saturation
110%, or two glass layers in one sightline; no background decoration is added merely to expose blur.

For row navigation, retain native table/header/cell semantics and one real anchor in the primary
cell. Jobs append a screen-reader-only “view job details”; Runs expose the full ID and job/time
context to AT even when visually shortened. Each menu is separately named for the row. Pointer
delegation activates the real link only for a primary unmodified click on noninteractive row space,
with no text selection. It ignores prevented events, modifiers, non-primary buttons, and origins in
links, buttons, inputs, selects, textareas, or menu items. Never wrap a row in an anchor, give the
row `role=link`/`tabindex`, or stretch a pseudo-element over nested controls. Sources:
https://www.w3.org/WAI/tutorials/tables/,
https://www.w3.org/WAI/ARIA/apg/patterns/link/, and https://vercel.com/design/guidelines.

### Perceptual color roles

Radix assigns steps 1–2 to backgrounds, 3–5 to normal/hover/active component surfaces, 6–8 to
separators/control borders/focus, and 11–12 to secondary/primary text. Geist independently separates
page backgrounds, component states, border states, high-contrast fills, and accessible text/icon
roles. Locron follows that anatomy with warm sand/olive neutrals and a separate custom amber scale;
it does not add a Radix Colors runtime dependency or copy Vercel values. Sources:
https://www.radix-ui.com/colors/docs/palette-composition/understanding-the-scale,
https://www.radix-ui.com/colors/docs/palette-composition/composing-a-palette, and
https://vercel.com/geist/colors.

| Role | Light | Dark |
|---|---:|---:|
| canvas | `#F7F5EF` | `#151512` |
| surface | `#FCFBF7` | `#1C1C18` |
| raised | `#FFFFFF` | `#24231E` |
| hover | `#F4F0E6` | `#25241F` |
| pressed | `#EBE5D7` | `#2D2B24` |
| selected | `#FFF0C2` | `#3A2C0D` |
| border | `#D9D5CA` | `#3A3931` |
| border-control | `#8D887E` | `#747164` |
| text | `#211F1A` | `#F3F0E8` |
| muted | `#6A655B` | `#AAA69B` |
| disabled-text | `#817C72` | `#858176` |
| accent | `#E3A91D` | `#E4AD2B` |
| accent-text | `#7A4A00` | `#F0BD4C` |
| on-accent | `#241A00` | `#201800` |
| focus | `#B87500` | `#E4AD2B` |
| primary | `#211F1A` | `#F3F0E8` |
| on-primary | `#FFFFFF` | `#151512` |

Calculated light ratios include text/canvas 15.10:1, muted/canvas 5.31:1,
control/surface 3.41:1, focus/surface 3.63:1, accent-text/selected 6.59:1, and
on-accent/accent 8.14:1. Dark ratios are 16.06:1, 7.52:1, 3.49:1, 8.39:1,
7.82:1, and 8.65:1 respectively. Status pairs stay as §26 and remain labelled. WCAG requires
4.5:1 normal text and 3:1 necessary controls/states; a focus indicator must remain distinguishable
from adjacent color. Sources: https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html,
https://www.w3.org/WAI/WCAG22/Understanding/non-text-contrast.html, and
https://www.w3.org/WAI/WCAG22/Understanding/focus-appearance.html.

CSS Color 4 describes OKLCH as more perceptually uniform, so it is useful in design tooling for
checking monotonic lightness and restraining amber chroma. Audited sRGB hex remains runtime truth:
it keeps screenshots deterministic and WCAG contrast explicit. Do not synthesize state colors at
runtime with `color-mix`, arbitrary alpha, or ad-hoc OKLCH deltas. Source:
https://www.w3.org/TR/css-color-4/#ok-lab.

Final verification crosses both themes with 1440/1024/768/390 px and 100%/200% zoom; mixed Korean,
Latin, and long identifiers; valid/invalid/large/exact-copy/XSS JSON; labelled and icon-only nav;
tooltip dismissal and persistence; supported/fallback/reduced/forced material; row surface click,
text selection, modified anchor clicks, menu isolation, Tab/Enter, and screen-reader link names;
every status/disabled/focus state; reduced motion; and text/icon/boundary/selected/status contrast.

## 28. Stable filtered-empty tables and control-to-help spacing (2026-08-25)

Grafana Saga explicitly recommends retaining a table header when no data exists, using the body-row
space to explain the condition and recovery, and omitting pagination when there is nothing to page.
Its empty-state pattern distinguishes filtered `not-found` from first-use creation and permits a
clear-filter action. PatternFly similarly distinguishes compact no-result recovery from getting-
started empty states and uses 24 px table-empty padding. Carbon instead removes the table so a screen
reader does not traverse headers before learning that no content exists, so header preservation is
not a universal rule. Locron accepts Grafana's stable operational context and mitigates Carbon's
concern through the existing immediate polite result count and one concise semantic body row, not an
illustrated empty page. Sources: https://grafana.com/developers/saga/templates/lists-of-objects/,
https://grafana.com/developers/saga/patterns/empty-state/,
https://v4-archive.patternfly.org/v4/components/empty-state/design-guidelines/, and
https://preview.carbondesignsystem.com/building-blocks/core/patterns/empty-states.

Jobs and Run history retain toolbar, frame, 36 px header, labels, and widths after a successful zero
response. Render `<tbody><tr><td colspan={visibleColumnCount}>…</td></tr></tbody>` with a 24 px centered
cell, a 112 px minimum content block, and at most 480 px copy, yielding at least 160 px body space.
Narrow object layouts use 24 px by 16 px and a 96 px minimum. Pagination disappears at total zero.
Filtered copy is `No jobs match these filters` or `No runs match these filters`, followed by one
sentence and a secondary `Clear filters` action. Clearing resets all active filters and returns focus
to search before the ordinary result announcement reports the restored count. Initial empty copy is
`No jobs yet` with Create job, or `No runs yet` with an explanation and View/Create job. Loading,
error, authorization, first use, and filtered zero remain distinct.

The existing adjacent `role="status" aria-atomic="true"` politely announces the count; the table
references that status and the body row is not another live region. W3C documents `role=status` for
search result counts without focus movement. Sources:
https://www.w3.org/WAI/WCAG21/Techniques/aria/ARIA22 and
https://www.w3.org/WAI/WCAG22/working-examples/aria-role-status-searchresults/.

Material and PatternFly use 4 px for helper text beneath a single control, while Grafana uses 16 px
between form components and 40 px between sections; Carbon allows 16–32 px after a radio group.
Locron adopts explicit semantic slots: label/legend to control 8 px, final segmented/radio edge to
help 8 px, help/error internal gap 4 px, last help/error to next field 20 px, and section to section
40 px. Theme help is normal-flow muted 13/18 text, at most 56 characters wide, associated to the
group through `aria-describedby`, and begins after the whole wrapped group. Sources:
https://m1.material.io/components/text-fields.html,
https://pf4.patternfly.org/components/form/,
https://www.patternfly.org/components/helper-text/,
https://grafana.com/developers/saga/patterns/forms/, and
https://carbondesignsystem.com/components/radio-button/usage/.

Reject a full-page no-result illustration, blank `tbody`, duplicate live regions, disabled clear
actions, fixed-height empty rows, generic collapsing gaps, negative-margin help, and tooltip-only
explanation.

## 29. Settings submission, readable JSON, and nested row consistency (2026-08-25)

PatternFly's current form guidance says to default to one form per page because multiple forms and
submit buttons create unnecessary confusion, and its full-page form pattern places the submit
action at the bottom of the form. GOV.UK likewise recommends one main call to action on a page and
left-aligns it with the form. PatternFly distinguishes true inline actions—which belong next to the
single field they affect—from full-page editing, where Save and Cancel commit or discard the whole
edit. Locron's durable settings share one scheduler configuration, so five near-identical per-field
`Review …` buttons incorrectly suggest independent forms and make multi-setting changes repetitive.
The accepted pattern is one dirty settings form with a bottom action group, one aggregate review,
and one reset to the last durable snapshot. Sources:
https://www.patternfly.org/components/forms/form/design-guidelines/,
https://www.patternfly.org/components/inline-edit/design-guidelines/,
https://pf3.patternfly.org/v3/pattern-library/forms-and-controls/buttons-on-forms/, and
https://design-system.service.gov.uk/components/button/.

The appearance theme remains an exception because it is explicitly browser-local and already
applies immediately. Environment values are also gathered into the same durable review: a staged
name/value becomes one pending change in the aggregate summary, and redaction still prevents the
saved value from being redisplayed. The review dialog lists only changed keys, shows human old/new
descriptions where values are safe, states that environment values remain redacted, and applies the
validated changes in a deterministic order. A partial failure must remain visible per key and must
not falsely report the whole form saved; successful keys refresh from the server and remaining
dirty values stay editable.

JSON's exact-source requirement and readable presentation are compatible if they are separate
representations. The viewer parses only valid input for presentation, then produces deterministic
two-space indentation while retaining the source token stream's key order, duplicate keys, numeric
lexemes, escape spelling, and literal values. Copy always targets the original source. This rejects
`JSON.parse` followed by `JSON.stringify`, which would collapse duplicate keys and may normalize
number or escape spelling. Invalid input bypasses formatting and remains exact. Preview thresholds
and line counts are calculated from the presented form so expansion describes what the user sees;
the exact original byte/character count remains available for copy semantics.

The existing whole-row navigation helper already encodes safe pointer delegation. The Job detail
Recent runs table is the uncovered repeated-run surface: it currently exposes only the shortened ID
anchor. Applying the shared row contract there removes the inconsistency without changing table
semantics or adding another navigation abstraction. The regression inventory must cover Recent runs
surface click, text selection, modified/non-primary clicks, the native anchor, and any future nested
actions exactly as the Jobs and Run history tables do.

The Jobs toolbar misalignment comes from composing two independent `Field` blocks in a flex row:
Search jobs owns a helper line while State filter does not, so `align-items:flex-end` aligns the
outer boxes instead of the label and control rows. Adding an arbitrary bottom margin to the select
would fix only this copy length and would regress when help wraps. The stable solution is a small
toolbar field grid with explicit label, control, and help rows; both fields occupy the first two
rows, Search alone occupies the help row, and the result status is a separate aligned area. Mobile
switches the grid back to one column so each field keeps its ordinary label/control/help flow.

## 30. Disabled-job disappearance and cross-route QA inventory (2026-08-25)

The defect reproduces against the authenticated review fixture. The Jobs route initially reports
three enabled jobs. Disabling `api-heartbeat` succeeds durably, but returning to Jobs with `All states`
selected reports only two results and omits the disabled job. The frontend then cannot produce a
`Disabled` result because its client-side filter only sees the already-restricted response.

The API contract is intentional and already exposes the required complete view. `GET /api/v1/jobs`
lists enabled jobs by default, matching the CLI's ordinary `list`, while the existing string-flag
query `GET /api/v1/jobs?all=1` passes `true` to the store and returns current enabled and disabled
records. Jobs currently requests the default endpoint and then presents an `All states`/`Enabled`/
`Disabled` client filter, so the UI label and its data source contradict each other. The accepted fix
is to use the complete-view endpoint for operator surfaces that must name or filter current disabled
jobs; the API default remains unchanged for compatibility.

Run history independently requests the same default Jobs endpoint to map durable `job_id` values to
current names. Consequently, runs belonging to a disabled job can fall back to a shortened ID even
though the current job record and name still exist. Run history should use the same complete current-
job view for enrichment. This does not invent names for removed jobs and does not alter server-side
literal history search, pagination, or immutable run identity.

The state mutation flow already refreshes Jobs in place when no run ID is returned, but live QA found
a second defect: choosing `…` → `Disable` immediately navigates to that job's detail route. Radix
portals the menu DOM outside the row while React still bubbles the synthetic click through the row's
component ancestry. The shared row guard currently treats an interactive target outside the row DOM
as unused row space, so it activates the row link. Interactive descendants and portalled interactive
targets must both suppress row navigation. Once both fixes land, `All states` retains the changed row,
`Disabled` gains it, and `Enabled` excludes it. QA must also prove the inverse transition,
preservation of search/filter state, direct detail reachability, and action-menu isolation. The review
fixture is restored after mutation checks so the user does not inherit test-only state.

The broader functional pass should prioritize connected workflows where isolated tests can miss
contract mismatches: authentication and asset freshness; Jobs state/search/navigation/action menus;
Run history debounce, partial search, names, and pagination; job/run detail JSON and row navigation;
Settings review/discard and theme locality; Diagnostics health loading; desktop/mobile overflow; and browser
error/warning logs. Destructive Remove and long-running/cancellation scenarios remain covered by the
automated server/CLI contract suites rather than mutating the user's live review fixture.

## 31. Linux-only dashboard service compilation regression (2026-08-25)

The v0.8.0 preparation commit `7fcb716` passed the complete local macOS battery, but that evidence
compiled only the active macOS `cfg` branches. PR #9 CI run
[32801485007](https://github.com/WhiteKiwi/locron/actions/runs/32801485007), on the same head, exposed
two Linux-only source-boundary defects before any test could execute.

The Rust 1.94.0 Linux lint job
[97663055695](https://github.com/WhiteKiwi/locron/actions/runs/32801485007/job/97663055695)
failed `cargo clippy --workspace --all-targets -- -D warnings` with five `E0425` errors in
`crates/locron-cli/src/service.rs`: the systemd `is_loaded`, `enable`, `start`, `stop`, and `unload`
methods bind their service context as `_ctx` but read `ctx.target.service_name()` at lines 1509,
1514, 1518, 1523, and 1535. The same lint job additionally rejected two Linux test-target
`dead_code` warnings: `DashboardServiceCleanup` at `service_backends.rs:122` and
`default_dashboard_token_path` at line 271. The Rust 1.94.0 Linux test job
[97663055756](https://github.com/WhiteKiwi/locron/actions/runs/32801485007/job/97663055756)
failed `cargo test --workspace --all-targets` on the same five `E0425` errors. Stable and aarch64
Linux legs failed the same source revision; both Rust-version rows are therefore affected by the
platform branch, not a toolchain-specific change.

The adjacent `ServicePort` implementations show the intended convention: a context argument is
named `ctx` when its target or paths are read and `_ctx` only when the implementation genuinely
does not use it. The systemd `session_available` and `reload` methods legitimately keep `_ctx`;
the five failing methods must bind `ctx`. A complete `_ctx` inventory found no other binding in
`service.rs` that subsequently reads `ctx`.

The two dead-code items support only the macOS dashboard launchd test. The cleanup type's sole
construction and the token-path helper's sole call are inside
`macos_launchd_backend_registers_refreshes_and_unregisters_the_dashboard`. Their definitions should
carry the same `#[cfg(target_os = "macos")]` boundary as that test. A blanket `allow(dead_code)`
would hide future platform drift and is rejected. `ServiceCleanup`, `uid`, and the common command
helpers remain cross-platform because the Linux direct-session test uses them.

The local toolchain has the `x86_64-unknown-linux-gnu` Rust target, but a target-specific Cargo check
stops in `libsqlite3-sys` and `aws-lc-sys` build scripts because the host has no
`x86_64-linux-gnu-gcc`; it never reaches Locron Rust source. Installing a cross C toolchain is not a
proportionate repository fix. The systemd module itself uses portable standard-library APIs and can
be compiled safely in ordinary unit-test builds without invoking `systemctl`. Extending its cfg from
Linux-only to Linux-or-test, plus a test that coerces `SystemdPort` to a `dyn ServicePort` trait
object, gives macOS Clippy and tests a persistent type-check seam for the complete implementation.
The authoritative native Linux confirmation remains the parent session's CI rerun after the fix is
pushed.

## 32. Platform-neutral service template seam (2026-08-25)

The first portability fix reached native Linux compilation and exposed the next platform-coupling
layer on PR #9 CI run
[32802465277](https://github.com/WhiteKiwi/locron/actions/runs/32802465277), head `95fbae5`.
Rust 1.94.0 lint job
[97665903962](https://github.com/WhiteKiwi/locron/actions/runs/32802465277/job/97665903962)
compiled the previous five systemd identifier fixes, then failed only because `DAEMON_LABEL` and
`DASHBOARD_LABEL` were unused at `service.rs:40` and `:44` in the Linux test build. Rust 1.94.0 test
job [97665904093](https://github.com/WhiteKiwi/locron/actions/runs/32802465277/job/97665904093)
compiled and ran 82 CLI unit tests; 80 passed and the two plist template tests failed. The daemon
assertion at line 1800 could not find `<string>dev.locron.daemon</string>`, and the dashboard
assertion at line 1821 could not find `<string>dev.locron.dashboard</string>`.

`Target::service_name()` intentionally selects the current service manager's runtime identity:
launchd labels on macOS and systemd unit file names on Linux. Linux tests compile `render_plist`,
which currently calls that current-platform selector, so it renders `locron.service` or
`locron-dashboard.service` into a launchd document. The constants are consequently unused, and the
template is semantically wrong. Weakening the template assertions or suppressing dead code would
hide that cross-platform error.

The platform-neutral boundary is an explicit launchd-label accessor on `Target`, available on
macOS and test builds. `render_plist` must use it regardless of the host compiling the template;
`service_name()` continues to choose the active manager identity for status envelopes, manager
commands, launchd runtime targets, and systemd unit paths. On macOS its branch may delegate to the
same launchd accessor so labels have one source of truth.

An adjacent renderer audit found no analogous current-platform leakage. `render_unit` embeds only
the target-specific description, executable, and arguments; the unit file path is chosen later by
the Linux systemd port through `service_name()`, where current-platform selection is correct.
`render_plist` owns the only manager identity embedded inside template contents. Plist file names
and log files already have explicit launchd/test-scoped accessors.

## 33. Dual-stack dashboard port-test ownership race (2026-08-25)

The platform-neutral template fix made warnings-denied lint green on both Linux toolchains in PR #9
CI run [32803014653](https://github.com/WhiteKiwi/locron/actions/runs/32803014653), head
`341bf2a`. The remaining failures are nondeterministic dashboard integration-test port ownership,
not server template or runtime compilation failures.

Linux aarch64 Rust 1.94.0 job
[97667526486](https://github.com/WhiteKiwi/locron/actions/runs/32803014653/job/97667526486)
failed `foreground_serve_falls_back_when_the_default_port_is_occupied`: the test's parsed startup
line still reported port 10824, violating its line-480 assertion. macOS aarch64 Rust 1.94.0 job
[97667526530](https://github.com/WhiteKiwi/locron/actions/runs/32803014653/job/97667526530)
failed `service_mode_keeps_the_default_port_fixed_when_occupied` because the child kept serving and
did not exit within the ten-second deadline at line 189. Linux x86_64 1.94.0/stable, Linux aarch64
stable, and macOS aarch64 stable passed the same tests; both Linux lint legs passed. The mixed
matrix result is consistent with an intra-binary scheduling race.

The server's dual-stack behavior is intentional. It attempts every configured loopback address on
one candidate port, warns and continues when only one family binds, and treats the port as
unavailable only when neither family binds. Existing server coverage proves that an occupied IPv4
address can validly serve on IPv6 at the same fixed port. Therefore a strict/fallback occupancy test
must keep every bindable loopback family unavailable for the child lifetime; occupying IPv4 alone
would not satisfy the product contract.

The dashboard integration binary has exactly three fixed-default-port tests:
`service_mode_keeps_the_default_port_fixed_when_occupied`,
`redirected_bare_serve_still_uses_foreground_fallback`, and
`foreground_serve_falls_back_when_the_default_port_is_occupied`. Rust runs integration tests in
parallel. Their shared `hold_fixed(10824)` helper treats zero newly acquired listeners as success,
assuming another owner will continue occupying both families. If test A owns both listeners, test B
sees zero and advances without ownership; when A finishes and releases before B's child binds, B can
bind 10824. Foreground then reports the supposedly occupied default, while fixed service mode starts
and outlives the completion deadline. The two CI failures are the opposite policy manifestations of
the same lifetime gap.

A poison-tolerant process-static mutex around all three fixed-default-port tests makes ownership
sequential within the integration binary. Each test either owns the bindable listeners for its
whole child lifetime or observes a genuinely external holder; another suite test can no longer be
the unowned transient holder. `hold_port()` for explicit random-port strictness remains parallel and
unchanged. No sleep, longer timeout, single-family assertion, or weakened outcome is needed.

## 34. crates.io source installation and trusted publication (2026-08-25)

This research supports the 2026-08-25 installation-channel amendment in `docs/SPEC.md`. It uses
current first-party Cargo, crates.io, Rust Project, and GitHub documentation plus the official
repositories of established Rust CLIs. It records the release design; it does not itself change
manifests, runtime behavior, or automation.

### Ecosystem and registry evidence

**The install command is named after the package, not the binary target.** Cargo installs a package
selected from crates.io and copies its executable targets into the install root's `bin` directory.
The default source is crates.io, and only packages with executable targets can be installed
([Cargo `install`](https://doc.rust-lang.org/cargo/commands/cargo-install.html)). Therefore a package
named `locron-cli` with a `[[bin]]` named `locron` supports `cargo install locron-cli`, not the
required `cargo install locron`. The user-facing package must be renamed to `locron`; retaining the
explicit `[[bin]] name = "locron"` is harmless and keeps the executable contract obvious.

`cargo install --locked` uses the `Cargo.lock` packaged with the crate instead of resolving newer
dependencies. Cargo packages include a lockfile by default specifically so source-installed
binaries can use it ([Cargo `package`](https://doc.rust-lang.org/cargo/commands/cargo-package.html),
[Cargo `install`](https://doc.rust-lang.org/cargo/commands/cargo-install.html#dealing-with-the-lockfile)).
This is the conventional secondary source-build channel: uv documents `cargo install --locked uv`,
calls out the compatible Rust toolchain requirement, and reserves self-update for its standalone
installer; bat likewise documents prebuilt/package-manager routes before
`cargo install --locked bat`
([uv installation](https://github.com/astral-sh/uv/blob/main/docs/getting-started/installation.md),
[bat installation](https://github.com/sharkdp/bat#installation)). Locron should use the same
presentation: installer and Homebrew first, then a clearly labeled source-build option.

**Published path dependencies need registry versions.** crates.io refuses a non-development
dependency specified only by `path`. Cargo's supported workspace form is
`{ path = "../locron-core", version = "..." }`: local builds use the path and the packaged manifest
uses the registry version
([Cargo dependency locations](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#multiple-locations)).
Locron's lockstep release rule is stronger than an ordinary compatible range, so every internal
edge should use the exact workspace release, for example `version = "=0.8.0"`, rather than the
implicit caret range from `version = "0.8.0"`. Put those path-plus-exact-version declarations in
`[workspace.dependencies]` and inherit them in members so there is one dependency declaration per
internal package. The root release-version update must update these exact requirements in the same
reviewed change.

The repository's dependency order is:

1. `locron-core`;
2. `locron-store` and `locron-engine` after core;
3. `locron-server` after core and store;
4. the renamed `locron` package after core, store, engine, and server.

All five must be public because the installable package has normal (not development-only)
dependencies on the other four. Publishing only the binary package is not possible under the
registry's dependency rules.

**Use Cargo's native workspace publisher on the supported toolchain.** Cargo 1.90 stabilized
`cargo publish --workspace` for interdependent packages. It verifies the full selected set as if
published, calculates dependency order, uploads ready dependency batches, and waits for uploaded
packages to appear in the registry index before advancing. Cargo's own announcement explicitly
states that it follows workspace dependencies in the right order and that dry-run verification
covers the full set
([Rust 1.90 announcement](https://blog.rust-lang.org/2025/09/18/Rust-1.90.0/#cargo-adds-native-support-for-workspace-publishing)).
The command documentation confirms that each upload is followed by polling for index visibility;
a timeout means the upload may already have succeeded and must be checked, not blindly retried
([Cargo `publish`](https://doc.rust-lang.org/cargo/commands/cargo-publish.html)). Locron's Rust 1.94
MSRV therefore supports the stable command. A hand-written publish loop and arbitrary sleeps are no
longer justified.

Workspace publication is deliberately **not atomic**. Cargo 1.90 warns that a network or server
failure can leave a partially published workspace. The release job must preserve the Cargo output,
report which package/version is visible, and stop downstream GitHub Release and Homebrew publication.
It must never move or replace the tag, use `--allow-dirty`, or use `--no-verify`. An operator can
inspect crates.io and resume only the missing packages with explicit repeated `-p` selections;
Cargo will again order that selected subset. Already-visible versions must be treated as immutable,
not overwritten.

**Published versions are append-only release records.** The Cargo publishing guide says a published
version cannot be overwritten and recommends a curated changelog plus a Git tag for every version.
Yanking removes a version from new dependency resolution but does not delete its data, does not
break an existing lockfile, and can be undone
([Cargo publishing guide](https://doc.rust-lang.org/cargo/reference/publishing.html),
[Cargo `yank`](https://doc.rust-lang.org/cargo/commands/cargo-yank.html)). Consequently a bad Locron
publication is remediated by publishing a fixed new version and, only when warranted, yanking the
bad one. A yank is not a rollback mechanism and cannot repair leaked credentials or replace source.

### Trusted publishing and least privilege

crates.io Trusted Publishing exchanges a GitHub Actions OIDC identity for a short-lived crates.io
token, eliminating a long-lived registry secret. The official crates.io announcement says the first
release of each crate must still be published manually; after that, each crate owner configures the
trusted repository/workflow and future releases use `rust-lang/crates-io-auth-action@v1`
([crates.io development update](https://blog.rust-lang.org/2025/07/11/crates-io-development-update-2025-07/#trusted-publishing),
[official auth action](https://github.com/rust-lang/crates-io-auth-action)). A GitHub trusted
publisher binds the owner or organization, repository, workflow filename, and optionally an
environment. Locron should configure all five packages for `WhiteKiwi/locron`, `release.yml`, and a
dedicated `crates-io` environment. The environment should allow only release tags and should use
required-reviewer protection if that is available for the repository.

The crates publication job alone needs:

```yaml
environment: crates-io
permissions:
  contents: read
  id-token: write
```

GitHub documents that `id-token: write` only permits requesting an OIDC token; it does not grant
repository write access. `contents: read` is sufficient for checkout
([GitHub OIDC reference](https://docs.github.com/en/actions/reference/security/oidc#workflow-permissions-for-the-requesting-the-oidc-token)).
Do not add `id-token: write` to the workflow-wide permissions. Keep `contents: write` only on the
separate GitHub Release job. Pass `${{ steps.auth.outputs.token }}` to the publish command as
`CARGO_REGISTRY_TOKEN`; the official action revokes its temporary token in its post step. Once one
subsequent release has proven all five trusted-publisher bindings, enable crates.io's
Trusted-Publishing-only mode and remove/revoke the bootstrap API token. crates.io blocks
`pull_request_target` and `workflow_run` trusted-publishing triggers, reinforcing the existing
release-tag trigger choice
([2026 crates.io update](https://blog.rust-lang.org/2026/01/21/crates-io-development-update/#trusted-publishing-enhancements)).

The one-time bootstrap is necessarily distinct from steady-state CI:

1. Prepare a clean reviewed release commit with all five package names, metadata, exact internal
   versions, lockfile, and tag version aligned.
2. Run the complete package/dry-run checks below with Rust 1.94.0.
3. From that exact commit, use a newly created narrowly scoped crates.io API token to run
   `cargo publish --workspace --locked`. Confirm all five versions and revoke the token.
4. Configure the same `release.yml`/`crates-io` trusted publisher on every package, then push the
   immutable tag. The tag workflow must recognize that the bootstrap versions already exist and
   verify them rather than attempting to overwrite them, or the bootstrap release should be
   explicitly exempted once; all later tags publish through OIDC.

The first-release exception must be documented in the operator release checklist; a permanent
`CARGO_REGISTRY_TOKEN` repository secret is rejected.

### Self-update ownership and service registration

Cargo documents installation-root precedence (`--root`, `CARGO_INSTALL_ROOT`, Cargo config,
`CARGO_HOME`, then `$HOME/.cargo`) and says it normally tracks installed packages in metadata in
that root. It also provides `--no-track`, which deliberately disables that metadata
([Cargo `install`](https://doc.rust-lang.org/cargo/commands/cargo-install.html)). Cargo does not
document a stable runtime API or compiled-in variable that tells a binary which installer copied
it. Parsing Cargo's private `.crates2.json`, assuming `~/.cargo/bin`, or looking only at the current
runtime value of `CARGO_HOME` cannot robustly cover custom roots, moved binaries, and `--no-track`.

The robust design is therefore positive standalone ownership, matching uv's established pattern:
uv loads an installer receipt and allows self-update only when that receipt belongs to the running
executable; absence is treated as another installation method
([uv self-update implementation](https://github.com/astral-sh/uv/blob/main/crates/uv/src/commands/self_update.rs)).
`install.sh` should atomically write a versioned Locron receipt under its install prefix, bind it to
the canonical executable path, and identify `standalone` as the owner. `locron self-update` should
proceed only when this positive receipt is valid. It may use a Cargo-root/metadata check only to
select a more specific message, never to authorize replacement. The safe fallback for every absent
or mismatched receipt is refusal, including custom-root and `--no-track` Cargo installs.

The Cargo guidance is:

```text
cargo install --locked locron
cargo uninstall locron
```

`--force` is unnecessary for an ordinary upgrade: Cargo reinstalls when the installed package
version or source changes; reserve it for deliberately rebuilding the same version. A generic
unmanaged-install refusal should still mention the Cargo update command so a Cargo user receives
the exact remedy even when Locron cannot prove the specific manager.

Self-update ownership and service ownership must remain separate checks. Absence of the standalone
receipt refuses only binary replacement. Cargo does not install or supervise the Locron daemon or
dashboard, so `locron service install` and `locron dashboard enable` remain allowed. The existing
package-manager service marker continues to block those operations only for integrations such as
Homebrew that actually own their service lifecycle. Reusing one broad “managed install” predicate
for both concerns would incorrectly disable Cargo users' service registration.

### Package metadata, documentation, and verification recommendation

Set shared crates.io metadata deliberately: `license`, `repository`, `readme`, `rust-version`, and
concise descriptions are required or strongly recommended; add at most five valid keywords and
categories such as `command-line-utilities`. Cargo documents that crates.io requires a description
and license, renders the declared README on the crate page, limits keywords/categories, and links
to docs.rs automatically when no separate documentation URL is supplied
([Cargo manifest reference](https://doc.rust-lang.org/cargo/reference/manifest.html#the-package-section)).
Do not add a redundant homepage that merely repeats the repository URL. Restrict every package to
`publish = ["crates-io"]` and keep the common root README and dual-license files in every generated
package. Internal crates should describe their Locron role and should not claim a separately stable
public-library API.

README and installation docs should show the channels in this order:

1. standalone installer (general users, prebuilt, service registration available);
2. Homebrew (package-manager lifecycle);
3. `cargo install --locked locron` (Rust 1.94+ source build, no automatic service registration);
4. Cargo update/removal commands and an explicit warning that `locron self-update` refuses it;
5. optional `locron service install` / `locron dashboard enable` after Cargo installation.

Before any upload, require a clean tree and run at least:

```text
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
cargo package --workspace --locked
cargo publish --workspace --dry-run --locked
```

Also run `cargo package -p <package> --list` for all five packages and inspect the generated
archives for the intended README/licenses, source, manifest normalization, lockfile, and absence of
state files, tokens, build outputs, and unrelated docs. Cargo's package command rewrites the
manifest, includes `Cargo.lock`, records best-effort VCS information, extracts the archive, and
builds it from scratch; `--no-verify` defeats the most important source-package check
([Cargo `package`](https://doc.rust-lang.org/cargo/commands/cargo-package.html)).

The release workflow should add a Rust-1.94.0 `publish-crates` job after the complete platform build
matrix and before the existing GitHub Release/Homebrew job. It first asserts that the tag, workspace
version, `locron --version`, lockfile package versions, and changelog agree; performs the workspace
dry run without credentials; then obtains the OIDC token and runs
`cargo publish --workspace --locked`. Downstream publication depends on its success. After index
visibility, verify the exact released source channel in a temporary root with
`cargo install --locked --root <temp> locron --version <exact-version>`, run the installed
`locron --version`, exercise Cargo self-update refusal plus daemon/dashboard registration behavior,
and remove the temporary root. This proves the registry artifact users actually receive, not only
the workspace checkout.

### Alternatives rejected

- Keep the package name `locron-cli`: rejected because it exposes the wrong install command.
- Publish only `locron`: rejected because its four normal path dependencies must resolve from the
  same registry.
- Maintain a custom dependency-order shell loop with sleeps: rejected because stable Cargo 1.90+
  now owns ordering, whole-workspace verification, and index polling.
- Treat workspace publication as transactional: rejected because Cargo explicitly documents
  partial publication on transport or server failure.
- Store a permanent crates.io API token in GitHub: rejected after the one-time bootstrap because
  official short-lived OIDC trusted publishing is available.
- Detect Cargo exclusively from `~/.cargo/bin`, `CARGO_HOME`, or `.crates2.json`: rejected because
  Cargo supports configured/custom roots and `--no-track`, and the private metadata format is not a
  documented runtime contract.
- Block service/dashboard registration whenever self-update is blocked: rejected because Cargo
  owns only the binary installation, not Locron's per-user operating-system services.

## 35. Dashboard default-port test lifetime race (2026-08-25)

CI run [32834650532](https://github.com/WhiteKiwi/locron/actions/runs/32834650532), job
[97760707674](https://github.com/WhiteKiwi/locron/actions/runs/32834650532/job/97760707674),
failed `redirected_bare_serve_still_uses_foreground_fallback`: the child reported the supposedly
occupied default port 10824. The other seven platform/toolchain test jobs passed the same suite,
and the immediately preceding run passed, which identifies shared-host timing rather than a
platform or product regression.

The earlier process-static mutex serialized only the three tests that deliberately occupy 10824.
It cannot own a host port across other test processes or external services. More importantly,
`hold_fixed` treats zero newly acquired listeners as success on the assumption that the current
owner will remain alive through the child bind. If that unowned listener closes between those
events, the child legitimately binds 10824. The mutex narrows this gap but cannot close it because
the test still borrows another process's listener lifetime.

The deterministic boundary is already available in the server configuration: a test may bind an
OS-assigned port on one loopback family, configure that same family, and retain the listener while
combining the owned preferred port with `PortPolicy::Foreground` or `PortPolicy::Fixed`. Server
tests can therefore prove real conflict, fallback, and strict failure without using the globally
meaningful product default; the existing partial-family contract continues to cover dual-stack
behavior independently. CLI unit tests can separately prove that no
explicit port selects foreground policy while explicit-port and service-mode forms select fixed
policy. Existing explicit random-port CLI integration tests continue to prove startup/output and
strict error mapping. This composition covers the same product contract while every network
precondition is owned for the full assertion lifetime.

Retried binds, longer timeouts, CI-level test serialization, and accepting port 10824 are rejected:
they retain the borrowed-resource race, hide it, or weaken the foreground fallback contract.

## 36. CI toolchain selection and cache pressure (2026-08-26)

Hosted run 32836599403 completed in about four minutes of wall time but scheduled fourteen jobs and
consumed about 26 runner-minutes. The test matrix crossed Rust 1.94.0 and `stable` with four native
platforms, while the lint matrix crossed both toolchains with Linux x86_64 and macOS arm64. The
platform coverage is valuable for process, filesystem, and service-manager code, but applying the
MSRV dimension to every native platform repeats the same compatibility signal.

The `stable` jobs did not provide their intended signal. `dtolnay/rust-toolchain` installed Rust
1.98.0, but commands executed inside the checkout reported Rust 1.94.0 because the repository's
`rust-toolchain.toml` directory override won toolchain selection. Job-level `RUSTUP_TOOLCHAIN` is
required so each matrix entry controls both Cargo execution and the cache environment fingerprint.

The Actions cache API reported 33 active caches totaling 10,539,851,766 bytes. Individual
`Swatinem/rust-cache` entries were roughly 179–451 MiB and were separated by job ID, platform,
architecture, and Rust environment. Restore time ranged from about 5 to 40 seconds while native test
commands took roughly two to three minutes, so dependency/target caching remains worthwhile. The
better reduction is fewer meaningful jobs and main-only cache saves, not disabling target caches.
GitHub documents a default 10 GB repository cache limit and least-recently-used eviction beyond the
configured limit, so the existing shape risks cache churn.

Twelve of those entries are release-tag caches: four entries each for `v0.9.0`, `v0.9.1`, and
`v0.9.2`, totaling 2,636,106,740 bytes. An immutable tag normally builds once, so writing a new cache
namespace for every release spends repository capacity without warming the next tag. The release
jobs should retain cache restoration but set `save-if: false`; GitHub's default-branch cache remains
the reusable producer for ordinary and release builds, while an exceptional rerun simply recompiles
anything absent from that reusable cache.

Retain stable tests on all four supported host combinations, retain stable lint on Linux and macOS
to cover OS-gated code, and run the Rust 1.94 MSRV test once on Linux x86_64. Installer and source
package jobs remain independent. This reduces fourteen jobs to nine while preserving every native
platform, both operating-system lint branches, MSRV execution, packaging, and installer evidence.
Pull requests restore compatible caches but only the default branch saves new entries.
