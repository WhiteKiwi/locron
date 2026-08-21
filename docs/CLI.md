# locron CLI Contract

## Status and scope

This document owns the reviewed milestone-1 command surface, rendering contract, and exit categories. Product behavior remains authoritative in `docs/SPEC.md`; component ownership remains authoritative in `docs/ARCHITECTURE.md`.

## Global options

```text
locron [--state-dir PATH] [--format human|json] [-v|--verbose...] [--debug] <command>
```

- `--state-dir` overrides state discovery for this invocation.
- `--format human|json` defaults to `human`; `--json` is an alias for `--format json`.
- `-v/--verbose` is repeatable. One level adds decisions and effective values; two levels add timing and storage context intended for operators.
- `--debug` enables developer trace diagnostics on stderr and implies maximum verbose human context. It never changes JSON stdout.
- Color is automatic only for a terminal and can be disabled with `NO_COLOR`; machine output never contains color.

## Command families

The first binary exposes these top-level commands:

```text
locron add NAME <schedule> <target> [policy options]
locron update NAME [job changes]
locron list [--all]
locron show NAME
locron enable NAME
locron disable NAME
locron remove NAME
locron preview <schedule-or-name> [--count N]
locron run NAME [--wait] [--dry-run]
locron cancel RUN_ID
locron history [NAME] [--limit N]
locron logs RUN_ID [--attempt N] [--follow] [--channel all|stdout|stderr|body]
locron why NAME
locron why --run RUN_ID
locron config get [KEY]
locron config set KEY VALUE [--dry-run]
locron export [--include-values --acknowledge-plaintext] [--include-history]
locron import PATH [--accept-plaintext-values] [--dry-run]
locron prune [--dry-run]
locron doctor
locron daemon run
```

Job references accept an exact live name or canonical UUID. Run references are canonical UUIDs. Human output may abbreviate an ID only in decorative tables; copyable output always includes the full ID.

## Schedule and target syntax

Exactly one schedule selector is required when creating a job:

```text
--cron EXPR [--timezone local|IANA_NAME]
--every DURATION [--anchor RFC3339]
--at RFC3339
```

Exactly one target is required:

```text
-- COMMAND [ARG ...]             direct process, argv preserved
--shell COMMAND                  explicit shell string
--http METHOD URL                HTTP target
```

The complete target and environment option vocabulary is:

```text
--cwd PATH
--env NAME=VALUE... [--clear-env on update] [--unset-env NAME... on update]
--env-file PATH | --no-env-file (update only)
--path PATH_LIST | --no-path (update only)
--shell-executable ABSOLUTE_PATH
--body TEXT | --body-file PATH | --json-body JSON | --clear-body (update only)
--header NAME=VALUE...
--header-env NAME=ENV_NAME...
--unset-header NAME... | --clear-headers (update only)
--success-status STATUS_OR_RANGE... [--clear-success-statuses on update]
--follow-redirects | --no-follow-redirects (update only)
```

`--cwd` and `--shell-executable` apply only to process/shell targets. Environment and PATH options
apply to every target because HTTP header environment sources use the same effective environment.
Runtime file and working-directory paths are expanded, made absolute, and lexically normalized at
registration. Job execution-PATH entries receive the same normalization. A path-bearing process
executable is normalized against the effective working directory; a bare executable remains bare.
A readable env file with group/other permission bits produces a path-only warning without reading
or printing its contents.

`--json-body` validates one JSON value, stores its normalized UTF-8 encoding, and supplies
`Content-Type: application/json` unless that header is explicitly configured. A partial body update
preserves an existing explicit content type. Status input accepts one integer or an inclusive range
such as `200-204`; ranges are normalized to a sorted unique list. Clearing and supplying success
statuses in one update means clear-then-set. An inline header and `--header-env` for the same
case-insensitive header name conflict.

Policy options are shared by add and update:

```text
--overlap skip|replace|allow
--missed-run skip|latest|all
--start-deadline DURATION | --no-start-deadline (update only)
--catch-up-limit 1..1000
--retries 0..10
--backoff fixed|exponential
--retry-delay DURATION
--retry-cap DURATION
--retry-timeout | --no-retry-timeout (update only)
--timeout DURATION | --no-timeout
--termination-grace DURATION
--per-job-concurrency N
```

Per-job concurrency is validated against the current durable global setting. A creation dry-run
uses the durable setting when state exists and the documented default 16 otherwise.

Five-field cron accepts wildcard, list, range, and step syntax; case-insensitive three-letter month and weekday names; Sunday as `0` or `7`; and `@yearly`, `@annually`, `@monthly`, `@weekly`, `@daily`, `@midnight`, and `@hourly`. It rejects seconds, year fields, `@reboot`, and Quartz extensions. Duration input accepts integer `s`, `m`, `h`, and `d` units without calendar-month interpretation.

Target-specific flags are rejected with another target kind. Options with mutually exclusive sources, such as inline body and body file, fail before persistence. `update` uses the same validators and creates an immutable revision; changing a schedule requires a complete new schedule selector.

## Update semantics

Every supplied update field is overlaid on the current normalized definition; omitted fields remain
byte-for-byte equivalent after normalization. `--rename`, `--description`/`--clear-description`,
`--tag`/`--clear-tags`, and `--enabled`/`--disabled` update metadata. Repeated `--tag` replaces the
complete tag list. Repeated env and header flags upsert named entries; explicit unset/clear flags
remove them.

A new target selector replaces the target and must be complete. Target-specific options without a
selector patch the existing compatible target and fail for another kind. A schedule change requires
exactly one complete selector; its new revision cursor begins at update commit time, and a new
interval without `--anchor` anchors at that same time. A non-schedule update carries the existing
cursor boundary forward. A request with no effective normalized change is rejected and creates no
revision.

Update dry-run opens existing state read-only and returns redacted normalized `before`, `after`, and
a deterministic sorted `changed_fields` list. It creates no revision, ID, cursor, queue sequence, or
wake notification.

## Dry run

`--dry-run` is valid for `add`, `update`, `run`, `config set`, `import`, and `prune`.

- It opens existing state read-only and never initializes or migrates a database.
- It performs parsing, normalization, cross-field validation, redacted target resolution, and relevant schedule/admission calculation.
- It does not allocate a durable UUID or queue sequence. A displayed placeholder is marked non-durable.
- It does not create, edit, enable, disable, remove, enqueue, cancel, signal, prune, rename output, or make an HTTP request.
- A run dry-run reports the policy decision that would be made at that instant and clearly states that capacity was not reserved.
- When required state does not exist, a creation dry-run uses documented defaults; a dry-run referring to existing state reports not found.

## Why and diagnostics

`why NAME` reports the current revision, enabled/deleted state, normalized schedule and timezone, cursor and next occurrence, missed-run/deadline interpretation, active runs, applicable overlap/per-job/global capacity decision, daemon ownership health, and redacted execution resolution.

`why --run RUN_ID` reports the immutable trigger and nominal time, state transitions, attempt outcomes, retry decisions, cancellation/replacement/supersession facts, output truncation/pruning facts, and terminal reason. Unknown facts are stated as unknown rather than inferred.

Verbose and debug output never replaces `why`: verbosity explains what the current command is doing, while `why` explains durable scheduler decisions. Diagnostics go to stderr. Secrets and configured values remain redacted at every level.

## Machine output

Every command supports one JSON document on stdout:

```json
{
  "schema": "locron.cli/v1",
  "ok": true,
  "command": "run",
  "data": {},
  "warnings": []
}
```

Errors use the same envelope with `ok: false` and an `error` object containing a stable lowercase-snake-case `code`, human message, and optional structured details. IDs are lowercase canonical UUID strings. Instants are RFC 3339 UTC with microseconds. Durations are integer microseconds. Arbitrary output bytes are represented with an explicit UTF-8 or base64 encoding tag.

Progress, verbose context, debug traces, and wait/follow output do not corrupt the JSON result. Streaming JSON uses newline-delimited envelopes with schema `locron.stream/v1`; the final record is a terminal result. Human prose is not a compatibility surface.

## Export and import

Exports use `locron.export/v1` and contain typed global settings plus normalized current live job
definitions. Human-mode stdout is the bare export document suitable for redirection; JSON mode keeps
the required single `locron.cli/v1` envelope and places that document in `data`. Output artifacts and
history are not included in this tranche. `--include-history` is rejected explicitly rather than
claiming an incomplete backup.

Default exports use `values_mode: "redacted"`. Sensitive inline environment values, inline HTTP
header values, and inline/JSON bodies are removed, never replaced with a literal sentinel, and each
job carries a sorted `omitted_values` path list. Import rejects a redacted document with any omission
because faithful creation or update is impossible. A redacted document without omissions remains
importable.

Plaintext export is deliberately non-interactive and requires both `--include-values` and
`--acknowledge-plaintext`; either flag alone is invalid. Plaintext import requires
`--accept-plaintext-values`. This two-sided acknowledgement is required for a faithful round trip.

Import accepts a bare `locron.export/v1` document, validates and normalizes the entire document
before opening a write transaction, and then applies settings and jobs atomically. Duplicate source
IDs/names or destination ambiguities reject the whole document. Resolution is deterministic:

1. A source ID matching a live destination job updates that job.
2. Otherwise a source name matching one live destination job updates that job and keeps its local ID.
3. Otherwise import creates a job, preserving the source ID only when no live or removed destination
   job owns it; an ID collision allocates a new UUIDv7.
4. A source ID and source name resolving to two different jobs is a conflict.

An effective update creates exactly one immutable revision; an identical import entry is a no-op.
Schedule changes begin at the import commit boundary, while unchanged schedules carry their cursor
boundary forward. Import dry-run is read-only, allocates no durable IDs, and reports deterministic
create/update/no-op actions; prospective collision allocations are shown as non-durable placeholders.

## Exit categories

| Code | Meaning |
|---:|---|
| 0 | Command succeeded; enqueue success does not imply target success |
| 1 | A waited-for target reached a non-success terminal outcome |
| 2 | CLI syntax or validation error |
| 3 | Requested identity not found or durable conflict |
| 4 | State unavailable, busy, locked for migration, daemon unavailable where required, or incompatible schema |
| 5 | Unexpected storage, I/O, protocol, or internal failure |
| 130 | The foreground client was interrupted; durable execution is not implicitly cancelled |

Machine output additionally carries the stable error or outcome code, so callers need not infer detail from the numeric category.
