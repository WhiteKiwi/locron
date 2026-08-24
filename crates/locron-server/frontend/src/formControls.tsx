import { type ReactNode, useEffect, useId, useState } from "react";
import { Field } from "./components";
import type { SecretValue } from "./types";
import { Plus, Trash2 } from "lucide-react";
import { LocronSelect } from "./ui";

export function Choice<T extends string>({
  legend,
  value,
  choices,
  onChange,
}: {
  legend: string;
  value: T;
  choices: ReadonlyArray<readonly [T, string, string?]>;
  onChange: (value: T) => void;
}) {
  const name = useId();
  return (
    <fieldset className="choice-group choice-cards">
      <legend>{legend}</legend>
      {choices.map(([key, label, description]) => (
        <label key={key}>
          <input type="radio" name={name} checked={value === key} onChange={() => onChange(key)} />
          <span><strong>{label}</strong>{description && <small>{description}</small>}</span>
        </label>
      ))}
    </fieldset>
  );
}

export function KeyValueRows<T extends string | SecretValue>({
  legend,
  value,
  onChange,
  secretSource = false,
}: {
  legend: string;
  value: Record<string, T>;
  onChange: (value: Record<string, T>) => void;
  secretSource?: boolean;
}) {
  const entries = Object.entries(value);
  const emit = (next: Record<string, string | SecretValue>) => onChange(next as Record<string, T>);
  const add = () => emit({ ...value, "": secretSource ? { source: "inline", value: "" } : "" });
  const replaceKey = (oldKey: string, newKey: string) => {
    const next = Object.fromEntries(entries.map(([key, item]) => [key === oldKey ? newKey : key, item]));
    emit(next);
  };
  return (
    <fieldset className="repeatable-rows" id={legend.toLowerCase().replaceAll(" ", "-")} tabIndex={-1}>
      <legend>{legend}</legend>
      {entries.map(([key, item], index) => {
        const secret = typeof item === "object" ? item as SecretValue : null;
        return (
          <div className="kv-row" key={`${index}-${key}`}>
            <label className="sr-only" htmlFor={`${legend}-key-${index}`}>{legend} name {index + 1}</label>
            <input id={`${legend}-key-${index}`} value={key} placeholder="NAME" onChange={(event) => replaceKey(key, event.target.value)} />
            {secret && <LocronSelect label={`${legend} ${index + 1} source`} value={secret.source} options={[{value:"inline",label:"Inline value"},{value:"environment",label:"Environment variable"}]} onChange={(source) => emit({ ...value, [key]: { ...secret, source } as SecretValue })} />}
            <label className="sr-only" htmlFor={`${legend}-value-${index}`}>{legend} value {index + 1}</label>
            <input id={`${legend}-value-${index}`} value={secret?.value ?? item as string} placeholder={secret?.source === "environment" ? "VARIABLE_NAME" : "value"} onChange={(event) => emit({ ...value, [key]: secret ? { ...secret, value: event.target.value } : event.target.value })} />
            <button className="icon-button" aria-label={`Remove ${legend} row ${index + 1}`} type="button" onClick={() => emit(Object.fromEntries(entries.filter((_, itemIndex) => itemIndex !== index)))}><Trash2 size={17} /></button>
          </div>
        );
      })}
      <button type="button" onClick={add}><Plus size={16} />Add row</button>
    </fieldset>
  );
}

export function EditableTextGrammar({ label, description, initial, onValid }: { label: string; description: string; initial: string; onValid: (text: string) => void }) {
  const [text, setText] = useState(initial);
  return <Field label={label} description={description}>{({ id }) => <textarea id={id} value={text} onChange={(event) => { setText(event.target.value); onValid(event.target.value); }} />}</Field>;
}

export function ValidationSummary({ errors }: { errors: Array<{ field: string; message: string }> }) {
  if (!errors.length) return null;
  return <div className="error-block" role="alert" tabIndex={-1} id="validation-summary"><strong>Fix {errors.length} field{errors.length === 1 ? "" : "s"}</strong><ul>{errors.map((error) => <li key={error.field}><button className="link" type="button" onClick={() => { const target = document.getElementById(error.field); target?.scrollIntoView({ block: "center" }); target?.focus(); }}>{error.message}</button></li>)}</ul></div>;
}

export function PathRows({ label, value, onChange }: { label: string; value: string; onChange: (value: string) => void }) {
  const [rows, setRows] = useState(value ? value.split(":") : []);
  useEffect(() => { if (value !== rows.join(":")) setRows(value ? value.split(":") : []); }, [value]);
  const id = label.toLowerCase().replaceAll(/[^a-z0-9]+/g, "-").replace(/-$/, "");
  const emit = (next: string[]) => { setRows(next); onChange(next.join(":")); };
  const set = (index: number, next: string) => { const copy = [...rows]; copy[index] = next; emit(copy); };
  return <fieldset className="repeatable-rows"><legend>{label}</legend><p className="field-help">Ordered absolute directories. Each row is one search location.</p>{rows.map((row, index) => <div className="kv-row" key={index}><label className="sr-only" htmlFor={`${id}-path-${index}`}>{label} path {index + 1}</label><input id={`${id}-path-${index}`} value={row} onChange={(event) => set(index, event.target.value)} /><button className="icon-button" aria-label={`Remove ${label} path ${index + 1}`} type="button" onClick={() => emit(rows.filter((_, item) => item !== index))}><Trash2 size={17} /></button></div>)}<button type="button" onClick={() => emit([...rows, ""])}><Plus size={16} />Add path</button><details><summary>Advanced colon-delimited value</summary><code>{rows.join(":") || "(empty)"}</code></details></fieldset>;
}

export function Section({ title, intro, children }: { title: string; intro?: string; children: ReactNode }) {
  return <section id={`job-${title.toLowerCase()}`} className="card form-section"><h2>{title}</h2>{intro && <p className="section-intro">{intro}</p>}{children}</section>;
}
