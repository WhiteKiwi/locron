# Repair the known-junction fixture binding

Owners: #149 (explicit prune), #172/#31 (current Windows CI), #162 (PR144 evidence).
Base: 9088a1b5f33bffbb2145c84073e8aef6f9d77808, tree
3edb6b640e6424094683498368e518ba12e660c2. Frozen SPEC remains unchanged.
This is a narrow follow-up to the causal foundation plan, not a production path-policy change.

## Actual evidence and decision

[Run38867063921](https://github.com/WhiteKiwi/locron/actions/runs/38867063921)
checks out merge b22cd729ac4f4d3157b92f375f43bdf9eb4d934b with the SAME tree.
All3 Core148 gates and all6 PR144195 ledgers pass, including39 runtime cleanups
per ledger. All6 library N08 rows now preserve the readonly file, return
HTTP500/state_error, retain first prune_pending and leave the second finalized.
All14 non-foundation jobs pass. The sole library failure in every context is
N06-api's initial "actual known junction target changed" assertion, BEFORE its
actual prune call. All3 explicit-prune receivers stop there; their CLI commands
have NOT executed. Both x64 jobs also cancel at the unchanged35-minute job limit.

Separate research confirms both Rust1.94/1.98 read_link can strip a short verbatim
drive prefix, while Core normalized_path deliberately uses that prefix. Actual
returned spelling is not recorded in this run; the representation cause remains
source-backed until closed native observations establish it. An unequal spelling
alone is not evidence that the held target or its bytes changed.

Change ONLY the two Windows N06 fixture setup comparisons. Under the ORIGINAL row
deadline, canonicalize the actual read_link target and compare with the held
target's normalized path; independently admit the ORIGINAL actual path through
existing root_id/PrivateDirectoryPlan::inspect_until and compare the COMPLETE
volume64/file128 ID with the recorded target ID. The original intended-target
DirectoryGuard remains live. Canonicalization is a following observation, never
no-follow authority, permission repair or production admission. Unknown, different,
unsafe or late observations fail. No fallback, arbitrary prefix stripping or
case folding is selected.

Before the aggregate assertion, print one closed N06 observation containing only
version, surface, row, fixed phase and booleans original_path_equal,
canonical_path_equal, full_id_equal. No raw paths, IDs, canary bytes or arbitrary
error/exception text. These facts can establish a representation mismatch to the
same privately admitted object; they do not identify the exact prefix class.

Preserve the first actual read_link PathBuf in the existing tuple. All later
byte-exact read_link comparison, intended target ID, canary ID/bytes/attributes,
outputs-parent ID, reparse type, original refusal/durable oracles and nonrecursive
known-link cleanup remain literal. No junction-self ID or hostile same-account
replacement guarantee is added. Unix and product code remain unchanged.

## Ordered work and Verify

1. Record evidence, decision and source lease before implementation. Parent updates
   Findings/Implementation, publishes this entire amendment to the four owners,
   verifies exact readback and rereads the completed plan.
   **Verify:** frozen SPEC; actual CLI nonexecution and source-backed spelling
   hypothesis remain explicit; only two Windows fixture comparisons are leased.
2. Separate diagnostic_surfaces development changes the two N06 setup blocks only.
   **Verify:** patch inverse restores both original files; every later row/oracle,
   first actual spelling, guard lifetime, clock and cleanup byte is unchanged.
   Closed observations contain no raw private data. Different canonical target or
   full ID still refuses; canonical observation never substitutes private admission.
3. Parent reviews the complete diff and qualifies the composed source.
   **Verify:** fmt on1.94/1.98, affected strict checks and Windows test typechecks;
   ordinary native CI must pass BOTH full library contexts, original complete
   explicit API+CLI prune keys and6 PR144195 ledgers on x64 stable/MSRV and ARM64,
   with all14 other jobs. No retry/skip/oracle weakening/time-limit extension.
   CI duplication repair requires its own researched amendment BEFORE source.

## Lease

diagnostic_surfaces owns ONLY
crates/locron-server/src/api_prune/qualification.rs and
crates/locron-cli/src/explicit_prune/qualification.rs N06 Windows setup blocks.
Parent owns planning/review/commits/publication. Other source remains frozen.
