# locron Release and CI/CD Policy

## Purpose

This document defines the official versioning, release, CI/CD, packaging, and distribution policies for `locron`. It establishes the operational contracts for building, verifying, signing, publishing, and maintaining release artifacts across supported platforms.

---

## 1. Versioning Policy

`locron` adheres strictly to **Semantic Versioning 2.0.0** (`MAJOR.MINOR.PATCH`):

- **`MAJOR` (x.0.0)**: Incompatible API or CLI breaking changes, breaking durable storage migrations that cannot be auto-migrated, or breaking wire/protocol changes.
- **`MINOR` (0.y.0 / x.y.0)**: Backward-compatible new features, commands, configuration options, or additive schema migrations. During pre-1.0 (`0.y.z`), breaking changes bump `MINOR`.
- **`PATCH` (0.y.z / x.y.z)**: Backward-compatible bug fixes, performance improvements, internal refactoring, or documentation updates.

### Workspace Lockstep Versioning
All workspace crates (`locron-core`, `locron-store`, `locron-engine`, `locron-cli`) share the single unified version defined in the workspace root `Cargo.toml` (`[workspace.package] version = "..."`). Independent crate versioning is forbidden.

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
- Every release MUST also publish the repository's `install.sh` (the root-level POSIX sh installer) as a release asset. It is fetched at `https://github.com/WhiteKiwi/locron/releases/latest/download/install.sh`, and the release workflow attaches the checked-out `install.sh` to the release.
- The installer downloads the platform archive through the same static `releases/latest/download/` redirects, verifies it against the release `SHA256SUMS.txt`, and installs it; it supports `LOCRON_VERSION` pinning and `LOCRON_INSTALL_DIR` overrides.

---

## 3. CI/CD Workflow Architecture

The repository employs two automated GitHub Actions workflows:

### A. Validation CI (`.github/workflows/ci.yml`)
- **Trigger**: Every push to any branch and pull requests targeting `main`.
- **Matrix**: 4 runner platforms (Ubuntu 24.04 x86_64, Ubuntu 24.04 ARM, macOS 15 Intel, macOS 14 ARM) × 2 Rust versions (`1.94.0` MSRV, `stable`).
- **Steps**:
  1. Check out repository with up-to-date action versions.
  2. Install toolchain with `rustfmt` and `clippy`.
  3. Validate formatting: `cargo fmt --all --check`.
  4. Validate linter: `cargo clippy --workspace --all-targets -- -D warnings`.
  5. Run complete test suite: `cargo test --workspace --all-targets`.
- **Job timeout**: The `test` job has a 30-minute budget (`timeout-minutes: 30`) so a hung runner or compile step fails fast instead of consuming the GitHub Actions default of 360 minutes.

### B. Release Automation (`.github/workflows/release.yml`)
- **Trigger**: Push of git tags matching `v*.*.*`.
- **Workflow authoring constraint**: Step `if:` conditions must stay env-based (`if: env.TAP_TOKEN != ''`). Referencing a secret expression directly inside a step `if:` (e.g. `${{ secrets.X != '' }}`) makes GitHub Actions fail workflow evaluation — every push then produces a zero-job phantom run and tag pushes never trigger the real pipeline.
- **Workflow Pipeline**:
  1. **Pre-flight & Verification**: Run tests across the release matrix to ensure zero regressions on the tagged commit.
  2. **Build Release Binaries**: Build with `cargo build --release --locked` (leveraging LTO and symbol stripping).
  3. **Package Archives**: Assemble `.tar.gz` bundles with binary, README, and licenses.
  4. **Generate Checksums**: Compute SHA-256 hashes for all generated archives into `SHA256SUMS.txt`.
  5. **Create GitHub Release**: Create a GitHub Release with auto-generated changelog and upload all archives, `SHA256SUMS.txt`, and `install.sh`.
  6. **Homebrew Tap Dispatch**: Trigger downstream update in `whitekiwi/homebrew-tap` with the new version and macOS archive URLs & SHA-256 hashes. The generated formula installs `locron` into `bin` and touches `lib/.disable-self-update` so `locron self-update` refuses package-manager-managed installs.
- **Job timeouts**: The `build` job has a 45-minute budget and the `publish` job a 10-minute budget (`timeout-minutes`). A hung build (e.g. a stalled runner) cancels the workflow instead of blocking the release indefinitely.

---

## 4. Package Distribution Channels

### 1. GitHub Releases (Direct Download)
- The primary source of truth for release binaries, release notes, and checksums.
- Standalone binaries can be downloaded, unpacked, and placed directly in `$PATH`.
- The `install.sh` asset is the convenience installer for macOS and Linux; it defaults to `~/.local/bin/locron` and verifies the archive against `SHA256SUMS.txt`.
- Installations from the installer or tarballs update themselves with `locron self-update` (verified, atomic replacement; never for package-manager-managed installs).

### 2. Homebrew Tap (`whitekiwi/homebrew-tap`)
- **Repository**: `https://github.com/whitekiwi/homebrew-tap`
- **Formula**: `Formula/locron.rb`
- **Installation**:
  ```sh
  brew tap whitekiwi/tap
  brew install locron
  ```
- **Update Automation**:
  Upon a successful GitHub Release, the release workflow calculates the SHA-256 for macOS ARM64 and Intel archives and creates a pull request (or direct commit) updating `Formula/locron.rb` in the tap repository.

### 3. Linux Packages (Debian/Ubuntu `.deb`, RedHat/Fedora `.rpm`)
- Package definitions specify `/usr/bin/locron` install target.
- `.deb` and `.rpm` packages are attached directly to GitHub Releases for distribution.

---

## 5. Release and Remediation (Rollback) Policy

### Standard Release Procedure
1. Ensure all milestone criteria and tests pass.
2. Update workspace version in `Cargo.toml` and run `cargo check` to update `Cargo.lock`.
3. Update `CHANGELOG.md` or release notes.
4. Commit version bump: `git commit -m "release: vX.Y.Z"`.
5. Create and push annotated tag: `git tag -a vX.Y.Z -m "locron vX.Y.Z" && git push origin vX.Y.Z`.
6. Monitor GitHub Actions release workflow execution until GitHub Release and Homebrew Tap update complete.

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
- **Permissions**: Release workflow uses least-privilege GitHub tokens (`contents: write` for release creation, minimal repository dispatch permissions).
