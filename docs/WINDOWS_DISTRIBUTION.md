# Windows distribution process

Windows support and catalog availability remain planned until the native implementation and
release acceptance gates pass. The first Windows release is unsigned; signing is deferred.
Locron retains its ZIP/portable WinGet format and proposed identifier `WhiteKiwi.locron`.

Use the same release and community submission sequence as the coordinated Pushman Windows
distribution work:

1. Review and merge the exact release candidate, then create one immutable stable version tag.
   Never replace a published tag or asset to correct a release.
2. Build and qualify the final native x64 and ARM64 MSVC ZIP bytes. Require Rust 1.94 compatibility,
   stock Windows dependencies, unsigned PE checks and standard-account acceptance without a
   development toolchain. Preserve the existing signed/notarized macOS process.
3. Generate the exact version-aware payload checksums and provenance attestations from those
   final bytes. Publish the canonical `WhiteKiwi/locron` release and verify the final asset
   inventory and SHA-256 values. Checksums and build provenance do not authenticate an unsigned
   executable as a signed publisher.
4. Generate the version, default locale and installer manifests from the final public ZIP
   hashes. Validate against the official WinGet schema and run native `winget validate` for both
   architectures. Portable manifests omit unsupported Scope metadata; the supported procedure
   explicitly passes `--scope user`.
5. Use the existing `WhiteKiwi/winget-pkgs` fork, a Locron-owned checkout and a separate
   `feat/locron-<version>` branch. Do not change Pushman's checkout, branch or submission.
   Submit one Locron manifest PR to `microsoft/winget-pkgs`; no proprietary catalog is introduced.
6. Follow the community provider checks and any human contribution-agreement requirement.
   Maintainer automation does not accept legal terms on the user's behalf. Do not infer package
   acceptance from local schema validation or an opened PR.
7. After catalog acceptance, verify the live public catalog and real user-scoped install,
   upgrade and uninstall on clean Windows 11 x64 and ARM64. Record the exact version and final
   artifact digests used for acceptance before claiming availability.

WinGet portable packages do not invoke Locron lifecycle hooks or enable services automatically.
With daemon/dashboard roles enabled, upgrades and removals require the explicit verified native
Prepare/Complete/Remove procedure in [INSTALL](INSTALL.md). `winget upgrade --all` and unattended
upgrade tools cannot perform this workflow and are outside this channel's supported procedure.
The manifest does not incorrectly label Locron as a self-updating package to conceal that
limitation. Use standalone installation when integrated service lifecycle/self-update is needed.

The parent planning/publication session owns release tags, release publication and catalog PRs.
Development sessions prepare reviewable inputs and verification evidence. See
[RELEASE](RELEASE.md) for release gates and [PROJECTS](PROJECTS.md) for execution tracking.

Primary references: [official manifest schemas](https://github.com/microsoft/winget-pkgs/tree/master/doc/manifest/schema),
[community contribution process](https://github.com/microsoft/winget-pkgs/blob/master/CONTRIBUTING.md),
[WinGet manifest validation](https://learn.microsoft.com/en-us/windows/package-manager/package/manifest#manifest-validation).
