# Current Windows foundation repair

Owning issues: #172/#31, with #162 for PR144 ownership and #149 for prune.
The user explicitly selected all current Windows foundation failures. Base source
is f8d77077967b9aab1a87b0397a84dfd0204bbf54. SPEC's existing private ownership,
identity, durability and finite-deadline contracts remain frozen. This phase changes
test sequencing and adds closed observations; it selects no product behavior change.

## Evidence and selected boundaries

The three linked research records distinguish actual native receipts from hypotheses:
[daemon](WINDOWS_CI_DAEMON_RESEARCH_2026-10-16.md),
[ownership](WINDOWS_CI_OWNER_RESEARCH_2026-10-16.md), and
[prune](WINDOWS_CI_PRUNE_RESEARCH_2026-10-16.md).
SID's corrected Core command has succeeded in all three Windows rows of run38857980733;
the whole run is still pending. Existing foundation failures remain required.

The daemon role lock precedes fresh Store initialization and endpoint creation.
It cannot authorize an immediate200ms connection expectation. A separately retained
reader of the existing guarded debug carrier may provide a startup hint before the
original authenticated probe. Starting is also a valid pre-running history state;
it must not be confused with unknown/terminal states. Neither observation changes
the probe's ownership authority, its deadline, or the final run/cancellation oracle.

PR144's exact default/file owner classes and N08's actual response/poststate remain
unmeasured. Obtain those facts before selecting permission or removal fixes. Rust's
readonly-ignore deletion fallback is a source-supported candidate, not yet a native
result. EL07's creator failure needs its existing safe persistence record before any
schema or budget change. No token mutation, take-ownership, attribute clearing,
artifact copying, database prewarming or assertion relaxation is selected.

## Ordered work and Verify

1. **Freeze research and publish the complete plan.** Parent reviews SPEC, Findings,
   implementation and all three research records, commits planning documents, posts
   this plan to the owning Issues and reads it back before separate source leases.
   **Verify:** each selected change below has an explicit owner and finite bounds;
   old failures and unknown causes remain visible. Existing issue acceptance remains.
2. **Add read-only ownership observations.** In the existing foundation toolchain
   step, record effective default-owner/user equality and closed token classes. In
   the existing PR144 producer, after its original selected-artifact/hash reads,
   observe only the namespace/controller/Store owner classes and protected-DACL bit.
   One fixed query-only helper accepts at most16KiB private stdin/four object roles,
   emits at most8KiB closed JSON, and has an independent5s diagnostic child cap with
   kill/wait. Validate every output field before rendering; never relay arbitrary
   stdout/stderr, SID, path, ACL, exception or response. Unavailable metadata remains
   unobserved. This independent observation never changes required gate results or
   the existing product/test deadlines. No extra build/test lane is added.
   **Verify:** closed-schema positive/refusal/oversize/malformed/child-timeout controls;
   exact artifact facts, original paths/hash/physical admission, tests, matrix/cache/
   permissions and all other commands remain unchanged. Native facts from all three
   rows are required before a causal owner repair. A later observer does not prove a
   historical rustc token, and path Get-Acl is never retained-handle authority.
3. **Add closed prune failure observations.** Only API qualification emits actual N08
   HTTP status, ok/error/code classes before additional I/O, then no-follow leaf
   class/readonly and the existing two selected candidates' state/timestamp-presence
   classes. Failed owned Store creation emits only StoreError::diagnostic scalars.
   Keep original response/error/SQL/file assertions and60s/90s deadlines literally.
   **Verify:** schema is finite and private values are absent; original failure still
   fails. Native response/leaf/state and creator stage/code/time facts determine the
   next repair plan. CLI summary failure remains unidentified until its own receipt.
4. **Repair genuine-daemon fixture sequencing.** Only the Windows CLI control helper
   adds an independently opened, identity-matched guarded debug-stderr reader owned
   until exact reap. Parse complete rows only with the existing v2 context/role/
   sequence/op validation and64KiB+1 total cap. Empty/split/temporary-EOF remains
   pending; wrong binding, malformed or capped data refuses. Preserve original
   producer readers at cursor0. Under existing5s/8s clipped30s and checked5ms pauses,
   genuine debug Wake/Cancel waits for returned begin_lifetime/ok, rechecks the actual
   child, then invokes the unchanged200ms authenticated probe exactly once. A hint
   is not endpoint authority. Foreign/negative/queued/release cases stay unchanged.
   Preserve existing history codes1..5 and add Starting6 only in History's unused
   three-bit domain. Wait through Starting only before Wake success/Cancel Running;
   unknown/terminal/retry states still refuse. Cancel still needs Running plus two
   progress samples, exact Cancelled and stopped later counters.
   **Verify:** bounded parser/state controls run through the already selected native
   output-capture contract, without a new build/lane. Exercise split/append/empty/
   malformed/wrong-context/cap, reader-position conservation and Starting6 encoding;
   missing/late/error hint cannot authorize success or renew time. Preserve original
   probe clocks/peer/ACK/31-byte literal, all negative selectors, producer capture,
   exact reap/cleanup and Unix source. Source review must show that a valid hint
   reaches only the unchanged probe; original absent/foreign-peer native controls
   still refuse, without claiming they supplied a valid startup hint. Pure expired-
   gate controls show that valid bytes cannot rescue an expired allowance. No new
   native delayed/foreign-endpoint fault-injection scenario is claimed. Run all eight original native selectors on
   x64 stable/MSRV and ARM64; no fixture retry or late-result promotion.
5. **Review native facts and finish all foundation failures.** Parent inspects full
   composed diffs, formatting, strict affected Clippy, actionlint and meaningful
   bounded controls. Commit/push once ready; existing PR CI performs native checks.
   **Verify:** record exact source/job/toolchain and every foundation outcome. Any
   remaining actual owner/removal/migration/daemon failure gets a researched planning
   amendment before source repair, never a retry/skip/timeout increase. Finally all
   ordinary Windows foundation and relevant Unix regressions pass on the same head;
   publish exact PR/Issue receipts without closing broader Windows acceptance.

## Leases and final review

Parent owns plans, commits and publication. Separate development owns source:
ownership: only the existing foundation native-toolchain/PR144-producer workflow
regions and .github/scripts/windows-owner-observation.ps1;
prune: crates/locron-server/src/api_prune/qualification.rs;
daemon: crates/locron-cli/tests/support/windows_cli_control.rs.
No agent may change another lease or a product/dependency/security contract. Any
material protocol/file selection change is reviewed in this plan first. The actual
PR144 fresh tempfile parent remains unobserved in this phase; do not add another
fixture helper/process or infer its owner from the namespace observation. This phase
adds diagnostic work and a guarded reader; it makes no measured speedup claim. The
earlier PR-only cancellation remains the selected CI efficiency improvement. It does
not waive the user's request for all current foundation failures to be repaired.
