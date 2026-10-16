# Publish pending migrations with one durable commit

Owners #181/#180; native qualification #172/#31. Reviewed base8ef6584 and
CI38916287297 attempt3 MSRV job117043164522. User requested completion of all
open PRs, including the remaining native blocker.

## Evidence and selected change

HTTP01 refuses in actual-store-creator before the row body. Open elapsed9.1354s,
commit3.663074s and admission5.069ms are observed; stage begin/admission_timeout,
retries0 and missing SQLite codes do not establish a BUSY holder. Preserve that
failure and all earlier native failures. Do not infer why the host was slow.

A fresh database currently performs five immediate transactions and five durable
commits. All five immutable SQL scripts contain transactional DDL with no nested
transaction/VACUUM/incompatible PRAGMA. Reduce fresh creation to one successful
COMMIT by applying all pending steps in one immediate transaction. Any supported
older version uses the same one-transaction pending chain. A current schema is
verified without BEGIN or COMMIT. This supersedes the earlier Implementation
per-step transaction-placement requirement, including the corresponding wording
in the contention plan. It changes neither SQL/checksums nor the migration ledger
format, schema version, recorded values or FULL/WAL durability.

## Admission, concurrency and failure invariants

Retain the original Operation and absolute five-second deadline from writable
open through preparation, writer/connection admission and the single migration
BEGIN. Only proven pre-BEGIN BUSY can retry. No new timer, migration-body replay
or COMMIT retry is permitted. An already-started OS/SQLite call is not preempted.

Preliminary application/version reads can reject unsupported state or recognize
a current schema. For pending work, reread these markers under acquired BEGIN
and verify the complete already-applied ledger before mutation. A concurrent
advance must be validated from the authoritative version, including every existing
checksum, rather than trusting a stale pre-BEGIN version. Each pending step still
uses the existing predecessor recheck and original SQL/name/checksum/binary/time
record. Verify newly applied records inside the transaction before commit.

If the admitted authoritative version is already current, verify it and release
the write transaction without COMMIT or migration writes; do not overwrite the
concurrent winner's records merely to report success.

The initial logical winner chooses its Windows execution PATH inside that same
transaction; stale losers preserve it. Historical partial versions and configured
values remain supported. A late pending-step failure rolls back the entire new
chain, leaving the prior committed schema and ledger. Failed COMMIT remains
unknown/nonretryable. Do not infer rollback merely from transaction destruction;
existing operation outcome observation uses actual connection state.

The current-schema path verifies all required checksums without mutation or a
write transaction. Read-only Store opening remains untouched. Foreign application
markers, too-new versions, missing records, checksum corruption and backwards
movement retain explicit refusals. Guarded private filesystem/DB/WAL/SHM handling,
all production deadlines and native test selectors/assertions stay unchanged.

## Ordered work and Verify

1. Parent records SPEC/Architecture/Storage/Findings/Implementation and this plan,
   updates matching execution issues and rereads the complete change.
   **Verify:** the selected atomic publication boundary and superseded per-step
   placement are explicit; historical SQL and five-second policy are unchanged.
2. Separate development implements one admitted pending chain and adds regression
   controls using real SQLite, without production hooks or dependency features.
   **Verify:** fresh state and every supported partial upgrade reach version5
   with one successful commit; current state performs zero commits and no write
   BEGIN; existing checksum/path/concurrency/recovery controls pass. A final-step
   ledger trigger refusal and a late DDL conflict leave complete prior schema,
   settings, ledger and application/version markers unchanged after rollback.
   Verify an expired Operation still refuses before the only BEGIN, and stale
   admission does not rewrite another initializer's metadata or accept corruption.
3. Qualify the composed final source with fmt, strict workspace/all-target Clippy,
   full workspace tests and the complete existing hosted matrix/MSRV/native gates.
   **Verify:** record actual commit counts as the deterministic cost reduction,
   actual native outcomes separately from Linux measurements, every required
   original job/selector/35-minute limit and receipt upload remains. A new failure
   receives diagnosis and a planned fix rather than an unexplained replay.
4. Parent reviews the handoff, publishes, merges the verified PR182 and checks
   exact source/tree and dependency changes before closing superseded PR176–179.
   **Verify:** all final native and required checks pass, main contains the
   reviewed tree and dependency references, and remaining broader Windows support
   tasks are not closed merely by this repository merge.

The expected improvement is four fewer durable commit boundaries on fresh state;
no fixed elapsed-time guarantee or historical holder diagnosis follows from that
source fact. Same-source native CI must still demonstrate the full acceptance.
