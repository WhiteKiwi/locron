# Windows Wake instance headroom and library harness composition, 2026-10-06

## Selected boundary and evidence

This complete amended plan is based on native `feb1c6a35af87121275da38c7714dacf9e634c2a`.
SPEC and architecture remain frozen. Only the two Source paths listed below may change
in a later, separate development lease; this selection does not authorize Source now.

Ordinary run `37387422638`, attempt1, is FINAL:16 required jobs SUCCESS,1 x64MSRV
FAILURE and2 optional SKIP. All17 completed original logs were acquired once. The three
native rows execute23 PASS/1 original Wake FAIL across24 selectors: stable x64 and ARM
are8/8; MSRV is7/8. The MSRV Run terminal `pipe_open` raw2 and daemon aggregate
`pipe_accept` raw231 precede the original Expired/History result. Later27947us cleanup,
root absence and post-gated CLEANED do not replace that failure. The failed native
allocation subcall, kernel instance inventory, scheduling and underlying cause are UNKNOWN.

The same MSRV library row has Engine69 PASS, Server42 PASS and Store95 PASS/1 FAIL.
The original Store test `first_logical_schema_seeds_the_platform_path_in_a_new_or_precreated_private_file`
refuses a returned successful one-attempt WAL query at its original5s post-gate;
its query interval is5,834,870us. This is neither an observed SQLite BUSY nor an owner/
security failure. Limiting harness composition below is an unmeasured resource hypothesis,
not a causal production fix or proof of the old default concurrency.

The existing complete native SID plan (670 lines), returned-pipe-stage plan (200 lines),
WAL diagnostic plan and all prior failed/current-head evidence stay byte-exact. Their
previous full-post/GET reviews remain historical by hash. Only this new complete plan
and the three new appendices require the new all-owner gate below. No old PASS transfers.

## Two future Source paths

| Path | Selected change | Protected remainder |
| --- | --- | --- |
| `crates/locron-engine/src/ipc.rs` | One literal finite Wake limit branch plus one real Windows IPC regression. | Every old selector/body, descriptor, public API, accept/conversion/protocol/Notify/control/cleanup call and deadline. |
| `.github/workflows/ci.yml` | Append ` -- --test-threads=2` to the existing Windows three-library test command only. | Conditions/order, all other commands/jobs/targets/features/filters, Unix blocks and inside-test concurrency. |

The production branch selects3 exactly when `role == "wake" && lifetime.is_none()`
and the action is `Action::Wake`; inspect the action by reference. Lifetime/Stop and
all other roles retain2. Keep protected descriptor/current SID/private root/full identity,
first-instance creation, remote refusal, inheritable=false, nonblocking acceptance,
conversion, strict ACK/receipt and original200ms protocol unchanged. Do not factor a
new configuration/public type, add a dependency or alter the sender/helper.

Pinned interprocess2.4.4 replenishes its listener before returning an accepted peer.
Conditionally, one retained prior counted instance + one connected instance + one
replacement requires3 slots. Three is the smallest finite allowance for that inventory;
a duplicate handle is not automatically another instance. Actual raw231 does not prove
this inventory or native allocation instruction. Three is not a general leak, unlimited
client or arbitrary concurrency guarantee. No early drop of the authenticated original
peer, reuse/disconnect redesign, unlimited cap, unsafe/default-DACL construction,
retry, warmup, yielding delay or native allocation telemetry is selected.

Microsoft's [CreateNamedPipeW](https://learn.microsoft.com/en-us/windows/win32/api/namedpipeapi/nf-namedpipeapi-createnamedpipew)
and [DisconnectNamedPipe](https://learn.microsoft.com/en-us/windows/win32/api/namedpipeapi/nf-namedpipeapi-disconnectnamedpipe)
describe last-handle and disconnected-client obligations; they do not measure the
current runner. The pinned listener/handle evidence and the existing
[overlapped-server](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-server-using-overlapped-i-o)
and [CancelIoEx](https://learn.microsoft.com/en-us/windows/win32/api/ioapiset/nf-ioapiset-cancelioex)
references do not authorize a new reusable-server API or cancellation-as-completion.

## One real held-probe regression

Add one Windows-only selector:
`ipc::tests::retained_authenticated_wake_probe_allows_four_handoffs_and_owned_teardown`.
Use the real secured `bind_wake` listener and its production accept/conversion/protocol/
Notify loop. Existing69 Engine cases and all old fixtures/helpers remain literal.
Do not use the old retrying client helper to manufacture readiness.

Retain one explicit test-local owner bundle: the actual TempDir and private-root guard,
Notify, original client, safe duplicate/metadata wrapper, current sequential-client slot
and actual listener JoinHandle. Keep these resources outside the catching future; the
operation borrows them. No catch/panic path may drop a client, root or handle before
listener cleanup. Setup must genuinely admit the private root and successfully bind
that owned listener before the whole5s horizon is born, immediately before first protocol.

The first actual client uses the same guarded wake/None endpoint. Safely duplicate its
real handle with the existing public adapters (`AsHandle::as_handle` followed by
`BorrowedHandle::try_clone_to_owned`, typed `PipeStream<Bytes,Bytes>::try_from`,
client-direction and `server_process_id` query).
Use metadata only for query/disposal, never protocol I/O; consume every wrapper with the
existing `evade_limbo` pattern and retain the original protocol client separately.
Require a nonzero actual server PID equal to the current owned test process; print
neither PID nor path. No self-reported process/endpoint text is identity proof.

Complete the fixed Wake frame, exact ACK and0xff receipt and consume its actual Notify.
Keep both that authenticated client and its metadata duplicate throughout four subsequent
sequential exchanges, using at most one current sequential-client slot. Each fresh
connection must complete the same fixed frame/exact ACK/0xff, consume its own Notify,
leave the actual listener live and refuse a same-endpoint first-instance collision.
Release each sequential sender only after its completed exchange; do not release the
original probe/metadata to make the next handoff pass. There are five actual exchanges
in total, not four including the original. No skipped iteration or fabricated notification.

Before each exchange compute `min(whole absolute5s deadline, exchange-entry +200ms)`.
Include connect, duplicate/query when applicable, frame/ACK/receipt/Notify/collision
and all actual return post-gates within that exchange/remaining horizon. Never renew
5s or200ms after a wait or native return. A timeout does not establish preemption of
synchronous native calls; on-time success requires the original return gates.

Use existing futures-util `catch_unwind` to collect the operation's success, error or
unexpected panic, without exposing the panic payload. Regardless of that outcome,
unconditionally abort the same owned listener and await that same JoinHandle under
one separate1s cleanup horizon. Await it by mutable borrow so expiry does not discard
ownership. Preserve the operation failure before cleanup; late completion or cleanup
success cannot mask an expired exchange. Successful operation is accepted only with
confirmed, on-time listener join and all exchange/PID/ACK/Notify/collision assertions.

After confirmed join, normally release the original probe, metadata, sequential client,
private guard and TempDir. If join fails, expires or is uncertain, retain/quarantine the
single owned bundle (listener/root/clients/metadata) for the test-process lifetime instead
of claiming release or dropping its state. No blocking unfinished join, new global registry,
replay or cleanup-as-success. The same finite bundle remains held on panic failure.

Only after the caught outcome and abort/join attempt may failure output use closed phase,
closed error kind, raw integer and fixed Boolean flags. No Error Debug/Display, descriptor,
SID/path/PID, unknown panic text or private payload. Unexpected panic remains failure;
never accept it or rethrow its unbounded original contents. No new production diagnostic
group, existing field/selector or native-query telemetry is introduced.
This closed-output rule governs the new explicit formatter; catch_unwind does not
suppress an already-installed Rust panic hook. No global hook change is selected.

## Windows library harness only

The existing Windows `Verify durable store and runtime library contracts` invocation
changes from `cargo test -p locron-store -p locron-engine -p locron-server --lib --locked`
to `cargo test -p locron-store -p locron-engine -p locron-server --lib --locked -- --test-threads=2`.
Its existing native-core condition/order is unchanged. Core and isolated stock commands,
service/GUI/CLI gates and all Unix commands stay literal; no global RUST_TEST_THREADS,
job/matrix/resource/profile changes or cargo compilation parallelism change.

This limits ordinary libtest case composition only. Original internally concurrent WAL,
worker, late-owner, admission and cleanup controls keep every thread/body/assertion and
original5s/8s/200ms/30s/25ms/5s-grace oracle. It neither skips a timeout nor makes a failed
result acceptable. Current-host contention and SHA/SQLite/native scheduling costs remain
unmeasured. Genuine changed-head execution is required, not an unchanged-head retry.

## Narrow supersession and required Verify

This selection supersedes only the returned-pipe plan's no-cap-change/max2/observation-only
clauses for the exact Wake branch/additive regression, and prior unspecified/default
Windows three-library harness-composition preservation for that one argv suffix.
All old plans remain untouched; no other exclusion, product contract or gate is superseded.
SPEC/architecture, security/ACL/TokenOwner, original sender/daemon/worker ownership and
all existing cancellation/deadline/counter/cleanup oracles remain frozen.

1. **Finalize this complete selected plan before Source.** Root commits exactly four Docs,
   POSTs the whole new plan to owning25/27/31/35/163/171/166/167, obtains exact full GET
   readbacks for ALL eight with original bodies/state/history retained, then actually
   rereads the entire final committed new plan and new appendices AFTER ALL GETs.
   **Verify:** staged/unstaged review, prefix/physical/inverse/protected ledger, eight
   exact-body readbacks and final full-read record. Old670/200 plans remain frozen by
   their reviewed hashes; do not invent repeated new whole-old-plan reviews.
2. **Hand exactly two Source paths to a separate developer.** Implement only the literal
   Wake3/other2 branch, one owner-safe real regression and one Windows argv suffix.
   **Verify:** whole-file inverse removes exactly that branch/new test/suffix; all old
   bodies/flags/calls/clocks, Unix commands and every other mode/blob are preserved.
   Review caught-resource lifetime, query disposal, actual Notify and failure-only closed
   formatter, unconditional borrowed-handle abort/join and uncertain-retention lanes.
   Permitted static checks are not native/type/Clippy acceptance; no owner-PC execution.
3. **Qualify one genuinely changed head on all three Windows foundation rows.** Require
   x64stable/x64MSRV/ARMstable actual Engine70 (old69 plus this named control), Core137
   and three isolated stock invocations, Store96 and Server42, with all eight original
   native CLI selectors per row (24 total) and all genuine producer/capture controls.
   **Verify:** actual command/toolchain/checkout/tree and named case/results, retained
   probe/five exchanges/Notify/collision/join assertions, no ignored/filtered-away new
   case, original WAL positive/refusal/concurrency controls and complete failed/unreached
   evidence. Old counts/PASS, scalar metadata or later cleanup are not current acceptance.
4. **Retain every required lint/downstream/delivery gate and owning closure criterion.**
   **Verify:** actual current-head fmt/strict lint, service/GUI/lifecycle/dashboard/HTTP,
   MCP/prune/maintenance, Unix/frontend/package/coherence/Guardian and triggered paired/
   snapshot results with exact Source/ordered-tree/package bindings at their real scope.
   Preserve missing/skipped/failed gates explicitly; no signing/Win11/two-user/task/logon/
   reboot/install/public WinGet or release acceptance follows from this bounded change.
   Keep25/27/31/35/163/171/166/167 open until their full Verify and required main/member
   contribution/publication succeed. Root alone owns Git/API/memory/publication.
