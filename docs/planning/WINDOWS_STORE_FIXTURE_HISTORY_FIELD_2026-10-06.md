# Explicit test-only Store observation in the snapshot fixture — 2026-10-06

## Actual boundary and frozen scope

Published combined head is fa948b8bc7bc123cbfb098790a00a0c882aa43e8.
Root-retained GET1 jobs112111548836 (Unix lint, lines459–504) and112111548779
(Windows lint, lines432–466) actually report E0027 at
store/dashboard_pr144_snapshot_fixture.rs336: Store pattern omits
history_observation. These are compile failures before this fixture executes,
not a measured guard/SQLite/drop failure or an overall completed-run claim.
Retain current published receipts, all historical failures and the running CI;
do not cancel, redispatch or transfer an old PASS.

Store.rs1019–1020 gates the field with cfg(test); both open constructors
(1072–1073 and1104–1105) initialize Mutex<Observation::default()>.
The history module (26–27) and snapshot fixture module (7572–7573) are cfg(test).
Observation contains counters and an optional channel CountBarrier; it has no
custom Drop. The default barrier is None. Source inventory finds this one
explicit actual Store destructuring; the other Store open forms are constructors.

Select ONLY the snapshot fixture close_store field pattern, immediately after
the existing paths: _, with matching cfg(test) and history_observation: _.
Do not select .., an observer binding/lock/query, a new close/drop call, a field
or constructor change, a Windows-only condition, or an allow/skip/dummy value.
Explicit ignore keeps exhaustiveness for other future fields. The field is
present in Unix and Windows library tests; matching cfg(test) keeps the field
pattern aligned with its declaration. Ordinary non-test builds exclude the
entire helper module and observation. No public Store/API/layout change.

## Exact future Source and ownership

Source lease, after the gate below, is ONE file:
crates/locron-store/src/store/dashboard_pr144_snapshot_fixture.rs.
Add only the two pattern lines with the existing conditional-field indentation:

    #[cfg(test)]
        history_observation: _,

All original fields, native guards, connection.into_inner/SQLite close Result,
error strings/order, Windows drop(read_guards) then drop(state_guard), and Ok(())
remain literal. The ignored field introduces no observer binding or explicit
operation; remaining fields follow ordinary compiler-managed drop semantics.
No hidden barrier-release timing or runtime cleanup PASS is inferred from text.
Keep all clocks, identity/descriptor/state absence/restoration/holders, rows,
assertions/selectors/evidence, A/B targets, C observations and production exact.
The prior five-file C semantic review did not include this A close helper;
this is a measured integration hole, not contributor completion or blame.
Frozen SPEC/architecture and existing plans remain unchanged.

## Ordered steps and concrete Verify

1. Root reviews/commits this plan and three literal append-only Docs tails.
   **Verify:** full four-Docs before/after/prefix inverse, all current modes/blobs/
   physical bytes/index protected; WHOLE plan POST/exact GET ALL #172/#31/#163/
   #162/#156, then ACTUAL entire FINAL COMMITTED plan and all new tails reread
   AFTER ALL GETs before a separate Source lease. No Source under preparation.
2. Separate developer applies only the named conditional ignored field.
   **Verify:** full physical/canonical inverse removes exactly those two lines;
   complete original close helper, Store declaration/two constructors/Observation,
   other destructuring, all other Source/Docs/modes/bytes preserved. Standalone
   pinned Rust1.94/1.98 formatting may affect only the new field lines; any wider
   token/layout change returns to Docs FIRST. Root full staged/unstaged review.
3. Root publishes one genuinely changed head and preserves current-run history.
   **Verify:** actual Unix/Windows strict lint and Rust1.94 plus current native
   Store libtest compile no longer report this E0027; actual original close/
   snapshot/history controls and full A/B/C/Native tests retain all oracles and
   owner/cleanup boundaries. Build/metadata alone is not runtime acceptance.
4. Root completes the existing final delivery plan at the user's actual scope.
   **Verify:** same-final-head eleven required nondeferred contexts, Guardian,
   paired/package/provenance and final genuine production Wake/Cancel evidence,
   exact main/member contributions. Record residual Windows CI in #172 and
   matching owners under the selected deferral policy; no failed/unrun criterion
   becomes green or closes merely from this compile correction.

The official [Rust E0027 explanation](https://doc.rust-lang.org/error_codes/E0027.html)
supports explicit missing-field ignore. Root already read it; no new dependency,
API/log/archive acquisition, compiler/runtime/parser or Verify execution here.
Root owns commit/Issues/full final reread/publication; development requires its
later separate lease. This preparation does not alter published Source or CI.
