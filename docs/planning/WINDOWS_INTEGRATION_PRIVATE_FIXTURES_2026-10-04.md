# Native integration positive fixtures

Continue Issue #31 without changing frozen SPEC, product privacy or unsigned-release policy.
Separate read-only research inventories each state/input/manager root and absent-path contract.
Source work starts only after the parent reviews these documents and reads back the Issue plan.

## Measured basis and boundary

Head302b1bf full run37145285065 executes301 CLI unit cases on x64 stable, ARM64 stable and
x64 Rust1.94: all pass, with zero ignored/filtered. It then executes the original71 CLI
integration cases:12 pass/59 fail on each row. Real stderr includes private-root refusals and
an actual `Usage: locron.exe` mismatch. Cargo stops there; later binaries are not newly qualified.
The four selected integration files are byte-identical between measured543,302 and the current
stock-fixture continuatione4d2ca1. The earlier23-binary run37140952254 measured102 cases in
these files:31 pass/71 fail, with59 printed directory-root refusals across CLI/feedback. Other
omitted error envelopes do not prove an identical cause. Independent native execution, wake,
target-rendering, installer/updater and41 CLI lint failures retain their separate scope.

Ordinary302 run37145289162 additionally fails two x64 lifecycle fixture Store initializations:
`sqlite-configure-wal` / `SQLite configuration deadline elapsed`. Keep that required failure and
the distinct earlier Server SQLITE_PROTOCOL15 reopen fact under Issue #25. This test-setup
slice cannot claim to repair either failure, and no protected merge bypass is permitted.

## Permitted Source

Only `crates/locron-cli/tests/cli.rs`, `dashboard.rs`, `feedback.rs`, `service.rs` and one small
shared `tests/support/private_state.rs` helper may change. No production file, dependency,
workflow, selector/name, cfg exclusion, skip, warning allowance, timeout or OS setting changes.
Do not blanket-replace process/shell literals, socket waits or table assertions in this slice.

1. Introduce a narrow fixture owning TempDir cleanup and exposing one state path. Windows creates
   a previously missing child through DirectoryGuard::private, obtains normalized_path and
   releases setup guards before real subprocesses. Unix retains the original TempDir root.
   Adapt only state-bearing CLI/feedback factories and helper argument types; keep discovery
   working parents and advisory input roots distinct. **Verify:** audit every constructor and
   use against the research inventory; Store/OutputWriter/CLI/input/output assertions use the
   same exposed path. Do not adopt or repair a broad TempDir root. Existing dry-run root-empty,
   state.db/outputs/config absence, history/ID/rollback and redaction assertions stay exact.
2. Convert the five existing dashboard positive state roots and stored-token setup. Use the
   existing private-new token helper with unchanged bytes, flush and released guard. In the
   two service positive setups, guard only their fresh `state` child on Windows. Manual lock
   setup there uses real DaemonLock::acquire with current PID, unique lifetime, version and
   service_mode=false; retain it through fake install. Unix keeps its original manual lock
   fixture. Disable seeds its64-byte token privately. **Verify:** original HTTP/port/URL/owner
   and token assertions, manual registered/deferred/guidance/last-enable/no-start assertions,
   and exact disable stop/status/unload/remove/reload order remain. Ordinary fake-manager
   JSON/log parents and copied inputs remain untouched. Preserve all initially absent state
   and missing-token status/refusal cases. No real Task Scheduler registration is selected.
3. Correct only three help assertions' expected basename to `locron.exe` on Windows and retain
   `locron` on Unix. **Verify:** canonical list/remove/service/dashboard commands, aliases,
   options/examples and successful help exit assertions remain; no production Clap change.
4. Parent reviews all changed files and their exact scope before publication. **Verify:** Rust
   1.94 fmt and diff whitespace checks pass; independently restore every original four-file blob
   from recorded transformations, preserve all other tracked blobs/modes, and conserve the
   original71/8/4/19 integration selectors and three private-token positive cases. Keep Unix
   process/shell literals and every existing deadline/assertion beyond authorized setup/usage
   expectation changes. Report a needed deviation before implementing it.
5. Collect fresh exact-head native results. **Verify:** ordinary protected CI on current main
   and original five full rows run without skips/bypass; record actual outcomes rather than
   expecting all integration failures to disappear. Use the separate verification-only
   no-fail-fast branch to execute all23 original binaries on native x64 stable/ARM64 stable/x64
   MSRV, reporting actual cases/ignored/filtered and newly exposed failures. All102 selected
   integration names and the three prior positive-token cases must actually execute. Zero-case
   crates and fake service order checks do not establish Windows feature or real-task acceptance.
   CLI Issue readback preserves historical failures and every unfinished acceptance gate.

No owner-PC executable/script, ACL/task/user/PATH/policy/reboot, infrastructure, public release
or WinGet submission is part of this handoff. Two-user privacy/IPC needs a separate concrete
native CI plan; integration positive fixtures cannot substitute for that independent proof.
