// Shared view components: escaping, dual-rendered times, state chips,
// schedule summaries, attempt segment strips, event badges, empty states,
// base64 decoding, and a small hand-written ANSI parser.
"use strict";

const Components = (() => {
  function escapeHtml(value) {
    return String(value)
      .replaceAll("&", "&amp;")
      .replaceAll("<", "&lt;")
      .replaceAll(">", "&gt;")
      .replaceAll('"', "&quot;")
      .replaceAll("'", "&#39;");
  }

  function escapeAttr(value) {
    return escapeHtml(value).replaceAll("`", "&#96;");
  }

  /// Local RFC 3339-ish instant from epoch microseconds.
  function fmtInstant(us) {
    if (us === null || us === undefined) return "";
    const date = new Date(us / 1000);
    if (Number.isNaN(date.getTime())) return "";
    const pad = (n, w) => String(n).padStart(w, "0");
    return (
      `${date.getFullYear()}-${pad(date.getMonth() + 1, 2)}-${pad(date.getDate(), 2)}` +
      `T${pad(date.getHours(), 2)}:${pad(date.getMinutes(), 2)}:${pad(date.getSeconds(), 2)}`
    );
  }

  /// Relative "x ago / in x" text from epoch microseconds.
  function relTime(us, now) {
    if (us === null || us === undefined) return "";
    const seconds = Math.round((us - now) / 1e6);
    const abs = Math.abs(seconds);
    let value;
    if (abs < 5) value = "now";
    else if (abs < 60) value = `${abs}s`;
    else if (abs < 3600) value = `${Math.round(abs / 60)}m`;
    else if (abs < 86400) value = `${Math.round(abs / 3600)}h`;
    else value = `${Math.round(abs / 86400)}d`;
    return seconds < 0 ? `${value} ago` : `in ${value}`;
  }

  /// Dual-rendered time: human text with the absolute instant in data-*.
  function dualTime(us, now) {
    const instant = fmtInstant(us);
    const relative = relTime(us, now);
    return `<time data-t="${escapeAttr(instant)}" title="${escapeAttr(instant)}">${escapeHtml(relative || "—")}</time>`;
  }

  function formatDuration(us) {
    if (us === null || us === undefined) return "—";
    if (us < 1e3) return `${us}µs`;
    if (us < 1e6) return `${(us / 1e3).toFixed(1)}ms`;
    const seconds = us / 1e6;
    if (seconds < 60) return `${seconds.toFixed(2)}s`;
    const minutes = Math.floor(seconds / 60);
    return `${minutes}m${Math.round(seconds % 60)}s`;
  }

  const ACTIVE_STATES = new Set(["queued", "starting", "running", "retry_wait"]);
  const TERMINAL_OK = new Set(["succeeded"]);

  function stateClass(state) {
    if (ACTIVE_STATES.has(state)) return "state-active";
    if (TERMINAL_OK.has(state)) return "state-good";
    return "state-bad";
  }

  function chip(text, className, spinner) {
    const icon = spinner ? '<span class="spinner" aria-hidden="true"></span> ' : "";
    return `<span class="chip ${className}">${icon}${escapeHtml(text)}</span>`;
  }

  function stateChip(state) {
    return chip(state, stateClass(state), ACTIVE_STATES.has(state));
  }

  function enabledChip(enabled) {
    return chip(enabled ? "enabled" : "disabled", enabled ? "state-good" : "state-muted");
  }

  function outcomeChip(run) {
    if (ACTIVE_STATES.has(run.state)) return stateChip(run.state);
    const outcome = run.outcome || run.state;
    return chip(outcome, stateClass(outcome), false);
  }

  /// Humanized schedule summary with the raw expression as secondary text.
  function scheduleSummary(definition) {
    const schedule = definition && definition.schedule ? definition.schedule : {};
    const raw = schedule.expression || "—";
    let summary;
    if (schedule.kind === "cron") {
      summary = `cron ${raw}`;
    } else if (schedule.kind === "once") {
      summary = "once";
    } else {
      summary = schedule.kind || "no schedule";
    }
    if (schedule.timezone && schedule.timezone.mode && schedule.timezone.mode !== "local") {
      summary += ` (${escapeHtml(schedule.timezone.mode)})`;
    }
    return `<span class="schedule">${escapeHtml(summary)} <span class="raw">${escapeHtml(raw)}</span></span>`;
  }

  function targetSummary(definition) {
    const target = definition && definition.target ? definition.target : {};
    switch (target.kind) {
      case "process":
        return `${escapeHtml(target.executable || "?")} ${escapeHtml((target.args || []).join(" "))}`;
      case "shell":
        return escapeHtml(target.shell || "?");
      case "http":
        return `${escapeHtml((target.method || "GET").toUpperCase())} ${escapeHtml(target.url || "?")}`;
      default:
        return escapeHtml(target.kind || "?");
    }
  }

  /// Attempt segment strip: one colored cell per attempt, width proportional
  /// to duration, linking into the log viewer.
  function attemptStrip(run, link) {
    const attempts = (run.attempts || []).filter((attempt) => attempt.duration_us !== null);
    if (attempts.length === 0) return '<span class="muted">no attempts</span>';
    const total = attempts.reduce((sum, attempt) => sum + Math.max(attempt.duration_us, 1), 0);
    const cells = attempts
      .map((attempt) => {
        const width = (Math.max(attempt.duration_us, 1) / total) * 100;
        const href = link ? `href="#/runs/${encodeURIComponent(run.id)}"` : "";
        return (
          `<a ${href} class="segment ${stateClass(attempt.state)}" title="attempt ${attempt.attempt_number}: ${attempt.state} ${formatDuration(attempt.duration_us)}" ` +
          `style="width:${width.toFixed(2)}%"></a>`
        );
      })
      .join("");
    return `<span class="segments">${cells}</span>`;
  }

  /// Event kind badges, in the healthchecks event-kind badge set.
  function eventBadge(kind) {
    const classes = {
      run_requested: "badge-neutral",
      run_cancelled: "badge-danger",
      cancellation_requested: "badge-danger",
      schedule_cursor_advanced: "badge-neutral",
      job_created: "badge-good",
      job_updated: "badge-good",
      job_deleted: "badge-muted",
      termination_unconfirmed_acknowledged: "badge-danger",
      output_prepared: "badge-good",
      output_finalized: "badge-good",
    };
    return `<span class="badge ${classes[kind] || "badge-neutral"}">${escapeHtml(kind)}</span>`;
  }

  function emptyState(title, detail, actionHtml) {
    return (
      `<div class="empty-state"><h2>${escapeHtml(title)}</h2>` +
      (detail ? `<p class="muted">${escapeHtml(detail)}</p>` : "") +
      (actionHtml ? `<div class="empty-action">${actionHtml}</div>` : "") +
      "</div>"
    );
  }

  function errorBlock(error) {
    return `<div class="error-block">${escapeHtml(error.message || String(error))}</div>`;
  }

  function decodeBase64(b64) {
    try {
      const binary = atob(b64);
      const bytes = new Uint8Array(binary.length);
      for (let i = 0; i < binary.length; i += 1) bytes[i] = binary.charCodeAt(i);
      return new TextDecoder("utf-8", { fatal: false }).decode(bytes);
    } catch {
      return `[invalid base64: ${b64.slice(0, 32)}…]`;
    }
  }

  /// Minimal hand-written ANSI SGR parser: bold and the standard foreground
  /// colors render as spans; everything else is dropped.
  const ANSI_COLORS = {
    30: "#555", 31: "#d1242f", 32: "#2a7a2a", 33: "#9a6700",
    34: "#2456c6", 35: "#8a2f8a", 36: "#00787a", 37: "#444",
    90: "#767676", 91: "#f87171", 92: "#4ade80", 93: "#facc15",
    94: "#60a5fa", 95: "#e879f9", 96: "#22d3ee", 97: "#d4d4d8",
  };

  function ansiToHtml(text) {
    let open = "";
    let out = "";
    const parts = text.split(/\x1b\[([0-9;]*)m/);
    // parts alternates: text, code, text, code, ...
    for (let i = 0; i < parts.length; i += 1) {
      if (i % 2 === 1) {
        const codes = parts[i] ? parts[i].split(";").map(Number) : [0];
        let style = "";
        for (const code of codes) {
          if (code === 0 || code === 39) style = "";
          else if (code === 1) style = "font-weight:bold;";
          else if (ANSI_COLORS[code]) style = `color:${ANSI_COLORS[code]};`;
        }
        open = style;
        continue;
      }
      const text = parts[i];
      if (!text) continue;
      out += open
        ? `<span style="${open}">${escapeHtml(text)}</span>`
        : escapeHtml(text);
    }
    return out;
  }

  /// Renders a redacted API value: the CLI's literal markers, never a value
  /// or a synthesized sentinel.
  function redactedValue(value) {
    if (value === null || value === undefined) return "—";
    if (typeof value === "object") {
      return `<code class="json">${escapeHtml(JSON.stringify(value, null, 2))}</code>`;
    }
    const text = String(value);
    if (text === "<redacted>") return '<span class="redacted">&lt;redacted&gt;</span>';
    return escapeHtml(text);
  }

  function nowUs() {
    return Date.now() * 1000;
  }

  return {
    escapeHtml,
    escapeAttr,
    fmtInstant,
    relTime,
    dualTime,
    formatDuration,
    ACTIVE_STATES,
    stateClass,
    chip,
    stateChip,
    enabledChip,
    outcomeChip,
    scheduleSummary,
    targetSummary,
    attemptStrip,
    eventBadge,
    emptyState,
    errorBlock,
    decodeBase64,
    ansiToHtml,
    redactedValue,
    nowUs,
  };
})();
