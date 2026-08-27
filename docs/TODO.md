# locron Milestone 1 TODO

This checklist tracks active and recently completed work against `docs/SPEC.md`, the durable
structure in `docs/ARCHITECTURE.md`, and the accepted approaches in `docs/IMPLEMENTATION.md`.
Deferred ideas that are not active commitments live in `docs/BACKLOG.md`.

If a planned implementation decision changes, update and review `docs/IMPLEMENTATION.md` and this checklist before changing code. Update `docs/ARCHITECTURE.md` first for a durable structure/invariant change and `docs/SPEC.md` first for an observable behavior/scope change.
Completed historical sections live in `docs/TODO-archive.md` (moved 2026-08-24); this file keeps open work and recent backlogs.

## CI toolchain and cache optimization (2026-08-26)

- [x] Replace the two `clippy::map_unwrap_or` violations with the direct `Result::is_ok_and`
  predicate without changing service detection behavior.
  **Verify:** focused source inspection finds no equivalent map/unwrap pattern and Rust 1.98.0
  warnings-denied all-target Clippy passes on the local macOS host.
  **Evidence:** focused repository search found no remaining command-output
  `map(...).unwrap_or(false)` expression, and Rust 1.98.0 all-target Clippy passed with warnings
  denied on macOS arm64.
- [x] Separate the pinned development/lint toolchain from the package MSRV and document the policy.
  **Verify:** `rust-toolchain.toml` selects exact Rust 1.98.0, `Cargo.toml` retains `rust-version =
  "1.94"`, the two lint matrix entries select 1.98.0, stable compatibility tests remain present,
  actionlint passes, and contributor commands describe the same split.
  **Evidence:** local inspection confirmed exact Rust 1.98.0 for development and both lint jobs,
  Rust 1.94 for the package contract and MSRV job, four floating-stable compatibility jobs, and
  matching contributor commands; actionlint passed.
- [ ] Prove the correction locally and on hosted runners without adding CI fan-out.
  **Verify:** formatting and Clippy pass on 1.98.0, all workspace all-target tests pass on 1.94.0,
  `git diff --check` passes, and one nine-job hosted run succeeds with the expected three toolchain
  roles; record the run and timing evidence before handoff.
  **Evidence:** local Rust 1.98.0 formatting and warnings-denied Clippy passed, all 441 Rust 1.94
  workspace all-target tests passed, and `git diff --check` passed. Hosted verification remains.

- [x] Correct toolchain selection and reduce the test/lint matrix from fourteen jobs to nine while
  preserving stable coverage on all four supported hosts, lint on both operating systems,
  and one Linux x86_64 Rust 1.94 MSRV gate.
  **Verify:** workflow inspection finds an explicit five-entry test matrix, two-entry lint matrix,
  job-level `RUSTUP_TOOLCHAIN`, and pre-command active-versus-selected compiler assertions in both
  jobs; hosted stable logs report stable Rust rather than 1.94.
  **Evidence:** local workflow inspection and an executable YAML contract confirm the five-entry
  test matrix, two-entry lint matrix, job-level toolchain selection, and compiler-path assertions;
  the local stable assertion selected Rust 1.98.0 instead of the repository's 1.94 override. Hosted
  run 32983148063 likewise recorded Rust 1.98.0 on all four stable test hosts and both stable lint
  hosts while the MSRV job recorded Rust 1.94.0.
- [ ] Restrict Rust cache creation to the default branch without disabling useful dependency and
  target restoration.
  **Verify:** every CI Rust cache step uses a main-only save condition, release-tag cache steps use
  restore-only mode, workflow YAML/actionlint pass, and pull-request/release execution can restore
  without creating branch- or tag-local entries.
  **Evidence:** the three CI cache steps use `save-if: github.ref == 'refs/heads/main'`. Inventory
  also identifies twelve release-tag caches totaling 2,636,106,740 bytes across the latest three
  tags, so release cache writes are included in this correction; hosted restore-only evidence
  remains open.
- [ ] Complete local and hosted verification and record measured job count, wall time, runner time,
  toolchain versions, and cache behavior.
  **Verify:** Rust 1.94 formatting, warnings-denied all-target Clippy, workspace all-target tests,
  `git diff --check`, and the nine-job hosted CI run pass; evidence is added before handoff.
  **Evidence:** local Rust 1.94 formatting and warnings-denied all-target Clippy pass, as do all 441
  workspace all-target tests across 21 suites. Test jobs no longer install unused rustfmt/Clippy
  components. Hosted run 32983148063 created exactly nine jobs and passed every toolchain assertion,
  but attempts 1 and 2 were both externally cancelled by `@WhiteKiwi` after about 65 seconds before
  the test, lint, source-package, timing, and cache-save evidence could complete.

## Deterministic dashboard port-policy verification (2026-08-25)

- [x] Replace global-default-port conflict tests with test-owned server policy contracts and pure
  CLI policy-selection coverage while preserving product behavior.
  **Verify:** focused tests prove owned-port foreground fallback, fixed conflict failure, the
  independent dual-stack partial-bind contract, and all three CLI policy branches; no test binds or
  assumes exclusive ownership of port 10824.
  **Evidence:** `locron-server` tests retain an OS-assigned IPv4 listener through both the
  foreground fallback and fixed `AddrInUse` assertions, while the existing occupied-IPv4/IPv6
  partial-bind contract still passes. The pure CLI selector test covers ordinary foreground,
  explicit-port, and registered-service forms. The dashboard integration suite retains its
  explicit random-port startup, serving, and strict error contracts, and source inventory finds no
  fixed-default-port occupancy helper or bind.
- [x] Stress the corrected dashboard/server seams under parallel execution.
  **Verify:** the dashboard integration binary and focused server tests each pass at least 20
  consecutive parallel runs with unchanged assertions and no retry-on-failure wrapper.
  **Evidence:** the complete dashboard integration binary passed 20 consecutive runs, and the
  complete `locron-server` library suite passed 20 consecutive runs; Rust's ordinary parallel test
  execution remained enabled and each command failed immediately on an unsuccessful run.
- [x] Complete the repository verification gate and inspect the final scoped diff.
  **Verify:** `cargo fmt --all --check`, warnings-denied workspace all-target Clippy, the complete
  workspace all-target suite, focused policy tests, and `git diff --check` pass; staged and
  unstaged changes contain only this correction and its planning evidence.
  **Evidence:** formatting and warnings-denied workspace all-target Clippy pass; all 441 workspace
  all-target tests pass across 21 suites, including the focused policy and retained partial-bind
  contracts; `git diff --check` passes. The final six-file unstaged diff contains only the three
  test files and three planning/evidence documents for this correction, with no staged changes.

## README information architecture refresh (2026-08-25)

- [x] Reorganize the README opening and practical workflow so the first screen identifies locron as
  a local, explainable scheduler for developers and agents on macOS and Linux, while preserving the
  accepted non-marketing voice and shipped command syntax.
  **Verify:** compare the rendered opening and section order with the frozen product positioning;
  run every changed command example against CLI help or an isolated state directory.
  **Evidence:** the opening now identifies the scheduler, audience, platforms, and durable
  explainability model before the first heading, then demonstrates add, preview, run, history, and
  explanation before installation. The complete opening workflow ran successfully against the
  current debug binary in an isolated state directory; all added target and policy examples passed
  dry-run validation, and every referenced locron command resolved through current CLI help.
- [x] Add concise local-failure, human/agent feedback-loop, scheduler-scope, and architecture
  explanations without overstating sleep detection, exactly-once execution, MCP, or OS-service
  integration.
  **Verify:** trace each technical claim to the specification, architecture, CLI/MCP contracts, or
  implementation tests; confirm cron, launchd, and systemd wording describes scope rather than a
  dishonest feature comparison.
  **Evidence:** the local-failure table and uncertainty warning trace to the accepted schedule,
  recovery, supervision, retention, and SQLite invariants; the interface inventory matches the CLI,
  dashboard, and MCP contracts. The scope section distinguishes cron's scheduling primitive and
  launchd/systemd service-management roles without a feature matrix, and the README explicitly
  rejects inferred sleep detection and exactly-once external effects.
- [x] Preserve and verify installation, dashboard, target/schedule, documentation, contribution,
  and license guidance after the reorganization.
  **Verify:** run the repository's Markdown link/reference checks, inspect heading and code-fence
  structure, and require `git diff --check` to pass on the final documentation-only diff.
  **Evidence:** all 21 local Markdown links and their fragments resolve, the banner asset exists,
  and the 16-heading/34-fence structure is balanced. Preserved service, dashboard, schedule, and
  target commands passed help or isolated dry-run checks; `git diff --check` passes.

## v0.9.2 patch release (2026-08-25)

- [x] Prepare the lockstep v0.9.2 workspace version and curated changelog entry for the completed
  lifecycle human-output and stale dashboard-route fixes.
  **Verify:** all five workspace packages and exact internal requirements report 0.9.2, the lockfile
  agrees, and the changelog section and comparison links identify v0.9.2.
  **Evidence:** Cargo metadata and the refreshed lockfile report all five packages at 0.9.2; the
  four workspace internal requirements are exact `=0.9.2`; the curated changelog and comparison
  links identify v0.9.2.
- [x] Run the release-version contract and focused local release checks on the exact release tree.
  **Verify:** formatting, warnings-denied Clippy, workspace tests, frontend tests/typecheck/build,
  release-version agreement, workspace publication dry-run, and `git diff --check` pass.
  **Evidence:** Rust 1.94 formatting and warnings-denied all-target Clippy pass; all 441 workspace
  tests and 65 frontend tests pass; TypeScript typecheck and the production build pass;
  `check-release-version.sh 0.9.2`, workspace package verification, the five-package publication
  dry run, and diff checks pass.
- [x] Commit and push the reviewed release revision, then create and push immutable annotated tag
  `v0.9.2`.
  **Verify:** `main` and `origin/main` point at the release commit, the tag resolves to that same
  commit, and the tag-triggered release workflow starts.
  **Evidence:** release commit `e052877` was pushed to `main`; annotated tag `v0.9.2` resolves to
  that exact commit and started release run
  [32822950430](https://github.com/WhiteKiwi/locron/actions/runs/32822950430).
- [x] Confirm publication and update this machine's managed installations and services.
  **Verify:** the release workflow, five crates, GitHub Release, and Homebrew formula report 0.9.2;
  the Homebrew binary reports 0.9.2; the daemon and standalone dashboard service are restarted on
  the new binary; and the dashboard returns HTTP 200.
  **Evidence:** release run 32822950430 passed all four platform builds, OIDC workspace publication,
  exact registry installation, GitHub publication, and Homebrew update; all five crates inventory as
  0.9.2 and the public release carries all ten expected assets. Main CI run
  [32822947779](https://github.com/WhiteKiwi/locron/actions/runs/32822947779) passed all 14 jobs and
  audit run [32822947860](https://github.com/WhiteKiwi/locron/actions/runs/32822947860) passed both
  policy jobs. Homebrew and standalone binaries both report 0.9.2; the Homebrew daemon and standalone
  dashboard LaunchAgents run the expected managed paths; dashboard status reports PID 82476 with an
  owner-only token, HTTP returns 200, and the served bundle is `app-BsbUp3HE.js`.

## Dashboard lifecycle human output and stale detail recovery (2026-08-25)

- [x] Replace raw serialized default output across daemon service, dashboard lifecycle, and
  successful self-update commands with labeled human reports while preserving every `--json`
  envelope and token secrecy boundary.
  **Verify:** focused integration tests cover service install/uninstall/status, dashboard
  enable/disable/status/token, and successful self-update in both applicable output modes; human
  stdout is not parseable as a JSON object, token output is labeled and copyable, and only the
  token command contains the token value.
  **Evidence:** the service/dashboard/self-update integration suites pass 45 tests; focused human
  contracts cover all seven lifecycle commands plus successful self-update, preserve the existing
  JSON assertions, reject JSON-object stdout in human mode, and verify dashboard status does not
  contain token material. The shared renderer no longer has a human pretty-JSON fallback; export's
  portable JSON document remains an explicit exception.
- [x] Render complete job and run not-found states for stale authenticated detail routes while
  preserving valid deep links and ordinary non-404 failure feedback.
  **Verify:** frontend component tests prove each 404 names the missing resource category, explains
  the stale/removal case, links directly to its collection, hides the raw API identifier-only
  feedback, and leaves valid and non-404 branches unchanged.
  **Evidence:** job and run component contracts cover 404 recovery and non-404 feedback alongside
  the existing valid-detail tests. Both missing states name their resource, explain removal/stale
  links, route directly to `#/jobs` or `#/runs`, and omit the raw API identifier message.
- [x] Run the focused and repository-level verification gates and record their evidence here.
  **Verify:** frontend tests and production build, service/dashboard/self-update integration tests,
  workspace formatting, warnings-denied all-target Clippy, relevant workspace tests, and
  `git diff --check` all pass on the intended tree.
  **Evidence:** all 65 frontend tests, TypeScript typecheck, and the Vite production build pass;
  Rust 1.94 workspace formatting and warnings-denied all-target Clippy pass; all 441 workspace
  all-target tests pass across 21 suites. The final diff and generated frontend bundle checks pass.

## v0.9.1 patch release (2026-08-25)

- [x] Prepare the lockstep v0.9.1 workspace version and curated changelog entry for the completed
  human `history` terminal-width improvement.
  **Verify:** all five workspace packages and exact internal requirements report 0.9.1, the lockfile
  agrees, and the changelog comparison links and release section name v0.9.1.
  **Evidence:** `cargo check --workspace` resolved all five packages at 0.9.1 and refreshed the
  lockfile; the root manifest carries four exact `=0.9.1` internal requirements and the curated
  changelog section and comparison links identify v0.9.1.
- [x] Run the release-version contract and focused local release checks on the exact release tree.
  **Verify:** formatting, release-version agreement, history-table tests, package checks, and
  `git diff --check` pass before the release commit is created.
  **Evidence:** workspace formatting, `check-release-version.sh 0.9.1`, both focused history-table
  test paths, clean-tree workspace package verification, workspace publication dry-run, and diff
  checks pass; the dry run packaged and verified all five 0.9.1 crates without uploading.
- [x] Commit and push the reviewed release revision, then create and push immutable annotated tag
  `v0.9.1`.
  **Verify:** `main` and `origin/main` point at the release commit, the tag resolves to that same
  commit, and the tag-triggered release workflow starts.
  **Evidence:** release commit `157bd06` was pushed to `main`; annotated tag `v0.9.1` resolves to
  that commit and started release run
  [32819209936](https://github.com/WhiteKiwi/locron/actions/runs/32819209936).
- [x] Confirm the complete automated publication across crates.io, GitHub Release, and Homebrew.
  **Verify:** the release workflow is green; all five crates report 0.9.1; the GitHub Release has
  archives, Linux packages, checksums, and installer; Homebrew stable resolves to 0.9.1.
  **Evidence:** release run 32819209936 passed all four platform builds, OIDC workspace publication,
  registry installation, GitHub publication, and Homebrew update; independent inventory reports all
  five crates at 0.9.1, the public release carries all ten expected assets, and refreshed Homebrew
  metadata resolves `whitekiwi/tap/locron` stable to 0.9.1. Main CI run
  [32819207261](https://github.com/WhiteKiwi/locron/actions/runs/32819207261) and audit run
  [32819207292](https://github.com/WhiteKiwi/locron/actions/runs/32819207292) also passed.

## crates.io source installation and trusted publication (2026-08-25)

Authorized by the 2026-08-25 installation-channel amendment in `docs/SPEC.md`, researched in
`docs/FINDINGS.md` §34, and planned in `docs/IMPLEMENTATION.md` “crates.io source installation and
trusted publication”. Version 0.9.0 completed the one-time registry bootstrap and the all-present
branch of the release path, with steady-state OIDC publication configured for later versions.

- [x] Complete and review the product contract, ecosystem research, accepted package graph,
  installation-ownership boundary, CI/CD design, and this verified checklist before implementation.
  **Verify:** all four planning layers name `cargo install --locked locron`, exact internal registry
  versions, trusted publication, the standalone receipt boundary, first-release bootstrap, and
  partial-publication handling; no unresolved implementation decision remains.
- [x] Rename the user-facing Cargo package to `locron`, centralize the four internal path-plus-exact
  version dependencies, add complete crates.io metadata/publication restrictions to all five
  packages, and update every package-name consumer without moving the source directory or adding a
  binary.
  **Verify:** Cargo metadata reports exactly five publishable packages and one `locron` binary;
  dependency inspection shows the accepted DAG and exact registry fallbacks; package manifests and
  the lockfile share one version; all prior package-targeted commands resolve under the new name.
  **Evidence:** Rust 1.94 metadata reports five crates.io-only packages at 0.8.0 and exactly one
  binary target, `locron`; normalized package manifests contain the eight accepted DAG edges backed
  by the four centralized `=0.8.0` requirements. The release-version gate enumerates every normal
  internal edge and rejects a stale exact version, caret, range, missing edge, or extra edge; its
  fixtures, dependency-direction check, and renamed package-targeted build/test commands pass.
- [x] Add the standalone installation receipt and require it for `locron self-update`, with
  Homebrew-, Cargo-, older-installer-, and generic-channel guidance while keeping daemon/dashboard
  registration available to Cargo installations.
  **Verify:** installer fixtures prove atomic receipt creation for default and custom destinations;
  self-update tests cover valid, absent, malformed, and mismatched receipts plus the existing
  Homebrew marker; service/dashboard tests prove receipt absence alone does not block registration.
  **Evidence:** 20 focused installer/self-update integration tests pass, covering the exact
  owner-only receipt at default/custom destinations, success, missing/malformed/version-mismatched/
  symlink receipts, Homebrew precedence, and pre-network refusal; the full service/dashboard suite
  remains green without a receipt requirement on registration.
- [x] Add source-package validation to push/PR CI and a least-privilege, protected-environment
  crates.io publication gate to the tag workflow using the official OIDC action, exact-version
  preflight inventory, native ordered workspace publication, idempotent all-present handling,
  partial-publication refusal, and registry-install verification before GitHub Release/Homebrew.
  **Verify:** workflow YAML and action lint pass; fixture/static checks cover none/all/partial
  inventories and version mismatches; permissions are job-scoped (`contents: read`, `id-token:
  write`) only on crates publication and `contents: write` only on GitHub Release; no permanent
  crates.io token or custom sleep/order loop exists.
  **Evidence:** YAML parsing and actionlint pass; deterministic scripts cover none/all/partial
  registry inventories, release-version mismatch, and stale/non-exact internal requirements;
  static workflow inspection confirms OIDC is
  conditional on `none`, partial fails, all skips, and job-scoped permissions contain no token,
  custom ordering, sleep, `--allow-dirty`, or `--no-verify`.
- [x] Update README, INSTALL, RELEASE, CLI/operator guidance, architecture/package naming, release
  checks, and usage measurement wording for the Cargo channel, source-build requirement,
  update/removal commands, service behavior, standalone receipt migration, one-time bootstrap, and
  recovery from a partial registry publication.
  **Verify:** every documented command matches CLI/Cargo syntax, links resolve, the package/channel
  order is consistent, and no document says a Cargo or manually copied binary may self-update.
  **Evidence:** command/reference searches and the local Markdown-link checker pass across README,
  installation, release, CLI, operator, architecture, status, acceptance, dashboard/MCP, and usage
  wording; installer/Homebrew precede the Cargo source channel and only receipt-bearing standalone
  installs claim built-in self-update.
- [x] Compact the live checklist by moving fully completed sections verbatim to
  `docs/TODO-archive.md`, archiving completed evidence from mixed sections, and retaining every open
  item with its context and verification method in this file.
  **Verify:** the before/after checkbox inventory accounts for every item; `docs/TODO.md` contains
  only active work and open follow-ups plus this current section; completed evidence remains
  searchable in the archive; BACKLOG ideas remain separate.
  **Evidence:** every fully completed top-level section moved to `docs/TODO-archive.md`; the four
  completed Usage, five completed terminal-width (including the objectively complete stale planning
  checkbox), and two completed process-group items moved while their four open follow-ups remain
  here with Verify text. `docs/BACKLOG.md` is unchanged by compaction.
- [x] Run the complete local gate and source-package rehearsal without uploading to crates.io.
  **Verify:** fmt, warnings-denied workspace Clippy, all workspace targets, dependency direction,
  shell syntax/shellcheck, workflow lint, package file inspection, `cargo package --workspace
  --locked`, `cargo publish --workspace --dry-run --locked`, focused install-ownership contracts,
  Markdown/reference checks, and `git diff --check` all pass on the intended tree.
  **Evidence:** Rust 1.94 fmt, warnings-denied all-target Clippy, 438 all-target tests, dependency
  direction, shell syntax/shellcheck, deterministic release/inventory fixtures, YAML/actionlint,
  local Markdown links, and diff checks pass. Clean-copy workspace package and publish dry-runs
  verify all five crates; each archive carries README plus both licenses, the server carries the
  committed frontend dist, normalized internal requirements are exact, and no upload occurred.
- [x] At the first crates.io-backed release, bootstrap all five packages manually from the exact
  clean release commit with a narrow temporary token, revoke it, configure each trusted publisher
  for the `crates-io` environment, then push the immutable tag and confirm the OIDC/all-present
  release path plus `cargo install --locked locron`.
  **Verify:** all five exact versions are visible, the temporary token is revoked, trusted-publisher
  bindings are recorded, the tag workflow and downstream GitHub/Homebrew publication are green, and
  a temporary-root registry install reports the tagged version.
  **Evidence:** all five packages are visible at 0.9.0; the bootstrap token was revoked locally and
  on crates.io; all five packages bind `WhiteKiwi/locron`, `release.yml`, and the `crates-io`
  environment with trusted publishing required for new versions. Release run
  [32811493858](https://github.com/WhiteKiwi/locron/actions/runs/32811493858) passed the all-present
  publication gate, exact-version Cargo installation, GitHub Release publication, and Homebrew
  update.
- [x] Fix the hosted source-package archive assertion to inspect the already materialized server
  package listing instead of piping `tar -tf` into `grep -q` under `pipefail`.
  **Verify:** workflow syntax/static checks pass locally and the assertion reads the saved server
  listing without a pipeline.
  **Evidence:** YAML parsing, actionlint, a package-archive fixture, the static source-package
  contract, and `git diff --check` pass on the corrective tree.
- [x] Confirm the complete hosted CI workflow on the corrective commit reports the source-package
  job and every other job green.
  **Verify:** record the successful corrective workflow run; failed run
  [32811491695](https://github.com/WhiteKiwi/locron/actions/runs/32811491695) remains the evidence that
  motivated the deterministic listing-file check.
  **Evidence:** corrective run
  [32812563494](https://github.com/WhiteKiwi/locron/actions/runs/32812563494) passed all 14 jobs,
  including package/archive inspection, workspace publication dry-run, installer checks, all lint
  jobs, and the eight platform/toolchain test jobs.
