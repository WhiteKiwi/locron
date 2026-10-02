# locron Release and CI/CD Policy

## Purpose

This document defines the official versioning, release, CI/CD, packaging, and distribution policies for `locron`. It establishes the operational contracts for building, verifying, signing, publishing, and maintaining release artifacts across supported platforms.

---

## Planned Windows distribution gates

The Windows support milestone adds native x64/ARM64 ZIPs starting at `v0.10.0`. Earlier releases
retain their exact historical Unix asset inventories. A Windows release adds exactly the two
`*-pc-windows-msvc.zip` archives, `install.ps1` and `uninstall.ps1`; the ZIP contains its exact
version/target directory with `locron.exe`, `README.md`, `LICENSE-MIT` and `LICENSE-APACHE`.
`SHA256SUMS.txt` covers the final payload archives/packages with bare filenames. Immutable
publication additionally checks the installer asset digests; reruns never replace changed bytes.

The first Windows release is unsigned. Windows signing remains a deferred milestone; existing
macOS signing/notarization remains required.
Windows release builds use Rust 1.94 and explicit native MSVC targets with `crt-static`; PR CI
verifies native compiler identity, PE architecture, no certificate table, normal/delayed system
DLL imports and `--version` with system-only PATH. Native package verification is retained as an
artifact. Clean Windows 11 standard-user install/run/update/uninstall gates are still required;
hosted images alone cannot prove absence of separately installed redistributables.
Executable-update acceptance also covers the exact-handle fs_at deletion and exclusive CreateNew
adapter, all mapped holders, competing entries and every durable backup/delete/create/write/receipt
phase on both architectures. It must prove verified rollback or an explicit disabled-task refusal
with preserved backups after interruption, including ambiguous deletion errors and an unrecorded
fresh leaf. A successful handoff alone never satisfies the confirmed-update gate.

Maintainers generate proposed WinGet manifests from final ZIPs and their complete published
checksum inventory, then validate them on Windows:

```powershell
python -B scripts/render-winget-manifest.py v0.10.0 final-release-inputs winget-manifests --validate
```

This creates three reviewable schema 1.12 manifests and performs no package install or submission.
The identifier `WhiteKiwi.locron` remains proposed until namespace availability and community
acceptance are verified. Final canonical HTTPS URLs, ZIP hashes, native architectures, minimum
Windows 11 version and relative executable paths must agree. WinGet portable manifests cannot
enforce user scope; the channel procedure supplies `--scope user`. Package ownership and the
task maintenance procedure must pass native upgrade/removal acceptance before channel promotion.
The existing install.ps1 asset supplies -Maintenance Prepare|Complete|Remove with exact package
executable paths and a durable operation UUID. Prepare confirms quiescence and keeps the recorded
tasks disabled; Complete validates current SID/package/source/path/architecture/version/hash
before restoring prior enabled states. Interrupted or mismatched operations fail with recovery
instructions and cannot reactivate an unverified executable. Remove touches only owned exact
registrations. This does not add a fifteenth release asset or manifest lifecycle hook.
Signing is a separate future milestone and must not be described as authentication for these ZIPs.

## 1. Versioning Policy

`locron` adheres strictly to **Semantic Versioning 2.0.0** (`MAJOR.MINOR.PATCH`):

- **`MAJOR` (x.0.0)**: Incompatible API or CLI breaking changes, breaking durable storage migrations that cannot be auto-migrated, or breaking wire/protocol changes.
- **`MINOR` (0.y.0 / x.y.0)**: Backward-compatible new features, commands, configuration options, or additive schema migrations. During pre-1.0 (`0.y.z`), breaking changes bump `MINOR`.
- **`PATCH` (0.y.z / x.y.z)**: Backward-compatible bug fixes, performance improvements, internal refactoring, or documentation updates.

### Workspace Lockstep Versioning
All workspace packages (`locron-core`, `locron-store`, `locron-engine`, `locron-server`, `locron`)
share the single unified version defined in the workspace root `Cargo.toml`. The four exact
internal dependency requirements must change with it. Independent versioning is forbidden.

### Git Tag Convention
- Release tags MUST follow the exact format `v{MAJOR}.{MINOR}.{PATCH}` (e.g. `v0.1.0`).
- Pre-release tags (if any) MUST use `v{MAJOR}.{MINOR}.{PATCH}-{alpha|beta|rc}.{N}` (e.g. `v0.1.0-rc.1`).
- Release tags are **immutable**. Once pushed and published, a tag must never be deleted, moved, or overwritten.

---

## 2. Supported Release Platforms and Artifacts

### Official Target Matrix
The release pipeline builds and distributes standalone, statically linked (or minimal libc linked) binary archives for the following official platforms:

| Platform / Architecture | Target Triple | Archive Name |
|---|---|---|
| **macOS Apple Silicon** (ARM64) | `aarch64-apple-darwin` | `locron-v{version}-aarch64-apple-darwin.tar.gz` |
| **macOS Intel** (x86_64) | `x86_64-apple-darwin` | `locron-v{version}-x86_64-apple-darwin.tar.gz` |
| **Linux x86_64** (glibc) | `x86_64-unknown-linux-gnu` | `locron-v{version}-x86_64-unknown-linux-gnu.tar.gz` |
| **Linux ARM64** (glibc) | `aarch64-unknown-linux-gnu` | `locron-v{version}-aarch64-unknown-linux-gnu.tar.gz` |

### Archive Structure
Each release `.tar.gz` archive MUST contain:
```text
locron-v{version}-{target}/
├── locron          # Stripped release binary (mode 0755)
├── README.md       # Repository README
├── LICENSE-MIT     # MIT License file
└── LICENSE-APACHE  # Apache-2.0 License file
```

### Checksums and Integrity
- Every release MUST generate a single `SHA256SUMS.txt` file containing the SHA-256 hashes of all release archive assets.
- Verification command: `sha256sum -c SHA256SUMS.txt` (Linux) or `shasum -a 256 -c SHA256SUMS.txt` (macOS).

### Install Script Asset
- Every release MUST also publish the repository's `install.sh` (the root-level POSIX sh installer) as a release asset. It is fetched at `https://github.com/WhiteKiwi/locron/releases/latest/download/install.sh`, and the release workflow attaches the checked-out `install.sh` to the release. A short URL, `https://locron.whitekiwi.link/install.sh`, 302-redirects to that asset (a CloudFront viewer-request function in front of a dummy origin — no hosted script copy), so the short URL always serves the release-consistent script; the GitHub URL remains the canonical one-liner in release documentation.
- The installer registers the installed binary as a login service (`locron service install`) after the atomic replace unless `LOCRON_NO_SERVICE=1` is set. Registration is best-effort: a failure warns and leaves the installation successful.
- The installer downloads the platform archive through the same static `releases/latest/download/` redirects, verifies it against the release `SHA256SUMS.txt`, and installs it; it supports `LOCRON_VERSION` pinning and `LOCRON_INSTALL_DIR` overrides.

---

## 3. CI/CD Workflow Architecture

The repository employs three automated GitHub Actions workflows:

### A. Validation CI (`.github/workflows/ci.yml`)
- **Trigger**: Pushes to `main`, pull requests, and manual dispatch.
- **Tests**: Rust 1.94.0 on Ubuntu x86_64 and stable on the four supported hosted platforms.
- **Lint**: pinned Rust 1.98.0, rustfmt, warnings-denied Clippy, and dependency-direction checks.
- **Installer/scripts**: shellcheck, checksum/version/inventory fixtures, and deterministic signing,
  immutable-publication, workflow, and updater-isolation fixtures. These tests use no credentials.
- **Source package**: clean Rust 1.94.0 locked workspace package and publish dry runs.
- The actual workflow has nine jobs including the test matrix; inspect `ci.yml` for exact gates.

### B. Release Automation (`.github/workflows/release.yml`)
- **Trigger**: Push of git tags matching `v*.*.*`.
- **Workflow authoring constraint**: Step `if:` conditions must stay env-based (`if: env.TAP_TOKEN != ''`). Referencing a secret expression directly inside a step `if:` (e.g. `${{ secrets.X != '' }}`) makes GitHub Actions fail workflow evaluation — every push then produces a zero-job phantom run and tag pushes never trigger the real pipeline.
- **Workflow pipeline**:
  1. Build the four release targets on their hosted platforms and produce archives plus four
     Linux packages. Tagged releases follow reviewed exact-revision CI and Audit checks.
  2. Sign both macOS archives on the maintainer's one-job foreground ephemeral Mac runner.
     The job accepts only a push in `WhiteKiwi/locron` at a release tag and selects the sole
     `locron-signing-${{ github.run_id }}` label. Configure this runner with `--ephemeral` and
     `--no-default-labels`; do not install a persistent runner service or expose it to PRs.
     The parent release operator owns exact-run routing, observation and de-registration.
  3. `scripts/sign-macos-release.py` validates private staged inputs and uses the existing login
     Keychain Developer ID identity `F998EC776E413B3E4D00D5A1D63BBE6FA5C5765E`, team
     `4H4Z446LHS`, stable identifier `dev.locron.cli`, runtime and timestamp. It acquires the
     persistent shared `~/Library/Caches/home-hub/apple-signing.lock` using `fcntl.flock` and
     never unlinks it. Keychain search lists, ACLs, defaults and contents remain unchanged.
     One ZIP of both signed executables is submitted using `c6s-notary`; accepted status,
     issue-free notary log, strict signature checks and online
     `codesign -R=notarized --check-notarization` verification are required.
     Both final archives appear only after every check succeeds. An internal receipt and notary
     diagnostics are retained separately; no extra public asset or private key is distributed.
     Bare executables and ZIPs cannot have tickets stapled. Developer attribution does not
     promise a combined background entry, cleanup of old records, or notification suppression.
  4. Publish all five crates only after build and signing succeed. Inventory exact versions;
     publish when none exist, skip when all exist, and fail on partial inventory. Use the protected
     `crates-io` environment and OIDC; install and verify the exact registry package afterward.
  5. Download only Linux artifacts and the separate signed-macos artifact. Verify exactly four
     archives and four packages, then generate bare-filename SHA256SUMS over those final bytes.
     Publish those eight files, checksums and the tagged installer (ten assets). Require curated
     changelog notes. An existing release is accepted only when all ten digests and its exact
     inventory match; never clobber assets or rewrite existing notes. Homebrew uses the same
     final archive digests and commits the rendered formula to the tap.
  6. On ephemeral hosted Linux x86_64, `scripts/smoke-published-updater.py` verifies the published
     v0.9.2 archive, creates a receipt-bearing temporary binary and durable fixture job, guards
     the expected latest tag, and runs the actual updater. It preserves HOME, isolates state and
     XDG config, removes service-session and Locron overrides, and checks version/envelope/digest,
     receipt, unchanged job state and absence of manager units. Never run it on a personal host.
- **Timeouts**: hosted builds 45 minutes, local signing 60, crates 30, GitHub publication 10,
  published-updater smoke 15. The signing command/lock budget is bounded independently.

### C. Dependency Audit (`.github/workflows/audit.yml`)
- **Trigger**: Daily on a schedule, and on any push or pull request that touches a `Cargo.toml`, `Cargo.lock`, `deny.toml`, or the audit workflow itself. Manual runs are available via `workflow_dispatch`.
- **Deliberately separate from `ci.yml`**: A newly published RUSTSEC advisory is not a defect in an unrelated pull request, so it must not turn that pull request red. Only dependency-affecting changes run the audit in PR context.
- **Checks**: `cargo deny` with two matrix splits — `advisories` and `bans licenses sources` — so a security advisory is distinguishable from a license or duplicate-version finding at a glance in the checks list. Policy lives in `deny.toml`: permissive licenses only, unknown registries and git sources denied, and duplicate versions warned.
- **Job timeout**: 15 minutes per matrix split.

---

## 4. Package Distribution Channels

### 1. GitHub Releases (Direct Download)
- The primary source of truth for release binaries, release notes, and checksums.
- Standalone binaries can be downloaded, unpacked, and placed directly in `$PATH`.
- The `install.sh` asset is the convenience installer for macOS and Linux; it defaults to `~/.local/bin/locron` and verifies the archive against `SHA256SUMS.txt`.
- Only receipt-bearing standalone-installer installations update with `locron self-update`.
  Manually copied tarballs, Cargo installs, source builds, and packages use their own channel.

### 2. Homebrew Tap (`whitekiwi/homebrew-tap`)
- **Repository**: `https://github.com/whitekiwi/homebrew-tap`
- **Formula**: `Formula/locron.rb`
- **Installation**:
  ```sh
  brew tap whitekiwi/tap
  brew install locron
  ```
- **Service**: the formula ships a `service` block (`run [opt_bin/"locron", "daemon", "run"]`, `keep_alive true`, `run_at_load false`) and a caveat: start the daemon with `brew services start locron` (installation never starts it), and `brew services restart locron` after an upgrade because `brew upgrade` leaves a running service on the old version. `locron self-update` and `locron service install|uninstall` refuse on the marker-bearing binary, directing users to `brew services`.
- **Update Automation**:
  Upon a successful GitHub Release, the release workflow calculates the SHA-256 for macOS ARM64 and Intel archives and creates a pull request (or direct commit) updating `Formula/locron.rb` in the tap repository.

### 3. Linux Packages (Debian/Ubuntu `.deb`, RedHat/Fedora `.rpm`)
- Package definitions specify `/usr/bin/locron` install target.
- `.deb` and `.rpm` packages are attached directly to GitHub Releases for distribution.
- Package installations never register the daemon automatically: the postinst/postin scripts print the registration guidance (how to run `locron service install` from a login session, or `locron daemon run` immediately), mirroring the script installer's no-session guidance.

### 4. crates.io (Rust source build)

Rust 1.94+ users install with `cargo install --locked locron`, update with the same command, and
remove with `cargo uninstall locron`. Cargo does not register services automatically; Locron's
service and dashboard registration commands remain available. All five workspace packages publish
in dependency order from the same immutable release revision.

---

## 5. Release and Remediation (Rollback) Policy

### Standard Release Procedure
1. Ensure all milestone criteria and tests pass.
2. Update the workspace version and all four exact internal versions in `Cargo.toml`, then run
   `cargo check` to update `Cargo.lock`.
3. Generate the release changelog: `git cliff --unreleased --prepend CHANGELOG.md` (see [Changelog Maintenance](#changelog-maintenance) below).
4. Review and curate the generated entry — reword, merge, and drop entries until it reads like a user-facing document.
5. Commit version bump: `git commit -m "release: vX.Y.Z"` with the workspace version and the curated changelog.
6. Create and push annotated tag: `git tag -a vX.Y.Z -m "locron vX.Y.Z" && git push origin vX.Y.Z`.
7. Monitor GitHub Actions release workflow execution until GitHub Release and Homebrew Tap update complete.

### First crates.io bootstrap

The first publication cannot use trusted publishing. From the exact clean, reviewed release commit,
run the full package and publish dry-run gate, create a narrow temporary crates.io token, and run
`cargo publish --workspace --locked`. Confirm all five exact versions, revoke the token immediately,
then configure trusted publishers on every package for owner/repository `WhiteKiwi/locron`, workflow
`release.yml`, and environment `crates-io`. Only then push the immutable tag; its idempotent inventory
must find all five versions and continue without uploading. No permanent registry token belongs in
GitHub secrets.

If publication stops partway, do not move the tag or rerun blindly. Inventory the five exact
versions, select only the absent packages with explicit repeated `-p` options from the same commit,
let Cargo order that subset, and rerun the release only after all five are visible. Published
versions cannot be overwritten; fix a bad release with a new version and yank only when warranted.

### Changelog Maintenance

- **Source of truth**: `CHANGELOG.md` follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) with UTC release dates. Release notes for the GitHub Release are generated from the same file, so the changelog and the release notes never diverge.
- **Generation**: [git-cliff](https://git-cliff.org) renders entries from commit history using [`cliff.toml`](../cliff.toml). The commit convention is the input: `feat:` → Added, `fix:` → Fixed, `perf:`/`refactor:`/`revert:` → Changed, `docs:` → Documentation. `ci:`, `test:`, `chore:`, and `release:` commits are deliberately omitted as not user-visible.
- **Curation is required**: git-cliff output is a draft. The maintainer curates before the release commit — entries may be reworded, merged, or dropped, but the generated section names and UTC date format stay intact so hand-editing and future regeneration coexist.
- **Breaking changes**: Declared with `!` after the type (`feat!:` / `fix!:`) or a `BREAKING CHANGE:` footer; during pre-1.0 this bumps the minor version.

### Remediation & Rollback Policy
- **Never modify existing release tags or published binary artifacts**.
- If a critical defect is discovered in release `vX.Y.Z`:
  1. Immediately draft a hotfix and test suite reproducing and fixing the issue.
  2. Bump version to patch release `vX.Y.(Z+1)`.
  3. Tag and publish `vX.Y.(Z+1)`.
  4. Update the GitHub Release notes of the defective `vX.Y.Z` with a warning banner advising users to upgrade to `vX.Y.(Z+1)`.
  5. The Homebrew tap formula will automatically point to `vX.Y.(Z+1)`.

---

## 6. Provenance, Security, and Signing

- **Build Isolation**: Binaries are built entirely in clean GitHub Actions runners using `--locked` Cargo dependencies.
- **Supply Chain Integrity**: Checksums are computed in the release runner and published alongside binaries.
- **Permissions**: permissions are job-scoped. Builds use `contents: read`; `publish-crates` alone
  uses `contents: read` plus `id-token: write` for a short-lived crates.io token; the GitHub Release
  job alone uses `contents: write`. There is no workflow-wide write permission or permanent
  crates.io token.

---

## 7. Usage and Installation Measurement

`scripts/usage.sh` prints one snapshot of locron's public distribution-channel usage. It depends only on `curl` plus standard `grep`/`sed`/`awk` (`jq` is not required to run it), and the GitHub CLI (`gh`) is optional and enables only the traffic section. Run it from the repository root:

```sh
sh scripts/usage.sh
```

The snapshot covers:

- **GitHub Releases** — per-release asset download totals and a grand total from the releases API. Counts are cumulative and reset when an asset is deleted and re-uploaded, so they are a floor rather than an exact ledger.
- **Stars** — the repository's `stargazers_count`.
- **Homebrew** — install counts for the `whitekiwi/tap/locron` formula over 30, 90, and 365 days. Analytics are anonymous and opt-out, so the counts understate real installs, and a formula with no recorded installs has no entry at all (rendered as 0).
- **crates.io** — `N/A (not published)` while the crate is unpublished; once published, the sum of the `/api/v1/crates/locron/downloads` series (trailing 90 days).
- **GitHub traffic** — 14-day views and clones (totals and uniques). This data is owner-only and appears only when `gh` is installed and authenticated (`gh auth login`); otherwise the script prints a one-line note on how to enable it.

When the unauthenticated GitHub REST quota (60 requests/hour) is exhausted, the GitHub sections print the limit message with retry guidance (`GITHUB_TOKEN` or `gh auth login`) instead of raw API errors. A failing section is marked `FAILED` while the remaining sections still print; the exit status is 0 only when every section succeeded.

For automation (e.g. a future scheduled snapshot job), the same snapshot as one flat JSON object:

```sh
sh scripts/usage.sh --json
```

Traffic keys are present in the JSON only when `gh` is authenticated; `crates_io` is `null` while the crate is unpublished; a failed section contributes a `<section>_error` string key instead of its numeric keys.
