import { useEffect, useState } from "react";
import { api } from "../api";
import { ByteSizeInput, DurationInput, Feedback, Field, ThemeControl } from "../components";
import { byteEquivalent } from "../domain/byteSize";
import { formatDuration } from "../domain/duration";
import { PathRows } from "../formControls";
import { Dialog, RouteHeader } from "../ui";

export type SettingsData = {
  global_concurrency: number;
  execution_path: string;
  run_retention_count: number;
  run_retention_age_us: number | null;
  output_limit_bytes: number;
  per_run_output_limit_bytes: number;
  environment: Record<string, string>;
};
type DurableKey = Exclude<keyof SettingsData, "environment">;
export type SettingChange = { key: DurableKey | `environment.${string}`; value: string; human: string; field: string };

const durableOrder: DurableKey[] = ["global_concurrency", "execution_path", "run_retention_count", "run_retention_age_us", "output_limit_bytes", "per_run_output_limit_bytes"];

function durationDescription(value: number | null) {
  if (value === null) return "no age limit";
  const exact = formatDuration(value);
  const names = { s: "seconds", m: "minutes", h: "hours", d: "days" };
  return `${exact.magnitude} ${names[exact.unit]}`;
}

function pathCount(value: string) { return value ? value.split(":").length : 0; }
function valueFor(key: DurableKey, data: SettingsData) { return key === "run_retention_age_us" && data[key] === null ? "none" : String(data[key]); }

export function collectSettingsChanges(saved: SettingsData, draft: SettingsData, envName: string, envValue: string): SettingChange[] {
  const changes: SettingChange[] = [];
  for (const key of durableOrder) {
    if (saved[key] === draft[key]) continue;
    const human = key === "global_concurrency" ? `Global concurrency: ${saved[key]} → ${draft[key]}`
      : key === "execution_path" ? `Execution search paths: ${pathCount(saved[key])} → ${pathCount(draft[key])} ordered directories`
      : key === "run_retention_count" ? `Runs retained per job: ${saved[key].toLocaleString()} → ${draft[key].toLocaleString()}`
      : key === "run_retention_age_us" ? `Retention age: ${durationDescription(saved[key])} → ${durationDescription(draft[key])}`
      : key === "output_limit_bytes" ? `Total retained output: ${byteEquivalent(saved[key])} → ${byteEquivalent(draft[key])}`
      : `Output retained per run: ${byteEquivalent(saved[key])} → ${byteEquivalent(draft[key])}`;
    changes.push({ key, value: valueFor(key, draft), human, field: `settings-${key.replaceAll("_", "-")}` });
  }
  if (envName || envValue) changes.push({ key: `environment.${envName}`, value: envValue, human: `Environment ${envName || "(missing name)"} will be ${Object.hasOwn(saved.environment, envName) ? "replaced" : "added"}; its value remains redacted.`, field: "settings-environment-name" });
  return changes;
}

function validationFailure(draft: SettingsData, envName: string, envValue: string) {
  if (!Number.isInteger(draft.global_concurrency) || draft.global_concurrency < 1 || draft.global_concurrency > 64) return { field: "settings-global-concurrency", message: "Global concurrency must be a whole number from 1 through 64." };
  if (!Number.isInteger(draft.run_retention_count) || draft.run_retention_count < 0) return { field: "settings-run-retention-count", message: "Runs retained per job must be a non-negative whole number." };
  if (envName || envValue) {
    if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(envName) || envName.startsWith("LOCRON_")) return { field: "settings-environment-name", message: "Environment name must use shell variable grammar and cannot start LOCRON_." };
  }
  return null;
}

export function Settings() {
  const [draft, setDraft] = useState<SettingsData | null>(null);
  const [saved, setSaved] = useState<SettingsData | null>(null);
  const [feedback, setFeedback] = useState("");
  const [pending, setPending] = useState<SettingChange[] | null>(null);
  const [envName, setEnvName] = useState("");
  const [envValue, setEnvValue] = useState("");
  const changes = draft && saved ? collectSettingsChanges(saved, draft, envName, envValue) : [];
  const dirty = changes.length > 0;

  useEffect(() => {
    const controller = new AbortController();
    api.get<SettingsData>("/api/v1/settings", { signal: controller.signal }).then(({ data }) => { setDraft(structuredClone(data)); setSaved(structuredClone(data)); }).catch((issue) => setFeedback(issue.message));
    return () => controller.abort();
  }, []);
  useEffect(() => {
    if (!dirty) return;
    const protect = (event: BeforeUnloadEvent) => { event.preventDefault(); event.returnValue = ""; };
    window.addEventListener("beforeunload", protect);
    return () => window.removeEventListener("beforeunload", protect);
  }, [dirty]);

  if (!draft || !saved) return <><h1>Settings</h1><Feedback kind={feedback ? "error" : "muted"}>{feedback || "Loading settings…"}</Feedback></>;

  const reveal = (id: string) => {
    const schedule = typeof requestAnimationFrame === "function" ? requestAnimationFrame : (callback: FrameRequestCallback) => window.setTimeout(callback, 0);
    schedule(() => { const element = document.getElementById(id); element?.scrollIntoView?.({ block: "center" }); element?.focus(); });
  };
  const update = <K extends keyof SettingsData>(key: K, value: SettingsData[K]) => setDraft({ ...draft, [key]: value });
  const discard = () => { setDraft(structuredClone(saved)); setEnvName(""); setEnvValue(""); setPending(null); setFeedback("Discarded unsaved durable changes. Browser appearance is unchanged."); };
  const review = async () => {
    const validationRoot = document.getElementById("main-content") ?? document;
    const invalidControl = validationRoot.querySelector<HTMLElement>("[aria-invalid=true]");
    if (invalidControl) { setFeedback("Fix the invalid field before reviewing changes."); invalidControl.focus(); return; }
    const invalid = validationFailure(draft, envName, envValue);
    if (invalid) { setFeedback(invalid.message); reveal(invalid.field); return; }
    const planned = collectSettingsChanges(saved, draft, envName, envValue);
    if (!planned.length) return;
    setFeedback(`Validating ${planned.length} durable change${planned.length === 1 ? "" : "s"}…`);
    const failures: Array<{ change: SettingChange; message: string }> = [];
    for (const change of planned) {
      try { await api.put(`/api/v1/settings/${encodeURIComponent(change.key)}`, { value: change.value, dry_run: "1" }); }
      catch (issue) { failures.push({ change, message: (issue as Error).message }); }
    }
    if (failures.length) { setFeedback(`Could not validate ${failures.map(({ change, message }) => `${change.key}: ${message}`).join("; ")}`); reveal(failures[0]!.change.field); return; }
    setPending(planned);
    setFeedback("");
  };
  const apply = async () => {
    if (!pending) return;
    const planned = pending;
    const intended = structuredClone(draft);
    const failures: Array<{ change: SettingChange; message: string }> = [];
    for (const change of planned) {
      try { await api.put(`/api/v1/settings/${encodeURIComponent(change.key)}`, { value: change.value }); }
      catch (issue) { failures.push({ change, message: (issue as Error).message }); }
    }
    setPending(null);
    try {
      const { data: canonical } = await api.get<SettingsData>("/api/v1/settings");
      const next = structuredClone(canonical);
      for (const { change } of failures) if (!change.key.startsWith("environment.")) (next as unknown as Record<string, unknown>)[change.key] = intended[change.key as DurableKey];
      setSaved(structuredClone(canonical));
      setDraft(next);
      if (!failures.some(({ change }) => change.key.startsWith("environment."))) { setEnvName(""); setEnvValue(""); }
      const savedCount = planned.length - failures.length;
      setFeedback(failures.length ? `Saved ${savedCount} of ${planned.length} changes. Failed ${failures.map(({ change, message }) => `${change.key}: ${message}`).join("; ")}. Remaining changes stay editable.` : `Saved ${savedCount} durable change${savedCount === 1 ? "" : "s"}.`);
    } catch (issue) {
      setFeedback(`Applied ${planned.length - failures.length} of ${planned.length} changes, but canonical refresh failed: ${(issue as Error).message}. Reload before retrying.`);
    }
    reveal("settings-feedback");
  };
  const removeEnvironment = async (name: string) => {
    try {
      await api.delete(`/api/v1/settings/${encodeURIComponent(`environment.${name}`)}`);
      const environment = { ...draft.environment }; delete environment[name];
      const savedEnvironment = { ...saved.environment }; delete savedEnvironment[name];
      setDraft({ ...draft, environment }); setSaved({ ...saved, environment: savedEnvironment });
      if (envName === name) { setEnvName(""); setEnvValue(""); }
      setFeedback(`Removed environment ${name}.`);
    } catch (issue) { setFeedback((issue as Error).message); reveal("settings-feedback"); }
  };
  const failureFeedback = feedback.includes("Failed") || feedback.includes("failed") || feedback.startsWith("Could not") || feedback.startsWith("Applied");

  return <>
    <RouteHeader title="Settings" description="Appearance stays in this browser. Durable scheduler changes are reviewed and applied together." />
    {feedback && <div id="settings-feedback" tabIndex={-1}><Feedback kind={feedback.startsWith("Saved") && !failureFeedback || feedback.startsWith("Removed") || feedback.startsWith("Discarded") ? "state-good" : failureFeedback ? "error" : "muted"}>{feedback}</Feedback></div>}
    <Dialog open={pending !== null} onOpenChange={(open) => { if (!open) setPending(null); }} title="Review durable changes" description="These scheduler settings are stored on this machine and applied in the listed order.">{pending && <><ul className="review-list">{pending.map((change) => <li key={change.key}>{change.human}</li>)}</ul><p className="field-help">Environment values remain redacted. If one key fails, Locron reports it and refreshes the durable snapshot.</p><div className="dialog-actions"><button className="primary" type="button" onClick={() => void apply()}>Apply {pending.length} change{pending.length === 1 ? "" : "s"}</button><button data-dialog-cancel type="button" onClick={() => setPending(null)}>Cancel</button></div></>}</Dialog>
    <section className="card"><h2>Appearance</h2><ThemeControl name="settings-theme" /></section>
    <section className="card"><h2>Execution</h2>
      <Field id="settings-global-concurrency" label="Global concurrency" description="Maximum simultaneous runs, from 1 through 64.">{({ id, describedBy }) => <input id={id} type="number" min="1" max="64" value={draft.global_concurrency} aria-describedby={describedBy} aria-invalid={!Number.isInteger(draft.global_concurrency) || draft.global_concurrency < 1 || draft.global_concurrency > 64} onChange={(event) => update("global_concurrency", Number(event.target.value))} />}</Field>
      <PathRows label="Execution search paths" value={draft.execution_path} onChange={(value) => update("execution_path", value)} />
    </section>
    <section className="card"><h2>Retention & output</h2>
      <Field id="settings-run-retention-count" label="Runs retained per job" description="Count-based retention remains active alongside age.">{({ id, describedBy }) => <input id={id} type="number" min="0" value={draft.run_retention_count} aria-describedby={describedBy} aria-invalid={!Number.isInteger(draft.run_retention_count) || draft.run_retention_count < 0} onChange={(event) => update("run_retention_count", Number(event.target.value))} />}</Field>
      <label className="check"><input type="checkbox" checked={draft.run_retention_age_us === null} onChange={(event) => update("run_retention_age_us", event.target.checked ? null : 30 * 24 * 60 * 60 * 1_000_000)} />No age limit</label>
      {draft.run_retention_age_us !== null && <DurationInput label="Run retention age" value={draft.run_retention_age_us} onChange={(value) => update("run_retention_age_us", value)} />}
      <ByteSizeInput label="Total retained output" value={draft.output_limit_bytes} consequence="Zero makes completed output immediately eligible for pruning." onChange={(value) => update("output_limit_bytes", value)} />
      <ByteSizeInput label="Output retained per run" value={draft.per_run_output_limit_bytes} consequence="Zero drains output but retains no payload and records truncation." onChange={(value) => update("per_run_output_limit_bytes", value)} />
    </section>
    <section className="card"><h2>Environment</h2><p>Values remain redacted after save. Entering an existing name stages a replacement in the aggregate review.</p>
      {Object.keys(draft.environment).map((name) => <div className="kv-row" key={name}><code>{name}</code><button type="button" onClick={() => { setEnvName(name); setEnvValue(""); }}>Replace</button><button type="button" onClick={() => void removeEnvironment(name)}>Remove</button></div>)}
      <Field id="settings-environment-name" label="Variable name" description="Shell variable grammar; LOCRON_ is reserved.">{({ id, describedBy }) => <input id={id} value={envName} aria-describedby={describedBy} onChange={(event) => setEnvName(event.target.value)} />}</Field>
      <Field id="settings-environment-value" label="Variable value" description="The saved value is never returned to the browser.">{({ id, describedBy }) => <input id={id} type="password" value={envValue} aria-describedby={describedBy} onChange={(event) => setEnvValue(event.target.value)} />}</Field>
    </section>
    <div className="settings-actions"><div><strong>{dirty ? `${changes.length} unsaved durable change${changes.length === 1 ? "" : "s"}` : "No unsaved durable changes"}</strong><p className="field-help">Appearance applies immediately and is never included here.</p></div><div><button type="button" onClick={discard} disabled={!dirty}>Discard changes</button><button className="primary" type="button" onClick={() => void review()} disabled={!dirty}>Review changes</button></div></div>
  </>;
}
