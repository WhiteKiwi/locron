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
- **An in-product `locron stats` command** aggregating durable run history is a product-behavior change and therefore requires its own SPEC amendment; it is recorded on the deferred product roadmap instead of being folded into this tooling.

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
- **Framework.** axum 0.8.9 (MSRV 1.80 — below locron's 1.94, no blocker) with tower-http 0.7.0 for bundled static assets and tokio-stream 0.1.19 for stream adapters; hyper 1.11.0 underneath. Nothing beyond that stack is needed for this surface.
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

