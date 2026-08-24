# locron Milestone 1 TODO

This checklist tracks implementation of the frozen `docs/SPEC.md` within the durable structure in `docs/ARCHITECTURE.md` and the milestone approach in `docs/IMPLEMENTATION.md`. It does not include packaging, the HTTP management/viewer surface, MCP, or desktop work.

If a planned implementation decision changes, update and review `docs/IMPLEMENTATION.md` and this checklist before changing code. Update `docs/ARCHITECTURE.md` first for a durable structure/invariant change and `docs/SPEC.md` first for an observable behavior/scope change.
Completed historical sections live in `docs/TODO-archive.md` (moved 2026-08-24); this file keeps open work and recent backlogs.

## README Agent Skill link follow-up (2026-08-24)

Documentation link only: the frozen product behavior and accepted implementation are unchanged, so
`docs/SPEC.md` and `docs/IMPLEMENTATION.md` need no amendment.

- [x] Limit the change to one Agent-friendly README reference to `WhiteKiwi/skills`, without
  duplicating that repository's installation commands or platform guidance.
- [x] Add the link and verify Markdown and the destination.
  **Verify:** the README contains the requested link once, all relative links resolve, the external
  destination responds successfully, and `git diff --check` passes.
  **Evidence:** `rg` finds one README link; the local link/fragment check passes; the destination
  returns HTTP 200 after redirects; `git diff --check` is clean.

## Consolidated job explanation backlog (2026-08-24)

Authorized by the frozen 2026-08-24 `docs/SPEC.md` consolidated-explanation amendment, contracted
in `docs/CLI.md` “Why and diagnostics”, and planned in `docs/IMPLEMENTATION.md` “Consolidated job
explanation implementation”.

- [x] Complete and review the command contract before implementation: live-job resolution,
  schedule/current-status facts, retained-history ordering, anomalous terminal vocabulary,
  explicit absence states, redaction, and human/JSON parity.
  **Verify:** `rg -n "explain NAME_OR_ID|Consolidated job explanation" docs/CLI.md
  docs/IMPLEMENTATION.md docs/TODO.md` finds all three planning layers; the new sections contain no
  unresolved decision or unrecorded behavior deviation.
  **Evidence:** the search returns the command surface and exact human/machine contract in
  `docs/CLI.md`, the shared-fact/history-selection design in `docs/IMPLEMENTATION.md`, and this
  phased checklist. A second review found no open question or dependency/schema change.
- [x] Add the thin `explain` command and shared current-job explanation path, including the bounded
  store read, redacted latest-run/latest-anomaly summary projection, and labeled human report.
  **Verify:** `cargo fmt --all --check`, `cargo clippy -p locron-cli --all-targets -- -D warnings`,
  and the new focused unit/store tests pass; a scratch-state invocation shows full canonical run
  IDs, durable reasons, explicit `none` sections, and `unknown` pending facts without target
  configuration leakage; a disabled-job regression assertion proves `why NAME` is unchanged.
  **Evidence:** `cargo fmt --all --check` and `cargo clippy -p locron-cli --all-targets -- -D
  warnings` are clean. The focused CLI unit test and focused store test pass; the store test proves
  the anomaly query reaches behind the 1,000-row history presentation cap and orders equal request
  times by canonical ID. An isolated scratch-state report showed the full job ID, explicit
  `LATEST RUN`/`LATEST ANOMALY` `none` sections, the separate eligibility/overlap/capacity-limit
  facts, and no configured environment value. The disabled-job contract assertion confirms
  `why NAME` still calculates a next occurrence and prints its prior `eligible` decision.
- [x] Add CLI contract/integration coverage for no history, success-only history, the same latest
  run/anomaly, an older anomaly after a newer success, an active latest run, removed/history and
  reused-name behavior, human/JSON parity, redaction, and the complete help surface.
  **Verify:** every named scenario has an assertion in `crates/locron-cli/tests/cli.rs` (or a
  focused integration suite), all new tests pass, and
  `complete_command_tree_has_consistent_help_surface` passes with `explain` discovered.
  **Evidence:** four new CLI contract tests cover every named state/history/removal/reuse scenario,
  human/JSON fact parity, canonical IDs, explicit absence/unknown wording, and secret redaction.
  `cargo test -p locron-cli` passes 73 CLI unit tests, 72 command-contract tests, and all 13 other
  CLI suites when `LOCRON_STATE_DIR` points at an isolated path; the generic help walk discovers
  `explain`, and the explicit help assertion verifies both name and UUID examples. The isolated
  state is necessary on this workstation because a real per-user daemon owns the default lock; an
  unisolated run's existing service test correctly observed that external daemon and deferred.
- [x] Update README and help examples to present `explain` as the consolidated entry point while
  retaining `why --run` for the detailed attempt/event trace and avoiding sleep inference.
  **Verify:** every documented command matches `locron <command> --help`; an isolated-state smoke
  run matches the shown human output; `rg -n "machine (sleep|suspended)|detected sleep" README.md`
  finds no unsupported telemetry claim.
  **Evidence:** the README tour now uses the actual shipped human `CURRENT STATUS`, `LATEST RUN`,
  and `LATEST ANOMALY` forms captured from an isolated state; the explainability and JSON examples
  include `explain`, while `why --run` remains the full attempt/event handoff. `locron explain
  --help` contains both documented examples, the unsupported-telemetry search returns no match,
  and `git diff --check` is clean.
- [x] Run targeted and full workspace verification and record evidence before handoff.
  **Verify:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  and `cargo test --workspace` pass; `git diff --check` is clean and only explain-related files are
  changed in this worktree.
  **Evidence:** `cargo fmt --all --check` is clean; `cargo clippy --workspace --all-targets -- -D
  warnings` is clean; `cargo test --workspace` passes 344 tests across the four crates and all
  integration suites (73 CLI unit, 72 CLI command-contract, 66 other CLI-suite, 36 core, 45 engine,
  52 store), with 0 failures. The test command used an isolated `LOCRON_STATE_DIR` so the workstation's
  real per-user daemon could not affect service fake expectations. `git diff --check` is clean, and
  status contains only the parent SPEC amendment plus explain code, tests, CLI/implementation/TODO,
  and README updates.
- [x] Parent session: publish v0.6.0 after PRs #5 and #6 merge — curate the changelog, bump the
  lockstep workspace version, commit, push `main`, create and push the annotated tag, then monitor
  the release and Homebrew tap workflows to completion.
  **Verify:** `Cargo.toml`, every workspace package in `Cargo.lock`, the changelog heading, binary
  `--version`, and annotated tag all report 0.6.0; the release workflow is green; the GitHub Release
  publishes all required assets with curated notes; the tap formula points at v0.6.0 and its
  test-bot run is green.
  **Evidence:** release commit `6103089` sets the lockstep workspace and lockfile packages to 0.6.0,
  adds the curated changelog entry, and reports `locron 0.6.0` in both human and JSON version output;
  signed annotated tag `v0.6.0` points to that commit. Release workflow
  [32717655943](https://github.com/WhiteKiwi/locron/actions/runs/32717655943) succeeded and published
  the non-draft [locron v0.6.0 release](https://github.com/WhiteKiwi/locron/releases/tag/v0.6.0)
  with the curated notes and all 10 required assets: four tarballs, two debs, two rpms, `install.sh`,
  and `SHA256SUMS.txt`. The repaired tap formula retains the four v0.6.0 URLs and matching checksums;
  tap commit `06ae05f` passed macOS and Linux test-bot run
  [32719051536](https://github.com/WhiteKiwi/homebrew-tap/actions/runs/32719051536).

## README product positioning refresh (2026-08-24)

Authorized by the 2026-08-24 `docs/SPEC.md` Product Positioning amendment and planned in
`docs/IMPLEMENTATION.md` “README product narrative”. This is documentation-only work; it does not
add planned diagnostics or scheduler telemetry.

- [x] Review and accept the README narrative before editing the README: lead with explainability,
  then reliability, then agent integration; preserve installation and service-operation facts.
  **Verify:** `rg -n "README product narrative|Cron that explains itself" docs/IMPLEMENTATION.md`
  finds the accepted section, including the then-current exclusions for `locron explain`, richer
  decision traces, and direct machine sleep telemetry.
  **Evidence:** the accepted section records the narrative order, the actual log lookup contract,
  the unchanged installation/service substance, and all three exclusions. The `locron explain`
  exclusion is superseded by the consolidated-explanation amendment and backlog above; the other
  two exclusions remain.
- [x] Rewrite the README opening and demonstration using only shipped CLI syntax and actual human
  output; keep captured logs tied to a canonical run ID rather than a job name.
  **Verify:** run the documented commands against an isolated state directory and compare the
  displayed output with the README; compare every command invocation with `locron <command>
  --help`; `rg -n "locron logs [A-Za-z]" README.md` finds no job-name logs example.
  **Evidence:** an isolated daemon/state run reproduced the documented `add`, two-occurrence
  `preview`, and the clearly labeled selected fields from `why backup` exactly (only IDs/timestamps
  vary); the same run verified `run --wait`, `history`, `history --format json`, `logs RUN_ID`,
  `why --run RUN_ID`, and `doctor`. Help review confirms each invocation; the job-name logs search
  returns no match.
- [x] Reorder the remaining capability sections as explainability, real-world reliability, and
  agent integration, without losing accurate installation, service, documentation, contributing,
  security, or license guidance.
  **Verify:** a heading scan shows that order; focused diff review confirms the retained guidance;
  searches find no unsupported claim for machine sleep state or detailed decision traces; the
  later-shipped `locron explain` surface is tracked in the consolidated-explanation backlog above.
  **Evidence:** the heading scan is `A 10-second tour` → `Explainability first` → `Reliability for
  machines that stop and restart` → `Agent-friendly by design`; installation and daemon startup,
  all seven documentation links, contributing/security, and dual-license guidance remain. The
  README states that downtime explanations use durable schedule cursors and reconciliation facts,
  rather than inferred sleep telemetry, and makes no richer decision-trace claim.
- [x] Run documentation checks and record the evidence.
  **Verify:** all relative Markdown links in `README.md` resolve; fenced command/config snippets
  parse where a local parser is available; `git diff --check` passes; a final rendered-text review
  finds no malformed Markdown.
  **Evidence:** a local UTF-8-aware link/fragment check reports every relative README link
  resolved; all eight `sh` fences pass `sh -n`; the MCP JSON fence parses with Ruby JSON; the banner
  asset exists; `git diff --check` is clean. No Markdown linter is installed, so headings, balanced
  fences, lists, and inline HTML were reviewed directly.
## One-time automatic removal (2026-08-24)

- [x] Add `completion_action` to normalized job definitions with a backward-compatible `retain` default; parse `--delete-after-run` and reject it unless the effective schedule is `--at`.
  **Verify:** CLI tests prove `--at --delete-after-run` normalizes to `delete`, while interval use fails validation.
  **Evidence:** `tests::delete_after_run_requires_one_time_schedule_and_is_snapshotted` passes.
- [x] Atomically soft-remove an eligible job when its scheduled one-time run becomes terminal, without changing run, attempt, event, or output retention.
  **Verify:** store tests cover successful terminal completion and manual-run exclusion; existing completion and failure-path tests exercise the same transaction helper.
  **Evidence:** `store::tests::scheduled_one_time_delete_action_soft_removes_but_keeps_history` and `store::tests::manual_run_does_not_consume_one_time_delete_action` pass; existing completion and failure-path tests remain green.
- [x] Preserve removed job names in history rendering and allow a non-reused removed name to filter history; retain UUID lookup after a name is reused.
  **Verify:** CLI contract tests cover all-history labels, removed-name filtering, and name-reuse UUID filtering.
  **Evidence:** `human_history_prints_the_aligned_table_with_header_always` now verifies the `NAME (removed)` label; the store auto-removal test verifies removed-name history lookup.
- [x] Run formatting, Clippy, and the workspace test suite, then record the evidence here.
  **Verify:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace --all-targets` exit zero.
  **Evidence:** all three commands passed on 2026-08-24; workspace tests include 73 CLI unit tests, 6 acceptance tests, and all store/core/engine tests.

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
  **Evidence:** `yaml.safe_load` succeeds on the edited workflow; the `installer`-job step list includes "Shellcheck usage.sh" and "Usage snapshot smoke (live APIs)"; the smoke logic passes positive (traffic-only failure) and negative (rate-limited releases) local runs. The on-`main` run is green: run 32644269125 (head 7edc89d), installer job succeeded.
- [x] Document the channel in `docs/RELEASE.md` as section 7 "Usage and Installation Measurement": what each number means, its limits (cumulative downloads, opt-out brew analytics, unpublished crates.io, 14-day traffic window), and how to run it.
  **Verify:** the commands in the section execute as written on a maintainer machine; the section renders as §7 with a stable anchor.
  **Evidence:** `docs/RELEASE.md` §7 exists with the `---` separator; both documented commands (`sh scripts/usage.sh` and `sh scripts/usage.sh --json`) execute on this machine exactly as written.

Follow-ups (open, not implemented here):

- [ ] A scheduled snapshot job for measurement history becomes a drop-in via `--json`; adopt it only when the daily commit noise is worth the history.
- [ ] When locron is published to crates.io, the script's crates.io section switches automatically; record the first real numbers in this checklist.
- [ ] The product-level `locron stats` command (local durable-run aggregation) needs its own SPEC amendment; tracked on the deferred product roadmap.

## Web administration backlog (2026-08-24)

Phase 1 of the ordered deferred product roadmap below. Authorized by the frozen 2026-08-24
`docs/dashboard/SPEC.md` (interactive product review); planned in `docs/dashboard/IMPLEMENTATION.md`; evidence
in `docs/FINDINGS.md` §14 (including the default-port 10824 verification). The frozen
`docs/SPEC.md` is not amended — the roadmap phases do not change its exclusions.

- [ ] Amend planning documents before code: `docs/ARCHITECTURE.md` first (fifth workspace member
  `locron-server` with its dependency row and arrows, the shared redaction boundary moving to
  `locron-core`, and the server-never-owns-daemon boundary note), then add the `locron dashboard`
  contract plus the API error-status mapping to `docs/CLI.md`.
  **Verify:** `rg -n "locron-server|redaction" docs/ARCHITECTURE.md` shows the new member,
  dependency direction, and core redaction responsibility; `docs/CLI.md` documents the
  `locron dashboard` family and the `locron.api/v1` envelope; no planning document marks an
  unresolved decision.
- [ ] Add the `locron-server` member to the workspace with axum 0.8.9, tokio-stream 0.1.19,
  rust-embed 8.12 (mime-guess only), axum-extra 0.12 (cookie), and getrandom 0.4 (all below MSRV
  1.94 per `docs/FINDINGS.md` §17); update the dependency-direction enforcement check.
  **Verify:** `cargo build`/`fmt --check`/`clippy -p locron-server` pass on Rust 1.94 and latest
  stable; dependency inspection shows `locron-server` depends only on `locron-core` and
  `locron-store` and nothing depends on it but `locron-cli`; the workspace still produces exactly
  one `locron` binary.
- [ ] Move the redaction boundary (`redacted_job`, `redact_definition`,
  `redacted_observable_run`, `redacted_run`, `redacted_settings_value`) from
  `crates/locron-cli/src/main.rs` to `locron-core` with no output change.
  **Verify:** the existing CLI contract and redaction tests pass unchanged;
  `cargo clippy --workspace --all-targets -- -D warnings` stays clean.
- [ ] Implement the middleware stack and token file: loopback-only bind with non-loopback refusal,
  Host allowlist (`localhost`/`127.0.0.1`/`[::1]`, port ignored), Origin check on unsafe methods,
  token acceptance (`Authorization: token` header plus entry-page paste, never a URL), session and
  `csrf_token` cookies (`SameSite=Lax`), double-submit CSRF with bearer exemption,
  `Referrer-Policy: no-referrer`, owner-only token file, and the unauthenticated entry page.
  **Verify:** middleware unit tests cover the Host allowlist variants, Origin
  present/mismatch/absent, CSRF match/mismatch and bearer exemption, token accept/reject, the
  entry-page paste flow, the Referrer-Policy header, that only the entry page is served without a
  token, and that no token ever appears in a served URL.
- [ ] Implement the `/api/v1` route families over the durable application commands with the
  `locron.api/v1` envelope, the CLI-category-to-HTTP-status mapping, dry-run parity, export
  download/import upload with acknowledgement rules, and blocking-pool store access.
  **Verify:** contract tests against a real server on an ephemeral loopback port and a temporary
  state directory cover token refusal, job CRUD mutating real SQLite, offline manual enqueue,
  export/import round trip, dry-run non-mutation, and the error mapping; redaction parity tests
  compare API payloads with CLI JSON output for the same fixtures.
- [ ] Implement the SSE run stream (`GET /api/v1/runs/{id}/stream`) over the existing framed-output
  reader with `frame`/`state`/`end` JSON events, session-cookie-only authentication, and keepalive.
  **Verify:** SSE tests receive ordered `frame`/`state` events for a live fixture run and exactly
  one terminal `end` event at finalization; disconnecting the stream never cancels the run.
- [ ] Implement and embed the hand-written viewer SPA (status chips, run timeline, monospace
  follow console log, CSRF-aware mutations) via rust-embed, with no CDN, no external assets, and no
  Node build step.
  **Verify:** an asset test serves every referenced asset from the binary with correct content
  types; a recorded manual browser checklist opens the access URL, walks the token paste and
  cookie handoff, list/detail/history, job creation with dry-run preview, a live follow, and a
  cancellation, and confirms no redacted value appears in DOM or JSON.
- [ ] Implement the dashboard service registration on the existing service-manager port: second
  registration target (`dev.locron.dashboard` / `locron-dashboard.service`, dashboard log paths,
  `dashboard serve` execution), `enable`/`disable`/`status`/`enable --reset` flows with the daemon
  registration's verified ordering, brew-marker refusal for registration, and the self-update
  post-replace refresh using the pre-replace canonical-path capture.
  **Verify:** fake-port tests cover the dashboard templates, enable idempotency and
  refresh-and-restart, `--reset` ordering, disable ordering, brew-marker refusal, and status
  fields; real-backend tests on the macOS leg register, restart, and unregister the dashboard
  LaunchAgent, and the Linux leg drives the dashboard unit under `dbus-run-session`; a self-update
  contract test proves the post-replace dashboard refresh happens exactly once and only after a
  successful replace.
- [ ] Wire the `locron dashboard [--port N] [--bind ADDR]` family (`serve` alias, `enable`,
  `disable`, `status`, `token`) in `locron-cli`, add the doctor exposure facts, and extend the
  help-surface acceptance walk to the new command.
  **Verify:** CLI tests cover the startup URL and token output, non-loopback `--bind` refusal,
  explicit `--port` strictness, foreground fallback, service-mode fixed port reporting,
  `enable --reset` regeneration, doctor facts in human and JSON output, and the help walk covers
  every new argument.
- [ ] Documentation final pass: `docs/CLI.md` (verified above), `docs/OPERATOR.md` (viewer
  operation, token lifecycle, the shared `loginctl enable-linger` note, what loopback does and
  does not protect), and the README documentation list entry for `docs/dashboard/SPEC.md`.
  **Verify:** documented commands execute as written; all new cross-links resolve.
- [ ] Run full workspace verification and record evidence, then mark roadmap phase 1 complete.
  **Verify:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  and `cargo test --workspace` pass on Rust 1.94 and latest stable; the four-target CI matrix is
  green; the browser-checklist and real-backend evidence are recorded in this section; roadmap
  phase 1 is checked with evidence pointing at `docs/dashboard/SPEC.md`, `docs/dashboard/IMPLEMENTATION.md`,
  and this section.

## Shutdown-drain test determinism and CI lint consolidation backlog (2026-08-24)

No product-behavior change (a test-harness script and the CI workflow only), so the frozen
`docs/SPEC.md` is not amended. Planned in `docs/IMPLEMENTATION.md` "Shutdown-drain test
determinism and CI lint consolidation".

Background: `daemon::tests::elapsed_shutdown_drain_cancels_runner_before_lifetime_end` failed on two
macOS CI legs — run
[32644652482](https://github.com/WhiteKiwi/locron/actions/runs/32644652482) (`macos-aarch64` /
Rust 1.94.0) and run
[32644735243](https://github.com/WhiteKiwi/locron/actions/runs/32644735243) (`macos-x86_64` /
Rust stable) — with the outcome assertion receiving `Some(TerminationUnconfirmed)` instead of
`Some(Cancelled)`. The test script's `sleep 1` adds a second process to the run's process group;
after the trapped `sh` exits, the orphaned sibling stays a zombie until launchd reaps it, and the
group-liveness probe (`kill(-pgid, 0)`, `runner.rs:767`) counts zombies as alive, so the test's
two 20 ms grace deadlines elapse first on a loaded macOS runner. 40 consecutive local runs pass
(the failure is CI-load-dependent). The fix makes the group single-member so leader reap equals
group absence and confirmation becomes event-driven; the CI part consolidates redundant lint
steps (clippy compile work halved, no arch-gated code exists to justify the arch duplication).

- [x] Amend planning documents before code: this checklist and the `docs/IMPLEMENTATION.md`
  section. The frozen `docs/SPEC.md` needs no amendment because neither change touches product
  behavior.
  **Verify:** `rg -n "Shutdown-drain test determinism" docs/IMPLEMENTATION.md docs/TODO.md`
  returns both sections, and no planning document marks an unresolved decision in them.
  **Evidence:** `rg` returns `docs/IMPLEMENTATION.md` (EOF section) and this checklist section;
  no unresolved decision marker in either; the SPEC is not amended (no product-behavior change).
- [x] Replace the test script's `sleep 1` loop with a pure-builtin loop
  (`while :; do :; done`) and raise its `termination_grace` from 20 ms to 1 s in
  `crates/locron-engine/src/daemon.rs`
  (`elapsed_shutdown_drain_cancels_runner_before_lifetime_end`); `shutdown_drain` stays at 10 ms.
  **Verify:** `for i in $(seq 1 100); do cargo test -p locron-engine --quiet
  elapsed_shutdown_drain_cancels_runner_before_lifetime_end || break; done` completes all 100
  iterations; `cargo test -p locron-engine` passes; the sibling
  `shutdown_drain_allows_natural_completion_before_lifetime_end` test passes unchanged.
  **Evidence:** the dev sub-session applied both changes (daemon.rs lines 1194/1200); the
  100-iteration loop completes all 100 iterations (each run 0.03–0.08 s); `cargo test -p
  locron-engine` passes 45 tests, 0 failed; the sibling test passes unchanged.
- [x] Add a dedicated `lint` job to `.github/workflows/ci.yml` (matrix `linux-x86_64` and
  `macos-aarch64` × Rust 1.94.0 and stable, `fail-fast: false`, rust-cache, `cargo fmt --all
  --check`, `cargo clippy --workspace --all-targets -- -D warnings`) and remove those two steps
  from the eight-leg `test` job so its legs run `cargo test --workspace --all-targets` only.
  **Verify:** the workflow parses as valid YAML; `rg -n "fmt --all|cargo clippy"
  .github/workflows/ci.yml` shows both commands only inside the `lint` job; a push run records all
  four lint legs and all eight test legs green (run ID recorded as evidence).
  **Evidence:** the workflow parses as valid YAML (ruby/psych — the plan's PyYAML example was
  unavailable; jobs are `test`, `lint`, `installer` with the lint matrix/timeout/fail-fast
  programmatically verified); `rg` finds fmt/clippy only at `ci.yml` lines 63–64 inside `lint`.
  Push run [32654818613](https://github.com/WhiteKiwi/locron/actions/runs/32654818613) (head
  `e9f693c`) concluded `success` — the first run of the new `lint` job (4 legs) alongside the
  eight-leg test matrix and the installer job.
- [x] Run full workspace verification. **Verify:** `cargo fmt --all --check`, `cargo clippy
  --workspace --all-targets -- -D warnings`, and `cargo test --workspace` pass locally on the
  installed toolchain.
  **Evidence:** `cargo fmt --all --check` clean; `cargo clippy --workspace --all-targets --
  -D warnings` clean; `cargo test --workspace` green across all binaries, 0 failed/panicked.

## Human output backlog (2026-08-24)

Authorized by the frozen 2026-08-24 `docs/SPEC.md` amendment (Human Output Contract, issue #4).
Planned in `docs/IMPLEMENTATION.md` "Human rendering implementation"; the rendering contract is in
`docs/CLI.md` "Human rendering".

- [x] Amend planning documents before code: SPEC amendment, CLI.md contract, IMPLEMENTATION
  section, this checklist.
  **Verify:** `rg -n "Human Output Contract|Human rendering|Human rendering implementation"
  docs/SPEC.md docs/CLI.md docs/IMPLEMENTATION.md` returns all three, and no planning document
  marks an unresolved decision in the new content.
  **Evidence:** `rg` returns `docs/SPEC.md` line 280 (amendment section + status note),
  `docs/CLI.md` line 277 (per-command contract), and `docs/IMPLEMENTATION.md` line 344
  (implementation approach); no unresolved marker in the new content.
- [x] Implement the table and confirmation-line renderers: `history` table, `add`/`update`,
  `enable`/`disable`, `remove`, `run` (queued/wait/dry-run), `cancel`, `config get|set|unset`,
  `import`, `prune` — each human branch replaced by its documented form; the JSON path stays
  byte-identical.
  **Verify:** `cargo fmt --all --check`, `cargo clippy -p locron-cli --all-targets -- -D
  warnings`, and `cargo test -p locron-cli` pass; manual scratch-state runs show the documented
  forms for each command, and each `--json` output is unchanged.
  **Evidence:** `cargo fmt --all --check` is clean; `cargo clippy -p locron-cli --all-targets --
  -D warnings` is clean; `cargo test -p locron-cli` passes 198 tests (67 unit in main.rs, 65
  contract in cli.rs). Manual scratch-state runs (`target/debug/locron --state-dir /tmp/...`)
  show the documented forms: `job added: NAME (UUID)` plus `schedule:`/`target:` lines;
  `job updated: NAME (UUID, revision 2)` plus summary lines; `job enabled:`/`job disabled:`;
  `job removed:`; `run queued: UUID (job NAME)` (with `warning: daemon is not running; run
  remains durably queued` on stderr when no daemon) then `run finished: UUID (STATE)` on
  `--wait`; dry-run decisions (`run would skip (overlap policy): NAME` etc.) followed by
  `dry run: no run created`; `cancellation requested: UUID (cancelled before execution)` and a
  second cancel returning exit 3 `durable conflict`; `global_concurrency=16` for `config get`;
  `KEY: configured` / `KEY: would be configured (dry run; no changes made)` / `KEY: unset` for
  set/unset; `created N, updated N, unchanged N` plus per-job action lines for import; `pruned:
  N runs, N outputs (N bytes)` (and `dry run: would prune ...`). Every command re-run with
  `--format json` produced byte-identical machine output.
- [x] Implement the report and value-list renderers: `show` sections, `preview` occurrence list,
  `why` job and run modes, `doctor` ok/warn/fail lines.
  **Verify:** the same battery plus manual checks of `why --run` on a terminal run and a
  `fail`-marked doctor check.
  **Evidence:** same clean battery as the previous step. Manual `why --run` against a terminal
  run made through a real daemon prints RUN (`run id`, `trigger: manual`, `nominal time: none`,
  `requested`, `state: succeeded`, `started`, `finished`, `duration`, `outcome`), ATTEMPTS
  (`attempt 1: succeeded (Nus)`), EVENTS (`{RFC3339} manual_enqueued`), and TERMINAL REASON
  (`reason: process exited successfully`). A `fail`-marked doctor check was produced manually by
  registering a job with a nonexistent executable: `fail process resolution: broken (executable
  not found: /nonexistent/bin/nope)`; the same scratch run also showed `warn daemon: not
  running` and `warn wake socket: missing` variants. `show` renders JOB/SCHEDULE/TARGET/
  POLICIES sections, `preview` prints `schedule: ...` then one RFC 3339 occurrence per line,
  and `doctor` renders every check with an `ok   `/`warn `/`fail ` prefix.
- [x] Add contract tests in `crates/locron-cli/tests/cli.rs` for every command's human form:
  empty and populated states, dry-run wording, redaction (no configured value in any human
  output), table-only ID abbreviation, and unchanged JSON assertions.
  **Verify:** the new tests pass, and the existing help-surface walk
  (`complete_command_tree_has_consistent_help_surface`) passes unchanged.
  **Evidence:** 15 new contract tests pass (cli.rs now 65, up from 50):
  `human_add_update_enable_disable_remove_print_outcome_lines`,
  `human_history_prints_the_aligned_table_with_header_always`,
  `human_run_prints_the_dry_run_decision_and_queued_lines`,
  `human_cancel_prints_the_resolution_line`, `human_show_prints_labeled_sections`,
  `human_show_why_and_list_never_leak_configured_values`,
  `human_preview_prints_the_schedule_line_then_occurrences`,
  `human_why_job_prints_labeled_sections`, `human_run_wait_streams_and_prints_the_terminal_outcome_line`,
  `human_why_run_prints_immutable_run_facts`, `human_doctor_prints_one_level_line_per_check`,
  `human_config_forms_print_key_value_and_action_lines`,
  `human_import_prints_counts_then_action_lines`, `human_prune_prints_the_pruned_counts`, and
  `human_forms_leave_the_json_envelope_untouched`. The help-surface walk
  (`complete_command_tree_has_consistent_help_surface`) passes unchanged. Redaction: the
  never-leak contract tests assert no configured value in any human output (add/show/why/list/
  preview/doctor), the JSON-envelope test asserts schema/command/data stability, and manual
  re-checks of `show`, `why`, `list`, and `--json show` on a state with configured environment
  values reported 0 leaks.
- [x] Check `README.md`, `docs/OPERATOR.md`, and `assets/screencast.sh` for JSON-wall sample
  output or `jq` pipes the human forms make obsolete; update the docs and note the screencast
  regeneration.
  **Verify:** `rg` finds no stale sample, and the screencast note is recorded.
  **Evidence:** `rg -n "locron (add|list|preview|why|doctor|history|show)" README.md
  docs/OPERATOR.md` returns only command listings with inline comments — no JSON-wall output
  samples; `assets/screencast.sh` no longer contains `jq` (all ten demo commands now run in
  plain human mode; the now-obsolete `--format json ... | jq` pipelines and the JQ_FILTER
  fallback machinery were removed, and `bash -n` passes). The screencast recording
  (`assets/screencast.svg`) is not regenerated in this session — regeneration requires
  svg-term-cli/Node and belongs with the release publication.
- [x] Run workspace verification, then close issue #4 with the fixing commit reference.
  **Verify:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  and `cargo test --workspace` pass; `gh issue view 4` shows CLOSED with a reference to the
  fixing commit.
  **Evidence:** `cargo fmt --all --check` is clean; `cargo clippy --workspace --all-targets --
  -D warnings` is clean; `cargo test --workspace` passes 330 tests across 20 suites with 0
  failures (locron-cli: 67 unit + 65 contract + 66 across its other 12 test suites; the +15
  contract tests over the export backlog's 315 total match this work). Closing issue #4 with
  the fixing commit reference is the parent session's publication step (this development
  sub-session does not run git/gh commands); the closing commit is the one that carries these
  changes.

## Usage snapshot smoke relocation backlog (2026-08-24)

No product-behavior change (CI placement only), so the frozen `docs/SPEC.md` is not amended.
Planned in `docs/IMPLEMENTATION.md` "Usage snapshot smoke relocation"; evidence in
`docs/FINDINGS.md` §18.

Background: CI run
[32654895285](https://github.com/WhiteKiwi/locron/actions/runs/32654895285) failed in the
`Installer / ubuntu-latest` job's "Usage snapshot smoke (live APIs)" step: the step never passed
`GITHUB_TOKEN` to the script, so the GitHub REST calls ran unauthenticated against the shared-IP
60/hour quota and the traffic-only tolerance predicate rejected the failure. The convention
(FINDINGS §18: Google SWE book, Fowler, freenet-core's identical failure) is that blocking push
CI must be hermetic and live third-party checks belong on a schedule — this backlog applies it.

- [x] Amend planning documents before code: FINDINGS §18, the IMPLEMENTATION section, and this
  checklist.
  **Verify:** `rg -n "18\. Live External API|Usage snapshot smoke relocation" docs/FINDINGS.md
  docs/IMPLEMENTATION.md docs/TODO.md` returns all three sections, and no planning document marks
  an unresolved decision in them.
  **Evidence:** `rg` returns `docs/FINDINGS.md` §18, the `docs/IMPLEMENTATION.md` EOF section,
  and this checklist section; no unresolved decision marker in them; the SPEC is not amended (no
  product-behavior change).
- [x] Add `.github/workflows/usage.yml` (weekly `schedule:` cron plus `workflow_dispatch`, one
  ubuntu-latest job, `env: GITHUB_TOKEN` with `permissions: contents: read`, JSON-shape assertion
  without the traffic-only tolerance) and remove the live-smoke step from the `ci.yml` installer
  job, keeping the hermetic shellcheck steps, the fake-`uname`/`ldd` refusals, and the pinned
  `v0.2.0` install smoke.
  **Verify:** both workflows parse as valid YAML; `rg -n "usage.sh --json" .github/workflows`
  shows the smoke only inside `usage.yml`; the next push CI run is green (run ID recorded as
  evidence); a manual `workflow_dispatch` run of the new workflow completes and records its
  snapshot in the run log.
  **Evidence:** the dev sub-session added `usage.yml` (39 lines; `on:` schedule cron `0 3 * * 1`
  + `workflow_dispatch`, `permissions: contents: read`, one `usage` job with the `GITHUB_TOKEN`
  env and the JSON-shape assertion) and removed only the 11-line live-smoke step from `ci.yml`
  (installer job now shellcheck → fake-uname refusals → pinned install). Both files parse with
  ruby/psych (`OK`, `OK`); `rg` finds the smoke only at `usage.yml:36` and no "Usage snapshot
  smoke" in `ci.yml`. Push run
  [32655930565](https://github.com/WhiteKiwi/locron/actions/runs/32655930565) (head `f03d631`)
  concluded `success` — the first push run without the live smoke. The first
  `workflow_dispatch` run (32655962445) failed on the tolerance interaction recorded in the
  follow-up step below.
- [x] Run workspace verification. **Verify:** `cargo fmt --all --check`, `cargo clippy
  --workspace --all-targets -- -D warnings`, and `cargo test --workspace` pass (the change
  touches only workflows, so this guards against accidental drift).
  **Evidence:** `cargo fmt --all --check` clean; `cargo clippy --workspace --all-targets --
  -D warnings` clean; `cargo test --workspace` green (51 engine unit tests + all doc-tests, 0
  failed); `sh -n scripts/usage.sh` passes.

### Follow-up: restore the traffic-only tolerance in the scheduled smoke

The first `workflow_dispatch` run (32655962445) failed: passing `GITHUB_TOKEN` also authenticates
the preinstalled `gh`, and the owner-only `/traffic/*` endpoints then fail by design, producing
`traffic_error` — the exact failure the removed step's tolerance existed for, and one the plan
draft missed. `traffic_error` is the *expected* state of an authenticated scheduled run, so the
tolerance (accept an exit confined to `traffic_error`, matching the old `ci.yml` step) must be
restored in `usage.yml`; any other `*_error` key or an invalid snapshot still fails the run as a
drift alert. `docs/IMPLEMENTATION.md` "Usage snapshot smoke relocation" is amended accordingly.

- [x] Restore the traffic-only tolerance in `.github/workflows/usage.yml` (mirror the removed
  step's `|| code=$?` pattern: the numbers check always runs, and a non-zero script exit is
  accepted only when `has("traffic_error")` and every `*_error` key is `traffic_error`).
  **Verify:** the workflow parses as valid YAML; a manual `workflow_dispatch` run completes
  `success` with the snapshot recorded in its log; the weekly cron path is the same step logic.
  **Evidence:** commit `eaa6c33` restored the pattern verbatim; the workflow parses (ruby/psych
  `OK`); `workflow_dispatch` run
  [32656326863](https://github.com/WhiteKiwi/locron/actions/runs/32656326863) concluded
  `success` — the first authenticated run with `traffic_error` tolerated, other sections
  green — and the push CI run
  [32656321754](https://github.com/WhiteKiwi/locron/actions/runs/32656321754) (head `eaa6c33`)
  concluded `success`.

## Terminal-width list table truncation backlog (2026-08-24)

Authorized by the frozen 2026-08-24 `docs/SPEC.md` amendment (Human Output Contract: Table
width). Planned in `docs/IMPLEMENTATION.md` "Terminal-width list table truncation"; evidence in
`docs/FINDINGS.md` §19.

- [ ] Amend planning documents before code: the SPEC amendment and status note, the `docs/CLI.md`
  list contract, the IMPLEMENTATION section, FINDINGS §19, and this checklist.
  **Verify:** `rg -n "Table width|no-trunc|Terminal-width list table" docs/SPEC.md docs/CLI.md
  docs/IMPLEMENTATION.md docs/FINDINGS.md` returns all four, and no planning document marks an
  unresolved decision in the new content.
- [x] Implement width resolution (`console::Term::stdout().size_checked()`), `truncate_display`
  (unicode-width), the `width` parameter on `render_list_table`, and the `--no-trunc` flag.
  **Verify:** the new unit tests pass (`truncate_display` ASCII/CJK/emoji/boundary cases;
  `render_list_table` with injected widths); `cargo fmt --all --check`, `cargo clippy -p
  locron-cli --all-targets -- -D warnings`, and `cargo test -p locron-cli` pass.
  **Evidence:** `cargo fmt --all --check` clean; `cargo clippy -p locron-cli --all-targets --
  -D warnings` clean; `cargo test -p locron-cli` 206 passed, 0 failed, including the new
  `truncate_display` unit tests (ASCII fit/no-fit, exact boundary, width-2 CJK, emoji, marker
  appended only on truncation, zero/minimum widths) and `list_table` injected-width tests
  (Some(40) truncating, Some(72)/Some(80) fitting, Some(20) too-narrow fallback, None).
  Cargo.lock gained no new package entries — the diff adds only the `console` and
  `unicode-width` edges under the `locron-cli` package. Real-PTY check (`TIOCGWINSZ` via
  pty) confirms column-count fitting: width 40 shrinks the TARGET column to the remaining 18
  display columns with a trailing `…`, `--no-trunc` restores full values, width 22/21 falls
  back untruncated, CJK targets truncate by display width. The plan's `(w, _)` destructuring
  was corrected to `(_, cols)` — console's tuple is `(rows, cols)` (verified in the console
  0.16.4 source, `unix_term.rs:53–67`); recorded in `docs/IMPLEMENTATION.md`.
- [x] Add contract tests: piped `ls` byte-identical with a long target, `--no-trunc` in the help
  walk, `--no-trunc --format json` identical to JSON without the flag.
  **Verify:** the new tests pass, and the existing help-surface walk
  (`complete_command_tree_has_consistent_help_surface`) passes unchanged.
  **Evidence:** the three new tests pass — `piped_human_list_prints_full_targets_byte_identically`
  (assert_cmd pipes stdout, so the size lookup fails and the full target prints, exact bytes),
  `list_help_advertises_no_trunc_for_list_and_its_alias` (both `list` and `ls`), and
  `list_no_trunc_is_accepted_with_json_and_ignored` (stdout and stderr byte-identical to
  `ls --format json`); `complete_command_tree_has_consistent_help_surface` passes unchanged
  (it walks the tree generically and does not pin the `list` flag set).
- [x] Run full workspace verification. **Verify:** `cargo fmt --all --check`, `cargo clippy
  --workspace --all-targets -- -D warnings`, and `cargo test --workspace` pass.
  **Evidence:** `cargo fmt --all --check` clean; `cargo clippy --workspace --all-targets --
  -D warnings` clean; `cargo test --workspace` 338 passed, 0 failed across all crates.
- [x] Parent session: publish v0.5.0 — version bump, curated git-cliff changelog, commit, annotated
  tag `v0.5.0`, push; monitor the release workflow per `docs/RELEASE.md`.
  **Verify:** the release workflow run is green (run ID recorded here), the GitHub Release is
  created with the curated notes, and the Homebrew tap update is dispatched.
  **Evidence:** release workflow run
  [32684370852](https://github.com/WhiteKiwi/locron/actions/runs/32684370852) concluded `success`
  (tag `v0.5.0`); GitHub Release
  [v0.5.0](https://github.com/WhiteKiwi/locron/releases/tag/v0.5.0) published with the curated
  changelog notes and all ten assets (four target tarballs, two `.deb`, two `.rpm`, `install.sh`,
  `SHA256SUMS.txt`); the tap formula updated by direct commit `85382afe` `bump(locron): 0.5.0`
  (the workflow commits to the tap rather than opening a PR). The changelog entry was hand-curated
  in Keep-a-Changelog format — git-cliff is not installed on the maintainer machine — which the
  release-notes extraction and future regeneration coexist with per `docs/RELEASE.md`.

Follow-ups (open, not implemented here):

- [ ] Apply the same terminal-width rule to the `history` table when a long `TRIGGER` value
  demonstrates the need.

## Tap formula audit and style backlog (2026-08-24)

Release-tooling fix — no product-behavior change, so the frozen `docs/SPEC.md` is not amended.
Planned in `docs/IMPLEMENTATION.md` "Accepted: tap formula marker and release pipeline"
(deviation note); evidence in `docs/FINDINGS.md` §20.

Background: the tap's `brew test-bot` failed on five consecutive bumps (0.4.0 through 0.5.0; the
0.5.0 run is 32684723747) with two offenses — `FormulaAudit/Miscellaneous: No need for FileUtils.
before touch` (`formula.rb:33`) and `Stable: version 0.5.0 is redundant with version scanned from
URL`. Both originate in the formula template embedded in `.github/workflows/release.yml`, so every
future bump reproduces them until the template is fixed. A first fix attempt (run 32685506683)
dropped the `version` line while keeping `#{version}` placeholders, and `brew readall` then
rejected the formula with `version (nil)` — the placeholders interpolate to nil at class-body
time, so the URL must carry the literal version for Homebrew to scan (FINDINGS §20).

- [x] Amend planning documents before code: FINDINGS §20, the IMPLEMENTATION deviation note, and
  this checklist.
  **Verify:** `rg -n "20\. Homebrew Formula|No need for FileUtils|redundant with version|version
  \(nil\)" docs/FINDINGS.md docs/IMPLEMENTATION.md docs/TODO.md` returns all three.
  **Evidence:** `rg` returns `docs/FINDINGS.md` §20 (both offenses, the `version (nil)`
  correction, and the accepted resolution), the `docs/IMPLEMENTATION.md` deviation note, and this
  checklist section; no unresolved decision marker in the new content; the SPEC is not amended
  (no product-behavior change).
- [x] Fix the `release.yml` formula template: render the literal `${VERSION}` into the four URL
  strings, drop the explicit `version "${VERSION}"` line, and write
  `touch lib/".disable-self-update"` without the `FileUtils.` prefix.
  **Verify:** the workflow parses as valid YAML; `rg -n 'FileUtils|version "|#\{version\}' 
  .github/workflows/release.yml` finds neither offense inside the template heredoc (the
  `VERSION=` assignment remains); the locron CI run after push is green (run ID recorded as
  evidence).
  **Evidence:** the workflow parses (ruby/psych `OK`); `rg` finds no `FileUtils`, `version "`, or
  `#{version}` in the template; rendering the heredoc with `VERSION=0.5.0` and the tap's real
  SHA256 values produces a formula byte-identical (modulo indentation) to the fixed tap
  `Formula/locron.rb`. Commits `7ec8d78` (drop version line, `touch`) and `0d7d449` (literal URL
  interpolation); CI runs 32685535659 and 32685866396 both `success`.
- [x] Apply the identical fix to the tap's `Formula/locron.rb` and push it.
  **Verify:** the tap's `brew test-bot` run on the fix commit is green (run ID recorded as
  evidence).
  **Evidence:** commits `5e9c6a8` and `faf5ac2` on the tap; the first attempt's `brew readall`
  rejection (`version (nil)`, run 32685506683) drove the literal-URL correction recorded in
  FINDINGS §20; test-bot run
  [32685847372](https://github.com/WhiteKiwi/homebrew-tap/actions/runs/32685847372) on `faf5ac2`
  concluded `success` — the first green run after five consecutive failures.
- [x] Parent session: record evidence here, commit the locron-side changes, push.
  **Verify:** the locron CI run on the push is green (run ID recorded as evidence); `gh run list
  -R whitekiwi/homebrew-tap` shows the fix run `success`.
  **Evidence:** locron CI run
  [32685866396](https://github.com/WhiteKiwi/locron/actions/runs/32685866396) on `0d7d449`
  concluded `success`; `gh run list -R whitekiwi/homebrew-tap` shows run 32685847372 `success`.
  Future bumps generate audit-clean formulas from the fixed template; the v0.5.0 tap formula was
  fixed directly and needs no re-release.

## Homebrew formula literal-rendering release fix (2026-08-24)

Authorized by the frozen 2026-08-24 `docs/SPEC.md` Homebrew-publication amendment and planned in
`docs/IMPLEMENTATION.md` “Accepted: literal Homebrew formula rendering”. Release v0.6.0 itself
succeeded, but the generated tap commit `b9d1d0c` lost every backtick-delimited command because the
formula body was an unquoted shell heredoc; tap test-bot run 32718394313 also found trailing
whitespace left by the removed substitution.

- [x] Replace the executable heredoc with a literal checked-in formula template and a validated
  token renderer, then make the release workflow call that renderer.
  **Verify:** the workflow parses as YAML; shellcheck passes the renderer; a fixture render contains
  the supplied version and four checksums, retains every literal backtick command, leaves no token,
  and has no trailing whitespace.
  **Evidence:** `packaging/homebrew/locron.rb.in` contains the literal Ruby body and five explicit
  token kinds; `scripts/render-homebrew-formula.sh` validates a SemVer release and four lowercase
  SHA-256 values, replaces exactly the expected token counts, and the release workflow invokes it
  after calculating the four checksums. Ruby/Psych parses `release.yml`; shell syntax, shellcheck,
  the fixed-value render, token search, and trailing-whitespace check all pass.
- [x] Add the deterministic renderer regression to push CI.
  **Verify:** the regression fails against the v0.6.0 backtick-stripped formula shape, passes against
  the checked-in template/renderer, both shell scripts pass shellcheck, and the CI workflow parses.
  **Evidence:** `scripts/test-render-homebrew-formula.sh` first verifies the intact rendered formula,
  then proves the same check rejects a fixture with all four backtick expressions removed; it also
  rejects malformed version/checksum inputs. The CI installer job shellchecks both scripts and runs
  the regression. Ruby/Psych and actionlint accept both edited workflows.
- [x] Repair `WhiteKiwi/homebrew-tap` v0.6.0 directly from the current URLs/checksums and the intact
  v0.5.0 guidance, then publish it on tap `main`.
  **Verify:** inspect staged/unstaged changes before commit; `ruby -c`, trailing-whitespace checks,
  Homebrew readall/style/audit where locally available, and the pushed tap `brew test-bot` run all
  succeed; record the tap commit and run ID.
  **Evidence:** tap commit `06ae05f` (`fix: restore locron formula guidance`) changes only the four
  damaged documentation lines, retains the v0.6.0 URLs/checksums from `b9d1d0c`, and is byte-for-byte
  identical to a v0.6.0 render from the new source template. `ruby -c`, `brew style`, whitespace,
  diff, staged/unstaged, and renderer-comparison checks pass. Tap test-bot run
  [32719051536](https://github.com/WhiteKiwi/homebrew-tap/actions/runs/32719051536) concluded
  `success` on both its Ubuntu and macOS jobs.
- [x] Verify and publish the locron release-tooling fix.
  **Verify:** formatting, workflow YAML/action lint, shell syntax/shellcheck, deterministic rendering,
  and relevant workspace checks pass; inspect staged/unstaged changes, commit with the repository
  message format, push the branch, open a PR, and record the commit, PR URL, and CI run result.
  **Evidence:** `cargo fmt --all --check`, workspace clippy with warnings denied, and all 344
  workspace tests pass locally; both scripts pass `sh -n` and shellcheck; the deterministic render
  regression, Ruby/Psych workflow parsing, actionlint, and `git diff --check` pass. Commit `5800581`
  (`fix: render Homebrew formula literally`) was staged from only the seven planned files and pushed
  as PR [#7](https://github.com/WhiteKiwi/locron/pull/7). CI run
  [32719339788](https://github.com/WhiteKiwi/locron/actions/runs/32719339788) concluded `success`
  across the installer, four lint, and eight platform/toolchain test jobs.

## Carried open items from archived backlogs

## Process-group cancellation confirmation on macOS (2026-08-25)

Authorized by `docs/IMPLEMENTATION.md` "Process-group cancellation confirmation on macOS". This
is a runner correctness and CI-reliability change; it does not amend the frozen product scope.

- [x] Treat a successful SIGKILL delivery (or `ESRCH`) plus direct-child reaping as confirmed
  process-group termination, without waiting for an orphan zombie to leave the process group.
  **Verify:** focused runner tests cover successful, already-absent, and failed SIGKILL delivery;
  the TERM-only path still requires group absence; `cancellation_kills_a_live_process_grandchild`
  passes repeatedly.
  **Evidence:** `sigkill_delivery_accepts_an_absent_group_but_not_permission_failure` covers all
  three signal results; `cancellation_kills_a_live_process_grandchild` passed 50 consecutive runs
  on macOS, including the TERM-to-KILL escalation through a live TERM-ignoring grandchild.
- [x] Run the full local quality gate and publish the CI reliability fix.
  **Verify:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  and `cargo test --workspace --all-targets` pass; the pushed macOS x86_64 stable CI job passes
  without a retry; record the run URL and result here.
  **Evidence:** all three local quality-gate commands passed; the complete 13-job CI matrix,
  including both macOS x86_64 toolchains, passed without a retry in
  [run 32743116590](https://github.com/WhiteKiwi/locron/actions/runs/32743116590).

- [ ] At the next real tag: verify the published formula creates the marker (`brew reinstall locron && locron self-update` refuses with brew guidance) and the release carries `install.sh`. Carried from the archived "Installer and self-update backlog (2026-08-23)" in `docs/TODO-archive.md`.
  **Verify:** next-tag marker/release evidence recorded in this section.

## Ordered deferred product roadmap


Every phase below is post-milestone work and requires its own reviewed SPEC before implementation;
none changes the exclusions in the current `docs/SPEC.md`.

1. [ ] Define the local HTTP viewer and mutation API, including local-port binding, authentication,
   origin/CSRF protections, exposure diagnostics, and reuse of durable application commands.
2. [x] Define the MCP surface over the same application boundary, including capability scope,
   approval boundaries, redaction, and local transport/security behavior. **Evidence:** frozen in
   `docs/mcp/SPEC.md` and `docs/mcp/IMPLEMENTATION.md`, shipped as `locron mcp` in v0.1.1 with unit
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
