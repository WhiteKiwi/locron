import { Check, Clipboard, WrapText } from "lucide-react";
import { useEffect, useMemo, useState } from "react";

export type JsonTokenKind = "key" | "string" | "number" | "literal" | "punctuation" | "whitespace" | "invalid";
export type JsonToken = { kind: JsonTokenKind; text: string };

const number = /^-?(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?/;
const whitespace = /^[\u0020\u0009\u000A\u000D]+/;

export function lexJson(source: string, validateStructure = true): { tokens: JsonToken[]; valid: boolean } {
  const tokens: JsonToken[] = [];
  let index = 0;
  while (index < source.length) {
    const rest = source.slice(index);
    const space = whitespace.exec(rest)?.[0];
    if (space) { tokens.push({ kind: "whitespace", text: space }); index += space.length; continue; }
    const character = source[index]!;
    if (character === '"') {
      let cursor = index + 1, validString = true, closed = false;
      while (cursor < source.length) {
        const current = source[cursor]!;
        if (current === '"') { cursor += 1; closed = true; break; }
        if (current.charCodeAt(0) < 0x20) validString = false;
        if (current === "\\") {
          cursor += 1;
          const escaped = source[cursor];
          if (escaped === "u") {
            const hex = source.slice(cursor + 1, cursor + 5);
            if (!/^[0-9a-fA-F]{4}$/.test(hex)) validString = false;
            cursor += 5;
            continue;
          }
          if (!escaped || !'"\\/bfnrt'.includes(escaped)) validString = false;
        }
        cursor += 1;
      }
      const text = source.slice(index, cursor);
      tokens.push({ kind: closed && validString ? "string" : "invalid", text });
      index = cursor;
      continue;
    }
    const numeric = number.exec(rest)?.[0];
    if (numeric) { tokens.push({ kind: "number", text: numeric }); index += numeric.length; continue; }
    const literal = ["true", "false", "null"].find((value) => rest.startsWith(value));
    if (literal) { tokens.push({ kind: "literal", text: literal }); index += literal.length; continue; }
    if ("{}[],:".includes(character)) { tokens.push({ kind: "punctuation", text: character }); index += 1; continue; }
    tokens.push({ kind: "invalid", text: character });
    index += 1;
  }
  for (let tokenIndex = 0; tokenIndex < tokens.length; tokenIndex += 1) {
    if (tokens[tokenIndex]!.kind !== "string") continue;
    let next = tokenIndex + 1;
    while (tokens[next]?.kind === "whitespace") next += 1;
    if (tokens[next]?.text === ":") tokens[tokenIndex] = { ...tokens[tokenIndex]!, kind: "key" };
  }
  let valid = !tokens.some((token) => token.kind === "invalid");
  if (validateStructure) try { JSON.parse(source); } catch { valid = false; }
  return { tokens: valid ? tokens : [{ kind: "invalid", text: source }], valid };
}

export function formatJsonPresentation(source: string): { tokens: JsonToken[]; text: string; valid: boolean } {
  const result = lexJson(source);
  if (!result.valid) return { ...result, text: source };
  const input = result.tokens.filter((token) => token.kind !== "whitespace");
  const tokens: JsonToken[] = [];
  let depth = 0;
  const space = (text: string) => tokens.push({ kind: "whitespace", text });
  const line = () => space(`\n${"  ".repeat(depth)}`);
  for (let index = 0; index < input.length; index += 1) {
    const token = input[index]!;
    const next = input[index + 1];
    const previous = input[index - 1];
    if (token.text === "{" || token.text === "[") {
      tokens.push(token);
      const matching = token.text === "{" ? "}" : "]";
      if (next?.text !== matching) { depth += 1; line(); }
    } else if (token.text === "}" || token.text === "]") {
      const matching = token.text === "}" ? "{" : "[";
      if (previous?.text !== matching) { depth -= 1; line(); }
      tokens.push(token);
    } else if (token.text === ",") {
      tokens.push(token); line();
    } else if (token.text === ":") {
      tokens.push(token); space(" ");
    } else tokens.push(token);
  }
  return { tokens, text: tokens.map((token) => token.text).join(""), valid: true };
}

export function jsonPreview(source: string, lines = 80) {
  let cursor = 0, seen = 0;
  while (cursor < source.length && seen < lines) {
    const lf = source.indexOf("\n", cursor), cr = source.indexOf("\r", cursor);
    let next = lf === -1 ? cr : cr === -1 ? lf : Math.min(lf, cr);
    if (next === -1) return source;
    if (source[next] === "\r" && source[next + 1] === "\n") next += 1;
    cursor = next + 1;
    seen += 1;
  }
  return source.slice(0, cursor);
}

export function jsonLineCount(source: string) {
  return source.split(/\r\n|\r|\n/).length;
}

const WRAP_KEY = "locron.json.wrap";
function initialWrap() { try { return localStorage.getItem(WRAP_KEY) === "true"; } catch { return false; } }

export function JsonViewer({ source, label = "Structured JSON" }: { source: string; label?: string }) {
  const presentation = useMemo(() => formatJsonPresentation(source), [source]);
  const lines = jsonLineCount(presentation.text);
  const sourceBytes = new TextEncoder().encode(source).byteLength;
  const large = lines > 200 || sourceBytes > 65_536;
  const [expanded, setExpanded] = useState(!large);
  const [wrap, setWrap] = useState(initialWrap);
  const [copyStatus, setCopyStatus] = useState("");
  useEffect(() => { setExpanded(!large); }, [source, large]);
  const visible = large && !expanded ? jsonPreview(presentation.text) : presentation.text;
  const result = useMemo(() => lexJson(visible, !large || expanded), [visible, large, expanded]);
  const setWrapping = (next: boolean) => { setWrap(next); try { localStorage.setItem(WRAP_KEY, String(next)); } catch { /* storage is optional */ } };
  const copy = async () => {
    try { await navigator.clipboard.writeText(source); setCopyStatus("Copied exact JSON."); }
    catch { setCopyStatus("Copy failed. Select the JSON and copy it manually."); }
  };
  return <section className="json-viewer" aria-label={label}>
    <header className="json-toolbar"><div><span className="json-language">{presentation.valid ? "JSON" : "Invalid JSON"}</span>{large && <span className="json-size">{lines} display lines · {sourceBytes.toLocaleString()} source bytes</span>}</div><div className="json-tools"><span className="json-copy-status" role="status" aria-live="polite">{copyStatus}</span><button type="button" aria-pressed={wrap} onClick={() => setWrapping(!wrap)}><WrapText size={16} aria-hidden="true"/>Wrap</button><button type="button" onClick={() => void copy()}>{copyStatus.startsWith("Copied") ? <Check size={16} aria-hidden="true"/> : <Clipboard size={16} aria-hidden="true"/>}Copy</button></div></header>
    <pre className={`json-code${wrap ? " wrap" : ""}`}><code>{result.tokens.map((token, index) => <span className={`json-${token.kind}`} key={index}>{token.text}</span>)}</code></pre>
    {large && !expanded && <div className="json-expand"><button type="button" onClick={() => setExpanded(true)}>Show all {lines} lines</button></div>}
  </section>;
}
