# Synchronize bounded native fixtures and serialize independent prune setup

Owner #172/#31; related #181. Base684304e, CI38983914154,
MSRV job117250826731. Continue the frozen product SPEC. This is a test scheduling
and setup correction, not a product timeout or support-policy change.

## Selected changes and evidence

All3 independent windows_prune_tests overlap in a single test process and fail
before their pruning assertions. Each real DB's one WAL attempt returns mode=wal
and DONE without BUSY/retry/SQLite error, but takes approximately6–7s inside
query_one, mostly after ROW, and the original5s check correctly refuses. Roots
and DBs are distinct. The exact delayed OS work is unknown. The command currently
omits the serial worker setting used by the complete native libraries. Add only
--test-threads=1 to this existing three-case command. Preserve each case and
subfixture, owned-state/privacy assertions and5s allowance. No success is claimed
until actual native execution completes on all original hosts.

The unchanged supervisor test previously expires before reaching one of four
selected gates. Native setup/scheduling has no100ms guarantee. For a test of
in-flight I/O, establish actual gate entry first under the original STOP_TIMEOUT
setup ceiling, then shorten the SAME request's shared Admission deadline using
stop_at(min(original setup deadline, now+100ms)). Use its existing real channel
receipt and production Budget::await_response, retaining the original250ms return
assertion. Never replace the request, reset/extend its deadline or synthesize an
entered flag. The worker remains withheld during timeout and quarantine.

Keep all4 boundary iterations and every original activation-lock, live-heartbeat,
no cooperative stop, no later Start, exact attempt count and released-worker
cleanup check. Add an actual already-expired Worker::request control proving
refusal before dispatch while the gate remains unentered. Existing late-ready
response coverage continues to test caller-deadline refusal. Setup itself must
fail boundedly if the selected gate is never entered, and use RAII gate release
on every failure. No production request/Admission/Budget code change is selected.

This narrowly supersedes the earlier native-expiry fixture requirement to include
pre-gate scheduling and setup in its100ms withheld-I/O measurement. It preserves
the100ms/250ms in-flight timeout and all product clocks. It also supersedes only
the default parallelism of the3 prune tests; service64/two-workers, Core/libs one
worker, every other selector, matrix, binding, upload and35-minute limit remain.

## Work and Verify

1. Parent reviews source/actual logs, updates Findings/Implementation and this
   plan, updates #172 and rereads the whole selected scope before handoff.
   **Verify:** measured WAL success-after-deadline and unsupported gate premise
   are distinguished from unproven OS causes; no product or budget change.
2. Separate development edits only the one workflow command and cfg(test)
   supervisor control, with no production request refactor.
   **Verify:** all3 prune cases execute serially; all4 real native gates must be
   entered boundedly before the same request deadline is shortened. Original
   100/250ms, quarantine/lease/heartbeat/cleanup oracles and pre-expired refusal
   remain; no fake signal, response replay, deadline extension or extra attempt.
3. Run fmt/strict Clippy and complete original hosted CI on the composed source.
   **Verify:** all native service64 selections and all3 prune cases pass on x64
   stable/MSRV and ARM64, full pending-migration regressions pass, all17 actual
   jobs and final original provenance/upload checks succeed within original
   limits. Earlier failures and other-source results remain separate evidence.
4. Parent reviews final diff, merges the verified PR182 and checks main tree and
   dependency references before closing superseded PR176–179.
   **Verify:** reviewed source matches delivered tree, no open PR is silently
   discarded, and broader Windows release/install acceptance stays separately
   tracked.
