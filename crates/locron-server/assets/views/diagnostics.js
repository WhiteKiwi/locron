// Diagnostics view: scheduler health facts, process resolution, integrity
// checks, and the global settings editor (dry-run first, then durable).
"use strict";

const DiagnosticsView = (() => {
  const C = Components;
  const esc = C.escapeHtml;
  const attr = C.escapeAttr;

  async function render(view) {
    view.innerHTML = `<div aria-busy="true">${C.emptyState("Loading diagnostics", "collecting health facts…")}</div>`;
    try {
      const [{ data: info }, settings] = await Promise.all([
        Api.get("/api/v1/diagnostics"),
        Api.get("/api/v1/settings").catch(() => ({ data: null })),
      ]);
      view.innerHTML = factsHtml(info) + settingsHtml(info, settings.data);
      bindSettings(view);
    } catch (error) {
      view.innerHTML = C.errorBlock(error);
    }
  }

  function factsHtml(info) {
    const resolutions = info.process_resolution || [];
    const resolutionRows = resolutions
      .map((entry) => {
        const ok = entry.status === "resolved";
        return `<tr>
          <td>${esc(entry.job_name)} <span class="muted">${esc(entry.job_id.slice(0, 8))}</span></td>
          <td><code>${esc(entry.requested_executable)}</code></td>
          <td>${C.chip(ok ? entry.status : "unresolved", ok ? "state-good" : "state-bad", false)}</td>
          <td>${ok ? `<code>${esc(entry.resolved_executable)}</code>` : `<span class="muted">${esc(entry.error || "")}</span>`}</td>
        </tr>`;
      })
      .join("");
    return `<div class="page-head"><h1>Diagnostics</h1></div>
      <section class="card">
        <h2>Health</h2>
        <dl class="facts">
          <dt>Daemon</dt><dd>${C.chip(info.daemon_running ? "running" : "not running", info.daemon_running ? "state-good" : "state-bad", false)}</dd>
          <dt>Wake socket</dt><dd>${C.chip(info.wake_socket ? "present" : "absent", info.wake_socket ? "state-good" : "state-muted", false)}</dd>
          <dt>State directory</dt><dd><code>${esc(info.state_dir)}</code></dd>
          <dt>Database</dt><dd><code>${esc(info.database)}</code></dd>
          <dt>Execution path</dt><dd><code>${esc(info.execution_path)}</code></dd>
          <dt>Global environment</dt><dd>${info.global_environment_names.length ? info.global_environment_names.map((name) => `<code>${esc(name)}</code>`).join(" ") : '<span class="muted">none</span>'}</dd>
        </dl>
      </section>
      <section class="card">
        <h2>Process resolution</h2>
        ${resolutions.length ? `<table class="runs-table">
          <thead><tr><th>Job</th><th>Requested</th><th>Status</th><th>Resolved</th></tr></thead>
          <tbody>${resolutionRows}</tbody>
        </table>` : '<p class="muted">no process or shell jobs to resolve</p>'}
      </section>
      <section class="card">
        <h2>Integrity checks</h2>
        <ul>${(info.checks || []).map((check) => `<li>${esc(check)}</li>`).join("")}</ul>
      </section>`;
  }

  function settingsHtml(info, settings) {
    if (!settings) return "";
    const env = settings.environment || {};
    const envRows = Object.entries(env)
      .map(
        ([name]) => `<div class="kv-row" data-env="${attr(name)}">
          <span class="kv-name-static">${esc(name)}</span>
          <input class="kv-value" type="password" placeholder="value is redacted — enter a new value to replace" autocomplete="off">
          <button type="button" class="link danger env-remove">remove</button>
          <span class="env-result muted"></span>
        </div>`,
      )
      .join("");

    const keyRow = (key, label, value) => `<div class="settings-row" data-key="${attr(key)}">
      <label>${esc(label)}</label>
      <input type="text" value="${attr(String(value))}" spellcheck="false">
      <button type="button" class="button key-save">Save</button>
      <span class="key-result muted"></span>
    </div>`;

    return `<section class="card">
      <h2>Settings</h2>
      ${keyRow("global_concurrency", "Global concurrency", settings.global_concurrency)}
      ${keyRow("execution_path", "Execution path", settings.execution_path)}
      ${keyRow("run_retention_count", "Run retention count", settings.run_retention_count)}
      <div class="settings-row">
        <label>Run retention age (µs)</label>
        <input type="text" value="${attr(String(settings.run_retention_age_us ?? ""))}" disabled spellcheck="false">
        <span class="muted">display only</span>
      </div>
      ${keyRow("output_limit_bytes", "Output limit (bytes)", settings.output_limit_bytes)}
      ${keyRow("per_run_output_limit_bytes", "Per-run output limit (bytes)", settings.per_run_output_limit_bytes)}
      <h3>Global environment</h3>
      <div id="settings-env">${envRows || '<p class="muted">no global environment values</p>'}</div>
      <div class="kv-row">
        <input id="env-new-name" type="text" placeholder="NAME" spellcheck="false">
        <input id="env-new-value" type="password" placeholder="value" autocomplete="off">
        <button id="env-add" type="button" class="button">Add</button>
        <span id="env-add-result" class="key-result muted"></span>
      </div>
    </section>`;
  }

  function bindSettings(view) {
    const result = (element, text, ok) => {
      element.textContent = text;
      element.className = ok ? "key-result state-good" : "key-result state-bad";
    };

    for (const row of view.querySelectorAll(".settings-row[data-key]")) {
      const key = row.dataset.key;
      const input = row.querySelector("input");
      const status = row.querySelector(".key-result");
      row.querySelector(".key-save").addEventListener("click", async () => {
        const value = input.value;
        status.textContent = "";
        try {
          await Api.put(`/api/v1/settings/${encodeURIComponent(key)}`, {
            value,
            dry_run: "1",
          });
          const { data } = await Api.put(`/api/v1/settings/${encodeURIComponent(key)}`, { value });
          result(status, `saved — ${key} is now ${JSON.stringify(data[key] ?? data.value)}`, true);
        } catch (error) {
          result(status, error.message || String(error), false);
        }
      });
    }

    for (const row of view.querySelectorAll("#settings-env .kv-row[data-env]")) {
      const name = row.dataset.env;
      const status = row.querySelector(".env-result");
      row.querySelector(".env-remove").addEventListener("click", async () => {
        try {
          const { data } = await Api.del(
            `/api/v1/settings/${encodeURIComponent(`environment.${name}`)}`,
          );
          result(status, data.action, true);
          row.hidden = true;
        } catch (error) {
          result(status, error.message || String(error), false);
        }
      });
    }

    view.querySelector("#env-add").addEventListener("click", async () => {
      const name = view.querySelector("#env-new-name").value.trim();
      const value = view.querySelector("#env-new-value").value;
      const status = view.querySelector("#env-add-result");
      if (!name) return;
      try {
        await Api.put(`/api/v1/settings/${encodeURIComponent(`environment.${name}`)}`, {
          value,
          dry_run: "1",
        });
        await Api.put(`/api/v1/settings/${encodeURIComponent(`environment.${name}`)}`, { value });
        result(status, `saved environment.${name}`, true);
        Router.navigate("#/diagnostics");
      } catch (error) {
        result(status, error.message || String(error), false);
      }
    });
  }

  Router.register("/diagnostics", render);
})();
