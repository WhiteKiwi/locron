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
