import { useEffect, useState } from "react";
import { api } from "../api";
import { RouteHeader, StatusBadge } from "../ui";

type DiagnosticsData = {
  daemon_running: boolean;
  wake_socket: boolean;
  state_dir: string;
  database: string;
  execution_path: string;
  checks: string[];
};

export function Diagnostics() {
  const [data, setData] = useState<DiagnosticsData | null>(null);
  const [error, setError] = useState("");
  useEffect(() => {
    const controller = new AbortController();
    api.get<DiagnosticsData>("/api/v1/diagnostics", { signal: controller.signal }).then(({ data: value }) => setData(value)).catch((issue) => setError(issue.message));
    return () => controller.abort();
  }, []);
  return <>
    <RouteHeader title="Diagnostics" description="Read-only health, exposure, paths, and integrity." />
    {error ? <p className="error-block" role="alert">{error}</p> : !data ? <div aria-busy="true">Loading diagnostics…</div> : <section className="card"><h2>Health & exposure</h2><dl className="facts"><dt>Daemon</dt><dd><StatusBadge status={data.daemon_running ? "running" : "not running"}/></dd><dt>Wake socket</dt><dd><StatusBadge status={data.wake_socket ? "present" : "absent"}/></dd><dt>State directory</dt><dd><code>{data.state_dir}</code></dd><dt>Database</dt><dd><code>{data.database}</code></dd><dt>Execution path</dt><dd><code>{data.execution_path}</code></dd></dl><h3>Integrity</h3><ul>{data.checks?.map((item) => <li key={item}>{item}</li>)}</ul></section>}
  </>;
}
