# locron Milestone 1 TODO

This checklist tracks active and recently completed work against `docs/SPEC.md`, the durable
structure in `docs/ARCHITECTURE.md`, and the accepted approaches in `docs/IMPLEMENTATION.md`.
Deferred ideas that are not active commitments live in `docs/BACKLOG.md`.

If a planned implementation decision changes, update and review `docs/IMPLEMENTATION.md` and this checklist before changing code. Update `docs/ARCHITECTURE.md` first for a durable structure/invariant change and `docs/SPEC.md` first for an observable behavior/scope change.
Completed historical sections live in `docs/TODO-archive.md` (moved 2026-08-24); this file keeps open work and recent backlogs.

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
- [ ] Commit and push the reviewed release revision, then create and push immutable annotated tag
  `v0.9.1`.
  **Verify:** `main` and `origin/main` point at the release commit, the tag resolves to that same
  commit, and the tag-triggered release workflow starts.
- [ ] Confirm the complete automated publication across crates.io, GitHub Release, and Homebrew.
  **Verify:** the release workflow is green; all five crates report 0.9.1; the GitHub Release has
  archives, Linux packages, checksums, and installer; Homebrew stable resolves to 0.9.1.

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
