import { useEffect, useState } from "react";
import { api } from "../api";
import { ByteSizeInput, DurationInput, Feedback, Field, ThemeControl } from "../components";
import { byteEquivalent } from "../domain/byteSize";
import { formatDuration } from "../domain/duration";
import { PathRows } from "../formControls";
import { Dialog, RouteHeader } from "../ui";

type SettingsData = {
  global_concurrency: number;
  execution_path: string;
  run_retention_count: number;
  run_retention_age_us: number | null;
  output_limit_bytes: number;
  per_run_output_limit_bytes: number;
  environment: Record<string, string>;
};
type Pending = { key: keyof SettingsData | string; value: string; human: string };

function durationDescription(value: number) {
  const exact = formatDuration(value);
  const names = { s: "seconds", m: "minutes", h: "hours", d: "days" };
  return `${exact.magnitude} ${names[exact.unit]}`;
}

export function Settings() {
  const [data, setData] = useState<SettingsData | null>(null);
  const [saved, setSaved] = useState<SettingsData | null>(null);
  const [feedback, setFeedback] = useState("");
  const [pending, setPending] = useState<Pending | null>(null);
  const [envName, setEnvName] = useState("");
  const [envValue, setEnvValue] = useState("");
  useEffect(() => {
    const controller = new AbortController();
    api.get<SettingsData>("/api/v1/settings", { signal: controller.signal }).then(({ data: settings }) => { setData(settings); setSaved(structuredClone(settings)); }).catch((issue) => setFeedback(issue.message));
    return () => controller.abort();
  }, []);
  if (!data) return <><h1>Settings</h1><Feedback kind={feedback ? "error" : "muted"}>{feedback || "Loading settings…"}</Feedback></>;
  const reveal = (id: string) => {
    const schedule = typeof requestAnimationFrame === "function" ? requestAnimationFrame : (callback: FrameRequestCallback) => window.setTimeout(callback, 0);
    schedule(() => { const element = document.getElementById(id); element?.scrollIntoView?.({ block: "center" }); element?.focus(); });
  };
  const update = <K extends keyof SettingsData>(key: K, value: SettingsData[K]) => setData({ ...data, [key]: value });
  const review = async (candidate: Pending) => {
    const invalid = document.querySelector<HTMLElement>("#main-content [aria-invalid=true]");
    if (invalid) { setFeedback("Fix the invalid field before reviewing this change."); invalid.focus(); return; }
    setFeedback("Validating change…");
    try { await api.put(`/api/v1/settings/${encodeURIComponent(candidate.key)}`, { value: candidate.value, dry_run: "1" }); setPending(candidate); setFeedback(""); }
    catch (issue) { setPending(null); setFeedback((issue as Error).message); reveal("settings-feedback"); }
  };
  const apply = async () => {
    if (!pending) return;
    try {
      await api.put(`/api/v1/settings/${encodeURIComponent(pending.key)}`, { value: pending.value });
      if (pending.key.startsWith("environment.")) {
        const name = pending.key.slice("environment.".length);
        setData({ ...data, environment: { ...data.environment, [name]: "<redacted>" } });
        setEnvName(""); setEnvValue("");
      } else if (saved && pending.key in data) setSaved({ ...saved, [pending.key]: data[pending.key as keyof SettingsData] } as SettingsData);
      setFeedback(`Saved ${pending.human}.`); setPending(null); reveal("settings-feedback");
    }
    catch (issue) { setFeedback((issue as Error).message); reveal("settings-feedback"); }
  };
  const reviewEnvironment = () => {
    if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(envName) || envName.startsWith("LOCRON_")) { setFeedback("Environment name must use shell variable grammar and cannot start LOCRON_."); return; }
    void review({ key: `environment.${envName}`, value: envValue, human: `environment ${envName}; its value will remain redacted` });
  };
  const removeEnvironment = async (name: string) => {
    try {
      await api.delete(`/api/v1/settings/${encodeURIComponent(`environment.${name}`)}`);
      const environment = { ...data.environment }; delete environment[name]; setData({ ...data, environment });
      setFeedback(`Removed environment ${name}.`);
    } catch (issue) { setFeedback((issue as Error).message); reveal("settings-feedback"); }
  };
  return <>
    <RouteHeader title="Settings" description="Appearance stays in this browser. Scheduler changes validate, show their consequence, then wait for explicit apply." />
    {feedback && <div id="settings-feedback" tabIndex={-1}><Feedback kind={feedback.startsWith("Saved") || feedback.startsWith("Removed") ? "state-good" : "error"}>{feedback}</Feedback></div>}
    <Dialog open={pending !== null} onOpenChange={(open) => { if (!open) setPending(null); }} title="Review durable change" description="This changes scheduler behavior and is stored on this machine.">{pending && <><p className="review-change">{pending.human}</p><div className="dialog-actions"><button className="primary" type="button" onClick={() => void apply()}>Apply change</button><button data-dialog-cancel type="button" onClick={() => setPending(null)}>Cancel</button></div></>}</Dialog>
    <section className="card"><h2>Appearance</h2><ThemeControl name="settings-theme" /></section>
    <section className="card"><h2>Execution</h2>
      <Field label="Global concurrency" description="Maximum simultaneous runs, from 1 through 64.">{({ id, describedBy }) => <input id={id} type="number" min="1" max="64" value={data.global_concurrency} aria-describedby={describedBy} onChange={(event) => update("global_concurrency", Number(event.target.value))} />}</Field>
      <button type="button" onClick={() => void review({ key: "global_concurrency", value: String(data.global_concurrency), human: `global concurrency ${data.global_concurrency}` })}>Review concurrency</button>
      <PathRows label="Execution search paths" value={data.execution_path} onChange={(value) => update("execution_path", value)} />
      <button type="button" onClick={() => void review({ key: "execution_path", value: data.execution_path, human: `execution search path in the displayed order` })}>Review paths</button>
    </section>
    <section className="card"><h2>Retention & output</h2>
      <Field label="Runs retained per job" description="Count-based retention remains active alongside age.">{({ id, describedBy }) => <input id={id} type="number" min="0" value={data.run_retention_count} aria-describedby={describedBy} onChange={(event) => update("run_retention_count", Number(event.target.value))} />}</Field>
      <button type="button" onClick={() => void review({ key: "run_retention_count", value: String(data.run_retention_count), human: `runs retained per job: ${(saved?.run_retention_count ?? data.run_retention_count).toLocaleString()} → ${data.run_retention_count.toLocaleString()}` })}>Review count retention</button>
      <label className="check"><input type="checkbox" checked={data.run_retention_age_us === null} onChange={(event) => update("run_retention_age_us", event.target.checked ? null : 30 * 24 * 60 * 60 * 1_000_000)} />No age limit</label>
      {data.run_retention_age_us !== null && <DurationInput label="Run retention age" value={data.run_retention_age_us} onChange={(value) => update("run_retention_age_us", value)} />}
      <button type="button" onClick={() => void review({ key: "run_retention_age_us", value: data.run_retention_age_us === null ? "none" : String(data.run_retention_age_us), human: `retention age: ${saved?.run_retention_age_us === null ? "no age limit" : durationDescription(saved?.run_retention_age_us ?? data.run_retention_age_us ?? 0)} → ${data.run_retention_age_us === null ? "no age limit" : durationDescription(data.run_retention_age_us)}` })}>Review age retention</button>
      <ByteSizeInput label="Total retained output" value={data.output_limit_bytes} consequence="Zero makes completed output immediately eligible for pruning." onChange={(value) => update("output_limit_bytes", value)} />
      <button type="button" onClick={() => void review({ key: "output_limit_bytes", value: String(data.output_limit_bytes), human: `total retained output: ${byteEquivalent(saved?.output_limit_bytes ?? data.output_limit_bytes)} → ${byteEquivalent(data.output_limit_bytes)}` })}>Review total output</button>
      <ByteSizeInput label="Output retained per run" value={data.per_run_output_limit_bytes} consequence="Zero drains output but retains no payload and records truncation." onChange={(value) => update("per_run_output_limit_bytes", value)} />
      <button type="button" onClick={() => void review({ key: "per_run_output_limit_bytes", value: String(data.per_run_output_limit_bytes), human: `output retained per run: ${byteEquivalent(saved?.per_run_output_limit_bytes ?? data.per_run_output_limit_bytes)} → ${byteEquivalent(data.per_run_output_limit_bytes)}` })}>Review per-run output</button>
    </section>
    <section className="card"><h2>Environment</h2><p>Values remain redacted after save. Entering an existing name replaces its value after review.</p>
      {Object.keys(data.environment).map((name) => <div className="kv-row" key={name}><code>{name}</code><button type="button" onClick={() => { setEnvName(name); setEnvValue(""); }}>Replace</button><button type="button" onClick={() => void removeEnvironment(name)}>Remove</button></div>)}
      <Field label="Variable name" description="Shell variable grammar; LOCRON_ is reserved.">{({ id }) => <input id={id} value={envName} onChange={(event) => setEnvName(event.target.value)} />}</Field>
      <Field label="Variable value" description="The saved value is never returned to the browser.">{({ id }) => <input id={id} type="password" value={envValue} onChange={(event) => setEnvValue(event.target.value)} />}</Field>
      <button type="button" onClick={reviewEnvironment}>{Object.hasOwn(data.environment, envName) ? "Review replacement" : "Review new environment value"}</button>
    </section>
  </>;
}
