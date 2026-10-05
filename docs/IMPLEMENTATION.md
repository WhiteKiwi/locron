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
   Record exact commands/revisions/results on the relevant repository issue before
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
scope and acceptance issues remain with their existing workstream.

Verify every pre-existing job against this exact main revision and retain the
reviewed frontend job from published PR38 head `c07a854`. Recheck frozen frontend
installation, typecheck, the complete test suite and production build, then run
Rust 1.98 formatting/Clippy plus Rust 1.94 and stable full workspace tests on the
integrated macOS source. Reuse the unchanged dependency graph and asset proof;
record a separate integration handoff before parent publication and exact-head CI.

After this reviewed plan and issue handoff, a separate development sub-session
owns implementation and documentation updates for any new decision. The parent
reviews and publishes. No tags, signing, registry upload, installer execution,
live jobs/services, Windows acceptance or broad storage deletion belongs here.

## Status and authority

This document plans the first program milestone against the frozen behavior in `docs/SPEC.md` and the durable structure in `docs/ARCHITECTURE.md`.

Accepted foundations are Rust edition 2024, Cargo resolver 3, Rust 1.94 MSRV, the official platform matrix, the four-crate dependency direction, one user-facing `locron` binary, and an engine-owned daemon entered through `locron daemon run`. The Windows internal GUI launcher is the documented exception to the earlier one-distributable-binary rule. Those decisions are not Draft.

> **Review state:** milestone-1 implementation choices are accepted. Update this document and the relevant repository issues before deviating in code. A change to observable behavior or scope updates `docs/SPEC.md` first; a change to durable component boundaries or invariants updates `docs/ARCHITECTURE.md` first. Reviewed CLI and storage contracts live in `docs/CLI.md` and `docs/STORAGE.md`.

`docs/FINDINGS.md` preserves the research path and does not override the frozen specification. In particular, v1 has no `queue-one` overlap policy and global concurrency defaults to 16, not 4.

## Native Windows 11 implementation (2026-10-02)

The Windows amendment in SPEC and adapter boundaries in ARCHITECTURE are the authority for this
milestone. FINDINGS §46 records the selected safe interfaces, source audit and limitations. The
initial release is unsigned; signing remains deferred in public proposal #37 and is not part of
this milestone's dependency graph. Execution progress/evidence belongs in repository Issues.
The 2026-10-03 migration reuses #24–#36 for the Windows tasks while retaining the original public
#23–#36 proposal and review text as history.

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

#### Unavailable state discovery diagnostics (#25, 2026-10-03)

The remaining unavailable-default criterion in #25 requires a recovery path in the actual CLI
error. Missing or unusable platform discovery currently reports only that discovery failed, and
a failed Windows KnownFolder adapter loses the state-discovery context. This correction retains
the frozen SPEC, override precedence, platform defaults and managed-path admission policy.

1. Add both supported state-directory overrides to unavailable-default errors. Wrap a failed
   Windows KnownFolder adapter in a typed discovery error that retains its original I/O source;
   ordinary filesystem failures remain ordinary I/O errors. Keep the existing `state_error`
   category, exit status 5 and human/JSON/stream envelope behavior. **Verify:** missing or unusable
   defaults and failed adapters identify state discovery and offer `--state-dir <PATH>` and
   `LOCRON_STATE_DIR`; the adapter's original cause remains available through the error chain.
2. Isolate only the existing KnownFolder result interpretation behind a private helper used by
   the production Windows discovery call. **Verify:** probe failure, non-string, empty and
   relative results fail explicitly; a native Windows absolute LocalAppData result still selects
   its `locron` child, including spaces and Unicode, without creating state or changing privacy
   policy. Tests supply probe results directly and do not change KnownFolder or security policy.
3. Exercise the real CLI error renderer in Unix child processes with HOME and both state
   environment variables removed. **Verify:** human stderr, JSON errors and terminal stream
   errors include both recovery alternatives and retain exit status 5 and `state_error`; explicit
   CLI and environment overrides permit recovery and retain CLI-over-environment precedence.
   Environment isolation applies only to spawned children, never the test process or host.

Run the focused store and CLI regressions, formatting and whitespace checks where the required
Rust toolchain is available. The existing native store-library CI gate covers the Windows result
fixtures. Record unexecuted checks explicitly. This change covers unavailable-default diagnostics;
the remaining discovery/path acceptance cases stay with #25 and do not establish Windows support.

Historical migration SQL/checksums stay immutable. A Windows database receives its captured
execution PATH in the transaction that creates its initial schema. The logical initial-schema
winner owns this default, independently of which opener created the empty physical file.
Already migrated settings, including an unchanged historical PATH, are preserved on reopen.

#### Atomic first Windows execution path (2026-10-03)

Exact Root43 `7c4e136` native lifecycle evidence read the historical POSIX seed before the first
Windows opener returned. A readable settings row and published daemon owner metadata do not prove
that Store initialization has finished. Merely waiting for an expected PATH would conceal the
separate product window: a competing writable opener can return before the physical creator's
post-migration update, or that creator can exit after committing the schema but before the update.

Capture the Windows default once for an opening migration, then parameterize the platform default
inside the existing initial step's `BEGIN IMMEDIATE` transaction, after its admitted version-zero
recheck and before its commit. Initial settings, platform PATH, application/schema markers and the
unchanged historical migration checksum become visible together. An interrupted uncommitted step
can roll back and be initialized by the next logical winner; a committed step already has the
correct PATH even if later migration steps have not finished. A stale loser rechecks admission and
never rewrites the winner's default. A valid empty database can receive its first logical schema;
version-one or newer databases keep their previous PATH, including POSIX or customized values.

Remove only the later physical-fresh settings update, its unused retained flag and diagnostic
stage. Keep guarded file creation, DB/WAL/SHM admission, later migration transactions, all SQL
source/checksums and Unix default behavior unchanged. A private initializer accepts explicit test
data so competing default values can be tested without changing the host/process environment.

Verify with two real WAL connections: before the admitted initial commit, an independent reader
cannot observe settings and a competing writer cannot initialize them; after commit, the reader
sees the winner's platform PATH and a stale opener with a different default preserves it. Cover
rollback before commit and closing the creator immediately after the initial commit, then complete
recovery on another connection with the correct default/checksum retained. Cover an existing empty
private file, completed fresh Store opening, and reopening versioned historical/custom settings.
Keep the native first-run lifecycle's original PATH assertion, actual target/control/exit checks
and thirty-second readiness deadline; qualify on x64, ARM64 and MSRV alongside the Unix suites.

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

The shared Windows engine factory is windows_child::OwnedChild::spawn(Command, ChildWindow),
with Hidden/Inherit policies and no mutable native-child escape. Exact argv, environment, cwd
and stdio remain caller-owned inputs; raw creation_flags on that Command are unsupported.
Register CreationFlags through the audited wrapper: Hidden selects CREATE_NO_WINDOW and Inherit
selects zero, then JobObject adds its temporary suspension. At the final native spawn closure,
after all wrapper pre_spawn hooks, explicitly apply that selected value plus CREATE_SUSPENDED
with the safe setter. Keep the logical CreationFlags wrapper free of explicit suspension so
JobObject resumes only after the external enrollment and its own Job assignment. This final
boundary must preserve CREATE_NO_WINDOW for Hidden and exclude NEW_CONSOLE/DETACHED flags;
Inherit retains zero user flags plus temporary suspension. The runner keeps Inherit behavior.
The native fixture records the actual last-set mask at that spawn boundary. A direct
CREATE_NO_WINDOW control and a direct DETACHED_PROCESS negative control establish that a
windowless private console can expose CONOUT$; absence of that device is not the Hidden
contract. Compare the wrapped child's real device behavior to those controls and retain exact
actual status/root reaping/empty Job/enrollment-failure assertions. Do not claim the device
probe proves window visibility or replace the mask with DETACHED_PROCESS/GetConsoleWindow.
Expose id, cached root try_wait, authoritative tree_empty, start_kill and
async confirm_exit_until/terminate_until accepting one std::time::Instant deadline. Only the
runner can take stdout/stderr through crate-private accessors. Confirmation polls both root
reaping and retained-Job emptiness, never the completion-port wrapper's wait result.
SpawnFailure::NotStarted carries the pre-spawn error; ExecutionMayHaveStarted carries the error
and SpawnContainment retaining the independent kill-on-close Job. Wrapper failure has lost the
root-wait capability, so even an empty retained Job cannot confirm that root exit. Keep that
guard through refusal/quarantine and never retry the uncertain child. Dropping it is emergency
kernel containment, not a successful cleanup result. Native fixtures preserve suspended enrollment,
uncertain refusal, immediate descendants, root-before-descendant exit, one absolute stop budget,
and headless policy through wrapper composition.

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

The joined native mapping helper now reports an empty map-name argument before its entry marker.
Pass [System.Management.Automation.Language.NullString]::Value as only that fixture's mapName
argument, preserving an unnamed real writable mapping. PowerShell otherwise converts raw $null
to an empty .NET string, which the Framework mapping constructor rejects. Keep the six-argument
FileStream overload, writable view, explicit original FileStream disposal, release/join and exact
stable-gate sharing refusal/bytes assertions. Verify: native x64/ARM64/MSRV must reach the original-
handle-closed marker, refuse the stable gate while the real writable mapping remains, then accept
it only after helper/view disposal with payload intact. A binder/setup error still fails visibly;
this fixture-only correction changes no production adapter, deadline or cold-loader behavior.

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

Refine writable bootstrap for the measured concurrent wal-create-new error 80. Existing private
DB/WAL/SHM admission remains strict. If a missing-leaf CreateNew returns only AlreadyExists,
perform one existing-only handle open with the same current-SID/protected-DACL/regular/no-reparse
validation; a private verified winner is accepted under the ordinary existing-leaf contract.
Unknown, broad, disappeared or changed leaves refuse; no permission repair, truncation, second
creation or general retry follows. Mark a database fresh only when this caller's explicit
CreateNew succeeded, preserving configured PATH and historical migration checksums.

Retain all three validated preparation handles and their ancestor guards until SQLite open,
configuration/migration and reported-filename/full-object-ID readback finish. Open the normalized
verified main file with READWRITE without CREATE through the existing win32-longpath VFS and
reject a read-only fallback. Rust creates the empty database explicitly before this call; SQLite
cannot create a different database behind a missing path. Preparation handles deny delete sharing
across this handoff. SQLite's actual ordinary DB/WAL/SHM handles then retain that no-delete policy
during connection use; release the preparation guards after acceptance so normal final-close
checkpoint/sidecar deletion stays intact. Do not retain writable guards past SQLite close or change
the separately reviewed read-only WAL/immutable gate.

Verify: (1) an exact private-leaf CreateNew winner race preserves its bytes/full ID, while a broad,
reparse or vanished winner refuses unchanged; no other error triggers the existing-leaf branch.
(2) actual concurrent first writable opens and final-writer-close/open races preserve committed
rows, current-SID ownership and reported full database identity. Native incompatible renames after
the preparation handoff still refuse while SQLite owns live DB/WAL/SHM handles. (3) the measured
server SSE shutdown fixture succeeds on all three native rows, and existing cold closed/no-journal
Unicode-long-path reads, live WAL commits, final-close cleanup and unsafe-sidecar refusal retain
their original assertions. Record other failures by their precise stage rather than treating
error 80 or a successful reopened handle as evidence that every concurrency failure is resolved.

Correct the separately measured simultaneous-migration and final-close admission failures before
qualifying writable Store concurrency. Keep each historical migration's existing transaction,
SQL bytes, checksum and durable record; do not catch MigrationConflict and reopen the database.
For every pending step, obtain BEGIN IMMEDIATE and re-read application_id/user_version under that
write transaction. Apply only the exact expected predecessor. If another initializer already
advanced to a supported version, strictly verify the applicable recorded migration checksum
under the transaction and skip that already applied step without changing its metadata. Existing
application-ID, too-new, missing/checksum and unexpected backward-version failures remain errors.
Refresh/verify later steps from the authoritative state; ordinary concurrent advance is not a
synthetic conflict. Preserve configured values and exact once-per-version migration records.

For Windows writable preparation, temporarily retain a shared byte-range lock on the already
strictly validated native database handle before any WAL/SHM lookup or creation. Use the safe
Rust 1.94 File::try_lock_shared API: its native range has offset zero and length u64::MAX, including
SQLite's database-lock bytes. Pinned sqlite3WalClose must own an exclusive database lock while
checkpointing and deleting SHM/WAL, and keeps that lock through deletion. This temporary shared
gate therefore excludes that cleanup window; it is not a replacement for SQLite's own lifetime
locks or authority to ignore a permission error. Retain the same full database identity and
ancestor guard throughout admission. No new permanent lock file, custom VFS, FFI or Drop policy.

Retry only TryLockError::WouldBlock on that same retained handle under one absolute five-second
admission bound, established before its first try. Check pre/post operation, cap sleeps to the
remaining duration, and return bounded contention on expiry. Other lock errors and every native
permission/raw-5/reparse/descriptor failure propagate immediately. Once both strictly private
no-delete WAL and SHM guards are live, explicitly and fallibly unlock the database before SQLite
connection/configuration/write. Keep all three leaf guards through existing exact native handoff
confirmation. RAII unlock/handle close covers every preparation error; no live Store escapes a
failed unlock. The gate denies ordinary database writes while held, so keep it limited to sidecar
preparation, never configuration, migration or normal store use. Normal final-close checkpoint
and journal deletion remain SQLite-owned; read-only passive/immutable rules stay unchanged.

Verify: (1) force a stale migration observation using two actual SQLite connections, advance with
the other initializer, then verify correct strict catch-up, exactly five immutable records and
preserved rows. Tampered/missing checksums, foreign/too-new markers and a backward observation
remain failures, not retry success. Keep the real simultaneous first Store-open barrier fixture.
(2) with the temporary native shared database gate actually held, close the real last writable
connection before sidecar preparation. Prove both sidecars still exist, retain full private IDs,
then complete handoff and ordinary writes. After dropping the accepted Store, verify durable
rows and actual final-sidecar removal; the existing raced final-close fixture retains every
iteration and assertion. (3) hold an actual exclusive database lock, prove bounded shared-gate
contention with no sidecar creation; release it inside the same budget and prove admission/write.
Exercise immediate non-contention failure and error-path lock release; hostile sidecar/ACL/reparse
fixtures remain fail-closed. Run all three native Store rows and Unix migration suites; no timing
exemptions, global-budget increase or blanket raw-5/AlreadyExists/SQLite-error retry is admitted.

Correct the measured Windows configuration contention separately from sidecar preparation and
migration catch-up. Keep the existing database/sidecar guards, no-CREATE connection, migration
ownership and read-only behavior. Windows startup owns its connection before returning a Store;
no other statement or explicit transaction may be active while admitting WAL. Split the fixed
WAL PRAGMA from the remaining connection settings. Consume exactly its effective-mode row through
statement completion and explicitly finalize on success and failure, even when stepping fails.
Require the effective mode to be WAL. A completion/finalization error cannot be replaced by an
earlier row; any non-BUSY finalization error prevents retry. Refuse an explicit transaction or
unexpected mode without changing it to success. No retry of Store::open, a SQL batch, migrations,
settings writes, native permission errors or SQLite errors other than exact BUSY/extended 5.

Establish one absolute five-second configuration deadline before its first attempt. Disable
SQLite's internal busy handler for this owned startup phase with a zero timeout: one statement's
internal locking-event waits must not reset or multiply the caller's allowance. Before every
WAL attempt and remaining-settings stage, check expiry. After finalization, check expiry
before accepting success or dispatching again. An exact BUSY 5 from the standalone WAL attempt
may yield for at most ten milliseconds capped to the remaining budget, with its failed statement
already finalized and autocommit verified. Reprepare the same WAL request against the current
committed header, so another initializer's durable WAL transition is observed rather than
replayed as an assumed failure. On persistent contention return the last original BUSY error;
an already expired entry or late successful completion refuses as a deadline error. Apply the
unchanged FULL/foreign-keys/NORMAL/trusted-schema settings once after WAL acceptance, within the
remaining allowance and without retries; restore the ordinary five-second busy timeout before
returning an accepted Store. Do not claim cancellation of stalled native I/O from deadline checks.

Add only fixed debug failure substages for WAL admission and remaining connection settings,
sharing the existing operation counter and original error category/raw code. No path, SQL text,
exception payload, row contents or new filesystem query. Unix configuration keeps its existing
behavior. A failed finalization or post-deadline response never releases a usable Store.

Verify: (1) use real independent SQLite connections on a disposable private rollback database.
Hold a reserved writer, prove an actual WAL-promotion BUSY result, and prove its explicitly
finalized failed statement no longer prevents the other connection from committing. A test-only
observation after a genuine finalized BUSY may signal that writer to release; it must not inject
results or replace the production statement/timeout path. Release within the original bound and
prove successful WAL acceptance, exact remaining settings, committed rows and final cleanup.
(2) retain the writer past a short supplied deadline and verify bounded original BUSY refusal,
autocommit/lock release and no subsequent attempt. An expired entry, explicit transaction,
non-WAL in-memory result and genuine non-BUSY SQLite error refuse without retries or unrelated
changes. Successful already-WAL admission must also consume/finalize its result. (3) retain every
assertion and iteration in simultaneous first Store opens and raced final-close fixtures: both
rows, full database identity, private sidecars, real SQLite no-delete handoff and final journal
removal. Qualify all three native Store/Core/server rows plus Unix suites and lint; if another
configuration failure remains, report its fixed substage instead of broadening retry policy.

Add fixed, bounded failure-stage diagnostics to Windows debug builds at writable Store::open
and StatePaths.ensure. Record only the selected root/outputs/tmp directory stage, state guard,
database/WAL/SHM existing-open versus explicit CreateNew, SQLite connection/configuration/migration,
fresh settings initialization and final leaf validation. Emit one fixed stage plus original
I/O kind/raw OS code or SQLite primary/extended error code when an admitted operation fails.
Include a process-local operation counter for concurrent opens; no path, SID, contents, SQL text,
token, environment or exception payload is rendered. Release builds and Unix behavior stay as
before. Return the original error without wrapping or changing its raw code, and perform no
new filesystem query, permission repair, retry or CreatedNew-to-existing adoption for diagnostics.
The SSE shutdown fixture's observed native 80 does not yet establish which leaf or operation
failed; cancellation of an async polling future also does not prove its admitted blocking Store
operation stopped. The diagnostics establish that boundary before any race-handling proposal.
Verify: (1) a private owned native fixture makes each existing-open/CreateNew failure explicit
and preserves the original error kind/raw code, while all successful facts stay silent. (2) the
actual server shutdown fixture reports the failing directory/database/WAL/SHM or SQLite stage,
with independently identified concurrent operation counters and no extra retry/adoption. (3)
native x64/ARM64/MSRV and lint preserve all current cold gates, managed ACL refusal and SQLite
tests; the exact failed stage is recorded before selecting any behavioral correction.

Native directory-sharing correction: exact Root44 c64832d fails both passive inspector rename
assertions on x64, ARM64 and Rust 1.94, while the other five inspector fixtures and expired-SID
proof pass. Metadata-only READ_CONTROL|FILE_READ_ATTRIBUTES handles did not prevent an empty
retained directory from being renamed. Correct the shared directory-opening primitive rather
than weakening those assertions or compensating only inside PrivateDirectoryPlan. Request
READ_CONTROL|FILE_READ_ATTRIBUTES|FILE_LIST_DIRECTORY (0x00020081), preserving FILE_SHARE_READ
only and BACKUP_SEMANTICS|OPEN_REPARSE_POINT. The list/data access is the minimal additional
directory right that participates in read-sharing accounting; it does not enumerate entries.
There is no metadata-only fallback when the caller lacks that access.

Use this same primitive for DirectoryGuard's existing/creating ancestor chains, the passive
inspector and StockAdapterGuard's retained ancestry. The explicit owner-only repair path may
add its existing WRITE_DAC right; do not add mutation access to ordinary guards. Descriptor-only
regular-file queries remain observations, with existing actual data-access leaf handles still
providing their sharing boundary. Preserve owner/DACL/reparse/full-ID checks, existing-only and
NotFound behavior, the original absolute deadline and finite-worker quarantine. This correction
does not create or repair a root, enable a privilege, relax a sharing failure, or alter Unix.

Verify: (1) bare DirectoryGuard and the passive missing-root plan retain an empty disposable
directory with no open child file; real ancestor rename and junction replacement fail with the
original object/descriptor/full identity intact, then succeed only after the guard is dropped.
Keep both originally failing passive assertions, including the delayed owned observation whose
driver deadline expires while its native handle stays live. (2) a current-SID-owned ancestor
that permits metadata/security reads but denies directory-list access refuses immediately,
without descriptor repair or creating its missing suffix; ordinary private child creation and
managed file/SQLite operations still succeed under retained directory guards. (3) qualify all
existing stock guard/full-ID, cold adapter, Restricted-policy and abrupt-parent proofs on native
x64, ARM64 and MSRV. Never attempt destructive rename/reparse changes on the actual stock tree;
the disposable shared-primitive fixtures establish that boundary. Native proof remains pending.

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

Read-only SQLite needs a retained leaf boundary in addition to existing-only directory guards:
SQLITE_OPEN_READ_ONLY alone may create WAL/SHM in a writable private directory. On Windows,
first try a narrow open_private_read_stable helper using an existing private parent and regular
no-reparse file, GENERIC_READ and FILE_SHARE_READ only. Validate the private descriptor on that
exact retained handle; deny existing/future database write and delete sharing, with no repair.
Only explicit native sharing violations permit the live-read fallback; ownership, path and
other errors stay immediate. A complete existing validated WAL/SHM pair also selects ordinary
WAL reads even when the stable main-file open succeeds. When that stable handle succeeds and WAL,
SHM and rollback journal
are all absent under the guarded root, hold it for the whole read connection and use immutable=1.
This is a closed-database choice backed by the live write exclusion, not a way to ignore committed
WAL data. A present journal or incomplete sidecar pair is never discarded or treated as immutable.

For an active database, retain existing private no-delete-sharing read handles for the database,
WAL and SHM while permitting ordinary writer access. Require both sidecars to exist and validate
their current-SID/private descriptors before SQLite opens. Refuse a missing/unsafe sidecar rather
than creating, preinitializing or taking it over; callers can retry through their normal flow.
Open ordinary read-only SQLite with locking/change detection and the existing five-second busy
timeout, retaining its ability to see later WAL commits. Keep all retained leaves until after
SQLite closes so the last writer cannot unlink/recreate a sidecar in the validation/open gap.
Ordinary SQLite shared-memory bookkeeping can still modify existing SHM bytes; the passive
contract is no state/sidecar creation, deletion, permission repair or migration. Retention can
leave the already existing WAL/SHM for a later writable close to clean up. Native proof must
confirm durable writer close and accurate reads across that transition before acceptance.

Enable SQLITE_OPEN_URI only for an internally constructed immutable URI. Percent-encode every
byte of the exact guarded UTF-8 Windows path after file:, including the verbatim prefix; append
only fixed mode=ro&immutable=1 parameters and select the bundled win32-longpath VFS explicitly.
Reject NUL or non-UTF-8 paths rather than changing their identity. The audited bundled URI parser
decodes escaped bytes directly without treating escaped query characters as options; the native
Windows long-path VFS preserves that filename through its wide-character open. Validate SQLite's
reported database filename against the retained full object identity before admitting reads.
Keep this adapter Windows-only and preserve Unix connection semantics and immutable migrations.

Native Verify: (1) a closed database with no sidecars, including a greater-than-260-character
Unicode/percent/hash path, returns the committed rows and remains free of WAL/SHM/journal creation;
new writable opens and a preexisting writable mapping refuse the stable gate until its release.
(2) an ordinary live reader sees uncheckpointed and later committed WAL rows; concurrent final
writer close cannot replace sidecar identities/owners, closes durably, and a subsequent writable
open/close cleans up the retained original sidecars after reader release. (3) missing roots,
partial/foreign-owned/broad sidecars and close/open races either produce accurate committed rows
under the verified boundary or refuse without state/ACL mutation; no immutable live fallback or
unexpected sidecar recreation is accepted. Record these as native evidence, not source-only proof.

During the build-foundation stage, unimplemented Windows permission changes fail with an explicit
unsupported-capability error, and permission diagnostics report `unsupported`. Compilation alone
must never turn a no-op permission adapter or a numeric placeholder into an owner-only fact.

Managed directories and data files accept only the current SID as owner and only current-SID/
SYSTEM allow entries; existing broad descriptors are refused rather than silently tightened. A
test that begins with an ordinary temporary directory creates a private managed child. Ancestor
directories may have trusted current-user, SYSTEM, Administrators or Windows TrustedInstaller
ownership, with the existing Stock/passive trusted-mutation ACE policy also applied to common
ancestors. Refuse nontrusted mutation grants instead of claiming that write-sharing directory
handles protect foreign-writable ancestry. Retained directory handles exclude delete sharing;
an incompatible existing handle is an actionable refusal, never a reason to drop the guard.
Resolve relative state overrides lexically against the current directory before opening guards;
reject drive-relative/root-relative ambiguity and network state roots. Canonicalize identity only
after every component has passed no-reparse handle inspection and the guards remain live.

The native 18ed2e23 correction keeps the minimal LIST | READ_CONTROL | READ_ATTRIBUTES access
0x20081 but shares READ | WRITE (3), never DELETE. Microsoft's rename/link target-directory
admission needs a parent write open; the earlier share-READ-only list guard also blocked legitimate
child hard-link creation and captured-output finalization after the file reader had exited.
Change only the shared directory helper's sharing policy, consumed by common, Stock and passive
ancestry; keep every existing leaf sharing policy and explicit repair access separate. Reuse the
existing Stock/passive directory-mutation ACE mask/deny/inherit-only semantics for common
trusted-owner ancestry. Current SID/SYSTEM/Administrators/TrustedInstaller are trusted; reject
nontrusted actual mutation, null DACL and unknown ACE types before creating a suffix. Permitted
sibling creation alone does not authorize changing the retained directory or existing child.
No ACL takeover/repair, temporary guard release, new worker, retry class or larger deadline is
introduced. Directory write-sharing is not a claim that trusted-account in-place mutation is
impossible. Retained no-delete object identity plus explicit ACL/no-reparse checks protects the
selected user boundary; ordinary leaf readers/gates continue enforcing their own write/delete
policy. Update stale no-write directory descriptions without weakening their actual proof.

Verify: (1) all three native rows retain the original five complete hard-link/rename fixtures and
their payload, alias-count/identity, held-reader refusal, eventual release and one-/five-second
budgets; they must pass with the parent guards still live. (2) the unchanged bare-empty-directory,
ancestor rename/junction replacement, list-denied and passive retained-prefix assertions still
refuse replacement while held and permit owned cleanup only after release; complete directory
IDs/descriptors remain unchanged. (3) on disposable current-SID-owned ancestry, actual nontrusted
generic write/all, delete/delete-child, EA/attribute and owner/DACL mutation allow grants refuse
before suffix creation without changing the original descriptor; read and inherit-only entries
and the existing sibling-creation-only rights remain acceptable. After restoring only the owned
fixture ACL, verify actual empty ancestry and strict private leaf behavior. Stock cold/Restricted/
parent-loss/guard-release and SQLite sidecar admission gates remain required and unchanged.

Managed file readers retain no-delete sharing. Dashboard/CLI follow reads release their handles
after each frame snapshot, but a concurrent snapshot can briefly prevent Windows finalization.
Retry only native sharing violations for at most five seconds, validating source and destination
again on each guarded rename attempt. Keep all other failures immediate. A reader held beyond
that bound leaves the synced partial intact and reports a real infrastructure failure for normal
recovery; never claim finalization or discard captured bytes when the rename has not succeeded.

Windows output pruning uses the shared guarded private-file deletion primitive after committing
the existing pending-retention transition. Derive the canonical output path from validated run
identity and attempt number; refuse an inconsistent durable relative path before marking pending.
Validate the private parent chain and existing leaf before deleting it; refuse reparse/non-file/
unsafe-descriptor paths, and mark retention complete
only after successful deletion or an already-missing leaf. Keep the existing Unix deletion path.
Native CLI fixtures prove valid output deletion, idempotent missing output, and unsafe-leaf
refusal with the pending state preserved and the unrelated object left intact.

Automatic Windows maintenance retains existing-only private guards for the output root and each
run directory while enumerating, recovering or removing artifacts. Canonical run/attempt names
and durable reference checks remain required. Use the shared guarded rename for repaired partials
and private deletion for retained/orphan files; preserve the five-second sharing-violation bound
and never recreate a missing run directory. Unsafe parents/leaves fail before repair or deletion,
and a failed removal keeps the existing pending transition for a later maintenance pass.
Repaired file contents still sync before rename, matching ordinary Windows output finalization.
The Unix directory fsync path remains Unix-only: Windows does not call a read-only directory
handle a successful metadata flush or require a privileged volume flush. Record rename/deletion
results before the existing SQLite completion transition; this does not claim a hardware-power-loss
directory-fsync guarantee. Native maintenance fixtures use private managed children and cover
partial-tail repair, already-renamed/missing recovery, protection of live attempts, retention and
pending-prune restart, canonical orphan deletion, unsafe-object refusal and bounded reader sharing.
Run the full maintenance harness on native x64/ARM64/MSRV as well as the existing Unix suite.
Age orphan fixtures with the observed clock plus two hours, preserving their grace/budget
assertions without requesting a SystemTime value beyond the Windows representation.

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

The pinned Tokio listener lacks the dead-on-arrival ERROR_NO_DATA reset implemented by its
synchronous counterpart. Retain a creation-time-secured synchronous listener with nonblocking
accept; poll WouldBlock at five-millisecond runtime yields. The library resets a disconnected
stored instance and creates its replacement before returning the accepted stream, preserving
continuous name ownership and first-instance collision protection. Change only that accepted
stream to blocking wait mode and move its handle through the safe OwnedHandle and Tokio
DuplexPipeStream conversions; all payload I/O remains asynchronous with the original 200 ms
deadline. A genuine listener accept failure remains explicit and fails the endpoint closed.
After accept has replenished the protected listening instance, a conversion failure rejects
only that accepted handle. Preserve the original listener, retained state/role lifetime and
collision refusal; log the exact wait-mode/ownership-transfer/Tokio conversion stage and native
cause. Yield five milliseconds before the next accept so a disconnected-peer storm cannot
starve cancellation. Do not retry conversion of that handle, connect to another owner, rebuild
the endpoint or start a new payload deadline. No FILE_NOT_FOUND client retry or full name rebind
is used. Aborting and awaiting the listener
task releases both accepted and listening handles before the lifetime lock drops. Native fixtures
open/drop a peer synchronously before the listener's first poll, prove another bind is still
refused, then deliver valid wake and exact-lifetime control. Existing malformed, idle, nonreading,
remote, collision and teardown assertions remain mandatory. Add an owned current-thread native
fixture that closes a connected peer after acceptance but before conversion, then proves the
same listener remains alive and collision-protected and delivers a fresh valid wake/control.
For each malformed-frame peer, assert listener liveness and duplicate-bind refusal before the
next open; preserve the exact never-as-wake and subsequent valid-message assertions.

Shared notification::instance_identity(root) and instance_identity_guarded(DirectoryGuard) return
the lowercase SHA-256 hex of the fixed locron-instance/v1 domain, SID length (LE32)/UTF-8 bytes,
volume serial (LE64) and full file ID (LE128). Windows-only file-id =0.2.3 supplies its reviewed
get_high_res_file_id safe API. Query the normalized path while the complete no-delete,
trusted-mutation-checked directory guard remains retained; reject an unsupported query instead
of using its low-resolution
fallback. No path text, Unicode folding, DefaultHasher or Rust enum/hash representation enters
the digest. Scheduler task names use this shared identity to avoid duplicate alias registrations.
Pipe names add an explicit protocol version, role and canonical UUID lifetime for control roles;
wake has no lifetime. Listener construction derives its name from the same guard it retains.

Path-based identity and client endpoint derivation use DirectoryGuard::existing_private and
never create or repair a missing state root. Ordinary first-run composition creates private state
explicitly while acquiring its role lock or opening its store, before deriving identity; listener
construction then retains that established guard. Registration/status/maintenance callers use
the guarded form after their explicit existing-state or installation boundary. Missing or stale
hint/control lookups fail without recreating an old root. Native tests call identity, endpoint,
wake and exact-lifetime stop with an absent private-child path and assert both refusal and that
the path remains absent; first-run CLI activation and dashboard tests continue proving explicit
creation and cross-binary identity on existing roots.
First-run test observers wait for an existing owner sidecar or database before calling their
guarded readers. These filesystem-presence checks are passive readiness hints only: expected
child PID, bounded metadata reads and authoritative locks still establish role ownership. The
test harness must never create the state it is proving the CLI creates.

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

The remote-pipe engine fixture is a reviewed generic PowerShell adapter caller. The accepted
stock-binary bootstrap retains verified JSON CmdletInfo objects; its input already becomes
$request through &$locronFromJson. Port both fixture output branches to
`| & $locronToJson -Compress`, without rediscovering ConvertTo-Json by command name. Keep the
localhost remote view, 200ms connection bound, rejected boolean, private listener guards and
actual remote-refusal assertion unchanged. The loader's security, worker limits and original
cold gate remain authoritative. Verify native x64/ARM64/MSRV engine tests execute this exact
fixture with the accepted loader and still reject the remote pipe view.

Maintenance and the registered supervisor use the Windows-only synchronous
notification::request_shutdown_guarded_until(&DirectoryGuard, role, lifetime, std::time::Instant).
Capture min(caller deadline, API-entry plus 200ms) once, before endpoint naming, worker spawn or
runtime startup, and pass that absolute value through the exchange. Reuse the retained existing
state guard and windows::cached_current_user_sid(), a crate-private OnceLock::get-only accessor
which never starts or waits for SID initialization and refuses when no verified SID is cached.
Runtime owns the notification implementation; filesystem development owns that cached accessor.
Check expiry before naming/spawn, after runtime startup and before each pipe-open attempt. Gate
every actual poll of the frame read/write futures, including a final consumption receipt which
was previously Pending; check the clock again when an I/O future returns Ready. Keep timeout_at
alongside those gates because Tokio polls the inner future before its timer. An expired poll
must never initiate another receipt write. An OS write queued before expiry can still complete
later, so a timeout means uncertain delivery, never proof of no shutdown dispatch or permission
to replay. No new guard, directory, SID adapter or detached worker is created on this path.
Join the short-lived worker on every result. Delivery acknowledgement still never proves exit.
The existing ordinary request_shutdown and wake sender keep their current behavior.

Native Verify: (1) a past caller deadline refuses before worker/connection creation; the
cached-only accessor refuses a fresh empty cache without an initializer or filesystem request.
(2) a stalled acknowledgement consumes at most the remaining caller budget and cleanup leaves
no background sender; saturated endpoints cannot reset that budget across retries. (3) a receipt
future returning Pending before expiry is not polled again when made ready after expiry; a ready
acknowledgement crossing the deadline never initiates a receipt. An on-time exchange cancels only
the exact registered role lifetime and still requires actual lock/process exit confirmation.
Keep timeout diagnostics consistent with potentially queued delivery rather than claiming an
absence of shutdown from the client's clock.

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
registration. Set PT0S execution limit, no battery/idle/network gates, IgnoreNew, and disabled
RestartOnFailure (Count=0/no interval); the reviewed native owner supplies three PT1M retries.
Definitions use absolute ExecAction path and correctly
escaped state/role arguments. Read semantic settings/status rather than localized schtasks text.
Preserve enabled/disabled role state on refresh; run roles directly or use a fixed hidden launcher
that waits and propagates the exact exit status. Task.Stop is a documented hard fallback
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

### Registered Windows role supervisor

The earlier Count=3 Scheduler restart candidate is superseded: read back RestartCount=0 with
no restart interval. Scheduler never restarts the entire native owner's four-start policy.
The internal Windows GUI launcher described below replaces the earlier fixed PowerShell launcher
and waits for a native Locron supervisor; Rust owns ordinary daemon/dashboard failure retries and
private exit facts. Introduce only the hidden Windows entry `service supervise --role daemon` or
`service supervise --role dashboard`, with the existing global state directory. Registration uses
the fixed dashboard port. Keep visible install/enable/uninstall/status syntax unchanged.

The supervisor creates the selected private daemon.activation.lock or dashboard.activation.lock
lease with its own canonical UUID, PID and registered-service marker, then binds the matching
daemon-activation/dashboard-activation cancellation endpoint. Retain this lease, its state-chain
guard and verified executable read guard throughout child startup, manual-owner waiting, running,
retry gaps and teardown. It is activation ownership; only the child daemon.lock or dashboard.lock
proves actual role ownership. Scheduler status must distinguish those conditions.

Each child gets a new canonical UUID through paired hidden `--supervisor-lifetime` and
`--worker-lifetime` arguments on `daemon run --service-mode` or `dashboard serve --service-mode`.
Require both options together and only in service mode, with exact internally generated argv.
Before creating any child state, validate the existing selected supervisor activation sidecar,
registered marker, UUID and held actual activation lock. An existing-only nonmutating lock probe
DaemonLock::probe_existing reports LockProbe::Missing/Free/Held without creating/truncating a
lock or writing metadata; Held alone is never PID authorization. Reject an absent/mismatched
parent lease and recheck its UUID around the held-lock observation.
The child owns daemon.worker.activation.lock or dashboard.worker.activation.lock and the
daemon-worker/dashboard-worker endpoint for its UUID before waiting for the actual role lock.
Use that same child UUID in the actual role metadata/control after acquisition. Keep the worker
lease/control through complete role teardown, abort and await all listeners before dropping it.
The ordinary service-mode path without these paired options retains its approved activation
waiter. Supervised waiting never signals a manual role or claims its lock; the registered daemon
still activates automatically after the manual daemon exits.

Share a Windows-only engine OwnedChild factory with the runner. It accepts the caller's exact
Tokio argv/stdio Command and ChildWindow::Hidden/Inherit policy, registers the safe CreationFlags
wrapper for headless service children, and uses
the reviewed suspended Enroll plus independent kill-on-close Job before resume. Expose only id,
try_wait, authoritative tree_empty and start_kill, plus async confirm_exit_until/terminate_until.
Confirmation requires both reaped root and empty retained Job PID list under one supplied deadline.
Classify spawn failure as NotStarted or ExecutionMayHaveStarted; enrollment/resume uncertainty
must carry retained cleanup ownership, quarantine and refuse retry of an unconfirmed worker.
The runner keeps its existing window policy. Do not expose the
raw mutable ChildWrapper. Drop's kernel Job cleanup is emergency containment, not reported proof.
Runtime owns this shared process factory and main/worker flag composition; service owns the
supervisor, dashboard worker composition and guarded registration/quiescence; store owns paths
and existing-only probe. Core adds only the fixed role allowlist, without public async types.

After a genuine nonzero child exit and confirmed complete tree exit, or a typed NotStarted spawn
with no execution possible, retry at most three times in the same combined four-start budget,
each after a cancellable sixty-second wait. Privacy, diagnostic/infrastructure failure and
ExecutionMayHaveStarted refuse without automatic replay. Never retry a still-mapped/unconfirmed old worker or
replay an uncertain spawn. Exit zero or cooperative cancellation ends the registration lifetime.
Eligible known-not-started retry requires the factory's typed NotStarted and only NotFound,
Interrupted or WouldBlock after the retained program/root/privacy/log prechecks succeeded.
PermissionDenied, InvalidInput, Other, Job setup, control and diagnostic failures refuse even
when no native child exists. Keep separate bounded completed and not_started fact entries, with
their combined count at most four. Exhausted NotStarted is infrastructure failure 70, without
inventing a child exit code; actual child i32 statuses remain unchanged.
During shutdown, cancel the exact worker endpoint once its PID/UUID/held lease match this owned
child, including manual-owner waiting. Forward cancellation while startup establishes that lease;
wait for actual child/tree/role/worker lease exit under one thirty-second budget. A remaining
child is an explicit refusal; a validated unchanged Task instance may then use the separately
documented forced Task.Stop fallback. Keep the parent activation endpoint available until child
and listener teardown completes, including retry gaps. Manual PIDs are never terminated.

Write bounded private Rust facts at phase transitions and after verified exit: schema version,
supervisor/child UUIDs, attempt count, waiting/running/retry/stopping/exhausted/error phase and the
actual child exit code. Use fixed service.daemon.runtime.json/service.dashboard.runtime.json paths
and at most four attempt entries, preserving every completed child UUID/PID/exit code.
Facts are diagnostics, never owner proof or arbitrary task source. Record
exhaustion before releasing activation ownership; do not replace actual exit diagnostics with a
synthetic successful Task result. Supervisor infrastructure failures remain explicit errors;
ordinary completed-role retry is proved by this live owner, not inferred from Scheduler settings.
After owned teardown, supervisor exhaustion returns the final actual child exit code and the
internal GUI launcher propagates it. Infrastructure/fact-write failures return a distinct
failure with retained diagnostics, never a fabricated child outcome or successful registration.

The hidden supervisor entry dispatches before ordinary state opening and awaits the service's
`Result<i32, ServiceError>` teardown result. An `Ok` code reaches the process boundary unchanged,
including a genuine child exit of 70 or a negative status. Every `Err` reaches direct process exit
70 without the public renderer or a join of unfinished quarantined work. Private runtime facts
retain the typed cause that distinguishes infrastructure refusal from child completion. Preserve
existing public service error codes (unsupported 2, managed 3, command/I/O 5) and unrelated generic
error handling; invalid Clap arguments still exit 2 before this hidden dispatch.

Move all potentially blocking native child/Job observations and private file operations to one
runtime-owned worker per registered activation. This worker owns the guarded root/executable,
activation lease and exact child/uncertain Job, with one finite command slot; it is not a
per-observation detached thread or a second PowerShell adapter. Keep the secured cancellation
listener on the main async runtime. Startup uses a bounded entry request; cancellation captures
one absolute thirty-second deadline before the first stopping observation or fact/log flush.
Propagate it through queue admission, handle read/probe/re-read, native wait/Job query, exact
shutdown delivery, role/worker lease confirmation, diagnostics, sync and listener teardown.
Check expiry before new operations and after responses, including already-ready responses.

Retain permanent lock handles and validated sidecar handles once the exact child PID/UUID owns
them. Re-read that retained metadata around the nonblocking actual-lock probe; an unrelated
manual role's sidecar is never retained across its exit, because doing so could obstruct the
next child's atomic sidecar publication. Missing startup leaves are observed under the retained
existing root; they never trigger directory creation or a fresh SID initialization. Handle
opens/readback still verify the actual private descriptor and no-reparse identity.

Once the authoritative owned root is reaped and its retained Job reports empty, release the
retained shared daemon/dashboard role-sidecar handle before subsequent fact/log I/O. Keep it
while either proof is uncertain. The combined native proof establishes that the old root and
descendants released their kernel handles; a now-held shared role lock cannot be attributed to
that completed child using its retained stale PID/UUID. Do not probe, signal, terminate or wait
for a new manual owner. Keep the independent owned worker-activation lease exit check, guarded
activation/control lifetime and all existing normal teardown proof. Fact-write failure still
refuses and quarantines remaining ownership rather than fabricating a child result.

Verify the gap with an actual registered fixture: authenticate its running role, finish its
root and descendants, then withhold completion-fact I/O under the same deadline while activation
remains held. A separate native manual owner must acquire that shared role lock and publish its
new PID/UUID sidecar. Release the I/O gate and confirm that supervisor completion and control/
activation teardown preserve the manual owner's live lock, new metadata and process; only the
fixture's own independent teardown may stop that manual process. An unconfirmed root/Job must
never release the old guard or admit an ordinary retry.

The driver awaits each worker response only within the remaining caller deadline. If a native
operation remains in flight at expiry, close worker admission and quarantine its retained root,
executable, activation and child/Job ownership. Do not join an unfinished worker, replay its
operation, claim cancellation/exit, start a replacement child, or schedule a post-deadline error
write. Already-queued writes may complete later; uncertainty stays an explicit infrastructure
failure. The dedicated hidden CLI exits 70 without a blocking renderer after expiry; process
exit closes its owned handles, and kernel Job containment is emergency cleanup, never proof.
A still-live in-process fixture keeps the quarantined ownership until its blocked worker exits,
so another activation/replacement remains refused. Normal completion confirms worker response,
real root plus empty Job, role/worker lease exit and listener teardown before releasing activation.

Preserve registered child stdout and stderr in current-SID/SYSTEM-only service.daemon.log or
service.dashboard.log under the retained state root. Initialize/open append handles before
spawning, pass safe cloned handles as exact child Stdio, and retain their guards through child
tree exit; registered output is not discarded. Runtime facts and log sync/rename are owned worker
operations under the same deadline. Never invoke the independent five-second rename retry on
the shutdown path; any retry uses only its remaining absolute budget. Failed/uncertain diagnostic
persistence cannot fabricate an actual child outcome. At most four attempt facts distinguish
actual completed exits from known-not-started attempts and infrastructure/refusal causes.

Expose Windows-only filesystem::rename_private_until(source, destination, std::time::Instant)
for this owned worker. Preserve the existing rename_private five-second contract. The new helper
uses the same retained existing-parent/no-reparse/private-leaf validation and retries only native
sharing codes 32/33; check expiry before dispatch and after each result, and cap every retry sleep
to the caller's remaining time. An expired or late successful native rename is an explicit
uncertain diagnostic failure, not evidence that no rename happened. The caller still quarantines
an in-flight uncancellable operation and never retries it from a second owner. Verify a held
reader released before the deadline succeeds, a retained reader refuses at the shared deadline
with the partial intact, and an already expired deadline performs no rename or directory creation.
After the approved development handoff, runtime owns only service/windows_supervisor.rs plus
its existing shared-child/main boundaries; service.rs, COM registration and typed maintenance
remain with the privacy/lifecycle developer.

Verify: (1) actual native fixtures stall the owned I/O operation before metadata read, during
fact/rename/sync and at a ready response crossing expiry; each caller refuses under one budget,
keeps quarantined guards/Job, sends no late/replayed control, and launches no later child. Normal
release proves exact owned locks/tree/listeners exit; manual ownership remains unchanged.
(2) four genuine role exits with three real sixty-second waits preserve every actual code,
including child 70 and negative status, with no descendant overlap. Read back Count=0 and observe
no fifth child/whole-supervisor SDK restart beyond the former restart interval. A known NotStarted
uses the same four-start ceiling; uncertain spawn and infrastructure failure never retry.
(3) startup/wait/running/retry cancellation and fact-write failure share the one deadline; bounded
stdout/stderr marker fixtures prove logs survive successful and failed roles privately, while
exhaustion and infrastructure 70 remain distinct. This supersedes the initial source's synchronous
deadline/fact behavior before publication or registration consumers.

Native Verify: (1) invalid/missing/stale parent and child lifetimes refuse without creating state;
manual ownership survives registered waiting, exact cancellation stops the waiter, and manual
exit admits the registered daemon with a new actual owner. (2) suspended enrollment, failed/uncertain
spawn, immediate grandchildren and root-before-descendant exit prove bounded root-plus-tree
ownership; headless flags survive wrapper composition. (3) one initial role failure plus three
actual sixty-second retries preserve all four exit facts, have no old descendant overlap and
exhaust observably; cancellation during startup, waiting, running or any retry gap confirms all
owned locks/processes exit while disabled registrations stay disabled. Actual Scheduler wrapper
exit/PID and task state must not be confused with daemon ownership or graceful exit.

Native CLI qualification selects the complete `service::` unit-test namespace with
`cargo test -p locron --bin locron --locked -- service:: --test-threads=1` on every existing
Windows foundation row: native x64 stable, native ARM64 stable and x64 Rust 1.94.0. Add one
semantic-test step after the original cold core gate; preserve the required job names, the
existing job deadline and the independent package/positive-distribution harness. Library tests
and targeted CLI filters do not select service::windows_supervisor, so successful package
compilation alone is insufficient qualification for this owner.

Give the unchanged cold core step the identifier `native_core`. Gate only the service semantic
step with `if: ${{ !cancelled() && steps.native_core.outcome == 'success' }}`. GitHub's explicit
status function permits this independent private-fixture harness after an unrelated library
failure, while requiring a genuinely successful original cold gate. A failed/skipped cold gate
or workflow cancellation prevents new service admission. Keep the service command, step order,
required names and all deadlines unchanged; use no continue-on-error, so any preceding library
failure still makes the whole required job fail.

Verify the condition matrix: successful cold core plus a library failure executes the full
service assertions and retains job failure; cold failure/skip or cancellation excludes that
step; ordinary success still selects the same complete serial harness. Native logs must show
actual service execution/test results, not a skipped-step or package-build qualification.

Give the existing durable store/engine/server library-contract step that same independent
`if: ${{ !cancelled() && steps.native_core.outcome == 'success' }}` admission condition. It runs
only after the unchanged cold core gate succeeds, in its original position after the required
isolated proof step. A failed Restricted/parent/EOF proof still fails the required job, but no
longer hides independent Store/Engine/Server results behind GitHub's implicit success condition.
Keep the exact cargo command, default library harness concurrency, cold/proof commands, all
assertions, required context names and existing job/operation deadlines. No continue-on-error,
retry, skipped assertion, duplicate test invocation or warmed replacement gate is introduced.

Verify: cold success plus an isolated proof failure actually executes both the unchanged
library harness and serial service harness while preserving job failure; cold failure/skip or
cancellation admits neither independent harness; ordinary success executes the same existing
tests once. Record native library counts and failures as independent qualification evidence,
never as success for the still-failed isolated proof.

Serial selection belongs only to this service harness. Every fixture uses a unique current-SID
private temporary child root and owned executable/processes; no live registration or shared
state is used. Keep real process/Job/lease/cancellation/log/fact/manual-owner assertions. The
genuine four-exit and known-not-started exhaustion fixtures each retain three actual sixty-second
waits; their serial execution therefore includes at least six minutes of real retry waits.
Do not skip, retry, warm the adapter before the cold gate, shorten those waits or widen operation
budgets to obtain a green run. The existing library suites retain their selected harness policy.

Verify all three native rows actually list and run service::windows_supervisor semantic tests,
including real child 70/negative codes, four combined attempts, I/O quarantine, startup/wait/run/
retry cancellation, private stdout/stderr, diagnostic refusal and the new-manual-publication gap.
Record exact revision/runner/toolchain and the service test count/results; failed native
compilation, an empty filter or a package-only pass cannot satisfy this qualification.

The hosted supervisor fixture's observed refusal is its source reader: DirectoryGuard::private
for the destination already succeeded, then open_read_no_follow(std::env::current_exe()) rejected
the native runner's D: source-image ancestry. This evidence does not show a temporary-directory
refusal. Preserve the existing unique disposable TempDir and final private child creation, add
an explicit is_private assertion for its current-SID/SYSTEM descriptor, and retain the normalized
child path. Never repair/adopt the workspace volume or weaken managed ancestry checks.
Each real supervisor must still acquire and retain its own live guards under its original budget.

The corrected fixture source is only the OS-reported already-running native test executable.
Copy it through a retained read-only std::File into create_private_new in that private child,
flush the copied image and retain the existing runtime executable validation. This test-only
reader does not authorize a managed state root or a production input/executable on the runner's
workspace volume. No arbitrary caller path can select the fixture source. The temporary
container remains disposable cleanup ownership rather than runtime security authority.

Verify: every existing supervisor fixture passes the private destination assertion and known
source-copy boundary, then reaches its intended native child/I/O/lease assertions with the real
three sixty-second retry waits on x64, ARM64 and MSRV. Broad ancestry/private-leaf refusals remain
enforced by existing Core contracts; no skip, setup retry, warming or longer lifecycle/test budget
is selected. A successful setup alone does not qualify supervisor effects. A future genuine
temporary-parent refusal needs its own measured fixture correction; this change does not infer it.

The bundled font license has a frozen byte digest. Apply `text eol=lf` only to
`crates/locron-server/assets/fonts/OFL.txt` and
`crates/locron-server/frontend/dist/fonts/OFL.txt` in repository attributes, so native Windows
checkout preserves the already committed LF bytes before embedding/building. Keep the existing
license digest, upstream provenance, font binaries and license blob unchanged; broader repository
normalization is outside this correction.

Verify both exact paths report the selected attributes and retain the original blob/digest in a
checkout with core.autocrlf=true. The unchanged official-font/license/provenance assertion must
pass on native x64, ARM64 and MSRV; record native results without accepting a normalized runtime
digest or removing the original source pin.

Windows registration uses the shared full-file-identity/SID digest for role-specific task names.

### Phase-scoped Task Scheduler transport and persistence ownership

Use one explicit owned Schedule.Service session per snapshot, quiesce, restore or remove phase.
Each session runs one fixed stock PowerShell 5.1 dispatch loop, not a new process per COM call.
Do not cache it globally or retain it across updater registry/PATH/WinGet work. Snapshot closes
and confirms its helper before returning the pure preflight input; quiesce closes before returning
QuiescedServices; restore/remove open fresh sessions and close before returning. State-root guards
and frozen semantic registration facts remain live independently of that transport. Registration
guards mean current-SID ACL proof plus a complete semantic compare before each effect; COM has
no exclusive registration handle, and holding an old COM object is not replacement protection.

The core Windows adapter exposes a narrow ScriptWorkerPermit acquired against the phase's
absolute deadline. Hold the existing single generic permit throughout COM helper ownership,
including confirmed cleanup or quarantine; never acquire another generic permit from that owner.
This retains the ceiling of one filesystem child plus one generic-or-COM child per CLI process.
All non-COM generic reads happen before opening or after confirmed session close. Journal callbacks
perform typed serialization and Rust private-handle writes only, with no PowerShell/COM/registry
calls. Filesystem dispatch uses its separate slot and remains available. An unconfirmed session
refuses the phase and retains its permit/guards; it does not continue to final PATH/WinGet reads.

Reuse the engine's approved OwnedChild with ChildWindow::Hidden, exact stock executable and
NoLogo/NoProfile/NonInteractive/EncodedCommand; remove PSModulePath, change no execution policy.
Configure the child Stdio with safe std::io::pipe child ends, available before the Rust 1.94 MSRV.
No raw mutable child or newly public output-pipe capability is needed. The phase owner retains
the suspended-before-resume Job and confirmed spawn result before sending private requests.
Three fixed I/O threads own the parent stdin/stdout/stderr ends. Each has a finite channel; one
command is active at a time, input is at most 64 KiB, each output frame and total diagnostic stderr
are at most 128 KiB. Read/write concurrently, recognize the cap at limit+1 without EOF, and never
spawn a waiting thread per request. A single phase worker owns the native child/Job and the exact
state/executable guards through close. Pipe EOF is not root/tree proof; join only finished I/O
workers after actual root reaping plus an empty Job, within the remaining phase deadline.

The fixed COM script uses only reviewed selectors for inventory/read/create/refresh/enable/run/
stop-exact-instance/delete and structured version/monotonic-ID/owned-PID input/replies. Reject
unknown fields/selectors, mismatched IDs/PIDs, malformed or oversized frames and unexpected extra
replies. Caller paths and task arguments are data; only compiled fixed source reaches EncodedCommand.
The new loop uses the already selected absolute stock Utility import and qualified JSON commands
with module autoload disabled; it does not change the existing generic adapter's unqualified
converter or assert Restricted-policy acceptance. A stock policy refusal remains explicit, with
no fallback, policy change or mutation replay. Check every task's current owner/protected ACL,
complete expected semantic definition and enabled transition immediately before its effect.
Task registration/start/stop timeout can leave a Scheduler effect in flight outside the helper
Job; saved intent and live readback determine recovery, never automatic replay or fabricated exit.

Capture one absolute thirty-second caller deadline at phase entry, including admission, cold
guard/SID verification, spawn, callback persistence, COM requests, role-control/lock observations
and session teardown. Core permit admission and every request accept that existing deadline,
with no fresh timeout per action. Before each new native or I/O operation and after every ready
response, check cancellation/expiry. The caller driver refuses at expiry without joining an
unfinished owned worker. That worker quarantines its retained guard/Job/permit, sends no next
mutation or callback after a late completion, and never treats emergency Job close as confirmed
cleanup. This is a bounded refusal; already queued COM/file I/O may still complete later.

The persistence callback is owned, Send + 'static, and moves into this phase worker; an arbitrary
borrowed synchronous callback cannot be made time-bounded by an outer timer. Distribution supplies
an owned journal writer or an Arc to its serialized writer, and on an unconfirmed phase returns
pending/refusal without blocking on that writer, appending another frame, or attempting rollback
behind still-live guards. Normal phase completion returns only after all admitted callbacks and
owned helper teardown are confirmed. Preserve the already frozen complete-record/count capacity
plan and before-effect intent/result ordering. No additional retry/failure callbacks are admitted
past expiry, and no background phase may start another helper or task after refusal.

Before creating or changing any task, validate the exact encoded action's Windows command-line
length against the native 32,767-UTF-16-unit ceiling, including executable and terminator. The
4,096-unit maintenance path ceiling does not promise that every pair of maximum-sized paths can
fit the nested transport; an oversized complete action refuses with zero task effects.

Verify: (1) actual cold COM inventory plus multiple semantic reads reuse one owned PID under one
entry budget; concurrent generic use remains within the two-child ceiling, and snapshot/quiesce/
restore closure permits subsequent real registry/PATH reads without self-deadlock. (2) stalled
startup/stdin/output/COM/callback and a ready response crossing expiry refuse under one deadline,
retain quarantine, send no later effect/callback and never replay a mutation. Wrong-ID/extra/
oversized frames and abrupt parent exit preserve Job containment and honest pending recovery.
(3) private Unicode/quote/backslash paths round-trip exact argv, action length overflow refuses
before task creation, changed task definitions/ACLs refuse before effects, and forced stopping
addresses only the unchanged RunningTask.InstanceGuid. Confirm actual role/activation locks and
task-instance exit independently; instance state, engine PID or helper EOF cannot substitute.

The native GUI action and sealed activation proposal below supersede the fixed stock PowerShell
EncodedCommand/one-value EncodedArguments launcher plan. Historical probes of that launcher are
not evidence for the revised action. Keep its source unselected while this complete proposal,
repository issue Verify criteria, companion inventory and producer/consumer interfaces are reviewed.
No dynamic PowerShell source, base64 fragment splice or installed script is selected instead.
Readback compares semantic principal/trigger/power/restart/action fields and retains a disabled
registration on refresh. COM may return account names after SID-based registration, so translate
actual principal/logon-trigger account identifiers to SIDs before semantic comparison; environment
username text never proves account identity. Cooperative failure first waits under the shutdown deadline; Task.Stop
then targets only the validated owned registration with the unchanged registered-service lifetime.
After this hard fallback, actual role-lock exit is still required and forced completion is
reported explicitly; unowned/manual holders remain a bounded refusal or registration deferral.

### Internal GUI launcher and sealed automatic activation (proposal)

This complete scope revision remains source-held until the completed documents, Project #30/#34
criteria, companion inventory and persistence reservation have been reviewed. It replaces the
earlier fixed PowerShell waiting-launcher choice and the held GUID-only activation candidate.
Keep the console `locron.exe` unchanged and add `locron-service-launcher.exe`, a small internal
Windows GUI-subsystem binary in the existing CLI package. Use the safe crate-root
windows_subsystem attribute; do not patch a published PE image, attach/allocate a console or
change the primary CLI subsystem. A `windows-service-launcher` feature gates the companion build
and keeps ordinary Unix builds/Cargo installations single-binary. Windows ZIP builds and
documented Windows Cargo service builds explicitly enable it and build both same-version
binaries. A missing/mismatched helper refuses registration with the exact channel's repair
guidance. No new crate, general GUI, script, policy exception, Visual Studio runtime, PowerShell 7
or system-wide installation is introduced.

The GUI launcher has only fixed role composition and bounded read-only identity/version probes.
Its native argv parser never echoes capability values. It discovers the console binary as the
constant sibling `locron.exe`, verifies both guarded objects, current SID, version/architecture/
launcher ABI and the registered definition, and uses OwnedChild::spawn with ChildWindow::Hidden
for the exact hidden `service supervise --role daemon|dashboard` child. Preserve suspended dual
Job enrollment before resume, the final native CREATE_NO_WINDOW mask, root-reaped plus empty-Job
proof and the original lifecycle deadlines/quarantine. Retain the exact Child and executable/root
guards through its entire lifetime. Propagate its genuine i32 status, including 70/negative;
infrastructure failure is direct 70 without a public renderer or unfinished-worker join. The
launcher neither owns the daemon/dashboard lock nor adds a second four-start retry policy.

The thin companion entry is an explicit second [[bin]] in the existing CLI package, with
required-features = [windows-service-launcher] and default-run = locron. The feature is absent
from default features. Its crate-root cfg_attr(windows, windows_subsystem = "windows") applies
only to that image; ordinary CLI and non-Windows default builds remain unchanged. Capture its
original startup Instant before SID, path, runtime or registration work. The first owned worker
operation opens only the reserved CONOUT$ device read-only with OPEN_EXISTING and compatible
read/write sharing, then closes that handle. Record only raw open/error facts: success requires
opened=true/error=null; failure requires opened=false/an actual nonzero i32 OS error. Errors
without an actual nonzero raw code refuse without emitting a probe object. A failed open never
becomes a boolean claim about attachment, and primary APIs do not guarantee the unattached
error code. No AttachConsole, AllocConsole or GetConsoleWindow path is introduced. Native GUI
and attached/detached console controls must establish the actual expected absence code; unrelated
permission/sharing/path failures refuse qualification and cannot authorize role dispatch.

Freeze the read-only modes as sole --version and sole --identity-probe. Version emits exactly
locron-service-launcher <workspaceVersion> followed by one newline. Identity emits one strict
JSON object with exactly schema=locron.windows-launcher-probe/v1, version, target,
launcher_abi=native-gui-v1, initial_conout_opened:boolean and initial_conout_error:i32|null,
with no extra/trailing bytes. Support only the documented native x64/ARM64 MSVC release targets.
The target fact derives from actual target_arch and target_env; unsupported targets refuse
probes, and an unsupported GNU build never advertises MSVC. These are actual process/first-entry
facts plus compiled release metadata, not receipt ownership, file-ID authentication, successful
registration or an activation witness. The caller must independently verify guarded pair bytes,
versions and actual PE subsystem.
Neither probe discovers/creates state, queries SID, connects to an activation pipe or starts a
child. Keep serialized output <=4 KiB and all worker/output work under its original 30-second
startup deadline, captured before the worker is admitted. Expired/uncancellable output is a
direct 70 refusal with retained worker ownership until process exit, never timely success
inferred from queued bytes. Version stdout uses the same finite worker/deadline.

Use one unbuffered owned duplicate of the inherited stdout handle for probe bytes. Inside the
existing worker, gate before/after Stdout::as_handle().try_clone_to_owned(), take safe File
ownership and gate every write/flush/drop completion under the same deadline. Never write probe
bytes into the process-global buffered Stdout/StdoutLock: Rust process exit can flush that buffer
after worker expiry. A missing/invalid inherited handle or native I/O error is fixed direct 70;
no path fallback, console attachment or public renderer is introduced. The worker retains its
File across uncancellable I/O, and late bytes remain uncertain rather than a successful probe.
Verify an actual blocked owned native pipe under the driver deadline, release only the held
test peer after refusal and confirm no later admitted operation; independently cross the deadline
inside an already-ready writer and prove no flush/success. Real GUI output is exact unbuffered
JSON/version bytes on owned file/pipe handles, with original 30-second admission and no new budget.

The role mode accepts only the fixed ordered state-dir, role and optional run-capability
arguments. Its parser errors never render argv or raw capability bytes; reject extra/duplicate
options or trailing data with the fixed invalid-arguments exit. The service-owned standalone
windows_activation_wire models/validators supply the exact capability/Hello/Permit/Child/Seal
ABI to both bins, with no dependency on service.rs or a public renderer. Do not wire an
unavailable consumer or synthesize a success path. Only the existing reviewed timely Permit
and actual peer/child/lifetime seal admit child dispatch or witness construction.

Verify: Cargo metadata exposes the required-features gate and default-run=locron; ordinary
builds/default cargo run select the console command. Explicit feature builds the same-version
GUI target on native x64/ARM64/MSRV. Independent PE inspection plus
the real first-entry CONOUT$ fact establish the entry policy. Strict parsing exercises missing,
duplicate, reordered, invalid-Unicode option/role/capability and secret-bearing extra arguments
with no raw secret in errors/output; state paths remain OsString data under the existing guarded
path policy. Test strict success/null versus failure/nonzero raw facts, non-OS/zero-error refusal,
and qualification's rejection of unrelated raw-3/5/32 errors. Native real GUI and separately
owned DETACHED_PROCESS console helpers must establish the actual absent-device error; an owned
CREATE_NEW_CONSOLE helper must observe a real successful CONOUT$ open. Preserve those exact raw
receipts; neither error equality alone nor a failed open proves attachment/visible-window geometry.
Inspect actual mapped PE 2/3 independently; no error swallowing or GetConsoleWindow surrogate
is permitted. Owned probes keep no state/task/PATH
mutations, emit the exact finite schema and refuse stalled/late output under the original bound.
Run the required feature-gated Windows launcher semantic target serially on all three existing
native rows after cold Core succeeds; keep the cold gate, existing job names and independent
service/library conditions. The distribution owner's native package checks separately consume
these exact two read-only modes and verify the full installed pair. Producer/service effects
remain separately held until native runtime qualification and the shared wire ABI/source review.

Read back one versioned action: Path is the guarded companion; Arguments is exactly generated
`--state-dir "<recorded root>" --role daemon|dashboard --run-capability "v1:$(Arg0)"`; and
WorkingDirectory is the recorded guarded root. Use the existing exact native argv escaping,
fixed argument order and one occurrence of every option. Both full executable identities, the
`native-gui-v1` action kind/launcher ABI, role, normalized root, instance digest, task name,
current-SID principal/logon trigger, task ACL, INTERACTIVE_TOKEN/LUA, PT0S, IgnoreNew and
RestartCount=0 belong to the semantic fingerprint. Transient Run parameters, observations and
the enabled flag do not. A definition
for the old PowerShell action is not silently accepted as the new fingerprint.

The action's Run value is data only: a canonical nonnil context UUID, one colon and 64 lowercase
hex digits from 32 freshly generated OS-random bytes. The stored action supplies the `v1:` prefix.
Only absent capability input, an empty string, exact `v1:` or exact `v1:$(Arg0)` selects ordinary
unwitnessed startup; these cover native no-parameter/empty/unsubstituted Logon behavior without
depending on base64 decoding. Any other malformed value refuses. There is no PowerShell source
interpolation or Run-to-environment assumption. Keep ordinary unparameterized Logon functional,
but it cannot confirm an operation's missing automatic-activation witness. Genuine Logon/reboot
verification remains the standard-user #36 gate, without changing the caller host's session.

Before one SDK Run, the service phase owner creates a protected first-instance local pipe for
this role/context under the retained state/SID identity and captures its existing absolute
thirty-second phase deadline. The name contains no secret. Keep the reviewed current-SID/SYSTEM
descriptor, remote rejection, retained guards and bounded hybrid accept/conversion behavior.
Allow only the launcher and its supervisor to authenticate; at most two accepted connections
plus one listening instance remain live. Frames are versioned fixed data of at most 1 KiB, with
the existing at-most-200 ms payload cap bounded again by the remaining phase time. Invalid peers
refuse the phase without another Run. No wake/control/job protocol is widened.

Persist RunIntent with an immutable context UUID and fixed-width capability digest before the
sole SDK call. The digest uses versioned length framing of operation/context, current SID, full
state identity, role and nonce. Raw nonce/Run capability stays only in live producer/transport;
never put it in the journal, receipt, RuntimeFacts, application logs or diagnostics. Entropy
failure refuses before Run. Preserve the actual SDK-returned InstanceGuid and durably confirm
that observation before issuing a child-start Permit. Revalidate the exact owned task and its
sole live IgnoreNew instance; an SDK return, task state or EnginePID alone is not proof.

The GUI captures its existing startup clock before SID/guard discovery. It first sends the fresh
expected capability/context/role and authenticates its real client_process_id on the actual
first accepted stream. This exchange alone cannot authorize OwnedChild creation. The producer
issues a bounded Permit only after the SDK GUID observation is durable, its owned definition
still matches and the original phase deadline remains live. The Permit binds this connection,
context/digest, role and GUID and carries the producer's remaining original budget. Clamp that
remaining duration to the GUI's pre-SID entry clock and existing startup deadline, rounding down
when encoded: gui_deadline = min(original_gui_deadline, gui_entry + permit_remaining). Refuse
overflow; this shortens admission rather than starting a new clock at receipt. Check expiry
at every poll and after Ready, then before and after each guard/child operation. A queued Run
arriving after phase expiry has no timely Permit, refuses without dispatching a child and leaves
the operation UnknownStart/pending. A queued I/O operation begun before expiry remains uncertain;
retain/quarantine its ownership and never infer cancellation or replay permission from timeout.

Only the timely Permit permits the exact hidden OwnedChild spawn. Keep that retained child/Job
and the authenticated first connection through the two-peer exchange, report its actual id and
pass only a bounded private stdin bootstrap to it. The second accepted stream comes from the
supervisor: its kernel client PID must match that retained child, and its new internally generated
supervisor UUID must match the actual registered activation lease/control lifetime. The producer
validates both peers and owned lifetimes, then sends the bounded matching seal ACK within its
original phase budget. PID/query/connection, registration, capability or deadline failure never
falls back to GUID-only authorization or another child/Run dispatch.

Expose a CLI-private AuthenticatedActivation value with no public constructor and no Deserialize
conversion from argument text or raw frames. It may implement Clone, Debug and Serialize only,
with private fields; RuntimeFacts needs no raw-field constructor/getters. The service-owned
supervisor consumer runs inside the existing finite lifecycle/I/O worker, after actual supervisor
activation/control ownership is established, with its retained lease/guard and original deadline.
Its boundary is authenticate_supervisor(&StatePaths, Target, supervisor_lifetime, Option<Bootstrap>,
absolute_deadline) -> Result<Option<AuthenticatedActivation>, ServiceError>; service owns the
asynchronous exchange and untrusted Bootstrap type, runtime supplies its existing owned context.
It accepts an optional untrusted private-stdin bootstrap of at most 1 KiB: absence returns an
unwitnessed None. With a bootstrap, it verifies the actual live server peer, expected capability/
context/role, its own PID and held supervisor UUID/control, and the matching timely seal ACK before
privately minting Some(AuthenticatedActivation). Parsing an ACK creates only an untrusted wire
value; neither that value nor a supplied GUID can construct the opaque local type. Runtime retains
and serializes the immutable optional value without adding a second worker or constructor.
RuntimeFacts adds one nullable serialized witness containing context UUID, digest, Scheduler InstanceGuid and
the authenticated launcher PID; the outer actual supervisor PID/UUID and worker PID/UUID remain
authoritative fields rather than duplicated claims. A hidden scheduler-instance argument, if
retained as an untrusted hint, cannot construct that value. Ordinary/manual supervision with a
valid live GUID alone remains unwitnessed. Public LockMetadata is unchanged. Keep RuntimeFacts
within its existing 16 KiB cap; reserve and test the complete maximum object across all four
attempts, typed causes and both absent/present witness forms before enabling the producer.

Final activation confirmation requires the exact current task GUID, matching context/digest in
protected facts, the authenticated supervisor PID/UUID with held activation/control lifetime,
and fresh actual role/worker ownership/readiness. Bind both executable objects and the recorded
root throughout that observation. No stale receipt, bare GUID, manual role or independent task
state can substitute. Startup exchanges do not gain independent thirty-second budgets: preserve
the phase owner's original deadline through SDK work, callback, transport and final observation;
the launcher's/supervisor's existing startup/lifetime owners keep their own original finite
admission boundaries. Check every polling/dispatch boundary and already-ready response. A queued
delivery can still complete after caller expiry; it remains uncertain and never authorizes replay
or a newly dispatched post-deadline callback. Quarantine in-flight uncancellable I/O and retain
its ownership as already specified; do not claim that a timeout proves no delivery or that Drop
confirms tree exit.

The typed activation fact adds immutable context/digest at the RunIntent edge beside the existing
one-time observed GUID, unknown-start bit and final confirmed GUID/lifetime. Original enabled
flags remain origin; disabled roles never Run or gain a witness. Keep activation callbacks
<=4E+2 and restore/quiesce <=4R+2: witness traffic adds no persistence callback. Recovery never
replays Run for any existing intent. It may complete by fresh live verification of an already
authenticated matching witness; loss before authentication remains UnknownStart/pending, with
zero redispatch. A later unwitnessed Logon or arbitrary different GUID does not rewrite the
original context/digest or fabricate completion. These explicit pending semantics supersede the
held candidate's ability to confirm an unrelated fresh automatic GUID. Retain the verified new
installation and bounded status/refusal; do not perform post-receipt file rollback merely because
startup could not be confirmed.

The canonical unsigned Windows ZIP inventory is exactly locron.exe, locron-service-launcher.exe,
README.md, LICENSE-MIT and LICENSE-APACHE. Embedded font licenses remain their existing source
integrity inputs; they are not extra extracted ZIP members. Verify both PE32+ architectures,
console subsystem 3 versus GUI subsystem 2, same version/launcher ABI, absent certificate tables,
final individual digests and the unchanged stock-DLL allowlist. Preserve the existing 64 MiB
aggregate download/unpacked limit.
Standalone payload inventory is exactly locron.exe, locron-service-launcher.exe, README.md,
LICENSE-MIT, LICENSE-APACHE, uninstall.ps1 and .locron-installer.ps1, plus its separate receipt:
seven payloads/eight managed leaves. The receipt requires both hashes/bindings and rejects
missing, unknown, duplicate, wrong-architecture/version/ABI or reparse members. A one-executable receipt cannot
authorize a two-executable takeover; Windows has no published baseline to migrate silently.
Install/update/uninstall, read-only status, retained helper requests, package ownership and
WinGet ZIP portable/package inventory all consume that same pair. Declare only locron.exe as the
nested portable command alias; the companion remains an exact sibling archive file tracked by
the package's complete file index. Listing a companion without an alias would still create its
filename link, so omission of PortableCommandAlias is not a link-suppression mechanism. Native
readback must prove WinGet owns both members; standalone never repairs or removes
a WinGet companion independently. Source-build service instructions require the matching
companion while preserving Cargo-managed update/removal ownership.

Inventory all exact current-SID registrations bound to either member across every recorded root.
Disable owned activation and confirm actual GUI launcher plus supervisor/worker/root/Job/control
exit before opening both final replacement gates under the original shared deadline. Unowned
holders of either file refuse without name-based killing. Journal each member's staged/old/new
identity and verified bytes before its effect; keep tasks disabled across a mixed intermediate
pair and publish a complete receipt only after both members verify. Pre-receipt rollback restores
both verified old members and their receipt before service restoration. Refresh/read back both
new bindings, preserve disabled roles and require sealed activation of requested enabled roles
before final completion. Interrupted member replacement can never be treated as a complete pair.

Explicitly revise the unpublished Windows journal ceiling from 128 frames/16 MiB to 140 frames/
18 MiB. The existing seven-leaf, R=E=2 complete reservation is 12*7 + Q10 + S10 + A10 + outer10 +
PATH4 = 128; the eighth managed leaf makes 12*8 + 10 + 10 + 10 + 10 + 4 = 140. Do not retain the
old limit and thereby refuse the ordinary two-role installation. Keep each complete frame
<=128 KiB, including its existing 68-byte framing overhead, and each private service record
<=128 KiB. Maximum frame payload stays 131,004 bytes. Including one maximum incomplete tail,
(140+1)*(131004+68) = 18,481,152 bytes < 18,874,368 bytes (18 MiB), leaving 393,216 bytes.
This storage bound does not admit a partial frame as an authoritative record. Retain checked
actual whole-object reservation, including both old/new file bindings, action fingerprints,
normalized paths, all optional activation keys/digests/GUIDs/PIDs, and every legal forward or
pre-receipt rollback branch. This ceiling is not permission for extra callbacks or unbounded
poll snapshots. Recompute the actual remaining branch at recovery admission; every capacity
failure occurs before disable, Run, file or PATH effects. Limits for Core IPC, records and Unix
products remain unchanged. Review the pure reservation and complete maximum objects before
effectful producer/consumer integration.

Reconcile every active distribution write-ahead, preflight, complete-record and package-inventory
paragraph to the same 140-frame/18 MiB aggregate, seven-payload/eight-managed-leaf contract before
selecting its source. Earlier single-executable six-payload and 128-frame/16 MiB text is superseded
where it states an active contract; clearly historical Findings evidence remains historical.
Use complete typed old/new pair objects and both full identities throughout those paragraphs,
not a companion basename as authority. This reconciliation must also cover the later distribution
owner's paragraphs when the plan is integrated; no future source is imported by this doc change.

Implementation order and Verify:

1. Freeze the revised product/architecture, exact pair/action ABI, typed context/digest edges and
   full serializer/journal reservations with Project #30/#34 readback. Verify ordinary R=E=2
   eight-leaf forward and earlier rollback fit 140/18 MiB, while overflow has zero effects.
2. Build/package the actual GUI companion on native x64/ARM64/MSRV and pin both inventories.
   Verify actual mapped PE subsystems, default GUI first-entry CONOUT$ absence, exact child spawn
   mask without DETACHED/NEW_CONSOLE, version/ABI/digests/stock imports, redirected console CLI
   behavior and missing/foreign/blocked companion refusal. No GetConsoleWindow surrogate.
3. Implement the producer/service launcher and sealed runtime consumer as separate reviewed
   source slices. Verify actual SDK Run-delivered nonce, kernel launcher PID, retained Child PID,
   new supervisor/worker UUIDs, real control/role readiness and current GUID correlation. A manual
   native launch supplied with the same valid live GUID alone must stay unwitnessed and cannot
   complete the operation; the manual owner survives waiting/cancellation.
4. Exercise wrong/expired/reused capabilities, forged PID frames from another real peer,
   nil/noncanonical UUID, wrong role/root/executable, collision/remote refusal and late-ready
   transport under the original bounds. Verify no extra Run, operation replay or newly dispatched
   post-phase GUI child/callback; an already queued uncertain operation cannot count as confirmation.
   Stall first authentication, GUID persistence and Permit delivery past expiry: no new GUI child
   may dispatch. Preserve the supervisor's separate existing ordinary worker retry policy.
   Verify no raw secret in durable/application output and retain actual root/tree/guard cleanup.
5. Run native VT_EMPTY demand start and a unique no-parameter time-triggered action with exact
   absent/empty/unsubstituted parser controls. Verify ordinary headless startup without witness,
   genuine child status/Count=0, and unchanged four-start retries. #36 separately records actual
   standard-user Logon/reboot/locked-session evidence on the same published pair.
6. Crash before/after RunIntent, SDK return, observed-GUID persistence, authentication and final
   confirmation. Verify existing intent never redispatches; authenticated live readback can finish,
   lost pre-authentication stays pending, disabled roles stay untouched and partial pairs restore
   correctly before receipt. Verify standalone and WinGet pair holders/rollback/capacity with
   unique hosted fixtures; no local host policy, tasks, PATH or session changes.

Service/COM/typed-record ownership remains with the privacy developer; RuntimeFacts/supervisor
and thin main consumption remain with runtime; companion packaging/receipt/install/WinGet and
aggregate journal ownership remain with distribution. Share only the reviewed opaque consumer
ABI. Source stays held until these owners and the parent review the complete proposal and its
repository issue Verify criteria; no parallel legacy launcher implementation or success stub is authorized.

Updater/package maintenance inventories every current-SID Locron task bound to either verified
installed executable member, across all state roots. Compare both full Windows file identities
while retained
no-follow file/ancestor guards remain live; path lowercasing and filename matching do not prove
an executable binding. Parse only the exact native GUI launcher action and its strictly generated
fixed argument template, with both executable bindings described above. Reconstruct the private
state guard and shared full instance digest, and validate role, deterministic task name, marker,
task ACL and both executable objects.
Malformed, foreign or unconfirmed bindings refuse maintenance before stopping any process.

Expose a guarded in-memory snapshot and a serializable versioned restore record containing the
current SID, previous executable pair, and each prior registered role's state root, instance digest,
task name, original enabled flag and exact semantic definition fingerprint. Exclude transient
run/result observations and the enabled flag from that fingerprint. Quiesce disables activation
for every owned binding before requesting exact lifetime shutdown; actual role-lock and waiting
launcher exit remain necessary. The private journal records confirmed quiescence and explicit
forced fallback facts, without containing executable task source or arbitrary role arguments.
Restore reconstructs guards and checks every existing definition against the recorded fingerprint
before the first write, then binds only prior registered roles to the new verified executable pair and
restores their exact enabled flags. Changed definitions/roots/SIDs fail closed; disabled roles stay
disabled. Missing registrations are not silently recreated from a stale record.

Freeze the CLI-private Windows maintenance protocol as snapshot_executable_roles(executable_pair),
ServiceSnapshot::{restore_record,persistence_plan}(), quiesce_roles(snapshot, persist),
QuiescedServices::restore_record(), recover_quiesced(record, persist),
ServiceRestoreRecord::persistence_plan(), restore_roles(quiesced, new_executable_pair, persist),
restore_record(record, new_executable_pair, persist) and remove_roles(quiesced, persist).
Those entry parameters are complete typed console/launcher pairs; a console path or a guessed
sibling basename cannot supply the pair's native ownership proof.
The persist callback receives a typed ServiceRestoreRecord and returns
ServiceError on a failed private-journal write. Invoke it with the complete original snapshot
before disabling the first task; callback failure changes nothing. Persist confirmed quiescence
and each explicit forced-stop fact before releasing both old executable read guards. Consuming the
live snapshot retains state-root and registration guards in QuiescedServices but releases those
old executable guards only after actual all-task exit, so they cannot collide with the updater's
exclusive replacement gate.

ServiceRestoreRecord uses a deny-unknown-fields versioned schema: current SID, both previous
absolute executable paths and full volume/file identities, prior roles, explicit phase and bounded
explicit forced Task instance identities. Each role stores its fixed daemon/dashboard selector,
existing root, shared full instance digest, deterministic task name, original enabled flag and
semantic definition fingerprint. Encode full identities and SHA-256 fingerprints as fixed-width
lowercase hexadecimal strings to preserve every bit through JSON/PowerShell. Bound the inventory
at 256 distinct registered bindings and refuse overflow before mutation. Records contain no
arbitrary executable source, command template or role arguments.

Add the Windows-only opaque Core PrivateDirectoryPlan::inspect_until(path, deadline) for the
fresh installer/service's no-effect preflight. Accept only the already frozen maintenance-path
contract: local absolute UTF-8 Windows paths, normalized valid components and at most 4,096
UTF-16 units. Validate this shape before SID or native work. Retain all existing ancestor
handles using the current no-reparse, trusted-owner/no-untrusted-mutation and no-delete
sharing policy. Stop only at a genuine NotFound; wrong object type, reparse, permission or other
errors refuse. An existing final root must carry the strict private current-SID/SYSTEM posture.
Canonicalize only that existing guarded prefix, then join the validated missing components;
no missing component, managed file, task or journal is created or repaired.

Expose readbacks normalized_path(), existing_guard(), existing_identity(), root_identity() and
missing_components(). existing_identity is the nearest existing directory's full volume/file
identity queried while its chain remains retained; root_identity is Some only for an already
verified private final root. Missing components describe absence observed under that guard,
not permission to adopt a later object or proof that this operation created one. No new-root
creation or rollback/removal API is selected by this passive plan.

Forward the same caller Instant into current_user_sid_until and check it before and after
each native guard, descriptor, canonicalization and full-ID query. Use this synchronous helper
only inside the existing finite owned phase worker: a deadline check cannot cancel blocked
native I/O on the caller thread. An unfinished worker retains its opened handles, sends no
later effect/callback and is never joined by the expired driver. Preserve ordinary state-path
creation and Unix APIs; maintenance's new inspector never invokes DirectoryGuard::private.

Verify: (1) existing private roots and missing multi-component Unicode/>260-character targets
return the exact retained root/ancestor full identity and missing suffix with no managed
directory/file/task/journal effects; an initially absent root remains absent after plan drop.
(2) broad/foreign roots, mutable foreign ancestors, reparse chains, wrong object types and
invalid/overlong paths refuse without repair; actual ancestor rename/reparse attempts fail
while the returned plan is live. (3) expiry before an empty SID cache initializes refuses with
no dispatcher admission, while a delayed owned native observation cannot admit another
operation or effect past the original deadline. Creation/rollback qualification follows only
after a separate atomic-created-object and finite typed fresh-record plan is approved.

Maintenance records admit only normalized UTF-8 local Windows paths of at most 4,096 UTF-16 units,
including prefixes/separators. Refuse control and forbidden filename characters in normal
components, unsupported prefixes, relative components or overflow before effects. The bound for
a future valid executable path's JSON representation is 3*4096+2 bytes, preserving Unicode and
greater-than-260-character paths without increasing journal limits. Compute the maximum serialized
record from the complete frozen snapshot plus the bounded future binding/definition, progress
and forced-instance fields; refuse if it cannot fit the 128 KiB private-record limit.

Expose a pure ServiceRestoreRecord::same_original(&other) comparison for the private journal's
typed service slot. Compare the version, SID, both previous executable paths/full volume/file bindings,
and the ordered roles' original role/root/instance/task name/enabled flag/definition fingerprint.
Ignore only mutable phase, next binding, forced facts, role progress and future-definition fields.
This accessor establishes a common frozen snapshot origin; each record must still pass account/
phase validation, allowed transition checks and live guarded ownership before any effect.
Verify: tampering with SID, either full-ID component, path/root/name, original enabled state,
definition or role order refuses the comparison; valid forward progress/future fields preserve
the same origin without independently authorizing a filesystem or task operation.

Also expose the pure ServiceRestoreRecord::matches_previous_identity(&FileIdentity) boundary.
Compare both full volume/file components of the frozen previous binding with distribution's
typed original executable inventory identity, without exposing mutable wire fields. Two
individually valid records do not establish that they refer to the same original executable.
Verify: the exact original identity matches; identical bytes or path paired with a different
volume or any different file-ID bit refuses. This comparison performs no I/O and does not
authorize replacement, restoration or any task effect. This retained console-specific accessor
does not match a complete pair: add the typed launcher identity/path readback and require both
members against the caller's guarded ExecutablePair before admitting an effect.

Expose pure next_path()->Option<&Path> and matches_next_identity(&FileIdentity)->bool accessors
without exposing service wire fields. Consumers validate the record's account/phase first, then
bind a Restoring/Restored frame to the actual guarded destination and strict receipt full ID (or
the verified package completion/old rollback leaf). None matches no identity; an existing next
binding compares both complete fixed-width volume/file components. Verify: None stays None and
refuses identity matching; the exact next path/identity succeeds, while any high or low identity
bit or different volume refuses even with identical bytes/path. These readbacks add no phase,
effect, callback count or ownership authority and never replace live existing-only guard checks.
Preserve these existing console readbacks, add their typed next-launcher counterparts, and bind
both members to the same verified next ExecutablePair; neither readback alone establishes it.

The persistence plan exposes that maximum record byte count and finite callback ceilings for R
frozen roles: quiesce <=4R+2, restore <=4R+2, remove <=2R+2. These include per-effect intent/result,
forced-stop facts and initial/terminal records; retries/polling do not append unbounded snapshots.

Qualify the accepted pure record ABI before admitting its pending effectful provider. In the
intermediate integration, compile windows_record only under Windows test configuration and omit
its production reexports; normal CLI builds gain no unused provider or synthetic consumer.
Run its strict decode, complete-capacity, exact-origin/full-ID and one-step successor fixtures
through the serial native service unit harness on x64, ARM64 and Windows MSRV. Add an actual
private owned-file binding fixture using retained existing no-follow guards and the complete
volume/file identity, then strictly round-trip a zero-role snapshot for the verified current SID.
Assert the real phase/quiesced readbacks on the existing forward walks; do not suppress unused
code or add throwaway production calls. Verify normal production lint has no record exposure,
the native record filter actually runs every pure fixture and the owned binding distinguishes
equal bytes on a different object. Qualification of these pure facts never claims COM effects,
activation, fresh registration, rollback or live ownership; production wiring awaits those consumers.

### Typed activation facts and complete restoration capacity

The earlier version-one/GUID-only candidate is superseded by the source-held GUI/witness
proposal at bdbd02fc. Freeze the service producer's concrete refinement before any effect source:

- ServiceRestoreRecord version two stores previous and optional next ExecutablePair objects,
  each with exactly console and launcher ExecutableBindings (normalized path plus full volume
  and file ID). Both names are fixed siblings, locron.exe and locron-service-launcher.exe.
  Reject missing/extra/same-object members and old single-image schema; there is no published
  Windows baseline to adopt. The live owner additionally verifies both bytes, PE architecture/
  subsystem, package version and launcher ABI. Pure decoding never supplies those native facts.
  Existing console path/identity readbacks keep their meaning; add typed previous/next launcher
  path and full-ID readbacks. same_original and validate_successor bind the ordered pair as well
  as every existing original role fact, and a once-set next pair/fingerprint remains immutable.
- Each originally enabled role may gain one ActivationFact with exactly context, digest,
  observed_instance, unknown_start, confirmed_instance and lifetime. Context is a fresh canonical
  nonnil UUID, digest is 64 lowercase hex; the existing GUID/lifetime rules stay unchanged.
  Reserve distinct contexts across roles. RunIntent sets only context/digest; UUID observations
  are null and unknown_start is false. Neither raw capability nor a claimed process PID is
  serialized in this record. Disabled roles always retain null and never Run. Earlier ability
  to confirm a different unrelated automatic GUID is removed: confirmation must carry the same
  authenticated operation context/digest. Lost-before-auth intent remains pending, never replayed.
- The digest domain is locron-activation/v1 followed by a NUL and unambiguous length-framed
  operation UUID, context UUID, verified SID, full state volume/file identity, fixed role and
  32 freshly generated OS-random nonce bytes. It binds the caller's operation and root, rather
  than only a task GUID. Raw context:nonce Run data exists only in the live producer/transport;
  no Debug/error/log/journal/fact path may render it. Entropy failure refuses before RunIntent.
- ServicesReadyToActivate retains both verified future images, every existing root and the
  validated registrations after refresh/enable readback. activate_roles consumes that owner,
  the canonical operation UUID and one owned Send + 'static persist callback. Its thirty-second
  phase deadline starts before SID/COM/pipe/native work. Close the phase's sole generic/COM
  session before returning to distribution's registry/PATH/package operations; callbacks only
  serialize and flush the already-owned Rust journal, never recurse into PowerShell/COM.
- Persist intent, invoke SDK Run once, persist its actual nonnil GUID, then authorize the GUI
  child through the protected first-instance pipe. The first actual kernel peer PID/capability
  exchange alone cannot spawn the supervisor. A timely Permit follows durable SDK observation
  and exact owned-task/single-instance readback. Its remaining duration shortens the launcher's
  pre-SID entry deadline (entry plus duration, never receipt time plus a fresh duration).
  Preserve the raw fixed action's v1: data parser and both guarded executable identities.
  Launcher then owns the exact hidden Child; the second kernel peer PID must match that live
  retained child and its newly held supervisor activation/control UUID. No claimed PID or CLI
  scheduler-instance text substitutes. Expiry/unknown native completion admits no newly
  dispatched Run, child, callback or replay, and retains quarantined ownership rather than
  joining it. An already queued native effect can finish late and remains uncertain.
- Expose only service-owned authenticate_supervisor(paths, role, actual_supervisor_lifetime,
  optional untrusted private-stdin Bootstrap, original Instant) returning
  Result<Option<AuthenticatedActivation>, ServiceError>. Absence is unwitnessed startup.
  Bootstrap stays bounded to 1 KiB and is never an authority value. AuthenticatedActivation is
  Clone/Debug/Serialize only, with private fields and no Deserialize/public constructor; only
  the timely live exchange and matching held lease mint Some. Its serialized fields are exactly
  context, digest, scheduler_instance and actual launcher_pid. Runtime stores this immutable
  optional value within its existing finite I/O/lifecycle owner and 16 KiB RuntimeFacts; pure
  protected-fact decoding remains a different readback type and cannot mint authentication.
- Keep S <=4R+2: Restoring entry, four ordered refresh/enable intent/result edges per role and
  terminal Activating with every fact null. A <=4E+2: unchanged Activating admission, at most
  intent/SDK-observation/one-unknown/owned-confirmation per enabled role and terminal Restored.
  Witness frames add no callbacks. Unknown is set at most once; confirmation does not erase it
  or rewrite the observed GUID. Existing intent always selects readback, never another SDK Run.
  Persist only one exact successor or unchanged admission; root/tree/role readiness is still
  independent live proof. Pre-receipt old-byte rollback and post-receipt new-byte forward
  activation remain mutually exclusive; post-receipt failure stays pending Restoring/Activating.
- ServicePersistencePlan's checked max_record_bytes reserves the actual complete version-two
  object, both future path maxima, all pair IDs/fingerprints, forced/progress fields and optional
  activation facts. The fixed maximum fact object is 317 ASCII bytes, versus null's four;
  the RuntimeFacts witness object is 212 bytes, versus null's four. These are schema checks,
  never substitutes for serializing the complete typed worst object across every legal phase.
  Preserve private record 128 KiB/frame 128 KiB limits, and consume the GUI proposal's checked
  eight-leaf 140-frame/18 MiB outer reservation. Full preflight covers every repeated service
  slot and the maximum legal forward/earlier rollback branch before any installed-state effect.

Verify the actual typed v2 maximum/successor matrix, both high/low identity bits and sibling/path
tampering, old-schema refusal, disabled/zero-role cases and every context/digest/GUID immutability
edge. Native qualification must prove Permit-before-spawn (no marker before SDK persistence),
actual two kernel peers/retained child/held control seal and manual valid-GUID refusal, plus lost
pre-auth/no-replay and already-authenticated live recovery. Count actual callbacks and complete
objects through both eight-leaf legal branches; overflow has zero effects. This refinement and
the complete GUI proposal require parent/Project review before producer/model/consumer source;
fresh-root creation/task rollback remains a separate held typed effect plan.

### Shared activation wire and local authentication boundary

Freeze the concrete wire shared by the service producer and internal GUI without importing the
public CLI renderer or service manager into the GUI. Service owns the standalone
service/windows_activation_wire.rs models, strict codecs and endpoint derivation; the GUI includes
that file by explicit path. Runtime owns its client/retained-child composition. Service alone owns
windows_activation.rs, the producer and authenticate_supervisor; runtime stores the resulting opaque
witness. This refines the reviewed complete GUI/version-two plan, not the superseded GUID-only plan.

- Wire Role has exactly daemon/dashboard, with a service-owned conversion to/from Target outside
  the standalone file. UUID values are lowercase canonical nonnil hyphenated strings; digests are
  exactly 64 lowercase hexadecimal characters and PIDs are nonzero u32. Every object requires
  version=1, rejects unknown/duplicate/missing fields and invalid enums, and remains untrusted.
  Protocol version one does not change the unpublished maintenance record's version two.
- A frame is a four-byte little-endian body length followed by strict UTF-8 JSON. The complete
  header plus body is at most 1,024 bytes; lengths zero or above 1,020 refuse before allocation.
  Decode requires exactly one complete frame, without trailing data. Incremental transport reads
  only that header and bounded body. Fixed schema/size errors never render serde errors, input,
  capabilities or fields. The separate read-only GUI probe retains its reviewed four-KiB cap.
- LauncherHello contains version/type, context, role and capability. Permit contains those public
  bindings plus digest, scheduler_instance, producer_pid and remaining_micros, omitting capability.
  Child contains context/role/digest/scheduler_instance and the GUI's actual retained child_pid.
  SupervisorHello contains context/role/capability/digest/scheduler_instance, launcher_pid and
  actual held supervisor_lifetime. Seal contains context/role/digest/scheduler_instance and both
  launcher_pid/supervisor_pid plus supervisor_lifetime. Each type has its own fixed discriminator;
  a valid frame at the wrong step still refuses. Frame PIDs never substitute for kernel queries.
- Bootstrap is a separate bounded private-stdin frame containing context/role/capability, digest,
  scheduler_instance, producer_pid, launcher_pid and remaining_micros. Missing stdin selects the
  ordinary unwitnessed path; malformed/present bootstrap refuses. It provides only expected live
  bindings and a shorter budget. No decoded Bootstrap, Permit, Seal or protected-fact value can
  construct AuthenticatedActivation or establish that the SDK Run occurred.
- Capability wraps exactly 32 bytes, encodes as 64 lowercase hex only in the fixed live Run data
  and private frames, and has a custom redacted Debug with no Display. Derive Debug only through
  that wrapper; parser/serialization/transport errors use fixed stage/category text. The raw Run
  argument and serialized secret buffers are never logged or included in receipts, journals or
  runtime facts. Keep the exact reviewed absent/empty/v1:/literal v1:$(Arg0) unwitnessed parser;
  every other input must be v1:<canonical context>:<64-lowercase-hex capability> or refuse.
- Pure codecs add no native I/O, admission worker or authentication constructor. Qualification
  initially includes the standalone module only in Windows tests; production exposure follows
  its actual GUI/service consumers, without synthetic calls or dead-code suppression. At use,
  every frame/receipt operation has min(original deadline, operation entry+200ms), with pre-poll,
  post-Ready and pre/post-native checks. Timeout of queued I/O remains delivery-uncertain.
- remaining_micros is an integer from 1 through 30,000,000, floor-rounded from the producer's
  existing remaining duration. Sub-microsecond remainder refuses. GUI computes
  min(original_gui_deadline, pre_SID_gui_entry.checked_add(duration)); overflow, an already elapsed
  result or any larger/zero encoding refuses. Never add it to receipt time. Bootstrap likewise
  only shortens the supervisor's already captured entry deadline; the producer's original phase
  deadline remains independent final authority. No frame starts a new thirty-second phase.
- Derive the endpoint only from a retained existing private root's shared full instance digest,
  fixed role and context: locron.activation.v1.<instance>.<role>.<context>. Never place a raw nonce
  in its name. The producer first-instance listener precedes Run, refuses collisions/remotes,
  retains the reviewed SID+SYSTEM descriptor/root/pair guards and admits at most two connected
  peers plus one listener. Clients use the actual named-pipe server_process_id; the producer uses
  each actual client_process_id. Producer PID from Permit/Bootstrap must match that kernel peer.
- Preserve the selected order: durable intent; sole SDK Run; durable actual GUID plus unchanged
  definition/single-instance readback; actual launcher Hello; timely Permit; retained hidden child;
  Child report; actual matching second SupervisorHello and held activation/control proof. Hello
  may already be buffered while the SDK observation is persisted, but no Permit is sent early.
  Send matching Seal separately to the GUI first, require its fixed 0xff receipt after checking its
  retained child has not exited, then send Seal to the supervisor. Its service consumer validates
  the real server/context/capability/own PID/held UUID, writes the final fixed 0xff receipt with
  all expiry gates, and only then privately mints Some. Producer confirms that receipt within
  its own original budget and freshly proves actual role readiness before durable confirmation.
  GUI keeps the child/Job and first connection through sealing. Receipt loss or late completion
  remains pending; no second Run/child or claimed atomic cross-process commit is inferred.
- AuthenticatedActivation stays private-field Clone/Debug/Serialize only, with exactly context,
  digest, scheduler_instance and launcher_pid. A service-owned cfg(test)-only maximal fixture
  factory permits runtime's actual complete RuntimeFacts serialization across None/Some, all
  completed/not-started partitions and bounded escaped messages under 16 KiB. It is unavailable
  in production and raw protected-fact decoding; field-only witness size is not capacity proof.
- The producer directly uses pinned getrandom=0.4.3 fill for the 32-byte nonce, already in the
  resolved UUID graph, through a Windows-only CLI dependency. Its audited Windows backend uses
  ProcessPrng and reports failure; add no custom backend, fallback or unsafe repository code.
  Generate inside the existing finite native owner with original deadline checks before/after;
  a stalled entropy call retains/quarantines that owner and cannot admit late intent or Run.
  Use existing SHA-256 over locron-activation/v1 plus NUL, then seven LE-u32-length-prefixed fields:
  canonical operation, context, verified SID, volume LE64, file ID LE128, role and nonce bytes.

Implementation order and Verify: (1) qualify strict pure round trips, every maximal complete
frame/Bootstrap, duplicate/unknown/version/UTF-8/length/UUID/hex/PID failures and redacted Debug/error
output. Verify Role/Target conversion and all empty/unsubstituted parser controls without effects.
(2) exercise floor/overflow/expired/late-ready budget cases and exact endpoint separation; verify
no receipt-clock extension, cold guard/SID work or frame can mint an opaque witness. (3) qualify
actual GUI and supervisor consumers/complete facts, then producer effects: stall GUID persistence
and Permit before expiry and prove no child marker; compare both real pipe peers with retained
child/control and reject a manual valid-GUID process, wrong nonce/role/root/PID and collision.
Count the unchanged S<=4R+2/A<=4E+2 callbacks and entire typed record/facts maxima; wire exchanges
add no journal callbacks or durable raw capability. Lost/late receipts stay pending with zero
redispatch, original phase budgets and retained uncertain ownership. Fresh-root effects remain held.

Distribution must reserve all repeated service records, frame overhead, file/receipt/inventory
transitions and the worst rollback path against the revised 140 frames/18 MiB before any task,
registry or file mutation. Reserve the actual validated remaining path again at recovery entry.
Failure to prove the complete budget refuses with zero effects; initial callback success alone
is not a reservation. The 256-binding inventory limit is only a ceiling, not a promise that it fits.

An interrupted unconfirmed record can resume disabling/quiescing only after reconstructing all
existing root guards and matching both old executable objects plus unchanged definitions; an
originally disabled role must still be disabled. A confirmed quiescent record reconstructs
existing roots/task guards and requires old definitions disabled/stopped without reopening the
old pair: postreplacement/WinGet Complete may legitimately find its members absent or replaced.
Restoration retains the new executable pair already verified by distribution's protected receipt/
package proof, validates both full objects and all existing registrations, then flushes a typed
Restoring intent with that new pair binding before refreshing any task. Only this phase plus fresh
verified new bytes admits an exact deterministic new definition or its original enabled flag as
idempotent recovery. A merely quiescent record requires original disabled definitions and cannot
adopt an unexplained new action. Never treat saved journal source as a task definition.
Removal likewise records exact per-role delete intent before the effect; only that recorded
intent permits an absent prior task during removal recovery, and no missing task is recreated.
Preserve original enabled flags and refuse a missing/changed root, SID, registration outside
that exact removal intent, or unexpected enable transition before any
write. This permits recovery after a completed per-role refresh without adopting unrelated tasks.

Native Verify: (1) failed initial persistence leaves every task/enabled flag/process unchanged,
and simulated abrupt helper exit after each disable resumes from the frozen original flags.
(2) missing/replaced roots, foreign SID, changed task action and unexpected enabling refuse
before task or state/journal creation. (3) two roots with both enabled and disabled roles quiesce
before the old executable guard releases; restore across a changed versioned path, including an
interrupted one-role refresh, returns every original enabled flag without creating missing roles.
Also verify confirmed recovery with a genuinely absent old executable, new-definition refusal
outside Restoring, failed restore-intent persistence with zero refreshes, 4,096/4,097-unit Unicode
path boundaries, worst future-path serialization and oversized full forward/rollback reservations
with zero disables/registry writes/file replacements. Callback observations must remain within
the published ceilings through graceful, forced and interrupted recovery paths.

The package flow composes these same APIs through the existing installer maintenance modes:
Prepare snapshots/quiesces all bindings for one verified executable pair and journals an operation UUID;
Complete validates the installed package and restores prior registrations; Remove quiesces and
removes only the validated prior registrations. No additional release asset or arbitrary manifest
hook is introduced. Distribution owns the private journal/receipt and package registration proof.

### Unsigned release, installation and update handoff

Add native x64/ARM64 MSVC ZIP builds containing locron.exe, locron-service-launcher.exe, README.md,
LICENSE-MIT and LICENSE-APACHE. Extend exact asset inventory and SHA-256 generation with
version-aware historical inventory compatibility;
keep existing Unix publication/signing inputs authoritative. The canonical release source is
WhiteKiwi/locron over verified HTTPS. Final published bytes, version/architecture and channel
metadata agree; checksums check integrity without independent publisher authentication.

The standalone PowerShell 5.1 installer validates archive source/digest/architecture/version and
safe paths before installing in a private user directory. Retain versioned exact-path receipts,
explicit PATH choice, optional daemon/dashboard registration and precise state-preserving removal.
The Windows updater verifies helper/destination ownership and stages verified bytes, quiesces all
owned mapped executable holders, suppresses restarts, hands off to a second process and confirms
completion before success. Locked/unowned MCP/manual holders are bounded refusals. Replacement
and receipt update retain verified rollback; incomplete registration restoration remains resumable.
No normal reboot-replacement path, optimistic updated:true or silently enabled dashboard.
WinGet uses the same final ZIPs,
InstallerSha256 and explicit package-manager ownership; self-update refuses its binary.

#### Concrete Windows distribution contracts

The Windows feature release inventory starts at v0.10.0. Tags before v0.3.0 retain their eight
Unix payloads plus checksums; v0.3.0 through v0.9.x additionally retain install.sh. Windows tags
add exactly the x86_64-pc-windows-msvc and aarch64-pc-windows-msvc ZIPs, install.ps1 and uninstall.ps1.
The ZIP has one exact version/target directory containing the five exact members above.
Check both final PE architectures, console/GUI subsystems, matching versions/launcher ABI and
absent certificate tables before accepting it.
SHA256SUMS retains bare names and covers all payload archives/packages; installer assets are
separately included in immutable publication digest verification. No historical asset is rewritten.

Windows release builds use Rust 1.94, windows-2025 for x64 and windows-11-arm for ARM64, explicitly
select their native MSVC target and set
RUSTFLAGS=-C target-feature=+crt-static. The explicit --target keeps this flag off host build scripts
and procedural macros. The locked cc 1.4.4, bundled libsqlite3-sys 0.38.2, ring 0.17.14 and
aws-lc-sys 0.44.0 build scripts propagate that choice to their C/C++ compilation. Check both normal
and delayed PE imports against a finite Windows system-DLL allowlist; reject Visual C++
redistributable DLLs, debug runtimes and other application DLLs in both images. Run both packaged
images' bounded version/identity probes with only Windows system directories in their child PATH.
These gates verify the build intent and direct dependencies; clean Windows 11 acceptance remains required to prove
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
installer accepts -Version, -InstallDirectory, -NoService, -Dashboard and -AddToPath. Fresh install
starts the daemon by default and enables the dashboard only when explicitly requested. It changes
only the current user's persistent PATH when -AddToPath is specified. Reinstallation preserves
every existing role's enabled state. After both new binaries and the receipt are fully durable and verified,
a registration failure leaves pending Restoring with a typed bounded cause and correct new bytes;
it is neither completed nor permission to roll back a mapped image behind reactivated roles.
Recovery resumes guarded idempotent restoration. Any earlier rollback first quiesces all verified
new roles before exact deletion/restoration of the old bytes; reserve that quiesce and old-role
restore path as well. Private installation directories and a versioned JSON receipt bind the
current SID, the complete canonical console/launcher pair, channel, target, version,
canonical archive URL, archive SHA-256 and both executable SHA-256 values. Existing unowned/
package-manager destinations are refused. Uninstall requires this receipt, quiesces only exact owned registrations,
removes only receipt-listed unchanged files and an installer-inserted PATH entry, and retains state.
Standalone and WinGet locations coexist; neither installer adopts the other's files.

Provide Windows-only native locron install and locron uninstall commands for the default
Restricted script-policy case. Native install accepts --version, --install-directory,
--no-service, --dashboard and --add-to-path with the same standalone defaults. --no-service
disables both roles on a fresh installation and conflicts with --dashboard before any
state/download/task effects. With an existing owned receipt, preserve every prior enabled state
and reject --no-service or --dashboard before installation effects, directing the operator to
explicit service/dashboard configuration commands. Existing-install defaults never enable a
previously disabled role.
Native uninstall accepts --install-directory, defaults to the running receipt-owned directory
when present and otherwise the ordinary standalone directory, and needs no network. Native
locron install --operation UUID is recovery only and conflicts with all new-install options.
Dispatch these commands before ordinary StatePaths discovery; their shared operation engine
opens state only after ownership/source validation and only for requested role registration.
WindowsInstallOptions carries state_root: Option<PathBuf>. For a new installation the main
dispatcher forwards the parsed global --state-dir or LOCRON_STATE_DIR override as explicit
metadata, resolving it to an absolute local path without creating state. Store that selection
in the protected original request before handoff. Fresh registration uses that frozen selection
or the ordinary default; an existing receipt preserves its already inventoried registrations
across roots rather than creating a new registration because an override was supplied.
UUID recovery rejects a fresh explicit --state-dir option and ignores ambient LOCRON_STATE_DIR;
the dispatcher retains clap's ValueSource to distinguish them before StatePaths discovery.
Its follow-up request contains no fresh state_root; the engine reads the original protected
request's selection instead. Recovery never rediscovers or substitutes the current shell's root.
The native frontend downloads and verifies canonical release bytes, writes the same protected
request and runs the same independently verified helper as the script frontend. It never loads
downloaded/retained .ps1 code or uses Invoke-Expression; fixed compiled stock OS adapters remain
the only PowerShell procedures. Unix CLI/help stays unchanged and the public asset count stays 14.
Verified downloads, the private copied helper and protected temporary bootstrap metadata may exist
to execute this frontend. They do not authorize effects on the installation or state. The helper
revalidates these bytes/source and current guards, then performs the complete typed preflight
before any installed-target, state, task, PATH, transaction-journal or backup mutation. Fresh role
setup uses an explicit verified destination executable and a typed registration/root rollback
record; it never derives the registration action from the copied helper's current_exe.
The optional script frontend must describe its actual execution-policy prerequisite: Restricted
blocks .ps1, RemoteSigned can require operator-selected unblocking of that reviewed asset, and
AllSigned or organizational policy can refuse it. Do not add automatic policy changes, a process
bypass flag, script-content command injection or protection disabling to either frontend.
If any native caller maps the installation being changed, use the same truthful pending
handoff as self-update: acceptance reports pending=true, updated=false, operation_id and
status_file, and never claims completed removal or prepared maintenance. Native operation output
also carries phase and prepared so status cannot confuse file installation with task preparation.
An independently extracted native executable can wait for confirmed work against a different
destination; its frontend still handles pending when exact file identity shows the same mapped
object. Reading an owned operation's status reports its phase without mutating state.

Select the Windows-only sibling receipt .locron-install-receipt-v2 and strict
locron.install/windows-v2 JSON schema. Its seven listed payloads are locron.exe,
locron-service-launcher.exe, README.md, LICENSE-MIT, LICENSE-APACHE, uninstall.ps1 and
.locron-installer.ps1 (the canonical install.ps1 release asset, retained to share the verified
bootstrap with removal). The receipt itself is the eighth managed leaf. Every listed file has
a SHA-256; the receipt binds its canonical directory, ordered console/launcher path-and-digest
objects, SID and standalone channel. These persistent receipt objects contain no native file
IDs. Retained live pair guards and the write-ahead journal independently prove both full native
identities; raw receipt rollback preserves the original bytes. Reject the unpublished Windows
v1 schema and filename as prerelease-only rather than treating them as pair ownership. Never
grant ownership through filename alone. Uninstall may retain a modified listed file with an
explicit warning, and removes the directory only when it is empty. For an opt-in user PATH
insertion, read HKCU\\Environment's
raw PATH without expanding variables and record both complete prior/resulting values and their
REG_SZ or REG_EXPAND_SZ kinds; a missing prior value remains distinguishable from an empty one.
The pure insertion planner preserves a present value's kind and uses REG_SZ only for a missing
value. Append the normalized literal installation directory once, preserving every existing
field byte-for-byte; preserve an existing trailing separator instead of adding another one.
Compare only validated absolute literal entries using the receipt's conservative normalized
spelling comparison. Do not expand environment references, resolve aliases or rewrite quoted,
relative or malformed fields. A pre-existing matching literal produces no insertion or new
receipt ownership; preserve any previously verified receipt-owned conditional rollback record.
Percent characters in the new directory are accepted in REG_SZ but refuse insertion into
REG_EXPAND_SZ, where a later environment read could interpret them as references. A selected
semicolon-containing directory also refuses opt-in insertion. These checks and the complete
bounded old/new raw-value-plus-kind payload occur before effects. Revalidate the exact current
raw value and kind immediately before a write; an intervening edit refuses that write.
Restore that prior value and kind only while both current raw value and kind equal the recorded
result; retain a subsequently edited PATH with a warning instead of deleting a potential user-owned
entry. Use stock RegistryKey operations rather than the
.NET user-environment getter/setter, which expands values and loses the original registry kind.
The installer does not synthesize an unaudited native broadcast adapter: it reports that persistent
PATH changes are available after the next sign-in, and does not claim to change running processes.
This receipt is Windows-only; the existing Unix receipt bytes remain
unchanged. Operation status uses locron.windows-status/v1 with operation_id, SID, canonical
executable, phase, current/new version, updated, prepared and warnings; the status file alone
cannot authorize changes, and is read only after validating its protected operation request.
Qualify the paired receipt and five-member ZIP as separate Windows-test-only pure modules before
connecting the effectful installer, updater or service provider. The existing six-payload launch
qualification and prerelease Windows v1 fixtures stay frozen as historical test evidence; their
presence does not provide a production v1 adoption or compatibility route. Production consumers
must use only the strict paired schema and five-member verifier, rejecting v1 and incomplete
pairs. Share the already-qualified path, hash, version and ZIP-catalog parsing primitives where
possible rather than weakening them or duplicating an effect authority. Do not manufacture a
paired ServiceSnapshot or wire public dispatch to make the pure qualification compile. The next
native gates exercise the v2 ordered paths/digests, exact seven-file map, v1/missing/extra/swapped
rejection and both PE subsystem/architecture/import checks without task, filesystem, PATH or
journal effects; full paired ownership and activation remain separate provider-dependent gates.

The strict receipt fields are schema, sid, channel, directory, executables (a two-member ordered
console/launcher list, each with path and sha256), target, version, launcher_abi (native-gui-v1),
archive_url, archive_sha256, files (the seven exact bare names mapped to hashes), and
user_path (null or before/after values plus before_kind/after_kind, where a missing before value/kind
is null and non-null kinds are String or ExpandString). Remove the ambiguous v1 top-level
executable/binary_sha256 duplicates. Crossvalidate both path/digest members against the exact
file map, canonical directory, shared target/version and native-gui-v1 ABI before accepting
the receipt. Status uses schema, operation_id, sid,
executable, phase, current_version, new_version, updated, prepared and warnings. Unknown fields or filename inventories
are refused. The uninstall.ps1 asset accepts -InstallDirectory; an installed copy defaults to its
own receipt-bearing directory and a downloaded copy to the ordinary standalone directory.
Validate the receipt and listed retained-installer hash before sharing that bootstrap, retain
the helper's verified read handle across launch/wait, and require no network for owned removal.
Malformed, missing or modified executable/bootstrap ownership evidence is a refusal.

The shared PowerShell bootstrap path converter must inspect raw components before calling
Framework GetFullPath. Preserve the absolute local-drive, control-character and alternate-stream
prechecks. For inspection only, translate slash to backslash and split the original suffix into
components; reject dot/parent segments, terminal dots/spaces and the existing reserved-device
forms before normalization can erase them. Then call GetFullPath and retain its existing final
component check and drive-root/trailing-separator output rules. Ordinary internal dots/spaces,
leading-dot names and Unicode remain valid text; normalized text never grants object ownership.
This corrects the existing ambiguous-input refusal, without changing ancestry, ACL, retained
handle, package or lifecycle authority.

Verify: retain all five original bootstrap refusal inputs/assertions with case-labelled fixture
diagnostics, add raw terminal/intermediate dot/space and dot/parent-segment cases with both
separator spellings and reserved-device extensions, and accept ordinary names plus drive roots.
Render uninstall.ps1 from install.ps1 through scripts/render-windows-uninstaller.py and require
exact --check parity. Use only local stock 5.1 AST/static Python checks; the changed real bootstrap
must run successfully on both native hosted architectures before qualification is recorded.

Keep the release inventory unchanged by placing WinGet maintenance in install.ps1:
-Maintenance Prepare|Complete|Remove with -Executable and -Operation selects that explicit flow.
Prepare copies the verified existing package console executable as the hidden helper into the
protected
LocalAppData\\locron-distribution\\operations\\UUID directory, outside the package installation.
It persists SID, package registration/source, both installed executable paths/digests/full native
bindings and all task definitions with their prior enabled/running states before suppressing
activation and confirming graceful exit.
Prepare returns operation_id, status_file and prepared=true only after the journal is durable and
quiescence is confirmed. Those owned tasks remain disabled until a successful Complete.
Complete validates this journal and the newly selected package registration/executable pair,
refreshes the stored paths and restores the recorded enabled states. Its current SID, package/source IDs,
canonical registered location, target, version, launcher ABI and both canonical-release binary
hashes must agree. Require actual package-index ownership of the exact sibling launcher while
only the console receives a WinGet alias; arbitrary or stale executable paths cannot resume tasks.
Before implementing the paired package-index verifier, qualify the selected client's actual
version-root layout. The pinned nonrecursive archive flow indexes our extracted root as a
Directory, not each executable as a File/hash row. Keep a per-executable-row gate refusing this
layout until the completed plan selects and verifies a directory binding plus canonical pair
proof, or a revised Windows ZIP/manifest layout. Registry metadata and a copied index JSON
object cannot replace retained existing-only index and pair guards. The source-feasible
rusqlite bounded-slice/in-memory reader is a candidate; freeze its dependency features, schema,
finite bounds, sidecar refusal and original-deadline ownership before source. First qualify
missing/stale/wrong-type/hash/index/root cases and actual native client install, upgrade and
removal; this passive reader supplies no service or package mutation authority.
The proposed next selection keeps the version-root five-member ZIP and console-only alias:
bind actual indexed-directory ownership to independent complete canonical-release byte proof.
For schema 1.0 require exactly the expected Directory and console Symlink rows, with empty
Directory hash/target and no GUI alias. Bind the index filename to the exact selected ARP product
code and registered location, never to a directory-name guess. Require current-SID ownership and
the existing source policy on its retained regular leaf/ancestry, full index identity and digest,
and all five guarded regular release leaves. Every leaf hash must match the verified canonical
ZIP; both actual executable identities, PE subsystems, native version and launcher ABI must agree.

Keep this new qualification separate from the historical single-image test route. Proposed
private interfaces are `verify_index_snapshot(bytes, registration, root, original_deadline)
-> IndexFacts` for pure bounded metadata and
`verify_package_pair_until(console, original_deadline) -> IndexedPair` for
the live read-only composition. IndexedPair is move-only and retains the real registration,
index guard/digest/full identity, guarded root ancestry, five source guards and canonical archive
proof; metadata or serde cannot construct it. It authorizes no lifecycle effect. Guard/SID/index
reads and real pair probes share the original admitted deadline and retained owner; uncertainty
retains resources and refuses a late proof. The pure reader receives that same absolute Instant,
checks it before deserialization and each fixed query, and captures it in the progress hook;
neither interface creates a fresh deadline.

Use Windows-only features serialize/limits/hooks on the already locked rusqlite 0.40.2. Read a
complete existing index of at most 4 MiB, then deserialize only that bounded slice into a read-only
in-memory database. Refuse WAL header versions and all journal/WAL/SHM sidecars without repair.
Limit metadata to eight rows, require exactly two portable rows, cap each SQL/value at 16 KiB and
columns at eight, forbid attached databases, and stop at the original deadline or the 1,024th
progress callback with the configured 1,000-instruction interval through the safe hook. This is
a finite callback/work check, not an exact instruction or wall-clock guarantee. Fixed schema/row
queries require ordinary tables and exact storage types; use memory-only temp storage, enable
defensive mode and disable trusted schema. SQLITE_LIMIT_LENGTH also applies to encoded rows:
select 65,664 bytes (`4 * 16,384 + 128`) for that row limit, while independently checking each
returned TEXT/BLOB value at the unchanged 16,384-byte cap before copying. Four maximum portable
values need at most 45 bytes of conservative record-header varints; the five-field fixed schema
query's four texts plus integer/header need 62 extra bytes. The seven-field column query's
three texts plus four integers/header need at most `3 * 16,384 + 104`, also below that ceiling.
Unknown schemas and arbitrary eight-large-value rows still refuse; the eight-column cap does
not authorize such payloads. Verify a valid combined two-path row above 16 KiB total and negative
single-value/unsupported-shape cases without changing snapshot/work/deadline limits or source
ownership. Count every new typed index/path/identity field in
actual outer-record preflight before later effects. Native tests
must preserve source/ACL bytes, refuse wrong/extra/missing/oversized/stale facts, and establish
real selected-client directory/pair ownership across install, upgrade and removal. Public
consumers and maintenance effects wait for the completed plan and relevant Project receipts.
On an interrupted/failed Complete, recover the journal and restore prior state only against a
still-valid recorded pair, or retain
disabled registrations with an explicit recovery error until a valid package is selected. Never
enable a mismatched pair. Remove quiesces and removes only the recorded exact pair-bound
registrations before the operator runs winget uninstall. The helper request schema is
locron.windows-operation/v1; installer, updater, uninstaller and maintenance share this internal
entrypoint and serializable validated lifecycle record. Missing, foreign or interrupted records
are explicit refusal/recovery cases. No new release asset or arbitrary manifest hook is introduced.
Validate each exact normalized package executable path and the registered location at no more
than 4,096 UTF-16 code units each, counting the supported verbatim transport prefix when present. Apply the
bound after local-drive normalization and again to the live canonical guarded representation;
stripping a prefix for path comparison does not remove it from this capacity count. A request
spelling cannot substitute for the guarded identity.
Prepare reserves each future Complete path at 12,288 encoded UTF-8/JSON bytes plus two string
quotes. Allowed BMP characters require at most three bytes per UTF-16 unit; supplementary pairs
require four bytes per two units and escaped backslashes require two. Controls and quotes are
already rejected. Count each repetition in every full lifecycle/journal record and the maximum
finite callback/rollback count; do not assume the later package path is as short as the old one.
Complete refuses an over-limit current registration before restore effects, preserving disabled
tasks and the protected journal. Runtime/state paths retain their existing policy.
Expose the same maintenance engine without unsigned script loading through Windows-only
locron maintenance prepare|complete|remove|status. Prepare/Remove require --executable with an exact
registered WinGet binary and create a fresh operation. Complete requires --operation UUID and
--executable with the newly active registered package binary; Status requires only --operation
UUID. Dispatch before default state discovery. WinGet install owns only its package/alias and
does not enable the daemon or dashboard: users opt in through the existing service install and
dashboard enable commands of that package executable. Standalone locron install and self-update
refuse this package location. Before winget upgrade/uninstall, maintenance must report confirmed
prepared/removed, not merely pending acceptance. After upgrade, Complete restores only the
recorded prior enabled states against the verified new package path. Ordinary native status
uses the strict protected request/status protocol for both standalone and package operations.
WinGet Prepare/Remove confirms mapped-holder exit through a separate existing-only source-policy
exclusive read/write/DELETE/share0 gate. Its owner must be the current SID, trusted SYSTEM/Admin
permissions and other read/execute may remain, and foreign mutation is refused under retained
existing ancestry guards. It never repairs package ownership/DACLs or writes/deletes package
bytes. The strict protected private gate remains mandatory for standalone replacement; the
ordinary read-owned source guard alone cannot prove a mapped executable has exited.
Automatic WinGet upgrade tools and upgrade --all cannot run the required task procedure; enabled
role installations require explicit maintenance and exclusion from those tools. Prefer standalone
installation for integrated service updates. RequireExplicitUpgrade is documented for self-updating
packages, whereas this package refuses self-update; do not assert that field merely to hide this
lifecycle limitation. Review this restriction before channel promotion rather than claiming all
unattended package paths are safe. The current manifest format remains portable.
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

The helper binds its exact console path/hash, SID, destination, old receipt and staged verified
executable pair in a durable operation request. It serializes updates, snapshots all exact
pair-bound owned tasks, disables activation, requests lifetime-bound shutdown and waits for owned-role exit plus
the original caller. Bounded exclusive read/write/DELETE opens of both old executable members,
with share_mode(0), are the final mapped-holder gates; unowned MCP/manual holders refuse
replacement rather than being killed. The same live leaf and ancestor guards validate the receipt,
current SID, private descriptors, both full volume/file identities and old hashes. Refuse read-only
or multiply linked replacement leaves. No write to either old executable is permitted.

Use Windows-only fs_at =0.2.1 (Apache-2.0, MSRV 1.71) for its safe consuming
fs_at::os::windows::FileExt::delete_by_handle(File) interface. This selects exact-handle deletion
instead of a path-based backup rename. First copy all old receipt-listed payloads and the old
receipt to the private operation directory, flush them, verify their hashes through retained
guards, and flush a write-ahead record containing their original identities, the lifecycle restore
record and delete intent. Only then consume the exact gated old File while retaining its parent
DirectoryGuard. Any deletion error is ambiguous: the dependency's read-only fallback can mark a
file deleted before returning an error. Preserve the backup and journal and inspect the guarded
leaf before deciding whether rollback is possible; never assume an error leaves the old path intact.

Create the new leaf only with CreateNew, read/write/DELETE access and share_mode(0), inherited
private creation security and immediate handle-bound descriptor verification. An existing entry
is a refusal, never permission to truncate, clobber or follow it. Record the new full file identity
durably before writing; write the complete verified bytes, flush and verify them and the new receipt
while the leaf gate still blocks executable launch. Apply the same identity-gated replacement
discipline to owned companion files. Rollback may delete only a leaf whose recorded identity and
ownership still agree, then use CreateNew for a verified backup. Unexpected entries remain intact
and keep task activation disabled. In particular, a crash between creation and durable identity
recording leaves an unrecognized leaf that recovery explicitly refuses; it cannot be safely
adopted by filename, empty length or matching bytes. A retained verified helper and backups remain
available even when the installed executable is absent or incomplete.

Recover standalone operations through locron install --operation UUID or, where script policy
permits it, install.ps1 -Operation UUID without -Maintenance; this
uses the protected original request, verified retained helper and durable journal, and reports a
confirmed rollback or completion, or an actionable refusal. Recovery never treats a partial status
file as authorization. Restore exact prior enabled/disabled registrations only after a verified
working binary and receipt exist, recording registration warnings separately from confirmed binary
replacement. Interrupted operations must be recovered or explicitly refused before a later update;
neither a stale request nor a task-state transition implies completion. No reboot replacement.

The write-ahead journal is one retained private read/write/DELETE/share0 journal.bin handle,
not a sequence of path-based replacements. Append frames consisting of a little-endian length,
the previous frame's SHA-256 (zero for the first), typed UTF-8 JSON and the current frame's SHA-256
over length, previous digest and JSON. Keep a complete frame at 128 KiB including its 68-byte
overhead, and select the reviewed aggregate ceiling of 18 MiB and 140 frames; validate the whole
bounded chain before recovery. Each locron.windows-journal/v1
frame repeats the operation UUID, SID, original request digest, monotonic sequence, the exact
accepted/quiescing/replacing/restoring/completed/prepared/removed/failed/rolled_back phase enum,
verified backup inventory and full original/created volume-and-file identities. Use the actual
service::ServiceRestoreRecord type for all-task lifecycle state, including its strict version,
original task fingerprints and confirmed quiescence; arbitrary JSON and saved task source cannot
authorize task effects. Preflight the complete serialized original lifecycle snapshot and reserve
the worst-case frame count and bytes for all quiesce callbacks, forward file/receipt/registration
transitions and rollback before any installed-target, state, task, PATH, transaction-journal or
backup mutation. Protected verified bootstrap metadata is the exception described above.
Preflight each planned fixed OS-adapter request/response against its 64 KiB input/128 KiB output
bounds as well, so a later oversized PATH or path payload cannot discover its limit after effects.
Count encoded frame overhead and repeated snapshot/inventory fields, using bounded maximum
representations for not-yet-created
identities and other future fields. A reservation must cover the entire operation, not only its
next append or successful path. An oversized snapshot or unprovable remaining budget refuses with
zero effects; do not raise those reviewed limits, truncate the snapshot or begin disabling tasks to discover its
size. The service adapter's 256-binding limit is a ceiling, not a promise that every such inventory
fits this distribution journal. Revalidate and reserve against the actual validated record at
each resumed-operation entry before another effect. The first flushed frame contains the complete
original lifecycle snapshot and durable verified backups before any disable/delete; later frames
record each delete intent and every newly created identity before bytes are written.
Embed every ServiceRestoreRecord as a serde JSON object in the concrete outer journal record,
never as an escaped JSON string. Its pure persistence_plan provides the maximum complete object
bytes and finite callback counts. Serialize the complete bounded worst-case outer representation
with its actual typed slots, then add checked growth for each repeated immutable original and
mutable current service slot: maximum object bytes minus that slot's serialized object bytes.
Every optional outer field has a fixed serialized key; a currently absent service slot is null
(four bytes), so reserve the maximum future object minus those four bytes. Include any future
outer value/collection growth and frame overhead separately; absence must not omit future field
names, punctuation or bindings from the reservation. A missing validated provider maximum,
arithmetic overflow, a baseline larger than its promised maximum, or an uncounted repeated slot
refuses preflight. The codec accepts only the resulting bounded byte-growth reservation and still
rejects any actual append beyond it. This is size accounting for the concrete typed record,
not permission to synthesize a replacement service record or replay arbitrary JSON.
The concrete existing-installation frame keeps immutable original and current typed service slots,
the original receipt and any fully verified new receipt, and a fixed-key optional package-completion
binding. Keep every absent slot as null. At most eight leaf entries identify the seven exact payload
names and the receipt; each freezes its original and durable backup full identities, byte count
and SHA-256, and records delete intent, confirmed absence, newly created identity, verified writes
and rollback identity through a strict enum. Derive backup and destination paths from those logical
names under retained guards; never save arbitrary paths or task source. Changed or unverifiable
uninstall companions are bounded retention facts, not authorized leaf entries.
Pure journal validation binds every frame to the protected original request, immutable inventory
and snapshot, checks consecutive sequence and permitted phase/leaf transitions, and rejects unknown
names or changed original facts. It cannot establish live ownership or completion.

Derive the initial existing-operation envelope through a read-only factory from retained Bootstrap,
Standalone/Removal and canonical Payloads proofs plus the real service snapshot. Re-read complete
identities and lengths from those exact retained file handles, require their normalized paths to
match the logical inventory, and bind the raw protected original request digest rather than a
reserialized request. Preserve every proof guard through backup and confirmed quiescence. The
factory neither creates a journal nor proves the helper's mapped image or a complete activation
budget. Recovery validates the original protected context and existing journal; it never rebuilds
an accepted snapshot from a newly replaced receipt or assumes the original executable still exists.
Verify that a real retained ownership proof produces the expected exact logical inventory; mismatched
paths/full identities/lengths or a recovery request refuse without effects. Different formatting of
the protected original JSON must bind its actual bytes, and a replaced receipt must not reconstruct
the old accepted snapshot. The caller retains the real service snapshot guards; the factory's
serialized output alone cannot authorize quiescence or replacement.

Mapped-helper qualification is a separate read-only launch gate. A private LaunchLease retains the
positively selected helper's complete byte digest, full volume/file identity, immutable leaf and
all ancestor guards before launching its absolute .exe path through native Command. Its selection
comes from retained standalone/removal ownership or canonical payload proof, never only from the
request's helper_sha256. The lease also retains the protected original request and its raw digest.
Keep these guards through qualification and acquire overlapping Bootstrap guards in the child;
there must be no interval in which either the leaf or an ancestor can be renamed, deleted or
opened for conflicting writes. A newly copied helper must match its recorded created identity
after reopening the immutable read gate. Matching bytes at a substituted object do not suffice.

Use the actual returned std::process::Child, not a caller-supplied PID, as the launch owner. After
child readiness, query Process.GetProcessById(child.id()).MainModule.FileName through a fixed
stock .NET script with run_script_json_bounded and the qualification phase's remaining budget.
Require the original Child.try_wait to report live both before and after this readback, validate
the normalized module path and reopen it under the still-held ancestry guards to compare its full
identity with the selected helper. A missing/truncated/mismatched name, process exit, unsupported
cross-bitness query or adapter error refuses qualification. The name query supplies a consistency
check; the pre-launch guard interval is what connects the mapped image to the selected file.
Neither current_exe, a reopened path/hash, an arbitrary process lookup nor Bootstrap alone proves
that interval. No new raw FFI or runtime source compilation is required.
The fixed script must read the Core adapter's parsed `$request.pid`, not its test-only raw
stdin String. Require the supplied PID to be an Int32/Int64 in 1..Int32.MaxValue before conversion
or process lookup; missing/null, text, Boolean, fractional, nonpositive and out-of-range values
refuse. Invoke get_MainModule/get_FileName explicitly to preserve getter exceptions rather than
letting PowerShell property access turn them into a misleading missing-module result. The parent
still compares the returned PID with its retained Child and retains every live/guard/deadline
check; the query occurs after actual Ready and receives only that phase's remaining budget.
Verify this fixed-adapter boundary with malformed PID inputs and the unchanged real copied-child
positive/exit/full-identity/retention cases on both native architectures. Source review identifies
the input mismatch behind the 7871c983 run's 92/93 result; successful mapped-image qualification
requires a fresh exact-revision native result. No query retry or fallback is part of this
correction.

Freeze a private locron.windows-helper-launch/v1 Challenge -> Ready -> Permit -> Qualified
exchange on two private one-way byte pipes. Each strict, deny-unknown frame is at
most 4 KiB, with four frames/16 KiB total, and binds the operation UUID, fresh session UUID, raw
original/current-request SHA-256 and helper's full identity/digest; Ready additionally binds the
actual child PID and a child-generated fresh UUID echoed by Permit/Qualified, preventing saved
frames from satisfying a new exchange. Encode full identities as fixed 16/32-character hex strings,
not lossy JSON numbers. No paths, task source or effect instructions come from this channel.
The parent constructs nonserializable
QualifiedLaunch only after real launch/readback/overlap checks; the child constructs its private
QualifiedBootstrap only through that exchange while retaining Bootstrap. Request/status JSON,
saved Qualified frames and a command-line flag cannot reconstruct either live proof. Apply one
30-second qualification deadline including cold adapter admission/readback and channel I/O,
followed only by the existing bounded owned-child cleanup. Refusal/uncertain cleanup retains the
lease in its owner until exit is confirmed; it cannot enter the operation engine or release an
unknown child's protection to perform effects. This follows the existing user-account security
boundary and does not claim protection against arbitrary code or debugger control as that user.
The narrow private ABI is LaunchLease::spawn -> PendingLaunch, PendingLaunch::qualify ->
QualifiedLaunch, begin_child(selector) -> ChildExchange and
qualify_child(Bootstrap, ChildExchange) -> ChildQualification -> QualifiedBootstrap; the latter
owns its Bootstrap. ChildQualification is retained owner state, not a live token: its asynchronous
finish borrows that state, then checked success permits the private QualifiedBootstrap constructor.
These tokens have private constructors and no Serialize/Deserialize/Clone implementation.
ChildExchange owns the actual receive endpoint, validated Challenge and a clock born before its
first selector/pipe/SID/guard work. After receiving Challenge, shorten that clock by the parent's
remaining budget; never reset it. Pass the shortened deadline into Bootstrap before acquiring
its overlapping guards. Only then can qualify_child consume both capabilities and finish Ready,
Permit/EOF and Qualified/flush. This additional argument carries live transport/deadline ownership,
not saved-frame authority; Bootstrap or a deserialized Challenge alone cannot construct it.
The parent tokens own the sole retained owner connection; that owner retains the original native
Child and every lease/guard/close task through qualification and confirmed cleanup. Token Drop
requests cleanup without an indefinite caller join. No PID/atomic flag replaces actual Child
liveness brackets, and an owner reply received after the original deadline cannot create a token.
The first live fixture consumer accepts existing standalone/removal source proof only. Canonical
new-payload, package and recovery integration remain separate consumers; recovery additionally
needs its validated original journal source and cannot rebuild authority from a replaced receipt.
The initial qualification fixture consumer stops there; none of these functions takes, creates
or casts a ServiceSnapshot, and none can dispatch the future operation engine.
Place parent and child retained ownership outside the unwind/wait boundary before polling any
fallible qualification future. A panic or timed-out close cannot drop its guard state or detach
an unknown native flush. Existing standalone/removal inventory verification receives the same
original deadline, initializes only the real bounded SID cache and checks that clock around each
retained receipt/payload read and inventory step. Expired verification refuses before spawning.
Its existing ordinary verification entrypoints keep their behavior; the launch gate never calls
an entrypoint that creates a nested phase budget. Verify expired inventory admission has no native
child or changed bytes and that both owner states retain protection during uncertainty.
The original deadline is born before launch-context guard/SID work, not after lease construction.
Propagate its remaining budget into Bootstrap's cold SID initialization and every adapter call;
do not warm the cache first or use a nested default 30-second initializer. Challenge carries only
the bounded remaining milliseconds for the child's local budget. The parent's original absolute
deadline remains authoritative across transport and process startup, so transit or a later child
timer cannot extend it. Check expiration before and after each synchronous native guard query.
Run potentially blocking native guard/spawn/channel operations in a finite retained owner, never
an indefinitely joined caller thread. Retain child, lease and outstanding I/O in that owner after
uncertain cleanup; permit at most one such launch owner per process and refuse another admission
while it remains live. A timed-out callback cannot later publish a qualified token or enter effects.
Frame wire size includes a four-byte little-endian JSON length prefix; each endpoint receives its
exact two phases, reject over-limit lengths/trailing JSON, and never search past noise for a frame.
After its second frame each receiver requires actual pipe EOF within the same deadline. Any
additional byte refuses immediately; an idle open writer, cancellation or read error is not EOF.

The parent creates and owns the send-only Challenge/Permit server before spawning the original
Child. After reading Challenge the child creates and owns the send-only Ready/Qualified server
within the propagated remaining budget. Both names are fixed local pipe names derived from the
verified SID digest and fresh session UUID, with distinct direction suffixes; selectors carry
only those exact names, not arbitrary network paths. Each server uses interprocess =2.4.4 with
the tokio feature, SecurityDescriptor::deserialize and PipeListenerOptions::create_tokio_send_only
with current-SID owner and an explicit protected SID+SYSTEM DACL, accept_remote(false), inheritable(false)
and instance_limit(2). Its initial creation uses FIRST_PIPE_INSTANCE; a collision refuses without
adopting an existing endpoint. Accept once and immediately drop the listener's unused replacement
instance; retain the connected stream. Limit 1 cannot be used because accept creates that replacement.
Dropping that Tokio listener is not synchronous proof that its native replacement handle has
closed: pinned mio retains native handles through pending IOCP completion records. Single accept
is enforced by consuming the listener, with no second accepted stream or qualification path;
an incidental later client open supplies no authority. Keep the runtime in the same finite owner
through cleanup, retain Child/lease/guards until runtime disposal is confirmed, and release its
admission only afterward. Any uncertain disposal remains on that owner with bounded caller refusal,
not an untracked runtime/handle leak or a second admission.

Both clients use Tokio ClientOptions read(true), write(false), explicit SECURITY_IDENTIFICATION
SQOS and byte mode; its safe open registers one overlapped I/O handle. For the client's server-PID
query only, safely clone its BorrowedHandle into OwnedHandle and wrap that clone in the synchronous
interprocess PipeStream<Bytes,Bytes>. After the query, consume this unsplit metadata-only wrapper
with safe evade_limbo even if the query failed; it never reads, writes, registers I/O or enters
default-drop limbo. The send-mode parameter exposes this cleanup method without granting protocol
send authority; the actual Tokio client stays receive-only. The constructor's optional native
ReOpenFile may request read/write access and fall back to the original cloned handle on failure.
Require actual client direction and peer PID in either case; never adopt metadata as proof.
This eliminates an untracked impossible-extraction fallback, not a new I/O endpoint. Verify both
native directions and query-error disposal within the original owner/deadline. Do not convert
the I/O client into an interprocess receive-only stream,
whose externally supplied handle starts with unknown flush state. These are safe owned-handle
operations with no workspace raw FFI or inherited handles. Bounded connection attempts, safe
client_process_id/server_process_id queries and all cold setup consume the original
deadline. Bind the parent's outgoing accepted client and incoming server to actual Child.id(),
bracketing peer queries with original Child live checks. The child binds both opposite peers to
the actual parent PID carried by Challenge and read from the connected endpoint; a claimed PID
alone never authorizes qualification. Query failure or wrong direction/peer refuses.

Each sender explicitly flushes its own server endpoint after its second frame, waiting for those
bytes to be read within the original deadline. Do not use interprocess's flush result as that
confirmation: its pinned Windows wrapper changes disconnected-pipe errors into success. Safely
clone the sender's BorrowedHandle into one temporary OwnedHandle, convert that handle into
std::fs::File and call File::sync_all in a retained blocking worker. Rust 1.94's Windows fsync
preserves FlushFileBuffers failure. The temporary clone performs no reads/writes or I/O registration;
retain its actual worker handle until completion, close the clone, then consume the original
sender with evade_limbo. Keep no split halves or other endpoint clones. A successful flush is
necessary but cannot replace the receiver's exact phase and terminal-EOF checks. The child reads
Permit followed by terminal EOF before
sending Qualified; the parent reads Qualified followed by terminal EOF while the original Child
and overlapping helper guards remain live. Do not substitute a quiet period, Peek, discarded noise
or process exit for closure. Interprocess's default send-stream drop can retain a flushing limbo
worker, so it is not the terminal operation. A timed-out raw flush remains owned/quarantined with
its outstanding worker, actual Child, lease and guards; neither evading limbo nor dropping a Tokio
join handle cancels that native operation. The finite owner keeps the admission permit and cannot
publish a qualified token after expiry; the caller never joins an uncertain worker indefinitely.

Use these same owned channels for the copied libtest fixture and null its harness stdout/stderr;
there is no alternate stderr protocol or fixture channel selector. No qualification bytes come
from global stdio, whose safely borrowed handles cannot be independently closed while the child
remains live. This uses the existing copied test executable and adds no helper binary, product
dispatch or execution-policy change. The transport consumes no direct winapi-util dependency.

The existing hidden helper entry and optional PowerShell route first enter a native read-only
broker. It validates launch context and uses this same guarded native launch; an already running
broker cannot qualify its own mapped image. A child-mode dispatch is an internal selector, not
authority, and refuses an absent/invalid exchange before operation effects. Final helper acceptance
remains separate: Qualified does not create a journal, write operation status, report pending/updated or
authorize task/PATH/file mutations. Recovery additionally needs the validated protected original
journal context; it must not derive helper/source authority from the current replaced receipt.
Keep this gate and the accepted preparation/model modules test-only until their real consumers
are reviewed together with the provider's live snapshot, activation and fresh-registration APIs.
Use a test-only ServiceRestoreRecord reexport only with those actual test consumers; do not
manufacture ServiceSnapshot casts, unused production exports or allow(dead_code) to enable them.

Verify on native x64/ARM64 using a real copied test executable and unique private fixture roots:
the guarded launch passes once, concurrent leaf/ancestor replacement and conflicting writes are
refused across readiness/guard overlap, changed created full identity or original request bytes
refuse, a wrong actual image/missing module/process exit or broken/stale/oversized channel refuses,
pipe collisions/wrong peers/trailing bytes or a held-open terminal sender refuse, terminal EOF
occurs while the original child remains live, and cold deadline/owned cleanup stays bounded.
Exercise real creation-time ACL/remote rejection and flush/drop behavior on both architectures,
including refusal after buffered data loses its reader, complete delivery before terminal close and an
unread/held-open endpoint timing out while retaining its worker and launch guards.
For that negative flush fixture, use a test-owned receive-only standard File with no IOCP
registration or background reads, write actual bytes, and observe the retained raw-flush worker
pending before closing that known native reader. Require the final native error rather than
assuming an empty disconnected pipe must fail. The single-accept fixture proves first-instance
collision refusal and delivery only on its sole consumed accepted stream; a late open may fail
or briefly reach the dropped unused instance, but must receive no qualification bytes. No repeated
connection probe, quiet-period result or successful empty flush can qualify a peer. All fixture
waits and cold setup retain the original deadline; native results remain hosted qualification gates.
Assert no installed-target/state/task/PATH/journal
mutation in every qualification fixture. Release executable qualification, actual helper
acceptance, complete lifecycle effects and clean Windows 11 acceptance remain separate gates.

Capacity counts all fixed nullable keys, the longest reachable enum spellings, both future
created/rollback full identities per leaf, every repeated typed service slot, and the future package path/key/version
bound before journal creation. Fresh installation additionally requires the service owner's real
typed registration/root rollback record and complete callback plan; the existing-installation
record cannot authorize fresh role creation or substitute null for that missing authority.
For the existing-installation path, no new daemon/dashboard activation occurs before all seven
replacement payloads and the new receipt are fully verified and durable. Before that boundary,
rollback uses the original confirmed-quiescent role record; it never activates a new executable.
After entering restoration, failure remains pending restoration and the journal rejects a return
to file rollback. This order avoids an uncounted new-origin service snapshot slot. Any future path
that activates new roles earlier must first add a separate real typed new-origin quiescence record
and its complete persistence budget. Task activation also needs its own recorded intent/result;
an enabled flag or enable intent cannot authorize replaying Task.Run after an uncertain result.
For existing operations, activation selects every originally enabled owned registration; its
transient prior running state is not authority and needs no new immutable original-running field.
Originally disabled roles retain their disabled setting and never receive Run. Activation starts
only after all registration/enable confirmations and the verified complete payload/receipt boundary.
The service owner must add a distinct typed activating phase with a durable per-role RunIntent
before dispatch, the immutable observed Task Scheduler InstanceGuid and the sealed matching
context/digest plus exact live supervisor/role lifetime confirmation, and an UnknownStart state
for uncertain dispatch or startup. A successful COM call,
an enabled setting, a task-state string or EnginePID alone cannot confirm the role. Recovery may
confirm an already authenticated exact owned instance through fresh guarded readback of that
same GUID/context/digest and live lifetime. An unrelated GUID or unwitnessed Logon launch cannot
complete the recorded activation. Recovery must not redispatch Run
from a saved intent, unknown state or enabled flag. No repeated polling record may consume an
unbounded callback budget. A timed-out persistence owner remains uncertain under the existing
retained-writer/guard rule.
Before effects, replace the model's registration-only summed-restore scaffold with the real
provider's complete maximum object growth and longest legal callback branch. Count the common
backup/quiesce prefix, then the maximum of forward replacement/restoration/activation and a
pre-restoration rollback/original-role-restoration/activation path. Those paths are mutually
exclusive under the frozen no-Restoring-to-file-rollback ordering; neither path may omit activation
intent, observed result, unknown-start recovery, final lifetime proof or outer/frame/PATH overhead.
The exact activation representation and callback ceiling must be reviewed with the service owner
before source changes. Fresh installation still requires its separate real typed root/task creation
and rollback record; the existing-operation snapshot cannot authorize that creation.
sync_all must succeed before the next
effect. Frame/size exhaustion, flush uncertainty, a truncated/corrupt tail or an unknown created
identity is an explicit refusal with backups retained and no replay inferred from status. Recovery
validates live guards, recorded identities and typed lifecycle state before any resumed mutation.
status.json is an advisory projection written through its exact guarded handle; a partial status
is never an authorization source or evidence of completion. Helper acceptance still requires a
validated protected original request and durable accepted state; final updated=true requires the
confirmed binary/receipt and restoration boundary.

Native safe-crate, ACL, mapped-holder, competing-leaf and crash-phase fixtures on both architectures
must pass before this candidate adapter enables support; the x64 OS primitive rehearsal in FINDINGS
is narrower evidence. Write-through and sync_all provide OS flush guarantees, not an unconditional
promise against storage hardware or filesystem failure.
Distribution filesystem fixtures retain a unique disposable TempDir only as a cleanup container.
Guard its existing ancestry, create a private child through DirectoryGuard::private, retain the
child guard and use its normalized_path for every fixture object. Assert the actual current-SID
owner/protected descriptor through is_private before exercising the operation. Do not assume the
container's inherited owner, repair that container, or weaken production checks for runner paths.
Run the positive Windows distribution contract harness with libtest --test-threads=1. Those
independent successful setup assertions share this test process's one filesystem and one generic
worker admission slots; unbounded libtest setup concurrency is not their success contract. Keep
explicit concurrent admission, cold-call, deadline and cleanup assertions unchanged. Do not warm
the first call, retry or skip a timeout, add helper slots, or extend the production 30-second budget.
Verify: the same reviewed revision passes every distribution assertion once on native x64/ARM64,
with zero ignored tests and the existing concurrency/deadline gates still exercised separately.
The stock PowerShell archive parser and ZIP fixture explicitly load System.IO.Compression before
using ZipArchive/ZipArchiveMode. Their stream APIs do not need FileSystem extension methods;
loading that separate assembly alone is not a reliable type-loading contract in a fresh 5.1
session. Use Add-Type's existing-assembly parameter, without source compilation or an execution
policy change. Verify the complete existing installer fixture in the native x64/ARM64 stock 5.1
no-profile CI session; retain its malformed archive and status refusals unchanged. Do not infer
bootstrap or native lifecycle completion from this archive fixture.

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

WinGet maintenance obtains a separate Windows-only `open_owned_executable_exclusive` proof gate
after recorded all-executable quiescence. Use existing-only guarded, trusted-owner ancestors and
a regular no-reparse leaf opened with read/write/DELETE access and share_mode(0); validate the
same current-SID owner and no nontrusted mutation grants as `read_owned_executable` on that exact
handle. Permit trusted SYSTEM/Administrators rights and foreign read/execute without requiring
the standalone private descriptor. Refuse read-only attributes, multiple hard links and missing
full file identity. Never create, repair, write or delete package bytes through this API; it only
proves mapped holders are gone and blocks new opens until the caller releases the gate for the
external package manager. Keep the standalone replacement APIs' protected SID/SYSTEM policy.
Native Verify: a disposable owned executable with Administrators write/foreign read-execute is
accepted, every nontrusted mutation grant is refused without byte/ACL changes, a mapped process
refuses the gate, the live gate blocks launch/read/write/rename, and release permits the original
executable to run with unchanged bytes and descriptor. Missing roots remain absent throughout.

### Native runtime fixtures

Keep portable process/output/HTTP behavior under native Windows tests, with a private managed
child of each temporary fixture root. A self-spawned native Rust test fixture exercises exact argv,
raw stdout/stderr, immediate descendant creation, root-first exit, cancellation, timeout and
kill-on-close without Git Bash or an installed scripting runtime. Signal-number/Unix process-group
assertions remain Unix-specific; equivalent Windows tests prove owned Job tree behavior.

Intentional native orphan/crash fixtures transfer each spawned descendant's process handle with
the safe `From<std::process::Child> for OwnedHandle` conversion into a shared test-only helper,
capturing its PID first when the fixture needs a marker. Retain the typed handle during the
intended fixture phase; ordinary scope exit closes that handle without waiting or terminating,
and abrupt parent exit lets Windows close it. Do not add a waiter, kill-on-drop helper, raw-handle
leak or lint exemption: those would alter the root-first/abandonment scenario or obscure ownership.
Ordinary tree/branch fixtures still wait normally. Verify the existing native contracts: a reaped
root with a live descendant cannot confirm completion, releasing that descendant allows success,
hard stop confirms root plus empty Job and stops its heartbeat, and runner cancellation/timeout/
kill-on-close preserve whole-tree termination. Native lint and the x64/ARM64/MSRV suites must pass
with the same assertions and deadlines.

The TLS trust fixture uses the already-locked tokio-rustls =0.26.4 with an explicit AWS-LC provider
and repository test-only self-signed DER certificate/key. It never installs a certificate into a
trust store, invokes external OpenSSL, or changes production trust policy. All architectures verify
an untrusted local TLS peer is a retryable transport failure. Bounded native IPC tests cover valid
wake/cooperative control, malformed frames, idle/nonreading clients, first-instance collisions,
role/lifetime separation and listener teardown while preserving durable reconciliation.

Separate headless CLI fixtures verify registered dashboard control against actual process exit
and a subsequently free dashboard lock, with both an active SSE stream and incomplete HTTP
headers/body. A gated native job publishes a heartbeat before and after dashboard exit; the
manual daemon retains its original lifetime and the durable run has no cancellation request.
Only after these observations does the fixture release the job's gate and confirm success.
Capture bounded startup JSON without exposing its token value and retain/reap every owned child.
An explicit absolute LOCRON_TEST_BINARY override belongs only to the integration-test harness,
so the downloaded same-revision CLI and test artifacts can run on a standard-user host. The
production CLI never reads this override. Native CI also runs the registered-dashboard unit
contracts after the actual lifecycle suite, keeping the established job names and deadlines.

Portable CLI composition unit fixtures create a private child state directory and use a real
native executable plus an absolute fixture working directory. A self-spawned Rust unit target
returns the same failure status as the Unix shell fixture, preserving the durable completion /
response-loss and retry-deadline contracts without requiring a Unix shell on Windows. Path-list
normalization tests construct the platform delimiter and assert each resolved entry separately.

### Change order and verification

1. Review SPEC, source-backed FINDINGS, these decisions and repository issues; freeze the minimum
   contract and safe interface versions before implementation. Verify: review resolves all contract
   gaps and each issue has concrete criteria; no premature support claim.
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

## Repository Issue execution tracking migration (2026-10-03)

The owner selected repository Issues for all maintainer TODOs and requested that the private
Locron Project be closed. This supersedes its live-tracking policy; the earlier migration remains
historical evidence. Product/design contracts and accepted Windows implementation decisions do
not change. All subsequent execution plans, deviations and verification evidence belong to the
relevant repository issue before source changes; completed historical work is not fresh proof.

The exhausted Project snapshot contains 83 drafts: 70 Done, eleven In Progress and two Todo.
Thirteen Windows drafts already have matching public issues #24–#36. Reuse those issue identities
and preserve their earlier public text as explicitly historical migration evidence, then copy the
full current draft body without dropping scope, checklist state, Verify criteria, dependencies or
progress. Convert the remaining seventy drafts through GitHub's supported draft-to-issue API,
retaining original Project item links. Map Done to a closed/completed issue, and preserve Todo
versus In Progress on open issues with explicit labels. Existing issue #4, Windows umbrella #23
and deferred unsigned-release signing follow-up #37 remain separate existing records.

1. Capture the exact exhausted source, Project metadata/fields and all existing issue identities
   before writes. Build a unique source-to-issue map and preserve original task text and status.
   Verify: 83 unique unarchived drafts, 13 unambiguous existing Windows identities, no omitted page
   or duplicate task destination; record source hashes and preserve unrelated existing issues.
2. Reuse/convert sequentially with destination and body readback after every write. Keep existing
   Windows numbers, prior issue text, Phase/Legacy ID, source order and all meaningful links.
   Verify: all 83 mappings have exact source bodies, titles and Verify/checklist content, 70 closed
   completed histories and thirteen open tasks (eleven active, two planned); no duplicate issue.
3. Update repository agent/contributor/TODO guidance and static migration links, then close the
   Project through the CLI with a retired README pointing to Issues. Preserve the closed Project
   as history, without deleting it or reclassifying pending work as complete.
   Verify: new instructions consistently use Issues, immutable source snapshots stay byte-exact,
   task/PR links are preserved, the final Project reads closed, and future progress goes to issue
   comments. Review and publish the documentation with the current PR work.

## Project-only execution tracking migration (historical; superseded 2026-10-03)

The following preserves the accepted 2026-10-02 plan and readbacks. Current task rules are in
[`ISSUES.md`](ISSUES.md); its source snapshot preserves the complete former Project workflow.

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

The Windows-support CI audit also found active readiness probes in global_environment,
attempt_history and service_lifetime. Apply this same passive PID/liveness contract to their
helpers, retaining owned-child cleanup and bounded startup stderr before reporting a failure.
Keep their real command, output and graceful-exit assertions under the original deadlines.

The general CLI startup helper must also use a plain bounded existing-file metadata read on
Unix: the production private-file helper retains its original parent-creation/permission behavior
there and is inappropriate for a passive fixture observer. Windows keeps the protected owner
sidecar because exclusive byte-range locking prevents reading the live lock bytes. Both branches
require the expected live child PID; retain the actual-lock ownership assertion after readiness.

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

The plan is restricted to this repository. Before an implementation deviation, update `docs/IMPLEMENTATION.md` and the relevant repository issues; update `docs/ARCHITECTURE.md` first when the durable structure or invariant changes.

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

The following compaction policy records the former repository-checklist workflow. Current execution
tasks live in repository Issues; see [`ISSUES.md`](ISSUES.md). Preserve the archived source and
evidence without resuming a live TODO checklist or moving new completed issues into it.

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

## Native pruning ACL fixture correction (2026-10-03)

Root43 `c2a7495` reached the original unsafe-output pruning fixture after the corrected initial Store
and PATH contracts passed. Its Get-Acl setup fails under the intentionally fixed stock binary
loader. This is a test-fixture compatibility correction within the frozen product and approved
ACL/reparse refusal plan; do not expand production module loading or alter pruning admission.

1. Keep the original fixture's JSON path input and Everyone-Read ACL mutation, replacing its
   unavailable Security cmdlets with .NET Framework File.GetAccessControl/SetAccessControl.
   **Verify:** stock PowerShell 5.1 parses the fixed literal; documented signatures match;
   production loader, filesystem and Store source remain byte-identical. AST/type inspection
   is not native execution, and local permission mutation is not part of this correction.
2. Preserve all existing negative-case assertions and owned-object cleanup. **Verify:** exact
   source review retains unsafe-file refusal, pending SQLite state and original bytes plus
   junction/directory refusal and unrelated-marker preservation; no test is skipped or softened.
3. Hand the recorded issue plan to a separate development session, then root-review/select it.
   **Verify:** fresh exact-head hosted native x64/ARM64/MSRV pruning 3/3 and the remaining CI
   pass before merge. Record actual revision/commands/results on issues #27/#31; initial releases
   stay unsigned and this fixture does not complete standard-user/reboot/release acceptance.

## Native WAL recovery contention correction (2026-10-03)

The measured Root43 `bfe7b8d` Server failure returns real SQLITE_BUSY_RECOVERY (261) from the
fixed WAL setup before its original five-second budget is used. The current predicate admits
only exact 5. This is a correction within the frozen SQLite/private-state product contract;
retain dashboard shutdown behavior and its actual active-SSE/durable-run fixture.

1. Extend only the finalized idle WAL admission predicate to explicit codes 5 and 261, including
   the existing step/finalize error-precedence boundary. **Verify:** full source review shows the
   same entry deadline, zero internal busy timeout, bounded ten-millisecond yields, autocommit
   checks, effective WAL readback and one-shot remaining settings; no other operation is replayed.
2. Preserve the four original real SQLite controls and their exact-5 assertions, immutable SQL,
   physical DB/sidecar admission and the Server regression fixture. **Verify:** unchanged control
   assertions and production boundaries; snapshot 517, timeout 773, LOCKED, read-only and I/O
   errors still refuse. Do not add a fake-error or unproven overlap fixture. Existing native 261
   is recorded failure evidence; a later passing run is not claimed to force the same error.
3. Record the issue plan, hand implementation to a separate developer and root-review/select it.
   **Verify:** source coverage is complete and all protected blobs/deltas match; fresh exact-head
   x64/ARM64/MSRV Server 31, Store 90, original concurrency/PATH/lifecycle and guarded-prune 3/3
   pass with all remaining checks before merge. Keep unfinished acceptance/release issues open.

Extend the already reviewed Framework ACL fixture correction to the later maintenance unsafe-
object setup at maintenance.rs:919/921, observed only after guarded pruning now passed.
**Verify:** a separate developer replaces those two statements, retaining the exact JSON input,
Everyone-Read rule, all refusal/recovery/prune/marker assertions, cleanup and deadlines. Root
reviews the complete diff and surrounding fixture; production maintenance/loader/filesystem
source stays byte-identical. Fresh exact-head native maintenance 11/11 on x64/ARM64/MSRV must
pass with pruning 3/3, the complete Server/Store suites and all other checks before merge.

## Atomic private bootstrap file creation (2026-10-03)

Native PR44 60e2fcc reaches the existing strict foreign-owner refusal in the stock-5.1 bootstrap.
Close the source-established missing creation descriptor within the frozen private-installation
contract. The failing object's actual owner/caller is unmeasured; retain strict ownership refusal.

1. Prepare FileSecurity with current SID owner, a protected DACL and exactly current SID/SYSTEM
   FullControl, then pass it to the Framework FileStream constructor at CreateNew. **Verify:**
   complete source review preserves normalized parent validation, FileShare.None, handle owner/ACL
   check, exact write/Flush(true)/finally and existing-file collision refusal; there is no later
   SetAccessControl repair, adoption, privilege change or execution-policy change.
2. Keep the original positive and negative bootstrap assertions and add fixed stage labels around
   existing positive root/leaf creation. **Verify:** all prior fixture bytes remain after removing
   only those labels; no path/SID is logged, no assertion/cleanup is removed and generated
   uninstall.ps1 remains byte-identical after the renderer check. No local native fixture or
   PowerShell inspection workaround is authorized after the recorded execution-policy refusal.
3. Hand the issue Verify plan to a separate development session and root-review/select its source.
   **Verify:** fresh exact-head hosted x64/ARM64 stock-5.1 bootstrap, all distribution cases and
   complete CI pass before merge; report measured stage/result on issues #32/#33. Standard-user
   installation/reboot/paired activation/publication remain separate acceptance gates, and the
   initial release stays unsigned with signing deferred in #37.

## First-run read-only empty database admission correction (2026-10-03)

The measured PR44 MSRV error 32 is reported around the complete create/reopen boundary. Source
review establishes an empty-file observation gap before the creator's guarded writable handle;
the same-tree latest green run does not remove it. Correct the observer within the frozen
private-state/first-run contract; no product specification or creation ownership policy changes.

1. Open an existing private database with the ordinary guarded READ/WRITE-sharing reader before
   attempting a stable read gate. Refuse zero length as initialization pending, retain the
   preflight handle and compare its full identity with the original admitted first handle.
   **Verify:** empty observation never takes a write-excluding gate or creates a database/journal;
   original ACL/reparse/owner validation precedes the length check. Keep the original stable
   fallback, journal pairing/rechecks, immutable/normal VFS selection and reported-path identity
   checks; propagate other errors. No creation/SQL retry, DACL change or repaired-object adoption.
2. Add one actual Windows private-file regression through public Store::open_read_only. **Verify:**
   a genuinely zero-byte private leaf refuses with Conflict, retaining bytes, full identity, ACL
   and absent journals; a real writable Store subsequently initializes that exact leaf and its
   settings are readable. Preserve all original Store tests and the entire lifecycle fixture,
   including its settings/PATH assertions and original 30-second/20-millisecond observation bounds.
   Valid closed snapshot writer-exclusion and live WAL observations must still pass.
3. Record the plan on #27/#31 before handing source to a separate developer, then root-review and
   publish the selected change. **Verify:** review covers every changed file and retained identity
   lifetime; Rust 1.94 formatting/locked metadata facts pass, and fresh exact-head x64/ARM64/MSRV
   Store (90 original plus the new regression), first-run lifecycle 12, remaining foundation/GUI,
   both 105-case distribution and stock-5.1 bootstrap suites, and all 19 CI/Audit/checks pass before
   head-matched merge. No local native/PowerShell workaround is used. Whole Windows 11 standard-user,
   paired install/update/activation, reboot and public release/WinGet acceptance stay open; signing
   remains deferred in #37.

### Windows dashboard wake diagnostic parity (2026-10-03)

Correct the confirmed Diagnostics display defect within the frozen read-only health and shared
platform-facts contract. SPEC, serialized API/CLI/MCP output, IPC and backend behavior stay fixed.
Use the existing Windows-only additive wake object; retain legacy Unix boolean compatibility.

1. Model the nullable legacy wake_socket and optional passive wake facts in the route. Present
   named-pipe transport and unprobed availability without a filesystem-socket health claim; keep
   Unix true/false as the existing present/absent facts. **Verify:** actual Windows-shaped JSON
   renders named pipe and not probed, each Unix boolean retains its old result, and null with
   missing or unsupported transport facts renders unknown rather than absent/healthy.
2. Add focused rendering regressions using the current API client boundary. **Verify:** those
   payload cases reach the rendered diagnostics facts and one existing GET; no extra mutation,
   endpoint probe or fallback request is introduced. Preserve loading/error handling and all
   existing facts. Tests qualify observable behavior rather than duplicate a formatter's logic.
3. Record these Verify criteria on #29 before a separate developer changes source; parent-review
   every changed file and publish through a normal PR. **Verify:** locked frontend tests, type
   checking/build and diff checks pass; root confirms no Rust/backend/protocol/lockfile changes,
   then the exact reviewed head passes the repository CI/Audit checks before head-matched merge.
   Broader two-user/two-root IPC, durable fallback, Windows 11 and public-release acceptance
   remain the existing open issue criteria; this correction completes only the display slice.

### Windows PATH entry anchoring correction (2026-10-03)

Correct the PATH-entry defect recorded in FINDINGS under #26's existing deterministic
execution contract. SPEC already requires the effective configured PATH/PATHEXT and job
working directory, so no product-scope amendment is needed. Before any filesystem lookup,
classify each Windows PATH entry with the same ambiguity rule as an executable request.
Skip drive-relative and root-relative entries; preserve absolute drive/UNC/verbatim entries
and anchor ordinary relative entries, including empty entries, to the explicit job cwd.

Skipping one unusable location preserves later usable PATH entries and the resolver's current
Option/missing-candidate contract. Refusing the whole list would change error policy across
entrypoints without improving job-cwd isolation. Retain PATHEXT order, absolute executable
canonicalization, argv, environment precedence and Unix behavior; add no ambient fallback.

1. Apply shared pre-lookup path admission in execution.rs. **Verify:** C:bin, C: and root-only
   entries refuse while ordinary relative, absolute drive, UNC and verbatim entries preserve
   their existing interpretation; ambiguous executable requests remain refused.
2. Add an isolated native subprocess regression with distinct process/job working directories.
   **Verify:** real files establish the ambient drive-relative/root-relative lookup control;
   the resolver skips those entries, returns no match when they are the only entries, and
   selects the job-relative/absolute candidate after an invalid entry. Empty PATH entries
   resolve against the job cwd. Only the disposable child receives a changed cwd/environment.
3. Review and qualify the focused change. **Verify:** existing mixed-case PATHEXT/Unicode/direct
   target tests and new native regressions pass on the repository's Windows lanes; retained Unix
   tests, formatting and diff checks pass. Pure UNC vectors prove classification, not access to
   a live network share. Record unavailable local tools and unrun native gates honestly; publish
   a Draft PR without claiming the broader Windows execution issue or release milestone complete.

## Guarded paired standalone inventory (2026-10-03)

Continue #33/#34's accepted paired ownership work under #27's existing private-file policy.
The v2 receipt and five-member archive parsers qualify in-memory inputs without retained
installed-file guards; the live standalone reader and preparation/transaction consumers use the
historical single-image v1 contract. Add a separate read-only paired inventory consumer without changing
SPEC, the v1 consumers, service activation, public commands, release packaging or support claims.
This module remains `cfg(all(windows, test))` beside the existing distribution foundations.
Native fixtures call the actual guarded reader; there is no production read-only entrypoint in
this slice. The returned inventory proves recorded local bytes and structural PE properties,
not a canonical download, executed version/ABI, helper authority or successful installation.

Use the existing current-user SID/native-target selection, existing-only private directory
guards, immutable private reads and full native file identities. The caller supplies only the
selected directory and its original absolute deadline; caller-provided SID, architecture or
receipt metadata cannot construct a successful inventory. Parse the retained v2 receipt bytes
against the actual guarded canonical directory, SID and target, then read all seven fixed
payloads under retained no-write/no-delete-sharing guards. Compare every digest to the strict
receipt map and the two ordered executable bindings; inspect the actual console/launcher bytes
using the existing unsigned PE/import validator with subsystems 3/2. Keep the raw receipt bytes,
the root/receipt/payload guards and all complete native identities together in a move-only type
with private fields and borrowed accessors. Never serialize this type into effect authority or
derive it from a deserialized receipt. Reject any repeated full identity among the receipt and
seven listed leaves, so distinct inventory names cannot alias the same guarded object. This
does not certify the absence of other hard links outside that inventory; exclusive replacement
still requires its separate single-link gate. Do not adopt or inspect unrelated entries.
Apply the existing 4,096 UTF-16-unit maintenance-path limit to the canonical root and every
joined and actually guarded receipt/payload path, including any retained transport prefix;
an equivalent shorter spelling in receipt metadata cannot bypass the live-path capacity bound.

Check the same caller deadline before and after each guarded operation, receipt parsing, hash
comparison and PE inspection, and before returning the complete inventory. Use the existing
`immutable_private_until` contract; do not create a fresh duration, deadline, retry or detached
worker. The synchronous caller owns the inventory construction and its guards through return
or refusal. An expired operation cannot return a successful partial or complete inventory.
Keep the 128 KiB receipt and 64 MiB individual payload read bounds. Process payloads sequentially
and release each payload byte buffer before reading the next; retain only their guards/digests/
identities. Working payload content is therefore bounded by one such buffer, not seven payload
copies, plus the bounded receipt bytes/parsed metadata and existing reader/parser allocation
overhead. No new aggregate disk-size restriction or promise about allocator overhead is added.

Implementation and verification order:

1. Add the guarded v2 reader and its test-only module registration. **Verify:** actual private
   receipt/seven-leaf fixtures return the unchanged raw receipt, seven exact guarded digests,
   ordered native PE pair and complete receipt/payload file IDs; held guards reject writes and
   replacement, and unrelated files stay unowned and unchanged.
2. Add bounded refusal fixtures through that same reader. **Verify:** missing roots/receipt/
   payloads stay missing; v1, foreign SID/channel/directory/target, malformed or swapped pair,
   changed hashes, bad console/GUI subsystem, repeated full identities, unsafe ACL/reparse leaves
   and over-limit metadata/payload files refuse without repair, deletion, journal, status, task
   or PATH effects. A pre-expired caller deadline refuses before discovery; missing-payload
   refusals release the earlier receipt guard. These fixtures do not qualify mid-operation
   timeout behavior or an actual foreign filesystem-owner change.
3. Review the complete scoped diff and run available formatting/static checks, then require the
   existing native x64/ARM64 distribution command on the published revision. **Verify:** all
   new guarded-reader fixtures plus every original distribution case pass without relaxed
   privacy, bounds, v1 fixtures or cfg gates. Record unavailable local Rust/Windows execution
   honestly; installer/update/activation, runtime version/ABI probes, immutable publication and
   standard-user Windows acceptance remain separate owning-issue gates.

### Paired Windows package producer in draft CI (2026-10-03, #32)

The five-member archive and native GUI probe contracts above are already selected. The current
producer still emits the historical console-only four-member archive, so add an explicit paired
mode to qualify actual packaged bytes before installation or public release integration. Keep
the default producer, release workflow, immutable public inventories and existing CI job names
unchanged. A separate CI artifact records a development candidate, not a published Windows release.

1. Add opt-in `package --launcher PATH` and `validate --paired` to the Windows package tool.
   Require exactly the console, GUI companion, README and two licenses under the existing
   version/target root. Validate both PE32+ machines, unsigned status, console/GUI subsystems
   3/2 and finite normal/delay imports before executing either image. Preserve the shared
   64 MiB archive/expanded bound. **Verify:** x64 and ARM64 fixtures pass; missing/extra/duplicate
   members, wrong architecture/subsystem, certificates, non-stock imports and size overflow
   fail before any native probe. Reject raw ZIP names changed by the ZIP reader's NUL handling
   before inventory acceptance. Default four-member validation and all historic asset tests pass.
2. Stage only verified archive bytes into a disposable directory and execute sole `--version`
   for each image and sole `--identity-probe` for the GUI companion. Use that directory as cwd,
   system-only child PATH, a 30-second subprocess timeout and at most 4 KiB accepted UTF-8 output
   with empty stderr. The output limit is checked after standard subprocess capture; it is not
   a capture-memory ceiling. This gate verifies the bounded probe contract of CI-built images.
   Require exact version lines and the existing strict six-field JSON object,
   including target, version, native-gui-v1 and consistent raw CONOUT facts; duplicate, omitted,
   unknown or trailing fields/bytes refuse. CONOUT facts do not establish activation or attachment.
   Validate a temporary candidate ZIP and read its final bytes before exclusively creating any
   final artifact path; copy/write/close failure cleans up only the new file from that attempt.
   **Verify:** injected execution fixtures inspect staged bytes, cwd, PATH and timeout; version,
   ABI, schema/type, trailing-output, timeout, unsuccessful-exit and final-copy failures leave no
   artifact; an existing final file remains unchanged when exclusive creation refuses.
   Validation of the final paired archive repeats these read-only probes and reports actual
   individual hashes, PE facts and launcher identity without deriving ABI from a requested tag.
3. Build the optional GUI binary with the existing native MSVC/static-CRT CI builds, preserve
   their four-member package artifact, then create a separately named paired draft artifact.
   Record source revision/repository, native toolchain, requested target, static CRT intent,
   actual probe facts, per-image hashes and final ZIP hash with the artifact. **Verify:** focused
   Python distribution fixtures and workflow validation pass; both hosted Windows architectures
   must execute the paired producer/validator on the recorded revision before native results can
   be reported as passing. Installer/update dispatch, role activation, standard-user/reboot
   acceptance, immutable release publication and WinGet remain their existing open gates.

### Guarded registered WinGet index prerequisite (2026-10-03)

Issue #35's bounded SQLite reader currently qualifies caller-supplied bytes. Add the actual
read-only registration/index connection as a narrower prerequisite to the accepted IndexedPair
composition. RegisteredIndex is an opaque, move-only observation of the actual current SID,
selected HKCU64 portable registration and retained index object. Its fields and construction
stay private. It is neither complete paired-file ownership nor authority to activate, install,
update, remove or repair anything. The five canonical ZIP/source leaves, both executable
identities/PE/version/ABI checks and actual console-link ownership remain separate work.

1. Add an absolute-Instant form of the existing stock JSON adapter, forwarding the earlier of
   the caller's original deadline and the existing thirty-second per-call cap. Keep its same
   finite worker, queue/child/cleanup ownership and quarantine behavior. The registered reader
   starts no replacement deadline, worker pool or native process of its own. **Verify:** an
   expired request refuses before adapter admission, a finite actual native registry read uses
   this API, and late returned observations cannot create a usable result. All historical
   adapter and single-image package paths retain their existing behavior.
2. Select the actual current-user registration for the exact console path with the existing
   fixed HKCU64 reader. Derive the index path only from the validated product code and registered
   location; guard that location, the exact version root and the existing index under the current
   package-source ACL policy. Retain its full native identity and a bounded complete snapshot,
   reject all -journal/-wal/-shm sidecars before and after the read, and call the existing <=4 MiB
   pure parser. The explicit expected_alias argument is solely a compared metadata expectation;
   no link is followed, opened or attested. **Verify:** real test-owned SQLite files pass without
   byte/descriptor changes; missing roots, wrong index/root/row facts, oversized files, sidecars,
   concurrent writers and unsafe descriptors/reparse paths refuse without creation or repair.
3. Re-read the actual SID and exact current registration before returning. Explicit later
   revalidation repeats registration, guarded-object identity/digest, ancestry and sidecar
   observations while retaining the original handles. Its deadline is capped to the original
   read's deadline even if the caller supplies a later Instant. **Verify:** changed SID or
   selected registration, changed index facts, new sidecars and late observations refuse; unit
   fixtures can supply metadata only through module-private test setup, while the public
   prerequisite entry point always reads actual native registration. Native no-match coverage
   reads existing registration without creating or modifying any registry key.

This synchronous prerequisite must run inside the caller's already finite owned native phase.
That owner must not retain the generic/COM script permit while entering this reader: its registry
query admits through that existing slot. Retained filesystem guards stay live across the query.
Pre/post gates surround each guard/read/query/parse boundary; they do not cancel a stalled native
filesystem call. A timed-out driver retains that worker and its handles until completion or
quarantine, and cannot reuse an uncertain observation. A registry snapshot is not a retained
registry lock: revalidation observes the exact current selection and refuses changes, but cannot
prove continuous immutability of registry metadata between calls. Stable index read handles deny
write/delete sharing; the parser only sees copied bytes and never reopens a SQLite path.

Keep the entire registered-reader source behind the existing Windows test-only distribution
staging boundary. No CLI, receipt/journal schema, manifest, release or package effect is added.
The existing native self_update::windows_ package selector must run these real-file regressions
on x64 and ARM64. A cloud environment without Rust/native Windows cannot claim those gates;
record actual local checks and leave compilation, native qualification, selected-client
install/upgrade/removal and catalog publication open in #35 and the Draft PR.

### Windows native validation and actual package compiler (2026-10-03)

Continue #31/#32 using [the reviewed verification plan](planning/WINDOWS_NATIVE_VALIDATION_2026-10-03.md).
FINDINGS records current downloaded package evidence and rustup override precedence. Explicitly
select 1.94.0 in the Windows package job and existing Windows release build step, rejecting an
unexpected compiler release/host before building. Preserve Unix compilation and signing flows.
Add optional manual CI discovery for native full workspace tests and warnings-denied all-target
Clippy with explicit toolchains. Existing component/GUI/package gates remain unchanged. Full
discovery failures must be recorded rather than hidden with skips or relaxed fixture deadlines.
Only the five discovery rows run during full_windows=true manual events, avoiding duplicate
ordinary CI; PR, push and default manual events retain their complete existing checks.

The parent reviews the plan and issues before the separate development session edits workflows,
then reviews Source and publishes a normal PR. Exact-head normal CI, downloaded compiler/package
facts, local finite x64 probes and all five manual full-suite results provide the Verify receipt;
they cannot stand in for remaining clean Windows 11 installation/reboot, public release, native
update/recovery or catalog acceptance. No product scope, signing policy or application Source changes.

### Measured native integration-test compilation follow-up (2026-10-04)

Full discovery on the reviewed CI branch exposed compilation prerequisites before runtime
testing. Continue the existing #31 portable-fixture scope and
[validation plan](planning/WINDOWS_NATIVE_VALIDATION_2026-10-03.md) after the separate research
receipt: reorder only crate documentation before unchanged Unix gates in four integration
crates, and seed the three positive dashboard/service token fixtures through the existing
private-new-file API. Preserve each test's names, body/commands, material and report assertions;
new fixture bytes and their parent are private at creation, flushed and no longer guarded when
the tested subprocess begins. No existing root is repaired or adopted.

The new handoff is test-only and does not authorize application, toolchain, dependency or
privacy-policy changes. Parent reviews all six Source files, documentation-prefix/body identity,
private-new setup and protected blobs before publication. Rerun the original five native full
commands and ordinary protected PR checks on the next exact head. Preserve any next lint or
runtime failure in #31 instead of widening cfgs, adding allow/ignore, or changing deadlines.
Existing component/GUI/distribution gates remain separately required. Frozen SPEC and first
unsigned-release/signing policy remain unchanged; compile success is not full support evidence.

### Native runtime-fixture correction and bounded timing evidence (2026-10-04)

Continue #31 through [the measured follow-up plan](planning/WINDOWS_RUNTIME_FIXTURE_FOLLOWUP_2026-10-04.md).
Research distinguishes invalid managed-root setup, shared test-owner interference and a real
field-help omission from unmeasured production faults. Keep Unix MCP setup exact and create a
missing guarded private Windows child. Isolate only five Gate parent fixtures before setup and
deadline creation; retain actual release and the uncertain case's internal second refusal.
Add the hidden supervise field's description, and match seven backend helpers/imports/Drop to
their existing macOS/Linux consumers while preserving Windows reporting cases. Keep all other
CLI staged-code lint diagnostics visible rather than broadening this handoff.

For the independent Core stall failure, add fixed-size lock-free in-memory observation of the
test-only result/diagnostic/return/send/receive interval. Leave the production adapter, original
timed receives, trace output ordering, native stalled read/retention, second refusal, late-work
and cleanup assertions intact. Event output belongs after known delivery/cleanup or the existing
failed receive message, never synchronously added to timed delivery. This is diagnostic Source;
one passing observation cannot retrospectively establish the failed run's cause.

Parent reviews docs and exact issue Verify readback before seven Source files are handed to a
separate development session. Reconcile accepted current-main changes without altering reviewed
patches, then collect fresh normal protected CI and five full native rows on the final exact head.
The unchanged-source no-fail-fast verification branch remains separate evidence. Preserve all
old/new failures and unfinished native/public-release scope; no owner-PC effectful acceptance.

### Stock crash-fixture counter publication and expiry observations (2026-10-04)

Continue #31 under the [new measured fixture plan](planning/WINDOWS_STOCK_CRASH_HEARTBEATS_2026-10-04.md).
FINDINGS records the exact current-head x64 MSRV failure and unchanged Core source, the missing
last counter/phase artifact, and the separately reproduced truncate/write hazard. The actual
cause of that CI timeout remains unconfirmed. Frozen SPEC and the earlier validation plans stay
unchanged; this repairs test observation/publication without changing process-security policy.

After the parent publishes the reviewed documentation and owning-issue pre-Source Verify note,
the separate development session changes only windows/loader_crash.rs and loader_crash_host.ps1.
If an isolated actual-process regression needs new explicit mode dispatch, add only those
cfg(test) arms in windows/loader_tests.rs; preserve every existing mode and selector. No product
loader, owned-child, guard/admission, dependency, toolchain or workflow change belongs here.

Each writer completes a new counter in the same private directory, flushes and closes it, then
uses explicit no-clobber initial publication or replacement of the published snapshot. Use the
already locked tempfile 3.27.0 API and stock Framework File.Move/File.Replace; propagate errors,
including sharing/metadata failures. Never truncate the final counter, delete it before moving,
fall back to copying/creation, or replace a failed parse with a default/saved number. Native
qualification must confirm actual reader sharing and process-stop behavior; this is not a
power-loss durability promise.

Store bounded counter bytes/status and fixed driver phase/timing facts during permitted reads
under the original deadline. On expiry, print only those saved observations; start no new file,
process or stock-adapter I/O for diagnostics. Preserve the single 45-second helper budget,
30-second adapter/observer/writer budgets, 3-second cleanup, 25-ms heartbeats, 200-ms settle
windows, and every real PID/handle/Job/tree/pipe/share=0 guard assertion. Parent reviews complete
source/cfg isolation and protected blobs; Verify includes controlled real-process unpublished
snapshot hard stops, both writers' initial/replacement failures, and unchanged fresh native
cold Core/EOF/Restricted/parent-exit gates on x64 stable, ARM64 stable and x64 Rust 1.94. Select
the new controlled modes after the normal parent-exit proof within its original helper budget,
without earlier cold-gate warm-up or a workflow change. Full discovery and all remaining clean
Windows 11, user/privacy, recovery and distribution acceptance
stay open and retain their failures; no skip, relaxed deadline, retry or quarantine reset qualifies.

### MCP process-fixture continuation under native target validation (2026-10-04)

Continue #31 using the [MCP fixture plan](planning/WINDOWS_MCP_NATIVE_PROCESS_FIXTURE_2026-10-04.md).
The seven-file correction now has ordinary native passes, while full CLI discovery still has
ten downstream MCP failures. FINDINGS distinguishes the source-backed invalid `/bin/echo`
Windows input from the unprinted actual error envelope. Frozen SPEC and production validation
stay unchanged; the goal is to reach the existing MCP assertions with a valid process definition.

After documentation and exact Issue Verify readback, a separate development session changes
only tests::valid_add_args in mcp.rs. Windows uses the actual running test image's absolute,
non-lossy path and fixed finite `--help` argv; Unix keeps the original command values. Preserve
process target type, job metadata/policy, private state setup, every test body/name/assertion
and original request_tool success refusal. This is definition/queue acceptance, not scheduled
process execution or echo-output proof. Add no new selector, skip, warning allowance or fallback.

Parent checks the complete one-helper diff, exact inverse conservation and every other blob,
then publishes fresh ordinary CI and the original five full native rows. Ten intended downstream
assertions must actually run and pass on x64 stable, ARM64 stable and x64 Rust1.94; any newly
reached integration/lint failure is retained. Separate stock-crash, integration, Core/Server
and public distribution work remain outside this slice and keep their unfinished Verify gates.

### Native integration positive setup and platform help expectations (2026-10-04)

Continue #31 under the [private-fixture plan](planning/WINDOWS_INTEGRATION_PRIVATE_FIXTURES_2026-10-04.md).
FINDINGS records the newly reached301-pass CLI unit binary,71-case integration failure and
independent required x64 Store-initialization timeout. Frozen SPEC and production privacy stay
unchanged. The parent reviews docs and exact Issue Verify readback before separate development.

Use one narrow test helper to retain TempDir cleanup while exposing a newly created guarded
private Windows child. Capture its normalized path and drop setup guards before child execution;
Unix keeps its original root. Adopt only selected state-bearing factories in CLI/feedback and
five dashboard positive cases. Keep advisory/discovery inputs, fake-manager parents and absent
paths separate. Use the existing private-new helper for stored tokens. The Windows manual-owner
positive creates real DaemonLock/current-PID/unique-lifetime metadata on its fresh private root
and retains that lock through fake install; Unix's original setup remains. Only three help
expectations change their Windows basename to locron.exe, retaining all canonical assertions.

Source is limited to four existing integration files and one shared private_state helper. Keep
all original case names, counts, negative contracts, dry-run absence/redaction/order checks and
deadlines. No production change, dependency, workflow, skip or warning allowance is selected.
Parent verifies exact inverse transformations and every other tracked blob, then publishes
fresh ordinary/full native results and complete per-binary discovery on the separate temporary
verification branch. Report downstream assumptions instead of masking failures or claiming all
102 integration cases are repaired. Real process/shell/wake equivalence, Store timeout/reopen,
stock-crash publication, actual two-user/registered-task and public distribution acceptance keep
their distinct plans and unfinished Verify criteria. No owner-PC effectful operation is selected.

### Stock heartbeat command-size and managed-null continuation (2026-10-04)

Continue the existing stock-heartbeat plan after measured e4 native pre-spawn failures. Keep
the three permitted Source files and original45s driver/30s adapter/owned cleanup boundaries.
In loader_crash_host.ps1 mark the shared publication definitions, existing complete proof
branch and unchanged normal-host tail with two unique static comment delimiters. A pure
loader_crash.rs builder selects shared plus normal or shared plus proof before prepare_adapter.
Two test-only OnceLock<String> slots retain those fixed strings for the existing static-lifetime
API; no leak, unsafe bridge, user-keyed cache or production signature change is needed.
Preserve both bodies and every actual-process assertion, using no new script file, runtime
script read, request-code interpolation, adapter fallback or workflow selection change.

Pass fully qualified NullString.Value directly as File.Replace's no-backup argument. This
does not permit metadata errors or replace the actual collision/share/stage/kill/reap checks.
Parent checks exact source-section conservation, both LF/CRLF full encoded command budgets,
format/actionlint/Windows-target compilation and all unrelated blobs before normal publication.
Fresh exact-head ordinary native rows must exercise the original parent crash and both
publication proofs. Preserve old unknown raw-error/timeout facts and full-discovery failures;
no required red check is bypassed. Reviewed docs and owning Issue Verify readback precede
the separate development handoff. All public acceptance gates remain unfinished.

### First-open WAL failure boundary diagnostics (2026-10-04)

Follow WINDOWS_WAL_CONFIGURE_DIAGNOSTICS_2026-10-04 and Issues #25/#31. Before
changing retry/ownership policy, a separate developer instruments only existing
Windows configuration/OpenTrace boundaries with a fixed owner-local debug
snapshot. Preserve the configure-entry5s clock and its WAL/settings scope,
fixed SQL, explicit finalization, idle-autocommit5/261 retry/error precedence,
10ms maximum contention yield and direct15 refusal. No added SQLite/FS call,
global synchronization, timed-path log or work after expiry. Release native
call policy remains exact. Debug stamps consume normal elapsed time.

After a failed result is known, emit one bounded768-byte ASCII receipt through
the existing debug error route: actual PID/local operation, fixed gate/phase,
attempt count, available numeric error/autocommit/mode categories and native
entry/return timing. No private strings or unbounded event history. Existing
control assertions and12 lifecycle cases retain their clocks/concurrency;
parent reviews full Source then publishes fresh ordinary/full/per-binary native
evidence. A passing later scenario does not diagnose an earlier failure. Missing
Server SSE-reopen connection/file ownership correlation remains separate, and
required red checks continue blocking merge.


### Stock heartbeat same-candidate rename control (2026-10-04)

Continue the existing test-only heartbeat plan after1a1's three native jobs pass the
original crash proof but expose the new rename expectationSome(32) versus actualSome(5).
FINDINGS records the precise operation distinction and the cancelled-run limitation.
After reviewed docs and Issue#31 Verify readback, a separate developer changes only
publication_proofs in loader_crash.rs. Stage one closed8 snapshot, retain its path, require
the actual5 refusal and the returned path's identical ownership, and require published
7/bytes7 while the target handle remains held. Close only that exact handle; publish the
same returned TempPath to8 and verify value/bytes. Publish complete7 to restore the
controlled baseline, then preserve the entire existing actual publisher stage/kill/reap
and final8 sequence. Every new I/O uses the original observation pre/post deadline checks.

No PowerShell/stock-open assertion, production code, dispatcher, workflow, dependency,
selector or clock changes. Parent reviews the whole one-function diff and all protected
blobs, runs applicable Windows-target compilation and formatting, then publishes fresh
ordinary plus full-discovery native evidence. The unchanged original proof and both new
publication proofs must pass all three ordinary native rows before merge. Retain every
full-suite failure and old unknown failure; broad Issues#31/#32 remain unfinished.


The same-candidate identity assertion must retain privacy and the warnings-denied lint
contract. Native lint111281642758 at f1cc5dc rejects assert!(left == right) as manual_assert_eq.
Bind the exact path equality once to a named boolean and assert that boolean with a fixed
failure message. This reports the identity failure without formatting either private path.
No warning allowance, identity weakening, or other Source/clock change is permitted.
The separate developer applies this single assertion-style correction after this plan.


### Stable cfg statement syntax after native compile refusal (2026-10-04)

Source5585936 ordinary37151318184, full37151330037 and isolated discovery37151378233
finished red before the requested application runtime tests. Separate always-run stock
bootstrap/small/60k diagnostics passed on the three foundation rows; these do not qualify
the failed compilation or application suite. Actual Rust1.94/stable/1.98 reject #[cfg] directly on
the two assignment expressions at windows_configure.rs168/237 with E0658. Release builds
also retain unresolved observations assignments (E0425). The immutable release-token inverse
and formatting checks could not establish compiler validity; Root's prior review missed this.

Select only a syntax correction: wrap those two existing debug observations in cfg-gated
blocks. Keep assignment expressions, captured existing autocommit results, cfg condition,
release erasure and every native call/result/gate/error/deadline byte-equivalent after inverse.
No feature/nightly/warning allowance, production/query/test/dependency/workflow change is selected.
Separate development owns only windows_configure.rs after Issue25/31 exact Verify readback.

1. Reproduce/repair the two stable cfg statement sites. Verify: Rust1.94 native Windows debug
   and release compilation both succeed; no unresolved observations or experimental syntax.
2. Conserve observation and native policy. Verify: two exact block-only transformations invert
   the whole prior file; all other tracked blobs/modes and old/native/new diagnostic tests remain
   exact; cfg-erased release policy still equals the ce641 original.
3. Review/publish fresh changed Source. Verify: Root reviews the one-file diff and static checks,
   then records actual ordinary/full/every-binary results at the new source. These previous runs
   reached no requested app suite cases and produced no new paired ZIP; separate stock
   diagnostics remain their own evidence. All runs remain failures, not skipped validations.


## Stable Rust syntax continuation before Source

Ordinary558 run37151318184 fails compilation, before native controls: Rust1.94
job111285640711 and Rust1.98 lint111285640451 report E0658 at windows_configure.rs
168 and237. The two cfg attributes attach directly to assignment expressions.
A separate developer may only enclose those same assignments in cfg-gated
statement blocks, keeping their exact debug-only evaluation points and values.
Verify the two-assignment inverse, unchanged release/native policy and all other
blobs, stable Rust1.94/1.98 syntax, formatting and fresh native compilation.
No feature gate, allow, clock, SQL, retry, diagnostic field or selector changes.
The original five-step plan and owning Issue criteria remain required.


### Native CLI process/shell/render fixture continuation (2026-10-04)

Select the one-file input-only plan in planning/WINDOWS_NATIVE_CLI_FIXTURE_PORTABILITY_2026-10-04.md
after complete native14-case/41-lint/later-binary triage in FINDINGS. Base main4ab equals
reviewed10fe. Root records exact Issue31 Verify before separate development. Windows success
targets use the actual cargo-built CLI with --help, and3 Target::Shell contracts use supported
explicit stock PowerShell static .NET output/sleep/marker/exit semantics. Preserve every Unix
value/body/argv,71 selectors, assertions, retry/clock/private-state/native cleanup; complete
human target/table byte oracles use actual target+argv without the production renderer.
Only11 listed existing test functions and narrow helpers may change. Wake readiness, both
doctor cases,41 full lint diagnoses and later distribution/Core cases remain unqualified.
Four ordered Verify criteria require reversible Source conservation, genuine native behavior,
exact human bytes and reviewed fresh native/ordinary CI results; no local native effects.


### PR134 lifecycle failure observation before Source (2026-10-04)

Apply the three Verify steps in planning/WINDOWS_DAEMON_CRASH_TREE_RECOVERY.md
to the existing test-only native lifecycle fixture. eae776 ordinary37157605645
x64 MSRV observes failed after running+heartbeat and dashboard exit/ownership proof,
but saved logs cannot identify the durable attempt or publication cause. Preserve
this failed qualification. Add fail-only bounded fixed durable facts and path-free
heartbeat failure facts; qualify the actual same-candidate held rename refusal and
release positive control under the existing bound. Keep immediate refusal, original
assertions/selectors/clocks, production/dependency/workflow blobs and frozen SPEC.
Separate development starts only after Issue30 exact Verify readback and parent plan
review. No retry, guard release, prewarm, warning allowance or success weakening is
authorized. Fresh exact-head ordinary native CI is required before PR134 merge.


### Default Windows doctor continuation before Source (2026-10-04)

Select planning/WINDOWS_DEFAULT_DOCTOR_2026-10-04.md after the separate complete
doctor/wake/consumer audit and actual495f phase correction. Root documents the
default-only observable unknown facts in SPEC/CLI and exact Issues29/31 Verify
before handing only service.rs/main.rs/tests/cli.rs to separate development.
The facts helper uses caller StatePaths, existing token observation and fixed
URL without default Windows UID/port/registration discovery. Unix/forced paths
and real errors stay exact; coherent null+unprobed gets exactly two new human
info lines beside the existing wake line. Preserve every old assertion/selector,
Unix input, clock and ownership call; native stock-PATH default/token/resolution
tests must execute. Four ordered Verify criteria require full Source review and
fresh native/Unix/fake evidence. No wake API/dependency/lifecycle effects are
authorized by this lease; remaining full lint/wake/cancellation/supervisor and
desktop/two-user/release gates remain open, without an unchanged-source retry.


### PR135 native doctor expectation correction before Source (2026-10-04)

Continue the original default-doctor lease after8644efb full37160909694 x64
301unitPASS then70PASS/5FAIL of75CLI integrations. Preserve the measured canonical
Windows resolved path and genuine exit5/service_io token refusal. Select only
independent canonical expected path and the complete existing ServiceError Display
prefix in the two newly added native cases, per planning/WINDOWS_DEFAULT_DOCTOR_2026-10-04.md.
Keep requested argv/stock PATH equality, private token non-disclosure and original
selectors/clocks. No production resolver/render/error/token/guard/dependency change
is selected. Three ordered Verify steps precede separate Source; parent reviews
exact inverse and fresh native/ordinary results. Unchanged WAL/disconnect/wake/cancel
and fullClippy41 failures stay visible and broader acceptance remains open.


### PR135 independent doctor CI gate before Source (2026-10-04)

The changed-test full run37162098246 reached all six doctor cases on ARM but
unchanged x64 supervisor expiry failed before CLI integration began. Add only an
independent Windows foundation doctor gate per planning/WINDOWS_DEFAULT_DOCTOR_2026-10-04.md,
before the existing registered service step. Run four native default-doctor cases
and both exact existing doctor cases on the current three-row compiler/architecture
matrix, with immediate nonzero refusal. Existing gates, all application/test blobs,
clocks and protections stay exact. Three Verify criteria require docs/Issues before
separate development, whole-workflow inverse and parent review, then fresh ordinary
native and paired-package evidence. Preserve prior full failures; do not repeat an
unchanged exploratory command or infer an unknown supervisor/WAL cause.

### Native CLI wake/cancel and independent exact gate before Source (2026-10-04)

Continue the existing two platform-incompatible CLI fixtures on reviewed main4644588.
Frozen product behavior remains actual prompt admission and confirmed cancellation.
Select exactly three Source paths per planning/WINDOWS_NATIVE_CLI_CONTROL_2026-10-04.md:
two test-only files and one additive existing foundation workflow step. The
private owner retains actual daemon/state/guard/client/query authority and original
5s/8s caller clocks; same-connected native server PID+strict ACK witnesses wake,
actual running+two native-target counter frames witnesses cancellation admission.
No readiness is inferred from paths, passive status or serialized identity.

Keep actual native wrappers safe, metadata-only duplication/no limbo, one cold
entry200ms, target30s/1201frames/64KiB read and production5s grace. Late/unknown
owners remain retained and refuse; exact owned cleanup precedes private teardown.
Use six individually frozen --exact selectors after runtime/doctor and before
service units under the existing status condition; auxiliary role returns count
zero acceptance. Four ordered Verify criteria cover ownership negatives, real
protocol/progress, whole-file/workflow conservation and all three changed-head
native rows plus original required/paired evidence. Root reviews full Docs/Issue
Verify before separate Source, then all Source and publication evidence. Old
wake/cancel/supervisor/WAL/full lint/later-binary failures remain; no clock increase,
retry/reset/whole-suite serialization, owner-PC effect or broader support claim.

### Untrusted negative native peer construction before Source (2026-10-04)

The wake/cancel continuation's three-path/six-selector lease selects safe public
Tokio Byte duplex first-instance/remote-refusal default-descriptor server only
for native negative helper children. It is untrusted test input, not filesystem-
inherited pipe privacy or production readiness. Real connected actual-child PID
and strict refusal/cleanup checks remain required; genuine daemon positive uses
the existing secured listener. No widestring/dependency/unsafe/API/clock change.
Apply the amended planning/WINDOWS_NATIVE_CLI_CONTROL_2026-10-04.md four Verify
steps only after Issue31 exact readback and Root plan review; no Source existed
before this detail. Retain actual child/client/runtime/root through confirmed
original-budget cleanup or quarantine, never default-Drop closure success.

### Refine Windows test admission and active driver clock before Source

Replace actual-Owner transfer ABI in planning/WINDOWS_NATIVE_CLI_CONTROL_2026-10-04.md
with no-argument private wake/cancel entries, one empty admitted worker and one
fixed resource-free case command. Partial owner and unchanged real state/daemon
setup live only on that worker; new tighter outer30s bounds setup, original literal
post-spawn5s/8s min still bounds all positive assertions and owned cleanup.
Retain once-published200ms deadline plus on-time completion seal from immutable
origin, independently polled in bounded driver slices without reopening/join.
Negative cleanup observation stays on original30s with admission closed and same
actual owner. Explicit empty/partial/channel/panic/late/unknown cleanup lanes
preserve resources; no native ownership reaches admission closure/error/result.
Apply the amended four Verify steps/three Source paths/six real selectors after
Root exact Issue31 readback and plan review. Preserve Unix/state factory/production/
deps/workflow existing commands and bounds. No hard admission/preemption claim,
renewal/retry/reset/skip, owner-PC effect or unqualified support/lint acceptance.

## Native Windows failure follow-up selected before separate Source (2026-10-04)

Keep frozen SPEC and existing required checks. In planning/WINDOWS_NATIVE_CLI_CONTROL_2026-10-04.md select only .github/workflows/ci.yml and the existing test-private windows_cli_control.rs: split the six existing commands into six independently admitted steps, same !cancelled/native_core-success condition and immediate exit propagation. Keep every selector and failed job. Observe current fixed intent and actual returned IO/status/first-work/ready metadata through checked self-contained scalar words; no real resource or error object reaches admission/driver/channels. All clocks, original operation order, cleanup precedence, actual Child/peer/ACK/run assertions and literal Unix bodies remain. A successful zero-byte ready read joins only the original NotFound continuation; exactbyte1 alone ends setup, other bytes/errors stay failure. No new marker mutation, IO, clock, sleep site or cancel readiness probe. Real controlled empty-window qualification remains pending a later explicit mechanism; current missing controls are not counted as PASS.

In planning/WINDOWS_STOCK_CRASH_HEARTBEATS_2026-10-04.md select only loader_crash.rs and loader_crash_host.ps1. Normal real Rust/PowerShell heartbeats CreateNew once and append21-byte zero-padded sequence/LF records under original30s/25ms; capped2048records/43008bytes with43009read sentinel. Retain the actual ready read File shareREAD|WRITE/noDELETE separately from failure-only observations and seek/read that exact handle under original45s until final settlement, then explicitly release before other controls/TempDir cleanup. Validate every complete sequence and exact unfinished next prefix, compare newly read count+tail_len, never use saved counts for missing/bad input. Native partial-tail/collision/rename/delete/refusal-and-same-operation-after-release controls use actual retained producers and streams within existing helper routes and one original remaining budget. Preserve every old snapshot/publication/observer/tree/pipe/guard gate and21existing helper modes, with no dependency/unsafe/production/workflow change. Three static cached normal/snapshot/append programs preserve whole old SnapshotProof bytes; require full LF/CRLF root+nestedchild commands below32767UTF16 with>=1024 new-program margin before hosted execution.

Root records both plans/Verify in Issue31 and reviews again before two separate disjoint Source leases. CLI development stays in its current PR136 checkout; Core development receives an isolated checkout at the same reviewed Docs head. Root reviews each full patch/protected inventory and integrates only complete returned commits, then publishes one combined changed head for ordinary hosted x64/ARM/MSRV. All original failures and unobserved cleanup remain until the new exact-head required gates genuinely pass; wide Issue31 and Windows11/account/reboot/full-lint/release acceptance remain open.

## Actual bounded managed append-refusal facts before Source (2026-10-04)

Frozen SPEC remains unchanged. Amend the fixture-only stock heartbeat plan before a separate two-file Source handoff at reviewed8110. Keep four fixed slots in original call order: producer_live rename/delete, writer_dead_readers_held rename/delete. Each begins unknown; after its existing Left gate, a pure dispatch-intent flag admits observation only of that existing catch. Preserve booleans, File calls, all gates/order/throws/owners/release continuations. At most5 existing managed nodes retain closed category io/access/other and their own complete signed Int32 HResult, explicit ended/null or fifth-node-truncated flag. No OS decoding, value/type refusal allowlist, message/type-name/path/PID/reflection/query/extra native operation or new error precedence is selected.

Primitive ASCII records join the existing single ToJson -Compress/default-depth result as exactly four strings; Rust requires bounded canonical grammar/i32 values and rejects missing/unknown/malformed facts without converting them to success. Collector failure returns unknown and never replaces original work error; later work failure still aborts without a fabricated final observation receipt. This is independent of actual same-candidate refusal/release,41-byte stream, kill/reap, retained readers, original root/Job/pipe/observer/guard proofs. Only new identifiers/indentation may compact to keep all actual complete root/child LF/CRLF commands below32,767 with at least1,024 margin, old SnapshotProof bytes and commands exact.

Root Docs/Issue31/final plan review precede a separate loader_crash.rs/loader_crash_host.ps1 Source lease. Root then reviews complete two-file diff/inverse/296 other mode/blob pairs, primitive records, Source command models and unchanged21 modes/selectors/deps/workflow/45/30/3s/25/5/200ms. Combined ordinary hosted x64/ARM/MSRV must genuinely observe all four records and every old/new ownership assertion at the same reviewed revision before merge. Static models are zero native acceptance; Issue31 remains open until full Verify succeeds.

## 2026-10-04: Observe existing lifecycle progress results after current-base failures

Implement the [selected lifecycle progress plan](planning/WINDOWS_LIFECYCLE_PROGRESS_OBSERVATIONS_2026-10-04.md) only after Issue31 exact readback and final Root plan review, in a separate developer session. Product SPEC is frozen; no service/publisher/production contract changes. Bind the one-file lease to the integrated clean Docs head based ond894a243619c93dacfdd34e9c7ec5ab068c588be and preserve all other integrated mode/blobs, including nativeb664/stock15ca/main15820.

Keep Fixture::heartbeat and its unrelated callers exact. The private continuation-only wrapper executes the same single read_to_string and, only on success, one u64 parse; it returns the original Option for the original baseline/max and advancement predicates. Descendant retains exactly one is_file followed lazily by read, parse and>0. Every iteration resets last/relation/marker evaluation slots; first-I/O projection persists across continuation baseline→advance. Pass actual stop_role ExitStatus from both existing SSE/idle callers without status queries; label it dashboard only. Observers hold enums/scalars, never error objects/resources after existing handling boundaries.

Append fixed suffix only after each of three original deadline assertions fails, retaining original message prefix/condition and original3s shared baseline+advance deadline,30s descendant deadline,20ms sleeps. No early hard_stop, extra emitter, read/capture/Store/native/Child::try_wait, argv/environment/Stdio/publisher/ownership/Drop/dependency/workflow changes, retry/serialization/cache prewarm/clock reset or lint suppression. Existing synchronous reads and Drop kill/wait are not claimed hard preemptible.

Root reviews full Source inverse, all16 selectors and actual evaluation/call sites, then publishes one changed head for ordinary original x64MSRV/x64stable/ARM lifecycle and downstream gates. Preserve actual failures and unknowns; a later green run proves its original assertions, not an explanation for1c45. Static formatting/metadata/domain models are not native compilation or behavior acceptance.


## Stock Clippy syntax conservation (2026-10-04)

Keep frozen SPEC and all production behavior. Follow WINDOWS_STOCK_CLIPPY_SYNTAX_2026-10-04.md: separate developer first integrates exact reviewed main814e689bb0717aba183dbd084e6151ba11ac4e99 with native3f5 and this Docs parent, preserving the independently selected inherited Source and literal Docs appendices. Then only loader_crash.rs receives the constant-chunk loop plus borrowed slice binding and the one-call collision let-else. Keep the original partial-tail acceptance, error order, constants, actual children/readers/guard/publication/mutation controls, command sizes, clocks and success/failure predicates; add no warning allowance, test/filter/dependency/clock/retry change. Root reviews the complete inverse/full tree and publishes ordinary changed-head hosted CI. Fresh Windows lint/MSRV and existing stock proofs must pass; native readiness/withheld failures stay failures until separately researched and selected, and broad Issue31 remains open.

## PR139 shared production controls before separate Source (2026-10-04)

Frozen SPEC and the existing dashboard completion contract remain unchanged. Select the exact appendix in planning/DASHBOARD_LISTENER_LIFECYCLE.md after Root review of the47-file design. Only crates/locron-server/src/lib.rs may change. Public bind delegates its existing parsing/loopback/range/fallback implementation through a private generic SocketAddr binder; public serve_until retains token/router/real Axum tasks and calls a private owning supervisor containing the same original select/broadcast/10s-drain/abort-all/full-join body. The existing Windows dashboard_ctrl_c stays byte-exact. No public hook, boxed callback/dependency/new duration/test-util/Store/CLI/policy/workflow change.

Add11 meaningfully controlled tests in that same private lib module: three bind cases and eight supervisor cases. Real per-case task/channel/Drop ownership and original shutdown receiver path verify notification, held-survivor non-completion, all joins, first-error retention and unchanged TimedOut precedence; one real10s expiry stays under a13s fixture guard. Preserve all31 old native cases, actual fixed owned-port conflict, active SSE/stalled contracts. Root Docs/Issue30 Verify/final review precede a separate one-file Source lease; full extraction inverses/all other modes-blobs/rules/static checks precede changed-head ordinary hosted x64 stable/ARM stable/x64 Rust1.94 and Windows warnings-denied lint. Planned42 native Server cases is inventory only until all11 named new cases actually pass without new skip/ignore. Broader detached/started-Store/account/install/release acceptance remains open.
## PR139 elapsed-payload lint repair before separate Source (2026-10-04)

The existing frozen SPEC is unchanged. Follow the measured-failure appendix in planning/DASHBOARD_LISTENER_LIFECYCLE.md. Select only the new test helper's outer timeout match in crates/locron-server/src/lib.rs: change the Err(_) panic arm to Err(error) and include : {error} after the original panic text. Preserve the actual abort/reap/Drop ordering before this arm and every other Source byte apart from rustfmt of this arm. Root Docs/Issue30/final plan review precede a separate developer lease; Root then reviews the full one-arm inverse and all other mode/blob conservation before one ordinary changed-head push. Fresh native and all warnings-denied lint outcomes are required before merge; the previous failed head remains failed.

## PR136 review follow-up before separate Source (2026-10-04)

Freeze product SPEC and integrate reviewed main814e689 without Source edits.
The native CLI and stock plans select a two-file lease at clean2e6c0cd:
windows_cli_control.rs privately stages/flushes/closes one owned marker before
single no-clobber publication, adds actual absence/completion/collision controls
inside existing actual peer children, and adds fixed existing metadata only on
withheld notice failure. loader_crash.rs changes only complete-chunk iteration
and collision let-else syntax for required strict lint. All other Source remains
exact, including cli.rs, lifecycle, workflow, PowerShell and all original owners,
read errors, clocks, selectors, old/new stock proofs and complete commands.

Four concrete Verify steps in the native plan require Docs/Issue31 readback
before separate development, whole two-file inverse/protected inventory,
complete Root Source/rules/static review and fresh exact-head ordinary native/
lint/paired qualification. Actual ReadyOpen raw32 is preserved as evidence;
the identified incompatible-sharing Source window does not prove which internal
call produced the old result. Withheld Disconnected stays unobserved until
already available metadata establishes more. Failure diagnostics never add
native I/O, delay, mutable authority or late success. No unchanged rerun,
deadline relaxation, ignored test/lint, owner-PC native effect or public release.


## PR136 concurrent ancestry integration (2026-10-04)

Before Source integration, select a normal merge of remote b131 into local062aa2 plus this Docs parent. Conserve both append-only planning histories. Resolve the stock syntax overlap to the exact complete remote loader_crash.rs blob; conserve the complete local ready helper and all other Source/workflow/lockfile mode/blobs. The developer owns integration, reports full-tree conservation and formatting/metadata checks, and does not publish. Parent rechecks current remote ancestry and publishes one fast-forward head, then requires fresh ordinary CI, native six-case and lifecycle evidence plus exact paired artifacts before merge. Issue31 remains open for wider unresolved acceptance.


## Lifecycle observer lint follow-up before separate Source (2026-10-04)

Frozen SPEC. After normal reviewed integration78af945, lease only the single parse match in LifecycleProgress::read_heartbeat, as documented in WINDOWS_LIFECYCLE_PROGRESS_OBSERVATIONS_2026-10-04.md. Use if let Ok(value) ... else with identical branch bodies, one parse call and unchanged observer/formatter/assertions/16 selectors/clocks. Root Docs and Issue31 readback precede the separate one-file developer; full inverse/all other mode-blobs and formatting/actionlint/locked metadata precede parent review and ordinary changed-head publication. All original native/lifecycle and required Windows lint gates remain required; no allowance, filter, clock change, owner-PC native execution or earlier-run cause is selected.


## Windows test-private readiness selection after b131 (2026-10-04)

Follow docs/planning/WINDOWS_NATIVE_PEER_PUBLICATION_2026-10-04.md on exactb131.
Three Source selections: test-private staged publisher+genuine seventh control+
separate notice failure snapshot; one additive original-matrix CI command; and
one lifecycle parse syntax site. Preserve original controller non-NotFound error
refusal, six commands/owners/3s-30s-200ms-5ms clocks, complete main139/stock and
all other mode/blob paths. Root recomputed notice bound654 including separator.
No new transport/dependency/native binding/production or timing waiver. Root
Docs/Issue31/final review precede the separate development lease, then immutable
Source review and genuinely changed-head hosted verification. Failed gates stay
held; wide installer/account/logon/reboot/public release acceptance is pending.

## Before-integration Root selection, 2026-10-04 (native73)

The complete ordered plan is [WINDOWS_NATIVE_CONCURRENT_INTEGRATION_2026-10-04.md](planning/WINDOWS_NATIVE_CONCURRENT_INTEGRATION_2026-10-04.md). SPEC stays frozen. Exact incoming mainc4/treef90 replaces the previous main814/prospective140 selection only within this plan. Parent Root handles Docs/Issues/final review/publication; separate development handles Git integration of already reviewed whole Source blobs.

First normally merge exact73 while retaining whole89 helper/workflow and identical1160589 lifecycle, with all five common+incoming+own Root Docs unions. Then normally merge exactc4; retain complete first-merge Docs and append only the exact new140 common-document suffix once, without duplicating139. Incoming cleanup-enabled/per-mode collision and queued-result2048 choices are historical and superseded by disabledDrop/seventh-control/fixed-only654 selections.

Verify before development: clean exact Source parent, Docs-only mode/blob diff, exact Issue readback and complete final plan reread. Verify integration: explicit parent OIDs, complete Source/Docs/rules/tree conservation, staged+unstaged inspection and permitted static checks; unexpected conflicts return to Root. Verify publication: Root full review, fresh remote ancestry and main pin, one ordinary FF. Verify acceptance: genuine changed-head hosted native rows and all original required gates; prior PASS, optional SKIP and unknown causes remain at their actual scope. Broad release/catalog/install/account/logon/reboot work stays open.

## PR140 finite output controls before separate development (2026-10-04)

Frozen SPEC is unchanged. Follow planning/WINGET_OUTPUT_RECOVERY_CONTROLS_2026-10-04.md. First compose reviewed oldPR140 and main137 through a recorded main merge; only renderer main() can be reconciled, exactly old140 pre-main plus merged137 suffix. A separate developer then adds scripts/test-winget-output.py and additive windows-paired-manifest workflow controls. Other inherited main137 paths remain exact; no renderer/writer redesign, dependency, package/catalog/installer/updater/native Locron effect is selected.

Use real file streams/IDs and one-shot stdlib boundary injections with actual underlying close, fixed small Unicode/LF documents, per-case ownership and explicit controlled second calls. Native Windows rows require actual platform/Python/runner architecture and nonzero file identities with no skip; register read-only attribute restoration before mutation and verify the exact test-owned file before changing it. The original Ubuntu workflow commands/timeout/permission/concurrency and all old tests stay exact; independent5-minute Windows x64/ARM rows run14 methods/38 controls and old entrypoint8. Outer job timeout does not preempt a synchronous native file call. Root Docs/Issues/final review precede Source; whole Source/inherited union/reverse workflow/rules checks precede one reviewed-head automatic CI publication. Public release/WinGet/clean-account requirements stay open.


## WinGet output controls: reviewed Source then main139 (2026-10-04)

Sourceaf824 was Root-reviewed, not executed. Its direct real-stream writer tests
and10/32 portable,14/38 native selections remain exact. Follow the appended
WINGET_OUTPUT_RECOVERY_CONTROLS_2026-10-04.md Git-only integration plan: exact
reviewed814, common15820, full incoming/own mode/blob union and both literal Docs
suffixes. No writer/test/workflow adaptation or newer main. Root Docs and CLI
Issues32/35/final review precede separate integration, Root Source review and
fresh Windows/Ubuntu CI. Clean-account/public-byte/real WinGet acceptance stays
open, signing37 deferred and initial release unsigned.


### PR136 continuation: owned bounded CLI output capture

The Windows acceptance helper retains each actual Child and a finite fresh private capture/guard, polls actual exit under the unchanged effective clock, then reads the same actual file identity through an independent no-follow reader with64KiB+1 cap. It preserves complete bounded stdout/status; no pipe EOF wait or shared-cursor seek. A separate additive hosted gate proves real held-writer/producer/collision/cap/live-refusal/kill-reap cleanup under its own bounded controls. Parent plans/reviews/publishes; a separate developer leases only the helper and additive CI step after exact Issue31 readback. Preserve73d3 source, original six cases/deadlines, unknown failed-head causes and wider open acceptance.


### Root capture plus genuine publication integration before Source (2026-10-04)

Follow planning/WINDOWS_NATIVE_CAPTURE_PUBLICATION_2026-10-04.md: preserve
reviewed222 private publication and fixed notice, add only734 owned-output
capture and its genuine independent gate, and integrate exact actual main9de596f8
without adapting its release/dashboard/WinGet Source. Fresh all-three-row
24-selector outcomes and all required gates must qualify the resulting head.

## Before-integration Root selection, 2026-10-04 (pr138)

The complete ordered plan is [RELEASE_PUBLICATION_MAIN140_2026-10-04.md](planning/RELEASE_PUBLICATION_MAIN140_2026-10-04.md). SPEC stays frozen. Exact incoming mainc4/treef90 replaces the previous main814/prospective140 selection only within this plan. Parent Root handles Docs/Issues/final review/publication; separate development handles Git integration of already reviewed whole Source blobs.

Retain all five exact PR138 snapshot paths and import reviewed main139/140 without Source adaptation; shared Docs are common15820 + complete incomingc4 suffix + complete new Root suffix.

Verify before development: clean exact Source parent, Docs-only mode/blob diff, exact Issue readback and complete final plan reread. Verify integration: explicit parent OIDs, complete Source/Docs/rules/tree conservation, staged+unstaged inspection and permitted static checks; unexpected conflicts return to Root. Verify publication: Root full review, fresh remote ancestry and main pin, one ordinary FF. Verify acceptance: genuine changed-head hosted native rows and all original required gates; prior PASS, optional SKIP and unknown causes remain at their actual scope. Broad release/catalog/install/account/logon/reboot work stays open.

## PR136 current-call scalar observation handoff (2026-10-04)

A separate developer leases only the private Windows CLI control helper after Issue31 exact GET and parent complete selected-plan reread. Ten fixed atomic words distinguish actual CLI attempts/returns, native calls, current wait/read, successful History wall time and last strict progress result; zero/overflow/invalid are diagnostics only. Reuse existing three actual capture controls for reset/retention/association assertions. Conserve original six controls, clocks, ownership, legacy formatter, Source calls and workflow exactly; renderer/caller CRLF bound is2048bytes. All actual native/type/lint gates and paired provenance remain fresh changed-head requirements. Four concrete Verify steps and full boundaries remain in the selected planning appendix.


## Current PR136 observer and concurrent publication integration

Root selects the exact disjoint observer4639 and concurrent6e98 contributions in [the completed integration plan](planning/WINDOWS_NATIVE_OBSERVER_CONCURRENT_2026-10-04.md). Preserve current main9de and all original gates; actual new observer qualification is pending and the concurrent x64 ChildExited/Run failure remains unexplained.

## Separate output repair qualification development (2026-10-04)

Follow planning/OUTPUT_REPAIR_QUALIFICATION.md in Store-then-Engine slices, with concrete Verify on all four selected steps and the original five-step research plan. Root freezes the private shared parser/opened-file seam and finite 96-key union before Issue #146 exact GET and complete plan reread. Separate Source development preserves public signatures, all writer and old-test bytes, native guards, policy differences and original clocks. Root handles full review, exact current-main Source union, normal publication and actual hosted qualification. No new public test API, dependency or workflow is selected.


## PR147 reviewed owner Drop-order conservation

Root found the shared repair wrapper destructures File and DirectoryGuard into reverse-dropped local bindings. Keep the exact returned pair as one tuple owner and borrow field0 so File still drops before its parent guards. The completed appendix in planning/OUTPUT_REPAIR_QUALIFICATION.md confines the separate two-statement correction; all functional controls, clocks, API and native claims remain unchanged.

## PR147 separate Git integration with reviewed main136

SPEC remains frozen. [The complete five-step plan](planning/OUTPUT_REPAIR_MAIN136_2026-10-04.md) selects exact normal Source union and whole incoming Docs plus all ordered byte-exact own insertion blocks; whole own/incoming inverses are required. Root exact Issue146 GET/final reread precede separate development; full original gates and actual functional/paired provenance qualify the combined head.

## Store147 selected receiver correction (2026-10-05)

Apply only the actual Clippy receiver autoref suggestion under planning/OUTPUT_REPAIR_READ_RECEIVER_2026-10-05.md after exact Issue146 readback and Root final reread. Preserve complete parent inverse, fixed47/48 controls, errors, guards and every old gate/clock; separate Source implementation and Root review precede normal publication/fresh hosted acceptance. This syntax change supplies no repair claim for unrelated Windows wake/Ack/GUI failures.


## Measured caller branch lint (2026-10-05)

Follow OUTPUT_CALLER_BRANCH_ORDER_2026-10-05.md under Issue146: Docs/Issue exact readback and whole final-plan reread before a separate one-block maintenance.rs test-only branch exchange; whole parent inverse and both literal branch bodies, protected mode/blob inventory,47/48 ledger, all clocks/assertions/cleanup and static fmt/diff evidence; then Root full review, normal publication and actual changed-head hosted/Guardian/native/paired qualification. All other Source and broad acceptance remain unchanged.

## Current grouped processing with arrivals

Follow GROUPED_PR_ARRIVALS_2026-10-05.md under Issue163 after the initialda7 grouped research plan. Preserve nineteen current open PRs in five groups and both parallel local/published contributions; retain all old actual evidence at its measured head. Research the newer141/166/167 and dependency168/169/170 before group-specific final Docs/Issue/GET/parent reread and separate Source. Complete composed-head/current-base regression, strict ordinary/Guardian/native/paired qualification plus exact squash/main/member-contribution proof precede member completion. This arrival update selects no Source, product behavior or expanded clock.

## Group B selected output/prune/state composition — 2026-10-05

After owning146/149/152/163 exact readbacks and Root entire final plan/ledger after-GET reread, use a separate developer to normally conserve all six contributions and adapt only the nine selected paths/actual formatting sites. Return complete original-body/contributor/Docs/key/physical/static proofs and leave fresh hosted94/96/state/native17/Guardian/full paired/current-base/squash/member proof to Root. Preserve every original policy/clock/owner/first error; no local runtime or old-head success promotion.
