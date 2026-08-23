// Runs view: paginated history with attempt segment strips, and a run detail
// with why facts, audit events, per-attempt static logs, and a live follow
// console over the SSE run stream (replay + live output + termination).
"use strict";

const RunsView = (() => {
  const C = Components;
  const esc = C.escapeHtml;
  const attr = C.escapeAttr;

  const HISTORY_PAGE = 20;

  // ---------------------------------------------------------------------
  // History
  // ---------------------------------------------------------------------

  async function renderHistory(view) {
    let offset = 0;
    view.innerHTML = `<div class="page-head">
      <h1>Run history</h1>
      <div class="actions">
        <input id="runs-job" type="search" placeholder="filter by job name or id">
        <button id="runs-refresh" type="button">Refresh</button>
      </div>
    </div>
    <div id="runs-body" aria-busy="true">${C.emptyState("Loading runs", "reading durable history…")}</div>
    <div id="runs-pager" class="pager" hidden>
      <button id="runs-prev" type="button">‹ Newer</button>
      <span id="runs-total" class="muted"></span>
      <button id="runs-next" type="button">Older ›</button>
    </div>`;

    const body = view.querySelector("#runs-body");
    const pager = view.querySelector("#runs-pager");
    const total = view.querySelector("#runs-total");
    const prev = view.querySelector("#runs-prev");
    const next = view.querySelector("#runs-next");
    const jobInput = view.querySelector("#runs-job");
    const refresh = view.querySelector("#runs-refresh");

    async function load() {
      body.setAttribute("aria-busy", "true");
      try {
        const job = jobInput.value.trim();
        const query = new URLSearchParams({
          limit: String(HISTORY_PAGE),
          offset: String(offset),
        });
        if (job) query.set("job", job);
        const [{ data }, jobsResult] = await Promise.all([
          Api.get(`/api/v1/runs?${query.toString()}`),
          Api.get("/api/v1/jobs").catch(() => ({ data: [] })),
        ]);
        const jobNames = new Map(
          jobsResult.data.map((record) => [record.id, record.name]),
        );
        body.setAttribute("aria-busy", "false");
        renderTable(body, data.runs, jobNames);
        total.textContent = `${data.total} run${data.total === 1 ? "" : "s"}`;
        prev.disabled = data.offset === 0;
        next.disabled = data.offset + data.runs.length >= data.total;
        pager.hidden = false;
      } catch (error) {
        body.setAttribute("aria-busy", "false");
        body.innerHTML = C.errorBlock(error);
      }
    }

    prev.addEventListener("click", () => {
      offset = Math.max(0, offset - HISTORY_PAGE);
      load();
    });
    next.addEventListener("click", () => {
      offset += HISTORY_PAGE;
      load();
    });
    refresh.addEventListener("click", load);
    jobInput.addEventListener("keydown", (event) => {
      if (event.key === "Enter") {
        offset = 0;
        load();
      }
    });
    await load();
  }

  function renderTable(body, runs, jobNames) {
    if (runs.length === 0) {
      body.innerHTML = C.emptyState(
        "No runs",
        "Runs appear here as soon as they are requested.",
      );
      return;
    }
    const now = C.nowUs();
    const rows = runs
      .map((run) => {
        const job = jobNames.get(run.job_id);
        return `<tr>
          <td><a href="#/runs/${encodeURIComponent(run.id)}">${esc(run.id.slice(0, 8))}</a></td>
          <td>${job ? `<a href="#/jobs/${encodeURIComponent(job)}">${esc(job)}</a>` : esc(run.job_id.slice(0, 8))}</td>
          <td>${C.dualTime(run.requested_at_us, now)}</td>
          <td><span class="badge badge-neutral">${esc(run.trigger)}</span></td>
          <td>${C.outcomeChip(run)}</td>
          <td>${C.formatDuration(run.duration_us)}</td>
          <td>${C.attemptStrip(run, true)}</td>
        </tr>`;
      })
      .join("");
    body.innerHTML = `<table class="runs-table">
      <thead><tr><th>Run</th><th>Job</th><th>Requested</th><th>Trigger</th><th>State</th><th>Duration</th><th>Attempts</th></tr></thead>
      <tbody>${rows}</tbody>
    </table>`;
  }

  // ---------------------------------------------------------------------
  // Detail
  // ---------------------------------------------------------------------

  async function renderDetail(view, params) {
    const id = params.id;
    view.innerHTML = `<div aria-busy="true">${C.emptyState("Loading run", "reading durable facts…")}</div>`;
    try {
      const [{ data: run }, why] = await Promise.all([
        Api.get(`/api/v1/runs/${encodeURIComponent(id)}`),
        Api.get(`/api/v1/runs/${encodeURIComponent(id)}/why`).catch((error) => ({ error })),
      ]);
      const whyData = why && !why.error ? why.data : null;
      view.innerHTML = detailHtml(run, whyData);
      bindDetail(view, run);
      renderConsole(view, run);
    } catch (error) {
      view.innerHTML = C.errorBlock(error);
    }
  }

  function detailHtml(run, whyData) {
    const now = C.nowUs();
    const attempts = run.attempts || [];
    const active = C.ACTIVE_STATES.has(run.state);
    const attemptsRows = attempts
      .map((attempt) => {
        const facts = [];
        if (attempt.exit_code !== null && attempt.exit_code !== undefined) {
          facts.push(`exit ${attempt.exit_code}`);
        }
        if (attempt.http_status !== null && attempt.http_status !== undefined) {
          facts.push(`http ${attempt.http_status}`);
        }
        if (attempt.http_content_type) facts.push(esc(attempt.http_content_type));
        if (attempt.error) facts.push(esc(attempt.error));
        if (attempt.reason) facts.push(esc(attempt.reason));
        return `<tr data-attempt="${attempt.attempt_number}">
          <td><button type="button" class="link attempt-tab">${attempt.attempt_number}</button></td>
          <td>${C.chip(attempt.state, C.stateClass(attempt.state), C.ACTIVE_STATES.has(attempt.state))}</td>
          <td>${C.fmtInstant(attempt.started_at_us) || "—"}</td>
          <td>${C.fmtInstant(attempt.running_at_us) || "—"}</td>
          <td>${C.fmtInstant(attempt.finished_at_us) || "—"}</td>
          <td>${C.formatDuration(attempt.duration_us)}</td>
          <td>${facts.length ? facts.join("<br>") : "—"}</td>
        </tr>`;
      })
      .join("");

    const whyBlock = whyData
      ? `<section class="card">
          <h2>Why</h2>
          <p class="muted">${esc(whyData.explanation)}</p>
          <p>Daemon: ${C.chip(whyData.daemon_running ? "running" : "not running", whyData.daemon_running ? "state-good" : "state-bad", false)}</p>
          <table class="events-table">
            <thead><tr><th>Time</th><th>Event</th><th>Details</th></tr></thead>
            <tbody>${(whyData.events || [])
              .map(
                (event) => `<tr>
                  <td>${C.dualTime(event.occurred_at_us, now)}</td>
                  <td>${C.eventBadge(event.kind)}</td>
                  <td>${C.redactedValue(event.details_json)}</td>
                </tr>`,
              )
              .join("")}</tbody>
          </table>
        </section>`
      : "";

    const snapshot = parseSnapshot(run);
    return `<div class="page-head">
        <h1>Run ${esc(run.id.slice(0, 8))}</h1>
        <div class="actions">
          <button id="run-cancel" type="button" class="button danger">Cancel run</button>
          <button id="run-refresh" type="button" class="button">Refresh</button>
        </div>
      </div>
      ${C.outcomeChip(run)}
      <span class="badge badge-neutral">${esc(run.trigger)}</span>
      ${run.reason ? `<span class="muted">${esc(run.reason)}</span>` : ""}
      <dl class="facts">
        <dt>Requested</dt><dd>${C.dualTime(run.requested_at_us, now)}</dd>
        <dt>Nominal</dt><dd>${C.fmtInstant(run.nominal_us) || "—"}</dd>
        <dt>Eligible</dt><dd>${C.fmtInstant(run.eligible_at_us) || "—"}</dd>
        <dt>Started</dt><dd>${C.fmtInstant(run.actual_started_at_us) || "—"}</dd>
        <dt>Finished</dt><dd>${C.fmtInstant(run.finished_at_us) || "—"}</dd>
        <dt>Duration</dt><dd>${C.formatDuration(run.duration_us)}</dd>
        <dt>Run id</dt><dd><code>${esc(run.id)}</code></dd>
      </dl>
      <section class="card">
        <h2>Attempts</h2>
        ${attempts.length ? `<table class="attempts-table">
          <thead><tr><th>#</th><th>State</th><th>Admitted</th><th>Running</th><th>Finished</th><th>Duration</th><th>Facts</th></tr></thead>
          <tbody>${attemptsRows}</tbody>
        </table>` : '<p class="muted">no attempts yet — the run has not been admitted</p>'}
      </section>
      <section class="card">
        <h2>Output</h2>
        <div class="console-toolbar">
          <button id="console-live" type="button" class="button primary">${active ? "Follow live" : "Replay stream"}</button>
          <button id="console-attempt" type="button" class="button">Load attempt output</button>
          <span id="console-status" class="muted"></span>
        </div>
        <pre id="console" class="console" aria-live="polite"></pre>
      </section>
      ${whyBlock}
      ${snapshot ? `<details class="card"><summary>Redacted snapshot JSON</summary><pre class="json-block">${esc(JSON.stringify(snapshot, null, 2))}</pre></details>` : ""}`;
  }

  function parseSnapshot(run) {
    if (!run.snapshot_json) return null;
    try {
      return JSON.parse(run.snapshot_json);
    } catch {
      return null;
    }
  }

  function bindDetail(view, run) {
    const refresh = view.querySelector("#run-refresh");
    const cancel = view.querySelector("#run-cancel");
    const status = view.querySelector("#console-status");
    const active = C.ACTIVE_STATES.has(run.state);

    if (!active) {
      cancel.disabled = true;
      cancel.title = "run is already terminal";
    }
    cancel.addEventListener("click", async () => {
      if (!window.confirm(`Request cancellation of run ${run.id.slice(0, 8)}?`)) return;
      try {
        const { data } = await Api.post(
          `/api/v1/runs/${encodeURIComponent(run.id)}/cancel`,
        );
        status.textContent = JSON.stringify(data);
      } catch (error) {
        status.textContent = error.message || String(error);
      }
    });
    refresh.addEventListener("click", () => Router.navigate(`#/runs/${encodeURIComponent(run.id)}`));
  }

  /// The console: one static tab per attempt (finalized frames from the
  /// logs endpoint) plus the live stream tab (SSE replay + live output).
  function renderConsole(view, run) {
    const console = view.querySelector("#console");
    const liveButton = view.querySelector("#console-live");
    const attemptButton = view.querySelector("#console-attempt");
    const status = view.querySelector("#console-status");
    const attempts = (run.attempts || []).map((attempt) => attempt.attempt_number);
    const selected = { attempt: attempts.length ? attempts[0] : 1 };
    let stream = null;

    function line(text, className) {
      const div = document.createElement("div");
      div.className = className || "log-line";
      div.innerHTML = text;
      console.appendChild(div);
    }

    function logElapsed(us) {
      const seconds = us / 1e6;
      return `${seconds >= 0 ? "+" : ""}${seconds.toFixed(1)}s`;
    }

    function closeStream() {
      if (stream) {
        stream.close();
        stream = null;
      }
      liveButton.disabled = false;
    }

    liveButton.addEventListener("click", () => {
      if (stream) {
        closeStream();
        liveButton.textContent = "Follow live";
        return;
      }
      console.innerHTML = "";
      liveButton.disabled = true;
      liveButton.textContent = "Stop following";
      status.textContent = "connecting…";
      stream = RunStream.open(run.id, {
        onOpen: () => {
          status.textContent = "connected — replaying, then live";
        },
        onError: (ended) => {
          if (ended) return;
          status.textContent = "connection lost — retrying…";
        },
        onEnd: (termination) => {
          status.textContent = `terminated: ${termination.state}`;
          closeStream();
        },
      });
      stream
        .on("run", (event) => {
          line(`${C.stateChip(event.state)} <span class="muted">state</span>`, "log-event");
        })
        .on("attempt", (event) => {
          line(
            `attempt ${event.attempt_number}: ${C.stateChip(event.state)}`,
            "log-event",
          );
        })
        .on("output", (event) => {
          const text = C.decodeBase64(event.data_b64);
          const channel = event.channel === "stderr" ? "stderr" : "stdout";
          line(
            `<span class="log-elapsed">${logElapsed(event.elapsed_us)}</span>` +
              `<span class="log-channel ${channel}">${channel}</span> ` +
              C.ansiToHtml(text),
            "log-line",
          );
        })
        .on("termination", (event) => {
          const reason = event.reason ? ` — ${esc(event.reason)}` : "";
          line(`run terminated: ${esc(event.state)}${reason}`, "log-terminal");
        });
    });

    attemptButton.addEventListener("click", async () => {
      try {
        const { data } = await Api.get(
          `/api/v1/runs/${encodeURIComponent(run.id)}/logs?attempt=${selected.attempt}&channel=all`,
        );
        console.innerHTML = "";
        for (const frame of data.frames) {
          const text = C.decodeBase64(frame.bytes);
          const channel = frame.channel === "stderr" ? "stderr" : "stdout";
          line(
            `<span class="log-elapsed">${logElapsed(frame.elapsed_micros)}</span>` +
              `<span class="log-channel ${channel}">${channel}</span> ` +
              C.ansiToHtml(text),
            "log-line",
          );
        }
        status.textContent = data.frames.length
          ? `attempt ${data.attempt}: ${data.frames.length} frames`
          : `attempt ${data.attempt}: no output frames`;
      } catch (error) {
        status.textContent =
          error.message === "output not found"
            ? "attempt output is not finalized yet — use Follow live"
            : error.message || String(error);
      }
    });

    for (const tab of view.querySelectorAll("button.attempt-tab")) {
      tab.addEventListener("click", () => {
        selected.attempt = Number(tab.closest("tr").dataset.attempt);
        status.textContent = `attempt ${selected.attempt} selected for "Load attempt output"`;
      });
    }
  }

  Router.register("/runs", renderHistory);
  Router.register("/runs/:id", renderDetail);
})();
