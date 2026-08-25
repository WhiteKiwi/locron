# locron CLI Contract

## Status and scope

This document owns the reviewed milestone-1 command surface, rendering contract, and exit categories. Product behavior remains authoritative in `docs/SPEC.md`; component ownership remains authoritative in `docs/ARCHITECTURE.md`. Operator procedures and completion evidence live in `docs/OPERATOR.md` and `docs/ACCEPTANCE.md`.

## Global options

```text
locron [-V|--version] [--state-dir PATH] [--format human|json] [-v|--verbose...] [--debug] <command>
```

- `--state-dir` overrides state discovery for this invocation.
- `--format human|json` defaults to `human`; `--json` is an alias for `--format json`.
- `-V/--version` prints `locron <version>` on stdout and exits 0 without opening the state directory; it honors `--format json` per the machine-output contract. The short `-v` remains the repeatable verbose flag.
- `-v/--verbose` is repeatable. One level adds decisions and effective values; two levels add timing and storage context intended for operators.
- `--debug` enables developer trace diagnostics on stderr and implies maximum verbose human context. It never changes JSON stdout.
- Color is automatic only for a terminal and can be disabled with `NO_COLOR`; machine output never contains color.

## Command families

The first binary exposes these top-level commands:

```text
locron add NAME <schedule> <target> [policy options]
locron update NAME [job changes]
locron list|ls [--all] [--no-trunc]
locron show NAME
locron enable NAME
locron disable NAME
locron remove|rm NAME
locron preview <schedule-or-name> [--count N]
locron run NAME [--wait] [--dry-run]
locron cancel RUN_ID [--acknowledge-unconfirmed]
locron history [NAME] [--limit N] [--no-trunc]
locron logs RUN_ID [--attempt N] [--follow] [--channel all|stdout|stderr|body]
locron why NAME
locron why --run RUN_ID
locron explain NAME_OR_ID
locron config get [KEY]
locron config set KEY VALUE [--dry-run]
locron config unset KEY [--dry-run]
locron export [--jobs NAME[,NAME...]] [--tag TAG[,TAG...]] [--include-values --acknowledge-plaintext] [--include-history]
locron import PATH|URL [--accept-plaintext-values] [--dry-run]
locron prune [--dry-run]
locron doctor
locron daemon run
locron service install|uninstall|status
locron dashboard [--port N] [--bind ADDR]   (alias: dashboard serve)
locron dashboard enable [--reset]
locron dashboard disable
locron dashboard status
locron dashboard token
locron self-update
```

A visible alias is accepted for `list` (`ls`) and `remove` (`rm`). An alias is the same command in every respect: it accepts the identical options and arguments and renders identical human and machine output. The `command` field of the `locron.cli/v1` envelope always reports the canonical name (`list`, `remove`), and usage and help render the canonical name with the alias shown for discovery. Aliases are a keyboard convenience; they do not add a semantic surface.

Human `list` output is a docker-style aligned table on stdout: a header line (`NAME`, `SCHEDULE`, `TARGET`, `ENABLED`) followed by one left-aligned row per live job, sorted by name. `--all` includes disabled jobs, and their `ENABLED` value distinguishes them. The header prints even when no job exists, matching `docker ps` with zero containers. Schedule summaries render as `cron 'EXPR'`, `every DUR`, or `at RFC3339`; target summaries as `run EXE [ARGS...]`, `shell CMD`, or `http METHOD URL`; enabled state as `yes` or `no`. When standard output is a terminal and the table would exceed the terminal width, the `TARGET` column — the table's final data column — is truncated to fit and marked with a trailing `…`; `NAME` and `SCHEDULE` always print in full. Truncation follows character display width (East Asian wide characters count as two columns). When standard output is redirected or piped, values print in full as before. `--no-trunc` prints full `TARGET` values on a terminal; the flag is accepted with machine output and has no effect there. Machine output is unchanged.

Job references accept an exact live name or canonical UUID. Run references are canonical UUIDs. Human output may abbreviate an ID only in decorative tables; copyable output always includes the full ID.

Ordinary `cancel RUN_ID` retains the normal queued/active cancellation semantics. A run quarantined
because process-group termination could not be confirmed rejects ordinary cancellation and explains
that explicit acknowledgement is required. `--acknowledge-unconfirmed` is valid only for that exact
quarantined run: it accepts the risk that the target may still exist outside locron's knowledge,
records an audit event, terminalizes the run as `interrupted_unknown`, and releases same-job
admission without signalling any recorded PID/PGID. Using the flag for another state or repeating it
after completion is a durable conflict. JSON output distinguishes `requested` cancellation from an
`acknowledged_unconfirmed` resolution.

## Global environment configuration

Named global environment values use the existing configuration key family. The canonical key is
`environment.NAME`, so an operator manages one value without replacing the complete map:

```text
locron config set environment.API_TOKEN VALUE [--dry-run]
locron config unset environment.API_TOKEN [--dry-run]
locron config get environment.API_TOKEN
```

`NAME` uses the same case-sensitive environment-name grammar as job `--env`: a leading ASCII letter
or underscore followed by ASCII letters, digits, or underscores. Names beginning exactly
`LOCRON_` are reserved and are rejected by get, set, unset, dry-run, and import. Values are UTF-8,
may be empty, and cannot contain NUL. Supplying `environment` without `.NAME`, an empty name, or an
unknown configuration key is a validation error.

Global environment values are sensitive plaintext configuration. Human `config get` output lists
configured names in lexical order as `environment.NAME=<redacted>`; a single-key read reports only
whether that name is configured. Set output is `environment.NAME: configured (value redacted)` and
unset output is `environment.NAME: unset`. A missing-name unset succeeds as an unchanged operation.
No verbosity or debug level prints a current, proposed, imported, or exported value.

Machine output never places a global environment value in `data`, `warnings`, or error details. A
single get returns `{"key":"environment.NAME","configured":true,"value_redacted":true}`. A set or
unset returns the same key plus `action` (`created`, `replaced`, `removed`, or `unchanged`), the
resulting `configured` boolean, `value_redacted: true`, and `dry_run`. An all-settings get returns
ordinary typed settings plus an `environment` object whose lexically sorted names each map to
`{"configured":true,"value_redacted":true}`.

Environment set and unset dry-runs use the ordinary read-only config path. They perform full name
and value validation and report the action that would occur, but do not initialize or migrate state,
write settings, or wake the daemon. When state does not exist, the documented empty environment
default is used. The proposed value remains redacted even though no write occurs.

## Schedule and target syntax

Exactly one schedule selector is required when creating a job:

```text
--cron EXPR [--timezone local|IANA_NAME]
--every DURATION [--anchor RFC3339]
--at RFC3339 [--delete-after-run]
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

`--dry-run` is valid for `add`, `update`, `run`, `config set`, `config unset`, `import`, and `prune`.

- It opens existing state read-only and never initializes or migrates a database.
- It performs parsing, normalization, cross-field validation, redacted target resolution, and relevant schedule/admission calculation.
- It does not allocate a durable UUID or queue sequence. A displayed placeholder is marked non-durable.
- It does not create, edit, enable, disable, remove, enqueue, cancel, signal, prune, rename output, or make an HTTP request.
- A run dry-run reports the policy decision that would be made at that instant and clearly states that capacity was not reserved.
- When required state does not exist, a creation dry-run uses documented defaults; a dry-run referring to existing state reports not found.

## Self-update

`locron self-update` replaces the running binary with the latest stable release.

- It resolves the latest release through the GitHub releases API (`https://api.github.com/repos/WhiteKiwi/locron/releases/latest`, overridable with `LOCRON_UPDATE_API_BASE`), then downloads the target platform's tarball and the release's `SHA256SUMS.txt` from `https://github.com/WhiteKiwi/locron/releases/download/<tag>/` (overridable with `LOCRON_UPDATE_ASSET_BASE`).
- The tarball is verified against its published SHA-256 before anything is touched; the binary is then replaced with one temp file and an atomic rename in the executable's directory. A failed, cancelled, or interrupted update leaves the existing binary installed and working.
- When the running version is the latest (or newer), the command exits 0 with `updated: false` and downloads nothing.
- Self-update first honors the Homebrew/package-manager marker (`lib/.disable-self-update` next to
  the executable): it refuses with exit 3 and code `update_managed_install`, directing the user to
  `brew upgrade locron`. Otherwise it proceeds only when the canonical executable has the valid
  versioned standalone-installer receipt written beside it by `install.sh`. A missing, malformed,
  or mismatched receipt refuses before network access with exit 3 and code
  `update_unowned_install`; guidance explains that Cargo users update with
  `cargo install --locked locron`, older script installations can adopt the receipt by rerunning
  the installer, and other users must update through their installation channel. Manually copied
  tarballs and source builds do not gain self-update authority merely from their filesystem path.
- Supported platforms are aarch64 and x86_64 on macOS and glibc Linux; musl Linux and other platforms fail with exit 2 and code `update_unsupported_platform`.
- The daemon and any running `locron` process keep the old code until they restart; self-update never signals running processes.

Machine output for a success carries `data`:

```json
{
  "schema": "locron.cli/v1",
  "ok": true,
  "command": "self-update",
  "data": {
    "current_version": "0.2.0",
    "new_version": "0.2.1",
    "updated": true
  },
  "warnings": []
}
```

Stable error codes: `update_unsupported_platform` (2), `update_managed_install` (3),
`update_unowned_install` (3), `update_rate_limited` (5), `update_network` (5),
`update_release_metadata` (5), `update_checksum_mismatch` (5), `update_io` (5).

After a successful replace, self-update runs `locron service install` on the replaced executable to refresh an existing service registration onto the new binary or to perform a first registration. This registration is best-effort: a failure produces a warning in the envelope (and on stderr in human mode) while the update stays successful.

## Service installation

`locron service install|uninstall|status` manages the daemon's registration with the platform's per-user service manager, without administrative privileges. The command family exists only on macOS and glibc Linux; any other platform fails with exit 2 and code `service_unsupported_platform`.

- On macOS the registration is a per-user LaunchAgent (`~/Library/LaunchAgents/dev.locron.daemon.plist`) that runs `<executable> daemon run` with `KeepAlive` and `RunAtLoad`, and writes the daemon log to `~/Library/Logs/locron/daemon.log`. The service is bootstrapped into the `gui/<uid>` domain with a `user/<uid>` fallback for sessions without a GUI.
- On Linux the registration is a systemd user unit (`~/.config/systemd/user/locron.service`) with `Restart=on-failure` and `WantedBy=default.target`, activated through `systemctl --user enable --now`; the daemon stops at logout and starts again at the next login.
- The registration always records the canonicalized absolute path of the running binary, so repeating an install after an update or a binary move refreshes the registration onto the current binary. A loaded service is signaled with SIGTERM and restarted under the engine's ordinary graceful-shutdown rules; active work completes or retries per the normal policies.
- When no per-user service manager session is available (SSH, containers, a machine without systemd), `service install` completes successfully with exit 0 and prints explicit guidance for registering and starting the daemon later. `service uninstall` still removes a stale registration.
- When a manually started daemon holds the state-directory lock, `service install` registers and enables the service but defers the start, with guidance; the service starts when the manual daemon exits. This preserves the single-owner guarantee: a registered service never creates a second scheduler for one state directory.
- Installations whose binary carries the package-manager marker (`lib/.disable-self-update` next to the executable) refuse `install` and `uninstall` with exit 3 and code `service_managed_install`, directing the user to the package manager's own service mechanism (for Homebrew, `brew services`).
- `service uninstall` signals SIGTERM, waits for the daemon process to exit, removes the service from the manager, and deletes the registration file. The binary itself is never touched.

Machine output for a successful `service install` carries `data`:

```json
{
  "schema": "locron.cli/v1",
  "ok": true,
  "command": "service install",
  "data": {
    "registered": true,
    "restarted": false,
    "deferred": false,
    "service_name": "dev.locron.daemon",
    "domain": "gui/501"
  },
  "warnings": []
}
```

`restarted` is true when an existing loaded service was refreshed onto the current binary; `deferred` is true when the start waits on a manual daemon holding the lock; `domain` names the manager domain (launchd) and is omitted when the manager has no domains (systemd); a no-session install includes `guidance` with the registration instructions. `service uninstall` carries `{"removed": bool, "stopped": bool, "service_name": ...}`; `service status` carries `registered`, `loaded`, `enabled` (bool or null), `domain`, `pid`, `executable`, `session_available`, and `service_name`.

Stable error codes: `service_unsupported_platform` (2), `service_managed_install` (3), `service_command_failed` (5), `service_io` (5).

## Dashboard (web administration)

`locron dashboard` serves the loopback-only web administration surface planned in
`docs/dashboard/SPEC.md` and `docs/dashboard/IMPLEMENTATION.md`.

- `locron dashboard` (identical to `locron dashboard serve`) runs the server in the foreground:
  it binds loopback only (`127.0.0.1` and `::1` by default; `--bind` accepts only loopback
  values and any other value is refused), prints the exact access URL (human form and the
  `locron.cli/v1` machine envelope per the machine-output contract), and serves until a signal.
  It does not require the daemon to be running and never takes daemon ownership.
- The default port is 10824. In foreground mode an occupied default port falls back to the next
  free port (up to ten successive ports, then an OS-assigned port) and the chosen URL is printed.
  In service mode the port is fixed: an occupied port makes the server exit and `status` reports
  the conflict. An explicit `--port N` is always strict and fails with an actionable error when
  occupied. When only one loopback family can be bound, the server warns and continues on the
  other.
- `locron dashboard enable` is the persistent path: it generates the access token when absent,
  registers a per-user dashboard service (`dev.locron.dashboard` LaunchAgent on macOS,
  `locron-dashboard.service` systemd user unit on Linux — a second registration target on the
  same service-manager port as the daemon, never touching the daemon registration), starts it
  immediately, and arranges automatic start at login. Repeating it refreshes and repairs the
  registration. `enable --reset` regenerates the token, then refreshes and restarts the service
  (the server reads the token at startup, so the restart invalidates the old token and any
  outstanding session cookies). `locron dashboard disable` unregisters the service and removes
  the token, warning when a foreground instance may still be running; `status` reports the
  service state, the access URL, and token facts (presence and file-permission posture only —
  never the token value); `token` re-displays the access token.
- Registration operations refuse package-manager-managed binaries exactly like `service`
  (`service_managed_install`, exit 3); foreground serving stays allowed.
- `locron doctor` additionally reports the dashboard exposure facts: token file presence and
  permission posture and whether a dashboard service is registered. It does not report the
  server as running; `dashboard status` does.

The access token is 32 random bytes hex-encoded (64 characters), stored owner-only (0600) in the
state directory, generated on first use, reused afterwards, and regenerated only by
`enable --reset` or removing the token file. It never appears in a URL, in logs, in diagnostics,
or in any response. The server accepts it through an `Authorization: token <t>` header (scripts
and automation) and through a one-time paste at the entry page, which sets a 90-day `SameSite=Lax`
session cookie (HttpOnly) and a `csrf_token` double-submit cookie. Every state-bearing page and
API endpoint requires the token or session cookie; the entry page is the only unauthenticated
response. State-changing requests are additionally protected against DNS rebinding (Host-header
allowlist: `localhost`, `127.0.0.1`, `[::1]`, port ignored), cross-origin submission (Origin
check on unsafe methods), and CSRF (double-submit token required on cookie-authenticated
mutations; bearer-token requests are exempt). All responses carry `Referrer-Policy: no-referrer`.

### HTTP API envelope and status mapping

Machine-readable API results use the versioned `locron.api/v1` envelope: success is
`{"schema":"locron.api/v1","ok":true,"data":...,"warnings":[...]}` and error is
`{"schema":"locron.api/v1","ok":false,"error":{"code":"...","message":"..."}}` (the schema field
is present in the error shape as well). `code` carries the stable CLI error categories verbatim:
`invalid_request` (validation), `not_found`, `durable_conflict`, `state_error`, and the service
categories. Errors map to HTTP statuses as follows:

| HTTP status | CLI category meaning |
|---|---|
| 400 | validation (`invalid_request`) |
| 401 | unauthenticated (no or invalid token/session) |
| 403 | refused or permission: Host not loopback, Origin mismatch, CSRF mismatch |
| 404 | `not_found` |
| 409 | `durable_conflict` (revision mismatch, no-op update, quarantine conflicts, busy) |
| 503 | daemon-required or state unavailable |
| 500 | unexpected internal failure (`state_error`) |

Route families map one-to-one onto the durable CLI command families — job
add/update/list/show/enable/disable/remove, schedule preview, manual run, cancel with
quarantine acknowledgement, history, logs, why, the full `config` surface (`environment.NAME`
grammar and redaction preserved), export download with the same acknowledgement rules, import
upload (including server-side URL import with the same TLS/16 MiB/10-redirect/30-second caps as
the CLI and userinfo rejection), prune, and diagnostics. Dry-run is supported wherever the CLI
supports it and never writes or wakes. Redaction goes through the shared core boundary, so API
payloads carry the same `<redacted>`/`value redacted` markers as the CLI. A mutation is durable
before the API reports success, and it sends the same best-effort wake hint.

## Why and diagnostics

`why NAME` reports the current revision, enabled/deleted state, normalized schedule and timezone, cursor and next occurrence, missed-run/deadline interpretation, active runs, applicable overlap/per-job/global capacity decision, daemon ownership health, and redacted execution resolution.

`why --run RUN_ID` reports the immutable trigger and nominal time, state transitions, attempt outcomes, retry decisions, cancellation/replacement/supersession facts, termination-unconfirmed acknowledgement, output truncation/pruning facts, and terminal reason. Unknown facts are stated as unknown rather than inferred.

`explain NAME_OR_ID` is the consolidated, live-job summary. It reports the job's canonical identity,
enabled state, revision, normalized schedule summary and timezone, next occurrence, current
eligibility posture, overlap decision, active-run count, global concurrency limit, and daemon
availability. It then reports the most recent run of any state and the most recent anomalous
terminal run, ordered by durable `requested_at_us` and canonical run identity. An anomaly is a
terminal state other than `succeeded`: `failed`,
`timed_out`, `cancelled`, `skipped_overlap`, `skipped_concurrency`, or `interrupted_unknown`.

Each reported run carries its full canonical run ID, trigger, nominal time, request time, current or
final state, actual start, finish, duration, and durable reason. The latest run and latest anomaly
may be the same run. `none` is printed when there is no run history, no anomaly history, a manual run
has no nominal time, or a disabled job has no next occurrence. An execution timing or reason that is
not yet known prints `unknown`; machine output uses `null` for either kind. A disabled job has no
next occurrence and an eligibility value of `disabled`; an enabled job reports
`subject_to_admission`, because the summary does not reserve or fully simulate current global and
per-job capacity. The separate overlap decision is `no_active_run`, `would_skip_overlap`,
`would_replace`, or `eligible_subject_to_capacity`. The reported global concurrency value is the
configured limit, not current usage. Daemon availability is a separate fact and does not rewrite
the scheduler eligibility posture. A removed job is not explainable through this live-job command;
its retained runs remain available through `history` and `why --run RUN_ID`. If a removed name is
reused, `explain` resolves only the new live identity and its history.

Machine output uses the ordinary `locron.cli/v1` envelope with `command: "explain"`. Its `data`
object has `job`, `schedule`, `current_status`, `latest_run`, and `latest_anomaly` objects matching
the human sections. The two run values are nullable. This summary intentionally omits immutable
snapshots, attempts, and events; the canonical run ID is the handoff to `why --run` for that detail.
It does not infer sleep, suspend, or another unrecorded cause.

Verbose and debug output never replaces `why`: verbosity explains what the current command is doing, while `why` explains durable scheduler decisions. Diagnostics go to stderr. Secrets and configured values remain redacted at every level.

## Human rendering

Human output renders the same facts as machine output in readable forms; machine output is the compatibility surface. The following contract applies to the human format only.

- `list` — the table documented in Command families (`NAME`, `SCHEDULE`, `TARGET`, `ENABLED`).
- `history` — an aligned table with the header always printed and one row per run, newest first:
  `TIME | JOB | TRIGGER | STATE | DURATION`. `TIME` is RFC 3339 UTC; `DURATION` renders in the
  largest whole human unit; the run ID may be abbreviated in the table only. On a terminal,
  `TRIGGER` is truncated with a trailing `…` when needed to fit the table while every other column
  remains intact. Redirected or piped output keeps full trigger values. `--no-trunc` restores full
  trigger values on a terminal and is accepted with no effect in machine mode.
- `show NAME` — labeled sections `JOB` (name, id, enabled, tags, revision), `SCHEDULE`,
  `TARGET`, and `POLICIES` (overlap, missed-run, deadline, retries, timeout, concurrency), one
  field per line.
- `add` / `update` — `job added: NAME (ID)` / `job updated: NAME (ID, revision N)` followed by
  the schedule and target summary lines in the form `list` uses.
- `enable` / `disable` — `job enabled: NAME` / `job disabled: NAME`.
- `remove` — `job removed: NAME`.
- `run` — `run queued: RUN_ID (job NAME)`; `--wait` follows with the streamed output and ends in
  a terminal outcome line; `--dry-run` prints the admission decision and states that no run was
  created.
- `cancel` — `cancellation requested: RUN_ID`, or the acknowledged-unconfirmed resolution line
  for a quarantined run.
- `preview` — a first line naming the schedule, then one RFC 3339 occurrence per line.
- `why NAME` — sections `JOB`, `SCHEDULE`, `ELIGIBILITY`, `POLICIES`, `DAEMON`; `why --run ID` —
  sections `RUN`, `ATTEMPTS`, and a terminal-reason section. One field per line; unknown facts
  state `unknown`.
- `explain NAME_OR_ID` — sections `JOB`, `SCHEDULE`, `CURRENT STATUS`, `LATEST RUN`, and
  `LATEST ANOMALY`. The last two sections use the same run-summary field order; an absent record is
  written as `none` rather than omitted. A manual run's non-applicable nominal time is also `none`;
  a start, finish, duration, or reason that is not yet known is `unknown`. Human eligibility renders
  as `disabled` or `subject to admission`; overlap decisions render as `no active run`, `would skip
  (overlap policy)`, `would replace`, or `eligible subject to capacity`. The corresponding machine
  values remain stable lowercase snake-case codes.
- `doctor` — one line per check: `ok   …`, `warn …`, or `fail …` carrying the check name and the
  fact or path it verified.
- `config get` — `KEY=VALUE` per configured key, sensitive keys redacted as documented.
  `config set` / `config unset` — `KEY: <action>` lines per the documented environment forms.
- `import` — `created N, updated N, unchanged N` plus per-job action lines; dry run states the
  simulation. `prune` — `pruned: N runs, M outputs (X bytes)`; dry run states what would be
  pruned.
- `export` — the bare export document (existing). `logs`, `service`, `self-update`, `version`,
  `daemon run`, and `mcp` keep their existing human forms.

No human form prints an escaped JSON string, nested object, or array, and every form obeys the
redaction rules at every verbosity level.

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

Errors use the same envelope with `ok: false` and an `error` object containing a stable lowercase-snake-case `code`, human message, and optional structured details. Version output uses the same envelope with `"command":"version"` and `data` `{"version":"0.1.1"}`. IDs are lowercase canonical UUID strings. Instants are RFC 3339 UTC with microseconds. Durations are integer microseconds. Arbitrary output bytes are represented with an explicit UTF-8 or base64 encoding tag.

Progress, verbose context, debug traces, and wait/follow output do not corrupt the JSON result. Streaming JSON uses newline-delimited envelopes with schema `locron.stream/v1`; the final record is a terminal result. Human prose is not a compatibility surface.

## Export and import

Exports use `locron.export/v1` and contain typed global settings plus normalized current live job
definitions. Human-mode stdout is the bare export document suitable for redirection; JSON mode keeps
the required single `locron.cli/v1` envelope and places that document in `data`. Output artifacts and
history are not included in this tranche. `--include-history` is rejected explicitly rather than
claiming an incomplete backup.

Without `--jobs` or `--tag`, export follows the invocation context. In an interactive terminal —
standard input, standard output, and standard error are all terminals, the `CI` environment
variable is unset, and the format is human — export shows a selection interface on standard error
listing every job,
initially all selected; confirming the initial selection exports the complete job set. In every
other context (pipe, redirection, no terminal, `CI` set, or JSON mode) export skips the interface
and exports every job. `--jobs` and `--tag` select by exact job name and exact tag, combine as a
union, deduplicate by job identity, and never show a selection interface in any context; a selector
value matching no job is a validation error before any output is produced.

The selection interface never writes to standard output: in every mode standard output carries only
the export document, so redirection and the single-result machine-readable contract are unchanged.

Default exports use `values_mode: "redacted"`. Global and job inline environment values, inline HTTP
header values, and inline/JSON bodies are removed, never replaced with a literal sentinel. The
document carries a sorted `omitted_values` JSON Pointer list for omitted global settings, including
paths such as `/settings/environment/API_TOKEN`, and each job carries its sorted omission list.
Import rejects a redacted document with any global or job omission because faithful creation or
update is impossible. A redacted document without omissions remains importable.

Plaintext export is deliberately non-interactive and requires both `--include-values` and
`--acknowledge-plaintext`; either flag alone is invalid. Plaintext import requires
`--accept-plaintext-values`. These acknowledgements apply equally to global environment values and
job values and are required for a faithful round trip. Without them, no command accepts or emits a
plaintext export containing either kind of value.

Import accepts a local path or an absolute HTTP or HTTPS URL; the document is validated and applied
identically regardless of source. A URL is fetched with mandatory TLS certificate verification, a
bounded redirect and size limit, and a total timeout; fetch failures map to the unexpected
I/O/protocol error category with retry guidance, and document validation failures keep their
existing categories. Import never prompts; dry-run reports the exact actions without writing. An
export document registers executable schedules: importing from a URL carries the same trust
boundary as installing a script obtained from that URL, and first-time imports should be reviewed
with `--dry-run`.

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
