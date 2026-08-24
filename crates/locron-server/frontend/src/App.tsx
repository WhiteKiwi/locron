import { useEffect, useState, type FormEvent } from "react";
import { api } from "./api";
import { Field, ThemeControl } from "./components";
import { Diagnostics } from "./routes/Diagnostics";
import { JobDetail, Jobs } from "./routes/Jobs";
import { JobForm } from "./routes/JobForm";
import { RunDetail, Runs } from "./routes/Runs";
import { Settings } from "./routes/Settings";
import { AppShell } from "./AppShell";
import { TooltipProvider } from "./ui";

type Route = { path: string; params: string[] };
const titles: Record<string, string> = {
  "/jobs": "Jobs · Locron",
  "/runs": "Run history · Locron",
  "/diagnostics": "Diagnostics · Locron",
  "/settings": "Settings · Locron",
};

export function parseRoute(hash = location.hash): Route {
  const path = (hash || "#/jobs").slice(1);
  return { path, params: path.split("/").filter(Boolean) };
}

export function titleForRoute(current: Route) {
  if (current.path === "/jobs/new") return "New job · Locron";
  if (current.params[0] === "jobs" && current.params.length > 1) return current.params[2] === "edit" ? "Edit job · Locron" : "Job · Locron";
  if (current.params[0] === "runs" && current.params.length > 1) return "Run · Locron";
  return titles[`/${current.params[0] ?? "jobs"}`] ?? "Locron dashboard";
}

function useRoute() {
  const [current, setCurrent] = useState(parseRoute);
  useEffect(() => {
    const update = () => setCurrent(parseRoute());
    addEventListener("hashchange", update);
    return () => removeEventListener("hashchange", update);
  }, []);
  useEffect(() => { document.title = titleForRoute(current); }, [current]);
  return current;
}

export function App() {
  const [authenticated, setAuthenticated] = useState<boolean | null>(null);
  const [daemon, setDaemon] = useState<boolean | null>(null);
  const current = useRoute();

  useEffect(() => {
    const controller = new AbortController();
    api.get<{ authenticated: boolean }>("/api/v1/session", { signal: controller.signal })
      .then(({ data }) => setAuthenticated(data.authenticated === true))
      .catch(() => setAuthenticated(false));
    return () => controller.abort();
  }, []);
  useEffect(() => {
    if (!authenticated) return;
    let controller = new AbortController();
    const poll = () => {
      controller.abort();
      controller = new AbortController();
      return api.get<{ daemon_running: boolean }>("/api/v1/diagnostics", { signal: controller.signal })
        .then(({ data }) => setDaemon(data.daemon_running))
        .catch(() => undefined);
    };
    void poll();
    const timer = setInterval(poll, 30_000);
    return () => { controller.abort(); clearInterval(timer); };
  }, [authenticated]);
  useEffect(() => {
    const expired = () => setAuthenticated(false);
    addEventListener("session-expired", expired);
    return () => removeEventListener("session-expired", expired);
  }, []);

  if (authenticated === null) return <main><div className="empty-state" aria-busy="true"><h1>Opening Locron</h1><p>Checking your local session…</p></div></main>;
  if (!authenticated) return <Entry onOpen={() => setAuthenticated(true)} />;

  const root = current.params[0] ?? "jobs";
  return <TooltipProvider><AppShell current={root} daemon={daemon}><RouteView route={current} /></AppShell></TooltipProvider>;
}

function Entry({ onOpen }: { onOpen: () => void }) {
  const [token, setToken] = useState("");
  const [error, setError] = useState("");
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    try { await api.post("/api/v1/session", { token: token.trim() }); setToken(""); onOpen(); }
    catch (issue) { setError((issue as Error).message); }
  };
  return <main id="main-content"><section id="paste-panel">
    <div className="entry-story"><div className="entry-eyebrow"><span aria-hidden="true">✦</span>Local by design</div><h1><span>locron</span><br />keeps time<br />on your machine.</h1><p>Inspect schedules, run history, and durable facts without sending operational data anywhere else.</p></div>
    <div className="entry-access"><p className="entry-kicker">Dashboard access</p><ThemeControl name="entry-theme" /><h2>Open your local workspace</h2><form onSubmit={submit}><Field label="Access token" description="Paste the 64 hexadecimal characters from locron dashboard token.">{({ id, describedBy }) => <input id={id} type="password" required value={token} aria-describedby={describedBy} onChange={(event) => setToken(event.target.value)} autoComplete="off" />}</Field><button className="primary">Open dashboard →</button></form>{error && <p className="error-block" role="alert">{error}</p>}</div>
  </section></main>;
}

function RouteView({ route }: { route: Route }) {
  if (route.path === "/jobs") return <Jobs />;
  if (route.path === "/jobs/new") return <JobForm />;
  if (route.params[0] === "jobs" && route.params[2] === "edit") return <JobForm reference={route.params[1]!} />;
  if (route.params[0] === "jobs" && route.params[1]) return <JobDetail reference={route.params[1]} />;
  if (route.path === "/runs") return <Runs />;
  if (route.params[0] === "runs" && route.params[1]) return <RunDetail id={route.params[1]} />;
  if (route.path === "/diagnostics") return <Diagnostics />;
  if (route.path === "/settings") return <Settings />;
  return <div className="empty-state"><h1>Unknown route</h1><p>This address is not a dashboard view.</p></div>;
}
