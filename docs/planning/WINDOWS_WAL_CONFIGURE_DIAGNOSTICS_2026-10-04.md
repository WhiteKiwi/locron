# Windows WAL configuration failure diagnostics

Continue Issues #25/#31 after ordinary CI37145289162 at Source302b1bf. This
diagnostic slice preserves the frozen SPEC. It does not select a timeout/retry
fix, move a clock, qualify the old failure or change SQLite ownership.

The idle-client lifecycle case failed at first Fixture Store::open with local
operation3; active-SSE failed there with operation2. Both reported
sqlite-configure-wal / IO TimedOut / raw_os=None. Their daemon/dashboard/job,
HTTP/SSE and shutdown assertions had not started. Operation counters are not
database identities. An earlier Server SSE reopen returned SQLite PROTOCOL15
and remains a separate observation requiring connection/file correlation.

Research receipt f66dfe9b0007d33afe5ac294974c1191e0f48320bb376cf519a02b9bf69a7f17
pins the Store call map. The one configuration5s starts after private-directory,
SID, DB-admission and new Connection preparation. It covers the fixed WAL
attempts and remaining settings. Each native attempt prepares the fixed PRAGMA,
query_one consumes ROW through DONE, then finalizes explicitly. Only5/261 after
finalization on an idle autocommit connection retry under that same clock.
Exhausted BUSY preserves its original code;15 and other errors return directly.
A late successful native call may already have effects; refusal never licenses
replay. The allowance is not a preemptive native-I/O timer.

## Selected implementation boundary

Allow Source only in crates/locron-store/src/windows_configure.rs and
crates/locron-store/src/windows_open.rs. Use fixed owner-local in-memory debug
observations, not Core cfg(test) hooks that integration dependencies do not build.
Release builds must retain the original configuration policy without new native
calls, logging, global contention or feature switches.

Capture configuration entry and elapsed/remaining facts at existing gates,
fixed gate/phase labels, a saturating attempt counter, last numeric SQLite
primary/extended codes, known autocommit results, and prepare/query_one/ROW-
callback/finalize entry/return stamps. DONE is confirmed only by query_one's
successful return; its internal second sqlite_step has no separately exposed
entry stamp. Do not add an unsafe/private SQLite bridge to manufacture it.
Mode is a fixed wal/memory/other/unobserved enum,
never the returned string. Observe existing native calls only: do not add
SQLite queries, filesystem operations, locks, scheduling or native calls after
expiry. Snapshot collection adds debug observation cost; it does not pause or
restart the original clock. Existing checks remain in their original order.

Emit at most one fixed bounded configuration-failure receipt only after the
operation's result is known, through OpenTrace's existing debug-error route.
Correlate with actual PID and the existing local operation counter. Bound the
receipt to768 ASCII bytes. No path, SID, SQL, environment, token, request data,
raw error text or returned mode string may be rendered. No per-attempt log or
unbounded event list. A missing observation stays unobserved, never successful.

Preserve exact SQL literals, error precedence, settings, checksums/dependencies,
guard and Connection lifetimes, explicit finalization,5s configure allowance,
10ms contention yield and every existing test assertion. Keep the4 existing
native configuration cases and all12 lifecycle cases with original concurrency
and shutdown/EOF/job bounds. No test skip/ignore/allow or broad retry. File-ID or
connection-close correlation for the older Server15 needs its own later plan.

## Steps and Verify

1. Review and publish this plan before the separate developer receives Source.
   **Verify:** three docs only; frozen SPEC blob unchanged; exact Issue #25/#31
   comment readback contains the scope and all five Verify criteria.
2. Implement the two-file fixed debug snapshots and failure-only rendering.
   **Verify:** root reads every changed line; original native call order, SQL,
   errors, finalization, clocks and unrelated tracked blobs/modes are conserved.
   Release configuration has no added native call or receipt. Fixed fields and
   bounded output never disclose private data or invent missing observations.
3. Validate the original expired-entry/explicit-transaction, real finalized BUSY
   release, persistent BUSY, non-WAL and readonly controls without weakening any
   assertion. **Verify:** Rust1.94 fmt, locked metadata and whitespace pass;
   fresh native controls retain actual original result codes and side effects.
   Any added diagnostic-contract case is inventoried separately and never
   represented as an organic historical5/261/15/timeout observation.
4. Publish reviewed integrated Source and collect fresh x64 stable, ARM64 stable
   and x64 Rust1.94 ordinary and full native results plus per-binary discovery.
   **Verify:** exact-head artifacts/logs preserve all original selectors and
   concurrency. Every failed configuration receipt identifies a gate/phase and
   actual available numeric facts. Dashboard acceptance requires the complete
   later assertions, not successful fixture initialization alone.
5. Review actual receipts before selecting a cause-specific follow-up.
   **Verify:** Issues retain original failures and distinguish late native
   return, pre-call expiry, BUSY and15. Required red rows remain merge blockers.
   No unchanged rerun, timeout inflation, detach/kill of a SQLite owner, blanket
   retry or historical cause claim; missing Server owner/file facts stay open.


### Stable cfg statement syntax after native compile refusal (2026-10-04)

Source5585936 ordinary37151318184, full37151330037 and isolated discovery37151378233
finished red before the requested application runtime tests. Separate always-run stock
bootstrap/small/60k diagnostics passed on the three foundation rows; these do not qualify
the failed compilation or application suite. Actual Rust1.94/stable/1.98 reject #[cfg] directly on
the two assignment expressions at windows_configure.rs168/237 with E0658. Release builds
also retain unresolved observations assignments (E0425). The immutable release-token inverse
and formatting checks could not establish compiler validity; Root's prior review missed this.

Select only a syntax correction: wrap those two existing debug observations in cfg-gated
blocks. Keep assignment expressions, captured existing autocommit results, cfg condition,
release erasure and every native call/result/gate/error/deadline byte-equivalent after inverse.
No feature/nightly/warning allowance, production/query/test/dependency/workflow change is selected.
Separate development owns only windows_configure.rs after Issue25/31 exact Verify readback.

1. Reproduce/repair the two stable cfg statement sites. Verify: Rust1.94 native Windows debug
   and release compilation both succeed; no unresolved observations or experimental syntax.
2. Conserve observation and native policy. Verify: two exact block-only transformations invert
   the whole prior file; all other tracked blobs/modes and old/native/new diagnostic tests remain
   exact; cfg-erased release policy still equals the ce641 original.
3. Review/publish fresh changed Source. Verify: Root reviews the one-file diff and static checks,
   then records actual ordinary/full/every-binary results at the new source. These previous runs
   reached no requested app suite cases and produced no new paired ZIP; separate stock
   diagnostics remain their own evidence. All runs remain failures, not skipped validations.


## Stable Rust syntax continuation before Source

Ordinary558 run37151318184 fails compilation, before native controls: Rust1.94
job111285640711 and Rust1.98 lint111285640451 report E0658 at windows_configure.rs
168 and237. The two cfg attributes attach directly to assignment expressions.
A separate developer may only enclose those same assignments in cfg-gated
statement blocks, keeping their exact debug-only evaluation points and values.
Verify the two-assignment inverse, unchanged release/native policy and all other
blobs, stable Rust1.94/1.98 syntax, formatting and fresh native compilation.
No feature gate, allow, clock, SQL, retry, diagnostic field or selector changes.
The original five-step plan and owning Issue criteria remain required.
