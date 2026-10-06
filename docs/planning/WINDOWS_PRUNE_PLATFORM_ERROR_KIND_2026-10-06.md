# Windows prune platform error-kind qualification (2026-10-06)

## Pins and current evidence

This test-only continuation starts at clean B `02bc875067a7d4099b4aa809c606a335d08b5742`.
The observed source is `2cf84aceb2362a22d1934f3e5494c548f7d6e75e`, CI37390581174/attempt1.
That completed run failed: 13 required successes, four failures and two optional skips.
Its original 24 native outcomes are 21 PASS/3 FAIL; ordinary Wake/Cancel failures remain open.
Each Windows foundation passed 28 preceding API rows including its genuine FI07 receipt,
then API N07 failed with returned family=store_io, kind=unrecognized and raw=32.
MSRV was Rust1.94.0; both stable foundations were Rust1.99.0. Lint used Rust1.98.0.
The separate map_or-only correction is committed at the selected base, without a new CI result.
CLI's current actual row, kind and raw remain UNKNOWN behind actual-nonzero-summary/ValueError.
Neither API raw32 nor preceding FI07 identifies a holder/path/native instruction or proves
N07's later exact-holder release, same-pending positive operation or completed cleanup.

The sealed research is `coord/windows-B-n07-error-kind-research-2cf84-37390581174-20261006`,
manifest SHA256 `d9d522f49676faabd9f32569c886b233eb2e64e80de7c2d3db4d238b1e44b962`.
All prior plans, failed receipts, source snapshots and original document prefixes remain literal.
SPEC and public product scope are frozen; this plan changes only two Windows test expectations.

## Selected two-predicate scope

Later separate development may change only the N07 assertion predicates in
`crates/locron-server/src/api_prune/qualification.rs` and
`crates/locron-cli/src/explicit_prune/qualification.rs`:

```rust
matches!(error.raw, Some(32 | 33))
    && error.kind == error.raw.map(|raw| io::Error::from_raw_os_error(raw).kind())
```

This supersedes historical PermissionDenied classifier clauses ONLY for these two N07 tests.
The exact returned raw32|33 gate stays first; absent/other raw values cannot reach the comparison.
The same returned kind must equal the public platform decoder of that same returned raw scalar.
Non-IO errors, guard raw=None, native5, missing kind and mismatched kind still fail.
No last_os_error/native query, second operation, error remap, retry or new resource owner is added.
The standard scalar error construction does not acquire a File, Child, guard or custom owner.
Do not name unstable Uncategorized, substitute ResourceBusy, add raw5 or broaden any dictionary.

Tagged Rust [1.94](https://github.com/rust-lang/rust/blob/1.94.0/library/std/src/sys/io/error/windows.rs),
[1.98](https://github.com/rust-lang/rust/blob/1.98.0/library/std/src/sys/io/error/windows.rs) and
[1.99](https://github.com/rust-lang/rust/blob/1.99.0/library/std/src/sys/io/error/windows.rs) decode
ERROR_ACCESS_DENIED5 as PermissionDenied and leave32/33 to the Uncategorized fallback.
That source inference is distinct from the actual closed output label unrecognized.
The public constructor/kind comparison follows each actual compiler's decoder without exposing
private values or making an actual holder, instruction, cause, type or native-success claim.

Keep product guard/error projections, the API closed239CRLF renderer, CLI context/message,
the same returned Error extraction order and every other predicate literal. Preserve held-file
full IDs, genuine production negative control, bytes/oracles/pending state, exact holder recheck
and drop, same-pending real remove+durable finish, positive receipts, cleanup/quarantine and Unix.
Preserve all operation/evaluation ordering, owners, selectors and original case/cleanup clocks.
Full-byte hashing/chunks/caps, the selected test-profile experiment, Cargo/dependencies/features,
debug assertions/overflow checks, workflows and every existing admission/security rule are unchanged.
No new synthetic error, mirror scanner, test, selector, clock, performance or production fix is selected.

## Four concrete Verify steps

1. **Complete the before-Source gate.** Root reviews and commits exactly these four Docs.
   **Verify:** preserve all old prefixes/failed receipts/Source bytes; POST the whole NEW plan to
   #27/#31/#162/#146/#149/#163 and PR147, exact GET ALL seven with original bodies/state/history
   preserved, then Root actually rereads the entire FINAL COMMITTED plan and all three new tails
   AFTER ALL GETs. Only a later explicit separate development lease grants Source authority.
2. **Implement only the two predicates in that separate session.**
   **Verify:** complete canonical and physical whole-file inverses plus every other mode/blob and
   working-byte protection; raw32|33, same-error projection/closed239/CLI message, holders/positive
   controls/Unix/clocks/profile remain exact. Only selected standalone Rust1.94/1.98 formatting
   and full locked OFFLINE metadata are allowed locally; no compiler/Clippy/tests/native/parser/
   PowerShell/fixtures. Unexpected layout, inputs or decisions stop for Docs FIRST.
3. **Qualify both original N07 controls on a fresh changed head in all three Windows foundations.**
   **Verify:** actual held full-ID, same helper Err/raw32|33 with matching platform kind, unchanged
   genuine production-negative refusal and bytes/ID/oracle/pending preservation; recheck/drop the
   exact original holder, execute the same-pending real remove+durable finish, prove absent files
   and exactly one surface-specific helper-finish-positive receipt, N07 row and all following
   original rows/support/Done/cleanup. Unknown or failed continuation is not completion; old API
   raw32 or CLI hidden ValueError is not this proof. No retries, accepted extra codes or bypass.
4. **Retain every remaining owning acceptance gate before closure.**
   **Verify:** fresh actual required Windows lint/Unix caller+hash fences/all original rows and
   receipts, ordinary Wake/Cancel and all24 native outcomes, paired/package/secret scans and actual
   main/member contribution satisfy their original criteria. Current21P3F and skipped work stay
   failed/unqualified; neither preceding green rows nor static/type/build facts replace these gates.

Root alone handles Issues, publication, actual hosted evidence, merge and closure.
This Docs-only selection authorizes no Source implementation, native execution or release claim.
