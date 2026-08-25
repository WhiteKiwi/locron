# locron Dashboard Implementation Plan

## Status and authority

This document plans the shipped dashboard surface against the frozen `SPEC.md` (this directory).
Its post-milestone delivery does not retroactively amend the exclusions in the root
`docs/SPEC.md`. Research evidence and rejected
alternatives are recorded in `docs/FINDINGS.md` §14 (security, transport, framework — including the
2026-08-24 default-port verification) and §16 (UI and API design); this document records the
accepted implementation choices and their trade-offs, including the 2026-08-24 product decisions.
The 2026-08-24 brand-system amendment is supported by `docs/FINDINGS.md` §22.

Durable-structure changes (workspace membership, the redaction boundary) update
`docs/ARCHITECTURE.md` before code, per the repository workflow.

## Architecture and approach

The surface is a new workspace library crate, `locron-server`, composed by `locron` through the
`locron dashboard` command family:

- `locron-server` depends on `locron-core` and `locron-store` only. It never depends on
  `locron` and never talks to SQLite tables directly — every read and mutation goes through the
  durable application commands, exactly like the CLI and MCP surfaces.
- `locron` remains the composition root: it owns argument parsing, human/machine rendering,
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
`redacted_run`, `redacted_settings_value`) currently live in `crates/locron-cli/src/main.rs`. A server
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
only `locron` depends on it), wired into the CI test matrix as the "Dependency direction"
step. The latest-stable clippy additionally introduced the `map(<f>).unwrap_or(false)` lint on
pre-existing `locron` package code; the three occurrences were rewritten as the equivalent
`is_ok_and(...)` (behavior unchanged) to keep `clippy --workspace --all-targets -- -D warnings`
green on both toolchains.

### Accepted: dashboard service registration

The persistent path reuses the existing service-manager port in `locron` (launchd and
systemd-user backends plus the deterministic fake, built for `locron service`), adding a second
registration target for the dashboard:

- macOS: LaunchAgent label `dev.locron.dashboard`, `ProgramArguments`
  `[<current_exe>, "--state-dir", <selected_state_dir>, "dashboard", "serve",
  "--service-mode"]`, `KeepAlive` true, `RunAtLoad` true,
  `StandardOutPath`/`StandardErrorPath` at `~/Library/Logs/locron/dashboard.log` (the daemon's
  log convention, one file per service).
- Linux: systemd user unit `locron-dashboard.service` at `~/.config/systemd/user/` with
  `ExecStart=<current_exe> --state-dir <selected_state_dir> dashboard serve --service-mode`,
  `Restart=on-failure`, `WantedBy=default.target`.

`locron dashboard serve` is a user-facing foreground entry identical to bare `locron dashboard`.
Only service templates carry the hidden `--service-mode` marker that selects fixed-port behavior;
redirected stdin does not change a user invocation's semantics. The enable/disable flows reuse the daemon registration's
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

Implementation notes (step 9):

- The foreground/service mode distinction is explicit: bare `locron dashboard` and a user-invoked
  `locron dashboard serve` use `Foreground` fallback regardless of stdin, while the hidden marker
  written only into launchd/systemd registration selects `Fixed` (an occupied port makes the
  server exit with an actionable error, which `dashboard status` reports through `loaded: false`
  plus guidance). An explicit `--port N` is always `Fixed`. Unit and integration tests cover the
  hidden service marker, a redirected bare invocation, and ordinary terminal foreground use.
- `--port N` and `--bind ADDR` are declared on `locron dashboard` (the bare
  form, matching the documented `locron dashboard [--port N] [--bind ADDR]`
  spelling), not on the `serve` subcommand; combined with a service-management
  subcommand (`enable`/`disable`/`status`) they are refused with a usage error.
  `--bind` accepts only the loopback literals `127.0.0.1` and `::1` (comma
  separated); anything else — including `localhost` and `0.0.0.0` — is refused
  with a stable usage error (exit 2, `invalid_request`). Bind/port runtime
  failures map to `service_io` (exit 5).

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

### Accepted: Locron brand system and dashboard refresh

The durable visual contract is a repository-root `DESIGN.md`, following the useful part of the
agent-readable design-guide pattern without claiming compatibility with an unverified Vercel file.
It records the brand promise, attributes, voice and tone, Locron wordmark and Roki usage, semantic
palette, typography, illustration and icon rules, layout tokens, component states, motion,
accessibility, responsive behavior, and concrete do/don't examples. README links it alongside the
dashboard specification. The guide lets future surfaces make original layouts while preserving the
same identity; it does not encode the current dashboard DOM as the brand.

The visual thesis is **calm local control that explains itself**. The dashboard uses a warm cream
canvas, quiet light work surfaces, charcoal hierarchy, graphite secondary text, and sunny yellow
only as the recognitional accent and focus signature. Operational states retain separate accessible
colors plus text or icon labels; yellow never doubles as warning. Rounded forms, crisp borders,
minimal layered elevation, and a small hand-drawn spark carry the README banner into the product.
Roki remains a sparse high-empathy character for entry or truly empty states rather than recurring
table decoration. The log console stays a deliberate dark technical counter-surface.

The viewer remains bundled plain HTML/CSS/JavaScript with system font stacks and no network, CDN,
font, Node build, 3D, GSAP, or Lottie dependency. A single CSS-token layer maps the documented
color, spacing, type, radius, border, elevation, and motion roles to every existing view. Light is
the primary application appearance rather than an automatic unrelated dark theme. Simple CSS
opacity/transform transitions provide short, snappy-gentle feedback and are disabled or flattened
under `prefers-reduced-motion`.

The shell becomes a deliberate product frame: a compact Locron identity and local-scheduler
context, clear Jobs / Run history / Diagnostics navigation, a persistent daemon state, generous
outer breathing room, and dense internal data rhythm. Page heads explain the view and keep one
filled primary action per decision area. Tables, facts, forms, notices, chips, empty states, run
segments, and the console share focus, hover, active, loading, disabled, error, and wrapping rules.
At narrow widths navigation and actions wrap, tables remain reachable without clipping content,
inputs stay touch-readable, and primary operations remain available. A skip link, meaningful
landmarks, visible `:focus-visible`, current-page semantics, non-color state labels, and reduced
motion are part of the implementation rather than a later audit.

The entry page is the most expressive surface: it pairs the wordmark and privacy/local-first
promise with the token form, but keeps the token flow direct and security copy factual.
Authenticated views are quieter. Error, destructive, cancelled, quarantined, and interrupted
states never use mascot jokes or celebratory motion.

The refresh also closes behavior gaps found while reviewing the integrated branch:

- Session bootstrap trusts a successful authenticated session-status response, not JavaScript
  visibility of the `HttpOnly` session cookie. A reload with a valid session stays in the app; a
  401 returns to the token entry.
- Inline HTTP job bodies are serialized with the API's byte-array semantics, and the form offers
  only methods the domain accepts. Create, edit, and dry-run share the same conversion.
- Dashboard service templates preserve the selected state directory and carry an explicit hidden
  service-mode argument. Bare `dashboard` and user-invoked `dashboard serve` are foreground even
  with redirected stdin; registered service mode alone uses the fixed-port policy.
- A bound server exposes the actual successful loopback address, so startup output uses
  `127.0.0.1` when IPv4 is bound and `[::1]` when only IPv6 is bound instead of fabricating an IPv4
  URL.
- SSE attempt events use the viewer's documented `attempt_number` field. Output events also carry
  the attempt number; the viewer deduplicates replayed output by attempt and sequence while
  preserving genuinely new frames after EventSource reconnects.
- Self-update treats a malformed successful dashboard-status envelope as a warning. It refreshes
  only when `data.registered` is explicitly boolean `true`; an explicit `false` remains the sole
  no-op result.

### Accepted: CLI composition and diagnostics

- Command family: `locron dashboard` (foreground; identical to `dashboard serve`),
  `enable [--reset]`, `disable`, `status`, `token`. Startup prints the access URL once — human
  form and the machine-readable envelope per `docs/CLI.md` — then serves until signal. Exit codes
  reuse the stable categories for state-dir, bind, and port failures; `disable` warns when a
  foreground instance may still be running.
- `locron doctor` gains the exposure facts: token file presence and permission posture, and whether
  a dashboard service is registered. It does not report a server as running; `dashboard status`
  does. The registration fact comes from a read-only probe through the same service-manager port
  (the `LOCRON_SERVICE_BACKEND=fake` test hook applies, so the fact is deterministic in tests).
- The help surface gains the new command with per-argument help, covered by the existing
  help-surface acceptance walk.

Implementation notes (step 9):

- `serve` is a subcommand in the same position as `enable`/`disable`/`status`/`token`; omitting the
  subcommand (bare `locron dashboard`) is the default-serve form, implemented as an optional
  subcommand. The serve path binds first (so the chosen port is known), prints the access URL
  (human line `Dashboard URL: http://<actual-bound-loopback>:<port>/` plus, on a fresh state directory, the
  newly generated token; machine envelope with `access_url` and token facts — never the value),
  then runs the server until signal.
- `dashboard token` is the explicit value surface: it ensures the token (generating it when the
  file is missing, matching "generated on first use") and prints the 64-hex value in both human
  and machine forms — the only output that ever contains the token, besides the first-run
  foreground line.

## Change order

1. Amend planning documents: `docs/ARCHITECTURE.md` first (fifth workspace member with its
   dependency row and arrows, the core redaction-boundary responsibility, and the
   server-never-owns-daemon boundary note), then update `docs/CLI.md` with the `locron dashboard`
   contract and the error-mapping table, and add this checklist to `docs/TODO.md` (`SPEC.md`
   and `docs/FINDINGS.md` §14 are already frozen/recorded).
2. Add the `locron-server` member to the workspace with the accepted dependencies; update the
   dependency-direction enforcement check; confirm one `locron` binary and no new binary.
3. Move the redaction boundary from `locron` to `locron-core` with no output change; CLI
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

## Brand refresh and behavior-correction change order (2026-08-24)

1. Add the durable `DESIGN.md` guide and README link, using the verified research and existing
   banner as the source of truth.
2. Apply the visual system to the shared dashboard shell and every existing viewer component,
   including entry, empty, error, responsive, keyboard, and reduced-motion states; add no runtime
   dependency.
3. Correct session reload, HTTP body/method, SSE schema/deduplication, actual bound URL, service
   state-directory/mode, and self-update envelope behavior with focused regression coverage.
4. Run formatting, lint, dependency direction, workspace tests, JavaScript syntax checks, and the
   existing real fixture walk.
5. Inspect the rendered entry and authenticated Jobs / Run history / Diagnostics surfaces at
   desktop and narrow widths in a real browser, fix visible or interactive regressions, then leave
   the fixture development server running for product review.

## Accepted: peer themes, Settings IA, exact inputs, and complete run search (2026-08-25)

The theme bootstrap is an inline head script before CSS. It accepts only `system`, `light`, or
`dark`, catches storage failures, resolves system through `matchMedia`, and sets `data-theme` plus
`colorScheme` before paint. Both schemes use the same semantic token names documented in
`DESIGN.md`. Geist Sans/Mono variable WOFF2 files and the upstream OFL are embedded locally;
Sans alone is preloaded and both use `font-display: optional` with Korean/system fallbacks.

The shell may use glass only where it improves hierarchy, always with an opaque baseline and an
`@supports` enhancement. Workbench content remains opaque. Reduced transparency forces the solid
baseline; reduced motion removes nonessential transitions.

Run search is a store-owned page operation. A single read transaction scans the durable
newest-first run/job join, normalizes query/run ID/current job name using Rust Unicode lowercase,
filters literally, computes the complete total, then offsets and bounds the page. API `q` and
exact `job` are mutually exclusive; exact-job semantics remain available. The client uses a
250 ms trailing debounce, `AbortController`, and generation/query checks.

Settings moves from Diagnostics to `#/settings`. Browser-local Appearance applies immediately and
is never sent in the durable settings save. Durable settings retain one-key API writes. Duration
controls share pure browser helpers that parse strings with BigInt and return only safe integer
microseconds; load chooses the largest exact compact unit and preserves exact round trips.

### Change order

1. Restore the two-theme design guide, local font provenance, bootstrap, semantic tokens, and
   deterministic asset coverage.
2. Add the store/API page-search operation and retention-age validation with focused tests.
3. Split Settings from Diagnostics and add the route/navigation/theme control.
4. Replace enumerated form choices with native fieldsets and raw duration fields with the exact
   shared control, then add abortable run search.
5. Restyle both themes and run focused formatting, clippy, Rust, JavaScript, hash, and diff checks.

## Accepted: typed frontend and operator-facing control system (2026-08-25)

The dashboard source moves to a Vite + React + strict TypeScript application under the server
crate. React and React DOM are the only initial runtime packages. Hash routing, native fetch,
EventSource, component state/reducers, and platform form controls are sufficient; a router, query
cache, global store, Axios, or generic design-system dependency would add more contract than this
single-user local application needs. Reusable boundaries include Shell, Field, ChoiceGroup,
DurationInput, ByteSizeInput, InstantInput, SecretKeyValueEditor, SearchInput, DataTable,
Pagination, InlineFeedback, ConfirmDialog, and schedule/policy sections.

Node and npm are development/CI dependencies only. Exact versions and the npm lock are committed.
The frontend gate installs with `npm ci`, runs strict `tsc --noEmit`, runs deterministic unit and
component tests, and produces a Vite `dist` without source maps or runtime external URLs. The
complete dist tree is committed and a clean build compares its file list and SHA-256 values. Rust
embeds only dist and never invokes npm from Cargo. Asset tests follow the built index references,
verify MIME and font provenance, reject remote assets, and keep only stable semantic markers rather
than asserting minified implementation strings. The server crate's existing internal workspace
dependencies are intentionally path-only and therefore make standalone `cargo package` manifest
verification inapplicable without a separate publication-policy change. `cargo package --list`
proves the dist inclusion and node_modules exclusion; a clean `cargo install --path` of the actual
CLI package, with no frontend command in the build graph, plus the embedded-handler tests proves
that Node is unnecessary for an end user. This keeps package publication policy outside the
dashboard change while testing the same Rust embed consumed by release builds.

The vanilla application remains the executable contract reference during migration. Pure behavior
ports first: envelope/session handling, signal refresh, theme resolution, exact duration/size
parsers, SSE attempt/sequence dedupe, and stale-safe search. Read-only routes follow, then JobForm
and Settings, and finally RunDetail/SSE. The Rust embed root switches only after route-by-route
browser parity. Legacy assets are removed only after the production tree passes that gate. React
effects own an AbortController or cleanup handle; development effect replay must not duplicate
fetches, submissions, or EventSource connections.

All editable controls use a shared Field contract with visible label, consequence-oriented
description, linked hint/error IDs, disabled/loading/invalid state, and first-invalid focus. Job
filters gain real labels and a settled result count. The long job form has Identity, Schedule,
Target, Environment, and Policy sections with stable navigation and one-column reading order.
Dependent inputs are conditionally revealed without losing entered state: missed All owns catch-up,
retries own delay/backoff/cap, overlap Allow owns per-job concurrency, and HTTP body source owns
Inline/File. Alert dialogs are replaced with in-context retryable feedback except for true blocking
browser failures.

`ByteSizeInput` parallels the exact duration implementation. It accepts one nonnegative decimal
magnitude plus B/KiB/MiB/GiB, including pasted suffixes, converts through strings and BigInt, emits
only exact safe integer bytes at the current JSON boundary, and displays the exact comma-grouped
byte equivalent. Zero consequences are explicit. Retention age gains a nullable `No age limit`
path through store/API; zero remains immediate expiry and requires warning copy. Settings with
pruning consequences use review-then-apply with human old/new values, not internal key names.

`InstantInput` uses local `datetime-local`, the schedule timezone selection, and an absolute preview.
It converts exactly at the API boundary and keeps epoch microseconds in a collapsed advanced detail.
Cron shows syntax help and the next occurrences; Every explains elapsed interval semantics. Tags,
success statuses, argument lines, environment names, and path lists each state their grammar and
reject invalid tokens rather than silently trimming or dropping them. Raw PATH editing uses
repeatable ordered rows in the normal workflow and shows the delimiter serialization only as an
advanced detail.

The visual composition uses a quiet, persistent shell and one dominant opaque workbench. It borrows
the supplied portfolio's strong typography and sparse amber focus, DeepSeek's generous split-space
discipline, and Grafana's dense operational hierarchy, while retaining Locron's cream, charcoal,
yellow, and Roki-derived geometry. Cards are used for real grouping, not every value; dividers and
spacing do most of the work. Glass and glow remain limited to shell/transient depth.

Browser chrome ships with a code-native SVG charcoal/amber spark favicon. The prepaint bootstrap
sets the resolved light/dark theme, CSS color scheme, and matching theme-color before stylesheet
paint; changes stay synchronized. Route effects set safe route-first titles (`Jobs · Locron`, `Run
history · Locron`, `Diagnostics · Locron`, `Settings · Locron`) and generic safe titles for forms
and detail views without placing mutable job/run data in browser history.

### Change order

1. Freeze frontend versions, build/dist contract, component boundaries, and the expanded field
   behavior tests while keeping the existing served application intact.
2. Port pure API/theme/duration/size/instant/search/SSE behavior and build the token, shell, Field,
   feedback, favicon, theme-color, and document-title foundations.
3. Port entry and read-only routes, then the sectioned JobForm and Settings with every inventory
   item resolved and backend nullable-retention/search support covered.
4. Run source/component and clean-dist reproducibility gates, switch Rust embedding to dist, remove
   legacy assets, and run changed-crate and workspace verification.
5. Compare both themes and all routes to the old behavior in a real browser at desktop, narrow,
   200% zoom, reduced preferences, keyboard-only, empty/error/loading, and realistic long-content
   states; fix regressions before publication and leave the fixture server running.

## Accepted: modern operator cockpit and accessible popup primitives (2026-08-25)

The amendment replaces the current horizontal glass header, ambient canvas glow, and repeated-card
composition with a flat operational shell. At 1024 px and wider, an opaque 224 px left rail owns
Locron identity, the four first-level destinations, daemon health, and utility access. From 768 to
1023 px it contracts to 64 px while preserving a visible active marker and accessible labels. Below
768 px, a 56 px top bar plus four labelled bottom destinations keeps every route one action away;
there is no hamburger-only IA. The route header owns the title, short task context, and one primary
action, while the route body provides one dominant border-and-divider workbench rather than a page
hero or an outer card around every section.

Jobs and Run history share an explicit responsive data pattern. Desktop renders a dense semantic
table with a wrapping filter/action toolbar, 36 px header, 44 px rows, aligned tabular values, text-
labelled state, and a stable final action-menu column. Below a 760 px container, the component
renders semantic object rows from the same data and actions instead of hiding columns or forcing
page-level horizontal scrolling. Row commands move to a named DropdownMenu trigger; destructive
items form the last separated group, and every command remains reachable by keyboard and touch.

Job create/edit uses a 720 px one-column form and, where space permits, a 176 px sticky section rail
for Identity, Schedule, Target, Environment, Policy, and Review. Sections are separated primarily
by 40 px spacing and dividers rather than floating cards. A solid sticky action tray shows review or
save, cancel, dirty, and saving state and reserves page space so it never covers the last error.
Settings reuses the same Field/section/action composition, visually separates browser-local
Appearance from durable scheduler policy, and presents pruning old→new consequences in a short
blocking dialog. Long forms never move into dialogs.

### Component boundary and dependencies

Locron retains native input, textarea, checkbox, and radio semantics, including immediate radio-card
comparison for consequential two-to-four-way choices. Fixed compact enumerations use a Locron
`Select` wrapper over `@radix-ui/react-select` 2.3.7. Row commands use `DropdownMenu` over
`@radix-ui/react-dropdown-menu` 2.1.24; short confirmations use `Dialog` over
`@radix-ui/react-dialog` 1.1.23; optional icon help uses `Tooltip` over
`@radix-ui/react-tooltip` 1.2.16. These individual packages replace hand-built popup state without
adopting Radix Themes or a general UI kit. `lucide-react` 1.34.0 supplies direct named outline icons;
dynamic icon-name loading, icon fonts, and runtime asset requests are prohibited.

One application-owned portal root sits beside the React root. Semantic tokens live on `:root` so
portalled content cannot lose theme inheritance. The depth scale is sticky 10, menu/tooltip 30,
overlay 40, dialog 50; popups are viewport-bounded. Route changes close layers. Tests exercise open,
highlighted, selected, invalid and disabled styling plus Enter, Space, arrows, typeahead, Escape,
outside pointer, scroll containment, background inertness, and focus restoration. Tooltip content is
never essential. A custom editable combobox and searchable timezone picker remain out of scope.

### Token and interaction implementation

`DESIGN.md` and CSS move from the earlier luminous-shell palette to the exact flat palette in
`docs/FINDINGS.md` §26. Component code consumes semantic roles only: canvas, surface, raised,
passive/control borders, text/muted, accent/on-accent/accent-soft, focus, primary/on-primary, and
four separate status pairs. Amber never doubles as warning. Desktop rails and all work surfaces are
opaque. Only narrow sticky chrome may gain a 92–94% surface and at most 10 px blur behind an
`@supports` enhancement; reduced transparency keeps the solid surface.

The shared scale is 4/8/12/16/24/32/40/48/64 px spacing; 4/6/8/12 px radii; 36 px compact controls,
40 px ordinary fields, 32 px desktop icon buttons, and at least 44 px touch targets. Geist remains
the only bundled product font with Korean/system fallbacks. Icons use 16 px normally, 18 px in
navigation, `currentColor`, and 1.75 stroke. Workbench shadow is none; only menus and dialogs receive
defined transient elevation. Motion is 80/120/160/200 ms for press, color, popup, and dialog/state
feedback with opacity/transform; reduced motion removes transform and caps opacity at 80 ms.

### Change order

1. Update `DESIGN.md`, semantic CSS tokens, dependency pins, the portal root, and reusable icon,
   Select, Menu, Dialog, Tooltip, status, shell, route-header, toolbar, and responsive-data
   component contracts before route styling.
2. Replace the authenticated shell and entry treatment, then migrate Jobs and Run history to the
   shared desktop-table/mobile-object-row pattern and accessible row menus.
3. Recompose Job detail, Run detail, Diagnostics, JobForm, and Settings around flat sections,
   bounded form measures, sticky section/action navigation, and short review/confirmation dialogs.
4. Add component tests for primitive keyboard/focus behavior, responsive semantic variants,
   persistent labelled actions, theme states, and long/error content; rebuild and compare the
   deterministic committed dist.
5. Run the full Rust/frontend gate, then inspect every route and transient state in both themes at
   desktop, compact rail, mobile, 200% zoom, keyboard-only, reduced motion, and reduced transparency.
   Correct documentation before code if the accepted behavior changes, then leave the verified
   fixture server running for review.

## Accepted: finish-quality refinement (2026-08-25)

The cockpit structure remains accepted. This pass refines code presentation, typographic metrics,
hover semantics, material, row navigation, and the neutral state ramp without reopening route IA.

### JSON viewer

Add a dependency-free read-only `JsonViewer` and pure RFC 8259 lexer. The lexer operates on the
original string and returns typed text spans for keys, strings, numbers, literals, punctuation, and
whitespace; it never parses and reserializes and never emits HTML. The viewer renders React text
nodes in one `<pre><code>` reading surface with an opaque header, language label, exact-copy action,
copy status, persisted wrap toggle, and assistive-text equivalent. Invalid content retains the
source with an explicit state. Above 200 lines or 64 KiB it initially renders 80 complete lines and
offers an explicit expansion while Copy always uses the full source. Expanded content is bounded
and internally scrollable. Existing redacted job snapshot, run snapshot, and audit JSON surfaces use
the shared viewer; no editor/highlighter dependency is added.

### Typography, hover, and material

`DESIGN.md` and CSS adopt the exact role metrics in `docs/FINDINGS.md` §27. Geist variable weights
replace the coarse 400/700 rhythm. Korean does not inherit negative tracking: mixed-language copy
uses the adjusted or zero-tracking role, and locally unavailable Pretendard is removed from the
declared fallback chain. Body, labels, controls, tables, metadata, and code receive separate line
metrics; icons align to the text baseline instead of being padded into place.

Labelled 224 px navigation and labelled mobile navigation do not mount a Tooltip or `title`.
Tooltips remain only in the 64 px icon-only rail and genuinely icon-only supplemental actions.
Hover/focus uses the authored surface ramp and active marker without repeating copy, lifting rows,
or adding shadow. Focus-visible remains independent and high contrast.

Glass is a functional overlapping layer, not page decoration. Desktop rail, workbench, tables,
forms, code, notices, and dialogs stay opaque. Sticky route/action chrome, mobile bottom navigation,
and transient menu/tooltip layers receive the §27 alpha, 14–16 px blur, restrained saturation,
hairline, and localized shadow only under `@supports`; a solid value precedes the enhancement.
Forced colors, increased contrast, reduced transparency, and an application solid-material hook
remove blur. Dialogs use opaque content over smoke. No gradient or glow is introduced to reveal it.

### Color and row interaction

Extend the semantic palette with exact `hover`, `pressed`, `selected`, and `disabled-text` roles in
both themes. Components move between audited semantic states instead of inventing opacity or local
hex. Warm neutral lightness defines hierarchy; amber stays limited to brand, focus, selection, and
small emphasis; warning and every other status retain their independent labelled pair. Runtime CSS
uses audited sRGB hex; OKLCH and APCA may assist design review but do not generate runtime colors.

Desktop tables and mobile object rows keep one descriptive native anchor in their primary field and
one separately named action menu. A shared pointer-row helper follows the anchor for an unmodified
primary click only when the event starts on noninteractive row space and no text is selected. It
ignores prevented, modified, non-primary, interactive-descendant, menu, and text-selection events.
Rows do not gain `role=link`, `tabindex`, keyboard handlers, or overlay anchors, preserving browser
link behaviors and a single keyboard stop. Hover, pressed, focus-within, and current markers make
the surface legible without card lift or shadow.

### Stable empty tables and form spacing

Jobs and Run history always render their desktop table frame and header after a successful load,
including zero results. A shared semantic empty body row spans the visible column count, keeps at
least 160 px of body context, and owns only readable copy plus one secondary recovery action. The
narrow object-list variant mirrors the same state without pretending an empty object exists.
Filtered zero resets all active route filters through `Clear filters` and focuses search; true first
use presents route-specific creation/navigation instead. Pagination is absent at total zero. The
toolbar's existing polite result status owns announcement and the row is not another live region.
Loading and error remain outside this successful-empty state.

Form layout exposes separate spacing roles rather than relying on one container gap: label/legend
to control 8 px, final grouped control to help 8 px, help-to-error 4 px, field-to-field 20 px, and
section-to-section 40 px. Theme help follows the complete segmented group in normal flow, uses muted
13/18 copy within 56ch, and is referenced by the group. The same contract prevents wrapped radios,
checkboxes, and review controls from colliding with their explanations.

### Change order

1. Update `DESIGN.md` with the type, surface, color, material, code-view, and row-interaction rules;
   add pure lexer/viewer and pointer-row contracts with focused tests.
2. Replace every raw JSON block, condition tooltips by actual icon-only need, and migrate Jobs/Runs
   desktop and mobile rows to the pointer enhancement with descriptive links and isolated menus.
3. Apply the typography metrics, warm state ramp, functional glass tokens, explicit form spacing,
   and stable empty table rows across shell, routes, controls, popups, code, and narrow composition.
4. Run type/component/asset/Rust and deterministic-dist gates, then browser-review code, mixed copy,
   row pointer/selection/menu/link behavior, material fallbacks, colors, and both themes across the
   full viewport and zoom matrix before publication.

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
- Browser reload while the `HttpOnly` session cookie is valid: session status authenticates the
  app without exposing or reading that cookie from JavaScript.
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
- EventSource reconnect after one or more output frames: replayed attempt/sequence pairs do not
  duplicate console lines, while later attempts whose sequence restarts at zero still render.
- Dashboard registration under an explicit state directory: the registered process and later
  refresh use that directory rather than the default state root.
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
- **Brand and accessibility checks:** `DESIGN.md` contains every accepted guide section and its
  documented tokens agree with the CSS token layer; browser inspection covers visible focus,
  current navigation, semantic state labels, long-content wrapping, keyboard paths, 200% zoom,
  reduced motion, and representative desktop and narrow viewports.
- **Refresh regressions:** browser reload remains authenticated through the `HttpOnly` session;
  HTTP create/edit/dry-run accepts non-ASCII inline bodies as bytes and refuses unsupported
  methods; SSE reconnect deduplicates attempt/sequence pairs; service templates retain state
  directory and explicit service mode; IPv6-only binding prints an IPv6 URL; malformed self-update
  status output produces a warning and no registration mutation.

## Settings submission, exact pretty JSON, and nested row parity (2026-08-25)

### Accepted: one durable Settings form and one aggregate review

Settings keeps separate `saved` and editable `draft` snapshots. A pure change collector compares
the six durable scalar/path/retention fields and optionally adds the environment name/value currently
entered by the user. Browser-local theme state is excluded because it applies immediately and never
touches daemon settings. Existing environment removal stays an explicit inline destructive action;
it is not disguised as a page submission.

The repeated per-field Review buttons are removed. After the Environment section, one bottom action
group exposes primary `Review changes` and secondary `Discard changes`. Both are disabled when the
durable draft is unchanged and no environment value is staged. Review first focuses any invalid
field, validates the environment grammar, then dry-runs every collected change through the existing
per-key settings endpoint. Only a fully successful dry-run opens one dialog listing every changed
setting and its safe human consequence. The dialog applies the same ordered list through the live
endpoint. Successful completion refetches the canonical settings snapshot, clears the environment
inputs and dirty state, and announces the saved count. A live per-key failure names that key, closes
neither the user's remaining draft nor the evidence of unsaved work, and refetches durable state so
the page never claims atomic behavior the API does not provide. No batch endpoint or store invariant
is introduced in this UI refinement.

The action group remains in normal document flow at the end of the form; it is not duplicated after
each section. Dirty-state copy and `beforeunload` protection cover accidental navigation while
durable settings differ from the saved snapshot. Reset restores the canonical saved fields and
clears the staged environment value without changing the immediate browser theme.

### Accepted: token-preserving pretty presentation

The JSON lexer remains the single source for exact syntax roles. A new pure formatter consumes a
valid token stream, drops only source whitespace, and inserts deterministic two-space indentation,
newlines after commas/open containers, and one space after colons. It never round-trips through a
JavaScript object. String, key, number, literal, and punctuation token text is emitted byte-for-byte,
so duplicate keys, exponent spelling, negative zero, Unicode escapes, slash escapes, and key order
survive. Empty arrays/objects stay on one line. Invalid JSON bypasses the formatter and stays exact.

JsonViewer derives presentation, visible preview, display line count, long-payload expansion, and
syntax spans from that formatted string. Its Copy action continues to write the original `source`
and reports `Copied exact JSON`; the accessible code text is the formatted presentation. The size
threshold still uses original UTF-8 bytes while the line threshold uses formatted lines, preventing
a one-line large object from evading progressive disclosure.

### Accepted: Recent runs uses the shared row contract

Job detail's Recent runs rows gain the same `clickable-row`, shared `navigateRow` delegation, and
`data-row-link` native anchor used by Jobs and Run history. The visible short ID remains compact,
while screen-reader text names the full run and detail destination. The row keeps native table
semantics and ignores text selection, modified/non-primary clicks, and interactive descendants.
No new navigation helper or synthetic row role/tab stop is added.

### Accepted: Jobs filters align by semantic rows

Jobs replaces the generic flex-only toolbar composition with a route-specific filter grid. Search
and State filter each render their label and control into matching grid rows; Search's helper text
occupies the following row without changing the neighboring control position. The result status is
placed in its own final grid column and aligned to the control row, not centered against the full
height of either Field. Existing `Field` semantics and labelled controls remain unchanged.

Below the table breakpoint the grid becomes one column in DOM reading order—Search, State filter,
then result status—and each Field returns to normal-flow spacing. Long helper/result copy wraps
inside the available width. The change is scoped to the Jobs toolbar so other forms do not acquire
special layout assumptions.

### Verification additions

- Settings component tests prove several scalar changes produce one review dialog and one ordered
  apply flow; unchanged/invalid states block review; discard restores saved values; theme changes do
  not dirty the durable form; staged environment validation/redaction and per-key failure recovery
  remain explicit; no per-field Review button remains.
- JSON pure/component tests prove deterministic two-space presentation while exact-copy preserves
  CRLF, duplicate keys, exponent/negative-zero spelling, Unicode and slash escapes; invalid JSON is
  unchanged; formatted line thresholds, expansion, wrap persistence, and literal-markup safety pass.
- Job detail tests activate a Recent runs row surface and verify navigation, native anchor isolation,
  text-selection and modified-click guards, full accessible run identity, and unchanged empty state.
- Jobs component/static and browser checks prove Search and State filter labels and controls share
  desktop grid rows, helper copy cannot shift the select, result status is independent, and 390 px
  stacking preserves reading order without overflow.
- Rebuild the committed dist twice, run strict typecheck, frontend tests, Rust asset tests, full
  workspace fmt/clippy/tests, and browser-check Settings review/apply/discard, pretty JSON in both
  themes, and Recent runs pointer/keyboard behavior at desktop and narrow widths.

## Disabled-job completeness and integrated dashboard QA (2026-08-25)

### Complete current-job source

Jobs changes its collection request from the API's enabled-only default to the existing complete
current-job form, using the established string boolean query. The route continues to search and apply
state filters locally so typing remains immediate and state changes need only one collection refresh.
No server default, store query, persistence rule, or CLI behavior changes.

Run history uses the same complete current-job collection solely for `job_id` to current-name
enrichment. The durable runs query, its literal server-side partial search, trailing debounce,
pagination, stale-response protection, and removed-job fallback remain unchanged. Keeping this second
consumer explicit prevents disabled schedules from degrading otherwise-current run labels.

### State-transition behavior

List action-menu enable/disable posts through the existing mutation endpoint, keeps the Jobs hash
route and active search/state controls, then refetches the complete collection. The shared row-click
guard rejects any interactive click target, including a menu item portalled outside the row DOM;
ordinary unused row space still delegates to the native row link. Under `All states`,
the row updates in place. Under a matching single-state filter it appears, and under the opposite
filter it leaves the result set with the existing stable empty-row treatment when necessary. The row
action remains isolated from whole-row navigation. Detail enable/disable continues to reload the same
detail route from durable state.

Disabled jobs do not need a preview request to claim a runnable next occurrence. The Jobs facts pass
records their next value as disabled/not scheduled while retaining last-run history; enabled jobs keep
the existing preview request. This avoids displaying a future occurrence that the daemon will not
admit while the schedule is disabled.

### Verification strategy

- Component tests require the exact complete-view Jobs request, cover enabled and disabled fixture
  rows under all three state filters plus partial name/tag search, and prove disable/enable refreshes
  without row navigation while preserving the selected filters. Row-navigation tests explicitly cover
  portalled interactive targets in addition to ordinary interactive descendants.
- Run history tests require complete-view name enrichment and retain the 250 ms trailing-debounce,
  immediate Enter, partial-name, stale-response, literal-character, pagination, and fallback cases.
- Static embedded-asset contracts pin both complete-view consumers and the disabled-next-occurrence
  presentation, followed by deterministic production builds and built-JavaScript syntax validation.
- The authenticated live QA matrix covers Jobs, Job detail, Run history, Run detail, Settings,
  Diagnostics health loading, action menus, whole-row navigation, desktop/mobile layouts, light/dark themes, result
  counts, empty states, and browser errors/warnings. QA state transitions are restored before handoff.
- Finish with frontend typecheck/tests, Rust asset/server contracts, workspace fmt, warnings-denied
  workspace clippy, and full workspace all-target tests before commit and server restart.
