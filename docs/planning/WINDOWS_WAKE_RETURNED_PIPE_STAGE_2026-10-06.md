# Windows wake returned pipe stages (2026-10-06)

This plan selects a bounded debug observation of already-returned errors at exact
`ef9693e3d05c959abc5893b6e9f3e1dca377bea5`, tree
`04ba96fccdd7b17290f64a5ba334e4c0ce4a61e8`. Frozen SPEC and ARCHITECTURE remain
unchanged. The existing native SID/cleanup plan and every historical result stay
intact. No pipe lifecycle or Wake correctness repair is selected.

## Evidence and the unresolved boundary

Root reviewed the complete immutable packet
`coord/windows-pipe-lifecycle-readonly-research-ef969-20261006`:
report SHA256 `d46e46a800ad19f80387b13563c88e404a9d0e5db232cfd74b2e8a1d29464d06`,
proposal SHA256 `b05e8ec4127751a3744e341f479795affb23fed7fdee9b9ab2bfd4be0a862164`.
Locked interprocess2.4.4 synchronous and Tokio listeners allocate a replacement
before handing out the connected instance. Engine uses maximum2 and terminates
on a non-WouldBlock accept error. If one previous instance is still counted,
that instance plus the connected instance plus replacement require3. This is a
conditional source model; actual instance accounting and the fatal return have
not been observed. Raising a finite cap does not establish a general bound.

The same verified root/full identity derives helper probe, production wake bind
and Run hint as role `wake` with lifetime None. Role-control and launchgate use
other names. The probe retains its authenticated client and metadata duplicate
through actual daemon reap. ACK consumption and the client's fixed0xff write do
not establish that the producer consumed that receipt or subsequently admitted
the durable run. No cross-process-relative timestamp comparison or absence of
all other attempt bindings is justified by recorded independent snapshots.

Pinned interprocess hides the owned Tokio server needed for safe disconnect/reuse.
Tokio1.53.1 provides a safe disconnect on a typed NamedPipeServer, but its safe
creation uses default security and its raw construction/protected creation require
unsafe APIs. Workspace unsafe_code=forbid applies. Pinned mio1.2.2 may retain
native completion ownership after a wrapper Drop. assume_flushed/evade_limbo,
poll_shutdown and cancellation requests are not native closure proofs. Neither
unlimited capacity, unsafe casts/default DACL, flush workers, direct reuse,
early fixture release, new retries nor delays are selected.

Root's retained current-result qualification reports ordinary
CI37372878811/attempt1 completed FAILED: ten required jobs succeeded and seven
were cancelled. Five Linux jobs stopped before steps with provider-assignment
errors during GitHub Actions' reported Major Outage. Two Windows jobs have
35-minute timeout annotations: ARM stopped at doctor; MSRV has Wake step metadata
FAILURE and stopped at service. Cancelled-job direct GET1 attempts returned404;
the documented attempt ZIP acquired once contains only the ten successful-job
logs. The cancelled jobs' actual Wake/accept cause and unprinted outcomes remain
UNKNOWN. No old3905 or other-head PASS/count transfers to ef969.

Current paired37372878741/attempt1 actually passes21 methods/49 controls on each
native x64/ARM row and16 methods/42 portable controls, zero skips. Root completed
the four current ZIP Source/PE/provenance/coherence inspection at22:10:20Z and
reports336 named package cases passed in total. These scoped results do not make
all CI, main, installation or release successful. All20 PRs remain open; no merge
or closure is authorized here.

## Five future Source paths and exact returned-result ownership

Future separate development may change ONLY:

| Source path | Selected observation |
| --- | --- |
| `crates/locron-core/src/notification.rs` | Windows debug-only closed stage scalar accompanying the SAME returned io::Result; no tracing dependency. |
| `crates/locron-engine/src/ipc.rs` | Actual terminal non-WouldBlock accept Err for wake/None Action::Wake only. |
| `crates/locron-cli/src/main.rs` | Forward the same Result and optional returned stage once through the existing debug hint path. |
| `crates/locron-cli/src/windows_wake_diagnostics.rs` | Strict failure-tag/role/error grammar in the existing private writer. |
| `crates/locron-cli/tests/support/windows_cli_control.rs` | Strict matching decoder and separately bounded closed failure summaries. |

At Core49-70 preserve the entire normal public send_wake signature/body, every
Unix path and release entry behavior. Explicitly add a cfg(all(windows,
debug_assertions)) crate-crossing observed entry; this is a new debug API, not a
claim that a private bridge already exists. Its result is the original owned
io::Result<()> plus an optional closed stage scalar. Success has no failure stage.
Endpoint derivation executes once; an actual returned derivation error is
pipe_name. Share the existing private executor/native calls with the normal path;
do not duplicate or replay a diagnostic version of the operation.

At Core307-355 carry only that scalar with the original Result through the SAME
existing worker result/join. Preserve original spawn/runtime/deadline checks,
ClientOptions flags, call order, native resources and error precedence. The same
io::Error returns unchanged; no rewrapping, clone, Display/Debug extraction or
second operation is needed to observe it. The actual terminal open Err at335 is
pipe_open; the actual returned exchange_frame Err at338 is pipe_exchange. The
raw231 branch retains its existing5ms continuation and emits no terminal open
failure for that intermediate busy result. Actual returned worker-admission,
runtime, join or outer-timeout/gate failures are pipe_infra; pending/last-entered
operations never supply that classification. Existing exchange_frame deadline
errors remain exchange returns. Keep the original default200ms or forwarded
absolute deadline, without reset, new worker/channel/pool or unfinished join.
Private factoring is limited to sharing those original calls/Results and must
prove release instrumentation erasure; normal/release public API stays literal.

At Engine159-161 observe only the actual terminal Err in the existing accept
branch, before its unchanged warning and break, gated to debug Windows and the
actual role wake/lifetime None/Action::Wake. Tag pipe_accept; preserve the same
borrowed error's closed kind/raw scalar. Do not tag role-control/launchgate,
WouldBlock, conversion refusal or frame/receipt timeout as pipe_accept. The
interprocess accept operation combines connect and replacement creation, so this
tag proves neither replacement failure, quota exhaustion nor a specific API.
Protected descriptor/local-only/instance choices, maximum2, original listener/
accepted-client retention, ACK/0xff/notify order and all branches remain.

At CLI4052-4065 only the Windows debug branch calls the observed Core entry once.
Forward its returned stage only if the SAME Result is Err, then preserve the old
hint summary, durable-enqueue outcome and unchanged final error/warning behavior.
The normal/release branch stays literal. Observations may record a late actual
return; they never turn it into timely success or change the original first Code.

## Closed carrier, decoder and failure lines

Use the existing private `locron::windows_wake_diagnostics` target and v2
context/actual-process binding, with ordinary fmt exclusion unchanged. Permit
only Run pipe_name/pipe_open/pipe_exchange/pipe_infra and Daemon pipe_accept.
Every new record is edge=err, value=None, attempt=0, a closed existing ErrorKind
bucket (including unknown fallback, never none) and optional signed i32 raw code.
Refuse pipe_accept/WouldBlock and pipe_open/raw231 as excluded continuation
domains; neither is the selected terminal return. No new fallback is implied.
No enter/ok record, success claim, extra field or raw error text is selected.
Keep all original operation indices, limit tag, attempt UUID/ordinal binding and
seven attempt-operation classifications; the five new tags are NOT attempts.
Preserve exact role/context, field-order, duplicate/unknown-field, canonical
number, sequence/time, capacity/fusion and binding-refusal policies in writer
and decoder. Malformed/refused/missing/overflow records remain unobserved facts;
they never mint readiness, new admission, cleanup authority or a guessed cause.
Keep original capture failure classifications; unobserved is not an Ok fallback.

Keep241-byte phase-row,256-record,64-key and64KiB-plus-sentinel limits, original
collector eligibility/order and every actual full-ID/file/child/guard owner.
The unchanged private layer's existing clock sampling/output remains charged to
original clocks; do not claim literally zero OS/output calls or added time.
No extra process/FS/security/IPC query, capture/read, timer, worker, wait or join.
Preserve every old CaseResult, first/completion fact and literal summary formatter.
Retain the latest validated new error per producer as a separate scalar only.
At the existing failure report point, after its existing capture/wait eligibility,
append at most one separate line per Run/Daemon; do not collect again on failure.
The line is `wake_pipe_stage/v1 role=<role> lane=cleanup_diagnostic stage=<tag>
seq=<sequence> kind=<bucket> raw=<None|Some(i32)>` on ONE physical line. If missing
or invalid, use only `wake_pipe_stage/v1 role=<role> lane=cleanup_diagnostic
fact=unobserved` on ONE line. No context/PID/SID/path/name/UUID/digest/full identity,
SQL/argv/environment/input, arbitrary stderr or error message is exposed.
Independent Run/Daemon records are not same-instruction causal facts.

The pure component-width model uses context32, role6, sequence3, tick3, attempt0,
new op13, bucket22, optional raw i32 minimum and u64 time20. Its conservative
cross-role row maximum is211 ASCII bytes includingCRLF, below241; exact allowed
role/op-pair and separate summary bounds are archived with this Docs handoff.
Each separate summary line must remain below256 includingCRLF, two below512.
These are symbolic bounds, not executed native serializer/decoder or observed
kernel-count proofs. Preserve old bounds and add closed grammar controls in the
existing decoder control path: all five valid tag/role pairs, opposite-role
refusal, nonerr/value/nonzero-attempt refusal, absent/duplicate/extra fields,
raw i32 endpoints/overflow, context mismatch and capacity refusal. Old control
bodies/selectors stay literal; these byte controls give ZERO native endpoint/
lifecycle/cause acceptance.

## Concrete ordered Verify and remaining acceptance

1. Root reviews/commits only these four Docs before Source. **Verify:** native
   plan's complete639-line prefix and all other original Doc prefixes are exact;
   ef969 Source/modes/blobs/physical bytes and frozen SPEC/ARCHITECTURE unchanged.
   Owning#27/#31/#35/#163/#171/#166/#167 receive the WHOLE amended native plan AND
   complete new returned-stage plan/Verify by exact POST/GET, retaining bodies,
   states and history. AFTER ALL GETs Root actually rereads both complete committed
   plans and every new Docs block before a separate five-path Source handoff.
2. Separate development implements only the selected observation. **Verify:**
   full staged/unstaged/committed delta and whole-file inverse/call-policy ledger
   for five paths; every other Docs-parent mode/blob/physical byte, normal/release
   public send_wake, Unix body, old selectors/assertions/clocks/native ownership
   and formatter strings exact. Actual bridge uses one operation/owned Result;
   no Core tracing/Cargo/public-release change. Text/domain models and existing
   grammar controls establish exact tag/role/error/attempt/bounds; release cfg
   erasure removes stages/emission with original calls/results. Locally only
   permitted static formatting/locked offline metadata/data proof; type/native
   fit and all actual controls remain hosted, NOT RUN on the owner PC.
3. Root reviews/publishes one genuinely changed ordinary head. **Verify:** exact
   head/base/ref, synthetic ordered parents and whole-tree binding; acquire each
   completed log/artifact once. Fresh strict fmt/warnings-denied lint, all three
   original8-selector native rows and queued/cancel positive controls, Core/stock/
   Engine/Server/Store/doctor/service/GUI/lifecycle/downstream/Unix/package/Guardian
   gates as actually triggered. Same real authenticated wake endpoint/retained
   probe peer and metadata, strict PID/ACK/receipt and original durable succeeded
   oracle; no early release, retry, warm-up, new readiness or clock growth. Returned
   stage may be absent if failure does not recur: missing is not a passing cause.
4. Root records actual scope before any repair or closure. **Verify:** failures,
   cancellations, skipped/absent/unrun gates and all old evidence remain explicit;
   paired/snapshot/ZIP/provenance count only this actually qualified head. No old
   PASS transfer, numeric fix, direct reuse, deadline/oracle waiver or release/
   clean-account/install/WinGet/logon/reboot acceptance. Each member still needs
   its own required Verify and proved main contribution before merge/closure.
   A concrete repair or different stage/owner contract returns to Docs first.

## Primary references and version limits

Pinned cached interprocess2.4.4/Tokio1.53.1/mio1.2.2 Source is bound in the reviewed
packet. Microsoft documents [maximum same-name instances and handle lifetime](https://learn.microsoft.com/en-us/windows/win32/api/namedpipeapi/nf-namedpipeapi-createnamedpipew),
[disconnect/reconnect semantics](https://learn.microsoft.com/en-us/windows/win32/api/namedpipeapi/nf-namedpipeapi-disconnectnamedpipe),
[completion-aware fixed-instance overlapped serving](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-server-using-overlapped-i-o),
and [cancellation request versus completed IO](https://learn.microsoft.com/en-us/windows/win32/api/ioapiset/nf-ioapiset-cancelioex).
These primary semantics do not reveal the missing CI fatal return or prove a
safe adapter/type/native implementation. Tokio latest1.53.2 documentation is
comparison only; the inaccessible exact1.53.1 webpage is not claimed read.
