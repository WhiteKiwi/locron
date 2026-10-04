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
  bodies/assertions, original state/daemon setup and post-spawn clock sites.
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

Windows branches move original PrivateState/TempDir plus actual daemon Child and
their caller absolute deadline into one finite owner. Public entry signatures:
run_wake_case(state:PrivateState, daemon:Child, original_deadline:Instant)->CaseResult
and run_cancel_case with the same arguments. Retain an existing-only private
DirectoryGuard, actual root/role context, runtime/client/query handles and bounded
phase result together. Expected PID is private actual Child.id, never serialized
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
