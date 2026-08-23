// Jobs view: list with per-row next occurrence and last outcome, job detail
// with why facts and recent runs, and create/edit forms with schedule preview
// and dry-run against the same validation the CLI uses.
"use strict";

const JobsView = (() => {
  const C = Components;
  const esc = C.escapeHtml;
  const attr = C.escapeAttr;

  function parseDefinition(job) {
    if (!job || !job.definition_json) return null;
    try {
      return JSON.parse(job.definition_json);
    } catch {
      return null;
    }
  }

  /// The wire job record keeps `tags_json` as a JSON string.
  function jobTags(job) {
    if (!job || !job.tags_json) return [];
    try {
      return JSON.parse(job.tags_json);
    } catch {
      return [];
    }
  }

  function jobHref(job) {
    return `#/jobs/${encodeURIComponent(job.name)}`;
  }

  // ---------------------------------------------------------------------
  // List
  // ---------------------------------------------------------------------

  async function renderList(view) {
    view.innerHTML = `<div class="page-head">
      <h1>Jobs</h1>
      <div class="actions">
        <input id="jobs-search" type="search" placeholder="search name, tag, description…">
        <select id="jobs-filter">
          <option value="all">all states</option>
          <option value="enabled">enabled</option>
          <option value="disabled">disabled</option>
        </select>
        <button id="jobs-refresh" type="button">Refresh</button>
        <a class="button primary" href="#/jobs/new">New job</a>
      </div>
    </div>
    <div id="jobs-body" aria-busy="true">${C.emptyState("Loading jobs", "reading durable job records…")}</div>`;

    const body = view.querySelector("#jobs-body");
    const search = view.querySelector("#jobs-search");
    const filter = view.querySelector("#jobs-filter");
    const refresh = view.querySelector("#jobs-refresh");

    async function load() {
      try {
        const { data: jobs } = await Api.get("/api/v1/jobs");
        body.setAttribute("aria-busy", "false");
        renderTable(body, jobs);
        fillPerJob(jobs);
      } catch (error) {
        body.setAttribute("aria-busy", "false");
        body.innerHTML = C.errorBlock(error);
      }
    }

    function applyFilters() {
      const query = search.value.trim().toLowerCase();
      const wanted = filter.value;
      for (const row of body.querySelectorAll("tbody tr[data-name]")) {
        const haystack = row.dataset.haystack || "";
        const enabled = row.dataset.enabled === "true";
        const matches =
          (!query || haystack.includes(query)) &&
          (wanted === "all" || (wanted === "enabled") === enabled);
        row.hidden = !matches;
      }
    }

    search.addEventListener("input", applyFilters);
    filter.addEventListener("change", applyFilters);
    refresh.addEventListener("click", load);
    await load();
  }

  function renderTable(body, jobs) {
    if (jobs.length === 0) {
      body.innerHTML = C.emptyState(
        "No jobs",
        "Create a job to start scheduling work.",
        '<a class="button primary" href="#/jobs/new">Create the first job</a>',
      );
      return;
    }
    const rows = jobs
      .map((job) => {
        const haystack = `${job.name} ${job.description || ""} ${jobTags(job).join(" ")}`.toLowerCase();
        const data = {
          "data-name": job.name,
          "data-enabled": String(job.enabled),
          "data-haystack": attr(haystack),
        };
        return `<tr ${Object.entries(data)
          .map(([key, value]) => `${key}="${value}"`)
          .join(" ")}>
        <td class="job-name">
          <a href="${jobHref(job)}">${esc(job.name)}</a>
          <div class="sub">${esc(jobTags(job).join(" · ") || "no tags")}</div>
        </td>
        <td class="job-schedule" data-cell="schedule"></td>
        <td class="job-next" data-cell="next"><span class="muted">…</span></td>
        <td class="job-last" data-cell="last"><span class="muted">…</span></td>
        <td class="job-actions" data-cell="actions"></td>
      </tr>`;
      })
      .join("");
    body.innerHTML = `<table class="jobs-table">
      <thead><tr>
        <th>Job</th><th>Schedule</th><th>Next</th><th>Last run</th><th></th>
      </tr></thead>
      <tbody>${rows}</tbody>
    </table>`;
    for (const job of jobs) {
      const row = body.querySelector(`tr[data-name="${CSS.escape(job.name)}"]`);
      if (!row) continue;
      const definition = parseDefinition(job);
      row.querySelector('[data-cell="schedule"]').innerHTML = definition
        ? C.scheduleSummary(definition)
        : '<span class="muted">unreadable definition</span>';
      row.querySelector('[data-cell="actions"]').innerHTML = listActions(job);
    }
  }

  function listActions(job) {
    const enabled = job.enabled ? "disabled" : "enabled";
    return `<div class="row-actions">
      <button type="button" class="link" data-action="run">run now</button>
      <button type="button" class="link" data-action="toggle">${enabled}</button>
      <a href="#/jobs/${encodeURIComponent(job.name)}/edit">edit</a>
      <button type="button" class="link danger" data-action="remove">remove</button>
    </div>`;
  }

  /// Per-row async fills: next occurrence (preview) and last outcome
  /// (runs history), in parallel, updating cells as each lands.
  function fillPerJob(jobs) {
    for (const job of jobs) {
      const row = document.querySelector(
        `#jobs-body tr[data-name="${CSS.escape(job.name)}"]`,
      );
      if (!row) continue;
      const nextCell = row.querySelector('[data-cell="next"]');
      const lastCell = row.querySelector('[data-cell="last"]');
      const preview = Api.get(
        `/api/v1/jobs/${encodeURIComponent(job.id)}/preview?count=1`,
      )
        .then(({ data }) => {
          const occurrence = data.occurrences && data.occurrences[0];
          const stamp = occurrence
            ? `<time data-t="${attr(occurrence)}" title="${attr(occurrence)}">${esc(occurrence)}</time>`
            : '<span class="muted">no future occurrence</span>';
          if (nextCell) nextCell.innerHTML = stamp;
        })
        .catch(() => {
          if (nextCell) nextCell.innerHTML = '<span class="muted">unavailable</span>';
        });
      const last = Api.get(
        `/api/v1/runs?job=${encodeURIComponent(job.name)}&limit=1`,
      )
        .then(({ data }) => {
          const run = data.runs && data.runs[0];
          if (lastCell) {
            lastCell.innerHTML = run
              ? runSummary(run)
              : '<span class="muted">no runs yet</span>';
          }
        })
        .catch(() => {
          if (lastCell) lastCell.innerHTML = '<span class="muted">unavailable</span>';
        });
      void Promise.allSettled([preview, last]).then(() => {
        const actions = row.querySelector('[data-cell="actions"]');
        if (actions) bindListActions(actions, job);
      });
    }
  }

  function runSummary(run) {
    const now = C.nowUs();
    return (
      C.outcomeChip(run) +
      " " +
      `<span title="${attr(C.fmtInstant(run.requested_at_us))}">${esc(
        C.relTime(run.requested_at_us, now),
      )}</span> ` +
      C.formatDuration(run.duration_us)
    );
  }

  function bindListActions(actions, job) {
    for (const button of actions.querySelectorAll("button[data-action]")) {
      button.addEventListener("click", async () => {
        const action = button.dataset.action;
        try {
          if (action === "run") {
            if (!window.confirm(`Start a manual run of "${job.name}" now?`)) return;
            const { data } = await Api.post(`/api/v1/jobs/${encodeURIComponent(job.id)}/run`);
            Router.navigate(`#/runs/${encodeURIComponent(data.run_id)}`);
          } else if (action === "toggle") {
            await Api.post(
              `/api/v1/jobs/${encodeURIComponent(job.id)}/${job.enabled ? "disable" : "enable"}`,
            );
            Router.navigate("#/jobs");
          } else if (action === "remove") {
            if (!window.confirm(`Remove job "${job.name}"? Runs already finished stay in history.`)) return;
            await Api.del(`/api/v1/jobs/${encodeURIComponent(job.id)}`);
            Router.navigate("#/jobs");
          }
        } catch (error) {
          window.alert(error.message || String(error));
        }
      });
    }
  }

  // ---------------------------------------------------------------------
  // Detail
  // ---------------------------------------------------------------------

  async function renderDetail(view, params) {
    const reference = params.id;
    view.innerHTML = `<div aria-busy="true">${C.emptyState("Loading job", "reading durable records…")}</div>`;
    try {
      const [{ data: job }, why, recent] = await Promise.all([
        Api.get(`/api/v1/jobs/${encodeURIComponent(reference)}`),
        Api.get(`/api/v1/jobs/${encodeURIComponent(reference)}/why`).catch((error) => ({ error })),
        Api.get(`/api/v1/runs?job=${encodeURIComponent(reference)}&limit=5`).catch(() => ({ data: { runs: [] } })),
      ]);
      const definition = parseDefinition(job);
      view.innerHTML = detailHtml(job, definition, why, recent);
      bindDetailActions(view, job);
    } catch (error) {
      view.innerHTML = C.errorBlock(error);
    }
  }

  function detailHtml(job, definition, why, recent) {
    const now = C.nowUs();
    const whyData = why && !why.error ? why.data : null;
    const whyEvents = whyData ? whyData.active_runs || [] : [];
    const runs = recent && recent.data ? recent.data.runs || [] : [];
    const whyBlock = whyData
      ? `<section class="card">
          <h2>Why</h2>
          <p class="muted">${esc(whyData.explanation)}</p>
          <dl class="facts">
            <dt>Next occurrence</dt><dd>${whyData.next_occurrence ? esc(whyData.next_occurrence) : '<span class="muted">none</span>'}</dd>
            <dt>Overlap policy</dt><dd>${esc(whyData.overlap)}</dd>
            <dt>Daemon</dt><dd>${C.chip(whyData.daemon_running ? "running" : "not running", whyData.daemon_running ? "state-good" : "state-bad", false)}</dd>
          </dl>
          ${whyEvents.length ? `<h3>Active runs</h3>${runsTable(whyEvents)}` : ""}
        </section>`
      : "";
    return `<div class="page-head">
        <h1>${esc(job.name)}</h1>
        <div class="actions">
          <button id="job-run" type="button" class="button">Run now</button>
          <a class="button" href="#/jobs/${encodeURIComponent(job.name)}/edit">Edit</a>
          <button id="job-toggle" type="button" class="button">${job.enabled ? "Disable" : "Enable"}</button>
          <button id="job-remove" type="button" class="button danger">Remove</button>
        </div>
      </div>
      ${C.enabledChip(job.enabled)}
      <span class="muted">revision ${esc(String(job.current_revision))} · id ${esc(job.id)}</span>
      ${job.description ? `<p>${esc(job.description)}</p>` : ""}
      ${definition ? definitionCards(definition, now) : '<div class="error-block">definition is unreadable</div>'}
      ${whyBlock}
      <section class="card">
        <h2>Recent runs</h2>
        ${runs.length ? runsTable(runs) : '<p class="muted">no runs yet</p>'}
      </section>
      <details class="card">
        <summary>Redacted definition JSON</summary>
        <pre class="json-block">${esc(JSON.stringify(definition, null, 2))}</pre>
      </details>`;
  }

  function definitionCards(definition, now) {
    const schedule = definition.schedule || {};
    const target = definition.target || {};
    const environment = definition.environment || {};
    const policy = definition.policy || {};

    const scheduleFacts = [];
    if (schedule.kind === "cron") {
      scheduleFacts.push(["Expression", schedule.expression]);
      const tz = schedule.timezone || {};
      scheduleFacts.push(["Timezone", tz.mode === "iana" ? tz.name : "local"]);
    } else if (schedule.kind === "every") {
      scheduleFacts.push(["Interval", C.formatDuration(schedule.interval)]);
      scheduleFacts.push(["Anchor", C.fmtInstant(schedule.anchor)]);
    } else if (schedule.kind === "at") {
      scheduleFacts.push(["At", C.fmtInstant(schedule.at)]);
    } else {
      scheduleFacts.push(["Kind", schedule.kind || "unknown"]);
    }

    const targetFacts = [];
    if (target.kind === "process") {
      targetFacts.push(["Executable", target.executable]);
      if ((target.args || []).length) targetFacts.push(["Args", target.args.join(" ")]);
    } else if (target.kind === "shell") {
      targetFacts.push(["Shell", target.shell]);
      targetFacts.push(["Command", target.command]);
    } else if (target.kind === "http") {
      targetFacts.push(["Method", target.method]);
      targetFacts.push(["URL", target.url]);
      if ((target.success_statuses || []).length) {
        targetFacts.push(["Success statuses", target.success_statuses.join(", ")]);
      }
      targetFacts.push(["Follow redirects", String(target.follow_redirects)]);
      if (target.body !== null && target.body !== undefined) {
        targetFacts.push(["Body", "<redacted>"]);
      }
      if (target.body_file) targetFacts.push(["Body file", target.body_file]);
      for (const [name, source] of Object.entries(target.headers || {})) {
        targetFacts.push([`Header ${name}`, `{source: ${source.source}, value: <redacted>}`]);
      }
    }

    const policyFacts = [
      ["Overlap", policy.overlap],
      ["Missed run", policy.missed_run],
      ["Catch-up limit", String(policy.catch_up_limit)],
      ["Retries", String(policy.retries)],
      ["Retry delay", C.formatDuration(policy.retry_delay)],
      ["Retry cap", C.formatDuration(policy.retry_cap)],
      ["Backoff", policy.backoff],
      ["Retry timeout", String(policy.retry_timeout)],
      ["Timeout", policy.timeout === null || policy.timeout === undefined ? "off" : C.formatDuration(policy.timeout)],
      ["Start deadline", policy.start_deadline === null || policy.start_deadline === undefined ? "off" : C.formatDuration(policy.start_deadline)],
      ["Termination grace", C.formatDuration(policy.termination_grace)],
      ["Per-job concurrency", String(policy.per_job_concurrency)],
    ];

    const envValues = Object.entries(environment.values || {});
    const envFacts = [
      ["File", environment.file || "none"],
      ["PATH override", environment.path || "none"],
      ["Values", envValues.length ? Object.keys(environment.values).join(", ") : "none"],
    ];

    const dl = (facts) =>
      `<dl class="facts">${facts
        .map(([key, value]) => `<dt>${esc(key)}</dt><dd>${C.redactedValue(value)}</dd>`)
        .join("")}</dl>`;

    return `<section class="card"><h2>Schedule</h2>${dl(scheduleFacts)}</section>
      <section class="card"><h2>Target</h2>${dl(targetFacts)}</section>
      <section class="card"><h2>Working directory</h2><code>${esc(definition.cwd)}</code></section>
      <section class="card"><h2>Environment</h2>${dl(envFacts)}
        ${envValues.length ? envTable(environment.values) : ""}
      </section>
      <section class="card"><h2>Policy</h2>${dl(policyFacts)}</section>`;
  }

  function envTable(values) {
    return `<table class="env-table">
      <thead><tr><th>Name</th><th>Value</th></tr></thead>
      <tbody>${Object.entries(values)
        .map(
          ([name, value]) =>
            `<tr><td>${esc(name)}</td><td>${C.redactedValue(value)}</td></tr>`,
        )
        .join("")}</tbody>
    </table>`;
  }

  function runsTable(runs) {
    const now = C.nowUs();
    return `<table class="runs-table">
      <thead><tr><th>Run</th><th>Requested</th><th>State</th><th>Duration</th><th>Attempts</th></tr></thead>
      <tbody>${runs
        .map(
          (run) => `<tr>
            <td><a href="#/runs/${encodeURIComponent(run.id)}">${esc(run.id.slice(0, 8))}</a>
              <div class="sub">${esc(run.trigger)}${run.nominal_us ? ` · ${C.fmtInstant(run.nominal_us)}` : ""}</div></td>
            <td>${C.dualTime(run.requested_at_us, now)}</td>
            <td>${C.outcomeChip(run)}</td>
            <td>${C.formatDuration(run.duration_us)}</td>
            <td>${C.attemptStrip(run, true)}</td>
          </tr>`,
        )
        .join("")}</tbody>
    </table>`;
  }

  function bindDetailActions(view, job) {
    const run = view.querySelector("#job-run");
    const toggle = view.querySelector("#job-toggle");
    const remove = view.querySelector("#job-remove");
    run.addEventListener("click", async () => {
      try {
        if (!window.confirm(`Start a manual run of "${job.name}" now?`)) return;
        const { data } = await Api.post(`/api/v1/jobs/${encodeURIComponent(job.id)}/run`);
        Router.navigate(`#/runs/${encodeURIComponent(data.run_id)}`);
      } catch (error) {
        window.alert(error.message || String(error));
      }
    });
    toggle.addEventListener("click", async () => {
      try {
        await Api.post(
          `/api/v1/jobs/${encodeURIComponent(job.id)}/${job.enabled ? "disable" : "enable"}`,
        );
        Router.navigate(`#/jobs/${encodeURIComponent(job.name)}`);
      } catch (error) {
        window.alert(error.message || String(error));
      }
    });
    remove.addEventListener("click", async () => {
      try {
        if (!window.confirm(`Remove job "${job.name}"? Finished runs stay in history.`)) return;
        await Api.del(`/api/v1/jobs/${encodeURIComponent(job.id)}`);
        Router.navigate("#/jobs");
      } catch (error) {
        window.alert(error.message || String(error));
      }
    });
  }

  // ---------------------------------------------------------------------
  // Create / edit form
  // ---------------------------------------------------------------------

  async function renderForm(view, params) {
    const reference = params.id;
    const editing = Boolean(reference);
    let job = null;
    if (editing) {
      view.innerHTML = `<div aria-busy="true">${C.emptyState("Loading job", "reading the durable definition…")}</div>`;
      try {
        const { data } = await Api.get(`/api/v1/jobs/${encodeURIComponent(reference)}`);
        job = data;
      } catch (error) {
        view.innerHTML = C.errorBlock(error);
        return;
      }
    }
    const definition = job
      ? {
          ...parseDefinition(job),
          name: job.name,
          description: job.description,
          tags: jobTags(job),
          enabled: job.enabled,
        }
      : blankDefinition();
    const base =
      job && job.name ? job.name : params.id ? params.id : "";
    view.innerHTML = formHtml(editing, base);
    populateForm(view, definition);
    bindForm(view, editing, base);
  }

  function blankDefinition() {
    const now = C.nowUs();
    return {
      schedule: { kind: "cron", expression: "", timezone: { mode: "local" } },
      target: { kind: "process", executable: "", args: [] },
      cwd: "",
      environment: { values: {} },
      policy: {
        overlap: "skip",
        missed_run: "skip",
        start_deadline: null,
        catch_up_limit: 100,
        retries: 0,
        retry_delay: 10000000,
        retry_cap: 300000000,
        backoff: "exponential",
        retry_timeout: false,
        timeout: 60000000,
        termination_grace: 5000000,
        per_job_concurrency: 1,
      },
    };
  }

  const SECRET = "<redacted>";

  function formHtml(editing, base) {
    const title = editing ? `Edit job ${esc(base)}` : "New job";
    return `<div class="page-head"><h1>${title}</h1></div>
      <form id="job-form" autocomplete="off">
        ${editing ? secretNotice(base) : ""}
        <section class="card">
          <h2>Identity</h2>
          <label>Name *<input id="jf-name" type="text" required spellcheck="false"></label>
          <label>Description<textarea id="jf-description" rows="2"></textarea></label>
          <label>Tags<input id="jf-tags" type="text" placeholder="tag1, tag2" spellcheck="false"></label>
          <label class="check"><input id="jf-enabled" type="checkbox" checked> enabled</label>
        </section>
        <section class="card">
          <h2>Schedule</h2>
          <label>Kind
            <select id="jf-sched-kind">
              <option value="cron">cron</option>
              <option value="every">every</option>
              <option value="at">at</option>
            </select>
          </label>
          <div id="jf-sched-cron">
            <label>Expression *<input id="jf-cron-expression" type="text" placeholder="*/5 * * * *" spellcheck="false"></label>
            <label>Timezone
              <select id="jf-cron-tz">
                <option value="local">local</option>
                <option value="iana">IANA</option>
              </select>
            </label>
            <label id="jf-cron-tz-name-wrap" hidden>Timezone name<input id="jf-cron-tz-name" type="text" placeholder="Europe/Berlin" spellcheck="false"></label>
          </div>
          <div id="jf-sched-every" hidden>
            <label>Interval (microseconds) *<input id="jf-every-interval" type="number" min="1" step="1"></label>
            <label>Anchor (epoch microseconds) *<input id="jf-every-anchor" type="number" step="1"></label>
            <button id="jf-anchor-now" type="button" class="link">use now</button>
          </div>
          <div id="jf-sched-at" hidden>
            <label>At (epoch microseconds) *<input id="jf-at-at" type="number" step="1"></label>
            <button id="jf-at-now" type="button" class="link">use now</button>
          </div>
          <button id="jf-preview" type="button" class="button">Preview next 5 occurrences</button>
          <ol id="jf-preview-list" class="preview-list muted" hidden></ol>
        </section>
        <section class="card">
          <h2>Target</h2>
          <label>Kind
            <select id="jf-target-kind">
              <option value="process">process</option>
              <option value="shell">shell</option>
              <option value="http">http</option>
            </select>
          </label>
          <div id="jf-target-process">
            <label>Executable *<input id="jf-process-exec" type="text" placeholder="/usr/bin/echo" spellcheck="false"></label>
            <label>Arguments (one per line)<textarea id="jf-process-args" rows="3" spellcheck="false"></textarea></label>
          </div>
          <div id="jf-target-shell" hidden>
            <label>Command *<textarea id="jf-shell-command" rows="3" spellcheck="false"></textarea></label>
            <label>Shell (absolute path) *<input id="jf-shell-path" type="text" placeholder="/bin/sh" spellcheck="false"></label>
          </div>
          <div id="jf-target-http" hidden>
            <label>Method
              <select id="jf-http-method">
                <option>GET</option><option>POST</option><option>PUT</option>
                <option>PATCH</option><option>DELETE</option><option>HEAD</option><option>OPTIONS</option>
              </select>
            </label>
            <label>URL *<input id="jf-http-url" type="text" placeholder="https://example.com/hook" spellcheck="false"></label>
            <label>Success statuses<input id="jf-http-statuses" type="text" placeholder="200, 201" spellcheck="false"></label>
            <label class="check"><input id="jf-http-redirects" type="checkbox" checked> follow redirects</label>
            <label>Body (inline)<textarea id="jf-http-body" rows="3" spellcheck="false" placeholder="empty means no body"></textarea></label>
            <label>Body file (absolute path)<input id="jf-http-body-file" type="text" placeholder="instead of inline body" spellcheck="false"></label>
            <h3>Headers</h3>
            <div id="jf-headers"></div>
            <button id="jf-add-header" type="button" class="link">add header</button>
          </div>
        </section>
        <section class="card">
          <h2>Working directory</h2>
          <label>cwd *<input id="jf-cwd" type="text" placeholder="/absolute/path" spellcheck="false"></label>
        </section>
        <section class="card">
          <h2>Environment</h2>
          <label>File (absolute path)<input id="jf-env-file" type="text" spellcheck="false"></label>
          <label>PATH override<input id="jf-env-path" type="text" spellcheck="false"></label>
          <h3>Values</h3>
          <div id="jf-env-values"></div>
          <button id="jf-add-env" type="button" class="link">add value</button>
        </section>
        <section class="card">
          <h2>Policy</h2>
          <label>Overlap
            <select id="jf-policy-overlap">
              <option value="skip">skip</option><option value="replace">replace</option><option value="allow">allow</option>
            </select>
          </label>
          <label>Missed run
            <select id="jf-policy-missed">
              <option value="skip">skip</option><option value="latest">latest</option><option value="all">all</option>
            </select>
          </label>
          <label>Start deadline (µs, empty = off)<input id="jf-policy-deadline" type="number" min="0" step="1"></label>
          <label>Catch-up limit<input id="jf-policy-catchup" type="number" min="1" max="1000" step="1"></label>
          <label>Retries<input id="jf-policy-retries" type="number" min="0" max="10" step="1"></label>
          <label>Retry delay (µs)<input id="jf-policy-retry-delay" type="number" min="0" step="1"></label>
          <label>Retry cap (µs)<input id="jf-policy-retry-cap" type="number" min="0" step="1"></label>
          <label>Backoff
            <select id="jf-policy-backoff">
              <option value="fixed">fixed</option><option value="exponential">exponential</option>
            </select>
          </label>
          <label class="check"><input id="jf-policy-retry-timeout" type="checkbox"> retry timed-out attempts</label>
          <label>Timeout (µs, empty = off)<input id="jf-policy-timeout" type="number" min="0" step="1"></label>
          <label>Termination grace (µs)<input id="jf-policy-grace" type="number" min="0" step="1"></label>
          <label>Per-job concurrency<input id="jf-policy-concurrency" type="number" min="1" max="64" step="1"></label>
        </section>
        <div class="form-actions">
          <button id="jf-dry-run" type="button" class="button">Dry-run</button>
          <button id="jf-save" type="submit" class="button primary">Save</button>
          <a class="button" href="#/jobs">Cancel</a>
        </div>
        <div id="jf-result" hidden></div>
      </form>`;
  }

  function secretNotice(name) {
    return `<div class="notice">
      Secret values (environment values, inline header values, and HTTP bodies) are never
      displayed or sent back. Each redacted value must be explicitly replaced or removed before
      saving, or loaded with the acknowledged plaintext button.
    </div>
    <div class="secret-loader">
      <label class="check"><input id="jf-load-values" type="checkbox">
        I acknowledge that secret values for this job will be visible in this browser session
      </label>
      <button id="jf-load-values-btn" type="button" class="button">Load current values for editing</button>
      <span id="jf-load-values-status" class="muted"></span>
    </div>`;
  }

  /// Fills every static field; secret rows render as unresolved redacted rows
  /// (edit) or empty rows (create).
  function populateForm(view, definition) {
    const q = (id) => view.querySelector(`#${id}`);
    q("jf-name").value = definition.name || "";
    q("jf-description").value = definition.description || "";
    q("jf-tags").value = (definition.tags || []).join(", ");
    q("jf-enabled").checked = definition.enabled !== false;

    const schedule = definition.schedule || {};
    q("jf-sched-kind").value = schedule.kind || "cron";
    syncScheduleKind(view);
    if (schedule.kind === "cron") {
      q("jf-cron-expression").value = schedule.expression || "";
      const tz = schedule.timezone || {};
      if (tz.mode === "iana") {
        q("jf-cron-tz").value = "iana";
        syncCronTz(view);
        q("jf-cron-tz-name").value = tz.name || "";
      }
    } else if (schedule.kind === "every") {
      q("jf-every-interval").value = schedule.interval ?? "";
      q("jf-every-anchor").value = schedule.anchor ?? "";
    } else if (schedule.kind === "at") {
      q("jf-at-at").value = schedule.at ?? "";
    }

    const target = definition.target || {};
    q("jf-target-kind").value = target.kind || "process";
    syncTargetKind(view);
    if (target.kind === "process") {
      q("jf-process-exec").value = target.executable || "";
      q("jf-process-args").value = (target.args || []).join("\n");
    } else if (target.kind === "shell") {
      q("jf-shell-command").value = target.command || "";
      q("jf-shell-path").value = target.shell || "";
    } else if (target.kind === "http") {
      q("jf-http-method").value = target.method || "GET";
      q("jf-http-url").value = target.url || "";
      q("jf-http-statuses").value = (target.success_statuses || []).join(", ");
      q("jf-http-redirects").checked = target.follow_redirects !== false;
      const body = target.body;
      if (body !== null && body !== undefined) {
        if (body === SECRET) {
          q("jf-http-body").dataset.redacted = "1";
          q("jf-http-body").placeholder = "body is redacted — empty keeps nothing; re-enter to replace";
        } else {
          q("jf-http-body").value = body;
        }
      }
      q("jf-http-body-file").value = target.body_file || "";
      for (const [name, source] of Object.entries(target.headers || {})) {
        addHeaderRow(view, name, source);
      }
    }

    q("jf-cwd").value = definition.cwd || "";

    const environment = definition.environment || {};
    q("jf-env-file").value = environment.file || "";
    q("jf-env-path").value = environment.path || "";
    for (const [name, value] of Object.entries(environment.values || {})) {
      addEnvRow(view, name, value);
    }

    const policy = definition.policy || {};
    q("jf-policy-overlap").value = policy.overlap || "skip";
    q("jf-policy-missed").value = policy.missed_run || "skip";
    q("jf-policy-deadline").value = policy.start_deadline ?? "";
    q("jf-policy-catchup").value = policy.catch_up_limit ?? "";
    q("jf-policy-retries").value = policy.retries ?? "";
    q("jf-policy-retry-delay").value = policy.retry_delay ?? "";
    q("jf-policy-retry-cap").value = policy.retry_cap ?? "";
    q("jf-policy-backoff").value = policy.backoff || "exponential";
    q("jf-policy-retry-timeout").checked = Boolean(policy.retry_timeout);
    q("jf-policy-timeout").value = policy.timeout ?? "";
    q("jf-policy-grace").value = policy.termination_grace ?? "";
    q("jf-policy-concurrency").value = policy.per_job_concurrency ?? "";
  }

  function syncScheduleKind(view) {
    const kind = view.querySelector("#jf-sched-kind").value;
    view.querySelector("#jf-sched-cron").hidden = kind !== "cron";
    view.querySelector("#jf-sched-every").hidden = kind !== "every";
    view.querySelector("#jf-sched-at").hidden = kind !== "at";
  }

  function syncCronTz(view) {
    const mode = view.querySelector("#jf-cron-tz").value;
    view.querySelector("#jf-cron-tz-name-wrap").hidden = mode !== "iana";
  }

  function syncTargetKind(view) {
    const kind = view.querySelector("#jf-target-kind").value;
    view.querySelector("#jf-target-process").hidden = kind !== "process";
    view.querySelector("#jf-target-shell").hidden = kind !== "shell";
    view.querySelector("#jf-target-http").hidden = kind !== "http";
  }

  /// One environment row; redacted values are unresolved until "replace"
  /// (reveals an input) or "remove" is chosen.
  function addEnvRow(view, name, value) {
    const container = view.querySelector("#jf-env-values");
    const row = document.createElement("div");
    row.className = "kv-row";
    const redacted = value === SECRET;
    const resolution = redacted
      ? `<select class="kv-resolve">
           <option value="">choose…</option>
           <option value="replace">replace</option>
           <option value="remove">remove</option>
         </select>
         <input class="kv-value" type="password" placeholder="new value (redacted values are never shown)" autocomplete="off" hidden>`
      : `<input class="kv-value" type="text" value="${attr(value)}" spellcheck="false">`;
    row.innerHTML =
      `<input class="kv-name" type="text" value="${attr(name || "")}" placeholder="NAME" spellcheck="false">` +
      resolution +
      `<button type="button" class="link danger kv-remove">remove</button>`;
    container.appendChild(row);
    if (redacted) {
      row.dataset.redacted = "1";
      row.querySelector(".kv-resolve").addEventListener("change", (event) => {
        const input = row.querySelector(".kv-value");
        input.hidden = event.target.value !== "replace";
        if (event.target.value === "replace") input.focus();
      });
    }
    row.querySelector(".kv-remove").addEventListener("click", () => row.remove());
  }

  function addHeaderRow(view, name, source) {
    const container = view.querySelector("#jf-headers");
    const row = document.createElement("div");
    row.className = "kv-row";
    const redacted =
      source && source.source === "inline" && source.value === SECRET;
    const sourceValue =
      source && source.value !== undefined && source.value !== null && !redacted
        ? String(source.value)
        : "";
    const resolution = redacted
      ? `<select class="kv-resolve">
           <option value="">choose…</option>
           <option value="replace">replace</option>
           <option value="remove">remove</option>
         </select>
         <input class="kv-value" type="password" placeholder="new inline value" autocomplete="off" hidden>`
      : `<select class="kv-source">
           <option value="inline">inline</option>
           <option value="environment">environment</option>
         </select>
         <input class="kv-value" type="text" value="${attr(sourceValue)}" placeholder="value or environment variable name" spellcheck="false">`;
    row.innerHTML =
      `<input class="kv-name" type="text" value="${attr(name || "")}" placeholder="Header-Name" spellcheck="false">` +
      resolution +
      `<button type="button" class="link danger kv-remove">remove</button>`;
    if (!redacted && source && source.source === "environment") {
      row.querySelector(".kv-source").value = "environment";
    }
    container.appendChild(row);
    if (redacted) {
      row.dataset.redacted = "1";
      row.querySelector(".kv-resolve").addEventListener("change", (event) => {
        const input = row.querySelector(".kv-value");
        input.hidden = event.target.value !== "replace";
        if (event.target.value === "replace") input.focus();
      });
    }
    row.querySelector(".kv-remove").addEventListener("click", () => row.remove());
  }

  function bindForm(view, editing, base) {
    const q = (id) => view.querySelector(`#${id}`);
    const form = q("job-form");

    q("jf-sched-kind").addEventListener("change", () => syncScheduleKind(view));
    q("jf-cron-tz").addEventListener("change", () => syncCronTz(view));
    q("jf-target-kind").addEventListener("change", () => syncTargetKind(view));
    q("jf-add-env").addEventListener("click", () => addEnvRow(view, "", ""));
    q("jf-add-header").addEventListener("click", () => addHeaderRow(view, "", null));
    q("jf-anchor-now").addEventListener("click", () => {
      q("jf-every-anchor").value = String(C.nowUs());
    });
    q("jf-at-now").addEventListener("click", () => {
      q("jf-at-at").value = String(C.nowUs());
    });
    q("jf-preview").addEventListener("click", async () => {
      const schedule = collectSchedule(view);
      if (schedule.error) {
        q("jf-preview-list").textContent = schedule.error;
        q("jf-preview-list").hidden = false;
        return;
      }
      try {
        const { data } = await Api.post("/api/v1/schedule/preview", {
          schedule: schedule.value,
          count: 5,
        });
        const list = q("jf-preview-list");
        list.innerHTML = "";
        for (const occurrence of data.occurrences || []) {
          const item = document.createElement("li");
          item.textContent = occurrence;
          list.appendChild(item);
        }
        list.hidden = false;
      } catch (error) {
        q("jf-preview-list").textContent = error.message || String(error);
        q("jf-preview-list").hidden = false;
      }
    });

    if (editing) {
      q("jf-load-values-btn").addEventListener("click", async () => {
        if (!q("jf-load-values").checked) {
          q("jf-load-values-status").textContent =
            "check the acknowledgment first — plaintext values require an explicit acknowledgment";
          return;
        }
        const status = q("jf-load-values-status");
        status.textContent = "loading…";
        try {
          const { data } = await Api.get(
            `/api/v1/export?jobs=${encodeURIComponent(base)}&include-values=1&acknowledge-plaintext=1`,
          );
          const exported = (data.jobs || []).find(
            (candidate) => candidate.name === base,
          );
          if (!exported) throw new Error("export did not include this job");
          const loaded = { ...exported.definition, name: base, description: exported.description, tags: exported.tags, enabled: exported.enabled };
          populateForm(view, loaded);
          status.textContent = "plaintext values loaded into the form — they remain in this browser session";
          status.classList.add("warning");
        } catch (error) {
          status.textContent = error.message || String(error);
        }
      });
    }

    q("jf-dry-run").addEventListener("click", async () => {
      const collected = collect(view);
      if (collected.errors.length) {
        showResult(view, "validation-error", collected.errors.join("\n"));
        return;
      }
      const payload = collected.payload;
      payload.dry_run = "1";
      try {
        if (editing) {
          const { data } = await Api.put(
            `/api/v1/jobs/${encodeURIComponent(base)}`,
            payload,
          );
          showResult(
            view,
            "ok",
            `dry-run: ${JSON.stringify(data, null, 2)}`,
          );
        } else {
          const { data } = await Api.post("/api/v1/jobs", payload);
          showResult(
            view,
            "ok",
            `dry-run: ${JSON.stringify(data, null, 2)}`,
          );
        }
      } catch (error) {
        showResult(view, "error", error.message || String(error));
      }
    });

    form.addEventListener("submit", async (event) => {
      event.preventDefault();
      const collected = collect(view);
      if (collected.errors.length) {
        showResult(view, "validation-error", collected.errors.join("\n"));
        return;
      }
      const payload = collected.payload;
      try {
        if (editing) {
          const { data } = await Api.put(
            `/api/v1/jobs/${encodeURIComponent(base)}`,
            payload,
          );
          Router.navigate(`#/jobs/${encodeURIComponent(data.name)}`);
        } else {
          const { data } = await Api.post("/api/v1/jobs", payload);
          Router.navigate(`#/jobs/${encodeURIComponent(data.name)}`);
        }
      } catch (error) {
        showResult(view, "error", error.message || String(error));
      }
    });
  }

  function showResult(view, className, text) {
    const box = view.querySelector("#jf-result");
    box.hidden = false;
    box.className = `result ${className}`;
    box.textContent = text;
    box.scrollIntoView({ block: "nearest" });
  }

  /// Reads the schedule group only (for the preview button).
  function collectSchedule(view) {
    const q = (id) => view.querySelector(`#${id}`);
    const kind = q("jf-sched-kind").value;
    const value = { kind };
    if (kind === "cron") {
      const expression = q("jf-cron-expression").value.trim();
      if (!expression) return { error: "cron expression is required" };
      value.expression = expression;
      value.timezone =
        q("jf-cron-tz").value === "iana"
          ? { mode: "iana", name: q("jf-cron-tz-name").value.trim() }
          : { mode: "local" };
    } else if (kind === "every") {
      const interval = Number(q("jf-every-interval").value);
      const anchor = Number(q("jf-every-anchor").value);
      if (!Number.isFinite(interval) || interval <= 0) {
        return { error: "interval must be a positive number of microseconds" };
      }
      if (!Number.isFinite(anchor)) {
        return { error: "anchor must be epoch microseconds" };
      }
      value.interval = interval;
      value.anchor = anchor;
    } else {
      const at = Number(q("jf-at-at").value);
      if (!Number.isFinite(at)) return { error: "at must be epoch microseconds" };
      value.at = at;
    }
    return { value };
  }

  /// Reads the whole form into the JobDefinition wire shape, collecting
  /// validation errors. Redacted rows must be resolved (replace or remove).
  function collect(view) {
    const errors = [];
    const q = (id) => view.querySelector(`#${id}`);
    const text = (id) => (q(id) ? q(id).value.trim() : "");
    const number = (id, name, optional) => {
      const raw = q(id) ? q(id).value.trim() : "";
      if (raw === "") return optional ? null : NaN;
      const parsed = Number(raw);
      if (!Number.isFinite(parsed)) {
        errors.push(`${name} must be a number`);
        return NaN;
      }
      return parsed;
    };

    const name = text("jf-name");
    if (!name) errors.push("name is required");
    const description = text("jf-description") || null;
    const tags = text("jf-tags")
      .split(",")
      .map((tag) => tag.trim())
      .filter(Boolean);

    const scheduleResult = collectSchedule(view);
    if (scheduleResult.error) errors.push(scheduleResult.error);

    const targetKind = q("jf-target-kind").value;
    const target = { kind: targetKind };
    if (targetKind === "process") {
      const executable = text("jf-process-exec");
      if (!executable) errors.push("executable is required");
      target.executable = executable;
      target.args = text("jf-process-args")
        .split("\n")
        .map((line) => line.trim())
        .filter(Boolean);
    } else if (targetKind === "shell") {
      const command = text("jf-shell-command");
      const shell = text("jf-shell-path");
      if (!command) errors.push("shell command is required");
      if (!shell) errors.push("shell path is required");
      target.command = command;
      target.shell = shell;
    } else {
      const url = text("jf-http-url");
      if (!url) errors.push("URL is required");
      target.method = (text("jf-http-method") || "GET").toUpperCase();
      target.url = url;
      target.success_statuses = text("jf-http-statuses")
        .split(",")
        .map((entry) => Number(entry.trim()))
        .filter((entry) => Number.isFinite(entry));
      target.follow_redirects = q("jf-http-redirects").checked;
      target.headers = {};
      const bodyInput = q("jf-http-body");
      const bodyFile = text("jf-http-body-file");
      if (bodyInput.dataset.redacted === "1" && bodyInput.value === "") {
        errors.push(
          "the HTTP body is redacted and was not re-entered; re-enter it or clear the body with the file field only",
        );
      } else if (bodyInput.value !== "") {
        target.body = bodyInput.value;
      } else {
        target.body = null;
      }
      if (bodyFile) target.body_file = bodyFile;
      if (target.body && target.body_file) {
        errors.push("provide an inline body or a body file, not both");
      }
      const headerErrors = collectRows(view, "#jf-headers .kv-row", "header", (row, entry) => {
        const resolved = resolveRowValue(row, entry, "header");
        if (resolved === undefined) {
          errors.push(`header ${entry.name || "(unnamed)"}: redacted value must be replaced or removed`);
          return;
        }
        if (resolved.omit) return;
        if (entry.name) target.headers[entry.name] = resolved.value;
      });
      errors.push(...headerErrors);
    }

    const cwd = text("jf-cwd");
    if (!cwd) errors.push("cwd is required");

    const environment = { values: {} };
    if (text("jf-env-file")) environment.file = text("jf-env-file");
    if (text("jf-env-path")) environment.path = text("jf-env-path");
    const envErrors = collectRows(view, "#jf-env-values .kv-row", "environment value", (row, entry) => {
      const resolved = resolveRowValue(row, entry, "environment");
      if (resolved === undefined) {
        errors.push(`environment ${entry.name || "(unnamed)"}: redacted value must be replaced or removed`);
        return;
      }
      if (resolved.omit) return;
      if (entry.name) environment.values[entry.name] = resolved.value;
    });
    errors.push(...envErrors);

    const policy = {
      overlap: text("jf-policy-overlap"),
      missed_run: text("jf-policy-missed"),
      start_deadline: number("jf-policy-deadline", "start deadline", true),
      catch_up_limit: number("jf-policy-catchup", "catch-up limit", false),
      retries: number("jf-policy-retries", "retries", false),
      retry_delay: number("jf-policy-retry-delay", "retry delay", false),
      retry_cap: number("jf-policy-retry-cap", "retry cap", false),
      backoff: text("jf-policy-backoff"),
      retry_timeout: q("jf-policy-retry-timeout").checked,
      timeout: number("jf-policy-timeout", "timeout", true),
      termination_grace: number("jf-policy-grace", "termination grace", false),
      per_job_concurrency: number("jf-policy-concurrency", "per-job concurrency", false),
    };

    const payload = {
      name,
      description,
      tags,
      enabled: q("jf-enabled").checked,
      definition: {
        schedule: scheduleResult.value,
        target,
        cwd,
        environment,
        policy,
      },
    };
    return { payload, errors };
  }

  /// Shared row collection for environment values and headers. `kind` selects
  /// the wire shape: "header" rows build `{source, value}` values (a replaced
  /// redacted inline header stays inline), "environment" rows plain strings.
  function collectRows(view, selector, kind, onRow) {
    const errors = [];
    for (const row of view.querySelectorAll(selector)) {
      const entry = { name: row.querySelector(".kv-name").value.trim() };
      onRow(row, entry);
    }
    return errors;
  }

  /// What happens to a row's value on save: `undefined` is an unresolved
  /// redacted value (blocks the save), `{omit: true}` drops the row, and
  /// `{value}` keeps it.
  function resolveRowValue(row, entry, kind) {
    const input = row.querySelector(".kv-value");
    const resolve = row.querySelector(".kv-resolve");
    if (row.dataset.redacted === "1") {
      const choice = resolve.value;
      if (choice === "") return undefined;
      if (choice === "remove") return { omit: true };
      const value = input.value;
      if (!value) return undefined;
      return kind === "header"
        ? { value: { source: "inline", value } }
        : { value };
    }
    const source = row.querySelector(".kv-source");
    const value = input.value.trim();
    return source
      ? { value: { source: source.value, value } }
      : { value };
  }

  Router.register("/jobs", renderList);
  Router.register("/jobs/new", renderForm);
  Router.register("/jobs/:id", renderDetail);
  Router.register("/jobs/:id/edit", renderForm);
})();
