# Stock parent-crash fixture heartbeat correction

This is a measured test-fixture continuation of Issue #31 and
[Windows native validation](WINDOWS_NATIVE_VALIDATION_2026-10-03.md). Frozen SPEC, that plan's
historical workflow/compilation handoffs and unfinished external Windows acceptance stay
unchanged. At planning review the separate research session had not changed Source. The parent
published docs-only ec13bed633672d080b7810a8596ba4e9801bd123 and read back the
[Issue #31 pre-Source criteria](https://github.com/WhiteKiwi/locron/issues/31#issuecomment-5971969016)
before handing implementation to the separate development session. Fresh native Source Verify
and parent publication remain required.

## Evidence and limits

Baseline PR #130 head is `99ea5c97ce925f41755fa86dfa54eeffb97313bb`; ordinary CI run 37139545888
checks merge `495bfd5817dc25deb8f03219ee5fbfc5359c4988` against main
`d56dcbbbb20a58c68a81d6348a54a77c87488e3d`. Job 111250945393 passes cold Core 130/130 and fresh
EOF/Restricted proof, then times out after 45.01 seconds in parent-exit-driver at
loader_crash.rs:107. That is a shared-deadline check in counter parsing, not native API phase
diagnostics. No final counter bytes, call-site phase or failed-job native artifact survive.
The timeout does not prove an empty file, completed host kill, three target exits or a product
loader defect. FINDINGS records unchanged full Core tree/fixture blobs at the prior head, current
head and actual merge; the earlier native_guard_stall failure remains separate evidence.

Both real counter writers truncate their final path before completing new digits. A portable
actual std::fs::write process stopped in that interval and killed/reaped leaves an empty file;
an independent snapshot process preserves the last complete number when killed before publish.
Those observations justify a narrow fixture correction, while the cause of this Windows timeout
remains unconfirmed. API source inspection cannot replace fresh native tests.

## Permitted Source

The default complete Source list is:

- crates/locron-core/src/windows/loader_crash.rs: counter candidate/publication helpers,
  heartbeat writer, driver phase/counter observations and isolated real-process regression.
- crates/locron-core/src/windows/loader_crash_host.ps1: the generic heartbeat's same-directory
  complete-candidate publication and an explicitly selected test-only publication gate if needed.

Only if those regressions require dispatch from the existing exact fixture entry point, also
change crates/locron-core/src/windows/loader_tests.rs by adding explicit cfg(test) mode arms.
Retain every existing mode, selector and unset/unknown-mode behavior. Both modules and the host
script are already cfg(test)-isolated by windows.rs. Production windows.rs,
owned-child/cleanup, guard/admission code, observer assertions, lockfile/dependencies, workflow
commands/order and toolchain selection remain outside this Source scope. Report a required
change outside it with evidence before implementing it.

## Change order and Verify

1. Finish documentation/issue review before Source. Preserve the exact failure, missing evidence
   and cause uncertainty in FINDINGS and Issue #31. **Verify:** parent reviews all three changed
   documents, publishes the docs commit, and reads back the issue's concrete pre-Source criteria.
   The development session starts Source only after the explicit handoff and records its baseline.
2. Publish complete counter snapshots from both writers. Create each candidate exclusively in
   the same private directory as its final heartbeat, write invariant decimal digits completely,
   flush, then close before namespace publication. Track the initial publication explicitly:
   fail a pre-existing first destination rather than silently adopting/overwriting it. Later
   updates replace the published snapshot without truncating it. Rust uses the already locked
   tempfile 3.27.0 NamedTempFile→into_temp_path(close)→persist_noclobber for the first candidate,
   then TempPath::persist for replacement. Stock PowerShell 5.1 uses a CreateNew FileStream,
   Flush/Dispose, then the Framework two-argument File.Move for the first candidate and
   File.Replace(candidate, destination, null, false) subsequently. Keep the candidate in the
   same directory/volume and propagate errors. No delete+move, copy, replacement-error creation
   fallback, metadata-error suppression, counter default or parse fallback is permitted.
   **Verify:** source review follows the exact locked/runtime API path; first-collision and
   replacement/share-refusal regressions fail explicitly for each writer. Complete successful
   publications parse as the actual expected numbers. Never claim power-loss durability, general
   cross-platform initial-move atomicity or preservation of the old name on every native error.
3. Retain bounded observations before expiry. Label each existing counter sample by its fixed
   driver phase; save the current counter name, at most 64 observed bytes, parse/not-found/error
   status and elapsed/remaining time. Save completed host-kill/reap, handles-bound,
   exits-confirmed, observer-release/exit and guard stages as they occur. Keep observations
   bounded (the two latest counter observations and a fixed-size phase history); output no
   private paths, SID, input, reply or secrets. At deadline refusal report only these saved facts.
   Diagnostics must not begin a new read, helper poll, stock query or other native I/O after the
   original deadline. Incomplete or invalid counters still fail under that deadline; a saved
   valid value is never substituted for a missing/malformed current read. Preserve 5-ms read
   polling and all existing material assertions. **Verify:** an expired fixture displays its
   last recorded phase/counter bytes/status without fresh I/O; malformed/missing cases retain
   failure and the original deadline. Phase recording never manufactures handle/exit proof.
4. Add controlled actual-process regressions for the unpublished window. Use an exact owned
   process and explicit test mode to stage/close its next complete snapshot, report that bounded
   fact, and remain alive before publication. Kill only that retained publisher, confirm its
   actual exit, prove the previous published number remains complete, then publish/verify the
   next number. Exercise Rust and stock PowerShell publication on native Windows; keep the
   normal parent-exit driver's real moving heartbeats and abrupt crash timing ungated. Select
   these controlled modes through the existing fixture entry point after the normal parent-exit
   proof, forwarding the original helper deadline. Add no earlier cold-gate warm-up or workflow
   selector. If the correction cannot fit this scope/budget, report the measured limitation
   before changing it. A local portable extraction records exact helper parity and Windows limits.
   **Verify:** each real publisher's stage receipt precedes the kill, actual owned-process
   completion is confirmed, old/new values match exactly, and fixture cleanup remains bounded.
   Use no unset-mode pass, simulated PID, default counter, reset, ignored failure or test retry.
5. Preserve ownership/cold qualification and publish the reviewed correction. Keep the single
   45-second isolated driver budget; original 30-second adapter, observer and writer limits;
   3-second cleanup; 25-ms heartbeat interval and each 200-ms settle window. The observer still
   binds the actual distinct fixed-worker/generic/native PIDs while alive and retains all three
   Process handles. Killing only the retained crash-host still triggers abrupt kernel Job
   closure. All three finite WaitForExit proofs, moving-before/stopped-after counters, actual
   root/tree/pipe cleanup, and stock share_mode(0) refusal while either helper is live remain.
   Final incompatible stock open succeeds only after both helpers and all targets are confirmed
   finished. The fresh orchestrator makes no stock/SID/filesystem dispatch of its own.
   **Verify:** parent inspects every changed Source blob, original assertions/values, cfg(test)
   reachability and unchanged protected blobs before normal PR publication. Rust 1.94/1.98
   formatting and git diff --check pass; run applicable local regressions and report limitations.
   Fresh exact-head x64 stable, ARM64 stable and x64 MSRV ordinary CI must run the unchanged cold
   Core→EOF-release→Restricted→parent-exit sequence without earlier warm-up, followed by the new
   isolated snapshot proof. Record actual host/compiler, modes, phases and failure output. Run
   the original full-discovery commands and retain every lint/runtime failure. Deadline
   enlargement, skip/allow/continue-on-error, quarantine reset or repeated runs that conceal a
   failed native contract cannot satisfy Verify.

## Runtime publication and remaining acceptance

FINDINGS anchors tempfile 3.27.0 to Cargo.lock and its checked Windows MoveFileExW implementation,
and Framework File.Move/File.Replace to Microsoft's pinned reference source and Win32 docs.
Candidates are closed before replacement. The current Rust counter reader uses the standard
FILE_SHARE_READ|FILE_SHARE_WRITE|FILE_SHARE_DELETE default, which permits destination delete
access; the separate share_mode(0) stock executable guard is deliberately incompatible and
unchanged. Replacement failures, including metadata/sharing errors and documented partial
rename outcomes, remain failures rather than a weaker success path. Primary source and portable
process proof establish the design only; fresh Windows process/filesystem evidence must qualify
the actual successful and interrupted publications.

Implementation refinement: the original parent-exit driver retains its one absolute deadline
through every new proof. New exact child modes receive a data-only remaining-millisecond budget
capped at 30 seconds; the parent's pre/post checks and retained-child termination still enforce
its earlier boundary, including process startup. Rust stages in a directly retained fixture
child. A separately retained helper owns the new stock adapter; its script starts and retains
the actual PowerShell publisher Process/handle, kills/reaps that exact child and verifies old/new
bytes. Its static encoded child source reuses the same publication functions as the normal
writer; directory and budget are environment data. No new dependency, unsafe clock bridge,
workflow selector or driver-owned adapter is introduced. Failure cleanup remains the existing
owned-helper/adapter Job cleanup rather than a new grace period or quarantine reset.

The implementation receipt preserves the original entry modes and appends only two explicit
publication-proof modes. Local macOS Core Rust 1.94 passes 45/45 tests. Exact-source portable
helpers use the same locked tempfile package/checksum/resolved dependencies; a directly owned
publisher stages a closed candidate, is killed/reaped, preserves counter 7 and then publishes 8.
Initial collision, numeric-to-malformed refusal, 64-byte saved observations, sixteen-entry phase
history and expired-read callback refusal are checked. Rust 1.94/1.98 formatting, actionlint and
diff whitespace checks pass. These local receipts do not qualify Windows sharing, Framework
replacement, Job/tree/pipe behavior or explain the original job's missing counter bytes.

This correction does not resolve all full-suite failures or complete Issues #31/#32. Clean
Windows 11 standard-user installation/task/reboot, independent-user privacy/IPC, active daemon
crash recovery, final public unsigned release/update/removal and catalog lifecycle remain their
existing separate acceptance work. Retain original failure receipts and do not claim that a
successful fixture correction retroactively establishes the missing bytes in Job 111250945393.

## Measured command-size continuation before Source

At e4d2ca16, native Core130 and EOF/Restricted succeed, but the parent-exit helper refuses
before actual generic spawn. Exact source reconstruction gives EncodedCommand32836 ASCII
units before executable, flags and NUL; CreateProcessW permits32767 total. This Source defect
is separate from the old45.01s expiry and the unobserved raw OS error. File.Replace's ordinary
$null backup argument also needs direct managed-null binding, without claiming that the failed
job reached it. FINDINGS records the measured bounds and primary references.

1. Publish reviewed docs and owning Issue criteria before handing Source to the separate
   developer. **Verify:** clean docs-only commit and exact Issue readback; retain all failed
   native rows and their missing raw-error limitation. Frozen SPEC and public scope stay exact.
2. Keep Source in loader_crash.rs, loader_crash_host.ps1 and the already permitted dispatcher.
   Add two unique static comment delimiters in the existing ps1; choose shared definitions
   plus only the original normal tail or complete proof branch using a pure Rust split/join.
   Retain two fixed strings in test-only OnceLock<String> slots for the existing static API;
   no unsafe/leak, dynamic-key cache or production signature change.
   **Verify:** both selected bodies retain their entire original program tokens, conditions,
   receipts and assertions. Delimiter matching preserves LF/CRLF. No new file, script I/O,
   request-code interpolation, stdin-code transport, production adapter or fallback.
3. Change only the no-backup argument to direct fully qualified NullString.Value. **Verify:**
   false ignoreMetadataErrors, initial no-clobber, complete closed candidates, actual sharing
   refusal and retained stage/kill/reap/7-preserved/8-published assertions remain exact.
4. Parent reviews complete scoped Source before publication. **Verify:** actual static normal
   and proof requests fit32767 UTF-16 command units including executable, original flags,
   quoting and NUL for LF and CRLF; preserve measurable margin. Windows-target Rust1.94 check,
   Rust1.94/1.98 fmt, actionlint and whitespace pass. All unrelated blobs/modes are conserved.
5. Collect fresh ordinary and original five full native rows on the exact current-main head.
   **Verify:** all three ordinary native rows pass actual original crash/observer/guard proof
   and both publication proofs inside the original45s boundary. The stock Framework5.1
   publisher must qualify real collision/share/child termination and complete replacement.
   Record remaining full-discovery failures and no skip/allow/deadline or ownership reset.
   Required checks must pass before merge; Issue #31/#32 and public acceptance remain open.


## Measured rename-refusal control before Source

At1a1, all three native rows pass Core131 and the original actual parent crash/handle/
heartbeat/guard proof. The new Rust rename refusal yieldsSome(5), while the test wrongly
expects the exclusive-open codeSome(32). FINDINGS records the exact logs, tempfile failure
ownership and primary references. The ordinary run was cancelled after these failures;
later cases are unqualified. Keep its failures rather than treating cancellation as a pass.

1. Publish reviewed docs and the owning Issue before Source. **Verify:** docs-only commit
   and exact Issue readback preserve the actual three-row5 receipt and all unknowns.
2. In only loader_crash.rs publication_proofs, create one complete closed8 candidate and
   retain its path. Require exactSome(5), identical returned TempPath and unchanged7/bytes7
   while the exact target handle remains open. **Verify:** no broad error allow-list, new
   candidate, path reconstruction, fallback, hidden retry or weakening of old assertions.
3. Close the exact held handle and publish that same returned candidate successfully.
   Require8/bytes8, then publish a complete7 snapshot and require7/bytes7 before the old
   actual retained-publisher proof. **Verify:** all new I/O has original pre/post observation
   checks; old stage8/livePID/kill/reap/preserved7/publish8 source and45s boundary are exact.
4. Parent reviews and publishes fresh native qualification. **Verify:** only the one test
   function changes; all other blobs including PS/stock-open32, dispatcher and workflow
   stay exact. Rust1.94 Windows-target check/tests compilation,1.94/1.98 fmt and whitespace
   checks pass. All three ordinary native rows exercise the original crash plus complete
   Rust and Framework5.1 proofs; five full-discovery rows retain their actual outcomes.
   Required checks must pass before merge; old45s cause and public acceptance stay open.


The same-candidate identity assertion must retain privacy and the warnings-denied lint
contract. Native lint111281642758 at f1cc5dc rejects assert!(left == right) as manual_assert_eq.
Bind the exact path equality once to a named boolean and assert that boolean with a fixed
failure message. This reports the identity failure without formatting either private path.
No warning allowance, identity weakening, or other Source/clock change is permitted.
The separate developer applies this single assertion-style correction after this plan.
