import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../api";
import { outputEventKey, Runs } from "./Runs";

vi.mock("../api", () => ({ api: { get: vi.fn() } }));
const get = vi.mocked(api.get);
const page = (id: string, total = 1) => ({ data: { runs: [{ id, job_id: "job-1", requested_at_us: 1_787_650_200_000_000, trigger: "manual", state: "succeeded" }], total, offset: 0 }, warnings: [] });
const emptyPage = () => ({ data: { runs: [], total: 0, offset: 0 }, warnings: [] });
function deferred<T>() { let resolve!: (value: T) => void; let reject!: (reason: unknown) => void; const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; }

describe("run history search", () => {
  beforeEach(() => { vi.useFakeTimers(); get.mockReset(); get.mockImplementation((path) => Promise.resolve(path === "/api/v1/jobs" ? { data: [], warnings: [] } : page("initial-run")) as never); });
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
      if (path === "/api/v1/jobs") return Promise.resolve({ data: [], warnings: [] }) as never;
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
    get.mockImplementation((path) => Promise.resolve(path === "/api/v1/jobs" ? { data: [], warnings: [] } : page("paged-run", 25)) as never);
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
    get.mockImplementation((path) => path === "/api/v1/jobs" ? Promise.resolve({ data: [], warnings: [] }) as never : Promise.reject(new Error("temporary search failure")) as never);
    fireEvent.change(input, { target: { value: "nightly" } });
    await act(async () => { await vi.advanceTimersByTimeAsync(250); });
    expect(screen.getByText(/temporary search failure/)).toBeTruthy();
    get.mockImplementation((path) => Promise.resolve(path === "/api/v1/jobs" ? { data: [], warnings: [] } : page("nightly-backup")) as never);
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    await act(async () => { await vi.advanceTimersByTimeAsync(0); });
    expect(screen.getByText("nightly-")).toBeTruthy();
    expect((input as HTMLInputElement).value).toBe("nightly");
  });

  it("keeps headers and a semantic mobile body for first-use empty history", async () => {
    get.mockImplementation((path) => Promise.resolve(path === "/api/v1/jobs" ? { data: [], warnings: [] } : emptyPage()) as never);
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
      if (path === "/api/v1/jobs") return Promise.resolve({ data: [], warnings: [] }) as never;
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
    get.mockImplementation((path) => path === "/api/v1/jobs" ? Promise.resolve({ data: [], warnings: [] }) as never : request.promise as never);
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
