# Native Windows validation continuation

The user requested Windows-only verification first. Frozen SPEC's Windows amendment and owning
Issues #31/#32 remain authoritative; no product behavior or support claim changes. Main
00e1006b817b5442fbc9b5b79e1b736d439661a3 is the initial measured baseline. FINDINGS records both
downloaded paired ZIPs' actual 1.98.0 compiler despite the intended 1.94.0 package selection.

1. Set RUSTUP_TOOLCHAIN=1.94.0 at the Windows package job and verify rustc release/native host
   before either executable build. Add the same step-scoped enforcement to the existing Windows
   release build without changing Unix compilation, release inventory, tags, signing or publication.
   **Verify:** actionlint and exact source review pass; fresh x64/ARM64 CI package logs and both
   legacy/paired verification records show actual 1.94.0 with the intended native host, static CRT,
   original package cases and staged final-byte version/GUI identity probes.
2. Add an explicit boolean workflow_dispatch input for full Windows discovery to the existing CI.
   Additional jobs run only when requested: all-target locked workspace tests on native x64 stable,
   ARM64 stable and x64 1.94.0; full all-target warnings-denied Clippy with 1.98.0 on both native
   architectures. Keep toolchain overrides explicit and retain existing PR/component gates.
   **Verify:** normal PR CI does not run the discovery jobs; an exact-branch manual invocation
   records the intended host/compiler and runs the unmodified full commands on all five rows.
   Failures stay visible; no skip, softened assertion, deadline change or unsupported adapter
   fallback is introduced to make discovery pass.
3. Review and publish through the ordinary protected-branch PR path, then collect completed raw
   logs, compiler records and final paired archives. Native default-feature full commands preserve
   the workspace's existing selection; the existing GUI-feature gates remain required separately.
   **Verify:** parent independently reviews all changed blobs, checks normal CI and records each
   discovery compile/test/lint result, including exact failed test/boundary where applicable.
   If the manual input cannot be dispatched before the new input exists on main, use the merged
   revision and record that precise revision rather than changing the workflow trigger.
4. Perform finite local x64 executable version/identity probes only on hash-matched downloaded
   draft bytes with system-directory-only child PATH and task-owned temporary paths. ARM64 runtime
   evidence comes from its native runner. **Verify:** both architecture ZIP/PE/hash records agree,
   three local x64 probes have exact output/exit facts, and temporary cleanup remains inside the
   checked absolute task directory. No local script, execution-policy, task, PATH or reboot change.
5. Record results via CLI in the owning Issues, retaining unfinished scope and current status.
   **Verify:** exact comment readback preserves original issue body/state; #31/#32 remain open
   until their full original requirements pass. Installation, GUI activation, active-crash
   recovery, independent-user IPC/privacy, final public bytes and WinGet lifecycle remain separate
   gates. The signing follow-up #37 remains deferred.

Only .github/workflows/ci.yml and the existing Windows build step in release.yml are authorized
for this Source handoff. If discovery exposes an application or fixture defect, report it with
measured native evidence before any new source plan. No application, test, lockfile or toolchain
file changes are authorized in this handoff.
