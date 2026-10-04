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

## Selected single-created heartbeat and real interruption controls (2026-10-04)

Source3a6e run37172647017 preserves the real post-crash NotFound and every missing observer/guard proof. Root reviewed the separate32-file investigation and5-file precise appendix before selecting the following fixture-only continuation. Only loader_crash.rs and loader_crash_host.ps1 are leased; loader_tests.rs/21modes, observer, original Core factory/permits/production/deps/workflow are exact. The old complete-snapshot controls keep their independent contract and are not called inside-ReplaceFile termination proof. Frozen SPEC is unchanged.

### Concrete Verify, in order

1. Before Source, **Verify:** exact4Docs commit and Source3a6 conservation, verified causal/research/design artifacts, Issue31 four-step CLI readback and final Root plan review; isolated independent Source checkout at reviewed Docs head follows.
2. Whole fixture and finite protocol review, **Verify:** only two leased paths; every other296 tracked blob/mode against reviewed Docs head and whole old assembled SnapshotProof are exact. CreateNew retained producer and exact retained ready reader/noDELETE share3, strict21byte ordered records/2048records/43008bytes/43009sentinel and exact unfinished-next-prefix rules. Both actual current count+tail components progress/settle appropriately; all read/unknown/overflow/malformed/rewind errorsfail. Reader resources remain outside bounded failure formatting and release only after final200ms settlement; no file-ID/path query or new clock.
3. Original and new native controls, **Verify:** preserve all old65-byte closed-snapshot7/8/collision/share/kill/reap proofs literally. Both actual Rust and fixedPS appenders share their normal implementation and realcomplete1+20byteprefix2, ownedchildready+live, originalkill/non-successreap, actual retained-filebefore/after tuple(1,20), writerdeath plus reader-held rename/delete refusal, identical operation succeeds onlyafter exact holders release, correctbytes/absence. Actual malformed complete/badtail/overflow/collision casesrefuse; expiredread cannot invoke new work. Same45/30/3s,25/5/200ms and all original guard/observer/tree/pipeconditions; unknown/latepriorrequest prevents subsequent adapter admission. No new mode/selector/errorallowlist/synthetic proof.
4. Root review then combined ordinary hosted qualification, **Verify:** complete two-file Source/rules/fmt/diff, every root+childLF/CRLF encodedcommand below32767 includingactualexe/flags/quotes/bootstrap/NUL and>=1024 margin fornewprograms, old SnapshotProofbytes/marginexact. All three native rows must actually pass original parent-loss proof, old snapshot proofs and both new append controls inside existing helpers, plus unchanged required/paired gates. Static/capacity/source review supplieszero Windows acceptance. Actual missing cleanup fails; wideIssue31 and externalWindows11/account/logon/reboot/full-lint/release remainopen.

### Exact selected appendix

#### Precise append-heartbeat contract

Root selects this appendix before the independent Source handoff. No native acceptance or frozen product scope changes follow from this plan. The actual 3a6e MSRV failure remains a post-crash generic pathname NotFound, with observer/guard completion unproved. An interrupted no-backup replacement is still an API-grounded candidate, not a measured 1176 or instruction-level cause.

## Object binding: select the actual retained reader, no new ID query

Select the smallest sound option: the first ready **actual read File** for each normal heartbeat is retained with READ access and share READ|WRITE (3), no DELETE. Subsequent samples seek that exact handle to zero and read current file bytes, with a check before and after every open/seek/read against the same original 45-second driver deadline. A second path open or a cached prior numeric result is not the sampling source. The handle pins the observed object and continues denying ordinary leaf deletion/rename after the producer's own no-delete writer closes on actual process exit. Reader owners live outside the display-only Observations structure; failure rendering never formats them, performs a query or reopens a file. Retain both readers through the final post-observer-release 200 ms settlement, then explicitly release them before TempDir cleanup or the unrelated old snapshot-publication controls.

The fixture creates its own unique TempDir, passes fixed child names and has only the reviewed producers/observer/controller touching these objects. No arbitrary same-account actor or product state ownership is added. Under that existing test-owned boundary, the retained handle is a direct object binding, so a separate path-based file-ID lookup is redundant. Merely retaining writer handles until they die and then reopening paths without a retained reader would leave a weaker post-exit object boundary and is not selected. Ordinary name mutation is prevented by the held reader and must be tested natively; a broken resource lifetime, read error, unexpected loss or unknown observation still fails. This does not claim discovery of unrelated parent-directory mutation or protection against arbitrary privileged same-account code.

Windows sharing rules keep each open handle's share constraints in force until it closes; denying DELETE denies ordinary delete/rename access while sharing WRITE admits the actual appender. The native controls below must establish those conditions on the real files rather than infer a PASS from this documentation. [CreateFileW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew).

Alternative, **not selected**: existing Core file-id=0.2.3 permits safe get_high_res_file_id(path), already used in stock.rs:243/274. While retaining the no-delete reader, a first ready query could bind only FileId::HighRes and every later path query could refuse any changed full volume/file identity. Every query would need pre/post checks on the same 45-second deadline; low-resolution fallback, identity/path printing and a new deadline would be forbidden. This adds native path work without strengthening same-handle samples. The new task did not obtain the crate's authored source from docs.rs (that read returned an internal error), so this alternative is grounded in the immutable existing use and prior approved API, not a newly claimed full dependency audit.

## Exact record and sample contract

Each actual producer uses CreateNew once for its normal final heartbeat file, with Write access/share READ|WRITE/no DELETE, and retains that stream. It appends only at current EOF; it never reopens, renames, deletes, truncates or seeks backward. A common producer operation builds exactly 21 ASCII bytes: the nonzero sequence as 20 zero-padded decimal digits, then LF. It writes all 21 bytes and flushes them before the original 25 ms sleep. Rust and fixed PowerShell use their respective real implementations of this same contract; no single-write atomicity or power-loss claim is made.

Hard bounds remain 2,048 records, 43,008 file bytes and a reader read cap of 43,009 bytes (overflow sentinel). If 2,048 full records exist, no additional partial record is permitted. Before any first complete record, only missing-at-creation or an empty/strict prefix of record 1 is unready and may use the original 5 ms poll under the same deadline. After the first ready handle is retained, no reopen/default/prefix salvage is permitted. Actual read errors, unknown results, file loss, oversized bytes, a malformed complete record, wrong sequence, wrong LF, non-ASCII or any invalid tail fail. An observed decrease of total validated prefix length also fails.

Validate **all** complete records in order 1..N, not only a last line. The trailing length may be 0, or 1..20 bytes that exactly equal the prefix of record N+1. At least record 1 must be complete before readiness; no complete record may be manufactured from an unfinished tail. Every stable comparison uses the freshly read tuple **(N, tail_len)**, including after each original 200 ms sleep. Because the entire completed sequence and exact next prefix are validated, that tuple uniquely determines the accepted bytes. Live progress still requires each completed N to increase. The original post-kill two-counter settlement and final post-observer-release settlement compare both tuple components, so a still-changing unfinished append cannot pass just because N stayed constant. Frozen failure classes are deadline/late-native, actual read_error, oversized, malformed_record, sequence_mismatch, invalid_tail, prefix_rewind and unknown; no_complete_record is unready only before first readiness for an exact valid prefix. No error is normalized into a successful count.

Counters and readers are not cleanup authority. The original parent reap, three associated process-handle exits, observer-held stock refusal, explicit observer release, actual observer success/root+Job+pipe confirmation, exclusive stock open and both settled counters all remain required. Failure displays retain only bounded saved count/tail/status/error facts and the original phase ring; they do not read after expiry. Actual reader seek/read is synchronous native work: pre/post gates reject late completion but do not claim hard preemption of a blocked call.

## Both real partial-append controls, with existing helper selectors

Use the existing `heartbeat-rust-staged` and `heartbeat-powershell-proof` routes; no new libtest test, environment mode or workflow selector is required. Normal heartbeat and control producers call the same Rust appender helper and same fixed PowerShell appender helper respectively. The control data is complete sequence 1 followed by exactly the first **20 bytes of sequence 2**, without its LF, flushed before an exact fixed readiness receipt tied to the retained producer Child/Process. This creates a genuine 41-byte actual file with expected tuple (1,20), rather than mirrored parser bytes or a manufactured exception.

The direct Rust staged helper can additionally create two dedicated append-control files before its existing readiness marker, while retaining both streams. Its pre-existing closed snapshot of 8 remains unpublished; every old readiness/PID, old snapshot 7/8, kill/reap and replacement assertion stays unchanged. Add controller reads around those original assertions without moving/removing them. The PowerShell append control is a separate static program run after the complete original PowerShell snapshot program within the same existing proof helper. It starts a real retained PowerShell staged child using the shared appender definitions, whose two streams also remain held until the controller's actual kill. Its new fixed ready marker uses CreateNew once, writes the actual retained producer PID, flushes and closes; it is never replaced or repeatedly published. The existing Rust scalar ready marker's original no-clobber publication stays exact. No command-source data or external executable is introduced.

For each producer, use two independent control files so rename and delete can each repeat **the identical operation on the identical named candidate** after all exact holders close. While streams and ready readers are held, both operations must refuse; save actual ErrorKind/raw code only as bounded observations, with no guessed broad error allowlist. Kill only the retained real Child/Process, require its actual non-success reap, seek/read each retained ready File and require the same real (1,20) bytes before and after kill. A still-retained reader must continue to block the same mutation after writer death. Release every reader/duplicate of that particular control file explicitly, then the same rename or same delete must succeed; rename additionally preserves and reads the exact 41 bytes at its destination, and delete confirms that exact file is absent. This positive release continuation prevents invalid paths/ACL failures from qualifying a generic initial refusal. The original snapshot sharing errors Some(5)/32 and all old collision/publish checks remain exact and separate.

CreateNew collision is checked using an actual pre-existing control path and preserved bytes, then an explicitly owned empty-path continuation succeeds after its known previous file is removed; it does not adopt a runner or unknown file. Ordinary producer paths remain CreateNew-only with no repair. The partial-tail control does not claim all arbitrary corruption is acceptable: finite actual-file malformed-complete/invalid-tail/overflow controls fail, and no body substitutes the last valid sequence from an invalid stream. These checks stay inside the existing driver/helper budgets; a failure to fit them is a planning problem, not permission to add time or omit a check.

## Static PowerShell layout and complete command-length qualification

Do **not** concatenate the new control into the old snapshot proof. Select three finite compiled programs with three OnceLock-held strings and a private selection enum: Normal, SnapshotProof and AppendProof. Keep the existing exact unique comments `# locron-heartbeat-proof-body` and `# locron-heartbeat-normal-body`; add unique `# locron-heartbeat-append-common` and `# locron-heartbeat-append-proof-body`. The physical loader_crash_host.ps1 layout is: original snapshot shared prefix; original SnapshotProof body; updated Normal body; append-common definitions; new AppendProof body. Select SnapshotProof as the original shared prefix plus original proof body **byte-for-byte**. Select Normal as append-common definitions before its normal body. Select AppendProof as append-common definitions before its new controller body. The append-common block carries a fixed literal shared-definition string for the normal producer and the new staged PowerShell child's actual appender. It never contains user-controlled source.

The staged child's additional static here-string has exact unique comments `# locron-heartbeat-append-child-begin` and `# locron-heartbeat-append-child-end`. Its command is shared literal appender definitions plus that literal body. These markers allow pure byte extraction/modeling of the child command; they are not native execution, reflection or a PowerShell parser. Program selection checks every marker occurs exactly once and in its fixed order, and rejects missing/duplicate/unknown layout. The unchanged observer bytes are not included or altered by this selection.

Expand the **existing same-named** stock_crash_sources_fit_the_encoded_windows_command_line test to the three selections and actual nested append-child program, both LF and CRLF. Measure the complete UTF-16 command: actual configured stock-executable namespace/path spelling, required quotes, every actual flag, UTF-16/Base64 source including unchanged Core bootstrap/trace composition, separators and terminating NUL. Model the source using the same exact static body assembly used for dispatch, not a shorter invented script/path. Preserve the old SnapshotProof program's entire command and margin exactly. Require every new/changed full command to stay below 32,767 UTF-16 units with at least 1,024 units of margin; actual numbers and hashes remain pending implementation. A failure requires revising static selection before any dispatch, never trimming an original body, switching transport/policy/executable, increasing the Windows limit or ignoring the case. The 32,836-unit historical failure remains evidence.

Only loader_crash.rs and loader_crash_host.ps1 would need Source work for this selected layout. loader_tests.rs, its existing exact modes, all libtest selectors, observer, Core factory/permits, dependencies and workflow stay byte-identical. Existing 65-byte scalar snapshot readers and old snapshot control assertions remain literal, including unrelated ready/PID reads. New append reader owners and parser live separately. Pure selection/capacity tests establish structural facts only and are zero native Windows acceptance.

## Original deadline and owner conservation

One original 45-second driver deadline still precedes helper creation. Both normal producers and observer retain their existing 30-second ceilings; failure cleanup remains the original separate 3 seconds, with unknown ownership held/refused. The existing native heartbeat cadence is 25 ms; counter readiness poll is 5 ms; settlement is 200 ms at each existing boundary. No extra deadline, budget reset, overlap waiver or synthetic success is added.

The existing PowerShell proof helper computes one original capped remaining deadline, runs the original snapshot adapter to confirmed root/Job/pipe completion, releases its exact generic permit/guards, then admits the AppendProof adapter only while that same deadline remains. Each child receives only the floored remaining budget, at most 30 seconds; controller pre/post gates still enforce the original caller deadline. Both requests use the existing owned Core factory and single generic slot; no recursive generic call or concurrent extra child is added. Any unknown or late prior ownership prevents the next request. The driver also refuses every late result under its original 45 seconds. No normal helper, staging operation or new proof becomes PASS from cleanup Drop.

Future native Verify must prove both real appender controls plus the original whole-crash and old snapshot proof on every original native row. This appendix contains no implementation or acceptance result, and retains every current failed flag and raw-log limitation.

## Selected actual managed refusal records after Source8110 review (2026-10-04)

Root's complete8110 review found that PowerShell's four actual rename/delete catches discard exception data. Rust already saves its actual bounded I/O facts. The following observer amendment closes the selected fixture Verify gap before integration; it does not change frozen SPEC or infer a native sharing error/cause. Root reviewed the full26-file research/candidate/snippet/identifier map and independently verified both Source hashes, old SnapshotProof and all10 complete LF/CRLF command calculations (manifest98da55bf/reportcb0c76c8/Root proofb1d9977d). The specimen is unapplied; all actual observations remain unmeasured.

### Exact two-file data-only selection

Only loader_crash.rs and loader_crash_host.ps1 may change after Docs/Issue31/final Root review. Keep the two existing PowerShell Refuse invocations in place, assigning fixed phases0/1. Allocate exactly four slots once, initialized '?': producer_live rename, producer_live delete, writer_dead_readers_held rename, writer_dead_readers_held delete. Keep every original File.Move/File.Delete and Left call, boolean assignment/decision, throw and ownership/release continuation. Set a pure per-operation dispatch-intent flag only after its existing Left and immediately before the original call; only a caught exception after that flag may fill that slot. A pre-dispatch deadline refusal is not a File-method observation. The flag does not witness actual unmanaged entry.

Read only the existing catch's Exception and at most5 existing outer-to-inner Exception nodes. Each node supplies its own full signed Int32 HResult rendered invariant decimal and a closed managed category: io via IOException, access via UnauthorizedAccessException, otherwise other. Category is not an Exception.Kind property, Rust ErrorKind, concrete type name or OS errno. No Message/FileName/GetType/reflection/GetBaseException/nested ErrorRecord/path/PID/native query/low-word masking or status translation. Use the existing InnerException link once per observed node; explicit '=' records observed null end, '~' records a still non-null next node after exactly5 observed nodes. No hidden node fact/cause is inferred. Missing/non-Exception/observation failure produces '?' without a fabricated zero/code; collector never throws over an original work error.

The scalar grammar is flag followed by1..5 comma-separated kind=hresult nodes. At most95 ASCII bytes per record,393 for the four-string JSON array and405 for the extra root field. Add only refusals as exactly four primitive strings to the current bounded final JSON with its single retained ToJson -Compress/default depth unchanged. Keep four slots separate so later catches cannot overwrite earlier facts. No new writer, diagnostic I/O, native call, retry/clock or serialized Exception/handle. Later original work failure still aborts and yields no final observation receipt; it cannot be suppressed to publish diagnostics.

Rust structurally requires four strings, ASCII<=95, '=' or '~',1..5 nodes (~ requires exactly5), kind exactly io/access/other, complete canonical signed i32 decimal (round-trip to_string; no '+', spaces, leading zeros or negative zero). All i32 values/categories are admissible observations, not an expected-error allowlist. Missing/'?'/invalid records fail qualification. Save/print only fixed phase/operation, category/full signed HResult, node order and chain ended/truncated; no object/debug/error text/PID/path. Preserve root/Job/pipe confirmation before acceptance. Existing actual same-candidate mutation refusals and later positive release,41-byte before/after data, genuine kill/non-success reap, EOF and no-delete-reader ownership independently determine the proof.

Only new Append/common variable/function identifiers and indentation may compact; new Normal appender call references can follow those names. Use unique lc- function names to avoid single-letter alias precedence. Preserve old SnapshotProof raw bytes/closed65-byte7/8 controls, quoted literals, original bootstrap/transport/executable/flags, body markers and mode selection. Recompute actual Source full root and both nested children in LF/CRLF including configured exe path/namespace, quotes, flags, UTF16/Base64 and NUL: all<32,767 with>=1,024 margin for new/changed commands; old SnapshotProof29,352/29,816 and raw SHA2a7eb52b remain exact. External maximum31,224/margin1,543 is feasibility only, not a value to fabricate as Source proof.

### Concrete Verify in required order

1. **Docs/Issue before Source. Verify:** exact3 Docs paths, frozen SPEC and8110 Source/modes/blobs unchanged; full research/readback/26 artifact verification plus independent10-command/raw old-snapshot proof; CLI Issue31 concrete Verify and final Root re-read precede a separate two-file developer lease.
2. **Whole Source/data conservation. Verify:** full reviewed staged/unstaged delta and reverse amendment to original8110 host/Rust blobs; every other296 tracked mode/blob against reviewed Docs head exact. Four finite real catch slots, max5 existing nodes, full signed HResult, no numeric semantic allowlist, default ToJson unchanged, first work precedence, original File/gate/order/owner/release controls,21 modes/all libtest selectors/observer/factory/permits/deps/workflow and45/30/3s/25/5/200ms preserved.
3. **Root source/geometry review. Verify:** complete two-file Source and deterministic OCR rules, no skipped paths; unknown/malformed record fails, bounded scalar grammar95/393/405 independently checked. Actual Source's complete LF/CRLF root/child commands satisfy<32,767 and new>=1,024 margin; old SnapshotProof raw/full commands exact. Static calculation/formatter/diff/locked metadata supplies no native or PS syntax result. Failure requires a Docs-first adjustment, never skipping an assertion or widening a clock.
4. **Combined changed-head native qualification. Verify:** Root integrates only fully reviewed Source into PR136, publishes changed head once for ordinary automatic CI on all original x64 stable/ARM stable/x64 Rust1.94 rows. Named actual four-slot records and every old/new appender/reader/kill/reap/same-candidate release/root/Job/pipe/observer/guard check must succeed at that exact revision; zero-case, missing/late/unknown remains failure. Retain six independent original native CLI case outcomes and remaining library/lifecycle/package/lint regressions distinctly; no retry, warmup, standard-user install/account/privacy or release claim follows. Issue31 stays open until full Verify/required merge succeeds.


### Before-Source syntax follow-up, 2026-10-04

Actual3f5 stock parent-exit assertions passed on all three Windows rows; four managed-record assertions passed inside the nested helper while concrete values remained outer-log unobserved. The two new test Clippy syntax findings are independently planned in WINDOWS_STOCK_CLIPPY_SYNTAX_2026-10-04.md. This selects only borrowed constant-size iteration with literal old slice body and same one-call collision error capture, after exact reviewedmain139 integration. It does not change this heartbeat protocol, managed-record retention, cleanup/publication semantics or original clocks and does not select any readiness/cause repair.
