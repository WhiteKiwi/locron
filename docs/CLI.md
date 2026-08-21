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
locron export [--include-values] [--include-history]
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

Five-field cron accepts wildcard, list, range, and step syntax; case-insensitive three-letter month and weekday names; Sunday as `0` or `7`; and `@yearly`, `@annually`, `@monthly`, `@weekly`, `@daily`, `@midnight`, and `@hourly`. It rejects seconds, year fields, `@reboot`, and Quartz extensions. Duration input accepts integer `s`, `m`, `h`, and `d` units without calendar-month interpretation.

Target-specific flags are rejected with another target kind. Options with mutually exclusive sources, such as inline body and body file, fail before persistence. `update` uses the same validators and creates an immutable revision; changing a schedule requires a complete new schedule selector.

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

Exports use `locron.export/v1`. They contain normalized current job definitions and global settings by default, with optional retained history. Output files are separate artifacts and are not embedded unless explicitly exported by a future command.

Inline environment/header values are redacted by default. `--include-values` requires an interactive confirmation or an explicit non-interactive acknowledgement. Import rejects an export containing plaintext values unless `--accept-plaintext-values` is present. Import validates the whole document before one atomic application and preserves source IDs only when they do not conflict.

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
