import { useEffect, useRef, useState } from "react";
import { api } from "../api";
import { Feedback, Field } from "../components";
import { humanDuration } from "../domain/duration";
import type { Definition, Job } from "../types";
import { ActionMenu, ActionMenuItem, Dialog, EmptyObjectList, EmptyTableRow, LocronSelect, ResponsiveData, RouteHeader, StatusBadge } from "../ui";
import { Play, Plus, RefreshCw } from "lucide-react";
import { JsonViewer } from "../json";
import { navigateRow } from "../rowNavigation";

const tagsFor = (job: Job) => {
  if (job.tags) return job.tags;
  try { return JSON.parse(job.tags_json ?? "[]") as string[]; } catch { return []; }
};

function scheduleSummary(job: Job) {
  try {
    const schedule = (JSON.parse(job.definition_json) as { schedule: Record<string, unknown> }).schedule;
    if (schedule.kind === "cron") return `${schedule.expression} · ${typeof schedule.timezone === "object" && schedule.timezone && "name" in schedule.timezone ? String(schedule.timezone.name) : "local"}`;
    if (schedule.kind === "every") return `every ${humanDuration(Number(schedule.interval))}`;
    if (schedule.kind === "at") return `at ${new Date(Number(schedule.at) / 1000).toLocaleString()}`;
  } catch { /* reported as unreadable */ }
  return "unreadable definition";
}

export function Jobs() {
  const [data, setData] = useState<Job[] | null>(null);
  const [query, setQuery] = useState("");
  const [state, setState] = useState("all");
  const [error, setError] = useState("");
  const [refresh, setRefresh] = useState(0);
  const [facts, setFacts] = useState<Record<string, { next: string; last: string }>>({});
  const search = useRef<HTMLInputElement>(null);
  useEffect(() => {
    const controller = new AbortController();
    setError("");
    api.get<Job[]>("/api/v1/jobs", { signal: controller.signal }).then(({ data: jobs }) => setData(jobs)).catch((issue) => setError(issue.message));
    return () => controller.abort();
  }, [refresh]);
  useEffect(() => {
    if (!data) return;
    const controller = new AbortController();
    void Promise.all(data.map(async (job) => {
      const [nextResult, lastResult] = await Promise.allSettled([
        api.get<{ occurrences: string[] }>(`/api/v1/jobs/${job.id}/preview?count=1`, { signal: controller.signal }),
        api.get<{ runs: Array<{ state: string; requested_at_us: number }> }>(`/api/v1/runs?job=${encodeURIComponent(job.name)}&limit=1`, { signal: controller.signal }),
      ]);
      const next = nextResult.status === "fulfilled" ? (nextResult.value.data.occurrences[0] ? new Date(nextResult.value.data.occurrences[0]).toLocaleString() : "no future occurrence") : "unavailable";
      const run = lastResult.status === "fulfilled" ? lastResult.value.data.runs[0] : undefined;
      const last = run ? `${run.state} · ${new Date(run.requested_at_us / 1000).toLocaleString()}` : lastResult.status === "fulfilled" ? "no runs yet" : "unavailable";
      return [job.id, { next, last }] as const;
    })).then((entries) => { if (!controller.signal.aborted) setFacts(Object.fromEntries(entries)); });
    return () => controller.abort();
  }, [data]);
  const rows = (data ?? []).filter((job) => (`${job.name} ${job.description ?? ""} ${tagsFor(job).join(" ")}`).toLocaleLowerCase().includes(query.toLocaleLowerCase()) && (state === "all" || (state === "enabled") === job.enabled));
  const filtered = Boolean(query.trim()) || state !== "all";
  const clearFilters = () => { setQuery(""); setState("all"); search.current?.focus(); };
  const emptyTitle = filtered ? "No jobs match these filters" : "No jobs yet";
  const emptyDescription = filtered ? "Clear the search and state filter to see every job." : "Create a job to schedule the first local operation.";
  const emptyActions = () => filtered ? <button type="button" onClick={clearFilters}>Clear filters</button> : <a className="button primary" href="#/jobs/new">Create job</a>;
  const act = async (job: Job, action: "run" | "enable" | "disable") => {
    try { const result = await api.post<{ run_id?: string }>(`/api/v1/jobs/${job.id}/${action}`); if (result.data.run_id) location.hash = `#/runs/${result.data.run_id}`; else setRefresh((value) => value + 1); }
    catch (issue) { setError((issue as Error).message); }
  };
  const rowActions = (job: Job) => <ActionMenu label={`Actions for ${job.name}`}><ActionMenuItem href={`#/jobs/${job.id}`}>View details</ActionMenuItem><ActionMenuItem onSelect={() => void act(job, "run")}>Run now</ActionMenuItem><ActionMenuItem href={`#/jobs/${job.id}/edit`}>Edit</ActionMenuItem><ActionMenuItem onSelect={() => void act(job, job.enabled ? "disable" : "enable")}>{job.enabled ? "Disable" : "Enable"}</ActionMenuItem></ActionMenu>;
  return <>
    <RouteHeader title="Jobs" description="Schedules, next occurrences, and the latest durable outcome." actions={<><button type="button" onClick={() => setRefresh((value) => value + 1)}><RefreshCw size={16}/>Refresh</button><a className="button primary" href="#/jobs/new"><Plus size={16}/>New job</a></>} />
    <search className="toolbar" aria-label="Filter jobs">
      <Field label="Search jobs" description="Match a name, description, or tag.">{({ id, describedBy }) => <input ref={search} id={id} type="search" value={query} aria-describedby={describedBy} onChange={(event) => setQuery(event.target.value)} />}</Field>
      <Field label="State filter">{({ id }) => <LocronSelect id={id} label="State filter" value={state} onChange={setState} options={[{value:"all",label:"All states"},{value:"enabled",label:"Enabled"},{value:"disabled",label:"Disabled"}]} />}</Field>
      {!error && <p id="jobs-results-status" className="toolbar-status muted" role="status" aria-atomic="true">{data === null ? "Loading jobs…" : `${rows.length} result${rows.length === 1 ? "" : "s"}.`}</p>}
    </search>
    {error ? <Feedback kind="error">{error}</Feedback> : data === null ? <div className="loading-state" aria-busy="true">Loading jobs…</div> : <ResponsiveData desktop={<div className="table-scroll"><table aria-describedby="jobs-results-status"><thead><tr><th>Job</th><th>Schedule</th><th>Next</th><th>Last run</th><th><span className="sr-only">Actions</span></th></tr></thead><tbody>{rows.length ? rows.map((job) => <tr className="clickable-row" onClick={navigateRow} key={job.id}><td><a data-row-link href={`#/jobs/${encodeURIComponent(job.id)}`}>{job.name}<span className="sr-only"> — view job details</span></a><span className="sub">{tagsFor(job).join(" · ") || "no tags"}</span></td><td>{scheduleSummary(job)}<span className="sub"><StatusBadge status={job.enabled ? "enabled" : "disabled"}/></span></td><td>{facts[job.id]?.next ?? "…"}</td><td>{facts[job.id]?.last ?? "…"}</td><td>{rowActions(job)}</td></tr>) : <EmptyTableRow columns={5} title={emptyTitle} description={emptyDescription} actions={emptyActions()} />}</tbody></table></div>} mobile={rows.length ? <div className="object-list">{rows.map((job) => <article className="object-row clickable-row" onClick={navigateRow} key={job.id}><div className="object-row-head"><div><a data-row-link className="object-title" href={`#/jobs/${encodeURIComponent(job.id)}`}>{job.name}<span className="sr-only"> — view job details</span></a><span className="sub">{tagsFor(job).join(" · ") || "no tags"}</span></div>{rowActions(job)}</div><dl><dt>Schedule</dt><dd>{scheduleSummary(job)}</dd><dt>State</dt><dd><StatusBadge status={job.enabled ? "enabled" : "disabled"}/></dd><dt>Next</dt><dd>{facts[job.id]?.next ?? "…"}</dd><dt>Last run</dt><dd>{facts[job.id]?.last ?? "…"}</dd></dl><button type="button" className="inline-action" onClick={() => void act(job,"run")}><Play size={15}/>Run now</button></article>)}</div> : <EmptyObjectList title={emptyTitle} description={emptyDescription} actions={emptyActions()} />} />}
  </>;
}

export function JobDetail({ reference }: { reference: string }) {
  const [job, setJob] = useState<Job | null>(null);
  const [feedback, setFeedback] = useState("");
  const [confirmingRemove, setConfirmingRemove] = useState(false);
  const [why, setWhy] = useState<{ explanation: string; next_occurrence?: string; overlap: string; daemon_running: boolean } | null>(null);
  const [recent, setRecent] = useState<Array<{ id: string; state: string; requested_at_us: number }>>([]);
  useEffect(() => {
    const controller = new AbortController();
    void Promise.all([
      api.get<Job>(`/api/v1/jobs/${encodeURIComponent(reference)}`, { signal: controller.signal }).then(({ data }) => setJob(data)),
      api.get<{ explanation: string; next_occurrence?: string; overlap: string; daemon_running: boolean }>(`/api/v1/jobs/${encodeURIComponent(reference)}/why`, { signal: controller.signal }).then(({ data }) => setWhy(data)).catch(() => undefined),
      api.get<{ runs: Array<{ id: string; state: string; requested_at_us: number }> }>(`/api/v1/runs?job=${encodeURIComponent(reference)}&limit=5`, { signal: controller.signal }).then(({ data }) => setRecent(data.runs)).catch(() => undefined),
    ]).catch((issue) => setFeedback(issue.message));
    return () => controller.abort();
  }, [reference]);
  if (!job) return <Feedback kind={feedback ? "error" : "muted"}>{feedback || "Loading job…"}</Feedback>;
  let definition: Definition | null;
  try { definition = JSON.parse(job.definition_json) as Definition; } catch { definition = null; }
  const act = async (action: "run" | "enable" | "disable" | "remove") => {
    try {
      if (action === "remove") {
        await api.delete(`/api/v1/jobs/${job.id}`);
        location.hash = "#/jobs";
        return;
      }
      const result = await api.post<{ run_id?: string }>(`/api/v1/jobs/${job.id}/${action}`);
      if (result.data.run_id) location.hash = `#/runs/${result.data.run_id}`;
      else location.reload();
    } catch (issue) { setFeedback((issue as Error).message); }
  };
  return <>
    <RouteHeader title={job.name} description="Durable definition and safe actions." actions={<><button onClick={() => void act("run")}>Run now</button><button onClick={() => void act(job.enabled ? "disable" : "enable")}>{job.enabled ? "Disable" : "Enable"}</button><a className="button" href={`#/jobs/${job.id}/edit`}>Edit</a><button className="danger" onClick={() => setConfirmingRemove(true)}>Remove</button></>} />
    <Dialog open={confirmingRemove} onOpenChange={setConfirmingRemove} title="Remove this job?" description="Finished runs stay in durable history. The schedule will stop running."><div className="dialog-actions"><button className="danger" onClick={() => void act("remove")}>Remove job</button><button data-dialog-cancel onClick={() => setConfirmingRemove(false)}>Keep job</button></div></Dialog>
    {feedback && <Feedback kind="error">{feedback}</Feedback>}
    {definition ? <DefinitionSummary definition={definition} /> : <Feedback kind="error">Definition is unreadable.</Feedback>}
    {why && <section className="card"><h2>Why</h2><p>{why.explanation}</p><dl className="facts"><dt>Next occurrence</dt><dd>{why.next_occurrence ?? "none"}</dd><dt>Overlap policy</dt><dd>{why.overlap}</dd><dt>Daemon</dt><dd>{why.daemon_running ? "running" : "not running"}</dd></dl></section>}
    <section className="card"><h2>Recent runs</h2>{recent.length ? <div className="table-scroll"><table><thead><tr><th>Run</th><th>Requested</th><th>State</th></tr></thead><tbody>{recent.map((run) => <tr key={run.id}><td><a href={`#/runs/${run.id}`}>{run.id.slice(0, 8)}</a></td><td>{new Date(run.requested_at_us / 1000).toLocaleString()}</td><td>{run.state}</td></tr>)}</tbody></table></div> : <p>No runs yet.</p>}</section>
    <details className="card"><summary>Redacted definition JSON</summary><JsonViewer source={job.definition_json} label={`Redacted definition JSON for ${job.name}`} /></details>
  </>;
}

function DefinitionSummary({ definition }: { definition: Definition }) {
  const schedule = definition.schedule.kind === "cron" ? `${definition.schedule.expression} · ${definition.schedule.timezone.mode === "iana" ? definition.schedule.timezone.name : "local"}` : definition.schedule.kind === "every" ? `Every ${humanDuration(definition.schedule.interval)} from ${new Date(definition.schedule.anchor / 1000).toLocaleString()}` : `Once at ${new Date(definition.schedule.at / 1000).toLocaleString()}`;
  const target = definition.target.kind === "process" ? `${definition.target.executable} ${definition.target.args.join(" ")}` : definition.target.kind === "shell" ? `${definition.target.shell}: ${definition.target.command}` : `${definition.target.method} ${definition.target.url}`;
  return <div className="definition-grid"><section className="card"><h2>Schedule</h2><p>{schedule}</p></section><section className="card"><h2>Target</h2><p>{target}</p></section><section className="card"><h2>Environment</h2><dl className="facts"><dt>Working directory</dt><dd><code>{definition.cwd}</code></dd><dt>Environment file</dt><dd>{definition.environment.file ?? "none"}</dd><dt>PATH override</dt><dd>{definition.environment.path ?? "none"}</dd><dt>Values</dt><dd>{Object.keys(definition.environment.values).join(", ") || "none"}</dd></dl></section><section className="card"><h2>Policy</h2><dl className="facts"><dt>Overlap</dt><dd>{definition.policy.overlap}</dd><dt>Missed runs</dt><dd>{definition.policy.missed_run}</dd><dt>Retries</dt><dd>{definition.policy.retries}</dd><dt>Timeout</dt><dd>{definition.policy.timeout === null ? "off" : humanDuration(definition.policy.timeout)}</dd></dl></section></div>;
}
