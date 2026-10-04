# Windows stock test Clippy syntax — 2026-10-04

SPEC stays frozen. This plan removes two observed warnings-denied test syntax
findings without changing heartbeat publication, partial append or ownership.
Root owns planning/review/publication. A separate developer owns integration
and Source. Actual execution is disposable hosted CI only.

## Evidence and exact pins

- Native Source3f5fb95e9d61477f4e5975f1165af63428411019, original stock file53398
  bytes/SHA248100e55b1013464a392daadcb3585821281d6b7240c1872ee15c7305f6b857.
- Completed run37184851251 Windows lint111384541928: actual test Clippy findings
  chunks_exact_to_as_chunks375:34 and manual_let_else1015:5. These are not causes
  of the separate negative-peer readiness or withheld-entry failures.
- Separate frozen9-file research manifest1720b73b9a7634e84ab0a32185e0a663dd6b75020f004340e44dafdb2b480c46;
  pinned [Rust1.94 slice source](https://raw.githubusercontent.com/rust-lang/rust/1.94.0/library/core/src/slice/mod.rs)
  and [array source](https://raw.githubusercontent.com/rust-lang/rust/1.94.0/library/core/src/array/mod.rs).
  Safe as_chunks1.88 and as_slice1.57 predate MSRV1.94; API source is not compiler acceptance.
- Reviewed and now merged main139814e689bb0717aba183dbd084e6151ba11ac4e99
  equals reviewed16e tree4b1ecc975f766807f4baafc4ae90e3c41d6b9c0b. Its complete
  fresh native/lint qualification and Root merge receipt are independent proof.

## Selected boundary

The Source parent is this Docs commit at native3f5 before exact main139 merge.
Record the two-parent Git merge and the entire inherited main139 path union
separately from the sole editable Source path: loader_crash.rs. Only reviewed
main139 Server Source may be imported byte-exact; keep every native/stock/CLI
helper/lifecycle/workflow path unchanged. Resolve append-only FINDINGS and
IMPLEMENTATION Docs conflicts as common base plus each literal branch suffix;
preserve both original byte sequences and this plan. Any other conflict returns
to Root before adaptation. Do not fetch/adopt a later main implicitly.

Replace the complete-record loop with safe as_chunks::<APPEND_RECORD_BYTES>(),
its first tuple member's iter/enumerate, and immediately borrow record.as_slice().
The old &[u8] loop body must remain literal. Keep the count division, independent
tail slice and validation literal; a proper partial tail must still be accepted.
No exact-divisibility assertion, new allocation, copy or unsafe block is selected.

Replace only the collision match initializer with let Err(collision) = the same
HeartbeatAppender::create call else the same panic. Keep arguments, one-call
evaluation, panic text and next AlreadyExists assertion. Normal Err creates no
appender owner; unexpected Ok diverges and remains ordinary temporary/unwind
cleanup. Exact destruction-instruction timing relative to panic machinery is
outside this claim. No explicit early drop, query, retry or synthetic Drop test.

## Ordered Verify

1. **Integrate only the exact reviewed main.** Verify both merge parents, complete
   inherited main139 modes/blobs and two literal Docs histories; all native3f5
   selected Source stays exact. Account for inherited paths independently of the
   editable one-file scope. Return any unplanned Source conflict before changing it.
2. **Apply the two local syntax changes.** Verify whole-file inverse restores the
   pre-edit merged stock file53398 bytes/SHA248100e... exactly; old slice body,
   count/tail/error precedence/21-byte and43008-byte bounds/collision arguments
   and assertions are literal. No new selector, warning allow or dependency.
3. **Review static conservation.** Verify full staged+unstaged patches before
   typed scoped commits; every other tracked mode/blob/SPEC/lock/workflow/PS
   program/owner/drop/check/45-30-3 clock and output format remains exact. Static
   fmt94/98, locked offline metadata and OCR file/rule review are permitted.
4. **Qualify the fresh hosted head.** Verify actual Rust1.94 compilation, coldCore131,
   three isolated stock proof invocations and warnings-denied Windows Clippy;
   preserve old commands/gates/filter counts. Root collects exact head/merge tree,
   event/attempt/job/step/raw hashes once. Failed or skipped native/runtime gates
   remain failed/unexecuted; no unchanged rerun or clock/condition change.
5. **Publish results at their actual scope.** Verify Root independently reviews the
   full Source/inverse/tree and fresh results, then records Issue31 via CLI.
   PR136 is held until all required gates pass; wide account/installer/logon/reboot/
   privacy/public-release acceptance is not closed by syntax or stock proof success.

Local application/native/PowerShell/parser/reflection/compiler/linker/tests and
fixtures stay unexecuted. No Issue/memory/API state/push/dispatch/rerun effects
belong to the Source lease. Root performs publication after reviewed handback.
