# Shared SQLite contention policy (#180, #181)

The 2026-10-16 SPEC amendment governs behavior. This plan applies to CLI, API,
MCP, daemon mutations and automatic maintenance. Existing SQL/checksums, atomic
rechecks, durability and privacy stay authoritative. Production state, installation,
services, release and unrelated Windows acceptance are outside this work.

## Research and complete writer inventory

At base a3f43fc, Store has one connection mutex per instance. Mutation entrypoints:

| Family | Implementations which must obtain common admission |
| --- | --- |
| Initialization | Store::open; Unix WAL/configuration; Windows guarded DB/sidecar preparation, WAL/configuration; every migration step |
| Jobs | create_job, update_job, apply_import, set_enabled, remove_job |
| Scheduling | enqueue_manual, materialize_with_summaries, admit |
| Cancellation | cancel_with_acknowledgement (including explicit quarantine acknowledgement) |
| Lifetime | begin_lifetime, end_lifetime |
| Attempts | mark_attempt_running_inner, complete_attempt, complete_pre_execution_failure, complete_runner_failure |
| Outputs | reconcile_output_finalized, reconcile_output_missing, mark_output_prune_pending, finish_output_prune |
| Settings | set_environment, set_setting |
| Metadata retention | mark_run_retention_pending, finish_run_retention |

materialize, cancel, mark_attempt_running, mark_attempt_running_with_executable and
finalize_output delegate to the listed implementation and must not acquire again.
PersistencePort implementations delegate to these operations. activity.rs,
cancellation.rs preview, history paging/search/latest queries use deferred read
transactions and obtain no writer admission. output.rs owns filesystem I/O only;
migration.rs is the other production SQL writer. Tests/raw diagnostic SQLite
connections remain external controls, not coordinated application writers.

Baseline autocommit writers are remove_job, end_lifetime,
reconcile_output_finalized/missing, set_setting and output prune mark/finish.
They will use immediate transactions under common admission; remove's live-row
lookup and update become one atomic unit. Postcommit job/settings/run reads occur
after releasing admission. No new lock spans output file operations or target I/O.

## Acquisition and single wait budget

A mutation creates one monotonic five-second deadline before acquiring any lock:
foreground shared `writer-intent.lock`, exclusive `writer.lock`, then the Store
connection mutex, then SQLite BEGIN IMMEDIATE. Independently open both guarded
private permanent files per operation; never clone a descriptor, unlink, truncate
or replace a lock file. Failed acquisition unwinds every earlier guard. Release
intent once writer and connection are owned, allowing foreground intent to register
during the current unit. Release writer after transaction completion/rollback and
before subsequent readback. Read-only opening/queries never create/probe these
files. No daemon lifetime lock is reused; standalone daemon-off writes remain valid.

Automatic maintenance runs synchronously inside a thread-scoped background mode,
keyed to its Store/state identity, with RAII restoration even during unwinding.
This avoids a global priority flag that could accidentally demote concurrent
foreground calls on the same Store. Each background SQL operation tries exclusive
intent, writer and connection without waiting, releases intent after admission,
and reports cooperative deferral when any resource is occupied. The scope is not
Send and cannot cross an async boundary. The existing synchronous maintenance
composition owns its lifetime. Between read phases and before each filesystem unit,
a nonblocking exclusive-intent probe detects waiting foreground work and yields.
This is advisory progress, not strict FIFO fairness; external SQLite writers and
old binaries do not follow these locks.

SQLite busy handlers are zero for mutations, so each retry consumes only remaining
shared time. Retry only BUSY at BEGIN IMMEDIATE before a transaction exists, using
bounded <=10 ms sleeps clipped to remaining time. Once BEGIN succeeds, do not replay
the body or commit. LOCKED, I/O, corruption and validation are never retried. Failed
commit is explicitly unknown and is never replayed; a proven dropped/rolled-back
transaction can report not committed only after explicit successful rollback or
observed autocommit after transaction drop; drop alone is not rollback evidence.
A failed commit remains unknown even if autocommit is subsequently observed.
Background BEGIN BUSY defers immediately without a sleep/retry loop.
Convert former autocommit writes to the same
boundary instead of retrying an uncertain statement.

Writable open has one admission/deadline spanning preparation, WAL and migration
begins, and releases admission before returning. Flow remaining deadline through
Windows DB admission and existing finalized-statement WAL loop; do not stack its
current separate five-second allowances. Unix WAL negotiation retries only a fully
finalized fixed PRAGMA before any application mutation. Current-schema verification
is read-only; migrations retain their existing transaction/version/checksum checks.
Never replay configuration batches, migration DDL or commit. Connection creation and
already-started OS I/O cannot be forcibly interrupted; measurements report reality.

## Diagnostic and observation contract

Core owns a transport-neutral serializable PersistenceDiagnostic record with safe
static operation/stage and classification (`busy`, `locked`, `io`, `sqlite`,
`admission_timeout`), primary/extended numeric SQLite codes when present, monotonic
elapsed, admission-wait, connection-wait, SQLite-wait, transaction-hold and commit
microseconds, retry count, retryable flag, outcome (`not_committed` or `unknown`)
and lock_owner=`unknown`. Stages distinguish preparation/configuration/migration,
intent, writer admission, connection, begin, execute and commit. Numeric OS error
codes may be included separately where available. Human messages explain the stage,
measured wait and next step without asserting a holder or successful rollback.

Store retains the original error chain for inspection but its new diagnostic
wrapper's Display and Debug never render raw SQLite/JSON/OS contents. Application
renderers use only the safe diagnostic projection, including anyhow alternate/debug
chains. Existing validation/not-found/conflict variants retain their semantics;
errors from mutation/configuration boundaries carry diagnostics rather than raw
parameters, SQL, environment values, argv, headers or bodies. Core's new structured
persistence variant preserves the record instead of reducing BUSY/LOCKED to text.

CLI JSON and terminal stream errors add error.storage; human stderr includes the
safe summary. Direct Store failure stays state_error/exit5; shared structured Core
persistence uses that same state failure category rather than falling through to
invalid_request. API retains generic Store state_error/HTTP500 with additive
error.storage (no blanket move to409/503). Transfer errors use the same projection.
MCP tool failures retain isError/text and add a structured error object carrying
code=state_error, safe message and storage. Protocol parse/argument errors retain
current behavior. Daemon completion adapters preserve typed conflict behavior and
log safe structured persistence summaries only.

A Store keeps at most its latest completed write observation in memory, with no
persistent telemetry table or new observer mutation. A safe read accessor supports
successful wait/transaction measurements and representative qualification. It
contains only the same static/scalar fields; no job identity or user payload.

## Shorter transactions and bounded maintenance

Decode materialization input policies and validate immutable run/summary inputs
before obtaining writer ownership. For admit, read bounded immutable run IDs and
snapshot/policy data before writer ownership; inside the transaction reselect the
current eligible rows and authoritative capacity/fairness/state facts, using prepared
policies only when exact run ID and immutable snapshot still match. A new candidate
missing from the prepared set defers to the next admission pass. Do not truncate or
change the existing scan limit, round-robin order, uniqueness, quarantine, replacement
or catch-up serialization. Tests must prove no stale decision crosses the boundary.

For retention rank rechecks replace full count scans with threshold-limited subqueries
for the same terminal rows, non-NULL finished time and `(finished_at_us,id)` ordering.
Use >=threshold semantics, including zero, and reread settings in the transaction.
No index/schema migration is proposed: measured existing runs_retention use supports
a bounded probe. Initial candidate window query remains outside write ownership;
record its latency and stop before further work when the pass deadline expires.

Automatic maintenance retains100 actions and adds a250 ms monotonic soft pass
budget. Check at phase/candidate boundaries and before filesystem work (including
orphan scanning); stop cooperatively, without turning expected deferral into a
maintenance error. Preserve durable pending prune/retention intents. If a deadline
or foreground wait appears after intent commit, leave pending work for a later
pass. Already-started filesystem I/O may exceed the soft allowance; no false hard
I/O bound is promised. Store writes in this scope never spend five seconds waiting
behind another writer, preserving completion/cancellation responsiveness.

## Implementation order and Verify

1. Add shared diagnostics, guarded lock admission/deadline helper and all writer/open
   integration. **Verify:** inventory scan has no unguarded mutation; real raw held
   writer short-release succeeds, sustained hold fails around the same total5s,
   codes/stage/waits survive Store and Core, failed removal leaves its row intact,
   release permits retry. Separate lock/connection waiting never creates a second5s.
2. Shorten immutable preparation and retention probes; expose latest observation.
   **Verify:** fixed representative fixture reports pre/post transaction hold with
   equivalent results; no duplicate occurrence/attempt, stale removal/update,
   concurrency-policy change or cancellation bypasses transaction rechecks. Retention
   ties/NULL/zero/settings changes and query plans preserve policy.
3. Integrate scoped background priority and elapsed budget. **Verify:** real separate
   processes/barriers demonstrate foreground intent prevents another background
   unit, handle drop/process exit recovers locks, no acquisition deadlock, deferred
   cleanup resumes and active outputs survive. Count+time limits and no-create reads
   hold; unsafe/symlink/private-state fixtures remain refusals.
4. Render additive diagnostics consistently. **Verify:** real held-writer CLI human,
   JSON and stream, real API mutation and MCP removal expose codes/stage/waits;
   secret-bearing fixtures stay absent at -vv and every envelope. Non-contention
   SQLite/IO/validation distinctions and Core mapping are regression-tested.
5. Qualify composed source. **Verify:** relevant tests plus full workspace tests,
   Rust1.94 MSRV,1.98 formatting/strict Clippy and diff/checksum review pass on this
   Linux host. Native macOS/Windows results remain unclaimed until those gates run;
   no unrelated Windows acceptance criterion or old failure is silently changed.

Research measurements and limitations are in FINDINGS. Repository Issues own phase,
progress and final exact-revision evidence. Parent plan review and issue updates
precede source implementation; parent owns commits and PR publication.

### Immutable preparation details (development clarification)

Admission preparation reads at most `min(hard_guard_available,64)*64` eligible
run IDs and immutable snapshots, without exclusive writer ownership. It carries
the same operation deadline through bounded connection acquisition, releases the
read mutex before writer admission, and never starts a fresh five-second budget.
The immediate transaction retains its original current-capacity-derived scan
limit and ordering. Prepared decode failures are raised only when the original
round-robin loop examines that candidate; unseen or snapshot-mismatched candidates
defer. All eligibility, cancellation, quarantine, replacement, catch-up, capacity
and fairness facts are reselected inside the transaction.

Manual enqueue similarly prepares its live job snapshot under that deadline,
then rechecks live identity, current revision and exact definition before changing
queue state. Materialization validates immutable run/reconciliation-summary inputs
and encodes summary details before writer admission; its cursor revision/live-job
check and all overlap facts remain inside the transaction. Deterministic test-only
barriers after preparation exercise concurrent cancellation, candidate creation,
job removal/revision and settings changes without timing-dependent sleeps.

## Preparation boundary clarification reviewed before source

`admit` and `enqueue_manual` may first acquire only the connection mutex for
read-only immutable preparation under the same Operation/deadline used by the
subsequent write. Release that mutex before acquiring intent/writer admission;
no read-preparation mutex is held while waiting for another resource. Include its
wait in connection_wait_us and its elapsed time in the original five-second
budget; `write_prepared` must never start a fresh allowance. The authoritative
transaction retains all current capacity/state/fairness checks. `enqueue_manual`
rechecks the exact live job/current revision/immutable definition after BEGIN and
returns a conflict on concurrent update/removal rather than enqueueing stale work.
Admit prepares at most the existing hard guard64 times the existing scan multiplier64;
its in-transaction scan still uses the original current-capacity-based bound/order.

## Committed readback diagnostic boundary

Mutations that return a job, run or settings record release writer ownership before
reading the result. Carry the original Operation and deadline through that readback:
acquire only the connection mutex within remaining time, use a nonwaiting SQLite
handler, and report adapter failures at `readback` with a known committed outcome.
Do not replay the mutation or start another wait allowance. Shared private read
helpers allow this boundary without changing ordinary public read behavior.
Preparation reads likewise disable the per-connection five-second handler so a
late preparatory read cannot stack another allowance. Semantic not-found/conflict
errors retain their categories. A raw PersistencePort adapter fallback preserves
primary/extended codes with stage/outcome explicitly unknown rather than merging
BUSY and LOCKED into a generic busy message.

Job removal resolves exactly one row inside its admitted transaction, then updates
that exact identity. A UUID-shaped name can match another job's identity; a bulk
OR-predicate update must never remove multiple jobs for one request. A collision
regression preserves the existing single-resource resolution behavior.

## Existing completion recovery and diagnostic privacy details

The daemon's existing completion-recording retries remain idempotent durable recovery:
the same immutable outcome and completion instant are retried, and Store rechecks
exact committed identity before acknowledging success. They do not rerun a target or
replay arbitrary mutation bodies after an uncertain commit. LOCKED/I/O in that existing
CompletionError::Transient vocabulary means durable storage is unavailable, not that
the new contention layer labels them BUSY or retries them in the same request. Durable
conflicts retain their existing stop/fallback path and later retries honor shutdown.

Raw adapter causes remain explicitly inspectable through original_error, while safe
observed errors do not traverse their raw cause through Error::source. This preserves
privacy in alternate anyhow/error/debug rendering. Streaming orphan enumeration checks
before each next unit instead of retaining an unbounded sorted directory snapshot.

## Resumable orphan traversal within the elapsed budget

Automatic maintenance retains one composition-owned traversal cursor between passes
instead of restarting every filesystem scan at its first entry. The daemon adapter
owns a mutex-protected cursor containing at most the guarded outputs iterator and
one guarded run-directory iterator. It keeps no growing work queue or telemetry
history and adds no schema/mutation. Resume those iterators after budget exhaustion
or foreground deferral, close guards when their sweep ends, and reset on state-path
change. Recheck current run/artifact protection per entry; do not cache liveness
across passes. Check the shared time/foreground budget before each new entry or I/O.
An already-started I/O remains noninterruptible. Test repeated bounded passes with
a referenced prefix and later orphan entries, including active-file protection and
state reset. This prevents a large stable referenced prefix from starving later
cleanup indefinitely. SQL pending intents retain their existing recovery protocol.
