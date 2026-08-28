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

const TERMINAL_RUN_STATES = new Set(["succeeded", "failed", "timed_out", "cancelled", "skipped_overlap", "skipped_concurrency", "interrupted_unknown"]);
function terminalRunState(state: string) { return TERMINAL_RUN_STATES.has(state); }

type OutputEvent = { attempt_number?: number; seq?: number; channel?: string; data_b64?: string };
export function outputEventKey(event: OutputEvent) {
  return `${event.attempt_number ?? "?"}:${event.seq ?? "?"}`;
}

type LogFrame = { channel: string; sequence: number; bytes: string };
function decodeOutput(channel: string, encoded: string) {
  try { return `[${channel}] ${new TextDecoder().decode(Uint8Array.from(atob(encoded), (character) => character.charCodeAt(0)))}`; }
  catch { return `[${channel}] <unreadable output>`; }
}
function decodeFrame(frame: LogFrame) { return decodeOutput(frame.channel, frame.bytes); }

type WhyData = { explanation: string; daemon_running: boolean; events: Array<{ id?: string; kind: string; occurred_at_us: number; details_json?: string }> };

export function RunDetail({ id }: { id: string }) {
  const [run, setRun] = useState<RunDetailData | null>(null);
  const [missing, setMissing] = useState(false);
  const [lines, setLines] = useState<string[]>([]);
  const [status, setStatus] = useState("");
  const [following, setFollowing] = useState(false);
  const [confirmingCancel, setConfirmingCancel] = useState(false);
  const [why, setWhy] = useState<WhyData | null>(null);
  const generation = useRef(0);
  const whyGeneration = useRef(0);
  const explanationRequest = useRef<AbortController | null>(null);
  const requests = useRef(new Set<AbortController>());
  const seenOutput = useRef(new Set<string>());
  const terminalReconciliation = useRef(false);

  const load = useCallback(async ({ initial = false, includeOutput = false }: { initial?: boolean; includeOutput?: boolean } = {}) => {
    const requestGeneration = generation.current;
    const controller = new AbortController();
    requests.current.add(controller);
    const current = () => !controller.signal.aborted && requestGeneration === generation.current;
    const explanationGeneration = ++whyGeneration.current;
    if (explanationRequest.current) {
      explanationRequest.current.abort();
      requests.current.delete(explanationRequest.current);
    }
    const explanationController = new AbortController();
    explanationRequest.current = explanationController;
    requests.current.add(explanationController);
    const currentExplanation = () => !explanationController.signal.aborted && requestGeneration === generation.current && explanationGeneration === whyGeneration.current;
    void api.get<WhyData>(`/api/v1/runs/${id}/why`, { signal: explanationController.signal }).then(({ data }) => {
      if (currentExplanation()) setWhy(data);
    }).catch(() => undefined).finally(() => {
      requests.current.delete(explanationController);
      if (explanationRequest.current === explanationController) explanationRequest.current = null;
    });
    try {
      const { data } = await api.get<RunDetailData>(`/api/v1/runs/${id}`, { signal: controller.signal });
      if (!current()) return;
      setRun(data);
      setMissing(false);
      if (initial) {
        if (terminalRunState(data.state)) setStatus("");
        else { setStatus("Connecting to live updates…"); setFollowing(true); }
      }
      if (includeOutput) {
        const output = await Promise.all(data.attempts.map(async (attempt) => {
          try {
            const result = await api.get<{ frames: LogFrame[] }>(`/api/v1/runs/${id}/logs?attempt=${attempt.attempt_number}`, { signal: controller.signal });
            return { attempt: attempt.attempt_number, frames: result.data.frames };
          } catch (issue) {
            if ((issue as { status?: number }).status === 404) return { attempt: attempt.attempt_number, frames: [] };
            throw issue;
          }
        }));
        if (!current()) return;
        const keys = new Set<string>();
        const durableLines: string[] = [];
        for (const attempt of output) for (const frame of attempt.frames) {
          keys.add(outputEventKey({ attempt_number: attempt.attempt, seq: frame.sequence }));
          durableLines.push(decodeFrame(frame));
        }
        seenOutput.current = keys;
        setLines(durableLines);
        setStatus(`Run finished: ${data.state}. Durable details reconciled.`);
      }
    } catch (issue) {
      if (!current() || (issue as Error).name === "AbortError") return;
      if ((issue as { status?: number }).status === 404 && initial) setMissing(true);
      else setStatus((issue as Error).message);
    } finally {
      requests.current.delete(controller);
    }
  }, [id]);

  useEffect(() => {
    const requestGeneration = ++generation.current;
    for (const request of requests.current) request.abort();
    requests.current.clear();
    explanationRequest.current = null;
    seenOutput.current = new Set();
    whyGeneration.current += 1;
    terminalReconciliation.current = false;
    setRun(null);
    setLines([]);
    setStatus("");
    setFollowing(false);
    setMissing(false);
    setWhy(null);
    void load({ initial: true });
    return () => {
      if (generation.current === requestGeneration) generation.current += 1;
      for (const request of requests.current) request.abort();
      requests.current.clear();
      explanationRequest.current = null;
    };
  }, [load]);

  useEffect(() => {
    if (!following) return;
    const streamGeneration = generation.current;
    const current = () => streamGeneration === generation.current;
    const source = new EventSource(`/api/v1/runs/${encodeURIComponent(id)}/stream`);
    const handlers = new Map<string, EventListener>();
    for (const name of ["run", "attempt", "output", "termination"]) {
      const handler = ((raw: MessageEvent) => {
        if (!current()) return;
        let data: Record<string, unknown>;
        try { data = JSON.parse(raw.data) as Record<string, unknown>; } catch { return; }
        if (name === "run" && typeof data.state === "string") {
          setRun((currentRun) => currentRun ? { ...currentRun, state: data.state as string } : currentRun);
        } else if (name === "attempt" && typeof data.attempt_number === "number" && typeof data.state === "string") {
          setRun((currentRun) => {
            if (!currentRun) return currentRun;
            const attempts = [...currentRun.attempts];
            const index = attempts.findIndex((attempt) => attempt.attempt_number === data.attempt_number);
            if (index === -1) attempts.push({ attempt_number: data.attempt_number as number, state: data.state as string });
            else attempts[index] = { ...attempts[index]!, state: data.state as string };
            attempts.sort((left, right) => left.attempt_number - right.attempt_number);
            return { ...currentRun, attempts };
          });
        } else if (name === "output") {
          const key = outputEventKey(data as OutputEvent);
          if (seenOutput.current.has(key)) return;
          seenOutput.current.add(key);
          setLines((currentLines) => [...currentLines, decodeOutput(String(data.channel ?? "output"), String(data.data_b64 ?? ""))]);
        } else if (name === "termination" && !terminalReconciliation.current) {
          terminalReconciliation.current = true;
          if (typeof data.state === "string") setRun((currentRun) => currentRun ? { ...currentRun, state: data.state as string } : currentRun);
          source.close();
          setFollowing(false);
          setStatus("Run finished — reconciling durable details…");
          void load({ includeOutput: true });
        }
      }) as EventListener;
      handlers.set(name, handler);
      source.addEventListener(name, handler);
    }
    source.onopen = () => { if (current()) setStatus("Connected — replaying, then live"); };
    source.onerror = () => { if (current() && !terminalReconciliation.current) setStatus("Connection lost — retrying; last durable details remain visible."); };
    return () => { handlers.forEach((handler, name) => source.removeEventListener(name, handler)); source.close(); };
  }, [following, id, load]);

  if (missing) return <>
    <RouteHeader title="Run not found" description="This run may have been removed by retention, or this link may be stale." actions={<a className="button primary" href="#/runs">View run history</a>} />
    <section className="card"><h2>This run is unavailable</h2><p>Return to Run history to choose an existing durable run.</p></section>
  </>;
  if (!run) return <Feedback kind={status ? "error" : "muted"}>{status || "Loading run…"}</Feedback>;
  const loadAttempt = async (attempt = run.attempts.at(-1)?.attempt_number ?? 1) => {
    const requestGeneration = generation.current;
    const controller = new AbortController();
    requests.current.add(controller);
    try {
      const { data } = await api.get<{ frames: LogFrame[] }>(`/api/v1/runs/${id}/logs?attempt=${attempt}`, { signal: controller.signal });
      if (controller.signal.aborted || requestGeneration !== generation.current) return;
      seenOutput.current = new Set(data.frames.map((frame) => outputEventKey({ attempt_number: attempt, seq: frame.sequence })));
      setLines(data.frames.map(decodeFrame));
      setStatus(`Loaded attempt ${attempt} output.`);
    } catch (issue) { if (!controller.signal.aborted && requestGeneration === generation.current && (issue as Error).name !== "AbortError") setStatus((issue as Error).message); }
    finally { requests.current.delete(controller); }
  };
  const cancel = async () => {
    const requestGeneration = generation.current;
    try { await api.post(`/api/v1/runs/${id}/cancel`); if (requestGeneration !== generation.current) return; setStatus("Cancellation requested."); void load(); }
    catch (issue) { setStatus((issue as Error).message); }
  };
  return <>
    <RouteHeader title={`Run ${run.id.slice(0, 8)}`} description="Immutable attempt and output facts." actions={<>{["queued", "admitted", "starting", "running", "retry_wait"].includes(run.state) && <button className="danger" type="button" onClick={() => setConfirmingCancel(true)}>Cancel run</button>}<button type="button" onClick={() => void load()}>Refresh</button></>} />
    <Dialog open={confirmingCancel} onOpenChange={setConfirmingCancel} title="Cancel this run?" description="Locron will request termination. A running process receives the configured grace period before it is forced to stop."><div className="dialog-actions"><button className="danger" type="button" onClick={() => { setConfirmingCancel(false); void cancel(); }}>Cancel run</button><button data-dialog-cancel type="button" onClick={() => setConfirmingCancel(false)}>Keep running</button></div></Dialog>
    <dl className="facts"><dt>State</dt><dd><StatusBadge status={run.state}/></dd><dt>Requested</dt><dd>{new Date(run.requested_at_us / 1000).toLocaleString()}</dd><dt>Run ID</dt><dd><code>{run.id}</code></dd></dl>
    <section className="card"><h2>Attempts</h2>{run.attempts.length ? <div className="table-scroll"><table><thead><tr><th>#</th><th>State</th><th>Duration</th><th>Error</th></tr></thead><tbody>{run.attempts.map((attempt) => <tr key={attempt.attempt_number}><td><button className="link" type="button" onClick={() => void loadAttempt(attempt.attempt_number)}>{attempt.attempt_number}</button></td><td>{attempt.state}</td><td>{attempt.duration_us === undefined ? "—" : humanDuration(attempt.duration_us)}</td><td>{attempt.error ?? "—"}</td></tr>)}</tbody></table></div> : <p>No attempts yet.</p>}</section>
    <section className="card"><h2>Output</h2><div className="console-toolbar">{!terminalRunState(run.state) && <button type="button" onClick={() => { setFollowing((value) => { const next = !value; setStatus(next ? "Connecting to live updates…" : "Live updates paused; the run continues."); return next; }); }}>{following ? "Pause live updates" : "Resume live updates"}</button>}<button type="button" onClick={() => void loadAttempt()}>Load latest attempt</button><Feedback>{status}</Feedback></div><pre className="console">{lines.join("\n")}</pre></section>
    {why && <section className="card"><h2>Why</h2><p>{why.explanation}</p><p>Daemon {why.daemon_running ? "running" : "not running"}.</p>{why.events.length ? <div className="table-scroll"><table><thead><tr><th>Time</th><th>Event</th><th>Details</th></tr></thead><tbody>{why.events.map((event, index) => <tr key={event.id ?? `${event.occurred_at_us}-${index}`}><td>{new Date(event.occurred_at_us / 1000).toLocaleString()}</td><td>{event.kind}</td><td>{event.details_json ? <details><summary>View JSON</summary><JsonViewer source={event.details_json} label={`Audit JSON for ${event.kind}`} /></details> : "—"}</td></tr>)}</tbody></table></div> : <p>No audit events.</p>}</section>}
    {run.snapshot_json && <details className="card"><summary>Redacted snapshot JSON</summary><JsonViewer source={run.snapshot_json} label={`Redacted snapshot JSON for run ${run.id}`} /></details>}
  </>;
}
