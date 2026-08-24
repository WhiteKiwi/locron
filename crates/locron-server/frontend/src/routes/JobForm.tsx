import { useEffect, useState } from "react";
import { api } from "../api";
import { DurationInput, Feedback, Field, InstantInput } from "../components";
import { Choice, KeyValueRows, PathRows, Section, ValidationSummary } from "../formControls";
import type { Definition, HttpTarget, Job, SecretValue, Target } from "../types";
import { LocronSelect, RouteHeader } from "../ui";

type ValidationError = { field: string; message: string };
type JobDraft = { name: string; description: string; tags: string; enabled: boolean; definition: Definition };
const FORM_SECTIONS = ["Identity", "Schedule", "Target", "Environment", "Policy", "Review"] as const;

export const defaultDefinition = (): Definition => ({
  schedule: { kind: "cron", expression: "", timezone: { mode: "local" } },
  target: { kind: "process", executable: "", args: [] },
  cwd: "",
  environment: { values: {} },
  policy: { overlap: "skip", missed_run: "skip", start_deadline: null, catch_up_limit: 100, retries: 0, retry_delay: 10_000_000, retry_cap: 300_000_000, backoff: "exponential", retry_timeout: false, timeout: 60_000_000, termination_grace: 5_000_000, per_job_concurrency: 1 },
});

function parseTags(raw: string) {
  if (!raw.trim()) return [];
  const values = raw.split(",").map((value) => value.trim());
  if (values.some((value) => !value)) throw new Error("Tags must be nonempty comma-separated names");
  return values;
}

export function parseSuccessStatuses(raw: string) {
  if (!raw.trim()) return [];
  return raw.split(",").map((value) => value.trim()).map((value) => {
    if (!/^\d+$/.test(value)) throw new Error("Success statuses must be whole numbers separated by commas");
    const status = Number(value);
    if (status < 100 || status > 599) throw new Error("Success statuses must be from 100 through 599");
    return status;
  });
}

function validateSecretRows(value: Record<string, SecretValue>, label: string, errors: ValidationError[]) {
  for (const [name, source] of Object.entries(value)) {
    if (!name.trim()) errors.push({ field: "http-headers", message: `${label} names cannot be empty` });
    if (!source.value) errors.push({ field: "http-headers", message: `${label} ${name || "row"} needs a ${source.source === "environment" ? "variable name" : "value"}` });
    if (source.source === "inline" && source.value === "<redacted>") errors.push({ field: "http-headers", message: `${label} ${name || "row"} is redacted; load values, replace it, or remove it` });
  }
}

export function buildJobPayload(draft: JobDraft, statusText: string) {
  const errors: ValidationError[] = [];
  if (!draft.name.trim()) errors.push({ field: "job-name", message: "Name is required" });
  let tags: string[] = [];
  try { tags = parseTags(draft.tags); } catch (issue) { errors.push({ field: "job-tags", message: (issue as Error).message }); }
  const definition = structuredClone(draft.definition);
  if (definition.policy.overlap !== "allow") definition.policy.per_job_concurrency = 1;
  if (!definition.cwd.trim()) errors.push({ field: "job-cwd", message: "Working directory is required" });
  if (definition.schedule.kind === "cron") {
    if (!definition.schedule.expression.trim()) errors.push({ field: "cron-expression", message: "Cron expression is required" });
    if (definition.schedule.timezone.mode === "iana" && !definition.schedule.timezone.name?.trim()) errors.push({ field: "cron-timezone", message: "IANA timezone is required" });
  } else if (definition.schedule.kind === "every" && definition.schedule.interval <= 0) errors.push({ field: "schedule-interval", message: "Elapsed interval must be greater than zero" });
  const target = definition.target;
  if (target.kind === "process") {
    if (!target.executable.trim()) errors.push({ field: "process-executable", message: "Executable is required" });
    if (target.args.some((argument) => !argument.trim())) errors.push({ field: "process-arguments", message: "Arguments cannot contain blank lines" });
  } else if (target.kind === "shell") {
    if (!target.command.trim()) errors.push({ field: "shell-command", message: "Shell command is required" });
    if (!target.shell.trim()) errors.push({ field: "shell-path", message: "Shell path is required" });
  } else {
    if (!target.url.trim()) errors.push({ field: "http-url", message: "HTTP URL is required" });
    try { target.success_statuses = parseSuccessStatuses(statusText); } catch (issue) { errors.push({ field: "http-statuses", message: (issue as Error).message }); }
    if (target.body === "<redacted>") errors.push({ field: "http-body", message: "HTTP body is redacted; load values, replace it, or clear it" });
    if (target.body && target.body_file) errors.push({ field: "http-body", message: "Choose an inline body or a body file, not both" });
    validateSecretRows(target.headers, "HTTP header", errors);
  }
  for (const [name, value] of Object.entries(definition.environment.values)) {
    if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(name)) errors.push({ field: "environment-values", message: `Environment name ${name || "(empty)"} is invalid` });
    if (value === "<redacted>") errors.push({ field: "environment-values", message: `Environment ${name} is redacted; load values, replace it, or remove it` });
  }
  if (definition.environment.path !== undefined && definition.environment.path.split(":").some((part) => !part.trim())) errors.push({ field: "environment-path", message: "PATH override cannot contain an empty directory" });
  const concurrencyValid = definition.policy.overlap === "allow" ? definition.policy.per_job_concurrency >= 2 && definition.policy.per_job_concurrency <= 64 : definition.policy.per_job_concurrency === 1;
  if (!concurrencyValid) errors.push({ field: "job-concurrency", message: definition.policy.overlap === "allow" ? "Allow requires concurrency from 2 through the global limit" : "Skip and Replace require concurrency 1" });
  if (definition.policy.catch_up_limit < 1 || definition.policy.catch_up_limit > 1000) errors.push({ field: "catch-up-limit", message: "Catch-up limit must be from 1 through 1000" });
  if (definition.policy.retries < 0 || definition.policy.retries > 10) errors.push({ field: "retries", message: "Retries must be from 0 through 10" });
  return { errors, payload: { name: draft.name.trim(), description: draft.description.trim() || null, tags, enabled: draft.enabled, definition } };
}

export function JobForm({ reference }: { reference?: string }) {
  const [draft, setDraft] = useState<JobDraft>({ name: "", description: "", tags: "", enabled: true, definition: defaultDefinition() });
  const [statusText, setStatusText] = useState("");
  const [feedback, setFeedback] = useState("");
  const [errors, setErrors] = useState<ValidationError[]>([]);
  const [acknowledged, setAcknowledged] = useState(false);
  const [preview, setPreview] = useState<string[]>([]);
  const [activeSection, setActiveSection] = useState<(typeof FORM_SECTIONS)[number]>("Identity");
  const editing = Boolean(reference);
  useEffect(() => {
    if (!reference) return;
    const controller = new AbortController();
    api.get<Job>(`/api/v1/jobs/${encodeURIComponent(reference)}`, { signal: controller.signal }).then(({ data }) => {
      const definition = JSON.parse(data.definition_json) as Definition;
      let tags = data.tags ?? [];
      if (!tags.length && data.tags_json) { try { tags = JSON.parse(data.tags_json) as string[]; } catch { tags = []; } }
      setDraft({ name: data.name, description: data.description ?? "", tags: tags.join(", "), enabled: data.enabled, definition });
      setStatusText(definition.target.kind === "http" ? definition.target.success_statuses.join(", ") : "");
    }).catch((issue) => setFeedback(issue.message));
    return () => controller.abort();
  }, [reference]);
  useEffect(() => {
    if (!("IntersectionObserver" in window)) return;
    const observer = new IntersectionObserver((entries) => {
      const visible = entries.filter((entry) => entry.isIntersecting).sort((first, second) => first.boundingClientRect.top - second.boundingClientRect.top)[0];
      if (!visible) return;
      const title = FORM_SECTIONS.find((section) => visible.target.id === `job-${section.toLowerCase()}`);
      if (title) setActiveSection(title);
    }, { rootMargin: "-20% 0px -65%", threshold: 0 });
    FORM_SECTIONS.forEach((section) => {
      const element = document.getElementById(`job-${section.toLowerCase()}`);
      if (element) observer.observe(element);
    });
    return () => observer.disconnect();
  }, []);
  const patch = (change: Partial<JobDraft>) => setDraft((current) => ({ ...current, ...change }));
  const setDefinition = (definition: Definition) => patch({ definition });
  const submit = async (dryRun: boolean) => {
    const invalidQuantity = document.querySelector<HTMLElement>("#job-form [aria-invalid=true]");
    if (invalidQuantity) { setFeedback("Fix the invalid field before review or save."); invalidQuantity.focus(); return; }
    const result = buildJobPayload(draft, statusText);
    setErrors(result.errors);
    if (result.errors.length) {
      setTimeout(() => document.getElementById(result.errors[0]!.field)?.focus(), 0);
      return;
    }
    try {
      const body = { ...result.payload, dry_run: dryRun ? "1" : undefined };
      const response = editing ? await api.put<{ name: string }>(`/api/v1/jobs/${encodeURIComponent(reference!)}`, body) : await api.post<{ name: string }>("/api/v1/jobs", body);
      if (dryRun) setFeedback("Dry-run passed. No durable state changed.");
      else location.hash = `#/jobs/${encodeURIComponent(response.data.name)}`;
    } catch (issue) { setFeedback((issue as Error).message); }
  };
  const loadSecrets = async () => {
    try {
      const { data } = await api.get<{ jobs: Array<{ name: string; definition: Definition }> }>(`/api/v1/export?jobs=${encodeURIComponent(draft.name)}&include-values=1&acknowledge-plaintext=1`);
      const job = data.jobs.find((item) => item.name === draft.name);
      if (job) setDefinition(job.definition);
      setFeedback("Plaintext values loaded for this browser session.");
    } catch (issue) { setFeedback((issue as Error).message); }
  };
  const previewSchedule = async () => {
    try { const { data } = await api.post<{ occurrences: string[] }>("/api/v1/schedule/preview", { schedule: draft.definition.schedule, count: 5 }); setPreview(data.occurrences); setFeedback(""); }
    catch (issue) { setPreview([]); setFeedback((issue as Error).message); }
  };
  return <form id="job-form" onSubmit={(event) => { event.preventDefault(); void submit(false); }}>
    <RouteHeader title={editing ? "Edit job" : "New job"} description="Identity, schedule, target, environment, and policy — with storage encodings kept behind human controls." />
    <div className="form-layout"><nav className="form-section-nav" aria-label="Job form sections">{FORM_SECTIONS.map((section) => <button className="link" type="button" key={section} aria-current={activeSection === section ? "step" : undefined} onClick={() => { setActiveSection(section); document.getElementById(`job-${section.toLowerCase()}`)?.scrollIntoView({ behavior: "smooth", block: "start" }); }}>{section}</button>)}</nav>
    <div className="form-content"><ValidationSummary errors={errors} />
      {editing && <section className="notice"><label className="check"><input type="checkbox" checked={acknowledged} onChange={(event) => setAcknowledged(event.target.checked)} />I understand loading plaintext secrets exposes them in this browser session.</label><button type="button" disabled={!acknowledged} onClick={() => void loadSecrets()}>Load current secret values</button></section>}
      <Section title="Identity"><Field id="job-name" label="Name" error={errors.find((error) => error.field === "job-name")?.message}>{({ id, describedBy, invalid }) => <input id={id} value={draft.name} aria-describedby={describedBy} aria-invalid={invalid} onChange={(event) => patch({ name: event.target.value })} />}</Field><Field label="Description">{({ id }) => <textarea id={id} value={draft.description} onChange={(event) => patch({ description: event.target.value })} />}</Field><Field id="job-tags" label="Tags" description="Comma-separated names; empty tokens are rejected." error={errors.find((error) => error.field === "job-tags")?.message}>{({ id, describedBy, invalid }) => <input id={id} value={draft.tags} aria-describedby={describedBy} aria-invalid={invalid} onChange={(event) => patch({ tags: event.target.value })} />}</Field><label className="check"><input type="checkbox" checked={draft.enabled} onChange={(event) => patch({ enabled: event.target.checked })} />Enabled</label></Section>
      <ScheduleFields definition={draft.definition} setDefinition={setDefinition} preview={preview} onPreview={previewSchedule} errors={errors} />
      <TargetFields definition={draft.definition} setDefinition={setDefinition} statusText={statusText} setStatusText={setStatusText} errors={errors} />
      <EnvironmentFields definition={draft.definition} setDefinition={setDefinition} errors={errors} />
      <PolicyFields definition={draft.definition} setDefinition={setDefinition} errors={errors} />
      <Section title="Review" intro="Validate the complete definition without changing durable state, then save when it is ready."><dl className="facts"><dt>Job</dt><dd>{draft.name || "Unnamed job"}</dd><dt>Schedule</dt><dd>{draft.definition.schedule.kind}</dd><dt>Target</dt><dd>{draft.definition.target.kind}</dd><dt>Enabled</dt><dd>{draft.enabled ? "yes" : "no"}</dd></dl><button type="button" onClick={() => void submit(true)}>Run dry-run validation</button></Section>
      {feedback && <Feedback kind={feedback.startsWith("Dry-run") || feedback.includes("loaded") ? "state-good" : "error"}>{feedback}</Feedback>}
    </div></div>
    <div className="form-actions"><div><span className="form-status">{editing ? "Editing durable job" : "Creating a durable job"}</span></div><div><a className="button" href="#/jobs">Cancel</a><button type="button" onClick={() => void submit(true)}>Dry-run</button><button className="primary">Save job</button></div></div>
  </form>;
}

function ScheduleFields({ definition, setDefinition, preview, onPreview, errors }: { definition: Definition; setDefinition: (value: Definition) => void; preview: string[]; onPreview: () => void; errors: ValidationError[] }) {
  const schedule = definition.schedule;
  const setSchedule = (next: Definition["schedule"]) => setDefinition({ ...definition, schedule: next });
  return <Section title="Schedule" intro="Choose the scheduling model first; only fields for that model remain active.">
    <Choice legend="Schedule kind" value={schedule.kind} choices={[["cron", "Cron", "Calendar expression"], ["every", "Every", "Elapsed interval"], ["at", "At", "One local date and time"]]} onChange={(kind) => setSchedule(kind === "cron" ? { kind, expression: "", timezone: { mode: "local" } } : kind === "every" ? { kind, interval: 60_000_000, anchor: Date.now() * 1000 } : { kind, at: Date.now() * 1000 })} />
    {schedule.kind === "cron" && <><Field id="cron-expression" label="Cron expression" description="Five fields: minute hour day-of-month month day-of-week." error={errors.find((error) => error.field === "cron-expression")?.message}>{({ id, describedBy, invalid }) => <input id={id} value={schedule.expression} aria-describedby={describedBy} aria-invalid={invalid} onChange={(event) => setSchedule({ ...schedule, expression: event.target.value })} />}</Field><Choice legend="Timezone" value={schedule.timezone.mode} choices={[["local", "Machine local"], ["iana", "IANA timezone"]]} onChange={(mode) => setSchedule({ ...schedule, timezone: mode === "local" ? { mode } : { mode, name: "UTC" } })} />{schedule.timezone.mode === "iana" && <Field id="cron-timezone" label="IANA timezone" description="For example Asia/Seoul or Europe/Berlin." error={errors.find((error) => error.field === "cron-timezone")?.message}>{({ id, describedBy, invalid }) => <input id={id} value={schedule.timezone.name ?? ""} aria-describedby={describedBy} aria-invalid={invalid} onChange={(event) => setSchedule({ ...schedule, timezone: { mode: "iana", name: event.target.value } })} />}</Field>}</>}
    {schedule.kind === "every" && <><DurationInput label="Elapsed interval" value={schedule.interval} onChange={(value) => setSchedule({ ...schedule, interval: value ?? 1 })} /><InstantInput label="Anchor" value={schedule.anchor} timezone="local" onChange={(value) => setSchedule({ ...schedule, anchor: value })} /></>}
    {schedule.kind === "at" && <InstantInput label="Run at" value={schedule.at} timezone="local" onChange={(value) => setSchedule({ ...schedule, at: value })} />}
    <button type="button" onClick={onPreview}>Preview next 5 occurrences</button>{preview.length > 0 && <ol className="preview-list">{preview.map((occurrence) => <li key={occurrence}>{occurrence}</li>)}</ol>}
  </Section>;
}

function TargetFields({ definition, setDefinition, statusText, setStatusText, errors }: { definition: Definition; setDefinition: (value: Definition) => void; statusText: string; setStatusText: (value: string) => void; errors: ValidationError[] }) {
  const target = definition.target;
  const setTarget = (next: Target) => setDefinition({ ...definition, target: next });
  return <Section title="Target">
    <Choice legend="Target kind" value={target.kind} choices={[["process", "Process", "Executable and arguments"], ["shell", "Shell", "Command through a shell"], ["http", "HTTP", "Request an endpoint"]]} onChange={(kind) => setTarget(kind === "process" ? { kind, executable: "", args: [] } : kind === "shell" ? { kind, command: "", shell: "/bin/sh" } : { kind, method: "GET", url: "", success_statuses: [], follow_redirects: true, headers: {}, body: null })} />
    {target.kind === "process" && <><Field id="process-executable" label="Executable" description="Absolute path or name resolved through Settings execution paths." error={errors.find((error) => error.field === "process-executable")?.message}>{({ id, describedBy, invalid }) => <input id={id} value={target.executable} aria-describedby={describedBy} aria-invalid={invalid} onChange={(event) => setTarget({ ...target, executable: event.target.value })} />}</Field><Field id="process-arguments" label="Arguments" description="One argument per line; intentional empty arguments are not supported." error={errors.find((error) => error.field === "process-arguments")?.message}>{({ id, describedBy, invalid }) => <textarea id={id} value={target.args.join("\n")} aria-describedby={describedBy} aria-invalid={invalid} onChange={(event) => setTarget({ ...target, args: event.target.value ? event.target.value.split("\n") : [] })} />}</Field></>}
    {target.kind === "shell" && <><Field id="shell-command" label="Command" error={errors.find((error) => error.field === "shell-command")?.message}>{({ id, describedBy, invalid }) => <textarea id={id} value={target.command} aria-describedby={describedBy} aria-invalid={invalid} onChange={(event) => setTarget({ ...target, command: event.target.value })} />}</Field><Field id="shell-path" label="Shell path" description="Absolute executable path, for example /bin/sh." error={errors.find((error) => error.field === "shell-path")?.message}>{({ id, describedBy, invalid }) => <input id={id} value={target.shell} aria-describedby={describedBy} aria-invalid={invalid} onChange={(event) => setTarget({ ...target, shell: event.target.value })} />}</Field></>}
    {target.kind === "http" && <HttpFields target={target} setTarget={setTarget} statusText={statusText} setStatusText={setStatusText} errors={errors} />}
  </Section>;
}

function HttpFields({ target, setTarget, statusText, setStatusText, errors }: { target: HttpTarget; setTarget: (target: Target) => void; statusText: string; setStatusText: (value: string) => void; errors: ValidationError[] }) {
  const [bodySource, setBodySource] = useState<"inline" | "file">(target.body_file !== undefined ? "file" : "inline");
  const [inlineText, setInlineText] = useState(Array.isArray(target.body) ? new TextDecoder().decode(Uint8Array.from(target.body)) : "");
  const [fileText, setFileText] = useState(target.body_file ?? "");
  useEffect(() => {
    if (Array.isArray(target.body)) setInlineText(new TextDecoder().decode(Uint8Array.from(target.body)));
    if (target.body_file !== undefined) setFileText(target.body_file);
  }, [target.body, target.body_file]);
  return <>
    <Field label="HTTP method">{({ id }) => <LocronSelect id={id} label="HTTP method" value={target.method} onChange={(method) => setTarget({ ...target, method })} options={["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD"].map((method) => ({value:method,label:method}))} />}</Field>
    <Field id="http-url" label="URL" error={errors.find((error) => error.field === "http-url")?.message}>{({ id, describedBy, invalid }) => <input id={id} type="url" value={target.url} aria-describedby={describedBy} aria-invalid={invalid} onChange={(event) => setTarget({ ...target, url: event.target.value })} />}</Field>
    <Field id="http-statuses" label="Success statuses" description="Comma-separated HTTP statuses from 100 through 599. Empty uses Locron defaults." error={errors.find((error) => error.field === "http-statuses")?.message}>{({ id, describedBy, invalid }) => <input id={id} value={statusText} aria-describedby={describedBy} aria-invalid={invalid} onChange={(event) => setStatusText(event.target.value)} />}</Field>
    <label className="check"><input type="checkbox" checked={target.follow_redirects} onChange={(event) => setTarget({ ...target, follow_redirects: event.target.checked })} />Follow redirects</label>
    <KeyValueRows legend="HTTP headers" value={target.headers} secretSource onChange={(headers) => setTarget({ ...target, headers: headers as Record<string, SecretValue> })} />
    <Choice legend="HTTP body source" value={bodySource} choices={[["inline", "Inline UTF-8 text"], ["file", "Absolute file path"]]} onChange={(source) => { setBodySource(source); if (source === "file") setTarget({ ...target, body: null, body_file: fileText }); else { const { body_file: _bodyFile, ...withoutFile } = target; setTarget({ ...withoutFile, body: inlineText ? Array.from(new TextEncoder().encode(inlineText)) : null }); } }} />
    {bodySource === "file" ? <Field id="http-body" label="Body file">{({ id }) => <input id={id} value={fileText} onChange={(event) => { setFileText(event.target.value); setTarget({ ...target, body_file: event.target.value }); }} />}</Field> : <Field id="http-body" label="Inline body" description={target.body === "<redacted>" ? "This value is redacted. Load values, replace it, or clear it before saving." : "Stored as UTF-8 bytes. Empty means no body."} error={errors.find((error) => error.field === "http-body")?.message}>{({ id, describedBy, invalid }) => <textarea id={id} value={inlineText} placeholder={target.body === "<redacted>" ? "<redacted>" : undefined} aria-describedby={describedBy} aria-invalid={invalid} onChange={(event) => { setInlineText(event.target.value); setTarget({ ...target, body: event.target.value ? Array.from(new TextEncoder().encode(event.target.value)) : null }); }} />}</Field>}
  </>;
}

function EnvironmentFields({ definition, setDefinition, errors }: { definition: Definition; setDefinition: (value: Definition) => void; errors: ValidationError[] }) {
  const environment = definition.environment;
  const setEnvironment = (next: Definition["environment"]) => setDefinition({ ...definition, environment: next });
  return <Section title="Environment" intro="Optional files and PATH precede explicit NAME=value overrides.">
    <Field label="Working directory" id="job-cwd" description="Absolute directory used for execution." error={errors.find((error) => error.field === "job-cwd")?.message}>{({ id, describedBy, invalid }) => <input id={id} value={definition.cwd} aria-describedby={describedBy} aria-invalid={invalid} onChange={(event) => setDefinition({ ...definition, cwd: event.target.value })} />}</Field>
    <Field label="Environment file" description="Optional absolute dotenv-style file path.">{({ id }) => <input id={id} value={environment.file ?? ""} onChange={(event) => { const next = { ...environment }; if (event.target.value) next.file = event.target.value; else delete next.file; setEnvironment(next); }} />}</Field>
    <PathRows label="Job PATH override" value={environment.path ?? ""} onChange={(value) => { const next = { ...environment }; if (value) next.path = value; else delete next.path; setEnvironment(next); }} />
    {errors.find((error) => error.field === "environment-path") && <p className="error" id="environment-path" tabIndex={-1}>{errors.find((error) => error.field === "environment-path")?.message}</p>}
    <KeyValueRows legend="Environment values" value={environment.values} onChange={(values) => setEnvironment({ ...environment, values: values as Record<string, string> })} />
    {errors.find((error) => error.field === "environment-values") && <p className="error">{errors.find((error) => error.field === "environment-values")?.message}</p>}
  </Section>;
}

function PolicyFields({ definition, setDefinition, errors }: { definition: Definition; setDefinition: (value: Definition) => void; errors: ValidationError[] }) {
  const policy = definition.policy;
  const patch = (value: Partial<Definition["policy"]>) => setDefinition({ ...definition, policy: { ...policy, ...value } });
  return <Section title="Policy">
    <Choice legend="Overlap" value={policy.overlap} choices={[["skip", "Skip", "Keep the current run"], ["replace", "Replace", "Supersede the current run"], ["allow", "Allow", "Run concurrently"]]} onChange={(value) => patch({ overlap: value, ...(value === "allow" && policy.per_job_concurrency < 2 ? { per_job_concurrency: 2 } : {}) })} />
    {policy.overlap === "allow" && <Field id="job-concurrency" label="Per-job concurrency" description="From 2 through the global concurrency configured in Settings." error={errors.find((error) => error.field === "job-concurrency")?.message}>{({ id, describedBy, invalid }) => <input id={id} type="number" min="2" max="64" value={policy.per_job_concurrency} aria-describedby={describedBy} aria-invalid={invalid} onChange={(event) => patch({ per_job_concurrency: Number(event.target.value) })} />}</Field>}
    <Choice legend="Missed runs" value={policy.missed_run} choices={[["skip", "Skip", "Do not catch up"], ["latest", "Latest", "Run only the newest missed time"], ["all", "All", "Catch up in order"]]} onChange={(value) => patch({ missed_run: value })} />
    {policy.missed_run === "all" && <Field id="catch-up-limit" label="Catch-up limit" description="Maximum missed occurrences reconciled at once, from 1 through 1000." error={errors.find((error) => error.field === "catch-up-limit")?.message}>{({ id, describedBy, invalid }) => <input id={id} type="number" min="1" max="1000" value={policy.catch_up_limit} aria-describedby={describedBy} aria-invalid={invalid} onChange={(event) => patch({ catch_up_limit: Number(event.target.value) })} />}</Field>}
    <Field id="retries" label="Retries" description="Zero means one initial attempt and no retry; maximum 10." error={errors.find((error) => error.field === "retries")?.message}>{({ id, describedBy, invalid }) => <input id={id} type="number" min="0" max="10" value={policy.retries} aria-describedby={describedBy} aria-invalid={invalid} onChange={(event) => patch({ retries: Number(event.target.value) })} />}</Field>
    {policy.retries > 0 && <><DurationInput label="Retry delay" value={policy.retry_delay} onChange={(value) => patch({ retry_delay: value ?? 0 })} /><Choice legend="Backoff" value={policy.backoff} choices={[["fixed", "Fixed", "Same delay each retry"], ["exponential", "Exponential", "Grow delay to the cap"]]} onChange={(value) => patch({ backoff: value })} /><DurationInput label="Retry cap" value={policy.retry_cap} onChange={(value) => patch({ retry_cap: value ?? 0 })} /><label className="check"><input type="checkbox" checked={policy.retry_timeout} onChange={(event) => patch({ retry_timeout: event.target.checked })} />Retry timed-out attempts</label></>}
    <DurationInput label="Start deadline" value={policy.start_deadline} nullable nullableLabel="Off" description="Off accepts an occurrence regardless of age; otherwise set the maximum delay before it is skipped." onChange={(value) => patch({ start_deadline: value })} />
    <DurationInput label="Timeout" value={policy.timeout} nullable nullableLabel="Off" description="Off lets an attempt run without a time limit; otherwise set its maximum runtime." onChange={(value) => patch({ timeout: value })} />
    <DurationInput label="Termination grace" value={policy.termination_grace} onChange={(value) => patch({ termination_grace: value ?? 0 })} />
  </Section>;
}
