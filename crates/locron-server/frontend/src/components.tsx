import { type ReactNode, useEffect, useId, useSyncExternalStore } from "react";
import { getTheme, setTheme, subscribeTheme, type ThemePreference } from "./theme";
import { useState } from "react";
import { formatDuration, parseDuration, type DurationUnit } from "./domain/duration";
import { byteEquivalent, formatByteSize, parseByteSize, type ByteUnit } from "./domain/byteSize";
import { instantPreview, parseInstant } from "./domain/instant";
import { CalendarDays } from "lucide-react";
import { LocronSelect } from "./ui";

export function Field({ id: providedId, label, description, error, children }: { id?: string; label: string; description?: string | undefined; error?: string | undefined; children: (ids: { id: string; describedBy?: string; invalid: boolean }) => ReactNode }) {
  const generated = useId(), id = providedId ?? generated, hint = `${id}-hint`, issue = `${id}-error`; const describedBy = [description && hint, error && issue].filter(Boolean).join(" ") || undefined;
  return <div className={`field${error ? " field-error" : ""}`}><label htmlFor={id}>{label}</label><div className="field-control">{children({ id, ...(describedBy ? { describedBy } : {}), invalid: Boolean(error) })}</div>{description && <p id={hint} className="field-help">{description}</p>}{error && <p id={issue} className="error" role="alert">{error}</p>}</div>;
}
export function ThemeControl({ name }: { name: string }) { const value = useSyncExternalStore(subscribeTheme, getTheme), help = useId(); return <fieldset className="theme-control" aria-describedby={help}><legend>Color theme</legend><div className="theme-options">{(["system", "light", "dark"] as ThemePreference[]).map((item) => <label key={item}><input type="radio" name={name} checked={value === item} onChange={() => setTheme(item)}/>{item[0]?.toUpperCase()}{item.slice(1)}</label>)}</div><p id={help} className="theme-group-help">Browser-local only; this does not change daemon settings.</p></fieldset>; }
export function Feedback({ kind = "muted", children }: { kind?: "muted" | "error" | "state-good"; children: ReactNode }) { return <p className={kind} role="status" aria-atomic="true">{children}</p>; }

export function DurationInput({ label, value, nullable = false, nullableLabel = "Off", description, onChange }: { label: string; value: number | null; nullable?: boolean; nullableLabel?: string; description?: string; onChange: (value: number | null) => void }) {
  const initial = value === null ? { magnitude: "", unit: "s" as DurationUnit } : formatDuration(value); const [magnitude, setMagnitude] = useState(initial.magnitude); const [unit, setUnit] = useState(initial.unit); const [error, setError] = useState("");
  useEffect(() => { const next = value === null ? { magnitude: "", unit: "s" as DurationUnit } : formatDuration(value); setMagnitude(next.magnitude); setUnit(next.unit); }, [value]);
  const commit = (raw: string, nextUnit = unit) => { setMagnitude(raw); if (!raw.trim() && nullable) { setError(""); onChange(null); return; } try { const parsed = parseDuration(raw, nextUnit); setUnit(parsed.unit); setError(""); onChange(parsed.value); } catch (issue) { setError((issue as Error).message); } };
  const normalize = () => { if (!magnitude.trim() || value === null) return; try { const compact = formatDuration(parseDuration(magnitude, unit).value); setMagnitude(compact.magnitude); setUnit(compact.unit); } catch { /* inline error remains */ } };
  const toggleNull = (off: boolean) => { if (off) { onChange(null); setError(""); return; } const fallback = 60_000_000; const compact = formatDuration(fallback); setMagnitude(compact.magnitude); setUnit(compact.unit); onChange(fallback); };
  return <Field label={label} description={description ?? "Enter one exact amount; a pasted s, m, h, or d suffix is accepted."} error={error}>{({ id, describedBy, invalid }) => <>{nullable && <label className="check"><input type="checkbox" checked={value === null} onChange={(event) => toggleNull(event.target.checked)} />{nullableLabel}</label>}{value !== null && <div className="duration-input"><input id={id} inputMode="decimal" value={magnitude} aria-describedby={describedBy} aria-invalid={invalid} onChange={(event) => commit(event.target.value)} onBlur={normalize}/><LocronSelect label={`${label} unit`} value={unit} options={[{value:"s",label:"seconds"},{value:"m",label:"minutes"},{value:"h",label:"hours"},{value:"d",label:"days"}]} onChange={(next) => { const typed = next as DurationUnit; setUnit(typed); commit(magnitude, typed); }} /></div>}</>}</Field>;
}

export function ByteSizeInput({ label, value, consequence, onChange }: { label: string; value: number; consequence: string; onChange: (value: number) => void }) {
  const initial = formatByteSize(value); const [magnitude, setMagnitude] = useState(initial.magnitude); const [unit, setUnit] = useState(initial.unit); const [error, setError] = useState(""); const [exact, setExact] = useState(byteEquivalent(value));
  useEffect(() => { const next = formatByteSize(value); setMagnitude(next.magnitude); setUnit(next.unit); setExact(byteEquivalent(value)); }, [value]);
  const commit = (raw: string, nextUnit = unit) => { setMagnitude(raw); try { const parsed = parseByteSize(raw, nextUnit); setUnit(parsed.unit); setExact(byteEquivalent(parsed.value)); setError(""); onChange(parsed.value); } catch (issue) { setError((issue as Error).message); } };
  return <Field label={label} description={`${consequence} Current exact value: ${exact}.`} error={error}>{({ id, describedBy, invalid }) => <div className="duration-input"><input id={id} inputMode="decimal" value={magnitude} aria-describedby={describedBy} aria-invalid={invalid} onChange={(event) => commit(event.target.value)}/><LocronSelect label={`${label} unit`} value={unit} options={["B","KiB","MiB","GiB"].map((item)=>({value:item,label:item}))} onChange={(next) => { const typed = next as ByteUnit; setUnit(typed); commit(magnitude, typed); }} /></div>}</Field>;
}

export function InstantInput({ label, value, timezone, onChange }: { label: string; value: number; timezone: "local" | string; onChange: (value: number) => void }) {
  const local = new Date(value / 1000); const initial = `${local.getFullYear()}-${String(local.getMonth()+1).padStart(2,"0")}-${String(local.getDate()).padStart(2,"0")}T${String(local.getHours()).padStart(2,"0")}:${String(local.getMinutes()).padStart(2,"0")}`;
  const [text,setText]=useState(initial),[preview,setPreview]=useState(instantPreview(value)),[error,setError]=useState("");
  useEffect(() => { const next = new Date(value / 1000); setText(`${next.getFullYear()}-${String(next.getMonth()+1).padStart(2,"0")}-${String(next.getDate()).padStart(2,"0")}T${String(next.getHours()).padStart(2,"0")}:${String(next.getMinutes()).padStart(2,"0")}`); setPreview(instantPreview(value)); }, [value]);
  const commit=(next:string)=>{setText(next);try{const parsed=parseInstant(next,timezone);setPreview(instantPreview(parsed));setError("");onChange(parsed)}catch(issue){setError((issue as Error).message)}};
  return <Field label={label} description={`Interpreted in ${timezone === "local" ? "this machine's local timezone" : timezone}. Absolute preview: ${preview}.`} error={error}>{({id,describedBy,invalid})=><><span className="date-input"><CalendarDays size={17} aria-hidden="true"/><input id={id} type="datetime-local" value={text} aria-describedby={describedBy} aria-invalid={invalid} onChange={event=>commit(event.target.value)}/></span><details><summary>Advanced epoch value</summary><code>{value} microseconds</code></details></>}</Field>;
}
