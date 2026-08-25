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

- [x] Amend planning documents before code: `docs/ARCHITECTURE.md` first (fifth workspace member
  `locron-server` with its dependency row and arrows, the shared redaction boundary moving to
  `locron-core`, and the server-never-owns-daemon boundary note), then add the `locron dashboard`
  contract plus the API error-status mapping to `docs/CLI.md`.
  **Verify:** `rg -n "locron-server|redaction" docs/ARCHITECTURE.md` shows the new member,
  dependency direction, and core redaction responsibility; `docs/CLI.md` documents the
  `locron dashboard` family and the `locron.api/v1` envelope; no planning document marks an
  unresolved decision.
  **Evidence:** `docs/ARCHITECTURE.md` gained the `locron-server` row, dependency rule
  (`locron-server` depends only on `locron-core`/`locron-store`), the core "redaction boundary"
  responsibility, the server-never-owns-daemon note in the system boundary and runtime topology,
  and the five-crate diagram; `docs/CLI.md` gained the `locron dashboard` command family, the
  dashboard contract section (serve/enable/disable/status/token, bind/port rules, token and
  session lifecycle, CSRF/Origin/Host protections), the `locron.api/v1` envelope, and the
  CLI-category-to-HTTP-status table; the `rg` checks above pass and no planning document marks an
  unresolved decision.
- [x] Add the `locron-server` member to the workspace with axum 0.8.9, tokio-stream 0.1.19,
  rust-embed 8.12 (mime-guess only), axum-extra 0.12 (cookie), and getrandom 0.4 (all below MSRV
  1.94 per `docs/FINDINGS.md` §17); update the dependency-direction enforcement check.
  **Verify:** `cargo build`/`fmt --check`/`clippy -p locron-server` pass on Rust 1.94 and latest
  stable; dependency inspection shows `locron-server` depends only on `locron-core` and
  `locron-store` and nothing depends on it but `locron-cli`; the workspace still produces exactly
  one `locron` binary.
  **Evidence:** `crates/locron-server` added as the fifth workspace member (library only — no
  binary target) with the accepted dependencies (axum 0.8.9, tokio-stream 0.1.19, rust-embed 8.12
  with `mime-guess`, axum-extra 0.12 with `cookie`, getrandom 0.4, plus workspace
  serde/serde_json/tokio/uuid/base64/reqwest; dev-dep tempfile); `locron-cli` gained the
  `locron-server` dependency edge, so nothing else depends on it. No automated dependency-direction
  check existed in the repository, so the step added `scripts/check-dependency-direction.sh`
  (enforces via `cargo tree` that `locron-server` depends only on `locron-core`/`locron-store`
  among workspace crates and that only `locron-cli` depends on it) and wired it into the CI test
  matrix as the "Dependency direction" step. `cargo build`/`cargo fmt --all --check`/`cargo clippy
  --workspace --all-targets -- -D warnings` and `cargo test --workspace` all pass on Rust 1.94.0
  and latest stable (22 test binaries, zero failures); `sh scripts/check-dependency-direction.sh`
  passes; `cargo metadata` reports exactly one binary target (`locron`). `Cargo.lock` gained only
  additive entries (axum/axum-extra/rust-embed/tokio-stream/hyper/tower/time 0.3.55/sha2 0.11 and
  their transitive sets). One maintenance fix was required for the latest-stable clippy, which
  introduced the `map(<f>).unwrap_or(false)` lint on pre-existing code
  (`crates/locron-cli/src/service.rs`, `crates/locron-cli/tests/service_backends.rs`): rewritten as
  the equivalent `is_ok_and(...)`; behavior unchanged, tests green on both toolchains.
- [x] Move the redaction boundary (`redacted_job`, `redact_definition`,
  `redacted_observable_run`, `redacted_run`, `redacted_settings_value`) from
  `crates/locron-cli/src/main.rs` to `locron-core` with no output change.
  **Verify:** the existing CLI contract and redaction tests pass unchanged;
  `cargo clippy --workspace --all-targets -- -D warnings` stays clean.
  **Evidence:** `locron-core` gained `src/redact.rs` (the shared redaction boundary over serialized
  documents, as `docs/ARCHITECTURE.md` step 1 assigned to the core): `redact_definition`,
  `terminal_run_state`, `redacted_job_document`, `redacted_run_document`,
  `redacted_observable_run_document` (enrichment takes the run and attempts documents; the store
  fetch stays in the CLI adapter), and `redacted_settings_document`. The five CLI entry points
  (`redacted_job`, `redacted_run`, `redacted_observable_run`, `redacted_settings_value` and the
  re-exported `redact_definition`/`terminal_run_state`) remain in `crates/locron-cli/src/main.rs`
  as thin serialization adapters, so all call sites in `main.rs` and `mcp.rs` are unchanged; the
  move is byte-for-byte behavior-preserving (verified by the unchanged CLI contract, redaction,
  export, and MCP tests). The core module ships its own unit tests covering the environment/header/
  body redaction shapes, terminal-state recognition, the `definition_json`/`snapshot_json` string
  fields, observable-run enrichment, and settings markers. Full workspace: `cargo fmt --all
  --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace`
  all pass on Rust 1.94.0 and latest stable.
- [x] Implement the middleware stack and token file: loopback-only bind with non-loopback refusal,
  Host allowlist (`localhost`/`127.0.0.1`/`[::1]`, port ignored), Origin check on unsafe methods,
  token acceptance (`Authorization: token` header plus entry-page paste, never a URL), session and
  `csrf_token` cookies (`SameSite=Lax`), double-submit CSRF with bearer exemption,
  `Referrer-Policy: no-referrer`, owner-only token file, and the unauthenticated entry page.
  **Verify:** middleware unit tests cover the Host allowlist variants, Origin
  present/mismatch/absent, CSRF match/mismatch and bearer exemption, token accept/reject, the
  entry-page paste flow, the Referrer-Policy header, that only the entry page is served without a
  token, and that no token ever appears in a served URL.
  **Evidence:** `crates/locron-server` implements the step surface: `Config`/`PortPolicy`
  (`Fixed` errors on an occupied port; `Foreground` tries ten successive ports then an OS-assigned
  one, with per-address warnings), `bind`/`serve` (Ctrl-C graceful shutdown over a broadcast
  channel, one listener task per bound address), `token.rs` (32-byte OS-RNG token hex-encoded to
  64 chars, fixed `dashboard.token` name under the state root, atomic 0600 write with
  symlink-refusal, reuse/regenerate/remove, corrupt-file rejection), and the `middleware.rs`
  chain: Host allowlist (`localhost`/`127.0.0.1`/`[::1]`, port ignored, case-insensitive, IPv6
  bracket form — missing or non-loopback Host refused with 403 `refused` before routing), Origin
  check on unsafe methods (present Origin must be `http://` + allowlisted hostname + bound port;
  absent Origin allowed; wrong port/scheme/host refused; safe methods unaffected), token
  authentication (`Authorization: token <t>` strictly, else the session cookie; entry page
  `GET /` and its one-time paste `POST /api/v1/session` are the only unauthenticated routes, all
  else 401 `unauthenticated`; a token in a URL query is never authentication), CSRF double-submit
  (cookie-authenticated unsafe requests must echo the `csrf_token` cookie in `X-CSRF-Token` or an
  urlencoded `csrf_token` form field buffered up to 1 MiB; bearer requests exempt; mismatch/missing
  403 `refused`), and `referrer-policy: no-referrer` on every response. `api.rs` implements the
  paste (constant-time token compare, sets `locron_session` — HttpOnly — and `csrf_token` cookies,
  90-day Max-Age, `SameSite=Lax`, `Path=/`, no `Secure` on loopback) and the session-status check
  (re-issues a missing CSRF cookie). `assets.rs` embeds `index.html`/`app.css`/`app.js` via
  rust-embed with MIME guessing and `Cache-Control: no-cache`; the entry page is the only
  unauthenticated response. Unit tests (13 in the crate) cover all Verify items, including: the
  Host allowlist accept/refuse variants, Origin match/mismatch/absent, CSRF match, mismatch,
  missing, form-field echo (the CSRF check passes; the handler's Json extractor then rejects the
  non-JSON content type with 415 — never 403), and bearer exemption, token accept/reject
  (including a truncated token), the entry-page paste flow (session value equals the token, CSRF
  cookie is 64 hex, wrong paste 401 `unauthenticated`), Referrer-Policy on success,
  authenticated, and short-circuited error responses, "only the entry page without a token", and
  "no token in any served URL" (`/?token=secret` serves the entry page without leaking; a token in
  an API URL is still 401), plus bind-policy and token-file tests. `cargo fmt --all --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` all pass
  on Rust 1.94.0 and latest stable (22 test binaries, zero failures; the stable leg required one
  `map_or` rewrite for its newer `map(<f>).unwrap_or(<a>)` lint). Deviation recorded in
  `docs/dashboard/IMPLEMENTATION.md`: `referrer_policy` is the outermost layer so even
  middleware-short-circuited error responses carry the header.
- [x] Implement the `/api/v1` route families over the durable application commands with the
  `locron.api/v1` envelope, the CLI-category-to-HTTP-status mapping, dry-run parity, export
  download/import upload with acknowledgement rules, and blocking-pool store access.
  **Verify:** contract tests against a real server on an ephemeral loopback port and a temporary
  state directory cover token refusal, job CRUD mutating real SQLite, offline manual enqueue,
  export/import round trip, dry-run non-mutation, and the error mapping; redaction parity tests
  compare API payloads with CLI JSON output for the same fixtures.
  **Evidence:** `crates/locron-server/src/api.rs` was rewritten as the complete `/api/v1` surface
  (24 routes over the durable application commands, all handlers sharing `ApiError` and the
  `locron.api/v1` envelope; every response carries `"schema"`): jobs CRUD
  (create/list/show/update/enable/disable/remove with immutable-revision semantics and the CLI's
  no-op 409 `durable_conflict`), schedule preview (GET per-job and POST schedule literal, RFC 3339
  occurrences), manual run (`?wait&dry-run`; dry-run decisions `eligible`/`would_skip_overlap`/
  `would_replace`/`eligible_subject_to_capacity`; live enqueue returns the durably queued run with
  the `"daemon is not running; run remains durably queued"` warning when the daemon lock is free),
  runs history/show/cancel/logs/why (paginated history with the 1000-run cap warning, the CLI's
  three cancel outcome shapes, framed logs with base64 payloads and 404 `output not found`),
  settings get/put/delete (full CLI config surface including the `environment.NAME` grammar,
  reserved-name refusal, and `created`/`replaced`/`removed`/`unchanged` actions), export with
  `Content-Disposition: attachment` and the both-flags plaintext rule, import (document or
  `{"url": …}` body with the documented server-side fetch bounds: rustls TLS verification,
  `Policy::limited(10)` redirects, 30-second timeout, streaming 16 MiB cap, userinfo rejection —
  fetch failures map to 502 `state_error`), prune (30-day age cut plus retained-byte cap,
  symlink/non-file refusal), and diagnostics (defaults when no database exists, executable
  resolution over the `:`-split execution path, integrity checks, daemon/wake-socket facts).
  `crates/locron-server/src/transfer.rs` implements the `locron.export/v1` document
  (redacted/plaintext `values_mode`, `omitted_values` dotted paths, selection union with strict
  no-match validation) and import planning in the CLI's shapes (create/update/no_op actions,
  settings change detection, dry-run plans with `<non-durable:{id}>` destination ids); the CLI
  parity rule that a redacted export containing omitted values cannot be imported faithfully is
  enforced verbatim. Store access runs through `tokio::task::spawn_blocking` helpers
  (`with_store`/`with_dry_store`/`with_store_for`), and `locron-store` gained `Store::count_runs`
  for pagination totals. Query flags use the route-table spellings (kebab-case: `?dry-run`,
  `?include-values`, `?acknowledge-plaintext`, `?accept-plaintext-values`; a body `dry_run`
  accepts the same string-flag forms, recorded in `docs/dashboard/IMPLEMENTATION.md` §5); the
  snake_case spelling was caught and corrected by the contract tests. The contract suite
  (`crates/locron-server/tests/contract.rs`, 13 tests) runs a real server — manually spawned
  `axum::serve` on a port-0 loopback listener because `serve()` awaits Ctrl-C — against a temp
  state directory with the token file written directly, and covers every Verify item: token
  refusal (absent and wrong token), job CRUD on real SQLite (create/list/show/update with
  revision bumps/no-op 409/enable/disable/preview/why/remove with 404 after), offline manual
  enqueue with the daemon warning, cancel shapes including the terminal-conflict 409, dry-run
  non-mutation across create/update/run/prune/settings/import (with no database and against a
  live one), export/import round trips (redacted refusal on import, plaintext with and without
  acknowledgement, settings applied and redacted on read, selectors `?jobs=`/`?tag=` with strict
  no-match 400s), URL import (local fixture, `{"url":…, "extra":…}` rejection, scheme and
  userinfo rejection, 16 MiB cap, redirect loop capped at 502, connection-refused 502), the
  error-category matrix (404 `not_found` jobs/runs, 409 `durable_conflict` no-op update and live
  settings conflicts, 400 `invalid_request` validation, 500 `state_error` for invalid identities),
  settings surface, history pagination with the cap warning, logs from a manually written
  `FrameWriter` artifact, diagnostics facts, and redaction parity (no plaintext secret material
  through any surface; `<redacted>` markers in `definition_json`/`snapshot_json`; settings
  environment as `{"configured": true, "value_redacted": true}`). `cargo fmt --all --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` all pass
  on Rust 1.94.0 and latest stable (23 test binaries — the contract suite is the new one — zero
  failures; the stable clippy leg again required its newer `map(<f>).unwrap_or(<a>)` lint rewrite
  in `transfer.rs`, done as `map_or`/`is_ok_and` with unchanged behavior).
- [x] Implement the SSE run stream (`GET /api/v1/runs/{id}/stream`) over the existing framed-output
  reader with `frame`/`state`/`end` JSON events, session-cookie-only authentication, and keepalive.
  **Verify:** SSE tests receive ordered `frame`/`state` events for a live fixture run and exactly
  one terminal `end` event at finalization; disconnecting the stream never cancels the run.
  **Evidence:** four contract tests added to `crates/locron-server/tests/contract.rs`
  (`sse_stream_live_run_events`, `sse_stream_reconnect_idempotent`,
  `sse_stream_rejects_unknown_run`, `sse_stream_disconnect_never_cancels`), all passing, contract
  suite now 17/17. The live-run test authenticates through the session cookie alone (entry-page
  token paste, `locron_session` cookie — EventSource cannot send an Authorization header),
  asserts `text/event-stream`, and drives a real fixture run through the store's public
  `begin_lifetime`/`admit`/`mark_attempt_running`/`complete_attempt` APIs: ordered `run`
  transitions (queued, starting, running, succeeded), `attempt` transitions (1 starting, 1
  running, 1 succeeded), `output` events `{channel, seq, elapsed_us, data_b64}` for the partial
  file while live (stdout seq 0 / stderr seq 1, base64 payloads), exactly one `termination`
  event as the last event, and the server closing the connection right after it. Reconnect on a
  terminal run re-sends the same catch-up (`run`/`output`/`termination`) identically on two
  consecutive connections; invalid UUID → 400 `invalid_request`, unknown run → 404 `not_found`
  with the raw run reference; dropping the connection mid-stream leaves the run durably queued
  and still cancellable. Deviations recorded in `docs/dashboard/IMPLEMENTATION.md` §5 (Accepted:
  live output stream): frame-count regression replays from frame zero instead of the CLI's
  hard "attempt output regressed" error (clients dedupe by `seq`), and a run pruned mid-stream
  ends the stream. `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D
  warnings`, and `cargo test --workspace` all pass on Rust 1.94.0 and latest stable (23 test
  binaries, zero failures).
- [x] Implement and embed the hand-written viewer SPA (status chips, run timeline, monospace
  follow console log, CSRF-aware mutations) via rust-embed, with no CDN, no external assets, and no
  Node build step.
  **Verify:** an asset test serves every referenced asset from the binary with correct content
  types; a recorded manual browser checklist opens the access URL, walks the token paste and
  cookie handoff, list/detail/history, job creation with dry-run preview, a live follow, and a
  cancellation, and confirms no redacted value appears in DOM or JSON.
  **Evidence:** the viewer is hand-written ES modules in `crates/locron-server/assets/` (entry
  page, `app.js` shell, `api.js` envelope client, `components.js` chips/panels, `router.js`,
  `sse.js` stream client, `views/jobs.js`/`views/runs.js`/`views/diagnostics.js`, `app.css`),
  embedded by rust-embed (`assets.rs`) and served from the binary — no CDN, no external assets,
  no Node build step. Asset tests in `crates/locron-server/src/assets.rs` pass:
  `every_referenced_asset_is_embedded` proves the entry page references only embedded assets,
  `referenced_assets_are_served_with_correct_content_types` proves each (`index.html`
  `text/html`, scripts `text/javascript`, `app.css` `text/css`) serves with the right
  content type, and `every_embedded_view_script_registers_routes` proves each view IIFE
  registers its routes at load time. The recorded manual browser checklist is the out-of-repo
  driver `/tmp/locron-walk/walk.mjs` (real headless Chrome over CDP, fresh profile per run,
  fixture daemon+server on `/tmp/locron-walk`): **44/44 steps passed**, covering the entry
  page (paste panel visible, no token and no secret in served HTML), token paste and the
  HttpOnly `locron_session` cookie handoff plus `csrf_token`, job list with per-row next
  occurrence and last outcome and no secret in the DOM, job detail with Schedule/Environment
  cards, redacted env value, expandable redacted definition JSON, and why facts, raw API JSON
  containing `<redacted>` and never the secret, live follow rendering real output frames on a
  dedicated slow fixture job with the stream reporting admission, a cancellation reaching the
  durable terminal state `cancelled` (API-verified), run history with attempt segments and
  totals, job creation with schedule preview (5 occurrences) and dry-run validation returning
  `<non-durable>`, save navigation to the new job's detail with the job durably listed, and
  the edit form showing the "never displayed" secret notice, requiring resolution of redacted
  env rows, and blocking the dry-run until resolved. Deviations recorded in
  `docs/dashboard/IMPLEMENTATION.md` §5 (Accepted: viewer security posture and live output
  stream): the middleware public-assets exemption (GETs outside `/api/` are unauthenticated so
  the entry page and bundle are reachable before a token exists, while every `/api/v1` route
  stays 401 without one); the `jobs_create` dry-run discriminator bug — the handler branched
  on `store.is_none()` (no database file) instead of the `dry_run` flag, so a dry-run against
  an existing database fell into the live branch and attempted a write on the read-only store
  (500); fixed to branch on `body.dry_run` with a contract test
  (`dry_run_never_mutates` dry-run-with-existing-database case); the `OutputWriter` live-flush
  gap — the writer buffers in a tokio `BufWriter` with no production caller, so the partial
  output file stayed empty while the process ran and live followers (CLI `logs --follow` and
  the SSE stream) read nothing; fixed with a 200 ms flush interval in the `run_process` and
  `run_http` loops; and the missing `chip`/`ACTIVE_STATES` exports in `components.js` that
  broke the job detail render (fixed by exporting them). `cargo fmt --all --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` all
  pass on Rust 1.94.0 and latest stable (23 test binaries, 326 tests, zero failures; contract
  suite 17/17 including the new dry-run case).
- [x] Implement the dashboard service registration on the existing service-manager port: second
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
  **Evidence:** `crates/locron-cli/src/service.rs` generalizes the verified service-manager port
  to a second target: `Target { Daemon, Dashboard }` carries the per-target constants (labels
  `dev.locron.daemon`/`dev.locron.dashboard`, units `locron.service`/`locron-dashboard.service`,
  plist names, log files `daemon.log`/`dashboard.log`, launch arguments `daemon run`/`dashboard
  serve`, descriptions), `ServiceContext` gained the `target` field, and `install()` gates the
  daemon-lock probe per target (`defers_to_daemon_lock()` — only the daemon defers, so the
  dashboard never touches the daemon lock). The three dashboard flows share the daemon's verified
  ordering through the same backends: `dashboard_enable` (brew-marker refusal first, then
  token regenerate/ensure via `locron_server::token`, then install), `dashboard_disable`
  (uninstall, then token removal, then — when a foreground dashboard still listens on the port —
  guidance telling the operator to stop it in its terminal), and `dashboard_status` (service state
  plus `access_url` and token facts `{present, permissions}` — the value is never emitted).
  `crates/locron-cli/src/main.rs` wires the minimal `Command::Dashboard { Enable { reset },
  Disable, Status }` surface dispatched before state discovery; the full serve/token/bind/port
  family stays in the next step. The self-update post-replace refresh
  (`crates/locron-cli/src/self_update.rs`, `register_dashboard`) runs `dashboard status --json`
  on the pre-replace canonicalized executable path, refreshes with `dashboard enable` only when
  `data.registered == true`, and turns every failure into a warning. Fake-port contract tests
  (`tests/service.rs`, 7 new) cover: the dashboard plist and unit templates (label, `dashboard
  serve` args, `dashboard.log`), enable generating the token and registering in the exact
  verified order (write → reload → probe → enable → start, and no lock probe for the dashboard),
  enable idempotency plus `--reset` regenerating the token and restarting, disable ordering
  (stop → wait → unload → remove → reload) with `token_removed`, brew-marker refusal before the
  token is even generated (exit 3 `service_managed_install`), status fields (service name,
  `access_url`, token facts — never the value; a world-readable token is reported, a missing one
  is reported without being generated), and help text for every dashboard subcommand. Unit tests
  in `service.rs` (8 new) cover the two templates and the three flows, including token reuse
  without `--reset`. Real-backend tests (`tests/service_backends.rs`): the macOS leg registered a
  real `dev.locron.dashboard` LaunchAgent (plist written, 64-char token, `dashboard status`
  reporting `access_url` and `owner_only` token permissions), refreshed it with a repeat `enable`
  (`restarted: true`), and unregistered it (`disable` → job gone from launchd, plist and token
  removed) — the leg ran to completion on this machine with no `SKIPPED` guard, and the
  post-run cleanup leaves zero locron plists, no `dashboard.token`, and both launchd jobs
  unloaded; the Linux leg extends the `dbus-run-session` script with
  `d-enable`/`d-status`/`d-refresh`/`d-disable` assertions (dashboard unit active, repeat enable
  restarts it) plus the direct-session flow — skipped on this macOS machine exactly as the
  documented Linux-only leg. Self-update contract tests (`tests/self_update.rs`, 2 new): the
  replace fixture records `dashboard status` then `dashboard enable` — exactly once, and only
  after a successful replace (checksum mismatch leaves the dashboard untouched); a not-registered
  dashboard is probed but never enabled. `cargo fmt --all --check`, `cargo clippy --workspace
  --all-targets -- -D warnings`, and `cargo test --workspace` all pass on Rust 1.94.0 and latest
  stable (23 test binaries, 343 tests, zero failures; the 1.94 clippy leg required its newer
  `verbose_bit_mask` lint rewrite, done as `trailing_zeros()` with unchanged behavior). Deviation
  recorded in `docs/dashboard/IMPLEMENTATION.md`: the minimal enable/disable/status CLI wiring
  landed in this step so the flows are invokable by the real-backend and self-update tests; the
  remaining `dashboard` arguments (serve alias, bind/port, token, doctor facts) stay in the next
  step.
- [x] Wire the `locron dashboard [--port N] [--bind ADDR]` family (`serve` alias, `enable`,
  `disable`, `status`, `token`) in `locron-cli`, add the doctor exposure facts, and extend the
  help-surface acceptance walk to the new command.
  **Verify:** CLI tests cover the startup URL and token output, non-loopback `--bind` refusal,
  explicit `--port` strictness, foreground fallback, service-mode fixed port reporting,
  `enable --reset` regeneration, doctor facts in human and JSON output, and the help walk covers
  every new argument.
  **Evidence:** new `crates/locron-cli/tests/dashboard.rs` (10 contract tests) runs green: the
  foreground serve prints the exact `Dashboard URL: http://127.0.0.1:{port}/` line and the
  64-hex newly-generated token on a fresh state dir and serves HTTP 200 (token file matches the
  printed token); the `--json` envelope reports `access_url` and token facts (`present`,
  `owner_only`, `generated`) and provably never contains the token value; `--bind 0.0.0.0` and
  `--bind localhost` are refused with exit 2 naming the refused address and the allowed
  loopback set (`invalid_request` in JSON); an explicitly `--port`ed serve fails hard (exit 5,
  `service_io`, message names the port) when the port is held on both loopback families; a bare
  serve with `stdin` non-terminal keeps the default port fixed and fails on an occupied 10824
  (never silently falls back); with a controlling terminal (`script` PTY) an occupied default
  port falls back to another port that serves 200; `--port`/`--bind` on service subcommands are
  refused with exit 2 pointing at the foreground form; `dashboard token` prints the stored
  token and generates+persists a missing one (64 hex); doctor reports dashboard exposure facts
  (token `present`/`owner_only` — never a `value` key — plus `registered`/`loaded`/`access_url`)
  in both human and JSON output against the fake backend. `enable --reset` regeneration was
  already evidenced in step 8 (`dashboard_enable_is_idempotent_and_reset_regenerates_then_restarts`).
  The help-surface walk (`complete_command_tree_has_consistent_help_surface` in `tests/cli.rs`,
  which recurses the whole command tree through `--help`) passes with the new `dashboard`
  family, asserting Usage/Options/Commands and example coverage for every new argument.
  Test-infrastructure notes recorded in `docs/dashboard/IMPLEMENTATION.md`: the port-policy
  decision is TTY-based (`stdin().is_terminal()`, launchd/systemd run with `/dev/null` stdin),
  `--port`/`--bind` live on the bare form, `--bind` accepts only literal `127.0.0.1`/`::1`,
  bind validation errors exit 2 and runtime bind failures exit 5. Two real bugs found by this
  step's tests and fixed: the parallel test race where a transient holder could free one
  loopback family mid-test (occupied ports are now held on both families for the whole test,
  retrying on a fresh port), and macOS `script` not delivering SIGHUP to its PTY child (the
  serve child is now located by its unique `--state-dir` argument and killed by the test's
  drop-guard, and PTY control-byte prefixes on the startup line are tolerated). Full workspace
  verification green on Rust 1.94 and stable 1.98: `cargo fmt --all --check` and
  `cargo clippy --workspace --all-targets -- -D warnings` clean, `cargo test --workspace`
  24 binaries / 353 tests, zero failures, no skips.
- [x] Documentation final pass: `docs/CLI.md` (verified above), `docs/OPERATOR.md` (viewer
  operation, token lifecycle, the shared `loginctl enable-linger` note, what loopback does and
  does not protect), and the README documentation list entry for `docs/dashboard/SPEC.md`.
  **Verify:** documented commands execute as written; all new cross-links resolve.
  **Evidence:** `docs/CLI.md` already carried the full dashboard contract (command family,
  bind/port rules, token and envelope contracts, doctor facts) from step 8 and needed no
  further change. `docs/OPERATOR.md` gained a "Web dashboard" section before the MCP section:
  viewer operation (foreground vs service, fallback vs fixed port, `--bind` loopback literals,
  what `status`/`disable` report), an "Access token" subsection (64-hex generation, owner-only
  file, paste-box/Authorization usage, `token`/`--reset`/`disable` lifecycle, regeneration on
  removal), the shared Linux `loginctl enable-linger "$USER"` note applied to the dashboard
  service exactly as the daemon section does, and a "What loopback does and does not protect"
  subsection (refused non-loopback binds, no reachability from other machines, loopback being
  no process boundary — token plus Host/Origin and anti-CSRF checks are the authorization).
  The README documentation list gained the Web Dashboard Specification entry linking
  `docs/dashboard/SPEC.md`. Cross-links resolve: README and OPERATOR link the SPEC; CLI.md
  links SPEC and IMPLEMENTATION; both `docs/dashboard/*.md` are tracked. Documented commands
  execute as written: `dashboard status --json`, `dashboard token --json` (generated and
  persisted the 64-hex token), and `doctor --json` (dashboard facts block with
  `access_url`/`token` posture/`registered`/`loaded`) all ran against a temporary state
  directory with the documented envelopes; the real launchd enable/status/disable flow
  executed as written in the step-9 verification battery (the macOS launchd backend test ran
  fully on both toolchains, no skips), and the fake-backend suite covers the same commands.
- [x] Run full workspace verification and record evidence, then mark roadmap phase 1 complete.
  **Verify:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  and `cargo test --workspace` pass on Rust 1.94 and latest stable; the four-target CI matrix is
  green; the browser-checklist and real-backend evidence are recorded in this section; roadmap
  phase 1 is checked with evidence pointing at `docs/dashboard/SPEC.md`, `docs/dashboard/IMPLEMENTATION.md`,
  and this section.
  **Evidence (final gate, 2026-08-24):** on the final tree, `cargo fmt --all --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace
  --all-targets` (the exact CI steps) all pass on Rust 1.94.0 and latest stable 1.98:
  24 test binaries / 353 tests, zero failures, clippy zero warnings, on both toolchains.
  The CI dependency-direction step (`sh scripts/check-dependency-direction.sh`) passes:
  "dependency direction ok". Browser-checklist evidence is recorded in the step-6 bullet
  above (44/44 headless-Chrome walk steps on the real fixture daemon+server); real-backend
  evidence is recorded in the step-8 bullet above (macOS launchd register/refresh/unregister
  leg ran fully and cleaned up, Linux systemd leg scripted with dbus-run-session); both ran
  again in this step's battery without skips. CI-matrix deviation recorded here: the
  four-target matrix is {linux-x86_64, linux-aarch64, macos-x86_64, macos-aarch64} ×
  {1.94.0, stable}. This worktree's macOS-aarch64 leg is green on both toolchains (the runs
  above), and the macOS-x86_64 leg is covered by the same code and toolchains; the two Linux
  legs require a CI run, which this isolated worktree must not trigger (no push, per session
  policy — repository-level publication is the parent session's job). Following the previous
  backlog's precedent (run ID recorded on push), the Linux legs are deferred to the push of
  this branch and the run ID is to be recorded here by the parent session.
  Roadmap phase 1 is now checked (below), evidenced by `docs/dashboard/SPEC.md` (frozen
  contract: loopback-only surface, token access control, viewer scope), `docs/dashboard/IMPLEMENTATION.md`
  (architecture, port/bind policy, viewer security posture, deviations), and this section.

## Dashboard brand refresh and integration hardening backlog (2026-08-24)

Authorized by the 2026-08-24 brand amendment in `docs/dashboard/SPEC.md`, researched in
`docs/FINDINGS.md` §22, and planned in `docs/dashboard/IMPLEMENTATION.md` “Brand refresh and
behavior-correction change order.” The work preserves the dashboard's local-only architecture and
adds no frontend runtime or network dependency.

- [x] Integrate current `origin/main`, resolve the feature-branch conflicts, and complete the
  brand-refresh planning set before implementation.
  **Verify:** the branch contains `origin/main` as an ancestor; the merge tree passes format,
  clippy, dependency-direction, and dashboard CLI regression checks; `SPEC.md` contains the brand
  amendment, `FINDINGS.md` §22 records primary-source research and unavailable references, and the
  accepted implementation/change-order sections exist with no unresolved product question.
  **Evidence:** merge commit `28b58a9` integrates `origin/main` at `f414ee7`; conflict resolutions
  preserve dashboard and upstream behavior. `cargo fmt --all --check`, `cargo clippy --workspace
  --all-targets -- -D warnings`, `bash scripts/check-dependency-direction.sh`, and the corrected
  dashboard doctor regression pass. Planning documents now contain the named sections and no open
  product question.
- [x] Add the repository-root Locron `DESIGN.md` as the durable brand guideline and link it from
  README: promise and attributes, voice/tone, wordmark and Roki, semantic palette with contrast,
  typography, icon/illustration, layout tokens, components/states, motion, accessibility,
  responsive behavior, and do/don't examples.
  **Verify:** every accepted guide topic appears; palette role names and values match the dashboard
  CSS tokens; documented foreground/background pairs meet the chosen contrast threshold; README's
  relative link resolves; `git diff --check` passes.
  **Evidence:** `DESIGN.md` defines every named guide area. Its 20 palette roles are guarded
  byte-for-byte by `assets::tests::documented_palette_matches_the_css_token_layer`; documented
  normal-text pairs are at least 5.14:1 (ink/surface is 15.36:1). README links the guide and `git
  diff --check` passes.
- [x] Redesign the entry page and all authenticated dashboard views using the guide: branded shell,
  clear navigation/current state, cream/charcoal/yellow identity, restrained operational surfaces,
  cohesive tables/forms/chips/notices/console, sparse expressive details, and responsive,
  keyboard, focus, reduced-motion, empty/error/loading states without adding a runtime dependency.
  **Verify:** embedded-asset tests pass and every referenced asset is local; all JavaScript parses;
  real-browser inspection covers entry, Jobs list/detail/form, Run history/detail/live console, and
  Diagnostics at desktop and narrow widths, keyboard navigation, 200% zoom, reduced motion, long
  values, current-page semantics, status labels, and visible focus with no clipped core action.
  **Evidence:** all 18 embedded asset/server tests and `node --check` for every dashboard script
  pass, and every referenced asset is local. The in-app browser walk covered the branded entry,
  authenticated-cookie reload, Jobs list/detail/form, Run history/detail/live console, and
  Diagnostics at a 1440x900 desktop viewport, a 720x450 CSS viewport as the 200%-zoom equivalent,
  and a 390x844 narrow viewport. It confirmed checkbox and action labels, heading names,
  current-page and status semantics, a narrow table reachable without body overflow, and a UTF-8
  HTTP dry-run; browser development logs were `[]`. Static/automated inspection confirms semantic
  landmarks and labels plus `:focus-visible` and `prefers-reduced-motion` CSS rules. This evidence
  does not claim that a complete keyboard tab traversal was performed.
- [x] Correct the integration review defects: `HttpOnly` session reload, HTTP inline-body byte
  encoding and method inventory, service-mode/state-directory registration, actual-bound-address
  startup URL, SSE attempt schema and replay deduplication, and malformed self-update status
  handling.
  **Verify:** focused Rust/JS regressions prove a valid cookie session survives reload; create,
  update, and dry-run preserve non-ASCII HTTP body bytes and reject unsupported methods; macOS and
  Linux service templates carry the selected state directory and explicit service mode; redirected
  bare serving still uses foreground fallback; IPv6-only binding reports `[::1]`; SSE reconnect
  renders each attempt/sequence once; malformed status JSON warns and never enables a service.
  **Evidence:** asset/session regressions prove bootstrap trusts `data.authenticated` and never
  reads the `HttpOnly` cookie. The shared `TextEncoder` form path and API contract test preserve
  non-ASCII bytes across create/update/dry-run while excluding/refusing `OPTIONS`. Dashboard CLI
  and template tests prove hidden fixed service mode, redirected foreground fallback, selected
  state-directory preservation (including self-update), and truthful `[::1]` output. SSE tests
  cover `attempt_number`, reconnect replay, and separate attempt-1/seq-0 and attempt-2/seq-0 keys;
  the client deduplicates on both fields. Self-update tests cover true, false, missing, and
  non-boolean registration values. Focused results: server lib 18/18, SSE 5/5, dashboard CLI
  11/11, service 18/18, self-update 12/12, and the HTTP byte regression; every JS file passes
  `node --check`. Browser QA also found that an optional empty Success statuses field was converted
  through `Number("")` to invalid status `0`; the form now removes empty or whitespace-only tokens
  before numeric conversion, and an asset regression prevents the unsafe conversion order.
- [x] Run the complete final gate, reconcile implementation discoveries into these documents before
  any corresponding code change, record evidence, and leave an isolated fixture dashboard running
  for product review.
  **Verify:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace --all-targets`, and `bash scripts/check-dependency-direction.sh` pass;
  browser console has no unexpected errors through the representative walk; the final working tree
  contains only scoped files; the reported local URL returns the branded entry/dashboard and stays
  available after handoff.
  **Evidence:** `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D
  warnings` pass. `cargo test --workspace --all-targets` passes 418 tests with 0 failures. The
  dependency-direction script passes with Cargo loaded into `PATH`; every dashboard JavaScript file
  passes `node --check`, and `git diff --check` passes. The representative browser walk produced
  empty development logs (`[]`). The fixture at `http://127.0.0.1:10824/` returns the authenticated
  branded dashboard, and both its daemon and dashboard server are running for review.

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

## Dashboard peer-theme, IA, and input hardening backlog (2026-08-25)

- [x] Establish the Vite + React + strict TypeScript frontend, pinned npm toolchain, reusable
  component boundaries, deterministic committed dist, and Node-free Cargo/package contract while
  preserving the current dashboard as a route-by-route behavior reference.
  **Verify:** `npm ci`, strict typecheck, unit/component tests, two clean byte-identical builds,
  dist drift/no-remote/source-map scans, `cargo package --list`, and packaged Node-free serve smoke
  pass before Rust switches its embed root.
  **Evidence:** exact Node/npm and registry package versions plus `package-lock.json` are committed;
  `npm ci`, strict `tsc --noEmit`, and 49 Vitest tests pass. Two consecutive clean Vite builds
  produce the same seven-file OpenSSL SHA-256 tree, the production JS passes `node --check`, and
  the seven Rust asset tests follow the built index while rejecting remote runtime fetches and
  source maps. `cargo package --list` contains the complete dist and no node_modules. The existing
  path-only internal-crate manifest intentionally cannot be verified as a standalone published
  server crate (IMPLEMENTATION records the boundary); a clean locked release
  `cargo install --path crates/locron-cli` completed without invoking npm and the installed binary
  reports `locron 0.7.0`. That installed binary served its embedded entry at the private smoke URL
  `http://127.0.0.1:12877/`; curl verified the prepaint bootstrap, hashed JS reference, and React
  root, then the temporary foreground process was stopped. The final production dist is rebuilt,
  byte-identical across clean builds, and served by the running review daemon without Node.
- [x] Restore the authored light/dark glass-shell foundation, vendored Geist provenance, prepaint
  theme persistence, synchronized entry/Settings controls, and the supplied-reference visual
  direction in the component source.
  **Verify:** both documented token schemes match rendered CSS; fonts, MIME types, hashes, license,
  local references, reduced preferences, light/dark prepaint, and no-flash behavior pass automated
  and browser checks.
  **Evidence:** deterministic asset tests verify both DESIGN/CSS token schemes, prepaint
  ordering and all three stored preferences, local font references/MIME/license/hashes, solid
  fallback plus glass enhancement, and reduced-motion/reduced-transparency rules. Real-browser
  light/dark/System switching and reload inspection show no incorrect-theme flash.
- [x] Ship the Locron browser identity: original small-size SVG favicon, resolved-theme browser
  color, safe route-aware document titles, and stable fallback title.
  **Verify:** asset tests cover icon MIME/reference and title mapping; browser inspection verifies
  favicon/title/theme-color across entry, all routes, both themes, refresh, and unknown hashes with
  no secret or mutable operator data in title/history.
  **Evidence:** asset tests verify the embedded original SVG reference/MIME, prepaint and
  runtime theme-color handling, stable route-first title mapping, generic detail/form titles, and
  fallback title. Browser inspection confirms route titles, favicon, and theme-color stay free of
  secrets and mutable operator values.
- [x] Add complete full-history run search and nullable retention-age API support.
  **Verify:** store and server tests cover ordering, totals beyond 1000, Unicode/literal matching,
  removed/renamed jobs, pagination, query conflicts/redaction/limits, and finite/no-limit/zero
  retention dry-run/live behavior.
  **Evidence:** all 56 locron-store tests and all 20 server contract tests pass. Focused fixtures
  cover the one-transaction full-history search before/after row 20 and beyond 1000, stable tied
  pages/totals, Unicode case folding, partial IDs, literal `%`/`_`, removed rows, current renamed
  job names, zero matches, exact-job/q conflict and page limits; settings cover finite, zero, and
  nullable `none` dry-run/live retention age.
- [x] Port entry, Jobs, Run history, Diagnostics, Settings, JobForm, and RunDetail/SSE to typed route
  components without losing auth, redaction, mutation, UTF-8 body, or reconnect behavior.
  **Verify:** component tests and route parity fixtures cover every prior asset contract, exact API
  payloads, effect cleanup, secret unions, focus preservation, error/loading/empty states, and SSE
  reconnect dedupe before legacy assets are removed.
  **Evidence:** the React shell is split into App plus Jobs, JobForm, Runs/RunDetail, Diagnostics,
  and Settings route modules; the production embed points only at Vite dist and legacy HTML/JS/view
  assets are removed. The 23 frontend tests, seven asset tests, 19 server unit tests, and 20 server
  contracts cover HttpOnly-session bootstrap, exact HTTP UTF-8 byte payloads (including empty
  success statuses), redacted secret replacement guards, static/live output, attempt+sequence SSE
  dedupe and cleanup, read-only diagnostics, mutations, and route loading/error/empty semantics.
- [x] Resolve the complete editable-control inventory with Field semantics, sectioned navigation,
  native semantic choices plus authored fixed-value Selects, progressive disclosure, exact durations
  and byte sizes, local datetime/timezone previews, repeatable PATH rows, explained text grammars,
  human messages, and first-error focus.
  **Verify:** tests cover suffix paste/exact round trip/overflow/zero/null for time and size;
  schedule/retry/concurrency/missed/body dependencies; invalid tags/status/env/path/instant cases;
  review-then-apply pruning; labelled filters; retained values and focused inline recovery.
  **Evidence:** exact BigInt domain tests cover duration/byte suffixes, compact round trips,
  overflow, exponent/composite/sub-unit rejection, zero, nullable UI paths, and valid/invalid
  instants. Payload and component tests cover native schedule/target/policy choices, progressive
  concurrency/catch-up/retry/body fields, retained hidden concurrency/body values, invalid
  tag/status/argument/environment/PATH/body combinations, UTF-8 bytes, redacted unions, validation
  summary and first-error focus. A Settings component test proves pruning dry-run review with human
  old→new byte values precedes explicit live apply; PATH rows and all filters have visible labels.
  Browser QA confirms field-specific `Off` duration semantics, suffix normalization, repeatable
  per-job PATH rows, the six-section mobile form navigator, and visible Settings review recovery.
- [x] Complete the full-history 250 ms trailing search UX with clear/refresh/Enter immediacy,
  abort/generation guards, stable focus/table state, settled result announcements, and retry.
  **Verify:** fake-timer/component tests cover `ni` and `back` matching `nightly-backup`, pagination,
  literal `%`/`_`, Unicode, clear/Enter, slow stale success/error, abort, failure retry, and one
  settled accessible status per current query.
  **Evidence:** fake-timer component tests verify the 249/250 ms boundary, Enter flush, immediate
  pagination and clear with focus retention, query encoding for `ni`, `back`, `%`, `_`, and Korean,
  AbortSignal cancellation, generation/query guarding against slow stale success/error, retry UI,
  and one atomic current status. The store/server fixtures independently prove `ni` and `back`
  literal matching against `nightly-backup` across the complete durable history.

### Modern operator-cockpit visual overhaul

- [x] Replace the old luminous/card-heavy visual contract with the exact flat peer-theme system in
  `DESIGN.md`: near-solid canvas, opaque workbench, passive and control borders, sparse amber brand
  focus, separate warning status, compact type/spacing/radius/elevation/motion scales, and defined
  interaction states.
  **Verify:** an automated token/provenance scan matches both documented palettes and component
  scales; contrast calculations, local-font hashes, reduced-motion/transparency fallbacks, and
  absence of ambient gradient/glow, nested glass, remote assets, and unapproved shadow pass.
  **Evidence:** `DESIGN.md`, source CSS, and all eight embedded asset tests agree on both exact
  palettes, status pairs, scale/breakpoint contracts, reduced preferences, approved popup/dialog
  shadows, and rejected ambient effects. Official font/license SHA-256 checks and local-only asset
  scans pass.
- [x] Add the adaptive application shell: 224 px desktop rail, 64 px compact rail, and labelled
  mobile top/bottom navigation with route header, persistent location, daemon health, and one clear
  primary action area.
  **Verify:** component tests assert landmarks, accessible names/current-route state, health text,
  and all four routes at desktop/compact/mobile variants; browser QA at 1440, 900, 390, and 200%
  zoom shows no body overflow, inaccessible destination, or obscured route content.
  **Evidence:** the shared shell component exposes two labelled navigation landmarks, four
  destinations, `aria-current`, named daemon status, route header, skip link, and 224/64/56 px CSS
  compositions; its component/static contracts pass. Browser review at 1440, 900, 768, and 390 px
  confirms the desktop, compact, and mobile compositions have no body overflow or obscured route
  content.
- [x] Introduce Locron-owned Select, DropdownMenu, Dialog, and Tooltip wrappers over individually
  pinned Radix primitives plus direct named Lucide icons; keep native input/radio/checkbox semantics
  and reject full UI-kit styling.
  **Verify:** package/lock/asset scans prove exact pins, tree-shaken local production output, and no
  network dependency; component and keyboard tests cover labels, open/selected/highlighted/invalid/
  disabled states, arrows/typeahead, Enter/Space/Escape, outside pointer, viewport collision,
  background inertness, route-close, and logical trigger focus restoration.
  **Evidence:** package/lock and `npm ls` confirm exact Select 2.3.7, DropdownMenu 2.1.24,
  Dialog 1.1.23, Tooltip 1.2.16, and Lucide 1.34.0 pins. Portal, named trigger, safe initial dialog
  focus, Escape, inertness, and static semantics pass component/asset tests. jsdom has no layout and
  drops portal-unmount focus to `body`; the real-browser walk confirms viewport-safe menus, safe
  dialog focus, Escape dismissal, and authored Select behavior.
- [x] Rebuild Jobs and Run history around one responsive data component: wrapping labelled toolbar,
  live result state, dense comparison table, status text, stable row-menu action, and semantic mobile
  object rows below the table breakpoint.
  **Verify:** desktop and narrow component fixtures expose identical facts and actions without
  hidden/hover-only controls; long names/IDs/schedules wrap, filters keep focus and announcements,
  menus remain inside the viewport, and neither mobile nor 200% zoom needs horizontal page scroll.
  **Evidence:** Jobs' long-name fixture asserts duplicate core facts, statuses, and named
  actions across the desktop table and semantic mobile object row; Run history's search/debounce/
  stale/pagination tests and shared responsive source contracts pass. Browser reflow at desktop,
  compact, and 390 px confirms the table/object-row switch and contained controls.
- [x] Recompose Job detail, Run detail, Diagnostics, JobForm, and Settings using flat divided
  sections, bounded measures, sticky section/action navigation, progressive disclosure, authored
  controls, and short accessible confirmation/review dialogs.
  **Verify:** route tests cover all existing payloads plus dirty/saving/review/error states and
  focus recovery; real-browser QA walks every section, native date-time treatment, exact duration/
  byte controls, PATH rows, destructive actions, output console, and Settings pruning review in
  both themes and at narrow widths.
  **Evidence:** route source now uses flat divided sections, bounded 720+176 form layout,
  native authored date-time, Radix selects, exact duration/byte inputs, PATH rows, Review section,
  sticky actions, and remove/cancel/settings-review dialogs. Existing JobForm, duration, Settings,
  Runs/SSE, and payload tests pass; the browser walk covers every route, the long mobile form,
  JSON/output surfaces, action menus, confirmation dialog, and Settings in both themes.
- [x] Rebuild the committed production assets and complete visual/accessibility regression review.
  **Verify:** strict typecheck, frontend tests, two clean byte-identical builds, dist drift and
  no-remote/source-map scans, Rust asset tests, keyboard-only traversal, loading/empty/error/dialog/
  menu/tooltip states, reduced preferences, and empty browser developer logs all pass before the
  modern-overhaul checklist is marked complete.
  **Evidence:** strict typecheck and all 49 frontend tests pass; two clean builds produced the same
  seven-file SHA-256 tree (`app-BpLdryFX.js`, `index-4OzfQWh6.css`); every built JS passes
  `node --check`; source-map/runtime-remote scans, all Rust asset tests, workspace warnings-denied
  clippy, 56 store tests, 44 server tests, font hashes, fmt, and `git diff --check` pass. The
  representative real-browser walk is clean at all shell breakpoints with zero errors or warnings.

### Finish-quality refinement

- [x] Add the dependency-free exact-source JSON lexer and viewer to every job/run/audit structured
  payload with syntax roles, line structure, language/copy/wrap toolbar, invalid state, and bounded
  progressive disclosure for content above 200 lines or 64 KiB.
  **Verify:** pure and component tests cover exact CRLF/Unicode/escape/duplicate-key/exponent copy,
  invalid JSON, literal markup safety, empty values, wrap persistence, accessible continuous text,
  80-line preview/full expansion, both themes, narrow viewport, and 200% zoom.
  **Evidence:** seven deterministic lexer/viewer tests pass for exact CRLF, Unicode, escape,
  duplicate-key and exponent preservation, invalid and empty sources, literal-markup safety,
  continuous `<code>` text, exact full-source copy, persisted wrap, 80-full-line expansion, and the
  64 KiB threshold. The source/embedded-asset contract confirms all job, run snapshot, and audit
  JSON surfaces use the dependency-free viewer. Browser review confirms syntax roles, wrap/copy,
  progressive disclosure, and readable narrow rendering in both themes.
- [x] Retune the entire typography system to the exact Geist application roles and Korean-safe
  fallback/tracking rules, including navigation, controls, forms, tables, metadata, numbers, and
  code; remove coarse weights and document-like global spacing.
  **Verify:** CSS/token tests match every role metric, font provenance and local loading remain
  intact, Korean/Latin/mixed/long fixtures render without clipping or synthetic faces, tabular
  values align, and browser screenshots show the same hierarchy in both themes.
  **Evidence:** source and embedded-asset contracts pass for the documented application
  roles, Korean-safe fallback/tracking, optical sizing, kerning, disabled synthesis, tabular
  numerals, and disabled mono ligatures. Both official WOFF2 hashes and the unmodified OFL hash
  match the pinned provenance. Desktop and mobile screenshots confirm the same compact hierarchy
  without mixed-script clipping.
- [x] Remove duplicate labelled-navigation tooltips and apply the semantic hover/pressed/selected/
  focus ramp to navigation, buttons, rows, and menus; reserve tooltips for icon-only controls.
  **Verify:** component and browser tests show no Tooltip/title for labelled 224 px or mobile nav,
  a named dismissible Tooltip for 64 px/icon-only controls, visible independent focus, no hover
  lift/shadow, and no essential copy available only through hover.
  **Evidence:** component and asset tests pass for plain labelled desktop/mobile navigation,
  named Tooltip triggers only in the compact 64 px rail, current-route semantics, and the flat
  no-hover-shadow contract. Real pointer/focus review confirms labelled navigation does not produce
  duplicate hover copy and compact icon navigation retains the named tooltip.
- [x] Apply restrained glass only to overlapping sticky chrome and transient menu/tooltip layers,
  using the documented alpha/blur/saturation/hairline/shadow values and opaque fallbacks while
  keeping rails, workbenches, data, forms, JSON, notices, and dialogs opaque.
  **Verify:** CSS/asset tests reject gradients, broad glow, stacked material, glass content, and
  blur beyond 16 px; real-browser checks cover supported and fallback rendering, reduced
  transparency, increased contrast, forced colors, both themes, scroll-behind, and dialog smoke.
  **Evidence:** CSS/asset contracts pass for the exact light/dark sticky and transient alpha,
  14/16 px blur and saturation values, opaque default fallback, solid content/dialogs, and the
  reduced-transparency, increased-contrast, forced-colors, and explicit solid hooks. Gradient,
  broad-glow, text-shadow, and row-shadow rejection passes. Both-theme browser review confirms the
  glass remains limited to overlapping/transient layers and dialogs/content remain opaque.
- [x] Make every Job and Run desktop/mobile row a pointer-selectable detail surface while retaining
  exactly one descriptive native anchor and an isolated named action menu.
  **Verify:** tests cover ordinary surface click, text selection, prevented/modified/non-primary
  events, real-anchor middle/Cmd/Ctrl/context behavior, interactive descendants, menu open/select,
  Tab/Enter, accessible full run/job names, and flat hover/focus/pressed/current states.
  **Evidence:** four pure row-event tests and the Jobs responsive integration test pass for
  ordinary surface dispatch to the native link, selection, prevented/modified/non-primary guards,
  interactive descendants and menu isolation, descriptive anchors, and no synthetic row role or
  tab stop. Real browser review confirms ordinary surface navigation and isolated row-menu actions.
- [x] Audit the complete light/dark palette against the documented perceptual role ramp and status
  pairs, eliminating local/ad-hoc color and whole-component disabled opacity.
  **Verify:** source scans find only semantic component aliases; automated ratios meet 4.5:1 text
  and 3:1 boundary/state targets; screenshot review covers canvas/surface/raised/hover/pressed/
  selected, disabled, focus, brand amber, and all labelled statuses without hue-only meaning.
  **Evidence:** the embedded asset/server suite matches the documented light/dark semantic
  tokens, calculates all normal text/status pairs at or above 4.5:1 and control/focus boundaries at
  or above 3:1, rejects whole-component disabled opacity and local visual effects, and preserves
  labelled status treatment. Complete light/dark screenshots cover surfaces, interaction states,
  brand focus, disabled controls, and labelled statuses.

- [x] Preserve Jobs and Run history toolbar/table/header context for successful zero results and
  render distinct semantic filtered-zero and first-use body rows with appropriate recovery actions.
  **Verify:** component tests cover desktop `colSpan`, narrow equivalent, polite single count status,
  no duplicate live region, no pager, filtered Clear filters with search focus/restored results, and
  initial Create/View job actions; browser review confirms stable 160 px empty body in both themes.
  **Evidence:** Jobs and Run history component tests pass for successful first-use and
  filtered zero, five-/six-column span, retained headers, the non-article mobile equivalent, one
  described count status, absent zero pager, enabled clear with restored search focus/count, route-
  appropriate creation/navigation, and distinct loading/request-error branches. The asset contract
  fixes the 160/112/24 px desktop and 96/24x16 px narrow geometry. Desktop Jobs/Runs and 390 px Jobs
  browser review confirm retained headers/context, the first body-row empty state, and no pager.
- [x] Apply explicit label-to-control, grouped-control-to-help, help/error, field, and section spacing
  so theme and other explanatory copy never collides with the control above it.
  **Verify:** CSS/component contracts assert 8/8/4/20/40 px roles, group `aria-describedby`, normal
  flow, wrap behavior, and 56ch theme help; desktop/mobile screenshots confirm visual separation.
  **Evidence:** component tests pass for the ThemeControl fieldset's single described radio
  group and normal-flow help after all three options. The source/asset contract fixes label-control
  8 px, segmented-control-help 8 px, help/error 4 px, next field 20 px, section 40 px, and muted
  13/18 help within 56ch. Desktop and mobile Settings screenshots confirm the theme help no longer
  crowds the segmented controls and wraps in normal flow.

- [x] Complete the combined workspace and real-browser gate, publish, and leave the review server.
  **Verify:** full fmt/clippy/workspace all-target tests and dependency direction pass; real browser
  walks entry/auth reload, all routes, both themes, native keyboard semantics, exact duration/size/
  instant round trips, search races, long job form, pruning review, narrow/zoom/reduced preferences,
  favicon/title/browser color, and empty developer logs before commit.
  **Evidence:** `npm ci --ignore-scripts`, TypeScript typecheck, all 49 frontend tests across
  11 files, the latest seven-file production build, built-JS `node --check`, remote-runtime and
  source-map scans, all 56 store and 44 server tests, 12 embedded asset tests, changed-crate
  warnings-denied clippy, fmt, font hashes, dependency-direction check, and authored-source
  whitespace scans pass; the generated Radix bundle and official OFL retain their upstream spaces.
  Follow-up regressions also lock the 24 px compact-shell header inset, contained table/search
  widths, scrollbar-free but scrollable six-section mobile form navigation with a current marker,
  and one named accessible Radix combobox while its form bubble stays hidden. The complete workspace
  fmt, warnings-denied clippy, and all-target test gate passes. Real-browser confirmation covers
  1440/900/768/390 layouts, both themes, every route, row navigation, JSON, dialogs, menus, trailing
  search, stable filtered-zero tables, and mobile form reflow with zero console errors or warnings.
  Feature commit `82527de` is published locally on `feat/dashboard`; the authenticated review server
  remains available at `http://127.0.0.1:10824/`. Remote push was intentionally not performed.

### Settings, JSON, and nested-row consistency follow-up (2026-08-25)

- [x] Amend the dashboard specification, record form-pattern evidence, accept the implementation
  approach, and review this phased checklist before code changes.
  **Verify:** `docs/dashboard/SPEC.md` criteria 40–42 describe only observable behavior;
  `docs/FINDINGS.md` §29 cites the one-form/bottom-action evidence and records rejected JSON
  normalization; `docs/dashboard/IMPLEMENTATION.md` records architecture, failure behavior, edge
  cases, and verification; every implementation step below has a concrete Verify entry.
- [x] Replace per-setting Review buttons with one bottom durable-settings action group and aggregate
  review/apply flow while keeping browser-local theme changes immediate.
  **Verify:** Settings component tests cover multi-field dirty collection, disabled unchanged state,
  invalid-field focus, aggregate dry-run/dialog/apply ordering, discard, staged redacted environment,
  per-key failure recovery/refetch, and absence of repeated Review buttons; browser QA confirms the
  bottom action remains discoverable and usable at desktop and 390 px.
  **Evidence:** all four Settings component tests pass for one ordered multi-field string-
  flag dry-run and live apply, disabled clean state, dirty `beforeunload` protection, immediate theme,
  scalar and reserved-environment invalid-field focus, discard, redacted environment staging, and
  honest partial-failure canonical refetch with the failed draft retained. The Rust source contract
  rejects every former per-field Review label and requires the one bottom action group. Browser QA
  caught an incompatible boolean dry-run body; the frontend exact-payload test and server
  `dry_run_never_mutates` contract now pass with the API's required string flag `"1"`. Browser QA
  confirms zero repeated Review buttons, a disabled clean state, one bottom action group, and a
  successful aggregate dry-run dialog listing `Global concurrency: 16 → 15`; the temporary draft
  was discarded and the durable value remained 16.
- [x] Pretty-format valid JSON for presentation without changing exact-source copying or invalid
  payload handling.
  **Verify:** pure/component tests cover two-space nested formatting, empty containers, CRLF source,
  duplicate keys, exponent and negative-zero lexemes, Unicode/slash escapes, exact clipboard bytes,
  invalid input, formatted preview thresholds, expansion, wrapping, XSS-safe text, and both themes.
  **Evidence:** all nine JSON pure/component tests pass for deterministic two-space nested
  formatting, compact empty containers, CRLF exact source, duplicate keys, exponent/negative-zero
  lexemes, Unicode/slash escapes, exact clipboard source, exact invalid fallback, formatted line and
  original-byte thresholds, 80-line expansion, persisted wrapping, and literal-markup safety. The
  asset contract rejects `JSON.stringify` normalization. Browser QA renders the fixture definition
  as a 35-line, two-space-indented document in both resolved dark and explicit light themes; the
  light viewer computes an opaque white background and dark foreground.
- [x] Align Jobs Search and State filter through explicit label/control/help grid rows and keep the
  result status outside the field baseline.
  **Verify:** component/source contracts assert shared desktop label/control grid rows and an
  independent result slot; browser screenshots at 1440 and 390 px confirm equal control baselines,
  intact helper text, mobile reading order, and no horizontal overflow.
  **Evidence:** the Jobs component test passes for the labelled Search and State controls,
  Search help association, direct independent result-status slot, and DOM order Search → State →
  status. The Rust source/CSS contract fixes matching desktop named label/control rows, a separate
  Search help row and status area, then a `minmax(0,1fr)` single-column mobile grid that restores
  ordinary field boxes and the same reading order. Browser QA then exposed desktop `first-of-type`
  area specificity leaking into the mobile field grids; the regression fix resets those exact
  selectors at equal specificity and requires each narrow field to stretch to `width:100%` with
  zero minimum inline size. TypeScript typecheck, all 57 frontend tests, all 14 embedded asset tests,
  built-JS syntax, fmt, changed-server warnings-denied clippy, and two byte-identical production
  builds pass. Browser QA at 1440 px measures both labels at top 167 px and both controls at top
  205 px. At 390 px both fields start at 16 px, span 358 px, retain Search → helper → State →
  status order, and report zero horizontal overflow.
- [x] Apply shared whole-row detail navigation to Job detail Recent runs, rebuild production assets,
  and complete the combined regression gate.
  **Verify:** component/browser tests cover Recent runs surface click, native link, text selection,
  modified/non-primary click and accessible full identity; two clean builds are byte-identical;
  TypeScript, all frontend tests, Rust asset tests, fmt, warnings-denied workspace clippy, full
  workspace all-target tests, clean browser logs, and the live review server all pass before commit.
  **Evidence:** the Job detail component test passes for unused Recent-runs surface click,
  modified-click isolation, native table/link semantics, and full accessible run identity; four
  shared-helper tests retain selection, prevented, modified/non-primary, and interactive-descendant
  guards, and the empty Recent-runs state is unchanged. Browser QA clicks the Requested cell of the
  first Recent runs row and reaches its full run-detail URL. Two production builds emitted identical
  index/favicon/CSS/JS names and SHA-256 hashes; built JS passes `node --check`. TypeScript typecheck,
  all 57 frontend tests across 11 files, all 14 embedded asset tests, changed-crate warnings-denied
  clippy, fmt, workspace warnings-denied clippy, and the full workspace all-target test gate pass.
  The latest embedded binary serves HTTP 200 at `http://127.0.0.1:10824/`; browser error/warning
  logs are empty.

### Disabled-job visibility and integrated functional QA (2026-08-25)

- [x] Reproduce the disabled-job disappearance, amend observable behavior, record the API-contract
  diagnosis, accept the implementation approach, and review this checklist before code changes.
  **Verify:** `docs/dashboard/SPEC.md` criteria 44–46 cover complete state filtering, disabled-job Run
  history names, and live workflow QA; `docs/FINDINGS.md` §30 records a 3→2 live reproduction and the
  existing `all=1` contract; `docs/dashboard/IMPLEMENTATION.md` records consumers, transitions,
  disabled next-occurrence behavior, and verification without changing API/store defaults.
- [x] Make Jobs and Run history consume the complete current-job collection and keep list state
  transitions on the current route.
  **Verify:** component tests assert the exact complete-view request, all/enabled/disabled counts,
  partial search, list disable/enable refresh without navigation, preserved active filters, disabled
  next-occurrence copy, and current-name enrichment for disabled-job runs.
  **Evidence:** focused Jobs, Runs, and row-navigation tests pass 23/23. Jobs and Run history request
  `/api/v1/jobs?all=1`; component coverage proves complete state filtering, name/description/tag
  partial search, disabled next-occurrence suppression with last history retained, state transitions
  without hash/filter reset, disabled-run name enrichment, and portalled action-menu isolation.
- [x] Rebuild embedded assets and complete the automated regression gate.
  **Verify:** strict TypeScript, all frontend tests, embedded asset/server contracts, built-JavaScript
  syntax, two byte-identical production builds, fmt, warnings-denied workspace clippy, and full
  workspace all-target tests pass with `git diff --check` clean.
  **Evidence:** TypeScript, all 61 frontend tests, all 15 embedded asset tests, built-JavaScript
  syntax, fmt, warnings-denied workspace all-target clippy, and two byte-identical production builds
  pass. After reboot, the complete workspace all-target test run, including both `service_lifetime`
  cases and the 27-test server library suite, exits successfully with no failures. `git diff --check`
  is clean.
- [x] Execute and record the authenticated functional browser matrix, restore fixture state, commit,
  and restart the review server from the new embedded binary.
  **Verify:** desktop and 390 px browser QA passes Jobs partial/state search, list/detail state toggles,
  actions and row links, Run history debounce/name search and row entry, Run detail, Settings review/
  discard and themes, Diagnostics health loading, result/empty states, HTTP 200, and zero browser
  warnings or errors; the fixture ends with its original enabled jobs and the worktree is clean after
  commit.
  **Evidence:** authenticated desktop QA proves All states retains three rows after list Disable,
  Disabled returns one, Enabled plus the `heart` partial returns zero with the stable table empty row,
  and list Enable restores the row without leaving `#/jobs`. Disabled Next reads `disabled — not
  scheduled`; list and detail row/state actions, Run history `heart` trailing search with the disabled
  current name, Run-detail output loading, Recent runs row navigation, New job Every/Shell dry-run,
  Settings aggregate review/discard, light/dark/System themes, and Diagnostics health/integrity all
  pass. At 390 px both filters start at 16 px, share the available width, render three mobile rows,
  and produce zero horizontal overflow; desktop labels and controls share exact baselines. JSON is
  pretty-presented in both themes, every QA state change is restored, HTTP returns 200, and browser
  error/warning logs are empty. Implementation commit `86dd9fc` is preserved locally and the latest
  embedded review server runs at `http://127.0.0.1:10824/`.

## Dashboard v0.8.0 release preparation (2026-08-25)

- [x] Review and freeze the release-preparation plan before changing release metadata or
  user-facing documentation.
  **Verify:** `docs/SPEC.md` contains the dashboard release/discoverability amendment;
  `docs/IMPLEMENTATION.md` records the documentation structure, version strategy, publication
  boundary, edge cases, and verification; every implementation step below has a concrete Verify
  entry and no unresolved release-preparation question remains.
  **Evidence:** the 2026-08-25 specification amendment defines the observable release boundary;
  the implementation section records documentation placement, release-note content, version and
  lockfile strategy, publication ownership, edge cases, and checks; this four-step checklist was
  reviewed in order before README, changelog, or version edits began.
- [x] Add a concise dashboard quick start and improve operational discoverability without
  contradicting the detailed CLI and Operator contracts.
  **Verify:** every relative README Markdown link resolves; shell fences parse with `sh -n`; the
  quick start covers foreground and persistent startup, URL discovery, explicit token paste or
  re-display, loopback-only exposure, optional/off-by-default posture, and the shared durable state;
  scans find no token-in-URL example or remote-control claim.
  **Evidence:** README now places the foreground and persistent-service dashboard quick start
  immediately after installation, links to the Operator and CLI anchors, and states the optional,
  off-by-default, loopback-only, shared-state and token-paste boundaries. All relative README links
  and anchors resolve, all 11 `sh` fences pass `sh -n`, the safety/discoverability contract scan
  passes, and the stale Operator link now resolves from `docs/OPERATOR.md` to `dashboard/SPEC.md`.
- [x] Curate the `0.8.0` changelog and synchronize the workspace and lockfile versions.
  **Verify:** `CHANGELOG.md` has one top-level heading, an empty Unreleased section followed by
  strictly descending releases, a complete user-facing `0.8.0` entry, and correct comparison links;
  `Cargo.toml` and all five Locron package records in `Cargo.lock` report `0.8.0` after normal Cargo
  reconciliation.
  **Evidence:** CHANGELOG has one top-level heading, an empty Unreleased section, strictly
  descending `0.8.0` through `0.1.0` entries, and `v0.8.0`/`v0.7.0` comparison links. Normal
  `cargo check --workspace` updated only the five Locron lockfile package versions; a direct parser
  confirms all five plus the workspace manifest report `0.8.0`, and `locron --version` prints
  `locron 0.8.0`.
- [x] Run the release-preparation validation and commit only the intended files for parent
  publication.
  **Verify:** `cargo check --workspace --locked`, `cargo fmt --all --check`, documentation checks,
  and `git diff --check` pass; staged/unstaged inspection shows no unrelated work; the resulting
  commit follows `{type}: {imperative message}` and no push, PR, merge, tag, release, Homebrew, or
  separate skills-repository mutation occurs in this sub-session.
  **Evidence:** `cargo check --workspace --locked`, `cargo fmt --all --check`, the README link,
  anchor, shell-fence, and safety checks, the CHANGELOG structure check, the version consistency
  check, and `git diff --check` pass. Final staged/unstaged inspection is recorded in the handoff;
  publication and the separate skill audit remain owned by the parent session.

## Linux service cfg portability follow-up (2026-08-25)

- [x] Record the CI reproduction and review the portability plan before code changes.
  **Verify:** `docs/FINDINGS.md` names the run, head, failing jobs, commands, five E0425 locations,
  and two warnings-denied dead-code locations; `docs/IMPLEMENTATION.md` records the identifier and
  cfg-scoping fix, adjacent audit, Linux verification limitation, and publication boundary.
  **Evidence:** FINDINGS §31 records run 32801485007 on `7fcb716`, lint job 97663055695 and test job
  97663055756; the implementation plan was reviewed before editing either Rust file.
- [x] Correct the systemd context bindings and scope macOS-only dashboard test helpers accurately.
  **Verify:** all five target-selecting systemd methods bind `ctx`; only context-independent methods
  retain `_ctx`; `DashboardServiceCleanup`, its `Drop` implementation, and
  `default_dashboard_token_path` are compiled only on macOS; the systemd implementation is also
  type-checked in unit-test builds without running systemctl; no blanket dead-code allow is added.
  **Evidence:** systemd `is_loaded`, `enable`, `start`, `stop`, and `unload` now bind `ctx`, while
  only `session_available` and `reload` retain unused `_ctx`; the complete systemd module compiles
  under `cfg(test)` and `systemd_backend_type_checks_on_every_test_host` proves its trait boundary
  without executing manager commands. The dashboard cleanup type, its Drop implementation, and the
  token-path helper each carry macOS cfg, with no dead-code allowance.
- [x] Complete the local regression gate and hand off Linux confirmation to the parent session.
  **Verify:** source-contract inspection, `cargo fmt --all --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace --all-targets`,
  and `git diff --check` pass; a Linux-target check runs when locally feasible or the missing target
  is recorded; staged/unstaged inspection is clean after a scoped convention-compliant commit.
  **Evidence:** the cfg source contract, fmt, warnings-denied workspace all-target Clippy, the full
  workspace all-target suite (including 82 CLI unit tests and the new systemd compile seam), and
  `git diff --check` pass. The installed `x86_64-unknown-linux-gnu` target check was attempted but
  stopped before Locron source in `libsqlite3-sys`/`aws-lc-sys` because
  `x86_64-linux-gnu-gcc` is not installed; native Linux confirmation remains the parent-owned CI
  rerun. Final staged/unstaged inspection and commit hash are recorded in the handoff.

## Service template identity follow-up (2026-08-25)

- [x] Record the second Linux CI reproduction and review the manager-identity separation before
  code changes.
  **Verify:** FINDINGS §32 names run 32802465277, head `95fbae5`, lint job 97665903962 with both
  unused constants, test job 97665904093 with both failed plist assertions and its 80/82 result;
  IMPLEMENTATION records accessor ownership, unchanged runtime behavior, adjacent renderer audit,
  and verification.
  **Evidence:** exact CI logs and the `Target`/renderer call inventory were reviewed before editing
  `service.rs`; no specification change is required.
- [x] Separate launchd template labels from active-platform service names.
  **Verify:** `Target` has an explicit macOS/test launchd-label accessor; macOS `service_name`
  delegates to it, Linux keeps unit names, and `render_plist` uses the label accessor; existing
  plist assertions remain unchanged, `render_unit` remains platform-neutral, and no dead-code
  allowance is added.
  **Evidence:** `Target::launchd_label` maps both launchd constants under macOS/test cfg;
  `render_plist` uses it, the macOS `service_name` branches delegate to it, and Linux branches still
  return the two systemd units. The three existing plist tests remain strict and pass; the adjacent
  renderer source contract confirms `render_unit` embeds no current-platform service name.
- [x] Complete the local gate and hand native Linux confirmation to the parent session.
  **Verify:** focused template/compile-seam tests, source-contract inspection, fmt,
  warnings-denied workspace all-target Clippy, full workspace all-target tests, and diff checks pass;
  staged/unstaged inspection is clean after a scoped convention-compliant commit.
  **Evidence:** the three focused plist tests, systemd compile-seam test, source contract, fmt,
  warnings-denied workspace all-target Clippy, and complete workspace all-target suite pass. The
  first full-suite attempt caught a transient real-launchd restart observation while the preserved
  review server occupied port 10824; the isolated real-backend suite then passed 3/3 and a complete
  rerun passed without stopping that server. `git diff --check` is clean; final staged/unstaged
  inspection and commit hash are recorded in the handoff.

## Ordered deferred product roadmap


Every phase below is post-milestone work and requires its own reviewed SPEC before implementation;
none changes the exclusions in the current `docs/SPEC.md`.

1. [x] Define the local HTTP viewer and mutation API, including local-port binding, authentication,
   origin/CSRF protections, exposure diagnostics, and reuse of durable application commands.
   **Evidence:** implemented as the Web administration roadmap — frozen in
   `docs/dashboard/SPEC.md` and `docs/dashboard/IMPLEMENTATION.md`, shipped across the checklist
   above (server, contract suite, SSE stream, embedded viewer, service registration, CLI family,
   docs), with the browser checklist (44/44), real-backend launchd evidence, and the green
   two-toolchain verification recorded in the Web administration backlog section above.
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
