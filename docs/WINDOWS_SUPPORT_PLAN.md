# Windows Support Plan

## Status

**Proposal only.** This document describes the recommended path to make Windows an officially
supported Locron platform. Merging this planning PR does **not** mean Windows is supported yet and
does not change the current macOS/Linux support statement.

Before implementation begins, acceptance of this proposal must be reflected through the repository's
authoritative planning workflow in this order:

1. amend `docs/SPEC.md` to add Windows to observable product scope and completion criteria;
2. record resolved Windows platform decisions and evidence in `docs/FINDINGS.md`;
3. fold the selected architecture and trade-offs into `docs/IMPLEMENTATION.md`;
4. add the phased implementation/verification checklist to `docs/TODO.md`;
5. only then change runtime or release code.

The intent of this PR is to make that next planning cycle concrete and reviewable before touching
platform-sensitive scheduler code.

**Distribution decision (2026-10-02):** the first Windows release will use unsigned MSVC ZIPs,
immutable GitHub release assets, and SHA-256 verification. Authenticode signing is a deferred,
non-blocking follow-up tracked in [#37](https://github.com/WhiteKiwi/locron/issues/37).

## Recommended support scope

### First official Windows release

- Windows 11 desktop releases supported by Microsoft.
- `x86_64-pc-windows-msvc`.
- `aarch64-pc-windows-msvc`.
- Per-user operation only, matching Locron's current product model.
- No administrator privileges required for normal install, daemon/dashboard registration, or use.
- The CLI, scheduler daemon, dashboard, MCP server, SQLite state, process targets, shell targets, and
  HTTP targets are all part of the supported surface.

GitHub currently provides standard public-repository hosted runners for both Windows x64 and Windows
11 ARM64, so both architectures can be continuously tested without adding a private runner.

### Explicitly deferred

- Windows Server as an independently supported product target.
- 32-bit x86 Windows.
- Windows Service / LocalSystem installation.
- MSI/MSIX packaging.
- Microsoft Store distribution.
- Authenticode signing for the first Windows milestone; track it separately in #37.
- system-wide/multi-user scheduling.
- WSL-specific integration beyond treating WSL as Linux when Locron is installed inside WSL.

A later milestone can add these only if there is a concrete distribution or enterprise use case.

## Current portability audit

The core scheduler and SQLite model are largely portable, but the current composition and execution
layers intentionally assume Unix in several places.

### Already portable or close to portable

- The durable model, schedules, policies, HTTP runner, dashboard HTTP surface, MCP model, and SQLite
  storage do not fundamentally depend on macOS/Linux.
- `std::fs::File::try_lock` already maps to `LockFileEx` on Windows, so the daemon's single-owner
  lock can keep the same high-level design.
- Tokio already provides Windows-capable process, networking, signal, and named-pipe primitives.

### Known Windows blockers

- `locron-engine` imports `nix` unconditionally and uses Unix process groups plus
  `SIGTERM`/`SIGKILL` for tree cancellation.
- daemon wakeup uses Unix datagram sockets in the CLI composition root.
- the default shell is hard-coded to `/bin/sh`, and shell snapshots always invoke `-c`.
- executable lookup currently follows Unix-style PATH behavior and does not model `PATHEXT`.
- default state discovery assumes `HOME` plus macOS/XDG conventions.
- "owner-only" state and token permissions are currently implemented only with Unix mode bits; the
  non-Unix branch is effectively a no-op.
- path-safety checks reject symbolic links but need an explicit Windows reparse-point/junction threat
  model.
- service management only has launchd and systemd backends.
- the installer and release archives are POSIX-shell/tar oriented.
- self-update replaces the running executable with an in-process rename, which cannot be assumed to
  work for a mapped/running Windows executable.
- tests contain Unix shell scripts, signals, paths, permissions, and process-tree fixtures that need
  platform-neutral equivalents rather than broad `cfg` skips.

## Platform behavior decisions

### State and security

Use a machine-local per-user state root under:

`%LOCALAPPDATA%\locron`

`--state-dir` and `LOCRON_STATE_DIR` continue to override discovery exactly as they do on other
platforms.

Windows state security must preserve the current intent of private `0700` directories and `0600`
files:

- state, lock metadata, dashboard token, receipts, and output artifacts must not become readable by
  arbitrary local users;
- use a current-user ACL/DACL policy rather than silently accepting inherited broad permissions;
- explicitly detect unsafe reparse-point/junction traversal at managed roots and output paths;
- keep SQLite state on a local filesystem as the supported contract.

Do not weaken the existing security model merely to make the build pass.

### Shell semantics

Keep **direct process execution** as the preferred portable mode.

For `--shell` on Windows:

- default to the Windows command interpreter (`cmd.exe`) with Windows-specific command arguments
  rather than pretending `-c` is portable;
- keep `--shell-executable` as the escape hatch for PowerShell, `pwsh`, Git Bash, or another
  explicitly selected shell;
- document that shell command strings are platform-specific and are not expected to be portable
  between POSIX shell and Windows command syntax.

Executable resolution must follow Windows expectations, including `PATH`, `PATHEXT`, absolute
drive/UNC paths, spaces, and case-insensitive extension lookup where applicable.

The minimal inherited environment also needs a Windows definition covering at least the equivalents
required for normal executable resolution and user paths (`PATH`, `PATHEXT`, `SystemRoot`,
`COMSPEC`, `USERPROFILE`, `USERNAME`, `TEMP`, `TMP`, `APPDATA`, and `LOCALAPPDATA`).

### Process-tree supervision

Replace the Unix process-group implementation behind a platform abstraction.

Recommended Windows model:

- create each process attempt inside a Windows Job Object;
- keep the job handle for the lifetime of the attempt;
- use the Job Object as the authoritative process-tree boundary for timeout, cancel, replace, daemon
  shutdown, and cleanup;
- preserve the durable Locron outcome model rather than exposing Windows-specific process states.

Windows has no universal `SIGTERM` equivalent. The implementation should preserve the product-level
contract ("request termination, allow a bounded grace period where a cooperative mechanism exists,
then force the process tree") without claiming that every Windows program receives a graceful signal.
A Windows console-control attempt may be used when it is reliable; `TerminateJobObject` is the
authoritative hard-stop boundary.

The implementation must add explicit tests for grandchildren and nested process trees. A successful
cancel/timeout cannot mean only that the immediate child disappeared.

### Daemon wakeup

Keep wakeup correctness-optional and durable-state-first, as today.

On Windows, replace the Unix datagram endpoint with a local named pipe behind the same logical
wake-port abstraction:

- the pipe is local-machine only;
- pipe access is restricted to the owning user (plus system principals required by Windows);
- payload remains an untrusted versioned wake hint;
- missing/stale/busy pipe behavior degrades to the existing periodic reconciliation rather than
  becoming a correctness failure.

Do not accept the default named-pipe DACL as the final security posture: Microsoft documents that the
default descriptor grants read access beyond the creator.

### Per-user daemon and dashboard registration

Use **Task Scheduler**, not a Windows Service, for the first Windows release.

The desired registration is:

- logon-triggered;
- current user;
- least-privilege/LUA run level;
- interactive-token or equivalent current-user registration;
- no stored account password;
- no administrator requirement;
- idempotent install/refresh/status/uninstall behavior;
- separate registrations for the daemon and dashboard, preserving today's separation.

Microsoft documents that a non-admin user can register a task for their own account without a password
when using S4U or an interactive logon type. The implementation should prove this on a clean standard
user account before the behavior is documented as supported.

Prefer a deterministic Task Scheduler adapter whose generated definition can be inspected in tests.
Do not make localized CLI output from `schtasks.exe` part of Locron's semantic contract.

### Dashboard

The dashboard stays loopback-only on Windows. Validate:

- IPv4 and IPv6 loopback binding;
- no unintended Windows Firewall exposure/prompt for loopback-only usage;
- token ACL/privacy;
- Task Scheduler lifecycle for `dashboard enable|disable|status`;
- browser behavior with the same local authentication contract.

## Implementation phases

### Phase 0 — authoritative planning

- Amend SPEC/FINDINGS/IMPLEMENTATION/TODO after this proposal is accepted.
- Freeze the minimum Windows version, shell behavior, Task Scheduler contract, state location,
  process-tree semantics, wake IPC, and distribution channels.
- **Verify:** every Windows-visible behavior has one authoritative planning location; planned
  milestone acceptance is explicit, while current released-platform statements stay accurate until
  the final release gates pass.

### Phase 1 — compile and path portability

- Move Unix-only dependencies/imports behind target-specific boundaries.
- Add Windows state discovery, environment handling, path normalization, executable lookup, and
  shell snapshot construction.
- Preserve `unsafe_code = "forbid"` in Locron workspace code. Prefer safe Rust wrappers or external
  OS adapters; any proposal to relax this policy requires a separate explicit decision.
- **Verify:** all workspace crates build and unit tests run on Windows x64 and ARM64.

### Phase 2 — supervision and wake IPC

- Add Windows Job Object process-tree ownership.
- Add Windows cancellation/timeout/replace/shutdown tests.
- Add the secure local named-pipe wake backend.
- Keep the 30-second durable reconciliation fallback as the correctness path.
- **Verify:** descendant processes cannot survive a confirmed hard cancellation, stale wake endpoints
  do not break scheduling, and crash/restart recovery preserves the existing durable outcomes.

### Phase 3 — service lifecycle and security

- Add Task Scheduler daemon and dashboard backends.
- Implement Windows ACL/reparse-point handling for managed state.
- Extend `doctor` and human/JSON diagnostics with Windows service and state facts.
- **Verify:** a clean non-admin Windows user can install/uninstall both registrations, reboot/log in,
  observe automatic scheduler startup, and cannot read another user's Locron token/state through the
  supported layout.

### Phase 4 — CI and release artifacts

Add Windows to CI before calling the target supported:

- stable tests on `windows-2025` x64 and `windows-11-arm` ARM64;
- pinned lint/toolchain coverage on at least Windows x64;
- one Windows MSRV build/test gate;
- release builds for `x86_64-pc-windows-msvc` and `aarch64-pc-windows-msvc`.

Release assets:

- `locron-v{version}-x86_64-pc-windows-msvc.zip`
- `locron-v{version}-aarch64-pc-windows-msvc.zip`

Each archive should contain the unsigned `locron.exe`, README, and both licenses. The existing
checksum inventory/immutability checks must include the final Windows assets while retaining
compatibility with historical releases. Publish through the canonical `WhiteKiwi/locron` GitHub
release over HTTPS; SHA-256 verifies downloaded bytes against that release, not publisher identity.

- **Verify:** both archives are built on the reviewed release revision; release source, checksums,
  version and architecture match; the executable is recorded as unsigned and runs on clean Windows
  11 VMs without Rust/Visual Studio installed under the documented supported security settings.

### Phase 5 — installation and updates

Provide three Windows channels:

1. **GitHub Releases** — unsigned ZIPs as the canonical release artifacts for the first milestone.
2. **Standalone PowerShell installer** — user-scoped install with a receipt and optional service
   registration, analogous to the current shell installer.
3. **WinGet** — preferred package-manager experience for normal Windows users.

Cargo remains available for Rust users once the crate builds on Windows.

Package-manager installs must keep the current ownership rule: Locron must not self-update a binary
owned by WinGet. The standalone installer owns only its own receipt-bearing installation.

Windows standalone self-update needs a Windows-specific handoff because overwriting the currently
running executable cannot rely on the Unix rename behavior. Use a verified second-process/helper
handoff that waits for the original process to exit, replaces the exact owned executable, preserves
receipt semantics, and refreshes previously enabled registrations. Do not use "replace on reboot" as
the ordinary update path.

- **Verify:** install, upgrade, downgrade refusal, interrupted update, untrusted-source/checksum/
  version/architecture failure, service refresh, uninstall, and package-manager ownership all have
  clean-VM tests. Installer and updater verification use the explicit initial unsigned policy;
  checksum verification must not be described as Authenticode or publisher authentication.

### Phase 6 — documentation and release acceptance

Only after the Windows gates pass:

- update README positioning from macOS/Linux to macOS/Linux/Windows;
- update INSTALL, OPERATOR, CLI, RELEASE, SECURITY, CONTRIBUTING, examples, and troubleshooting;
- add Windows examples without rewriting POSIX examples as if shell syntax were portable;
- add a release acceptance run on fresh x64 and ARM64 Windows 11 machines.

Windows support is complete only when the same release revision has green CI, unsigned immutable
published artifacts with verified checksums, verified package-manager metadata, documented security
policy limitations, and clean-machine acceptance evidence. Signing enrollment is not a release gate.

## Distribution and non-code work

The runtime changes are only part of the milestone. The release should not be called complete until
the following operational work is done.

### Initial unsigned distribution and deferred signing

The maintainer selected unsigned distribution for the first Windows milestone. Release packaging,
PowerShell installation, self-update, and WinGet must agree on this policy. Provider eligibility,
business-history checks, and signing credentials are deferred to #37 and do not block this release.

- build both architectures from the reviewed revision and publish immutable ZIPs and SHA-256
  checksums through the canonical GitHub release;
- verify the exact release source, version, architecture and final archive digest before installation
  or update; preserve receipt, ACL and rollback requirements;
- state clearly that the executable has no Authenticode publisher identity and that a checksum from
  the same release source checks integrity without independently authenticating the publisher;
- document observed Defender/SmartScreen behavior and unknown-publisher prompts without promising
  warning-free execution; Smart App Control or organizational policy can block unsigned files;
- record security settings and download provenance in clean-machine acceptance, including browser
  downloads with Mark of the Web; document blocked configurations as limitations and do not make
  disabling Windows protections an automatic installer/update action.

If signing is adopted later, publish a new immutable release with an explicit verification-policy
migration, trusted timestamps, and a stable validated publisher/profile that tolerates legitimate
certificate renewal. Do not rewrite prior unsigned assets or silently fall back from a required
signature check to checksum-only verification. Signing still does not guarantee SmartScreen reputation.

### WinGet publication

Treat WinGet as the Windows equivalent of the maintained Homebrew channel, not as an afterthought.

- reserve a stable package identifier (recommended: `WhiteKiwi.locron`);
- publish x64 and ARM64 manifests from the same immutable GitHub release;
- use user scope / portable package behavior where it fits the CLI;
- automate or at least script manifest generation and checksum updates;
- make upgrade/uninstall behavior explicit;
- ensure WinGet owns updates for WinGet-installed binaries.

### Installation UX

Decide and document before shipping:

- the standalone per-user install directory;
- whether the standalone installer modifies the user PATH or prints an explicit opt-in command;
- where service/dashboard logs live;
- uninstall behavior;
- what is intentionally preserved (jobs/history/state) after uninstall;
- how package-manager installs differ from standalone installs.

### Support and compatibility testing

Maintain a Windows-specific acceptance matrix covering at least:

- standard non-admin user;
- x64 and ARM64;
- username/path containing spaces and non-ASCII characters;
- long paths;
- locked/unlocked desktop sessions;
- reboot and sign-out/sign-in;
- Defender/SmartScreen/Smart App Control behavior and any organizational policy restrictions;
- direct process, `cmd.exe` shell, explicit PowerShell/`pwsh` shell, HTTP target;
- timeout, cancel, replace, retry, missed-run recovery, crash recovery;
- dashboard and MCP;
- install/update/uninstall through each supported channel.

Do not make a developer machine with Visual Studio, Git Bash, or PowerShell 7 installed the only
acceptance environment.

### Documentation and support expectations

Windows support introduces user questions that do not exist on Unix:

- Task Scheduler entries and their lifecycle;
- SmartScreen / unsigned publisher status, canonical download source, and checksum verification;
- PATH and command-extension resolution;
- Windows path quoting;
- PowerShell versus `cmd.exe`;
- antivirus/EDR false positives;
- where LocalAppData state lives.

These need operator guidance and actionable `doctor` output before the support badge changes.

## Release-go/no-go checklist

Windows becomes an advertised supported target only when all are true:

- [ ] SPEC/FINDINGS/IMPLEMENTATION/TODO are updated and reviewed.
- [ ] Windows x64 and ARM64 CI are blocking and green.
- [ ] process-tree cancellation is proven with descendants.
- [ ] current-user state/token ACLs are verified.
- [ ] named-pipe wake behavior and fallback are verified.
- [ ] daemon and dashboard register without admin/password on a clean standard account.
- [ ] reboot/login recovery is verified.
- [ ] unsigned immutable ZIP assets and checksums are published from the reviewed revision.
- [ ] published source/version/architecture/digests match the initial unsigned policy; security
      warnings and blocked configurations are documented, without requiring signing enrollment.
- [ ] standalone install/update/uninstall passes on clean VMs.
- [ ] WinGet ownership/update behavior is verified.
- [ ] README/INSTALL/OPERATOR/CLI/RELEASE/SECURITY/CONTRIBUTING no longer contradict support.
- [ ] clean-machine acceptance evidence exists for both architectures.

## References

- Microsoft Job Objects:
  https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects
- Microsoft Named Pipe Security:
  https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-security-and-access-rights
- Microsoft Task Scheduler security contexts:
  https://learn.microsoft.com/en-us/windows/win32/taskschd/security-contexts-for-running-tasks
- Microsoft per-user LocalAppData known folder:
  https://learn.microsoft.com/en-us/windows/win32/shell/knownfolderid
- Microsoft SmartScreen reputation guidance:
  https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation
- Microsoft WinGet manifest documentation:
  https://learn.microsoft.com/en-us/windows/package-manager/package/manifest
- GitHub-hosted runner reference:
  https://docs.github.com/en/actions/reference/runners/github-hosted-runners
- Rust `std::fs::File` locking:
  https://doc.rust-lang.org/std/fs/struct.File.html
