# locron Operator Guide

This guide turns the milestone-1 contracts in [SPEC.md](SPEC.md) and [CLI.md](CLI.md) into a
day-to-day operating reference. The repository is still an active milestone implementation; use
[STATUS.md](STATUS.md) and [ACCEPTANCE.md](ACCEPTANCE.md) to distinguish implemented behavior from
pending acceptance evidence.

## Start and inspect the scheduler

locron is one per-user scheduler for one local state directory. Run one daemon for that directory:

```sh
locron daemon run
```

Use a service manager only to keep this command alive. Do not create parallel cron, launchd, or
systemd entries for individual locron jobs. Confirm ownership and durable state with:

```sh
locron doctor
locron config get
locron list --all
```

## Schedules

Every job has exactly one schedule. Preview before registration or enablement when timing matters:

```sh
locron preview --cron "0 3 * * *" --timezone Asia/Seoul --count 5
locron add heartbeat --every 30s -- /usr/bin/printf 'ok\n'
locron add nightly --cron "0 3 * * *" --timezone Asia/Seoul --shell './backup.sh'
locron add deploy-once --at 2026-09-01T09:00:00+09:00 -- /usr/local/bin/deploy
locron add health-check --every 5m --http GET https://example.test/health
```

- Calendar schedules use five fields. DST gaps are skipped and a repeated wall time produces one
  occurrence.
- Fixed intervals advance from their durable anchor, not from the previous completion time.
- One-time schedules disable after their scheduled occurrence is resolved. Manual runs do not
  consume the scheduled occurrence.
- Schedule updates affect only future occurrences. Disable/re-enable and manual runs do not move an
  interval anchor.

## Admission policies

Set policies at `add` time or change them with `update`.

| Concern | Choices | Operator effect |
|---|---|---|
| Overlap | `skip` (default), `replace`, `allow` | Skip records a terminal explanation; replace terminates confirmed prior work before starting the newest candidate; allow uses bounded concurrency. |
| Missed run | `skip`, `latest`, `all` | Latest creates one catch-up run; all creates an oldest-first bounded batch; `--start-deadline` excludes stale work. |
| Retry | disabled by default; up to 10 retries | A retry is another attempt of the same run. Known configured failures are eligible; cancellation and unknown crash outcomes are not. |
| Concurrency | global 16 by default; per-job policy limit | Global pressure keeps eligible work queued. Same-job pressure is bounded; catch-up `all` is the explicit bounded queue. |

Examples:

```sh
locron update nightly --overlap replace --missed-run latest
locron update health-check --retries 3 --backoff exponential --retry-delay 10s --retry-cap 5m
locron update heartbeat --overlap allow --per-job-concurrency 4
locron config set global_concurrency 32
```

Reducing global concurrency does not terminate active attempts. The new value applies to the next
admission decision; no new attempt starts while the active count is at or above it.

## Timeout, cancellation, and crash recovery

Attempts time out after 60 seconds by default. Configure `--timeout DURATION` or explicitly remove
the timeout with `--no-timeout`. Process and shell targets run in a process group. Timeout,
cancellation, and replacement send graceful termination, wait the configured grace period, then
force termination. A deliberately detached descendant is outside the portable guarantee.

```sh
locron update nightly --timeout 20m --termination-grace 10s
locron history nightly --limit 20
locron cancel RUN_ID
locron why --run RUN_ID
```

After an unclean daemon exit, a stale active attempt becomes `interrupted_unknown`; locron neither
infers success nor retries or signals its recorded stale process identity. If termination could not
be confirmed, the run remains an active-blocking quarantine. Inspect it first, verify the external
process or side effect independently, then acknowledge only when accepting that uncertainty:

```sh
locron why --run RUN_ID
locron cancel RUN_ID --acknowledge-unconfirmed
locron why JOB_NAME
```

Acknowledgement is not ordinary cancellation. It is valid only for the exact quarantined run,
records an audit fact, ends the run as `interrupted_unknown`, and releases same-job admission.

## Retention

The milestone contract retains terminal run metadata for 90 days, bounded by 1,000 runs per job and
10,000 globally. Output defaults to 30 days, 10 MiB per run, and 256 MiB globally. Active runs are
never pruned; output pruning keeps metadata that explains truncation or removal.

Inspect settings and preview explicit cleanup before applying it:

```sh
locron config get
locron prune --dry-run
locron prune
locron doctor
```

The current build implements bounded explicit output pruning. Automatic startup/periodic cleanup,
startup artifact repair, and complete metadata age/count retention remain pending acceptance work;
see [STATUS.md](STATUS.md). Do not treat configured bounds as proof that every cleanup path has run.

## Plaintext and exactly-once boundaries

Inline environment values, process arguments, shell strings, inline HTTP headers, and inline bodies
are plaintext user-local configuration. Normal inspection and diagnostics redact configured values,
but locron cannot stop a target from printing a secret into captured output. Use runtime files with
owner-only permissions or an external secret provider when values must not be stored in the locron
database.

Plaintext export requires explicit acknowledgement on both export and import:

```sh
locron export --include-values --acknowledge-plaintext > locron-export.json
locron import locron-export.json --accept-plaintext-values --dry-run
locron import locron-export.json --accept-plaintext-values
```

locron prevents duplicate creation of one durable scheduled occurrence. It cannot promise
exactly-once effects for arbitrary processes or HTTP endpoints across a crash. Targets requiring
that property must use `LOCRON_RUN_ID` as an idempotency key and make the external operation
idempotent.

## Diagnostics and recovery checklist

1. Run `locron doctor` to check paths, database integrity, daemon ownership, and wake-socket state.
2. Run `locron why JOB_NAME` for eligibility, next occurrence, policy, and capacity blockers.
3. Run `locron why --run RUN_ID` for attempts, retries, replacement, cancellation, truncation,
   pruning, and the terminal reason.
4. Read `locron history JOB_NAME` and `locron logs RUN_ID --attempt N`; use `--channel` to isolate
   `stdout`, `stderr`, or HTTP `body`.
5. If the daemon is absent, restart `locron daemon run`. Durable queued work remains queued; a wake
   notification is only a hint, never the source of truth.
6. For `interrupted_unknown`, verify the external system before manually rerunning. A new manual run
   has a new run ID and may repeat an already-applied side effect.

`-v` and `--debug` add redacted stderr diagnostics. They do not change JSON stdout and are not a
replacement for `why`.

## Output examples

Current human output is readable JSON plus warnings on stderr:

```text
$ locron run nightly
{
  "run_id": "0198f000-0000-7000-8000-000000000001",
  "state": "queued"
}
warning: daemon is not running; run remains durably queued
```

Machine mode emits one `locron.cli/v1` document on stdout:

```text
$ locron --json run nightly
{"schema":"locron.cli/v1","ok":true,"command":"run","data":{"run_id":"0198f000-0000-7000-8000-000000000001","state":"queued"},"warnings":["daemon is not running; run remains durably queued"]}
```

The reviewed streaming contract uses newline-delimited `locron.stream/v1` records and ends with a
terminal result. Live partial-file follow and that envelope are not implemented yet, so the
following is a contract-shape example, not output the current build can produce:

```text
{"schema":"locron.stream/v1","record":"output record (shape pending implementation)"}
{"schema":"locron.stream/v1","record":"terminal result (shape pending implementation)"}
```

Until streaming acceptance is complete, use terminal `locron logs RUN_ID` output and a separate
`locron why --run RUN_ID` query. Do not build automation around the current `--follow --json`
behavior.
