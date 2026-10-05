# Native process-token SID and common Windows qualification — 2026-10-05

## Selected outcome and evidence

The owner requests fixing the remaining failures and finishing the open PRs. Continue
existing privacy #27, CI #31 and integration #163. Frozen SPEC/ARCHITECTURE remain
unchanged: this replaces an internal identity query, retaining the same process-account
identity, privacy, cancellation, public output and deadline contracts. Signing #37 stays
deferred and nonblocking. This common repair is published against main before unrelated
representative PRs integrate it; older #141 deferral does not prevent the current requested
review and repair. Wider release/account/logon/reboot acceptance remains in its owners.

Source baseline is main `f90a9d5dc2e6bafbc163567b333c398777647402`, tree
`e9dd9431603eb7900ee1e6ee2d63144be0d040fb`. The complete two selected Core files
are mode/blob/byte equal to researched PR166 `17133aa1917fc50d2801bf58c33552e20f978d30`:
windows.rs blob `9f9a7de2fdd87e832b00ac365303a29fed82e240`, SHA256
`da710e8e7d26a5ada8cb0d3f4deb6376b407dd7e280fc247edb4d53105426ed9`; filesystem_worker.rs
blob `408b93b82fad2c3d32a7c3fb5755b99e29cb519a`, SHA256
`6ba6df75658601b12d07f3d78d9fea1eb0ceac44decdfec8370b92c872013ad4`.

Separate read-only research report SHA256
`a413614f30336a560cf097e8da24b4a0ad4193fc495924c55f12628d31a6c2d7`
and its 59-artifact manifest SHA256
`5c762b0a4f76314af51d0558e7f4442788981be2e1a60061e83f4eb1e8ed03fb`
resolve API, identity, ownership and existing-state caller questions. Actual PR166
run37305633954 MSRV observes Cancelled with only9.464ms remaining before the required
25ms stop pause and second read. Successful History wrappers total5.901301s there and
4.301380s on stable. These complete wrapper costs overlap the native5s grace; they are
not SQLite/native-query timings or proof of the sole cause. ARM raw32 before Cancel,
late positive-capture refusal, and #141/#147 guarded input failures remain separate.

Every fresh History CLI starts with an empty process-local SID cache. Its explicit-state
existing-root/outputs/tmp/DB/WAL/SHM route otherwise uses native guards and SQLite;
current_user_sid is the source-proven cold PowerShell dependency. Missing directories
or sidecars still require the existing atomic private creation adapter. Default state
discovery still has its independent KnownFolder script. Actual historical leaf presence,
latency saved, native grace start, Job-empty and durable completion are unmeasured.

## Identity and finite ownership

Use already locked Windows-only windows-permissions=0.2.4 safe
utilities::current_process_sid, fallible wrappers::ConvertSidToStringSid, fallible
OsString UTF-8 conversion and the literal existing S-1-/numeric-component validation.
The checksum-qualified crate archive SHA256 is
`9e2ccdc3c6bf4d4a094e031b63fadd08d8e42abd259940eb8aa5fdc09d4bf9be`;
eight relevant cached members match it. The process-token identity matches the ordinary
existing stock child, including parent-thread impersonation; no username/environment/
caller identity, Sid Display/expect, raw FFI, dependency or fallback is selected.

Keep current_user_sid, current_user_sid_until, cached-only IPC, OnceLock and the
existing single SID_INITIALIZER. Refine only the private cache-query seam so its actual
owned permit moves into the native query owner before work. Global production lifetime
is static; immediate existing test closures can retain a borrowed local permit lifetime.
No extra pool or global detached-resource registry is selected.

One admitted std thread owns token query, conversion, validation and native disposal.
After every native object is disposed, its finite single-result channel transfers
io::Result<String> together with the SAME scalar initializer permit, publishing without
blocking on an absent receiver. A private transport Result distinguishes refusal before
a reply from the replied native Result; immediate test closures can return their original
borrowed permit, while production transfers its static permit. Retain the original absolute API-entry deadline, capped
at30s, across admission, pre/post native stages, receive, cache publication and return.
Only the caller can cache an on-time verified result. A native call is not preemptible:
on timeout, refuse without joining an unfinished owner or releasing its permit. Late
work cannot cache or authorize replacement work. The permit stays with the owner during
native work/disposal, then with the queued reply or receiving caller through cache/refusal
and return. Only already-native-disposed scalar/channel state can drop on the caller.
Receiver drop or try_send refusal releases that returned permit once; a blocked owner
keeps it until actual disposal. This closes the worker-exit-before-cache duplicate-query
window without an extra pool, atomic handshake, polling or native resource in the reply.
Subsequent admission remains bounded. No child
is spawned for SID; stock filesystem plus generic/COM child ceilings stay unchanged.

Native query/conversion failure, invalid UTF-8/grammar, thread creation failure, panic/
disconnect or pre-publication expiry refuses explicitly without initializing an empty
cache. Owner, receiver and caller deadline gates reject a late reply before publication.
A verified entry already present or published after the pre-set admission check remains
if a later post-publication or outer-return check refuses. The clock check and OnceLock
publication are not atomic; expiry between them can refuse with a verified entry retained.
No rollback or clearing of a verified entry is selected. A later independent call can
retry after actual owner release; no automatic replay or PowerShell fallback.
Thread-creation refusal drops only scalar/channel/permit state, with no acquired token.
Keep native errors without printing SID/path/username or the failed OsString. Library
internal allocation/retry/CloseHandle/LocalFree and panic/OOM are not interruptibility
or cleanup-timing guarantees. Existing stock30s/3s cleanup applies to its actual children.

## Source and test boundary

Separate development owns only:

- crates/locron-core/src/windows.rs: private permit transfer, finite native SID query and
  meaningful ownership/error/cache controls. Preserve existing cache test names, results
  and clocks while adapting only their private closure seam as required.
- crates/locron-core/src/windows/filesystem_worker.rs: phase_order_fixture's implicit
  current_user_sid startup becomes its existing private request("sid", None, original
  deadline), plus a scoped actual native-vs-stock SID equality control without printing
  SID values. Preserve ordered phases and every real cold/creation/EOF/Job/parent-exit
  owner, helper route, assertion and original deadline.

No filesystem policy/creation, Store, CLI, Engine/grace, native helper counter oracle,
workflow, dependency/lockfile, public schema/receipt/selector or Unix change. The new
native-owner fixtures may gate a query provider to test driver behavior; they do not
claim a Win32 API was forced to hang. New decisions or wider Source return to Docs first.

Owner-PC source/runtime/compiler/native/test/fixture/PowerShell/parser/ACL/account/task/
PATH/policy/install/reboot effects remain unselected. Allowed static checks are pinned
standalone rustfmt1.94/1.98, locked offline Cargo metadata and Git/hash/stdlib text proof.
Actual behavior executes on disposable hosted Windows CI. Preserve5s Wake/8s Cancel/
whole30s/25ms stop/200ms probe and production5s grace exactly; no warm-up, clock reset,
accepted error, skip, oracle waiver, unchanged-head rerun or manual dispatch.

## Ordered work and concrete Verify

1. Root freezes research, Docs and existing owning issues before Source. **Verify:**
   independently check all59 artifacts and both complete Source equality bindings;
   Docs-only exact diff preserves frozen SPEC/ARCHITECTURE and original main tree outside
   the selected Docs. #27/#31/#163 contain this entire plan by exact POST/GET, original
   bodies/state retained. After ALL GETs Root rereads the whole final committed plan and
   every new Findings/Implementation/Issues block before separate development.
2. Separate development implements only the two-file scope and returns a clean commit.
   **Verify:** full staged/unstaged/committed patch, protected whole modes/blobs and old
   region inverses, unchanged dependencies/guards/creation/Stock/Store/Engine/helper/
   workflows/clocks; pinned formatting/static metadata as applicable. Hosted controls
   preserve every old cache selector and prove timeout while the owner remains pending,
   second admission refusal, no late cache, native disposal before scalar permit transfer,
   retention through cache publication, release before later reuse, on-time once-sharing,
   error/disconnect/spawn-refusal lanes and cached-only noninitialization. Gated providers
   prove the ownership boundary, not real native duration. Native and type fit are pending.
3. Root independently reviews and publishes the genuinely changed main-based PR.
   **Verify:** exact clean parent/head/main, full two-file/Docs scope and preserved rules;
   one ordinary push, attached PR, actual ordered synthetic merge parents and equal tree.
   Reuse frozen completed logs; acquire new completed-job raw logs only once each and
   retain raw plus declared CSI-only view. No successful job metadata replaces behavior.
4. Qualify and integrate the common fix, then independently qualify existing groups.
   **Verify:** all three native Windows rows pass actual process-token/explicit stock SID
   identity, original cold phases/queue/creation/EOF/containment, old cache/new owner,
   unchanged Wake/Cancel/capture/progress-stop/cleanup and full required MSRV/lint/Unix/
   package gates; genuine GitGuardian and complete paired provenance/PE/ABI/byte proof
   for the exact head/base as triggered. Record actual measured wrapper costs where
   admitted, with unobserved native phases explicit. Merge only qualified current source;
   every representative/member retains its own contribution/Verify before closure.

Publication is not completed Windows support. Keep wider #23/#25–#36 and signing#37 at
their actual state; #27/#31 stay open for remaining broad acceptance. A remaining native
failure returns to source-backed research under the same clocks and ownership contracts.

## Hosted lint and coupled cold-SID correction (2026-10-06)

PR171 exact Source b4c422a1cc6d9f5f3ac026aa4213bc02e39d9a57 was published against
mainf90. Ordinary CI37340588591/attempt1 Windows lint111866362894 completed FAILURE:
Rust1.98 reports needless_pass_by_value at windows.rs722 on the owned receive_sid_reply
Receiver; the retained CSI-only log lines405–419 ends with one prior lib error and exit1.
Raw SHA256 c59cd588606cfb64fd69926d3ce8ef46c7afeeee5addd6ed2ab058f46e36a804;
CSI-only SHA256 75de252383d17f285b078c849dbbad28d2df4def822dfbcb91b8ded7639b2d7c.
Root acquired those bytes once. This is actual lint evidence, not a runtime failure.

Completed snapshot3 reports19 jobs:13 ordinary metadataPASS,4FAIL (three Core rows and
lint),2 optionalSKIP. Root retains all17 completed raw job logs, each acquired once.
Windows package/coherence metadata is PASS, with no Root ZIP byte proof yet; this is
not package/install/release acceptance. Original eight CLI controls across three rows,
including Wake/Cancel, remain unrun after the failed foundation cold gate.

The completed ARM111866362649, x64stable111866362684 and x64MSRV111866362789
rows each report Core136PASS/1FAIL, zero ignored/filtered. All six new SID selectors
pass, but existing actual_cold_sid_preserves_the_forwarded_qualification_deadline fails:
the isolated sid-deadline child reaches loader_tests.rs181's original pid>0 assertion.
Its native SID call, grammar and elapsed check precede that failure; the new native path
does not implicitly start the stock worker. The fixed marker is unreached, the cold gate
fails and CLI/native downstream acceptance is unrun. This is not proof Cancel is fixed.

The original owned receiver lifetime remains selected. Add ONLY drop(receiver) after
the original remaining(deadline)? and immediately before the existing final reply
expression in receive_sid_reply. Keep its owned signature, receive call, post-deadline
gate, returned result/error precedence and every early-error RAII path literal. This
explicitly consumes the same scalar receiver at the existing exit; the returned reply
still owns the SAME permit. No borrowed API/lifetime, native disposal, clock, fallback,
warning allowance, new test/selector, dependency or owner policy changes.

Extend the earlier cumulative two-file repair boundary only with cfg(test)
crates/locron-core/src/windows/loader_tests.rs. In ONLY its existing sid-deadline branch,
keep the native current_user_sid_until(deadline), start, original20ms sleep, ADAPTER_TIMEOUT,
deadline and SID grammar literal. Immediately after native SID/grammar, assert the
Boolean observed_pid()==0, then perform genuine existing filesystem_worker::request(
"sid", None, SAME deadline) and fixed Boolean native==stock comparison without rendering
either SID. Keep the original elapsed<ADAPTER_TIMEOUT, pid>0, marker and hostile-module
isolated helper scope after that addition. Both operations share the original30s boundary;
no clock reset, warm-up, stock substitute for the native API, skip or assertion waiver.
All other loader_tests branches and filesystem_worker.rs remain byte-exact b4c422.

A read-only host audit identifies a second direct native-SID-to-stock-PID coupling in
cfg(test) crates/locron-core/src/windows/loader_crash.rs1353-1388. The parent-exit-host
route's fixed thread currently calls current_user_sid(), checks grammar, then requires a
positive filesystem-worker PID and publishes fixed-pid. Native SID does not start that
worker. This is Source-backed prospective fixture evidence; b4c422's failed cold gate
skipped parent-exit-driver, so no second runtime failure or containment result was measured.

In ONLY host()'s existing fixed thread, capture one Instant::now()+ADAPTER_TIMEOUT at its
original native-query entry. Use the existing current_user_sid_until(deadline) rather than
the default wrapper so its native query and the following genuine existing
filesystem_worker::request("sid", None, SAME deadline) share that original30s boundary.
Retain the original SID grammar, then require fixed Boolean native==stock without printing
either value BEFORE the original positive PID assertion and fixed-pid publication. Never
create/reset a deadline after native lookup. Keep the generic thread's separate original
deadline, prepare/capture/spawn, joins and all other host bytes literal. Parent/observer
owned Child and Job containment, three actual target handles, heartbeat progress/stop,
reader/capture/disposal, publication proofs, guard refusal/release and all original
assertions remain literal. The driver45s, stock30s and existing3s cleanup stay unchanged.
No new thread, selector, warm-up, native substitute, clock reset or assertion waiver.

This latest amendment explicitly supersedes the prior cumulative three-file boundary:
cumulative Source is windows.rs, filesystem_worker.rs, loader_tests.rs and loader_crash.rs.
The follow-up changes only windows.rs, loader_tests.rs and loader_crash.rs;
filesystem_worker.rs stays byte-exact b4c422. No further Source or test scope is selected.

Root read the [stable Clippy manual](https://rust-lang.github.io/rust-clippy/stable/index.html?groups=cargo#needless_pass_by_value):
the lint detects an unconsumed by-value argument; its reference suggestion is not the
selected ownership policy. The log's versioned Rust1.98 reference page was not read.
Stable manual semantics and static formatting are not Rust1.98/native acceptance.

1. Record and reread the complete amended plan before Source. **Verify:** four Docs only,
   complete earlier prefixes retained, b4c422 Source unchanged; #27/#31/#163 whole-plan
   exact POST/GET retains original bodies/state, then Root rereads the whole committed
   plan and all new Docs AFTER ALL GETs before returning the separate Source lease.
2. Apply the owned-receiver drop and both coupled stock-SID fixture repairs. **Verify:** inspect
   full staged/unstaged/committed diff; removing that one drop line restores windows.rs,
   removing the new native/stock observation block restores loader_tests.rs, and removing
   only the fixed-thread deadline/request/Boolean addition plus restoring its default SID
   call restores loader_crash.rs. Each whole file must equal b4c422. Independently count
   the new Docs-parent tree:316 entries minus these three follow-up paths protects313
   modes/blobs, including every filesystem_worker.rs byte. All original selectors, old
   assertions, clocks, branches and other host/loader bytes stay exact. Scoped pinned
   rustfmt1.94/1.98, locked offline metadata and diff checks only locally; native
   compilation/Clippy/tests are NOT RUN.
3. Root reviews and publishes changed Source, then qualifies actual hosted results.
   **Verify:** clean exact parent/head/base, one ordinary push and exact synthetic tree;
   fresh Windows warnings-denied lint plus all three old/new native identity/cache/
   ownership, actual cold forwarded deadline/stock PID and the genuine parent-exit
   handles/Job/heartbeats/capture/guard-release proofs plus original required gates must
   pass. Preserve failed b4c422 lint and three cold-gate failures as measured history;
   no unchanged-head rerun, prior PASS transfer, skip, allowance or relaxed clock.

## Hosted lib-test syntax correction (2026-10-06)

Actual d89cd3e122d2de9aa2703fded3ac8027962e4715 CI37346840859/attempt1 Windows lint
111887419929 completed FAILURE. Its one-GET CSI-only lines422-459 report ONLY two
locron-core lib-test errors: manual_assert_eq at loader_tests.rs179's new zero-PID
assertion, and elidable_lifetime_names at windows.rs812-815's immediate_sid_reply helper.
Earlier production receiver lint is no longer reported in the checked lib stage419-421.
Raw SHA256 cf1660737a05e9d5fba7f4a049624eb4b6ab087ddfafc2f86f4870eb34ac416a;
CSI-only SHA256 e23573a6b5e040add78cf1aee878caf6af7d80b843fc244c6c2d6576ac55b2ec.
Root snapshot2 reports10 ordinary metadataPASS/1FAIL/2optionalSKIP/5pending; the three
native and two package jobs are pending in that snapshot. These are lint observations,
not native SID/containment/Wake/Cancel/package acceptance. Preserve both earlier failed
runs and their actual unrun scopes; no earlier PASS transfers to this changed head.
Root's later snapshot3 metadata shows all three rows progressed through Core/stock modes
into registered service semantics. Stable/MSRV have no failed step at that snapshot;
ARM's native CLI cancellation-ownership step is marked failure while jobs are running.
Completed raw qualification of actual137/native-selector/Cancel outcomes is not available
in this draft. ARM cause remains UNKNOWN; step progress is not complete runtime PASS.

Select ONLY loader_tests.rs's new zero-PID assertion syntax as
assert_eq!(super::filesystem_worker::observed_pid(), 0). Its predicate remains identical;
only integer PID values can enter the failure message, never SID/path values. Keep all
native-first, genuine same-deadline stock request, fixed Boolean identity and original
elapsed/positive-PID/marker statements literal. No assertion is removed or relaxed.

Select ONLY the cfg(test) immediate_sid_reply signature in windows.rs: remove the <'a>
binder, replace the input with WorkerPermit<'_> and the output with SidReply<'_>. Its
(query(), permit) body and every caller remain literal. The one input lifetime still
binds the returned SAME borrowed permit under the Rust Reference's single-input elision
rule; no 'static, borrowing API change, lifetime widening or production policy change.
Root read the [stable manual's assertion rule](https://rust-lang.github.io/rust-clippy/stable/index.html?groups=cargo#manual_assert_eq),
[stable lifetime rule](https://rust-lang.github.io/rust-clippy/stable/index.html?groups=cargo#elidable_lifetime_names)
and [Rust Reference lifetime elision](https://doc.rust-lang.org/reference/lifetime-elision.html).
These are primary semantics references, not Rust1.98 type/native proof; no claim is made
that the log's versioned1.98 manual pages were read. Actual type/Clippy fit remains hosted.

This latest follow-up changes only windows.rs and loader_tests.rs within the unchanged
cumulative four-file boundary. Filesystem_worker.rs and loader_crash.rs remain byte-exact
d89cd3e1, including the generic separate deadline and all parent-exit containment/capture/
heartbeats/disposal. Preserve every clock, selector, old positive PID, privacy, cache,
owner/permit, returned-error gate and control. No allowance, new test, dependency, public
API, workflow, fallback, skip, retry, clock growth or unchanged-head rerun is selected.

1. Root records the complete amended plan before Source. **Verify:** exactly four append-only
   Docs, all original prefixes and d89cd3e1 Source exact; owning #27/#31/#163 whole-plan
   POST/exact GET retains original bodies/state. Root then actually rereads the whole
   final committed plan and new Docs AFTER ALL GETs before separate development resumes.
2. Apply only these syntax replacements. **Verify:** full staged/unstaged/committed diff;
   reversing the one assertion replacement and three signature substitutions restores
   both complete files to d89cd3e1. Independently verify316 Docs-parent entries and314
   protected modes/blobs. Scoped rustfmt1.94/1.98, locked offline metadata/diff locally;
   native compilation/Clippy/runtime are NOT RUN on the owner PC.
3. Root reviews/publishes the genuinely changed head. **Verify:** clean exact parents/tree,
   fresh warnings-denied Windows lint and all three original/new SID, cold/PID/containment,
   unchanged downstream and required gates qualify actual results. Preserve failed/optional/
   pending/unrun scope explicitly; no old PASS/count, Cancel repair or support claim.

## Join reviewed Group D before the next common qualification (2026-10-06)

This amendment supersedes ONLY the earlier native-alone qualification/merge-first order
for already reviewed Group D. Integrate exact PR166/D
`17133aa1917fc50d2801bf58c33552e20f978d30` into the gated common SID/style line before
its next qualification and merge. Other guard groups A/B/C/E remain later. Frozen SPEC
and ARCHITECTURE stay unchanged. No new observation seam, product repair, raw output,
public API, test, clock, fallback or security waiver is selected by this amendment.

The current clean Source head is `6164815869a02942d6bb348b93b2d407282c6759`, tree
`78f7439ce0901119718e443dc8eebd312677fb97`. Root reviewed the complete two-style patch,
46-artifact packet, raw/canonical inverses and314 protected entries of316 without a
finding. Its static fit is not hosted lint/native acceptance. Exact D17133 has tree
`0af46c07a6b32730f23d4da9ccba1992dbdd7e37`; mainf90 is its ancestor. D's complete
mainf90 delta is26 paths, nine Source and17 Docs. All nine Source paths are disjoint from
this common repair's four cumulative Core Source paths. Root's complete prior D review
is reused only for those immutable bytes, not a current runtime result. A separate
read-only D audit reports no concrete new SID-to-stock-PID fixture coupling; original
cold/stock controls stay unchanged. No additional coupling repair is selected.

Completed d89cd3e1 CI37346840859/attempt1 remains FAILURE:15 ordinary metadataPASS,
2FAIL and2 optionalSKIP. All three rows actually pass Core137, all six new SID selectors,
the forwarded cold deadline and isolated EOF/restricted/parent-exit proofs. Engine69,
Server42, Store96, doctor6, service64 and GUI12 pass in all three rows. The original
8x3 native CLI matrix is23PASS/1FAIL, with zero ignored: both x64 rows pass all eight;
ARM fails only original Cancel. Both x64 downstream rows pass lifecycle16/dashboard4/
prune3/maintenance11/MCP4/HTTP21; those six ARM commands are skipped after Cancel fails.
Windows test-Clippy separately fails manual_assert_eq and elidable_lifetime_names;
616481 contains only the already reviewed syntax correction, not a runtime repair.

ARM's first work is ChildExited/Run at1,580,445us after the actual Run CLI returns0,
before the first successful History observation or any Cancel dispatch. A later cleanup
observation reports the daemon's application exit5; its stdout/stderr are null. Store
stage, category, native error, SQLite code and daemon error text are UNKNOWN. Helper
flags97 attest its owned root reap/private-state cleanup checks, not target Job emptiness
or descendant absence. Do not infer a cause from application5, architecture or the last
independent observation. The frozen read-only packet
`windows-native-sid-d89-37346840859-readonly-20261006` report SHA256
`1fe81c525ab53932a7c09f604ebddc5c6da94ca9cb3eef07523be8cc27335175` establishes this
missing-daemon-output boundary. Its historical needless_lifetimes wording is superseded
by the actual raw elidable_lifetime_names diagnostic; do not rewrite the frozen packet.

At d89, both native package jobs actually pass168 named tests each, with zero failed/
ignored and133 filtered, and the complete four-archive/provenance/PE/coherence proof
passes. GitGuardian passes for d89. Package report SHA256
`1d1dcc14ff605ea89bb04a094bf0401fbc3c29acb7a27c15b4235303f7b68cec` binds that exact
head; no test, package, Guardian or earlier D result transfers to the joined head.

### Existing D capability and unchanged ownership

D's existing cancellation_daemon prepares two CreateNew guarded private output files
and retained exact Stdio duplicates, then performs the same actual daemon spawn with
the original8s post-spawn clock. complete freezes first work before cleanup. Only after
confirmed root reap can observe_pair_after_reap admit an optional read under the original
entry-born30s normal clock. Unfinished/uncertain cleanup retains the same owner; no new
observation work is admitted after expiry and late returns never become timely success.

The existing reader verifies full original identity, independent cursor0 and no-follow
ownership, and keeps the64KiB plus one sentinel cap. PairSummary publishes only fixed
stream/capture/count and recognized closed Store stage/category/Io kind+returned raw or
SQLite primary/extended codes. No SID, private path, PID, SQL, argv, environment, raw
stderr or arbitrary message is exposed. Existing Store debug emitter bytes are equal in
mainf90/d89/D. A missing, unrecognized, ambiguous, late or refused record remains unknown;
this capability does not guarantee classification or fix the historical daemon exit.

Keep original first-work/error priority, counters/progress-stop, actual children/Jobs,
peer PID/frame/ACK, same-owner disposal/drop/reap/quarantine and all existing D capture/
producer controls. Preserve5s Wake,8s Cancel,200ms probe,30s outer,25ms stop pause,
production5s grace,64KiB sentinel and all other selected bounds. No additional readiness,
retry, warm-up, timeout growth, oracle waiver or raw dump is selected here.
[Microsoft GetExitCodeProcess](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-getexitcodeprocess)
and [Rust Child](https://doc.rust-lang.org/std/process/struct.Child.html)/
[process pipes](https://doc.rust-lang.org/std/process/index.html) were read by Root as
exit-value/explicit ownership/blocking guidance only, not proof of the daemon cause,
current native completion, library compilation or interruptibility.

### Ordered integration and concrete Verify

1. Root selects the complete amended plan before Git integration. **Verify:** only these
   four Docs append; all earlier raw/canonical prefixes and616481 Source remain exact.
   Existing privacy#27/CI#31/integration#163 receive the whole plan by exact POST/GET,
   preserving bodies/state/history. AFTER ALL GETs Root actually rereads the whole final
   committed plan and every new Docs block before granting its Git-only integration lease.
2. Root performs a normal merge of EXACT D17133 into that gated common head. **Verify:**
   clean preflight, named backup, ordered parents [gated common head,D17133], mainf90
   ancestry, whole mode/blob union and every native four/D nine Source blob retained.
   Retain both complete reviewed Docs append histories once. No force/rebase/cherry-pick,
   newer incoming head, guard A/B/C/E, algorithm adaptation or test/workflow rewrite.
   A Source conflict or noncontext decision stops for developer research/Docs first.
3. Root reviews the complete joined tree and ordinarily publishes it once. **Verify:**
   clean exact head/base/ref, complete contribution/inverse ledger and truthful PR scope;
   fresh automatic attempt1 binds synthetic ordered parents and the complete joined tree.
   Acquire each completed raw job/artifact only once, retaining raw/declared projection;
   reuse historical bytes without reGET. No unchanged-head rerun or manual dispatch.
4. Qualify this joined head's actual behavior and packages. **Verify:** all three original
   eight-selector rows and existing D output/producer/publication positive/refusal controls,
   original SID six/cache/cold/EOF/policy/parent-exit, doctor/service/GUI and all required
   downstreams, fmt/warnings-denied lint/MSRV/Unix/Guardian pass. Bind actually triggered
   paired/snapshot/package gates and complete current-head ZIP/PE/ABI/provenance/coherence;
   mark absent/skipped/unrun explicitly, never import d89/D counts. Preserve first work,
   actual returned status and closed timely capture facts separately; missing/late/error
   records cannot waive failure or authorize a guessed runtime repair.
5. Root merges only the qualified exact joined tree and verifies contribution to main.
   **Verify:** actual merge/head/tree and complete selected member-source conservation;
   PR166/167 remain open until their required Verify/qualification AND main contribution
   are proved. Only then may they close as verified subsumed work. Ancestry alone, static
   success or a diagnostic record is not closure; broad#27/#31/#163 and release/account/
   logon/reboot/signing acceptance retain their actual remaining scope.


## Test-only cleanup return and first positive-result observations (2026-10-06)

This amendment selects observation only at reviewed joined c39ebeb3d2c3784bf8a06f383fa8d490f79edf61,
tree9cec9f922b141ab4d0fd03f1cbd657cb64995663, ordered parents [4860d25,D17133].
CI37355065492/attempt1 remains FAILURE:16 ordinary jobs pass, MSRV foundation fails,
and2 optional jobs skip. Original eight native selectors across three rows are22PASS/2FAIL.
Stable/ARM pass all eight and their downstream commands; MSRV fails original Cancel and
capture and skips lifecycle16/dashboard4/prune3/maintenance11/MCP4/HTTP21. All three
pass Core137, Engine69, Server42, Store96, doctor6, Service64, GUI12 and the three isolated
stock modes; current strict Windows fmt/Clippy passes. No runtime repair is established.

MSRV original Cancel freezes Expired/Cleanup, flags63, elapsed_us8388318, intentDropState;
first work is Success/Progress. Daemon CleanupWait application1 is a returned cleanup
fact, not a prior fatal daemon observation. History observed Cancelled and stopped progress;
its4,112,464us accumulated wrappers are wall-time, not SQLite/kernel cost. No CLEANED or
later worker result is printed. Daemon_spawn_us378185 is entry-relative, not spawn cost or
the exact post-spawn8s expiry anchor. Capture's queued control passes at entry-relative
5663046us; cancel's first work is Expired/Connect at6389918us and later completion is
Expired/Cleanup at30002453us, flags33/releasedfalse. Connect cause remains UNKNOWN.
The current positive collector refuses closed admission and quarantines; changing that
permit, eligibility, order or quarantine is not selected. Preserve the frozen failure packet
windows-native-sid-joined-c39-msrv-readonly-37355065492-20261006/report.md SHA256
15b26ca80ce821c6ab0ab691a648139993786a91b6fec41ab63f54c4ca5f88e8 and all raw evidence.

### Minimal private Source boundary and matched cleanup records

Future separate development changes ONLY crates/locron-cli/tests/support/windows_cli_control.rs.
Existing CaseResult Display/Debug, ProducerControlScalar, ProducerSummary, PairSummary,
all original summaries/assertions/selectors and every native/collector call remain literal.
Core SID four paths, Store/Engine/IPC/CLI/Unix/workflows/dependencies are protected.
The following anchors refer to exact c39, not a future line-number mapping:

- Observations386-506, CaseResult2930-2942 and Control2991-3031/3143-3164: add a private
  fixed cleanup observation/snapshot only. Four AtomicU64 words start at0/unobserved;
  snapshots load each once with Acquire, without retrying to manufacture coherence.
- Owner3263-3316: one worker-local first-group claim starts false, owns no resource and
  changes once before the first DropState observation. It is never reset. Only this SAME
  owner publishes; complete and later Drop/release cannot pair different removal calls.
- release4755-4757: after the existing gate/intent and before the SAME drop(self.state.take()),
  claim the first group, sample its entry Instant and publish entry. Retain that local Instant
  and the Boolean actual state presence from the existing root Option; no path is emitted.
  Immediately after that same drop actually returns, sample return and publish its record.
  No return on panic/blocking/uncertain work is invented; no TempDir::close substitution.
- release4765-4775: observe ONLY the already-returned root.try_exists Result as absent,
  present or io_error before its original post-gate/branch. Publish CLEANED only immediately
  after the existing flag(CLEANED), never on a returned drop alone. Existing IO observation,
  error priority, post-gates and actual query remain unchanged. No second existence query.
- complete4787-4866 and Owner::drop4870-4905: preserve work-first/cleanup.and(work), reap,
  collector eligibility/order, original retained owner and quarantine exactly. TempDir's
  destructor returning does not report its deletion Result; the original root query/flag
  remains the release evidence. No new result, release authority or timely-success oracle.

Use four checked self-contained u64 records with this exact layout: sample_us bits0-24,
owner-computed duration_us bits25-49, clock_class bits50-51, value bits52-53, operation
bits54-59, event bits60-62 and valid bit63. Widths25+25+2+2+6+3+1=64. Zero is unobserved.
Event1/2 are DropState operation50 entry/return; event3/4 are CleanupStateExists operation51
root-return/CLEANED. RoleNoChild and groupfirst are fixed by these private slots/sole-owner
claim, not a PID or serialized authority. Reject other headers/domain/reserved values as
invalid; no panic, acceptance or cleanup decision follows diagnostic decoding.
Clock1 means representable origin-relative0..30000000us, NOT timely case admission;
clock2 is outside_range and clock3 invalid_clock, both with numeric time unobserved and
stored sample0. Entry/return value0/1 means none/present; root value0/1/2 means absent/
present/io_error; CLEANED value0 means confirmed. Non-return duration bits must be0.
Return carries checked floor(actual_return.checked_duration_since(actual_entry)) in us
only for the locally matched first call with representable entry/return and0..30000000us.
Otherwise duration uses the25-bit sentinel33554431 and renders unobserved. A reader may
render duration ONLY when its entry and return validate as this same first group/state;
it never subtracts independent snapshot stamps. Missing/invalid/mismatched return gives
unobserved duration. Root/CLEANED facts remain separately observed, not inferred from it.

At most four fresh Instant::now samples for the claimed group: entry, actual return,
already-returned root query, and existing CLEANED publication; repeat teardown adds none.
These are OS wall-clock samples (Rust Instant uses Windows QPC), charged to the original
clock. Do not claim zero added native calls or kernel-only deletion cost. The returned
interval brackets the compound Drop/private-state/TempDir destructor and observation
overhead; it is not a cause, interruptibility or per-file latency measurement. Preserve
all original5s Wake/8s post-spawn Cancel/200ms probe/30s outer/25ms stop/5s native grace,
checks/sleeps/native queries and byte caps; no clock reset/growth or new admission gate.
Exact post-spawn expiry remains unobserved; no optional expiry-slot fields are selected.

### Full first positive result and bounded separate lines

producer_controls6555-6717 retains the ENTIRE immutable first CaseResult after the existing
receive_until. Keep every scalar field, including original observation/call/capture/pair
proof facts; never place an actual Child/guard/client/runtime/reader/owner on a driver or
channel. Where original6570-6574 currently moves it, hold only an optional distinct cleanup
result from the SAME conditional wait_cleanup_until and borrow that result or the first
for the existing completion assertions. No new wait/join, channel, gate or Code priority.
Existing finish_if_returned/case-summary/producer-summary/assertion order remains literal.

Append eight separately closed producer_first/v1 lines at the existing positive outcome
report point after finish_if_returned, without another observation wait: (1)code/phase/
flags/frame_bytes/elapsed_us/stdout_bytes/cli_live_seen/capture_proof; (2)intent/first_work;
(3)last_io; (4)three role status slots; (5)cli/wait/native_enter; (6)native_return/wrapper_return/
capture_read; (7)last_history/history_cost/last_progress; (8)daemon_spawn_us/history_rel/
progress_rel. Reuse the existing checked EventDisplay/CallWordDisplay and closed enum
names. No full one-line CaseResult formatter, raw packed word dump, SID/path/PID/UUID/
context/digest/file identity/SQL/argv/environment/input/raw stream/error message. Pair
proof identities remain retained privately and unprinted; existing pair summaries stay exact.

Add four cleanup_state/v1 lines, one per slot, with scopefirst/completion_snapshot or
after_existing_wait, casequeued/cancel/wake, event and a closed fact{op,role,group,clock,
t_us,value}; only the return line adds drop_wall_us. A zero/invalid slot prints that fixed
classification. The first snapshot stays frozen. In drive_producer5237-5277, ONLY after its
already-existing failure-only wait_producer_until, copy the four scalar slots once for a
separate later diagnostic; admission/dispatch failure and normal success use unobserved
later facts. Expose them through the existing failure-output wrapper after its literal
ProducerSummary. No new polling/read/collector/delay; a late actual return is a separate
returned fact, never a replacement first Code, CLEANED success or permission to release.
Positive controls report their frozen first and existing completion snapshots separately.

The external source-domain width model proves eight first lines219/135/120/225/182/198/
214/105 ASCII bytes includingCRLF, and four cleanup lines156/181/163/160 includingCRLF.
The longest NEW line is225<256; eight first lines total1398 and two cleanup groups total1320,
so all selected new positive output is at most2718<3072 bytes. Two cleanup-only groups are
at most1320<1536. These are independent component maxima, not claimed reachable native
combinations. Bound u128 decimal39, u32 decimal10, signed i32 decimal11 and capped time
8 digits; invalid/overflow/unobserved fixed branches are included. New summaries do no
query or OS sampling while formatting and their output cost stays in the original clock.
Do not change original larger summaries to fit the NEW-line bound or silently truncate facts.

### Independent diagnostic A/B qualification before common merge

Supersede ONLY the previous common-main-first schedule for reviewed original guard groups
A and B. Root may independently publish DIAGNOSTIC drafts using EXACT c39 Source plus
original A d24a2a65c05ef52eea644723d8f8ea5a11c0286a or
B b6c9d1ecbf04db7e4dfd5b71a26de48d501ea4a0; Docs-only descendants are allowed. Do not
silently import pending observation Source, a newer main, C/E or an unreviewed guard edit.
Preserve each full reviewed member contribution/own Verify, unchanged main/c39 sources
outside that group and all required oracles. A Source conflict/new decision returns to
Docs first. This is diagnostic isolation, not a cause/fix claim or prerequisite waiver.
Current c39 FAILURE remains historical failed qualification; no PASS/count transfers.
A/B draft failure or skipped step stays failed/unqualified. Common-main inclusion and each
member's own actual required acceptance/merge/contribution proof remain mandatory before
closure; ancestry or a successful unrelated boundary is insufficient. C/E remain later.

1. Root commits only the four Docs and records complete owning#27/#31/#163 plans. **Verify:**
   complete old prefixes, c39 Source/all329 tree entries except four Docs unchanged,
   exact whole-plan POST/GET retains original Issue bodies/state; AFTER ALL GETs Root
   actually rereads the whole final plan and every new Docs block before Source/integration.
2. Separate development implements only the helper observation slice. **Verify:** whole-file
   inverse restores c39 helper, all other Docs-parent modes/blobs and old selectors/bodies/
   assertions/clock expressions/calls/collectors/owners remain exact. Text/domain models
   check packed headers/25-bit limits/sentinel/first-only mismatches and every line bound;
   matched returned duration only, first result retained, no added process/FS/security/IPC
   query or leaked text. Scoped pinned fmt/static metadata only locally; type/native pending.
3. Root reviews/publishes one changed observation head. **Verify:** exact parents/tree,
   fresh hosted24 native outcomes and both genuine positive subcontrols plus original
   Core/stock/doctor/service/GUI/downstream, strict fmt/lint/MSRV/Unix/package/Guardian
   gates; missing/skipped/failed controls remain explicit. Observe actual entry/return/root/
   CLEANED separately without requiring a failure to repeat or treating diagnostics as PASS.
4. Root may qualify independent diagnostic A/B drafts before common-main merge. **Verify:**
   exact c39 plus each approved original guard Source/Docs-only descendant, full contribution
   union and each own Verify; actual fresh required jobs/guards, all existing failures/oracles
   and package bindings remain revision-specific. No rerun, weakening, skipped gate or
   A/B close/merge merely from this permission. No current integration is performed here.
5. Complete common/main and member acceptance before closure. **Verify:** qualified exact
   actual merge/main contribution and every member's own required Verify; PR166/167/A/B
   stay open until those proofs, and broad#27/#31/#163/release/account/logon/reboot remain
   at their actual scope. No production repair is selected; any concrete repair needs new
   research/Docs/owning readback/final review before Source. C/E qualification still follows.


## Hosted cleanup-observation lint correction (2026-10-06)

Exact Source3905f944b973f51ebb530363024b26cda428d167, tree
172e8d830896d2f6d2a178ecf1c4400ec21339d6, CI37367154610/attempt1 retains an
actual Windows Rust1.98 lint FAILURE at job111954962586. CSI-only463-497 reports
exactly nonminimal_bool at helper2983-2984 and struct_excessive_bools on Owner3752,
then CLI-test compilation refusal due two errors. Raw SHA256
9a44aeebfaa5ae6ad164e04d6029a3068715ff5bad01f724edb7a8e461a80d88;
CSI-only SHA2563fc3b9de0fed91019342beca840eb8c6dc43cb4bef7c8d9e32fd650916e075ae.
These actual pinned diagnostics are authoritative; the versioned1.98 manual page was
unavailable. No claim of reading that page or local type/Clippy proof is made.

ARM111954962954 actually passes Core137 and seven of eight original native selectors;
only Wake fails, Expired/History flags31 elapsed_us5476339. Its frozen first cleanup
snapshot is unobserved. After the existing diagnostic wait, the SAME owner's matched
DropState return prints drop_wall_us27331, then the existing child-root query returns
absent and CLEANED is confirmed. RootReturn is recorded BEFORE the unchanged post-gate;
CLEANED follows that gate and absence classification. Neither returned unit nor late
facts overwrite first failure, grant timely success or prove whole TempDir-container
removal/the hidden deletion Result. Both genuine queued/cancel positive controls pass
with immutable first/completion and separate checked records. Their success does not
repair Wake. Root's retained snapshot2 has completed-success x64stable111954962906 and
MSRV111954963303 foundations; complete current-head qualification remains pending and
strict lint failed. Cancelled/missing/skipped jobs are not PASS; no earlier result transfers.

Select ONLY the six literal spans in the separately reviewed unapplied patch SHA256
73c6e1e9368cd477f19e233a89203aa90f75c54ad1a2374d7ba9f6ad01a05fd4, inside
crates/locron-cli/tests/support/windows_cli_control.rs. Replace (A||U)&&(B||U) by
U||(A&&B): A is duration_us<=CALL_TIME_US, U is equality with the existing unknown-duration
sentinel, and B is clock==1. Preserve value_valid/sample_valid and every surrounding
header/domain/encode/decode branch. The full eight Boolean rows and30 numeric boundary
samples prove the pure equivalence, not native timing or compiler fit.

Insert the private two-case CleanupObservation::{Unclaimed,Claimed} before Owner and
map ONLY its four cleanup_observed member sites: field type, false initialization,
first-claim conditional and true assignment. False/true become Unclaimed/Claimed; the
condition uses matches!(..., Claimed). Keep the sole transition at the SAME worker-local
first DropState claim before its original Instant sample. The enum is not serialized,
release authority or a new owner/state machine; it preserves the existing first-group
observation decision. No Copy/borrow/lifetime or actual resource/Result/drop change.

No allowance/expect, warning suppression, unrelated MSRV fetch_update edit, extra Source,
new query/worker/sample/slot/event/wire value, clock/reset/retry/readiness/collector change
or production repair. Preserve every old first Code/phase/summary/assertion/selector,
clocks5s/8s/200ms/30s/25ms/5sgrace, native owner/reap/retention/quarantine and formatter
bounds. Reverse all six spans to restore the complete3905 helper byte-for-byte; formatting
outside these selected expressions returns for Docs first. The full prior569-line prefix,
frozen SPEC/ARCHITECTURE, all Core/Store/Engine/CLI/Unix/CI/Cargo and other bytes stay exact.

1. Root freezes this complete amendment before Source. **Verify:** exactly four Docs,
   all earlier raw/canonical prefixes and3905 Source/protected tree entries unchanged;
   owning#27/#31/#35/#163/#171/#166/#167 receive the WHOLE final plan by exact POST/GET
   with original bodies/state/history retained. AFTER ALL GETs Root actually rereads the
   whole committed plan and every new Docs block before separate development resumes.
2. Separate development applies only the six-span helper correction. **Verify:** full
   staged/unstaged/committed diff, whole raw/canonical inverse to3905, unchanged selector/
   oracle/clock/owner/grammar/formatter regions and every other Docs-parent mode/blob;
   eight truth rows/state mapping and checked domains stay equal. Scoped pinned standalone
   rustfmt1.94/1.98, full locked offline Cargo metadata and Git/text proof only locally;
   compiler/Clippy/runtime/native/PowerShell/parser/fixture effects are NOT RUN.
3. Root reviews and ordinarily publishes the genuinely changed head. **Verify:** exact
   parents/main/synthetic equal tree, fresh strict warnings-denied lint and all three
   original eight-selector rows plus genuine queued/cancel controls and all old Core/
   stock/doctor/service/GUI/downstream/Unix/package/Guardian gates as actually triggered.
   Record failed/unrun/optional/absent scope honestly; no unchanged-head rerun or PASS
   transfer. Every member keeps its full required Verify and actual main contribution
   before merge/closure; PR166/167 and broad Windows/privacy/release owners stay open
   until those proofs. This syntax correction is not a Wake or causal runtime repair.
