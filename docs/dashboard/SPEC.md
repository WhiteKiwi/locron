# locron Dashboard Specification

## Status

Frozen on 2026-08-24 after interactive product review. This document defines the
web-administration surface listed as phase 1 of the deferred product roadmap in `docs/TODO.md`.
Per that roadmap, it does not change the exclusions in the frozen `docs/SPEC.md`. Implementation
choices and their trade-offs are recorded in `IMPLEMENTATION.md` (this directory); research evidence in
`docs/FINDINGS.md` §14.

## Purpose

This document defines the local web administration surface in `locron`: a loopback-only web
dashboard and HTTP management API that lets the single owning user inspect and manage jobs, runs,
captured output, and diagnostics in a browser. The surface is opt-in, reuses the same durable
application commands as the CLI and MCP surfaces, and runs as a process separate from the scheduler
daemon.

---

## 1. Goals and Observable Completion Criteria

### Goals

1. Give the owning user a browser view of the scheduler's observable state — jobs, schedules, runs,
   attempts, captured output, and diagnostics — with the same guarantees as the CLI.
2. Make every durable operation available on the command line available through the API with
   identical validation, normalization, atomicity, redaction, and stable error meaning.
3. Keep the surface strictly local, single-user, and opt-in: nothing listens unless the user
   explicitly enables it or runs it in the foreground.
4. Keep the runtime lightweight and self-contained within the single `locron` binary, supervised
   separately from the scheduler daemon.

### Observable Completion Criteria

1. `locron dashboard` starts a foreground loopback-only server and prints the exact access URL.
   `locron dashboard enable` registers the dashboard as a per-user service that starts immediately
   and again at login, as a process separate from the scheduler daemon. An explicitly requested
   port that is occupied fails with an actionable error; the default port falls back to the next
   free port in foreground mode and is fixed in service mode, and the chosen address is always
   printed or queryable.
2. The surface is off by default: without `enable` or an explicit foreground run no listener
   exists. Without a valid access token the server serves only a minimal entry page; no job, run,
   output, or diagnostic state is exposed. Every state-bearing page and API endpoint requires the
   token.
3. A browser can list and inspect jobs, preview schedules, and view run history, attempts, and
   captured output with CLI-equivalent redaction, truncation markers, and time rendering.
4. A browser can perform every mutation the CLI supports — job create/update/enable/disable/remove,
   manual run, cancellation including quarantine acknowledgement, global settings, prune, and
   import/export — with CLI-equivalent validation, dry-run where the CLI supports it, and stable
   error mapping.
5. A mutation is durable before the API reports success, and it wakes the daemon exactly like a CLI
   mutation. The dashboard works while no daemon is running, including offline manual submission,
   and responses distinguish durable acceptance from execution completion. Dashboard service
   restarts never affect scheduling.
6. Live output for an active run streams as it is written, bounded by the same capture and
   retention limits, with an explicit stream-termination signal. Following a run never cancels it.
7. State-changing requests cannot be triggered cross-origin: the server validates the Host header
   against loopback names, validates an Origin header against the server origin when present, and
   requires an anti-CSRF token on cookie-authenticated mutations so another page cannot forge a
   mutation.
8. Exposure diagnostics report the bound address, token facts, service state, and security posture;
   the server refuses non-loopback binding, and the web diagnostics page reports the same facts as
   the command-line diagnostics.
9. Machine-readable API results use a versioned JSON envelope, and API errors map to the stable CLI
   error categories.

---

## 2. In Scope vs Out of Scope

### In Scope

- Single-binary entrypoint: the `locron dashboard` command family.
- HTTP/1.1 on IPv4 and IPv6 loopback (`127.0.0.1`, `::1`).
- Viewer pages: job list/detail, create/edit with schedule preview, run history and attempt
  breakdown, log viewer with follow, why, and doctor.
- REST API over the durable application-command boundary.
- Per-user service registration (a LaunchAgent on macOS, a systemd user unit on Linux) through the
  same service-manager boundary the daemon uses, with the same stop-at-logout behavior.
- Durable per-state-directory random access token, Host/Origin validation, anti-CSRF protection,
  and redaction parity.
- Bundled static assets; the viewer requires no network access and no CDN.
- An SSE stream for live captured output.

### Out of Scope

- Binding non-loopback interfaces, TLS termination, remote access, and reverse-proxy or
  remote-viewer support.
- Multi-user accounts, per-user authorization, or sharing one scheduler between users.
- Another execution engine or any path that bypasses durable application commands.
- Serving over the MCP transport or running the server inside the daemon process.
- Automatic registration by installers or updates: only an explicit `enable` registers the
  dashboard service.
- The desktop application (roadmap phase 3) remains a separate surface that may consume this API.

---

## 3. Launch, Service Registration, Binding, and Ports

- `locron dashboard` runs the server in the foreground. It does not require the daemon to be
  running and does not take daemon ownership; Ctrl-C stops it and removes the listener.
- `locron dashboard enable` is the persistent path: it generates the access token when absent,
  registers a per-user service (separate from the daemon's registration), starts it immediately,
  and arranges automatic start at login. Repeating it refreshes and repairs the registration.
  Installers and updates never register the dashboard on their own.
- The daemon registration command family remains daemon-only; the dashboard family manages only
  the dashboard registration. The two registrations never touch each other.
- The dashboard service is a process separate from the scheduler daemon. It reads the same durable
  state, sends the same wake hint after mutations, and works while the daemon is offline. Its
  restarts never affect scheduling, and daemon restarts do not stop the dashboard.
- On Linux the dashboard service stops at logout and starts again at the next login; keeping it
  running after logout is the same documented optional operator step (`loginctl enable-linger`)
  that applies to the daemon.
- The server binds loopback only. Any other bind address is refused, not merely warned about.
- The default port is 10824. In foreground mode an occupied default port falls back to the next
  free port and the chosen address is printed. In service mode the port is fixed: an occupied port
  is reported through the status command rather than silently changing the bookmarked address.
- `locron dashboard token` re-displays the access token; `locron dashboard enable --reset`
  regenerates it and restarts the service; `locron dashboard disable` unregisters the service and
  removes the token, warning about any foreground instance the user must stop themselves.

---

## 4. Access Control and CSRF

- On first use the server generates a high-entropy random token and stores it in an owner-only
  file in the state directory. Later starts reuse it; regeneration is explicit, and removing the
  token file causes regeneration on the next start.
- The token never appears in a URL. It is accepted through an `Authorization` header (scripts and
  automation) and through a one-time paste at the entry page, which sets a same-site session
  cookie so later visits need no token.
- Every API endpoint and every state-bearing page requires a valid token or session cookie. The
  entry page is the only unauthenticated response.
- The Host header must be a loopback name of the server (`localhost`, `127.0.0.1`, `[::1]`);
  anything else is refused before routing, defeating DNS-rebinding attempts.
- State-changing requests carrying an Origin header with an origin different from the server's
  loopback origin are refused. In addition, every cookie-authenticated mutation requires a
  double-submit anti-CSRF token issued by the server (cookie plus custom header or form field), so
  another loopback origin cannot forge a mutation even with a valid session cookie. Requests
  authenticated solely by the bearer token in the Authorization header are exempt, because a
  cross-site page cannot attach that header.
- The token never appears in logs, diagnostics, or API responses. Diagnostics report only token
  presence and file-permission facts.
- All responses carry `Referrer-Policy: no-referrer`.

---

## 5. Viewer Surface

- Job list: name, schedule summary, enabled state, next occurrence, and last outcome.
- Job detail: full definition and policies, recent runs, and why explanation. Create/edit forms
  validate against the same rules as the CLI and support dry-run preview.
- Run history: trigger, nominal time, timing, outcome, attempt breakdown, and
  skip/supersession/acknowledgement events.
- Log viewer: stream order preserved, redaction applied, truncation and discard markers rendered,
  follow mode with an explicit termination notice.
- Diagnostics page: scheduler health, effective paths, daemon availability, exposure facts, and
  settings.
- The viewer is fully self-contained: all assets are bundled in the binary.

---

## 6. Mutation API Surface

- REST endpoints map one-to-one onto durable application commands: job
  create/update/enable/disable/remove, schedule preview, manual run (enqueue and wait),
  cancellation with quarantine acknowledgement, global settings, retention/prune, export
  (download) and import (upload with the same plaintext-acknowledgement rules), and doctor.
- Machine-readable responses use a versioned `locron.api/v1` envelope with the same field
  semantics as the CLI's machine output.
- Redaction is the same central boundary: inline environment values, sensitive headers, and body
  content never appear in normal responses.
- Errors map the stable CLI error categories to HTTP statuses (validation, conflict, not-found,
  busy, refused); the mapping is documented in `IMPLEMENTATION.md`.

---

## 7. Live Updates

- An SSE endpoint streams captured output for one run as it is written, as JSON text frames
  preserving stream order, plus run/attempt state transitions and a final termination event.
  Consumers follow the same framed output the CLI follows; retention bounds apply unchanged. The
  endpoint authenticates through the session cookie, because browsers cannot attach an
  Authorization header to an EventSource connection and the token never appears in a URL.
- Schedule preview and next-occurrence rendering are point-in-time calculations and require no push
  channel.

---

## 8. Daemon Interaction

- Every mutation reuses the durable application commands: commit to SQLite first, then send the
  same best-effort wake hint the CLI sends. The wake socket is never treated as an API.
- Success responses describe durable state. Submission success does not promise execution
  completion, matching the CLI contract.
- The server never signals processes, cancels runs, or resolves quarantine directly; those remain
  application commands.

---

## 9. Exposure Diagnostics

- Startup prints the exact access URL; the status command reports service state, the access URL,
  and token facts.
- `doctor` reports the token file presence and permissions and whether a dashboard service is
  registered. The web diagnostics page reports the same facts as the command-line diagnostics.
- Documentation states the surface's guarantees and non-goals plainly: it is reachable only from
  the local machine, and it does not protect state from other processes of the same user, exactly
  like the CLI.

---

## 10. Open Questions

None after `docs/FINDINGS.md` §14; implementation choices and their trade-offs are recorded in
`IMPLEMENTATION.md`.
