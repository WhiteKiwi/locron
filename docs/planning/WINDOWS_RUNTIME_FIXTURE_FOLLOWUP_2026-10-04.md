# Measured native runtime-fixture follow-up

Frozen SPEC, #31 and the first unsigned-release/signing decision remain unchanged. Separate
research retains the7ad/543 failure evidence and complete caller inventory. Current branch
6939b35e235009b905739e0636c6c3658d71f956 imports accepted main throughd56dcbb; the eleven earlier
PR130 blobs are conserved exactly. The temporary no-fail-fast caller is not imported here.

1. Review research, FINDINGS and IMPLEMENTATION, record this plan and owning #31 Verify criteria
   before Source. **Verify:** docs-only planning commit and exact CLI comment readback precede
   separate development; current worktree is clean and frozen SPEC/source invariants unchanged.
2. Change only mcp::tests::fresh_paths setup on Windows: retain TempDir, select a missing private
   child, obtain DirectoryGuard::private normalized path, and release the setup guard normally.
   Preserve Unix StatePaths::new(dir.path()) and every case body/name/assertion.
   **Verify:** all eleven actual mutation/dry-run/report cases reach their intended assertions
   on native x64/ARM64/MSRV; remaining error cases also run with a valid root. Existing broad-root,
   foreign-owner, reparse and private-creation contracts remain separately unchanged and required.
3. Add a narrow test-only isolation mutex for exactly the five parent Gate owner-sensitive
   fixtures from the research inventory. Acquire it before fixture setup/deadline birth, hold it
   through existing actual release/readback, fail on poison and never reset OWNER. Preserve the
   separate actual copied-child ENTRY namespace/selector and concurrent negative probes.
   **Verify:** all five cases run in ordinary unrestricted harness concurrency and pass meaningful
   native assertions; uncertain ownership continues rejecting internal second admission until
   actual cleanup. Phase30s/cleanup3s and every native image/EOF/guard/child assertion are exact.
4. Add only concise role field help to hidden Windows supervise. Add matching macOS/Linux cfgs
   to the seven measured backend helpers, related imports and ServiceCleanup Drop. Do not gate
   the crate or add allow/expect. **Verify:** unchanged recursive description contract passes on
   Windows; both backend-exclusion reports remain selected; Unix helper/test bodies remain exact.
   Full warnings-denied Clippy still reports any remaining staged CLI diagnostics honestly.
5. Observe the Core guard-stall result-delivery interval using bounded lock-free test-only
   timestamps for result-computed, diagnostic entry/exit, actual function return, external send
   and receive (including timeout). No secrets/input/path payload in event output. Preserve the
   current test-only diagnostic call and its position, and emit new timing only after known
   delivery/cleanup or in the existing failure message. **Verify:** Source review conserves
   production behavior and original30+3s/30+4s timed receives, stock guard/permit retention,
   incompatible-open, second refusal, late no-spawn/input/marker and cleanup assertions.
   Native logs identify reached/missing events; no passing run claims an earlier failure's cause.
6. Parent reviews all changed Source plus blob/selector conservation and publication readiness.
   **Verify:** Rust1.94 format/actionlint/diff checks pass; all other tracked blobs/modes match the
   reviewed baseline; new code remains fixture/help/test-only timing. No task/PATH/policy/ACL
   repair, adapter fallback, deadline increase, retry, blanket serialization or warning allowance.
7. Publish PR130 final scope, integrate current base without semantic patch drift, and collect
   fresh normal CI plus the original five full native rows. **Verify:** exact final head/native
   compiler/host, failed cases and timing snapshots are retained; required normal gates pass
   before protected merge, old failures remain recorded. Full suites/public support remain open
   unless their original criteria actually pass; exact CLI #31/#32 readback preserves body/state.

Source handoff is limited to:
- crates/locron-cli/src/mcp.rs (tests::fresh_paths only)
- crates/locron-cli/src/windows_launch_gate.rs (five parent test fixtures and test-only mutex)
- crates/locron-cli/src/service.rs (hidden supervise role help only)
- crates/locron-cli/tests/service_backends.rs (seven helpers/imports/Drop cfg only)
- crates/locron-core/src/windows.rs (cfg(test) return/timing observation only)
- crates/locron-core/src/windows/generic_trace.rs (test-only bounded timing only)
- crates/locron-core/src/windows/loader_tests.rs (guard-stall observation only)

Application privacy/worker/owner/security policy, other staged package APIs/style diagnostics,
workflow selection, dependencies, test assertions/names/budgets and all owner-host effects remain
outside this slice. A changed decision or newly measured defect needs a reviewed plan first.
