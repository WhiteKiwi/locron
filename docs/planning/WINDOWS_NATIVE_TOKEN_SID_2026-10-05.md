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
