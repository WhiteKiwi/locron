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

### Independent Jobs facts test readiness (2026-10-03)

The PR43 frontend gate failed the complete-collection fixture while synchronously reading its
four `succeeded` facts. Jobs publishes collection data first; a separate effect then joins preview
and latest-run requests before publishing facts. The two-result status therefore proves only
collection readiness. Await the existing four-facts assertion with waitFor, preserving the
disabled-preview, filtering and desktop/mobile assertions; production behavior does not change.
Evidence: [failed frontend gate](https://github.com/WhiteKiwi/locron/actions/runs/37036692807/job/110936539289).

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

Hosted run 33026056426 completed after the GitHub Actions incident and proved that runner startup,
tests, packaging, and installer validation were healthy: seven jobs passed and only the two lint
jobs failed. Both lint jobs used Rust 1.98.0 and rejected pre-existing
`Result::map(...).unwrap_or(false)` expressions under `-D warnings` with
`clippy::map_unwrap_or`; Rust 1.94 Clippy had accepted the same source. The latest cache-only commit
did not introduce those expressions.

Using a floating `stable` channel for a blocking lint gate makes the accepted warning set change
without a repository change, while pinning all compatibility tests would lose forward-compiler
coverage. Separate those concerns: keep the test matrix on floating stable plus the explicit 1.94
MSRV leg, but pin blocking formatting and Clippy to Rust 1.98.0. Make 1.98.0 the checked-in
development toolchain so the documented local command and CI execute the same linter. Keep
`workspace.package.rust-version = "1.94"` as the package/MSRV contract. A later lint-toolchain bump
must therefore be an intentional repository change whose local lint fixes land with the bump.

The correction was verified by hosted push run 33027459455: all nine jobs succeeded in five minutes
of wall time and 1,312 runner-seconds. Both lint hosts reported exact Rust 1.98.0, all four native
compatibility hosts ran floating stable, and the Linux MSRV and source-package jobs ran Rust 1.94.0.
The post-job cache steps also completed successfully. After the run, the cache inventory contained
28 entries totaling 10,030,878,419 bytes, down from 33 entries and 10,539,851,766 bytes before the
optimization as older variants were evicted.

## 37. Active dashboard run detail live-follow gap (2026-08-28)

The reported run `01a0465e-2799-7ba0-8491-67cf453a5aea` reached `succeeded` after about
149.9 seconds with one attempt and two stdout frames. Its authenticated run-detail request returns
HTTP 200 in about 5 ms, and the dashboard log contains no corresponding request error. These facts
rule out a slow or failed durable-detail read and point to the client after its initial snapshot.

The server stream already provides the required source of truth. It emits named `run`, `attempt`,
`output`, and final `termination` events, polls durable state and framed output every 200 ms, replays
current state and output from the beginning on each connection, and closes after exactly one
terminal event. Contract tests cover ordered state transitions, reconnect replay, arbitrary output
bytes in `data_b64`, and one final termination event.

The run-detail client does not consume that contract correctly:

- live following initializes disabled, so an active detail page opens no stream until the user
  presses `Follow output`;
- `run` and `attempt` events are parsed but ignored, leaving the snapshot state and attempt table
  stale throughout execution;
- `output` frames carry `data_b64`, but the client reads a nonexistent `text` property, so a stream
  that is manually opened still cannot render the captured bytes correctly;
- its replay-deduplication set belongs to one `EventSource` effect lifetime, so pausing/resuming or
  a newly created source forgets prior frames and appends replay duplicates; and
- terminal reconciliation can start without a per-view generation guard, allowing a late request
  to update an unmounted or different detail view.

The fix belongs in the frontend. After the initial detail response, active states should enable
following automatically. One run-detail lifetime should own replay keys and a request generation;
state events should update the visible snapshot immediately, output should decode `data_b64`, and
termination should close following then run one guarded full reconciliation. A stream error should
only change connection feedback because native `EventSource` reconnects automatically and the last
durable snapshot remains useful. Terminal initial snapshots should never construct an
`EventSource`.

A client-owned key of `(attempt_number, seq)` matches the server's replay identity. Keep that set
across automatic reconnects and pause/resume, but reset it when the route run identity changes.
Manual durable output loading should rebuild the visible output and seed the same identity set from
the returned frames so a subsequent replay does not duplicate those frames.

## 38. Rustls advisory in the dependency audit (2026-09-28)

The latest main-branch [Audit run 36437796931](https://github.com/WhiteKiwi/locron/actions/runs/36437796931)
failed only its `cargo-deny / advisories` job; `bans licenses sources` and the separate main-branch
CI run passed. A local `cargo deny --all-features check advisories` on that revision reproduced
`RUSTSEC-2026-0285` against locked `rustls 0.23.43`. The same finding appears in the preceding
scheduled Audit run 36317841500, so it is not a runner-only failure.

The [RustSec advisory](https://rustsec.org/advisories/RUSTSEC-2026-0285.html), checked on
2026-09-28, identifies affected `rustls` versions and names `0.23.45` as the first patched
version. The workspace uses `rustls` transitively through `reqwest 0.13.5`, including the CLI,
engine, and server; `cargo tree -i rustls --locked` confirms that path. The workspace manifest
accepts `reqwest 0.13`, so a targeted lockfile update should resolve the advisory without adding
a direct dependency, changing the supported platforms, or weakening `deny.toml`.

## 39. v0.9.4 security release readiness (2026-09-29)

At planning time, the merged fix is main revision `403e0d6`; its dependency Audit run
[36442239774](https://github.com/WhiteKiwi/locron/actions/runs/36442239774) passed, and its main
CI run was still in progress. The latest published release and immutable tag are v0.9.3, whose
revision predates the fixed lockfile. The workspace version is still 0.9.3. The exact v0.9.4
crates.io inventory script reported `none`, and no remote `v0.9.4` tag exists.

The repository's [release workflow](../.github/workflows/release.yml) runs on a new `v*.*.*` tag,
not on a main-branch merge. Its pipeline builds four platform archives, publishes all five
lockstep crates with trusted publishing, verifies a registry installation, creates the GitHub
Release with checksums and installer, and updates the Homebrew tap. The
[release procedure](RELEASE.md#standard-release-procedure) requires a curated changelog and a
version-consistent reviewed commit before the immutable tag. A new v0.9.4 patch release is thus
needed to distribute the already merged security fix; the existing v0.9.3 tag must remain intact.

## 40. First feedback: updater, registration warnings, and latest-run visibility (2026-09-30)

Research checked the current v0.9.4 source, the v0.9.2 updater source, and the public
[v0.9.4 release](https://github.com/WhiteKiwi/locron/releases/tag/v0.9.4). No installed binary,
live job, daemon, or dashboard was changed, and no self-update operation was executed.

### L-1: published filename format does not match the updater parser

The [release checksum asset](https://github.com/WhiteKiwi/locron/releases/download/v0.9.4/SHA256SUMS.txt)
downloaded directly from GitHub contains
`91e3927e3b140714277c7ea9a4925b59bef40954094f7ce48f19c6e47857091f  ./locron-v0.9.4-aarch64-apple-darwin.tar.gz`.
`gh release view v0.9.4 --repo WhiteKiwi/locron --json tagName,url,assets` confirms the matching
archive is uploaded, has size 5,644,611 bytes, and has the same SHA-256 digest. The workflow's
`sha256sum ./*.* > SHA256SUMS.txt` deliberately emits the leading `./`.

Both v0.9.2 and current `checksum_for` compare the second whitespace-delimited field directly
with the bare archive filename. Thus the published line cannot match, even though its hash and
archive exist. The default updater downloads this exact asset from
`https://github.com/WhiteKiwi/locron/releases/download/<tag>/SHA256SUMS.txt`, after selecting the
latest tag through the GitHub releases API. No alternate checksum manifest is used. Explicit
`LOCRON_UPDATE_API_BASE` and `LOCRON_UPDATE_ASSET_BASE` overrides can change those origins;
their presence in the reporter's process was not established.

The standalone installer uses a different grep-based lookup and accepts this published form,
including the `./` in its relative downloaded filename. Therefore installation or replacement
through the installer is consistent with reaching v0.9.2; the report does not establish a prior
successful built-in self-update. Existing integration fixtures generate bare names, explaining
why the published-format mismatch was not covered.

Normalize only checksum filename syntax: accept bare names and one leading `./`, and support
the standard optional `*` binary-mode marker before that filename. Preserve exact remaining
filename comparison rather than accepting arbitrary paths or suffix matches. Preserve 64-digit
hex validation, archive hashing before replacement, ownership checks, and atomic replacement.
Use the published `./` format in the successful end-to-end fixture; cover bare/binary-marker
forms and malformed, missing, wrong-name, and mismatch failures without mutating real services.

The parser fix alone cannot restore the update path for already installed affected clients:
their old parser must verify the next release before they can download the fixed binary. Future
releases must also emit bare filenames, for example by hashing explicit `*.tar.gz *.deb *.rpm`
operands inside the artifact directory rather than `./*.*`. This also avoids accidentally hashing
an existing checksum file during a repeated generation. Do not rewrite published v0.9.2–v0.9.4
assets or tags. The installer accepts bare names in both pinned and latest-release modes, while
Homebrew computes archive hashes independently and crates.io does not consume SHA256SUMS.txt.
Add a local release-checksum fixture that verifies expected bare names and `sha256sum -c` round
tripping. Keep new-parser compatibility with historical `./` checksum assets as a separate test.

### L-2: reuse CLI execution resolution as an advisory probe

CLI `doctor` already uses `engine_target` and `resolve_attempt_executable`. These apply the
execution PATH and global environment, job PATH, environment-file values, then inline job
values; relative PATH entries and relative executable paths resolve against job cwd. Calling
only the low-level engine resolver with the CLI's inherited PATH would lose this behavior.
The underlying resolver checks for a file, not executable permission, so warnings should claim
resolution failure rather than exhaustive execution readiness.

CLI `add` and `update` currently collect only environment-policy warnings. Their existing
warning branches cover human stderr and machine envelopes in both live and dry-run modes.
Read existing settings through `config_dry_run_settings(paths, true)`, which uses defaults when
no state database exists, and probe only `Target::Process` via `engine_target` with a synthetic
diagnostic attempt. Do not execute the target or create durable state. An environment-file or
settings failure must become a safe advisory warning, not reject an otherwise valid mutation.
Do not echo effective environment values or raw resolver errors that could include them.

Offer `did you mean --http <METHOD> <URL>?` only when executable `http` is missing, its first
argument is an uppercase method supported by Locron, and its second argument parses as an
HTTP(S) URL. Render the validated method and literal `<URL>` placeholder; do not repeat the
actual URL, argv, or credentials. A successfully resolved `http` remains a valid process target.
Keep the scope at CLI add/update: API and MCP have separate mutation handlers, and the API doctor
has a different simpler resolver. Broad cross-surface refactoring is unnecessary for this fix.

### L-3: project the latest retained run without loading history

`list_jobs` currently returns definitions without a run observation. `latest_and_anomalous_runs`
defines the canonical ordering as `requested_at_us DESC,id DESC`, but invoking it for every list
row would introduce repeated reads and load unused anomaly records and execution snapshots.
The existing `runs_history(job_id, requested_at_us DESC, id)` index supports a focused latest-run
lookup without a schema migration.

Add a dedicated list projection that reads jobs and optional latest run id/state in one SQLite
snapshot, preferably one query with a correlated latest-run selection. Preserve existing
`list_jobs` callers and its filtering/name order. Add `latest_run: {id,state}` or `null` to CLI
list machine rows and a human `LAST RUN` column, using `none` for no retained run. Apply the
same retained-run ordering across revisions and triggers, including active and skipped states;
do not infer whether a job ever ran after retention removed its history. Keep the state column
visible when fitting terminal width by extending the fixed-column budget; only TARGET shrinks
under the existing table-fitting contract.

Consecutive-failure counts remain deferred because retry attempts, skipped/cancelled states,
and retention require an agreed definition. Target-label renaming is also deferred. Regression
coverage should distinguish enabled state from run state, exercise tied request times, no
retained runs and filtering, preserve redaction, and verify dry runs leave state unchanged.

## 41. v0.9.5 feedback correction release readiness (2026-09-30)

Read-only inventory confirms the latest public [GitHub Release is v0.9.4](https://github.com/WhiteKiwi/locron/releases/tag/v0.9.4).
`git ls-remote --tags origin refs/tags/v0.9.5` returns no tag, and
`sh scripts/crates-version-inventory.sh 0.9.5` reports `none` for the five exact package versions.
The workspace version and four exact internal requirements still identify 0.9.4 at this check;
the correction tree is not yet a version-consistent release candidate.

The selected patch corrects existing self-update verification and diagnostics/observability of
existing process-registration and run-state behavior. It adds no command or configuration option,
changes no scheduling/retry policy, introduces no storage migration, and preserves platforms and
installation channels. Machine list observations are additive and checks remain advisory. Curated
notes must not promise failure streaks, exhaustive execution readiness, renamed targets, or
installed service changes. This follows the bug-correction/remediation patch path in
[RELEASE.md](RELEASE.md#standard-release-procedure); new commands, policies, or additive migrations
would instead require its minor-version policy.

The [tag workflow](../.github/workflows/release.yml) already builds four platform archives and
four Linux packages, publishes all five workspace crates with trusted publishing, verifies registry
installation/ownership, attaches checksums and `install.sh`, then updates Homebrew. Bare filenames
in future checksums let affected older clients verify the next release before obtaining the fixed
parser. Historical tags/assets stay intact. Homebrew independently hashes archives and crates.io
does not consume the checksum file.

Remaining local gates are lockstep metadata/lockfile refresh, UTC-dated curated v0.9.5 changelog,
version-agreement script, security/license audits, Rust 1.94 MSRV validation, and clean-source
package/publish dry runs. The correction tree passed 446 locked all-target workspace tests,
formatting, warnings-denied Clippy, actionlint and checksum/script fixtures on Rust 1.98 macOS
arm64; that does not establish candidate metadata or Rust 1.94 compatibility. Preserve patched
Rustls and avoid unrelated lockfile changes. A private clean temporary Git snapshot can verify
packaging without `--allow-dirty`, upload, or changing the main checkout.

Actual [CI workflow](../.github/workflows/ci.yml) definitions govern hosted gates rather than the
historical matrix summary in RELEASE.md. Exact candidate/merge CI and Audit must pass before the
annotated immutable v0.9.5 tag; manifest changes trigger Audit's existing path filter. After tagging,
verify workflow success, five exact registry versions, eight bare checksum entries and matching
archive/package digests, matching installer source, and Homebrew v0.9.5 URLs/digests. Inventory any
partial publication before retries. Neither preparation nor publication includes this host's
installed binary or live jobs/services; use extracted artifacts/isolated fixtures for validation.

### Publication verification

PR #18's candidate `b718ab5` and merge `c6d1aaa6a3126bb9d54b2b83b17f99a93115abd1` passed
their exact hosted CI/Audit gates. The annotated v0.9.5 tag identifies the merge commit, and
[Release run 36660197485](https://github.com/WhiteKiwi/locron/actions/runs/36660197485) succeeded
across four platform builds, all five registry packages, a fresh registry installation, GitHub
assets, and Homebrew publication. The [public release](https://github.com/WhiteKiwi/locron/releases/tag/v0.9.5)
was published on 2026-09-30 UTC; exact registry inventory is `all`.

Direct downloads verified all eight archive/package hashes against bare checksum entries and
GitHub asset digests. This also checks the old parser's exact-filename lookup contract without
running a live updater or altering service registration. The tagged installer bytes match the
release asset. An extracted macOS arm64 binary and a pinned temporary no-service standalone
installation both report 0.9.5; the latter writes the correct receipt without creating scheduler
state. The Homebrew formula has all four v0.9.5 archive URLs and matching hashes. Existing
versioned assets/tags and this host's installed binaries, real jobs, and services remain intact.
The [Homebrew tap validation](https://github.com/WhiteKiwi/homebrew-tap/actions/runs/36661419833)
also passed on both macOS and Linux at formula commit `578c9e7e3c01496ba03eb29dbe6dc0f812809e94`.

## 42. Second feedback: target labels, stable launchd refresh, identity, and published updater smoke (2026-09-30)

Research checked main `f690f44`, current release/service/CLI sources and their regression fixtures,
the v0.9.2 updater/service implementation, Apple's current host manpages, and primary Apple
ServiceManagement documentation. No executable, registration, service, job, release artifact, or
signing account was changed by this research session.

### Human target labels

`list_target_summary` is the common redacted human formatter for list rows, add/update result
summaries (including dry runs), and show's TARGET section. Change only its prefixes to
`process: EXE [ARGS...]`, `shell: CMD`, and `http: METHOD URL`. Preserve argument order and existing
redaction. `why` and `explain` do not currently render these target summaries, so this scope does
not require adding target sections there. Stored/JSON target kinds remain `process`, `shell`, and
`http`; the machine list's existing latest-run observation and all API/MCP targets stay intact.

CLI tests include exact target/table strings, aliases, redirected full output, width fitting,
`--no-trunc`, and human/JSON redaction checks. Update expected human strings and target width
budgets, retain exact full-table and exact-fit assertions, and add direct-process `http` versus
HTTP-target discrimination. Do not replace structured machine kinds, arguments, or warning hints.
Update CLI/operator examples and formatter comments consistently.

### Loaded macOS services already use SIGTERM without rebootstrap

The current install flow is `session_available → write_registration → reload → is_loaded`.
For a loaded macOS service it then calls `restart → stop → launchctl kill SIGTERM` and reports
status. `reload` is a no-op; `enable`, `bootout`, and `bootstrap` are not called on this loaded
branch. An ordinary self-update invokes this flow by executing the newly replaced binary's
`service install`; an already registered dashboard similarly receives `dashboard enable`.
Thus the demonstrated unnecessary action is the identical plist write, not repeated explicit
rebootstrap. `write_private` currently rewrites bytes and permissions even when content matches.

Both plists have stable labels (`dev.locron.daemon`, `dev.locron.dashboard`), canonical executable
paths, `RunAtLoad=true`, and `KeepAlive=true`. Apple-supplied host manuals
`/usr/share/man/man1/launchctl.1` and `/usr/share/man/man5/launchd.plist.5` distinguish loading
service definitions from signaling processes, define persistent enable/disable overrides, and
state that boolean KeepAlive keeps the job running. This supports preserving the existing loaded
registration while SIGTERM/KeepAlive restarts onto a binary replaced at the same path.

Compare the desired rendered plist bytes with the existing file before writing; a match should
leave the file/inode/mtime intact. A missing file is first registration; a different file is a
changed definition, not an ordinary same-path binary replacement. Propagate inspection errors
rather than silently assuming unchanged. Keep machine install outcome fields stable.

Semantics to preserve and test:

- Loaded, matching definition: skip file rewrite, preserve loaded domain/enable overrides, send
  the existing graceful SIGTERM, and observe a replacement process in a controlled backend test.
- Unloaded service, including an explicitly disabled service: explicit install/enable currently
  enables and starts it. A content match must not turn explicit install into a no-op.
- Manual daemon lock held with service unloaded: retain registration/enable but defer start.
  A manually running daemon is not signaled. The dashboard does not defer to the daemon lock.
- No manager session: retain the existing guidance before any registration write.
- Changed definition: current code writes the new plist but SIGTERM does not reload its cached
  launchd definition. Fresh definition loading requires a distinct unload/bootstrap path; define
  its graceful shutdown and failure semantics before extending this correction.

The uninstall-oriented `wait_until_stopped` returns success even at its timeout and treats a
replacement PID as the old process having exited. It cannot alone prove a changed-definition
reload is graceful: KeepAlive can respawn the previous definition before bootout. Do not silently
reuse it and claim guaranteed safe reload. The smallest correction can preserve current changed-
definition semantics while avoiding identical writes; a broader reload correction needs a reviewed
policy and targeted tests. Avoid `kickstart -k` as a shortcut because it kills a running process.

Use temporary contexts for plist-byte comparisons and fake/backend-command fixtures for loaded,
changed, disabled, deferred, and no-session call ordering. Existing `service_backends` tests operate
real managers and default-state registrations; this user's personal host is not an appropriate
place to execute them for new behavior. Any real launchd observation belongs on an ephemeral CI
runner. File-write reduction does not establish elimination of BTM notifications or historical
entries; no background-item database reset is justified by this source evidence.

### Release signing and unbundled identity

The release workflow runs Cargo's release build (the release profile strips symbols), copies the
binary into an archive directory and creates the tarball. There is no explicit Developer ID
signing, stable signing-identifier setting, notary submission, keychain import, or app bundle in
that pipeline. Plists include no `AssociatedBundleIdentifiers`, and the project does not use
SMAppService. A stable launchd label is not a signing certificate or bundle identity.

Apple's [Updating helper executables from earlier versions of macOS](https://developer.apple.com/documentation/servicemanagement/updating-helper-executables-from-earlier-versions-of-macos),
checked through its official Markdown representation, explains that legacy helper/app association
requires an actual app bundle with matching Team Identifier known to Launch Services. Without
bundle association, the UI uses the signing certificate's organization; unattributable unsigned
executables fall back to the executable name. Do not add a fictitious bundle identifier or claim
that a plist label controls developer attribution. A stable code identifier may help maintain
identity across builds, but ad-hoc signing alone supplies no recognized developer certificate.

The parent session independently found linker ad-hoc signatures with no TeamIdentifier in both
installed v0.9.2 and downloaded official v0.9.5 arm64 binaries, with different generated signing
identifiers. It also found a valid local Developer ID Application identity. These are parent-owned
read-only host measurements, not proof of a usable GitHub signing credential or notarization path.
Parent research will establish exact signing/account requirements; this source review does not
justify exporting a private key, enrolling an account, or claiming notifications are resolved.

### Safe published v0.9.2 updater verification

Add a post-publication job on an ephemeral GitHub-hosted Linux x86_64 runner. Guard Linux/architecture
and the expected release tag; download the official v0.9.2 archive/checksums, verify its hash while
accepting historical `./` syntax, extract/copy the binary into a temporary fixture, and write the
exact standalone receipt payload (`locron.install/v1\nstandalone\n`) beside that copy. Assert the
baseline reports 0.9.2 before invoking its real self-update against the published latest release.
This verifies the old running parser rather than only compiling/testing the corrected parser.

Preserve the runner's real HOME. Put `LOCRON_STATE_DIR` and `XDG_CONFIG_HOME` inside the fixture;
unset `XDG_RUNTIME_DIR` and `DBUS_SESSION_BUS_ADDRESS`, plus service-backend/update-origin overrides.
Both v0.9.2 and current SystemdPort return no-session before writing registration when the runtime
variable is absent. The isolated XDG config path also makes dashboard status find no preexisting
registration, preventing dashboard enable. `LOCRON_NO_SERVICE` is installer-only and cannot be
used to suppress updater service effects. The job must not run on the owner's Mac or install into
its normal prefix.

Check the updater envelope's from/to versions and success, final binary version/digest, absence
of unit files, and unchanged isolated job state. Preserve published assets/tags. Ensure the latest
release tag matches the workflow's expected tag before invoking the old updater, and fail clearly
if publication visibility or a concurrent newer release invalidates that assumption. Downloaded
archive checksum/digest verification and the existing fake updater/service regressions remain
separate gates; a Linux smoke proves published updater compatibility, not macOS signing or BTM UI
behavior. No actual updater smoke was executed by this read-only research session.


### Parent identity measurements and selected v0.9.6 signing route

The parent completed additional read-only host checks and a disposable-copy signing proof. The
installed v0.9.2 and downloaded v0.9.5 arm64 binaries have linker ad-hoc signatures, no TeamIdentifier,
and generated identifiers `locron-d286d7a30a27f9e0` and `locron-961b8069c8f84fb9`. A temporary copy
signed with the existing Developer ID Application identity passed strict verification with
`Identifier=dev.locron.cli`, `TeamIdentifier=4H4Z446LHS`, hardened runtime, and secure timestamp.
The existing login-keychain `c6s-notary` profile authenticated a read-only notary history request;
that proves authentication, not accepted notarization of Locron. No private key was exported.

The parent's BTM inventory identifies exactly two Locron legacy agents, daemon and dashboard,
with one live process each. Both currently have `Name=locron`, null DeveloperName and Unknown
Developer attribution. The reporter subsequently confirmed two same-name rows, consistent with the two independent
services. Neither observation establishes historical-record cleanup behavior. Rewriting avoidance and stable recognized
signing may affect future attribution, but historical-entry cleanup and notification elimination
remain unproven. Developer ID identifies its certificate owner rather than inventing a Locron app
bundle or organization.

At this check the repository has only `TAP_GITHUB_TOKEN` as an explicitly configured Actions secret,
only the `crates-io` environment, and no Locron self-hosted Actions runner. The selected route keeps
four hosted build jobs, then runs a tag-only macOS signing job using a one-job foreground ephemeral
runner controlled by the parent. It consumes both unsigned Mac artifacts, signs their binaries
through the already authorized Mac keychain, notarizes ZIP payloads, strictly verifies signature/
team/identifier/runtime/timestamp, assesses Gatekeeper, and emits a separate signed-macos artifact.
The parent owns transient runner registration, execution and cleanup; no persistent launchd runner,
private-key export, new signing account, or permanent CI signing credential is introduced.

Home-hub [runner policy](/Users/whitekiwi/workspaces/home-hub/hosts/RUNNER_PLAN.md) and
[Apple runner runbook](/Users/whitekiwi/workspaces/home-hub/runners/c6s-clients/INFRA.md) require
cooperating with `~/Library/Caches/home-hub/apple-signing.lock` and using SSD runner/cache paths.
The existing ios-lab helper implements the lock with Python `fcntl.flock` on an open persistent
file. Use that same advisory-lock primitive, wait with a bounded deadline, and release the handle
in all outcomes; do not unlink/replace the lock file or change global keychain search lists. Use
run-scoped staging under the parent's SSD workspace and retain diagnostic metadata, not credentials.

Apple's [Customizing the notarization workflow](https://developer.apple.com/documentation/security/customizing-the-notarization-workflow),
checked in its official Markdown representation, supports `notarytool` submission and accepted
status verification. It explicitly distinguishes ZIP notarization from stapling: neither ZIPs nor
standalone binaries support attaching a ticket directly. Submit the signed binaries in ZIP payloads
using `c6s-notary`, require a structured `Accepted` result, then assess the exact final signed
binary. Repackage those unchanged signed bytes into the existing platform tarballs. This route
supports online Gatekeeper verification and must not claim an offline stapled distribution.
A signing proof alone must not satisfy the accepted-notarization gate.

### Signing pipeline edge cases and minimum verification design

- Fail closed on a non-tag/non-push/noncanonical repository event, unexpected tag/version, missing
  expected archive, wrong architecture/layout, missing signing identity/profile, wrong team/code
  identifier, absent runtime/timestamp, failed strict verification, non-Accepted notarization, or
  failed Gatekeeper assessment. Do not emit a publishable signed artifact after a partial two-
  architecture success. Keep notary request IDs/status and rejection logs for diagnosis.
- Read the two unsigned artifacts from this exact workflow run; require both expected signed
  archive names before artifact upload. `publish-crates` must depend on successful signing, so no
  partial registry release precedes a signing failure. The GitHub publisher must download Linux
  artifacts and the signed-macos artifact explicitly, never merge all original build artifacts.
  Generate checksums and Homebrew hashes only after selecting the signed Mac tarballs.
- No binary stripping, modification, rebuild, or signing operation may follow accepted submission;
  only archive-container repacking is allowed. Verification should re-extract final archives and
  compare their binary bytes/digests with the accepted signed payload. `codesign --verify` proves
  integrity but does not replace team/identifier/runtime/timestamp or notarization checks.
- Workflow event guards protect the trusted signing job, but repository-scoped self-hosted runners
  can accept other eligible jobs. Parent activation must target/watch the exact approved release
  run, use restrictive or run-scoped labels, and prevent unrelated PR/arbitrary-ref jobs from
  reaching this temporary runner. A guard only inside the signing script is not a complete runner
  isolation boundary. Keep the runner available for only one expected job and remove it afterward.
- Minimum deterministic tests can inject a command runner into signing orchestration: two-archive
  success and output names, identity/verification failure, notary rejection/invalid status, missing
  architecture, and final-archive byte preservation. Fixtures use fake tools/results and temporary
  files, without using the keychain or network. Add static workflow assertions for tag guards,
  signed dependency gates, and explicit publisher artifact selection. Keep one actual parent-owned
  signing/notary acceptance proof and the published old-updater smoke as separate live gates.


### Actual notarization rehearsal and corrected standalone-code validation

The parent submitted a disposable two-architecture signed Locron v0.9.5 ZIP through the existing
profile. Submission `48af4dbd-0222-4aa7-84ae-465d2aec04a2` returned Accepted with `issues=null`,
and its log contains tickets for both architectures. The preliminary `spctl --assess --type execute`
checks nevertheless exited 3 with `rejected (the code is valid but does not seem to be an app)`.
This is an unsuitable app-assessment tool for this unbundled executable, not evidence that the
accepted notary tickets disappeared. Re-signing identical binaries produced matching ticket
cdhashes; the parent verified both with
`codesign --verify --strict --verbose=4 -R=notarized --check-notarization`, each exit 0 and an
explicit notarized requirement satisfied.

Apple DTS's [Testing a Notarised Product](https://developer.apple.com/forums/thread/130560),
updated 2024-12-05 and checked 2026-09-30, distinguishes apps from other code. It specifies
`codesign -R="notarized" --check-notarization` for other code on macOS 10.15+. The implementation
plan therefore replaces spctl app assessment with the strict online notarized code requirement.
Receipts record that actual command, requirement and exit result rather than a fabricated spctl
source. Failed checks retain bounded diagnostics, without providing partial publication archives.
This quick code check does not establish fresh-machine quarantine behavior or offline startup;
Apple recommends separate fresh-machine product testing, and standalone tickets cannot be stapled.


### Corrected full-helper rehearsal and clean package evidence

The parent reran the complete corrected signing helper on disposable copies of the two official
v0.9.5 Mac archives. Its internal `locron.macos-signing/v1` receipt records
`dev.locron.cli`, team `4H4Z446LHS`, and Accepted submission
`f6dc05ed-d6ba-4f25-80f2-7923c1c5db66`. The retained notary log has `issues=null` and tickets for
both architectures. Both strict online notarized code requirements returned exit 0. The helper
verified architecture/version/signature properties and repacked archives without changing the
signed executable bytes. This validates the selected pipeline on real disposable inputs; it
neither modifies the existing public v0.9.5 assets nor claims v0.9.6 has already been published.
The parent confirmed preservation of the installed binary and plist hashes/inode/mtime, and the
existing Keychain search list/default.

A clean private Git snapshot containing tracked candidate sources and exactly the four new
release scripts passed Rust 1.94 `cargo package --workspace --locked` and
`cargo publish --workspace --dry-run --locked`. All five crate inventories, README/licenses,
excluded-build-material and server dist checks passed; no package was uploaded. The parent owns
these checks and retained `clean-msrv-package.log` under its private publication workspace.
Hosted exact-revision CI/Audit, tagged signing/publication, published old-updater smoke, final
channel inventory and post-publication host readback were subsequent parent gates; their completed
results are recorded in §44.


## 43. Acceptance daemon startup observer interferes with its lock (2026-09-30)

Merged-main CI run `36668890592`, Rust 1.94 Linux job `109739419238`, failed at
`acceptance_matrix.rs:135` in `run_wait_renders_ordered_retry_output_frames_for_one_durable_run`:
`daemon exited during startup`, with `Some(ExitStatus(unix_wait_status(512)))` (exit 2).
The same-tree PR passed its nine CI jobs and two Audit groups. The acceptance harness, daemon
acquisition and store lock source are unchanged between v0.9.5 and merged main `26a6128`.
The parent retained the exact failing hosted job log in its private publication workspace.

The harness currently polls `DaemonLock::try_prove_free`. Source proves this is an active probe:
it creates/opens the permanent lock file, sets permissions, briefly acquires an exclusive
nonblocking OS lock, then unlocks it. The concurrently starting daemon also uses a nonblocking
exclusive acquisition and immediately fails when that lock is held. The observer can therefore
cause the very startup refusal it is trying to detect. The hosted daemon's stderr was discarded,
so its exact fatal cause is unavailable; the race is established in source, while attributing
this particular exit 2 to it remains a hypothesis. Do not silently rerun the failure to green.

Use a test-only passive observer: read existing `daemon.lock` bytes, deserialize public
`LockMetadata`, and require its PID to match the spawned, still-live child. Missing/partial/stale
metadata is not readiness; it remains bounded by the existing startup deadline. The observer
must never create a file, acquire/release a lock, or change permissions. Keep the test deadline
and daemon product behavior unchanged. A tempfile-backed stderr stream avoids pipe backpressure
and supplies bounded diagnostics on premature child exit or timeout. Construct the RAII daemon
guard before observing so a failing assertion also cleans up the spawned fixture child.

Deterministic regressions should cover absent/malformed/stale metadata without mutation, matching
metadata readable while an exclusive owner holds the lock, and bounded stderr on a deliberately
failed child. Run the corrected acceptance suite under MSRV and pinned stable tooling, then use
exact-revision hosted CI/Audit as the publication gate. This correction is confined to the test
harness; it requires no product API, scheduler policy, storage migration, service manager, signing,
or dependency change.


The reviewed correction is implemented only in `tests/acceptance_matrix.rs`. Startup observation
now reads existing metadata and requires the spawned PID while its child is live, without any
lock probe or write. Tempfile stderr avoids pipe backpressure and supplies at most 8192 captured
bytes plus a truncation marker. The daemon guard exists before observation and cleans up failures.
Three deterministic regressions verify missing/malformed/stale/held-lock observations without
mutation, bounded stderr from an intentionally failed child, and timeout guard reaping.
Rust 1.94 and 1.98 each passed all nine acceptance tests, including the previously failing retry
scenario. Pinned warnings-denied locked all-target CLI Clippy, formatting and diff checks passed.
These results verify the harness correction but do not recover the discarded hosted stderr or
retroactively establish the exact original exit-2 cause. Exact-revision hosted PR/main CI and Audit
subsequently passed before tagging; see the publication receipt in §44.


During Windows-support PR41, head a6f0189/run 37041311097 Linux stable failed
global_environment::execution_layers_global_environment_and_persists_the_resolved_executable
at its startup observer, again with discarded stderr. Source shows that helper still actively
acquires the lock before the daemon is ready; attempt_history and service_lifetime retain the
same interference path. Extend the approved passive expected-child-PID observation to these
remaining fixtures. The exact hosted fatal cause remains unavailable until captured diagnostics
replace the discarded stream; keep all existing startup budgets and actual behavior assertions.

## 44. v0.9.6 signed publication and preservation receipt (2026-09-30)

Reviewed [PR #19](https://github.com/WhiteKiwi/locron/pull/19) implemented the second feedback.
The scoped test-harness follow-up [PR #20](https://github.com/WhiteKiwi/locron/pull/20) passed all
nine [PR CI jobs](https://github.com/WhiteKiwi/locron/actions/runs/36670144377) and both
[Audit groups](https://github.com/WhiteKiwi/locron/actions/runs/36670144256). Its reviewed merge
`531d1a48e796e1949de4e1bbd1e6562646935f46` passed all nine
[main CI jobs](https://github.com/WhiteKiwi/locron/actions/runs/36670573034), including full hosted
service-backend and clean MSRV package gates, and both
[main Audit groups](https://github.com/WhiteKiwi/locron/actions/runs/36670575858). The immutable
annotated `v0.9.6` tag object `6090839c60019b24c399d83671cdd56df1f60a84` peels to that merge.

All eight jobs of [release run 36671250966](https://github.com/WhiteKiwi/locron/actions/runs/36671250966)
succeeded: four hosted platform builds, the two-architecture Mac signing job, workspace registry
publication with exact registry-install verification, GitHub/Homebrew publication, and the real
published-old-updater smoke. The one-job foreground ephemeral runner #21 had only the run-specific
label `locron-signing-36671250966`. An administrator-owned pre-job hook checked repository, push
event, tag, commit, run ID/attempt, workflow/ref and job against the approved context before checkout;
the actual job logged approval and runner ID 21. It exited 0 and de-registered afterward. Its owned
temporary runtime, SSD work and cache directories were removed, with six unrelated runner listeners
still live. No signing key export, new CI signing secret, Keychain ACL/search/default change, or
persistent runner service was needed.

The retained `locron.macos-signing/v1` receipt records identifier `dev.locron.cli`, team
`4H4Z446LHS`, and Accepted notary submission `33bee0a5-7d30-4c76-82e2-799f41057b0f`.
The notary log has `issues=null` and tickets for both architectures. Independent downloads from
[the public release](https://github.com/WhiteKiwi/locron/releases/tag/v0.9.6) match both archive and
signed executable hashes in the receipt. Apple Silicon and Intel binaries report 0.9.6, have the
expected architecture, Developer ID Application authority, team, identifier, hardened runtime and
secure timestamp, and both pass
`codesign --verify --strict --verbose=4 -R=notarized --check-notarization`. A private standalone
no-service installation reports 0.9.6 and preserves the signed Apple Silicon executable bytes.
This verifies released code and online tickets, not fresh-machine quarantine, offline behavior,
Settings attribution refresh, notification suppression, or cleanup of historical background rows.

Independent crates.io inventory reports all five 0.9.6 packages. GitHub carries exactly ten assets;
all eight bare-name archive/package checksums and GitHub asset digests match downloaded bytes.
The installer matches the tag. Debian and RPM metadata agree on 0.9.6-1 and their intended
architectures; both Debian payload binaries match the corresponding Linux tarballs. Homebrew
revision `3fb2113c3b4fccdfaebc46ec9fbd4e9d12120f93` has all four v0.9.6 URLs and matching hashes.
Both Linux and macOS jobs in [tap run 36672303131](https://github.com/WhiteKiwi/homebrew-tap/actions/runs/36672303131)
passed. Isolated disabled-job fixtures using the public Mac binary confirm distinct `process:`,
`http:` and `shell:` human labels in add/list/show, preserved LAST RUN, and unchanged JSON target kinds.

The hosted Linux x86_64 smoke used the actual public v0.9.2 standalone binary and its self-update
path against this public release. Its `locron.published-updater-smoke/v1` receipt records
`from=0.9.2`, `to=0.9.6`, `job_state_preserved=true`, `manager_units=0`, and final binary SHA-256
`9eeb91c241756f8c7df068b5d5df604bb23c773bb1d845d45b5940ca9d17e172`. This is an actual upgrade on
an isolated hosted runner, separate from the reporter's earlier Mac v0.9.2 → v0.9.5 confirmation.

Post-publication owner-host readback preserves the installed 0.9.2 binary hash/inode, both LaunchAgent
hashes/inodes/mtimes, daemon/dashboard PIDs 859/870, and all 45 durable job records excluding naturally
advancing scheduler cursors. Keychain search/default metadata and v0.9.2–v0.9.5 release IDs and asset
IDs/names/sizes/digests/update timestamps agree with the baseline. No owner installation, job,
service registration, or global background-item database reset was performed. The same-name two-row
observation is consistent with the two independently registered services; the release reduces
identical plist writes and supplies distribution identity without combining those services.


## 45. Project-only task migration research (2026-10-02)

### Frozen source and current inventory

The parent froze `docs/TODO.md` from base `93b4144cc5f9d056609e3f88e37e9451ea2ad506`
before migration planning. The private `TODO.source.md` snapshot SHA-256 is
`aeaf61423326384d4e38709da731e764029777c6bf808c4d06403372b6ee9d52`.
Read-only verification found the live file byte-identical to that snapshot: 713 lines,
69 top-level checklist items across 15 phases, 68 complete and one open, no nested checkboxes,
and a Verify entry for every item. Preserve this original inventory independently of later
administrative planning notes; do not accidentally migrate newly appended migration steps.

| Phase | Original items | Open |
| --- | ---: | ---: |
| Acceptance startup observer correction before v0.9.6 tagging | 3 | 0 |
| Second feedback and v0.9.6 signed macOS release | 8 | 0 |
| v0.9.5 feedback correction release | 5 | 0 |
| First feedback triage | 5 | 0 |
| v0.9.4 security patch release | 5 | 0 |
| Rustls advisory remediation | 3 | 0 |
| v0.9.3 patch release | 3 | 0 |
| Active dashboard run detail live following | 4 | 0 |
| CI toolchain and cache optimization | 6 | 1 |
| Deterministic dashboard port-policy verification | 3 | 0 |
| README information architecture refresh | 3 | 0 |
| v0.9.2 patch release | 4 | 0 |
| Dashboard lifecycle human output and stale detail recovery | 3 | 0 |
| v0.9.1 patch release | 4 | 0 |
| crates.io source installation and trusted publication | 10 | 0 |

The sole open item is source order 41 at line 433, “Restrict Rust cache creation to the default
branch without disabling useful dependency and target restoration.” Preserve it as Todo: source
configuration alone is not its hosted restore-only evidence. The crates.io phase also has a shared
introductory authorization/release-context paragraph before its first item; retain it for all ten
items. Multiline title continuations and all Verify, Evidence, local/hosted evidence, commands,
run IDs and links must survive. The parent measured a longest fully joined title of 368 characters;
if an API title limit requires abbreviation, document a deterministic display-title rule and retain
the exact full source item in its body. Do not silently truncate requirements.

Read-only GraphQL inventory found two existing owner Projects, both private, with no title `locron`.
The requested destination is a new private `locron` Project owned by WhiteKiwi and linked to
WhiteKiwi/locron. Repository issue #4 remains CLOSED with its original human-output title and URL;
no issue creation, deletion, conversion or status change belongs to this migration.

### Draft content and Project item identity

[GitHub's draft-item documentation](https://docs.github.com/en/issues/planning-and-tracking-with-projects/managing-items-in-your-project/adding-items-to-your-project)
confirms that draft issues live only in a Project and support title/body/custom fields. Repository,
labels and milestones require conversion to a repository issue. Link the Project itself to Locron;
do not convert drafts or populate their repository field. A Project row's Status is separate from
its draft content type. Use Todo, In Progress and Done, mapping the original one open item to Todo
and 68 checked items to Done, with no In Progress migration items.

Read-only GitHub GraphQL schema introspection confirms distinct `DraftIssue.id` and
`ProjectV2Item.id`. Draft IDs commonly use `DI_`; item IDs commonly use `PVTI_`. Treat both as opaque
Node IDs: content edits target the draft ID, while custom-field/status/position updates target the
Project item ID. Record both from readback, never derive one from the other. ProjectV2Item supplies
`fullDatabaseId` as BigInt; `databaseId` is deprecated for its 32-bit limit. Store the full identifier
without lossy JavaScript-number conversion. DraftIssue has no URL field in the checked schema;
use the canonical Project URL and a verified item deep link based on its full database identifier,
not a guessed Issue URL or a decoded Node ID. UI deep-link behavior remains a readback check.

[GitHub's Projects API guide](https://docs.github.com/en/issues/planning-and-tracking-with-projects/automating-your-project/using-the-api-to-manage-projects)
shows the draft creation mutation and separate field-update operation. Create drafts using
`addProjectV2DraftIssue`, retain its returned Project item, and fetch its draft content. Read actual
field/option IDs before updates rather than assuming default Status IDs. Use Phase (full original
heading), Legacy ID (stable unique source key) and Source order (1–69) for deterministic traceability.

### Drift-free migration and uncertain mutations

Build a deterministic manifest from the frozen snapshot, not the changing live checklist. Each
record should retain source order/phase/status, original title and exact item block, relevant phase
context, immutable source commit/line reference, payload hash and planned metadata. Preserve all
original item bytes in ticket bodies or an explicitly marked original-source block. If title
normalization is needed, compare against the manifest's display title while retaining its full
original content. Source order reconstructs the old sequence regardless of default Project sorting.
Legacy IDs must be unique and present in the initial body as well as the eventual custom field.

The checked mutation input has `clientMutationId` but no documented idempotency-key guarantee.
Do not treat an echoed client ID as deduplication. After an uncertain create response, pause writes
and page through destination drafts, including archived items, looking for that exact body marker
and matching payload before retrying. One match recovers its IDs; multiple matches or mismatched
content require reconciliation, not another create or automatic deletion. Record each acknowledged
item/field update in a private checkpoint. A failed later field update must resume updates on the
existing item rather than create a replacement. Apply the same no-blind-retry rule to Project
creation by reading owner Projects and confirming intended identity first.

Before changing repository authority, perform fresh paginated GraphQL readback of all rows and
custom field values: exactly 69 unique original Legacy IDs, all DraftIssue content, source order
1–69, exact phase and manifest title/body/hash agreement, 68 Done/one Todo/zero In Progress, and
no unintended archive state. Confirm destination title/owner/private visibility and repository
link. Verify pagination is exhausted, not merely that the first page count looks right. Read back
issue #4 and repository issue inventory to establish no issue mutation. Partial migration or a
verification mismatch must leave the old checklist authority intact.

### Documentation cutover and historical preservation

Only after complete readback should repository instructions declare Project draft tickets the
execution/progress authority. `docs/TODO.md` can become a concise migration pointer with the
canonical private Project link, source commit/hash and inventory; it must not retain a parallel
live checklist. Preserve `docs/TODO-archive.md` historical sections and their old commands/links
verbatim. A small clearly separated present-day notice can explain that its former live-TODO
reference is historical. Keep BACKLOG's inactive ideas and content out of this migration.

Live-reference changes are needed in AGENTS.md's core read order, planning steps and Progress
section; CONTRIBUTING.md's planning table, completion rule and automatic issue-first advice;
IMPLEMENTATION.md's authority paragraph and active deviation rule; ARCHITECTURE.md's document map;
STATUS.md's progress-authority sentence; and BACKLOG.md's activation guidance. Older implementation
sections explicitly prescribing live TODO compaction should be marked historical/superseded rather
than silently rewritten as though they originally used Projects. Dashboard implementation's old
TODO references describe completed work; qualify them as historical when needed. README has no
TODO link, but its contribution section is an appropriate short discoverability pointer to the
new task workflow. Research and historical evidence references need not be mechanically replaced.

The private Project will be unavailable to readers without permission. State that explicitly and
keep public product requirements/architecture and historical source evidence available in the
repository. Future maintainers must review relevant Project drafts after SPEC/FINDINGS/IMPLEMENTATION,
record concrete Verify criteria for multi-step work, and mark Done only after verification.
This administrative migration does not change Locron product behavior, tests, release channels,
or runtime state. Parent-owned external creation/readback and PR publication follow plan review;
this research session changed only this findings section.


Parent plan clarification: keep external contributor bug reports on the existing repository issue
tracker; draft-only applies to maintainer execution tasks, not a ban on public bug reports. Update
`.github/PULL_REQUEST_TEMPLATE.md` so implementation PRs refer to relevant Project drafts while
external bug-fix PRs can still refer to repository issues. The selected local layout is a workflow
and static legacy-to-draft map in `docs/PROJECTS.md`, a byte-identical source snapshot in
`docs/planning/TODO-2026-10-02.md`, and a pointer/management guide in `docs/TODO.md`. Put archival
context outside the snapshot file so its required SHA-256 remains exact. Update only the archive
introduction's present-day pointer; preserve its historical body. Keep public source requirements
and static migration references readable while making private Project access limitations explicit.
An optional migration helper belongs in the parent's private work directory, with parent review
and execution of mutations; it is not a new shipped Locron feature or public operational script.


### Verified migration and documentation cutover (2026-10-02)

The parent created private Project `PVT_kwHOAe6fwM4BlcwN`, number 4, owned by WhiteKiwi and linked
to WhiteKiwi/locron. Fresh exhaustive page-size-25 readback verified 69 unique unarchived DraftIssue
items, exact deterministic manifest title/body content, all phase/Legacy ID/source-order fields,
and 68 Done/one Todo/zero In Progress. The three TABLE_LAYOUT views are Active (number 1,
`-status:Done`), History (number 2, `status:Done`), and All tasks (number 3, no filter). Repository
issue #4 and the complete native issue inventory retained their baseline identities and state.

The private helper records acknowledged IDs and field updates with fsynced checkpoints; a known
Project item ID is recovered by direct node readback when list visibility lags. Missing known
nodes, foreign entries, content drift and uncertain mutations stop further writes. Nine offline
fixtures passed, including those recovery/refusal cases and multi-page/full-width ID handling.
The acceptance receipt and mapping stay in the parent's private migration work directory; the
repository contains only the workflow, static 69-ticket link map and byte-identical source snapshot.

Five display titles were abbreviated at a word boundary to at most 240 characters for readability,
not an asserted API limit. Full original blocks, all Verify/Evidence text and the shared phase
preface remain in draft bodies. Historical Done states preserve original records; they do not claim
new execution of past checks. `LOCRON-TODO-041` remains Todo. Archive and BACKLOG historical bodies
remain unchanged; only their present-day intros point to the new execution authority. Item links
use read-back fullDatabaseId values; the API verification did not test browser navigation for each
private deep link. Public source links and the Project/view links provide traceable alternatives.


The documentation cutover's local gate passed: byte-identical frozen snapshot with the recorded
SHA-256; archive and BACKLOG bodies unchanged after their first section heading; 69 static mapping
rows matching the acceptance receipt and immutable source links; no checklist in the TODO pointer;
50 existing local Markdown targets; documentation-only file scope; and `git diff --check`.
The private helper suite passed all nine fixtures again after migration. The parent owns PR
publication; publication and hosted-check evidence is recorded on the migration PR. No Rust tests
were rerun for this documentation-only diff.

## 46. Native Windows 11 adapters and safety boundaries (2026-10-02)

### Scope and evidence boundary

This research follows the Windows amendment in SPEC. It selects feasible interfaces for Rust
1.94 while retaining `unsafe_code = "forbid"` in every Locron workspace crate. The first Windows
release is unsigned; Authenticode onboarding and publisher pinning are deferred and are not
preconditions for these runtime choices. The 2026-10-02 plan used the private Locron Project for
execution tasks and verification evidence. The 2026-10-03 workflow amendment moves current
progress to repository Issues; dated Project references below preserve their original context.

Evidence consists of Microsoft/Rust documentation, inspection of the dependency source identified
below, and read-only reflection confirming that stock Windows PowerShell 5.1 exposes
`DirectoryInfo.Create(DirectorySecurity)`. This is interface/source feasibility, not a claim that
the Windows implementation has compiled or passed standard-user acceptance. The parent installed
an isolated Rust 1.94 toolchain; this host still lacks a usable MSVC/SDK linker. Native x64/ARM64
compilation and behavioral evidence must come from Windows runners and clean Windows 11 accounts.

Cargo's workspace lints apply to workspace members which inherit them. Safe dependency APIs may
encapsulate unsafe OS calls without weakening Locron's lint. Do not add a workspace FFI module,
disable the lint, or infer that raw `windows`/`windows-sys` bindings are safe wrappers.
Source: [Cargo workspace lints](https://doc.rust-lang.org/cargo/reference/workspaces.html#the-lints-table).

### Process ownership before target execution

Select `process-wrap = "=10.0.1"` with `default-features = false` and `tokio1`, `job-object`,
`kill-on-drop` features, plus target-specific `win32job = "=2.0.3"`. `process-wrap` declares
Rust 1.87 and uses stable Tokio/Windows interfaces. `win32job`'s inspected source uses edition
2021 and stable APIs, but declares no MSRV; the locked dependency graph still needs a Rust 1.94
gate. Pin the reviewed versions initially and review updates rather than assuming their future
MSRV policy matches Locron's.

`process-wrap` does not expose or accept an externally supplied Job handle. Its actual safe
composition point is `process_wrap::tokio::CommandWrapper::post_spawn(&mut Command, &mut Child,
&CommandWrap)`. Create and retain a separate `win32job::Job` for each attempt, configure
`ExtendedLimitInfo::limit_kill_on_job_close()`, and implement this hook entirely in safe Locron
code: obtain `tokio::process::Child::raw_handle()`, require `Some`, and call
`Job::assign_process(handle as isize)`. Add the dependency's `JobObject` and `KillOnDrop` wrappers
to the command. No `BorrowedHandle::borrow_raw`, raw-handle ownership conversion, or unsafe block
is needed in Locron.

The ordering is material: all `pre_spawn` hooks run first; `JobObject` adds `CREATE_SUSPENDED`;
the native child is created; all `post_spawn` hooks run while it remains suspended; only then
`JobObject::wrap_child` assigns its internal nested job and resumes the child. Thus the separately
retained Locron job owns the attempt before target code can run, including a child which spawns
grandchildren immediately. `KillOnDrop` enables native child cleanup if a post-spawn hook fails
and kill-on-close for the dependency's job. On a Locron enrollment failure, call `start_kill`,
never resume, and preserve/report cleanup uncertainty rather than claiming a completed spawn.
Windows 11 supports nested jobs, including an inherited outer job from Task Scheduler; any
incompatible inherited-job configuration must fail closed, not silently run outside ownership.

Use the safe wrapped child's `start_kill()` for the dependency's `TerminateJobObject` operation,
but use the separately retained job's `query_process_id_list()` to confirm the tree is empty.
Do not treat `process-wrap`'s `wait()` or `try_wait()` as authoritative tree-exit confirmation:
its inspected `wait_on_job` returns after a completion-port message without checking that it is
`JOB_OBJECT_MSG_ACTIVE_PROCESS_ZERO`, and `try_wait` ultimately returns the immediate child's
status. A root exit, pipe EOF, or successful termination request alone is insufficient.
Poll the owned job under a bounded confirmation deadline; an error remains unconfirmed and uses
Locron's existing quarantine/outcome rules. The inspected `win32job` query has a fixed buffer for
approximately 1024 process IDs and returns an error if that is insufficient; never translate this
error into an empty tree. Its list includes processes in child jobs. Keep both job handles alive
through confirmation and output finalization, with neither breakaway limit enabled.

This boundary supervises processes in the owned jobs. Job Objects are not a sandbox against
same-account target code which delegates work to an unrelated service/WMI process. Do not
advertise cancellation of all work performed anywhere on the machine as a Job Object guarantee.

The shared OwnedChild factory source audit confirms process-wrap 10.0.1 JobObject::pre_spawn
overwrites a Command's raw creation_flags and reads only the public CreationFlags wrapper.
Therefore Hidden/Inherit must set that wrapper's inferred public flag newtype, preserving
CREATE_NO_WINDOW through temporary suspension without adding raw Windows bindings. The existing
runner has no raw creation_flags and retains Inherit behavior. generic_wrap::spawn_inner loses
the original native Child on a post_spawn or wrap_child error; no root ExitStatus can then be
recovered from the safe Job query. An uncertain spawn carries a retained independent Job guard
and refuses replay even if its process list later empties. Ordinary successful children retain
their root try_wait capability and require it together with Job emptiness under one supplied deadline.
This factory encapsulates the same reviewed enrollment hook; it exposes no native mutable child
or process-wrap wrapper chain. Native hidden fixtures can safely test console-device availability
by opening CONOUT$, while wrapper policy assertions check the explicit CREATE_NO_WINDOW value.

The subsequent native x64/ARM64/MSRV failure of the CONOUT$-absence assertion exposed a semantic
mismatch, not lost creation flags. The downloaded process-wrap 10.0.1 archive SHA-256 equals the
locked 1f21b97672d2dc848e7b25701ab4535618b92f4861c13cc3f7f7bed52ad3c8da. Its CreationFlags and
JobObject hooks preserve the user flags and add only CREATE_SUSPENDED; Tokio forwards them to
Rust, whose Windows spawn adds CREATE_UNICODE_ENVIRONMENT without NEW_CONSOLE or DETACHED.
Microsoft's pinned console-allocation specification states that CREATE_NO_WINDOW creates a new
console host without inheritance, using an invisible initial connection. Its console server
allocates the buffers before testing whether that connection deserves a visible window.
Opening CONOUT$ may therefore succeed under the correct windowless policy. DETACHED_PROCESS
has different console-allocation behavior and is not a replacement production policy.

Make the final safe Command::creation_flags setter explicit at the native spawn closure, after
every wrapper pre_spawn hook: the selected CreationFlags value plus temporary CREATE_SUSPENDED,
with no conflicting CREATE_NEW_CONSOLE/DETACHED_PROCESS bits. Keep the logical wrapper value
unchanged so JobObject still resumes only after both independent Jobs are enrolled. A test-only
receipt records the exact value passed to that last setter at the actual spawn boundary.
Native controls compare a direct CREATE_NO_WINDOW child and a direct DETACHED_PROCESS child
using the same console-device probe; the former may have a buffer, the latter must not inherit
or allocate one. Retain actual root status, reaping, empty Job and failed-enrollment coverage.
These observations and the audited final setter qualify the selected policy; console-device
availability alone does not establish window visibility. No GetConsoleWindow heuristic is used.

Sources: [pinned Microsoft allocation semantics](https://github.com/microsoft/terminal/blob/2b5336c1fca1e53ceeaac09710a13c478938cc8d/doc/specs/%237335%20-%20Console%20Allocation%20Policy.md#interaction-with-existing-apis),
[buffer allocation and visible-window decision](https://github.com/microsoft/terminal/blob/2b5336c1fca1e53ceeaac09710a13c478938cc8d/src/host/srvinit.cpp#L798),
[Rust 1.94 native spawn flags](https://github.com/rust-lang/rust/blob/1.94.0/library/std/src/sys/process/windows.rs#L302).

Sources: [process-wrap v10.0.1 manifest](https://github.com/watchexec/process-wrap/blob/v10.0.1/Cargo.toml),
[hook ordering](https://github.com/watchexec/process-wrap/blob/v10.0.1/src/generic_wrap.rs),
[suspension/assignment](https://github.com/watchexec/process-wrap/blob/v10.0.1/src/tokio/job_object.rs),
[Windows job operations and wait implementation](https://github.com/watchexec/process-wrap/blob/v10.0.1/src/windows.rs),
[kill-on-drop hook](https://github.com/watchexec/process-wrap/blob/v10.0.1/src/tokio/kill_on_drop.rs),
[creation flag composition](https://github.com/watchexec/process-wrap/blob/v10.0.1/src/tokio/creation_flags.rs),
[win32job ownership](https://github.com/ohadravid/win32job-rs/blob/17080e2a29ea244bb4e58400dc27739075b2fbc8/src/job.rs),
[win32job process-list query](https://github.com/ohadravid/win32job-rs/blob/17080e2a29ea244bb4e58400dc27739075b2fbc8/src/query.rs),
[Microsoft suspended creation](https://learn.microsoft.com/en-us/windows/win32/procthread/process-creation-flags),
[console-device handles independent of stdio redirection](https://learn.microsoft.com/en-us/windows/console/console-handles),
[job inheritance, breakaway, kill-on-close and WMI exception](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects),
[nested process-list semantics](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-jobobject_basic_process_id_list).

### Intentional native descendant handle ownership (2026-10-03)

The pinned Rust 1.94 standard library has a safe `From<std::process::Child> for OwnedHandle`
implementation, stable since Rust 1.63. It consumes the child and transfers its Windows process
handle without waiting or terminating. `OwnedHandle::drop` closes the handle with `CloseHandle`;
it does not kill or reap a process. This permits test-only root-first/crash fixtures to retain
typed ownership while preserving their deliberate live descendants. Capture the PID before the
transfer when required; external retained Job/root observations remain the completion authority.
Normal fixtures still wait, and no workspace unsafe code or MSRV change is required. Native
behavior and lint acceptance remain pending after the reported fixture lint failures.
Sources: [Rust 1.94 Windows process-handle conversion](https://github.com/rust-lang/rust/blob/1.94.0/library/std/src/os/windows/process.rs),
[Rust 1.94 owned-handle drop](https://github.com/rust-lang/rust/blob/1.94.0/library/std/src/os/windows/io/handle.rs).

### Private filesystem creation, handle-based ACLs and reparse checks

Use a fixed stock Windows PowerShell 5.1 adapter for initial private-directory creation. Obtain
the current SID from `[System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value`, not
from mutable USERNAME/USERPROFILE text. Build `DirectorySecurity`, set its owner to that SID,
call `SetAccessRuleProtection(true, false)`, and add only that SID and LocalSystem with FullControl
and ObjectInherit/ContainerInherit. Call `DirectoryInfo.Create(DirectorySecurity)` on one missing
component at a time beneath a validated existing parent. Microsoft documents this overload as
creation with security already applied; creating a broadly inherited directory and repairing it
afterward would create an exposure interval. The overload does nothing to an existing directory,
so existing roots require separate ownership/ACL validation before use.

The inspected `windows-permissions` APIs read/set existing object security; no atomic directory
creation interface was established there. A `cap-primitives` creation path with a caller-supplied
Windows descriptor has not been verified. Check a purely safe Rust alternative in the first
development step if it reduces the adapter cost, but require the same creation-time DACL and
anchored no-reparse guarantees. The concrete fallback above is available meanwhile; this
remaining alternative check does not block platform compilation foundations.

The equivalent directory SDDL is `O:<sid>D:P(A;OICI;FA;;;<sid>)(A;OICI;FA;;;SY)`; protected file
SDDL omits the inheritance flags. Do not grant Everyone, Authenticated Users or Builtin Users.
Do not read or write a token/database/output in a foreign-owned root. Refuse filesystems which
cannot supply and enforce the required DACL rather than returning success from a no-op branch.

For handle-bound readback and existing-file ACL repair, select target-specific
`windows-permissions = "=0.2.4"` and its explicit safe
`wrappers::GetSecurityInfo(&File, SeObjectType::SE_FILE_OBJECT, Owner | Dacl)` and
`wrappers::SetSecurityInfo(&mut File, SE_FILE_OBJECT, Dacl | ProtectedDacl, None, None,
Some(dacl), None)` APIs. An SDDL string parses safely to `LocalBox<SecurityDescriptor>`; its
`owner()` and `dacl()` expose borrowed safe objects. Inspect/read back only Owner/Dacl information,
not SACLs requiring additional privilege. Use explicit wrappers, not the `WindowsSecure`
convenience trait: inspected trait methods pass `SE_UNKNOWN_OBJECT_TYPE`, and its handle
`set_multiple` incorrectly derives DACL/SACL flag presence from the group argument. The explicit
wrappers avoid those defects, allow the correct object type and protected-DACL flag, and have
source-visible error returns. The crate is old, depends on winapi 0.3.9, and has no MSRV claim;
pinning, Rust 1.94 compilation and native ACL tests remain required before acceptance.

Use stable safe `std::os::windows::fs::OpenOptionsExt::{access_mode, share_mode, custom_flags}`
and `MetadataExt::file_attributes` to open and inspect directories/files. Directory guards need
`FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT`; reject
`FILE_ATTRIBUTE_REPARSE_POINT` for every traversed managed component, including junctions, rather
than relying only on `file_type().is_symlink()`. Final-component no-follow does not protect a
parent path. Open the chain from an anchored local drive, retain guards while accessing children,
and omit FILE_SHARE_DELETE so a validated ancestor cannot be renamed beneath the operation.
Where another user could modify a directory, omit FILE_SHARE_WRITE as well to prevent a
write-access reparse-point mutation; refuse sharing/ownership conflicts. This guarded-chain
approach is an inference from documented CreateFile sharing/reparse semantics and must be tested
with adversarial ancestor replacement. Do not claim a separate metadata check closes a race.

New data files can inherit from an already private, guarded parent; validate existing files via
their own no-follow handles before reading or writing sensitive contents. Keep the root/managed
directory guards through SQLite's path-based opens (including WAL/SHM), output creation and
finalization. A conservative implementation may refuse unsafe overrides and unusual reparse
layouts; it must not normalize by following them first. Permission diagnostics must inspect the
actual descriptor and distinguish valid, unsafe and unreadable states.

For a custom state root, validate and guard each existing ancestor, then create each missing
managed component with the private descriptor and open/read back that component before advancing.
If creation races with an existing component, inspect that actual component and refuse foreign
ownership or unsafe ACL/reparse state; success from Create or an already-exists result is not
validation. Do not read secrets first or blindly take ownership to repair an override. Object and
container inheritance supplies owner+SYSTEM rights to new files/subdirectories under a verified
private parent; validate the resulting owner/DACL and use protected descriptors when explicitly
repairing an already-owned object. Test both new default roots and nested custom roots, including
creation races, unsupported filesystems, inherited broad grants and ancestor junction mutation.

Keep these common path/ACL/guard primitives in `locron-core`, shared by store, engine output and
server-token callers, without adding a sixth crate or putting service/job execution there. The
stock PowerShell directory-create operation is a narrowly scoped internal filesystem adapter;
Task Scheduler registration/control stays in CLI. Each caller must hold the returned guard for
the entire sensitive operation rather than treating successful initialization as permanent
proof that a path remains safe.

The PowerShell adapter should be a fixed reviewed script invoked through the absolute stock
`System32\WindowsPowerShell\v1.0\powershell.exe` with `-NoProfile -NonInteractive` and a fixed
encoded command. Supply paths/options as structured stdin JSON; never interpolate them into
PowerShell source, invoke a PATH-resolved PowerShell, require pwsh, use `-ExecutionPolicy Bypass`,
or modify policy. Return structured JSON plus a bounded nonzero failure for unavailable/policy
blocked adapters. Use native safe wrappers for ordinary file reads/writes, reserving fixed
PowerShell operations for private-directory creation and Task Scheduler management. Those
filesystem and Task Scheduler interfaces remain separate and do not accept arbitrary commands.

Sources: [atomic DirectoryInfo creation](https://learn.microsoft.com/en-us/dotnet/api/system.io.directoryinfo.create?view=netframework-4.7.2),
[inheritance protection](https://learn.microsoft.com/en-us/dotnet/api/system.security.accesscontrol.objectsecurity.setaccessruleprotection),
[explicit GetSecurityInfo wrapper](https://github.com/danieldulaney/windows-permissions-rs/blob/8740e4efbd88dd01046ad9c169894f3a52eb6e2c/src/wrappers/get_security_info.rs),
[explicit SetSecurityInfo wrapper](https://github.com/danieldulaney/windows-permissions-rs/blob/8740e4efbd88dd01046ad9c169894f3a52eb6e2c/src/wrappers/set_security_info.rs),
[safe SDDL/security-descriptor APIs](https://github.com/danieldulaney/windows-permissions-rs/blob/8740e4efbd88dd01046ad9c169894f3a52eb6e2c/src/structures/sd.rs),
[convenience-trait limitations](https://github.com/danieldulaney/windows-permissions-rs/blob/8740e4efbd88dd01046ad9c169894f3a52eb6e2c/src/windows_secure.rs),
[Rust safe open flags](https://doc.rust-lang.org/std/os/windows/fs/trait.OpenOptionsExt.html),
[CreateFile flags and sharing lifetime](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew),
[reparse-point mutation](https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ni-winioctl-fsctl_set_reparse_point).

### Native stock-adapter policy and parent-exit proof

Microsoft documents that process execution policy is held in the session environment and does
not write registry policy; Group Policy still takes precedence. The early Desktop source has
System.Management.Automation.Internal.SecuritySupport.GetExecutionPolicy(string shellId), with
MachinePolicy/UserPolicy/Process ordering and the actual Microsoft.PowerShell.ExecutionPolicy
enum. This is evidence for a fixed reflection candidate, not a PowerShell 5.1 runtime claim. Use
only the exact method in the already loaded PSObject assembly and require actual Restricted in
both real adapter children. Missing binding or a different effective policy is a failed proof,
never an env-text substitute, module import, policy bypass or skipped assertion.
Sources: [process policy and precedence](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_execution_policies?view=powershell-5.1),
[Desktop method and scope source](https://github.com/PowerShell/PowerShell/blob/v6.0.0-alpha.18/src/System.Management.Automation/security/SecuritySupport.cs#L230).

The existing fixed worker has a cfg(test)-only source-composition seam and bounded child-phase
parser. Add the one compiled policy assertion/token only for the exact isolated Restricted mode;
generic source uses the same assertion. Match tokens to actual generic/fixed PIDs and real
JSON/SID/CreateNew results. Concurrent generic JSON/read-only COM and fixed filesystem proof
keeps the original thirty-second API, three-second cleanup and forty-five-second helper bounds.
No production operation, public selector or caller-provided script enters the fixed protocol.

Exact 8814d8c MSRV job 111106970873 passes cold Core 88/88 and the mandatory independent EOF
proof in 3.42 seconds. Its next Restricted fixture times out at loader_tests.rs:201, the fixed
thread's policy-observation loop after real SID/CreateNew/ACL assertions, then the failed join
masks the saved generic result. This log does not show a policy, binary-binding or COM failure.
The fixed policy observation currently scans only ChildPhases' sixteen-entry diagnostic ring:
five startup tokens include the policy assertion receipt, followed by five tokens for each of
SID, directory creation and file creation. Twenty tokens necessarily evict that startup receipt.
This source-grounded explanation remains subject to the unchanged native two-child proof.

Keep a separate cfg(test)-only observed-policy bit in the actual owned child's ChildPhases,
latched exclusively by the exact successfully parsed policy-confirmed token. The recent-phase
ring remains bounded to sixteen and a newly owned child begins false. Keep its actual PID/Arc
association and effective enum assertion; no environment text or guessed policy can set it.
An overflow/reset/unknown-token fixture plus the required native Restricted mode must prove
the durable receipt without relaxing the original deadline, policy or child-lifetime gates.

Root exit and PID disappearance do not prove descendant cleanup. Job kill-on-close supplies
emergency containment when the parent closes its kernel handles; an independent observer must
retain actual process handles while the targets are alive and wait for those associated objects.
Select an owned crash-host with real generic/fixed children plus one native heartbeat descendant,
and a separate real generic observer. Kill only the retained crash-host after all three target
handles are bound. Confirm their exits, both stopped heartbeat files and actual helper cleanup
under existing bounds; do not replace the intended crash with a normal parent wait/destructor.
Sources: [Job child membership and kill-on-close](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects),
[parent handle closure and descendant behavior](https://learn.microsoft.com/en-us/windows/win32/procthread/terminating-a-process),
[finite associated-process waits](https://learn.microsoft.com/en-us/dotnet/api/system.diagnostics.process.waitforexit?view=netframework-4.8.1).

The mixed Core test process can legitimately retain a prior filesystem worker's stock guard for
sixty idle seconds. A final share=0 open there cannot identify this fixture's guard release.
Run the two exact proof-driver modes in fresh hosted test processes after the unchanged cold
gate, with no stock/SID call from their orchestrator. The incompatible read-only open must remain
blocked while the observer still owns its guard and succeed only after both owned helpers have
actually exited. A host-only exit cannot release the observer's independent guard. This adds
native evidence rather than warming the cold gate, extending timeouts or mutating system files.

### Post-cold diagnostic adapter alignment (2026-10-03)

PR41 head 1fb5301, run 37081545855, ARM job 111082951981 passed all 82 cold Core and 61 Store
tests. Its later diagnostic's zero-input child timed out at 30,051 ms with actual cleanup
confirmed; independent small and 60k children passed at 23,166 and 22,894 ms. The stock PE was
native 0xAA64, PowerShell 5.1.26100.9457, invoked from native pwsh 7.6.6. This is an actual failing
post-cold diagnostic, not a failed privacy test or evidence for extending the product deadline.
Source: [exact native ARM gate and diagnostic](https://github.com/WhiteKiwi/locron/actions/runs/37081545855/job/111082951981).

The diagnostic's direct and structured child sources invoke unqualified ConvertTo-Json and
ConvertFrom-Json, despite removing inherited PSModulePath. They therefore measure a distinct
command-discovery/bootstrap path from the accepted guarded binary loader. The original elapsed
times include command binding/JSON serialization, so they do not isolate process-start time or
prove autoload to be the sole delay. Preserve those historical observations, and qualify the
actual reviewed Core adapter in the corrective diagnostic rather than re-running this mismatch.
Sources: [legacy diagnostic source](https://github.com/WhiteKiwi/locron/blob/1fb5301b77e3949ac008fa84e747ae84ef1ba4fe/.github/scripts/windows-adapter-diagnostic.ps1),
[accepted Core adapter](https://github.com/WhiteKiwi/locron/blob/1fb5301b77e3949ac008fa84e747ae84ef1ba4fe/crates/locron-core/src/windows.rs),
[retained JSON commands](https://github.com/WhiteKiwi/locron/blob/1fb5301b77e3949ac008fa84e747ae84ef1ba4fe/crates/locron-core/src/windows/stock_json.ps1).

Windows PowerShell 5.1 documents that empty-string ConvertFrom-Json input produces no output.
Select a cfg(test)-only prepared-input clear for the zero-input helper while retaining actual
EOF reading and command-object parsing. Its static fact script must ignore request data and
assert the received text was empty. Native failure of that contract remains a failed proof;
replacing it with {} would conceal the original zero-byte requirement.
Source: [PowerShell 5.1 empty-input contract](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.utility/convertfrom-json?view=powershell-5.1#-inputobject).

Three independent exact helper modes preserve each existing 30/3-second adapter/cleanup limit
and forty-five-second isolated-helper bound without serial cold-start allowance inside a helper.
Require actual guard/Job/pipe ownership, real SID/version/architecture, original UTF-8 sizes and
echo/length assertions, bounded stage/PID receipts and confirmed cleanup. The script remains a
post-cold failing gate on all three native rows; no host policy edit, raw stock child, fallback,
warm-up, test retry or passing cleanup fact from destructor/PID disappearance is accepted.

### Local named pipes with creation-time security

The accepted stock loader at 8674237 validates the Utility binary's retained ConvertFrom-Json
and ConvertTo-Json CmdletInfo objects, implementing assembly/type and loaded location before
passing them to a generic static caller. Its request input already invokes &$locronFromJson.
The existing engine remote-pipe fixture still invokes unqualified ConvertTo-Json in both its
successful-connect and rejected-connect output branches. Select the minimal two-invocation port
to `| & $locronToJson -Compress` within that one static fixture string. Preserve the same output
schema, Connect(200), remote view, actual rejection assertion and owned listener lifetime; no
fallback, connection retry, deadline change or bootstrap warming is introduced.
Source: [accepted guarded stock JSON bootstrap](https://github.com/WhiteKiwi/locron/blob/8674237e1948c311aea1a7ea98d0549255b98240/crates/locron-core/src/windows/stock_json.ps1).

Maintenance control must consume its caller's remaining shutdown budget instead of starting a
new two-hundred-millisecond exchange after endpoint naming or runtime startup. The exact pinned
Tokio 1.53.1 timeout implementation polls the inner future before its timer; an immediately ready
exchange therefore needs explicit expiry checks as well as timeout_at. Check before opening the
pipe, writing the frame and writing the final consumption receipt. A receipt after the deadline
could otherwise dispatch a shutdown after maintenance already reported refusal.

Every successful Windows directory guard already queries and verifies the current SID. Still,
the deadline-aware path should use a separate cached-only SID accessor: OnceLock::get does not
wait for an initializer, and absence must refuse without starting a filesystem worker. This
makes the no-new-adapter boundary explicit even if guard construction later changes. Existing
ordinary wake/control senders retain their behavior.
Sources: [pinned Tokio timeout polling](https://github.com/tokio-rs/tokio/blob/75fef53d0a8590c2d1dbb63672aa7b7d1ef51155/tokio/src/time/timeout.rs#L211),
[nonblocking OnceLock lookup](https://doc.rust-lang.org/std/sync/struct.OnceLock.html#method.get).

Select target-specific `interprocess = "=2.4.4"` with its `tokio` feature and a safe widestring
constructor for SDDL input. It declares Rust 1.75. Its Windows safe
`SecurityDescriptor::deserialize(&U16CStr)` and
`PipeListenerOptions::new().path(name).security_descriptor(Some(sd)).accept_remote(false)`
allow a user+SYSTEM DACL to be supplied at creation, without Tokio's unsafe
`create_with_security_attributes_raw` call in workspace code. Use a byte listener with bounded
duplex consumption acknowledgements, non-inheritable handles and an explicit finite client/read
timeout. The source creates the first instance with `FILE_FLAG_FIRST_PIPE_INSTANCE`, subsequent
instances without that flag, and applies `PIPE_REJECT_REMOTE_CLIENTS` when remote acceptance is
false. Do not set instance_limit to 1: this API allocates a replacement listener instance during
accept, and its documentation says that limit breaks accept.

Derive the stable pipe identity from the verified account SID and the guarded directory file identity,
with a versioned prefix and digest. Two users and two roots must never share a
name. Use an explicit protected descriptor allowing only that SID and SYSTEM. Full pipe rights
to the owner are acceptable for this same-account boundary; do not accidentally grant generic
write to broader principals, since it includes permission to create pipe instances. All payloads
remain untrusted hints with size/version checks. A first-instance collision is a refused wake
endpoint with durable reconciliation fallback, not permission to attach to an existing server.
Client opens must be bounded and must not enable server impersonation unnecessarily. Establish
the daemon's owner lock before binding and retain a server instance while accepting clients so
the name cannot disappear between connections. The separate authenticated lifecycle-control
message described below must not be interpreted from an ordinary wake hint.

Sources: [interprocess manifest](https://github.com/kotauskas/interprocess/blob/2.4.4/Cargo.toml),
[safe descriptor deserialization](https://github.com/kotauskas/interprocess/blob/2.4.4/src/os/windows/security_descriptor/owned.rs),
[listener options](https://github.com/kotauskas/interprocess/blob/2.4.4/src/os/windows/named_pipe/listener/options.rs),
[first-instance, remote rejection and descriptor application](https://github.com/kotauskas/interprocess/blob/2.4.4/src/os/windows/named_pipe/listener/create_instance.rs),
[Microsoft named-pipe ACL/instance rights](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-security-and-access-rights),
[Microsoft pipe creation flags](https://learn.microsoft.com/en-us/windows/win32/api/namedpipeapi/nf-namedpipeapi-createnamedpipew).

### Named-pipe dead-on-arrival acceptance (2026-10-03)

PR43 df4630a/run 37041815226 failed the same malformed-frame fixture on x64 stable and MSRV:
after a raw peer wrote an invalid frame and closed, the next connection reported ERROR_FILE_NOT_FOUND.
The pinned interprocess 2.4.4 Tokio listener returns its stored server's connect error directly.
Its synchronous listener instead clears ERROR_NO_DATA with DisconnectNamedPipe before reusing
that same instance. Microsoft's contract requires this reset when a peer closes before acceptance.
The current fatal accept branch drops the owning endpoint; retrying a client open would conceal
the lost listener rather than repair it.

Use the same safe creation-time-secured synchronous listener in nonblocking accept mode only.
WouldBlock yields to the runtime; its replacement server is created before the accepted handle
is handed out, keeping the name continuously owned. Switch each accepted stream to PIPE_WAIT,
transfer it through the safe OwnedHandle::try_from(PipeStream), then use the safe Tokio
DuplexPipeStream::try_from(OwnedHandle). Payload reads/writes remain overlapped Tokio I/O under
the original 200 ms budget. No synchronous payload I/O or flush is used. Both reset and handle
conversions are present in the package's recorded VCS revision e27f397daebff9054f8e2f7b3dc034bae36b2867.
Verify a peer opened and closed before the first listener poll, continuous first-instance collision
refusal, malformed/shutdown-not-wake framing, subsequent valid wake/control, and abort/await teardown.

Sources: [pinned synchronous listener reset](https://github.com/kotauskas/interprocess/blob/e27f397daebff9054f8e2f7b3dc034bae36b2867/src/os/windows/named_pipe/listener.rs),
[safe Tokio handle conversion](https://github.com/kotauskas/interprocess/blob/e27f397daebff9054f8e2f7b3dc034bae36b2867/src/os/windows/named_pipe/tokio/stream/impl/handle.rs),
[ConnectNamedPipe reset contract](https://learn.microsoft.com/en-us/windows/win32/api/namedpipeapi/nf-namedpipeapi-connectnamedpipe).

#### Accepted-peer conversion must not release listener ownership

The later native malformed-frame failure still reports FILE_NOT_FOUND at the next client open.
Source audit of interprocess 2.4.4 identifies a separate lifecycle bug in the hybrid adapter:
the synchronous accept creates and retains the replacement listening instance before returning
the accepted stream, but the adapter's conversion-error branch breaks its loop and drops that
already-replenished listener. A peer may close between acceptance, switching wait mode, safe
handle transfer, pipe-flag query and Tokio registration; the exact native conversion stage must
remain diagnostic evidence rather than be guessed from the next client's absent endpoint.

Reject and close only the failed accepted handle, log its exact stage/native cause, then yield
under the existing five-millisecond accept policy. Retain the original protected listening
instance and guard until explicit cancellation/abort/await teardown. This admits new independent
peers; it does not retry the failed peer, rebind a name or conceal a true listener accept failure.
Verify an actual peer close precisely between accept/conversion on an owned current-thread
fixture, listener liveness plus collision refusal after every malformed peer, subsequent exact
wake/control delivery and final name/guard release. Existing 200 ms payload and caller budgets
are unchanged.

Sources: [replacement before accepted-handle handoff](https://github.com/kotauskas/interprocess/blob/e27f397daebff9054f8e2f7b3dc034bae36b2867/src/os/windows/named_pipe/listener.rs#L59),
[owned conversion stages and failure handles](https://github.com/kotauskas/interprocess/blob/e27f397daebff9054f8e2f7b3dc034bae36b2867/src/os/windows/named_pipe/tokio/stream/impl/handle.rs#L18).

### Passive missing-state observation (2026-10-03)

A fresh-state source audit found that Windows open_private cleared file creation flags but still
called DirectoryGuard::private for its parent. A role-sidecar or read-only observer could thus
create directories while reporting no file. Require existing-only guards at Windows observer
boundaries and retain explicit creation in writable/owning paths. Tests must inspect absence
without preinitializing the root; existing populated fixtures do not prove this property.

### Measured ARM64 stock PowerShell bootstrap (2026-10-03)

PR41 head 70fe31d run 37031019704, ARM64 job 110917557150, still failed eight cold fixtures after
the output-cap deadlock correction. Its post-gate diagnostics independently measured 22,504,
22,552 and 22,593 ms for three stock PowerShell 5.1 children: no-stdin version/SID, small structured
stdin and 60 KiB structured stdin. All stdout/stderr/EOF processing completed. The stock image
was native ARM64 (PE machine 0xAA64), and the invoking shell was PowerShell 7.6.6. This isolates
slow stock process bootstrap on that hosted image independently of Locron's pipe framing; it
does not establish the underlying Windows/.NET bootstrap cause or a universal ARM64 latency.

Current SID discovery, every missing directory component and every new managed file each start
a separate stock child. Two thirty-second worker slots therefore cannot keep concurrent callers
inside their original queue/startup budgets when each start consumes about 22.5 seconds. The
five-second script-entry and ten-second output-phase assumptions also expire before the script
can execute. Raising production deadlines or skipping ACL tests would not address this cost.

Select a fixed persistent filesystem dispatch worker for SID and the already reviewed atomic
.NET security constructors, with bounded framed JSON, original per-request deadlines, finite
queue/idle lifetime and no arbitrary source/data execution. A suspended pre-enrolled kill-on-close
Job is required because a static cached child is not dropped on parent process exit. Keep one
filesystem and one generic worker slot; confirm worker cleanup and refuse uncertain mutation
outcomes without replay. Native proof remains required for cold first use, concurrent callers,
bad frames, output caps, idle restart and actual abrupt-parent-exit child termination. Task
Scheduler cannot assume repeated fresh 22.5-second COM launches fit a thirty-second quiesce;
its separate fixed-worker design is still pending. Preserve phase markers in real-stock timeout/
cap fixtures under the original thirty-second maximum and measure prompt cleanup after entry.

Evidence: [native ARM64 job and post-gate diagnostics](https://github.com/WhiteKiwi/locron/actions/runs/37031019704/job/110917557150).

The selected one-generic-child limit makes unrelated test-harness timeout/cap cases compete for
the same intentionally occupied thirty-second slot. Serialize only native core-library harness
cases while preserving explicit concurrent-caller and saturated-queue fixtures. Isolate fixed
worker protocol faults and parent-crash cases in exact test subprocesses; failure is expected to
invalidate queued work in that process and must not contaminate unrelated state fixtures.
Other library harnesses retain normal parallelism. This is test scheduling for deliberate owned
failures, not a cold-start workaround or a larger production deadline.

The implementation audit selects Windows-only `file-id =0.2.3` (MIT OR Apache-2.0, declared
Rust 1.77). Its exact registry source exposes safe `get_high_res_file_id(path)`, opens a
backup-semantics access-zero file, then queries `GetFileInformationByHandleEx(FileIdInfo)`.
It returns the full `FileId::HighRes { volume_serial_number: u64, file_id: u128 }`, converting
the 128-bit identifier from little-endian bytes. It has no public borrowed-handle query, so use
its path API only while the complete no-delete-sharing, trusted-mutation-checked DirectoryGuard
remains live. The directory-write sharing correction below supersedes the earlier no-write claim.
Unsupported queries refuse IPC/registration; do not call the automatically falling-back API.

`same-file =1.0.6` was rejected: its Windows key exposes a low 64-bit file index through equality/
Hash, and its source notes ReFS's 128-bit IDs can collide after truncation. Path uppercasing and
unspecified Rust Hash encodings also cannot provide stable identity. Encode the fixed
`locron-instance/v1\0` domain, SID byte length (LE32)/UTF-8, volume (LE64), and file ID (LE128)
explicitly before SHA-256. The retained guard prevents file-ID reuse during listener ownership;
role/control UUID framing separates live instances without hashing path spelling.

Sources: [file-id 0.2.3 published source](https://docs.rs/crate/file-id/0.2.3/source/src/lib.rs),
[published manifest and MSRV](https://docs.rs/crate/file-id/0.2.3/source/Cargo.toml),
[same-file Windows key limitation](https://github.com/BurntSushi/same-file/blob/1.0.6/src/win.rs),
[Microsoft full file identity](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_id_info).

### Directory child admission and retained-object privacy (2026-10-03)

Exact head 18ed2e23, native run 37098728418, fails the same five Core fixtures on x64 stable,
x64 MSRV and ARM64: three hard-link/rename admissions return Windows 32 after their leaf handles
are closed, the persistent-reader fixture still cannot rename after releasing its reader, and
the caller-budget rename expires after its fifty-ms reader release. The new empty-directory
rename/replacement and list-denied ancestry fixtures pass. This separates legitimate child-name
mutation from the retained directory object's own rename/delete boundary; it is not a reason to
drop guards, loosen leaf policy or extend a deadline. Microsoft's FILE_RENAME_INFORMATION explains
that IopOpenLinkOrRenameTarget opens the target directory with FILE_WRITE_DATA | SYNCHRONIZE.
CreateFile's no-write sharing excludes that parent open, whereas omitting FILE_SHARE_DELETE
excludes delete/rename access to the retained directory object. FILE_LIST_DIRECTORY remains the
data-access bit that made the directory's sharing checks effective in the previous qualification.

Select directory access 0x20081 with FILE_SHARE_READ | FILE_SHARE_WRITE, without delete sharing.
Sharing permits an otherwise authorized open; it never grants ACL rights. The earlier claim that
no-write sharing protects foreign-mutable ancestors is no longer valid. Common ancestry must now
apply the existing Stock/passive allow-ACE mutation rule as well as trusted-owner verification:
current SID, SYSTEM, Administrators and TrustedInstaller may mutate; nontrusted actual-object
generic-write/all, delete-child/delete, write-EA/attributes, WRITE_DAC or WRITE_OWNER grants refuse.
Keep deny/inherit-only handling and the existing separate sibling-creation rights distinction;
unknown ACE forms/null DACL refuse. Private roots/leaves still require their strict SID/SYSTEM
descriptor, and stock leaf data remains write/delete-excluding. A directory handle with write
sharing cannot claim to prevent all in-place writes by the trusted owner; SECURITY's account/root
boundary is unchanged. Existing reparse points refuse, and no-delete object retention continues
to block directory replacement. The proposed combination requires actual native proof.

Sources: [CreateFile sharing and lifetime](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew),
[rename target-directory access](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/ns-ntifs-_file_rename_information),
[directory access rights](https://learn.microsoft.com/en-us/windows/win32/fileio/file-access-rights-constants),
[per-file hard-link sharing](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createhardlinkw).

The next worker run, PR41 head 762bed7/run 37038880251, passed x64 stable and MSRV core/store.
ARM64 passed 66 core fixtures, including isolated simultaneous cold callers, but its first
private-file fixture timed out at filesystem CreateNew. Post-gate stock PowerShell 5.1.26100.9457
probes measured 513/469/456 ms for bootstrap/small/60k structured input, much faster than the
earlier image's 22.5-second measurements. Those later probes do not locate the actual first cold
worker's delay. Select test-only phase/elapsed/operation/request-ID/owned-PID breadcrumbs on its
real dispatch path, without path/SID/payload values, warm-up, new deadlines or mutation replay.
Do not call the failed cold acceptance transiently successful from later bootstrap performance.
Evidence: [exact native ARM64 worker run](https://github.com/WhiteKiwi/locron/actions/runs/37038880251/job/110943792703).

The real-request diagnostics in PR41 head a6f0189/run 37041311097 identify the first stalled
operation as SID discovery, rather than the later CreateNew call that reports its failure.
ARM64 child PID 8688 spawned at 48 ms and input was flushed at 49 ms; no reply arrived by
30,004 ms, and owned root-plus-Job cleanup was confirmed at 30,052 ms. This rules out caller
queue and process creation latency in that request, while leaving the fixed PowerShell host,
source initialization and request body unlocated. Inspection of filesystem_worker.ps1 confirms
there is no Add-Type or dynamic C# compilation. Its first stages are Console encoding setup,
ReadLine, ConvertFrom-Json, WindowsIdentity.User and JSON reply serialization. Select bounded
test-only fixed phase tokens on stderr to locate the gap before replacing any of those stages.
Do not infer a production fix, warm the original gate, or replay a timed-out mutation from the
later successful fixtures or stock bootstrap probe.
Evidence: [first SID exchange and confirmed cleanup](https://github.com/WhiteKiwi/locron/actions/runs/37041311097/job/110951832994).

PR41 head cebdfe4/run 37044222227 narrows the real first SID gap further. Its child spawned at
49 ms and input flushed at 50 ms; fixed source entry arrived at 3,097 ms, encoding at 3,114 ms,
and the actual input line at 3,123 ms. No JSON-parse completion arrived before the thirty-second
timeout, and owned cleanup completed at 30,041 ms. The source's first unqualified ConvertFrom-Json
therefore contains the remaining gap, including command discovery/import and converter activation.
This does not yet prove which component is slow.
Evidence: [actual first JSON-stage timeout](https://github.com/WhiteKiwi/locron/actions/runs/37044222227/job/110961511069).

Windows PowerShell 5.1 documents automatic import on first command use, its session-local None
preference, explicit module-file imports and cmdlet/function/alias import restrictions. Qualified
command names select the intended module. Select an absolute PSHOME stock Utility manifest import
with only its two JSON cmdlets, disable autoload in the child and qualify those two calls; the
converter implementation and JSON parameters remain unchanged. Static before/after-import tokens
will distinguish module initialization from the remaining converter gap without extra budget.
Read-only inspection of this host's stock manifest confirms the JSON exports and stock nested
binary/script modules; it is not native execution or ARM64 performance evidence. The inference
that avoiding discovery helps the cold failure remains a hypothesis until the original CI gate.
Sources: [5.1 module-autoload preference](https://raw.githubusercontent.com/MicrosoftDocs/PowerShell-Docs/main/reference/5.1/Microsoft.PowerShell.Core/About/about_Preference_Variables.md#psmoduleautoloadingpreference),
[5.1 explicit/restricted imports](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/import-module?view=powershell-5.1),
[5.1 command qualification](https://raw.githubusercontent.com/MicrosoftDocs/PowerShell-Docs/main/reference/5.1/Microsoft.PowerShell.Core/About/about_Command_Precedence.md#use-module-qualified-names).

PR41 f476fc3/run 37049957470 passes 69 of 70 native ARM64 core fixtures. The remaining broad-file
test reaches its generic run_script_json ACL setup after private SID/directory/CreateNew succeeded,
then fails at that separate Get-Acl/Set-Acl exchange's original thirty-second timeout. The fixed
worker's cold concurrent and ordered phase fixtures pass. Post-gate stock probes still take
23,240/22,660/22,738 ms, so neither that later success nor first-SID evidence identifies the
generic child's stalled JSON or security-module stage.
Evidence: [actual broad-file setup failure and post-gate probes](https://github.com/WhiteKiwi/locron/actions/runs/37049957470/job/110980584946).

The broad-file/root fixtures only need to add an Everyone Read ACE to a disposable owned object.
Use .NET Framework File/Directory GetAccessControl and SetAccessControl for that deliberate setup,
avoiding the unrelated PowerShell Security module's Get-Acl/Set-Acl discovery. This retains an
independent platform ACL operation and the same real descriptor, repair/refusal and original-byte
assertions; generic JSON conversion, child budgets and production ACL adapters remain separate.
This choice does not claim the generic adapter's cold startup or restrictive-policy behavior is
resolved. Native acceptance and generic lifecycle tests remain required.
Sources: [File ACL read](https://learn.microsoft.com/en-us/dotnet/api/system.io.file.getaccesscontrol?view=netframework-4.8.1),
[File ACL write](https://learn.microsoft.com/en-us/dotnet/api/system.io.file.setaccesscontrol?view=netframework-4.8.1),
[Directory ACL read](https://learn.microsoft.com/en-us/dotnet/api/system.io.directory.getaccesscontrol?view=netframework-4.8.1),
[Directory ACL write](https://learn.microsoft.com/en-us/dotnet/api/system.io.directory.setaccesscontrol?view=netframework-4.8.1).

### Generic adapter failure visibility (2026-10-03)

The exact PR41 c919530/run 37053606277 ARM64 core gate still passes 69/70 after the two
compiled .NET ACL-fixture setup changes. That result does not establish a Security-module cause
or resolve the separate generic adapter. Fixed-worker cold SID and concurrent first-use gates
pass on this revision. The generic adapter still reads EOF and calls unqualified ConvertFrom-Json;
its caller scripts generally call unqualified ConvertTo-Json, unlike the explicitly bound fixed
filesystem dispatcher. Stock startup, JSON command discovery/binding and the caller operation
remain separate possible delays until this actual adapter reports fixed child phases.

PR43 cb5074a/run 37055608817 has a different mapping fixture failure on all three native rows
(90/91 core tests pass). The x64 job 110999439152 reports NotFound at filesystem.rs:998. Direct
line-number inspection of this exact commit places that assertion at marker.unwrap(); the release
write is line 996, but its independent result and temporary ancestry must also be retained. The
fixture currently asserts its marker before inspecting its joined helper result, so the hosted
log does not disclose whether setup failed, timed out, or never created the marker. Do not label
this a generic timeout or a mapped-file protection failure from that assertion alone.

PowerShell 5.1 documents explicit module-path imports, restricted imported member sets and
module-qualified command lookup. The existing Utility manifest includes a nested .psm1 as well
as its binary module; Cmdlet/Function filters do not prove that Restricted can load the manifest.
Audit a generic converter binding separately from its measured phase cause, keep all existing
static caller commands working, and require hosted process-only stricter-policy/forged-module
fixtures before claiming that the native installation path works under Restricted. No production
policy change, bypass, warm-up or local rejected rehearsal is permitted.

Sources: [PR41 ARM64 failure](https://github.com/WhiteKiwi/locron/actions/runs/37053606277/job/110992744736),
[PR43 x64 mapping fixture](https://github.com/WhiteKiwi/locron/actions/runs/37055608817/job/110999439152),
[PowerShell 5.1 Import-Module](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/import-module?view=powershell-5.1),
[PowerShell execution policies](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_execution_policies?view=powershell-5.1).

The portable test-only TLS fixture reuses locked tokio-rustls 0.26.4 (MIT OR Apache-2.0,
Rust 1.71) and rustls 0.23.45. Audited published source provides safe TlsAcceptor::from/accept and
ServerConfig::builder_with_provider/with_single_cert; an explicit AWS-LC provider reuses the
existing reqwest graph and avoids platform OpenSSL executable assumptions. Embedded certificate
and PKCS#8 key bytes belong only to tests and are never added to an operating-system trust store.
Source: [tokio-rustls published manifest](https://docs.rs/crate/tokio-rustls/0.26.4/source/Cargo.toml),
[TLS acceptor API](https://docs.rs/tokio-rustls/0.26.4/tokio_rustls/struct.TlsAcceptor.html).

### Windows maintenance directory flush boundary (2026-10-03)

The existing CLI maintenance helper opens a directory with read-only File::open before sync_all.
Rust 1.94's Windows File::fsync calls FlushFileBuffers, whose supported file handle requires
GENERIC_WRITE. Microsoft also requires FILE_FLAG_BACKUP_SEMANTICS to open a directory; ordinary
File::open does not supply that directory contract. A privileged volume flush is unsuitable for
the standard-user product. Keep Unix directory fsync in its backend and use the already selected
Windows repaired-file sync plus guarded rename/deletion contract. Successful native filesystem
operations and SQLite completion are testable; they do not prove a hardware-power-loss directory
metadata flush. Maintenance must not report a no-op Windows sync as such proof.

Sources: [Rust 1.94 Windows filesystem source](https://github.com/rust-lang/rust/blob/1.94.0/library/std/src/sys/fs/windows.rs),
[CreateFile directory handle requirements](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew),
[FlushFileBuffers access and volume-flush requirements](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-flushfilebuffers).

The old orphan fixtures used i64::MAX/2 epoch microseconds to age files. Rust 1.94's Windows
SystemTime adds durations as checked signed 100-nanosecond intervals; that fixture timestamp
cannot be represented. Use the actual fixture clock plus two hours to cross the one-hour grace
period on every supported backend. Keep production cutoff overflow refusal unchanged.

Source: [Rust 1.94 Windows SystemTime checked conversion](https://github.com/rust-lang/rust/blob/1.94.0/library/std/src/sys/pal/windows/time.rs).

### Existing-only read-only SQLite boundary (2026-10-03)

SQLite documents that a read-only WAL connection is possible when both sidecars already exist,
when its directory permits their creation, or when the database is immutable. Therefore merely
omitting SQLITE_OPEN_CREATE does not establish a no-sidecar-creation observer. WAL can contain
committed data absent from the main file, and the last connection normally checkpoints and
removes WAL/SHM. Immutable connections skip locking/change detection and can return incorrect
results if their file changes. A retained write-excluding main-file gate plus confirmed absence
of every journal is needed before choosing immutable; live reads must retain validated existing
sidecars and continue normal WAL locking. These are implementation inferences from the documented
behavior, with native concurrent close/write evidence still required.
Sources: [SQLite read-only WAL and lifecycle](https://sqlite.org/wal.html#read_only_databases),
[immutable URI behavior](https://sqlite.org/uri.html).

The exact cached libsqlite3-sys 0.38.2 bundle is SQLite 3.53.2. Inspection of its sqlite3.c finds
sqlite3ParseUri decoding percent escapes into filename bytes before VFS admission; escaped query
characters remain filename bytes, while an escaped NUL can terminate the pathname. winOpen uses
GENERIC_READ for read-only files and FILE_SHARE_READ|FILE_SHARE_WRITE; win32-longpath is registered
with the wide Windows open and longer pathname bound. Native winFullPathname keeps the verbatim
prefix outside its separate Cygwin branch. Preserve the exact guarded UTF-8 filename by encoding
every byte in the internal file: URI, reject NUL/non-UTF-8, and select that bundled VFS. A general
URL conversion that drops the verbatim prefix does not prove the greater-than-260-character path.
Sources: cached libsqlite3-sys 0.38.2 sqlite3/sqlite3.c, sqlite3ParseUri/winOpen/winFullPathname/
winLongPathVfs; [SQLite URI options and UTF-8 decoding](https://sqlite.org/uri.html).

CreateFile sharing rules allow a retained read handle to exclude subsequent write/delete opens;
an incompatible existing writer or mapping must refuse it. SQLite's normal WAL close path ignores
the unlink return after checkpoint, so retained no-delete sidecar handles may leave existing
files for a later writable cleanup. This does not prove native durability by inspection: test
actual commit visibility, final writer close, sidecar identities, and subsequent cleanup before
claiming a passive read-only path. No missing sidecar is automatically repaired for a reader.
Sources: [CreateFileW sharing rules](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew),
cached bundled sqlite3.c sqlite3WalClose/winShmUnmap.

### Maintenance journal path bound (2026-10-03)

Maintenance needs a finite bound for a future WinGet Complete executable path before Prepare can
reserve its entire private journal. Select 4,096 UTF-16 units per normalized recorded local path.
Validate normal components against Windows' forbidden filename characters and control range,
while retaining structural drive/verbatim prefixes and separators. Valid UTF-8 scalars consume
at most three JSON content bytes per UTF-16 unit; separators escape to two, supplementary scalars
use four bytes for two units, and forbidden quotes/controls cannot require six-byte escapes.
The conservative future path bound is therefore 12,290 bytes including JSON quotes. This is a
selected maintenance limit, not a Windows filesystem limit or a runtime-state path restriction.
Use the actual complete snapshot plus bounded future restore/forced-stop fields to preflight
record size and all repeated forward/rollback callbacks before the first task disable. A 256-role
inventory ceiling alone cannot prove a 128 KiB record/128-frame/16 MiB journal will fit.
Source: [Windows filename character rules](https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file#naming-conventions).

### Guarded control deadline polling (2026-10-03)

The locked Tokio 1.53.1 timeout future polls its inner future before checking the timer. A
one-time clock check before awaiting a receipt therefore cannot prevent a previously Pending
write from being polled when it becomes ready after the deadline. Select an expiry gate on each
actual read/write future poll, plus a post-Ready check and the same absolute timeout_at. A native
deterministic fixture must make the receipt Pending, advance past its deadline and make it ready,
then prove no later receipt poll or new write occurs. A second fixture crosses expiry while
consuming an already-ready acknowledgement and must never initiate the receipt.

These gates prevent new client I/O initiation after expiry; they cannot retract a write already
queued to the operating system. Timeout therefore leaves delivery uncertain and does not prove
the server did not dispatch shutdown. Exact-lifetime cancellation and actual lock/process exit
remain necessary; neither timeout nor acknowledgement authorizes replay or replacement.

Source: [locked Tokio timeout polling order](https://github.com/tokio-rs/tokio/blob/75fef53d0a8590c2d1dbb63672aa7b7d1ef51155/tokio/src/time/timeout.rs#L211).

### Registered supervisor deadline and retry ownership refinement (2026-10-03)

Review of the initial supervisor source found a real deadline gap: repeated private native file
observations, transition writes, sync_all and the independent five-second rename-sharing retry
were synchronous outside the supplied shutdown budget. Cached verified SID and retained existing
root guards mean these paths normally do not start another filesystem PowerShell query, but that
fact cannot bound a native file operation. A timeout around an uncancellable blocking operation
also cannot retract an already initiated write or prove thread/process/lock exit.

Select one runtime-owned native lifecycle/I/O worker with a finite request slot and retained state,
executable, activation and child/Job ownership. The driver waits only for the caller's remaining
absolute deadline; the worker checks the same deadline before each new operation and after replies.
On timeout, close admission and quarantine the worker/ownership, without joining an unfinished
thread or starting another cleanup/fact-write budget. Late I/O completion remains uncertain. An
error cannot authorize replacement, another child start or a claim of graceful exit. This design
uses safe owned Rust handles and channels rather than treating a synchronous syscall as cancellable.

RestartCount is the number of Scheduler task restarts, not a typed Locron child-outcome policy.
Keeping Count=3 around a native four-start supervisor can restart that entire loop. The observed
positive role exit which did not restart does not prove all exit codes behave that way. Select
one owner: disable Scheduler RestartOnFailure (read back Count=0 and no restart interval) and let
the retained native supervisor own the four-start budget. Preserve genuine final child codes,
including negative Windows statuses and genuine child 70, in both exit facts and launcher
propagation; infrastructure 70 remains a different typed private cause. A known NotStarted spawn
may retry within the same total four-start budget; execution/cleanup uncertainty never may.
An action which cannot launch the native owner fails visibly and does not create an independent
SDK retry budget. The next logon or explicit activation is a new lifetime.

Sources: [Scheduler restart count](https://learn.microsoft.com/en-us/windows/win32/taskschd/tasksettings-restartcount),
[Scheduler restart interval](https://learn.microsoft.com/en-us/windows/win32/taskschd/tasksettings-restartinterval),
[Rust bounded channel receive](https://doc.rust-lang.org/1.94.0/std/sync/mpsc/struct.Receiver.html#method.recv_timeout),
[Rust nonblocking join observation](https://doc.rust-lang.org/1.94.0/std/thread/struct.JoinHandle.html#method.is_finished).

The inspected OwnedChild factory constructs its independent Job before native command spawn and
maps Job-creation failures to io::Error::other. Therefore NotStarted alone is not a launch-retry
classification. Development selects only typed NotStarted with NotFound, Interrupted or WouldBlock
after all retained program/root/privacy/log prechecks; PermissionDenied, InvalidInput, Other and
all Job/control/diagnostic failures remain refusal. The four-start limit combines known-not-started
and actual completed attempts. Final known-not-started exhaustion has its own bounded private facts
and infrastructure 70, never a fabricated completed child code.
Sources: [reviewed independent Job creation](https://github.com/ohadravid/win32job-rs/blob/17080e2a29ea244bb4e58400dc27739075b2fbc8/src/job.rs),
[Rust 1.94 I/O error kinds](https://doc.rust-lang.org/1.94.0/std/io/enum.ErrorKind.html).

#### Completed supervisor role sidecar handoff

A retained authenticated role-sidecar handle denies replacement. After the old owned root has
exited, a new manual daemon/dashboard can acquire the shared permanent role lock while its new
sidecar publication waits on that retained handle. Reading the old retained PID/UUID around a
now-held lock cannot identify the new holder as the completed child. Microsoft documents that
process termination closes that process's handles; root termination alone leaves descendants,
so the independently retained Job must also report empty before releasing these old role guards.

After that combined proof, release the retained shared-role sidecar before any later fact/log
I/O. Do not probe or authenticate the shared role as the completed old child, and do not wait,
signal or terminate a new manual owner. Preserve the separate actual owned worker-activation
lease exit check, activation ownership and listener teardown. This is an ownership refinement,
not permission to release while a root/descendant or Job query remains unconfirmed.
Sources: [process termination resources](https://learn.microsoft.com/en-us/windows/win32/procthread/terminating-a-process),
[ExitProcess handle closure and descendant behavior](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-exitprocess).

#### Native supervisor harness selection

The existing native library command tests core/store/engine/server; the targeted CLI commands
select other areas and do not match service::windows_supervisor. Review of that module's fourteen
semantic fixtures therefore cannot be qualified by a native package build or those filters.
Select the complete service:: CLI unit namespace in the existing x64/ARM64/MSRV foundation
matrix with --test-threads=1, after the original cold core gate. This preserves independent
private fixtures while making the two real three-by-sixty-second exhaustion policies execute
serially. Keep all existing operation deadlines, child/Job proof, fault assertions and required
status names; record the actual test count and results for the exact revision.

At native f8b7854/868c537, core and engine passes led to two server-library failures, so the
following service semantic step was skipped by the default success condition. Those runs do
not qualify the supervisor. GitHub documents that an explicit status function overrides the
default success gate, and step.outcome reports the original result. Retain the original cold
core command with id native_core and admit the service step only with
`!cancelled() && steps.native_core.outcome == 'success'`. This obtains independent owned-fixture
evidence after unrelated library failure without accepting the failed job, admitting after a
failed cold gate or running after cancellation. Keep all source assertions and budgets.
Sources: [GitHub status-check conditions](https://docs.github.com/en/actions/reference/workflows-and-actions/expressions#status-check-functions),
[original step outcome](https://docs.github.com/en/actions/reference/workflows-and-actions/contexts#steps-context).

#### Hash-bound bundled license checkout

Native revision f8b785465d2d2b2b419d3b576529552d76b77f66 failed the unchanged official-font
license assertion with SHA-256 1dc6255c3f36ba15736d427a9ee527608a297797255369926ca6952ecc7a781c
instead of 2b2da563e79400b61818402ca9f26a73d52468268b7fc715e92143c1e799737e.
Both embedded and frontend-dist OFL.txt paths have committed blob
0ba7f7cae3f9a6bda533889c836687097e956443: 4384 bytes, 93 LF and no CRLF. The Windows
checkout with core.autocrlf=true produces 4477 bytes; its exact CRLF projection has the failed
digest. This proves a checkout conversion, without changing the frozen upstream license pin.

Git documents that unspecified line endings follow core.autocrlf/platform behavior, while
`text eol=lf` preserves index LF on checkout. Select those attributes for the two exact OFL.txt
paths used by embedded assets and frontend output. Leave font/license source blobs, the expected
digest and other repository paths unchanged; verify attribute resolution and byte identity under
Windows checkout conversion before repeating the original native provenance assertion.
Source: [Git text and eol checkout attributes](https://git-scm.com/docs/gitattributes#_checking_out_and_checking_in).

### Task Scheduler and cooperative lifecycle

#### Windows role-lock diagnostic refinement during development

Rust 1.94's Windows `File::try_lock` calls `LockFileEx` at offset zero with both length words set
to `u32::MAX`, exclusive and fail-immediately flags. This covers diagnostic bytes in the permanent
lock file. Microsoft's remarks explain that another process, and even another handle opened by
the owner, cannot read the exclusively locked region. Unix advisory-lock observer behavior cannot
therefore be carried over by simply reading the same lock file on Windows.

Keep the permanent lock as the sole ownership proof. Publish diagnostic `LockMetadata` plus an
explicit registered-service marker in a private atomic sidecar while ownership is held. A bounded
no-follow observer reads that sidecar, and a separately secured endpoint validates the exact live
role/lifetime before accepting shutdown. Stale sidecar contents never authorize PID signalling
or prove liveness; service/update completion still waits for actual corresponding role-lock exit.
Native tests must read the observer metadata while the lock remains held, refuse a second owner,
and confirm OS lock release after orderly exit/crash.

Sources: [pinned Rust 1.94 Windows lock implementation](https://github.com/rust-lang/rust/blob/1.94.0/library/std/src/sys/fs/windows.rs#L431),
[LockFileEx mandatory range semantics](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-lockfileex).

### Measured generic JSON binding and binary-only candidate (2026-10-03)

At loader head 8674237, all three native Core rows fail before child spawn with Win32 123
(invalid filename), including direct stock-guard acquisition. Source inspection finds fixed
multi-component joins containing forward slashes followed by a literal verbatim prefix. Microsoft
documents that the verbatim prefix disables path-string parsing; Rust documents that forward slash
is not a separator in a verbatim prefix. This is a source-grounded cause candidate, pending native
proof. Rebuild validated local drive/root/normal components with native separators before verbatim
opens, including an ordinary SystemRoot spelling that uses forward slashes. Reject an embedded
separator in an already-verbatim component rather than silently reinterpret it. No trust, no-follow,
sharing, full-identity, deadline or owned-worker boundary changes. Sources:
[Win32 file namespaces](https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file),
[Rust Windows prefixes](https://doc.rust-lang.org/std/path/enum.Prefix.html).

At corrected path head d615b120, native stock guard/identity probes pass, but the actual generic
and fixed filesystem children fail during Assembly.LoadFrom before JSON binding. The error names
file:// followed by the verbatim local path as a missing assembly. Framework's reference source
sets LoadFrom's argument as CodeBase, then VerifyCodeBase treats any leading two backslashes as a
file URI rather than a local DOS path. Prefix removal after LoadFrom is too late. This isolates
the next loader boundary; native correction remains required and does not establish policy proof.
Sources: [Framework assembly binding](https://raw.githubusercontent.com/microsoft/referencesource/main/mscorlib/system/reflection/assembly.cs),
[Assembly.LoadFrom](https://learn.microsoft.com/en-us/dotnet/api/system.reflection.assembly.loadfrom?view=netframework-4.8.1).

The existing current_user_sid entry captures a fresh thirty-second deadline. Reusing it from an
already bounded mapped-helper qualification can extend that caller's operation after native
bootstrap work has consumed part of its budget. Select an absolute-deadline SID entry sharing
the same verified cache, finite initializer and fixed filesystem owner, with expiry checks around
both cache and query results. Do not prewarm the cache or treat returned SID text as an executable
or process-ownership proof. The existing three-second owned-cleanup allowance remains explicit;
native cold completion under the forwarded caller deadline still requires hosted qualification.

Keep verbatim native stock guard paths and handles. Derive a separate canonical absolute DOS
library path only after those handles are retained, refuse components that ordinary Windows path
normalization could reinterpret (including trailing dots/spaces, devices and alternate streams),
and require the converted path's full high-resolution volume/file ID to equal the retained native
identity. Run this query inside the same admitted finite owner with pre/post deadline checks;
stalled or late results retain the original quarantine boundary. Only the verified library path
reaches Framework LoadFrom. No UNC/URI search, manifest import, policy change or trust fallback is
selected. The stock executable and all managed state/install long-path behavior remain unchanged.

The newer server cooperative-shutdown fixture reports native error 80 (AlreadyExists) on its
final writable Store::open. Its call sequence includes state directory ensure/guard, private
database/WAL/SHM precreation, SQLite open/configuration/migration and final leaf validation; the
existing panic does not identify the exact stage. API Store work may already be admitted to a
blocking worker when SSE cancellation drops its async poll, so a sidecar lifecycle race is only
a hypothesis. Select fixed debug-only failure-stage/counter/code diagnostics preserving the
original error and all refusal behavior, with no new path reads or payload rendering, before
assigning or correcting a cause. Hosted native evidence is required; no local behavioral policy
rehearsal or AlreadyExists adoption follows from this observation.

The post-namespace native server gate now reaches this diagnostic: operation 11 fails exactly
at wal-create-new with Windows error 80. The same fixture admits concurrent per-request writable
Store opens; NotFound followed by CreateNew therefore has a legitimate winner race, but this
result alone does not prove that the winner's object is private or unchanged. Reuse the strict
existing-leaf policy after only an explicit AlreadyExists result, and retain the verified DB/WAL/SHM
handles through SQLite startup rather than dropping each precreation handle immediately. Do not
repair, truncate, accept a reparse point or retry creation after any other failure.

The pinned bundled Windows SQLite VFS uses FILE_SHARE_READ | FILE_SHARE_WRITE, without delete
sharing, for ordinary database/WAL and shared-memory file handles. Use these actual native handles
after configuration acquires the sidecars; keep the private Rust preparation guards until that
handoff and exact reported database identity validation finish. SQLITE_OPEN_READWRITE without
CREATE requires an existing file, so Rust's guarded explicit creation remains the only database
creation authority. This prevents an unguarded SQLite database-create fallback; read-only mode
fallback must also refuse. Other final-open failures remain distinct until native evidence resolves
them. Sources: [SQLite open flags](https://www.sqlite.org/c3ref/open.html),
[Windows VFS source](https://sqlite.org/src/file?name=src/os_win.c).

Exact runtime head 823bd240 fails concurrent first writable opens at sqlite-migrate with
MigrationConflict on all three native rows. The MSRV row additionally fails a raced final close
at shm-open with PermissionDenied/raw Windows 5; the same race passed on the other two rows.
The migration code reads a version before BEGIN IMMEDIATE, then treats a later checked version
as a conflict. SQLite serializes writers: an already completed, strictly verified supported step
can be accepted under the acquired write transaction without replaying SQL or rewriting history.
Move that authoritative re-read/verification into each step's write ownership. This source-grounded
cause is distinct from the SHM failure; neither passing rows nor CreateNew winner handling proves
all Store concurrency is fixed. [SQLite transaction contract](https://sqlite.org/lang_transaction.html).

The pinned libsqlite3-sys 0.38.2 bundle is SQLite 3.53.2, source ID d6e03d8c777cfa2d35e3b60d8ec3e0187f3e9f99d8e2ee9cac695fd6fcdf1a24.
Its sqlite3WalClose implementation first obtains an exclusive database lock before
checkpoint/SHM/WAL deletion and explicitly keeps it until returning. Its Windows SHM purge unmaps
regions and closes shared handles before calling winDelete. Win32 documents access-denied opens
for files marked for deletion, but raw error 5 alone cannot prove that condition or justify retry.
Select a temporary shared database-handle admission gate before any sidecar lookup, rather than
classifying or blindly retrying that error. Rust 1.94's safe try_lock_shared uses immediate
LockFileEx over offset zero and two u32::MAX length words; it overlaps the VFS lock bytes.
Shared locks permit reads and deny ordinary writes, including by their owner; mapped I/O bypasses
that byte-range rule, so the proof relies on SQLite's actual exclusive cleanup protocol, not a
general claim that all writes are impossible. Release the gate before SQLite configuration/write.
Native held-cleanup/queue/handoff/final-removal proof remains pending; permission/identity failures
still refuse immediately. Sources:
[pinned bundled SQLite source](https://docs.rs/crate/libsqlite3-sys/0.38.2/source/sqlite3/sqlite3.c),
[Rust 1.94 lock implementation](https://github.com/rust-lang/rust/blob/1.94.0/library/std/src/sys/fs/windows.rs#L451),
[LockFileEx rules](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-lockfileex),
[Win32 deletion contract](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-deletefilew).

Exact runtime head 7c4e136 passes all 118 Core fixtures on all three native rows, including the
retained-directory and child-mutation regressions. Its simultaneous first writable Store-open
fixture still fails at sqlite-configure with SQLITE_BUSY/extended 5 on ARM64 operation 14 and
x64 MSRV operation 15; x64 stable passes. The native shared admission/final-close and migration
catch-up fixtures pass. The existing configuration diagnostic covers five PRAGMAs in one batch,
so the failed individual PRAGMA is not yet measured. Keep that uncertainty explicit.

Pinned SQLite 3.53.2 OP_JournalMode changes into WAL through sqlite3BtreeSetVersion, which first
opens a read transaction and can then request an exclusive write transaction to change the
database header. btreeBeginTrans deliberately omits the busy callback on a contended read-to-write
promotion; its comment describes the competing reader/reserved-lock deadlock. This supplies a
source-grounded explanation for an immediate busy result despite the configured five-second
busy timeout, rather than evidence that a longer timeout or whole-Store retry is required.
SQLite's public busy-handler contract makes the same exception. Its journal-mode contract returns
the effective mode, including the old mode when a change cannot be made; acceptance must therefore
verify WAL, not merely successful statement preparation or a discarded result row. Sources:
[busy-handler contract](https://sqlite.org/c3ref/busy_handler.html),
[journal-mode result](https://sqlite.org/pragma.html#pragma_journal_mode),
[pinned JournalMode/Btree implementation](https://docs.rs/crate/libsqlite3-sys/0.38.2/source/sqlite3/sqlite3.c).

The pinned rusqlite 0.40.2 execute_batch consumes only the first step and drops each statement;
it neither verifies that WAL was returned nor exposes that statement's finalization error.
Use a separately owned fixed WAL statement, consume its sole row through completion and explicitly
finalize it before considering only exact SQLITE_BUSY/extended 5 for another attempt. SQLite's
autocommit VDBE halt path commits or rolls back and releases transaction locks; finalizing a
failed statement is necessary before yielding to the competing initializer. A successful row
alone is insufficient because completion/reset/finalize can still report an error. Any other
SQLite error, unexpected mode or explicit transaction remains an immediate refusal. The
five-second entry deadline must cover handler waits, every attempted WAL statement and sleeps;
it must not restart on each attempt. This bounds admitted lock contention, not uninterruptible
native disk I/O. Native proof of the correction remains pending. Sources:
[rusqlite batch implementation](https://docs.rs/crate/rusqlite/0.40.2/source/src/lib.rs),
[rusqlite statement implementation](https://docs.rs/crate/rusqlite/0.40.2/source/src/statement.rs),
[statement completion/reset](https://sqlite.org/c3ref/reset.html),
[explicit finalization](https://sqlite.org/c3ref/finalize.html).

Use zero busy timeout during this owned Windows configuration phase, rather than assigning
remaining time to an internal handler whose locking-event waits are not the caller's absolute
deadline. The public zero-timeout contract disables that handler. The sole fixed WAL admission
loop owns the five-second allowance and yields only after its failed statement is finalized;
remaining local settings are not retried. Restore the ordinary five-second connection timeout
only for an accepted configuration. [busy-timeout contract](https://sqlite.org/c3ref/busy_timeout.html).

Exact head 608ab19's ARM64 native job 111026239366 entered the actual generic script at about
210 ms and its JSON conversion at 219 ms, but never reached json-parsed before the original
30-second deadline; owned cleanup finished at 30.042 seconds. The preceding 4887 trace measured
another first conversion returning only near 22.09 seconds. The statement-separator correction
removed the diagnostic parser/argument error; the remaining delay is actual unqualified converter
binding, not a filesystem ACL call or an increased/deferred queue budget.

Read-only inspection confirms the Windows PowerShell 5.1 Utility manifest names both a binary
DLL and Utility.psm1 in NestedModules. Restricting its exported functions does not prove that
the nested script was never loaded. Microsoft's Import-Module documentation permits an explicit
DLL filename or Assembly input and says those import only that file/assembly's members. Restricted
policy blocks psm1 and configuration script files. Select guarded explicit binary imports as a
candidate; native cold timing and policy acceptance still need proof. Sources:
[Import-Module 5.1](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/import-module?view=powershell-5.1),
[execution policies](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_execution_policies?view=powershell-5.1).

The selected stock library guard cannot depend on current_user_sid: it is also needed before
the first filesystem SID request. Its trust policy is therefore the existing Windows servicing
boundary of SYSTEM, Administrators and TrustedInstaller ownership/mutation only, with no current-
SID exemption. Retained no-delete directory and write/delete-excluding binary handles, with
explicit no-reparse/handle-bound descriptor checks and the trusted-mutation rule,
must establish the binary location before assembly import; a GAC directory suffix, module name,
strong-name token or post-load path string alone does not establish file ownership/privacy.

Native file opens, handle-bound descriptor reads and identity queries can themselves block.
Adding them before the existing generic driver's unconditional join would invalidate the entry
deadline even if every child pipe is timed. Run them in the admitted finite owner and retain
partial guards/permit on a stalled operation; the caller only receives a bounded result and never
joins an unfinished owner. Explicit post-operation expiry checks prevent a late guard result from
starting a child. The selected stall fixture uses an actual blocking anonymous-pipe read inside
this native guard phase; it does not fabricate a stalled GetSecurityInfo result.

The generic adapter's current Tokio kill_on_drop is weaker than kernel crash containment: process
exit does not guarantee Rust destructors run. Reuse the reviewed Core filesystem worker's pinned
process-wrap/win32job suspended-enrollment pattern before importing stock binaries or sending
private input. The retained independent kill-on-close Job must span guard ownership, running/idle
child and confirmed cleanup; root-wait loss remains quarantined even when the Job later empties.
The official [Job Objects contract](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects)
describes kill-on-close termination when the final Job handle closes. Native parent-crash evidence
is still pending and cannot be inferred from kill_on_drop or a successful normal-child exit.

The fixed filesystem owner's private-channel EOF path now retains uncertain cleanup ownership,
but source inspection is not native proof of that branch. A deterministic isolated fixture can
add one cfg(test)-only anonymous-pipe ReadFile to its owned cleanup boundary. A started blocking
read cannot be cancelled by JoinHandle::abort; hold the writer in the fixture until after the
unchanged three-second cleanup deadline. Require actual root reaping and an empty retained Job
before the deliberate pipe remains unconfirmed. This tests real outstanding native cleanup I/O,
not a fabricated Job-query/termination failure or a claim that a production descriptor call hung.
The private sender's final drop must park the owner with its stock/Job handles, even after that
read is later released. Native execution and the retained-handle assertion remain pending.
Tokio documents that abort cannot cancel an already started blocking task:
[pinned blocking-task contract](https://docs.rs/tokio/1.53.1/tokio/task/fn.spawn_blocking.html).

The first hosted private-channel EOF fixture on 93ebcb38 timed out waiting for its helper's
confirmation at forty seconds on all three native rows. The actual guarded diagnostic probes
passed; this failure is separate. Source inspection identifies a fixture-specific wait gap:
adapter_poll_until gates each Future poll against expiry but schedules no timer. A blocked
native read's JoinHandle cannot wake that gated future until the held writer is released, while
the fixture correctly waits for quarantine before releasing it. Wrap this owned read wait in
Tokio timeout_at using the same original cleanup deadline, retaining the per-poll expiry gates.
No production deadline, adapter or ownership policy changes. Source explains the circular wait;
native root/Job/read/quarantine confirmation remains required after the correction.
[Pinned timeout implementation](https://docs.rs/tokio/1.53.1/tokio/time/fn.timeout_at.html).

The corrected EOF fixture on exact 1c748ce reaches the owned read/quarantine receipt and exits
its helper in 3.38 seconds on the MSRV row, then its final exclusive stock open fails with raw
Windows sharing error 32. The enclosing cold Core test process has previously initialized its
global filesystem dispatcher and retains an unrelated stock guard; a fresh child alone does
not isolate this final release assertion from a live grandparent. Move the complete native EOF
proof to a mandatory post-cold exact helper selection after that mixed test process exits.
Require an actual exclusive-open baseline before its helper starts, followed by held sharing
violations and actual helper-exit release. A failed baseline remains an honest unrelated-holder
failure. Keep the original owned native read, root/Job confirmation, timer, late completion,
retained handles and finite waits; do not retire production caches or reinterpret error 32 as
successful release.

Exact Root44 c64832d native CI 37093718551 reports 120/122 Core tests passing on every native
row. Both passive inspector rename assertions fail because fs::rename returns success while
their metadata-only retained directory handle remains live (MSRV job 111119246570, ARM64
111119246407, x64 111119246523). The shared DirectoryGuard and stock ancestry open paths use the
same READ_CONTROL|FILE_READ_ATTRIBUTES access mask, so this is not solely a new inspector issue.
No-delete flags on a metadata observation alone are insufficient evidence of object retention.

CreateFileW distinguishes attribute-only requests from sharing-controlled read/write/delete
access. FILE_LIST_DIRECTORY and FILE_READ_DATA are the same native bit 1 for directory data,
whereas FILE_READ_ATTRIBUTES is bit 128. Select the minimal data/list bit in addition to existing
READ_CONTROL/attributes, with the existing read-only sharing and no-reparse directory flags.
This source-grounded correction must still prove actual empty-directory rename/reparse refusal,
late owned-handle retention and refusal when list access is denied. The proposed access change
is not native qualification and does not authorize takeover or fresh-root creation.
[CreateFileW access and sharing](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew),
[file and directory access rights](https://learn.microsoft.com/en-us/windows/win32/fileio/file-access-rights-constants).

Fresh service preflight cannot call DirectoryGuard::private for an absent root: that existing
entry point intentionally creates missing directories. A passive plan can retain the nearest
existing no-reparse directory chain, its full identity and the missing suffix without making
that suffix exist. An already-existing final root must separately satisfy the strict current-SID
private descriptor; an unsafe existing root is not an opportunity for takeover or repair.

The current stock DirectoryInfo.Create(DirectorySecurity) adapter reports created:true after
success, but .NET's internal directory implementation also accepts a concurrent existing
directory. That flag is not an exclusive creation receipt for fresh-install rollback. The
pinned fs_at 0.2.1 mkdir_at implementation is create-only and returns a native directory handle,
but it requests DELETE and hardcodes read/write/delete sharing. Its open_dir_at path supports
an explicit FILE_CREATE disposition, FILE_DIRECTORY_FILE option and custom desired access;
that remains a creation candidate requiring a complete descriptor/retained-identity/rollback
audit, not an approved effect primitive. Implement the passive inspector independently while
new-root creation, adoption and deletion remain held.
[Framework directory source](https://github.com/microsoft/referencesource/blob/main/mscorlib/system/io/directory.cs),
[pinned fs_at source](https://docs.rs/fs_at/0.2.1/src/fs_at/win.rs.html),
[native create dispositions](https://learn.microsoft.com/en-us/windows/win32/api/winternl/nf-winternl-ntcreatefile).

The held GUI proposal changes a service binding from one image to an exact console/launcher
pair; the previously qualified pure version-one record cannot describe that pair or a sealed
Run context. The proposed service record therefore uses an unpublished version-two schema and
keeps native ownership distinct from decoding. A GUID-only hint or a first authenticated
launcher connection is insufficient: the producer must durably observe the sole SDK Run GUID
before sending a remaining-budget Permit for actual hidden child creation. Runtime's proposed
consumer receives only a service-minted Serialize-only witness after the second kernel peer and
held supervisor lease/control proof. No raw capability or arbitrary PID is recovery authority.

The exact fixed JSON ActivationFact (context/digest, three UUID options and unknown_start) has
a 317-byte maximum versus null's four bytes. The optional RuntimeFacts witness (context/digest,
Scheduler GUID and maximum u32 launcher PID) has a 212-byte maximum versus null's four bytes.
These pure schema counts do not prove a complete record or the sixteen-KiB facts object fits;
actual typed full-object reservation/serialization and native handshake gates remain required.
The two-image record/consumer/producer refinement is planning only, dependent on complete GUI
and private Project review. It does not qualify source, activation or fresh-root effects.

The concrete GUI activation transport can reuse pinned interprocess 2.4.4's safe synchronous
peer queries: its Tokio named-pipe stream client_process_id/server_process_id call the matching
kernel functions on the actual connected handle. Claimed frame PIDs or a Scheduler GUID do not
replace those observations, retained child/control ownership, or the fresh Run capability.
Strict bounded JSON decoding remains a raw-data boundary and cannot mint local authentication.
[Kernel client PID contract](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-getnamedpipeclientprocessid),
[pinned stream source](https://docs.rs/interprocess/2.4.4/src/interprocess/os/windows/named_pipe/tokio/stream/impl.rs.html).

The resolved UUID graph already contains getrandom 0.4.3. Its safe fill API reports any failure
and does not promise initialized bytes on error; the audited Windows backend passes the exact
slice pointer/length to ProcessPrng in bcryptprimitives and accepts only TRUE. The crate is
MIT/Apache-2.0, requires Rust 1.85 and introduces no additional package version for this direct
Windows CLI edge. Generate the complete 32-byte nonce inside the existing finite native owner,
never from UUID text or a clock. Even normally fast entropy remains uncancellable native I/O;
late success cannot authorize intent/Run. This source audit is not native handshake qualification.
[Pinned fill contract](https://docs.rs/getrandom/0.4.3/getrandom/fn.fill.html),
[pinned Windows backend](https://docs.rs/getrandom/0.4.3/src/getrandom/backends/windows.rs.html).

The reviewed runtime factory correction at 967fa34 selects a final safe creation_flags setter
inside spawn_with after wrapper pre_spawn hooks. Core's local factory should preserve that same
native boundary: hidden plus temporary suspension at actual spawn, with the logical JobObject
policy unsuspended for resume after both enrollments. Capture the mask at real spawn rather than
treating a configured wrapper or CONOUT$ device availability as complete visibility evidence.

### Native mapping fixture null-string binding (2026-10-03)

The actual joined mapping fixture's native error is an empty mapName, before its entry marker;
it is separate from the measured generic converter delay. PowerShell's documented NullString
type explicitly supplies null to a .NET string method parameter and is present in its 5.1
reference assemblies. The .NET Framework reference source permits null but rejects a nonnull
empty mapName and delegates the six-argument FileStream overload without reopening the path.
Select NullString::Value only for this fixture argument, retaining the real writable view and
original-handle-close proof. Native success is pending. Sources:
[NullString contract](https://learn.microsoft.com/en-us/dotnet/api/system.management.automation.language.nullstring?view=powershellsdk-7.4.0),
[Framework mapping implementation](https://raw.githubusercontent.com/microsoft/referencesource/main/System.Core/System/IO/MemoryMappedFiles/MemoryMappedFile.cs#L162).

### Task Scheduler phase transport and callback bounds (2026-10-03)

Measured stock PowerShell starts can consume about twenty-two seconds on hosted ARM64 even for
small structured input. Reopening a helper per semantic read/disable/poll cannot preserve a
shared thirty-second lifecycle budget. Select a phase-scoped fixed COM dispatch child, holding
the existing generic permit only during that phase and closing it before updater PATH/WinGet
generic reads. A persistent filesystem child uses the other existing slot; this does not create
a third stock helper or change the fixed filesystem/generic source contracts.

Rust std::io::pipe is stable since 1.87 and supplies safe child Stdio ends within the 1.94 MSRV.
Its read/write operations can block and EOF requires every writer copy to close. Therefore the
approved engine OwnedChild can retain native Job ownership while three fixed I/O workers handle
the external pipe ends; no mutable child escape is necessary. EOF or a completed pipe write is
not a root/descendant exit fact. A synchronous journal callback can also block, so the selected
phase driver requires an owned Send+'static callback and quarantines unfinished I/O/guard/Job
ownership at the deadline rather than claiming cancellation or waiting indefinitely to join.

Scheduler generates a distinct RunningTask.InstanceGuid for each activation, and RunningTask.Stop
stops that instance. Its object is not an exclusive registration lock: validate the current-SID
security descriptor and semantic definition before each effect and reconcile a timed-out RPC as
uncertain. GetInstances is limited by caller security context, consistent with current-SID/LUA
registrations but not proof of all privileged tasks. Only fixed-name owned registrations are
in scope; engine PID and missing collection entries alone never prove Locron role exit.

Sources: [Rust 1.94 pipe implementation](https://github.com/rust-lang/rust/blob/1.94.0/library/std/src/io/pipe.rs),
[safe anonymous pipe contract](https://doc.rust-lang.org/std/io/fn.pipe.html),
[task-instance identity](https://learn.microsoft.com/en-us/windows/win32/taskschd/runningtask-instanceguid),
[exact instance Stop](https://learn.microsoft.com/en-us/windows/win32/taskschd/runningtask-stop),
[instance visibility](https://learn.microsoft.com/en-us/windows/win32/taskschd/registeredtask-getinstances),
[task security descriptor](https://learn.microsoft.com/en-us/windows/win32/taskschd/registeredtask-getsecuritydescriptor),
[CreateProcessW command-line limit](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-createprocessw).

Select the same fixed stock Windows PowerShell 5.1 adapter using the supported scripting COM
interface `New-Object -ComObject Schedule.Service`, `Connect()`, `NewTask(0)`, root-folder
`RegisterTaskDefinition` and typed task properties. This avoids unsafe COM in Rust and avoids
localized schtasks output. Use deterministic account/state/role task names; separate daemon and
dashboard definitions; current verified SID; LogonTrigger with that UserId; LogonType 3
(`TASK_LOGON_INTERACTIVE_TOKEN`); RunLevel 0 (`TASK_RUNLEVEL_LUA`); registration flags 6
(`TASK_CREATE_OR_UPDATE`); no password. Supply a task DACL retaining SYSTEM access. The creator
can manage their own tasks without admin under this registration type. Verify this on a real
standard account; an elevated hosted runner is not evidence of that product guarantee.

Set ExecutionTimeLimit to `PT0S`, DisallowStartIfOnBatteries and StopIfGoingOnBatteries to false,
RunOnlyIfIdle and RunOnlyIfNetworkAvailable to false, and no idle-stop condition. Use
MultipleInstances IgnoreNew and an explicit bounded RestartCount/RestartInterval, initially three
restarts at `PT1M`. The default duration is 72 hours and default battery settings can prohibit or
stop the daemon. Interactive-token operation includes a locked logged-in session; signed-out
operation is outside SPEC. Do not substitute S4U, which has documented network/encrypted-file
limitations. New login restarts ordinary durable reconciliation/missed-run recovery; task
StartWhenAvailable is not itself proof of logon or durable catch-up behavior.

Generate inspectable definitions with absolute ExecAction.Path, correctly escaped arguments,
explicit working directory and role/state identity. Read status as structured COM facts and
compare semantic definitions before refresh. Do not use IRunningTask.EnginePID as the Locron
process PID. Preserve enabled/disabled state across refresh and update. If log capture needs a
launcher, it must use CreateNoWindow, keep the Task Scheduler action alive until the real role
exits, propagate its exit status and preserve direct argv; do not register a fire-and-forget
launcher whose successful exit prevents failure restart.

`RegisteredTask.Stop(0)` stops immediately and is not Locron's graceful drain protocol. For
refresh/uninstall/update, disable automatic activation first, send an explicit authenticated
role/lifetime Shutdown request over a separately secured control endpoint, and have the role
cancel the existing CancellationToken/shutdown future. Wait for actual lifetime/daemon-lock exit
under a bounded deadline before task deletion/replacement, then restore the prior registration
state when appropriate. Task Stop is only the hard fallback after the cooperative deadline; it
does not justify declaring descendants stopped. Each attempt's kill-on-close ownership and
subsequent durable recovery remain necessary for forced termination or sign-out. Control
delivery failure is observable; neither a wake hint nor a task-state transition alone confirms
a completed graceful stop.

Sources: [TaskFolder registration and flags](https://learn.microsoft.com/en-us/windows/win32/taskschd/taskfolder-registertaskdefinition),
[non-admin own-account security context](https://learn.microsoft.com/en-us/windows/win32/taskschd/security-contexts-for-running-tasks),
[logon types](https://learn.microsoft.com/en-us/windows/win32/api/taskschd/ne-taskschd-task_logon_type),
[settings defaults](https://learn.microsoft.com/en-us/windows/win32/taskschd/taskschedulerschema-settingstype-complextype),
[execution limit](https://learn.microsoft.com/en-us/windows/win32/taskschd/tasksettings-executiontimelimit),
[restart parameters](https://learn.microsoft.com/en-us/windows/win32/taskschd/taskschedulerschema-restartonfailure-settingstype-element),
[ExecAction](https://learn.microsoft.com/en-us/windows/win32/taskschd/execaction),
[engine PID semantics](https://learn.microsoft.com/en-us/windows/win32/taskschd/runningtask-enginepid),
[immediate Stop and caller rights](https://learn.microsoft.com/en-us/windows/win32/taskschd/registeredtask-stop).

Development selected a hidden waiting stock PowerShell launcher using static EncodedCommand and
data-only EncodedArguments. The documented UTF-16LE EncodedArguments representation accepts a
serialized CLIXML argument array; a native Windows PowerShell 5.1 fixture round-tripped one
base64 JSON value containing Korean text, quotes, ampersands, dollar/backtick characters and a
trailing backslash without source interpolation. This proves argument transport only; real task
ownership, role exit propagation and crash restart still require native integrated acceptance.
Source: [Windows PowerShell 5.1 executable parameters](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_powershell_exe?view=powershell-5.1).

A disposable native registration probe on this host returned elevated=false and successfully
created/read/deleted one uniquely named disabled task. Readback confirmed INTERACTIVE_TOKEN/LUA,
PT0S, three PT1M retries, IgnoreNew, disabled battery/idle/network gates, and a protected
current-SID/SYSTEM-only task DACL with full-control mask 0x1f01ff. It did not execute a Locron role
or alter an existing task. Principal/logon-trigger readback returned account names despite SID
inputs, establishing that semantic identity comparison must resolve those names to actual SIDs.
This proves standard-user registration/readback only, not running-role ownership or restart.

The fixed hidden waiting launcher also ran in a uniquely named native task on the standard-user
host. Task state was observed running for about three seconds, then reported the nested owned
child's exact exit code 17 after its two-second wait; the driver deleted only the marker-verified
fixture. This establishes waiting/exit propagation, not Locron daemon-lock ownership. Failure
restart is tested separately before that behavior can be claimed.

The subsequent native restart probe did not observe a second run within eighty seconds after a
demand-started waiting launcher returned exit 17. Two further uniquely named time-triggered tasks
likewise propagated exit 17 and E_FAIL (-2147467259), respectively, without a restart in that
bound. Each task was disabled and deleted after marker verification. Changing the trigger or
returning a failing HRESULT did not establish unexpected-role-exit restart. Microsoft's protocol
specification describes RestartOnFailure in terms of unmet start conditions or failure to start
an action, a narrower description than the general API overview. The later one-owner corrective
contract above supersedes the initial native action-launch retry settings with Count=0; neither
probe proves ordinary completed role retries. A reviewed owned retry
supervisor/cancellation lifetime and actual-role-exit diagnostics remain prerequisites; a bare
PowerShell sleep loop would leave an uncontrolled gap after the child's activation lock exits.
Source: [MS-TSCH RestartOnFailure](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-tsch/2ff4aa5a-7bc4-449f-bbb1-27475645867f).

### Recommended design order and remaining proof

The first native ARM64 core run at PR #41 head `2f5516e` failed four initial private-directory
fixtures together about thirty seconds after test launch. The failures precede private-file
creation; x64 stable and pinned MSRV core/store runs succeeded. Current first SID discovery is a
racy cache get/query/set, allowing concurrent callers to start separate cold PowerShell processes.
Cold-start contention is a plausible cause, rather than a proved filesystem failure. Development
selects two bounded adapter permits and one finite SID initializer, with the unchanged thirty-second
deadline measured from API entry including queue wait. Saturated-queue and startup/termination
tests remain required; a fresh native ARM64 run must establish whether this resolves the failure.
Evidence: [native ARM64 job](https://github.com/WhiteKiwi/locron/actions/runs/37021167379/job/110884313590).

The next native ARM64 run at head `960a446` retained timeouts after bounded permits and the single
SID initializer were introduced; six filesystem fixtures and the script-entry marker fixture
failed. The output-limit fixture passed only after thirty seconds. Source audit found that the
reader returned an oversized successful buffer and enforced its limit only after waiting for
the child to exit. That consumes a complete permit while an output-blocked child remains alive,
so development selects immediate reader failure and the existing confirmed kill/reap cleanup.
It does not establish that this is the only ARM64 cause. Post-gate bounded stock PowerShell
diagnostics separately measure PE/process architecture, version/SID bootstrap and small/60k UTF-8
stdin/EOF round trips without warming the original cold-start tests. Operation deadlines and
privacy coverage remain unchanged.
Evidence: [subsequent native ARM64 job](https://github.com/WhiteKiwi/locron/actions/runs/37027297548/job/110905081384).

Hosted native tests found an elevated token's default file owner can be Administrators despite
an explicitly private SID-owned parent. Development retains the strict owner check and selects
the .NET Framework FileStream CreateNew constructor with FileSecurity to set owner/DACL atomically.
A standard-user stock PowerShell 5.1 fixture created a 355-character verbatim Unicode file path,
read back the current SID owner and protected SID/SYSTEM-only DACL, and observed numeric Win32
error 80 on duplicate creation. It cleaned only its owned empty files and directories. This proves
the adapter's long-path/descriptor/duplicate behavior; elevated Rust and SQLite acceptance remains
required. The fixed stock adapter clears inherited PSModulePath so PowerShell 7's module paths do
not cause PowerShell 5.1 to autoload incompatible ACL modules on hosted CI.
Source: [FileStream with FileSecurity](https://learn.microsoft.com/en-us/dotnet/api/system.io.filestream.-ctor?view=netframework-4.8.1#system-io-filestream-ctor(system-string-system-io-filemode-system-security-accesscontrol-filesystemrights-system-io-fileshare-system-int32-system-io-fileoptions-system-security-accesscontrol-filesecurity)).

Windows file sharing without FILE_SHARE_DELETE prevents concurrent rename until the reader
closes its handle. Output readers currently collect one snapshot and close before the next poll.
Development therefore retains the strict leaf sharing policy and selects a five-second retry
only for native sharing violations, revalidating both guarded paths each time. A persistent
holder leaves the synced partial recoverable and the failure observable. Native acceptance
must hold a framed-output reader across finalization and verify bytes after reader release.
Source: [CreateFileW sharing and rename semantics](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew).

Updater replacement needs a separate read/write, delete-only-sharing file gate. Microsoft's
CreateFile sharing contract permits delete/rename under that flag and exempts attribute-only
queries from sharing restrictions. Consequently a retained gate can exclude read/write/launch
without by itself proving a subsequent path-based rename moved that exact file. Exact source
handle rename uses SetFileInformationByHandle with appropriate access, but the reviewed ACL/file
identity helpers do not expose a safe borrowed-handle rename. Adapter selection and adversarial
native proof remain open; no unsafe workspace shim or unreviewed replacement fallback is selected.
Sources: [CreateFileW sharing contract](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew),
[SetFileInformationByHandle](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-setfileinformationbyhandle).

A disposable private copy of cmd.exe refused the proposed read/write delete-only gate while its
owned process was mapped (Win32 32). After that process exited, the live gate refused a new
launch (Win32 32). Path-based backup rename and rollback succeeded while the original stream
remained open and its bytes matched. The fixture cleaned its process/files/private directory.
This proves sharing compatibility and exclusion only; exact-object rename remains unresolved.

First freeze and compile the version-pinned safe interfaces, including the suspended post-spawn
enrollment and independent job query; then add guarded private state and descriptor readback;
then construct the account/state-isolated wake/control pipes; finally add Task Scheduler lifecycle
on top of the verified shutdown path. The existing Unix implementations remain separate target
backends, not broad test skips. Runtime acceptance must include immediate grandchildren, parent
exit with descendants alive, hard kill and daemon crash, failed enrollment/query, ancestor
junction swaps, a second user's read/write attempts, pipe occupation/remote rejection, clean
standard-account registration, battery/locked-session operation and graceful shutdown with active
work. This research has not performed those tests. Relevant repository issues must include Verify
criteria and record the results before the platform is advertised as supported.

### Hosted supervisor source-image refusal before semantic execution (2026-10-03)

Exact Root43 CI 37089768203, MSRV native job 111107408527, executed the complete serial service
filter: 26 passed, 17 failed, zero ignored. Thirteen supervisor assertions stopped in shared
Fixture::new line 1411 at open_read_no_follow(std::env::current_exe()) with PermissionDenied,
"unsafe managed path: \\?\D:\". The private destination guard at line 1408 had already succeeded.
This is a fixture setup refusal before role effects, not evidence that their retry, cancellation,
manual-owner, logging or quarantine contracts passed. The observed D: object is the workspace
source image, not the temporary root. Preserve the unique disposable child and assert its final
protected current-SID/SYSTEM leaf; never relax real managed-root checks to fit the runner.

The fixture's current source reader guards D: while copying its already-running test image.
A fixture-only retained std::File reader for exactly
std::env::current_exe(), copied to a fresh private managed destination, separates trusted test
image bytes from the managed-root policy. Production input/executable validation and every
supervisor-owned guard stay unchanged. Native execution of the resulting fixtures remains
pending; no child, tree or lifecycle success is inferred from the setup failure.

### First Windows PATH initialization exposes a committed historical seed (2026-10-03)

Exact Root43 `7c4e136` CI 37100504912, x64 native job 111138932141, passed all 118 Core,
81 Store and 43 service assertions. The lifecycle filter passed eleven and failed one: the first
registered daemon fixture read `/usr/local/bin:/usr/bin:/bin` at windows_lifecycle.rs:262, whereas
the inherited Windows PATH was expected. This log does not show a completed Store opener or a
persistent wrong seed; it establishes an early observation of committed settings.

Source inspection explains that observation. The historical initial SQL inserts the POSIX PATH.
Its first migration transaction commits before later migration steps; settings() can read that
row from early schemas. Store::open then writes the Windows default only after migrate returns,
and only when its preparation created the physical database file. Daemon role metadata can be
published before that Store opening, so it supplies identity rather than an initialization barrier.

There is also a distinct product gap, independent of the fixture's early read: a different writable
opener can finish with the old seed while the creator is between migration commits and its later
update. If the creator exits in that interval, future openers have physical-fresh=false and do not
perform that update. The failure log alone does not establish either sequence occurred in CI.
Changing the observer to wait until its expected string appears would not close these sequences.

SQLite documents that separate connections do not see an uncommitted transaction's changes and
that BEGIN IMMEDIATE obtains write admission before modifying data. Therefore the selected
correction seeds the Windows default inside the existing logical initial-schema transaction,
alongside settings and its verified migration markers, before the first commit. The physical
create winner is not the logical initialization authority. Historical SQL/checksums and already
versioned settings remain unchanged. Real two-connection tests will prove pre-commit visibility,
competing-writer exclusion, post-commit winner preservation and rollback/connection-close recovery;
the unchanged actual first-run lifecycle assertion remains the product canary. These are planned
Verify criteria, not locally executed SQLite or native Windows evidence.

Sources: [SQLite isolation](https://sqlite.org/isolation.html),
[SQLite transaction admission and rollback](https://sqlite.org/lang_transaction.html),
[exact failed native job](https://github.com/WhiteKiwi/locron/actions/runs/37100504912/job/111138932141).

### Native pruning fixture uses an unloaded Security cmdlet (2026-10-03)

Exact Root43 `c2a7495` CI 37105022659 passed native Core 118, Engine 69, Server 31, Store 90,
Service 43, lifecycle 12 and registered-dashboard 4 on the completed x64 stable/MSRV rows.
The original concurrent-first-open test and first-run PATH assertion now pass. Guarded pruning
passed its private-file removal and missing-file cases, but its unsafe-output fixture failed
at main.rs:7354 before any unsafe ACL/pruning assertion: Get-Acl was unavailable.

This is a fixture setup failure. The reviewed stock_json prefix deliberately disables module
autoloading and imports only retained, identity-checked Framework Utility/Management binaries.
Get-Acl/Set-Acl belong to Security and are not selected. Loading that module or re-enabling
ambient discovery would change the loader contract to accommodate a test rather than exercise
its existing boundary. The same fixed fixture can obtain and update the temporary file's
FileSecurity through the documented .NET Framework System.IO.File.GetAccessControl and
SetAccessControl methods in mscorlib. Preserve the Everyone-Read rule and all later refusal,
pending-state, byte-preservation, junction and directory assertions. No production ACL/loader,
Store or pruning policy changes are needed. The method signatures are verified documentation;
the corrected fixture has not yet run natively and must pass on all three hosted rows.

Sources: [completed x64 stable job](https://github.com/WhiteKiwi/locron/actions/runs/37105022659/job/111151723399),
[completed x64 MSRV job](https://github.com/WhiteKiwi/locron/actions/runs/37105022659/job/111151723430),
[Framework file ACL read](https://learn.microsoft.com/en-us/dotnet/api/system.io.file.getaccesscontrol?view=netframework-4.8.1),
[Framework file ACL write](https://learn.microsoft.com/en-us/dotnet/api/system.io.file.setaccesscontrol?view=netframework-4.8.1).

### Actual WAL recovery contention is outside the current retry predicate (2026-10-03)

Root43 `bfe7b8d` CI 37107116124 passes Core 118, Engine 69 and Service 43 on both completed
x64 rows. Server passes 30/31: the original active-SSE shutdown test fails when reopening its
durable state at lib.rs:1060. The retained trace reports sqlite-configure-wal, primary
DatabaseBusy and extended 261. Cargo stops there, so these rows do not run the Store suite or
later lifecycle/pruning gates. This is different from the earlier concurrent-first-open failure.

The current WAL predicate accepts extended code 5 only and immediately returns 261. The log
does not show exhaustion of the five-second admission budget. SQLite documents 261 as
SQLITE_BUSY_RECOVERY; the pinned SQLite 3.53.2 source returns it after an unsuccessful shared
recovery-lock acquisition while the WAL index is being recovered. Guarded sidecar preparation
does not acquire that SHM recovery lock. Preserved WAL/SHM leaves can require index recovery
after a clean close too; a process crash or a particular blocking connection is not established.

Select an explicit {5,261} allowlist only inside the idle, fully finalized, fixed WAL statement
admission. Keep the same API-entry five-second budget and ten-millisecond bounded yields,
step/finalize error precedence, verified WAL result and one-shot remaining settings. Snapshot
517, timeout 773, LOCKED, read-only and I/O errors remain outside that allowlist. No arbitrary
SQL operation, transaction, migration or generic Store open is replayed. The four existing
native WAL controls retain their exact-5 assertions; the actual Server fixture stays unchanged.

The measured native 261 failure is real regression evidence. A passing Server suite does not
prove that every execution encountered 261. No deterministic recovery-suspension hook was
found in the current safe SQLite API: WAL recovery scans through VFS operations outside its
ordinary progress/trace callbacks. A barrier plus a large WAL alone is not a deterministic
fixture. Additional organic recovery observation remains a separate investigation; do not
fabricate an Error value or add a timing-dependent acceptance test for this correction.

Sources: [completed x64 stable failure](https://github.com/WhiteKiwi/locron/actions/runs/37107116124/job/111157711707),
[completed MSRV failure](https://github.com/WhiteKiwi/locron/actions/runs/37107116124/job/111157711755),
[SQLite recovery and snapshot codes](https://sqlite.org/rescode.html#busy_recovery),
[WAL close/recovery contention](https://sqlite.org/wal.html#sometimes_queries_return_sqlite_busy_in_wal_mode).

### The later maintenance fixture has the same unloaded ACL cmdlet (2026-10-03)

Root43 `bfe7b8d` ARM64 completes Server 31, Store 90, lifecycle 12 and guarded pruning 3/3.
The next complete maintenance harness passes 10/11, then its unsafe-object setup fails at
maintenance.rs:925 on Get-Acl. Root44 `60e2fcc` MSRV reaches the same failure after passing
the corresponding Core 127, Server 31, Store 90, Service 64, GUI 12 and pruning 3/3 gates.
The fixed loader and error are identical to the earlier main.rs fixture compatibility case.
The only remaining Get-Acl/Set-Acl references under crates are these two maintenance setup
statements. Apply the same documented Framework FileSecurity substitution; retain its
Everyone-Read rule and all later recovery/prune/byte/junction/foreign-object assertions.
Production maintenance, loader and filesystem admission remain unchanged. Fresh native
maintenance 11/11 on all three rows is required; earlier pruning success does not qualify it.

Sources: [Root43 completed ARM64 job](https://github.com/WhiteKiwi/locron/actions/runs/37107116124/job/111157711798),
[Root44 completed MSRV job](https://github.com/WhiteKiwi/locron/actions/runs/37107738582/job/111159474352).

### Exact executable replacement and recoverable deletion (2026-10-03)

A path-based rename does not identify the source by the exact retained executable handle.
The audited Windows implementations of cap-primitives, atomic-write-file and renamore resolve
source/destination paths for their rename operation; they do not provide the needed consuming
source-File replacement interface. This is an interface limitation of these audited versions,
not evidence that safe Rust executable replacement is impossible.

The concrete candidate is Windows-only `fs_at = "=0.2.1"`, Apache-2.0, declared MSRV 1.71.0.
Its safe `fs_at::os::windows::FileExt::delete_by_handle(self: File)` returns
`Result<(), (File, io::Error)>` and calls SetFileInformationByHandle on that exact owned File,
first with FileDispositionInfoEx and then with a compatibility fallback when appropriate.
Workspace call sites need no unsafe block and preserve Rust 1.94 and unsafe_code=forbid.
It does not provide an atomic rename transaction; the design must make deletion recoverable.

Acquire the old regular executable with read/write/DELETE access, OPEN_REPARSE_POINT and
share_mode(0), under the existing strict private descriptor and retained ancestor checks.
Microsoft documents that zero sharing denies later read/write/delete opens, delete permission
also governs rename, and CreateNew fails on any existing entry. The exclusive old gate verifies
the exact old receipt, bytes and complete volume/file identity and is retained until exact-handle
deletion. The audited file-id 0.2.3 high-resolution query opens with access_mode(0), so a metadata
query remains compatible with the live gate; the guarded leaf cannot be renamed during that query.
Refuse a read-only or multiply linked replacement leaf rather than repairing ownership or
mutating a second link. The shared filesystem adapter needs explicit exclusive-existing and
exclusive-create entry points: ordinary open_private intentionally has shared read/write behavior
and must not silently discard the replacement gate's sharing/access requirements.

Before deletion, copy and flush a verified old binary, receipt and owned companion payloads into
the private operation root. Persist the serializable all-task restore record, old leaf identities,
backup hashes and delete intent before consuming the old File. Retain the parent guards and create
the destination only with CreateNew, private inherited security, immediate handle-bound readback
and the same exclusive gate. Persist the fresh complete file identity before writing; flush and
verify every new byte and the receipt before releasing the executable gate or restoring task
activation. Rollback applies the same exact-identity deletion and exclusive creation rules to
recorded leaves; it never overwrites an unexpected entry.

Source review found an important error boundary: the fs_at read-only compatibility branch can
successfully mark a file for deletion and then return Err when restoring its attribute fails.
Therefore every deletion error is ambiguous, even if ordinary supported files avoid that branch.
Keep verified backups and the durable journal; inspect the guarded destination and reconcile the
recorded identities instead of assuming the old path survived. If it is absent, a verified backup
can be created exclusively. If it is an unrecognized entry, preserve it, keep activation disabled
and report explicit recovery instructions. A crash between fresh creation and identity-record
flush is likewise an explicit refusal boundary, not an automatic permission to delete an empty
or partially written file. The retained verified helper resides outside the replaced directory,
so install.ps1 -Operation UUID can inspect/recover a standalone operation even when locron.exe is
missing. Status files are derived reporting and never authorize recovery by themselves.

A tools-only rehearsal on this standard-user Windows 11 x64 host used a test-owned copy of the
stock command interpreter, hidden owned child processes and a unique temporary directory. The
exclusive read/write/DELETE gate refused the mapped image with sharing violation; after child
exit it blocked a competing rename and relaunch. With a flushed verified backup and write-ahead
record, exact-handle FileDispositionInfoEx deletion succeeded. Exclusive CreateNew kept launch
blocked until complete bytes were flushed and checked. A competing destination made CreateNew
fail with ERROR_FILE_EXISTS and its marker bytes and backup remained unchanged. All owned children
and test paths were cleaned up. This establishes OS primitive behavior on one x64 host; it did
not compile fs_at in this workspace, exercise ARM64, validate the complete ACL adapter, or prove
crash recovery/all-task lifecycle. Native Rust fixtures and every crash phase remain release gates.
Windows write-through plus sync_all request durable OS flushes; Microsoft's caching guidance
explicitly limits claims about hardware write-through support.

Sources: [fs_at version metadata](https://docs.rs/crate/fs_at/0.2.1),
[exact consuming deletion source](https://docs.rs/crate/fs_at/0.2.1/source/src/win.rs),
[CreateFile sharing, CreateNew and caching](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew),
[handle-bound deletion and DELETE access](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-setfileinformationbyhandle),
[file-id metadata-only open](https://docs.rs/crate/file-id/0.2.3/source/src/lib.rs),
[audited cap-primitives path rename](https://github.com/bytecodealliance/cap-std/blob/main/cap-primitives/src/windows/fs/rename_unchecked.rs),
[audited atomic-write-file path rename](https://github.com/andreacorbellini/rust-atomic-write-file/blob/master/src/imp/generic.rs),
[audited renamore path rename](https://github.com/indianakernick/renamore/blob/master/src/windows.rs).

### Preserve the raw per-user PATH and its registry kind (2026-10-03)

The .NET Framework user-environment getter reads Registry.GetValue with its default expansion
behavior, while its setter uses Registry.SetValue without an explicit kind. A REG_EXPAND_SZ PATH
containing `%USERPROFILE%` can therefore be expanded and rewritten as REG_SZ. Recording only the
returned string cannot restore the original persistent value. Stock RegistryKey.GetValue with
DoNotExpandEnvironmentNames, GetValueKind and SetValue with the recorded String/ExpandString kind
provide a safe interface for exact raw-value preservation. Missing and empty values are distinct.
Rollback also compares the current raw value and kind with the recorded result before restoring.

Windows documents an Environment WM_SETTINGCHANGE broadcast for immediate shell notification.
The audited winsafe 0.0.29 SendMessageTimeout entry point is explicitly unsafe, so it is not a
usable call site under this workspace's unsafe_code=forbid policy. No additional native adapter
is required for the optional persistent PATH edit: report that the new value applies after the
next sign-in, without changing process PATH or claiming immediate shell propagation.

Sources: [.NET Framework user-environment implementation](https://github.com/microsoft/referencesource/blob/main/mscorlib/system/environment.cs),
[unexpanded registry reads](https://learn.microsoft.com/en-us/dotnet/api/microsoft.win32.registryvalueoptions?view=netframework-4.8.1),
[explicit registry value kinds](https://learn.microsoft.com/en-us/dotnet/api/microsoft.win32.registrykey.setvalue?view=netframework-4.8.1),
[Environment change notification](https://learn.microsoft.com/en-us/windows/win32/winmsg/wm-settingchange),
[audited winsafe SendMessageTimeout](https://docs.rs/crate/winsafe/0.0.29/source/src/user/handles/hwnd.rs).

### Unsigned first install under stock script policy (2026-10-03)

Microsoft documents that Restricted permits individual commands but blocks script files.
RemoteSigned can also block unsigned downloaded scripts with Zone.Identifier until the operator
selectively unblocks that reviewed asset; Unblock-File does not change execution policy and
cannot make Restricted execute scripts. AllSigned and organizational policy can impose additional
restrictions. This host's stock PowerShell 5.1 policy refused the test .ps1 source; source AST
parsing succeeds, but that is not installer execution evidence.

The selected route adds native Windows-only install, uninstall and UUID recovery commands to the
same binary, sharing the strict verifier, protected request, fixed OS adapters and helper engine.
It executes compiled procedures and does not load arbitrary .ps1 contents, select Bypass or
change user/machine policy. It therefore removes the unsigned-script prerequisite for native
installation while continuing to respect native executable controls. Keep the .ps1 assets as an
optional frontend with the actual policy prerequisite stated. Test the native route on clean
standard-user Windows 11 x64/ARM64 under Restricted before announcing support.
The same native maintenance/status frontend supplies the already selected portable WinGet
quiesce/restore procedure. Portable manifests execute no script/native lifecycle hook; first
package installation has manual service configuration, and package upgrade/removal requires
confirmed maintenance rather than assuming automatic task behavior.

Sources: [PowerShell 5.1 execution policies](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_execution_policies?view=powershell-5.1),
[selective file unblocking](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.utility/unblock-file?view=powershell-5.1),
[organizational policy precedence](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.security/set-executionpolicy?view=powershell-5.1).

### Portable WinGet automatic-upgrade limitation (2026-10-03)

Schema 1.12 documents RequireExplicitUpgrade as identifying packages that upgrade themselves;
it excludes them from upgrade --all by default. Locron deliberately refuses self-update for
WinGet-owned files, so that field cannot truthfully describe this channel merely because task
maintenance requires an explicit workflow. Do not change the proposed manifest to assert it
without a supporting community policy decision. Official WinGet documentation says upgrade --all
attempts available updates and has no Locron Prepare/Complete hook.

An enabled daemon/dashboard attached to a portable package therefore needs the explicit reviewed
maintenance procedure. Unattended upgrade tools and upgrade --all do not perform it and cannot
be advertised as supported with those roles enabled. Prefer the standalone channel when integrated
service lifecycle/self-update is required. This limitation remains a channel-promotion review
item; no automatic safety, pin, self-updating flag or hook is inferred from manifest validation.
The native installer work can proceed independently without changing the portable format.

Sources: [schema 1.12 RequireExplicitUpgrade meaning](https://github.com/microsoft/winget-pkgs/blob/master/doc/manifest/schema/1.12.0/installer.md#fields),
[WinGet upgrade-all behavior](https://learn.microsoft.com/en-us/windows/package-manager/winget/upgrade#upgrade---all).

### Bound the future package path before preparing maintenance (2026-10-03)

WinGet Complete selects the newly registered executable after an external package upgrade, so
Prepare cannot reserve only the old path's length. The selected maintenance scope caps both the
normalized executable and registered location at 4,096 UTF-16 code units. Repeat the check on the
actual guarded canonical paths, including their supported verbatim transport prefix. The four-unit
prefix leaves at most 4,092 normalized drive-path units; it cannot be stripped from capacity checks
merely because path comparison strips it. This string bound never grants ownership. Runtime and
state-path handling are unchanged. A larger new package location refuses Complete before restoration, leaving
owned tasks disabled and the protected journal available.

For the native JSON encoding, each allowed BMP character takes at most three UTF-8 bytes per
UTF-16 unit, each supplementary character takes four bytes per surrogate pair, and an escaped
backslash takes two bytes. Local path validation rejects controls and quotes. Thus 12,288 bytes
plus string quotes is a conservative bound per future path. Preparation must count every repeated
path, full typed service snapshot, cumulative forced-stop fact and callback/rollback frame before
any effects. The 128 KiB frame/16 MiB/128-frame limits and 256-binding service ceiling are retained;
an oversized inventory refuses with zero effects rather than truncating recovery authority.

This is a distribution reservation decision, not a claim that Windows has a 4,096-unit path limit.
Native boundary fixtures must verify normalization, UTF-16 counting, JSON escaping and refusal
before supported maintenance is promoted.

Sources: [Windows path character rules](https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file),
[pinned serde_json UTF-8 fragments and escape table](https://docs.rs/crate/serde_json/1.0.151/source/src/ser.rs).

### Native portable ownership registry references (2026-10-03)

WinGet's official PortableARPEntry source selects the HKCU 64-bit uninstall view for user scope,
including ARM64. It records WinGetPackageIdentifier, WinGetInstallerType, WinGetSourceIdentifier,
DisplayVersion and InstallLocation as string values. TargetFullPath is the legacy single-file
reference; indexed ZIP installations can omit it. The existing selected nested ZIP path therefore
remains the exact expected location rather than an alias or a recursive executable search.

The native maintenance verifier can read these fixed values through the reviewed stock registry
adapter without creating a key. Retain existing trusted-owner/no-reparse location and immutable
source-file guards, repeat the canonical path bound and compare the actual executable against
the complete canonical release/archive digest. Registry metadata alone never authorizes task
effects; revalidate its exact reference before lifecycle mutation. This verifies the already
selected ownership contract, without changing manifest format or assuming a WinGet hook.

Sources: [WinGet user-scope view and exact value names](https://github.com/microsoft/winget-cli/blob/b0f6209cc58841c19e432325b127ab3c33a8f6f8/src/AppInstallerCommonCore/PortableARPEntry.cpp),
[portable archive index and legacy target behavior](https://github.com/microsoft/winget-cli/blob/b0f6209cc58841c19e432325b127ab3c33a8f6f8/src/AppInstallerCLICore/PortableInstaller.cpp).

### Literal PATH insertion and frozen native state selection (2026-10-03)

RegistryValueKind distinguishes literal REG_SZ from expandable REG_EXPAND_SZ. A new installation
directory containing percent characters can be stored literally in REG_SZ, but preserving an
existing expandable kind could reinterpret it when another process retrieves the environment.
Refuse that insertion instead of changing the existing kind or expanding its prior fields.
Semicolon-containing destinations remain unrepresentable as one PATH field. Detect duplicates
only among validated normalized absolute literal fields; a match is not newly installer-owned.
This conservative comparison does not equate environment references, aliases or arbitrary
quoted/relative fields. Exact raw value and kind remain the conditional write/rollback predicate.

Native dispatch occurs before ordinary state discovery. Its new-install state override must be
request metadata rather than an ambient lookup by the copied helper. Clap retains whether the
global option came from the command line or environment. Recovery can therefore reject a new
explicit override, ignore an ambient override and retain the protected original selection.
This closes the pre-discovery frontend contract without changing ordinary state-path behavior.
Verify missing/empty and String/ExpandString PATH cases, literal duplicates, percent and semicolon
refusals, an intervening registry edit, CLI/environment install selection, and UUID recovery in
a differently configured shell before exposing the native commands.

Sources: [literal and expandable registry value kinds](https://learn.microsoft.com/en-us/dotnet/api/microsoft.win32.registryvaluekind?view=netframework-4.8.1),
[locked clap argument value provenance](https://github.com/clap-rs/clap/blob/13f2db5072d600c11d8d6298e4e9ba53ffc6c1ab/clap_builder/src/parser/matches/value_source.rs).

### Native distribution fixture root ownership (2026-10-03)

The exact PR #44 package run at eff2c19 built both native archives, then failed thirteen
distribution fixture setups on each architecture at restrict_owned(temp.path(), true).
The failure paths used RUNNER~1, but the logs did not establish that spelling as the cause.
The source explicitly refuses repair when the object's owner is not the current SID; a
disposable TempDir's inherited ownership is therefore not an acceptable setup assumption.
These failures preceded journal, bootstrap, source-ownership and replacement assertions.

Use the existing core fixture approach: retain a unique TempDir for cleanup, guard its existing
ancestry, create a missing private child through DirectoryGuard::private, and use that child's
normalized_path for all test objects. Retain the child guard and assert is_private(path, true),
which verifies the actual current-SID owner and protected private descriptor. Do not repair or
adopt the inherited TempDir, loosen production ownership/ancestry predicates or bypass a guard.
This corrects fixture setup; all mapped-image, backup, unknown-leaf and crash checks still need
actual native execution on both architectures.

Evidence: [x64 package job](https://github.com/WhiteKiwi/locron/actions/runs/37058613767/job/111009366048),
[ARM64 package job](https://github.com/WhiteKiwi/locron/actions/runs/37058613767/job/111009366093),
[reviewed core guard and native private-root fixture](https://github.com/WhiteKiwi/locron/blob/eff2c19b78ddc9c7f80848c76e7eb4eee56e3b76/crates/locron-core/src/filesystem.rs).

### Native positive distribution harness admission (2026-10-03)

The next exact PR #44 run at 846c6c2 again built both native archives. The x64 distribution
contracts finished with 37 passes and eight failures, all in the private-child setup at
windows_fixture.rs:17: six reported stock Windows adapter deadline elapsed and two filesystem
caller deadline elapsed. ARM64 finished with 44 passes and one failure: the current-user PATH
read reported stock Windows adapter timed out. Its other distribution assertions, including the
private-root corrections, passed. These logs establish bounded timeouts, not the specific queue,
cold-start or host-load cause, and do not justify relaxing ownership or deadline checks.

Each libtest process shares the approved one filesystem plus one generic/COM worker admission
slots. Serialize the independent positive distribution fixtures with --test-threads=1 so setup
success does not rely on unbounded parallel requests finishing inside each 30-second admission
and execution deadline. Preserve explicit concurrent admission/cold-call/deadline fixtures and
all production slots, budgets, guards and cleanup rules. Do not warm the first call, skip failed
assertions, retry timeouts or infer completion from a deadline. A fresh reviewed revision must
pass the complete native x64/ARM64 distribution harness once, with no ignored tests; dedicated
concurrency/deadline gates remain separate required evidence.

Evidence: [exact x64 package job](https://github.com/WhiteKiwi/locron/actions/runs/37064979742/job/111030476464),
[exact ARM64 package job](https://github.com/WhiteKiwi/locron/actions/runs/37064979742/job/111030476465).

### Stock PowerShell ZIP assembly loading (2026-10-03)

The exact PR #44 run at d34aaad passed all 45 native distribution assertions on both architectures
with no ignored tests. Its next stock PowerShell 5.1 fixture step failed before archive assertions:
New-ZipFixture loaded only System.IO.Compression.FileSystem, then could not resolve ZipArchive.
The installer parser has the same load statement and must be corrected with the fixture.

Microsoft's .NET Framework API reference places ZipArchive and ZipArchiveMode in
System.IO.Compression.dll. FileSystem provides ZipFile and filesystem extension APIs; our two
consumers use only archive/entry streams. Explicitly load the defining System.IO.Compression
assembly before constructing those types in each consumer. Do not rely on an incidental dependency
load or a prior profile/session. This loads an existing framework assembly, without compiling
source, downloading a runtime or changing execution policy.

Verify in the existing native x64/ARM64 stock 5.1 CI step: a fresh no-profile session constructs
the fixture ZIP and runs every existing positive and negative archive/status assertion. AST checks
are supplemental; fixture execution and actual bootstrap qualification remain distinct gates.

Sources: [ZipArchive definition and extension separation](https://learn.microsoft.com/en-us/dotnet/api/system.io.compression.ziparchive?view=netframework-4.8.1),
[ZipArchiveMode assembly](https://learn.microsoft.com/en-us/dotnet/api/system.io.compression.ziparchivemode?view=netframework-4.8.1),
[Windows PowerShell 5.1 Add-Type assembly parameter](https://github.com/MicrosoftDocs/PowerShell-Docs/blob/main/reference/5.1/Microsoft.PowerShell.Utility/Add-Type.md),
[exact native package run](https://github.com/WhiteKiwi/locron/actions/runs/37068595779).

### Task activation proof and fresh-root preflight (2026-10-03)

RegisteredTask.Run returns a RunningTask object, but Microsoft documents success without a start
when AllowDemandStart is false. InstanceGuid is generated by Task Scheduler for each run; it is
not a caller-selected transaction identifier. Enabled describes registration availability, and
GetInstances exposes only instances at or below the caller's security context. Preserve the
selected least-privilege definition and require actual GUID plus owned role/lifetime readback;
neither COM success nor scheduler state proves the Locron role started.

The selected existing-operation policy activates every originally enabled owned registration,
regardless of a transient stopped state, and never starts an originally disabled registration.
The provider must freeze distinct start intent, observed/confirmed proof and unknown-start states,
including bounded no-redispatch recovery, before complete journal preflight can authorize effects.
Reserve actual future typed object bytes and the longest mutually exclusive forward/rollback
branch; the current registration-only model deliberately refuses Completed.

Fresh creation needs a separate no-effect ancestry plan. DirectoryGuard::private creates missing
components and cannot qualify that plan before the full native preflight. Existing-only guards,
created-directory full identities and exact empty-directory removal need real safe interfaces.
Only created exact task definitions/roots can authorize rollback; unrelated state stays intact.
The service owner must freeze that real bounded record instead of a null or synthetic snapshot.

Sources: [Run behavior](https://learn.microsoft.com/en-us/windows/win32/taskschd/registeredtask-run),
[generated instance identity](https://learn.microsoft.com/en-us/windows/win32/taskschd/runningtask-instanceguid),
[enabled registration property](https://learn.microsoft.com/en-us/windows/win32/taskschd/registeredtask-enabled),
[instance security visibility](https://learn.microsoft.com/en-us/windows/win32/taskschd/registeredtask-getinstances),
[reviewed directory guard interfaces](https://github.com/WhiteKiwi/locron/blob/cb5074a711a80dbf66bf04d64d8edbbb94006684/crates/locron-core/src/filesystem.rs).

### Mapped-helper qualification through a guarded launch (2026-10-03)

Rust current_exe returns a path and documents rename/security limitations. Microsoft
QueryFullProcessImageName and GetMappedFileName likewise return names, not a file handle or full
volume/file identity. Reopening such a name and hashing the current leaf cannot retrospectively
identify an image loaded before a rename or replacement. Bootstrap's retained helper proof is
therefore necessary but not independently sufficient to qualify its already running image.

CreateFile sharing restrictions remain in force while the handle is open; FILE_SHARE_DELETE
permits rename as well as deletion. The reviewed immutable leaf and all ancestor guards retain
FILE_SHARE_READ without write/delete sharing. CreateProcess with an explicit application path
selects that executable, and Rust 1.94's native Command implementation passes its resolved absolute
.exe path as lpApplicationName. The selected inference is conditional on retaining those exact
guards from before CreateProcess through acquisition of overlapping child guards and qualification:
the path cannot change to a different object during that interval. This is a launch proof, not a
new API that retrieves a mapped image's file ID. CreateProcess success precedes initialization and
does not establish child readiness or operation acceptance.

The stock Framework Process.MainModule identifies the module used to start the local process;
its FileName is a path, can be unavailable during initialization or truncated, and MainModule can
throw on unsupported bitness or process exit. A fixed bounded stock .NET query supplies readback
without new Rust FFI. Bracket its PID lookup with live checks on the original native Child and
retain that Child/lease throughout; exit or uncertainty refuses, preventing a reused PID from
authorizing a different process. Require normalized-path and exact retained-object agreement.
Actual native x64/ARM64 behavior, including the stock adapter's architecture, remains unverified
for this new gate. No query failure is permission to fall back to current_exe alone.

Rust 1.94's raw attribute-list spawning remains unstable, so arbitrary inherited handle-list
transfer is not selected. A private four-frame qualification exchange keeps the parent lease alive
until overlapping child protection is confirmed. Borrowed global Rust stdout cannot be safely
closed independently while the child remains live; the copied libtest harness also writes its own
stdout. Waiting for stdio EOF would require losing that live-child overlap or an indefinite wait.
Two owned one-way named-pipe endpoints replace the earlier stdio/stderr proposal; no quiet period,
Peek or noise filtering is selected. Live qualification tokens still come only from the native
launch owner and its actual Child/channel, never deserialized metadata.
The repository's existing SECURITY account boundary excludes arbitrary same-account code/debugger
control. No unsigned publisher authentication, retrospective proof for a manually started broker,
service snapshot authority or completed operation is inferred from this plan.

The concrete child sequence needs a private ChildExchange in addition to Bootstrap: the receive
endpoint/Challenge must be acquired before cold Bootstrap SID/guard work so the actual remaining
budget can reach those calls. The selected private begin_child -> ChildExchange and
qualify_child(Bootstrap, ChildExchange) sequence retains that endpoint and original shortened
deadline; neither object has a deserialization/clone constructor for live proof. The parent
Pending/Qualified token retains the sole owner connection, while that owner retains the actual
native Child/lease/guards and uncertain I/O. This is ownership plumbing, not a new process-ID
authentication API. The first real copied-helper fixture consumes existing standalone/removal
proof; canonical payload/package/recovery authority and all operation effects remain unqualified.
The concrete async implementation must place those owned resources outside its unwind/wait
boundary. ChildQualification first owns Bootstrap plus ChildExchange; asynchronous finish borrows
that state, and only checked success can mint QualifiedBootstrap. This prevents a future's panic
or timeout from discarding guard/close ownership. The existing inventory verifier likewise needs
the caller's original clock around each receipt/payload step rather than an unbounded outer call;
its normal entrypoints retain their behavior. These are retention/deadline refinements, not new
service authority, effects or native qualification evidence.

The source-feasibility audit found that cold current_user_sid currently creates its own 30-second
deadline. A bounded initializer must initialize that same cache with the launch phase's remaining
budget; surrounding a fresh default-budget call with clock checks is insufficient. Native guards
and pipe reads can also block, so the caller needs a finite retained owner and uncertain-cleanup
quarantine rather than an indefinite join. These are planned refinements, not native proof.

Pinned interprocess 2.4.4 exposes safe client_process_id/server_process_id on connected streams,
safe creation-time descriptor/listener options, and safe synchronous stream/OwnedHandle conversions.
Create the parent-to-child and child-to-parent pipes as send-only servers owned by their respective
senders. Tokio ClientOptions safely opens receive-only overlapped clients with identification SQOS.
Only for the server-PID query, clone the client's BorrowedHandle into OwnedHandle and wrap it in
synchronous PipeStream<Bytes,Bytes>; after the query consume that metadata-only wrapper through
its safe evade_limbo method on success and failure. This wrapper performs no reads/writes or
second registration. The original Tokio client remains receive-only. The metadata wrapper's
send-mode parameter merely exposes deterministic safe cleanup: evade_limbo clears unknown flush
state and closes instead of entering limbo, avoiding an unexpected extraction-error leak.
Its optional ReOpenFile may request read/write access and may fail; pinned construction then
retains the cloned original handle. Neither outcome relaxes the required actual client direction
and server-PID checks, and all native calls still consume the retained owner's original deadline.
This avoids unsafe Tokio raw-handle calls in this workspace and double I/O registration. It also
avoids wrapping an ordinary send-only handle: GetNamedPipeInfo requires additional read-attribute
rights for a write-only pipe. Peer-PID queries are consistency checks under the original live Child
and continuous guards, not standalone process authentication. Actual client-side server-PID
behavior and native architecture/ACL facts remain qualification requirements.

Use the already selected private SID+SYSTEM descriptor, local-only first-instance creation and
noninheritance. Accept exactly once with a two-instance ceiling, then drop the replacement listener;
the connected stream keeps the name reserved until its own closure. Microsoft documents that a
server FlushFileBuffers waits for the client to consume buffered bytes. A further pinned-source
audit found that interprocess's c_wrappers::flush calls downgrade_eof, converting BrokenPipe and
ERROR_PIPE_NOT_CONNECTED into success. Its explicit flush result therefore cannot confirm this
strict terminal delivery. The selected safe refinement clones the connected sender's
BorrowedHandle into a temporary OwnedHandle, converts it to std::fs::File and runs sync_all in a
retained blocking worker. Rust 1.94's Windows fsync calls FlushFileBuffers through cvt and preserves
failure; the safe owned-handle File conversion has been stable since Rust 1.63. This metadata/flush
clone performs no reads/writes or overlapped registration and closes before the original sender
is consumed with evade_limbo. Plain interprocess drop may instead hand an unflushed stream to its
limbo worker. A timed-out raw flush is not cancelled by dropping its join handle or evading limbo:
the finite launch owner must retain the outstanding worker, actual Child, admission permit, lease
and guards without an indefinite caller join or a late qualified token. Both receiver phase/EOF
checks remain mandatory; successful raw flush alone is not launch proof. Native positive delivery,
disconnected-reader refusal and unread/held-open timeout/retention tests on both architectures
remain unverified and cannot be inferred from this source-feasibility check.
After Permit and Qualified respectively, each receiver must observe exact terminal EOF within the
original deadline and reject any extra byte before it; neither sender needs to exit its process.
Both copied fixture stdio outputs are null, and there is no extra channel-selector ABI or need for
a direct winapi-util dependency. Four frames and both terminal reads remain read-only qualification,
with no helper acceptance, status/journal write or lifecycle/file effect.

Sources: [Rust current_exe](https://doc.rust-lang.org/std/env/fn.current_exe.html),
[image-name query](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-queryfullprocessimagenamew),
[mapped-file name](https://learn.microsoft.com/en-us/windows/win32/api/psapi/nf-psapi-getmappedfilenamew),
[sharing/rename contract](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew),
[explicit executable and initialization](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-createprocessw),
[pinned Rust 1.94 native spawn](https://github.com/rust-lang/rust/blob/1.94.0/library/std/src/sys/process/windows.rs),
[Framework main module](https://learn.microsoft.com/en-us/dotnet/api/system.diagnostics.process.mainmodule?view=netframework-4.8.1),
[process-handle/PID lifetime](https://learn.microsoft.com/en-us/windows/win32/procthread/process-handles-and-identifiers),
[module name/truncation](https://learn.microsoft.com/en-us/dotnet/api/system.diagnostics.processmodule.filename?view=netframework-4.8.1),
[pinned unstable attribute API](https://github.com/rust-lang/rust/blob/1.94.0/library/std/src/os/windows/process.rs),
[safe owned-handle conversion/extraction](https://github.com/kotauskas/interprocess/blob/2.4.4/src/os/windows/named_pipe/stream/impl/handle.rs),
[safe metadata-wrapper cleanup](https://github.com/kotauskas/interprocess/blob/2.4.4/src/os/windows/named_pipe/stream/impl/send.rs),
[safe peer-PID queries](https://github.com/kotauskas/interprocess/blob/2.4.4/src/os/windows/named_pipe/tokio/stream/impl.rs),
[Tokio read-only client/SQOS options](https://github.com/tokio-rs/tokio/blob/tokio-1.53.1/tokio/src/net/windows/named_pipe.rs),
[descriptor/first-instance/remote flags](https://github.com/kotauskas/interprocess/blob/2.4.4/src/os/windows/named_pipe/listener/create_instance.rs),
[accept/replacement lifecycle](https://github.com/kotauskas/interprocess/blob/2.4.4/src/os/windows/named_pipe/tokio/listener.rs),
[explicit flush and terminal drop](https://github.com/kotauskas/interprocess/blob/2.4.4/src/os/windows/named_pipe/tokio/stream/impl/send.rs),
[retained async flush worker](https://github.com/kotauskas/interprocess/blob/2.4.4/src/os/windows/tokio_flusher.rs),
[pinned flush error conversion](https://github.com/kotauskas/interprocess/blob/2.4.4/src/os/windows/c_wrappers.rs),
[pinned disconnected-pipe downgrade](https://github.com/kotauskas/interprocess/blob/2.4.4/src/os/windows/misc.rs),
[Rust 1.94 owned handles](https://github.com/rust-lang/rust/blob/1.94.0/library/std/src/os/windows/io/handle.rs),
[Rust 1.94 error-preserving fsync](https://github.com/rust-lang/rust/blob/1.94.0/library/std/src/sys/fs/windows.rs),
[client PID](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-getnamedpipeclientprocessid),
[server PID](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-getnamedpipeserverprocessid),
[metadata access requirements](https://learn.microsoft.com/en-us/windows/win32/api/namedpipeapi/nf-namedpipeapi-getnamedpipeinfo),
[server flush contract](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-flushfilebuffers),
[pipe terminal operations](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-operations),
[existing account boundary](../SECURITY.md#out-of-scope).

### Automatic activation witness and GUI companion proposal (2026-10-03)

Task Scheduler generates InstanceGuid for each task run. It identifies an instance; no primary
contract makes a user-supplied copy of that GUID an origin credential. IRegisteredTask::Run accepts
VT_NULL/VT_EMPTY or BSTR parameters; a single BSTR supplies Arg0, and the returned IRunningTask
describes the instance. Action Arguments and WorkingDirectory admit ArgN substitution. The SDK
also documents a no-error/no-run case when AllowDemandStart is false. Therefore an SDK success,
unique current GUID and manual argument text do not by themselves correlate a new native role.
Sources: [Run parameters and return](https://learn.microsoft.com/en-us/windows/win32/api/taskschd/nf-taskschd-iregisteredtask-run),
[action variable properties](https://learn.microsoft.com/en-us/windows/win32/taskschd/task-actions),
[generated instance identifier](https://learn.microsoft.com/en-us/windows/win32/taskschd/runningtask-instanceguid).

The pinned interprocess 2.4.4 synchronous and Tokio PipeStream APIs expose safe client_process_id,
server_process_id and peer_process_id. The upstream implementation queries the actual borrowed
pipe handle through GetNamedPipeClientProcessId/GetNamedPipeServerProcessId and returns io::Result;
the workspace needs no unsafe shim or dependency-version change. A server can bind the first
authenticated peer to the GUI launcher and the second to its retained OwnedChild.id. A frame's
claimed PID cannot replace this query, and the launcher PID must never be confused with its child.
Sources: pinned crate src/os/windows/named_pipe/{stream,tokio/stream}/impl.rs and c_wrappers.rs,
[pinned Tokio source](https://docs.rs/crate/interprocess/2.4.4/source/src/os/windows/named_pipe/tokio/stream/impl.rs),
[actual named-pipe client PID](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-getnamedpipeclientprocessid).

The proposed authority is one fresh 32-byte OS-random capability delivered only through the sole
SDK Run and live bounded transport, then exchanged against those actual pipe peers and a new
supervisor/control lifetime. First authenticate the real launcher peer/capability, then durably
record the actual SDK GUID; only a timely producer Permit may authorize its retained child.
Permit remaining time is anchored at the GUI's pre-SID entry clock, not at receipt, so it only
shortens admission. A late queued Run without a timely Permit cannot create another child.
The second real pipe peer must match the retained child's PID and actual owned supervisor UUID/
control; a timely matching ACK then mints the supervisor's local opaque value in the existing
finite lifecycle worker. Raw ACK parsing alone remains untrusted and never deserializes that
value. These order/budget gates need native late-ready/queued-Run proof before qualification.
A sealed service-produced optional RuntimeFacts witness holds only
the context/digest/GUID/launcher PID; GUID-only CLI input remains unwitnessed. This follows
SECURITY.md's user-account boundary. It does not claim protection against arbitrary code/debugging
or theft of another same-account process's argv/state. No raw capability is journaled. Already
authenticated live readback can recover; loss before authentication stays pending with zero Run
replay, rather than pretending a later unparameterized Logon proves the original operation.

TaskSettings.Hidden controls visibility in Scheduler UI, not initial process-console allocation.
Run flags supply identity/session/constraint choices, with no console-suppression flag. Rust's
console subsystem can create a new console when no console exists; its windows subsystem runs
detached. Thus a direct action for today's console CLI cannot establish the required no-flash
entry guarantee. Select an internal Windows-GUI-subsystem companion while preserving the console
CLI. Actual mapped subsystem 2, first-entry CONOUT$ absence and code that never attaches/allocates
a console qualify the GUI entry; this must not reinterpret CREATE_NO_WINDOW child behavior,
which retains the earlier final-mask/direct-control tests and may own an invisible console host.
Sources: [Hidden is task UI visibility](https://learn.microsoft.com/en-us/windows/win32/taskschd/tasksettings-hidden),
[Run flags](https://learn.microsoft.com/en-us/windows/win32/api/taskschd/ne-taskschd-task_run_flags),
[Rust subsystem contract](https://doc.rust-lang.org/reference/runtime.html#the-windows_subsystem-attribute),
[MSVC subsystem](https://learn.microsoft.com/en-us/cpp/build/reference/subsystem-specify-subsystem?view=msvc-170).

Raw native `v1:$(Arg0)` remains data whether a no-parameter action leaves an empty substitution or
the literal marker; the exact parser treats only those specified absence forms as unwitnessed.
Primary Run docs do not establish which form genuine Logon supplies. Keep the actual VT_EMPTY,
scheduled no-parameter and later standard-user Logon gates. The prior PowerShell EncodedCommand
launcher cannot safely gain arbitrary trailing argv, and unexpanded Arg0 can break a spliced
EncodedArguments value before its static code runs. The companion avoids making correctness
depend on that unproved decoding path; no dynamic script fragment or Run environment route is
selected. Earlier hidden-PowerShell probe receipts remain historical evidence only.

The current primary WinGet portable archive flow records each top-level extracted entry before creating
links for NestedInstallerFiles; an omitted command alias on a listed executable defaults to its
filename. The proposed manifest therefore lists only the console command as a nested portable
alias and requires the sibling companion in the complete archive/package file index. Schema
permission for multiple portables alone is not proof of no extra link. Validate actual ownership,
upgrade and removal of both images with the selected native client before publication.
Source: [WinGet portable archive file/index/link flow](https://github.com/microsoft/winget-cli/blob/39739564a4aaf1071b17d163ec332a08b9bcf05c/src/AppInstallerCLICore/Workflows/PortableFlow.cpp#L177).

Distribution's current strict six-payload inventory plus receipt is seven managed leaves. The
companion adds one canonical ZIP member and one managed leaf, yielding eight. The held complete
R=E=2 reservation 12*7 + Q10 + S10 + A10 + outer10 + PATH4 reaches all 128 frames; the revised
schedule reaches 140. Keeping 128 would refuse an ordinary daemon-plus-dashboard installation.
The explicit proposed ceiling is 140 frames/18 MiB, with the 128 KiB complete-frame and private
record limits unchanged. Pinned windows_journal includes its 68-byte overhead inside FRAME_LIMIT:
maximum payload is 131,004 bytes. 140 complete maximum frames plus one bounded incomplete tail
occupy (140+1)*(131004+68) = 18,481,152 < 18,874,368 bytes, leaving 393,216 bytes. A partial frame
still cannot authorize effects. Checked actual serialization still covers all old/new
pair bindings, fingerprints, escaped paths and optional witness fields before any effects; these
numbers do not permit extra callbacks or unbounded recovery. Both native images require the same
version/architecture/ABI/source verification, receipt/WinGet ownership, actual holder quiescence
and complete pre-receipt pair rollback. No GUI source, witness implementation or expanded journal
limit has been qualified by this research; the revised complete plan requires parent/Project
review before separate producer, consumer and distribution development.

### Native pipe closure fixture evidence (2026-10-03)

PR #44 revision 0503d55's exact run 37089833448 exercised 71 distribution fixtures on each
native architecture: 69 passed and two transport assertions failed. A client open immediately
after dropping the unused replacement succeeded; raw flush of a zero-byte pipe also succeeded
after dropping its Tokio reader. These observations do not prove a second accepted peer or
authenticated delivery. The pinned interprocess accept creates a replacement before returning
the original stream. Tokio 1.53.1 deregisters on drop, but mio 1.2.2 retains native Arc-owned
handles for queued read/connect/event completions and cancellation does not synchronously
complete them. The same finite runtime owner must retain its admission and launch guards through
confirmed disposal; the caller must not join it indefinitely or treat a late open as authority.

Microsoft's server flush contract concerns buffered bytes; it does not promise failure for an
empty disconnected pipe. The planned meaningful negative fixture therefore connects a safe
receive-only standard File without IOCP/background reads, sends actual bytes, observes the
owned raw flush pending, closes that known reader, and requires its final native error. Single
accept remains enforced by the consumed listener and sole connected stream. Its native fixture
keeps first-instance collision refusal and sole-stream delivery/terminal EOF, while any late
open must receive no qualification bytes. No synchronous-drop, empty-flush, quiet-period or
PID-metadata inference replaces exact frames, actual EOF, live Child and continuous guard proof.
This is an evidence-driven fixture refinement; the actual mapped-helper gate is still unqualified.

Sources: [pinned listener replacement](https://github.com/kotauskas/interprocess/blob/2.4.4/src/os/windows/named_pipe/tokio/listener.rs),
[Tokio deregistration](https://github.com/tokio-rs/tokio/blob/tokio-1.53.1/tokio/src/io/poll_evented.rs),
[mio native completion/drop ownership](https://github.com/tokio-rs/mio/blob/v1.2.2/src/sys/windows/named_pipe.rs),
[asynchronous cancellation](https://learn.microsoft.com/en-us/windows/win32/api/ioapiset/nf-ioapiset-cancelioex),
[server buffered-byte flush](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-flushfilebuffers).


### Mapped-image query input binding (2026-10-03)

PR #44 revision 7871c983, run 37095934375, exercised 93 distribution fixtures on each native
architecture. Jobs 111125738916 (x64) and 111125738927 (ARM64) each passed 92 and failed only
actual_copied_child_guard_overlap_and_terminal_eof_qualify_without_effects, with
`stock Windows adapter failed: No initialized main module filename`. Both logs have zero ignored
cases. The parent had already received the actual Ready frame before the image readback; these
logs do not establish an executable-loader startup race or justify retrying the query.

The fixed Core adapter's caller input is the parsed `$request` object. Its test-only diagnostic
wrapper also keeps raw stdin in the String `$locronInput`. MODULE_QUERY instead reads
`[int]$locronInput.pid`. A String has no pid property: documented PowerShell member access returns
null for a missing property, and conversion to Int32 changes null to zero. The production wrapper
does not define that raw-input variable either. This source-established mismatch therefore selects
PID zero rather than the retained Child's actual PID. Framework reference source identifies zero
as the idle process and rejects its module enumeration. PowerShell property access can also hide
getter exceptions as null, explaining why the script's own missing-filename error loses the
underlying module-query reason. The logs do not contain a sampled query PID; corrected native
qualification remains pending.

Select the existing parsed `$request.pid`, require an actual Int32/Int64 value in the positive
Int32 range before conversion, and invoke the Framework module/filename getter methods explicitly
so their exceptions remain refusals. Preserve one actual Child, live brackets, Ready ordering,
continuous helper/ancestor guards, actual path/full identity/hash comparison and the original
30-second deadline. A real missing module, exit, unsupported query or mismatch still refuses.
No alternate path source, default PID, query retry, sleep or new phase budget is selected.

Sources: [PowerShell 5.1 member and getter semantics](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_operators?view=powershell-5.1#member-access-operator-),
[null-to-integer conversion](https://learn.microsoft.com/en-us/powershell/scripting/learn/deep-dives/everything-about-null#value-types),
[Framework idle-process and module enumeration](https://github.com/microsoft/referencesource/blob/main/System/services/monitoring/system/diagnosticts/ProcessManager.cs#L432),
[Framework MainModule getter](https://github.com/microsoft/referencesource/blob/main/System/services/monitoring/system/diagnosticts/Process.cs#L441),
[MainModule availability](https://learn.microsoft.com/en-us/dotnet/api/system.diagnostics.process.mainmodule?view=netframework-4.8.1),
[process-handle and identifier lifetime](https://learn.microsoft.com/en-us/windows/win32/procthread/process-handles-and-identifiers).

### GUI companion entry and bounded probe refinement (2026-10-03)

Microsoft documents CONOUT$ as the active console-screen-buffer device even when ordinary
stdout is redirected. Its CreateFileW console contract requires an attached process but does
not guarantee an exact error for an unattached caller. OPEN_EXISTING's generic missing-device
raw-2 rule cannot settle that console-specific behavior. GUI processes start without console
attachment, whereas an explicit CREATE_NEW_CONSOLE control can give a console process a separate
console. Preserve raw facts as opened:boolean/error:i32|null, success/null versus failure/nonzero;
never convert a failed open into an attachment assertion. Missing raw code/zero errors refuse
probe output. Actual PE subsystem 2/3, native GUI/DETACHED_PROCESS absence-code controls and a
real successful console control remain independent hosted gates; unrelated permission/sharing
errors cannot qualify headless operation. The exact absence code remains a native qualification
fact, not an API guarantee.
Sources: [Console device handles](https://learn.microsoft.com/en-us/windows/console/console-handles),
[OPEN_EXISTING and sharing](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew),
[GUI and explicit console creation](https://learn.microsoft.com/en-us/windows/console/creation-of-a-console).

Cargo required-features can keep the companion out of ordinary builds, and default-run preserves
the existing console selection when the explicit companion feature is present. Rust's
windows_subsystem attribute selects the Windows GUI image without changing the primary command.
The proposed sole --version line and exact <=4 KiB locron.windows-launcher-probe/v1 JSON object
are bounded read-only metadata/entry facts, not file ownership or automatic activation proof.
One owned worker contains potentially blocking console-open/stdout work under the pre-worker
30-second entry deadline; expiry cannot join unfinished I/O or infer a timely success from
queued bytes. Native classification, timeout and package-pair qualification are still pending.
Sources: [Cargo target feature admission](https://doc.rust-lang.org/cargo/reference/cargo-targets.html#the-required-features-field),
[Default command selection](https://doc.rust-lang.org/cargo/reference/manifest.html#the-default-run-field),
[Rust Windows subsystem](https://doc.rust-lang.org/reference/runtime.html#the-windows_subsystem-attribute).

Rust 1.94 process::exit calls runtime cleanup. Stdout cleanup uses try_lock, but when a line
buffer is available it replaces/drops that buffered writer; a remaining payload can therefore
be flushed outside the expired worker. Merely containing StdoutLock in the worker is insufficient
for the probe's rule against fresh write/flush admission or timely success after expiry. An
already queued native write may finish late, and its bytes remain uncertain. Select safe
Stdout::as_handle, BorrowedHandle::try_clone_to_owned and File::from(OwnedHandle), all stable
since 1.63, for unbuffered owned output.
The audited clone uses non-inheritable DuplicateHandle with the same access. Null stdout is an
explicit supported borrowed-handle case; cloning it does not prove writable output, and actual
File write failure must refuse. All duplicate/write/close work stays in the finite owner; this
does not promise cancellation of an already-queued write or bypass ordinary Windows process exit.
Native blocked-I/O and late-ready output qualification is still pending.
Sources: [Rust exit and cleanup](https://raw.githubusercontent.com/rust-lang/rust/1.94.0/library/std/src/process.rs),
[buffered stdout cleanup](https://raw.githubusercontent.com/rust-lang/rust/1.94.0/library/std/src/io/stdio.rs),
[safe owned Windows handle conversions](https://raw.githubusercontent.com/rust-lang/rust/1.94.0/library/std/src/os/windows/io/handle.rs).

### WinGet version-root index feasibility (2026-10-03)

The pinned archive flow uses nonrecursive `directory_iterator`, records a Directory entry for
an extracted top-level directory, and later records the nested console alias. InstallFile moves
or recursively copies that directory; ApplyDesiredState indexes the same desired entries without
expanding them into child File rows. Our five-member ZIP has one versioned root directory.
Consequently this source does not establish separate console/launcher File rows or their hashes
in the portable index. The earlier top-level-entry finding must not be read as recursive file
hash coverage. A verifier requiring both executable File/hash rows would refuse this layout.
Keep that refusal until a reviewed selection qualifies directory-index ownership together with
the exact canonical pair, or changes the Windows ZIP/manifest layout. No layout, ownership
guarantee or portable lifecycle hook is changed by this research. Actual selected-client
install/upgrade/remove index readbacks remain mandatory before a WinGet support claim.

PortableInstaller selects `<ARP product code>.db` inside registered InstallLocation. Its schema
1.0 has a `portable` table with path/type/hash/link-target fields; File is type 1 and Directory is
type 2. The hash column's declared BLOB affinity does not prove binary hash storage: the pinned
wrapper binds the PortableFileEntry string as SQLite TEXT. Inspect actual storage type and
selected-client metadata/version rather than coercing arbitrary rows into executable ownership.

An exact-byte, safe Rust reader is feasible without reopening the index through SQLite. The
already locked rusqlite 0.40.2 `serialize` feature provides safe
`deserialize_read_exact(MAIN_DB, bounded_slice, slice.len(), true)` on an in-memory connection.
First retain existing-only source-policy ancestry/leaf guards and full identity, read a bounded
complete byte snapshot, then parse that slice and retain the live guard. This avoids unsafe
OwnedData construction and SQLite path/URI races. Parse only fixed ordinary-table queries with
finite size/row/column/work limits under the original admitted deadline; no SQL comes from the
index. WAL-mode main bytes are not directly deserializable. Refuse WAL/SHM/journal sidecars and
WAL header versions rather than rewriting their bytes or silently discarding committed state.
The safe API and schema are source-feasible candidates, not native client qualification or
effect authority; dependency features and precise parser bounds require plan/Project review.

Sources: [nonrecursive archive inventory](https://github.com/microsoft/winget-cli/blob/39739564a4aaf1071b17d163ec332a08b9bcf05c/src/AppInstallerCLICore/Workflows/PortableFlow.cpp#L177),
[directory installation and index writes](https://github.com/microsoft/winget-cli/blob/39739564a4aaf1071b17d163ec332a08b9bcf05c/src/AppInstallerCLICore/PortableInstaller.cpp#L127),
[index filename](https://github.com/microsoft/winget-cli/blob/39739564a4aaf1071b17d163ec332a08b9bcf05c/src/AppInstallerCLICore/PortableInstaller.h#L67),
[portable table](https://github.com/microsoft/winget-cli/blob/39739564a4aaf1071b17d163ec332a08b9bcf05c/src/AppInstallerRepositoryCore/Microsoft/Schema/Portable_1_0/PortableTable.cpp#L19),
[file types and hash string](https://github.com/microsoft/winget-cli/blob/39739564a4aaf1071b17d163ec332a08b9bcf05c/src/AppInstallerCommonCore/Public/winget/PortableFileEntry.h#L9),
[actual string binding](https://github.com/microsoft/winget-cli/blob/39739564a4aaf1071b17d163ec332a08b9bcf05c/src/AppInstallerSharedLib/SQLiteWrapper.cpp#L64),
[safe bounded-slice deserialization](https://docs.rs/rusqlite/0.40.2/rusqlite/struct.Connection.html#method.deserialize_read_exact),
[SQLite in-memory/WAL contract](https://www.sqlite.org/c3ref/deserialize.html).

### WinGet indexed-directory and canonical-pair proposal (2026-10-03)

The selected validator's official v1.29.380 tag resolves to commit
000f6b55151cb0f1afd2933bb54c62a4724b9ca8. Its archive flow also indexes top-level entries,
including our versioned root Directory, then the sole console Symlink. Its installer moves or
recursively copies that directory; upgrade removes previous indexed entries before installing
the next ones, and removal recursively removes the indexed Directory. This supports retaining
the five-member version-root ZIP and one console alias. It does not supply per-image index
hashes. Directory index ownership and canonical-release byte integrity are separate facts.

Propose binding the unique current-SID HKCU package/source/version/location registration to its
existing `<product code>.db`, one exact Directory row for the version/target root, and the sole
console alias row pointing at that root's console. Require the Directory's empty hash/target
fields and strict ordinary-table schema 1.0; reject duplicate, foreign, stale or extra portable
entries, including a GUI alias. Keep existing-only ancestry and exact index/source file guards.
Independently enumerate exactly the five regular release leaves and verify all five against the
strict canonical ZIP, both live executable identities, PE2/3, native architecture, version and
launcher ABI. An indexed Directory alone cannot authenticate nested bytes or adopt an incomplete
pair. WinGet's legitimate failed-symlink PATH fallback is not mapped-image or activation proof.

The safe in-memory index parser can reuse locked rusqlite 0.40.2 with Windows-only serialize,
limits and hooks features; no new package version or workspace crate is required. Propose a
4 MiB complete main-file snapshot, at most eight metadata rows and exactly two portable rows,
16 KiB SQL/value bounds, eight columns, no attached databases and at most 1,024 progress
callbacks at the configured 1,000-instruction interval; the interval is not an exact opcode or
wall-clock guarantee. Fixed queries inspect storage types and ordinary tables; defensive mode
and memory-only temp storage are enabled and trusted schema is disabled. Its progress hook checks
the original caller deadline. WAL/header and sidecar refusal remains explicit. Native guard,
read and parse work stays inside one retained finite owner, including uncertain cleanup; no
fresh timeout or caller join reconstructs authority.
The proposed parser bounds and ownership contract require parent/Project review before source.

Verify with genuine selected-client native x64/ARM64 install/upgrade/remove readbacks of the
directory index, complete pair, console alias and absent GUI alias. Pure fixtures must refuse
wrong row type/schema/storage, path/hash/source/version mismatches, missing/extra leaves,
sidecars and over-budget work. A release-source audit or copied row DTO is not this live proof.

Sources: [official v1.29.380 release](https://github.com/microsoft/winget-cli/releases/tag/v1.29.380),
[released archive inventory](https://github.com/microsoft/winget-cli/blob/000f6b55151cb0f1afd2933bb54c62a4724b9ca8/src/AppInstallerCLICore/Workflows/PortableFlow.cpp#L197),
[released directory/upgrade/removal behavior](https://github.com/microsoft/winget-cli/blob/000f6b55151cb0f1afd2933bb54c62a4724b9ca8/src/AppInstallerCLICore/PortableInstaller.cpp#L111),
[released schema fields](https://github.com/microsoft/winget-cli/blob/000f6b55151cb0f1afd2933bb54c62a4724b9ca8/src/AppInstallerRepositoryCore/Microsoft/Schema/Portable_1_0/PortableTable.cpp#L22),
[safe deserializer/config/progress APIs](https://docs.rs/rusqlite/0.40.2/rusqlite/struct.Connection.html),
[safe runtime limits](https://docs.rs/rusqlite/0.40.2/rusqlite/limits/index.html),
[VM progress callback semantics](https://www.sqlite.org/c3ref/progress_handler.html).


### Portable-index value and encoded-row bounds (2026-10-03)

SQLite's SQLITE_LIMIT_LENGTH constrains the complete encoded row as well as any individual
String/BLOB. A 16 KiB setting therefore rejects a Symlink row containing two individually allowed
paths whose combined encoding exceeds 16 KiB. Keep the approved per-value and SQL limits at
16,384 bytes and select a separate finite encoded-row ceiling of 65,664 bytes:
`4 * 16,384 + 128`. Enforce every returned TEXT/BLOB value separately before copying it.

For the four-column portable record, even four maximum-size values plus a conservative
nine-byte size varint and four nine-byte serial-type varints fit the selected ceiling. The fixed
five-field schema query has at most four bounded text values, one eight-byte integer and six
nine-byte header varints: its extra 62 bytes also fit the 128-byte allowance. Seven-field column
introspection has at most three bounded text values and four eight-byte integers plus eight
nine-byte header varints; its `3 * 16,384 + 104` ceiling is smaller still. Other fixed metadata
and index queries return fewer bounded fields. This is no permission to accept arbitrary
eight-column rows of eight maximum-size strings; unknown schema/shapes continue to refuse.

Verify a valid two-path row above 16 KiB total with each field within its original bound, and
refuse any single over-limit value or unsupported row/schema. The 4 MiB snapshot, eight metadata
rows, exact two ownership rows, eight-column cap, zero attached databases, 1,024 callbacks at a
1,000-instruction interval and original absolute deadline remain fixed. No larger source input,
new clock, SQLite path reopen or live SID/package/source authority follows from this allowance.

Sources: [SQLite complete-row length limit](https://www.sqlite.org/c3ref/c_limit_attached.html),
[encoded record headers and integer sizes](https://www.sqlite.org/fileformat2.html#record_format),
[safe rusqlite limits](https://docs.rs/rusqlite/0.40.2/rusqlite/limits/index.html).

### Bootstrap raw path refusal before Framework normalization (2026-10-03)

At PR #44 revision 5041e392, native x64 job 111141838176 and ARM64 job 111141838133
each passed all 105 distribution tests, including the corrected held-child gate and nine index
cases. Their next stock PowerShell 5.1 bootstrap step failed with `fixture failed: ambiguous path`.
The fixture uses the same message for five original relative, UNC, alternate-stream, trailing-dot
and reserved-device inputs. Neither completed log identifies the input that was accepted.

ConvertTo-LocronPath checks absolute-drive syntax, controls and alternate streams before calling
Framework Path.GetFullPath, but checks component ambiguity only on the returned string. Microsoft
documents that this normalization evaluates dot/parent segments and removes certain ending dots
and spaces. Framework reference source routes GetFullPath through NormalizePath; its legacy
branch explicitly drops terminal dots/spaces and handles dot segments. The active Framework
compatibility branch is not recorded in these logs. This is a source-established ordering gap:
normalizing first can erase the raw spelling that the later ambiguity check needs. `C:\locron.`
is a documented candidate, not a separately observed failing input or a reason to relax the test.

Check every original component before GetFullPath, treating slash and backslash as equivalent
separators only for this inspection. Refuse dot/parent components, terminal dots or spaces and
the existing reserved-device forms, retaining the existing prechecks and post-normalization
component check. Accept ordinary internal dots/spaces, leading-dot names, Unicode, drive roots
and allowed separator normalization. No trim or Framework rewrite can convert a refused input
into an accepted installation/removal path; valid normalized text still supplies no live identity.

Keep all five original refusal assertions, add case-labelled raw ambiguity and ordinary-path
fixtures, and generate the identical uninstall check through the existing renderer. Local
verification is stock 5.1 AST parsing plus static renderer/Python checks only. Actual x64/ARM64
bootstrap requalification remains pending; no execution-policy or filesystem/registration gate
is changed.

Sources: [Windows normalization, relative components and character trimming](https://learn.microsoft.com/en-us/dotnet/standard/io/file-path-formats#path-normalization),
[Framework GetFullPath and normalization dispatch](https://github.com/microsoft/referencesource/blob/main/mscorlib/system/io/path.cs#L301),
[Framework terminal dot/space handling](https://github.com/microsoft/referencesource/blob/main/mscorlib/system/io/path.cs#L681),
[Windows trailing-character and reserved-name rules](https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file#naming-conventions).

### Atomic private bootstrap file ownership (2026-10-03)

PR #44 at 60e2fcc passed all 105 distribution cases on each native x64/ARM64 MSRV package row
and then failed its stock PowerShell bootstrap with the existing foreign-owner refusal at
install.ps1:52. The completed logs record no actual owner SID, object path
or detailed caller stack, so it does not establish an Administrators-owned leaf or a particular
failed object. Keep that measurement gap explicit rather than weakening the refusal.

The source establishes a fresh-file creation gap: New-LocronPrivateDirectory supplies an explicit
current-user owner and protected current-user/SYSTEM DACL, but Write-LocronPrivateFile uses plain
File.Open(CreateNew) before asserting the returned handle's owner. Windows selects a new object's
default owner from the creating token; inherited access rules do not independently bind that
owner to the current user. An elevated runner's different default owner is therefore a plausible
explanation, not a measured diagnosis of this run.

Stock .NET Framework provides the seven-argument FileStream constructor accepting FileSecurity.
Its reference implementation passes the descriptor to the native creation call; the existing
Core filesystem worker already uses that overload for fresh private files. Supply the current
SID owner, protected DACL and exactly current-user/SYSTEM FullControl before CreateNew. Preserve
exclusive FileShare.None, the strict post-creation handle check, bytes/Flush(true)/finally, parent
validation, collision refusal and no existing-object adoption or ACL repair. FullControl on the
new private handle also admits its descriptor readback; it grants no additional principal access.

The uninstaller renderer does not select Write-LocronPrivateFile, so the generated uninstaller
must remain exact. Retain every existing positive Unicode-path write/read/hash, receipt roundtrip,
collision, unsafe ACL/owner/junction refusal and bounded owned cleanup assertion. Add only fixed
stage labels before/after the existing positive private-root and private-leaf creation so a future
native refusal can be located without logging paths or SIDs. Static review/renderer checks cannot
qualify filesystem behavior; fresh exact-head native x64/ARM64 stock-5.1 bootstrap is required.

Sources: [Windows default owner selection](https://learn.microsoft.com/en-us/windows/win32/secauthz/owner-of-a-new-object),
[CreateFile security descriptor at creation](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew),
[Framework FileStream security overload](https://learn.microsoft.com/en-us/dotnet/api/system.io.filestream.-ctor?view=netframework-4.8.1#system-io-filestream-ctor(system-string-system-io-filemode-system-security-accesscontrol-filesystemrights-system-io-fileshare-system-int32-system-io-fileoptions-system-security-accesscontrol-filesecurity)),
[Microsoft Framework implementation](https://github.com/microsoft/referencesource/blob/main/mscorlib/system/io/filestream.cs).

### First-run empty database observation and creation handoff (2026-10-03)

PR44 4507d5e's completed MSRV job 111166508166 fails the original first-run registered-daemon
fixture with `stage=database-create-new`, I/O category and native error 32. Its full tree is
identical to 38d3c191, whose subsequent CI37110977454 and all 19 checks pass. That later pass does
not close the source-established overlap. The earlier log does not identify the competing holder.

The stage encloses all of Core create_private_new: the worker atomically creates an empty private
file, disposes its Framework handle, and Rust then opens the guarded read/write handle. Worker
stderr failures become Error::other without a raw OS code; only explicit collision 80/183 replies
are mapped back to raw codes. Error 32 is compatible with the Rust reopen, rather than evidence
that the Framework CreateNew call failed. Microsoft's CREATE_NEW contract returns existing-file
error 80, and MS-FSA's FILE_CREATE collision precedes ordinary existing-stream share checks.
Reducing the worker's FullControl request would not close the subsequent handle-handoff gap.

The original wait_initialized_store observer opens read-only whenever the database leaf exists,
then reads settings. Store currently admits an empty private database through its no-journal,
immutable branch and returns a retained FILE_SHARE_READ gate. Before settings rejects the empty
schema, that gate can exclude the creator's read/write reopen. This is a source-supported
mechanism; the failing run's exact holder and interleaving remain unmeasured.

Use an existing-only, guarded ordinary READ handle with READ/WRITE sharing before attempting a
write-excluding gate. Refuse a zero-length database as initialization pending; retain that
preflight handle through full file-identity comparison with the original first admitted handle.
This check does not create, migrate, repair, retry or declare a nonempty file ready. Keep every
original stable/ordinary-WAL branch and journal/ACL/reparse/identity refusal.

The pinned SQLite 3.53.2 WAL transition initializes the first main page through the header-version
rollback transaction before Locron migrations run (OP_JournalMode, sqlite3BtreeSetVersion and
newDatabase). Its pagerOpenWalIfPresent zero-page branch does not admit a zero-byte main file as
ordinary committed-WAL state. Thus this refusal covers the pre-initialization handoff, not valid
closed snapshots or live committed WAL. External truncation is not a supported readiness proof.

The new regression must use a real private zero-byte file and the public read-only Store API,
retain bytes/full identity/ACL and journal absence, and then actually initialize a writable Store
at the same path. Preserve the original closed snapshot writer-exclusion, live WAL later-commit,
partial/broad-sidecar and final-close tests plus the unchanged lifecycle deadline. Native hosted
qualification is required; a static argument or a same-tree green rerun is not regression evidence.

Sources: [CREATE_NEW and Windows sharing](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew),
[MS-FSA existing-stream FILE_CREATE collision](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-fsa/41f3734a-5bba-4c3b-9d04-7baafc9b7bfe),
[SQLite immutable URI behavior](https://sqlite.org/uri.html),
[SQLite read-only WAL](https://sqlite.org/wal.html#read_only_databases).
The selected bundle's concrete implementation is libsqlite3-sys 0.38.2 sqlite3.c, SQLite 3.53.2;
research preserves exact local source anchors without introducing a dependency update.

### Windows dashboard wake diagnostic parity (2026-10-03)

At merged main 1441b7b, Core WakeFacts deliberately reports transport named_pipe,
socket_present None and availability unprobed on Windows. The diagnostics API serializes the
legacy wake_socket as null and adds those passive facts under wake. Unix keeps its established
filesystem-presence boolean without the additive object. The read never connects to a pipe,
sends a hint or creates state; transport selection is not endpoint-health evidence.

The Diagnostics route still declares wake_socket as a boolean and uses its truthiness for the
present/absent badge. Consequently the actual Windows payload renders Wake socket: absent,
discarding the named-pipe/unprobed facts. The generic JSON API client performs no conversion
that could correct this mismatch. This is an observed display defect within #29's existing
accurate-platform-facts criterion, not a new protocol or product-scope decision.

Consume the existing nullable field and optional passive wake object in the route. Preserve
both Unix boolean results, display named-pipe transport with unprobed availability on Windows,
and render missing/unsupported facts as unknown rather than inventing absence or availability.
No backend, IPC, permission, state-discovery or diagnostics polling change is required.

Evidence: crates/locron-core/src/notification.rs WakeFacts/wake_facts and its missing-state test;
crates/locron-server/src/api.rs diagnostics response; frontend/src/routes/Diagnostics.tsx and
frontend/src/api.ts; the root acceptance audit recorded on #29 before implementation.

### Windows PATH entries can discard the job working directory (2026-10-03)

At main b6b98eb, execution::resolve_executable rejects a drive-relative or root-relative
executable request, but applies cwd.join to every non-absolute PATH directory. Rust's Windows
PathBuf::push/Path::join contract states that a prefix without a root replaces the base;
therefore a PATH entry such as C:bin remains drive-relative after that join. The subsequent
filesystem lookup can select a program through the process's ambient drive directory instead
of the job's explicit working directory. A root without a prefix also discards the job directory,
retaining only its drive prefix. The existing mixed-case/Unicode resolution test does not cover
either PATH-entry form.

The frozen SPEC already requires resolution from the effective configured PATH/PATHEXT and
working directory. Apply the executable request's ambiguity rule to each PATH entry before
filesystem lookup. Skip an ambiguous entry while retaining later usable entries, as for other
unusable search locations; refusing the entire list would add a new configuration/error policy.
Ordinary relative entries, including empty entries, keep their existing job-cwd meaning;
absolute local/UNC/verbatim entries keep their existing meaning. This does not change argv,
shell selection, PATHEXT order, environment precedence or Unix resolution.

Primary evidence: [Rust PathBuf::push](https://doc.rust-lang.org/std/path/struct.PathBuf.html#method.push)
and [Path::join](https://doc.rust-lang.org/std/path/struct.Path.html#method.join).
Code evidence: crates/locron-core/src/execution.rs resolve_executable at main b6b98eb;
execution scope remains the existing Windows environment/resolution issue #26. The isolated
native regression must demonstrate the actual ambient candidate and selected job candidate;
static path reasoning alone is not native execution evidence.

### Native Windows package toolchain evidence (2026-10-03)

At main `00e1006b817b5442fbc9b5b79e1b736d439661a3`, completed CI
[37129629579](https://github.com/WhiteKiwi/locron/actions/runs/37129629579) passes the existing
native matrix. Both downloaded paired package verification records report actual `rustc 1.98.0`,
although the package job installs 1.94.0. The checkout's rust-toolchain.toml selects 1.98.0 and
the package job does not set RUSTUP_TOOLCHAIN. The release Windows build has the same selection
gap. Native foundation MSRV coverage is separately explicit and does not qualify the release
compiler. ZIP hashes, PE roles/imports and finite native version/GUI identity probes still match;
these are development candidates rather than public immutable release assets.

[Rustup override precedence](https://rust-lang.github.io/rustup/overrides.html) places the
RUSTUP_TOOLCHAIN environment override before the checkout toolchain file, which takes precedence
over the default toolchain. Thus installing/selecting a default does not enforce the intended
compiler inside this checkout. Set the Windows package/build override explicitly and reject
unexpected compiler release or native host before building. Existing Unix build and macOS signing
selection remain unchanged.

The same CI has selected native component and package gates, but not Windows all-target workspace
tests or all-target warnings-denied Clippy. Run those full commands as explicit manual discovery
on disposable native x64/ARM64 runners, preserving failures and original tests. Discovery results
cannot complete clean Windows 11 standard-user installation, task/reboot, two-user access,
public unsigned download, update/recovery or WinGet acceptance.

### Full native test compilation prerequisites (2026-10-04)

Reviewed PR130 head63473f1 has successful ordinary CI37132853263, including actual Rust1.94
on both native package rows. Separately, full discovery37132861132 failed all five native rows
during compilation/lint, before any full runtime test body executed. x64 stable identifies
unguarded Unix permission imports in service.rs:343/605; ARM64 stable and x64 MSRV identify
dashboard.rs:391/392. Both Clippy rows reject missing crate docs in existing Unix-only crates.
Keep complete failure logs and do not infer full supported behavior from selected-suite success.

The separate research session reproduced the documentation failure with native Rust1.94 and
1.98 using standalone metadata-only compilation, without linking or executing app/test code.
Putting unchanged crate documentation before the existing crate-level cfg fixes that failure;
both the minimal repro and the actual service_lifetime body pass warnings-denied metadata
compilation. The research inventory finds four affected crates: attempt_history,
crash_boundaries, global_environment and service_lifetime. Their bodies, test names and existing
Unix selection can remain byte-identical. A module wrapper would unnecessarily change test
namespaces. The [Rust Reference's crate-level cfg rule](https://doc.rust-lang.org/reference/conditional-compilation.html#the-cfg-attribute)
explains that attributes before a false crate cfg survive and attributes after it are removed.

The three permission blocks seed positive dashboard owner-only-token fixtures, not Unix-specific
behavior. Existing core::filesystem::create_private_new creates missing private parent components
and an exclusive new private leaf under validated guards: Unix creation uses 0700/0600 and
Windows performs private creation rather than repairing an existing broad descriptor. Use that
existing API for these seed bytes, flush and release the guard before starting the subprocess;
do not create a broad directory/file first and chmod, skip Windows, or soften owner_only/no-secret
assertions. This changes test setup only. Actual Windows fixture acceptance still needs the next
native run; metadata and source inspection cannot qualify private creation or subprocess output.

Other recorded warnings and legacy shell/state fixture assumptions remain discovery findings.
The scoped compilation correction cannot establish full lint/runtime success, Windows11
standard-user installation, active-crash recovery, two-user privacy/IPC or public distribution.

### Measured Windows fixture interference and return visibility (2026-10-04)

Separate read-only research pins7ad2256 and compares the relevant unchanged blobs at543e823.
Full37135594179 has256/16 CLI unit results on all three native rows. Later37135898657 at543e823
has264/15 x64 stable and263/16 ARM64/MSRV after additional unrelated main tests. The difference
is the uncertain launch-owner case passing on x64; other owner-negative cases still fail at
initial admission, while the positive actual copied-child case passes. No failing cleanup
boundary or production owner-reset defect is established by those admission failures.

MCP fresh_paths uses the already-created TempDir as its managed root. Eleven successful-tool
cases fail at initial locron_add_job, with captured directory-root/PermissionDenied diagnostics.
Production private creation verifies existing roots instead of repairing broad descriptors.
Retaining TempDir and creating a missing private Windows child fixes the fixture prerequisite;
Unix setup and all mutation/dry-run/report assertions can remain unchanged. No individual
owner/ACE or 8.3 path cause is established by the captured error category.

The Gate's complete caller inventory finds five parent fixtures sharing process-wide OWNER.
The actual copied-helper selector enters a separate process, and transport uses a different
owner atomic. A narrow test-only mutex around those five parent proofs, acquired before setup
and deadline birth, preserves the uncertain case's actual internal second-admission refusal.
Never clear OWNER for isolation; poison or retained ownership must remain a failure.

The CLI description test identifies missing metadata on the hidden Windows supervise role
field; add its field description without excluding hidden commands or changing parsing.
Seven unused service_backends helpers only serve the existing macOS/Linux consumers. Gate
those exact helpers, related imports and cleanup impl consistently; retain the crate's two
Windows exclusion-reporting cases and every Unix body. Full543 Clippy additionally reveals29
staged CLI diagnostics beyond those seven; this correction cannot claim full lint completion.

Ordinary543 CI37135898238 x64 stable also has129/130 Core results: the isolated guard-stall
child times out at loader_tests.rs273. Caller-failed is recorded at33001ms, before its external
reply deadline of original+4s; the actual return/send interval is unmeasured. windows.rs joins
only a finished worker, then test-only trace.report synchronously prints before returning.
There is no evidence of an unconditional blocked-worker join. Add bounded in-memory timing
for result computation, diagnostic entry/exit, function return and external send/receive under
the unchanged30+3s/30+4s clocks. Emit only after delivery/cleanup or in the original failure
message, with fixed event names/counters. This observes the unresolved boundary; it does not
select a fix, suppress diagnostics, reset permits or increase any budget.

Research artifacts: windows-runtime-discovery-triage-20261004/report.md and measured-evidence.json
under the retained task review directory. They include all25 MCP fixture callers, complete
Gate/transport owner inventories, six compared blobs and the raw Core log SHA256
486f9ef7c416a10a5dc1ef0358623cfaf84c2b7552b73d7c1bb9b35daab23874.

### Stock parent-crash heartbeat publication and missing failure observations (2026-10-04)

PR #130 head `99ea5c97ce925f41755fa86dfa54eeffb97313bb`, based on main
`d56dcbbbb20a58c68a81d6348a54a77c87488e3d`, has fifteen passing ordinary rows and one
failed x64 MSRV foundation row in [CI run 37139545888](https://github.com/WhiteKiwi/locron/actions/runs/37139545888).
[Job 111250945393](https://github.com/WhiteKiwi/locron/actions/runs/37139545888/job/111250945393)
checks out merge `495bfd5817dc25deb8f03219ee5fbfc5359c4988`. Its step
`Verify isolated stock adapter policy and parent containment` passes the fresh EOF-release and
Restricted drivers, then fails `windows::loader_tests::owned_loader_fixture_child` in
parent-exit-driver mode after 45.01 seconds at `windows/loader_crash.rs:107:36`:
`Custom { kind: TimedOut, error: "stock Windows adapter deadline elapsed" }`.
The original cold Core command had already passed 130/130 tests. Later independent stock
startup measurements use different owned PIDs and cannot supply the failed driver's receipt.

Source parity was checked before this documentation handoff. The complete `crates/locron-core`
tree is `857adf2f06476ec0dd9143aff386ebd5d749748f` at the earlier reviewed head
`543e82392754c2e2a1286ae7ae8e22c9c3f89f95`, current PR head and actual CI merge. The two
relevant fixture blobs are `7749964fdc4e3e2b70b87d957fe0e203eae01638` for loader_crash.rs and
`3899d49738fde8e08d011dc0b1b3d3bae22c3bb6` for loader_crash_host.ps1. All eleven existing
PR-owned blobs also match that earlier head. Thus this failure does not establish a new
production loader change or the cause of the separate earlier cold native_guard_stall timeout.

Line 107 is the original shared deadline check inside `beats`, which waits for each counter to
parse as u64. The driver calls it at five different stages. Reaching it follows the required
`three-live-handles` marker check, but this stack does not identify which call/counter failed,
whether the host had been killed, whether all three target handles had signalled exit, or the
counter's final bytes. There is no saved phase/counter observation in that panic. The helper's
private stdout/stderr files are not printed by this failure path; the temporary directory is
removed during unwind. This failed MSRV row has no uploaded native diagnostic artifact in the
run inventory. The actual file bytes and last completed phase are unavailable, so an empty
counter is a hypothesis for this CI failure, not an observed fact. The deadline could also
have expired before a particular read began.

An independently established fixture defect makes that hypothesis plausible. The Rust
`heartbeat` writes the final native-heartbeat path with `fs::write` every 25 ms. The generic
PowerShell loop writes generic-heartbeat with `[IO.File]::WriteAllText` at the same interval.
Both truncate the published file before completing its new digits. The installed Rust 1.94
standard-library source (`library/std/src/fs.rs:419`) implements fs::write with File::create
followed by write_all. During the separate PR #133 investigation, a real native Rust writer was
observed in this window, stopped, killed and reaped; its final counter stayed empty. The retained
portable receipt (`pr133-heartbeat-write-repro.rs` and `.py`) reports actual_std_fs_write,
stopped_between_truncate_and_write, empty_counter_after_hard_stop and owned_process_reaped as
true. A separate real-process snapshot regression preserved the old complete counter 7 after
killing the unpublished candidate, then published complete counter 8. These macOS observations
establish the write-window hazard and proposed staging principle, not Windows Job/replacement
qualification or this failed job's exact cause.

The permitted Rust implementation needs no dependency change. Cargo.lock selects tempfile
3.27.0 (checksum `32497e9a4c7b38532efcdebeef879707aa9f794296a4f0244f6f69e9bc8574bd`), already
a Core dev-dependency. Its checked source, src/file/mod.rs:170-204/220-276/898-904 and
src/file/imp/windows.rs:92-121, provides a same-directory candidate, explicit close through
into_temp_path, no-clobber initial publication and replacement through MoveFileExW. The Windows
implementation sets only MOVEFILE_REPLACE_EXISTING for replacement; it has no cross-volume copy
fallback. [TempPath::persist](https://docs.rs/tempfile/3.27.0/tempfile/struct.TempPath.html#method.persist)
documents atomic replacement, but no file/directory synchronization. persist_noclobber's general
portable contract is weaker; the verified Windows implementation uses a single no-overwrite
move. Candidate flush/close and process-termination tests must not be described as power-loss
durability or a portable atomicity guarantee for initial publication.

Stock Windows PowerShell 5.1 can use the .NET Framework two-argument File.Move for the first
completed candidate and File.Replace(candidate, destination, null, false) thereafter.
[Microsoft's Framework reference source](https://github.com/microsoft/referencesource/blob/ec9fa9ae770d522a5b5f0607898044b7478574a3/mscorlib/system/io/file.cs#L1221)
maps those calls to MoveFile and ReplaceFile and propagates native errors. The
[File.Replace contract](https://learn.microsoft.com/en-us/dotnet/api/system.io.file.replace?view=netframework-4.8)
requires an existing destination; passing false retains metadata-error refusal. The
[ReplaceFileW contract](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-replacefilew)
requires one volume and combines replacement in one API, but documents failure states that can
change the destination's name. Its WRITE_THROUGH flag is unsupported. Consequently this design
must fail on replacement errors, without delete+move, fallback creation, guessed counters or
claims that every failed replacement preserves the old path. Successful publication and an
interrupted publisher require fresh native proof on the actual Framework/filesystem.

Windows sharing also matters. ReplaceFileW opens the destination with delete access and the
candidate without sharing, so the candidate must already be closed and a live destination reader
must permit delete sharing. The actual counter reader uses ordinary Rust File::open through
read_to_string; the installed Rust 1.94 Windows OpenOptions source defaults to
FILE_SHARE_READ|FILE_SHARE_WRITE|FILE_SHARE_DELETE (`library/std/src/sys/fs/windows.rs:206`).
This is separate from the existing deliberate share_mode(0) stock executable guard, whose
refusal/release assertions must remain unchanged. The fixture modules and embedded host script
are reached only from the cfg(test) loader_crash/loader_tests declarations in windows.rs:29-34.
No product loader, owned-child, guard or admission source is needed for this handoff. The
[fixture correction plan](planning/WINDOWS_STOCK_CRASH_HEARTBEATS_2026-10-04.md) preserves the
original budgets and all actual-process assertions; owning Issue #31 and fresh native Verify
remain required before any support or acceptance claim.

### MCP native process fixture after private-root discovery (2026-10-04)

Reviewed c676e37baeb8049fae3eb65cf007bb397850253d passes all sixteen ordinary rows and
GitGuardian in run37143494890. Original five-row full discovery37143508598 still fails:
x64 stable job111262652892, ARM64 stable111262652865 and x64 Rust1.94 job111262652832 each
execute301 CLI unit cases with291 passed/10 failed and zero ignored/filtered. The five
owner-sensitive Gate fixtures, recursive argument description and separate shell at-schedule
case now pass in all three logs. The remaining ten failures are MCP cases, all at request_tool's
unchanged success assertion1855, reporting only `tool locron_add_job failed`. Their actual
tool error envelope is not printed; a specific returned error is not an observed fact.

Separate read-only research pins the MCP source and traces a common invalid fixture input.
valid_add_args1859–1870 supplies process command `/bin/echo`, `hello`. parse_target877–879
retains that executable and args; tool_add_job1021 invokes definition.validate, which reaches
target.rs120–129. On Windows a separator-containing executable must be absolute, including
its prefix. `/bin/echo` is rooted without a drive/UNC prefix and fails that requirement. This
is a source-backed input-contract mismatch, not a claim to have recovered the hidden error
envelope. All ten failed cases depend on this shared process fixture. Their add/update/read,
dry-run and durable queued/cancel assertions do not run an Engine or spawn that scheduled
command. The independent shell at-schedule fixture already uses default_shell and passes.

A minimal test-only correction can keep Target::Process and choose the actual current test
image's absolute Windows path with fixed `--help` argv. The image is already running; path
lookup/encoding must fail explicitly rather than inventing or lossily encoding an executable.
The argument remains a finite native help request if executed, while these MCP cases still
qualify job definition/queue behavior rather than echo output or child execution. Unix retains
the exact `/bin/echo`, `hello` values. Every case body/name/assertion stays unchanged, and no
production validation, state/permission policy, process owner or deadline change is needed.
The [MCP follow-up plan](planning/WINDOWS_MCP_NATIVE_PROCESS_FIXTURE_2026-10-04.md) precedes Source.

The full Clippy rows now report41 CLI diagnostics per architecture after the seven Unix-helper
errors were removed. Accepted main through #129 contributes staged APIs and test-style work
beyond the earlier29-diagnostic baseline. These warnings remain denied and outside this
single-helper correction. The87 measured integration failures, distinct Core admission/Server
WAL-reopen observations, stock-crash heartbeat work and public acceptance remain open.

### Native integration positive-root research and new Store initialization failure (2026-10-04)

Head302b1bf full run37145285065 reaches301 CLI unit cases, all passing on native x64 stable,
ARM64 stable and x64 Rust1.94, zero ignored/filtered. The previously failing ten MCP cases
now reach their unchanged definition/queue assertions; no scheduled child is exercised there.
It next reaches71 CLI integration cases,12 passed/59 failed on each architecture/compiler.
Complete logs print private-root refusals and the Windows `locron.exe` help basename. Cargo
then stops; this does not qualify later binaries. Both full Clippy rows retain41 CLI diagnostics.

Separate read-only research pins the four CLI/dashboard/feedback/service integration files to
the earlier measured543 source. Those blobs remain exact through302 ande4d2ca1. The earlier
complete run37140952254 records102 cases in these files,31 passed/71 failed;59 errors actually
print the private-directory refusal. A positive TempDir inherited broad Windows permissions,
while the existing DirectoryGuard::private API can create a missing private child. Setup should
normalize that child and release guards before subprocesses, keeping TempDir cleanup ownership.
The constructor inventory distinguishes state roots from discovery working directories,
advisory inputs, ordinary fake-manager JSON/log parents and intentionally absent paths. Dry-run
root-empty/DB-absence tests must not preseed state. Stored token bytes need private-new creation;
the manual-owner positive needs a genuine Windows DaemonLock and its owner metadata, rather
than a broad plain File lock. Existing missing-token/status refusal paths remain absent.
Feedback's extensionless `bin/http` placeholder is advisory-only: execution.rs checks the exact
file before PATHEXT candidates, so a new PE/extension requirement is not supported by the source.

Remaining POSIX execution fixtures, target display expectations and wake.sock readiness need
their own selected equivalence plan. A held lock precedes engine wake binding and cannot prove
a named-pipe endpoint or ACK. Native send_wake can consume a separate30-second identity budget;
calling it inside a5-second fixture would not preserve that fixture's original bound. The
[positive-fixture plan](planning/WINDOWS_INTEGRATION_PRIVATE_FIXTURES_2026-10-04.md) therefore
changes only five test files and three help basename expectations before measuring downstream.

Ordinary302 run37145289162 passes15 actual rows and fails x64 stable job111267842987. Its
two dashboard-exit cases stop at windows_lifecycle.rs48 Store fixture initialization, before
dashboard exit. The actual diagnostic is operation=2, stage=sqlite-configure-wal, category=io,
kind=TimedOut, raw_os=None, followed by SQLite configuration deadline elapsed. Its cause remains
unconfirmed; the distinct earlier Server SQLITE_PROTOCOL15 reopen failure is preserved under
Issue #25. Neither this positive-root correction nor an unrelated later pass repairs that
observed required-gate failure. No unchanged rerun, timeout increase or broad retry is selected.

### Stock heartbeat fixture command-line boundary (2026-10-04)

Integrated head e4d2ca16e17752d330275b8907cb10a31a318413 conserves both reviewed fixture
patches. Ordinary CI37146145032's x64 stable111270385871 and MSRV111270385843 pass all130
Core cases and the unchanged isolated EOF/Restricted proofs. Parent-exit-driver then fails
after0.06s: loader_crash.rs272 reports its actual helper exited101, and the helper's668 refusal
is `generic adapter exited before actual spawn` after0.01s. Saved trace reaches worker-entered
and caller-failed only. Actual Win32 error detail was not retained by that existing refusal;
do not invent a returned error code or conflate this with the earlier45.01s timeout.

Parent and separate read-only research independently reconstruct the exact unchanged
generic_trace::source format, shared stock_json bootstrap and current HOST literal. Original
ec13 HOST has1386 UTF-8 bytes; wrapped source is4677 UTF-16 units and EncodedCommand12472
ASCII units. Current HOST has9022 bytes; wrapped source is12313 UTF-16 units and encoded
argument32836 units. That argument alone exceeds CreateProcessW's32767-character limit,
which includes the terminating Unicode NUL. The standard retained extended stock path,
original flags, argv0 quotes and NUL produce32952 units; CRLF checkout increases the bound.
This is a confirmed static platform-contract defect consistent with the pre-spawn trace;
the failed job's specific raw OS code remains unobserved.

The normal parent-exit host also contains the full new publication-proof branch even when
the operation is absent. Select shared definitions plus only the intended compiled static
body. Unique comment delimiters in the existing test-only ps1 permit a pure Rust split/join
without reading scripts from disk, interpolating request data, fallback or changing the
production adapter. Preserve every normal/proof program token, assertion, clock and child
ownership boundary. Verify LF and CRLF complete command lengths with executable/flags/NUL,
and collect new actual native proof results before merge.

Separate primary-source research identifies a downstream string-null issue: PowerShell's
NullString is specifically for passing managed null to .NET string parameters. Framework
File.Replace treats a non-null backup name as a path; an empty name fails path validation.
Use direct fully qualified NullString.Value for the existing no-backup call, preserving false
ignoreMetadataErrors and all refusal assertions. The current job did not reach that API, so
this is a source-contract correction requiring its own actual Framework5.1 publication proof.
Sources: [CreateProcessW](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-createprocessw),
[NullString](https://learn.microsoft.com/en-us/dotnet/api/system.management.automation.language.nullstring),
[Framework File.Replace](https://learn.microsoft.com/en-us/dotnet/api/system.io.file.replace?view=netframework-4.8.1),
and retained actual job logs under the private PR review evidence directory.

### Initial lifecycle WAL timeout observations and diagnostic scope (2026-10-04)

Read-only research at unchanged Store Source302b1bf separates two first-open
failures from dashboard shutdown and the older Server SSE reopen. Correction to
the compressed integration note above: idle-client local operation3 and
active-SSE operation2 both fail at Fixture::new's first Store::open, before any
daemon/dashboard/job/client/shutdown activity. They report ConfigureWal IO
TimedOut/raw_os=None, not PROTOCOL15. These process-local counters do not
correlate databases. Complete logs are retained; no missing cause is inferred.

Private/SID/DB-admission and new Connection preparation end before the original
configure-entry5s clock. That one clock covers WAL and remaining settings. Fixed
PRAGMA query_one consumes ROW and DONE and explicitly finalizes even on errors.
Only idle-autocommit5/261 retry after finalization; exhaustion preserves the
SQLite code and15 is immediately returned. The observed IO timeout can be an
entry/pre-call gate or a refused late success. Pinned SQLite3.53.2 has native
WAL/VFS retry paths independent of busy_timeout(0), but current logs do not show
which boundary ran slowly. No retry, serialization or larger timeout is justified.

Select a bounded debug-only owner-local failure snapshot before a cause fix,
using only existing gates/calls and fixed numeric/category data. Capture native
prepare/query_one/ROW callback/finalize boundaries, query_one success-confirmed
DONE, attempt count and timeout
gate without timed-path logs or new native work. PID/local operation correlation
is not file identity. The prior Server15 needs separate owned connection/file
facts. Research receipt SHA256
f66dfe9b0007d33afe5ac294974c1191e0f48320bb376cf519a02b9bf69a7f17 and the reviewed
WINDOWS_WAL_CONFIGURE_DIAGNOSTICS_2026-10-04 plan preserve original policy and
all unqualified acceptance gates. No Source was implemented by the planning parent.


### Measured rename-refusal expectation before Source (2026-10-04)

Head1a1e5967ed179ef4b43654a0dd951118b62a5a1b qualifies the command-size correction in
ordinary CI37148405286: all three native jobs pass131 Core cases and the original
parent-exit-targets-handles-heartbeats-guards-confirmed marker. Their new Rust publication
proof then fails at loader_crash.rs531: actual raw OS errorSome(5), expectedSome(32).
The x64 stable111276979316, ARM111276979308 and MSRV111276979298 logs agree. The ordinary
run was cancelled after those actual failures to obtain logs promptly; later native
steps and packages are not qualified by that cancelled run. Full discovery37148403193
remains a separate failed receipt; no whole-workspace or public acceptance claim follows.

The locked tempfile3.27.0 implementation retains the failed TempPath in PathPersistError.
Its Windows persist sets candidate attributes and calls MoveFileExW with replacement;
that envelope does not distinguish the two internal syscall failures. Microsoft rename
processing documents access-denied refusal when replacing an open target. Win325 is
ERROR_ACCESS_DENIED;32 is ERROR_SHARING_VIOLATION. The prior expectation imported the
exclusive CreateFile open result into a different rename operation without evidence.
Do not generalize that all filesystems or rename refusals always return5.

For this measured fixture, require the exactSome(5) and add a same-candidate positive
control. Retain the closed8 candidate returned in the failure, verify its identity and
unchanged published7 while the exact held target handle remains open, then close that
handle and publish the same TempPath successfully to8. Restore a complete7 snapshot
before the unchanged actual child stage8/kill/reap/preserved7/publish8 proof. This control
separates the deliberate held-target state from a permanently invalid candidate, while
preserving failure propagation and the original45-second absolute driver deadline.
The earlier45.01s failure remains unexplained; successful new receipts do not reconstruct
its missing bytes. Exclusive stock-open and PowerShell File.Replace error32 assertions
remain unchanged and require their own native evidence. Sources: locked tempfile source;
[Microsoft rename processing](https://learn.microsoft.com/en-us/windows-hardware/drivers/ifs/rename-and-hard-link-processing);
[Win32 errors5/32](https://learn.microsoft.com/en-us/windows/win32/debug/system-error-codes--0-499-);
[actual CI](https://github.com/WhiteKiwi/locron/actions/runs/37148405286).


### Stable cfg statement syntax after native compile refusal (2026-10-04)

Source5585936 ordinary37151318184, full37151330037 and isolated discovery37151378233
finished red before the requested application runtime tests. Separate always-run stock
bootstrap/small/60k diagnostics passed on the three foundation rows; these do not qualify
the failed compilation or application suite. Actual Rust1.94/stable/1.98 reject #[cfg] directly on
the two assignment expressions at windows_configure.rs168/237 with E0658. Release builds
also retain unresolved observations assignments (E0425). The immutable release-token inverse
and formatting checks could not establish compiler validity; Root's prior review missed this.

Select only a syntax correction: wrap those two existing debug observations in cfg-gated
blocks. Keep assignment expressions, captured existing autocommit results, cfg condition,
release erasure and every native call/result/gate/error/deadline byte-equivalent after inverse.
No feature/nightly/warning allowance, production/query/test/dependency/workflow change is selected.
Separate development owns only windows_configure.rs after Issue25/31 exact Verify readback.

1. Reproduce/repair the two stable cfg statement sites. Verify: Rust1.94 native Windows debug
   and release compilation both succeed; no unresolved observations or experimental syntax.
2. Conserve observation and native policy. Verify: two exact block-only transformations invert
   the whole prior file; all other tracked blobs/modes and old/native/new diagnostic tests remain
   exact; cfg-erased release policy still equals the ce641 original.
3. Review/publish fresh changed Source. Verify: Root reviews the one-file diff and static checks,
   then records actual ordinary/full/every-binary results at the new source. These previous runs
   reached no requested app suite cases and produced no new paired ZIP; separate stock
   diagnostics remain their own evidence. All runs remain failures, not skipped validations.


### Complete native discovery and CLI input assumptions (2026-10-04)

Frozen SPEC behavior is unchanged. Actual current ordinary run37152818444 passes16 rows;
PR130 is merged as4ab7b9c, whole-tree equal to reviewed10fe. Full run37153461249 remains
red: CLI301 passes, cli.rs57/14 of71, both Clippy rows41 diagnoses. Manual-only
every-binary run37152995293 conserves all10fe application/test/dependency blobs and
executes23 targets/17 nonempty/6 zero-case on native x64stable,ARMstable,x64MSRV:
775PASS/34FAIL/809 cases,0 ignored/filtered. Dashboard8 and service19 pass; remaining
failures are CLI14,feedback2,install_sh5,self_update11,Core2 per row. Core selectors
vary and fail acquiring the capacity-one owner; the second loop iteration may already
have executed earlier native work. The particular owner/quarantine is unobserved.
Do not label those as proved child/output/ACL failures or erase historical observations.

Separate read-only triage retained all14 full CLI bodies/three native failure blocks,
all41 lint diagnostics on both architectures with cfg/consumers, and every later failure.
Report SHA256 cfe17bb47dc8bc0c8207782713cb0807d9b78d8c5238eb29a319ac598787c886;
receipt e8780a3f0175464a5880823dedbb0e292c0e71197f545d2e1e48215473b46259.
Of14 CLI failures,12 have Unix executable/display/shell/socket assumptions and2 expose
the native doctor path, which calls unconditional id-u/unsupported backend selection.
Several fixtures discard the first original error, so exact first stage is not claimed.
PrivateState already supplies the correct guarded private child/normalized path/lifetime.

Select only native success-process inputs, three explicit stock PowerShell shell bodies,
and exact human target rendering in cli.rs first. Existing shell_arguments supports
PowerShell with -NoProfile,-NonInteractive,-Command. The actual cargo-built locron image
with --help is a finite native successful process; no fake success/extra selector is needed.
Keep Unix inputs/argv/scripts/render expectations exact. Passive wake_facts is always
unprobed, and send_wake's200ms exchange is not a caller absolute5s authority; neither
substitutes real owned listener readiness. Doctor, wake readiness, distribution and Core
ownership/lint surfaces require distinct subsequent plans. No owner-PC native fixture is
selected. Full failures remain visible and Issue31 remains open.


### PR134 required native dashboard continuation failure (2026-10-04)

Main133-integrated head eae776ff2bd2a3a9b1fa591139615b9a3139e48e ordinary
37157605645 x64 Rust1.94 job111304173492 reaches all16 lifecycle selectors:
15pass/1fail/0ignored/0filtered. actual_dashboard_exit_is_bounded_with_idle_http_clients
fails at windows_lifecycle.rs266: durable state failed versus required running.
wait_running had observed running and a parseable real heartbeat; dashboard process/role
exit and manual daemon ownership checks had returned before this equality. Both other
native foundation rows pass. Preserve the raw job log (SHA256
a499e0366cb8c370531f7362628ae0daa95cab994f3f39b88e1ddb1304e92e43).
The log contains no run reason, attempt exit/result or heartbeat publication error.
A heartbeat persist panic is a possible target-exit path, not an established cause.
Rust1.94 default File::open and tempfile named-file opens retain delete sharing, so a
concurrent ordinary heartbeat read does not itself prove a rename-sharing failure.

Full discovery37157905776 at that same head runs all301 CLI units successfully on
x64 stable/ARM64 stable/x64 MSRV, then all71 CLI integrations with67pass/4fail and
0ignored/filtered. Remaining failures are durable_cancel_terminates_a_running_process,
human_doctor_prints_one_level_line_per_check, human_forms_leave_the_json_envelope_untouched
and wake_socket_makes_new_manual_run_promptly_visible_to_daemon. Later binaries are
not reached; both full Clippy rows retain41 errors. The failed required lifecycle
qualification remains separate and must pass at newly reviewed Source before merge.


### Default Windows doctor research and measured correction (2026-10-04)

Separate committed-source report SHA25601d3ae05af66956e816cc3f21b53f386c7a7e14244427546c82210e647e9040b,
candidatecf37e5ca1afb1e85c7c03ad33dc221896d7122c9d59aef4389feda8074b47318
and receiptbb85cdc7b03ff7bd50b236483553d61e8f8f9153c1b454f120125049d9ce7259
audit37 snapshots and all direct/typed clients. Doctor requires the service facts
before rendering; that helper constructs Unix UID context before default Windows
backend refusal. CLI main is the only caller; independent HTTP/MCP/browser DTOs
do not consume these registered/loaded fields. Default-only null/unprobed is
selected before Source, while Unix/fake boolean facts and actual token ACL errors
remain exact. See planning/WINDOWS_DEFAULT_DOCTOR_2026-10-04.md.

Actual495f full37157212227 ARM/MSRV67/4 reaches human_doctor2941 and human_forms
3181 after successful add prerequisites. This corrects the report's rooted-path
inference: the first doctor exit/error code is still unprinted; do not attribute
these runs to add rejection, UID or unsupported backend. A genuine native target
is still necessary for resolution acceptance. x64's separate300/1 supervisor
unit failure preempts all CLI integration cases; lint repeats the old41 sites.
Nothing qualifies actual wake, full suite, two-user privacy or release support.


### PR135 native doctor assertion correction before Source (2026-10-04)

Current main134-integrated8644efb full37160909694 x64 job111313961833
executes301CLI units successfully, then75CLI integrations70pass/5fail,0ignored
and filtered. Both existing doctor tests now pass. The new process-resolution
case compares actual canonical \\?\D:... executable to the requested ordinary
D:... path. Core execution's existing resolve_executable returns std::fs::canonicalize
on Windows; the expected resolved path must be independently canonicalized too.
Requested executable and stock PATH assertions remain exact and separate.

The new invalid-token-leaf test reaches exit5/service_io, then rejects its own
unwrapped message-prefix expectation. Existing ServiceError::Io Display uses
"service management failed: {message}"; main renders that Display without stripping
it. Its actual message text was not printed in the failed assertion. Preserve
the genuine ACL/type refusal and select the complete fixed display prefix, with
an explicit failed assertion message rather than catching or downgrading errors.
The same row also preserves cancellation/wake failures and one unchanged
disconnect test Store after-WAL finalize deadline overrun (5469705us under the
original5s clock, configuration modewal/done/autocommit observed). Their causes
and broader runtime qualification are not repaired by this expectation slice.


### PR135 measured early full-suite failure and missing x64 doctor evidence

Changed-test head cc07299206e5ee83106d95dbcd8af13214e86765 full37162098246
x64stable job111317472009 stops at CLI unit300PASS1FAIL: unchanged
service::windows_supervisor::tests::native_io_expiry_quarantines_activation_child_and_all_later_admission
panics at windows_supervisor.rs1687 because the actual native I/O boundary was not
entered. CLI integration is unexecuted; this is not doctor qualification or a
proved cause. Raw log SHA25633038ff578152d6ca17011b2cb78b21244714c79021e435f43557c02dc19ac6b
is retained. ARM job111317472061 reaches301unitPASS and75CLI73PASS2FAIL with
all four new plus two existing doctor cases PASS; only wake/cancel fail there.
Both full Clippy rows retain exactly the prior41 diagnostic message sequence.
Select a strictly additive independent doctor gate in the ordinary Windows
foundation matrix before the existing service command. This avoids an unrelated
early failure hiding doctor evidence while retaining every existing refusal.
No application/test changes or unchanged full rerun are selected.

### Native CLI wake/cancel fixture continuation before Source (2026-10-04)

Separate research pins e6eb887 and proves both old selector bodies unchanged
through current main4644588/PR13502f. Source waits for Unix wake.sock on Windows,
although Engine binds a named pipe. Old495f ARM/MSRV reaches wake.exists1124 and
the final cancel assertion1214; cc37162098246 ARM/MSRV reaches301 unitPASS and
CLI73/75 with wake1142/cancel1232 failures. Actual terminal reason/phase timing is
unprinted; the platform mismatch does not establish a production failure cause.
Wake reuses one5s clock; cancel one8s, production grace5s, Unix sleep30 remain.

Checksum-qualified Tokio1.53.1/interprocess2.4.4 Source confirms the safe native
same-connected-handle metadata composition: NamedPipeClient.AsHandle clone,
sync byte PipeStream conversion, is_client/server_process_id, every-result
evade_limbo; frame/ACK remain on the original Tokio stream. All14 primary members
match locked crate archives; existing Windows CLI dev graph needs no change.
Research report SHA2563f476ec623d6f26748c272e34d46b5381d242dbf19ac8798c1575ade549c7eee,
candidate ABI c4131680961256c934504ee03d778ee46167f84007a6ec45e1118cc8d159fd26,
receipt934f7a27e0124a099657da180e49c234818eb84f38361b2b72f7054e083ac231.
Native API metadata success still requires real positive/foreign-peer execution.

Additional read-only CI research selects one additive exact acceptance step,
explicitly adding ci.yml as a third Source path before separate development.
The same !cancelled/native_core-success expression includes a status function,
so prior failure stays failed without imposing a hidden success-only precondition.
[GitHub status-check semantics](https://docs.github.com/en/actions/reference/workflows-and-actions/expressions#status-check-functions)
support independent qualification before the known later service-unit preemption;
this does not diagnose or repair that supervisor failure. Research receipt
bbe7bd2683e09dd085576f04adbfef3c69792b69f1855513c1e379cd24a38467 also preserves
the old41 lint sequence. planning/WINDOWS_NATIVE_CLI_CONTROL_2026-10-04.md freezes
six actual selectors, auxiliary zero-acceptance roles, original clocks/finite
owners and strict progress/protocol before Issue31 Verify. No owner-PC effects.

### Native negative wake peer default descriptor boundary (2026-10-04)

At merged main46445881, independent research verifies locked Tokio1.53.1 safe
Byte/duplex/first-instance/remote-refusal create with NULL security attrs; no
direct widestring edge or unsafe application API is needed. The default pipe
DACL is intentionally untrusted and gains no privacy from an existing-private
filesystem root. Actual same-connection PID mismatch decides the foreign-peer
refusal; only the secured production daemon supplies positive wake readiness.
Mio1.2.2 Drop requests connect/read cancellation and permits writes to complete;
neither future cancellation nor Drop proves synchronous closure. Existing finite
actual Child/client/runtime/guard ownership and cleanup/quarantine stay required.
Root reviewed every addendum/source-pin byte hash. Addendum SHA256
ca5284748b851bac8c1de1dbacd06e10e67401020a6abd6d8d180d9fc4a9b307;
receipt eccaf14167d6c3a620e84d5cca83269f9f7e56f454d3b916880700bf6fb73ad2.
[Tokio pinned source](https://docs.rs/crate/tokio/1.53.1/source/src/net/windows/named_pipe.rs),
[Microsoft pipe security](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-security-and-access-rights),
[CancelIoEx completion contract](https://learn.microsoft.com/en-us/windows/win32/api/ioapiset/nf-ioapiset-cancelioex).

### Native test owner admission and probe-clock research (2026-10-04)

Separate research5db5c2a freezes three allowed uncommitted WIP paths. Report
902c57b46d0e29aa059cf1ab21a9b0962fdd4aa667a226d63835c240f18ee92a,
proposal8fefb54feeaa6397fd560540a9a53797b01d0c75f1cd7f4e2a5739f59613e422,
receiptd903d84107ec91f1725cd85b1d903e556ee6f43f3bdb63c5bdc00d626adba4bd;
Root verifies all34 artifacts and reads complete report/ABI/Source call map.
Rust1.94 Windows thread creation drops initializer on failed CreateThread;
SendError/TrySendError returns payload, so actual Owner handoff can run blocking
Drop on driver. WIP Driver case-only wait misses blocked query's entry200ms.
Select resource-free one-command admission before fixture resources, same-worker
partial Owner/setup/actual daemon; new tighter30s setup horizon + original literal
post-spawn5s/8s minimum. Retain probe deadline and on-time completion seal from
immutable origin; independently bounded result polling, no blind slot clear or
late reopening. Safe API cannot establish hard admission/preemption/abort cleanup.
No native test/compiler/parser/mutation occurred; Source remained stopped. Actual
six-selector changed-head native acceptance still required. Tagged primary std
thread/channel source and SDK facts are retained in the hash-qualified report;
no closure/channel metadata is substituted for actual child/guard/peer/cleanup.

## Native Windows changed-head failures and selected fixture follow-up (2026-10-04)

Completed ordinary run37172647017 on Source3a6efccb is FAILURE:17 executed jobs14PASS/3Windows foundationFAIL, six selected cases by three rows8 executed=5PASS/3FAIL with10notrun. Actual wake passes all3; x64 cancel reports ChildExited/Run/flags97, ARM cancel Expired/Cleanup/flags1, MSRV malformed Native/Setup/flags97. The preceding libraries/doctor/service/GUI and two exact-tree paired packages passed, but later lifecycle/MCP/HTTP/uploads are skipped at this head. Root independently verifies61 qualification artifacts/raw logs/paired structure and merge tree, proof35c26bd4a624e065bd4a4d7c88dc5aa97bd51544cbd81b75039d61b5663d242f; Issue31comment5976163511 records the exact outcome.

Separate causal research b48716c3f387136563e8f28a316cb365c354e14277978f0e69169ef9f3f082b9/22files and stock research418f786da4ce280f7dd482f39155a149c8f04b5b21faf17931deb821fd66a922/32files are independently verified (Root5b94e3ff4b44bc37831ba1114d3755f7235df57d982fb0c71430fda07c1671ef). Under exact Source, x64 daemon exits after successful submit before first history liveness; actual status/cause is unobserved. ARM expiry snapshots unfinished cleanup, not the pre-cleanup work failure. MSRV malformed fails before the ACK; CreateNew exposes a zero-byte peer-ready path before its later write, but actual empty/error/status is unobserved. Root selects six independent exact-selector CI steps, fixed records from already returned operations and successful-empty Unpublished continuation only under the original loop/clock. The fixed design086fee7791249aa00e6034f4def2e79c967e458dbaadabedcbb2285daeacaef5 plus4files gives52operations/zero, six self-contained u64 records plusone u8intent and579CRLF-byte output. No stderr/text/private identity or extra native/error query is selected; no cancel readiness probe.

The separate MSRV stock parent proof confirms parent reap/three-associated exits/observer-held stock refusal but generic-heartbeat NotFound prevents later observer/guard/pipe cleanup. The no-backup ReplaceFile [documented1176](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-replacefilew) can remove the old pathname; it is an API-grounded unsupported surviving-name assumption, not measured1176 or an inside-call kill. Root selects appendix11e985ed6401722a5c6d1d7e6b29dec4c5920cd555c08d96c42f83fc0d8c2c00/5files: one CreateNew retained appender per real producer, strict bounded21-byte records, actual same retained no-delete reader for every sample, current count+tail stability, and both real partial-append hard-stop controls. Original65-byte snapshot controls/observer/owned Core factory and all original lifetimes remain exact. New normal/append command programs must fit the full Windows limit before dispatch; no fit/native qualification is inferred now.

## Missing actual PowerShell append-refusal observations at Source8110 (2026-10-04)

Root reviewed both complete Source8110 fixture deltas, all120 Source artifacts and the whole-file inverse, old SnapshotProof and10 complete command models (Root review5af4519f). The newly selected Rust refusal controls save actual ErrorKind/raw values, but both PowerShell Refuse invocations catch rename/delete only into booleans. No retained alternative receipt saves their actual exception data. This is a selected Verify gap;8110 is unqualified and is not yet published or merged. All genuine41-byte appender/lifetime/held-reader/release proof obligations remain distinct from observations.

The separate26-file refusal research is fully reviewed and independently hashed (manifest98da55bff267087fd5b9ada5c0614a51cb1a7fcdfb0e7fe4ad5b7c178de48cba; reportcb0c76c8740e7a5a96d4d14027963732654f6a655917c15baa5c31b8a52e555b; Root static proofb1d9977de714de2255c20e5d7fa6534d536d79acf7fa67eb2036abdbb8a34d99). [Framework Exception.HResult](https://learn.microsoft.com/en-us/dotnet/api/system.exception.hresult?view=netframework-4.8.1) provides a whole signed Int32 status, not an OS errno; [InnerException](https://learn.microsoft.com/en-us/dotnet/api/system.exception.innerexception?view=netframework-4.8.1) supplies the existing reference/null. [PowerShell5.1 catch guidance](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_try_catch_finally?view=powershell-5.1) establishes the caught ErrorRecord's Exception boundary; actual wrappers and values remain unmeasured. Closed managed io/access/other projections preserve actual categories without runtime type names. Low-word decoding, expected-value/type admission or synthetic exceptions would not satisfy this observation contract.

An unapplied complete specimen independently models four fixed primitive string slots with at most5 existing nodes each; record95 ASCII/array393/extra root field405 bytes. Source-only identifier/indentation compaction confined to new Append/common code and its new Normal function references fits every full LF/CRLF root/child command below32,767 with at least1,024 margin; maximum31,224/margin1,543. Old SnapshotProof raw SHA2a7eb52bae39c85c4b3bb55c37352ed77f222d9329175ac9d67df68f22072ae9 and full29,352/29,816 commands remain exact. This demonstrates feasibility only, not implemented Source or PS/native acceptance. Preserve prior3a6 failure and all absent acceptance flags.

## 2026-10-04: Observe existing lifecycle progress results after current-base failures

Current PR138 head1c45ef10b32b26c70d7ccc8fcf0d47b4dc260dc3, ordinary CI37180022092/pull_request/attempt1 has two actual x64 lifecycle failures (each15PASS/1FAIL), while ARM16 and its downstream gates pass. MSRV1.94 fails the existing3s preserved-target advancement assertion after dashboard stop, ownership/state checks and a readable baseline. Stable1.99 fails descendant readiness within the original30s before hard_stop; forced crash/restart/recovery/no-retry are unreached in that row. Whole libtest durations are not per-phase measurements. Actual target/descendant status, captures and native cause are unprinted. No causal repair or timing relaxation follows.

Read-only research119 artifacts and35 immutable Source records were independently verified; Root proof36d67e15576565b4dbe03694292a66e4e07783de2e228d280ba1e47a04eec960. The lifecycle file is52426bytes/SHA45db1848f326a7cf80ee656d36f6415c8d8ce93676581726c889d5c896c53165 at1c45 and integratedd894a243619c93dacfdd34e9c7ec5ab068c588be. b664's CLI helper and8110/15ca Core append observations do not cover these separate Rust lifecycle read/publish paths. Root reviewed and purely integrated those distinct Sources plus main15820; their native qualification remains pending.

Select only a one-file failure-only observer of already-returned marker/read/parse/numeric facts and the already-returned dashboard ExitStatus. First returned I/O kind/raw code is immutable within the progress observation, not a first process error or causal classification. False is_file means only the returned boolean; unread/parse-unexecuted slots remain unobserved for that iteration. Numeric nongreater/zero and parse error remain distinct. No extra read/status/capture/Store/native call, diagnostic thread or resource owner is selected.

The closed8-field suffix uses fixed enums and Option<i32> scalar projections only; conservative20736 combinations give226bytes includingCRLF, complete original message273. This is a static text bound, not native syntax/timing/cleanup evidence. See [selected before-Source plan](planning/WINDOWS_LIFECYCLE_PROGRESS_OBSERVATIONS_2026-10-04.md). Keep failed run1c45 authentic and all wide Issues open until their original Verify and required publication succeed.


## Stock test Clippy syntax at exact3f5 (2026-10-04)

Completed Windows run37184851251/attempt1 preserves two warnings-denied Rust1.98 test diagnostics at loader_crash.rs375 chunks_exact_to_as_chunks and1015 manual_let_else; they are independent of the negative-peer Native/Setup/ReadyOpen/raw32 and unobserved withheld-entry cause. Root independently qualified190 artifacts/92 immutable Source snapshots and the first-work/cleanup boundary (proof6a237eede588d185a2faac9e524ae9192395bada670b4b38fc88d60cdb6cbf27). The separate read-only9-file recommendation c8f9bc8400e5c4f4a2bf1ea748ad232fa6e4c4fbaf996e22405d499f31399013 and pinned official Rust1.94 core sources establish safe as_chunks stability1.88 and array as_slice stability1.57. Borrowing each complete21-byte array as a slice preserves the old validation body/count/independent tail/error order without requiring full divisibility. Let-else evaluates the same collision create once, moves the same expected error and diverges on unexpected success; it does not promise identical temporary destruction instructions. No compiler/native test was run in this research.

PR139 is now merged as main814e689bb0717aba183dbd084e6151ba11ac4e99, whole tree4b1ecc975f766807f4baafc4ae90e3c41d6b9c0b exactly reviewed16e. Fresh139 native42/lifecycle16/HTTP21 and all four Windows Clippy commands passed; Root127/63 proof9ed1e5e2a0a403844c0814c402e9321a7d567a3290a022e2924d63454a513518 and one-shot merge journalf97b10ed95dc576f5f2025d1fb5a7b21cf007e10498f829a0c31f4f2d9a3dc59. Import that exact reviewed main union separately before the selected one-file syntax change; conserve the native/stock/lifecycle changes and both branches' planning history. The merge is not a causal fix for PR136 failures.

## PR139 listener-policy and shutdown controls before Source (2026-10-04)

Root fully reviewed PR139 c847dec1cf924ed06fdcab29f6f0ec93ba69e96c and its existing ordinary hosted run37176501497: native Server31 and existing active-SSE/stalled tests passed on x64 stable, ARM stable and x64 Rust1.94. These existing cases do not execute the selected whole-list socket-count refusal, upper-port saturation, failed/clean/panicked-listener/signal-error broadcast and full-survivor drain/error-precedence/timeout controls. Root keeps that Verify gap open before merge; no observed failure cause or new native success is claimed.

The separate read-only47-artifact design (manifest7c5cdac6d8ddfdb66bbb7ed8a207a89c4832fbe10d802ec224c8065a2a39ccae; Root proof460ddb9ab362c5ae1eac65e3a63459c09d4637bee7a435ae9d6c34abf1a45a26) is fully reviewed and independently verified against15 app Git snapshots and5 exact Tokio1.53.1 primary files from Cargo.lock. Tokio join_next is cancel-safe within select; abort_all does not remove/complete owned tasks and requires subsequent join_next until empty. timeout polls the inner future before the delay and cannot preempt non-yielding/blocking work. Workspace does not enable test-util. These Source contracts justify shared private seams and the real existing10s timer; they do not prove actual task/OS failure behavior.

Select one Source file and two private seams: exact production bind/preflight/policy with a SocketAddr-to-TcpListener future boundary, and the original select/broadcast/drain implementation with original JoinSet/watch/composition-stop/signal-result futures. Callback-count0 proves refusal before native socket admission; explicit55 high-port requests plus10 final0 across65526..65535 prove non-wrapping exhaustion through production policy without assuming real fixed ports free. Separate actual native bind calls establish IPv4/IPv6 and native high-port prefixes; simulated AddrInUse is not actual occupancy. Real JoinSet tasks/controlled failure/real panic/watch survivors/Drop+join receipts prove loop behavior. Private injected signal error is post-adapter control, not genuine Windows Ctrl-C registration failure. Windows dashboard_ctrl_c mapping, old31 native test bodies, all clocks/callers/public APIs/deps/workflow remain exact. Detached Axum connections and begun blocking Store workers remain separate unqualified Issue30 scope.
## PR139 measured warnings-denied failure before repair (2026-10-04)

Ordinary changed-head run37183332242, exact PR head4852c51ae38cf28568db34d861e7b22e85b339df/attempt1, completed FAILURE. Saved completed job metadata shows three Windows foundation jobs SUCCESS, while Windows/Linux/macOS lint jobs fail. The first Windows and Linux raw lint logs both identify exactly one new locron-server lib-test error at lib.rs881: the fixture's Err(_) panic arm triggers match_wild_err_arm under -D warnings. MacOS's specific diagnostic and the new42-case native evidence remain under separate read-only qualification; their content is not inferred from these two logs. The real result does not waive any required lint or native gate.

The helper already aborts/reaps its owned controller and observes captured yielding-task Drop receipts before handling the timeout result. Select only binding the returned Elapsed value and including its Display in the existing failure panic after that cleanup. This uses an already-returned error and adds no call, query, retry, clock, task or owner. Existing13s fixture/10s supervisor guards, eleven control oracles and production behavior remain unchanged. No allow attribute or warning suppression is selected.

## PR136 review: completed-marker sharing race and strict lint (2026-10-04)

Root reviewed original3f5fb95 ordinary37184851251 raw native/lint logs:
x64/ARM first-work Native/Setup with ReadyOpen Other/raw32 in four negative
cases, x64 withheld Disconnected before its notice with no CaseResult, MSRV
all six cases pass. Both stable rows stop before lifecycle/downstream admission;
service64/GUI12 and the explicit stock diagnostic controls still execute.
Required lint reports chunks_exact_to_as_chunks and manual_let_else in the
new stock Rust helper; later CLI/GUI lint is skipped. None permits merging.

Separate read-only review of the complete2324-line helper and callers finds
final create_private_new exposes peer-ready during private ACL CreateNew.
filesystem_worker.ps1 requests FullControl, including DELETE; no-follow
inspection/open shares only READ|WRITE. [Microsoft CreateFileW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew)
documents incompatible sharing and delete-access refusal. This proves a Source
race path, not the exact internal instruction/holder of the observed ReadyOpen.
The withheld underlying failure and old lifecycle/full/native unknowns remain.

Locked tempfile3.27.0 already exists in CLI dev dependencies. Its absolute
TempPath::try_from_path adds no CWD I/O; persist_noclobber retains the owned
candidate on failure and invokes Windows MoveFileExW without overwrite/copy.
Select closed private staging before final publication, actual staged-absence/
complete-read/collision-preservation controls inside existing real peer children,
and failure-only scalar snapshots for the existing withheld notice. Keep every
reader refusal and original clock/selector/owner boundary. Two strict lint
syntax corrections preserve the stock parser and collision assertion. See the
amended native and stock plans; Source waits for Issue31 exact readback and
the separate developer lease. No old missing visible-empty control or concrete
managed HResult is fabricated as current acceptance.


## Concurrent PR136 stock syntax publication (2026-10-04)

Remote b131edc1ad420532c69b91c854ddb8ceb187b767 contains a Docs-before-Source borrowed-slice as_chunks correction and the same main814e689 union. Its complete stock diff is two syntax sites, with no native helper/lifecycle/PowerShell/CI change from3f5. Both syntax choices are safe Rust1.94 operations; retain the independently published borrowed-slice blob to avoid replacing reviewed work. Local062aa2 supplies the separately selected closed-candidate readiness publication and failure-only unobserved outcome diagnostic. Complete locked metadata succeeds with354 packages/nodes after locked dependency downloads and no lockfile change; actual Windows compilation and native acceptance are still pending.


## Observed lifecycle strict lint after stock syntax correction (2026-10-04)

Remote b131 completed ordinary37189751211 with4 failed executed jobs: Windows strict lint and all three foundations. The raw lint111399286359 identifies only the new observer parse match at windows_lifecycle.rs547, Clippy single_match_else; later GUI lint is skipped. Root reviewed its complete suggested if-let/else rewrite against the selected observer's unchanged one-read/one-parse and Numeric/ParseError Option contract. Stock syntax lint passed. Separately, actual native observations include repeated ready-open raw32, History expiries, daemon exit5, withheld Disconnected and ARM target/debug/locron.exe removal access5; these are preserved without attributing an unmeasured cause. The integration78af945 keeps all native controls/clocks intact and does not claim to repair those additional failures.


## Root-reviewed private peer publication research (2026-10-04)

Issue31: Root independently verified readiness43/13 Git snapshots and retained
prior192 files, plus lifecycle syntax19/raw42100. Actual raw32 holder/path/native
instruction and withheld Disconnected cause remain unobserved. Fixed FullControl
DELETE/share3 vs consumer share3 is a primary-Source/API inference, not observed
runtime cause. Select private created stage -> leaf-close -> one disabled-cleanup
tempfile3.27 no-clobber move under retained parents, with a new genuine standalone
control, independent failure-only notice snapshot and captured lifecycle if-let.
Root proof hashes are recorded in WINDOWS_NATIVE_PEER_PUBLICATION_2026-10-04.md;
the complete selected plan precedes Source and adds no product scope. Parent-
observed final-Empty/held-exit lane remains unselected/unqualified. No source/API
or native qualification was produced by this research.

## Root reviewed current-main integration research, 2026-10-04 (native73)

Separate 119-file native concurrency and 195-file snapshot integration research was independently checked against complete immutable Git trees, blobs and literal Docs components (Root proof db041b40d60e05b3b8b6e1df4397247dc456dac4be0eea942751d6b4ea5d0f5b). Actual PR140 merged main is c4ef8b90276a5119a46eeaac17c931abad4382ae with exact reviewed tree f90fbc9e7ae83b1221e003eb3fd43090e93cf3bd. Its fresh hosted qualification is complete; it is not qualification of either new integration.

Incoming73 preserves the outer private root guard and has bounded1502/844-byte notices within its2048 plan. Its TempPath cleanup and queued-result/elapsed observer differ from Root-reviewed89. Select exact89 Source, preserve both historical Docs and import only exact merged main140; no73 native qualification or old failure cause is inferred.

## PR140 output recovery Verify research before Source (2026-10-04)

Root reviewed the separate69-artifact research against25 immutable app Git blobs/modes, four full trees and the complete composed renderer. Current main15820 is entire-tree equal to reviewed merged137. Preserve old140 pre-main bytes and main137 main() suffix exactly; combined55f41774c7e70d7da17bdf9f47059e2effc4c0881182ede3b7c9802548bb3269. PR140 lacks the concrete required recovery/native controls, but this read-only research demonstrates no production writer defect. Earlier paired/entrypoint/snapshot successes do not qualify these paths.

Select ten portable methods/32 controls through actual fd-backed operations plus four hosted-Windows methods/six controls for real exclusive collision, ordinary-file/directory identity replacement, actual read-only deletion denial and close/namespace reuse. Distinguish each deterministic injected generation/write/flush/close error from the genuine OS observations. Retain original error identity/notes, unknown objects and unrelated subtrees; no hostile-race/atomic visibility/durability guarantee. Python/Windows primary contracts support these selected oracles but actual results remain pending. Root research proof1f360be93ecce1d60129f952bc8d21440f5630896546472a2c49c1e5974c739a.


## Root review of WinGet output Source and exact main139 integration (2026-10-04)

Root full review af824 verifies128 artifacts/complete union/299 protected Source
pairs and actual741-line control semantics; no blocking Source finding. Planned
portable10/32 and Windows14/38 remain unexecuted. Select only reviewed main814
Git integration, preserving full new tests/workflow/renderer55f4 and incoming
Server exactly. The appended WINGET_OUTPUT_RECOVERY_CONTROLS_2026-10-04.md plan
requires literal Docs-suffix union, before-Source Issues32/35, Root final reread
and separate developer before changed-head hosted qualification/publication.


### PR136 failed73d3 and owned stdout capture selection

Fresh37191426679 confirms16/17 ordinary executed jobs, four strict Windows lint commands and17/18 original native CLI cases passed; ARM cancellation Expired/History and later GUI remove/raw5 remain failures with unknown actual call/holders. Parent re-read Owner::output: its unbounded pipe read precedes actual Child.wait. Select guarded fresh regular captures, actual child polling and full64KiB+1 bounded collection under original clocks, plus independent actual producer/held-write/collision/cap/live-negative cleanup controls. This corrects a source-backed helper boundary without claiming the measured ARM cause. Exact failure observation: Issue31#5978690173. Before-Source selection is in WINDOWS_NATIVE_CLI_CONTROL_2026-10-04.md; all production/other Source and progress semantics stay exact73d3.


### Root actual734 capture and publication composition research (2026-10-04)

Separate research and Root independent proofs dd28aa1a/138f0dd4 identify the
disjoint capture delta, preserved private publisher/fixed-only notice, exact
three capture-only Docs suffixes and actual ARM Expired/History failure. Actual
run37196440237 has20PASS/1FAIL; pre/post cancellation and daemon/cleanup cause
remain unknown. The new exact main9de596f8 retains reviewed138 tree51c7643.
Selected before-Source composition and four Verify steps are in
planning/WINDOWS_NATIVE_CAPTURE_PUBLICATION_2026-10-04.md. No owner-PC native
execution or causal production repair is selected.

## Root reviewed current-main integration research, 2026-10-04 (pr138)

Separate 119-file native concurrency and 195-file snapshot integration research was independently checked against complete immutable Git trees, blobs and literal Docs components (Root proof db041b40d60e05b3b8b6e1df4397247dc456dac4be0eea942751d6b4ea5d0f5b). Actual PR140 merged main is c4ef8b90276a5119a46eeaac17c931abad4382ae with exact reviewed tree f90fbc9e7ae83b1221e003eb3fd43090e93cf3bd. Its fresh hosted qualification is complete; it is not qualification of either new integration.

PR138 and merged main have disjoint Source changes; the original unmodified forecast is 306 paths/tree6564ad98a2053813eb00adfb8f93626e9d860765. Root Docs adds protected suffixes and one plan. Old1c45 x64 lifecycle294/877 failures remain measured failures with causes unobserved.

## PR136 completed capture qualification and ARM History observation gap (2026-10-04)

Current7343/run37196440237 completed16 ordinary PASS/1 ARM failure. Actual original controls17/18 and existing capture controls3/3 (all9 producer/ownership markers) pass. The remaining actual ARM first-work is Expired/History; CleanupKill intent is not returned kill/reap evidence. Cumulative legacy HistoryCliWait/code0 is not reliably the current CLI result. Source-only producer sharing/tail and startup/capture cost possibilities remain unmeasured. Select only scalar current-call epochs, real entry/return/wait and last completed same-run History/progress records per planning/WINDOWS_NATIVE_CLI_CONTROL_2026-10-04.md. No clock/oracle/admission/production change is selected.


## Current PR136 observer and concurrent publication integration

Root selects the exact disjoint observer4639 and concurrent6e98 contributions in [the completed integration plan](planning/WINDOWS_NATIVE_OBSERVER_CONCURRENT_2026-10-04.md). Preserve current main9de and all original gates; actual new observer qualification is pending and the concurrent x64 ChildExited/Run failure remains unexplained.

## PR 141 Windows first-lock publication and qualification review (2026-10-04)

At original head 1e0c19a82d9efd4f67135d32f16617b2e41adcae, token.rs opens the permanent lock with Core open_private_or_create. Core filesystem_worker.ps1:46–47 exposes CreateNew's final name while its FullControl/FileShare.ReadWrite constructor remains live; filesystem.rs:574–583's concurrent guarded open omits delete sharing. The access/share combination can produce raw32 before try_lock; occurrence and holder in actual PR 141 are unmeasured. Select Windows-only closed private candidate/no-clobber publication with exact deadline and ownership/error precedence, preserving Unix and the permanent final. Locked tempfile 3.27.0 archive/API checksum matches Cargo.lock and is already a Server dev edge; a target-Windows usage introduces no new package/version. Original sequential CI success does not qualify concurrency/failure/publication. docs/planning/DASHBOARD_TOKEN_SERIALIZATION.md defines 11 portable and 2 native drivers, real OS and injected-fault evidence separately, under Issue 162; main PR 139 and Issue 30 remain intact.

## PR141 pre-publication qualification suffix review (2026-10-04)

Full parent Source review at9534372 found the newly introduced scratch-name .tmp ends_with predicate risks the same primary Clippy rule observed in PR142. This is unexecuted Source inference. The selected direct last-dot/lowercase-tmp comparison preserves the exact generated-name oracle, all13 functional drivers and production behavior; see planning/DASHBOARD_TOKEN_SERIALIZATION.md.

## PR141 full Source review and publication selection (2026-10-04)

Root full review of9534372 and exact suffix b8acdfe conserves allfive selected Source files/old3 tests/Unix literal/Core constructor/Cargo.lock and original295 protected modes-blobs. Static checks pass;13 actual functional drivers and type/lint remain hosted-unmeasured. Select normal exactc4 repository publication only, with full Source/Docs union proof and fresh original gates per planning/DASHBOARD_TOKEN_SERIALIZATION.md. Temporary receipt loss was recovered from conserved Git objects and Issue GET; no runtime acceptance is inferred.

## PR141 returned-error lint prediction (2026-10-04)

Three new support Err(_) panic arms match the previously observed PR139 warnings-denied lint. Select only binding the existing io::Error and printing kind()/raw_os_error() after each fixed message, preserving privacy and all operation/clock/ownership behavior. This is Source prediction, not runtime failure; see the three-step Verify in planning/DASHBOARD_TOKEN_SERIALIZATION.md.

## Token qualification publication after release snapshot merge (2026-10-04)

PR #138 was merged externally as main commit 9de596f8ffe48e8036f775ad1fddbea9302915a1. Its tree matches reviewed head 886e24a and ordinary synthetic commit 97b8d03. The 17 ordinary gates, Guardian, 16 snapshot tests and paired x64/ARM package provenance passed. Integrating these reviewed sources changes no token behavior. PR #141 Source commit 654d5a4 retains the full 9534372 implementation, b8acdfe suffix edit and exactly three scalar-only returned-error arms. Root read the full Source and independently proved each later inverse. New token runtime qualification remains unmeasured.

1. Integrate exact main commit 9de596f by a normal Git merge. **Verify:** incoming Source, CI and Cargo plus all five selected token Source paths retain their reviewed modes and blobs. Reject any Source conflict or adaptation. Preserve both complete planning histories after the common prefix; only shared planning documents may be reconciled.
2. Publish by normal fast-forward to the unchanged original PR #141 remote head 1e0c19a. **Verify:** fresh REST head and main binding match; the original remote is an ancestor; staged and unstaged scope is clean. The PR body states actual evidence and remaining qualification. No forced push or retry of an unchanged failed head.
3. Qualify the published head against the current base before ordered merge. **Verify:** all 13 new Windows drivers, 11 portable drivers and the original three tests pass on their actual hosted platforms. Require x64/ARM/MSRV native tests, Unix tests, warnings-denied lint, all ordinary gates, Guardian and paired package provenance. Final synthetic, head and squash trees must agree. Keep broad native acceptance open; static receipts do not establish runtime results.

## PR141 actual failed-head review and terminal peer gap (2026-10-04)

Exact4fa ordinary37204722580/1 is completed11SUCCESS/6FAIL/2optionalSKIP. Linux Clippy observes unchecked_time_subtraction and needless_pass_by_value in test_support; Windows non-test token Clippy observes two question_mark arms. All three Windows token suites first fail with ConnectionReset/raw10054 in atomic_failures, then twelve driver-mutex poison failures; each43PASS/13FAIL, old3PASS. Source confirms a read after fully validated Done and later polls of completed peers, but actual reset phase, first child exit/capture/subcase and native cause remain UNKNOWN. Separate x64 supervisor1687 native-boundary failure remains unqualified; ARM/MSRV same64 service cases PASS. Preserve the full actual/unknown distinction and select only the three-path terminal/lint repair with actual C1/C2/C3 read-retention assertions in planning/DASHBOARD_TOKEN_TERMINAL_2026-10-04.md.

## PR141 terminal Source review and qualified-main136 integration

Root reviewed complete704709e terminal patch/contexts and independent whole3 inverses/309protected entries; lint/terminal/C1-C3 scope is preserved, runtime acceptance pending. Historical4fa RST phase/child/capture/native cause and separate x64 supervisor cause remain UNKNOWN. Follow planning/DASHBOARD_TOKEN_MAIN136_2026-10-04.md for exact reviewed main136 integration before fresh original gates.

## Token141 actual reset observation selection (2026-10-05)

Fresh621 CI37217720691/1 passed14 gates including all3lint and5Unix but failed all3Windows token libraries43PASS/13FAIL. First atomic-fault read reset10054 lacks actual owner/last-frame/pre-kill Child/capture/subcase/cause; ARM has one prior Write/cleanup_false completion only. The separate support-only plan DASHBOARD_TOKEN_READ_FAILURE_2026-10-05.md selects safe bounded actual pre-cleanup observation while preserving failure, all terminal/ownership/privacy checks and original clocks. Existing wake/cancel and historical4fa causes remain UNKNOWN; no production repair is inferred.


## Measured token observer lint (2026-10-05)

Fresh observer CI37222583220/1 exposes only measured private observer syntax in all three completed strict Rust1.98 lint jobs: comparison_to_empty at support831 and manual_assert at997/1108. Root read each actual raw diagnostic. Preserve the same safe location parse and all failure-observation operations while returning a report from a private builder to one unconditional fatal panic boundary. Current native/runtime results remain pending; no child/reset/capture cause or production repair is inferred.

## Actual normal producer exit before validated prefix receipt (2026-10-05)

Completed f8 CI37222583220/1 is11PASS/6FAIL/2optional: every first native atomic-fault read failure now actually observes pre-kill Child exit0, stdout161/stderr0, active assigned peer and partial framing. x64/ARM lastBeforeCleanup8/next9 has288/101 bytes; MSRV lastLeafClosed7/next8 has254. The next event/payload and exact TCP mechanism remain UNKNOWN. Root verified actual synthetic37d836ce9d2578a0923e2c1e251131ed3a9dcd1f whole tree equalsf8. A separate full Source/lifetime investigation confirms the known producer sends Done then immediately closes its streams without parent receipt, and its six atomic cases have no unread Release command. Successful send is not recipient receipt under the official Winsock contract. Select a bounded existing-clock receipt for the validated prefix through Done and six actual held/live/released controls; preserve every original failure and product/native/cleanup boundary. This is a private protocol correction candidate, not a measured RST cause or production token defect. Rust1.94/1.99 normal Winsock cleanup is empty; WSACleanup attribution would be unsupported.

## Actual token destination refusal without printed error scalars (2026-10-05)

Published321 CI37226872791/1 completed14PASS/3nativeFAIL/2optional with all3 strict Clippy/all5 Unix/Guardian passing and whole actual checkout8930690 tree equal321. All three Server suites improved to54PASS/2FAIL:11 portable functional token selectors and support pass; destination sharing first refuses with a raw value outside the unchanged32/33 predicate, then the constructor selector fails from mutex poison. The actual kind/raw/elapsed/rename/cleanup values and Core failing phase remain UNKNOWN; the second cleanup case was not reached. Source links the destination assertion to validated Done and unchanged successful finish. Separate ARM wake/cancel and MSRV wake causes remain UNKNOWN. Select only the <=256ASCII existing-frame scalar failure message in planning/DASHBOARD_TOKEN_SHARING_OBSERVATION_2026-10-05.md; no accepted-error, ownership, clock or gate change follows.

## Observed destination raw5 and strict real controls (2026-10-05)

Changed652 CI37231275837/1 completed14PASS/3nativeFAIL/2optional. Exact three-row values are PermissionDenied/Some(5), renames1/cleanup1, producer elapsed378/371/418ms; original token/Core clocks and other32/33 callers remain literal. Whole actual checkoutad128398 has parentsmainf90/head652 and equal head tree. Allthree Core131/Engine69/Service64/GUI12 passed; fullStore is unqualified after earlierServer failure, and oldCLI22/24 has separate twoWake failures of unknown cause. These observations belong to652 and do not retroactively measure321's unknown raw values.

Root reviewed [Rust1.94 Windows rename](https://github.com/rust-lang/rust/blob/4a4ef493e3a1488c6e321570238084b38948f6db/library/std/src/sys/fs/windows.rs#L1311) and [Rust1.99 Windows rename](https://github.com/rust-lang/rust/blob/b940084d7eb6a299eb4bfeb8e34901bc051e7ac4/library/std/src/sys/fs/windows.rs#L1321): the byte-identical3258-byte functions may preserve originalMoveFileExW raw5 after a failed secondary DELETE-open/FileRenameInfoEx attempt. Exact Core stage/secondary errno/cause is unknown, and POSIX semantics prevent a universal open-target refusal promise. The unchanged Core held-source tests passed3×3 and differ from held-destination. Select only the scenario-specific exact raw5 plus real std/Core same-candidate identity/nonmutation/release positives and token new-scratch rotation positive in DASHBOARD_NATIVE_DESTINATION_2026-10-05.md; all new controls remain unmeasured pending actual hosted qualification.

## PR144 HTTP qualification continuation (2026-10-04)

Root independently reviewed exacta5e7 with125 immutable artifacts/63 Source snapshots, full API inverse,34 test paths and5 checksum-qualified primary members (proof6dcfb572a514dd74c5a488649552108efd5d460629115a3941a7e3d2c09ce72c). The actual bool/text/null DTO grammar and existing-environment read-only preview fix have no complete new HTTP regression matrix. Select only contract.rs genuine authenticated router controls, absent-state checks and closed-writer durable snapshots against actualmain9de. Source-proven wake exclusion is recorded; actual native wake observation/dev edge is not selected in this tranche. SPEC/runtime/API/UI/old helpers/tests/manifests/workflow remain protected.


## PR144 full incoming f90 compatibility before Git integration (2026-10-05)

Separate read-only research and independent Root proof 4c13f900da8ca5702f6b143e0b15532b1831fd12a3a12caa0f715c2524ae72ae select normal exactf90 integration with no Source adaptation. Preserve complete incoming FINDINGS/IMPLEMENTATION as authoritative prefixes and append only reviewed own/new components once; incoming native histories are not a guessed common9de suffix. Actual mainf90 native22PASS2FAIL remains failed, while the selected own HTTP/macOS Source has not gained new hosted acceptance. Full scope and every Verify are in docs/planning/PR144_MAIN_F90_INTEGRATION_2026-10-05.md.


## Exact HTTP query fixture failure and independent Windows collection (2026-10-05)

CI37221831243/1 has seven actual skipped_overlap-versus-queued failures in the new query test; seeded original was already active under the unchanged skip policy. Root489-artifact/17raw/319 Source proof4dd970d7 binds the full synthetic checkout. Select per-job unqueued fixtures only in that second matrix. Server/Core/Store test dependencies and private router fixtures permit independently collecting native HTTP after cold Core success, retaining the earlier failed job and unknown native causes. See planning/HTTP_QUERY_FIXTURE_ISOLATION_2026-10-05.md before Source.

## Dashboard144 readonly qualification selection (2026-10-05)

Source review and the separate 195-row research distinguish actual Axum422 JSON type refusal from API400 validation, readonly SQLite logical state from WAL/SHM coordination, and API wake attempts from delivered hints. Windows mixed-case previews need a fixed raw legacy seed before measurement because the production setter canonicalizes keys. New controls remain UNMEASURED. The whole selected ledger and independent inputs are in planning/DASHBOARD_BOOLEAN_QUALIFICATION_ROWS.md and DASHBOARD_BOOLEAN_FIXTURE_INPUTS.json; ownership/build/clocks are fixed in DASHBOARD_BOOLEAN_QUALIFICATION_2026-10-05.md. Original CI37178272278/1 passed17 jobs with2 optional skips and does not qualify these new controls.


## Dashboard144 private stage retention review (2026-10-05)

Independent review of unpublished5ec finds that discarded TempPath::keep failures can leave implicit stage deletion enabled in locked tempfile3.27.0. The pure cleanup flag must be disabled before the existing no-clobber publication. No actual deletion or native failure was observed. The three-helper scope, Source conservation and current published1f5 ancestry boundary are frozen in [the supplemental retention plan](planning/DASHBOARD_BOOLEAN_STAGE_RETENTION_2026-10-05.md).
## PR 142 generic refusal review and missing admission qualification (2026-10-04)

Head507777cedd5bd02b162d9b347c2770cdf4585bc0 removes URI interpolation from authentication errors. Both contract.rs token_refusal expectations still contain /api/v1/jobs; ordinary run 37177628448 fails those assertions on eight platform jobs, while existing Server library tests and the other contract cases pass. Preserve the generic response and update those two expectations. Source review also confirms that single_header distinguishes actual absence, multiple fields and undecodable bytes, loopback_port refuses unsupported suffixes and malformed decimal ports, and unsafe Origin requires the actual bound port. Existing tests do not establish the adversarial matrix. Add actual-router admission/body-poll and real HTTP canary controls in PR 142 under Issue 162; PR 145 retains the separate cookie/CSRF slice. The selected tests use synthetic tokens and private ephemeral fixtures only; no installed dashboard credentials or live service are involved.

### PR142 reviewed admission Source/current-main handoff (2026-10-04)

Source4d229e61 preserves the full production middleware prefix and original contract except the two selected generic messages; parent-reviewed tests statically inventory152 rows. Format/locked metadata/diff pass; all new runtime/type/Clippy/native qualification remains pending. A normal current-main c4ef8b90 integration must preserve both Docs histories and the reviewed Source union, followed by fresh candidate/current-base CI and paired provenance. Refs #162; #163.
### PR142 measured embedded URL suffix lint (2026-10-04)

Changed-head37198413636 has14 ordinary successes/three lint failures. Raw Windows/Linux lint identifies middleware831 ends_with(.js) case-sensitive-file-extension lint. Keep the exact lowercase embedded URL rule through rsplit_once plus exact suffix js; preserve all152 rows and other Source. No case relaxation/allow/gate waiver. Root Docs/Issue162 readback precede separate one-assertion Source repair and fresh changed-head qualification.


## PR142 reviewed qualification and exact main138 integration

The [completed integration plan](planning/DASHBOARD_HEADERS_MAIN138_2026-10-04.md) retains actual prior qualification at C4 and selects only the exact main9de Source/Docs union before fresh final-head acceptance. No Source adaptation or prior-result promotion is selected.


## PR142 exact main136 integration selection

PR136 is merged as f90a9d5 with exact reviewed/tested tree and actual ordinary/native/paired acceptance. Follow [the selected integration plan](planning/DASHBOARD_HEADERS_MAIN136_2026-10-04.md) to preserve the reviewed own Source and complete historical document blocks before fresh combined-head qualification. Prior successes do not qualify the new tree.

## Dashboard145 strict cookie/recovery research and selection (2026-10-05)

The complete original198 control research distinguishes missing cookies from malformed and duplicate/unreadable states, actual body polls from delivery, real issuer headers from DOM storage, and plaintext Axum rejections from API envelopes. Original public GET admitted malformed cookies as unauthenticated; preserving that load boundary adds six real entry/script controls without dropping any original key. Selected204=190 backend+14 client controls remain UNMEASURED. Source/CI evidence, primary request/asset code and pin reads establish the narrow recovery route and the need to generate the actually shipped RustEmbed dist. See planning/DASHBOARD_COOKIE_RECOVERY_2026-10-05.md and its complete original/public ledgers. Historical CI37178548023/1 failures remain evidence, not a claim about the future composed candidate.

## Dashboard145 public predicate clarification before Source (2026-10-05)

Both original cc0aaa and reviewed incoming b1d843 actually use GET and !uri.path().starts_with("/api/") for public entry. The initial selection's “outside /api/v1/” wording was incorrect. Preserve that whole existing /api/ predicate and reuse it solely to allow bad-session public loading; /api/other and /api/v2 remain protected. This corrects Docs before integration or Source; all original198 and newPUBLIC6 wires/keys remain unchanged.

## Dashboard145 pinned generation environment before Source (2026-10-05)

The developer observed default Node26.7.0 and pnpm10.34.6 without installing/building. The existing mise Node24.21.0 binary is present and Root directly read its exact version. Select that existing bin directory only in the copied child-command environment for the one isolated frozen install/build and verify both exact pins there. No global PATH/config/shim/tool installation/upgrade is selected. No generation, Source or runtime proof is claimed by these version reads.

## Dashboard145 sentinel handle compatibility before dependent Source (2026-10-05)

Preserved Core create_private_new returns a read/write GuardedFile; Windows open_private_read_stable uses FILE_SHARE_READ. The draft fixture cannot retain that writer while opening a stable reader. The actual API contract selects a same-full-ID handoff after initial sync: capture writer identity, release writer, reopen the known no-follow existing leaf read-only, compare its complete identity and retain that guard. This is Source-backed static compatibility, not a native result. No Core, 204-row ledger, old assertion, generation or deadline changes are selected; the full supplement is planning/DASHBOARD_COOKIE_SENTINEL_HANDOFF_2026-10-05.md.


## Independent Unix sentinel readers (2026-10-05)

Refs #162 / PR145. Root reviewed clean Source0c0086445f98ce8498fdf0ae670deb6a51ac2ce4 and its whole inverses/single generated build. Initial sentinel handoff is no-follow and read-only, but two independent Unix readers still use File::open between symlink_metadata checks. Those calls can follow a leaf link. DASHBOARD_COOKIE_UNIX_NOFOLLOW_2026-10-05.md selects only existing Core no-follow GuardedFile substitutions, the corresponding bounded reader borrow and unused import removal. All204 keys, Windows/production/frontend/generated bytes, clocks and quarantine policy remain frozen; this mismatch is static, not measured native failure.


## Actual Windows token destination raw5 and release-success qualification (2026-10-05)

Current652 ordinary37231275837/1 fails14PASS/3nativeFAIL/2optional at actual PermissionDenied/raw5 (378/371/418ms; wrapper renames1/cleanup1), followed by constructor mutex poison. Root proofc91cab51f7ba4fb5f5c2955f7c89e316223d417ff52bedfd854684c87444f989 binds284 research artifacts,968 canonical inputs, all325 tested mode/blobs and17 original logs. The destination predicate mismatches this returned error; native instruction/ACL/holder/cause and checked failed-root teardown remain unknown. Select destination-only failed-PermissionDenied preservation plus actual same-root released regeneration/full destination and permanent-lock identity witnesses under unchanged clocks. Preserve the shared32/33 and complete unreached scratch/constructor criteria. Independent x64 wake failures and skipped downstream gates remain open.

## Grouped arrivals and exact contributor boundaries (2026-10-05)

Fresh inventory has19 open PRs at unchangedmainf90. Git ancestry confirms newer141946 and1673a descend from selected652/e9d; newer1667429 diverges from localdf30 with only Docs differences and identical whole WinGet Source/CI. New168/169 are two distinct one-line action updates in ci.yml,170 is one Engine dev-dependency plus its exact lock package. The initial16-member/four-group checkpoint remains historical; a fifth dependency group168/169/170 is selected only for read-only research. Preserve all contributions and review new producer diagnostics and action/dependency Sources before new final integration decisions, per GROUPED_PR_ARRIVALS_2026-10-05.md.

## Group A combined ownership and current native boundary (2026-10-05)

The complete groupA read-only handoff8dbdaeef establishes a52-path source/doc union with both divergent144 contributions and actual newer946. Existing parent-owned CreateNew preparation can support193/195 boolean rows without token/Core adapters;145 remains state-missing. All946 held/replacement identity witnesses must be preserved with strict f559 finite native controls. Current946 actual constructor failure follows the first hidden=true loop but its event/Done/native cause is unknown; existing validated scalar reporting can locate it without new reads or changing failure/deadline. GROUP_A_DASHBOARD_INTEGRATION_2026-10-05.md selects the precise union, creator-ID/absence and reporting adaptations; all composed outcomes remain unmeasured.

## Group A measured compile/type admission failures — 2026-10-05

Changed-head3f741 fresh CI37255108990/1 fails before fixture/native execution: borrowed temporary Cookie/Set-Cookie GetAll, moved cleanup String, two strict lint expressions and DOM RenderResult generic mismatch. Actual synthetic550baf8a has ordered f90/3f741 parents and equal tree. Source-backed owned iteration/borrow/empty-unit/literal-delimiter/test-only Pick corrections preserve full policy/oracles; constructor and Windows lifecycle causes remain UNKNOWN. See GROUP_A_COMPILE_ADMISSION_2026-10-05.md.

## Group A initial fixture admission evidence — 2026-10-05

Actual1e0 fresh CI separates three strict lint sites, Unix populated-preview horizon, Windows13cookie fresh-root privacy refusals, PR144 facts no-follow input refusal and Constructor Done-before-event. Full native read-only handoff18a0f794 and locked SQLite/tempfile Source support initial private Unix DB creation, two-directory owned Windows fixture roots, existing-base canonicalization and a closed facts diagnostic; actual causal primitives remain unknown. Complete scope, count correction and Verify are in GROUP_A_FIXTURE_ADMISSION_2026-10-05.md.

## Group A Boolean preparation API boundary — 2026-10-05

Before dependent Boolean Source, actual fresh_root/callers and Core APIs show no parser preparation deadline/finite worker or public directory-handle identity shortcut. Preserve original Boolean root/callers and parser30 origin; narrow WROOT to cookie's existing deadline/full-ID boundary. Empty child removal followed by outer failure retains surviving outer without rollback. Full selected authority is the amended GROUP_A_FIXTURE_ADMISSION_2026-10-05.md.

## Group A Unix-only selector scope (2026-10-05)

Actual04c Windows1.98 raw39817B/SHA2c03536e613126d0d88d74303e803d9bf609154ab30ed8c211e71ae7d4d8f827 contains exactly two dead-code constants, not a measured fixture/native cause. Separate HANDOFF31/27manifest proves all eight identifier references: two Server declarations and two uses inside existing cfg(unix) run_owned, plus four independent Windows native-example references. Select exactly matching cfg(unix) declaration attributes; preserve values, complete bodies, independent controller/example and all other bytes. No allow/expect/remove/new clock or broader preparation is selected. See GROUP_A_UNIX_SELECTOR_SCOPE_2026-10-05.md; hosted acceptance remains pending.
