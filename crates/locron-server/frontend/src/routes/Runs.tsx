import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "../api";
import { Feedback, Field } from "../components";
import { humanDuration } from "../domain/duration";
import type { Job, Run, RunDetailData } from "../types";
import { JsonViewer } from "../json";
import { ActionMenu, ActionMenuItem, Dialog, EmptyObjectList, EmptyTableRow, ResponsiveData, RouteHeader, StatusBadge } from "../ui";
import { RefreshCw } from "lucide-react";
import { navigateRow } from "../rowNavigation";

const PAGE_SIZE = 20;
type SearchPage = { runs: Run[]; total: number; offset: number };

export function Runs() {
  const [query, setQuery] = useState("");
  const [offset, setOffset] = useState(0);
  const [runs, setRuns] = useState<Run[]>([]);
  const [total, setTotal] = useState(0);
  const [status, setStatus] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(true);
  const [loaded, setLoaded] = useState(false);
  const [requestTick, setRequestTick] = useState(0);
  const generation = useRef(0);
  const immediateRequest = useRef(true);
  const input = useRef<HTMLInputElement>(null);

  const request = useCallback((immediate: boolean) => {
    immediateRequest.current = immediate;
    setRequestTick((tick) => tick + 1);
  }, []);

  useEffect(() => {
    const controller = new AbortController();
    const requestedQuery = query.trim();
    const requestGeneration = ++generation.current;
    const delay = immediateRequest.current ? 0 : 250;
    immediateRequest.current = false;
    const timer = setTimeout(async () => {
      setBusy(true);
      const searching = setTimeout(() => {
        if (requestGeneration === generation.current) setStatus("Searching durable history…");
      }, 150);
      try {
        const params = new URLSearchParams({ limit: String(PAGE_SIZE), offset: String(offset) });
        if (requestedQuery) params.set("q", requestedQuery);
        const [{ data }, jobs] = await Promise.all([
          api.get<SearchPage>(`/api/v1/runs?${params}`, { signal: controller.signal }),
          api.get<Job[]>("/api/v1/jobs?all=1", { signal: controller.signal }).catch((issue) => {
            if ((issue as Error).name === "AbortError") throw issue;
            return { data: [], warnings: [] };
          }),
        ]);
        if (controller.signal.aborted || requestGeneration !== generation.current || requestedQuery !== query.trim()) return;
        const names = new Map(jobs.data.map((job) => [job.id, job.name]));
        setRuns(data.runs.map((run) => { const jobName = names.get(run.job_id); return jobName ? { ...run, job_name: jobName } : run; }));
        setTotal(data.total);
        setLoaded(true);
        setError("");
        setStatus(requestedQuery ? `${data.total} matching run${data.total === 1 ? "" : "s"} for “${requestedQuery}”.` : `${data.total} run${data.total === 1 ? "" : "s"}.`);
      } catch (issue) {
        if (!controller.signal.aborted && requestGeneration === generation.current && (issue as Error).name !== "AbortError") {
          setError((issue as Error).message);
          setStatus("");
        }
      } finally {
        clearTimeout(searching);
        if (requestGeneration === generation.current) setBusy(false);
      }
    }, delay);
    return () => { clearTimeout(timer); controller.abort(); };
  }, [query, offset, requestTick]);

  const runNow = () => request(true);
  const clearFilters = () => { setQuery(""); setOffset(0); request(true); input.current?.focus(); };
  const filtered = Boolean(query.trim());
  const emptyTitle = filtered ? "No runs match these filters" : "No runs yet";
  const emptyDescription = filtered ? "Clear the search to return to complete durable history." : "Run an existing job or create one to start durable history.";
  const emptyActions = () => filtered ? <button type="button" onClick={clearFilters}>Clear filters</button> : <><a className="button" href="#/jobs">View jobs</a><a className="button primary" href="#/jobs/new">Create job</a></>;
  const showResults = runs.length > 0 || (loaded && !error);
  return <>
    <RouteHeader title="Run history" description="Literal search across every durable run ID and current job name." actions={<button type="button" onClick={runNow}><RefreshCw size={16}/>Refresh</button>} />
    <search className="toolbar run-search" aria-label="Search run history">
      <Field label="Search by run ID or current job name" description="Try nightly, back, Unicode, %, _, or part of a run ID.">{({ id, describedBy }) => <div className="search-input"><input ref={input} id={id} type="search" autoComplete="off" spellCheck={false} value={query} aria-describedby={describedBy} onChange={(event) => { setQuery(event.target.value); setOffset(0); }} onKeyDown={(event) => { if (event.key === "Enter") { event.preventDefault(); request(true); } }} /><button type="button" onClick={clearFilters}>Clear</button></div>}</Field>
      <p id="runs-results-status" className={error ? "error" : "muted"} role="status" aria-atomic="true">{error ? <>{error} <button className="link" type="button" onClick={runNow}>Retry</button></> : status}</p>
    </search>
    {!loaded && busy && !error && <div className="loading-state" aria-busy="true">Loading run history…</div>}
    {showResults && <ResponsiveData busy={busy} desktop={<div className="table-scroll"><table aria-describedby="runs-results-status"><thead><tr><th>Run</th><th>Job</th><th>Requested</th><th>Trigger</th><th>State</th><th><span className="sr-only">Actions</span></th></tr></thead><tbody>{runs.length ? runs.map((run) => <tr className="clickable-row" onClick={navigateRow} key={run.id}><td><a data-row-link href={`#/runs/${run.id}`}>{run.id.slice(0, 8)}<span className="sr-only"> — view full run {run.id} for {run.job_name ?? run.job_id}, requested {new Date(run.requested_at_us / 1000).toLocaleString()}</span></a></td><td>{run.job_name ?? run.job_id.slice(0, 8)}</td><td>{new Date(run.requested_at_us / 1000).toLocaleString()}</td><td>{run.trigger}</td><td><StatusBadge status={run.state}/></td><td><ActionMenu label={`Actions for run ${run.id.slice(0,8)}`}><ActionMenuItem href={`#/runs/${run.id}`}>View details</ActionMenuItem><ActionMenuItem href={`#/jobs/${run.job_id}`}>View job</ActionMenuItem></ActionMenu></td></tr>) : <EmptyTableRow columns={6} title={emptyTitle} description={emptyDescription} actions={emptyActions()} />}</tbody></table></div>} mobile={runs.length ? <div className="object-list">{runs.map((run) => <article className="object-row clickable-row" onClick={navigateRow} key={run.id}><div className="object-row-head"><a data-row-link className="object-title" href={`#/runs/${run.id}`}>Run {run.id.slice(0,8)}<span className="sr-only"> — view full run {run.id} for {run.job_name ?? run.job_id}, requested {new Date(run.requested_at_us / 1000).toLocaleString()}</span></a><ActionMenu label={`Actions for run ${run.id.slice(0,8)}`}><ActionMenuItem href={`#/runs/${run.id}`}>View details</ActionMenuItem><ActionMenuItem href={`#/jobs/${run.job_id}`}>View job</ActionMenuItem></ActionMenu></div><dl><dt>Job</dt><dd>{run.job_name ?? run.job_id.slice(0,8)}</dd><dt>Requested</dt><dd>{new Date(run.requested_at_us / 1000).toLocaleString()}</dd><dt>Trigger</dt><dd>{run.trigger}</dd><dt>State</dt><dd><StatusBadge status={run.state}/></dd></dl></article>)}</div> : <EmptyObjectList title={emptyTitle} description={emptyDescription} actions={emptyActions()} />} />}
    {showResults && total > 0 && <div className="pager"><button disabled={offset === 0} onClick={() => { immediateRequest.current = true; setOffset(Math.max(0, offset - PAGE_SIZE)); }}>Newer</button><span>{total} total</span><button disabled={offset + runs.length >= total} onClick={() => { immediateRequest.current = true; setOffset(offset + PAGE_SIZE); }}>Older</button></div>}
  </>;
}

type OutputEvent = { attempt_number?: number; seq?: number; channel?: string; text?: string };
export function outputEventKey(event: OutputEvent) {
  return `${event.attempt_number ?? "?"}:${event.seq ?? "?"}`;
}

type LogFrame = { channel: string; sequence: number; bytes: string };
function decodeFrame(frame: LogFrame) {
  try { return `[${frame.channel}] ${new TextDecoder().decode(Uint8Array.from(atob(frame.bytes), (character) => character.charCodeAt(0)))}`; }
  catch { return `[${frame.channel}] <unreadable output>`; }
}

export function RunDetail({ id }: { id: string }) {
  const [run, setRun] = useState<RunDetailData | null>(null);
  const [missing, setMissing] = useState(false);
  const [lines, setLines] = useState<string[]>([]);
  const [status, setStatus] = useState("");
  const [following, setFollowing] = useState(false);
  const [confirmingCancel, setConfirmingCancel] = useState(false);
  const [why, setWhy] = useState<{ explanation: string; daemon_running: boolean; events: Array<{ id?: string; kind: string; occurred_at_us: number; details_json?: string }> } | null>(null);
  const load = useCallback(() => {
    const controller = new AbortController();
    void Promise.all([
      api.get<RunDetailData>(`/api/v1/runs/${id}`, { signal: controller.signal }).then(({ data }) => setRun(data)),
      api.get<{ explanation: string; daemon_running: boolean; events: Array<{ id?: string; kind: string; occurred_at_us: number; details_json?: string }> }>(`/api/v1/runs/${id}/why`, { signal: controller.signal }).then(({ data }) => setWhy(data)).catch(() => undefined),
    ]).catch((issue) => {
      if ((issue as { status?: number }).status === 404) setMissing(true);
      else if ((issue as Error).name !== "AbortError") setStatus((issue as Error).message);
    });
    return controller;
  }, [id]);
  useEffect(() => {
    setRun(null);
    setStatus("");
    setMissing(false);
    setWhy(null);
    const controller = load();
    return () => controller.abort();
  }, [load]);
  useEffect(() => {
    if (!following) return;
    const source = new EventSource(`/api/v1/runs/${encodeURIComponent(id)}/stream`);
    const seen = new Set<string>();
    const handlers = new Map<string, EventListener>();
    for (const name of ["run", "attempt", "output", "termination"]) {
      const handler = ((raw: MessageEvent) => {
        let data: Record<string, unknown>;
        try { data = JSON.parse(raw.data) as Record<string, unknown>; } catch { return; }
        if (name === "output") {
          const key = outputEventKey(data as OutputEvent);
          if (seen.has(key)) return;
          seen.add(key);
          setLines((current) => [...current, `[${String(data.channel)}] ${String(data.text ?? "")}`]);
        } else if (name === "termination") {
          setStatus(`Stream ended: ${String(data.state)}`);
          setFollowing(false);
          load();
        }
      }) as EventListener;
      handlers.set(name, handler);
      source.addEventListener(name, handler);
    }
    source.onopen = () => setStatus("Connected — replaying, then live");
    source.onerror = () => setStatus("Connection lost — retrying");
    return () => { handlers.forEach((handler, name) => source.removeEventListener(name, handler)); source.close(); };
  }, [following, id, load]);

  if (missing) return <>
    <RouteHeader title="Run not found" description="This run may have been removed by retention, or this link may be stale." actions={<a className="button primary" href="#/runs">View run history</a>} />
    <section className="card"><h2>This run is unavailable</h2><p>Return to Run history to choose an existing durable run.</p></section>
  </>;
  if (!run) return <Feedback kind={status ? "error" : "muted"}>{status || "Loading run…"}</Feedback>;
  const loadAttempt = async (attempt = run.attempts.at(-1)?.attempt_number ?? 1) => {
    try {
      const { data } = await api.get<{ frames: LogFrame[] }>(`/api/v1/runs/${id}/logs?attempt=${attempt}`);
      setLines(data.frames.map(decodeFrame));
      setStatus(`Loaded attempt ${attempt} output.`);
    } catch (issue) { setStatus((issue as Error).message); }
  };
  const cancel = async () => {
    try { await api.post(`/api/v1/runs/${id}/cancel`); setStatus("Cancellation requested."); load(); }
    catch (issue) { setStatus((issue as Error).message); }
  };
  return <>
    <RouteHeader title={`Run ${run.id.slice(0, 8)}`} description="Immutable attempt and output facts." actions={<>{["queued", "admitted", "running", "retry_wait"].includes(run.state) && <button className="danger" type="button" onClick={() => setConfirmingCancel(true)}>Cancel run</button>}<button type="button" onClick={() => load()}>Refresh</button></>} />
    <Dialog open={confirmingCancel} onOpenChange={setConfirmingCancel} title="Cancel this run?" description="Locron will request termination. A running process receives the configured grace period before it is forced to stop."><div className="dialog-actions"><button className="danger" type="button" onClick={() => { setConfirmingCancel(false); void cancel(); }}>Cancel run</button><button data-dialog-cancel type="button" onClick={() => setConfirmingCancel(false)}>Keep running</button></div></Dialog>
    <dl className="facts"><dt>State</dt><dd><StatusBadge status={run.state}/></dd><dt>Requested</dt><dd>{new Date(run.requested_at_us / 1000).toLocaleString()}</dd><dt>Run ID</dt><dd><code>{run.id}</code></dd></dl>
    <section className="card"><h2>Attempts</h2>{run.attempts.length ? <div className="table-scroll"><table><thead><tr><th>#</th><th>State</th><th>Duration</th><th>Error</th></tr></thead><tbody>{run.attempts.map((attempt) => <tr key={attempt.attempt_number}><td><button className="link" type="button" onClick={() => void loadAttempt(attempt.attempt_number)}>{attempt.attempt_number}</button></td><td>{attempt.state}</td><td>{attempt.duration_us === undefined ? "—" : humanDuration(attempt.duration_us)}</td><td>{attempt.error ?? "—"}</td></tr>)}</tbody></table></div> : <p>No attempts yet.</p>}</section>
    <section className="card"><h2>Output</h2><div className="console-toolbar"><button type="button" onClick={() => setFollowing((value) => !value)}>{following ? "Stop following" : "Follow output"}</button><button type="button" onClick={() => void loadAttempt()}>Load latest attempt</button><Feedback>{status}</Feedback></div><pre className="console">{lines.join("\n")}</pre></section>
    {why && <section className="card"><h2>Why</h2><p>{why.explanation}</p><p>Daemon {why.daemon_running ? "running" : "not running"}.</p>{why.events.length ? <div className="table-scroll"><table><thead><tr><th>Time</th><th>Event</th><th>Details</th></tr></thead><tbody>{why.events.map((event, index) => <tr key={event.id ?? `${event.occurred_at_us}-${index}`}><td>{new Date(event.occurred_at_us / 1000).toLocaleString()}</td><td>{event.kind}</td><td>{event.details_json ? <details><summary>View JSON</summary><JsonViewer source={event.details_json} label={`Audit JSON for ${event.kind}`} /></details> : "—"}</td></tr>)}</tbody></table></div> : <p>No audit events.</p>}</section>}
    {run.snapshot_json && <details className="card"><summary>Redacted snapshot JSON</summary><JsonViewer source={run.snapshot_json} label={`Redacted snapshot JSON for run ${run.id}`} /></details>}
  </>;
}
