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

## Selected observed-failure continuation before Source (2026-10-04)

Root reviewed completed37172647017, the independent causal packet and the precise observation design before this decision. Only the existing CI workflow and test-private helper are leased; cli.rs/literal Unix bodies and every other Source remain unchanged. The research design below defines the selected fixed domains/slots/renderer and successful-empty transition. Its real-empty boundary controls remain a separate required acceptance gap: no deterministic publication-window mechanism or new selector is selected in this lease. The six existing native cases still run and fail normally; natural absence of an Empty event is never deterministic control proof.

Split the six existing exact cargo commands into six steps in their existing order, each with the identical !cancelled() && steps.native_core.outcome == 'success' condition, pwsh and immediate original LASTEXITCODE failure propagation. All existing steps/commands outside this block remain exact. A canceled run or failed/unexecuted cold Core admits none; successful cold Core admits every one despite an earlier case failure. No continue-on-error, retry, auxiliary acceptance, new timer or selector.

### Concrete Verify and independent handoff

1. Docs/Issues before Source. **Verify:** exact4Docs commit, frozen SPEC/complete Source3a6 unchanged, causal22+stock32 and design4+appendix5 artifacts verified; Issue31 CLI exact readback and final Root review, then separate Source developer.
2. Complete Source conservation and finite observation. **Verify:** only CI/helper changes, unchanged cli.rs/Unix/production/deps/private-state and every other296 blob/mode against reviewed Docs head. Preserve original calls/order/5s/8s/200ms/30s and cleanup.and(work), ignored kill, owner-only worker/channels/seals/refusal. Checked records/closed operation and role domains, signedi32 optional values and reserved bits obey the exact contract below; zero remains unobserved, first work survives cleanup, independent snapshots are not same-instruction evidence. Full CaseResult rendering stays<=579 includingCRLF and<=768; no private/error-text/status fabrication or new IO. Only successful0-byte ready read joins the existing5ms loop, exact1ready/otherdata/errors original; actual empty controls remain explicitly unqualified.
3. Six-step conservation. **Verify:** exact same six selectors, order, each condition/shell/exit handling and all surrounding workflow bytes. No skip/error waiver/new dependency/clock or auxiliary success. Root reviews complete changed Source/rules and 1.94/1.98fmt/actionlint/locked offline graph; static checks add zero native acceptance.
4. One combined changed-head ordinary native qualification. **Verify:** actual one-match result for each six selectors on x64stable/ARM64stable/x64Rust1.94, exact checkout/tree, raw log and paired hashes. Preserve secured real-daemon positive peer, strict actual PID/direction/frame/ACK rejection and original idle/withheld cleanup, genuine same-run Running+two counters -> durableCancelled -> stable progress+daemonlive and timelyreap/rootremoval. Any status/operation/late/unknown failure remainsFAIL; wider Issue31/release and unimplemented real empty-window controls stay open.

### Fixed observation contract retained from independent design

#### Empty publication and finite observations

Source stays immutable/clean at `3a6efccb8b1b10989d544ca4210cb669e45a7ade`.
Root selects this contract through the ordered Docs/Issue/final-review handoff above. It does not establish the measured cause of the MSRV failure.

## Empty ready file

Keep `setup_peer`'s existing actual-child liveness and clock checks, private
no-follow open, original successful read limited to two bytes, and post-read
gate. On **successful** read only:

| Bytes observed | Meaning / action |
| --- | --- |
| length 0 | Unpublished; take the existing 5ms loop pause, then the next original iteration |
| exact single byte `1` | Published; exit setup loop, then perform the original actual same-connection PID/query/ACK test |
| any other 1 or 2 bytes | Original Native failure immediately |

An open NotFound keeps its existing loop behavior. Every other open/read error
keeps its original Native failure, including any sharing/permission/read error.
No error is newly retried; no empty file is accepted as ready. A closed/expired
admission still fails at the existing gates. Each continuing iteration checks
the retained actual Child again before opening. A child exiting during the
existing 5ms pause is detected at the next iteration's original try_wait.

The existing pause already bounds itself by the one original outer30s horizon.
The Empty branch joins exactly the existing NotFound continuation, so this adds
no new native operation, rename, filesystem write, sleep call, clock origin or
backoff. Repeated empty reads remain bounded by the original horizon. The ready
file is merely auxiliary setup input: exact `1` is not daemon/pipe trust, and
the later actual peer PID, direction and strict ACK requirements stay intact.
The negative default-DACL peer remains deliberately untrusted.

This eliminates a demonstrated Source race path: CreateNew exposes an empty
final pathname before the auxiliary child writes/flushed `1`. It does **not**
establish that the actual MSRV Native/Setup failure read empty bytes. A new
observed non-NotFound native error would still fail and require a separate cause.

Conservation target: only the Empty result transitions to the original loop;
expected-byte, unexpected-byte, read/open error, liveness, expiry, ownership,
cleanup and every downstream assertion retain their original policy.

## Observation slots

Use fixed scalar slots in the existing Control, initialized to zero:

1. `last_intent: AtomicU8`, values 0 (unobserved) or one selected operation1..52.
2. `last_io_error: AtomicU64`, one last returned **actual io::Error** event.
3. Three fixed `last_exit_status` AtomicU64 slots: actual daemon, negative peer,
   actual control CLI. Each word also contains its producing operation/role.
   This prevents cleanup of a control CLI from erasing the daemon exit status.
4. `first_work: AtomicU64`, first Code+Phase at work completion, before Cleanup.
5. `last_ready_read: AtomicU64`, successful bounded read classification/length.

There are six u64 slots plus one u8, with no allocation/dynamic map/handle.
All current-intent/error/status/work/ready slots are **independent snapshots**:
do not combine them into a claim that they came from the same instruction or
operation invocation. Repeated operations have no fabricated sequence/time
correlation. A last intent is not by itself proof that a native call is still
running. Each event word is atomic and self-contained, so its own operation,
role and value cannot be torn between two publications.

Zero means unobserved, not success, alive, empty or no error. Records are never
cleared to manufacture a newer absence fact. Only a newly returned corresponding
event replaces its slot. Independent scalar snapshots do not authorize cleanup,
peer trust, a new child, late success or replay.

## Fixed operation/role domain

Operation0 is unobserved. The following 52 operation IDs fit a six-bit field
and the requested at-most64 domain. All values and displayed names are fixed.

| ID | Existing operation |
| --- | --- |
| 1 | StateSetup (existing private factory) |
| 2 | StateGuard |
| 3 | CaseCurrentExe |
| 4 | DaemonSpawn |
| 5 | PeerSpawn |
| 6 | ChildLiveness |
| 7 | ReadyOpen |
| 8 | ReadyRead |
| 9–12 | AddCliSpawn, RunCliSpawn, HistoryCliSpawn, CancelCliSpawn |
| 13–16 | AddCliRead, RunCliRead, HistoryCliRead, CancelCliRead |
| 17–20 | AddCliWait, RunCliWait, HistoryCliWait, CancelCliWait |
| 21–24 | HistoryJson, HistorySelect, SubmitJson, SubmitSelect |
| 25–27 | ProgressOpen, ProgressRead, ProgressValidate |
| 28–30 | RoleMetadataRead, RoleLockProbe, RoleMetadataRepeat |
| 31–37 | EndpointName, RuntimeBuild, PipeOpen, PipeClone, PipeConvert, PipeDirection, PipePid |
| 38–40 | PipeFrameWrite, PipeAckRead, PipeReceiptWrite |
| 41–43 | CleanupTryWait, CleanupKill, CleanupWait |
| 44–50 | DropStdout, DropMetadata, DropQueryHandle, DropClient, DropRuntime, DropGuard, DropState |
| 51 | CleanupStateExists |
| 52 | WorkOutcome |

Role is a fixed three-bit value:0 NoChild,1 Daemon,2 NegativePeer,3 ControlCli.
It is assigned from the actual retained slot/callsite, never supplied PID,
serialized identity or a response. The foreign case's daemon child slot1 still
has Daemon role; its peer child slot0 has NegativePeer role. The enum itself does
not prove a child/status exists; only an actual returned status event does.

Intent updates bracket only original operations; place entry publication after
the original entry gate where one exists and before the original call. Do not
move/add gates, calls, sleeps or clocks to serve the diagnostic. For validation/
JSON/StoreError/non-io conversion failures, intent and the original work Code
can be available while the io event remains unobserved/older. Do not fabricate
an io::ErrorKind from those types or add an unsafe/native error bridge.

## Checked self-contained u64 layout

The common header is identical for every event word:

| Bits | Meaning |
| --- | --- |
| 63 | Valid bit1; all-zero is unpublished |
| 60–62 | Fixed tag1 IO error,2 actual ExitStatus,3 first work,4 ready read |
| 47–59 | Reserved zero |
| 44–46 | Fixed child role0..3 |
| 38–43 | Producing operation1..52 |
| 0–37 | Tag-specific payload below |

IO error payload: bits0–31 are the raw i32 bit pattern, bit32 is raw-present,
bits33–37 are a closed kind bucket1..17. If raw-present is false, raw bits must
be zero. Capture `.kind()` and `.raw_os_error()` from the returned existing
io::Error only; do not call GetLastError or format its message. Closed buckets:
Other1, NotFound2, PermissionDenied3, AlreadyExists4, WouldBlock5, TimedOut6,
Interrupted7, InvalidInput8, InvalidData9, UnexpectedEof10, WriteZero11,
BrokenPipe12, NotConnected13, ConnectionAborted14, ConnectionRefused15,
ConnectionReset16, Unsupported17. All other/non-exhaustive Rust kinds map to
Other; optional raw integer remains exact.

ExitStatus payload: bits0–31 are the exact returned `.code()` i32 bit pattern,
bit32 is code-present; bits33–37 must be zero. Publish only when an original
try_wait returns Ok(Some(actual ExitStatus)) or original wait returns Ok(actual
ExitStatus). Ok(None) produces no status record. A returned status with codeNone
is a valid event with code-present false; it is distinct from no status event.
No new try_wait/wait/query occurs. Preserve negative values losslessly, no u8
truncation. The op/role in each fixed slot must match that actual producer.

First-work payload: bits0–3 are the existing Code discriminant0..12 and bits4–7
the existing Phase0..13; bits8–37 zero. Tag3, op52, role0. Publish once from the
already computed `work` Result and last existing phase **before** changing phase
to Cleanup. Success here means work returned Ok, not overall case/cleanup success.
Panic caught by the existing catch_unwind records Panicked plus the last phase.
An admission failure or driver refusal before work returns leaves this word0.
Cleanup and Drop never overwrite this first result. Overall original
`cleanup.and(work)` precedence and Code remain unchanged.

Ready-read payload: bits0–1 Empty0 / Expected1 / Unexpected2, bits2–3 length0..2,
bits4–37 zero; tag4, op8, role2. Empty requires length0, Expected length1 and the
already tested exact byte1, Unexpected length1 or2. Publish only after a
successful existing read; a failed read with partial bytes publishes only its
actual io error, not a complete readiness event. No content byte is rendered.

Use safe lossless byte conversions for signed i32 packing, checked closed
domains and zero-reserved bits. No unsafe casting, transmute or bridge. A
diagnostic construction/decode problem must not panic, manufacture a value or
change the original returned Code: render fixed `invalid` for an invalid word.

## Exact capture points and unchanged priorities

- current_exe/actual child spawn: capture errors from the existing result. After
  successful spawn retain/anchor the actual child in its reserved slot before
  any dependent metadata publication or subsequent fallible work.
- actual liveness: record status only from the existing try_wait result. Keep
  ChildExited / Native / Expired branch order and original post-return gates.
- ready open/read and progress open/read: capture returned io errors; expected
  NotFound remains expected loop/no-progress input even if last_io_error records
  that numeric event. It must not alter overall work Code.
- original CLI spawn/read/wait: attribute each to Add/Run/History/Cancel role3;
  anchor Child first, retain stdout as originally, preserve status/JSON handling.
- cleanup try_wait/kill/wait: record actual returned io errors/status, then keep
  original policy. In particular an ignored kill error **remains ignored**;
  last_io_error from that call is not the returned cause unless original code
  actually uses it. Reap remains required before handles/state release.
- drop operations: publish intent only. Drop has no returned io::Result/status
  and must never be recorded as native-close or success proof. The existing
  state try_exists result may supply an actual io error; its bool/flags handling
  stays exact.

A returned event can be captured immediately after the existing call returns,
even when the subsequent original gate refuses it as late. It is then an
observed return, **not timely success**. No extra query or native call is made
after expiry; final output is the already selected scalar snapshot. No stderr,
error Debug/Display, PID, path, SID, argv, env, UUID, input or private frame data.

Renderer: retain the old fixed Code/Phase/flags/frame_bytes/elapsed fields, then
append the following exact candidate shapes (no newlines):

```
 intent=<op>
 last_io={op=<op>,role=<role>,kind=<bucket>,raw=<none|i32>}
 daemon_status={op=<op>,role=<role>,code=<none|i32>}
 peer_status={op=<op>,role=<role>,code=<none|i32>}
 cli_status={op=<op>,role=<role>,code=<none|i32>}
 first_work={op=WorkOutcome,role=NoChild,code=<Code>,phase=<Phase>}
 ready={op=ReadyRead,role=NegativePeer,class=<class>,len=<0..2>}
```

Each event may instead be the fixed word `unobserved` or `invalid`; `none` is
only an absent optional value in a valid event. Tag/role validation precedes
rendering; a status in the wrong role slot renders invalid. Each complete event
prints its own fixed operation/role, so no stale neighboring field is assumed
to be its cause. Use maximum u8/u32/u128 decimal widths for the original fields
and the exact signed i32 decimal width for raw/exit values. This finite shape is
bounded well below768 ASCII bytes; `renderer-bound.json` records the exact
conservative domain-bound arithmetic. No dynamic error formatting or arbitrary
string participates. A formatting failure may not waive the actual test/result.
No handles enter records, admission closures, channels, errors or the driver.

## Verification needed before claiming the empty transition

1. Static inverse: all existing operations/call order/SQL-free helper branches,
   original5ms continuation/30s outer/5s wake/8s cancel/200ms probe, selectors,
   Unix bodies, conditions and owner/drop/cleanup priorities remain exact except
   the single successful-empty continuation and metadata observations. Check
   construction/decoding max domains, zero-unpublished, negative raw/code round
   trips, reserved bits and exact bounded ASCII without an implementation mirror
   as the sole proof.
2. Meaningful native controls must exercise a **real actual-owned peer** with a
   visible private zero-byte marker before final `1`: observe Empty while that
   Child remains live, no connection/frame/readiness then, followed by Expected
   and the original real PID/query/ACK behavior. A held-empty/child-exit control
   must still refuse at original30s or actual ChildExited; unexpected1/2byte and
   actual read/open errors must retain original failure/no-frame policy. A
   deterministic publication-window mechanism/new selector is **not selected
   here** and requires a parent-reviewed finite fixture plan before Source; do
   not use sleeps/fake readiness or call a naturally absent Empty observation
   deterministic proof.
3. Changed-head hosted allthree rows run all six independent exact selectors
   despite earlier failures. Record fixed actual operation/error/status/work/
   ready facts with unrelated snapshots explicitly separated. Expected positive
   cancel still requires same Running run +two strict counters -> durable
   Cancelled -> stable counters +daemon live and confirmed timely cleanup.
   Malformed/foreign/idle/withheld assertions and their ownership/clocks remain.
4. Original failures may persist: an observed PermissionDenied/share error or
   daemon startup error is not eligible for empty-read handling. Root reviews a
   separate causal plan if needed. No retry-all, timeout growth, readiness probe,
   production mutation, result waiver or early quarantine release.

The held-empty/negative controls above specify necessary evidence, not Source
mechanisms or authority to expand the currently selected six acceptance cases.
This packet adds no local native execution and leaves Source clean.

## Reviewed ready publication follow-up before Source (2026-10-04)

Ordinary run37184851251 at3f5fb95e9d61477f4e5975f1165af63428411019 fails
required x64stable/ARMstable foundation and Windows lint. Four negative cases
return first_work Native/Setup with actual ReadyOpen raw32 and no successful
ready read. x64 withheld instead reports Disconnected before its controlled
query notice; its underlying work/result is unobserved. MSRV executes all six.
A successful prior run or a neighboring error slot does not establish a cause.

Separate Source research finds an actual publication race path: the private
CreateNew worker exposes the final leaf while its FileStream requests FullControl
(including DELETE). The unchanged no-follow reader shares READ|WRITE without
DELETE, which is incompatible with that still-open handle. Microsoft CreateFileW
documents this refusal. ReadyOpen also performs ancestry/inspection, so the old
logs do not identify the precise internal call or holder. This is a Source defect
and an API-grounded explanation, not an instruction-level causal measurement.

Lease only crates/locron-cli/tests/support/windows_cli_control.rs for this
continuation. The other leased Source file is the two syntax-only stock lint
corrections recorded in WINDOWS_STOCK_CRASH_HEARTBEATS_2026-10-04.md. No workflow,
cli.rs, lifecycle, production, dependency, selector or other Source change.

For the actual negative peer child, replace final-path CreateNew/write with one
owned fixed candidate in its retained existing-private root. Create it privately,
anchor its ownership, write exactly byte1 and flush, close the writer, then invoke
the existing locked tempfile3.27.0 TempPath::persist_noclobber once. Only an
absolute owned candidate may enter TempPath::try_from_path, after its private
CreateNew succeeds. Retain the parent guard through publication. Windows uses
MoveFileExW without REPLACE_EXISTING/COPY fallback; no rename_private replacement,
retry, second publication or adoption of a foreign candidate/final is permitted.
Child pre/post checks use its unchanged original30s. Parent setup and driver
retain their original horizon and gates. Late native completion refuses;
cleanup stays with those same owners.

Add actual controls inside the already selected peer child before exposing its
real final marker. With the staged writer still held, the final no-follow open
must return actual NotFound. After close and publication, that same final must
read exactly1 through the original bounded two-byte reader. A separate owned
pre-existing final and candidate must produce actual AlreadyExists and preserve
the original final bytes when the same no-clobber helper attempts publication.
All control resources stay in the actual child/root and under its original clock;
no sleep window, simulated exception, fresh worker/channel/selector or timer.
Only the later real peer/PID/strict protocol and timely cleanup qualify a case.

Use fixed owned names peer-ready.pending, peer-ready-collision.pending and
peer-ready-collision. The collision final is exact0, candidate exact1; check the
returned candidate path with a named boolean, then both unchanged actual bytes
through the bounded reader and the returned candidate's actual TempPath::close.
Never print path comparison Debug. Run collision controls before publishing the
real ready marker, so no control I/O enters the parent's probe200ms. These are
test-owned private objects, without an arbitrary same-account adversary claim.

This supersedes the final-CreateNew fixture mechanism. Preserve setup_peer's
successful-empty Unpublished compatibility branch and every non-NotFound
open/read refusal verbatim. The historical visible-empty control was never
implemented and remains unqualified; it is not counted as proof of the new
absent-before-completion publication contract.

On failure of the existing withheld notice.recv_timeout only, preserve the
returned RecvTimeoutError and append existing Control scalar/observation snapshots.
A single nonblocking result.try_recv may expose an already queued CaseResult;
Empty/Disconnected render completed result=unobserved. Do not wait, perform
native/Child/Store/path I/O, reopen admission or translate missing data into
success. ReleaseGate unwind and all original later assertions remain exact.
Only fixed enum/numeric/bool formatting is allowed; no private path/PID/error
message/frame. Each snapshot remains independent of neighboring events.
The driver snapshot reports outcome=unobserved and reads only the existing
phase, flags, frame_bytes, elapsed_us and ObservationSnapshot scalars directly.
Do not construct a CaseResult with a synthetic Native or other work outcome.
Only an actually queued completed CaseResult may render its real code/outcome.
Record this refinement and owning Issue31 readback before changing that Source.
The failure line combines at most two existing bounded CaseResult renderings
and fixed notice/queued-state words; require a conservative2048 ASCII-byte bound.

### Four concrete Verify steps

1. Docs and Issue31 before Source. Verify: these decisions and observed failures,
   frozen SPEC, complete protected inventory at integrated2e6c0cd, exact Issue31
   four-step readback and final Root plan review precede separate development.
2. Two-file implementation conservation. Verify: only this helper and the two
   stock syntax edits; every other Docs-head blob/mode exact. Actual staged
   NotFound -> closed complete1, same-helper AlreadyExists with original bytes,
   single no-clobber publication/owned failure path, unchanged guarded reader and
   all six selectors, Unix bodies,5s/8s/200ms/30s,1ms/5ms polling, grace5s,
   admission/seals/first-work/cleanup precedence and all ownership obligations.
   Withheld failure retains its actual error; queued-or-unobserved metadata adds
   no native operation, delay or acceptance.
3. Complete Root review and static checks. Verify: all original PR136 Source and
   docs plus the complete new delta reviewed, fixed bounded diagnostics and
   Rust1.94/1.98 formatting/actionlint/diff/locked graph pass. Existing complete
   PowerShell programs/old snapshot geometry, strict append parser/tail and all
   unleased Source stay byte-identical. Static controls count zero native proof.
4. Fresh exact-head ordinary qualification. Verify: all three original native
   rows execute each six exact one-match cases, actual controls and original
   core/service/GUI/lifecycle/downstream assertions, Windows lint and all required
   contexts; paired ZIP provenance/tree matches. Preserve37184851251 and every
   unmeasured withheld/lifecycle/native/full-lint cause. Missing, late, unknown,
   skipped or failed proof stays failure. No owner-PC native execution or wider
   Windows11/account/reboot/public release acceptance follows.


### Concurrent stock-only publication reconciliation

The parent reviewed local helper commit062aa2 and remote b131: the remote leaves the original native helper, CLI cases, lifecycle observer, stock PowerShell and CI unchanged, but already includes the reviewed main814e689 and a borrowed-slice stock syntax correction. Integrate that remote ancestry normally, preserve the local helper byte-for-byte, and use the remote loader_crash.rs byte-for-byte as specified in the stock plan. Preserve all Docs appendices. This integration does not explain either original native failure. Full locked metadata now succeeds after downloading missing locked packages; this is dependency resolution, not compilation or native qualification.


## Owned CLI output capture after failed73d3 qualification

Refs #29/#31; SPEC and product behavior are frozen. Fresh ordinary37191426679 on73d3a0c7e2418ad57c9eed113a66f950b009749e completed16 SUCCESS/1 FAILURE/2 exploratory SKIP. All four Windows lint commands and17/18 original native CLI cases passed. ARM cancellation returned Expired/History at8467361us, intentHistoryCliRead, lastProgressOpenNotFound2 and earlierHistoryCliWait0. First work/current producer exit/internal read/EOF holder remain unobserved. Later ARM GUI removal of locron.exe failedraw5 with unknown holder. Preserve exact observation5978690173 and its raw hashes; no instruction-level causal explanation is claimed.

Parent actual Source review establishes that Owner::output performs unbounded ChildStdout.read_to_end before actual Child.wait. A blocked producer or another retained pipe writer can keep that helper's native owner outside its checks. Select a test-helper boundary correction, not a production or measured-cause fix. All six original selector assertions, wake5s/cancel8s/probe200ms/child30s/outer30s, post-spawn anchors, cleanup grace, admission/seal semantics, existing guards and no-follow refusals remain exact. Keep progress final creation/strict17-byte records as a separate unchanged question in this slice. No warm-up, loader/worker reset, detached pipe-reader task, deadline extension or unchanged rerun.

Lease only crates/locron-cli/tests/support/windows_cli_control.rs and one additive exact-case step in .github/workflows/ci.yml. Every other Source blob/mode, including cli.rs, lifecycle, stock Rust/PowerShell, production, Cargo files and selectors, stays exact73d3. No dependency/unsafe/platform-policy change.

Each actual CLI invocation owns a new private CreateNew capture under the existing retained private root, using the existing filesystem worker under the original case clock. Use fixed prefix plus checked monotonically increasing counter with MAX512; overflow/collision refuses without replacing or adopting a file. Reserve finite ownership before native work. The Owner retains every original GuardedFile and its DirectoryGuard until cleanup; an active independent reader is anchored in the same Owner. The only stdout Stdio handle is an actual File::try_clone; its shared cursor is never sought/reused by the parent. Retain actual Child immediately after spawn before any later check/allocation. All native operations have original effective pre/post checks and fixed enum/scalar observations.

Poll the actual Child::try_wait under the existing effective absolute case deadline and a short bounded poll interval. Only actual Some(ExitStatus) authorizes capture collection; never synthesize exit, re-spawn a producer or wait on pipe EOF. After real exit, an independent guarded no-follow read at offset0 compares existing public filesystem::file_identity of the original capture and reader. Read at most64KiB+1, reject oversized capture, preserve the full actual bounded stdout and actual status for unchanged JSON/status assertions. No prefix-only success, truncation-as-success or manufactured output. Release an active reader only within the same owner. Expired/non-success/I/O errors still proceed to exact owned Child cleanup; capture/read guards survive until actual child reap. Unknown cleanup stays quarantined. Drops of actual captures precede private-state removal after all exact children are reaped.

Add one independently selected hosted acceptance test windows_cli_control::native_cli_output_capture_contract. Add one fixed auxiliary child selector native_cli_output_target; without its exact private test role it returns immediately like the existing auxiliary selectors. The gate uses at most three sequential fresh admitted Owners, each bounded by the same immutable gate-entry30s outer horizon and admitted only after the previous exact owner is reaped/cleaned; it never reopens a refused owner. It proves: actual locron --version ExitStatus/full independently expected version bytes while a directly owned actual write duplicate remains open; regular-file bounded read does not depend on writer EOF; fresh capture identities/no-clobber collision preserve original bytes; actual auxiliary stdout producer writes and flushes a fixed65,537-byte payload then exits, causing the64KiB+1 cap refusal (harness bytes are not falsely counted as exactly65,537); an actually live auxiliary Child observed by try_wait(None) is refused at spawn-return+1s, then that exact Child is killed/reaped and checked cleanup completes inside the gate30s. The new1s is solely this independent negative control's poll horizon, never any original case's deadline or timer. Auxiliary live work has a finite bound beyond1s; actual exit/cleanup are checked, not inferred from sleeping. Every control returns fixed numeric/code evidence after its assertions, with no private paths/tokens/frame/error messages. The overflow oracle requires actual producer exit0, saved read_count65,537, explicit cap-refusal branch and checked owned cleanup, not a generic error. The64KiB cap is collection refusal, not an OS write quota. Synchronous native/file calls remain bounded by returned pre/post checks rather than a claim of preemption.

1. Docs/Issue before Source. Verify: parent independently reviews complete original/source proposal, all failed-head results and this final selection; Docs commit, exact Issue31 REST readback and final plan reread precede separate development. Historical parallel Issue plans are preserved, not silently substituted.
2. Confined ownership implementation. Verify: only helper and one additive CI command change; whole-file inverse/protected modes restore73d3, actual fresh guarded captures/identity/counter/original Child anchors, no shared seek/pipe EOF and64KiB+1 refusal are reviewed. Every original selector/assertion/clock and production/dependency byte stays exact.
3. Finite native controls and static review. Verify: original six independent commands remain exact; new acceptance/auxiliary selectors execute the same capture helper and actual producers/held writer/collision/live timeout/cap/kill-reap/cleanup, zero fake status/skip. Rust1.94/1.98 formatting, locked offline metadata, actionlint and diff pass. Local static checks count zero native qualification; no owner-PC fixture/compiler/native execution.
4. Exact changed-head qualification. Verify: normal fast-forward push only after fresh remote-head review; all three original native rows pass each original six cases plus additive capture gate, core/service/GUI/lifecycle/downstream and Windows lint; all required ordinary jobs/Guardian/current-main paired ZIP provenance pass. Failed/missing/late/skipped cases remain failure, no unchanged rerun. Record actual fresh causes/unknowns before merge; Issues29/31 and wider account/install/reboot/public acceptance stay open.

## Selected current-call observation handoff (2026-10-04)

Original7343/run37196440237/attempt1 is completed: ordinary executed16 PASS/1 ARM failure; original controls17/18 and capture controls3/3 (nine actual capture markers) pass. ARM first-work is actual Expired/History and cleanup completion is unobserved. Issue31 comment5979427889 retains the measured result. The frozen product/native acceptance scope is unchanged. Parent selects the following immutable139-line research proposal, SHA25699200a284f02fd986724264541d1e3d3fca6c1fccdef3c3fcbc861ff435acb1b, for a separate helper-only developer. The four Docs-first steps below own this next Source choice; speculative producer/performance changes remain unselected.

# PR136 ARM cancellation: read-only handback and finite observation proposal

ARM 실패 원인은 아직 확정할 수 없다. 이번에는 **test-private helper의 scalar 관측만** 선택한다. 원래 동작을 수정하지 않고 current CLI attempt, 실제 native return, 마지막 성공 History/progress를 분리해 다음 changed-head CI에서 원인을 측정한다. 이 문서는 구현이 아니다.

## 고정 근거와 실제 결과

- Candidate `7343fe8417b44d89fc22efde7b6853fe7cfda001`, tree `844cdfd4a6c24bc2e8bf00cd01a224095bb4b270`; current-main `c4ef8b9` 통합 후의 Source다.
- Helper `crates/locron-cli/tests/support/windows_cli_control.rs`: 2,999 lines, SHA256 `9fdc37e0efdce67c66928631197d247a4d2c0dca5253b9909905fc7034c40f25`.
- Fresh ordinary [run37196440237/attempt1](https://github.com/WhiteKiwi/locron/actions/runs/37196440237): completed/failure, 16 SUCCESS / 1 ARM foundation FAILURE / 2 exploratory SKIP. 원래 six는 17/18 PASS, capture gate는 3/3 PASS다. Capture의 실제 세 subcase marker는 세 native rows에서 9/9 존재한다. 이 counts는 Root가 저장한 `../pr136-7343-completed-case-matrix.json`의 실제 raw lines와 이번 ARM raw를 연결했다. 다른 두 row raw 전체를 이 연구자가 새로 다운로드하지는 않았다.
- ARM job111419207891 raw는 completed 후 성공 다운로드 한 번, 158,973 bytes / 1,674 lines / SHA256 `6db69fd85ef1eca428b201744bb6eaec8b4b7b598ca58de3fe2991bd20a37b84`. 첫 CLI 시도는 terminal-escape 출력 보호로 exit1/빈 stdout이었다. `--allow-escape-sequences` 재호출만 실제 raw를 반환했다. `raw-log-receipt.json`은 CLI read attempts2 / successful raw downloads1을 구분한다. CI rerun은 없었다.
- Job metadata의 compile 및 capture PASS는 기존 candidate Source의 실제 type/runtime 근거다. 이 미구현 proposal의 type/runtime 근거가 아니다. Paired coherence metadata SUCCESS도 ZIP 내부 provenance 확인과 별개다. ARM lifecycle/downstream/artifact steps는 SKIP이며 qualification이 아니다.

ARM raw1118–1126의 관측은 다음과 같다.

```text
code=Expired phase=Cleanup flags=1 frame_bytes=0 elapsed_us=8471067
intent=CleanupKill last_io=unobserved daemon_status=unobserved peer_status=unobserved
cli_status={op=HistoryCliWait,role=ControlCli,code=0}
first_work={op=WorkOutcome,role=NoChild,code=Expired,phase=History}
ready=unobserved stdout_bytes=unobserved cli_live_seen=true
```

`first_work`는 worker가 실제 work Expired/History를 먼저 반환했음을 뜻한다. `phase=Cleanup`과 `CleanupKill`은 그 뒤의 scalar snapshot/intent다. Kill 반환, daemon exit, REAPED 또는 CLEANED는 관측되지 않았다. `flags=1`은 GUARD만 있다. Legacy status/error slots는 독립적인 누적 반환 사실이다. `HistoryCliWait/code0`은 현재 History CLI의 exit일 수도 있고 이전 호출의 exit일 수도 있다. 현재 호출의 stdout read count는 기록되지 않았으며, 현재 capture에서 실제 try_wait(None)은 최소 한 번 관측됐다. Raw32, 불완전17-byte tail, target constructor 반환, cancel 전후 구간 및 현재 CLI 내부 정체 원인은 이 로그에 없다.

이전73d3/run37191426679의 Expired/History8467361us, 마지막 ProgressOpen/NotFound/raw2, 후속 GUI removeEXE/raw5는 별도 역사다. Issue31 comment5978690173 및 `../pr136-73d3-arm-failures.json`의 hashes를 보존한다. Capture selection은 planning fc68c7cbd0f2c4281cf68cf20fbb16fb89f262c3 / Issue31 comment5978764862에서 먼저 선택됐다. 새 capture gate의 ARM raw1290–1295는 actual held-writer/version/collision, actual read_count65537/explicit oversized refusal, actual live None/1s refusal/kill-reap-cleanup의 PASS다. 이 사실이 ARM cancellation 원인 해결을 증명하지 않는다.

## Source 호출과 시계

| 경계 | Exact candidate 위치와 의미 |
| --- | --- |
| 원래 callers | cli.rs1133–1188 wake, 1191–1264 cancel. Windows는 실제 helper 결과의 succeeded assertion을 유지한다. Unix literal bodies는 그대로다. |
| 입장과 원래 anchor | helper1967–2059, spawn_child1049–1097. T0/outer30s는 resource-free admission 전에 태어난다. 실제 daemon.spawn 반환 Instant를 Child anchor 뒤에 원래5s/8s deadline에 사용한다. Setup에 소비한 T0 elapsed를 post-spawn 경과로 바꾸지 않는다. |
| 각 CLI | helper1193–1435: 새 private CreateNew capture → actual clone/Stdio → actual Child anchor → try_wait under original clock → actual Some 후 independent guarded reader/full FileIdentity equality → at most65537 actual bytes. Capture512 ceiling, collision refusal 및 원래 actual status/full output oracle을 유지한다. |
| 실제 History | helper1437–1455: daemon 실제 liveness → actual CLI `--json history` → full actual JSON parse → same run_id 선택 → actual state. 반복마다 새 CLI와 Store::open이 있다. |
| cancel 전후 | helper2096–2176. Same Running run+두 strict counters 뒤 cancel CLI, 같은 run Cancelled 뒤 stable progress+daemonlive가 필요하다. 두 History loops 모두25ms continuation이다. |
| progress reader | helper2178–2220. Actual NotFound만 empty-vector input이다. 다른 open/read error, >64KiB, 불완전17-byte tail, 잘못된 hex/newline, 역행 counter는 원래 Progress refusal이다. |
| 실제 producer | helper2534–2575. Production Engine이 current integration executable의 native_cancel_target을 실제 소유/실행한다. Existing-private guard와 final-path create_private_new writer,25ms/30s/1201-frame ceiling을 유지한다. Helper가 직접 이 target의 Child status를 소유하지 않는다. |
| durable cancellation | main.rs1086–1123 → Store::cancel_with_acknowledgement store.rs1861–1957의 immediate transaction → daemon.rs262–280의200ms 확인 → Windows runner/windows.rs154–200의 원래 cancellation/5s grace/owned Job kill+confirmation → main.rs4363–4425의 durable completion. CLI cancel exit0과 actual Cancelled History는 서로 다른 경계다. |
| 생산자와 Running의 차이 | daemon.rs231–262는 mark_running 뒤 runner를 시작한다. runner.rs223–235는 OutputWriter::create 뒤 Windows runner/OwnedChild spawn으로 간다. Running History만으로 target 진입/constructor 반환/두 counter를 추론할 수 없다. |
| teardown | helper1648–1833. 실제 retained daemon/peer/active CLI Child만 kill/wait하고, 원래 outer30s 안의 reaping 뒤에 guards/captures/state를 해제한다. Unknown은 quarantine다. Driver는 unfinished owner를 join/drop하지 않는다. |

Native helper/Core filesystem calls는 동기식이다. Source의 pre/post check와 bounded driver refusal은 OS call 선점을 뜻하지 않는다. 실제 timestamps도 호출 경계의 관측이며 그 사이 CPU/OS/Store/producer 내부 원인을 측정하지 않는다. Capture, startup, repeated History 및 production grace는 비동기 실행과 일부 겹친다. Source만으로 이 비용들을 단순 합산해 ARM 예산 초과 원인으로 확정하지 않는다.

Core cost 경로는 filesystem.rs149–173/706–707/842 → windows.rs589–677 → filesystem_worker.rs29/303–355/483–528/604–672다. CLI마다 process-local SID/dispatcher cache가 새로 시작될 수 있다. History는 main.rs1125–1165/1247–1249 → store.rs1013–1056/1739–1766 → paths.rs83–96의 실제 state/SQLite 준비를 수행한다. 어느 호출이 실제로 얼마나 소비했는지는 현재 raw에 없다.

Progress의 final-name publication에는 별도 Source race 가능성이 있다. filesystem_worker.ps1:46은 FullControl(DELETE 포함)/shareRW로 final을 생성하고47에서 닫는다. filesystem.rs568–596의 no-follow reader는 shareRW/noDELETE다. 이 양립하지 않는 열린 handle 조합은 Windows sharing refusal 경로다. [CreateFileW sharing 계약](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew), [FileSystemRights 정의](https://learn.microsoft.com/en-us/dotnet/api/system.security.accesscontrol.filesystemrights?view=netframework-4.8.1)가 이를 뒷받침한다. Returned Rust RW writer는 다른 구간이다. `write_all`과 kill은 application record transaction을 보장하지 않으므로 strict-tail 경계도 유지한다. [Rust Write 계약](https://doc.rust-lang.org/std/io/trait.Write.html#method.write_all)에 따른 Source inference이며, 이번 ARM에서 두 경로가 실제 실패했음은 관측하지 못했다. 이 selection에는 producer/publication/parser 변경, raw32 retry, prefix-only 성공을 넣지 않는다.

## 선택할 Source lease와 불변식

Source는 **crates/locron-cli/tests/support/windows_cli_control.rs 한 파일만**이다. CI, cli.rs, private_state, Core/Store/Engine, stock Rust/PowerShell, Cargo/lock/API, frozen SPEC 및 다른 Source는 그대로다. 기존 capture acceptance/auxiliary selectors 및 actual 세 subcases를 재사용한다. 추가 selector/native gate/CLI 호출/fixture file/IPC/query/status call은 없다.

Work Code, admission, original flags, first-work, phase/seal/clock/cleanup precedence는 관측 코드로 바꾸지 않는다. 관측 overflow, invalid domain, missing association 및 inconsistent snapshots는 diagnostic의 overflow/invalid/unobserved만 만든다. 이를 Result<_,Code>로 반환하거나 `Control::refuse/encode/check/publish_*`를 호출해서는 안 된다. 모든 기존 pre/post gates는 원래 위치에 남는다. Original wake5s/cancel8s/probe200ms/target30s/outer30s,1ms/5ms polling,25ms target/History pause,production5s grace,1201frames/64KiB/512 captures 및 six assertions는 그대로다.

Legacy ObservationSnapshot, 그 formatter와 누적 intent/io/status/work/ready slots를 그대로 둔다. 새로운 fixed Snapshot은 CaseResult의 suffix에만 넣는다. Metadata는 fixed enums/numeric/booleans이며 실제 resource를 포함하지 않는다. PID/path/SID/runUUID/token/raw JSON/error text/argv/environment/progress record content를 추가하지 않는다.

## Unique actual attempt와 scoped observations

Capture filename count를 epoch로 사용하지 않는다. Worker-private checked CLI attempt counter를 독립적으로 둔다. `captured_output`에 실제 진입할 때마다1..513의 고유 epoch를 배정한다. 513은 기존512 capture의 다음 refusal attempt도 보이게 한다. 원래 entry gate, CreateNew collision 또는 spawn 전 error로 child가 없어도 새 attempt다. Unexpected514 이후에는 diagnostics만 overflow가 되며 원래 helper가 원래 Code를 반환한다. 파일 이름/counter/ownership은 관측 epoch와 연결하지 않는다.

작은 outer wrapper에서 metadata begin을 수행하고 **원래 captured_output body 전체**를 실행한 다음, 실제 Result를 그대로 저장·반환한다. 이 wrapper는 Code를 번역하지 않는다. 원래 early-return도 actual wrapper-return Code/epoch를 기록한다. Scope 안의 실제 native step entry/return에만 serial을 배정한다. Existing check 뒤/기존 call 또는 call closure 직전에 entry를 기록하고, 실제 반환 및 원래 resource anchor 직후/원래 post-check 전에 return을 기록한다. Child/GuardedFile/reader의 기존 Owner-field anchor보다 먼저 새로운 fallible 작업을 넣지 않는다. File clone의 기존 worker-local 소유권도 이동/변경하지 않는다. 관측은 무할당·infallible scalar 작업이다. 이 step 경계는 기존 Rust API/closure의 경계이며 OS syscall 진입을 증명하지 않는다. 기존 argument/guard lookup이 실패했으면 actual wrapper Err이지 kernel operation 실패로 꾸며서는 안 된다.

새 attempt begin은 current wait/native-entry/native-return/wrapper-return/capture-read slots만0으로 초기화한다. Epoch가 포함된 current-call word를 Release로 마지막에 게시한다. Driver는 Acquire fixed snapshot만 읽으며 spin/retry/wait해서 coherence를 만들어내지 않는다. 중간 reset snapshot이라도 각 word 자체의 epoch/step이 남으므로 오래된 word를 새 호출의 반환으로 해석하지 않는다.

실제 try_wait Result는 None/Some/Err를 구별한다. Some만 실제 ExitStatus.code()의 optional signed i32를 가진다. Some(codeNone)과 None은 다르다. None/Err에는 code를 만들지 않는다. 실제 positive/negative i32 bit pattern을 보존한다. [Rust Child::try_wait](https://doc.rust-lang.org/std/process/struct.Child.html#method.try_wait)의 실제 반환만 사용하고 추가 wait/query는 없다.

Capture bytes는 실제 collect_capture read가 반환한 뒤 저장된 누적 bytes.len만 기록한다. Epoch+actual step serial이 있는 별도 word를 쓰고, 그 Source read에서 실제 공개된 count와만 연관시킨다. Existing `stdout_bytes` prefix는 그대로 독립 legacy snapshot이다. Collision의 수동 preserved-file reader, 버전 제어의 외부 identity checks 및 cleanup은 current CLI native/wait/read/wrapper slots를 덮어쓰지 않는다. Scope를 wrapper actual return 직후 닫는다. Cleanup의 기존 legacy observations 및 모든 actual cleanup assertions는 그대로 실행한다.

## History/progress의 last-completed 의미

Worker-private History serial은 실제 `history()` entry마다 독립적으로 증가한다. 현재 History가 아직 captured_output에 진입하지 못하면 CLI attempt는 미관측이다. 이전 epoch를 새 History에 붙이지 않는다. Source prelude의 고정 History context에는 serial/stage/실제 entry Instant만 있고 resource가 없다. 실제 captured_output에 진입하면 해당 History serial이 그 고유 CLI epoch와 연결된다. History liveness 실패나 parser 실패에서 이전 completed word를 새 결과로 바꾸지 않는다.

`current_cli`는 가장 최근 실제 captured_output attempt의 record다. 다음 History가 prelude에서 거부되어 captured_output에 진입하지 못하면 이전 record를 유지하고 그 새 History의 CLI epoch는 관측되지 않은 것으로 해석한다. Prelude를 새 CLI spawn으로 기록하지 않는다. Worker-private pending History context의 새 serial을 이전 completed/current CLI word에 끼워 넣지 않는다.

`last_history`는 **실제 full CLI JSON을 parse하고 same run_id의 state를 선택한 실제 Ok 결과**에서만 한 atomic word로 게시한다. 이 word에는 History serial/CLI epoch/BeforeCancel 또는 AfterCancel stage/closed actual state/returned time이 함께 있다. Unknown string은 fixed Other이며 원래 caller의 refusal은 그대로다. 다음 CLI/History begin에서 이 word를 지우지 않는다. `last_history.epoch/history != current_cli.epoch/history`이면 이전 completed 사실로 명시한다. 이를 current run-state나 현재 실행 중인 CLI의 반환으로 쓰지 않는다.

`history_cost`는 실제 완료된 History 함수의 entry→actual successful return Duration을 worker에서 측정한 last_us와 sum_us, 해당 History serial을 한 word에 담는다. 각 Duration.as_micros의 실제 정수 합이며 실패/진행 중 History는 포함하지 않는다. CPU 비용이 아니라 실제 wall time이다. State word와 cost word는 독립이며 같은 History serial일 때만 같은 완료 호출로 연결한다. 합/시간 overflow는 diagnostic만 overflow다. 완료되지 않은 현재 output의 entry/wrapper-return timestamps와도 구별한다.

`last_progress`는 actual reader/parser 반환 사실을 한 word에 담는다. 실제 NotFound는 class Missing/count=unobserved다. 성공한 완전 strict parse만 class Valid/count0..3855다. Complete zero와 Missing은 다르다. Malformed/IO/Expired outcome은 class Refused/Expired/count=unobserved로 기록하되 원래 Code/branch/gate를 그대로 반환한다. Reader가 현재 반환하지 않으면 이전 completed word를 남긴다. Reader context의 마지막 CLI epoch/History serial/stage/time도 word에 포함해 새 History/CLI와 차이가 명시되게 한다. 이 epoch는 target producer의 identity/진입/exit가 아니다. Stable progress와 durable Cancelled의 원래 oracle을 대체하지 않는다.

Stage는 fixed0 Other,1 WakeHistory,2 BeforeCancel,3 CancelCommand,4 AfterCancel,5 StoppedProgress다. 원래 control.phase와 별개인 관측 context다. Entry stage만으로 CLI spawn/request commit/cancellation success를 주장하지 않는다. 기존 실제 output/state/counter assertions가 그 사실을 확인한다.

HistoryState는 fixed1 Queued,2 Running,3 Cancelled,4 Succeeded,5 Other다. ProgressClass는1 Missing,2 Valid,3 Refused,4 Expired다. NativeBoundary는0 Entered,1 ReturnedOk,2 ReturnedErr,3 WaitNone,4 WaitSome이다. CurrentWait는1 None,2 Some,3 Err다. 나머지 값은 invalid이며 word0만 unobserved다. Existing Code0..14/Phase0..14/Operation1..61/ChildRole0..3을 보존한다.

## 유한 storage와 clock domains

최대 ten new AtomicU64 words의 payload80bytes, controlled capture proof용 AtomicU8 payload1byte와 worker-private 작은 checked counters/Instant context다. Rust struct padding/sizeof를 주장하지 않는다. Proof bits는 원래 flags와 별개다. Dynamic logs/maps, resources, new threads/timers, global serialization 및 observer cleanup 권한은 없다.

| Word | 하나의 checked word에 담을 값 |
| --- | --- |
| current_cli | epoch10,history10,phase4,stage3,entered_us25,valid63. Epoch1..513. History prelude만으로 이 word를 게시하지 않는다. |
| native_enter/native_return | elapsed_ms15,epoch10,step18,operation6,phase4,role3,boundary/result3,valid63; 나머지 reserved0. 같은 epoch+step만 같은 actual native call이다. |
| current_wait | signed-code32+present1,epoch10,step18,None/Some/Err2,valid63. None/Err의 code bits는0; Some(codeNone)도 구분한다. |
| wrapper_return | actual output-helper Code4,epoch10,history10,phase4,stage3,returned_us25,valid63. Actual Result만 게시한다. |
| capture_read | actual cumulative count17,epoch10,step18,valid63. Bounded0..65537; reader 반환 전/미실행은 word0이다. |
| last_history | actual closed-state3,epoch10,history10,stage3,returned_us25,valid63. Same-run actual successful selection만 게시한다. |
| history_cost | last_duration_us25,sum_completed_us25,history10,valid63. Scalar association만 한다. |
| last_progress | count12,history10,epoch10,returned_us25,stage3,class3,valid63. Missing/refused/expired는 count0 encoded/count=unobserved 표시다. |
| daemon_spawn | 실제 기존 spawn-return Instant의 T0 offset_us+1;0 unobserved. Child를 원래 slot에 anchor한 뒤 기록한다. 원래 deadline 계산은 기존 반환 Instant 그대로다. |

모든 timestamp offset은 기존 Control.entered의 real Instant T0가 origin이다. history_cost의 Duration은 위의 실제 함수 entry→return 경과다. µs는0..30,000,000, ms는 실제 Duration.as_millis의0..30,000이다. µs25/ms15는 관측 정밀도이고 deadline을 반올림/변환하지 않는다. 기존 top-level elapsed_us/u128도 그대로다. Late/unrepresentable duration은 fixed overflow이며30s로 clamp하지 않는다. Diagnostic 시간 변환에 `Control::encode`를 재사용하지 않는다. 원래 case-expiry는 outer와 min될 수 있으므로 거기서8s를 빼 actual spawn time을 꾸며내지 않는다.

Step serial은 각 actual CLI attempt의 실제 scoped operation마다1..262143이다. Reset은 새 고유 epoch와 함께만 일어난다. Fixed-byte read의 positive count는 실제 bytes를 증가시키므로 capture당 최대65537 positive read returns와 EOF/오류 한 번이고, bounded5ms polling도 기존 horizon을 따른다. 어떤 예상 밖 overflow도 original work Code/admission을 바꾸지 않는다. Epoch, History, step, domain/reserved bits, signed conversion 및 time sentinel 모두 checked다. Zero/unobserved, valid empty/None, invalid, overflow를 구별하고 magic sentinel이 정상 word와 겹치지 않음을 증명한다.

각 record는 self-contained다. 서로 다른 atomic words의 snapshot은 여전히 같은 instruction 시점이 아니다. Epoch+step이 같으면 기록된 동일 call의 두 실제 경계로 연결할 수 있지만, latest entered와 다른 last returned만으로 현재 OS call 정체/producer status를 단정하지 않는다. 새로운 words는 ownership/peer trust/EOF/cleanup/admission authority가 아니다.

## 기존 세 actual capture controls의 추가 검증

새 native selector/I/O/call을 넣지 않는다. 기존 full actual producer/status/bytes/collision/held writer/oversize/live/reap/cleanup assertions는 하나도 제거하지 않는다. Version의 기존 두 실제 invocation과 collision/no-child invocation 사이에서만 fixed metadata-only begin checkpoint를 허용한다. 이 checkpoint는 resource나 IO callback을 받지 않고 기존 controlled case에만 붙는다. Original six에는 observer assertion이나 추가 gate를 붙이지 않는다.

새 관측 assertions는 **capture contract의 driver에서** completed scalar records/proof bits를 검사한다. Worker checkpoint는 true/false fixed evidence bits만 저장하고 Result/Code/admission을 바꾸지 않는다. Version에서 먼저 끝난 epoch1/epoch2와 begin-reset의 facts는 실제 기존 output 결과와 새 scalar snapshot을 연결해 fixed bits에 보존한다. Version proof bits1 first-complete,2 second-begin-reset,4 second-complete,8 collision-no-child,16 retained-through-original-control-IO의 mask31을 쓰며 허용되지 않은 bits는 invalid다. Oversized/Live의 post-cleanup 현재 words는 driver가 직접 검사한다. Proof bits는 CaseResult의 scalar field에만 넣고 formatter에는 추가하지 않는다. 이 evidence collector는 새로운 actual operation이나 원래 work Code의 성공/실패를 만드는 oracle가 아니다. 새 gate assertion 실패는 observation qualification의 test failure로 보고 원래 work Code와 구분한다.

1. **Version/held writer/collision:** 첫 actual exit0/full exact version bytes 뒤 epoch1의 Some(actual code0), actual captured count, same-epoch actual native return 및 actual wrapper Success를 검사한다. 두 번째 실제 invocation begin checkpoint에서 epoch2이고 current wait/read/wrapper/native-return이 unobserved임을 검사한다. 첫 legacy actual exit0을 두 번째 producer의 exit로 쓰지 않는다. 두 번째 실제 exit/full bytes 뒤 epoch2/Some0/Success를 검사한다. 기존 third collision attempt는 독립 epoch3, actual StdoutCreate Err/actual wrapper CaptureCollision이며 new current wait/read는 unobserved, 실제 active_cli 없음이다. 뒤의 원래 preserved-byte reader와 cleanup이 epoch3 records를 바꾸지 않음을 검사한다. 원래 collision actual bytes1/held writer/fresh identity/cleanup assertions를 그대로 유지한다.
2. **Oversized:** 원래 actual auxiliary exit0 및 actual count65537/explicit CaptureOversized/checked cleanup을 모두 유지한다. New current wait는 이 actual attempt의 Some0, capture-read는 같은 epoch의 실제65537, wrapper-return은 같은 epoch의 CaptureOversized다. Cleanup 뒤에도 이 마지막 actual work facts가 보존돼야 한다. Generic Err 또는 manufactured count/status는 통과시키지 않는다.
3. **Live:** 원래 actual Child try_wait(None), spawn-return+1s refusal, no output read, same exact kill/reap/CLEANED/outer30s를 모두 유지한다. New wait는 해당 epoch의 actual None이고 code=none다. Capture-read는 unobserved, actual wrapper-return은 Expired다. Cleanup의 actual wait/kill status는 legacy에 남아 기존 oracle을 만족하되 new work wait None을 Some으로 덮어쓰지 않는다. First-work/flags/child ownership/clock 역시 원래 그대로다.

Pure finite domain/bit/sentinel/formatter 검증은 actual native proof로 세지 않는다. 임의 literal words의 independent expected masks로0/valid None/Some(codeNone)/signed min/max/collision/no-child/time0/time boundary/time overflow/epoch513 and overflow514/history serial mismatch/step overflow/reserved bits를 검토한다. Decoder 자체의 결과를 다시 oracle로 쓰거나 synthetic History state를 actual CLI/producer evidence라고 부르지 않는다. 기존 three capture controls는 CLI epoch mechanics를 검증한다. 실제 completed History/progress의 새 관측 및 ARM 내부 원인 판별은 original actual cases의 fresh changed-head execution에 남는다.

## Renderer와 Source inverse

Legacy CaseResult prefix와 ObservationSnapshot formatter는 그대로다. Suffix에서 새 enums는 closed numeric IDs를 쓰고 기존 Source의 OP_NAMES/Phase/ChildRole/Code와 위 stage/state/class 표를 Docs에 연결한다. Whole-record zero/invalid/overflow는 고정 단어다. No arbitrary string/Debug input/error text를 렌더링하지 않는다.

`candidate-renderer-bound.json`은 **미구현 candidate text의 static 산술**이다. Legacy snapshot449, legacy CaseResult618, 새 suffix666, 전체 CaseResult1284, cancellation assertion CRLF1338, withheld queued-result assertion CRLF1963 bytes다. Native time-field `overflow`8자를 포함하고 기존 u128 최대39자리/signed i32 최소11자리도 포함한다. Limit2048까지85bytes가 남는다. Legacy ObservationSnapshot을 확장하면 이 계산은 무효다. Developer/Root가 실제 Rust formatter의 모든 emitted variants 및 whole original assertion contexts로 다시 증명해야 한다. 이것은 runtime/type test PASS가 아니다.

원래 captured_output/history/read_progress bodies는 observation wrapper/lines를 제거하는 inverse로 원문과 정확히 복구한다. All existing calls/check/order/paths/counters/Result priorities/allocations/actual owner anchor positions를 비교한다. Original CaseResult legacy prefix, flags/first-work/publish/seals, lifetime cleanup와 all six/capture assertions를 보존한다. Whole helper는 Root가 다시 읽고, 다른 모든 tracked mode/blob 및 CI/cli.rs/Unix/producer/reader/production/Cargo가 exact protected인지 inventory로 확인한다. 무관한 Source 변경이나 동작 fix는 금지다.

## Docs-first four-step handoff

1. **Root Docs/Issue31 selection. Verify:** failed head/raw/hash/counts/unknowns와 이 observation-only 범위를 planning/WINDOWS_NATIVE_CLI_CONTROL_2026-10-04.md, FINDINGS.md, IMPLEMENTATION.md, ISSUES.md에 append한다. Frozen SPEC/old histories/capture selection/current-main ancestry를 보존한다. Exact Issue31 GET readback과 Root의 최종 complete plan reread 뒤 separate developer에게 helper 한 파일만 lease한다. 이 연구자는 Docs/Issue/Source/refs/memory를 쓰지 않는다.
2. **Helper-only developer. Verify:** 독립 attempt ID, scoped actual entry/return/wait/read/wrapper, actual completed History/progress와 clock domains, reset/retention/no-cleanup-overwrite 및 observational-only overflow를 구현한다. Existing full Source inverse와 every other mode/blob proof, original literal clocks/six assertions/capture three actual assertions/ownership 조건을 보존한다. Native/owner-PC 실행과 compiler는 아직 하지 않는다.
3. **Root full Source/static review. Verify:** complete helper/diff/inverse/finite domain/actual-emitted line<=2048CRLF를 읽고 Rust1.94/1.98fmt, locked offline metadata, diff를 확인한다. Workflow byte-identical이므로 new actionlint path는 없다. Compiler/type/strict Clippy 및 native acceptance는 hosted gates에서 판단한다. Static models/metadata를 native 결과로 세지 않는다.
4. **Fresh changed-head ordinary qualification. Verify:** 새 reviewed commit/tree에서 original six+existing capture gate를 x64stable/ARMstable/x64Rust1.94로 모두 실행한다. Epoch/control 추가 assertions와 original oracle이 모두 실제 통과해야 한다. Required lint/core/service/GUI/lifecycle/downstream/paired provenance도 따로 확인한다. ARM이 다시 실패하면 새 coherent returned observations가 보여주는 최소 actual cause만 Docs-first로 다음 선택한다. Unchanged-head rerun, timeout 증가, retry-all/raw32 retry, global serialization, skip/fake-time, producer publication/parser/production 변경을 이 lease에 넣지 않는다. Failed/missing/late/unknown proof는 여전히 FAILURE다.

## Read coverage와 제한

이 연구자는 immutable helper2,999/2,999 lines와 current planning680/680 lines를 실제 읽었다. Original two CLI caller bodies와 Unix clocks/cleanup1133–1264, CI native block108–150, Core filesystem1–310/550–735 및706–731/813–846, worker.ps1 전체66lines, 위 production call-map의 정확한 범위를 읽었다. Runner/windows production prefix1–293 및 windows_child production1–263, daemon205–311, Store1731–1817/1844–1980/1010–1108, CLI main의 위 명시된 범위, filesystem_worker1–228/300–423/453–548/590–680, paths50–130 및 windows589–695를 검토했다. 나머지 copied files의 전체가 다 reviewed라고 주장하지 않는다. Full copies와 hashes는 source-inventory.json/production-snapshot-inventory.json에 보존했다.

Old failed73d3 receipt, Issue31 exact comments5978690173/5978764862, complete prior output research와 new current case matrix를 읽었다. Initial Issue31의 큰 body는 일부 검색만 했으며 full-body coverage로 세지 않는다. ObsDog no-observe query로 current PR136 capture/peer-race 두 exact blocks를 read-only로 읽었고 no memory writes다. 새 actual raw/REST 상태가 memory의 in_progress보다 최신이다.

이번 작업에서 Source/Docs/refs/fetch/push/CI dispatch/Issue/memory/compiler/native fixture/PowerShell/installed service 실행은 없었다. Private immutable Source copies, read-only REST/raw receipts, call-map 및 이 proposal/static arithmetic만 작성했다. Runtime 원인을 측정한 뒤 필요한 최소 behavior slice는 아직 선택하지 않았다.
