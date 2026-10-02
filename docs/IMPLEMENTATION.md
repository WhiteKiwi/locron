# locron Milestone 1 Implementation Plan

## Dashboard pnpm tooling review (2026-10-02, PR38)

The owner's request replaces npm dependency installations with pnpm's shared
store and uses one common development toolchain: Node `24.21.0` LTS and pnpm
`10.34.6`. Scheduler, dashboard behavior, Rust compatibility, installation and
release contracts stay unchanged, so the frozen product SPEC needs no amendment.

1. Normalize the nested frontend mise/manifest pins and current installation
   commands; compare every imported dependency version and integrity against
   the removed npm lockfile. **Verify:** frozen install under the common tools
   preserves the exact graph and required native build-script policy.
2. Inspect frontend consumers, CI/release and embedded assets; convert remaining
   current npm installation/build calls without changing published asset or
   Cargo packaging contracts. **Verify:** frontend typecheck, full unit suite
   and production build pass; generated tracked assets are reviewed, and the
   existing Rust embedding/source-package checks appropriate to those changes
   pass without publication. Preserve historical npm verification receipts.
3. Qualify and integrate the source change. **Verify:** complete final diff and
   changed-file review, expected-head hosted CI and the ordered PR38 merge.
   Record exact commands/revisions/results on the private Project draft before
   Done. Hosted Rust-cache follow-up LOCRON-TODO-041 remains a separate task.

### Frontend verification and installation policy

The current CI gates Rust and source packages but does not rebuild the dashboard.
Add one Ubuntu frontend job for the pinned Node/pnpm tools, frozen installation,
typecheck, all unit/component tests and production build. Compare the complete
generated distribution with the tracked tree, including newly created paths, so
the Rust embed cannot silently ship an old bundle. One platform is sufficient for
these browser assets; keep the existing Rust platform/MSRV/lint matrix unchanged.
Use the runner-owned temporary directory for mise extraction and disable its
remote cache. This avoids shared temporary ownership conflicts without changing
the independent Rust cache-restoration task.

The imported graph retains all 197 package versions and integrity values. Install
with `--ignore-scripts`: the installed locked packages declare no installation
lifecycle hook, and platform-specific Rolldown binaries are registry packages.
The optional `fsevents` entry in the old npm lock does not require a lifecycle
script for the installed build/test graph. The production build and tests must
demonstrate that this policy preserves the required Vite/native behavior. Node
type declarations remain at the locked version; downgrading them would change the
dependency graph and is unnecessary if the Node 24 gate succeeds.

The source-package boundary already uses exact-version workspace dependencies
and a Rust 1.94 package/publication dry-run gate. Correct the dashboard plan's
older path-only/non-publication explanation to describe that current policy;
this tooling review changes neither Cargo manifests nor release publication.

### Integration with newer main (2026-10-03)

PR39 and PR40 entered main while PR38 was under review. Merge main
`82af6471ad477d96416ee67ad88cc2392abf98aa` normally and keep both Windows
foundation job definitions: their three native build/scheduling matrix legs and
one pinned lint leg are a staged build foundation, not Windows product acceptance.
Keep all existing Unix Rust, installer and source-package gates unchanged and
add the independent frontend job beside them. Windows ownership, implementation
scope and private acceptance drafts remain with their existing workstream.

Verify every pre-existing job against this exact main revision and retain the
reviewed frontend job from published PR38 head `c07a854`. Recheck frozen frontend
installation, typecheck, the complete test suite and production build, then run
Rust 1.98 formatting/Clippy plus Rust 1.94 and stable full workspace tests on the
integrated macOS source. Reuse the unchanged dependency graph and asset proof;
record a separate integration handoff before parent publication and exact-head CI.

After this reviewed plan and Project handoff, a separate development sub-session
owns implementation and documentation updates for any new decision. The parent
reviews and publishes. No tags, signing, registry upload, installer execution,
live jobs/services, Windows acceptance or broad storage deletion belongs here.

## Status and authority

This document plans the first program milestone against the frozen behavior in `docs/SPEC.md` and the durable structure in `docs/ARCHITECTURE.md`.

Accepted foundations are Rust edition 2024, Cargo resolver 3, Rust 1.94 MSRV, the official platform matrix, the four-crate dependency direction, one `locron` binary, and an engine-owned daemon entered through `locron daemon run`. Those decisions are not Draft.

> **Review state:** milestone-1 implementation choices are accepted. Update this document and the relevant Project draft tickets before deviating in code. A change to observable behavior or scope updates `docs/SPEC.md` first; a change to durable component boundaries or invariants updates `docs/ARCHITECTURE.md` first. Reviewed CLI and storage contracts live in `docs/CLI.md` and `docs/STORAGE.md`.

`docs/FINDINGS.md` preserves the research path and does not override the frozen specification. In particular, v1 has no `queue-one` overlap policy and global concurrency defaults to 16, not 4.

## Native Windows 11 implementation (2026-10-02)

The Windows amendment in SPEC and adapter boundaries in ARCHITECTURE are the authority for this
milestone. FINDINGS §46 records the selected safe interfaces, source audit and limitations. The
initial release is unsigned; signing remains deferred in public proposal #37 and is not part of
this milestone's dependency graph. Execution progress/evidence belongs in private Project drafts;
public #23–#36 remain proposal and review context rather than a second live execution checklist.

The first portability commit is explicitly a build foundation: native `windows-2025` x64 and
`windows-11-arm` ARM64 jobs check the workspace, exercise the portable scheduling domain, and
check pinned Windows MSRV/lint. Process execution fails with an actionable pending-adapter reason;
wake remains a reconciliation-only fallback until the secured adapter lands. These intermediate
gates are expanded to all-target behavioral tests with the runtime changes and do not claim
Windows product support. Existing Linux/macOS full-suite gates remain required throughout.

### Build, paths and execution configuration

Keep the existing five-package graph, Rust 1.94 MSRV and workspace unsafe-code prohibition. Make
nix/Unix imports target-specific and bring native Windows x64/ARM64 CI alongside the first changes.
Use LocalAppData for default machine-local state. Retain explicit state overrides, file-lock
ownership and SQLite WAL semantics; path strings never imply safe ownership by themselves.

Historical migration SQL/checksums stay immutable. Only a newly created Windows database receives
the captured Windows execution PATH after migration, guarded by its untouched default/zero-update
settings state; existing configured PATH values are preserved on reopen.

Shared environment/path helpers normalize Windows environment keys case-insensitively, reject
same-layer collisions and reserved-name variants, and apply precedence consistently across CLI,
dashboard and MCP. Resolve executables against effective PATH/PATHEXT; recognize drive/UNC and
separator paths, preserve argv/Unicode/spaces, and reject implicit .cmd/.bat direct execution.
Build shell snapshots by explicit family: cmd.exe /D /S /C, PowerShell/pwsh -NoProfile
-NonInteractive -Command, and POSIX shells -c. Unknown ambiguous Windows shell families produce
an actionable configuration failure. Persist absolute selected executables before spawn.

For the cmd /D /S /C snapshot, construct the final command tail with Windows CommandExt::raw_arg
and one outer quote pair; cmd's /S parser strips that pair and receives the original command text.
Do not apply C-runtime argv escaping to that command tail. Other executable arguments continue
through the ordinary argv serializer; the persisted executable/args snapshot format is unchanged.

Windows user-selected import/body/environment inputs use the same no-follow read helper with
retained ancestor/leaf guards until bytes have been read; relative CLI input paths resolve against
the current directory. Existing Unix reads retain their established behavior. Diagnostics share
transport facts: Unix wake_socket keeps its filesystem-presence boolean, while Windows reports
named_pipe, no filesystem socket, and unprobed availability. A diagnostics read never sends a hint
or creates a pipe/state directory just to claim availability.

### Race-free process-tree supervision

Use Windows-only process-wrap =10.0.1 with tokio1/job-object/kill-on-drop, plus win32job =2.0.3.
An attempt retains an independent win32job Job with kill-on-close. A safe CommandWrapper post_spawn
hook assigns the suspended Tokio child by raw_handle to that job; process-wrap's JobObject
pre_spawn sets CREATE_SUSPENDED and its wrap_child assigns its nested job before resuming. Failed
enrollment kills the suspended child and fails closed. Neither job enables process breakaway.

The root child and output streams are not tree-exit evidence. Query the retained job's process-ID
list under a bounded deadline on completion, timeout, cancellation, replacement and output-error
cleanup. Query errors or capacity overflow remain unconfirmed and retain existing quarantine/
interrupted-unknown rules. Do not rely on process-wrap's completion-port wait as proof of an empty
tree. The safe ChildWrapper::try_wait supplies only the root status; do not use its unsafe mutable
native-child accessor. Keep handles through confirmation/finalization so daemon crash triggers kill-on-close.
Use ordinary bounded natural completion/drain; hard tree termination is explicit where Windows
has no generic cooperative target signal. Keep Unix signal-group behavior in its existing backend.

On root exit, permit descendants the configured termination-grace window to finish naturally;
if they outlive it, stop the owned tree and classify the attempt as a non-retryable failure rather
than reporting the root's zero status as complete success. Cancellation and timeout similarly
permit bounded natural exit before hard termination, without claiming a delivered generic signal.
Post-spawn enrollment/resume failures preserve an unconfirmed termination outcome because the
suspended native child cannot be independently recovered after a wrapper failure. Query failures
never become proof of tree exit. After confirmed empty-tree/root exit, output drain has its own
finite grace deadline; leaked external pipe holders cannot indefinitely retain an attempt.

### Private state and guarded filesystem access

Use a fixed stock PowerShell 5.1/.NET DirectoryInfo.Create(DirectorySecurity) adapter to create
missing managed root components with a protected current-SID/SYSTEM-only inheritable DACL at
creation. Paths/options arrive as structured stdin JSON, never interpolated source. Use the
absolute stock PowerShell with -NoProfile -NonInteractive and a reviewed encoded script; do not
require pwsh or change execution policy. Existing roots require ownership/descriptor validation.

Create missing managed files through the stock .NET FileStream CreateNew/FileSecurity constructor
with an explicit current-SID owner and protected SID/SYSTEM DACL. Elevated tokens can otherwise
assign Administrators as the default owner even beneath a private parent. Keep the guarded parent
live through empty-file creation and subsequent Rust no-follow handle/ACL readback, before any
caller writes data. Use its canonical verbatim parent path for the .NET adapter, including long
paths. Report existing-file races from the IOException's numeric Win32 HResult; never repair or
truncate the raced-in file. Lock creation explicitly opens or creates, while sensitive output/
token/database creation remains CreateNew. Existing-file opens never infer creation from options.
Remove inherited PSModulePath only for the stock adapter, allowing PowerShell 5.1 to discover its
own built-in modules instead of loading incompatible PowerShell 7 modules from the calling shell.
Use one shared binary-only stock JSON bootstrap in the generic adapter, fixed filesystem worker
and phase-scoped COM worker. Before spawn, a SID-independent native core StockAdapterGuard retains
the exact stock PowerShell executable and selected GAC library files plus every ancestor through
owned cleanup/idle/quarantine. Build their absolute paths only from the absolute Windows root and
fixed Windows PowerShell 5.1/.NET Framework identities; no module search, cwd fallback or repair.
Join each fixed stock path component separately and reconstruct the validated absolute local
drive/root/normal-component sequence before adding its verbatim namespace. Preserve native OsStr
components and Unicode; never prepend a verbatim prefix to an unnormalized slash-containing string.
Already-verbatim components containing a forward slash refuse rather than changing their meaning.
Keep the existing local-drive-only, root and normal-component admission checks and all native trust,
reparse, sharing, identity and original-deadline checks. Verify: (1) pure Windows component fixtures
cover mixed ordinary SystemRoot separators, Unicode and every resulting ancestor, while rejecting
relative/UNC/device/parent and verbatim-slash inputs; (2) actual stock-file guarded full identities
and the real native guard-stall callback pass on x64, ARM64 and MSRV; (3) the original cold generic
and filesystem gates spawn through those same production paths within their unchanged budget.
Require SYSTEM/Administrators/TrustedInstaller owner, no untrusted effective write/append/EA/
attributes/delete/WRITE_DAC/WRITE_OWNER/generic write/all grants on each library/executable, and
no untrusted control/delete/reparse-mutation grants on any retained ancestor. Creation of an
unrelated sibling alone does not grant mutation of the guarded existing chain. The current SID
does not bypass these checks, avoiding the cold SID/bootstrap dependency cycle.

Construct the stock guard only inside the already admitted finite owner worker. Caller admission
updates only the existing permit counter; it performs no native path, descriptor or identity I/O.
Each native guard operation has explicit pre/post checks against the original API-entry deadline.
The generic result driver replaces its unconditional thread join with a deadline-bounded channel
receive, rejects a late ready result, and joins only an already finished worker. The filesystem
owner and COM phase owner follow the same boundary. A blocked native guard operation retains its
slot and every partially acquired handle in quarantine; its caller returns within the existing
thirty-second operation plus three-second owned-cleanup allowance. When that operation eventually
returns, the owner checks expiry before any next guard operation, process spawn or private input.
There is no second admitted child, replay, or successful ownership fact behind that refusal.

Transfer the complete stock guard into the exact child owner before spawn. It remains live through
the filesystem worker's sixty-second idle interval, generic/COM exchanges, and root/Job/pipe cleanup.
Confirmed cleanup releases it; uncertain cleanup retains it with the child/Job/permit in quarantine.
The generic adapter currently spawns a Tokio child without a Job and cannot claim abrupt-parent
containment from kill_on_drop. Select the same pinned safe Core-local suspended Job enrollment
pattern already used by the filesystem worker, including CreationFlags, process-wrap JobObject
and a separate win32job kill-on-close handle. Both Job enrollments precede resume and private input;
Core gains no Engine dependency. Generic cleanup requires the reaped root, authoritative retained
Job emptiness and finished pipe workers within its existing three seconds. A spawn failure that
lost root-wait capability retains its independent Job/guard/slot without treating empty Job state
as a root-exit proof. Parent kernel handle closure supplies emergency containment, never a reported
graceful exit or permission to replay an uncertain child.

The Core-local factory also sets the final native CREATE_NO_WINDOW | CREATE_SUSPENDED mask in
the safe spawn closure after all wrapper pre_spawn hooks. Its logical CreationFlags remain only
CREATE_NO_WINDOW, so JobObject resumes after both enrollments. This fixes the actual spawn boundary
without adding DETACHED_PROCESS or CREATE_NEW_CONSOLE. Verify the captured final mask in an actual
owned child; CONOUT$ availability alone cannot distinguish an invisible private console from a
visible console. The native binary loader/ownership fixtures remain required independently.

Verify this boundary with an isolated actual native guard-phase stall: after retaining real
no-follow stock ancestor/leaf handles, a cfg(test)-only anonymous-pipe ReadFile blocks the owner
until the test releases its owned pipe. The driver must time out, retain that slot/handles and
refuse another admission; releasing the pipe after expiry must produce no subsequent spawn/input
marker. This exercises real blocking native I/O inside guard ownership, not a claim that the
security-descriptor API itself was forced to hang. A separate owned parent-crash helper must prove
the actual generic PowerShell child's kernel Job termination and stopped heartbeat; Drop or an
unjoined worker alone is insufficient evidence. No fixture warms the original cold gate.

Pass only these retained canonical library paths as child environment data. Static bootstrap
loads/imports the exact binary, verifies its full assembly identity and actual loaded location,
and validates each retained JSON CmdletInfo's implementing assembly/type against that binary.
Use the returned command objects for both JSON directions; never rediscover them by a module
name. Disable module autoload and remove inherited PSModulePath. Import binary Utility cmdlets
for already reviewed generic callers (including Add-Type/Start-Sleep), plus the guarded binary
Management module for the reviewed junction/registry callers. The fixed filesystem/COM loops
need only Utility. Import no manifest, script module, format/type file, alias or function and
change no policy. Binding/location/descriptor/policy refusal is explicit with no fallback.

Native Win32 guard paths and Framework assembly loader paths have an explicit separate boundary.
Continue opening/retaining the exact canonical verbatim native files and every ancestor. Before
exposing a library to LoadFrom, derive an absolute local DOS spelling with native separators;
refuse URI/UNC/device/stream, dot/parent, trailing-dot/space and other normalization-sensitive
components. Query that spelling's full HighRes identity under the already retained native chain
and require exact equality. This conversion and native query occur in the existing finite owned
worker, with pre/post gates against its original entry deadline and no late spawn or unconditional
join. Library environment getters return only this verified Framework spelling; the executable
getter keeps the native spelling. Bootstrap refuses a verbatim/relative/URI library argument
before LoadFrom and retains the full assembly, loaded-location and CmdletInfo checks.

Verify: (1) pure namespace fixtures round-trip canonical Unicode/space/percent/hash paths and
reject ambiguous components, devices, UNC/URI and verbatim loader input without a search fallback.
(2) the actual stock guard probes compare both complete identities for each converted library,
retain replacement-denying handles, and preserve refusal/quarantine on a native stall. (3) the
original cold generic JSON, fixed SID/CreateNew and existing x64/ARM64/MSRV native core fixtures
load the guarded binaries under their unchanged thirty/three-second bounds. Restricted, forged-
module and abrupt-parent proof remain independent gates; this namespace correction cannot claim
their completion.

The fixed Task Scheduler waiting launcher also avoids a JSON/module bootstrap before Rust can
guard its state. Keep its one generated CLIXML/base64 argument value, but encode a versioned,
length-delimited UTF-8 record containing only SID, exact executable, state root and fixed role.
Static .NET BinaryReader with throwing UTF-8 decoding enforces lengths, protocol, no trailing
bytes and the fixed role before ProcessStartInfo; Rust readback accepts only the identical generated
representation. No data becomes executable source. This replaces the earlier base64-JSON payload
choice without changing exact argv, waiting/exit propagation or the original enabled flags.

Preserve the one-filesystem plus one-generic/COM child ceiling, original per-entry 30-second
budget and three-second owned adapter cleanup, with no warm-up or mutation retry. Verify:
(1) the original cold native x64/ARM64/MSRV core gate and actual first generic JSON request pass;
static phases show binary binding/conversion inside the unchanged budget. (2) forged user module
paths/cwd/name collisions and foreign-mutable/reparse stock candidates refuse or cannot execute
their marker; retained library/ancestor handles block replacement while the child is live.
(3) isolated actual children under process-only Restricted retain the inherited host policy and
round-trip generic JSON, filesystem SID/CreateNew, COM inventory and launcher exact Unicode/
quote/backslash argv; native wait/exit, caps, wrong-frame and parent-exit containment remain tested.

Bound stock adapter concurrency to two owned workers per process. A single thirty-second deadline
starts at API entry and includes permit wait, runtime/process startup and all input/output work;
permit saturation fails under that deadline rather than spawning more cold PowerShell processes.
Serialize first SID discovery under the same finite budget and share its verified cached result.
Keep input/output limits and failure semantics; owned kill/reap cleanup may add its existing
three-second termination-confirmation bound after the operation deadline. Native tests
must still exercise startup/script stalls and saturated permits; do not extend the deadline or
reduce privacy coverage to mask ARM64 cold-start contention.
An output reader that reaches the maximum plus one byte fails the operation immediately and
enters the same owned kill/reap cleanup. Do not wait for normal child exit before enforcing the
output cap: a child blocked on its output pipe would otherwise consume a complete adapter permit
deadline and starve independent state calls. Native fixtures must prove prompt output-cap refusal
and confirmed cleanup, independently of startup timeout and saturated-queue acceptance.
A bounded stock-adapter entry point accepts a caller's remaining duration, capped at the same
thirty-second operation maximum. Service polling uses the remaining shared lifecycle deadline;
a fresh adapter invocation cannot silently restart the complete shutdown budget.

Expose current_user_sid_until(Instant) for an already bounded bootstrap qualification. Capture
the caller's absolute deadline, cap it to API-entry plus the existing thirty-second maximum,
and use that same value for finite SID initializer admission and the existing fixed SID dispatch.
Check expiry before reading a verified cache, before a cold query, after its result and before
returning. Only a verified success received before that value may enter the shared cache;
failures remain retryable by a later independent call. Preserve the ordinary SID API and cached-only
IPC accessor. This adds no warm-up, child, mutation replay or metadata ownership authority.
The fixed worker retains its existing separate three-second cleanup/quarantine allowance.

Verify: (1) an expired call refuses even with a verified cache and performs no query; a finite
initializer-wait fixture consumes the caller's supplied deadline rather than starting thirty
seconds afterward. (2) a gated query that returns success only after expiry cannot publish a
cache value, while an in-budget result is shared by concurrent callers. (3) the actual isolated
cold fixed SID probe records its original caller deadline and accepts only an in-budget result;
the mapped-helper bootstrap caller forwards its existing qualification deadline without prewarming.

Native ARM64 evidence measured about 22.5 seconds for every stock PowerShell 5.1 startup,
including a no-stdin version/SID probe. Replace repeated filesystem process starts with one
process-local fixed filesystem worker containing only SID discovery, private-directory creation
and private CreateNew-file creation. The existing .NET descriptor constructors and post-operation
Rust guards remain authoritative. Each request is structured JSON with a version, monotonically
assigned request ID and fixed operation selector; the worker accepts no source, command or
executable in request data. Requests and reply frames retain the 64 KiB/128 KiB limits, with a
separate bounded stderr capture. Compare the reply ID and operation/result shape before accepting
it. Failure never triggers an automatic replay of a mutating creation request.

Retain one bounded owner thread and request queue for this filesystem worker. An absolute
thirty-second deadline starts at each public API entry and includes queue admission, cold worker
start, input, reply and validation. Requests already expired in the queue perform no work. A
timeout, EOF, malformed reply, wrong ID or output-limit violation kills/reaps the owned worker
under the separate three-second cleanup bound and wakes pending callers; future calls may create
a fresh worker. An idle sixty-second interval likewise closes and confirms the worker before
retiring it. Keep at most two stock children per process: one filesystem worker and one generic
reviewed-script worker; idle retention does not permit unbounded child/thread accumulation.

Spawn the persistent filesystem worker suspended and enroll it in the same reviewed safe
process-wrap =10.0.1 JobObject/kill-on-drop plus independent win32job =2.0.3 kill-on-close Job
pattern used for attempts before resuming it. The owner thread retains the Job handle through
confirmed root exit and empty-tree query. Parent abrupt exit closes that handle in the kernel;
static-cache or thread destructors and Tokio kill_on_drop alone are not this crash guarantee.
Spawn/enrollment/resume or cleanup uncertainty remains an explicit failure, never a successful
creation fact or an automatic replay. No private data is sent to an unconfirmed child.

Enable process-wrap's pinned creation-flags feature and supply CREATE_NO_WINDOW through its
CreationFlags wrapper, rather than only calling Tokio Command::creation_flags. JobObject's
pre_spawn derives its flags from that wrapper and otherwise overwrites the raw command flags.
The reviewed wrapper preserves CREATE_NO_WINDOW while adding temporary CREATE_SUSPENDED;
the existing post_spawn enrollment still precedes resume and private request input. Verify the
registered flag bits and JobObject wrapper in a unit fixture, and retain the native fixed-worker
reply/owned-cleanup fixtures. Source inspection proves the composed creation flags; a headless
runner's lack of a visible console alone does not prove CREATE_NO_WINDOW.

Native cold gates remain before the diagnostic probes, with no warm-up step. Fixtures retain the
production thirty-second maximum for a real stock process and report startup separately from
the script phase. A script-entered marker proves timeout cleanup after actual entry; an
output-phase marker and exact output-limit error prove prompt refusal after the cap is reached.
Use the separate three-second cleanup bound rather than assuming ARM64 enters within five or
ten seconds. Add concurrent-first-request, idle retirement/restart, failure-with-queued-callers,
wrong-ID/oversized-frame and abrupt-parent-exit fixtures. Generic arbitrary-script tests and
Task Scheduler COM calls remain outside the fixed filesystem dispatch; a later fixed COM worker
requires its own reviewed contract and the same shared lifecycle deadline.
Run the native core-library harness with --test-threads=1 because its real-stock timeout/cap
fixtures deliberately consume the one shared generic child slot for up to thirty seconds.
This serializes unrelated harness cases, not the implementation: explicit concurrent-first-use
and saturated-queue fixtures still create real concurrent callers and retain their original
deadline assertions. Run destructive fixed-worker faults and abrupt-parent cases in exact spawned
test-helper processes, so those intentional failures cannot invalidate other state tests.
Keep the core cold gate before post-gate probes and retain normal harness parallelism in the
other libraries. Splitting the CI core/store invocations changes no coverage or production bound.

Qualify private filesystem-channel EOF in an exact isolated helper with no unrelated dispatcher
or SID cache. Use a private bounded channel and the real fixed worker to complete one SID frame
under its original thirty-second entry deadline, then drop the final sender. At that owner's
cleanup boundary, a cfg(test)-only started blocking task reads an actual owned anonymous pipe.
Retain the writer in the helper and require the real root-reaped plus empty-Job observation before
the unchanged three-second cleanup budget expires with that native read still pending. Fixed
bounded test receipts identify entry, actual root/tree confirmation and retained quarantine;
they contain no SID/path/request values and introduce no production selector or behavior.

Verify that the private owner thread remains live rather than being joined or dropped, and that
a read-only incompatible open of its actual stock leaf returns the native sharing violation.
Release the pipe only after expiry, observe that actual read's completion, and verify ownership
still refuses the same incompatible open with no second spawn/request/effect. This proves the
EOF branch preserves uncertain I/O ownership after a late completion; it does not simulate a
failed root wait or claim the production security API was made to hang. End only the disposable
helper through its retained parent-owned process handle; kernel process exit then closes the
quarantined handles. Final guard-release proof runs in a fresh exact orchestrator with no other
stock worker, so a cached filesystem guard cannot create a false failure or success. The ordinary
cold gate stays first, helper waits remain finite, and production thirty/three-second budgets,
mutation no-replay, idle behavior and one-filesystem/one-generic ceiling remain unchanged.

The EOF fixture's deliberately pending native read must have both an active timer and per-poll
expiry checks. Wrap only that cfg(test) owned-read future in timeout_at with the exact original
three-second cleanup Instant; do not create a fresh budget, abort/drop the started blocking
task or alter production adapter polling. On timer expiry, return the cleanup error while its
JoinHandle stays in the retained Worker, then observe real quarantine before releasing the
writer. Verify the same real root/empty-Job, late read completion, two held-stock sharing
violations, owner-thread liveness and final helper-exit guard release under the existing finite
waits. The prior forty-second circular wait is not passing evidence for any of those assertions.

Select the complete EOF proof only through mandatory post-cold `eof-release-driver`, invoking
the existing isolated `eof-driver`/`eof-helper` chain. Remove its enclosing ordinary mixed-Core
test invocation, whose still-live cached dispatcher independently holds the stock leaf. Keep
the original forty-five-second outer helper and forty-second driver bounds, with no production
change. Before starting the EOF helper, the fresh driver must actually open and close the stock
leaf with read access and share_mode(0); an unrelated holder refuses this baseline. Then retain
the existing helper-held error 32, expiry/late-read-held error 32, exact helper kill/reap and
final exclusive-open success assertions. Verify: all three native rows run this exact mode as
the required first post-cold ownership proof after the cold Core process exits, followed by the
unchanged Restricted/parent-crash proofs; no coverage, guard or deadline is weakened or skipped.

Add cfg(test)-only bounded phase breadcrumbs to the actual filesystem dispatch path: API entry,
queue admission, owned-child spawn start/completion, input write/flush completion, reply receipt,
timeout/refusal and cleanup confirmation. Emit only fixed operation/phase names, monotonic request
ID, owned PID and elapsed/remaining milliseconds; never SID, path, input, reply or secret values.
The first failing cold state fixture must expose these facts without an earlier warm-up. Keep the
production source, request framing, mutation no-replay rule and original thirty/three-second
bounds unchanged. Verify that native failure output identifies the last completed stage; use the
same x64/ARM64/MSRV cold gates and distinguish post-gate startup measurements from that request.

Add a separate native stock-adapter proof step after the original cold core gate on the same
x64/ARM64/MSRV rows. Select the exact existing owned_loader_fixture_child with fixed
restricted-driver and parent-exit-driver modes in fresh test processes; record each actual
mode/result. The ordinary mixed Core harness can retain its earlier filesystem stock guard for
sixty seconds, so it cannot supply the final incompatible-open release proof. The fresh proof
orchestrator must perform no stock/SID/filesystem-dispatch call of its own. Keep required names,
the original cold command/order and all job deadlines; run no probe before cold qualification.

Restricted mode sets PSExecutionPolicyPreference=Restricted only on its owned child helper.
Before binding JSON, a cfg(test)-only compiled assertion in both the actual generic and fixed
PowerShell source resolves SecuritySupport from the already loaded PSObject assembly, selects
the exact NonPublic|Static GetExecutionPolicy(string) method for Microsoft.PowerShell, and
requires the actual Microsoft.PowerShell.ExecutionPolicy enum to be Restricted. Missing
PowerShell 5.1 reflection compatibility or an overriding policy fails explicitly. No Security-module
import, Set-ExecutionPolicy, bypass argument, host/registry policy change or production selector is added.
Report a fixed bounded policy-confirmed token tied to each actual owned PID.

Preserve that actual fixed-child receipt independently of its rolling diagnostic history.
Add one cfg(test)-only observed-policy bit to each ChildPhases; latch it only when the existing
exact policy-confirmed token is parsed from that owned child's bounded stderr. policy_observation
reads this bit from the current retained ChildPhases/PID association, rather than searching a
sixteen-entry ring whose startup token can be evicted by three successful requests. Keep the
ring, production source, token/reflection and all deadlines unchanged. A fresh ChildPhases
starts false; arbitrary stderr, another child or environment text cannot supply confirmation.
Verify: feed the existing recognized token followed by more than sixteen recognized request phases,
retain its confirmation while the ring stays bounded, and prove fresh/unknown-token states
remain unconfirmed. Then require actual Restricted generic JSON/COM and fixed SID/CreateNew/
ACL/PID proof on all native rows; the observed timeout or a saved generic result alone is not
passing evidence. Parent-crash qualification follows only after that required mode succeeds.

Within that helper, run the real generic Unicode JSON round-trip/read-only Schedule.Service
inventory and fixed SID/CreateNew/private-ACL operations concurrently, preserving each API's
original thirty-second entry deadline and three-second cleanup. The existing forty-five-second
isolated helper bound stays unchanged; no sequential cold-start allowance is added. Use retained
JSON command objects and the existing forged-module/cwd marker; the marker must remain absent.
Core source and its private policy composition/token parser remain filesystem-owner scope.

Parent-exit mode owns a crash-host and an independent observer through retained native Child
objects. The crash-host starts its real fixed worker and generic PowerShell child; the latter
starts one exact current-test-executable heartbeat descendant with data-only argv/environment,
UseShellExecute=false and hidden stdio. Observe actual worker/generic/descendant identities while
alive, including real fixed replies and generic spawn facts. The observer opens and retains all
three Process handles, then publishes the bounded handles-bound marker before any crash.

The orchestrator kills/reaps only its retained crash-host, preserving abrupt kernel Job closure.
The observer must confirm actual associated-process exits with finite WaitForExit under its
single original adapter budget; API completion is accepted only before that deadline. Verify
both generic/native heartbeat files stop, not merely that PID lookup fails. A read-only share=0
stock open refuses while either helper owns the guard, including after crash-host exit while
the observer remains live. Permit the final successful open only after both helper processes,
all observed targets and the observer's normal adapter pipe/Job cleanup are confirmed finished;
otherwise fail and retain bounded cleanup ownership. No system file mutation or unrelated PID signalling occurs.

Verify: (1) each fresh Restricted child reports actual enum/PID proof, JSON/COM and fixed
SID/CreateNew/private ACL succeed, and forged code remains unexecuted. (2) before-crash retained
handles and moving heartbeats prove real live descendants; killing only the owned parent makes
all three handles signal exit without replay or graceful-exit claims. (3) the observer-held guard
continues refusing incompatible opens, then final confirmed helper exit permits a read-only open
and heartbeat samples stay unchanged. Record revision/image/toolchain, modes, stages and counts;
compile-only, unset helper mode, timeout or missing marker cannot qualify these gates.

The post-cold stock diagnostic still launches its own raw PowerShell children with unqualified
JSON cmdlets, while the accepted Core adapter uses retained guarded binaries and command objects.
Correct that diagnostic boundary by invoking the exact owned_loader_fixture_child with three
fixed LOCRON_STOCK_LOADER_FIXTURE modes: diagnostic-bootstrap, diagnostic-small and diagnostic-60k.
Each selection runs in a fresh hosted test process and independently retains the existing
forty-five-second isolated-helper bound, thirty-second API-entry budget and three-second owned
cleanup bound. Do not combine two sequential cold starts inside one helper, add a warm-up, change
the original cold Core command/order or let a later diagnostic pass replace a failed cold gate.

All three helpers use the real prepare_adapter/run_adapter_worker, StockAdapterGuard, owned
suspended Job factory, bounded pipes and retained locronFromJson/locronToJson command objects.
The cfg(test)-only bootstrap helper clears only the prepared input bytes, preserving genuine
zero-byte stdin and EOF through the normal bootstrap. Its compiled static fact script ignores
request data and requires the actual locronInput length to be zero; it must not substitute an
empty JSON object or skip parsing. The two structured helpers send the existing Unicode echo
and five-character/60,000-character payloads, validating the exact 54/60,049 UTF-8 input lengths,
echo equality and payload lengths. Static caller JSON output uses &$locronToJson -Compress.

Retain the runner-host PE/file-version and invoking-shell architecture/version facts. Record
each actual child PID, PowerShell 5.1 version, is64bit/process architecture, actual current-token
SID, byte count, elapsed time, input/EOF/output phase receipts and success or timeout category.
Require a valid S-1 SID and PS5.1 facts, native architecture agreement, input-written and ordered
binding/JSON/caller phases, and actual root-exit/empty-Job/pipe completion before a passing fact.
Use bounded existing trace/error data on failure. Cleanup uncertainty or a retained quarantine
is a failed diagnostic, never a synthesized cleanup-confirmed value from process Drop/PID absence.
No fixed-worker warm-up, adapter fallback, source supplied by environment, host policy change or
new public Core API is introduced. Core helper source remains the filesystem owner's scope.

The CI diagnostic script retains its host metadata header, replaces its private raw child-launch
implementation with these three exact cargo/helper selections, and records every independent
result before failing if any selection failed. Keep the existing post-cold always condition,
job names and deadlines; no continue-on-error, retry, suppressed assertion or weakened output cap.
Verify on native x64/ARM64/MSRV: all original facts and zero/small/60k assertions execute against
the actual guarded Core path; a timeout or missing phase remains a failing gate with real owned
cleanup facts, and the original unwarmed cold suite still runs first. Pair the reviewed Core
helper and CI source before publication; command compilation alone is not native qualification.

The next exact ARM64 run located the first failure in the SID exchange: spawning the owned child
and flushing its input took 49 ms, but no reply arrived before the original thirty-second
deadline. The fixed PowerShell source contains no Add-Type or dynamically compiled C# helper.
The current parent-side stages cannot distinguish stock host/source initialization, Console
encoding and ReadLine, first JSON cmdlet activation, SID lookup, or reply serialization. Before
changing any production operation, add test-only static child-phase tokens on stderr at source
entry, encoding completion, input-line receipt, JSON parse completion, SID completion and reply
serialization/flush. Select these tokens only from fixed source at compile time; no caller data,
SID, path, payload or exception text enters a token. Preserve the existing stderr capture cap and
early failure, collect at most sixteen timestamped recognized tokens per owned child, and attach
only those tokens to the existing failure diagnostics. These timestamps mean parent receipt,
not a claimed child CPU measurement. Production source, framing and deadlines remain unchanged.

Verify: (1) the actual first failing request runs without warm-up or replay and distinguishes the
last completed child phase from the already measured input flush; absence of the source-entry
token remains an unresolved host/source bootstrap gap. (2) native successful SID and create
requests produce the expected ordered fixed phases without contaminating stdout frames or
revealing private input, while unknown stderr content is never promoted into diagnostic facts.
(3) capture-limit and timeout fixtures still fail promptly under thirty seconds plus the separate
three-second confirmed cleanup; token retention stays bounded and failed queued work is not
replayed. Use these facts to review the next implementation choice before changing production.

The measured first request now reaches input-line receipt in about 3.1 seconds but never reaches
the JSON-parse completion token before thirty seconds. Command discovery, module import and the
first JSON invocation still share that unmeasured interval; do not label autoload the proven cause.
Select a narrow fixed-worker binding change: disable module autoload in this child session and
explicitly import only ConvertFrom-Json/ConvertTo-Json cmdlets from the absolute stock Utility
manifest under the running stock host's PSHOME. Import no exported functions or aliases, qualify
both JSON calls with Microsoft.PowerShell.Utility, and retain the same stock converters and
parameters. No request value chooses a module, assembly, function or source. Keep inherited
PSModulePath removal, NoProfile/NonInteractive, the existing execution-policy behavior and all
descriptor operations; policy/import failure is a visible refusal rather than a bypass.

Add test-only fixed before/after-import phase tokens to the existing bounded stderr facts. Import
is inside the first caller's original thirty-second cold budget, not a warm-up or separate startup
allowance. The existing input-line/JSON/SID/reply tokens locate any remaining first invocation gap.
Stock Utility is the same OS module previously selected by autoload; read-only inspection of this
host's 5.1 manifest confirms both JSON cmdlets and its stock nested binary/script modules. This
binding choice removes module search from request processing but is not yet a performance proof.

Verify: (1) the original unwarmed x64/ARM64/MSRV cold gate completes or reports the last import/
JSON phase under the unchanged thirty-second operation and three-second confirmed cleanup bounds;
later successful probes cannot turn a failed gate into acceptance. (2) an isolated actual worker
uses the fixed stock converters despite hostile inherited module paths, preserves Unicode/percent/
hash paths and strict reply frames, and emits ordered bounded import/SID/CreateNew phases.
(3) malformed input, output caps and timed-out creation still refuse without executing request
source, increasing children, replaying a mutation or changing execution policy. Native evidence
must establish whether the measured ARM64 delay is resolved before marking the privacy gate done.

The measured remaining ARM64 broad-file failure is its separate generic ACL-fixture setup, after
private creation succeeded. Change only the two broad file/root setup scripts to the compiled
.NET Framework File/Directory GetAccessControl and SetAccessControl methods; preserve the same
Everyone Read ACE, JSON input/output and original generic thirty-second plus three-second bounds.
Avoid importing the unrelated PowerShell Security module for a test-owned ACL mutation. Do not
change production worker framing, JSON binding, deadlines, cold gate ordering or host policy.
This is a fixture dependency correction; generic cold-stage/policy acceptance is still pending.

Verify: (1) native ARM64/x64/MSRV original cold core suites run unchanged and the deliberately
broad file fails private validation before truncation with all original bytes intact. (2) the
owned broad directory still refuses ordinary private adoption, remains broad until explicit
repair, then has real current-SID/SYSTEM privacy. (3) real generic startup/stall/output-bound and
fixed-worker cold/concurrent phase tests still execute without a warm-up, retry, skipped test or
budget increase; setup failure stays a visible failure rather than a passing privacy assertion.

Instrument the actual generic adapter separately from the fixed filesystem dispatcher. Add only
cfg(test) fixed child tokens for source entry, encoding, complete stdin read, JSON conversion
entry/completion, caller entry/completion and catch. Retain a bounded recognized-token trace while
stderr is read, and report it on actual adapter failure even when timeout cancels its pipe task.
Do not render request/SID/path/payload values or promote unknown stderr into phase facts. Preserve
production source/JSON binding, the per-entry thirty-second deadline, separate three-second owned
cleanup, 64 KiB input and 128 KiB per-output bounds, permit accounting and original cold ordering.
Explicit stock Utility binding is a researched next candidate, not a measured generic root cause
or an approved production switch in this diagnostic step. A stricter-policy candidate must be
tested only in an owned hosted child; production and host policy remain unchanged.

The mapping fixture must attempt release and join its helper before any assertion, preserve the
independent marker, release, helper and test-owned ancestry observations, and inspect the joined
helper error before an absent-marker assertion can hide it. Stop waiting when the helper already
exited; this shortens failure reporting without changing either adapter budget or mapped-handle
acceptance. Never turn a missing marker into a passing mapped-file assertion.

Verify: (1) original native ARM64 broad-file cold failure now identifies the actual generic child
phase under the same thirty-plus-three bound without warming/replaying state creation. (2) the
mapping fixture reports its actual setup/stall error and independent temporary-parent status after
owned cleanup, or proves a writable mapping still rejects the stable read gate after the original
FileStream closes. (3) isolated successful generic calls emit ordered fixed phases, malformed/
oversized/stalled calls keep both caps and confirmed cleanup, and hostile module paths/stricter
policy remain explicit pending acceptance until an actual reviewed binding fixture proves them.

Before writable SQLite open, explicitly precreate missing database/WAL/SHM files with that
descriptor and validate them again after configuration/migration, before accepting application
operations. Normal SQLite sidecar deletion on the last close remains intact; the next writable
open precreates missing sidecars again. Read-only validation uses the actual supplied database
filename and its sidecars, and performs final readback without changing the file-creation contract.

Use Windows-only windows-permissions =0.2.4 explicit GetSecurityInfo/SetSecurityInfo wrappers with
SE_FILE_OBJECT, Owner/Dacl and ProtectedDacl flags; avoid the audited-buggy convenience trait.
Safe Windows File open flags permit no-follow handle readback and directory guards. Reject every
managed/ancestor reparse point, unsafe foreign ownership and unsupported ACL filesystems. Retain
guards that prevent path replacement/mutation through SQLite/output/token operations; native
adversarial tests must validate the guarded-chain sharing policy, including custom roots.
New files inherit only from validated private parents; inspect existing files before reading
sensitive contents. Shared core primitives let store, engine output and server token enforce the
same rule without reversing the workspace dependency graph. doctor reports measured ACL facts.

Windows existing-file observers use existing-only private parent guards. In particular,
open_private, role-sidecar reads, missing-file deletion and read-only SQLite validation may
return absence but never create a state root, lock, database or sidecar while observing it.
Explicit private-directory/CreateNew/permanent-lock creation remains in the owning writable
composition path. Preserve Unix permission/open semantics. Native fixtures inspect an initially
absent root before and after each passive operation, verify NotFound/None/idempotent deletion,
and prove that the separate writable first-run path still creates correctly owned state.

During the build-foundation stage, unimplemented Windows permission changes fail with an explicit
unsupported-capability error, and permission diagnostics report `unsupported`. Compilation alone
must never turn a no-op permission adapter or a numeric placeholder into an owner-only fact.

Managed directories and data files accept only the current SID as owner and only current-SID/
SYSTEM allow entries; existing broad descriptors are refused rather than silently tightened. A
test that begins with an ordinary temporary directory creates a private managed child. Ancestor
directories may have trusted current-user, SYSTEM, Administrators or Windows TrustedInstaller
ownership. Retained no-delete/no-write-sharing handles protect even foreign-writable ancestors;
an incompatible existing handle is an actionable refusal, never a reason to drop the guard.
Resolve relative state overrides lexically against the current directory before opening guards;
reject drive-relative/root-relative ambiguity and network state roots. Canonicalize identity only
after every component has passed no-reparse handle inspection and the guards remain live.

Managed file readers retain no-delete sharing. Dashboard/CLI follow reads release their handles
after each frame snapshot, but a concurrent snapshot can briefly prevent Windows finalization.
Retry only native sharing violations for at most five seconds, validating source and destination
again on each guarded rename attempt. Keep all other failures immediate. A reader held beyond
that bound leaves the synced partial intact and reports a real infrastructure failure for normal
recovery; never claim finalization or discard captured bytes when the rename has not succeeded.

### Wake, cooperative role control and Task Scheduler

Use Windows-only interprocess =2.4.4 with tokio, safe SDDL SecurityDescriptor deserialization and
PipeListenerOptions security_descriptor/accept_remote(false). The audited source establishes the
first-instance flag and remote rejection; do not use instance_limit=1 because accept creates a
replacement listener. Derive names from verified SID + guarded directory file identity + endpoint role.
Owner/SYSTEM-only descriptors apply at creation. Both CLI and dashboard notification senders use
the same bounded versioned-hint backend with a fixed length prefix and consumption acknowledgement;
the duplex pipe carries no other responses. Dispatch control only after the client consumes and
confirms the acknowledgement, so immediate idle-role shutdown cannot abort its own reply.
A drop guard always clears the pipe's flush obligation,
including listener abort during acknowledgement; accepted-client cleanup never creates an
unbounded FlushFileBuffers worker. Bind after the owner lock and retain reconciliation on
absent/busy/occupied endpoints. Bound reads and avoid unnecessary server impersonation.

Shared notification::instance_identity(root) and instance_identity_guarded(DirectoryGuard) return
the lowercase SHA-256 hex of the fixed locron-instance/v1 domain, SID length (LE32)/UTF-8 bytes,
volume serial (LE64) and full file ID (LE128). Windows-only file-id =0.2.3 supplies its reviewed
get_high_res_file_id safe API. Query the normalized path while the complete no-write/no-delete
directory guard remains retained; reject an unsupported query instead of using its low-resolution
fallback. No path text, Unicode folding, DefaultHasher or Rust enum/hash representation enters
the digest. Scheduler task names use this shared identity to avoid duplicate alias registrations.
Pipe names add an explicit protocol version, role and canonical UUID lifetime for control roles;
wake has no lifetime. Listener construction derives its name from the same guard it retains.

Separate secured control endpoints bind role/lifetime identity and deliver only graceful shutdown
requests to that role's existing cancellation token. Lifecycle coordination first disables automatic
task activation, requests stop, and waits for confirmed role/lock exit. A failed request/remaining
holder is an actionable bounded failure; task-state alone cannot report graceful completion.

Keep core's public boundary free of async-runtime types: it shares normalized user/state endpoint
identity, fixed message framing and a bounded synchronous hint sender. Its Windows client uses
Tokio ClientOptions with identification-only SQOS, rather than interprocess's default impersonation
capability. One short-lived current-thread runtime executes on a dedicated worker, with a finite
connect/write/ack deadline and joined cleanup; it never nests block_on inside a caller's runtime
or leaves a background writer/flush thread. Engine owns the asynchronous named-pipe
listener and role-control cancellation adapter. Server uses the core sender without gaining an
engine dependency; CLI composes engine listeners after acquiring the owning lifetime lock.
Headless Windows roles retain cooperative control when console Ctrl-C registration is unavailable;
that diagnostic alone cannot terminate a registered dashboard before its control future runs.
Dashboard shutdown publishes a private watch signal to close live SSE responses, then stops new
connections and drains finite HTTP work. A bounded connection-drain deadline aborts remaining
HTTP tasks; this affects only dashboard transport and never cancels durable scheduler jobs.

Windows role locks retain actual OS byte-range ownership and publish a separate private, atomic
owner sidecar containing diagnostic lifetime identity and whether the process is a registered
service. This keeps observers from trying to read bytes covered by the Windows lock. The sidecar
never proves ownership or authorizes PID killing: control validates the exact live lifetime,
and lifecycle completion still requires the corresponding daemon/dashboard lock to be free.

Use stock PowerShell Schedule.Service COM with structured inputs/output, deterministic SID/state/
role task names, current SID LogonTrigger, INTERACTIVE_TOKEN, LUA, no password, and create/update
registration. Set PT0S execution limit, no battery/idle/network gates, IgnoreNew, and bounded
RestartOnFailure (three retries, PT1M). Definitions use absolute ExecAction path and correctly
escaped state/role arguments. Read semantic settings/status rather than localized schtasks text.
Preserve enabled/disabled role state on refresh; run roles directly or use a fixed hidden launcher
that waits and propagates exit status so restart works. Task.Stop is a documented hard fallback
after cooperative timeout, with kill-on-close/recovery behavior, not graceful-drain evidence.

Registered `daemon run --service-mode` first acquires the private daemon.activation.lock and its
atomic registered-service sidecar with a unique activation lifetime. Bind an internal
daemon-activation control endpoint to that lifetime, then passively retry actual daemon-lock
acquisition under cancellation. The wait has no arbitrary execution-duration limit; a manual
daemon is never signalled, and scheduler ownership is not claimed before its real lock is acquired.
Once acquired, establish the ordinary daemon ownership/control before beginning scheduling. Keep
the activation lock/control through this registered process's complete lifetime, and release it
only after actual daemon ownership and all listeners have been torn down. This gives maintenance
one exact activation lifetime spanning waiting, running and exit without a handover gap.
Status distinguishes a waiting registered task from the actual daemon lock owner. Installation
therefore retains automatic activation after a manual daemon exits. Quiesce disables the task,
requests the exact activation lifetime, and waits for the activation lock plus waiting launcher
to exit under the shared thirty-second cooperative deadline. An unchanged owned activation may
then use explicit Task.Stop; actual lock/launcher exit still requires finite confirmation. A
remaining manual holder is untouched and can still refuse executable replacement. Runtime owns
main/waiter/core-control allowlist wiring; store owns the activation path and service owns the
guarded observer/inventory/quiescence. No new public product role or CLI command is introduced.

Windows registration uses the shared full-file-identity/SID digest for role-specific task names.
Select a fixed hidden stock PowerShell 5.1 launcher: `-EncodedCommand` carries only static source,
and `-EncodedArguments` carries a serialized CLIXML array containing one base64 JSON request.
The launcher validates the current SID and uses ProcessStartInfo with UseShellExecute=false,
CreateNoWindow=true, exact escaped Windows argv and an explicit working directory; it waits for
the role and returns that role's exit code. The wrapper's PID is never the role's PID. This
keeps paths/data out of executable source and avoids a separate installed script or policy change.
Readback compares semantic principal/trigger/power/restart/action fields and retains a disabled
registration on refresh. COM may return account names after SID-based registration, so translate
actual principal/logon-trigger account identifiers to SIDs before semantic comparison; environment
username text never proves account identity. Cooperative failure first waits under the shutdown deadline; Task.Stop
then targets only the validated owned registration with the unchanged registered-service lifetime.
After this hard fallback, actual role-lock exit is still required and forced completion is
reported explicitly; unowned/manual holders remain a bounded refusal or registration deferral.

Updater/package maintenance inventories every current-SID Locron task bound to the verified
installed executable, across all state roots. Compare full Windows file identity while retained
no-follow file/ancestor guards remain live; path lowercasing and filename matching do not prove
an executable binding. Parse only the fixed launcher command plus its strictly generated one-value
CLIXML/base64-JSON argument representation. Reconstruct the private state guard and shared full
instance digest, and validate role, deterministic task name, marker, task ACL and executable.
Malformed, foreign or unconfirmed bindings refuse maintenance before stopping any process.

Expose a guarded in-memory snapshot and a serializable versioned restore record containing the
current SID, previous executable, and each prior registered role's state root, instance digest,
task name, original enabled flag and exact semantic definition fingerprint. Exclude transient
run/result observations and the enabled flag from that fingerprint. Quiesce disables activation
for every owned binding before requesting exact lifetime shutdown; actual role-lock and waiting
launcher exit remain necessary. The private journal records confirmed quiescence and explicit
forced fallback facts, without containing executable task source or arbitrary role arguments.
Restore reconstructs guards and checks every existing definition against the recorded fingerprint
before the first write, then binds only prior registered roles to the new verified executable and
restores their exact enabled flags. Changed definitions/roots/SIDs fail closed; disabled roles stay
disabled. Missing registrations are not silently recreated from a stale record.

The package flow composes these same APIs through the existing installer maintenance modes:
Prepare snapshots/quiesces all bindings for one verified executable and journals an operation UUID;
Complete validates the installed package and restores prior registrations; Remove quiesces and
removes only the validated prior registrations. No additional release asset or arbitrary manifest
hook is introduced. Distribution owns the private journal/receipt and package registration proof.

### Unsigned release, installation and update handoff

Add native x64/ARM64 MSVC ZIP builds containing locron.exe, README and both licenses. Extend exact
asset inventory and SHA-256 generation with version-aware historical inventory compatibility;
keep existing Unix publication/signing inputs authoritative. The canonical release source is
WhiteKiwi/locron over verified HTTPS. Final published bytes, version/architecture and channel
metadata agree; checksums check integrity without independent publisher authentication.

The standalone PowerShell 5.1 installer validates archive source/digest/architecture/version and
safe paths before installing in a private user directory. Retain versioned exact-path receipts,
explicit PATH choice, optional daemon/dashboard registration and precise state-preserving removal.
The Windows updater verifies helper/destination ownership and stages verified bytes, quiesces all
owned mapped executable holders, suppresses restarts, hands off to a second process and confirms
completion before success. Locked/unowned MCP/manual holders are bounded refusals. Replacement,
receipt update and registration restoration are rollback-capable; no normal reboot-replacement
path, optimistic updated:true or silently enabled dashboard. WinGet uses the same final ZIPs,
InstallerSha256 and explicit package-manager ownership; self-update refuses its binary.

#### Concrete Windows distribution contracts

The Windows feature release inventory starts at v0.10.0. Tags before v0.3.0 retain their eight
Unix payloads plus checksums; v0.3.0 through v0.9.x additionally retain install.sh. Windows tags
add exactly the x86_64-pc-windows-msvc and aarch64-pc-windows-msvc ZIPs, install.ps1 and uninstall.ps1.
The ZIP has one exact version/target directory containing locron.exe, README.md and both licenses.
Check final PE architecture, executable version and absent certificate table before accepting it.
SHA256SUMS retains bare names and covers all payload archives/packages; installer assets are
separately included in immutable publication digest verification. No historical asset is rewritten.

Windows release builds use Rust 1.94, windows-2025 for x64 and windows-11-arm for ARM64, explicitly
select their native MSVC target and set
RUSTFLAGS=-C target-feature=+crt-static. The explicit --target keeps this flag off host build scripts
and procedural macros. The locked cc 1.4.4, bundled libsqlite3-sys 0.38.2, ring 0.17.14 and
aws-lc-sys 0.44.0 build scripts propagate that choice to their C/C++ compilation. Check both normal
and delayed PE imports against a finite Windows system-DLL allowlist; reject Visual C++
redistributable DLLs, debug runtimes and other application DLLs. Run the packaged executable's
version check with only Windows system directories in its child PATH. These gates verify the
build intent and direct dependencies; clean Windows 11 acceptance remains required to prove
runtime behavior without Visual Studio, a Rust toolchain or separately installed VC redistributables.
The finite allowlist includes only the explicitly reviewed api-ms-win-core-synch-l1-2-0.dll API
set used by the native Rust build. Microsoft's
[WaitOnAddress requirements](https://learn.microsoft.com/en-us/windows/win32/api/synchapi/nf-synchapi-waitonaddress)
identify this synchronization contract as available since Windows 8. Other API-set names still
require individual review; no prefix-based exemption admits an unknown dependency.
PR CI builds both native Windows release targets with this configuration, packages the current
workspace version as a test input, validates the actual ZIP and retains import/hash/toolchain facts.
The rustc host triple must equal the selected target before building either native package.
Branch names never select release publication inventory, and this job creates no tag or release.
This choice follows the [Rust linkage reference](https://doc.rust-lang.org/reference/linkage.html),
[Cargo flag scoping](https://doc.rust-lang.org/cargo/reference/config.html#buildrustflags),
[Microsoft CRT redistribution guidance](https://learn.microsoft.com/en-us/cpp/windows/redistributing-visual-cpp-files?view=msvc-170)
and [PE import format](https://learn.microsoft.com/en-us/windows/win32/debug/pe-format).

The standalone Windows default executable is LocalAppData\\Programs\\locron\\locron.exe. The
installer accepts -Version, -InstallDirectory, -NoService, -Dashboard and -AddToPath. It starts the
daemon by default, enables the dashboard only when explicitly requested, and changes only the
current user's persistent PATH when -AddToPath is specified. Reinstallation preserves a registered
dashboard's previous enabled state. Registration failures are explicit warnings after a confirmed
binary install, not a fabricated claim of service health. Private installation directories and a
versioned JSON receipt bind the current SID, canonical executable, channel, target, version,
canonical archive URL, archive SHA-256 and executable SHA-256. Existing unowned/package-manager
destinations are refused. Uninstall requires this receipt, quiesces only exact owned registrations,
removes only receipt-listed unchanged files and an installer-inserted PATH entry, and retains state.
Standalone and WinGet locations coexist; neither installer adopts the other's files.

Keep the release inventory unchanged by placing WinGet maintenance in install.ps1:
-Maintenance Prepare|Complete|Remove with -Executable and -Operation selects that explicit flow.
Prepare copies the verified existing package executable as the hidden helper into the protected
LocalAppData\\locron-distribution\\operations\\UUID directory, outside the package installation.
It persists SID, package registration/source, exact executable hash and all task definitions with
their prior enabled/running states before suppressing activation and confirming graceful exit.
Prepare returns operation_id, status_file and prepared=true only after the journal is durable and
quiescence is confirmed. Those owned tasks remain disabled until a successful Complete.
Complete validates this journal and the newly selected package registration/executable, refreshes
the stored paths and restores the recorded enabled states. Its current SID, package/source IDs,
canonical registered location, target, version and canonical-release binary hash must agree;
arbitrary or stale executable paths cannot resume tasks. On an interrupted/failed Complete,
recover the journal and restore prior state only against a still-valid recorded binary, or retain
disabled registrations with an explicit recovery error until a valid package is selected. Never
enable a mismatched path. Remove quiesces and removes only the recorded exact executable-bound
registrations before the operator runs winget uninstall. The helper request schema is
locron.windows-operation/v1; installer, updater, uninstaller and maintenance share this internal
entrypoint and serializable validated lifecycle record. Missing, foreign or interrupted records
are explicit refusal/recovery cases. No new release asset or arbitrary manifest hook is introduced.
Use Windows-only zip =8.6.0 (MSRV 1.88) with only deflate-flate2, reusing the workspace's Rust
flate2 backend. Read exact inventory members with bounded sizes into memory, reject encrypted,
special/reparse/duplicate members and never use a generic path-extract operation. Persist all
managed files through the shared safe filesystem guard API. PowerShell performs stock .NET
private staging and passes typed operation JSON to the same verified binary helper; it needs no
runtime compiler or native API code generation.

Windows self-update cannot wait synchronously while its caller still maps the destination. A
verified copy of the owned running executable in a private operation directory executes the hidden
update-helper entrypoint. The helper is launched detached with breakaway requested; if an enclosing
Job Object prevents breakaway, the update refuses handoff instead of risking helper termination
when the caller exits. The caller returns only after helper acceptance, with updated=false,
pending=true, an operation UUID and status-file location. This exit 0 means handoff accepted, not
replacement complete. self-update --status UUID reports pending until the helper confirms binary,
receipt and registration-restoration results; only confirmed replacement yields updated=true.
Failed or rolled-back operations report an error. Unix synchronous output is unchanged.

The helper binds its exact path/hash, SID, destination, old receipt and staged verified executable
in a durable operation request. It serializes updates, snapshots all exact executable-bound owned
tasks, disables activation, requests lifetime-bound shutdown and waits for owned-role exit plus
the original caller. A bounded exclusive write-open of the old executable is the final mapped-holder
gate; unowned MCP/manual holders refuse replacement rather than being killed. Retain the file
guard with delete sharing through the backup rename, maintain private rollback binary/receipt and
write a durable journal before each irreversible step. Restore exact prior enabled/disabled
registrations after verification, recording restoration warnings separately from confirmed binary
replacement. Interrupted operations must be recovered or explicitly refused before a later update;
neither a stale request nor a task-state transition implies completion. No reboot replacement.

WinGet uses WhiteKiwi.locron, user-scoped ZIP/portable installers and the same immutable ZIP hashes,
with architecture-specific nested executable paths and the locron command alias. The receipt-free
WinGet package location and its package-manager registration identify managed ownership; the
standalone installer refuses those paths and self-update directs to winget upgrade --id
WhiteKiwi.locron --exact. WinGet cannot run arbitrary portable-install hooks, so upgrades/removals
require the documented explicit service-disable/upgrade-or-remove/service-refresh procedure;
durable state is preserved. Generate reviewable manifests from final local or downloaded canonical
release assets and validate with winget validate; submission/publication remains parent-owned.
Use the three-file WinGet 1.12 manifest schema (version, en-US default locale and installer), with
MinimumOSVersion=10.0.22000.0. WinGet portable manifests ignore Scope and emit validation warnings
when it is supplied. Omit that unsupported field and require --scope user in the documented
install/upgrade procedure; do not claim the manifest enforces per-user scope. This is verified with
winget 1.29.380 and follows Microsoft's
[portable scope contract](https://github.com/microsoft/winget-cli/blob/master/doc/specs/%23182%20-%20Support%20for%20installation%20of%20portable%20standalone%20apps.md).
Generation refuses pre-Windows tags, mismatched ZIPs/checksums and
an existing output directory. WhiteKiwi.locron remains a proposed identifier until availability and
community submission acceptance are verified. Managed ownership is read from the 64-bit view of
HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall: WinGetPackageIdentifier,
WinGetInstallerType=portable, a nonempty WinGetSourceIdentifier and InstallLocation must bind the
canonical selected executable. For older single-file portable registrations, TargetFullPath is
also an exact-path binding. ZIP registrations use their registered InstallLocation and versioned
relative executable path; a package-looking directory name alone is insufficient. This uses
[WinGet's ARP source](https://github.com/microsoft/winget-cli/blob/master/src/AppInstallerCommonCore/PortableARPEntry.cpp)
and [portable installer source](https://github.com/microsoft/winget-cli/blob/master/src/AppInstallerCLICore/PortableInstaller.cpp).

The replacement adapter must retain a narrow exclusive read/write gate opened without reparse
traversal, with handle-bound current-SID/private-DACL validation inside a guarded parent. Existing
mapped holders must refuse this gate, and new launch/read/write attempts must remain refused
while it lives. Retain the exact handle and parent guards through the selected replacement and
rollback; reopening it through ordinary shared private-file helpers would collide with the gate.
Delete sharing alone does not prevent another same-SID rename, so path-based rename cannot be
presented as exact-handle identity proof. The selected adapter must use an audited safe API;
workspace unsafe code, dynamic P/Invoke, in-place replacement and optimistic success are refused.
Native adversarial tests must prove mapped-holder refusal, blocked new launch, competing-leaf
refusal and rollback under the live gate. The concrete recovery design also remains subject to
the documented native crash-phase gates.

Reading a WinGet-owned executable source uses a separate retained no-reparse file/ancestor guard.
Require the current SID as file owner and refuse every effective nontrusted-account allow entry
granting file write, append, delete, WRITE_DAC or WRITE_OWNER; trusted SYSTEM/Administrators access
is permitted and other accounts may retain read/execute access. Inspect the descriptor on the
retained handle without taking ownership or silently repairing the package. Helper, journal and
standalone destination paths keep their stricter current-SID/SYSTEM-only private policy. Native
tests must preserve permitted package read access and refuse broad write/delete/control rights.

### Native runtime fixtures

Keep portable process/output/HTTP behavior under native Windows tests, with a private managed
child of each temporary fixture root. A self-spawned native Rust test fixture exercises exact argv,
raw stdout/stderr, immediate descendant creation, root-first exit, cancellation, timeout and
kill-on-close without Git Bash or an installed scripting runtime. Signal-number/Unix process-group
assertions remain Unix-specific; equivalent Windows tests prove owned Job tree behavior.

The TLS trust fixture uses the already-locked tokio-rustls =0.26.4 with an explicit AWS-LC provider
and repository test-only self-signed DER certificate/key. It never installs a certificate into a
trust store, invokes external OpenSSL, or changes production trust policy. All architectures verify
an untrusted local TLS peer is a retryable transport failure. Bounded native IPC tests cover valid
wake/cooperative control, malformed frames, idle/nonreading clients, first-instance collisions,
role/lifetime separation and listener teardown while preserving durable reconciliation.

### Change order and verification

1. Review SPEC, source-backed FINDINGS, these decisions and Project drafts; freeze the minimum
   contract and safe interface versions before implementation. Verify: review resolves all contract
   gaps and each draft has concrete criteria; no premature support claim.
2. Add build/state/configuration foundations with native x64/ARM64 compile/core/store CI, then
   process-tree/wake behavior and shared Windows fixtures. Verify: Rust 1.94 build, platform
   environment parity and real descendants/cancel/timeout/recovery pass on both architectures.
3. Enforce ACL/reparse privacy and role-control/Task Scheduler lifecycle. Verify: actual standard
   account isolation, ancestor-swap refusal, occupied/remote pipes, long-running settings and
   graceful exact-instance refresh/removal pass; Unix behavior remains green.
4. Add immutable ZIPs, installer, rollback handoff and WinGet generation/validation. Verify: corrupt
   source/digest/archive/version/architecture, locks, interruption and ownership preserve a working
   installation and durable state; historical release inventory and macOS signing gates still pass.
5. Run same-revision clean Windows 11 standard-user acceptance and complete support/operator docs.
   Verify: no compiler/shell-toolchain runtime prerequisite, both native architectures, reboot/login,
   locked-session and security-policy evidence; merge/release/channel evidence precedes Done.

Keep fixture portability meaningful: genuinely Unix signal tests may be target-specific, but
Windows equivalents must cover the same observable contract. Do not skip scheduling, privacy,
process-tree or ownership coverage merely to get a green Windows job. Signing eligibility,
credentials and business-history checks remain outside these steps.

## Project-only execution tracking migration (2026-10-02)

This is an administrative workflow change within the unchanged product specification. FINDINGS
§45 establishes the frozen source and the Project API constraints. Move execution scope, ordered
tasks, Verify criteria, evidence and progress to a private `locron` GitHub Project owned by
WhiteKiwi and linked to WhiteKiwi/locron. Product contracts, design decisions and durable research
remain repository documents. New execution tasks are Project-only drafts; the public bug-reporting
issue tracker remains available to external contributors.

Use the original `docs/TODO.md` at `93b4144cc5f9d056609e3f88e37e9451ea2ad506`, SHA-256
`aeaf61423326384d4e38709da731e764029777c6bf808c4d06403372b6ee9d52`, as the migration inventory:
69 items, 68 checked and one unchecked, across 15 phases. Newly added administrative migration
steps are not part of that frozen inventory. Leave the existing TODO archive and inactive BACKLOG
ideas as historical/deferred material, and do not close the remaining hosted cache verification.

The parent owns GitHub mutations and PR publication. The development sub-session owns the scoped
documentation cutover and a private, reviewable one-off migration helper. No production code,
workflow, version, published release, installed service or scheduled job changes belong here.

1. Freeze and review the migration contract and source inventory.
   **Verify:** source bytes/hash and the 69/68/1 inventory agree; every source item has Verify;
   research resolves draft/item identity, field updates, duplicate prevention and live references.
2. Build a deterministic manifest and review the complete plan before destination writes.
   **Verify:** Legacy IDs `LOCRON-TODO-001` through `LOCRON-TODO-069`, source order 1–69 and immutable
   source-line links are unique. Bodies retain exact source item text and any shared phase preface.
   Join wrapped titles for display; use a word-boundary abbreviation of at most 240 characters
   when needed for readability, retaining the complete original title and requirements in the body.
3. Create and configure the private destination and migrate the frozen items.
   **Verify:** Project identity, repository link, Todo/In Progress/Done options, Phase, Legacy ID
   and Source order fields are read back. Active, History and All tasks views separate unfinished
   and completed work. Sequential acknowledged writes preserve item/content IDs in private
   checkpoints; an uncertain response stops writes and requires readback rather than blind retry.
4. Verify the entire remote migration before changing repository authority.
   **Verify:** pagination is exhausted; exactly 69 unique unarchived DraftIssue items have matching
   manifest title/body/phase/order/Legacy ID, with 68 Done, one Todo and zero In Progress. Repository
   issue inventory and closed issue #4 are unchanged. Store a static acceptance receipt and mapping.
5. Cut over the maintainer workflow without losing historical evidence.
   **Verify:** `docs/planning/TODO-2026-10-02.md` is byte-identical to the frozen source;
   `docs/PROJECTS.md` records workflow and static source-to-draft mapping; `docs/TODO.md` is only a
   pointer and contains no live checklist. AGENTS, contributor/PR guidance and document maps agree
   on Project authority. Historical archive bodies remain unchanged and BACKLOG stays inactive.
6. Review and publish the documentation-only pull request.
   **Verify:** scoped diff, local Markdown links, source snapshot/mapping and remote acceptance
   checks pass; no product source or runtime state changes are staged. Commit/push the exact
   reviewed files, attach the opened PR and report its hosted checks. PR merge is a later action.

### Verified migration evidence

Steps 1–4 passed before the repository authority cutover: the parent created private Project
[WhiteKiwi/locron #4](https://github.com/users/WhiteKiwi/projects/4), configured the three table views
and metadata fields, and verified all 69 unarchived DraftIssue rows through exhausted paginated
readback. Exact manifest titles/bodies, phase context, Legacy IDs, source order, and the original
68 Done/one Todo status distribution matched. Repository issue #4 and the full issue inventory
remained unchanged. The private helper's nine deterministic fixtures passed, including uncertain
writes, acknowledged-ID recovery when list results lag, null-node refusal, and pagination.

The source snapshot, static 69-ticket map, authority references, local links, and archive/BACKLOG
body-preservation checks passed for step 5: 69 mapping rows retain 68 Done/one Todo, the pointer
has no checkboxes, and all 50 local Markdown targets in the changed documents exist. The snapshot
SHA-256 matched and `git diff --check` passed. The parent owns documentation-only PR publication;
publication and hosted-check evidence is recorded on the migration PR. The requested deliverable
is an opened PR; merging it requires a later instruction.
The historical Done states preserve their existing evidence rather than asserting that all past
checks were rerun during migration. Future tickets require Verify evidence and any explicitly
required merge/publication before Done; see [`PROJECTS.md`](PROJECTS.md).

## Acceptance startup observer correction before v0.9.6 tagging (2026-09-30)

FINDINGS §43 records the merged-main MSRV failure and an established harness interference path;
the discarded daemon stderr leaves the exact hosted fatal cause uncertain. This is a test-only
verification correction, so the product SPEC and runtime lock behavior stay unchanged.

Replace the acceptance helper's active `try_prove_free` startup probes with a passive read of
existing `LockMetadata`. Require the spawned child's PID and child liveness; retry only absent,
partial, or stale metadata within the existing startup deadline. Never acquire or modify the
observed lock. Capture daemon stderr in a fixture tempfile, read a bounded diagnostic on exit or
timeout, and establish the RAII child guard before startup observation so failures reap fixture
children. Preserve the existing acceptance scenarios, startup budgets, and restart semantics.

Verify passive no-write/no-lock observation with deterministic missing/malformed/stale/held-lock
fixtures, and verify bounded stderr with a deliberately failed child. Run the acceptance suite
on Rust 1.94 and the pinned lint toolchain, plus formatting, locked warnings-denied Clippy and diff
checks. The parent reviews the scoped branch and exact hosted PR/main CI and Audit before tagging;
a blind rerun of the failed unchanged test is not this correction. No manager, Keychain, installed
binary, publication, dependency or product source operation belongs to the development session.

## Second feedback and v0.9.6 signed macOS release (2026-09-30)

Implement the reviewed SPEC amendments with FINDINGS §42 as the evidence record. The reporter
confirmed two same-name background entries; read-only host inspection identifies one daemon and
one dashboard, not two instances of either service. Preserve both independent services. No schema,
dependency, app bundle, installed binary, or live Locron state change belongs to this work.

### Human target summaries and registration-file stability

Change the shared redacted human target formatter to `process:`, `shell:`, and `http:` for list,
show, and add/update summaries, including dry runs. Preserve JSON target kinds, argv order,
warnings, and redaction. Update exact-output and terminal-width tests and operator examples;
keep LAST RUN visible under the current TARGET-only truncation rule.

In the macOS registration writer, compare the desired plist bytes with the existing regular
registration file before writing. Matching content with safe existing permissions leaves the
file/inode/mtime unchanged. Missing or different files retain the current write path; unsafe
permissions must still be repaired. Propagate inspection errors. Leave service-manager ordering,
explicit enable of unloaded services, manual-lock deferral, dashboard independence, and the
loaded SIGTERM/KeepAlive restart intact. Do not add bootout/bootstrap or kickstart to the ordinary
update path. Changed-definition reload remains deferred because the present cached-definition
and bounded shutdown policy need separate design. Test actual temporary file metadata as well
as existing fake-port order and machine envelopes; do not execute real-manager tests on this host.

### Recognized macOS distribution identity

The existing release build jobs remain hosted and produce unsigned internal archives. Add a
tag-only macOS signing job after those builds, restricted to the exact repository and a release
tag. It selects a release-specific foreground ephemeral runner on the maintainer's Mac; it is
not a persistent runner service and accepts one reviewed job before de-registration. The parent
publication session owns registration, exact-run observation, and cleanup. Do not expose this
runner to PR or arbitrary-branch work. Use only `locron-signing-${{ github.run_id }}` as the runner selector, with
`--no-default-labels` on the one-job runner. Private signing material remains in the existing login Keychain, and its search list,
ACLs, defaults, and contents remain unchanged.

A standard-library signing helper validates exactly the two expected Mac archive layouts,
rejects traversal/symlink/unexpected members, verifies architecture and version, and stages copies
in a private temporary directory. Acquire the existing shared `fcntl.flock` at
`~/Library/Caches/home-hub/apple-signing.lock`; never unlink that lock or use a different locking
protocol. Sign both executables with the existing exact Developer ID Application identity,
`--timestamp`, `--options runtime`, and stable identifier `dev.locron.cli`. Verify strict signatures,
expected team `4H4Z446LHS`, identity, timestamp, runtime flag, and executable architecture.

Submit a ZIP containing both signed binaries with the existing `c6s-notary` login-Keychain
profile, require accepted status, and inspect its log for issues. Bare executable tickets and ZIP
files cannot be stapled; validate the online `notarized` code requirement with
`codesign --check-notarization` after acceptance and document this limit. Repack the original archive layouts with the final signed binary bytes, README/licenses,
and correct executable mode. Stage all outputs before replacing the output directory's files;
failure must not supply unsigned or partially signed publication inputs. Retain a compact
non-secret verification receipt as an internal workflow artifact, not an added public download.

Upload final archives as a separate signed-macos artifact. The registry publisher requires signing
success, and the GitHub publisher downloads only the Linux build artifacts and signed-macos output.
Generate checksums after signing; Homebrew consumes those same archive bytes. Verify the expected
four archive/four package inventory before publication. An existing versioned release is only
accepted after its inventory and hashes match the intended files; never clobber its assets.

Use deterministic archive/signature/notary/code-requirement fixtures for fail-closed boundaries, source
workflow checks for signed-only inputs and job dependencies, and a local disposable real signing
rehearsal before tag publication. A recognized certificate supplies developer attribution for an
unbundled executable; it does not create an app identity, guarantee UI grouping, remove historical
BTM records, or prove that every macOS notification ceases. Do not claim those outcomes in notes.

### Published updater compatibility

After GitHub publication, run a real v0.9.2 standalone updater on ephemeral hosted Linux x86_64.
Verify the historical archive checksum, create a temporary copy with the exact standalone receipt,
and guard the expected latest release tag. Preserve HOME; isolate `LOCRON_STATE_DIR` and
`XDG_CONFIG_HOME`, and remove manager-runtime and update-origin/backend overrides. The systemd
no-session branch returns guidance before registration, and isolated config prevents dashboard
registration. Assert version/envelope/digest results, no unit files, and unchanged fixture job
state. Never run this smoke on the owner's Mac. Add deterministic parsing/refusal fixtures to CI
and preserve the existing checksum parser/generation and service tests.

### Release and verification order

Advance workspace metadata, exact internal requirements, and only the five lockfile records to
0.9.6; preserve Rustls 0.23.45 and unrelated dependencies. Curate UTC-dated user-facing notes for
target distinction, identical-plist stability, and official macOS signing/notarization. Record
no promise of notification suppression, one combined background entry, or changed-definition
reload. Update CLI/RELEASE documentation and all three planning documents as decisions arise.

Pass version checks, formatting, warnings-denied locked Clippy, complete locked workspace tests,
both cargo-deny groups, signing/smoke/script fixtures, actionlint, shellcheck, and clean MSRV
workspace package/publish dry runs. Review staged and unstaged changes, commit the scoped release
candidate, and pass hosted PR CI/Audit before merging. Verify exact merged main checks before its
immutable annotated v0.9.6 tag. The parent then starts the one-job signing runner for that reviewed
tag and observes signing, publication, old-updater smoke, registry inventory, assets/digests,
Homebrew, and runner cleanup. Read back this host's Locron registration/PIDs and signing Keychain
metadata to confirm preservation. Inventory durable publication state before any retry.

## Rustls advisory remediation (2026-09-28)

This is dependency maintenance under the existing safe-by-default product contract; it changes no
Locron command, API, schema, or supported platform, so the frozen specification remains unchanged.
The hosted and local audit evidence and the upstream fixed-version boundary are recorded in
`docs/FINDINGS.md` §38.

Update only the locked `rustls` package to the first patched release, `0.23.45`, using Cargo's
targeted resolver command. Accept a related transitive lockfile change only if Cargo requires it;
do not introduce a direct manifest pin or an advisory ignore. This keeps the security gate active
and minimizes unrelated dependency movement after the recent Dependabot update.

Verify the selected lockfile version and dependency path, run both cargo-deny audit groups, then
check the locked workspace against the Rust 1.94 MSRV and run the relevant test gate. Confirm the
hosted pull-request Audit and CI jobs on the exact proposed revision before publication. If the
patched crate cannot satisfy the current manifest or MSRV, revisit this plan before expanding the
dependency change.

## v0.9.4 security patch release (2026-09-29)

Prepare v0.9.4 from the reviewed main-branch Rustls fix. Advance the one workspace version and
all four exact internal dependency requirements in lockstep, then let Cargo refresh only the five
workspace package records in the lockfile. Preserve the patched `rustls 0.23.45` resolution and
avoid unrelated dependency updates, source changes, or audit exemptions.

Create a curated `Security` changelog entry stating that the published TLS dependency is updated
to resolve RUSTSEC-2026-0285, with no claim that scheduler behavior changed. Use the release's UTC
date and update the Unreleased and v0.9.4 comparison links. The existing v0.9.3 tag and artifacts
are immutable.

Verify version agreement, the patched dependency graph, both cargo-deny groups, formatting,
warnings-denied Clippy, complete locked workspace tests, and a clean-tree workspace package and
publish dry run. Use a release branch and pull request so the exact candidate receives hosted CI
and Audit checks before merging. Recheck the merged main revision and its hosted checks; tag only
that clean, verified release commit with an annotated v0.9.4 tag.

After tag push, wait for the release workflow and verify all five exact crates, the registry
installation gate, GitHub Release assets and checksums, and the Homebrew formula at v0.9.4. If the
registry becomes partial or a publication stage fails, inspect the exact durable state before any
retry; never move the immutable tag or overwrite an existing package version. The release does not
change this machine's installed Locron binary or running services.

## First feedback triage (2026-09-30)

Implement the accepted SPEC amendment with no schema migration, dependency changes, release
publication, or mutation of installed jobs/services. Findings §40 records source and release evidence.

1. Normalize checksum filenames by removing the optional binary-mode `*` marker and a single
   leading `./`, then compare the whole name. Keep 64-hex validation, lowercase comparison, and
   download verification before replacement. Reject unrelated paths instead of matching basenames.
   Generate future release checksums from bare filenames rather than `./*.*`; this is necessary
   for older updaters to fetch the fixed build. Do not edit historical release assets.
2. Compose advisory direct-process registration checks from the same `engine_target` resolution
   used by CLI doctor, using current global settings or equivalent non-mutating defaults for a
   new state directory and a non-durable diagnostic attempt. Check the normalized effective job,
   including environment precedence, relative PATH entries, and CWD. Warnings go to human stderr
   or the JSON envelope's `warnings`, on real and dry-run add/update paths. Resolution failures
   never reject registration. Preserve environment warnings. Do not execute the target or fetch
   HTTP/body files. For non-resolution configuration failures, report inability to check without
   exposing arbitrary environment-file content. Only a missing bare `http` executable, supported
   uppercase HTTP method, and valid HTTP(S) URL trigger the `--http METHOD <URL>` hint; never echo
   the supplied URL or trailing argv in the hint. CLI-only scope avoids widening API/MCP contracts.
3. Add a store list projection that reads each live job and its latest retained run identity/state
   in one SQLite snapshot, ordered by job name and with latest chosen by
   `requested_at_us DESC,id DESC`. Preserve `list_jobs` for existing consumers. Select only the
   run fields needed, without parsing snapshots, querying anomalies, or using history's cap.
   JSON list rows gain `latest_run: {id,state}` or `null`; other job projections stay unchanged.
   The human table gains `LAST RUN`, showing the current durable state or `none`. Reserve this
   column when fitting TARGET so state is always visible; keep `--no-trunc` and pipe behavior.
   Active runs, skipped/cancelled states, removed identities/name reuse, and pruned history are
   observations, not inferred failures or success. No consecutive-failure counter is introduced.
4. Update CLI/operator guidance and the Unreleased changelog, then verify the focused updater,
   installer, registration, store, and human/JSON contracts plus a release-checksum fixture in
   the existing CI shellcheck/script gate, locked workspace tests,
   warnings-denied Clippy, formatting, workflow syntax/actionlint where available, and diff checks.

The development sub-session owns code, contracts, and checklist evidence after plan review.
The parent reviews the resulting diff and test report. Existing output redaction and dry-run
non-mutation are explicit regression gates. This scope intentionally leaves installed-binary
replacement and release publication for a separate requested release.

## v0.9.5 feedback correction release (2026-09-30)

Prepare the reviewed feedback corrections as one lockstep patch release. The release corrects
existing update/registration/inspection behavior without adding commands, execution policy,
dependencies, or a storage migration. Findings §41 records release preconditions.

Advance the workspace package version and all four exact internal requirements from 0.9.4
to 0.9.5, then use Cargo to refresh the five workspace lockfile entries without dependency movement.
Move the reviewed user-visible Unreleased entries into a curated 0.9.5 entry dated 2026-09-30 UTC,
and advance comparison links. Preserve the bare-checksum generation and its CI contract so older
updaters can verify the new build.

Verify version agreement, patched dependencies and both cargo-deny groups, formatting,
warnings-denied locked workspace all-target Clippy and tests, workflow/script checks, and
Rust-1.94 clean-tree workspace package/publish dry runs with no upload. The development sub-session
prepares metadata and evidence; the parent inspects/stages/commits the scoped tree, creates a PR,
and requires the exact head's CI/Audit checks before merging. Recheck main's hosted results before
creating and pushing the immutable annotated v0.9.5 tag.

Wait for the release workflow to complete, then verify all five registry versions, all four
archives/four Linux packages/checksums/installer, bare checksum names and archive digests, and the
Homebrew formula. If publication becomes partial, inspect the exact durable state before a bounded
retry; never move a tag or replace existing versioned assets. Verify a downloaded macOS arm64 binary
and isolated no-service installer smoke. Do not replace the installed binary or restart live services.

## Milestone approach

Implement from the inside out: deterministic domain behavior, transactional storage, daemon orchestration and runners, then the thin CLI. This order makes time, crash, and concurrency policy testable before it is coupled to a real clock or terminal.

1. `locron-core` defines normalized commands, schedules, policies, state transitions, and testable ports.
2. `locron-store` implements the architecture's persistence invariants with real SQLite transactions and migrations.
3. `locron-engine` implements the complete daemon runtime, first against fake time/execution and then against process, shell, and HTTP runners.
4. `locron` composes those layers for short-lived commands and `locron daemon run`; it does not acquire daemon responsibilities.

This layering costs some domain/store mapping and trait design up front. It is justified by deterministic testing and by future viewer, MCP, and desktop surfaces needing the same behavior without depending on CLI parsing or SQLite layout.

## README product narrative (2026-08-24)

The README leads with **“Cron that explains itself.”** and earns that claim with shipped,
inspectable behavior. Its first screen moves from cron's silent-failure problem to locron's durable
history and explanations, then shows a short CLI path using the real `add`, `preview`, and `why`
syntax and current human-rendered output. Run-specific follow-up uses `history` and `why --run`;
captured output remains separate because the queued-run confirmation or `history --format json`
must supply the canonical run ID required by `locron logs <RUN_ID>`.

The capability story follows the specification's accepted order: explainability (`why`, `history`,
`logs`, `preview`, `doctor`), reliability (explicit missed-run and overlap policies, durable
occurrence identity, recovery), then agent integration (`--format json`, dry-run mutations, and
MCP over the same application boundary). SQLite, WAL, migrations, and process-group details appear
only as supporting evidence. Installation and service-start guidance stays unchanged in substance,
and all examples must be checked against the shipped help and isolated scratch-state output. The
README advertises the shipped `locron explain` consolidated summary while retaining `why` as the
detailed job/run diagnostic. It does not advertise richer event-derived decision traces or direct
machine sleep telemetry.

## README information architecture refresh (2026-08-25)

Keep the accepted “Cron that explains itself.” positioning, but make the first screen answer four
questions without relying on the banner or badges: locron is a local scheduler; it records and
explains scheduled execution; it is for developers and automation agents; and it supports macOS
and Linux. Add one restrained supporting line for the human-and-agent audience. Do not describe
locron as AI-powered, a cron wrapper, an MCP server, or a GUI for an operating-system scheduler.

Reorder the existing material into a short operational story. The opening tour uses only shipped
syntax and demonstrates create, preview, inspect, and explain; installation follows early enough
that a convinced reader does not need to cross the reliability and agent sections first. A compact
problem statement explains why laptop sleep, restarts, network loss, missed occurrences, and
overlaps require durable scheduling facts. The reliability section then maps those conditions to
the existing missed-run, overlap, retry, timeout, cancellation, supervision, output-retention, and
startup-reconciliation behavior without claiming direct sleep detection or exactly-once external
effects.

Describe human and machine interfaces as views over the same scheduler model. Human surfaces are
the readable CLI and optional loopback-only dashboard; automation surfaces are the versioned CLI
envelope, non-mutating dry runs, and MCP. A small Mermaid feedback loop may show plan, preview or
dry-run, mutate, execute, inspect, and adjust, but it must remain secondary to concrete commands.
MCP keeps its current configuration example and exact shipped inventory while no longer carrying
the entire agent story by itself.

Add two compact scope clarifications after the practical workflow. First, explain without a
winner-takes-all feature matrix that cron is a portable scheduling primitive and launchd/systemd
are native service managers, while locron owns a consistent cross-platform job/run/attempt model,
history, policies, and explanations. Second, show the actual component boundary: CLI, dashboard,
and MCP enter the shared application boundary; the engine schedules and supervises process, shell,
and HTTP targets; SQLite stores durable state. Link to the architecture document for invariants
instead of expanding storage and process details in the README.

Preserve installation-channel ownership guidance, dashboard token and loopback boundaries, target
and schedule examples, documentation links, contribution guidance, and licensing. Verify every
command against CLI help or contract tests, validate local Markdown links, and run Markdown/style
and diff checks available in the repository. No source, schema, API, or behavior change belongs in
this documentation-only refresh.

## Consolidated job explanation implementation (2026-08-24)

The new `locron explain NAME_OR_ID` command remains a thin CLI composition over existing durable
facts. One shared current-job explanation helper resolves the live job, deserializes its normalized
definition, calculates its next schedule occurrence at one sampled wall-clock instant, reads active
runs, checks daemon ownership, and loads global concurrency. Both `why NAME` and `explain` consume
these facts. The existing `why NAME` behavior remains unchanged, including its calculated next
occurrence and overlap-oriented eligibility for a disabled job; `explain` alone suppresses the next
occurrence and reports `disabled`, as required by its consolidated-summary contract. For an enabled
job, `explain` reports `subject_to_admission` rather than claiming capacity is currently available;
the overlap decision and configured global limit are separate facts. It does not duplicate the
store's transactional admission simulation or imply that reading the report reserves capacity.

A focused store read supplies the most recent run and latest anomalous terminal run with two bounded
queries in one read transaction, using the existing run mapping and the canonical
`requested_at_us DESC, id DESC` order. The anomaly predicate uses the persisted terminal-state
vocabulary rather than reason-text matching.
This avoids relying on the general history command's 1,000-row presentation cap when retention has
not yet pruned an unusually large burst. Live-job resolution happens before the read, which preserves
the soft-delete boundary and prevents a reused name from collecting the removed identity's history.

A dedicated redacted run-summary projection selects only canonical identity, trigger, nominal and
request times, current/final state, derived actual-start/duration facts, finish time, and durable
reason from the existing observable-run representation. It excludes the immutable target snapshot,
attempt details, and event details because `explain` is a summary; `why --run` remains their detailed
surface. Human output and the JSON `data` object are both rendered from one JSON-shaped report.
Human rendering distinguishes known absence (`none`: no run/anomaly, disabled next occurrence,
manual nominal time) from a fact that is not yet known (`unknown`: start, finish, duration, or
reason), and translates the machine eligibility codes into readable phrases. No schema migration,
new durable state, new dependency, or MCP surface is required; the added store operation is read-only
and uses existing indexes and mapping.

Edge cases are pinned as contracts: no history, only successful history, one anomalous run serving
as both latest and anomaly, an older anomaly behind a newer success, an active latest run, a removed
job whose run remains explainable by ID, a removed name reused by a new live job, and unchanged
`why NAME` output for a disabled job. Redaction tests put sensitive target configuration in the job
and assert it appears in neither human nor JSON output. The generic help-surface walk must discover
`explain` and its example.

## Accepted implementation decisions

### Accepted: identifiers and timestamps

Use RFC 9562 UUIDv7 for stable job, run, and scheduler-lifetime identities. Use parent-scoped increasing numbers for job revisions and attempts, and a database-local increasing integer for the event cursor. Keep user-editable names separate from identity.

Persist UUIDs as lowercase canonical text rather than 16-byte blobs. This costs some index and database space but keeps a local SQLite store directly inspectable and makes CLI, future HTTP/MCP, export, and logs use the same representation. UUID time ordering helps locality, but UUID-embedded time is never authoritative and semantic ordering always uses an explicit timestamp plus identity.

Persist instants as signed 64-bit Unix epoch microseconds in UTC and render them externally as RFC 3339 UTC strings. Preserve the configured timezone and schedule source separately. Measure elapsed time with a monotonic clock and persist durations in integer microseconds so wall-clock adjustment cannot produce negative execution time.

### Accepted: output storage

Stream stdout, stderr, and HTTP response bodies through one serializer into a per-attempt framed file under the user state directory. The versioned, length-delimited format preserves observed channel order and arbitrary bytes and records channel, sequence, monotonic elapsed time, and payload. A file is partial while active and is closed and atomically renamed on the same filesystem when finalized. Compression is not part of v1.

SQLite stores the logical output key, lifecycle, retained payload and physical file sizes, discarded-byte count, truncation fact/time, and pruning state. The path is derived only from durable identities. Output directories and files use owner-only permissions, and maintenance and readers do not traverse user-controlled symbolic links.

The 10 MiB per-run allowance covers retained payload across all attempts of that run. Writers keep draining after a per-run or global limit is reached, discard further bytes with saturating accounting, and do not change the target result. The 256 MiB global bound uses physical retained size: reclaim the oldest eligible terminal output first, then discard new capture if nothing eligible can be reclaimed. Render truncation from metadata so it cannot be confused with target bytes.

An attempt and logical output identity are durable before output creation and spawn. Startup repairs referenced partial files to their last complete frame and reconciles metadata; the attempt still follows `interrupted_unknown` recovery. Pruning uses durable pending state, file removal, and a completion transaction so it can resume after a crash. Delete verified unreferenced files only after a grace period.

This adds a small framed-file protocol and filesystem/database recovery work, but avoids SQLite BLOB/WAL amplification, supports efficient live follow, preserves stream order, and returns disk space immediately when output is removed.

Startup maintenance runs after scheduler ownership and stale-lifetime classification but before new
admission. It repairs every referenced partial artifact to its last valid frame, synchronizes and
renames that artifact to its final path, and then commits reconciled output metadata. A referenced
artifact with no safe regular file becomes `missing`. Existing `prune_pending` rows resume file
removal before their durable completion transition. Verified unreferenced regular files under the
managed output tree are removed only after a one-hour grace period; symbolic links and unexpected
filesystem objects are never followed or removed automatically.

The daemon also performs one bounded maintenance batch on startup and each safety reconciliation.
Output age/byte pruning completes before metadata deletion. Terminal run metadata is selected oldest
first when it exceeds the 90-day age bound, the fixed 1,000-per-job bound, or the configurable global
count bound (default 10,000). Active work is excluded. Filesystem deletion and SQLite transitions
remain separate restartable steps, and one pass considers at most 100 artifacts or runs. Maintenance
failure is reported as degraded diagnostics but does not authorize in-memory cleanup or discard
newly due work. The schema-v3 upgrade establishes the frozen 90-day default for databases created by
the earlier milestone schema, whose placeholder value was unlimited.

### Accepted: SQLite operation and daemon coordination

Bundle the tested SQLite library and configure each connection with WAL journal mode, `synchronous=FULL`, foreign keys enabled, normal locking mode, a five-second busy timeout, and untrusted schema features disabled. The durability cost is intentional because a pre-spawn run commit lost after power failure could permit duplicate external execution. State directories on network filesystems are unsupported.

The daemon has one serialized writer lane and at most three reader connections; each short-lived CLI process uses one connection. Writers use immediate transactions for read-modify-write operations. Transactions never span target execution or other external I/O. Checkpoint WAL periodically in passive mode and attempt a truncate checkpoint during graceful shutdown without waiting indefinitely for readers.

A CLI returns a stable database-busy error after its five-second deadline. The daemon retries required durable transitions instead of discarding them and stops new admission while persistence is degraded. It resumes only after the store becomes writable and the transition is committed. A permanent completion conflict — durable state that cannot accept the outcome after retry — is not retried forever: the engine logs it once and terminalizes the attempt as `interrupted_unknown` where the store permits, so a poisoned transition can never pin a run in `running` indefinitely. Completion idempotency checks compare durable identity fields (path, byte counts, truncation) and never the retry timestamp.

Use the Rust standard library's non-blocking exclusive file lock on a permanent owner-only lock file to enforce one daemon per state directory. Acquire it before migration and lifetime creation, retain the descriptor for the full daemon lifetime, mark it close-on-exec, and never remove the file on normal shutdown. Best-effort PID, lifetime UUID, start time, and binary version text aids diagnostics but never authorizes PID signalling or stale-lock breaking. OS lock release handles process death; durable scheduler-lifetime records explain prior work.

The daemon migrates after ownership acquisition. A CLI encountering an older schema may migrate only after it temporarily proves the daemon lock is free; otherwise it reports that a daemon restart is required. Revalidate the schema version inside the migration transaction. Reject databases newer than the binary. This prevents a new CLI from changing the schema beneath an older running daemon.

### Accepted: one-time automatic removal

`--delete-after-run` is represented by a `completion_action` in the immutable job definition and therefore in every run snapshot; absent values deserialize as `retain` for backward-compatible imports and existing state. CLI validation permits `delete` only when the effective schedule is `--at`. It is intentionally a definition-lifetime action, not a caller-attachment or output-retention setting.

When a scheduled one-time run with `completion_action=delete` reaches its final terminal transition, the store soft-removes its job in that same immediate transaction. Retry scheduling keeps the job live; manual runs never qualify. Pre-execution and runner-infrastructure terminal paths use the same predicate. The removal writes `removed_at_us`, disables the job, and leaves revisions, runs, attempts, events, and output artifacts referentially intact under normal retention. Atomicity prevents a crash from recording successful execution while leaving an auto-delete definition live, or from removing it before the terminal run exists.

History renders a removed job's retained name with a removed marker. A live name resolves normally; when no live job has that name, history may resolve the removed name. Once the name is reused, the old history remains addressable by its UUID.

### Accepted: durable CLI-to-daemon control

Commit job mutation, manual enqueue, cancellation intent, and global configuration changes to SQLite before attempting notification. Use an owner-only Unix datagram socket solely as a versioned best-effort wake hint. Do not send command content or treat the socket as a management API; on receipt the engine coalesces messages and rereads durable state.

The daemon creates the socket only after taking scheduler ownership. The owner may replace a verified stale socket on startup and removes its own socket on graceful shutdown. A missing endpoint, full socket buffer, send failure, or filesystem-socket path-length limit produces degraded latency rather than a failed committed command. State-path diagnostics expose whether wake acceleration is available.

The engine event loop waits for the earliest calculated schedule/retry deadline, datagram wake, attempt-completion wake, termination signal, or 30-second safety reconciliation. On every wake it samples wall time and reconciles cursors, covering suspend/resume and clock movement without turning the engine into a fixed 30-second scheduler. The earliest pending admission deadline (queued or retry-wait run) is read durably so an idle daemon admits eligible work at its deadline instead of at the next reconciliation boundary; an attempt completion notifies the loop so a freshly scheduled retry deadline is observed without a reconciliation delay.

Wait/follow observes the same committed run with bounded SQLite polling and framed-file reads. Client disconnection never emits cancellation. This avoids a v1 request/response protocol, authentication and reconnection state, and a hidden management server while retaining offline correctness and prompt normal operation.

### Accepted: fairness and replacement coalescing

Use durable round-robin admission. In one pass consider each eligible job once, beginning after the last durably admitted job, and admit at most one attempt from that job. Repeat passes while global capacity remains. Commit attempt creation and the new cursor together. Process, shell, HTTP, scheduled, manual, and retry work receive no hidden priority or weight in v1.

Order a job's ordinary eligible work by durable eligibility time and queue sequence. Treat a missed-run `all` batch as an ordered lane whose oldest non-terminal member, including any retry, gates the next member. This preserves oldest-first catch-up while permitting normal occurrences to interact with the active batch under the frozen overlap policy.

For `replace`, store at most one pending normal replacement candidate per job. A newer occurrence atomically marks the previous candidate `skipped_overlap` with a supersession reason and successor identity, then becomes the candidate. Persist one termination intent and do not send duplicate termination requests. Admit the newest candidate only after prior termination is confirmed; otherwise terminalize it with an explicit replacement failure. Replace a queued or retry-wait run without signalling. Do not coalesce members already selected into the same missed-run `all` batch.

This may leave a slot unused for the short duration of a pass and gives no preferential latency to manual work, but it provides deterministic sharing, bounded replacement state, and no general overlap queue.

### Accepted: runtime and dependencies

Use Tokio 1.x with only the multi-thread runtime, macros, time, process, signal, Unix networking, synchronization, I/O utility, and filesystem features required by the engine. Use `tokio-util` cancellation tokens and task tracking for structured shutdown. Blocking `rusqlite` operations remain behind store interfaces and run off Tokio worker threads; no workspace crate exposes Tokio types as domain values.

Use `rusqlite` 0.40 with default features disabled and bundled SQLite enabled. Use `reqwest` 0.13 with default features disabled, Rustls TLS, streaming, and JSON only. Redirect handling is engine-owned with reqwest automatic redirects disabled, so cross-origin sensitive-header removal and the 10-hop cap exactly match the product contract.

Use Jiff 0.2 for instants, civil time, IANA zones, system-local discovery, ambiguity classification, and RFC 3339 conversion. Read the operating system IANA database on supported Unix platforms and make missing timezone data a validation/doctor error rather than silently using UTC.

Do not depend on a cron evaluator. The core implements the accepted five-field grammar as bounded bit sets and performs field-jumping civil-time enumeration. It maps each matching civil minute with Jiff: gaps are skipped and folds select only the earlier instant. This avoids third-party DST behavior that conflicts with the frozen specification and keeps cron, interval, and one-time enumeration under one injected-clock test model.

Use `uuid` 1.x (`v7`, `serde`), `serde`/`serde_json` 1.x, Clap 4.x derive, `thiserror` 2.x for typed library errors, and `anyhow` 1.x only at the CLI composition boundary. Use `nix` 0.31 signal/process features behind Unix adapters, Rust standard file locking, `tracing`/`tracing-subscriber` for redacted diagnostics, `crc32fast` for framed-output corruption detection, `blake3` for non-secret audit hashes, and `base64` for arbitrary-byte machine output.

Use property and integration testing with `proptest`, `tempfile`, `assert_cmd`, and `predicates`; local HTTP fixtures use Tokio networking rather than adding a production server framework. Exact patch versions are resolved into the committed lockfile and must build on Rust 1.94. Default crate features are disabled where they would add alternate TLS, native system dependencies, implicit proxy behavior, or unused protocols.

### Accepted: command, storage, and persisted compatibility

Use the command and diagnostic contract in `docs/CLI.md`, including non-mutating dry-run, durable-fact `why`, repeatable verbose output, redacted debug traces, versioned JSON/stream envelopes, export/import versions, and stable exit categories.

Use the state discovery, logical schema, output framing, migration, and ownership contract in `docs/STORAGE.md`. Milestone 1 has no separate configuration file; global configuration is typed durable state. Treat machine field names, policy vocabulary, export schema, frame version, SQLite migration order, and stable IDs as compatibility surfaces. Human prose and private table layout are not public APIs.

Normalize add and update through one job-definition overlay/validation path. Add supplies required
schedule and target values plus defaults; update begins with the current typed definition and applies
only explicit tri-state changes. Compare normalized values before persistence, reject no-op updates,
and pass the current durable global concurrency into policy validation. Persist one revision and one
new cursor row in the same transaction; carry the prior cursor for a non-schedule edit and use commit
time for a changed schedule.

Represent HTTP headers as typed inline or effective-environment sources in the normalized domain.
Expand success ranges before persistence, normalize JSON bodies to bytes, and resolve header env
sources only at execution. Central redaction removes inline env/header/body payloads from normal
rendering and records explicit omission paths in safe exports.

Use typed `locron.export/v1` documents rather than reusing inspection JSON or serialized database
rows. The CLI validates schema, settings, IDs, names, definitions, omissions, plaintext acceptance,
and duplicate input before mutation. It computes deterministic destination actions, then one
immediate store transaction revalidates destination identity/name facts and applies settings plus all
job creates/updates. A failure rolls back the entire import. History/output import remains deferred
and an explicit requested history export fails rather than returning partial data.

## Scheduler and runner implementation

### Schedule reconciliation

Implement schedule evaluation as a pure operation over a normalized revision, durable cursor facts, previous events, and an injected clock. Cron uses the configured IANA or symbolic system-local zone; interval schedules advance by whole multiples from the durable anchor; one-time schedules yield at most one scheduled occurrence.

On engine startup, wake, normal tick, and observed job revision, reconcile `(cursor, now]`. Apply start deadline, then `skip`, `latest`, or bounded `all`, then persist unique runs and bounded range summaries with the cursor in one optimistic transaction. Resolving a due one-time schedule also marks its current job revision disabled in that transaction, whether the occurrence executes or is explained by missed-run policy; a manual run never performs this transition. On a conflict, recalculate from fresh state rather than committing a partial batch.

A backward wall-clock move does not rewind the durable cursor or recreate an occurrence. Re-enabling evaluates disabled elapsed time under missed-run policy and never resets an interval anchor. A new schedule revision begins at its creation/explicit anchor boundary and cannot backfill time before it existed.

Disabled elapsed time is a dedicated nullable cursor fact, never an `updated_at` comparison. Disable
sets it once, re-enable preserves it, and successful cursor/materialization clears it. Migration from
schema v1 leaves existing rows NULL rather than guessing historical disablement.

Trade-off: cursor-driven reconciliation requires more durable state than recomputing only the next time, but makes sleep, restart, timezone change, and missed ranges explainable and idempotent.

The bounded reconciliation implementation separates occurrence *selection* from range accounting.
Interval schedules derive first/last indexes and counts arithmetically. Calendar schedules compile
one 400-year Gregorian match cycle, reject a calendar expression with no possible matching civil
date, and use cycle-position binary search to jump directly between matching dates while retaining
at most `catch_up_limit + 1` matching civil times. Calendar range counts use the same cycle arithmetic
plus explicit timezone-gap correction, so summary counts do not create or visit every occurrence or
elapsed UTC minute. Gap correction is linear only in actual timezone transitions inside the range;
the supported Jiff civil domain (-9999 through 9999) gives this work a hard date bound and no tzdb
transition pattern is assumed to repeat every 400 years.
Compilation is distinct from reconciliation: one reconciliation pass shares exactly one compiled
calendar object across deadline accounting, eligible accounting, newest selection, and summary
boundary lookup. The daemon keeps a revision-keyed compiled-schedule cache, so unchanged jobs do not
rebuild their Gregorian cycle on every safety tick; a revision change gets a new cache entry and
symbolic local timezone resolution remains per pass rather than being cached. Tests compare a bounded
oracle matrix and instrument compilation count, while a repeated-reconciliation work-bound test
guards against elapsed-range-dependent compilation or allocation.
One-time schedules remain a single comparison. All three return the same pure reconciliation result:
the newest eligible window in chronological order and zero, one, or two compact range facts for
deadline exclusion and policy/limit omission.

The pure reconciliation input also carries an explicit missed/normal boundary derived from the
durable reason for the pass. Startup, disabled elapsed time, and detected suspend/downtime mark their
elapsed range missed; a steady-state schedule wake identifies the exact normal boundary it was
waiting for. No fixed wall-lateness grace or safety-tick duration is allowed to infer this policy,
because changing daemon latency must not change an occurrence from normal to missed. Start-deadline
comparison is inclusive at the cutoff: an occurrence exactly `deadline` old remains eligible.

The engine receives paired wall/monotonic clock samples plus a timezone resolver port; its
coordinator supplies the explicit durable elapsed-range classification described above. A material
wall/monotonic divergence is an explicit clock-jump or suspend reason classified as missed; it is
not nominal-time lateness and does not depend on the safety interval. A steady-state `Normal` input
splits by occurrence boundary: its newest due occurrence is the one normal wake boundary, while any
older elapsed prefix is explicitly reconciled as missed under the job policy. Recovery,
disabled, and suspend ranges are wholly `Missed`. This split retains at most
`catch_up_limit` missed occurrences plus one normal occurrence, records an exact compact summary for
the omitted prefix, and never silently drops work when a safety pass covers multiple boundaries.
One reconciliation pass samples
them once, then uses those immutable inputs for every job in that pass. A symbolic
`local` schedule therefore follows a resolver change on the next pass, while a fixed IANA schedule is
unaffected. Tests use mutable fake ports to model disable/re-enable, wall-clock jumps, suspend
detection, and local-zone replacement without sleeps or process-global timezone mutation.

### Overlap, concurrency, and admission

Queued and `retry_wait` runs count as active for same-job overlap. Normal scheduled and manual occurrences use exactly `skip`, `replace`, or `allow`:

- `skip` produces a terminal `skipped_overlap` explanation when another run is active.
- `replace` records cancellation intent and admits the newest replacement only after prior termination is confirmed.
- `allow` admits to the job bound and records `skipped_concurrency` for a new normal occurrence beyond it.

Members already materialized in one bounded `all` catch-up batch remain durable and run in scheduled-time order. Global capacity exhaustion leaves otherwise eligible work queued. The global default is 16 with range 1 through 64. `skip` and `replace` have effective concurrency one; `allow` defaults to two.

Admission rechecks state and capacity transactionally immediately before creating an attempt. The
daemon uses a fixed semaphore of 64 permits as the process-local hard ceiling and passes its current
available permits to the admission store operation. In the same immediate transaction that selects
attempts, the store rereads the durable global setting, counts durable starting/running attempts, and
caps selection to `min(hard_guard_available, configured_limit - active_attempts)`. The setting remains
validated to 1 through 64. This atomic recheck means a CLI setting writer serializes wholly before or
after admission; a stale pre-read cannot over-admit after a decrease or hide capacity after an
increase. Because one daemon serializes ticks, attempt tasks acquire their hard-guard permits before
another admission pass; a concurrent completion can only make the calculation conservatively small.
A decrease below current active work yields zero admission without cancellation, and a later
increase is visible on the next wake/pass without rebuilding the semaphore or restarting the daemon.

The acceptance matrix is deterministic store/engine testing rather than timing-based daemon tests.
For each overlap policy it crosses scheduled, manual, and catch-up work with zero/global/per-job
capacity, then repeats the relevant rows after a durable limit reduction. Retry-wait occupies the
same-job overlap set, and an eligible retry remains the same catch-up lane member and precedes the
next member. A retry already durably selected is not rechecked against its original occurrence's
start deadline; deadline filtering is complete before the run's first attempt.

### Retry and crash recovery

Classify an attempt result before scheduling retry. Retries default to zero, are capped at 10, and use fixed or capped exponential delay without jitter. A retry intent and not-before time are durable and continue as part of the same run. Timeout qualifies only when explicitly selected; cancellation, configuration error, replacement, and unknown crash outcome never do.

Resolve user cancellation in one immediate transaction after reading the current run state. A
queued or retry-wait run has no process to terminate, so mark it terminal `cancelled`, set its finish
time and reason, remove any retry intent, and append the cancellation event in that transaction. A
starting or running run keeps its state and receives durable cancellation intent for the engine to
observe and confirm through normal termination. Missing identities are not found; every terminal
state is a stable conflict rather than an apparently successful repeated request.

The final mark-running transaction rechecks cancellation intent before external execution. If a
user or replacement request arrived while the attempt was durably `starting`, the transaction
terminalizes the attempt/run and its not-yet-created output artifact without spawning; the daemon
treats this as a normal no-execute decision, not persistence degradation. A transient store error at
this boundary never releases the admitted attempt or authorizes execution from memory. The task keeps
its capacity permit and retries the same transaction with capped exponential delay; every retry
therefore rechecks durable cancellation. Retry waiting is interruptible by daemon shutdown, which
leaves the durable `starting` attempt for next-lifetime unknown recovery and still never spawns.
Persistent failure reports degraded persistence and blocks that task until shutdown or a durable
ready/no-execute decision.

Required transitions are idempotent under a commit-success/response-loss ambiguity. Repeating
mark-running for the same already-running attempt returns ready only after rechecking that exact
attempt and current cancellation; cancellation observed on the ambiguous retry terminalizes before
spawn. The runner produces a target outcome once, retains it in the task, and retries durable
completion with the same capped backoff without re-executing the target. The composition clock is
sampled exactly once when that outcome returns; the resulting completion instant, retry eligibility,
and immutable completion command are reused across every retry rather than recalculated by the store
adapter. Shutdown interrupts this
retry and leaves the active durable record for next-lifetime unknown recovery. Repeating an already
committed identical completion succeeds idempotently; a mismatched result remains a conflict.

For a running process, termination is a bounded process-group-liveness state machine rather than a
detached sleep task. Send TERM once and observe both the owned leader wait handle and process-group
existence during the grace interval. Leader exit alone is insufficient because an in-group descendant
may still be running. At the grace boundary send KILL only while the group still exists, then require
both leader reap and group absence within a second bounded grace. Signal errors other than an already-gone group and an
unconfirmed child become a typed non-retryable termination-confirmation result. Its completion
transaction marks the attempt `interrupted_unknown`, keeps the original run in an active-blocking
quarantine with no live runner ownership, and terminally fails the queued replacement candidate.
Startup recovery preserves that quarantine but never inspects or signals its recorded PID/PGID;
therefore neither the failed candidate nor later same-job work can overlap an unconfirmed process.
Quarantine is never cleared merely by a daemon restart. Admission hard-blocks the quarantined job
regardless of a later `skip`, `replace`, or
`allow` revision. `skip` submissions retain ordinary overlap explanations; a new `replace` submission
terminally fails in its enqueue transaction instead of waiting forever; `allow`, scheduled, and
catch-up submissions become explainable overlap terminals rather than a permanent queued backlog.
The existing cancel application command gains one explicit acknowledgement mode. Its immediate
transaction accepts only the exact active-blocking `termination_unconfirmed` run, writes a dedicated
audit event, and terminalizes that run as `interrupted_unknown`. Ordinary cancel returns an
actionable conflict for quarantine, while acknowledgement of a non-quarantine or a repeated
acknowledgement is a stable conflict. Neither path reads or signals a recorded process identity.
Never signal a recorded stale PID/PGID after lifetime recovery. A process
that deliberately escapes its inherited group remains outside v1 process-tree control.

At daemon startup, the engine creates its lifetime and marks stale non-terminal attempts from an older lifetime `interrupted_unknown` before normal admission. It does not inspect, attach to, signal, or automatically retry a stale recorded PID/PGID. Fault injection must cover every transaction/spawn/completion boundary because persistence cannot prove whether an arbitrary external side effect occurred.

Retry classification and timing are one pure decision over the immutable run snapshot, attempt
number, known outcome class, and injected completion instant. Fixed delay never doubles;
exponential delay is `base * 2^(attempt-1)` with saturating arithmetic and the configured cap.
Durable `retry_wait` is the only restart-resumable retry source. Completion rejects retry plans for
cancelled, configuration, replacement, or interrupted-unknown outcomes, and startup recovery deletes
any inconsistent retry intent attached to an unknown run rather than synthesizing an attempt.

Crash tests use store fault points at durable admission, spawn acknowledgement, and completion. A
committed attempt without a committed result is always recovered as `interrupted_unknown`, including
the target-exited/result-not-committed boundary. A queued one-time occurrence is never rematerialized
because occurrence uniqueness and its advanced cursor are committed together.

Acceptance fault tests inject failure before admission, after durable admission while starting,
after the running acknowledgement, and after a target outcome but before completion commit. They
assert no pre-admission attempt, no pre-spawn execution, unknown recovery without retry after an
ambiguous external side effect, completion retry without a second execution, and one-time occurrence
uniqueness across every recovery boundary. The 1,000 catch-up limit is exercised through the real
adapter and SQLite store: exactly the newest 1,000 explicit runs are materialized, one compact event
accounts for the omitted prefix, admission begins at the oldest retained nominal time, and duplicate
reconciliation adds neither runs nor per-occurrence omission events.

### Process and shell runner

Resolve execution configuration immediately before an attempt. Direct execution preserves argv boundaries. Shell execution selects an explicit absolute shell and never loads interactive configuration implicitly. Resolve a bare executable from the locron-owned effective `PATH` and persist the selected absolute path before spawn.

Durable admission precedes runtime file reads and target construction. Resolve each admitted attempt
independently: if its immutable snapshot, environment file, body file, URL, path, or other runtime
configuration cannot be resolved, create and finalize its empty framed output artifact and commit a
known non-retryable failed attempt/run. Do not fail an admitted batch collection in a way that leaves
one or more rows running without an execution task.

Construct the environment in the order frozen by `docs/SPEC.md`, writing reserved `LOCRON_*` values last. Missing CWD/executable/env-file and malformed runtime configuration are known non-retryable failures.

Start process and shell targets in a new Unix process group. Timeout, cancellation, and replacement send `SIGTERM`, wait the configured grace, then send `SIGKILL`. Do not commit a confirmed cancellation until termination is observed. On normal daemon signal, engine admission stops, active work receives the natural-completion window, and remaining process groups follow the same termination path.

### HTTP runner

Validate method, absolute URL, body-source exclusivity, header sources, and success ranges before queueing. At execution, verify TLS, follow redirects only when explicitly configured, cap redirects at 10, and remove sensitive headers across origins. The attempt timeout covers the whole request.

Manual redirect handling follows conventional method semantics: `303` becomes `GET` except for
`HEAD`; `301` and `302` rewrite `POST` to `GET`; `307` and `308` preserve method and body. Whenever
the method becomes `GET`, discard the request body and entity headers. Sensitive authentication and
cookie headers remain stripped whenever the redirect crosses origins.

Map connection/name-resolution/transport failure, status 408/429, and 5xx to known retry-eligible failures. Other 4xx responses fail without default retry. Stream the response body through the same bounded capture mechanism as process output; truncation never changes HTTP success classification.

### Thin CLI composition

Short-lived commands parse input, create normalized application commands, invoke shared validation/storage operations, and render typed results. They do not duplicate schedule, admission, retry, or redaction policy. Manual submission commits before returning its run ID; wait/follow attaches to the same durable run and client disconnection does not cancel it.

The version flag is owned by the CLI instead of clap's built-in version flag: the built-in flag exits during parsing and cannot honor `--format json`. `disable_version_flag = true` removes it, and a top-level, deliberately non-global `-V/--version` boolean is handled in `main` before tracing or state discovery, rendering `locron <version>` for human output and the standard `locron.cli/v1` envelope with `command` `version` and `data` `{"version": ...}` for JSON output, using `env!("CARGO_PKG_VERSION")`. Keeping the flag non-global preserves the existing rejection of `locron add -V` as an unexpected argument. The subcommand field becomes `Option<Command>`; when version is present the subcommand is ignored, and when it is absent the CLI reproduces clap's two existing failure surfaces byte-identically: a bare `locron` renders the full help to stderr with exit code 2 through the container-level `arg_required_else_help`, and an invocation with other arguments but no subcommand re-parses the original arguments with `subcommand_required(true)` via `try_get_matches_from` to emit the native `MissingSubcommand` error and subcommand list (`Command::error` alone renders a raw error without the command context, so it is not used). Because the field is optional, `override_usage = "locron [OPTIONS] <COMMAND>"` keeps the required-command spelling in every help and error path, and the version flag's `display_order` places it after the automatic help flag to preserve the baseline option order. The full parse completes before the version short-circuit, so invalid arguments such as `-V --format bogus` are rejected with exit code 2 rather than printing the version. Note that `required_unless_present` on the subcommand field is silently ignored by clap_derive 4.6 and must not be relied on.

`list` and `remove` carry Clap 4 visible subcommand aliases (`ls`, `rm`) on their command variants. The aliases are visible rather than hidden so `locron --help` advertises the shorthand. An alias resolves to the same enum variant, so dispatch, option handling, help, and the canonical command names hard-coded at each `render` call site are untouched: machine output for an aliased invocation is byte-identical to the canonical spelling. This adds no dependency and changes no product behavior, so the frozen specification is not amended.

Human `list` output renders a docker-style aligned table instead of the shared pretty-JSON fallback: a header line plus one row per live job, columns NAME, SCHEDULE, TARGET, and ENABLED derived from the redacted durable record only. Schedule summaries are `cron 'EXPR'`, `every DUR`, or `at RFC3339`; target summaries are `run EXE [ARGS...]`, `shell CMD`, or `http METHOD URL`; ENABLED is `yes` or `no`. Alignment is hand-rolled from the maximum column width with no new dependency — the workspace has repeatedly rejected non-essential crates — and values are never truncated. An empty result prints the header alone, matching `docker ps` with zero containers. Only the human `list` path changes: the `list` dispatch arm in `execute` branches on format (table for `Format::Human`, the unchanged shared `render` otherwise), so the JSON envelope, the canonical `command` field, and every other command's rendering are untouched. Human output is not a compatibility surface, but contract tests pin the table so it cannot regress accidentally. Other list-like commands (`history`) keep the pretty-JSON fallback until a reviewed decision extends table rendering.

Implementation deviations from the plan above, all confined to `locron`:

- The summaries parse the redacted `definition_json` as JSON values rather than deserializing into typed `JobDefinition`: a redacted inline body is the string `"<redacted>"`, which serde rejects for the typed `Vec<u8>` body field ("expected a sequence"). Value-level parsing still reads only the redacted record, so the redaction guarantee is unchanged.
- The table's renderers are named `list_schedule_summary`/`list_target_summary` because the export-selection work in the same file already owned the typed `schedule_summary(&Schedule)` name for its picker rows; both share `human_duration`.
- `every DUR` renders the largest whole unit (`s`, `m`, `h`, or `d`) that divides the stored microseconds, matching the CLI's input grammar; a sub-second value (which the grammar can never produce) falls back to the raw `{N}us` rendering rather than truncating.
- Rows never carry trailing whitespace: the last column is not padded.

`locron daemon run` loads configuration, constructs `locron-store` behind core ports, constructs `locron-engine`, and enters its daemon runtime. Signal loops, locks, reconciliation, runners, maintenance, and graceful shutdown remain inside the engine.

## Human rendering implementation (2026-08-24)

Implements the frozen human-output-contract amendment (issue #4). Each command's `Format::Human` branch calls a dedicated renderer instead of the shared pretty-JSON fallback; the `Format::Json` path and the `render` envelope are untouched, so machine output is byte-identical. Renderers consume only the redacted records the JSON path already uses, so redaction parity holds by construction.

Shared helpers live beside the existing list renderer: column alignment and human durations reuse `render_list_table` and `human_duration`; schedule and target summaries reuse `list_schedule_summary` and `list_target_summary`; run-state and trigger names render from the existing state vocabulary. No new dependency is added — every form is hand-rolled `println!` composition. `doctor` keeps its existing check evaluation and only changes presentation. `why` reuses its explanation facts and flattens them into labeled sections. `logs`, `run --wait` streams, `export`, `service`, `self-update`, `version`, and `mcp` are already conformant and are not changed.

Contract tests pin every command's human form for empty and populated states, dry-run wording, table-only ID abbreviation, and redaction, mirroring the existing help-surface walk so a new command cannot omit its human form silently. The README demo screencast (`assets/screencast.sh`) dropped its `jq` pipes as part of this change; the recording itself is regenerated separately.

Implementation deviations, all confined to `locron` and confined to human branches:

- The plan text above claims `run --wait` streams are "already conformant and are not changed". In fact the human wait stream is part of this work: after the queued line it now prints the terminal outcome line `run finished: {id} ({state})`, per the `docs/CLI.md` contract. The streamed progress lines themselves are unchanged.
- `why --run` also prints an EVENTS section (`  {RFC3339} {kind}` per durable event) beyond the contract's RUN/ATTEMPTS/terminal-reason sections. The events are already loaded to produce the terminal-reason text, so this costs no extra record access and the contract does not forbid it.
- The `why --run` RUN section omits a job-name line: the job is soft-deleted in the general case, so a name would require a best-effort extra lookup with an ambiguous fallback. The run id, trigger, and timestamps identify the run without it.
- The `history` table has no run-id column — the contract's "run ID may abbreviate in table only" permission is unused. When the job has been soft-deleted, the JOB column falls back to the abbreviated job id (first 8 hex characters) instead of the missing name.
- Dry-run wording choices: `{key}: would be configured (dry run; no changes made)` for `config set`, `dry run: would create N, update N, unchanged N; no changes made` for `import`, `dry run: would prune {runs} runs, {outputs} outputs ({bytes} bytes)` for `prune`, and `{decision}: {name}` plus `dry run: no run created` for `run --dry-run`.
- `prune` reports "N runs" as the count of distinct run ids: prune rows are per-attempt, and a run with several attempts is counted once.
- `import` no-op action lines print the import plan's pre-existing job name and id for the no-op entry (a no-op plan entry does not carry a resolved record id).

## Edge cases to handle explicitly

- Two daemon commands start against one state directory.
- A CLI mutation races engine reconciliation of an older revision.
- Manual enqueue succeeds with no daemon and becomes visible after daemon start.
- A job is disabled, re-enabled, edited, renamed, or soft-deleted while work is queued/running.
- A deleted name is reused without confusing historical identity.
- Cron aliases and day-of-month/day-of-week OR semantics.
- System-local timezone changes while disabled or while the daemon runs.
- DST spring gap, fall repetition, backward wall-clock movement, and a large forward jump.
- Long downtime summary calculation does not iterate an unbounded number of instants.
- `all` keeps the newest bounded window and executes it oldest-first.
- A normal occurrence arrives while catch-up is active under every overlap policy.
- Global capacity is exhausted or reduced below the current running count.
- Several replace occurrences arrive during graceful termination; failed termination never creates concurrency.
- A retry becomes due while another occurrence arrives, or the daemon restarts during `retry_wait`.
- The daemon dies before spawn, after spawn/before running commit, while running, or after target exit/before result commit.
- PID/PGID reuse is never used during recovery; a child may create grandchildren or escape its process group.
- CWD, executable, env file, or HTTP body file disappears after registration.
- PATH resolution changes after a package upgrade and remains auditable per attempt.
- Environment parsing rejects invalid names, NUL, and reserved values; every output mode redacts configured sensitive values.
- HTTP redirect crosses origin, response exceeds capture bounds, or timeout occurs during streaming.
- Output finalization or pruning is interrupted between filesystem and database operations.
- Age, count, and byte retention bounds are exceeded together while active runs remain protected.
- SQLite is busy, disk is full, migration fails, or state comes from a newer incompatible schema.
- An unsupported Windows, 32-bit, or musl/Alpine build is not accidentally advertised as an official v1 artifact.

## Change plan

The plan is restricted to this repository. Before an implementation deviation, update `docs/IMPLEMENTATION.md` and the relevant Project draft tickets; update `docs/ARCHITECTURE.md` first when the durable structure or invariant changes.

1. Keep the reviewed decisions in this document, `docs/CLI.md`, and `docs/STORAGE.md` synchronized before implementation deviations.
2. Create the edition-2024, resolver-3 virtual workspace with the four accepted crates, `rust-version = "1.94"`, one `locron` binary, workspace lint/profile/dependency policy, and CI on Rust 1.94 plus latest stable.
3. Implement `locron-core` domain values, normalization, state transitions, pure schedule enumeration, policy validation, and fake clock/store/executor ports.
4. Implement versioned `locron-store` migrations and transactions for jobs/revisions, cursors, runs/attempts/retries/events, lifetimes, settings, output metadata, uniqueness, soft deletion, and bounded retention.
5. Implement the `locron-engine` daemon runtime: ownership, startup recovery, reconciliation, overlap/concurrency admission, retry, cancellation, maintenance, signals, and graceful shutdown.
6. Implement process, explicit-shell, and HTTP runners in `locron-engine`, including environment/path resolution, process groups, timeout/cancellation, and bounded output capture.
7. Implement thin `locron` commands and composition, including `locron daemon run`, human/versioned machine output, offline enqueue, wait/follow, import/export, prune, and doctor. Do not add another daemon crate or binary.
8. Add deterministic unit, integration, fault-injection, retention/disk-pressure, and platform tests for macOS 14+ and Linux kernel 5.14+/glibc 2.34+ on `aarch64` and `x86_64`.
9. Complete user/operator documentation and map every `docs/SPEC.md` completion criterion to executable evidence without introducing deferred viewer, MCP, desktop, packaging, or service-installation work.

## Verification strategy

- **Architecture and workspace:** check edition 2024, resolver 3, `rust-version = "1.94"`, exactly four members, one `locron` binary, the documented dependency graph, and absence of a daemon crate/binary. Run formatting, all-target compilation, strict lints, tests, and docs/link checks on Rust 1.94 and latest stable.
- **Domain unit tests:** use table/property tests and injected time for cron/interval/at enumeration, schedule revisions, DST/timezone changes, duration overflow, policy validation, retries, and every legal/illegal state transition.
- **Store integration tests:** use temporary real SQLite databases for clean and upgrade migrations, constraints, transaction races, occurrence idempotency, cursor/run atomicity, lifetime recovery, busy handling, soft deletion, output consistency, and retention order.
- **Engine integration tests:** use fake time and fake executors for wake/downtime, disabled intervals, bounded catch-up, global/per-job admission, ordering, replacement, retries, cancellation, maintenance, signals, and restart without nondeterministic sleeps.
- **Runner tests:** use real process trees and local HTTP fixtures for argv/environment/CWD/PATH, output streaming, redirect/TLS policy, result classification, timeout, TERM/KILL escalation, and grandchildren.
- **Fault injection:** terminate the daemon at every transaction/spawn/completion boundary and assert durable identity, `interrupted_unknown`, no unknown-outcome retry, and no duplicate scheduled or one-time occurrence.
- **Retention/resource tests:** exceed per-run/global output, metadata age/count, catch-up, concurrency, and database-pressure bounds; assert bounded work, deterministic truncation/eviction, and active-run protection.
- **CLI contract tests:** assert human and machine results, IDs, redaction, error categories, offline enqueue, wait disconnect, import/export round trips, invalid option rejection, doctor output, and thin delegation of `locron daemon run` to the engine.
- **Platform verification:** run process-group, signal, filesystem permission, timezone, service-lifetime, crash, and global concurrency 16/64 tests on macOS 14+ and Linux kernel 5.14+/glibc 2.34+ across `aarch64` and `x86_64`. Windows, 32-bit, and musl/Alpine results are informational only.
- **Acceptance audit:** map all 16 `docs/SPEC.md` completion criteria to an automated test or a documented official-platform check. Milestone 1 is incomplete while any criterion lacks evidence.

## Installer and self-update implementation (post-milestone delivery, 2026-08-23)

This section plans the installation-channels amendment to `docs/SPEC.md`: the Homebrew-independent one-line installer and the built-in self-update subcommand. Evidence and rejected alternatives are recorded in `docs/FINDINGS.md` §11.

### Accepted: evergreen install script

Ship one POSIX `sh` script, `install.sh` at the repository root, as the source of truth. The release pipeline attaches it to every GitHub Release, so the canonical one-liner is version-consistent with the artifacts it installs:

```
curl -fsSL https://github.com/WhiteKiwi/locron/releases/latest/download/install.sh | sh
```

The script resolves "latest" exclusively through `releases/latest/download/{asset}` redirects and never calls the GitHub REST API, avoiding the 60/hour unauthenticated limit documented in `docs/FINDINGS.md` §11. A pinned install uses `LOCRON_VERSION=vX.Y.Z`, switching the base to `releases/download/vX.Y.Z/`. Supported targets are exactly the four published release targets: `aarch64-apple-darwin`, `x86_64-apple-darwin`, `aarch64-unknown-linux-gnu`, `x86_64-unknown-linux-gnu`. Detection is `uname -s` (Darwin/Linux) and `uname -m` (arm64/aarch64, x86_64); musl detection via `ldd` must refuse with an actionable error because only glibc builds are published. Any other OS or architecture fails with an actionable unsupported-platform error before any download.

The script downloads `SHA256SUMS.txt` from the same release, selects the line for the chosen target, validates the entry as 64 hex characters, and verifies with `shasum -c` (or `sha256sum`) before extracting. Trust rests on HTTPS to the same origin as the artifacts — the same trust model as mise's pinned-version path. Extraction happens in `mktemp -d` with a cleanup trap; the binary is copied to a temp file inside the install directory, made executable, and atomically renamed over the target, correcting the `rm`+`mv` window present in mise.run's own script.

The default install path is `$HOME/.local/bin/locron`, overridden by `LOCRON_INSTALL_DIR` (a full file path; a directory value is an error, as in mise). No root is required and no shell configuration is modified: the script prints per-shell guidance for adding the directory to `PATH` when it is absent, detected from `$SHELL` like mise's `after_finish_help`. Re-running the same command downloads, verifies, and atomically replaces the binary — this is the update path for script-installed users, and no skip-if-exists option is added because re-running is cheap and deterministic. Missing `curl`/`wget`, download failures, checksum mismatches, extraction failures, and unwritable install directories each produce a specific actionable error and a non-zero exit.

A short custom domain serves the one-liner as `https://locron.whitekiwi.link/install.sh` (added 2026-08-24, completing the TODO follow-up). It is not a hosted script copy: a CloudFront viewer-request function (distribution `E2SNYXU6Z3ZE4N`, function `locron-redirect`, OAC `E2BUNP08WL3O60` in front of a private dummy S3 origin) 302-redirects `/install.sh` to the canonical release asset above and other paths to the repository. The served script is therefore always the version-consistent release asset with no release-pipeline change and no drift, the same trust level as the GitHub one-liner; the GitHub URL remains canonical in the documentation.

### Accepted: self-update subcommand

Add `locron self-update` to the CLI. It updates only to the latest stable release; pinning remains an installer function per the frozen specification.

Version resolution uses `GET https://api.github.com/repos/WhiteKiwi/locron/releases/latest`. This is an explicit user-triggered action, so the unauthenticated rate limit is acceptable; a rate-limit or network failure maps to the CLI's stable error categories with retry guidance, and no file is touched. The subcommand then downloads the matching tarball and the release's `SHA256SUMS.txt`, verifies the tarball hash (adding `sha2`, `tar`, and `flate2` as pure-Rust dependencies of `locron`, plus `reqwest` with rustls/stream/json for the API and asset downloads — reqwest is already a workspace crate, and the `self_update`/`self-replace` crates were rejected as unnecessary surface for one temp-file-plus-rename), and extracts in a temporary directory. The extracted binary is copied to a temp file in the same directory as the running executable and replaced with a single `fs::rename`, which is atomic on both platforms: the running process keeps its old inode, and the next invocation executes the new binary.

Package-manager refusal follows the mise pattern with a marker we control: the tap formula creates `lib/.disable-self-update` under the brew prefix at install time, and `self-update` refuses with a stable error directing the user to `brew upgrade locron` when the marker exists next to the canonicalized current executable. Script-installed and source-installed binaries have no marker and remain updatable. All verification and download failures occur before the rename, so a failed or interrupted update leaves the existing binary installed and working, as the specification requires. Human output reports the current and new version or "already up to date"; machine output uses the standard `locron.cli/v1` envelope with `command: "self-update"`.

Testability uses rustup's override seam: `LOCRON_UPDATE_API_BASE` and `LOCRON_UPDATE_ASSET_BASE` environment variables default to the production hosts and let contract tests point at a local HTTP fixture serving a fake `releases/latest` document, tarballs, and checksums. No automatic update check or auto-update behavior is added in this amendment.

### Accepted: tap formula marker and release pipeline

The formula template embedded in `.github/workflows/release.yml` gains one line that creates the self-update marker inside the prefix (`touch lib/.disable-self-update`), and the pipeline attaches `install.sh` to GitHub Releases so the canonical one-liner exists for every published version. `docs/CLI.md` documents the `self-update` command under the reviewed CLI contract, and the README installation section adds the one-liner plus the per-channel update story (re-run the script, `brew upgrade`, or `self-update`). *(The per-channel story later moved to `docs/INSTALL.md`, which the README installation section now links.)*

Implementation deviation, corrected 2026-08-24: the deployed template wrote the marker line as `FileUtils.touch` and carried an explicit `version` line, and the tap's `brew test-bot` failed on five consecutive bumps with the corresponding `brew style` and `brew audit` offenses (evidence in `docs/FINDINGS.md` §20). The template now writes `touch lib/".disable-self-update"` and renders the literal version into the four URL strings (`.../download/v${VERSION}/...`) with no `version` line — Homebrew scans the version from the literal URL token, the canonical pattern for GitHub-release binary formulas. Keeping `#{version}` placeholders without an explicit `version` line fails formula loading (`version (nil)`), so the template interpolates the version at generation time instead. Behavior is unchanged; the fix takes effect at the next tag because the workflow evaluates at the tagged commit.

### Edge cases to handle explicitly

- A machine has both a brew-installed and a script-installed locron; `PATH` order decides which runs, and each channel updates through itself.
- The daemon is running during self-update: replacement is atomic and the running daemon keeps the old code until its next restart; the operator documentation states this explicitly.
- The script or self-update runs on musl Linux, Windows, or an unknown architecture: refuse with the published-platform error, never guess.
- `LOCRON_INSTALL_DIR` is a directory or an unwritable path; `/tmp` is mounted noexec (mirror rustup's actionable message).
- A checksum file line for the target is missing or malformed; the tarball is truncated or corrupted mid-download.
- The GitHub API is rate-limited or unreachable during self-update; the tap formula is installed with an old marker layout.
- A pinned `LOCRON_VERSION` refers to a release whose checksum asset or tarball does not exist.

### Verification additions

- **Installer static checks:** `sh -n` and shellcheck pass in CI; the script contains no bashisms; a fixture-server test runs it end-to-end on macOS and Linux CI legs against a fake release layout (latest redirect, pinned version, checksum mismatch, unsupported arch, unwritable dir) with `LOCRON_VERSION` and the asset-base override, then executes the installed binary's `-V`.
- **Installer release check:** a pinned run against the real `v0.1.1` release into a temporary directory installs a working binary on both macOS architectures and Linux.
- **Self-update contract tests:** local HTTP fixture drives latest resolution, checksum verification, atomic replacement (the pre-update process keeps running while new invocations run the new binary), marker-file refusal with brew guidance, "already up to date", rate-limit error mapping, and JSON envelope output; failure injection proves the old binary is untouched after download/verify errors.
- **Formula marker:** the tap formula template contains the marker line, and a manual `brew reinstall` followed by `self-update` refusal is recorded as evidence at the next release.
- **Platform matrix:** the existing four-target CI runs the new suites; Windows, 32-bit, and musl results remain informational.

### Accepted: literal Homebrew formula rendering (2026-08-24 release follow-up)

The v0.6.0 release exposed a shell-expansion defect in the inline, unquoted formula heredoc: Ruby
documentation backticks were executed as shell command substitutions before the formula was written.
The release workflow therefore no longer owns an executable heredoc. A checked-in formula template
is plain data with explicit version and checksum tokens, and a small POSIX renderer validates the
release version and all four lowercase SHA-256 values before replacing only those tokens. The
workflow redirects the renderer's standard output into the cloned tap. Literal Ruby comments and
caveats never pass through shell evaluation, while release-derived values still render into the
four literal URLs and checksum fields Homebrew requires for version scanning.

The renderer fails if a value has the wrong shape or a template token remains. A deterministic
regression script renders fixed fixture values and asserts the complete package-manager guidance,
service-upgrade caveat, literal backticks, URLs, checksums, marker, and absence of trailing
whitespace. Push CI runs this check and shellchecks both scripts. This keeps the release-only path
executable before the next tag rather than relying on another publication to discover template
corruption.

The already-published v0.6.0 formula is repaired directly in `WhiteKiwi/homebrew-tap`: retain the
current v0.6.0 literal URLs and checksums, restore the guidance byte-for-byte from the style-clean
v0.5.0 formula, then require the tap's `brew test-bot --only-tap-syntax` workflow to succeed. No
locron release asset or product binary changes, and no v0.6.1 re-release is needed.

## Daemon service installation implementation (post-milestone delivery, 2026-08-23)

This section plans the daemon-service amendment to `docs/SPEC.md`: per-user registration and automatic startup of the daemon by the script installer, a Homebrew service definition for `brew services`, and refresh-and-restart behavior on updates. Evidence and rejected alternatives are recorded in `docs/FINDINGS.md` §12.

### Accepted: binary-owned service registration

`locron` owns a new `locron service install|uninstall|status` family behind a small service-manager port. The port has two real backends — launchd (macOS) and systemd user units (Linux) — and a deterministic fake for tests. `locron-engine` and the store are unchanged: the daemon already performs graceful SIGTERM shutdown, single-owner locking, and stale-attempt classification, which is everything a service manager requires of it. install.sh and self-update call the subcommand rather than shelling out to `launchctl`/`systemctl` themselves, keeping the POSIX script thin and the behavior unit-testable. No new dependencies: the backends run `launchctl`/`systemctl` as child processes.

Templates are embedded constants, not files shipped in archives. The macOS plist carries label `dev.locron.daemon`, `ProgramArguments` `[<current_exe>, "daemon", "run"]`, `KeepAlive` true, `RunAtLoad` true, and `StandardOutPath`/`StandardErrorPath` both at `~/Library/Logs/locron/daemon.log` (created at install; the Homebrew default-log-path convention). The Linux unit is `locron.service` at `~/.config/systemd/user/` with `ExecStart=<current_exe> daemon run`, `Restart=on-failure`, and `WantedBy=default.target`. Registration always uses the canonicalized absolute path of the running binary, so repeating it repairs a registration whose binary moved or was replaced.

### Accepted: macOS registration flow

Install writes the plist user-owned 0644, runs `launchctl enable gui/<uid>/<label>`, and consults `launchctl print` for the label. If the job is already loaded, install refreshes the plist and sends `SIGTERM` with `launchctl kill`; `KeepAlive` then restarts the job on the new binary, and the engine's ordinary graceful-shutdown sequence handles active work. If not loaded, install first checks the state-directory daemon lock with the store's existing lock probe and then bootstraps, so a manual daemon holding the lock is never shadowed by a restart loop. `bootstrap` into the `gui` domain can fail outside a GUI login session (for example over SSH); the backend falls back to the `user/<uid>` domain with an explanatory note, the path Homebrew itself uses. Because the termination semantics of `bootout` are undocumented (open question recorded in `docs/FINDINGS.md` §12), uninstall signals `SIGTERM` first, waits for the signaled process to exit, and runs `bootout` plus plist removal as cleanup; a live macOS test validates this ordering rather than relying on an assumed contract. Two launchd realities shape the uninstall wait: a KeepAlive job never leaves the domain until `bootout`, so the wait watches the process (its pid disappears, or a respawned pid replaces it) instead of the domain; and `launchctl kill`/`bootout` fail with exit 3 ("No process to signal"/already unloaded) whenever the job is between KeepAlive respawns, which is treated as the state the caller wants. Status reports the loaded domain, PID, and binary path from `launchctl print`.

### Accepted: Linux registration flow

The systemd backend first proves a usable user manager: `XDG_RUNTIME_DIR` set and the user bus reachable, probed with `systemctl --user show-environment`. Without one (SSH, containers, cron), `service install` prints the explicit guidance required by the specification and exits zero — installation remains successful. With a manager, install writes the unit, runs `systemctl --user daemon-reload`, then `enable --now`. When the unit is already active the refresh runs `stop` followed by `enable --now` — a bare `enable --now` would never restart a loaded daemon, because `systemctl start` on an already-active unit is a no-op; the `stop` signals SIGTERM and the subsequent start launches the new binary, the same graceful sequence as macOS. Uninstall runs `stop` and `disable`, removes the unit, and reloads. Status reports `is-active` and `is-enabled`. The unit stops with the login session by design; the operator guide documents `loginctl enable-linger` as the optional step for boot persistence (self-lingering requires no administrator authentication per `docs/FINDINGS.md` §12).

### Accepted: installer, self-update, and package integration

install.sh, after its atomic binary replace, runs `<installed> service install` unless `LOCRON_NO_SERVICE=1`, passing its output through. A zero exit with guidance output (no Linux session) leaves the install successful, exactly as the specification requires. Any other non-zero exit from the registration attempt also leaves the installation successful: the script warns and continues, because the binary replacement is the essential install and the registration attempt is best-effort by design (`LOCRON_NO_SERVICE` exists to decline it). The same tolerance is recorded in this backlog's step evidence.

self-update runs `service install` on the replaced executable after its own successful atomic replace (the child inherits the environment and its output is captured so the update envelope stays clean): if the daemon was service-managed, this refreshes and restarts it onto the new binary; if no registration existed, it performs a first registration; if the daemon was started manually, the registration is written and the lock check defers the start until the manual daemon stops. Registration is best-effort: a failed post-replace registration becomes a warning in the update envelope (and on stderr in human mode), never an update failure. The brew-managed refusal reuses the existing `lib/.disable-self-update` marker: `service install` and `service uninstall` refuse on a marker-bearing binary with a stable error directing to `brew services`.

The update flow must not re-resolve its own executable path after the replace: on Linux `/proc/self/exe` of a process that renamed its binary over itself resolves to the deleted old inode (`path (deleted)`), so the post-replace `fs::canonicalize(current_exe())` fails and the registration was silently skipped. The flow therefore captures the canonical executable path once before the atomic replace and threads it through `replace_binary` and `register_service` (macOS never showed the bug because `_NSGetExecutablePath` returns the exec-time path string without re-checking the filesystem).

The release.yml formula template gains a `service` block (`run [opt_bin/"locron", "daemon", "run"]`, `keep_alive true`, `run_at_load false`) and a caveats line pointing at `brew services start locron`; installation never starts the service. `brew upgrade` does not restart running services (`docs/FINDINGS.md` §12), so after an upgrade that caveat remains the documented restart path. The deb/rpm postinst prints the same guidance as the no-session Linux path; it never registers anything.

### Edge cases to handle explicitly

- `service install` while a manual `locron daemon run` holds the state lock: write and enable the registration, then report that it will start the daemon after the manual process stops, without bootstrapping.
- A service-loaded daemon exits because the lock is held elsewhere: launchd/systemd keep retrying at their throttle interval — safe (the engine's single-owner check never executes work twice) but visible in status output.
- macOS over SSH: `gui` bootstrap failure falls back to `user/<uid>` with a note.
- The binary is removed or moved after registration: restarts fail until `service install` re-registers; status surfaces the stale path.
- Linux self-update: the running process's `/proc/self/exe` points at the deleted inode after the replace; any post-replace path resolution must reuse the pre-replace capture.
- Two concurrent registrations: idempotent writes and enable calls; the last one wins.
- An update restart lands while jobs run: the engine's graceful-shutdown sequence applies unchanged, and interrupted attempts follow the existing recovery contract.
- Linux logout while jobs run: the session manager signals the daemon and the same graceful sequence runs.
- `service status` on an unsupported platform or without a state directory: a stable diagnostic, with no registration attempted.

### Verification additions

- **Fake-port contract tests:** template rendering (canonicalized path, label/unit name, KeepAlive/RunAtLoad, Restart=on-failure/WantedBy, log paths), enable/bootstrap/kill ordering, lock-held deferral, brew-marker refusal, no-session guidance, and machine-output envelopes, all without touching a real service manager. The envelope's `service_name` assertion uses the platform-native name (`dev.locron.daemon` on macOS, `locron.service` on Linux), never a hard-coded label.
- **Real-backend tests:** on the macOS CI leg, register/restart/unregister against the domain available on CI (`gui` when a GUI session exists, `user/<uid>` otherwise), asserting the plist, loaded state, and graceful SIGTERM restart with a marker process; on the Linux leg, run a real user manager under `dbus-run-session` to cover daemon-reload, enable --now, stop, and disable.
- **install.sh fixtures:** a default run attempts registration and tolerates the guidance exit; `LOCRON_NO_SERVICE=1` skips it entirely.
- **Release artifact checks:** the formula template contains the `service` block and the release attaches the updated script; a built .deb contains the postinst guidance.
- **Live evidence items:** `brew services start locron` starts the daemon, and `brew upgrade` leaves the old daemon running until `brew services restart` — recorded at the next release, like the self-update marker check.
- **Platform matrix:** the existing four-target CI runs the new suites; Windows, 32-bit, and musl results remain informational.

## Usage and installation measurement (maintainer tooling, 2026-08-23)

This section plans maintainer-facing measurement of locron's public distribution channels. It changes no product behavior, so the frozen `docs/SPEC.md` is not amended. Evidence and rejected alternatives are recorded in `docs/FINDINGS.md` §13.

### Accepted: dependency-free snapshot script

`scripts/usage.sh`, POSIX `sh`, depends only on `curl` and the optional `gh`, and prints one snapshot with these sections:

1. **GitHub Releases** — per-release asset download totals and a grand total from `GET /repos/WhiteKiwi/locron/releases`, paginating with `per_page=100` and following `Link` header pages with a sane page cap. Counts are cumulative and reset on asset re-upload, so the output labels them accordingly.
2. **Stars** — `stargazers_count` from the repository endpoint.
3. **Homebrew** — `whitekiwi/tap/locron` install counts for 30, 90, and 365 days from formulae.brew.sh; a missing entry renders as 0; output notes the anonymous/opt-out undercount.
4. **crates.io** — queries `/api/v1/crates/locron` with the descriptive User-Agent required by the data-access policy; prints `N/A (not published)` while unpublished and switches to the downloads endpoint automatically when the crate exists. The `/downloads` endpoint is a trailing-90-day series, so the published value is labeled as such; all-time totals live on the crate endpoint (recorded in `docs/FINDINGS.md` §13).
5. **GitHub traffic** — views and clones (14-day totals and uniques) via `gh api`; printed only when `gh` is present and authenticated, otherwise a one-line note explains how to enable it.
6. **Rate-limit awareness** — when the unauthenticated REST quota is exhausted, the GitHub sections print the limit message with retry guidance (`GITHUB_TOKEN` or `gh auth login`) instead of raw API errors.

`--json` emits the same snapshot as one flat JSON object (traffic keys present only when authenticated) for future automation. A per-section failure marks that section and lets the remaining sections print; the exit code reflects whether any section failed. The script parses JSON with only portable `sh` tooling (`grep`/`sed`/`awk`) — `jq` must not be a runtime requirement. Every heredoc is quoted; the repository incident memory requires it.

### Edge cases to handle explicitly

- Unauthenticated rate limit exhausted: GitHub sections degrade with actionable guidance, and the brew/crates.io sections still print.
- Tap formula with zero recorded installs: no analytics entry — render 0, never an error.
- Release list longer than one page: follow the `Link` header with a sane page cap.
- `gh` installed but unauthenticated or without owner access: traffic section omitted with a note.
- Network failure mid-run: later sections still print; non-zero exit.

### Verification additions

- **Static checks:** `sh -n` and shellcheck clean; the script contains no bashisms; all heredocs quoted.
- **CI smoke:** the existing `installer` job in `.github/workflows/ci.yml` gains a step that runs the script in `--json` mode against the live APIs (the authenticated `GITHUB_TOKEN` keeps the REST quota off the 60/hour limit) and asserts the JSON parses and each numeric field is a non-negative integer. The owner-only `/traffic/*` endpoints reject the Actions `GITHUB_TOKEN`, so the step additionally permits an exit confined to `traffic_error` while still failing on any other `*_error` key.
- **Local live check:** a real run prints all sections with numbers matching independently computed `jq` totals for the same day; the brew section renders 0 and crates.io renders `N/A` until their first real values.

## Export selection and URL import implementation (2026-08-24)

This section plans the 2026-08-24 `docs/SPEC.md` amendment: export job selection (interactive default on a TTY, deterministic filters, non-interactive full export) and import from a URL. Evidence and rejected alternatives are recorded in `docs/FINDINGS.md` §15. The change is confined to `locron`; `locron-core` and `locron-store` are unchanged, and the frozen dashboard spec's whole-document export download/import upload is unaffected (selection or URL support there would be its own dashboard spec change).

### Accepted: selection as a filter over the existing export path

Selection never reaches the store or domain crates. `locron` resolves the export subset from the same `list_jobs(true)` result the existing `export` function already reads, then hands the filtered list to the existing `export_job` mapping; the document shape, redaction, and omission accounting are untouched.

`--jobs NAME[,NAME...]` and `--tag TAG[,TAG...]` take exact names/tags, combine as a union, and deduplicate by job ID. Any selector value matching no job is a validation error before any output is produced (exit category 2), so a typo can never silently produce a smaller backup. Filters are valid with both human and JSON output and always suppress the picker. A zero-job state skips the picker entirely because there is nothing to select (export of settings only remains legal, as today).

### Accepted: interactive default on a TTY, deterministic everywhere else

Interactivity is decided once per invocation, before any output: stdin, stdout, and stderr must all be terminals (`std::io::IsTerminal`), the `CI` environment variable must be absent, output format must be human, and no `--jobs`/`--tag` selector may be present. Only that combination shows the picker; every other context exports the complete job set exactly as the current CLI does, so scripts, pipes, redirections, CI, and JSON consumers see no behavior change. This is the gh/OpenSpec/diagramkit convention from `docs/FINDINGS.md` §15, and the gh-gist piped-prompt bug is the counterexample this design rules out: non-TTY can never prompt.

stderr joins the decision because the picker renders there: dialoguer's stderr terminal refuses to render on a redirected stderr (`NotConnected`), so a TTY stdin/stdout with a redirected stderr must fall back to the deterministic full export rather than fail the command. This mirrors the both-terminals rule's intent: if the selection interface cannot render, the invocation is non-interactive.

The picker is a dialoguer 0.12 `MultiSelect` (MIT, rust-version 1.66, `default-features = false` — `editor`/`password` are unneeded) with the term target set to stderr, listing jobs by name with each item's schedule summary, every item initially selected, and Enter confirming. Rendering on stderr keeps the "human stdout is the bare export document" contract (`docs/CLI.md`) intact even while a picker is visible; the picker never writes to stdout. The picker interaction is wrapped behind a small selection port so contract tests drive a deterministic fake without a PTY.

For contract tests, a scripted picker substitutes for the TUI without a PTY: when `LOCRON_TEST_EXPORT_PICKER` (test-only hook, documented in the test file) is set to a comma-separated job name list, export treats the invocation as having three terminals, and the selection port returns exactly those job names (rendering its prompt line on stderr) instead of running dialoguer. The `CI`, format, and selector terms of the decision still apply, so the hook cannot make a scripted or JSON export interactive; it only replaces the TUI inside a context that already qualifies.

### Accepted: URL import reuses the whole-document import path

`locron import` accepts an absolute `http://` or `https://` URL in addition to a path. URL detection is an explicit scheme check (an `scheme://`-shaped input is parsed as a URL; anything else is a path — never a `Path::exists` guess). The CLI fetches the body with the existing reqwest/rustls client configuration (mandatory TLS verification), with a 16 MiB in-memory cap enforced while streaming, a 10-redirect cap, and a 30-second total timeout; URLs with a userinfo component are rejected at parse time as a validation error (exit category 2, like any bad argument — the category-5 set below covers what happens after the CLI commits to fetching). The fetched bytes then enter the existing `parse_import_document` → validate → plan → one-transaction apply path byte-for-byte unchanged, so redaction rejection, plaintext acknowledgement, deterministic resolution, dry-run, and rollback are identical for both sources. Fetch failures (DNS, TLS, timeout, cap, redirect excess, non-2xx, non-HTTP scheme) map to exit category 5 with an actionable message and retry guidance; document validation failures keep their existing categories. Import never prompts — `--dry-run` is the preview, and the post-import summary already reports create/update/no-op actions.

The trust boundary is documented, not coded around: `docs/CLI.md` and `docs/OPERATOR.md` state that an export document registers executable schedules and importing from a URL carries the same trust boundary as installing a script from that URL, with `--dry-run` recommended for first-time imports. No signature, pinning, or checksum scheme is added in this amendment; the existing redaction rules remain the value-protection mechanism.

### Edge cases to handle explicitly

- stdout is a TTY but stdin is not (input redirected): no picker — full export, matching the three-terminals rule.
- stdin and stdout are TTYs but stderr is redirected (`locron export 2>file`): no picker — the interface cannot render on a non-terminal stderr, and dialoguer would fail the command; the invocation falls back to the deterministic full export.
- `CI` is set while running in a real terminal (wrappers, `script -qec`): no picker — the environment marker wins, per the OpenSpec chain.
- Picker shown while the daemon edits jobs concurrently: selection resolves against the same `list_jobs` snapshot used for the document; a concurrently deleted job simply exports its last-read definition, and a concurrently created job appears in the next export.
- `--jobs` with a duplicate name and `--tag` with overlapping matches: union by job ID, one document entry per job.
- Zero registered jobs: no picker; settings-only export remains valid.
- URL import of a document with omitted values without `--accept-plaintext-values`: rejected exactly like a file import (existing rule).
- Fetch succeeds but the body is not valid UTF-8 JSON or exceeds 16 MiB: stable validation/protocol error before any write.
- URL import while the destination store is busy or migration-locked: existing exit category 4 behavior unchanged.
- Human mode with a URL: the bare document/plan renders on stdout exactly as for a file; fetch diagnostics and warnings go to stderr.

### Verification additions

- **Selection contract tests:** `--jobs`/`--tag` union and dedup, no-match validation error before output, JSON mode with and without selectors, redaction parity between a selected export and a full export of the same jobs, and round trips (`export --jobs` → `import`) reproducing exactly the selected jobs.
- **Interactivity tests:** the selection port is driven by a deterministic fake; the interactivity decision is a pure function tested across the TTY/CI/format/selector matrix; contract tests (via the `LOCRON_TEST_EXPORT_PICKER` hook, which drives the picker branch without a PTY) assert stdout carries only the document while the picker prompt renders on stderr, that an empty selection yields a settings-only export, that `CI` still wins over the hook, and that JSON mode never instantiates the picker.
- **URL import fixture tests:** a local HTTP fixture serves valid documents, redacted documents, malformed JSON, oversized bodies, redirect chains, and 404/500 responses; assertions cover successful atomic import, dry-run non-mutation, category-5 fetch failures, rollback on a late destination conflict, and identical behavior versus the same document as a file.
- **Platform matrix:** the existing four-target CI runs the new suites; no platform-specific code is introduced (TTY detection and stderr rendering are portable via dialoguer and `std::io::IsTerminal`).

## Shutdown-drain test determinism and CI lint consolidation (2026-08-24)

No product-behavior change — a test-harness script and the CI workflow only — so the frozen `docs/SPEC.md` is not amended. Failure evidence (run IDs, local reproduction) is recorded in `docs/TODO.md` "Shutdown-drain test determinism and CI lint consolidation backlog".

### Accepted: single-member process group makes drain-cancel confirmation event-driven

`daemon::tests::elapsed_shutdown_drain_cancels_runner_before_lifetime_end` failed on two macOS CI legs — run 32644652482 (`macos-aarch64` / Rust 1.94.0) and run 32644735243 (`macos-x86_64` / Rust stable), different platforms each time — with the outcome assertion receiving `TerminationUnconfirmed` instead of `Cancelled`.

Root cause: the test script's `while :; do sleep 1; done` puts a second process (`sleep`) in the run's process group. The runner confirms termination only when the owned leader wait handle has resolved and `kill(-pgid, 0)` reports the whole group absent (runner `wait` branch, `runner.rs:315`; group probe `observe_group_absence`, `runner.rs:767`). When the trapped `sh` exits, its orphaned `sleep` sibling is reaped by launchd/init on its own schedule; until then it remains a zombie that the group-liveness probe counts as alive, so the test's two 20 ms grace deadlines (TERM → KILL → confirm, `runner.rs:351`) can elapse before group absence becomes observable on a loaded macOS runner. The production path is unaffected in practice — its default graces are 5 s + 5 s, which the reap latency cannot plausibly exceed — so the flake is an artifact of the test shrinking the grace to 20 ms.

The fix makes confirmation event-driven rather than budget-driven. Replace the script with a pure-builtin loop (`while :; do :; done`) so the group has exactly one member: when the runner reaps the leader, group absence is true in the same poll, `termination_confirmed` is set without any deadline firing, and the outcome is `Cancelled` regardless of machine load, parallel test contention, or scheduler latency. The test also raises `termination_grace` from 20 ms to 1 s as belt-and-braces, so even a pathological TERM-to-trap delay cannot reach the KILL stage before the trap writes its marker file; the grace then gates only the escalation step, not the asserted outcome. `shutdown_drain` stays at 10 ms: it only decides when the daemon issues cancellation, and the attempt tracker cannot complete before that cancellation, so the drain always elapses. The trap still fires promptly in a builtin loop (the shell checks for pending traps between commands), and the loop burns one CPU core only from `ready` to the trap — tens of milliseconds, confined to one unit test.

Rejected alternatives:

- `serial_test` or `--test-threads=1`: reduces the CPU contention that amplifies the race, but keeps the fixed two-deadline budget against launchd reap latency, so the test would still flake on a loaded runner; the workspace has no other timing-sensitive test needing global serialization.
- Lengthening the graces alone: same residual race, just rarer — a fixed budget stays load-dependent.
- Redefining production confirmation as leader-reap-only (dropping the group-absence requirement): changes behavior the frozen documents record (SPEC: "Completion is not reported until termination is confirmed"; IMPLEMENTATION: "Leader exit alone is insufficient because an in-group descendant may still be running") to fix a test artifact. Not accepted.
- `cargo nextest`: its parallel runner and retry support are marginal here — the engine test binary finishes in under a second — and a retry wrapper would mask rather than remove the race.

### Accepted: dedicated lint job over OS × toolchain, tests over the full platform matrix

`cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings` run in all eight matrix legs of `.github/workflows/ci.yml`. Clippy findings are platform-sensitive only through OS-gated code (this repository's history contains two backlogs where macOS-only or Linux-only dead code failed the opposite OS legs), and the workspace contains no architecture-gated code, so the architecture duplication among the eight clippy compile passes carries no signal.

The workflow gains a `lint` job — matrix `linux-x86_64` and `macos-aarch64` × Rust 1.94.0 and stable, `fail-fast: false` — running fmt and clippy, and the `test` job's eight legs keep only `cargo test --workspace --all-targets`. This preserves exactly the coverage that has caught real bugs here (OS-gated dead code on both OSes, both toolchains' lint sets — formatting and lint rules have drifted between Rust versions before) while halving clippy compile work. rust-cache stays in the lint job so its clippy artifacts warm across runs.

### Edge cases to handle explicitly

- A toolchain update changes rustfmt or clippy output between Rust 1.94 and stable: both toolchains remain in the lint matrix because the CI contract requires clean results on both.
- macOS-only or Linux-only items that are dead code: still caught, because both operating systems remain in the lint matrix.
- The busy-loop script's CPU burn: bounded by the test's own cancellation path and confined to one unit test.
- The `termination_grace` change must not alter what the test proves: it still asserts `Cancelled` via the drain-elapsed → daemon-cancel → TERM-trap path, now with the outcome independent of the deadline values.

### Verification additions

- The changed test passes 100 consecutive local runs plus the macOS CI legs, and the sibling `shutdown_drain_allows_natural_completion_before_lifetime_end` test remains untouched and green.
- The `lint` job passes on both OS legs and both toolchains; the eight `test` job legs pass with `cargo test` only.

## Usage snapshot smoke relocation (2026-08-24)

No product-behavior change — CI placement only — so the frozen `docs/SPEC.md` is not amended. Evidence and rejected alternatives are recorded in `docs/FINDINGS.md` §18.

### Accepted: scheduled smoke workflow, hermetic push CI

CI run 32654895285 failed in `Installer / ubuntu-latest` at the "Usage snapshot smoke (live APIs)" step: `scripts/usage.sh --json` exited non-zero with a non-`traffic_error` key. The step never passed `GITHUB_TOKEN` through `env:`, so the script's GitHub REST calls ran unauthenticated against the shared-IP 60/hour quota even though the script supports the token (`usage.sh` line 80). The step's tolerance predicate (`has("traffic_error") and [all *_error keys == "traffic_error"]`) then rejected the failure, as designed.

New `.github/workflows/usage.yml`: `on: schedule: [cron: '0 3 * * 1']` (weekly, Monday 03:00 UTC) plus `workflow_dispatch`; one `usage` job on ubuntu-latest: checkout, run `sh scripts/usage.sh --json` with `env: GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}` and `permissions: contents: read`, then assert the JSON parses and every numeric field is a non-negative integer. The traffic-only tolerance from the removed step is carried over, for a reason the first plan draft missed: passing `GITHUB_TOKEN` also authenticates the preinstalled `gh`, and the owner-only `/traffic/*` endpoints then fail by design (the Actions token lacks push/admin access), so `traffic_error` is the *expected* state of an authenticated scheduled run, not a drift signal — the first `workflow_dispatch` (run 32655962445) failed on exactly this. Any other `*_error` key or an invalid snapshot still fails the run as a maintainer drift alert. No artifacts are published; the run log is the record.

`ci.yml`'s installer job loses the live-smoke step; the hermetic steps stay: shellcheck on `install.sh` and `scripts/usage.sh`, the fake-`uname`/`ldd` refusal tests, and the pinned `v0.2.0` install smoke (release-asset download through the CDN-backed redirect — no REST quota consumption — consistently green across the matrix runs, e.g. 32644269125 and 32654818613).

### Edge cases to handle explicitly

- Scheduled workflows run only on the default branch and auto-disable after 60 days of repository inactivity — acceptable for a maintainer measurement tool; `workflow_dispatch` covers manual runs.
- `gh` is not authenticated in the scheduled run: the traffic section prints its note and the script still exits 0 when every other section succeeds (existing script behavior, unchanged).
- formulae.brew.sh and crates.io have their own limits and are not authenticated: their sections can still fail a scheduled run — that is the drift alert working as intended, never a push gate.

### Verification additions

- `ci.yml` parses and the installer job's step list no longer includes the live smoke; `rg -n "usage.sh --json" .github/workflows` shows the smoke only inside `usage.yml`.
- The scheduled workflow cannot fire from a push (GitHub schedules run on the default branch with real cron timing); a manual `workflow_dispatch` run is recorded as evidence at first trigger.
- The next push CI run is green; run ID recorded in `docs/TODO.md`.

## Process-group cancellation confirmation on macOS (2026-08-25)

No product-scope change: cancellation still reports `Cancelled` only after the owned leader has
been reaped and its process group has received termination. The frozen `docs/SPEC.md` is unchanged.

The macOS x86_64 stable CI failure in [job 97478078694](https://github.com/WhiteKiwi/locron/actions/runs/32741897719/job/97478078694)
was a deterministic gap in the runner's confirmation rule, exposed intermittently by process
reaping latency. The cancellation test creates a TERM-ignoring grandchild. After SIGKILL succeeds,
that child can remain a zombie in the original process group until launchd reaps it. POSIX
`kill(-pgid, 0)` reports that zombie as present, even though SIGKILL has made further execution
impossible. The runner therefore incorrectly emitted `TerminationUnconfirmed` after its second
grace deadline.

The runner will retain group-absence probing before escalation: a TERM outcome remains confirmed
only after both the direct child is reaped and the process group is absent. Once SIGKILL was either
delivered successfully or found the group already absent (`ESRCH`), and the direct child is reaped,
the runner will classify the cancellation or timeout as confirmed without waiting for an
unreapable-by-locron zombie to disappear. Any SIGKILL error other than `ESRCH`, or a missing direct
child reap, remains `TerminationUnconfirmed`.

This is stronger than increasing a timeout: it removes a host-controlled zombie-reaping race while
retaining the safety boundary for a live descendant that did not receive SIGKILL.

### Verification additions

- Unit-test the SIGKILL delivery predicate for success, `ESRCH`, and a permission error.
- Run the live grandchild-cancellation regression repeatedly on macOS where available, then run
  the complete workspace test and lint battery.
- Confirm the next macOS x86_64 stable CI run reports the cancellation test as passed without a
  job retry.

## Terminal-width list table truncation (2026-08-24)

This section plans the 2026-08-24 `docs/SPEC.md` amendment (Human Output Contract: Table width). Evidence and rejected alternatives are recorded in `docs/FINDINGS.md` §19. The change is confined to `locron`; `locron-core`, `locron-store`, and `locron-engine` are unchanged.

### Accepted: TTY-only truncation of the table's final column

Width resolution is `console::Term::stdout().size_checked()` — the `TIOCGWINSZ` ioctl that docker and kubectl use. `console` 0.16 is already in the `locron` dependency graph through dialoguer 0.12 (verified with `cargo tree -p locron -i console`: console 0.16.4 → dialoguer 0.12.0 → locron), so declaring it as a direct dependency with `default-features = false` adds zero lockfile entries. A failed size lookup — stdout redirected, piped, or otherwise not a terminal — means no truncation, so the one mechanism is both the width source and the TTY gate. The width is sampled once per invocation; a mid-print window resize is not chased (docker and kubectl behave the same). The tuple is `(rows, cols)` — verified in the console 0.16.4 source (`unix_term.rs:53–67` returns `(winsize.ws_row, winsize.ws_col)`), so the width is the second element; a PTY check at real widths confirms the truncation budget tracks the column count.

Display width uses `unicode-width` 0.2, already locked transitively, declared directly on `locron`. The pure helper `truncate_display(&str, max_width) -> String` walks characters, sums display width, and appends the `…` marker (display width 1) only when the value actually shrinks; a value that fits is returned unchanged.

`render_list_table` gains a `width: Option<u16>` parameter, resolved once in the `list` dispatch arm for human format only. Column padding is unchanged; fitting is a separate step: the natural table width is `name_width + 1 + schedule_width + 1 + target_width + 1 + 7` (the final `ENABLED` column is unpadded), and when it exceeds the terminal width only `TARGET` — the table's final data column — absorbs the deficit. Earlier columns never truncate: `NAME` is the key for every other command, schedule summaries are inherently short, and truncating a middle column would misalign every column after it. When the deficit leaves less than one display column for `TARGET` (a pathological terminal width), no truncation occurs and the table wraps exactly as it does today — documented, not silently cut data.

`--no-trunc` is a clap boolean on `List`. It is a rendering flag, not a data flag: it restores full `TARGET` values on a terminal and is accepted with no effect in machine mode, whose envelope stays byte-identical either way. `show` is unchanged — it already prints the complete definition. The `history` table is unchanged in this amendment; applying the same rule there is a deferred follow-up when a long `TRIGGER` value demonstrates the need.

Testability needs no PTY: `truncate_display` and `render_list_table` are pure functions whose width is an injected parameter, so unit tests call them directly with widths of 40, 80, and `None`. Contract tests keep asserting piped `list` output — assert_cmd pipes stdout, the size lookup fails, and the output must be byte-identical to today's full-value table; the help-surface walk covers the new flag.

### Edge cases to handle explicitly

- A terminal narrower than `NAME + SCHEDULE + ENABLED` alone: no truncation, rows wrap as today.
- CJK or emoji in a target: fitting uses display width, never byte or character count; a truncation may split a grapheme cluster (acceptable in a summary table — the full value lives in `show`).
- `--no-trunc` with piped stdout: a no-op, because pipes already print full values.
- `--no-trunc` with `--format json`: accepted and ignored; the envelope is unchanged.
- An empty job list: header only, unchanged.
- Window resized after invocation start: the sampled width stands for the invocation.

### Verification additions

- **Unit tests:** `truncate_display` — ASCII fit/no-fit and exact-boundary cases, width-2 CJK, emoji, ellipsis appended only when truncation occurs, and zero/minimum widths; `render_list_table` with injected widths covering the truncating, fitting, and too-narrow fallback paths.
- **Contract tests:** piped human `ls` with a long target is byte-identical to the pre-change table; `--no-trunc` appears in `locron ls --help` and is accepted; `ls --no-trunc --format json` output is identical to `ls --format json`.
- **Workspace battery:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` pass on the installed toolchain; the four-target CI matrix stays green.

## Terminal-width history table truncation (2026-08-25)

This section implements the 2026-08-25 `docs/SPEC.md` amendment by extending the accepted list-table
mechanism to the human `history` table. The change remains confined to `locron`; storage, ordering,
redaction, and machine-readable history data are unchanged.

`History` gains a `--no-trunc` rendering flag. In the human dispatch path, stdout width is sampled
once with the existing `console::Term::stdout().size_checked()` lookup unless the flag is present.
The same failed-lookup behavior remains the TTY gate, so redirected and piped output receives no
width and therefore retains every value. Machine mode accepts and ignores the flag, preserving the
same envelope and data.

Refactor the history renderer into a printing wrapper and a pure table builder with an injected
optional width, matching the list renderer's test seam. The natural table width includes the five
maximum display-width columns and the four literal ` | ` separators. If it exceeds the sampled
terminal width, only `TRIGGER` receives the remaining budget after reserving the natural widths of
`TIME`, `JOB`, `STATE`, and `DURATION` plus separators. A budget of at least one display column uses
the existing Unicode display-width truncation helper and its trailing `…`; a smaller budget falls
back to the full table so no preserved column or separator is destroyed. Column padding is based on
display width so a wide trigger remains aligned and fits the same budget. Empty history remains the
unchanged header-only table.

Verification covers pure history-table rendering at fitting, truncating, absent-width, too-narrow,
and Unicode widths; CLI contracts for help, piped full output, and machine-output equivalence with
`--no-trunc`; formatting; warnings-denied all-target workspace Clippy; all workspace targets; and
`git diff --check`.

## Dashboard v0.8.0 release preparation (2026-08-25)

This section implements the corresponding `docs/SPEC.md` amendment without changing dashboard
runtime behavior. The feature implementation and its integrated browser QA are already complete;
this pass makes the shipped surface discoverable, gives it one release identity, and prepares the
reviewed branch for the repository publication workflow.

### Documentation and release-note structure

The README will introduce the dashboard immediately after installation and before the broader CLI
quick start. The shortest supported flow is foreground-first: start `locron dashboard`, open the
printed loopback URL, and paste the printed token into the entry page. A compact persistent-service
alternative will use `locron dashboard enable`, with `status` for the stable local URL and `token`
for intentional token re-display. The copy must say that the dashboard is optional, disabled by
default, loopback-only, and backed by the same durable jobs, runs, settings, and diagnostics as the
CLI. It must not place a token in a URL, imply remote access, or imply that the dashboard replaces or
owns the scheduler daemon.

The existing Operator and CLI references remain the detailed source of truth. Their dashboard
sections will be checked for stale paths or statements; only user-facing inconsistencies found in
that review will change. The README documentation index will point to the operational guide as well
as the design/specification material so a new user can move from quick start to lifecycle and
security detail.

`CHANGELOG.md` will be normalized into chronological Keep a Changelog order, because its current
header and Unreleased section sit below several released entries after an earlier merge. A curated
`0.8.0` entry will summarize the complete user-visible dashboard: lifecycle commands, loopback and
token boundary, authenticated management and diagnostics, responsive light/dark interface,
human-friendly schedule/size controls, debounced partial search, row navigation, and redacted
pretty JSON. Test-only and internal implementation details stay out of the entry. The comparison
links will advance from `v0.7.0` to `v0.8.0`, while Unreleased remains empty and ready for the next
change.

### Version synchronization

The workspace package version changes from `0.7.0` to `0.8.0`, a backward-compatible feature
release. Member manifests inherit the workspace version, so `Cargo.toml` is the only hand-edited
manifest. `cargo check --workspace` will perform Cargo's normal lockfile reconciliation; the five
workspace package records in `Cargo.lock` must all report `0.8.0`, with dependency versions
otherwise unchanged.

### Publication boundary and maintained skill

This repository preparation does not open or merge a pull request, create a tag, publish a GitHub
release, update Homebrew, or modify the separate `WhiteKiwi/skills` repository. Those external
mutations belong to the parent publication session after review. The parent will compare the
released dashboard command surface against the Locron skill before publication; if it changes the
skill, that repository's generated packages and validation must be synchronized independently.

### Verification additions

- Validate every relative README Markdown link and every shell fence used by the quick start; scan
  the new dashboard copy for token-in-URL examples and contradictory remote/off-by-default claims.
- Confirm `CHANGELOG.md` has one heading, strictly descending released versions, a `0.8.0` entry,
  and comparison links based on `v0.8.0` and `v0.7.0`.
- Run `cargo check --workspace --locked` after normal lockfile generation, and assert the workspace
  manifest plus all five Locron lockfile package records report `0.8.0`.
- Run `cargo fmt --all --check`, `git diff --check`, and inspect staged and unstaged changes so the
  release-preparation commit contains only the specification amendment, planning documents,
  user-facing documentation, changelog, and version metadata.

## Linux service cfg portability follow-up (2026-08-25)

This is a compilation-portability correction for the already-specified daemon and dashboard
service behavior. No root or dashboard specification changes: systemd receives the same target
service names, and the real-backend tests keep the same platform ownership.

The systemd `ServicePort` methods that select a daemon or dashboard unit must rename their existing
`_ctx` binding to `ctx`. There is no data-flow or command change; the correction merely makes the
identifier used in each existing `systemctl` argument available in the Linux compilation branch.
The adjacent context-independent methods retain `_ctx`, making the unused/used distinction
compiler-enforced.

The macOS-only dashboard cleanup type, its `Drop` implementation, and the default dashboard token
path helper will receive `#[cfg(target_os = "macos")]`. Scoping both the type and implementation is
necessary: scoping only the construction leaves the definition dead on Linux, while scoping only
the type leaves an invalid implementation. The common daemon cleanup remains available on Linux.

Verification proceeds from the narrowest platform seam outward:

1. Compile the systemd module on Linux and in unit-test builds, and add a no-command unit test that
   boxes `SystemdPort` as a `dyn ServicePort` trait object. A source contract additionally confirms
   only truly unused arguments retain `_ctx` and that the dashboard cleanup type, implementation,
   and token helper are under macOS cfg.
2. `cargo fmt --all --check`, warnings-denied workspace all-target Clippy, and the complete
   workspace all-target test suite run on the available macOS toolchain to detect cross-platform
   fallout.
3. Attempt the installed Linux Rust target check, record any missing cross-C-toolchain boundary, and
   require the parent-owned native CI rerun as the final Linux compile proof.

The follow-up commit contains only this implementation correction and its FINDINGS,
IMPLEMENTATION, and TODO evidence. The parent session owns push and PR workflow actions.

## Service template identity follow-up (2026-08-25)

No service behavior changes. This follow-up separates two concepts that were equivalent only on a
macOS host: the active platform manager's service identity and the launchd identity embedded in a
plist template.

Add `Target::launchd_label()` under `cfg(any(target_os = "macos", test))`, mapping daemon and
dashboard directly to `DAEMON_LABEL` and `DASHBOARD_LABEL`. The macOS branch of
`Target::service_name()` delegates to this accessor, retaining one launchd label source of truth.
Linux `service_name()` continues returning `DAEMON_UNIT`/`DASHBOARD_UNIT`, so all systemd manager
commands, unit paths, CLI output, and API output are unchanged.

`render_plist` uses `launchd_label()` instead of `service_name()`. That makes the template output a
property of the requested format, not of the host executing its test. Existing daemon/dashboard
plist assertions remain strict and become portable; the launchd constants become genuinely used in
Linux test builds. `render_unit` stays unchanged because it embeds no service-manager name.

Verification requires the two focused plist tests and the systemd compile seam on the local host,
a source contract proving `render_plist` uses `launchd_label` while unit-path/manager calls retain
`service_name`, warnings-denied all-target Clippy, full workspace all-target tests, fmt, and diff
checks. Native Linux remains parent-owned CI confirmation after publication of the scoped fix.

## Dashboard fixed-port test serialization follow-up (2026-08-25)

Runtime binding behavior remains unchanged: partial IPv4/IPv6 success is valid, foreground falls
back only when no configured family can bind the candidate port, and fixed mode errors under the
same all-family conflict condition.

Add a test-only `serialized_default_port()` helper in the dashboard CLI integration suite. A
process-static `Mutex<()>` returns a poison-tolerant guard so a failed test does not prevent later
cleanup or diagnostics. Acquire that guard before `hold_fixed(DEFAULT_PORT)` in exactly the three
tests that exercise default-port fixed/fallback behavior, and retain it through child cleanup by
ordinary lexical lifetime. Random explicit-port tests do not share the resource and stay parallel.

The existing `hold_fixed` dual-family helper and all assertions remain unchanged. In a clean CI
environment, each serialized test owns both loopback listeners. If an external process owns the
default port, the existing helper can still observe that stable conflict; the mutex specifically
eliminates unowned conflicts created by another test in this process.

Verification includes a source inventory proving every `hold_fixed(DEFAULT_PORT)` call acquires the
guard, repeated high-parallelism runs of the complete dashboard integration binary, focused fixed,
redirected, and PTY fallback tests, and the full fmt/warnings-denied Clippy/workspace all-target
battery. The review server stays running; native matrix confirmation remains parent-owned.
## crates.io source installation and trusted publication (2026-08-25)

The accepted distribution design follows the evidence in `docs/FINDINGS.md` §34. crates.io is a
secondary source-build channel for Rust users; the standalone installer and Homebrew remain the
general-user paths. The user-facing Cargo package is named `locron`, retains the single `locron`
binary target, and is published together with the four library packages required by its normal
dependency graph.

### Package graph and metadata

Rename the `locron` Cargo package to `locron` without moving its source directory or creating a
second binary. Centralize each internal dependency in `[workspace.dependencies]` as a local `path`
plus an exact registry version equal to the lockstep workspace release. Members inherit those
declarations so local builds use the workspace source while packaged manifests resolve the exact
crates.io release. Cargo's native workspace publisher then owns the dependency order:
`locron-core`; `locron-store` and `locron-engine`; `locron-server`; then `locron`.

All five packages inherit the repository, root README, authors, dual license, Rust version,
crates.io-only publication restriction, keywords, and command-line category. Package descriptions
remain role-specific. The internal libraries are published implementation packages required by the
binary, not a promise of an independently stable public API. Release version updates must change
the workspace version and every exact internal version in one reviewed commit.

### Installation ownership

Binary ownership and service ownership stay separate. `install.sh` writes an atomic owner-only
receipt named `.locron-install-receipt-v1` beside the installed executable after a verified atomic
replacement. Its exact two-line payload is `locron.install/v1` then `standalone`; the deterministic
sibling location binds it to the canonical executable directory, and moving the binary without the
receipt intentionally drops self-update authority. `locron self-update` accepts only a regular,
non-symlink receipt with that exact payload before downloading or replacing anything. An absent or
malformed receipt refuses with stable machine output and channel guidance. Cargo users receive
`cargo install --locked locron`; receipt-less older script users are told to rerun the standalone
installer once; other installations are told to use their installation channel. The existing
Homebrew marker remains the stronger Homebrew-specific message.

Cargo does not own launchd or systemd registration, so its receipt-less binary may still run
`locron service install` and `locron dashboard enable`. The Homebrew marker continues to block
those mutations because Homebrew does own their service lifecycle. Manual tarball/source and
deb/rpm installs likewise keep the existing service-registration behavior even though built-in
self-update refuses them.

### CI and release flow

Push/PR CI runs `cargo publish --workspace --dry-run --locked` on Rust 1.94 after the ordinary
workspace gate and inspects the package set, normalized manifests, bundled README/licenses, and
absence of repository-only or secret material. No check uses `--allow-dirty` or `--no-verify`.

The tag workflow adds a dedicated `publish-crates` job after the complete binary build matrix and
before GitHub Release/Homebrew publication. Only that job receives `contents: read` and
`id-token: write`, is bound to the protected `crates-io` environment, exchanges GitHub OIDC through
the official `rust-lang/crates-io-auth-action@v1`, and passes the short-lived token as
`CARGO_REGISTRY_TOKEN` to `cargo publish --workspace --locked`. The GitHub Release job alone keeps
`contents: write`.

The job first proves tag, workspace, lockfile, binary, and changelog version agreement, then queries
all five exact package versions with a descriptive user agent. None present permits publication;
all present is an idempotent rerun/bootstrap case and skips upload; a partial set fails with an
inventory and explicit recovery guidance. Workspace publication is not atomic, so downstream
publication never runs after a partial failure. After all versions become visible, install the
exact `locron` version into a temporary Cargo root with `--locked`, verify its version and
self-update refusal, and run non-mutating service/dashboard status checks.

crates.io requires one manual first publication per new package before trusted publishers can be
configured. The release guide therefore defines a one-time bootstrap from the exact clean release
commit using a newly created narrow API token, immediate token revocation, trusted-publisher
bindings for all five packages to `WhiteKiwi/locron` + `release.yml` + `crates-io`, and then the
ordinary immutable tag. The tag job observes all versions already present and performs the same
registry-install verification without trying to overwrite them. Later tags use OIDC only.

### Documentation and TODO compaction (historical; superseded 2026-10-02)

The following compaction policy records the former repository-checklist workflow. Execution tasks
now live in Project-only drafts; see [`PROJECTS.md`](PROJECTS.md). Preserve the archived source and
evidence, but do not resume a live TODO checklist or move new completed Project tasks into it.

README and installation/release documentation list the prebuilt installer and Homebrew before
`cargo install --locked locron`, explain the Rust 1.94 source-build requirement, distinguish Cargo
update/removal from Locron service registration, and document the one-time trusted-publisher
bootstrap and partial-publication recovery.

Keep `docs/TODO.md` as the live checklist: move fully completed top-level sections verbatim to
`docs/TODO-archive.md`; for mixed sections, archive the completed evidence and retain only the open
follow-up with enough context and its verification method. Verify the apparently stale unchecked
terminal-width planning item against the existing SPEC/CLI/IMPLEMENTATION/FINDINGS records before
marking and archiving it. Do not move any genuinely open checkbox, and preserve the current
`docs/BACKLOG.md` distinction between inactive ideas and committed work.

### Verification strategy

Before handoff, run formatting, warnings-denied workspace Clippy, all workspace targets, dependency
direction, shell syntax and shellcheck for changed scripts, workflow YAML/action lint, workspace
package and publish dry-runs on Rust 1.94, per-package file/archive inspection, exact-version Cargo
installation into a temporary root where possible, self-update ownership tests, CLI help/contract
tests, Markdown link/reference checks, and `git diff --check`. No real crates.io upload is part of
implementation verification.

### Source-package archive inspection follow-up (2026-08-25)

The first hosted source-package run proved that piping `tar -tf` directly into `grep -q` is unsafe
under the runner's `bash -o pipefail`: once `grep` finds the expected member and exits, `tar` can
receive a broken pipe and make the successful inspection fail. Archive listings are already
materialized into one file per package for the other checks, so the server asset assertion must
read that saved listing as well. This keeps the check exact, avoids suppressing genuine `tar`
failures, and makes local and hosted behavior deterministic. Verification reruns the failed hosted
job on the corrective commit and requires the source-package job plus the complete CI workflow to
pass.

## Dashboard lifecycle human output and stale detail recovery (2026-08-25)

The CLI output audit follows command dispatch rather than only searching the shared renderer. The
ordinary scheduler commands already select a command-specific human renderer before calling the
machine envelope renderer. The remaining accidental pretty-JSON paths are the three daemon service
commands, the four non-foreground dashboard lifecycle commands, and successful `self-update`.
Human `export` remains intentionally serialized because its stdout is the portable export document,
not a command result report.

Remove the shared renderer's pretty-JSON human fallback after the audit. Reaching it in human mode
is an internal contract violation, which prevents a later command from silently reintroducing this
class of omission; intentional serialized surfaces such as export continue to render explicitly.

Keep the existing JSON data shapes and error envelopes unchanged. In human mode, service and
dashboard lifecycle commands instead print a stable labeled report. Installation reports service
identity and registration/restart/defer facts; removal reports service identity and stop/removal
facts; status reports registration, running/enabled/session state and optional manager facts.
Dashboard status extends that report with the access URL and token presence/permission posture,
while dashboard disable reports token removal. `dashboard token` prints the access URL and an
`Access token:` label followed by the unmodified token on its own line so it remains immediately
copyable. Existing actionable guidance remains on stderr and no command other than token or the
first foreground startup line reveals the token value. Successful human self-update reports the
old and new versions and whether replacement occurred.

The frontend keeps successful-login deep links intact. Job and run detail loaders distinguish a
404 response from loading and from other request failures. A missing detail route renders its own
route header and card that name the resource category, explain that the durable resource may have
been removed or the link may be stale, and provide a direct collection link (`#/jobs` or `#/runs`).
The raw missing identifier is not used as the page-level explanation. Valid detail responses keep
the existing detail UI, and non-404 failures keep their ordinary request feedback so operational
errors are not mislabeled as absent data.

Implementation and verification order:

1. Add focused CLI contract assertions for all lifecycle human forms and successful self-update
   while retaining their existing JSON assertions.
2. Add job and run detail tests for 404, non-404, and valid response branches, then implement the
   explicit missing-resource states without changing hash routing.
3. Run the complete service/dashboard/self-update integration suites and frontend test/build
   checks, followed by workspace formatting, warnings-denied Clippy, relevant workspace tests, and
   diff checks.

The plan review confirms that no state, API, routing, authentication, token storage, or machine
schema change is required. The changes are limited to presentation selection and explicit 404
recovery, so the durable architecture remains unchanged.

## Deterministic dashboard port-policy verification (2026-08-25)

The dashboard specification and runtime binding behavior remain unchanged. Replace CLI integration
contracts that manufacture a conflict on the global default port 10824 with two deterministic
seams. In `locron-server`, bind an OS-assigned port on one loopback family, configure the server to
that same family, and retain the listener while exercising both `Foreground` and `Fixed` policies
against that owned preferred port. Foreground must select a different bound port; fixed must return
`AddrInUse`. Existing coverage continues to verify the independent partial-family behavior. The
helper must never accept an empty listener set as proof of occupancy.

At the CLI service boundary, unit-test the pure port-policy selector: absent explicit port plus
ordinary foreground selects fallback, while an explicit port or hidden registered-service mode is
fixed. Retain explicit OS-assigned-port integration coverage for human/JSON startup, serving, and
strict occupied-port error mapping. Remove the redirected and PTY tests whose only additional
mechanism is a race-prone conflict on 10824, along with the global-port occupancy and mutex helpers.

Verification proceeds in three layers: focused server and CLI policy tests; repeated parallel runs
of the dashboard integration binary and relevant server tests; then formatting, warnings-denied
workspace Clippy, the complete workspace all-target suite, and diff checks. The plan deliberately
does not add sleeps, retries, weakened assertions, test-only production environment variables, or
workflow-level serialization.

## CI toolchain and cache optimization (2026-08-26)

This is a CI correctness and resource-use correction. It changes no Locron product behavior,
platform support, MSRV, release workflow, or durable contract, so `docs/SPEC.md` remains unchanged.
Evidence and rejected alternatives are recorded in `docs/FINDINGS.md` §36.

The test matrix uses an explicit include list: stable Rust on Linux x86_64, Linux arm64, macOS
x86_64, and macOS arm64, plus Rust 1.94.0 on Linux x86_64 as the MSRV gate. The lint matrix uses the
exact Rust 1.98.0 development toolchain on Linux x86_64 and macOS arm64, preserving both OS-gated
Clippy branches without letting a moving stable release silently change the blocking warning set.
Installer and Rust-1.94 source-package job commands remain unchanged.

Every matrix job exports its selected toolchain through `RUSTUP_TOOLCHAIN` in addition to installing
it. The checked-in `rust-toolchain.toml` selects exact Rust 1.98.0 with rustfmt and Clippy for normal
development, while `Cargo.toml` retains Rust 1.94 as the MSRV contract. The test and lint recording
steps compare rustup's active compiler path with the compiler selected explicitly by the matrix and
fail on a mismatch before any command runs. Rust cache keys fingerprint `RUST*` environment
variables, so stable, pinned lint, and MSRV artifacts remain separate.

`Swatinem/rust-cache` continues restoring dependency and target artifacts because restore cost is
well below native compilation time. Cache saves are restricted to `refs/heads/main`; pull requests
may restore default-branch caches but do not create branch-local job variants. Existing caches are
left to GitHub's normal eviction until the optimized workflow is proven; cleanup is a separate,
reproducible maintenance operation after hosted verification. Release-tag jobs also restore through
the action but never save: an immutable tag is not a reusable cache producer, and three recent tags
already account for twelve one-release entries and about 2.64 GB.

Verification requires workflow YAML parsing and actionlint, local formatting and warnings-denied
Clippy under the pinned Rust 1.98.0 development toolchain, workspace tests under Rust 1.94, a clean
diff, and a hosted push run with exactly nine successful jobs. Hosted logs must show floating stable
on compatibility tests, exact 1.98.0 on lint, and 1.94.0 on the MSRV leg. Neither pull-request nor
release-tag cache misses may create new entries.

## Active dashboard run detail live following (2026-08-28)

The existing run detail API remains the initial and terminal durable snapshot, while the existing
run stream remains the incremental transport. The primary detail response renders and decides
automatic following as soon as it completes; the auxiliary explanation request is independently
guarded and may enrich the page later, so a slow or unavailable explanation never holds the whole
detail surface in its loading state. No server route, event schema, authentication, durable state,
cancellation, or retention behavior changes.

The detail component classifies the initial run state with the same terminal-state vocabulary as
the backend. A successful non-terminal snapshot automatically enables following; a terminal
snapshot stays static. The prior snapshot remains rendered while the stream is connecting or
reconnecting, and connection feedback distinguishes connecting, connected, paused, lost/retrying,
and terminal reconciliation from initial loading or request failure.

One component lifetime owns a route generation, a latest-explanation generation, an output
replay-key set, and whether terminal reconciliation has already begun. Changing the run identity
or unmounting invalidates outstanding detail, explanation, log, and terminal-reconciliation
requests and closes the active stream. A newer refresh also supersedes an older auxiliary
explanation response. Every async completion and event handler checks the applicable generation
before updating React state.

Named stream events are applied as follows:

- `run` replaces the visible run state;
- `attempt` upserts the matching attempt number and state while preserving already loaded durable
  fields, ordered by attempt number;
- `output` decodes `data_b64` as bytes, renders UTF-8 with replacement for malformed sequences,
  and appends only when `(attempt_number, seq)` has not been seen in this run-detail lifetime;
- `termination` is accepted once, closes following, updates the terminal state immediately, then
  performs one full guarded refresh of run, attempts, explanation, audit events, and retained
  output.

Pause closes the current `EventSource` without mutating the run or clearing rendered facts. Resume
opens a new source and uses the lifetime replay-key set to discard replayed output. Manual output
loading replaces the visible output with the selected attempt's durable frames and seeds replay
keys for those frame sequences, preventing a later stream replay from duplicating them.

Implementation order:

1. Extend focused run-detail tests with a controllable `EventSource` and deferred API requests.
2. Add guarded snapshot loading, automatic active following, live state application, base64 output
   decoding, lifetime deduplication, pause/resume, and one terminal reconciliation.
3. Rebuild the committed dashboard distribution and verify its embedded-source contracts.
4. Run focused frontend tests, frontend typecheck and production build, then proportionate Rust
   server/asset verification and repository diff checks.

Plan review confirms that the server stream already carries every required event and durable replay
behavior. The minimal correction is therefore frontend-only plus its committed production bundle;
changing polling, SSE framing, or scheduler state would increase risk without addressing the
observed client omissions.

## v0.9.3 patch release (2026-08-28)

Prepare v0.9.3 as a lockstep workspace patch release from the reviewed active run-detail
correction. Change the workspace package version and all four exact internal dependency
requirements from 0.9.2 to 0.9.3, then refresh the lockfile through Cargo so all five package
records agree. No dependency, feature, platform, API, schema, or workflow behavior changes as part
of this release preparation.

Curate the changelog directly from the user-visible correction: an active run detail renders as
soon as its primary snapshot arrives, follows state, attempt, and output changes automatically,
and performs one complete reconciliation on termination. Omit the intervening toolchain, cache,
and workflow-only commits because the changelog policy intentionally excludes CI-only work. Add
the v0.9.3 comparison link and advance the Unreleased comparison base without rewriting prior
release sections.

Verification runs against the exact local release candidate without publishing it: require the
release-version contract for 0.9.3, formatting, warnings-denied workspace all-target Clippy, the
complete workspace all-target test suite, frontend focused/full tests plus typecheck and production
build, workspace package and publish dry-runs with the lockfile enforced, and final status/diff
inspection including `git diff --check`. The parent session owns the release commit, immutable tag,
push, hosted release workflow, registry/GitHub/Homebrew publication, and post-publication checks.
