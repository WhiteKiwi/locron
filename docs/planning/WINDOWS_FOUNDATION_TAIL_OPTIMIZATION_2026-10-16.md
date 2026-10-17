# Remove measured Windows foundation tail duplication

Owners #172/#31; related #149/#162. Base9088/tree3edb6b and its completed
ordinary CI38867063921. Continue the frozen SPEC and N06 amendment.
This explicitly amends only the earlier causal plan's unchanged-tail-target,
unchanged-upload and all-serial-service constraints. No product behavior changes.

## Measured basis and isolated exception

Actual ANSI-stripped step logs distinguish compilation from runtime. Doctor
recompiles190 crates in99s ARM,139s MSRV and141s x64stable. Service relinks CLI in
24.36/28.33/24.43s; ARM HTTP contract recompiles48 crates in31.96s. These are observed
cost ceilings, not promised net savings: new integration links still cost time.
B recovery is an already-built default-feature test target on the same source and
compiler; its full producer record confirms features[], opt0/debuginfo2 and debug
assertions/overflow checks. Qualification variables are runtime reads, with no
custom compile-time environment or repository build script. Preserve separate
production and recovery targets/profiles and both A/B full library contexts.

ARM/x64stable service64 tests consume383.45/382.78s. Two tests deliberately wait
three real60s delays EACH, at least360s of the serial selection. Separate audit
finds64 tests: generic24, dashboard4, activation2, wire8, record11, supervisor15.
Each real fixture owns its temporary root, IDs, worker, Job, cancellation and
child environment. No process-global environment/CWD mutation or service fault
static exists; Core test-only fault globals are not linked into the CLI tests.
Shared SID initialization/filesystem actor remain bounded and unchanged. The
100ms native-expiry and80ms heartbeat controls remain scheduler-sensitive.

The CI guideline defaults to serial testing and removing duplicated setup first.
Select one measured exception: ONLY the existing complete service:: command uses
two outer test workers. Both original real180s minimum-wait assertions and all64
tests remain literal. Core and both complete library contexts retain one worker.
Do not claim an observed speedup until the same native selection actually passes.
An actual new failure requires research/planning, never retry or relaxed timing.

## Coherent target reuse and upload provenance

After explicit prune qualification and BEFORE doctor, a CI-only preparation helper:
- Validates the same native source/compiler/host and absolute existing B recovery
  target, selected from original producer receipts; no guessing or global fallback.
- Uses unchanged scripts/ci_prune_artifact_facts.py to verify ORIGINAL production
  and recovery hashes/full IDs and original binding identity/bytes. Never rebind.
- Uses CargoJSON --no-run to select the already tested original GLOBAL cold-Core
  executable, requiring its matching package/lib/default-feature/test profile,
  exactly one executable, fresh=true, and original target-directory placement.
  Record full identity and hash. No tests or setup are repeated.
- Records a bounded preparation receipt and selects B recovery for EVERY later
  Cargo command, including GUI, diagnostics, lifecycle, maintenance, MCP, history
  and HTTP contracts. Keep every selector/feature argument and success-path scope.

Preparation failure must never silently select the global target. Add only the
native_tail success prerequisite to existing explicit post-preparation guards,
preserving each original predicate; default-success steps keep their defaults.
The post-cold diagnostic keeps if:always() but first reports fixed unavailable
and exits nonzero if preparation did not succeed, before any Cargo invocation.
Successful preparation runs every original diagnostic command through B. This
explicit dependency changes failure-path scheduling, never a successful scope or
the final failed status. Core, producers, libraries and receipt uploads are ungated.

Keep GUI's original feature argument; its feature build may change unbound
debug/locron.exe, but must never change the bound default-feature recovery PE,
separate production PE or binding. No compile-variable identity claim follows
from sharing a target directory. Final verification checks original full bindings
again and the original global cold-Core identity/hash; any mismatch fails.

The old upload expects one coherent directory tree, whereas global and B outputs
can occupy different Windows drives. One CI-only final packager stages upload-only
copies into a fresh checkout subdirectory, preserving the original target/debug
layout, cold-Core, and EVERY original required executable role/glob plus facts,
checksums and Engine evidence. Each executable role must actually exist; bound the
inventory, reject links/reparse/path escapes/duplicates and refuse overwrite.
Record ORIGINAL source path/full ID/hash and verified copy hash in a manifest.
The cold-Core member comes from its original tested global executable, never a
new B Core compilation. Native facts and locron.exe checksum follow the actual B
tail outputs. Existing artifact name/retention/failure semantics remain.

Upload copies are packaging outputs ONLY: never executable inputs, qualification
bindings, guard admissions or replacement originals. This is the sole exception
to the previous no-copy wording, and cannot repair an inadmissible artifact.
Original PR144 and prune uploads/locators/JSON schemas remain unchanged.
No dependency, token setter, native helper, cache relocation, new job/lane,
required-check rename, omitted test, retry or timeout change.

## Ordered work and Verify

1. Parent records actual timing/source audit and this entire selected plan,
   updates Findings/Implementation and the four owning Issues, checks exact
   readback, and rereads the final plan before separate development.
   **Verify:** measured ceilings, unmeasured exception risk and upload-only-copy
   boundary are explicit; prior protected artifact admission remains authoritative.
2. Separate contention_development implements ONLY workflow tail routing, the
   service worker-count exception, and one CI-only two-phase helper using the
   unchanged artifact reader; add focused boundary tests where meaningful.
   **Verify:** all original Cargo invocations/selectors/features and success-path
   scopes remain, except service threads1→2; original failure guards are preserved
   with the explicit preparation prerequisite and diagnostic fail-stop above.
   Core/libs1, matrix,35-minute limit,
   producer bodies/targets, cache and original receipt upload bytes are unchanged.
   Missing/freshness mismatch/wrong target/rebinding/changed ID/hash/unsafe or
   absent required upload members refuse; positive copy manifest hashes match.
3. Parent reviews full staged/unstaged diffs, all fixture protection receipts and
   helper limits; performs actionlint, Python compilation, relevant helper controls
   and fmt1.94/1.98 alongside the N06 Windows test typecheck.
   **Verify:** no product source changed in this amendment; original cold-Core
   source and original production/recovery bindings remain the recorded originals,
   GUI feature variant is explicit, and upload copies never feed native execution.
4. Publish the composed source once ready and qualify ordinary PR CI.
   **Verify:** all3 foundation jobs finish within unchanged35 minutes, ALL64 service
   cases and BOTH full libraries,6 PR144195 ledgers, original complete API+CLI prune,
   all8 original daemon selectors and every remaining native tail pass on the SAME
   source with all14 non-foundation jobs. Inspect actual timings and uploaded
   complete layout/source manifest; record exact head/merge/tree and actual counts.
   No old PASS transfer, merge, issue closure or Windows11 support claim.

## Source leases

contention_development owns .github/workflows/ci.yml selected tail changes and
scripts/ci_windows_foundation_tail.py plus focused
scripts/test-windows-foundation-tail.py if needed. Existing artifact reader stays
unchanged. diagnostic_surfaces keeps its separate two-file N06 lease.
shorten_transactions performs independent read-only fixture/scheduling review.
Parent owns Docs, review, all commits and publication.
