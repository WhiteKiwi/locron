# Native wake and cancellation fixtures, before Source

Root-reviewed continuation of Issue31 on main46445881e320f440c02d94d7e9e5838033933fea
(whole-tree equal reviewed PR13502f9868). Frozen SPEC remains unchanged: actual
prompt manual admission and confirmed cancellation are already required behavior.
Source belongs to a separate developer after exact Issue Verify readback.

Research pin e6eb887 and its two old fixture bodies are conserved at this base.
Old495f ARM/MSRV wake fails wake.sock.exists; cancellation reaches its final
cancelled assertion after actual running/cancel success. Current cc full37162098246
ARM/MSRV execute301 units and73/75 CLI cases; the same wake1142/cancel1232 fail.
No logged terminal reason/phase timing establishes a production cause. x64's
unchanged supervisor1687 failure precedes all CLI integrations. Doctor's six
native cases now qualify separately and remain outside this implementation.

## Selected Source boundary

Exactly three Source paths are allowed:

- crates/locron-cli/tests/cli.rs: cfg(windows) helper declaration and only the two
  existing wake/cancel bodies; preserve their names and complete literal Unix
  bodies/assertions. Windows delegates the same actual state/daemon setup and
  post-spawn clock to the one admitted worker under the refinement below.
- crates/locron-cli/tests/support/windows_cli_control.rs: new Windows-only private
  owner, transport/progress helper and explicit real helper/auxiliary selectors.
- .github/workflows/ci.yml: one additive exact native acceptance step in existing
  Windows foundation rows, after runtime/doctor and before existing service units.

No production/public API, private_state helper, other caller/test, manifest/lock/
dependency, installer/update/task or remaining workflow changes. Existing public
CLI Windows interprocess2.4.4 dev edge and Tokio1.53.1 net/io/time APIs suffice;
unsafe_code=forbid and all lint/protection rules remain. Original wake5s,
cancel8s, one entry200ms, finite target30s and production5s grace stay fixed.

## Private owner and witness

Windows test-private entry signatures are run_wake_case()->CaseResult and
run_cancel_case()->CaseResult. Reserve one empty worker before any actual fixture
resource, send only a fixed case command, and create/retain the same real
PrivateState/TempDir/daemon Child on that worker. The literal5s/8s deadlines still
begin immediately after actual daemon spawn; their effective limit is the minimum
with one new outer30s horizon born before admission. This explicitly supersedes
the previous actual-Owner handoff ABI; it adds a tighter setup bound rather than
enlarging operational acceptance. Retain an existing-only private DirectoryGuard,
actual root/role context, runtime/client/query handles and bounded phase result
together. Expected PID is private actual Child.id, never serialized
metadata. No missing-root creation/repair, PID kill, force unlock, worker replay,
replacement/global reset, cache warmup or whole-suite serialization.

Check the original deadline before/after each synchronous native/CLI/Store/file
operation and driver acceptance. The driver never joins an unfinished owner or
drops a runtime waiting for pending native work. On expiry close admission and
retain all exact owners; late output cannot become success. Once native work
returns, only exact original-child emergency kill/wait may continue. Release
guard before TempDir only after actual root reaping; unknown cleanup is failure.
Normal success must include owned cleanup inside the original caller budget.
This is retained ownership and bounded driver refusal, not native-call preemption.

## Actual wake protocol

Root/name/role/held passive daemon lock checks use the same original5s budget;
they do not establish readiness. Derive the endpoint via public guarded naming.
Probe entry precedes cold runtime/open/duplicate/native metadata/frame work;
operation deadline=min(original_deadline, probe_entry+200ms), never renewed after
connection/ACK. Only preconnection NotFound/231 can poll5ms inside this one clock.
After connection or uncertain queued write there is no reconnect/replay/retry.

Use original duplex byte Tokio NamedPipeClient with SECURITY_IDENTIFICATION.
Clone its borrowed native handle through safe try_clone_to_owned, construct
interprocess sync PipeStream<Bytes,Bytes>, require is_client and query the actual
same-connection server_process_id. The duplicate is metadata-only; evade_limbo
on every query result, close constructor-failure's plain OwnedHandle. No second
IOCP registration, protocol I/O/clone/split/default limbo/unsafe/mem::forget.
Keep the original client and child-live brackets around peer/ACK checks.

Require nonzero peer PID==actual owned daemon PID before frame. Original client
writes length15 plus public WAKE_MESSAGE15, reads exact ACK_MESSAGE14 and writes
0xff receipt (31 fixed bytes), each pre/post poll checked. No flush/peek/EOF or
unprobed/metadata-only readiness. ACK proves listener delivery, not job admission.
Then preserve actual CLI add/run/history/succeeded with the Windows cargo-built
locron --help success target; Unix /usr/bin/true stays byte-exact.

## Actual cancellation progress

Keep the original8s post-spawn clock; remove only the Windows socket-file wait.
No extra wake readiness probe. Same actual daemon live + same durable run running
+ two strictly increasing native-target counters must precede CLI cancel.
The real target is the current integration executable invoked by production
Engine OwnedChild with exact windows_cli_control::native_cancel_target, finite
argv, private-root cwd and nonreserved WINDOWS_CLI_CANCEL_ROLE and
WINDOWS_CLI_CANCEL_PROGRESS env.
No shell/compiler/short naturally completing/fake success target.

Target retains existing-private parent and a create_private_new writer, emits
complete16-hex-counter+newline records every25ms, explicit30s original duration
AND1201-frame ceiling (20417 bytes). No inferred atomic writes/count-by-sleep.
Existing-only nofollow reader caps64KiB, validates complete syntax/strictly
increasing counters. Absence or fewer than two complete frames cannot witness
progress; bounded observation may continue only under the same original8s.
Malformed/incomplete-tail/oversize/native errors refuse rather than accepting a
partial prefix. No fresh clock, file authority/ACL repair or replay.

Preserve actual CLI add/run/history/cancel/history and running/cancelled assertions.
Require same run cancelled and stopped progress within remaining original8s.
Production cancellation's existing tree confirmation and default5s grace remain;
insufficient cold-start/cleanup budget is a measured failure, not permission to
enlarge it. Unix literal sleep30 and exact owned-daemon cleanup remain.

## Frozen selectors and helper clocks

Six genuine acceptance selectors (explicit --exact, no namespace-wide filter):

1. wake_socket_makes_new_manual_run_promptly_visible_to_daemon
2. durable_cancel_terminates_a_running_process
3. windows_cli_control::foreign_wake_peer_is_refused_before_frame
4. windows_cli_control::malformed_wake_ack_is_refused
5. windows_cli_control::idle_wake_peer_expires_original_probe_deadline
6. windows_cli_control::withheld_native_return_retains_case_owner_at_deadline

Helper scenarios have one original30s test/cleanup horizon born before their
effects. Actual foreign native peer Child, malformed fixed ACK/version and
idle/incomplete exchange must refuse; owned-daemon positive is the original
wake case. Withhold an actual native query result/return to prove driver expiry
retains child/root/client until controlled release and actual reaping, without
unfinished join/late success. This is a controlled return gate, not an OS-hang
or hard-kernel-termination claim. Each probe still caps original entry200ms.

Auxiliary native_cancel_target and native_wake_peer_target roles are explicitly
selected actual children, finite30s; a default unselected return is ZERO acceptance.
Do not include them in the new CI command list or count them as meaningful full
suite passes. Actual parent observation supplies role execution proof.
CaseResult formatter is fixed enum/numeric/bool <=1KiB; no path, SID/PID/UUID,
token, raw error/argv/environment or private frame in public receipts.

## Four ordered Verify steps

1. Implement private test owner/helper. Verify: same actual Child/state/guard and
   caller clock survive expiry; withheld return proves bounded driver refusal and
   retained ownership, controlled release confirms exact cleanup; no unfinished
   join/reset/replacement, no unknown-owner success or private diagnostic leak.
2. Port only two Windows fixture bodies and qualify protocol/progress. Verify:
   original named selectors/Unix bodies/assertions and5s/8s clocks remain exact;
   wake actual same-peer/ACK plus manual succeeded, cancel actual running+two
   counters then cancelled/stopped progress; actual foreign/badACK/idle refusals
   execute. Preserve30s target/1201frames/64KiB read/200ms probe/5s grace.
3. Add one independent native CI step. Verify: exact six selectors above execute
   individually, each Cargo nonzero exits immediately, same existing
   !cancelled/native_core-success condition; all existing commands/conditions/
   matrix/clocks/protections stay exact. Removing that one block restores main CI.
   No retry/continue-on-error/warning allowance/aux-role count or skip substitute.
4. Review and record actual changed-head evidence. Verify: full three-file Source
   and whole-file/workflow inverses, all other blob/modes/Unix/Cargo/production exact;
   format/actionlint/diff and Root review pass before publication. Fresh ordinary
   x64 stable, ARM stable and x64 Rust1.94 execute all six real cases and original
   required gates, paired artifacts match the exact head/tree. Preserve old full
   logs/unknown causes/41 lint diagnostics; record any cold expiry or unknown
   cleanup without broader Windows11/two-user/public distribution acceptance.

Research reports/candidate ABI are hash-pinned in FINDINGS. Root reviewed them
and the scope exception for the third CI Source path before Issue readback and
separate development. No owner-PC native fixture or policy effect is authorized.

### Selected negative-only native peer construction before Source

Separate checksum-qualified Tokio1.53.1/Mio1.2.2 research resolves the developer's
safe listener construction without a direct widestring edge. The three-path
Source boundary, six real selectors, actual Child/PID query, frame/clock/cleanup
and whole Unix/production/workflow conservation remain exact.

Only native_wake_peer_target's deliberately untrusted helper endpoint uses safe
Tokio ServerOptions::new().pipe_mode(PipeMode::Byte).first_pipe_instance(true)
.reject_remote_clients(true).create(endpoint), within its I/O-enabled retained
runtime. Existing defaults are duplex; safe create passes NULL security attrs.
No raw/unsafe constructor, custom SDDL, new dependency or production listener
change. The filesystem root guard does NOT make this global pipe owner-private.
Default DACL is untrusted adversarial test input: never daemon/security readiness,
two-user/remote authorization or endpoint-name authority. first-instance refusal
is collision protection, not trusted ownership or a timing assumption.

All helper contexts derive expected PID only from an actual retained Child.
Foreign-peer case compares the real connected server PID to the distinct actual
expected child and refuses before frame. Malformed/idle and controlled-return
cases may query their real selected child to exercise transport refusal; no
helper ACK/default DACL can mint production readiness. Only the original owned
daemon wake case uses the production secured listener and qualifies the positive
same-daemon/ACK/manual succeeded contract. No fixture-created pipe replaces that
listener, and no serialized PID is witness.

Tokio connect cancellation safety and Mio Drop/CancelIoEx are not synchronous
native closure proof. Pending writes may remain. Keep actual child, client/server,
lease/root and runtime in the existing finite owner until original-budget cleanup
confirmation; otherwise quarantine/refuse. No default-Drop success, blocking
unfinished join, reconnect/replay, new clock, quiet-period or global serialization.

Apply the existing four Verify steps to this detail: frozen crate/archive graph
and all production security remain exact; actually connect the foreign native
child and refuse its actual mismatched PID before frame; malformed/idle/controlled
return selectors retain exact admission/deadline/cleanup facts; the genuine
secured daemon positive executes; all six real selectors pass in three native
rows after Root full Source/inverse review. No unselected child-role acceptance.

### Resource-free admission and independently observed probe deadline before Source

Separate read-only WIP research pins5db5c2a. The uncommitted draft captured an
actual Owner in thread::spawn: failed OS admission drops that closure on the
driver and Owner::Drop can kill/wait/park there. Reserving an empty thread then
sending Owner is also unsafe: Full/Disconnected returns Owner and a live ACK
cannot prevent receiver death. The draft driver additionally waits only case
5s/8s/helper30s, so blocked synchronous query hides worker-side200ms expiry.
Neither draft has native acceptance; dependent Source stopped before this plan.
Root selects the following private test refinement, retaining frozen SPEC.

Keep exactly the same three Source paths, six genuine selectors, original literal
Unix bodies/assertions and all existing production/private_state/dependencies/
gates. Windows bodies delegate to the no-argument entries selected above. A
private non-Clone CaseAdmission may contain only scalar Control, capacity1 fixed
case-command and fixed CaseResult channels plus JoinHandle. CaseKind Wake/Cancel
carries no path/string/Child/TempDir/guard/runtime/native handle or resource-owning
callback. No generic driver-supplied work closure may capture actual resources.
Auxiliary/helper admission likewise captures only fixed metadata/control channels.

One Instant origin T0 and original outer T0+30s are born BEFORE empty admission.
Safe Builder::spawn captures only resource-free controls/channels. Its OS error,
panic, Full/disconnect or driver unwind drops no actual fixture resource. One
try_send only, capacity1/no sender cloning/rendezvous/retry/second command. Check
post-admission expiry before dispatch. Native thread creation/OS scheduling/std
internals are synchronous and not hard-preemptible; no real-time30s promise is
made for admission itself. There is no actual state/Child/guard on that caller.

The one already-live worker constructs a partial Owner before effects. Use the
unchanged real state factory and exact daemon run/stdout-null/stderr-null spawn.
Anchor each acquired object immediately on that worker BEFORE any fallible check,
allocation/assertion/callback; preallocate bounded child storage before spawning.
After actual daemon.spawn returns, retain the Child and anchor the literal original
5s wake/8s cancel deadline, effective=min(outer, original post-spawn deadline).
Publish case expiry only once. Late setup return anchors its result then refuses;
only exact original-owner emergency cleanup follows, never new work or replay.
Explicit partial branches handle empty owner, state without child and actual
child context without inventing root/cleanup facts. Owner remains outside the
caught operational/setup closure on that worker. Panic cleanup/unknown parking
never moves it to driver; abort/process exit is not confirmed cleanup.

Positive outer30s is a NEW tighter setup bound on previously unbounded setup.
It does not extend original post-spawn5s/8s protocol/progress/cleanup acceptance.
Every native/CLI/file/Store work checks admission/effective clock before/after.
Normal success requires all genuine assertions and confirmed owned cleanup
BEFORE effective5s/8s. After expiry driver closes admission irreversibly and
returns fixed refusal without joining unfinished owner or releasing its resources.
Same-worker emergency cleanup may use only the remaining original outer horizon;
late/uncertain native return/cleanup cannot be success or a fresh work budget.
Guard/TempDir release still requires actual root reaping, unknown retains owner.

Select checked u64 nanosecond offsets from immutable T0, max30,000,000,000ns;
stored offset+1 and0 unpublished. Release publication/Acquire observation.
Case expiry once-published; ONE probe's expiry=min(effective case, entry+200ms)
is published BEFORE cold runtime/open/duplicate/query/frame. Duplicate, invalid,
out-of-bound or inconsistent publication refuses. No rounded-up remaining/new
Instant, renewed deadline, blind slot clear or reopening. Keep once-published
probe expiry and separate on-time completion seal with actual completion offset
after all probe/live-bracket/post-checks return. Zero means incomplete; seal>=
expiry or outside case/outer refuses. Release seal is commit boundary for the
single writer; coherent atomic equivalent is permitted with the same facts.
Metadata seals are not peer/readiness/native closure authority.

Driver polls capacity1 fixed result with at most1ms slices bounded by min(outer,
published case, incomplete probe expiry), checking before/after observation. First
observed expiry permanently closes admission; even a later seal cannot undo it.
An on-time sealed probe may continue the remaining original5s/8s case. This is
deadline refusal after OS scheduling, not hard native preemption. Join only an
already-finished thread after an on-time result, never poll a JoinHandle until
the outer clock as part of200ms refusal. Driver Drop closes scalar admission only.

Negative idle/withheld tests separate first200ms refusal from terminal cleanup.
A metadata-only wait_cleanup_until(original_outer) may observe the SAME worker's
fixed result with admission still closed; it ignores expired probe only to await
cleanup, never to admit work/reconnect or reset a clock/join unfinished thread.
Keep actual child/client/root/query flags and sharing-violation guard proof,
controlled release and actual reap/cleanup. Unknown cleanup fails; the final
expected Expired cannot be late success. Result try_send is nonblocking and
metadata-only, never contains Owner or impedes cleanup.

Apply the existing four Verify steps to this refinement, explicitly checking:
1. Verify all actual owners are acquired/anchored only on the one worker; admission
   closure, command/error/result and driver contain no actual native resources.
   Source static/panic/disconnect/empty/partial/late lanes are explicit; original
   real state factory/daemon args and whole Unix/other blob/modes remain exact.
2. Verify original5s/8s post-spawn min, new one30s outer and once-published200ms
   offset/seal bounds, no blind clear/reopening/rounding/renewal and bounded driver
   observation. Pure static channel/seal checks count zero native acceptance.
3. Verify real foreign/badACK/idle/withheld/positive wake and actual cancel progress
   preserve all original assertions/protocol/security/target1201frames/64KiB/
   production5s grace, first refusal separate from confirmed original-owner cleanup.
4. Verify complete three-file/inverse/rules/fmt/actionlint/locked graph, then fresh
   six-selector native x64/ARM/Rust1.94 plus unchanged required/paired evidence.
   No skip/retry/serialization/new dependency, owner-PC native effect or broader
   Windows11/two-user/full41-lint completion claim. Root Docs/Issue exact readback
   and final plan review precede the resumed separate developer's Source lease.
