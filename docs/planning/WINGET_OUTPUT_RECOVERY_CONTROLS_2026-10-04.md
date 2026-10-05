# Selected WinGet output recovery controls

Refs #32/#35. Frozen Windows SPEC is unchanged. Root selected this exact finite design after full report/candidate/receipt/composed renderer and relevant primary review. Research is not native qualification: the Root proof verifies69 artifacts and25 Git blob/mode records, complete current main/pr137 tree equality, and exact composed renderer55f41774c7e70d7da17bdf9f47059e2effc4c0881182ede3b7c9802548bb3269. No writer defect is demonstrated. Before-Source Issue readback and final plan review are still required.

## Pinned composition and finite Source scope

- PR140: `70c9b999d6b6b0ddac4691ff8ab9674cdc43ba8b`.
- Merged PR137 / main: `15820ed96cc6da2bd8c37d42ec8788fd2180114b`.
- Old base: `46445881e320f440c02d94d7e9e5838033933fea`.
- The outside-packet composition is PR140's complete pre-`main()` bytes plus
  merged137's complete `main()` suffix. Its SHA-256 is
  `55f41774c7e70d7da17bdf9f47059e2effc4c0881182ede3b7c9802548bb3269`.
  Nothing was applied during research. The separate developer performs the planned Git main merge and this exact Source composition after Docs/Issues/final review; Root handles publication.
- **Selected qualification Source: two paths plus the exact renderer composition:**
  1. new `scripts/test-winget-output.py`;
  2. `.github/workflows/windows-paired-manifest.yml`.
  3. `scripts/render-winget-manifest.py`, solely the exact main() composition described above. Its complete pre-main bytes remain oldPR140 and suffix remains main137. No writer implementation change.
- No renderer refactor, public dependency-injection API, native binding, dependency,
  installer/updater/catalog/release change is needed to test the selected writer.
  If a test demonstrates a real production defect, preserve its failure and ask
  Root to plan that different Source change first.

## Test boundary and ownership

Use the existing `importlib` loading convention to load the actual composed
renderer. Call its existing private `_write_documents` rather than a duplicate
writer or cleanup implementation. Public behavior remains covered by the eight
unchanged entrypoint tests, including the validator and explicit internal legacy
route, and the fourteen unchanged paired-manifest tests.

Every case owns one fresh `TemporaryDirectory` parent, a small fixed three-document
map, absent output, and at most two additional sibling fixture paths. Use spaces
and Unicode in the parent/output name. All marker bytes and expected document
bytes are fixed and bounded (each fixture document at most 4 KiB); this is a test
input bound, not a new renderer limit. The parent sentinel is outside output.

The only monkeypatch seam is existing Python operations, scoped to the selected
output and `xb` mode. Save the real `Path.open`, `Path.lstat`, `Path.unlink`,
`Path.rmdir` and `os.fstat` before patching. Delegate every unrelated call. Never
patch `_same_object`, synthesize its identity, or return a fake stream/file
descriptor. Wrapper `fileno()` always comes from the real owned stream.

At a selected second/third open, raise one preconstructed ordinary `OSError`
before opening that leaf. A selected write error performs a real bounded prefix
write and then raises that same object. A short-write control performs one actual
prefix write and returns its real smaller count. A selected flush error first
delegates real flush and then raises. A selected close error **closes the actual
stream first**, then raises from the context exit. These are injected branch
controls, never evidence that Windows itself failed to write, flush or close.

For all other outcomes `__exit__` closes the real stream and returns false. Keep
all acquired stream objects in the case owner so closure can be asserted after
the writer returns/raises; a wrapper's reported `closed` must delegate to its
real stream. On an unexpected teardown error, the test fails rather than
silently proceeding to a retry. Inject one stage failure at a time; do not invent
a priority promise when both write and context exit independently fail.

Deterministically perform namespace changes at the next `xb` call after earlier
streams have closed. There is no racing thread, new timer, sleep or production
hook. Foreign means **not created by this writer attempt**, not another OS user.
All foreign/control objects are still owned by the disposable case for final
fixture teardown. Assertions precede teardown. Directory replacement renames the
old directory to an absent sibling and creates a new directory at the requested
name; it never recursively deletes either tree. This measures the existing
identity check, not a hostile time-of-check/use race.

## Proposed ten portable selectors (32 explicit sub-controls)

| Selector | Input/effect | Required observable oracle |
|---|---|---|
| `test_success_writes_exact_utf8_lf_and_returns_output` | Fixed three documents, including Unicode and LF | Same output Path; exact three leaf names and independent expected UTF-8 bytes; no CRLF translation; every real stream closed; parent sentinel unchanged. |
| `test_preflight_refusals_have_no_output_or_parent_effect` | Nine bad names (`''`, `.`, `..`, slash, backslash, colon, NUL, None, integer), empty map, second value with unencodable surrogate | Expected original preflight exception; output and missing parents remain absent; no open/create call. First valid document must not be written before the second encoding fails. |
| `test_existing_output_and_ancestors_are_preserved` | Existing directory containing marker, or existing regular file (two sub-controls) | Real `FileExistsError`; exact original object identities/bytes/entries survive; no writer open/unlink/rmdir. Existing ancestors and sentinel untouched. |
| `test_second_and_third_stage_failures_cleanup_and_allow_one_new_attempt` | Positions 2/3 times open/write/short-write/flush/close (ten) | All acquired streams actually closed; same injected origin object except writer-created short-write error; only matching created files removed; output absent, created parents retained. One subsequent **unpatched explicit call** to the same output succeeds with exact bytes. This is not retrying a failed test. |
| `test_cleanup_continues_and_preserves_origin_notes` | One unlink raises cleanup error; origin with normal add_note or add_note=None (two) | Origin identity/type/args unchanged; other matching created leaf still removed in reverse order; retained leaf and directory remain. Python supporting notes gets cleanup diagnostics, not a replacement exception; fallback without add_note retains the same origin. |
| `test_unavailable_created_identities_leave_output_for_inspection` | Initial post-mkdir lstat failure; first real file's fstat failure (two) | Origin survives. Empty directory with unknown identity, or unrecorded newly created leaf, remains; no guessed deletion. An unpatched second call refuses existing output rather than adopting it. |
| `test_disappeared_created_file_is_not_a_cleanup_error` | Remove first recorded file at third open, then inject origin | Remaining recorded matching leaf removed; output empty/removed; original error preserved with no missing-file cleanup note; explicit unpatched second call succeeds. |
| `test_unrelated_entries_survive_nonrecursive_cleanup` | Add unrelated regular file and nested directory/marker before injected third-open failure | Both recorded created leaves removed; actual rmdir refuses nonempty output; foreign subtree byte/identity snapshot unchanged; original exception gets a retained-directory note. |
| `test_replaced_created_file_is_retained_while_other_owned_leaf_is_removed` | Actual replacement of first closed file, then third-open origin | Replacement identity differs while type remains regular file; second matching leaf removed; replacement bytes/identity retained; output retained; original origin plus replacement/retention notes. A second call refuses without changing this tree. |
| `test_replaced_output_directory_retains_new_tree_and_parked_original` | Actual old-directory rename plus new directory, then third-open origin | Distinct ordinary-directory identities; both new foreign tree and parked original tree unchanged; cleanup stops before unlink/rmdir of either. Origin/notes survive; a second call refuses the replacement path. |

The counts are test method counts, not assertions or operating-system error
counts. The declared sub-controls above total 32. Implemented selectors and
actual completed sub-controls must be counted in changed-head results; a method
count alone does not attest each parameterized branch.

## Four additional hosted-Windows selectors (six explicit sub-controls)

The native command must refuse a non-Windows host, Python below the selected
3.11 note baseline, or zero/unavailable fixture file IDs. It must not skip or
downgrade these controls. Record Python implementation/version, pointer width,
reported process architecture and the workflow's actual runner architecture;
do not infer native ARM Python from the ARM host label. The evidence claim is
actual Windows filesystem I/O on that host, not execution of either Locron image.

| Selector | Real Windows operation | Required oracle / injection distinction |
|---|---|---|
| `test_windows_actual_exclusive_collision_preserves_the_interposed_leaf` | At second/third `xb` open, create that leaf using saved real open, then delegate the writer's real open (two) | Real Python `FileExistsError`/errno, optional winerror recorded rather than assumed 183; no injected open exception. Earlier owned leaves cleaned; interposed bytes/identity retained. Unpatched repeat refuses without mutation. |
| `test_windows_actual_replacements_are_detected_by_real_file_identities` | Closed first-file replacement, or closed-directory rename plus fresh directory (two) | Actual `lstat`/`fstat` observations have nonzero IDs; same type/device with changed file/directory ID, not a synthetic stat or type trick. Injected third-open origin triggers real cleanup; replacement and parked trees survive. Namespace replacement is real, originating generation error is injected. |
| `test_windows_readonly_cleanup_error_preserves_origin_and_partial_output` | After two closed files, set first writer-created file's **read-only attribute** with `os.chmod(..., stat.S_IREAD)`, then inject third-open origin | Real `Path.unlink` of that matching file returns PermissionError / winerror 5; other matching file is removed; actual rmdir of retained nonempty output fails. Origin object/notes and retained exact bytes/ID survive. A second call refuses existing output. Restore only this known test-owned file's attribute in checked cleanup; no ACL/owner repair, then explicitly remove test-owned remainder. |
| `test_windows_real_closed_streams_allow_namespace_reuse_and_a_second_run` | Normal writer real create/write/flush/close; after return real file rename out/back and file/directory deletion; one fresh run | Actual stream closed flags and path/fstat ID correspondence, exact UTF-8/LF bytes, actual rename/deletion succeeds after close, parent persists, one fresh call succeeds. This establishes ordinary closure and namespace reuse, not fsync/power-loss durability or OS-failed close. |

Read-only teardown is registered **before** setting that attribute. `unittest`
cleanup order clears that known retained file before removing the fresh parent;
cleanup failure is reported as a test error, never ignored. Before a teardown
attribute change, verify the exact fixture identity still matches the captured
object. Other preservation-test objects are removed only by their case owner
after their assertions. Product code never repairs or adopts retained output.

No native file-lock/ctypes binding, other account, junction privilege, package
operation, WinGet invocation or process target is needed for these controls.
Unavailable native file identity/attribute/replace/close behavior is a hard
failed qualification, not a skip. Disk-full, antivirus, power loss, unconfirmed
native close failure and arbitrary same-account races remain unqualified.

## Small workflow change, not a release gate bypass

Only extend `.github/workflows/windows-paired-manifest.yml`:

1. Add the new test path to both existing PR/push path filters. Append one portable
   recovery command after the existing three commands in the existing Ubuntu job.
   Keep its runner, 5-minute timeout, contents-read permission, concurrency and
   every old command exactly.
2. Add two independent native rows (`windows-2025` / x64 and `windows-11-arm` /
   ARM64), matrix fail-fast false and 5-minute outer job timeout. Use the already
   Root-verified immutable checkout SHA
   `3d3c42e5aac5ba805825da76410c181273ba90b1`; no new setup action/dependency is
   required. Preserve the old job's checkout ref rather than unrelated cleanup.
3. The new native rows verify `os.name == 'nt'`, actual `RUNNER_ARCH` against their
   matrix, and the selected Python prerequisite. Run
   `python -B scripts/test-winget-output.py --native-windows`, then the unchanged
   `python -B scripts/test-winget-entrypoint.py`. Require all 14 new methods / 38
   planned sub-controls and all eight existing entrypoint methods, no ignored,
   filtered or unexpected skip. Native errors or missing Python fail the row.
   No environment fallback, package download, `winget`, executable probe or
   PowerShell command is invoked by these new controls.

The renderer currently has no native-call cancellation or elapsed-time contract.
Do not add a production timeout/retry, invent bounded synchronous Windows I/O,
or relax existing entrypoint subprocess bounds (10 seconds / validator 120).
The new job's 5-minute timeout is only an outer harness bound. Each injection is
one-shot and synchronous; no sleeping/racing worker. These independent jobs do
not modify ordinary CI, release actions, cold Core order or other native gates.
Whether new status contexts become a required ruleset is a parent repository
publication decision, not implicitly established by this workflow candidate.

## Three-or-more-step Verify handoff

1. **Compose immutable reviewed changes before test implementation.** Verify:
   full main tree matches reviewed137; each PR140 writer line/import and its
   recovery plan survive; public `main()` is exactly merged137's paired default
   including `--paired`, validator argv/check/120-second timeout and print order.
   All other paths/modes are the exact reviewed union. No PR138 snapshot Source
   or PR139 listener Source is implicitly imported here.
2. **Add direct portable recovery controls.** Verify: all ten selectors / 32
   sub-controls pass on fresh head; the open/write/flush/close matrix exercises
   actual production `_write_documents`, with real fd-backed streams and
   strict error identity, output/parent effects and the one explicit second call.
   Existing fourteen paired / eight entrypoint cases and their literal bodies
   are unchanged and freshly pass, including validation failure retaining all
   three complete files without printed success.
3. **Add native filesystem oracles.** Verify: actual Windows x64 and ARM64 rows
   report all four additional methods / six sub-controls complete, native
   collision/replacement/read-only-delete/closed-stream reuse receipts, no
   skip/fake identity, and no native write/close fault claimed from injection.
   Failed or ambiguous controls remain failed with captured output, not retried.
4. **Qualify the workflow and conservation.** Verify: path filters and independent
   rows select the new file, existing workflow content can be reversed byte-exact
   by removing the selected additions, actionlint passes and every other tracked
   blob/mode/dependency/build/test fixture is conserved. Exact changed-head
   metadata records head, merge tree, event, attempt, matrix host, each command,
   selectors/sub-controls, skipped steps and raw-log hashes.
5. **Record acceptance at its actual scope.** Verify: Root independently reviews
   the full Source patch and all fresh results before deciding PR140 merge.
   Issue #35 stays open for final immutable public assets, real WinGet structural
   validation/install/upgrade/uninstall/provider and package-owned pair proof;
   #32's clean-machine/published-artifact gates are not inferred from this suite.

## Conservation obligations

- Reconstruct the complete composed renderer byte-for-byte from the two pinned
  reviewed regions. No change to `_same_object`, writer loops, exception/notes,
  native I/O order, YAML schema/contents, pair/legacy API defaults or public CLI.
- Preserve every old test selector/body/assertion/clock in
  `test-winget-entrypoint.py`, `test-winget-paired.py`,
  `test-windows-distribution.py`, raw-ZIP tests and all Rust tests.
- The workflow inverse removes only the new path-filter entries, appended
  portable step and new two-row job; it must yield original merged137 workflow.
- Protected full tree inventory is authoritative. Beyond new test/workflow,
  Source, lockfiles, modes, dependencies, versions, toolchains, release assets,
  package aliases, permissions and process/native/installer/task policies remain
  exact. Root owns planning-doc additions and publication; record their separate
  selected deltas rather than hiding them under a Source-only count.
- Snapshot all proposed fault-boundary counters, actual native observations and
  final raw logs with complete hash manifest. A deterministic before-check
  replacement does not prove atomic unlink protection against a subsequent
  hostile race, and these tests do not promise all-or-nothing visible output.

The Source parent will be this Docs commit before main merge. Record the two-parent Git merge and exact selected inherited main137 path union separately from the three editable Source paths. Import inherited entrypoint/default files only byte-exact; preserve all original PR140 writer/planning bytes. A conflict requiring any other Source decision returns to Root before implementation. No local Python application imports/tests, fixtures, native compiler or Windows effects run; only static text/hash/Git/actionlint and bounded outside-domain checks. All actual tests execute through changed-head hosted CI.


## Exact reviewed main139 integration after Source handback (2026-10-04)

Root reviewed the complete Source handback af824dc3cf2121109fa0fe86026f6af7ce48ca50
before selecting this Git-only integration. Root proof36c7545bc1959077da12bd522092b3ce44f95dbbf3f7ff6f2a49b0020cafd8b2
independently verifies128 artifacts, complete300-path merge union,301-path Source
tree,299 protected modes/blobs, full741-line tests/workflow/OCR rules and exact
renderer55f41774c7e70d7da17bdf9f47059e2effc4c0881182ede3b7c9802548bb3269.
10/32 portable and14/38 Windows are static inventory counts; nothing executed.

This Docs commit is the next Source parent. The separate developer integrates
only exact reviewed and now merged main139814e689bb0717aba183dbd084e6151ba11ac4e99
whose whole tree4b1ecc975f766807f4baafc4ae90e3c41d6b9c0b equals reviewed16e6.
The common main is15820ed96cc6da2bd8c37d42ec8788fd2180114b. No later main,
PR136/138/155 or production adaptation is selected.

1. **Preserve Docs histories. Verify:** both Git merge parents are recorded.
   Resolve append-only FINDINGS/IMPLEMENTATION conflicts as exact common-base
   bytes + complete own branch suffix + complete incoming branch suffix. No
   original suffix is omitted, rewritten or deduplicated; SPEC remains exact.
2. **Import the reviewed union only. Verify:** every incoming main139 mode/blob,
   including complete Server lib and new listener elapsed plan, matches814.
   Keep renderer55f4,741-line new output test, entire additive workflow and all
   existing paired14/ZIP21/entrypoint8/distribution15 bytes exact fromaf824.
   Account for inherited paths independently; no new editable Source or behavior.
   An unexpected Source conflict returns to Root before any adaptation.
3. **Review complete integration. Verify:** full staged/unstaged patch before a
   typed integration commit; complete Git union/modes/blobs and literal Docs
   suffix proof, renderer/test/workflow inverses and all Source hashes. Static
   actionlint/diff/OCR are sufficient for unchanged Source; no local app/test/
   native/compiler/PowerShell/account/task/PATH/policy/probe effect.
4. **Qualify genuinely changed publication. Verify:** Root independently reviews
   the clean handback and ordinarily FF-pushes PR140 once. New Ubuntu command
   actually reports10 methods/32 completed controls; independent x64/ARM Windows
   each14/38 with actual Python/process/host metadata and returned errno/winerror/
   nonzero identities, followed by unchanged entrypoint8. Collect every actual
   outcome and raw hash, no carryover/skip/rerun. All required CI stays required.
5. **Keep wider acceptance open. Verify:** Root records Issues32/35 and considers
   PR140 merge only after current required checks pass. Public final bytes,
   unsigned UX/standard-user installation, real WinGet validate/catalog/provider/
   install-upgrade-remove and lifecycle ownership remain their existing gates.

Root owns this planning delta and publication; the separate lease is integration
only. Existing new tests remain reviewed Source, with no import/test execution on
the owner PC. Current unsigned decision/signing37 deferment are unchanged.
# WinGet output recovery controls — 2026-10-04

Status: selected before-Source plan. Refs #35. Frozen SPEC; best-effort recovery only.

## Evidence and scope

Original PR140 at70c9b999d6b6b0ddac4691ff8ab9674cdc43ba8b has a reviewed15-line recovery plan and a guarded exclusive writer, but its existing tests do not cover the selected failure/recovery paths. Separate read-only research and parent full Source review found one concrete diagnostic gap: FileNotFoundError from output-directory lstat is swallowed by the individual-leaf absence handler, so a renamed-away output tree can retain manifests without an incomplete-cleanup note. The original exception propagates; no foreign deletion was observed.

Earlier Issue35 comment5977693549 specifies missing portable/native Verify controls. Its referenced unpublished Docs9038ed6e object could not be fetched (not our ref), so this plan is grounded in the available original plan, exact Source and the actual Issue comment, not a claim to have read that unavailable199-line plan. Separate research proposal SHA-2566cd701f353466fc03f72f67e8a744f2ee2f1653a425fb57df8e492b43e963b04 records the finite control design; no control was executed in research.

## Selected implementation

Normal-merge exact reviewed main814e689bb0717aba183dbd084e6151ba11ac4e99 before new Source. Preserve original140 whole pre-main and main814 whole main() (the latter is reviewed PR137 paired CLI default). Their pre-fix composed renderer SHA-256 is55f41774c7e70d7da17bdf9f47059e2effc4c0881182ede3b7c9802548bb3269. Preserve both Docs appendices.

Lease only scripts/render-winget-manifest.py, new scripts/test-winget-output-recovery.py and .github/workflows/windows-paired-manifest.yml. In the writer cleanup, check the created output-directory identity in its own exception boundary before each recorded leaf. Unavailable/missing/changed directory identity annotates the primary exception and stops leaf cleanup; only an individually missing leaf continues. A missing output directory at final rmdir also annotates cleanup not confirmed. Preserve original exception identity, recorded file identity tests, reverse/nonrecursive cleanup, exclusive creation and all successful output bytes. No chmod, recursive deletion, removal of parent directories, retry inside production, new dependency or path-security claim.

Keep three manifest strings/schema/names/URLs/sole console aliases, bounded ZIP/checksum inspection, public paired CLI default, explicit internal legacy route, and120-second structural validator outside generation recovery exactly. Tests use frozen expected full manifests from the old templates and actual input digests; independent successful byte checks may not simply echo the new writer result.

## Finite portable controls

Exactly10 method groups contain the following32 independent completed control IDs. Ubuntu runs these without a skip. On Windows the same controls run before the six native additions.

|ID|Condition and oracle|
|---|---|
|P01|paired renderer: both real x64/ARM64 ZIP inputs, three full manifest bytes equal frozen old renderer output|
|P02|explicit internal legacy renderer: full three-file bytes equal frozen old renderer output|
|P03|writer valid Unicode content and LF: exact UTF-8 bytes under a Unicode scratch path|
|P04|unencodable text: no directory or file creation|
|P05|escaping ../ leaf: no output or outside file effect|
|P06|empty document map: no output effect|
|P07|pre-existing directory and marker: identity and bytes unchanged|
|P08|pre-existing regular output file: identity and bytes unchanged|
|P09|pre-existing directory symlink: link and owned target unchanged; setup failure cannot skip|
|P10|single injected second-file open failure; original error, real closed streams, complete cleanup and unpatched retry|
|P11|single injected third-file open failure; original error, real closed streams, complete cleanup and unpatched retry|
|P12|single injected second-file write exception; original error and complete cleanup/retry|
|P13|single injected third-file write exception; original error and complete cleanup/retry|
|P14|second-file real positive prefix write returning that short count; full refusal/cleanup/retry|
|P15|third-file real positive prefix write returning that short count; full refusal/cleanup/retry|
|P16|single injected second-file flush failure, actual close, original error and cleanup/retry|
|P17|single injected third-file flush failure, actual close, original error and cleanup/retry|
|P18|second-file actual close then injected close exception; original error and cleanup/retry|
|P19|third-file actual close then injected close exception; original error and cleanup/retry|
|P20|foreign second leaf appears at actual exclusive-open boundary; retained identity/bytes and owned earlier leaves removed|
|P21|foreign third leaf appears at actual exclusive-open boundary; retained identity/bytes and owned earlier leaves removed|
|P22|first created leaf renamed and replaced before primary failure; replacement and moved original unchanged, note present|
|P23|first created leaf removed before primary failure; other owned leaves removed without spurious missing-leaf cleanup failure|
|P24|unrelated marker before primary failure; own leaves removed, marker/root retained, incomplete cleanup noted|
|P25|created directory renamed and foreign directory substituted before primary failure; both actual IDs/trees retained, note present|
|P26|created directory renamed away with no substitute before primary failure; moved tree unchanged and incomplete cleanup note present|
|P27|one injected directory lstat failure after mkdir: unconfirmed directory retained, primary error and note preserved|
|P28|one injected second-file fstat failure after real open: unconfirmed file retained, prior tracked leaf removed, primary error/note preserved|
|P29|one unlink cleanup OSError following a distinct primary error: primary object unchanged; unremoved owned leaf/root and note retained|
|P30|one rmdir cleanup OSError after all tracked leaves removed: primary object unchanged; empty same-ID root and note retained|
|P31|main validator failure after actual paired generation: three exact complete files retained; primary validator error; no printed success|
|P32|main validator success after actual paired generation: exact argv/check=True/120s; complete bytes; single output path line|

Method groups: P01,P02,P03; P04,P05,P06,P07,P08,P09; P10,P11; P12,P13,P14,P15; P16,P17,P18,P19; P20,P21,P22,P23,P24; P25,P26; P27,P28; P29,P30; P31,P32.

## Actual Windows controls

Exactly4 method groups contain6 controls, selected by --windows on both original hosted architectures. Injected stage errors remain labeled injected; actual OS errors/IDs are reported from their real returned values. No guessed raw Windows code is accepted.

|ID|Condition and oracle|
|---|---|
|W01|third leaf created by hook using real file I/O; source exclusive open yields actual Windows OSError; no fake errno or IDs|
|W02|closed first leaf moved and replaced using actual Windows filesystem operations; distinct nonzero IDs retained through cleanup|
|W03|actual old directory renamed and new directory substituted; actual distinct nonzero IDs and both trees preserved|
|W04|owned first leaf made readonly after its stream closes; actual unlink refuses; same-ID file/read-only flag persists and original error plus cleanup note retained|
|W05|successful generation: closed streams permit real rename/delete/root removal and later exact same-path generation|
|W06|one injected close error after real close: source cleanup confirmed; real same-path retry/rename/delete succeeds|

Method groups: W01; W02,W03; W04; W05,W06.

## Fixture and workflow ownership

Use real fd-backed streams; each injected stage error occurs once at the specified second/third production boundary. Delegate actual write/flush/fileno/close, including an actual positive prefix for short writes and real close before a late-close exception. Check primary error identity, closed streams, remaining real object IDs/bytes and notes; only confirmed-cleanup cases attempt one unpatched same-path retry. No whole fake streams/stat results or synthetic native success.

All scratch roots and foreign-for-production fixtures are test-owned and registered in an actual dev/ino/type ledger. Before readonly mutation, register exact-ID guarded restoration; recheck the root and original leaf before restoring its original mode/deleting. Do not repair/delete substituted unknown objects or rely on TemporaryDirectory/rmtree automatic chmod callbacks for native fixtures. Completion is recorded only after assertions and all required fixture teardown. Reject duplicate/missing IDs; setup/teardown failure remains failure. An unsupported symlink setup cannot skip or change host policy.

Record actual Windows OS build, Python version/executable/ABI, runner architecture and process architecture (including interpreter platform and pointer width). Compare expected matrix architecture to the actual native interpreter; emulation is not native ARM64 evidence. Use the existing runner labels windows-2025 and windows-11-arm, fail-fast:false, actions/setup-python@v6 with python-version:3.14 and explicit matrix architecture:x64/arm64. Preserve contents:read, concurrency cancellation, the original Ubuntu job and its three commands/5-minute timeout; add only new test path filters, one portable step and an independent two-row Windows fixture job with5-minute timeout. Use python -B. No ordinary Rust CI/package/service/deadline/filter change. Official setup-python inputs document x64/arm64 selection; the current official python-versions manifest has stable3.14 builds for both win32 architectures. This selects one ordinary CPython interpreter, with no freethreaded variant or emulation. The fixture must still verify the actual interpreter architecture.

## Five steps with concrete Verify

1. Docs/Issue before Source. Verify: parent re-reads this complete plan, FINDINGS/IMPLEMENTATION, frozen SPEC and available Issue35 history; exact REST comment readback precedes separate developer Source. Preserve original Source until handoff.
2. Reviewed ancestry and diagnostic correction. Verify: normal main814 merge conserves both Docs histories and the exact composed renderer; the documented cleanup diagnosis is the only renderer delta, inverse restores the composed blob, all manifest templates/main()/validator120s are exact, every other inherited mode/blob is conserved.
3. Portable controls. Verify: complete32 IDs in10 methods use actual files/closed fd-backed streams and specified single injections; original exception identity, cleanup/retry, ambiguity/foreign retention and validator boundaries pass. Full old paired14 and entrypoint8 Source blobs remain exact. No owner-PC effectful fixture or compiler run; execution evidence comes from hosted jobs.
4. Native controls and workflow publication. Verify: each native architecture executes portable32+native6, actual nonzero identities/errors, guarded readonly restoration and closed-handle reuse; log native process architecture without guessing. Workflow inverse restores exact main814 except selected filters/newsteps/newjob; actionlint and full Source/Docs/staged/unstaged parent review precede ordinary changed-head push.
5. Exact-head qualification and merge. Verify: Ubuntu portable32 plus original paired14/rawZIP21/entrypoint8; both Windows rows38 controls and entrypoint8 with no skipped or failed control; all ordinary required native/lint/package/coherence contexts and exact paired ZIP provenance succeed against current main before merge. If main advances, normal integration and fresh changed-head evidence are required. No bare rerun can replace failed evidence. Retain Issues32/35 wider clean-machine/public release/catalog/install/upgrade/uninstall acceptance open.


## Concurrent existing-suite reconciliation before further Source

The parent independently read remote4a978f20a1160ca035eb8fba9adf9b2f1b292802, its complete741-line scripts/test-winget-output.py,92-line workflow,156-line composed renderer and245-line selection. Separate read-only receipt SHA4c66eb7f4e02b4b33056807463b18737ff5401b062600767d94b7b1f5a0a136e verifies all old14/8 test blobs, exact renderer55f41774c7e70d7da17bdf9f47059e2effc4c0881182ede3b7c9802548bb3269 and normalmain814 union. Reuse this existing suite per Issue163; do not add the separate test-winget-output-recovery.py candidate. Preserve that uncommitted candidate privately with actual hashes before reverting only our own three selected WIP paths for the normal remote merge. Preserve remote4a ancestry/all unleased Source byte-for-byte and both Docs histories: remote complete blobs as canonical prefixes plus literal unique local90d/reconciliation appendices; originalPR140/main139 content must survive once. Unexpected Source conflicts return to parent before adaptation.

This selection supersedes only the candidate suite/path/count choices above. Frozen SPEC/product behavior stays exact. New Source lease is scripts/render-winget-manifest.py, existing scripts/test-winget-output.py and .github/workflows/windows-paired-manifest.yml. Keep every original remote10 portable and4 native selector, all32 portable/6 native exact control keys, all existing assertions and real prefix-write/flush/actual-close-before-injected-close semantics. No smaller replacement matrix. The original3f5/b131/73d3 native failures are unrelated pending PR136 evidence, not a reason to waive ordinary CI.

The original renderer missing-directory diagnosis persists: output-directory and created-leaf checks share a FileNotFoundError handler and final missing output silently passes. Separate directory checking/error-note boundary from leaf handling, stop remaining deletion when the directory identity is absent/changed, and add the final missing-directory retention note. Existing missing-created-leaf continue remains valid. Preserve primary exception object/type/args, successful bytes, templates, main/validator argv/check/120s, reverse/nonrecursive exact-object cleanup and all other renderer bytes. A06 must fail the pre-fix writer's omitted-note oracle and pass the selected correction on hosted execution; static reasoning is not a measured test result.

Existing CaseOwner must use explicitly owned mkdtemp and an actual root/parent/directory/leaf/symlink identity ledger for every fixture creation/move. Register the owner before fallible preparation. Guard exact root/ancestors/currentleaf immediately before every path teardown/attribute mutation; unknown/replaced IDs fail and retain, no adoption or recursive cleanup/auto-repair/finalizer. Close all actual owned streams first; verify actual original mode before the readonly change and preregister its restoration. Restore that saved original mode only after root/parent/leaf IDs still match; do not normalize a replacement or guess identity. All fixture-owned foreign/replacement/parked trees are nonrecursively removed only after preservation assertions; no TemporaryDirectory/rmtree. Missing already removed recorded objects may be checked as absent. Failed/unfinished teardown prevents completed control receipt.

Preserve the original actual readonly control PermissionError/winerror==5 assertion and its origin/bytes/ID/mode observations. Microsoft DeleteFileW documents read-only file refusal as ERROR_ACCESS_DENIED (https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-deletefilew). This is an existing specific negative oracle, not a guessed code for other operations. Continue recording all actual returned errno/winerror values; do not widen allowed errors to make this assertion pass.

Add the ten portable control labels A01–A10 without removing old keys:
- A01/A02 in test_rendered_manifests_match_frozen_full_bytes: actual paired and explicit internal legacy ZIP inputs; independently frozen complete three-manifest UTF8/LF bytes, actual ZIP hashes. No production renderer used as its own oracle.
- A03 test_existing_directory_symlink_preserves_owned_target: actual directory symlink, unchanged link/target IDs/bytes, no output adoption. Unsupported setup fails rather than skips or gaining privileges.
- A04/A05 test_portable_exclusive_collisions_preserve_interposed_leaf: actual second/third xb collision and returned FileExistsError; preserve foreign bytes/ID and original earlier cleanup assertions.
- A06 test_renamed_away_directory_has_cleanup_not_confirmed_notes: actual output moved without substitute before third-open origin; exact parked tree and separate directory/final missing cleanup notes; preserve primary exception.
- A07 extends test_unavailable_created_identities_leave_output_for_inspection: one second-fstat failure after real open, actual close, first tracked leaf removed and unknown second retained with notes/original exception.
- A08 test_empty_output_rmdir_failure_preserves_primary: one distinct injected rmdir after matching leaves removed; retain empty same-ID output and original exception/cleanup note.
- A09/A10 test_validator_preserves_frozen_complete_manifests: actual paired main generation with only validator failure/success controlled; preserve three full independent bytes, exact validator argv/check/120 and no printed success on failure or one exact path line on success.

Add native label N07 in test_windows_late_close_error_allows_real_cleanup_and_namespace_reuse: one second-file close error after actual close, original Source cleanup, one explicit unpatched retry and real rename/delete/rmdir with actual IDs/closed streams. Existing native six remain exact. These additions give16 portable methods/42 controls; native adds5 methods/7 controls for21 methods/49 controls. Freeze the complete old method:label key-set plus A01–A10/N07. Reject unknown/missing/duplicate/skip/unexpectedsuccess/unfinished keys as well as wrong cardinality; emit completion only after all assertions and required checked teardown. Preserve actual operation counts/error observations, without claiming injected write/flush/close failures are OS failures.

Use reviewed actions/setup-python@v6 ordinary CPython3.14 with explicit x64/arm64 in existing two native rows; retain existing checkout SHA and all original Ubuntu commands. Assert actual Windows, RUNNER_ARCH/matrix, CPython3.14,64-bit pointer, process architecture and sysconfig.get_platform native ABI; print checked actual metadata and fail emulation/mismatch. Keep5min/failfastfalse/permissions/concurrency/pathfilters; extend existing file filters/commands only as selected. Existing entrypoint8 and paired14/rawZIP21 remain byte-exact, no release/winget/executable/task/owner-PC effects.

1. Docs/Issue before Source. Verify: parent reviews full remote Source/Docs/proposal, preserves current WIP privately, commits this reconciliation and gets exact Issue35 GET readback before separate developer mutation. Frozen SPEC and Issue163 routing remain clear; literal old Docs history retained.
2. Normal remote integration and confined writer/test ownership. Verify: actual two-parent merge conserves remote4a ancestry/unleased modes/blobs and local unique Docs; renderer inverse restores55f4; existing32/6 selectors/keys/assertions persist; only chosen three Source paths change, actual ledger/no-auto-repair/saved-mode behavior is fully reviewed.
3. Expanded finite portable qualification. Verify: hosted Ubuntu executes16 methods/42 completed exact keys, including pre-fix A06 diagnostic regression, full paired/legacy/validator bytes and all single real/injected fault branches; old paired14/rawZIP21/entrypoint8 freshly pass unchanged. No owner-PC application import/fixture/compiler execution; static AST/actionlint/diff/inverse are not runtime passes.
4. Actual native qualification. Verify: both actual native CPython architectures execute21 methods/49 keys plus unchanged entrypoint8, all old readonly/collision/replacement/close assertions and new late-close reuse; actual metadata/IDs/errors/cleanup/no-skips qualify. Record raw hashes and preserve failures without unchanged rerun or numeric-error waiver.
5. Current-base publication and acceptance. Verify: parent reviews full clean handback/protected inventory and fresh remote before ordinary FFpush; final current-main ordinary required gates/Guardian/paired provenance and changed-head independent workflow pass before ordered merge. Keep Issues32/35 wider immutable public bytes, clean-machine WinGet/catalog/install/upgrade/remove/ownership acceptance OPEN; no public tag/submission.

## Follow-up publication after PR 140 merge (2026-10-04)

PR 140 was merged independently as c4ef8b90276a5119a46eeaac17c931abad4382ae; its entire tree equals reviewed head 4a978f20a1160ca035eb8fba9adf9b2f1b292802. The selected follow-up Source is already implemented in separate-developer commit 2d6a1c4d6339ae174fbfca4d8dc5022914766153 and parent-reviewed in full: renderer160 lines, existing test suite 1171 lines and workflow98 lines. Original149 assertion calls, old32+6 keys, old selectors and protected 299 paths remain conserved. No new42/49 controls have run; original PR 140 CI success belongs to its original Source only.

1. Record this publication scope and Issue 35 Verify before Git reconciliation. Reuse the existing suite and previously selected three Source blobs exactly, keeping the directory-diagnostic fix, explicit identity ledger/guarded teardown, A01–A10/N07 and selected native CPython3.14 rows. **Verify:** parent independently compares complete Source hashes, old assertion ASTs, writer/workflow inverses and all nonleased mode/blob paths; no alternative suite or production/native behavior change appears.
2. Normal-merge exact published mainc4ef8b9 into the reviewed private branch, preserving both histories. If Git reports overlapping added/modified Source, choose the already-reviewed full2d6a1c4d blob for only the three leased paths; every other Source must equal main. Keep canonical published Docs plus every unique literal local 90d/973/publication appendix, with no duplicate shared heading. Publish an independent follow-up Draft and link PR 140/Issue 35. **Verify:** complete merged index equals that three-blob/main union, both parent commits remain ancestors, each original Doc line is byte-preserved in order, and the PR diff contains only the selected Source and planning appendices.
3. Require fresh exact follow-up head hosted ordinary/lint/Guardian/package provenance and standalone Ubuntu42 plus native Windows x64/ARM64 49 controls each, exact completion keys, no skip/unknown/duplicate and actual native CPython3.14/64-bit process/ABI facts. **Verify:** save actual run/job/attempt/check/log and artifact/tree evidence; re-integrate a later main and qualify that actual candidate before merge. Keep failure/unobserved results explicit and Issue 35's clean-account, real installed WinGet/public release acceptance open; no local effectful fixture, installed Locron/service, public tag/release or support promotion.

## Measured Windows A03 path-representation repair before Source (2026-10-04)

Follow-up PR166 ef46a98d3d7462b4e9fb94825f982995553b7222 automatic paired-writer run37198110854/attempt1 completed: Ubuntu executes42/42 controls; Windows x64 and ARM64 each execute48/49 with one A03 failure at test-winget-output.py820. The actual readlink result uses the extended Windows namespace prefix while the original target variable uses the drive-path representation. Both fail the lexical equality assertion; neither log establishes a production writer failure. The control completion-key guard correctly refuses the missing A03. Native job111424057432 raw369651 bytes/SHA42c041968b5519379bb84f2edb781bbd7f4c1cc4a1f27871df0f08b9acd06b7a; job111424057440 raw363083 bytes/SHAe80a351facca5653732301b48c9b4e38653751d57b5e7fcdd787a030d6ebb294.

Python3.14 primary documentation states that Windows os.readlink returns the substitution path, commonly with the extended namespace prefix, and Path.readlink delegates that behavior. Path.samefile checks whether two paths identify the same file and raises on inaccessible files. Sources: https://docs.python.org/3.14/library/os.html#os.readlink and https://docs.python.org/3.14/library/pathlib.html#pathlib.Path.samefile. No string stripping, lexical normalization or guessed Windows target substitution is selected.

1. Lease only test_existing_directory_symlink_preserves_owned_target inside scripts/test-winget-output.py. Immediately after actual test-owned symlink creation, retain the actual readlink result and assert actual samefile with the test-owned target before invoking the renderer. After the existing FileExistsError refusal, compare readlink exactly with that saved actual result and assert actual samefile again. **Verify:** unchanged link representation plus actual target identity are checked on both sides of the real writer call; inaccessible/broken/replaced targets remain failures.
2. Preserve the symlink's independent no-follow identity assertion, full target snapshot, zero open/unlink/rmdir assertions, parent identity and checked CaseOwner cleanup/completion. Keep all original149 assertion calls outside the selected A03 correction, every control key/count, all other methods and complete renderer/workflow bytes exact. **Verify:** full-file inverse and all other modes/blobs prove the one-method Source slice;42 portable/49 native selections, no skip and all5-minute workflow horizons remain exact.
3. Root Docs and exact Issue35 readback precede a separate Source developer. Parent reviews the complete diff/inverse/static AST and publishes a normal fast-forward from ef46. **Verify:** fresh changed-head paired-writer Ubuntu42/x64Windows49/ARM64Windows49, ordinary required CI/lint/Guardian and paired provenance must pass on the actual current-base candidate before merge. This records a measured test expectation correction, not a waiver or unchanged failed-head rerun.
