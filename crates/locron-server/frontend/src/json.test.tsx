import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { JsonViewer, jsonLineCount, jsonPreview, lexJson } from "./json";

describe("exact RFC 8259 JSON lexer", () => {
  it("preserves CRLF, Unicode, escapes, duplicate keys, and exponent spelling", () => {
    const source = '{\r\n  "한글": "<tag>\\n",\r\n  "한글": -1.20e+03\r\n}';
    const result = lexJson(source);
    expect(result.valid).toBe(true);
    expect(result.tokens.map((token) => token.text).join("")).toBe(source);
    expect(result.tokens.filter((token) => token.kind === "key")).toHaveLength(2);
    expect(result.tokens.some((token) => token.kind === "number" && token.text === "-1.20e+03")).toBe(true);
  });

  it("marks malformed and empty sources invalid without changing them", () => {
    for (const source of ["", '{"broken": tru}', '"unterminated']) {
      const result = lexJson(source);
      expect(result.valid).toBe(false);
      expect(result.tokens.map((token) => token.text).join("")).toBe(source);
    }
  });

  it("takes the first complete lines without normalizing line endings", () => {
    const source = "one\r\ntwo\nthree\rfour";
    expect(jsonLineCount(source)).toBe(4);
    expect(jsonPreview(source, 2)).toBe("one\r\ntwo\n");
  });
});

describe("JsonViewer", () => {
  const writeText = vi.fn(() => Promise.resolve());
  const values = new Map<string, string>();
  const storage = { clear: () => values.clear(), getItem: (key: string) => values.get(key) ?? null, setItem: (key: string, value: string) => values.set(key, value), removeItem: (key: string) => values.delete(key), key: () => null, get length() { return values.size; } };
  beforeEach(() => {
    vi.stubGlobal("localStorage", storage);
    localStorage.clear();
    writeText.mockClear();
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });
  });

  it("renders literal markup as text, exposes one continuous code value, and copies exact source", async () => {
    const source = '{"html":"<img src=x onerror=alert(1)>","escape":"\\uD55C"}\r\n';
    const { container } = render(<JsonViewer source={source} />);
    expect(container.querySelector("img")).toBeNull();
    expect(container.querySelector("pre code")?.textContent).toBe(source);
    fireEvent.click(screen.getByRole("button", { name: "Copy" }));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith(source));
    expect(await screen.findByText("Copied exact JSON.")).toBeTruthy();
  });

  it("labels invalid JSON and preserves its plain source", () => {
    const source = '{"x": <script>alert(1)</script>}';
    const { container } = render(<JsonViewer source={source} />);
    expect(screen.getByText("Invalid JSON")).toBeTruthy();
    expect(container.querySelector("script")).toBeNull();
    expect(container.querySelector("pre code")?.textContent).toBe(source);
  });

  it("previews 80 full lines, expands, and keeps full copy", async () => {
    const source = `[\n${Array.from({ length: 201 }, (_, index) => `  ${index}${index === 200 ? "" : ","}`).join("\n")}\n]`;
    const { container } = render(<JsonViewer source={source} />);
    expect(screen.getByRole("button", { name: `Show all ${jsonLineCount(source)} lines` })).toBeTruthy();
    expect(jsonLineCount(container.querySelector("pre code")?.textContent ?? "")).toBe(81);
    fireEvent.click(screen.getByRole("button", { name: "Copy" }));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith(source));
    fireEvent.click(screen.getByRole("button", { name: /Show all/ }));
    expect(container.querySelector("pre code")?.textContent).toBe(source);
  });

  it("persists wrap preference and applies the 64 KiB disclosure threshold", () => {
    const source = JSON.stringify({ payload: "x".repeat(65_537) });
    const first = render(<JsonViewer source={source} />);
    const wrap = screen.getByRole("button", { name: "Wrap" });
    fireEvent.click(wrap);
    expect(wrap.getAttribute("aria-pressed")).toBe("true");
    expect(localStorage.getItem("locron.json.wrap")).toBe("true");
    expect(screen.getByRole("button", { name: /Show all 1 lines/ })).toBeTruthy();
    first.unmount();
    render(<JsonViewer source="{}" />);
    expect(screen.getByRole("button", { name: "Wrap" }).getAttribute("aria-pressed")).toBe("true");
  });
});
