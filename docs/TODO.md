# locron Milestone 1 TODO

This checklist tracks implementation of the frozen `docs/SPEC.md` within the durable structure in `docs/ARCHITECTURE.md` and the milestone approach in `docs/IMPLEMENTATION.md`. It does not include packaging, the HTTP management/viewer surface, MCP, or desktop work.

If a planned implementation decision changes, update and review `docs/IMPLEMENTATION.md` and this checklist before changing code. Update `docs/ARCHITECTURE.md` first for a durable structure/invariant change and `docs/SPEC.md` first for an observable behavior/scope change.

## 1. Close implementation decisions

- [x] Review and accept or replace every implementation recommendation in `docs/IMPLEMENTATION.md`.
- [x] Record the accepted Rust edition 2024, resolver 3, Rust 1.94 MSRV, CI toolchains, and official macOS/Linux platform floor.
- [x] Record `locron-engine` as daemon-runtime owner and `locron-cli` as the thin entrypoint exposing `locron daemon run` in the single v1 binary.
- [x] Select UUIDv7 identity roles, canonical text encoding, UTC epoch-microsecond instants, RFC 3339 rendering, and monotonic elapsed-time measurement.
- [x] Select per-attempt framed output files, SQLite-owned metadata, bounded capture behavior, atomic finalization, and restartable recovery/pruning.
- [x] Select bundled SQLite WAL/FULL operation, connection bounds, busy/degraded behavior, migration coordination, and permanent OS-locked daemon ownership.
- [x] Select SQLite-authoritative CLI commands, best-effort Unix datagram wakeup, safety reconciliation, and polling-based wait/follow.
- [x] Select durable round-robin fairness, ordered catch-up admission, and single-candidate replacement coalescing.
- [x] Select SQLite table layout, dependency set, state paths, CLI shape, dry-run/why diagnostics, JSON/export versioning, and exit-code categories.
- [x] Record the accepted decisions and trade-offs in `docs/IMPLEMENTATION.md`, `docs/CLI.md`, and `docs/STORAGE.md` before creating manifests or migrations.

**Verify:** from the repository root, `rg -n "Draft recommendation|unresolved|remain.*select|need.*review" docs/IMPLEMENTATION.md docs/CLI.md docs/STORAGE.md` returns no undecided item required by workspace, schema, or CLI work, and the reviewed documents conform to `docs/SPEC.md`.

## 2. Establish the Rust workspace

- [x] Create the Rust edition 2024, resolver-3 virtual workspace with exactly `locron-core`, `locron-store`, `locron-engine`, and `locron-cli`.
- [x] Set Rust 1.94 as workspace MSRV and configure shared package metadata, dependencies, profiles, formatting, lints, and one lockfile.
- [x] Run CI on Rust 1.94 and latest stable and enforce the dependency direction documented in `docs/ARCHITECTURE.md`.
- [x] Keep one v1 distributable `locron` binary with `locron daemon run`; do not create `locrond`, a `locron-daemon` crate, or future-surface crates.

**Verify:** from the repository root, workspace metadata reports edition 2024, resolver 3, `rust-version = "1.94"`, exactly the four approved members, and one `locron` binary; format, compile, lint, and test commands pass on Rust 1.94 and latest stable; dependency inspection shows no forbidden crate edge or daemon crate/binary.

## 3. Implement and test the domain core

- [x] Add stable identifiers, job/revision/schedule/target/policy/run/attempt/event types and application commands/results.
- [x] Add cross-field validation and normalization for exactly one schedule and target, all policy bounds, paths, environment, HTTP configuration, and reserved names.
- [x] Implement pure cron, interval, and one-time occurrence enumeration with injected clock/timezone inputs.
- [x] Implement explicit legal run/attempt transitions and persistence, clock, and executor ports.
- [x] Update the applicable planning document first if a discovered edge case requires behavior or structure not represented in the current plan.

**Verify:** core unit/property tests cover aliases, cron day OR behavior, interval anchors, schedule revisions, DST gaps/repetitions, clock movement, duration overflow, invalid combinations, and every state transition without SQLite or real-time sleeps.

## 4. Implement durable SQLite state

- [x] Add versioned migrations for the durable model and invariants in `docs/ARCHITECTURE.md` using the accepted choices from `docs/IMPLEMENTATION.md`.
- [x] Implement atomic job revision/cursor mutation, manual run creation, reconciliation materialization, admission, attempt completion/retry, cancellation intent, recovery, and retention operations.
- [x] Prove queued/retry-wait cancellation terminalizes atomically while active cancellation remains
  durable intent, with stable not-found/conflict results.
- [x] Enforce scheduled occurrence uniqueness, ordered attempt uniqueness, foreign keys, soft deletion, and legal persisted states.
- [x] Add single-scheduler ownership and safe concurrent CLI access.
- [x] Implement output metadata and the accepted output-storage consistency protocol.

**Verify:** integration tests against temporary real databases pass for clean and upgrade migrations, concurrent writers, rollback injection, duplicate occurrence insertion, cursor/run atomicity, stale lifetime recovery, soft-delete history, busy/disk failure handling, and interrupted output finalization.

## 5. Implement scheduler and admission semantics

Current acceptance tranche (2026-08-21):

- [x] Add explicit operator acknowledgement for `termination_unconfirmed` quarantine without stale
  process signalling. **Verify:** store and CLI tests prove exact-state-only atomic release,
  acknowledgement event/history/why visibility, ordinary-cancel guidance, no PID/PGID access,
  non-quarantine rejection, and stable repeated conflict.
- [x] Apply durable global concurrency changes to a running daemon. **Verify:** deterministic daemon
  tests change 1→3→1→3 while attempts are active, prove no restart or active cancellation, zero
  admission below active count, prompt expansion, hard maximum 64, and no store/semaphore over-admit.
- [x] Complete the overlap/admission/retry matrix for this tranche. **Verify:** table-driven tests
  cross `skip|replace|allow` with scheduled/manual/catch-up, zero/global/per-job capacity and
  reductions; retry-wait versus a normal occurrence and retries beyond the original start deadline
  remain deterministic and explainable.
- [x] Exercise catch-up limit 1,000 through reconciliation and durable admission. **Verify:** a real
  adapter/store test materializes exactly the newest 1,000 rows, executes/admit-orders them oldest
  first, emits one compact exact omitted-range summary, and duplicate reconciliation is idempotent
  without additional rows or events.
- [x] Complete deterministic lifecycle fault-boundary coverage feasible in this tranche. **Verify:**
  injected store/executor tests cover before admission, starting before spawn, running after spawn,
  outcome before completion, and one-time restart uniqueness without sleep-heavy timing or unknown
  retry.

Prior correctness tranche whose broad verification clauses remain open (2026-08-21):

- [x] Replace elapsed calendar scanning with bounded newest-window reconciliation and compact exact
  range summaries shared by cron, interval, and one-time schedules.
  **Verify:** pure tests cover long sparse ranges, deadline-before-policy ordering, limits 1 and
  1,000, newest selection/oldest execution, DST gap/fold, backward/forward wall moves, local-zone
  replacement, disable/re-enable, explicit recovery versus steady-state boundaries, exact cutoff and
  1-microsecond deadline edges, and duplicate reconciliation without sleeps.
- [x] Centralize retry classification/backoff and prove durable `retry_wait` restart behavior and all
  forbidden retry classes.
  **Verify:** deterministic clock/store tests cover fixed and capped exponential delay, retry count
  exhaustion, timeout opt-in, restart eligibility, cancellation/configuration/replacement/unknown
  exclusion, and deadline interaction.
- [x] Make replacement supersession and confirmation fully durable and exercise admission capacity
  interactions.
  **Verify:** store/engine tests cover newest-only `skipped_overlap` supersession, queued/retry-wait
  replacement without signal, active cancellation intent, confirmation failure, catch-up isolation,
  and `skip|replace|allow` across scheduled/manual/catch-up and changed capacity.
- [x] Add deterministic lifecycle fault boundaries without acting on stale process identities.
  **Verify:** injected faults before spawn, after admission, while running, and after target exit show
  `interrupted_unknown`, no unknown retry, and no duplicate one-time occurrence.

- [x] Implement the long-lived daemon runtime in `locron-engine`, including lifetime/lock ownership, loop coordination, signals, bounded maintenance, and graceful shutdown.
- [x] Reconcile startup, wake, ticks, job revisions, disabled intervals, and wall-clock changes from durable cursors.
- [x] Prove due one-time resolution disables atomically, downtime catch-up remains unique, and manual
  submission neither consumes nor disables the schedule.
- [x] Apply start deadline and `skip|latest|all`, including newest bounded selection, oldest-first catch-up execution, and bounded summary events.
- [x] Apply `skip|replace|allow`, queued/retry-wait active accounting, global default 16/range 1..64, and per-job limits.
- [x] Implement known-failure retry classification, fixed/capped-exponential delay, durable retry wait, and no retry for unknown outcomes.
- [x] Implement cancellation/replace intents and startup classification as `interrupted_unknown` without stale PID action.
- [x] Update `docs/IMPLEMENTATION.md` and `docs/TODO.md` before a milestone approach change; update `docs/ARCHITECTURE.md` first if the durable engine boundary or lifecycle invariant changes.

**Verify:** deterministic fake-clock/fake-executor suites pass for downtime, sleep, enable/disable, DST, backward/forward clock movement, bounded catch-up, every overlap/concurrency combination, capacity changes, retry/restart, cancellation, replacement confirmation failure, and duplicate-free one-time recovery.

## 6. Implement target runners and output capture

- [x] Implement direct argv and explicit-shell execution with normalized CWD and effective PATH resolution.
- [x] Build the minimal/layered environment and inject reserved `LOCRON_*` values last.
- [x] Create and supervise process groups with timeout, TERM grace, KILL escalation, cancellation, replacement, and graceful daemon shutdown.
- [x] Implement HTTP methods, absolute URLs, bodies/files, headers/env headers, success ranges, TLS, redirect policy, timeout, and retry classification.
- [x] Persist the final HTTP response content type with each attempt and expose it through durable
  history/diagnostics without retaining other response headers.
  **Verify:** runner tests cover present, absent, and redirect-final content types; clean/upgrade
  migration and attempt-history tests prove the value survives restart and machine output.
- [x] Prove `301`/`302`/`303` redirect method rewriting and `307`/`308` method/body preservation while
  retaining cross-origin sensitive-header stripping.
- [x] Stream, follow, truncate, finalize, and prune output under approved limits without blocking target completion.
- [x] Prove a post-admission runtime-file/configuration failure becomes a non-retryable terminal run
  with consistent finalized output rather than an orphaned running attempt.

**Verify:** real process-tree and local HTTP fixture tests pass on macOS and Linux for argv/env/CWD/PATH, grandchildren, TERM/KILL, disappearing runtime files, redirects across origins, TLS, response classes, timeout while streaming, output truncation, redaction boundaries, and target exit/result mapping.

## 7. Implement the thin CLI composition and commands

- [x] Implement job CRUD, enable/disable/remove, schedule preview, history/show/logs, and cancellation.
- [x] Complete shared add/update normalization for metadata, schedules, every target/environment
  option, policy bounds, current global concurrency, cursor boundaries, dry-run diff, and no-op
  rejection.
- [x] Complete typed redacted/plaintext export and whole-document atomic import with acknowledgement,
  collision planning, rollback, and fresh-state round trip tests; keep history explicitly deferred.
- [x] Implement durable offline manual enqueue, run ID output, wait/follow behavior, and outcome exit mapping.
- [x] Implement import/export with explicit env-value acknowledgement, pruning, and diagnostics.
- [x] Expose `locron daemon run` as a thin composition entrypoint into the daemon runtime owned by `locron-engine`.
- [x] Provide equivalent versioned machine-readable results without requiring prose parsing.
- [x] Implement non-mutating dry-run, durable-fact `why`, repeatable verbose context, and redacted debug tracing.
- [x] Ensure CLI handlers call shared application validation rather than duplicating policy.

**Verify:** CLI contract tests pass for human and machine modes, all command families, offline enqueue, disabled/manual/one-time behavior, client disconnect without cancellation, redaction, import/export round trip, soft removal, invalid/conflicting options, diagnostics, and stable error categories; workspace inspection finds only the `locron` binary and confirms `locron daemon run` delegates to `locron-engine`.

## 8. Prove crash, retention, and resource safety

- [x] Inject daemon death before spawn, after the durable running commit and spawn, while running,
  and after target exit/before final commit.
- [x] Exercise retention age/count/byte limits and interrupted pruning/finalization.
- [x] Stress global concurrency 16 and maximum 64, large elapsed intervals, maximum catch-up 1,000, noisy output, and SQLite contention.
- [x] Run process-group and service-lifetime checks on macOS 14+ and Linux kernel 5.14+/glibc 2.34+
  for `aarch64` and `x86_64`. **Evidence:** GitHub Actions run
  [32506527959](https://github.com/WhiteKiwi/locron/actions/runs/32506527959) passed the complete
  Rust 1.94/stable matrix on all four official platform targets.
- [x] Treat Windows, 32-bit, and musl/Alpine results as deferred/informational rather than v1 release gates.
- [x] Update the applicable `docs/ARCHITECTURE.md`, `docs/IMPLEMENTATION.md`, and `docs/TODO.md` content before applying any platform-driven design deviation.

**Verify:** fault/stress reports demonstrate durable occurrence identity, correct `interrupted_unknown` classification, no implicit retry or duplicate one-time run, bounded materialization/storage, correct eviction order, no active-run pruning, and documented cross-platform process cleanup guarantees.

## 9. Complete documentation and milestone acceptance

- [x] Document installation-from-source and operation of the program milestone without claiming package-manager support.
- [x] Document schedule, overlap, missed-run, retry, concurrency, timeout/cancellation, crash, retention, plaintext secret boundary, and exactly-once limitations.
- [x] Document diagnostics and recovery procedures plus human and machine CLI examples.
- [x] Map every frozen completion criterion to automated or cross-platform verification evidence.
- [x] Review that deferred viewer/API, MCP, desktop, package publication, and service installation work did not enter milestone 1.
- [x] Keep durable future decisions under `docs/decisions/` only when a reviewed ADR is needed; split CLI/storage contracts only after review justifies dedicated documents.

**Verify:** a completion matrix links all 16 `docs/SPEC.md` criteria to passing evidence on the official macOS/Linux platform matrix; documentation examples execute successfully; architecture and implementation cross-links resolve; repository search and dependency/workspace inspection find no deferred surface implementation or premature empty ADR document.

## Follow-up CLI acceptance backlog

- [x] Complete the per-option help surface: every argument of every command renders a concise
  description, semantic value names (`<EXPR>`, `<DURATION>`, `<METHOD> <URL>`, `<NAME=VALUE>`),
  possible-value help for every value enum, and update-only markers on update-only flags, without
  changing parsing behavior or the frozen contract in `docs/CLI.md`.
  **Verify:** a unit test walks the generated Clap command tree and asserts every non-hidden
  argument has non-empty help text; `cargo test -p locron-cli` passes, and manual inspection of
  `locron add -h`, `locron update -h`, and `locron --help` shows usage, examples, and described
  options.

- [x] Audit and complete the entire CLI help surface without expanding the current scheduler
  semantics tranche. Cover `locron help`, `locron -h`, `locron --help`, and every direct command's
  `<cmd> help` form where Clap supports it, `<cmd> -h`, and `<cmd> --help`. Help must succeed without
  otherwise-required arguments, exit zero, show the command's options and useful examples, provide
  a route back to parent/top-level navigation, and follow an explicit stdout/stderr contract.
  **Verify:** generate the complete Clap command tree in an acceptance test and exercise every
  supported help spelling automatically so a newly added command or nested command cannot omit help
  coverage.

- [x] Exclude live-lifetime starting/running attempts from output recovery. A maintenance pass
  previously selected every `pending`/`active` output artifact, so a just-admitted attempt whose
  `.partial` file did not exist yet could be reconciled as `missing` before the runner created it,
  permanently blocking finalization and leaving the run stuck `running`. Recovery now requires the
  attempt to be terminal or owned by a lifetime other than the live daemon's.
  **Verify:** a store regression test proves a `pending` artifact owned by the current lifetime is
  never returned as a recovery candidate while the same artifact owned by a different (dead)
  lifetime is; `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  and `cargo test --workspace` pass; the `acceptance_matrix` binary passes 12 consecutive runs
  under CPU load with 0 failures.

## Version output backlog (2026-08-23)

- [x] Amend `docs/SPEC.md`, `docs/CLI.md`, and `docs/IMPLEMENTATION.md` for `-V/--version`
  machine output. **Verify:** `docs/SPEC.md` records the amended version-reporting behavior;
  `docs/CLI.md` documents the flag, its human output, and its JSON envelope; and
  `docs/IMPLEMENTATION.md` records the flag-ownership approach and the preserved clap failure
  surfaces.
- [x] Implement top-level `-V/--version` handling in `crates/locron-cli` with
  `disable_version_flag`, an `Option<Command>` subcommand, and the reproduced
  `arg_required_else_help`/`MissingSubcommand` fallbacks. **Verify:** `cargo build -p
  locron-cli`, `cargo fmt --check`, and `cargo clippy -p locron-cli` pass; manual runs show
  `locron -V` and `locron --version` printing `locron <CARGO_PKG_VERSION>` on stdout and exiting 0.
  **Evidence:** all three pass with no warnings; a byte-level comparison against the v0.1.1
  baseline shows identical output for bare `locron`, `-v`, `add -V`, `-h`/`--help`, and plain
  `-V`/`--version`, with exit codes preserved.
- [x] Add contract tests in `crates/locron-cli/tests/version.rs`. **Verify:** `cargo test -p
  locron-cli` passes, covering human plain text; `--format json` and `--json` envelopes
  (`locron.cli/v1`, `ok: true`, `command: "version"`, `data.version == CARGO_PKG_VERSION`, empty
  warnings); exit code 0; no state-dir access; plus regressions: bare `locron` renders full help to
  stderr with exit 2, `locron -v` fails with the missing-subcommand error and exit 2, and `locron
  add -V` fails as an unexpected argument with exit 2.
  **Evidence:** 114 `locron-cli` tests pass, including the 10 new `version.rs` contract tests.
- [x] Run the workspace verification. **Verify:** `cargo test --workspace` and `cargo clippy
  --workspace` pass, and the help-surface acceptance tests still pass with the new flag.
  **Evidence:** the full workspace test suite and clippy pass; the clap help-surface acceptance
  tests pass unchanged.

## Workflow node24 migration backlog (2026-08-23)

- [x] Record the node24 action migration here and confirm `docs/RELEASE.md` needs no change (its CI
  section already requires up-to-date action versions). **Verify:** `docs/RELEASE.md` line "Check
  out repository with up-to-date action versions" covers the change; the superseded milestone-1
  `actions/checkout@v4` Verify clause above points at this section.
- [x] Bump the flagged node20 actions to their node24 majors in `.github/workflows/ci.yml` and
  `.github/workflows/release.yml`: `actions/checkout@v4`→`@v7`, `actions/upload-artifact@v4`→`@v7`,
  `actions/download-artifact@v4`→`@v8`. Keep input usage unchanged (`name`/`path`/`merge-multiple`,
  plain checkout). **Verify:** `rg -n "checkout@v4|upload-artifact@v4|download-artifact@v4"
  .github` returns nothing; both files parse as YAML; the v7/v8 input names used are present in the
  published action.yml files.
  **Evidence:** commit `07d6a79` changed exactly five version strings; both files parse as YAML;
  no v4 references remain.
- [x] Push to `main` and confirm the CI matrix passes with no Node 20 deprecation warnings. The
  tag-only release workflow cannot be dry-run without publishing; its publish path is verified by
  review and the next real release. **Verify:** the `ci.yml` run on `main` completes green on all 8
  matrix legs and `gh run view <run> --log | grep "Node.js 20 is deprecated"` returns no matches.
  **Evidence:** run
  [32613812071](https://github.com/WhiteKiwi/locron/actions/runs/32613812071) concluded `success`
  across the full matrix; the log contains zero `Node.js 20 is deprecated` warnings.

## Daemon robustness backlog (2026-08-23)

- [x] Replace the fixed 30-second event-loop wait with the documented earliest-deadline wait.
  The loop must read the earliest pending admission deadline (queued or retry-wait run) durably
  and sleep until that deadline or the 30-second safety reconciliation, whichever is earlier; an
  attempt completion must notify the loop so a freshly scheduled retry deadline is observed
  without reconciliation delay. **Verify:** engine tests drive the loop with injected time and
  prove admission at the deadline rather than at the next 30-second boundary, and that a
  completion wake is observed after an attempt commits; store tests cover the
  earliest-deadline query. **Evidence:** engine tests
  `run_until_ticks_at_earliest_pending_admission_deadline` (paused time: ticks at the 1s
  admission deadline, not the 30s safety boundary) and
  `attempt_completion_notifies_the_wake_handle`; store test
  `earliest_pending_eligible_at_us_covers_queued_and_retry_wait_runs` (empty, queued-only,
  retry-wait-only, mixed-earliest, terminal-excluded).
- [x] Distinguish permanent completion conflicts from transient persistence errors in the
  engine port and stop retrying them. A permanent conflict logs once and terminalizes the
  attempt as `interrupted_unknown` where the store permits instead of pinning the run in
  `running`. Completion idempotency checks compare durable identity fields, never the retry
  timestamp. **Verify:** engine tests prove a conflicting completion does not retry forever
  and the run reaches a terminal state; store tests cover timestamp-independent idempotent
  recompletion. **Evidence:** engine tests
  `permanent_completion_conflict_falls_back_once_and_never_retries` (exactly one fallback
  `complete_runner_failure(ExecutionMayHaveStarted)`, attempt terminal as
  `interrupted_unknown`, 5s timeout guard) and `permanent_runner_failure_conflict_breaks_without_retry`;
  store tests `finalize_output_reconciliation_is_idempotent_across_timestamps`,
  `runner_failure_recompletion_is_idempotent_across_timestamps`, and
  `runner_failure_terminalizes_an_attempt_whose_output_was_already_missing`.

## Post-milestone delivery backlog

These items begin only after every milestone-1 completion criterion above is satisfied. They do not
change the package-publication exclusion in `docs/SPEC.md`, and this milestone does not implement
their workflows or release infrastructure.

- [x] Define CI/CD policy and version/release policy. **Verify:** `docs/RELEASE.md` defines SemVer lockstep
  workspace versioning, immutable tag conventions, official 4-target artifact matrix, SHA-256
  checksums, CI/CD pipeline contracts, Homebrew/Linux packaging integration, and rollback/remediation
  procedures.
- [x] Complete GitHub build/test CI and GitHub Releases for release operations, including macOS and
  Linux architecture artifacts, checksums, provenance, signing policy, and rollback policy; review
  the next supported major of `actions/checkout` to remove the current Node 20 deprecation
  annotation. **Verify:** `.github/workflows/ci.yml` uses an up-to-date `actions/checkout` major
  across 4 official platforms (migrated from v4 by the "Workflow node24 migration backlog" below);
  `.github/workflows/release.yml` implements automated 4-target matrix builds, tar.gz
  packaging with README and dual licenses, SHA-256 checksum generation, and GitHub Release publication.
- [x] Publish the package through `whitekiwi/homebrew-tap`. **Verify:** `Formula/locron.rb` in
  `whitekiwi/homebrew-tap` defines multi-platform URL and SHA-256 targets for macOS and Linux on arm64
  and x86_64 with install and test blocks.
- [x] Publish apt/deb and yum/rpm-family packages through supported repositories. **Verify:**
  `crates/locron-cli/Cargo.toml` specifies deb and generate-rpm metadata, and
  `.github/workflows/release.yml` automatically builds `.deb` and `.rpm` packages on Linux runners and
  attaches them to GitHub Releases.
- [x] Automate Homebrew tap releases from approved version tags. **Verify:** `.github/workflows/release.yml`
  automatically computes SHA-256 sums for macOS/Linux archives and updates `Formula/locron.rb` in
  `whitekiwi/homebrew-tap`.
- [x] After milestone 1 and Homebrew delivery are complete, update this repository's README for
  install, upgrade, operation, and troubleshooting, and update `whitekiwi/homebrew-tap`'s README for
  tap/install, supported versions, and release provenance, with reciprocal links. **Verify:**
  `README.md` incorporates the banner asset from `assets/banner.jpg`, comprehensive badges, multi-channel
  installation instructions (Homebrew, deb/rpm, binary tarballs, cargo source), quick start,
  cross-links to `docs/OPERATOR.md`, `docs/CLI.md`, `docs/RELEASE.md`, and reciprocal link to
  `whitekiwi/homebrew-tap`.

## Installer and self-update backlog (2026-08-23)

Authorized by the frozen 2026-08-23 `docs/SPEC.md` amendment (installation channels and self-update). Planned in `docs/IMPLEMENTATION.md` "Installer and self-update implementation"; evidence in `docs/FINDINGS.md` §11.

- [x] Amend planning documents before code: SPEC (frozen), FINDINGS §11, IMPLEMENTATION section.
  **Verify:** `rg -n "Installation Channels|11\. One-line Installer|Installer and self-update implementation" docs/SPEC.md docs/FINDINGS.md docs/IMPLEMENTATION.md` returns the amendment, the evidence section, and the implementation section, and no planning document marks an unresolved decision.
- [x] Add `install.sh` (POSIX `sh`, repository root): target detection, latest/pinned resolution through `releases/latest/download` redirects, `SHA256SUMS.txt` verification, atomic replace into `$HOME/.local/bin` (or `LOCRON_INSTALL_DIR`), PATH guidance, actionable errors.
  **Verify:** `sh -n install.sh` passes; fixture-driven runs (asset-base override) cover latest, pinned, checksum mismatch, unsupported arch, and unwritable dir with actionable errors and exit 1; a real `LOCRON_VERSION=v0.1.1` run on macOS arm64 installs a working binary, and re-running stays idempotent. shellcheck was unavailable locally — carried in the follow-up below.
- [x] Smoke the real v0.1.1 release through the script.
  **Verify:** `LOCRON_VERSION=v0.1.1 LOCRON_INSTALL_DIR=<tmp>/bin/locron sh install.sh` exits 0, prints `Installed locron v0.1.1 to ...`, the binary answers `locron 0.1.1`, and a re-run replaces it idempotently (macOS arm64). The Linux x86_64 leg is carried in the follow-up below.
- [x] Implement `locron self-update` in `locron-cli` (deps `sha2`, `tar`, `flate2`, plus `reqwest` rustls/stream/json as the TLS client; `LOCRON_UPDATE_API_BASE`/`LOCRON_UPDATE_ASSET_BASE` test seams): latest resolution, checksum verification, atomic self-replace, marker refusal (`lib/.disable-self-update`), human and `locron.cli/v1` output, stable error mapping.
  **Verify:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test -p locron-cli` pass; the real-API smoke `locron --json self-update` returns the `locron.cli/v1` envelope with `current_version`/`new_version`/`updated` and exit 0. The reqwest addition is recorded in `docs/IMPLEMENTATION.md`.
- [x] Self-update contract tests against a local fixture.
  **Verify:** 9 contract tests green against a local std-TcpListener fixture: latest install (JSON envelope; replaced binary runs), human output, already-up-to-date without asset downloads, checksum-mismatch no-op leaving the old binary untouched, atomic replace while a live child process holds stdin, marker refusal with brew guidance, rate-limit mapping, and missing-asset/malformed-checksum metadata errors.
- [x] Add the marker line to the formula template in `.github/workflows/release.yml` (`lib.mkpath` + `FileUtils.touch lib/.disable-self-update`) and attach `install.sh` to GitHub Releases (both `gh release create` and `gh release upload` paths).
  **Verify:** the workflow parses as valid YAML and the template contains the marker creation. The generated-formula and manual `brew reinstall locron && locron self-update` refusal checks are deferred to the next real tag (follow-up below).
- [x] Update documentation: README one-liner and per-channel update story; `docs/CLI.md` `self-update` contract; `docs/RELEASE.md` channel and asset additions; operator note that a running daemon keeps the old code until restart.
  **Verify:** all four documents updated; CLI contract tests pass with the new subcommand; the frozen SPEC wording is followed.
- [x] Run full workspace verification.
  **Verify:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` all pass locally. The four-target CI matrix run is deferred until the change is pushed (follow-up below).

### Follow-up (open)

The four-target CI matrix is already green: run [32617992398](https://github.com/WhiteKiwi/locron/actions/runs/32617992398) concluded `success` on commit `4dc570c` after the ETXTBSY and fixture-race fixes (the earlier run 32617079023 failed only on the two platform-specific test-harness legs now fixed).

- [ ] Serve the one-liner under a short custom domain in the mise.run style (e.g. `https://install.locron.dev` → `releases/latest/download/install.sh`) once a domain and hosting are available; the GitHub release URL remains canonical until then.
  **Verify:** `curl -fsSL <domain> | sh` installs a working binary; README documents the domain one-liner.
- [x] Close the installer CI verification gaps: shellcheck, unsupported-platform/musl refusals, and the Linux install leg, via the new `installer` job in `.github/workflows/ci.yml`.
  **Verify:** the `installer` job passes on `main`; the log shows shellcheck clean, the three refusal paths with their exact messages, and the pinned `v0.2.0` install answering `locron -V`.
  **Evidence:** run [32621273837](https://github.com/WhiteKiwi/locron/actions/runs/32621273837) concluded `success` on commit `b12e5c7` (which fixed the SC2295 finding shellcheck 0.9.0 flagged in `install.sh`); the `installer` job and all 8 matrix legs are green.
- [ ] At the next real tag: verify the published formula creates the marker (`brew reinstall locron && locron self-update` refuses with brew guidance) and the release carries `install.sh`.
  **Verify:** next-tag marker/release evidence recorded in this section.
- [x] Minor cleanup: the HOME-unset guard at the top of `install.sh` is dead code after the default expansion and can be removed. Deferred to the session currently editing `install.sh` (service-registration wiring) to avoid file conflicts.
  **Evidence:** the guard was removed in the service-registration session; `sh -n install.sh` passes and the 4 `tests/install_sh.rs` fixture tests still pass unchanged.

## Daemon service installation backlog (2026-08-23)

Authorized by the frozen 2026-08-23 `docs/SPEC.md` amendment (Daemon Service Installation). Planned in `docs/IMPLEMENTATION.md` "Daemon service installation implementation"; evidence in `docs/FINDINGS.md` §12.

- [x] Amend planning documents before code: SPEC (frozen), FINDINGS §12, IMPLEMENTATION section, and this checklist.
  **Verify:** `rg -n "Daemon Service Installation|12\. Daemon Service|Daemon service installation implementation" docs/SPEC.md docs/FINDINGS.md docs/IMPLEMENTATION.md` returns all three sections (SPEC amendment line 9 + section, FINDINGS §12, IMPLEMENTATION section); `rg -n "Draft|unresolved|need.*review" docs/IMPLEMENTATION.md` finds no undecided item in the new section.
  **Evidence:** all checks pass; the SPEC amendment line records this amendment.
- [x] Add the service-manager port to `locron-cli`: launchd and systemd-user backends behind a shared port plus a deterministic fake; embed the plist/unit templates (label `dev.locron.daemon`, `locron.service`, canonicalized `current_exe` path, `KeepAlive`/`RunAtLoad`, `Restart=on-failure`/`WantedBy=default.target`, `~/Library/Logs/locron/daemon.log`).
  **Verify:** unit tests render both templates with a canonicalized binary path and the required keys; `cargo fmt --check` and `cargo clippy -p locron-cli` pass; workspace inspection still finds only the `locron` binary and no new dependency.
  **Evidence:** 15 unit tests in `service::tests` pass, including `plist_template_renders_required_keys_and_paths` and `unit_template_renders_required_keys_and_paths`; `cargo fmt --all --check` and `cargo clippy -p locron-cli --all-targets -- -D warnings` pass; `locron-engine`/`locron-store` Cargo.toml unchanged (no new dependency).
- [x] Implement `locron service install|uninstall|status` over the port with human and `locron.cli/v1` output, brew-marker refusal, lock-held deferral, and no-session guidance (exit 0).
  **Verify:** fake-port contract tests cover install idempotency, refresh-and-restart when loaded, deferral when the state lock is held by a non-service daemon, uninstall signal-then-bootout ordering, status fields, marker refusal with brew guidance, no-session guidance exit 0, and JSON envelopes; `cargo test -p locron-cli` passes.
  **Evidence:** 11 contract tests in `tests/service.rs` pass (install ordering, refresh restart, flock deferral, uninstall ordering, status fields, guidance exit 0 human+JSON, marker refusal exit 3 `service_managed_install`, forced-unsupported exit 2, help text); `cargo test -p locron-cli` fully green.
- [x] Implement the launchd backend (`enable`/`bootstrap`/`print`/`kill`/`bootout`, gui→user domain fallback) and the systemd backend (`daemon-reload`/`enable --now`/`stop`/`disable`/`is-active`, user-manager detection via `XDG_RUNTIME_DIR` + bus probe). Deviations recorded in `docs/IMPLEMENTATION.md`: a loaded unit is refreshed with `stop` then `enable --now`, because `systemctl start` on an already-active unit is a no-op and a bare `enable --now` would never restart a loaded daemon onto a new binary; `launchctl kill`/`bootout` exit 3 ("no process to signal"/already unloaded, hit in the KeepAlive respawn window) is treated as success; uninstall waits for the signaled process to exit (pid gone or replaced by a respawn) rather than for the job to leave the domain, which KeepAlive jobs never do until `bootout`.
  **Verify:** real-backend tests on the macOS CI leg register, gracefully restart (SIGTERM + KeepAlive relaunch observed via a marker process), and unregister in the available domain; the Linux CI leg drives the backend against a real user manager under `dbus-run-session`; a stripped-environment test proves the no-session guidance path.
  **Evidence:** `tests/service_backends.rs` passes on macOS — `macos_launchd_backend_registers_restarts_and_unregisters` ran against the real gui/501 domain (plist written, log dir created, lock-pid marker changed after refresh, job left the domain on uninstall); `linux_systemd_leg_reports_what_cannot_run_here` reported the systemctl/dbus-run-session absence. Linux real leg still CI-only (see report).
- [x] Wire install.sh (attempt registration after replace unless `LOCRON_NO_SERVICE=1`, pass output through) and self-update (post-replace `service install`). Deviation recorded in `docs/IMPLEMENTATION.md`: a non-zero registration exit (other than the guidance-exit-0 case) also leaves the install successful — the script warns and continues, since the binary replacement is the essential install and registration is best-effort.
  **Verify:** installer fixture tests on macOS and Linux legs cover default registration, `LOCRON_NO_SERVICE=1` skip, and guidance-exit tolerance; self-update contract tests prove the service refresh happens only after a successful replace and performs first registration when none exists.
  **Evidence:** 4 fixture tests in `tests/install_sh.rs` pass (offline `file://` release host; default registration records one `service install` after the replace, `LOCRON_NO_SERVICE=1` records none, guidance-exit-0 passes through without a warning, and a failed registration warns with the retry command while the install stays successful); 9 `tests/self_update.rs` tests pass — the successful-replace tests assert the post-replace `service install` invocation happened exactly once, and the no-replace tests (already up to date, checksum mismatch) assert no registration; `cargo fmt --all --check` and `cargo clippy -p locron-cli --all-targets -- -D warnings` pass.
- [x] Add the brew `service` block (`run [opt_bin/"locron", "daemon", "run"]`, `keep_alive true`, `run_at_load false`) and a `brew services start locron` caveat to the release.yml formula template; add deb/rpm postinst guidance.
  **Verify:** the workflow YAML parses; the template contains the service block; a built .deb inspected with `dpkg-deb` contains the guidance text; the generated-formula check is deferred to the next real tag.
  **Evidence:** `yaml.safe_load` on `.github/workflows/release.yml` succeeds; the template contains `service do`, `keep_alive true`, `run_at_load false`, and the `brew services start locron` caveat; a locally built `target/debian/locron_0.2.0-1_arm64.deb` (cargo-deb 3.7.0, release build) was inspected with `ar`/`tar` (no `dpkg-deb` on this macOS host) and its `control/postinst` is mode 755, passes `sh -n`, and contains the guidance text; `rpm-scripts/postin` also passes `sh -n`. The generated-formula check stays deferred to the next real tag.
- [x] Update documentation: `docs/CLI.md` service-family contract, `docs/OPERATOR.md` registration and linger guidance, `docs/RELEASE.md` formula/package additions, and the README install section.
  **Verify:** documentation examples execute; cross-links resolve; CLI contract tests cover the new command family.
  **Evidence:** `docs/CLI.md` gains the command-family line and a "Service installation" section (platforms, refresh/deferral/no-session/brew-refusal semantics, install/uninstall/status envelopes, error codes); `docs/OPERATOR.md` gains "Run the daemon as a service" with `loginctl enable-linger` guidance and updated updating semantics; `docs/RELEASE.md` documents the installer registration, formula `service` block/caveats, and postinst guidance; README install section covers script-installer registration, `LOCRON_NO_SERVICE=1`, `brew services start/restart`, and package guidance. `locron service status --json` executed on this machine matches the documented envelope; the service command family is covered by the 11 `tests/service.rs` contract tests; no new cross-links were introduced.
- [x] Run full workspace verification and record live brew evidence.
  **Verify:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` pass; the four-target CI matrix passes after push; at the next real tag, live evidence shows `brew services start locron` starts the daemon and `brew upgrade` leaves the old daemon running until `brew services restart`.
  **Evidence:** locally, `cargo fmt --all --check` (no diffs), `cargo clippy --workspace --all-targets -- -D warnings` (clean), and `cargo test --workspace` (exit 0; 20 test binaries all `ok`, 0 failed/panicked — including the 60 `service.rs` unit tests, 11 `tests/service.rs` contract tests, 4 `tests/install_sh.rs` fixture tests, 9 `tests/self_update.rs` contract tests, and the real-macOS `tests/service_backends.rs` leg) all pass. The four-target CI matrix and live brew evidence at the next real tag are recorded as deferred in the Verify clause.

## Linux service-backend compile backlog (2026-08-23)

The four-target CI matrix has been red on every Linux leg since the daemon service commit: the launchd-only items in `crates/locron-cli/src/service.rs` compile on Linux as dead code (clippy `-D warnings`), the systemd module carries an unused `Path` import and a `map().unwrap_or_else()` lint, and `tests/service_backends.rs` formats `PathBuf`s with `Display` in its Linux-only systemd flow.

- [ ] Gate the launchd-only items so Linux builds compile clean: `LABEL`, `LOG_DIR`, `LOG_FILE`, `escape_xml`, and `render_plist` under `#[cfg(any(target_os = "macos", test))]` (mirroring the existing `render_unit` pattern; the plist unit tests run on every platform), `PLIST_NAME` under `#[cfg(target_os = "macos")]`, the launchd-only `ServiceContext.uid` field under a documented targeted `allow(dead_code)`, the unused `Path` import and the `map().unwrap_or_else()` in the systemd module fixed, and the PathBuf `Display` uses in `tests/service_backends.rs` switched to `.display()`.
  **Evidence (first push, 2026-08-23):** commit 1977574 applied the `src/service.rs` half and the `Display` fix. The macOS legs of run 32623729732 (head 9c8170d) went green, but all four Linux legs still failed clippy on the test target: the macOS-only helpers `launchctl_ok`, `daemon_lock_pid`, and `wait_until` in `tests/service_backends.rs` are dead code on Linux (`default_daemon_lock_held` and `ServiceCleanup` stay shared — the Linux test uses both). The residual fix is a second commit gating those three helpers under `#[cfg(target_os = "macos")]`.
  **Evidence (second push, 2026-08-23):** commit 8e94ac4 gated the three helpers. Run 32642906172 (head d8c296c) still fails all four Linux legs, now on exactly one error per leg: `unused imports: Duration and Instant` in the same test target — the `wait_until` gate orphaned the `std::time` import (line 19). Every other workspace target compiles clean on Linux (clippy reports all errors per target, and no other target reports any), so this is the last residual: commit 8ebe173 gates that import. Convention learned and applied: every `#[cfg(target_os)]`-gated item must gate its own imports too.
  **Evidence (third push, 2026-08-23):** commit 8ebe173 gated the import. Run 32643088278 (head dc6f50b) confirms the compile backlog itself is resolved: clippy passed on all four Linux legs and the matrix failure moved to `cargo test --workspace --all-targets`, where two Linux-only `tests/self_update.rs` assertions failed (the post-replace service registration never ran — see the self-update registration backlog below). This step's verification completes when the full matrix is green on push.
  **Verify:** `cargo fmt --all --check`, `cargo clippy -p locron-cli --all-targets -- -D warnings`, and `cargo test -p locron-cli` pass on macOS; the four-target CI matrix passes on push (run ID recorded as evidence).

## Linux self-update registration backlog (2026-08-23)

On Linux, `locron self-update` silently skips the post-replace `service install`: after the atomic rename the running process's `/proc/self/exe` resolves to the deleted old inode (`path (deleted)`), so `register_service()`'s `fs::canonicalize(env::current_exe())` fails and returns no warnings. macOS masks the bug (`_NSGetExecutablePath` returns the exec-time path string). CI run 32643088278 (head dc6f50b) caught it: `self_update_installs_the_latest_release` and `atomic_replace_keeps_a_running_process_on_the_old_binary` failed on all four Linux legs with `left: []` on the service-log assertion.

- [ ] Capture the canonical executable path once in `update()` before the atomic replace and thread it through `replace_binary` and `register_service`, so no post-replace path resolution happens (`crates/locron-cli/src/self_update.rs`).
  **Verify:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` pass on macOS; the two self_update tests pass on the Linux CI legs (run ID recorded as evidence).

## Workspace lint alignment backlog (2026-08-23)

- [x] Bring `locron-core` and `locron-store` under `[lints] workspace = true` and clear every warning
  that enables: document all public items in both crates, fix the four `cast_sign_loss` sites in the
  store admission test with `try_from`, replace the infallible `try_into().expect` conversions in
  `FrameReader::next_frame` with documented `unwrap_or_default`, and add `#[must_use]` to
  `DaemonLock::file` and `StatePaths::new`.
  **Verify:** `cargo clippy --workspace --all-targets -- -D warnings` reports zero warnings (465 before
  the pass — 244 missing struct-field docs, 75 missing variant docs, 69 missing method docs, and the
  rest structs, functions, consts, and type aliases, plus 9 non-doc lints); `cargo fmt --all --check`
  and `cargo test --workspace --all-targets` pass (287 tests across 16 binaries); no `unsafe` was
  introduced; the only `#[allow]` added is a scoped `clippy::needless_pass_by_value` on
  `Store::materialize_with_summaries`, whose by-value cursor parameter is public API consumed by
  `locron-cli`.

## README demo screencast backlog (2026-08-23)

- [ ] Generate `assets/screencast.svg` from `assets/screencast.sh` and embed it at the top of the
  README under the badges. The script records `add → list → preview → run → history → why → doctor`
  against a throwaway state directory with its own daemon and is verified to run; rendering needs
  `svg-term` (`npm install -g svg-term-cli`), or a GIF via `vhs`, plus
  `<p align="center"><img src="assets/screencast.svg" alt="locron demo" width="800"></p>` in
  `README.md`.
  **Verify:** the rendered asset plays the full command sequence in a browser or on GitHub; the
  README image renders on the repository front page; the recording shows no local paths, machine
  names, or state from the recording host (the script's `--state-dir` isolation and jq filters
  already prevent this — confirm visually).

## README installation restructure (2026-08-23)

No product-behavior change, so the frozen `docs/SPEC.md` is not amended.

- [x] Move the per-channel installation, updating, and uninstalling content out of the README into
  a new `docs/INSTALL.md`; the README keeps the two most common one-liners (Homebrew, install
  script) and links the guide from its Installation section and Documentation list. The guide adds
  uninstall instructions per channel and a verify-the-installation section, fixes the tarball unpack
  pattern to the real archive layout, and links `docs/OPERATOR.md`/`docs/CLI.md` instead of
  duplicating service/update semantics. `docs/IMPLEMENTATION.md` notes where the per-channel update
  story moved.
  **Verify:** no repository link points at the removed README install anchors; no fact from the old
  README installation section is missing from `docs/INSTALL.md`; the README Documentation list
  includes the new guide; the new guide's anchors (`#updating`, `#uninstalling`, `#verify-the-installation`) render.
  **Evidence:** `rg "README.md#|#installation"` finds no stale anchors; a line-by-line diff review
  accounts for every sentence of the old section in either the slim README section or the guide; the
  Documentation list and guide anchors render on GitHub.

## Usage measurement backlog (2026-08-23)

Maintainer tooling only — no product-behavior change, so the frozen `docs/SPEC.md` is not amended. Planned in `docs/IMPLEMENTATION.md` "Usage and installation measurement"; evidence in `docs/FINDINGS.md` §13.

- [x] Amend planning documents before code: FINDINGS §13, IMPLEMENTATION section, and this checklist.
  **Verify:** `rg -n "13\. Usage and Installation Measurement|Usage and installation measurement" docs/FINDINGS.md docs/IMPLEMENTATION.md` returns both sections, and no planning document marks an unresolved decision in them.
  **Evidence:** both sections exist with no unresolved markers; implementation-day corrections (crates.io `/downloads` 90-day semantics, CI traffic-tolerance) were reconciled into both documents.
- [x] Add `scripts/usage.sh` per the accepted design (POSIX `sh`, `curl` + `grep`/`sed`/`awk` only, optional `gh`, quoted heredocs, per-section failure degradation, `--json`).
  **Verify:** `sh -n` passes; shellcheck reports no findings; a live run prints all sections — GitHub totals equal the independently computed sums, brew renders 0, crates.io renders `N/A (not published)`; `--json` parses with `jq` and matches the human numbers; with `gh` authenticated the traffic section prints, and the script still works without it.
  **Evidence:** `sh -n`/`dash -n` and shellcheck 0.11.0 clean; live run exit 0 with totals matching an independent `jq` computation taken at the same moment (total 32 — v0.3.0 0 / v0.2.0 21 / v0.1.1 8 / v0.1.0 3; stars 1; brew 0/0/0; crates.io `N/A (not published)`; traffic 77/5 views, 187/39 clones); `--json` parses and matches the human numbers; without `gh` on PATH the traffic section degrades to the one-line note and exit stays 0; fixture mocks prove rate-limit, pagination, page-cap, published-crate, and JSON-escaping paths.
- [x] Add the CI check step to the existing `installer` job in `.github/workflows/ci.yml` (shellcheck on `scripts/usage.sh` plus the live `--json` smoke).
  **Verify:** the workflow parses as valid YAML; the `installer` job passes on `main` after push (run ID recorded as evidence).
  **Evidence:** `yaml.safe_load` succeeds on the edited workflow; the `installer`-job step list includes "Shellcheck usage.sh" and "Usage snapshot smoke (live APIs)"; the smoke logic passes positive (traffic-only failure) and negative (rate-limited releases) local runs. The on-`main` run is deferred until push (run ID recorded here).
- [x] Document the channel in `docs/RELEASE.md` as section 7 "Usage and Installation Measurement": what each number means, its limits (cumulative downloads, opt-out brew analytics, unpublished crates.io, 14-day traffic window), and how to run it.
  **Verify:** the commands in the section execute as written on a maintainer machine; the section renders as §7 with a stable anchor.
  **Evidence:** `docs/RELEASE.md` §7 exists with the `---` separator; both documented commands (`sh scripts/usage.sh` and `sh scripts/usage.sh --json`) execute on this machine exactly as written.

Follow-ups (open, not implemented here):

- [ ] A scheduled snapshot job for measurement history becomes a drop-in via `--json`; adopt it only when the daily commit noise is worth the history.
- [ ] When locron is published to crates.io, the script's crates.io section switches automatically; record the first real numbers in this checklist.
- [ ] The product-level `locron stats` command (local durable-run aggregation) needs its own SPEC amendment; tracked on the deferred product roadmap.

## Ordered deferred product roadmap

Every phase below is post-milestone work and requires its own reviewed SPEC before implementation;
none changes the exclusions in the current `docs/SPEC.md`.

1. [ ] Define the local HTTP viewer and mutation API, including local-port binding, authentication,
   origin/CSRF protections, exposure diagnostics, and reuse of durable application commands.
2. [x] Define the MCP surface over the same application boundary, including capability scope,
   approval boundaries, redaction, and local transport/security behavior. **Evidence:** frozen in
   `docs/MCP_SPEC.md` and `docs/MCP_IMPLEMENTATION.md`, shipped as `locron mcp` in v0.1.1 with unit
   and integration test coverage.

### MCP (Model Context Protocol) Implementation Checklist

- [x] Add `locron mcp` subcommand and stdio JSON-RPC 2.0 loop with strict stderr logging isolation. **Verify:** unit tests verify JSON-RPC serialization, initialization handshake, ping, and stderr routing.
- [x] Implement all MCP Tools (`locron_list_jobs`, `locron_get_job`, `locron_add_job`, `locron_update_job`, `locron_enable_job`, `locron_disable_job`, `locron_remove_job`, `locron_run_job`, `locron_cancel_run`, `locron_get_logs`, `locron_why`, `locron_preview_schedule`, `locron_doctor`) with dry-run support. **Verify:** integration tests execute each tool call through piped stdin/stdout and verify state mutations and error responses.
- [x] Implement MCP Resources (`locron://jobs`, `locron://jobs/{id}`, `locron://history/{id}`, `locron://logs/{id}`, `locron://doctor`) and Prompts (`schedule_task`, `diagnose_failure`). **Verify:** integration tests verify resources/list, resources/read, prompts/list, and prompts/get.
- [x] Document MCP integration in README.md and Operator Guide with Claude Desktop and Cursor configuration snippets. **Verify:** documentation examples and configuration snippets pass review.
3. [ ] Define the desktop application as a client of the same scheduler/application contracts,
   without introducing another scheduling engine.
4. [ ] Define macOS App Store delivery after the desktop contract, including sandboxing,
   entitlements, background execution constraints, review requirements, update provenance, and the
   relationship to direct/package-manager installations.
5. [ ] Define the local usage-statistics command (`locron stats`) aggregating durable run history
   (requires its own reviewed SPEC).

**Verify:** each phase supplies its own completion criteria, threat/compatibility review, automated
tests, and delivery evidence in its future SPEC/TODO set before its checkbox can be completed.
