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
