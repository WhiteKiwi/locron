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
   A full_windows=true manual run executes only these five discovery rows; the ordinary CI jobs
   retain their exact behavior on PR, push and default manual events. Their event condition merely
   avoids duplicate normal checks during this explicitly selected discovery run.
   Newly introduced third-party action references use verified full commit hashes; existing
   ordinary job action selection remains unchanged. **Verify:** both new matrices resolve to the
   independently observed upstream Rust toolchain/cache revisions and actionlint passes.
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

## Measured compilation follow-up (2026-10-04)

Ordinary CI37132853263 on head63473f1 succeeds and verifies actual Rust1.94 package compilers.
Separate full discovery37132861132 fails all five rows during compilation/lint. The research
receipt uses standalone Rust1.94/1.98 metadata compilation and inspected existing private-file
APIs; see FINDINGS. Frozen SPEC remains unchanged. This following test-only handoff supersedes
the earlier workflow-only Source limitation for the six listed integration files only.

1. In attempt_history.rs, crash_boundaries.rs, global_environment.rs and service_lifetime.rs,
   place the unchanged crate docs before the existing crate-level cfg(unix). Keep the cfg and
   everything after the initial doc/cfg prefix unchanged. **Verify:** exact reversal restores
   each original complete blob; no test name/body, selector, deadline or platform selection
   changes. Metadata-only warnings-denied compilation on Rust1.94/1.98 accepts the otherwise
   empty Windows crates. Existing Unix suites still run under ordinary PR CI.
2. In dashboard.rs and service.rs, replace only the three positive private-token seeding
   sequences currently using Unix PermissionsExt/from_mode. A small local test helper uses
   existing create_private_new on the absent token path, allowing that API to create a missing
   private parent, writes the same 64-byte fixture, flushes and releases its guard before child
   execution. Do not precede it with fs::create_dir_all/fs::write or repair an existing root.
   Keep all commands, token values, owner_only/redaction/report assertions and other fixtures.
   **Verify:** source review confirms exclusive new creation and guard release, no Unix-only
   imports remain in these three setups, and all original test names/assertions remain. Actual
   private-file behavior is qualified only by native tests, not by source/format checks.
3. Parent reviews the completed plan and exact owning-issue Verify note before handing these
   six integration files to the separate development session. **Verify:** source begins after
   the reviewed docs commit and exact CLI issue readback; Source receipt preserves all other
   tracked blobs/modes, formatting passes and git diff --check is clean.
4. Update PR130 around its final compilation/discovery scope and run unchanged ordinary CI plus
   all five manual full discovery commands on the new exact branch head. **Verify:** native
   full commands either reach runtime/lint beyond the measured prerequisites or expose a
   precise retained next failure; report per-row compiler, phase and failed contracts honestly.
   New unused-helper, staged-code, broad-root or shell assumptions remain visible. No added
   skips, warnings allowances, deadline enlargement or retries are authorized by this slice.
5. Record exact results and remaining scope in #31/#32 using CLI, then use normal protected
   review/merge requirements for the reviewed PR. **Verify:** original issue bodies/states are
   retained, current-base ordinary checks pass on the exact head, and any full discovery
   failures remain visible and prevent a full support/whole-issue completion claim.

Only these six test files are authorized in this follow-up. Application code, other fixtures,
workflow behavior, lockfiles/toolchain files, local effectful acceptance and release publication
are outside it. Any further correction requires measured findings and a docs/issue plan first.
