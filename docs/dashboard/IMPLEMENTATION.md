# locron Dashboard Implementation Plan

## Status and authority

This document plans the dashboard surface against the frozen `SPEC.md` (this directory), which is
phase 1 of the deferred product roadmap in `docs/TODO.md`. The frozen `docs/SPEC.md` is not
amended: per the roadmap, this phase does not change its exclusions. Research evidence and rejected
alternatives are recorded in `docs/FINDINGS.md` §14 (security, transport, framework — including the
2026-08-24 default-port verification) and §16 (UI and API design); this document records the
accepted implementation choices and their trade-offs, including the 2026-08-24 product decisions.

Durable-structure changes (workspace membership, the redaction boundary) update
`docs/ARCHITECTURE.md` before code, per the repository workflow.

## Architecture and approach

The surface is a new workspace library crate, `locron-server`, composed by `locron-cli` through the
`locron dashboard` command family:

- `locron-server` depends on `locron-core` and `locron-store` only. It never depends on
  `locron-cli` and never talks to SQLite tables directly — every read and mutation goes through the
  durable application commands, exactly like the CLI and MCP surfaces.
- `locron-cli` remains the composition root: it owns argument parsing, human/machine rendering,
  state-path discovery, the service registration through the existing service-manager port, and the
  `locron dashboard` wiring that constructs the store and starts the server. The server is a
  library; the distributable binary remains the single `locron`.
- `locron-engine` is untouched. The server does not take daemon ownership, does not run scheduling,
  and sends only the same best-effort wake hint the CLI sends after a durable mutation.

This mirrors the architecture doc's own forward reference ("Future viewer/API, MCP, and desktop
crates are added only when those milestones begin") and keeps the engine's daemon runtime free of
any HTTP surface.

### Redaction boundary move

The central redaction helpers (`redacted_job`, `redact_definition`, `redacted_observable_run`,
`redacted_run`, `redacted_settings_value`) currently live in `locron-cli/src/main.rs`. A server
crate cannot depend on the CLI, and the desktop surface later needs the same boundary, so the
redaction functions move to `locron-core` as the shared redaction boundary. The CLI keeps its
rendering and calls the core boundary; its contract tests stay green without output changes. This
is a durable-responsibility change and is recorded in `docs/ARCHITECTURE.md` (core gains
"redaction boundary" responsibility) before the move.

## Accepted implementation decisions

### Accepted: framework and crate

Use `axum` 0.8.9 (declared MSRV 1.80, below the workspace MSRV 1.94), `tokio-stream` 0.1.19 for
stream adapters, `rust-embed` 8.12 with only the `mime-guess` feature for bundled static assets,
`axum-extra` 0.12 with only the `cookie` feature for the `CookieJar`, and `getrandom` 0.4 for the
token RNG (already transitively present via uuid; adding it as a direct dependency costs zero new
crates). All MSRVs verified 2026-08-24 in `docs/FINDINGS.md` §17: rust-embed 1.80, axum-extra
1.80, cookie 0.18 1.56, getrandom 1.85. `tower-http` is not added: the middleware here
(Host/Origin/CSRF/token/Referrer-Policy) is small, explicit, and testable by hand, and the assets
are embedded rather than served from disk. No other framework, no WebSocket crate, and no Node
build toolchain enter the repository.

Static assets are served by one custom handler (`/` plus `/{*path}`) following rust-embed's own
axum example: `Asset::get`, `Content-Type` from `content.metadata.mimetype()`, body from
`content.data`, 404 on a miss, and a `Cache-Control: no-cache` response header (or an ETag from
`metadata.sha256_hash()`) so a republished viewer is picked up immediately.

Cookies use a plain unsigned `CookieJar` (no signing or encryption features — the session value is
the access token itself, and the `csrf_token` cookie is a double-submit value compared at the
server, neither needs integrity protection against its own owner). Attributes via the cookie
builder: `HttpOnly` on the session cookie only, `SameSite::Lax`, `Path=/`, `Max-Age` 90 days via
the cookie crate's own `time` duration (`cookie::time::Duration::days(90)` — the `std::time`
type does not compile there), no `Secure` flag on plain-HTTP loopback.

Middleware is a `middleware::from_fn`/`from_fn_with_state` chain — Host allowlist, then Origin
check on unsafe methods, then token authentication, then CSRF double-submit, then
`Referrer-Policy` injection — applied with `Router::layer` after all routes and the fallback are
registered, so every route including static assets passes through it (axum's documented ordering).
The token-authentication layer exempts GETs whose path is outside `/api/` (the entry page and the
viewer bundle it references); every `/api/v1` route stays token-gated.

Reconciliation (implementation step 4): `referrer_policy` is applied as the last (outermost)
layer instead of the innermost, because responses short-circuited by the security middleware
(the 401/403 envelopes from Host/Origin/auth/CSRF) otherwise bypass it and would leave the
browser without a Referrer-Policy on exactly the requests a cross-site attacker cares about.
Execution order is therefore Referrer-Policy, Host, Origin, authenticate, CSRF, handler. The
Verify clause ("Referrer-Policy header") is exercised on success, authenticated, and
short-circuited responses.

Blocking `rusqlite` calls stay behind store interfaces and run on the Tokio blocking pool
(`tokio::task::spawn_blocking` around store operations), matching the daemon's rule that blocking
SQLite work never runs on async worker threads. The server opens one store connection per request
through the store's existing open path (WAL mode, five-second busy timeout, standard pragmas — no
connection-pool crate): SQLite WAL supports any number of readers alongside the daemon's writer,
and connection-per-request keeps the short-lived CLI-like access model. No workspace crate exposes
Tokio types as domain values.

Server-side URL import reuses the CLI's `fetch_import_url` bounds verbatim: reqwest with rustls,
`redirect(Policy::limited(10))`, a 30-second timeout, a streaming 16 MiB cap enforced on the
accumulated stream (no Content-Length pre-check — it can lie), and userinfo rejection. The
workspace reqwest features already cover this; no new dependency.

`locron-server` exposes one composition entry (roughly `serve(store, paths, config)`) that builds
the router and runs it; the CLI owns startup output and exit codes.

No automated dependency-direction check existed when the plan was written; the step-2 "enforcement
check" update materialized as `scripts/check-dependency-direction.sh` (a `cargo tree`-based check
that `locron-server` depends only on `locron-core`/`locron-store` among workspace crates and that
only `locron-cli` depends on it), wired into the CI test matrix as the "Dependency direction"
step. The latest-stable clippy additionally introduced the `map(<f>).unwrap_or(false)` lint on
pre-existing locron-cli code; the three occurrences were rewritten as the equivalent
`is_ok_and(...)` (behavior unchanged) to keep `clippy --workspace --all-targets -- -D warnings`
green on both toolchains.

### Accepted: dashboard service registration

The persistent path reuses the existing service-manager port in `locron-cli` (launchd and
systemd-user backends plus the deterministic fake, built for `locron service`), adding a second
registration target for the dashboard:

- macOS: LaunchAgent label `dev.locron.dashboard`, `ProgramArguments`
  `[<current_exe>, "dashboard", "serve"]`, `KeepAlive` true, `RunAtLoad` true,
  `StandardOutPath`/`StandardErrorPath` at `~/Library/Logs/locron/dashboard.log` (the daemon's
  log convention, one file per service).
- Linux: systemd user unit `locron-dashboard.service` at `~/.config/systemd/user/` with
  `ExecStart=<current_exe> dashboard serve`, `Restart=on-failure`, `WantedBy=default.target`.

`locron dashboard serve` is the non-interactive server entry the service executes; the bare
`locron dashboard` behaves identically. The enable/disable flows reuse the daemon registration's
verified behavior: enable-and-bootstrap ordering, refresh-and-restart of an already-loaded job,
lock-unrelated deferral does not apply (the dashboard holds no daemon lock; a port conflict is
handled as below), brew-marker refusal for registration operations (foreground serving stays
allowed), and uninstall's signal-then-bootout ordering. The two registrations are independent:
`locron service` never touches the dashboard unit and vice versa.

`locron dashboard enable` = ensure token, then register/refresh/start the service; `disable` =
unregister, then remove the token; `status` = service state plus the access URL and token facts;
`enable --reset` = regenerate the token, then refresh-and-restart the service (the server reads the
token at startup, so the restart picks up the new token and invalidates the old one).

self-update, after a successful atomic replace, refreshes the dashboard registration when one
exists, using the same pre-replace canonical-path capture that the daemon registration uses (the
Linux `/proc/self/exe` deleted-inode lesson applies identically). install.sh never registers the
dashboard.

Implementation notes (step 8):

- The port generalized as `Target { Daemon, Dashboard }` in `crates/locron-cli/src/service.rs`,
  carrying the per-target label/unit/plist name, log file, launch arguments, description, and
  whether the daemon-lock deferral applies (only the daemon defers — the dashboard never probes
  the daemon lock, verified by the fake-port call log). `ServiceContext` gained the `target`
  field; both launchd and systemd backends derive everything from it.
- `dashboard enable` = refuse managed installs (brew marker), then token regenerate/ensure, then
  `service install` semantics with the verified ordering. `enable --reset` regenerates the token
  and refresh-restarts a loaded service. `dashboard disable` = uninstall, then token removal, and
  — when something still listens on the service-mode port — a warning naming the foreground
  instance the operator must stop (the port is fixed in service mode, so a foreground server and
  a service cannot coexist; the foreground one must exit before the service can bind). `dashboard
  status` reports the service state, the access URL, and token facts (`present`/`permissions`,
  mode bits only — never the token value).
- The minimal `locron dashboard enable|disable|status` CLI surface landed in step 8 (dispatched
  before state discovery, so no state directory is required to report status) because the
  real-backend and self-update tests must be able to invoke the flows; the remaining arguments
  (serve alias, `--bind`, `--port`, `token`, doctor facts) stay in step 9 per the change order.
- The self-update refresh probes registration with `dashboard status --json` on the pre-replace
  canonicalized executable and refreshes with `dashboard enable` only when `data.registered` is
  true; every failure becomes a warning, so a broken dashboard registration never fails the
  update itself.

### Accepted: bind, port, and Host validation

- Bind loopback only. `--bind` accepts only loopback values (`127.0.0.1`, `::1`, or the default
  `127.0.0.1,::1`); any other value is refused at startup with the stable usage error — there is
  no code path that binds a non-loopback interface.
- Default port **10824**, verified unassigned in the IANA service-names registry on 2026-08-24;
  the earlier candidate 45123 was rejected because one documented hostile endpoint used it
  (evidence in `docs/FINDINGS.md` §14). Foreground mode: occupied default port falls back to the
  next free port (up to ten successive ports, then an OS-assigned port) and the chosen URL is
  printed. Service mode: the port is fixed — an occupied port makes the server exit and `status`
  reports it — so the bookmarked address never silently moves. An explicit `--port N` is always
  strict and fails with an actionable error when occupied (the Vite `strictPort` convention).
- Host-header validation compares the hostname only and ignores the port, because the port can be
  the fallback value. Accepted hosts are `localhost`, `127.0.0.1`, and `[::1]` (case-insensitive,
  canonical IPv6 bracket form); anything else is 403 before routing.
- If only one loopback family can be bound, the server warns and continues on the other.

### Accepted: access token and session

- Token material: 32 random bytes from the OS RNG, hex-encoded (64 characters). Stored owner-only
  (0600) at a fixed name under the state directory; generated on first use (foreground or
  `enable`), reused afterwards, regenerated by `enable --reset`, and removed by `disable`. The
  server reads the token at startup; a running server is unaffected by a later file change, which
  is why `enable --reset` restarts the service.
- Transport: `Authorization: token <t>` (the Jupyter scheme) for scripts and automation, and a
  one-time paste at the entry page for browsers. **The token never appears in any URL.** The
  authenticated entry sets a session cookie (value is the token, `HttpOnly`, `SameSite=Lax`,
  `Path=/`, 90-day lifetime — product decision 2026-08-24 — no `Secure` flag on plain-HTTP
  loopback).
- Every `/api/v1` route requires a valid token; without one the server serves only the entry page
  and its static assets and returns 401 from API routes. The viewer bundle is public exactly
  because it must load before any token exists: the paste form itself is served by `app.js`, so
  the assets cannot sit behind the gate they are needed to unlock. The bundle carries no data —
  redaction is server-side — and the server is loopback-bound, so public static assets add no
  exposure. Recorded at implementation (step 7): the step-4 contract test asserting `GET /app.js`
  → 401 was a planning error — it deadlocks the entry page — and the fix (token authentication
  exempts GETs outside `/api/`) is the behavior this plan records. The token is never logged,
  never echoed in responses, and never appears in diagnostics (presence and permission facts
  only).
- All responses carry `Referrer-Policy: no-referrer`.

### Accepted: CSRF and Origin protection

- Host allowlist (above) is the DNS-rebinding defense; it applies to every request, including
  requests without an Origin header.
- On unsafe methods (POST, PUT, PATCH, DELETE), a present Origin header must equal the server
  origin (`http://<host>:<port>` of the bound loopback address); mismatch is 403. An absent Origin
  is allowed (same-origin navigations, curl, EventSource).
- Double-submit CSRF: the first authenticated visit also sets a `csrf_token` cookie (random 32-byte
  hex, not `HttpOnly`, `SameSite=Lax`). A cookie-authenticated unsafe request must echo that value
  in an `X-CSRF-Token` header or form field; mismatch is 403. Requests authenticated solely by the
  bearer token in the Authorization header skip the CSRF check, because a cross-site page cannot
  attach that header — the Jenkins API-token crumb exemption, cited in `docs/FINDINGS.md` §14. The
  entry-page token paste is likewise safe without a CSRF token: a cross-site attacker cannot know
  the token, and with one account login-CSRF is meaningless.

### Accepted: API surface

- Base path `/api/v1`, one route family per durable application command family (`docs/FINDINGS.md`
  §16 route inventory, accepted 2026-08-24):

  | CLI command | Route |
  |---|---|
  | `add` | `POST /api/v1/jobs` |
  | `update` | `PUT /api/v1/jobs/{name\|uuid}` (immutable-revision semantics) |
  | `list` | `GET /api/v1/jobs` |
  | `show` | `GET /api/v1/jobs/{name\|uuid}` |
  | `enable`/`disable` | `POST /api/v1/jobs/{id}/enable`, `/disable` |
  | `remove` | `DELETE /api/v1/jobs/{id}` |
  | `preview` | `POST /api/v1/schedule/preview`, `GET /api/v1/jobs/{id}/preview?count=` |
  | `run` | `POST /api/v1/jobs/{id}/run?wait&dry-run` |
  | `cancel` | `POST /api/v1/runs/{id}/cancel` (`acknowledge_unconfirmed` flag) |
  | `history` | `GET /api/v1/runs?job=&limit=&offset=` |
  | `logs` | `GET /api/v1/runs/{id}/logs?attempt=&channel=` |
  | `why` | `GET /api/v1/jobs/{id}/why`, `GET /api/v1/runs/{id}/why` |
  | `config get\|set\|unset` | `GET /api/v1/settings`, `PUT /api/v1/settings/{key}`, `DELETE /api/v1/settings/{key}` (full CLI config surface, `environment.NAME` grammar and redaction preserved) |
  | `export` | `GET /api/v1/export?jobs=&tag=&include-values&acknowledge-plaintext` (`Content-Disposition: attachment`) |
  | `import` | `POST /api/v1/import?accept-plaintext-values&dry-run` — body is the export document, or a JSON object `{"url": "https://…"}` requesting server-side fetch |
  | `prune` | `POST /api/v1/prune?dry-run` |
  | `doctor` | `GET /api/v1/diagnostics` |

  Not mirrored: `daemon run` (process supervision), `service` (registration; the dashboard family
  has its own), `self-update` (binary replacement) — their facts surface through diagnostics, per
  `SPEC.md` §9. Request bodies mirror the CLI's machine-readable field semantics; `dry_run` is
  supported wherever the CLI supports it (in query strings the flag parameters are kebab-case —
  `?dry-run`, `?include-values`, `?acknowledge-plaintext`, `?accept-plaintext-values` — matching
  the route table; where `dry_run` rides in a request body it accepts the same string-flag forms
  as query parameters: `"true"`/`"1"`/`""` for true, `"false"`/`"0"` for false, absent for
  false). URL import uses the same server-side fetch bounds as the
  CLI's URL import: mandatory TLS verification, 16 MiB streaming cap, 10-redirect cap, 30-second
  timeout, and userinfo rejection. Recorded at implementation (step 7): the dry-run create path
  in `jobs_create` discriminated on `store.is_none()` — "the state database file does not exist"
  — instead of the request's `dry_run` flag, so on any server with an existing state database a
  dry-run `POST /api/v1/jobs` fell into the live-create branch and either failed against the
  read-only dry-run store ("attempt to write a readonly database") or would have durably created
  the job. The fix (this plan records it) is to branch on `body.dry_run` explicitly — the same
  discriminator the update and manual-run dry-run paths already use — and the contract test
  suite gained a dry-run-with-existing-database case. The manual browser checklist exposed it:
  the form's dry-run button returned a `state_error` on the walk fixture server, which always
  has a state database.
- Envelope: the versioned `locron.api/v1` envelope — success
  `{"schema": "locron.api/v1", "ok": true, "data": ..., "warnings": [...]}`, error
  `{"ok": false, "error": {"code", "message"}}` — where `code` carries the stable CLI error codes
  verbatim (Slack `ok`/`error`/`warning` and Telegram `ok`/`result` precedents, `docs/FINDINGS.md`
  §16). HTTP status mapping (research mapping, product decision 2026-08-24): unauthenticated 401,
  refused/permission 403, validation 400, not-found 404, conflict/busy 409,
  daemon-required/state-unavailable 503, internal 500. The exact table is part of the CLI.md
  contract update.
- Export selectors mirror the CLI's export selection (`--jobs`/`--tag` union with dedup and
  no-match validation); the viewer offers the selection as checkboxes, never a TTY picker.
- Redaction goes through the shared core boundary; parity tests compare API payloads with CLI JSON
  output for the same fixtures.

### Accepted: live output stream

- `GET /api/v1/runs/{id}/stream` uses `axum::response::sse` (`text/event-stream`). The endpoint
  authenticates through the session cookie only — EventSource cannot set an Authorization header,
  and the token never appears in a URL.
- Events are typed named JSON events (OpenAI-style naming, `docs/FINDINGS.md` §16): `output`
  events carry `{channel, seq, elapsed_us, data_b64}` (base64 so arbitrary bytes survive; the
  viewer renders text channels and marks binary), `attempt` and `run` events carry the respective
  state transitions, and a final `termination` event carries the terminal outcome (idempotent on
  EventSource reconnect). The stream reads the same framed output the CLI `--follow` uses and
  respects the same retention bounds; following never cancels the run. A `KeepAlive` ping guards
  stale connections.
- Implementation notes (deviation from the CLI where the CLI's behavior is a terminal error):
  the stream polls the store every 200 ms exactly like `logs --follow` (final artifact first,
  then the in-progress partial; an incomplete tail ends the read at the last complete frame).
  Recorded at implementation (step 7): the manual browser checklist exposed that live output
  never reached a follow client for quiet processes — the runner's `OutputWriter` buffers frames
  in a `tokio::io::BufWriter` and nothing flushed the partial file while a process ran, so
  `1.partial` stayed empty (0 bytes) until finalization and the 200 ms pollers (CLI `logs
  --follow`, this stream) read nothing until the run ended. The fix (recorded here) is a 200 ms
  flush interval in the runner's process and HTTP capture loops that flushes the partial file
  without renaming, so follow clients see frames within one poll of their production; the
  buffered-writer design is otherwise unchanged and finalization still renames the partial.
  Frame-count regression replays the file from frame zero instead of aborting with the CLI's
  "attempt output regressed" error — events carry `seq`, so viewers dedupe; and a run that
  disappears mid-stream (durable retention prune) ends the stream rather than polling forever.
  The first poll emits the current `run` state as the connect catch-up; the server closes the
  connection immediately after the single `termination` event.
- Job-list freshness is ordinary polling from the SPA (the healthchecks model); no push channel is
  added for lists.

### Accepted: viewer

- A hand-written single-page viewer (plain HTML/CSS/JS, no framework, no build step, no CDN, no
  external fonts) embedded in the binary via `rust-embed`. No Node toolchain enters CI.
- **Information architecture** (healthchecks two-level drill-down, `docs/FINDINGS.md` §16): one
  chrome shell with persistent top navigation (Jobs, Run history, Diagnostics) and hash-routed
  views — `#/jobs` (landing job list), `#/jobs/:id` (detail: definition, policies, why, recent
  runs), `#/jobs/new` and `#/jobs/:id/edit` (form with schedule preview and dry-run),
  `#/runs` (run history), `#/runs/:id` (attempts, events, log viewer), `#/diagnostics` (scheduler
  health, paths, daemon availability, exposure facts, and the settings editor covering the full
  `config` command surface with CLI redaction). The header carries a daemon-availability indicator
  fed by `/api/v1/diagnostics`.
- **Job list row:** status chip (enabled plus last-outcome color, spinner while a run is active),
  name with tags, schedule summary (humanized form with the raw expression as secondary text),
  next occurrence (relative text with the absolute RFC 3339 instant in a `data-*` attribute —
  dual rendering is the universal convention; machine values stay CLI-equivalent, so `SPEC.md`
  criterion 3 needs no amendment), last outcome with duration, and row actions (run now, show,
  enable/disable, remove). Search box and enabled/disabled filter above the table; no pagination.
- **Run visualization:** one row per run — trigger, nominal time, outcome chip, timing, attempt
  count — with a horizontal strip of status-colored segments, one per attempt, width proportional
  to duration, linking into the log viewer (Jenkins Stage View cells, GHA step dots).
  Skip/supersession/acknowledgement events render as annotation rows with distinct badges
  (healthchecks event-kind badge set). Pagination is `limit`/`offset` with a `total` count.
- **Log viewer:** monospace pane with line numbers and click-to-copy permalinks (GHA), per-line
  timestamps on by default with a toggle (a scheduler's primary axis is time), tail-first open with
  a "load older" paging control (Jenkins `consoleTailKB` precedent), client-side search, follow
  mode as pinned auto-scroll over the SSE stream with an explicit "stream ended" notice, and
  inline truncation/discard markers at the capture point (locron owns the marker data; stronger
  than healthchecks' silent capture). ANSI bytes are preserved in the API and rendered by a small
  hand-written ANSI parser bundled in the viewer.
- **Empty/edge states:** onboarding card with the create action on an empty `#/jobs` ("No jobs
  yet" + Create job), a distinct "no matching jobs found" filtered-empty state, a persistent
  daemon-offline banner in the header fed by diagnostics, and redacted values rendered as the
  CLI's literal `<redacted>`/`value redacted` markers — never a value or a synthesized sentinel.
- **SPA structure** (no build toolchain): `index.html` shell plus hand-written `router.js` (hash
  parsing and `hashchange` dispatch), `api.js` (fetch wrapper that adds `X-CSRF-Token` on
  cookie-authenticated mutations and maps error codes), `views/` (one render function per route),
  `components.js` (status chips, dual-rendered times, attempt segments, tables), and `sse.js`
  (EventSource wrapper with reconnecting state). Hash routing survives refresh and bookmarking
  with zero server fallbacks (MDN `hashchange`).
- The SPA authenticates via the session cookie, echoes `X-CSRF-Token` from the `csrf_token` cookie
  on every mutation, and uses `EventSource` (cookie-authenticated) for live logs.

### Accepted: CLI composition and diagnostics

- Command family: `locron dashboard` (foreground; identical to `dashboard serve`),
  `enable [--reset]`, `disable`, `status`, `token`. Startup prints the access URL once — human
  form and the machine-readable envelope per `docs/CLI.md` — then serves until signal. Exit codes
  reuse the stable categories for state-dir, bind, and port failures; `disable` warns when a
  foreground instance may still be running.
- `locron doctor` gains the exposure facts: token file presence and permission posture, and whether
  a dashboard service is registered. It does not report a server as running; `dashboard status`
  does.
- The help surface gains the new command with per-argument help, covered by the existing
  help-surface acceptance walk.

## Change order

1. Amend planning documents: `docs/ARCHITECTURE.md` first (fifth workspace member with its
   dependency row and arrows, the core redaction-boundary responsibility, and the
   server-never-owns-daemon boundary note), then update `docs/CLI.md` with the `locron dashboard`
   contract and the error-mapping table, and add this checklist to `docs/TODO.md` (`SPEC.md`
   and `docs/FINDINGS.md` §14 are already frozen/recorded).
2. Add the `locron-server` member to the workspace with the accepted dependencies; update the
   dependency-direction enforcement check; confirm one `locron` binary and no new binary.
3. Move the redaction boundary from `locron-cli` to `locron-core` with no output change; CLI
   contract tests must pass unchanged.
4. Implement the middleware stack and token file: loopback bind/refusal, Host allowlist, Origin
   check, token acceptance (`Authorization` header plus entry-page paste), session and CSRF
   cookies, `Referrer-Policy`, and the entry page.
5. Implement the `/api/v1` route families over the durable application commands with the
   `locron.api/v1` envelope, the error mapping, and blocking-pool store access.
6. Implement the SSE run stream over the existing framed-output reader.
7. Implement and embed the viewer SPA.
8. Implement the dashboard service registration on the existing service-manager port (second
   registration target, enable/disable/status/`--reset` flows, brew-marker refusal for
   registration, dashboard log paths) and the self-update refresh of a registered dashboard.
9. Wire the `locron dashboard` command family and the doctor exposure facts; extend the
   help-surface acceptance test.
10. Documentation final pass: `docs/CLI.md`, `docs/OPERATOR.md` (viewer operation, token lifecycle,
    the `loginctl enable-linger` note shared with the daemon, what loopback does and does not
    protect), README documentation list entry for `docs/dashboard/SPEC.md`.
11. Full verification and evidence recording in `docs/TODO.md`, including the four-target CI
    matrix.

## Edge cases to handle explicitly

- Default port occupied: foreground falls back and prints the chosen port; service mode exits and
  `status` reports the conflict; explicit `--port` fails with an actionable error (no silent
  fallback on explicit intent).
- Token file missing at startup (fresh state dir, deleted file, or `disable`): regenerate and print
  it (foreground) or generate silently at service start (service mode; the owning user reads it
  via `dashboard token`).
- Attacker domain resolving to loopback (DNS rebinding): Host allowlist 403s before routing.
- Another loopback origin attempting a cookie-authenticated POST: Origin mismatch or missing
  `X-CSRF-Token` stops it; bearer-token requests need no CSRF token.
- Browser navigation after the session cookie expires or is cleared: entry page with a one-time
  paste (`dashboard token` re-displays the value).
- The daemon is offline: reads work, mutations commit durably, enqueue succeeds, doctor explains.
- `enable --reset` invalidates outstanding session cookies and the old token; the service restart
  applies it.
- `enable` while the service is already registered: refresh-and-restart idempotently (the
  `service install` semantics).
- `disable` while a foreground instance is running: unregister and remove the token, warn that the
  foreground process must be stopped by the user.
- self-update with a registered dashboard: refresh the registration using the pre-replace
  canonical path capture; a failed refresh is a warning, never an update failure.
- brew-managed binaries: registration operations refuse with brew guidance; foreground serving
  stays allowed.
- IPv6-only environments: one loopback family unboundable is a warning, not a failure.
- A quarantine-termination-unconfirmed run: the cancel route carries the same acknowledgement
  requirement and stable conflicts as the CLI.
- Export/import through the API: same document validation, acknowledgement, and rollback rules; a
  URL import uses the same TLS/16 MiB/10-redirect/30-second caps as the CLI and rejects userinfo
  URLs.

## Verification strategy

- **Middleware unit tests:** Host allowlist (case, port, `[::1]` forms, attacker domain, absent
  port), Origin present/mismatch/absent on unsafe methods, double-submit CSRF match/mismatch and
  bearer exemption, token accept/reject paths, entry-page paste flow, `Referrer-Policy` header on
  all responses, entry page and static assets only without a token (and 401 from every `/api/v1`
  route), and no token in any served URL.
- **API contract tests:** a real server on an ephemeral loopback port over a temporary state
  directory — token refusal, job CRUD round trips mutating real SQLite, offline manual enqueue,
  export/import round trip with acknowledgement (including the `?jobs=`/`?tag=` selectors and a
  local-fixture URL import with the TLS/size/redirect/timeout caps), dry-run non-mutation, the
  error-category mapping matrix per the accepted table, and envelope schema strings.
- **Redaction parity tests:** API job/run/settings payloads equal the CLI JSON output for the same
  fixtures; no configured secret material appears.
- **SSE tests:** subscribe, inject frames through the store fixture, receive ordered
  `output`/`attempt`/`run`/`termination` events, an idempotent terminal event at finalization, no
  cancellation on disconnect, cookie authentication only.
- **Service registration tests:** fake-port tests cover the dashboard target (templates with
  canonicalized path, label/unit names, log paths), enable idempotency and refresh-and-restart,
  `--reset` ordering, disable ordering, brew-marker refusal, and status fields; real-backend tests
  on the macOS leg register, restart, and unregister the dashboard LaunchAgent in the available
  domain, and the Linux leg drives the dashboard unit under `dbus-run-session`; a self-update
  contract test proves the post-replace dashboard refresh happens exactly once and only after a
  successful replace.
- **CLI tests:** `locron dashboard` startup URL and token output, `--bind` refusal, explicit-port
  strictness, foreground fallback, doctor exposure facts, and the help-surface acceptance walk
  covering every new argument.
- **Workspace verification:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets
  -- -D warnings`, `cargo test --workspace` on Rust 1.94 and latest stable; dependency-direction
  inspection finds no forbidden edge; only the `locron` binary exists.
- **Platform matrix:** the existing four-target CI runs the new suites.
- **Manual browser checklist (recorded as evidence):** enable the service, open the access URL,
  paste the token, verify the cookie handoff, render list/detail/history, create a job with
  dry-run preview, follow a live run, cancel it, and confirm redacted values never appear in the
  DOM or JSON.
- **Port evidence:** the 10824 IANA-unassigned verification and the 45123 rejection are already
  recorded in `docs/FINDINGS.md` §14; no further check is needed.
