# Preserve fixed worker phase receipts during reader contention

Owners #172/#31; related #162/#149. Source57e4c4d0b2c6b7dd0ed20013cc8f713748e7b784,
tree89f24f735c7c6f59cad9d15faa0d2c353e7b27a4. Frozen SPEC is unchanged.
The library deduplication amendment and all its same-head acceptance stay intact.
This new scope is ONE test-only recorder acquisition, not a product/deadline change.

## Actual failure and unknown boundary

Run38892570339, native MSRV job116945340333 checks out same-tree merge
d615a4dd7b1b9e02acc631983633ffd2bc6fe77e. Core147PASS/1FAIL in118.07s:
owned_sid_and_create_requests_report_ordered_child_phases. Its isolated
worker_fixture_child fails after30.01s at shared wait_for_phases timeout1824;
the parent reports that child failure at1610. This occurs BEFORE the retired
duplicate library steps. Downstream missing-prune/adapter failures do not establish
a separate product cause. Other rows/results are still being qualified.

The receipt does not print which of3 waits expired, which fixed phase was absent,
or the actual mutex holder/schedule. Those historical facts remain UNKNOWN.
A wait has only been reached after its preceding native operation returned success;
absence of a diagnostic receipt alone is not a demonstrated native-work failure.

## Demonstrated defect and independent controls

The cfg(test) ChildPhases observer recognizes only fixed exact phase tokens and
keeps at most16 ordered records. Its try_lock silently discards a recognized token
on contention. The stderr parser still advances past that line, so it is never
replayed. The exact-order fixture simultaneously locks this queue to inspect it.
Stdout reply completion and asynchronous stderr receipt are independent: the child
writes reply-flushed AFTER stdout flush, and the caller can poll the queue while
that last receipt is being recorded. A permanent missing receipt can therefore
make the unchanged completeness oracle wait until30seconds. This is a reproduced
source defect consistent with the failure, not proof of its historical callsite.

Independent std-only scratch controls extract the exact original observer. On
BOTH Rust1.94.0 and1.98.0, holding its actual mutex while submitting9 valid tokens
deterministically loses9/9; uncontended exact9 succeed. The candidate differs only
in try_lock→lock: an actual WouldBlock probe confirms contention, no completion is
observed during the held100ms control horizon, release permits exact9, then clear
permits exact5. Completion is bounded by the scratch2s control, not a new native
budget. These are synchronization controls, not Windows runtime qualification.

## Holder audit and selected one-line repair

Separate development changes ONLY ChildPhases::observe queue acquisition from
self.0.try_lock() to self.0.lock() in
crates/locron-core/src/windows/filesystem_worker.rs. Preserve its if-let-Ok
poison handling, fixed parser, policy atomic, timestamping and bounded ring.
ChildPhases::attach and Diagnostic's separate best-effort queues remain unchanged.
No new API/mode/process/thread/test/selector, alternate parser, retry or diagnostics.

Every holder was independently inventoried: observe performs at most16 pop/push;
attach visits at most16 entries and calls record_at's bounded queues/nonblocking
locks, with NO emit/I/O; fixture readers compare at most16 names then release before
sleep; clear statements release before real DirectoryGuard/create calls. Remaining
unit holders perform data-only assertions/timestamp changes. No successful path
holds this mutex across await/sleep/native I/O/reentrant observe; Diagnostic never
acquires it, so no inverse lock-order cycle was found. Dispatcher and fixture caller
are separate threads. Poison is not recovered/adopted as valid phase evidence.

A std mutex does NOT provide a timed/preemptible scheduler guarantee. The repair
relies on these bounded critical sections; it does not claim constant wall time.
Existing mismatch assertions may hold the guard until panic unwinds; this existing
failed-test path is not reclassified as success. Existing exact9 bootstrap/exact5
directory/exact5 file ordering, same real child/PID, unknown-token/CR/privacy and
16-ring semantics,30s operation/90s outer deadlines and ownership/cleanup stay exact.
The changed recorder exists only in cfg(test); all production code bytes stay exact.

## Ordered work and concrete Verify

1. Parent records actual failure, research, this plan and Implementation; publishes
   WHOLE final plan to all4 owners, exact-GET verifies and rereads after independent
   decision review BEFORE separate source GO.
   **Verify:** SPEC frozen; unknown historical wait/phase/holder explicit; exact
   extracted controls and complete holder/reentrancy/poison audit support the narrow
   selection; no source edit or unchanged-head rerun first.
2. Separate contention_development owns ONLY the one observe acquisition line.
   **Verify:** complete-file inverse restores57e4; cfg(test) scope and every other
   byte, attach/Diagnostic/parser/tokens/caps/9+5+5 assertion/clock/owner/cleanup
   unchanged; original148 native tests remain, no added mirrored repository tests.
3. Parent and independent shorten_transactions review complete source and reports.
   **Verify:** fmt1.94/1.98, affected Core test type/strict checks as available,
   diffcheck/whole-file inverse; executed scratch original-loss/candidate-preservation
   controls retain exact provenance. Cross/portable checks never qualify Windows.
4. Publish ONE genuinely changed composed head and ordinary full CI.
   **Verify:** same-head/tree all17 actual jobs PASS within original native35/
   Unix30minutes; all3 cold Core148 include unchanged real phase-order fixture;
   one full B Engine75/Server75/Store134+3ignored and PR144195/39cleanup/80documents
   per original native host, actual original Acontroller39/AStore26+absent13,
   all3 prune88/full bindings, all24 CLI/new readiness/capture, native output91,
   full service/GUI/lifecycle/maintenance/MCP/HTTP and actual final original binding/
   fresh tested GLOBAL Core/source fullIDs/all roles/copyhash/manifest/native upload.
   No old-PASS transfer or prepare-only/copied-raw-capture shortcut.
5. Record exact results in PR/Issues; investigate any new failure before another
   source decision.
   **Verify:** pending/cancelled/failed are not passed, remaining delivery/Issue162
   criteria stay open; no timeout increase, test omission, ordering relaxation,
   replay, merge/release/installed binary/service/production state change.

Root owns planning/review/commits/publication; separate development owns source.
A future failure may expose a different mechanism; record its real evidence and
amend the plan before another change rather than claiming this race was proven
as every historical timeout's cause.

## Primary references and retained controls

- https://doc.rust-lang.org/std/sync/struct.Mutex.html#method.try_lock
- https://doc.rust-lang.org/std/sync/struct.Mutex.html#method.lock
- https://docs.rs/tokio/1.53.1/tokio/sync/struct.Mutex.html#which-kind-of-mutex-should-you-use
- https://doc.rust-lang.org/std/thread/fn.sleep.html

Exact independent report/control/extraction receipts:
windows-foundation-phase7-worker-phase-research/REVIEW.md and its execution receipt;
completed MSRV log and separate primary holder audit/proposal under the owned
scratch workspace. These paths are handoff hints, not runtime dependencies.
