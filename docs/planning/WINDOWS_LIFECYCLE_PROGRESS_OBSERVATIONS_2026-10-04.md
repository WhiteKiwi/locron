# Windows lifecycle progress observations — 2026-10-04

Status: selected before-Source plan. Frozen product SPEC; observation only. Issue31 remains open.

## Evidence and exact scope

Ordinary CI37180022092 at1c45 has x64MSRV progress failure after dashboard exit/baseline and x64stable descendant readiness failure before hard_stop; ARM16 passes. Root research proof36d67e15576565b4dbe03694292a66e4e07783de2e228d280ba1e47a04eec960 verifies119 artifacts/35 Source records. Existing lifecycle52426bytes/SHA45db1848f326a7cf80ee656d36f6415c8d8ce93676581726c889d5c896c53165 is exact at integratedd894a243619c93dacfdd34e9c7ec5ab068c588be. No actual child status/native cause or repair is established.

Lease only crates/locron-cli/tests/windows_lifecycle.rs: three original failing deadline assertions, private stack-only observation/formatter/read wrapper, original descendant lazy loop and two existing SSE/idle continuation callers. Leave Fixture::heartbeat/other callers, all16 selectors, native publishers, argv/environment/Stdio, production/deps/workflow and all other actual Docs-base mode/blobs exact.

## Selected closed observation contract

Use only already-returned results: one original read_to_string and, on success, one original u64 parse; descendant first executes is_file once and lazily evaluates those calls. Keep original Option and baseline current.max(previous)/current>baseline/descendant>0 predicates. Every iteration resets last/relation/marker; never label an unevaluated operation as observed. Keep first I/O kind/raw once across continuation baseline→advance; project/drop existing error objects at the original handling boundary. Pass actual returned dashboard ExitStatus into the private continuation helper from both original callers; this is not target status or fresh daemon liveness.

Suffix prefix windows-lifecycle-progress-failure; ordered8 fields:

|field|closed domain|
|---|---|
|site|preserved_baseline/preserved_advance/descendant_pre_crash|
|last|unobserved/io_error/parse_error/numeric|
|relation|unobserved/not_compared/zero/nonzero/nongreater/greater|
|marker|unobserved/false/true|
|first_kind|unobserved/not_found/permission_denied/interrupted/invalid_data/would_block/timed_out/other|
|first_raw|none or canonical complete signedi32|
|dashboard_success|unobserved/false/true|
|dashboard_code|none or canonical complete signedi32|

Unobserved first_kind implies rawnone; unobserved dashboard_success implies codenone. Descendant dashboard fields stay unobserved/none. Numeric baseline is not_compared, advancement uses greater/nongreater, descendant uses nonzero/zero. Successful original predicates still return/break and emit nothing. Format only on original assertion failure: preserve original prefix and predicate; suffix226bytes/complete273 includingCRLF, no path/PID/text/counter/error-string/configured value. Closed models supply no native acceptance.

Keep exactly original3s shared baseline+advance,30s descendant,20ms sleep, native reads and full existing process/lock/TempDir/capture/cleanup ownership. No additional Store/attempt/capture/native/status call, Child::try_wait, descendant output redirection, diagnostic threads/globals/owners, early hard_stop, retry/serialization/cache prewarm/clock reset/widening or production/public API change. Existing synchronous read and Drop kill/wait are not preemption guarantees.

## Four steps with concrete Verify

1. Root reviews research/actual Source stages, records FINDINGS/IMPLEMENTATION/this plan and Issue31, confirms readback, then rereads this final plan before separate development. Verify: exact run/head/stages/unobserved limits and frozen one-file boundary; no causal repair or Source effects before lease.
2. Separate developer implements only this one file on the explicit reviewed Docs head. Verify: complete unified-hunk and operation/block inverse to52426bytes/SHA45db; every other Docs-base mode/blob conserved, all16 selectors/old predicates/single-call lazy order/args/environment/Stdio/owners/clocks exact. b664/15ca/main15820 Sources remain exact; no fake native evidence or lint hiding.
3. Root reviews every changed hunk/helper/caller. Verify: closed domain226/273, first-I/O immutable/iteration-unknown shapes and genuine returned scalar projection; formatting/locked metadata separately from hosted compilation/native acceptance. No owner-PC native/PS/parser/reflection/compiler/effectful fixture checks.
4. Publish one reviewed changed head for the unchanged ordinary native x64MSRV/x64stable/ARM rows and cargo test -p locron --test windows_lifecycle --locked. Verify: exact head/event/attempt, all16 names with original assertions and no ignored/filter/skip/serialization/retry; retain failures/new fixed fields or actual original successes plus downstream gates. Green does not establish1c45 cause; wider acceptance/publication remains open.
