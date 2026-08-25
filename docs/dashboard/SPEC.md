# locron Dashboard Specification

## Status

Frozen on 2026-08-24 after interactive product review and amended on 2026-08-24 to define the
Locron brand experience. This document defines the shipped web-administration surface. Its
post-milestone delivery does not retroactively change the milestone exclusions in the frozen
`docs/SPEC.md`. Implementation
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
5. Make the dashboard feel unmistakably Locron: calm, local-first, warm, and precise, with the
   approachable character of the project artwork and the clarity expected of an operational tool.

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
10. A documented Locron brand system defines the product promise, voice, wordmark use, color,
    typography, iconography and illustration, layout, components, motion, accessibility, and clear
    examples of correct and incorrect use.
11. The entry and authenticated dashboard surfaces consistently apply that system while preserving
    dense, scan-friendly presentation of status, upcoming work, anomalies, and available actions.
12. The dashboard remains usable with a keyboard and assistive technology, maintains readable
    contrast, respects reduced-motion preferences, and adapts from narrow mobile-sized viewports to
    large desktop displays without hiding core operations.
13. Returning users can reload or revisit the dashboard through their established local session,
    and every printed or reported access URL corresponds to an address the current server actually
    bound.

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
- A desktop application remains a separate deferred surface that may consume this API; it is
  preserved in `docs/BACKLOG.md` rather than committed by this specification.

---

## 3. Launch, Service Registration, Binding, and Ports

- `locron dashboard` runs the server in the foreground. It does not require the daemon to be
  running and does not take daemon ownership; Ctrl-C stops it and removes the listener.
- `locron dashboard enable` is the persistent path: it generates the access token when absent,
  registers a per-user service (separate from the daemon's registration), starts it immediately,
  and arranges automatic start at login. Repeating it refreshes and repairs the registration.
  The registration preserves the selected state directory. Installers and updates never register
  the dashboard on their own.
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

## 5. Brand and Visual Experience

- The brand promise is dependable local automation without infrastructure ceremony. Its tone is
  capable, direct, reassuring, and lightly playful; operational truth is never obscured by
  personality or decoration.
- The visual language takes its cues from the project artwork: warm paper-like neutrals, strong
  charcoal structure, a restrained sunny-yellow accent, rounded utilitarian forms, and occasional
  hand-drawn character details. It must remain an original Locron system rather than imitating any
  reference brand.
- The primary hierarchy answers four questions quickly: what is running, what happens next, what
  needs attention, and what action is safe to take. Secondary detail is progressively disclosed.
- Motion explains state changes and navigation, stays brief, never delays an operation, and is
  removed or simplified when the user requests reduced motion.
- Light surfaces are the primary application environment. Output and terminal-like content may use
  a focused dark surface when it improves legibility, while retaining the shared brand accents and
  accessible contrast.
- A reusable brand guide is the source of truth for future product and documentation surfaces. It
  includes practical examples and guardrails so new work can remain consistent without copying the
  dashboard layout.

---

## 6. Viewer Surface

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

## 7. Mutation API Surface

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

## 8. Live Updates

- An SSE endpoint streams captured output for one run as it is written, as JSON text frames
  preserving stream order, plus run/attempt state transitions and a final termination event.
  Consumers follow the same framed output the CLI follows; retention bounds apply unchanged. The
  endpoint authenticates through the session cookie, because browsers cannot attach an
  Authorization header to an EventSource connection and the token never appears in a URL.
- Schedule preview and next-occurrence rendering are point-in-time calculations and require no push
  channel.

---

## 9. Daemon Interaction

- Every mutation reuses the durable application commands: commit to SQLite first, then send the
  same best-effort wake hint the CLI sends. The wake socket is never treated as an API.
- Success responses describe durable state. Submission success does not promise execution
  completion, matching the CLI contract.
- The server never signals processes, cancels runs, or resolves quarantine directly; those remain
  application commands.

---

## 10. Exposure Diagnostics

- Startup prints the exact access URL; the status command reports service state, the access URL,
  and token facts.
- `doctor` reports the token file presence and permissions and whether a dashboard service is
  registered. The web diagnostics page reports the same facts as the command-line diagnostics.
- Documentation states the surface's guarantees and non-goals plainly: it is reachable only from
  the local machine, and it does not protect state from other processes of the same user, exactly
  like the CLI.

---

## 11. Open Questions

None after `docs/FINDINGS.md` §14; implementation choices and their trade-offs are recorded in
`IMPLEMENTATION.md`.

## 12. Peer themes, information architecture, and input amendment (2026-08-25)

17. The bundled dashboard offers authored light and dark schemes plus a persistent `system`
    preference. Theme selection is applied before first paint, remains browser-local, follows OS
    changes only in system mode, and is available on both the entry surface and Settings page.
    Restrained translucent chrome may frame navigation and transient controls, while tables,
    forms, notices, destructive content, and logs remain opaque workbench surfaces.
18. Authenticated navigation is Jobs, Run history, Diagnostics, and Settings. Settings owns
    Appearance, Execution, Retention & output, and Environment editing; Diagnostics is read-only
    health, path, exposure, process-resolution, and integrity evidence.
19. Schedule, target, timezone, overlap, missed-run, and retry-backoff choices use native radio
    controls with fieldset/legend semantics. Durations use an exact reusable decimal magnitude and
    native s/m/h/d unit selector, round-trip microseconds without floating-point loss, and reject
    exponent, composite, sub-microsecond, overflowing, or unsafe JSON-number values.
20. Run history supports complete Unicode-case-insensitive literal substring search across run ID
    and the durable job row's current name. Filtering precedes total and pagination over the full
    history in stable newest-first order. The UI debounces typing, cancels obsolete requests, and
    never lets stale responses replace the current query.
21. Every editable dashboard surface uses operator-facing concepts instead of storage encodings.
    Data sizes use readable binary units, instants use local date/time with an explicit timezone
    and preview, numeric limits expose their valid range and consequence, and structured text
    inputs explain their grammar with examples. Raw bytes, epoch microseconds, path separators,
    and similar internal representations are hidden from the primary workflow and appear only as
    clearly labelled advanced details when they remain operationally necessary. Related controls
    reveal only when relevant, defaults and disabled states are explicit, and validation identifies
    the affected field, preserves entered values, and focuses a useful recovery target. The visual
    treatment follows Locron's own cream/charcoal/yellow identity while adopting the supplied
    references' restrained surfaces, generous spacing, strong typography, and quiet information
    density rather than copying their branding.
22. Browser chrome is part of the Locron identity: the dashboard has a distinctive small-size
    favicon, theme-aware browser color, and concise route-aware document titles that identify both
    Locron and the current task without exposing secrets or mutable operator data.
23. The expanded dashboard is maintained as type-checked, reusable UI components rather than
    page-sized string templates and ad-hoc DOM mutation. Production remains self-contained,
    deterministic, and free of runtime CDN or network dependencies, and installing or running the
    published Locron binary does not require a JavaScript toolchain. Frontend source and the exact
    embedded production assets are both reviewable and verified against drift.

## 13. Modern operator-cockpit amendment (2026-08-25)

24. The authenticated application uses a persistent desktop navigation rail and a compact mobile
    navigation treatment so product identity, current location, primary actions, and daemon health
    remain immediately legible. Each route has one restrained header and one dominant workbench;
    ornamental hero areas, ambient gradients, broad glow, and a card around every value are not part
    of the operational interface.
25. Light and dark themes express the same hierarchy through semantic tokens: a quiet near-solid
    canvas, opaque working surfaces, crisp dividers, strong foreground contrast, and sparse amber
    brand focus. Blur or translucency is limited to transient menus and compact navigation chrome,
    has an opaque fallback, and never reduces the legibility of data, forms, warnings, or logs.
26. Every interactive control belongs to one Locron component family. Selects, overflow menus,
    dialogs, tooltips, segmented choices, buttons, text fields, date-time fields, and disclosure
    controls have authored visuals and complete hover, pressed, open, focus-visible, invalid,
    disabled, loading, and reduced-motion states. No control appears as an unintentional browser
    default. Complex popup behavior uses accessible, keyboard-complete primitives rather than an
    ad-hoc imitation, while native form semantics remain the underlying contract where useful.
27. Jobs and run history prioritize comparison and action: compact toolbars, labelled live filters,
    dense rows, stable column alignment, tabular numerals, status labels that do not rely on color,
    useful empty/loading/error states, and an overflow-safe action menu. Narrow screens replace the
    table composition with an equally complete scan-friendly layout without horizontal page
    overflow or hidden core actions.
28. Create and edit flows use a calm one-column reading order within a bounded measure, grouped by
    visible section headings and supported by section navigation on wide screens. Dependencies are
    progressively disclosed, advanced wire values recede, validation stays next to the affected
    field, and save/review actions remain discoverable during long forms. Settings uses the same
    hierarchy, with browser-local appearance clearly separated from durable scheduler policy and
    destructive or pruning consequences reviewed before application.
29. Typography, spacing, iconography, radius, border, elevation, and motion follow a small explicit
    scale documented in the Locron guide. Operational copy uses locally bundled Geist with system
    and Korean fallbacks, icons use one coherent outline language with accessible labels, and motion
    is brief and state-explanatory rather than decorative. Long names, schedules, IDs, paths, and
    translated copy remain readable at 200% zoom and narrow widths.
30. The finished experience is visually reviewed in both themes at desktop and mobile widths across
    entry, Jobs, Run history, job creation, Settings, Diagnostics, menus, dialogs, validation,
    loading, empty, and error states. It must feel like an original Locron operations product: the
    supplied portfolio informs typographic confidence and sparse amber emphasis, DeepSeek informs
    quiet space and flat surfaces, and Grafana informs information density and operational clarity;
    none is copied as a skin.

## 14. Finish-quality amendment (2026-08-25)

31. Structured JSON is presented as an operational code viewer rather than an undifferentiated text
    block. It preserves valid text exactly, supports readable syntax roles, line structure, copying,
    wrapping or bounded scrolling, and progressive disclosure for long payloads without requiring a
    network editor or making color the only distinction.
32. Typography is tuned as an application system rather than browser-document defaults. Body,
    navigation, labels, data, metadata, and monospace content each have a deliberate size, line
    height, weight, and tracking; mixed Korean and Latin copy remains balanced, dense information is
    readable, and controls align optically without loose legacy-web spacing.
33. Hover and focus feedback never repeats information already visible. In particular, labelled
    navigation items do not show duplicate tooltips; tooltips are reserved for genuinely icon-only
    supplemental controls. Menus, rows, buttons, and navigation use quiet surface, border, icon, and
    active-marker changes with distinct focus-visible behavior.
34. A restrained glass treatment may identify genuinely layered chrome and transient surfaces in
    both themes through controlled translucency, subtle saturation, a hairline highlight, and a soft
    localized shadow. Main workbenches, tables, forms, code content, notices, and dense reading
    surfaces remain opaque. Every glass surface has an authored solid fallback and respects reduced
    transparency and contrast requirements.
35. Final visual review compares the result against current public AI-product interface guidance and
    the supplied design-skill references for hierarchy, typography, spatial rhythm, interaction
    states, code presentation, motion, and accessibility. Reference techniques are recorded with
    evidence, but no generic AI gradient, excessive glow, ornamental animation, or copied brand skin
    replaces Locron's calm charcoal, cream, and amber identity.
36. A Job or Run row is itself a clear detail-navigation target rather than requiring precise clicks
    on only the title or shortened ID. Pointer users can activate the quiet row surface, keyboard and
    assistive-technology users receive one descriptive primary link, and embedded command menus
    remain separate controls that never trigger navigation. Hover, focus-within, pressed, and current
    states communicate selectability without turning each row into a floating card.
37. Color is authored as a complete perceptual system, not a collection of isolated swatches. Both
    themes define distinguishable canvas, workbench, raised, hover, selected, and control-boundary
    levels; amber chroma is restrained outside small brand moments; semantic statuses remain mutually
    distinguishable; and text, icons, focus, borders, and disabled states meet their applicable
    contrast targets. The guide records role, pairing, dark-theme adaptation, and misuse for every
    token so later components do not invent local colors.
38. Explanatory copy that follows a control group has deliberate separation from the final control;
    it never visually collides with segmented theme choices, radio cards, toolbars, or action rows.
    The shared spacing contract distinguishes label-to-control, control-to-help, and section gaps.
39. Filtered empty results preserve their operational context. Jobs and Run history keep the toolbar,
    table frame, column headers, and stable surrounding layout, then render one semantic full-width
    body row that explains the zero result and offers a small clear-filter recovery action. A truly
    empty dataset uses the same stable table structure but different copy and a route-appropriate
    primary next action; it is not confused with a loading or request-error state.

## 15. Settings, JSON, and nested-row consistency amendment (2026-08-25)

40. Durable scheduler settings behave as one editable page rather than a stack of independent
    mini-forms. A user may change several fields before committing them, sees one persistent dirty
    state, and reaches one clear action group after the final settings section. The primary action
    reviews all pending durable changes together before applying them; a secondary reset action
    restores the last saved values. Unchanged settings cannot be submitted, validation identifies
    the affected fields before review, and browser-local appearance remains immediate and outside
    the durable save transaction.
41. Valid structured JSON is formatted for reading by default with deterministic indentation and
    line breaks, while preserving values, key order, number spelling, duplicate keys, and string
    escapes from the source. Copying still returns the exact original payload rather than the
    formatted presentation. Invalid JSON remains visibly invalid and is shown exactly as received.
    Long-payload expansion, wrapping, narrow-screen behavior, and syntax roles apply to the
    formatted presentation without hiding that exact-copy contract.
42. Every repeated Job or Run summary that identifies a detail destination follows the same row
    navigation contract, including Recent runs inside Job detail. Pointer activation of unused row
    space opens the detail, the primary native link remains available to keyboard and assistive
    technology users, modified/link interactions retain browser semantics, and any nested action
    control stays isolated from row navigation.
43. Jobs filter controls share one deliberate desktop alignment system. Search and state labels sit
    on the same baseline, their controls start and end on the same rows, and optional help text does
    not push one field out of alignment with its neighbor. The result count occupies a separate
    status slot rather than participating in the form-field baseline. At narrow widths the fields
    stack in reading order with their labels and help intact, without introducing horizontal page
    overflow.

## 16. Disabled-job visibility and functional QA amendment (2026-08-25)

44. The Jobs route treats `All states` literally: every current enabled or disabled job is available
    to the list, partial text search, and the state filter. `Enabled` shows only enabled jobs and
    `Disabled` shows only disabled jobs. Changing a job state from the list refreshes that same route
    without silently navigating away; the changed row stays visible or leaves the current result set
    only when the selected state/search filter requires it. A disabled job remains reachable by its
    native detail link and can be enabled again.
45. Run history continues to show the current job name for runs whose job is disabled, so disabling a
    schedule does not degrade durable-history labels or partial job-name search. Removed-job history
    keeps the existing immutable/fallback identity behavior.
46. The live dashboard is acceptance-checked as an integrated operator workflow, not only as isolated
    components. Jobs list/search/state transitions, list and detail enable/disable, row navigation,
    Run history trailing partial search, Settings aggregate review/discard, Diagnostics health load,
    responsive layouts, both themes, authentication, and clean browser logs all have recorded pass or
    explicit failure evidence before the review server is handed back.
