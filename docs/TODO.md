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

- [ ] Add stable identifiers, job/revision/schedule/target/policy/run/attempt/event types and application commands/results.
- [x] Add cross-field validation and normalization for exactly one schedule and target, all policy bounds, paths, environment, HTTP configuration, and reserved names.
- [x] Implement pure cron, interval, and one-time occurrence enumeration with injected clock/timezone inputs.
- [ ] Implement explicit legal run/attempt transitions and persistence, clock, and executor ports.
- [ ] Update the applicable planning document first if a discovered edge case requires behavior or structure not represented in the current plan.

**Verify:** core unit/property tests cover aliases, cron day OR behavior, interval anchors, schedule revisions, DST gaps/repetitions, clock movement, duration overflow, invalid combinations, and every state transition without SQLite or real-time sleeps.

## 4. Implement durable SQLite state

- [x] Add versioned migrations for the durable model and invariants in `docs/ARCHITECTURE.md` using the accepted choices from `docs/IMPLEMENTATION.md`.
- [ ] Implement atomic job revision/cursor mutation, manual run creation, reconciliation materialization, admission, attempt completion/retry, cancellation intent, recovery, and retention operations.
- [x] Prove queued/retry-wait cancellation terminalizes atomically while active cancellation remains
  durable intent, with stable not-found/conflict results.
- [x] Enforce scheduled occurrence uniqueness, ordered attempt uniqueness, foreign keys, soft deletion, and legal persisted states.
- [x] Add single-scheduler ownership and safe concurrent CLI access.
- [ ] Implement output metadata and the accepted output-storage consistency protocol.

**Verify:** integration tests against temporary real databases pass for clean and upgrade migrations, concurrent writers, rollback injection, duplicate occurrence insertion, cursor/run atomicity, stale lifetime recovery, soft-delete history, busy/disk failure handling, and interrupted output finalization.

## 5. Implement scheduler and admission semantics

Current correctness tranche (2026-08-21):

- [ ] Replace elapsed calendar scanning with bounded newest-window reconciliation and compact exact
  range summaries shared by cron, interval, and one-time schedules.
  **Verify:** pure tests cover long sparse ranges, deadline-before-policy ordering, limits 1 and
  1,000, newest selection/oldest execution, DST gap/fold, backward/forward wall moves, local-zone
  replacement, disable/re-enable, explicit recovery versus steady-state boundaries, exact cutoff and
  1-microsecond deadline edges, and duplicate reconciliation without sleeps.
- [ ] Centralize retry classification/backoff and prove durable `retry_wait` restart behavior and all
  forbidden retry classes.
  **Verify:** deterministic clock/store tests cover fixed and capped exponential delay, retry count
  exhaustion, timeout opt-in, restart eligibility, cancellation/configuration/replacement/unknown
  exclusion, and deadline interaction.
- [ ] Make replacement supersession and confirmation fully durable and exercise admission capacity
  interactions.
  **Verify:** store/engine tests cover newest-only `skipped_overlap` supersession, queued/retry-wait
  replacement without signal, active cancellation intent, confirmation failure, catch-up isolation,
  and `skip|replace|allow` across scheduled/manual/catch-up and changed capacity.
- [ ] Add deterministic lifecycle fault boundaries without acting on stale process identities.
  **Verify:** injected faults before spawn, after admission, while running, and after target exit show
  `interrupted_unknown`, no unknown retry, and no duplicate one-time occurrence.

- [ ] Implement the long-lived daemon runtime in `locron-engine`, including lifetime/lock ownership, loop coordination, signals, bounded maintenance, and graceful shutdown.
- [ ] Reconcile startup, wake, ticks, job revisions, disabled intervals, and wall-clock changes from durable cursors.
- [x] Prove due one-time resolution disables atomically, downtime catch-up remains unique, and manual
  submission neither consumes nor disables the schedule.
- [ ] Apply start deadline and `skip|latest|all`, including newest bounded selection, oldest-first catch-up execution, and bounded summary events.
- [ ] Apply `skip|replace|allow`, queued/retry-wait active accounting, global default 16/range 1..64, and per-job limits.
- [ ] Implement known-failure retry classification, fixed/capped-exponential delay, durable retry wait, and no retry for unknown outcomes.
- [ ] Implement cancellation/replace intents and startup classification as `interrupted_unknown` without stale PID action.
- [ ] Update `docs/IMPLEMENTATION.md` and `docs/TODO.md` before a milestone approach change; update `docs/ARCHITECTURE.md` first if the durable engine boundary or lifecycle invariant changes.

**Verify:** deterministic fake-clock/fake-executor suites pass for downtime, sleep, enable/disable, DST, backward/forward clock movement, bounded catch-up, every overlap/concurrency combination, capacity changes, retry/restart, cancellation, replacement confirmation failure, and duplicate-free one-time recovery.

## 6. Implement target runners and output capture

- [ ] Implement direct argv and explicit-shell execution with normalized CWD and effective PATH resolution.
- [ ] Build the minimal/layered environment and inject reserved `LOCRON_*` values last.
- [ ] Create and supervise process groups with timeout, TERM grace, KILL escalation, cancellation, replacement, and graceful daemon shutdown.
- [ ] Implement HTTP methods, absolute URLs, bodies/files, headers/env headers, success ranges, TLS, redirect policy, timeout, and retry classification.
- [x] Prove `301`/`302`/`303` redirect method rewriting and `307`/`308` method/body preservation while
  retaining cross-origin sensitive-header stripping.
- [ ] Stream, follow, truncate, finalize, and prune output under approved limits without blocking target completion.
- [x] Prove a post-admission runtime-file/configuration failure becomes a non-retryable terminal run
  with consistent finalized output rather than an orphaned running attempt.

**Verify:** real process-tree and local HTTP fixture tests pass on macOS and Linux for argv/env/CWD/PATH, grandchildren, TERM/KILL, disappearing runtime files, redirects across origins, TLS, response classes, timeout while streaming, output truncation, redaction boundaries, and target exit/result mapping.

## 7. Implement the thin CLI composition and commands

- [ ] Implement job CRUD, enable/disable/remove, schedule preview, history/show/logs, and cancellation.
- [x] Complete shared add/update normalization for metadata, schedules, every target/environment
  option, policy bounds, current global concurrency, cursor boundaries, dry-run diff, and no-op
  rejection.
- [x] Complete typed redacted/plaintext export and whole-document atomic import with acknowledgement,
  collision planning, rollback, and fresh-state round trip tests; keep history explicitly deferred.
- [ ] Implement durable offline manual enqueue, run ID output, wait/follow behavior, and outcome exit mapping.
- [ ] Implement import/export with explicit env-value acknowledgement, pruning, and diagnostics.
- [x] Expose `locron daemon run` as a thin composition entrypoint into the daemon runtime owned by `locron-engine`.
- [ ] Provide equivalent versioned machine-readable results without requiring prose parsing.
- [x] Implement non-mutating dry-run, durable-fact `why`, repeatable verbose context, and redacted debug tracing.
- [x] Ensure CLI handlers call shared application validation rather than duplicating policy.

**Verify:** CLI contract tests pass for human and machine modes, all command families, offline enqueue, disabled/manual/one-time behavior, client disconnect without cancellation, redaction, import/export round trip, soft removal, invalid/conflicting options, diagnostics, and stable error categories; workspace inspection finds only the `locron` binary and confirms `locron daemon run` delegates to `locron-engine`.

## 8. Prove crash, retention, and resource safety

- [ ] Inject daemon death before spawn, after spawn/before running commit, while running, and after target exit/before final commit.
- [ ] Exercise retention age/count/byte limits and interrupted pruning/finalization.
- [ ] Stress global concurrency 16 and maximum 64, large elapsed intervals, maximum catch-up 1,000, noisy output, and SQLite contention.
- [ ] Run process-group and service-lifetime checks on macOS 14+ and Linux kernel 5.14+/glibc 2.34+ for `aarch64` and `x86_64`.
- [ ] Treat Windows, 32-bit, and musl/Alpine results as deferred/informational rather than v1 release gates.
- [ ] Update the applicable `docs/ARCHITECTURE.md`, `docs/IMPLEMENTATION.md`, and `docs/TODO.md` content before applying any platform-driven design deviation.

**Verify:** fault/stress reports demonstrate durable occurrence identity, correct `interrupted_unknown` classification, no implicit retry or duplicate one-time run, bounded materialization/storage, correct eviction order, no active-run pruning, and documented cross-platform process cleanup guarantees.

## 9. Complete documentation and milestone acceptance

- [x] Document installation-from-source and operation of the program milestone without claiming package-manager support.
- [ ] Document schedule, overlap, missed-run, retry, concurrency, timeout/cancellation, crash, retention, plaintext secret boundary, and exactly-once limitations.
- [ ] Document diagnostics and recovery procedures plus human and machine CLI examples.
- [ ] Map every frozen completion criterion to automated or cross-platform verification evidence.
- [ ] Review that deferred viewer/API, MCP, desktop, package publication, and service installation work did not enter milestone 1.
- [ ] Keep durable future decisions under `docs/decisions/` only when a reviewed ADR is needed; split CLI/storage contracts only after review justifies dedicated documents.

**Verify:** a completion matrix links all 16 `docs/SPEC.md` criteria to passing evidence on the official macOS/Linux platform matrix; documentation examples execute successfully; architecture and implementation cross-links resolve; repository search and dependency/workspace inspection find no deferred surface implementation or premature empty ADR document.

## Post-milestone delivery backlog

These items begin only after every milestone-1 completion criterion above is satisfied. They do not
change the package-publication exclusion in `docs/SPEC.md`, and this milestone does not implement
their workflows or release infrastructure.

- [ ] Define CI/CD policy and version/release policy. **Verify:** choose concrete evidence when the
  follow-up delivery plan begins.
- [ ] Complete GitHub build/test CI for release operations and review the next supported major of
  `actions/checkout` to remove the current Node 20 deprecation annotation. **Verify:** choose the
  supported artifact, action-version, and runner matrix in the follow-up delivery plan.
- [ ] Publish the package through `whitekiwi/homebrew-tap`. **Verify:** choose install/upgrade/remove
  checks when packaging work enters scope.
- [ ] Automate Homebrew tap releases from approved version tags. **Verify:** choose provenance,
  checksum, rollback, and end-to-end release checks in the follow-up delivery plan.
- [ ] After milestone 1 and Homebrew delivery are complete, update this repository's README for
  install, upgrade, operation, and troubleshooting, and update `whitekiwi/homebrew-tap`'s README for
  tap/install, supported versions, and release provenance, with reciprocal links. **Verify:** choose
  documentation example and link checks in the follow-up delivery plan. Before writing, inspect
  `~/Downloads/locron.jpg` as a visual-asset candidate and research README patterns in established
  public CLI/scheduler repositories. Cover badges, quick start, install/upgrade, examples, support
  and release provenance, and documentation links; do not copy or modify the asset until that
  follow-up is explicitly in scope.
