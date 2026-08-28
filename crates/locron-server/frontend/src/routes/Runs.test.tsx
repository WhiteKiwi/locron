import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../api";
import type { RunDetailData } from "../types";
import { outputEventKey, RunDetail, Runs } from "./Runs";

vi.mock("../api", () => ({ api: { get: vi.fn(), post: vi.fn() } }));
const get = vi.mocked(api.get);
const page = (id: string, total = 1) => ({ data: { runs: [{ id, job_id: "job-1", requested_at_us: 1_787_650_200_000_000, trigger: "manual", state: "succeeded" }], total, offset: 0 }, warnings: [] });
const emptyPage = () => ({ data: { runs: [], total: 0, offset: 0 }, warnings: [] });
function deferred<T>() { let resolve!: (value: T) => void; let reject!: (reason: unknown) => void; const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; }

class FakeEventSource {
  static instances: FakeEventSource[] = [];
  readonly listeners = new Map<string, Set<EventListener>>();
  onopen: ((event: Event) => void) | null = null;
  onerror: ((event: Event) => void) | null = null;
  closed = false;
  constructor(readonly url: string) { FakeEventSource.instances.push(this); }
  addEventListener(name: string, listener: EventListener) {
    const listeners = this.listeners.get(name) ?? new Set<EventListener>();
    listeners.add(listener);
    this.listeners.set(name, listeners);
  }
  removeEventListener(name: string, listener: EventListener) { this.listeners.get(name)?.delete(listener); }
  close() { this.closed = true; }
  emit(name: string, data: unknown) {
    const event = new MessageEvent(name, { data: JSON.stringify(data) });
    for (const listener of this.listeners.get(name) ?? []) listener(event);
  }
  open() { this.onopen?.(new Event("open")); }
  fail() { this.onerror?.(new Event("error")); }
}

const detail = (id: string, state: string, attempts: RunDetailData["attempts"] = []) => ({ data: { id, job_id: "job-1", requested_at_us: 1_787_650_200_000_000, trigger: "manual", state, attempts }, warnings: [] });
const explanation = { data: { explanation: "Durable facts", daemon_running: true, events: [] }, warnings: [] };

describe("run history search", () => {
  beforeEach(() => { vi.useFakeTimers(); get.mockReset(); get.mockImplementation((path) => Promise.resolve(path === "/api/v1/jobs?all=1" ? { data: [], warnings: [] } : page("initial-run")) as never); });
  afterEach(() => vi.useRealTimers());

  it("uses a 250 ms trailing debounce and flushes Enter immediately", async () => {
    render(<Runs />);
    await act(async () => { await vi.runAllTimersAsync(); });
    get.mockClear();
    const input = screen.getByRole("searchbox", { name: /search by run id/i });
    fireEvent.change(input, { target: { value: "ni" } });
    await act(async () => { await vi.advanceTimersByTimeAsync(249); });
    expect(get).not.toHaveBeenCalled();
    await act(async () => { await vi.advanceTimersByTimeAsync(1); });
    expect(get.mock.calls.some(([path]) => String(path).includes("q=ni"))).toBe(true);
    get.mockClear();
    fireEvent.change(input, { target: { value: "back" } });
    fireEvent.keyDown(input, { key: "Enter" });
    await act(async () => { await vi.advanceTimersByTimeAsync(0); });
    expect(get.mock.calls.some(([path]) => String(path).includes("q=back"))).toBe(true);
  });

  it("uses the complete current-job view to enrich runs for a disabled job", async () => {
    get.mockImplementation((path) => Promise.resolve(path === "/api/v1/jobs?all=1" ? { data: [{ id: "job-1", name: "disabled-nightly-backup", enabled: false, definition_json: "{}" }], warnings: [] } : page("disabled-job-run")) as never);
    render(<Runs />);
    await act(async () => { await vi.runAllTimersAsync(); });
    expect(get.mock.calls.some(([path]) => path === "/api/v1/jobs?all=1")).toBe(true);
    expect(get.mock.calls.some(([path]) => path === "/api/v1/jobs")).toBe(false);
    expect(screen.getAllByText("disabled-nightly-backup")).toHaveLength(2);
    expect(screen.getAllByRole("link", { name: /disabled-job-run.*disabled-nightly-backup/ })).toHaveLength(2);
  });

  it("uses attempt plus sequence for reconnect dedupe identity", () => {
    expect(outputEventKey({ attempt_number: 1, seq: 0 })).toBe("1:0");
    expect(outputEventKey({ attempt_number: 2, seq: 0 })).toBe("2:0");
  });

  it("aborts and ignores a slow stale success and stale error", async () => {
    render(<Runs />);
    await act(async () => { await vi.runAllTimersAsync(); });
    const first = deferred<ReturnType<typeof page>>(), second = deferred<ReturnType<typeof page>>();
    let historyCall = 0;
    const signals: AbortSignal[] = [];
    get.mockImplementation((path, init) => {
      if (path === "/api/v1/jobs?all=1") return Promise.resolve({ data: [], warnings: [] }) as never;
      signals.push(init?.signal as AbortSignal);
      historyCall += 1;
      return (historyCall === 1 ? first.promise : second.promise) as never;
    });
    const input = screen.getByRole("searchbox", { name: /search by run id/i });
    fireEvent.change(input, { target: { value: "ni" } });
    await act(async () => { await vi.advanceTimersByTimeAsync(250); });
    fireEvent.change(input, { target: { value: "back" } });
    await act(async () => { await vi.advanceTimersByTimeAsync(250); });
    expect(signals[0]?.aborted).toBe(true);
    await act(async () => { second.resolve(page("back-run")); await Promise.resolve(); });
    expect(screen.getByText("back-run")).toBeTruthy();
    await act(async () => { first.reject(new Error("stale failure")); await Promise.resolve(); });
    expect(screen.queryByText(/stale failure/)).toBeNull();
    expect(screen.getByText("back-run")).toBeTruthy();
  });

  it("pages, clears, and refreshes immediately", async () => {
    get.mockImplementation((path) => Promise.resolve(path === "/api/v1/jobs?all=1" ? { data: [], warnings: [] } : page("paged-run", 25)) as never);
    render(<Runs />);
    await act(async () => { await vi.runAllTimersAsync(); });
    get.mockClear();
    fireEvent.click(screen.getByRole("button", { name: "Older" }));
    await act(async () => { await vi.advanceTimersByTimeAsync(0); });
    expect(get.mock.calls.some(([path]) => String(path).includes("offset=20"))).toBe(true);
    const input = screen.getByRole("searchbox", { name: /search by run id/i });
    fireEvent.change(input, { target: { value: "%_한글" } });
    await act(async () => { await vi.advanceTimersByTimeAsync(250); });
    expect(get.mock.calls.some(([path]) => String(path).includes("q=%25_%ED%95%9C%EA%B8%80"))).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Clear" }));
    await act(async () => { await vi.advanceTimersByTimeAsync(0); });
    expect((input as HTMLInputElement).value).toBe("");
  });

  it("retries the current failed query without changing it", async () => {
    render(<Runs />);
    await act(async () => { await vi.runAllTimersAsync(); });
    const input = screen.getByRole("searchbox", { name: /search by run id/i });
    get.mockImplementation((path) => path === "/api/v1/jobs?all=1" ? Promise.resolve({ data: [], warnings: [] }) as never : Promise.reject(new Error("temporary search failure")) as never);
    fireEvent.change(input, { target: { value: "nightly" } });
    await act(async () => { await vi.advanceTimersByTimeAsync(250); });
    expect(screen.getByText(/temporary search failure/)).toBeTruthy();
    get.mockImplementation((path) => Promise.resolve(path === "/api/v1/jobs?all=1" ? { data: [], warnings: [] } : page("nightly-backup")) as never);
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    await act(async () => { await vi.advanceTimersByTimeAsync(0); });
    expect(screen.getByText("nightly-")).toBeTruthy();
    expect((input as HTMLInputElement).value).toBe("nightly");
  });

  it("keeps headers and a semantic mobile body for first-use empty history", async () => {
    get.mockImplementation((path) => Promise.resolve(path === "/api/v1/jobs?all=1" ? { data: [], warnings: [] } : emptyPage()) as never);
    render(<Runs />);
    await act(async () => { await vi.runAllTimersAsync(); });
    expect(screen.getAllByText("No runs yet")).toHaveLength(2);
    const table = screen.getByRole("table");
    expect(within(table).getAllByRole("columnheader")).toHaveLength(6);
    expect(within(table).getByRole("cell").getAttribute("colspan")).toBe("6");
    const status = screen.getByRole("status");
    expect(status.textContent).toBe("0 runs.");
    expect(table.getAttribute("aria-describedby")).toBe(status.id);
    expect(document.querySelector(".mobile-data article")).toBeNull();
    expect(document.querySelector(".empty-object-list .data-empty")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Newer" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Older" })).toBeNull();
    expect(screen.getAllByRole("link", { name: "View jobs" })).toHaveLength(2);
    expect(screen.getAllByRole("link", { name: "Create job" })).toHaveLength(2);
  });

  it("recovers from filtered zero immediately and focuses the search", async () => {
    get.mockImplementation((path) => {
      if (path === "/api/v1/jobs?all=1") return Promise.resolve({ data: [], warnings: [] }) as never;
      return Promise.resolve(String(path).includes("q=missing") ? emptyPage() : page("restored-run")) as never;
    });
    render(<Runs />);
    await act(async () => { await vi.runAllTimersAsync(); });
    const input = screen.getByRole("searchbox", { name: /search by run id/i });
    fireEvent.change(input, { target: { value: "missing" } });
    await act(async () => { await vi.advanceTimersByTimeAsync(250); });
    expect(screen.getAllByText("No runs match these filters")).toHaveLength(2);
    expect(screen.getByRole("status").textContent).toContain("0 matching runs");
    expect(screen.queryByRole("button", { name: "Older" })).toBeNull();
    const clear = screen.getAllByRole("button", { name: "Clear filters" });
    expect(clear).toHaveLength(2);
    fireEvent.click(clear[0]!);
    expect(document.activeElement).toBe(input);
    await act(async () => { await vi.advanceTimersByTimeAsync(0); });
    expect((input as HTMLInputElement).value).toBe("");
    expect(screen.getAllByRole("link", { name: /restored.*view full run/ })).toHaveLength(2);
    expect(screen.getByRole("status").textContent).toBe("1 run.");
  });

  it("does not present initial loading or failure as a successful empty history", async () => {
    const request = deferred<ReturnType<typeof emptyPage>>();
    get.mockImplementation((path) => path === "/api/v1/jobs?all=1" ? Promise.resolve({ data: [], warnings: [] }) as never : request.promise as never);
    render(<Runs />);
    await act(async () => { await vi.advanceTimersByTimeAsync(0); });
    expect(screen.getByText("Loading run history…")).toBeTruthy();
    expect(screen.queryByRole("table")).toBeNull();
    expect(screen.queryByText("No runs yet")).toBeNull();
    await act(async () => { request.reject(new Error("history unavailable")); await Promise.resolve(); });
    expect(screen.getByText(/history unavailable/)).toBeTruthy();
    expect(screen.queryByText("Loading run history…")).toBeNull();
    expect(screen.queryByRole("table")).toBeNull();
    expect(screen.queryByText("No runs yet")).toBeNull();
  });
});

describe("run detail recovery", () => {
  beforeEach(() => get.mockReset());

  it("renders a complete recovery state when a stale run route returns 404", async () => {
    const missing = Object.assign(new Error("01a03413-3cce-7ef1-8031-95987dc0fa02"), { status: 404 });
    get.mockImplementation((path) => path === "/api/v1/runs/missing-run" ? Promise.reject(missing) as never : Promise.resolve({ data: { explanation: "", daemon_running: false, events: [] }, warnings: [] }) as never);
    render(<RunDetail id="missing-run" />);
    expect(await screen.findByRole("heading", { name: "Run not found" })).toBeTruthy();
    expect(screen.getByText(/removed by retention.*link may be stale/i)).toBeTruthy();
    expect(screen.getByRole("link", { name: "View run history" }).getAttribute("href")).toBe("#/runs");
    expect(screen.queryByText("01a03413-3cce-7ef1-8031-95987dc0fa02")).toBeNull();
  });

  it("keeps non-404 run detail failures as request feedback", async () => {
    get.mockImplementation((path) => path === "/api/v1/runs/run-1" ? Promise.reject(Object.assign(new Error("database temporarily unavailable"), { status: 503 })) as never : Promise.resolve({ data: { explanation: "", daemon_running: false, events: [] }, warnings: [] }) as never);
    render(<RunDetail id="run-1" />);
    expect(await screen.findByText("database temporarily unavailable")).toBeTruthy();
    expect(screen.queryByRole("heading", { name: "Run not found" })).toBeNull();
  });
});

describe("active run detail live following", () => {
  beforeEach(() => {
    get.mockReset();
    FakeEventSource.instances = [];
    vi.stubGlobal("EventSource", FakeEventSource);
  });
  afterEach(() => vi.unstubAllGlobals());

  it("renders and automatically follows from primary detail before the auxiliary explanation resolves", async () => {
    const slowExplanation = deferred<typeof explanation>();
    get.mockImplementation((path) => path.endsWith("/why") ? slowExplanation.promise as never : Promise.resolve(detail("slow-why-run", "running")) as never);
    render(<RunDetail id="slow-why-run" />);
    expect(await screen.findByRole("heading", { name: "Run slow-why" })).toBeTruthy();
    expect(FakeEventSource.instances).toHaveLength(1);
    expect(screen.queryByText("Durable facts")).toBeNull();
    await act(async () => { slowExplanation.resolve(explanation); await slowExplanation.promise; });
    expect(screen.getByText("Durable facts")).toBeTruthy();
  });

  it("automatically follows active state and applies replay-safe run, attempt, and base64 output events", async () => {
    get.mockImplementation((path) => Promise.resolve(path.endsWith("/why") ? explanation : detail("active-run", "queued")) as never);
    render(<RunDetail id="active-run" />);
    expect(await screen.findByRole("heading", { name: "Run active-r" })).toBeTruthy();
    expect(FakeEventSource.instances).toHaveLength(1);
    const source = FakeEventSource.instances[0]!;
    expect(source.url).toBe("/api/v1/runs/active-run/stream");
    act(() => {
      source.open();
      source.emit("run", { state: "running" });
      source.emit("attempt", { attempt_number: 1, state: "running" });
      source.emit("output", { attempt_number: 1, seq: 0, channel: "stdout", data_b64: "aGVsbG8=" });
      source.emit("output", { attempt_number: 1, seq: 0, channel: "stdout", data_b64: "aGVsbG8=" });
    });
    expect(screen.getAllByText("running")).toHaveLength(2);
    expect(screen.getByText("[stdout] hello")).toBeTruthy();
    expect(screen.getByText("Connected — replaying, then live")).toBeTruthy();
    act(() => source.fail());
    expect(screen.getByText(/Connection lost.*last durable details remain visible/)).toBeTruthy();
    expect(screen.getByText("[stdout] hello")).toBeTruthy();
  });

  it("pauses and resumes without clearing durable content or duplicating replayed frames", async () => {
    get.mockImplementation((path) => Promise.resolve(path.endsWith("/why") ? explanation : detail("pause-run", "running", [{ attempt_number: 1, state: "running" }])) as never);
    render(<RunDetail id="pause-run" />);
    await screen.findByRole("heading", { name: "Run pause-ru" });
    const first = FakeEventSource.instances[0]!;
    act(() => first.emit("output", { attempt_number: 1, seq: 3, channel: "stdout", data_b64: "b25jZQ==" }));
    fireEvent.click(screen.getByRole("button", { name: "Pause live updates" }));
    expect(first.closed).toBe(true);
    expect(screen.getByText("Live updates paused; the run continues.")).toBeTruthy();
    expect(screen.getByText("[stdout] once")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Resume live updates" }));
    expect(FakeEventSource.instances).toHaveLength(2);
    act(() => FakeEventSource.instances[1]!.emit("output", { attempt_number: 1, seq: 3, channel: "stdout", data_b64: "b25jZQ==" }));
    expect(screen.getAllByText("[stdout] once")).toHaveLength(1);
  });

  it("does not open a stream for an initially terminal run", async () => {
    get.mockImplementation((path) => Promise.resolve(path.endsWith("/why") ? explanation : detail("done-run", "succeeded", [{ attempt_number: 1, state: "succeeded" }])) as never);
    render(<RunDetail id="done-run" />);
    await screen.findByRole("heading", { name: "Run done-run" });
    expect(FakeEventSource.instances).toHaveLength(0);
    expect(screen.queryByRole("button", { name: /live updates/i })).toBeNull();
  });

  it("reconciles terminal run, explanation, attempts, and retained output exactly once", async () => {
    let detailCalls = 0;
    get.mockImplementation((path) => {
      if (path.endsWith("/why")) return Promise.resolve(explanation) as never;
      if (path.includes("/logs?attempt=1")) return Promise.resolve({ data: { frames: [{ channel: "stdout", sequence: 0, bytes: "ZmluYWw=" }] }, warnings: [] }) as never;
      detailCalls += 1;
      return Promise.resolve(detailCalls === 1 ? detail("finish-run", "running", [{ attempt_number: 1, state: "running" }]) : detail("finish-run", "succeeded", [{ attempt_number: 1, state: "succeeded", duration_us: 42 }])) as never;
    });
    render(<RunDetail id="finish-run" />);
    await screen.findByRole("heading", { name: "Run finish-r" });
    const source = FakeEventSource.instances[0]!;
    act(() => {
      source.emit("termination", { state: "succeeded", reason: "complete" });
      source.emit("termination", { state: "succeeded", reason: "complete" });
    });
    expect(await screen.findByText("Run finished: succeeded. Durable details reconciled.")).toBeTruthy();
    expect(screen.getByText("[stdout] final")).toBeTruthy();
    expect(screen.getByText("0.000042s")).toBeTruthy();
    expect(detailCalls).toBe(2);
    expect(source.closed).toBe(true);
    expect(FakeEventSource.instances).toHaveLength(1);
  });

  it("closes the old stream and ignores its events when the run identity changes", async () => {
    get.mockImplementation((path) => {
      if (path.endsWith("/why")) return Promise.resolve(explanation) as never;
      return Promise.resolve(path.includes("run-b") ? detail("run-b", "succeeded") : detail("run-a", "running")) as never;
    });
    const view = render(<RunDetail id="run-a" />);
    await screen.findByRole("heading", { name: "Run run-a" });
    const oldSource = FakeEventSource.instances[0]!;
    view.rerender(<RunDetail id="run-b" />);
    await screen.findByRole("heading", { name: "Run run-b" });
    expect(oldSource.closed).toBe(true);
    act(() => oldSource.emit("run", { state: "failed" }));
    expect(screen.getByText("succeeded")).toBeTruthy();
    expect(screen.queryByText("failed")).toBeNull();
  });

  it("aborts terminal reconciliation when the detail view unmounts", async () => {
    const terminalDetail = deferred<ReturnType<typeof detail>>();
    let detailCalls = 0;
    let terminalSignal: AbortSignal | undefined;
    get.mockImplementation((path, init) => {
      if (path.endsWith("/why")) return Promise.resolve(explanation) as never;
      detailCalls += 1;
      if (detailCalls === 1) return Promise.resolve(detail("unmount-run", "running")) as never;
      terminalSignal = init?.signal as AbortSignal;
      return terminalDetail.promise as never;
    });
    const view = render(<RunDetail id="unmount-run" />);
    await screen.findByRole("heading", { name: "Run unmount-" });
    act(() => FakeEventSource.instances[0]!.emit("termination", { state: "succeeded" }));
    expect(terminalSignal?.aborted).toBe(false);
    view.unmount();
    expect(terminalSignal?.aborted).toBe(true);
    terminalDetail.resolve(detail("unmount-run", "succeeded"));
  });
});
