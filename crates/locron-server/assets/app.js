// Application shell: session bootstrap (token paste or cookie handoff),
// topbar and daemon banner, diagnostics polling, and router startup.
"use strict";

const App = (() => {
  let daemonTimer = null;
  let routerStarted = false;

  const el = (id) => document.getElementById(id);

  function showApp() {
    el("paste-panel").hidden = true;
    el("app-panel").hidden = false;
    el("topbar").hidden = false;
    if (!routerStarted) {
      Router.start();
      routerStarted = true;
    } else {
      Router.navigate(Router.currentPath());
    }
    pollDaemon();
    if (daemonTimer) clearInterval(daemonTimer);
    daemonTimer = setInterval(pollDaemon, 30000);
  }

  function showPaste() {
    el("app-panel").hidden = true;
    el("topbar").hidden = true;
    el("daemon-banner").hidden = true;
    el("paste-panel").hidden = false;
    if (daemonTimer) {
      clearInterval(daemonTimer);
      daemonTimer = null;
    }
  }

  async function pollDaemon() {
    try {
      const { data } = await Api.get("/api/v1/diagnostics");
      const running = Boolean(data.daemon_running);
      const indicator = el("daemon-indicator");
      indicator.textContent = running ? "daemon running" : "daemon not running";
      indicator.className = `daemon ${running ? "running" : "stopped"}`;
      indicator.title = `daemon ${running ? "" : "not "}running — scheduler availability`;
      el("daemon-banner").hidden = running;
    } catch {
      // Keep the last known indicator; the next poll retries.
    }
  }

  function bindPaste() {
    const form = el("paste-form");
    const error = el("paste-error");
    form.addEventListener("submit", async (event) => {
      event.preventDefault();
      const token = el("token").value.trim();
      error.hidden = true;
      if (!token) return;
      try {
        await Api.post("/api/v1/session", { token });
        el("token").value = "";
        showApp();
      } catch (err) {
        error.textContent = err.message || String(err);
        error.hidden = false;
      }
    });
  }

  async function boot() {
    // The session status call issues the CSRF cookie when it is missing;
    // the session cookie (set by the paste) is what unlocks the app.
    try {
      await Api.get("/api/v1/session");
    } catch {
      // Server unreachable: the paste panel stays up and reports errors.
    }
    if (Api.hasSession()) {
      showApp();
    } else {
      showPaste();
      bindPaste();
    }
  }

  window.addEventListener("session-expired", showPaste);
  window.addEventListener("DOMContentLoaded", boot);
})();
