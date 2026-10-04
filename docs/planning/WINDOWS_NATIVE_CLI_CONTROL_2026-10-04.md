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
