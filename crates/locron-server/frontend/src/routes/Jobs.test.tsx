import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../api";
import type { Job } from "../types";
import { JobDetail, Jobs } from "./Jobs";

vi.mock("../api", () => ({ api: { get: vi.fn(), post: vi.fn() } }));
const get = vi.mocked(api.get);
const post = vi.mocked(api.post);
function deferred<T>() { let resolve!: (value: T) => void; let reject!: (reason: unknown) => void; const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; }

describe("responsive jobs data", () => {
  beforeEach(() => {
    post.mockReset();
    post.mockResolvedValue({ data: {}, warnings: [] });
    get.mockImplementation((path) => {
      if (path === "/api/v1/jobs?all=1") return Promise.resolve({ data: [{ id: "job-1", name: "nightly-backup-with-a-very-long-operator-visible-name", description: "archive", tags: ["production", "filesystem"], enabled: true, definition_json: JSON.stringify({ schedule: { kind: "every", interval: 300_000_000 } }) }], warnings: [] }) as never;
      if (String(path).includes("/preview")) return Promise.resolve({ data: { occurrences: ["2026-08-26T00:00:00Z"] }, warnings: [] }) as never;
      return Promise.resolve({ data: { runs: [] }, warnings: [] }) as never;
    });
  });

  it("renders matching core facts and named actions in table and mobile object variants", async () => {
    render(<Jobs />);
    await waitFor(() => expect(screen.getAllByText("nightly-backup-with-a-very-long-operator-visible-name")).toHaveLength(2));
    expect(get.mock.calls.some(([path]) => path === "/api/v1/jobs?all=1")).toBe(true);
    expect(get.mock.calls.some(([path]) => path === "/api/v1/jobs")).toBe(false);
    expect(screen.getAllByText("production · filesystem")).toHaveLength(2);
    expect(screen.getAllByText("every 5m")).toHaveLength(2);
    expect(screen.getAllByText("enabled")).toHaveLength(2);
    expect(screen.getAllByRole("button", { name: "Actions for nightly-backup-with-a-very-long-operator-visible-name" })).toHaveLength(2);
    expect(screen.getAllByRole("link", { name: /nightly-backup.*view job details/ })).toHaveLength(2);
    const table = screen.getByRole("table"), row = within(table).getByRole("row", { name: /nightly-backup/ });
    expect(row.getAttribute("role")).toBeNull();
    expect(row.getAttribute("tabindex")).toBeNull();
    location.hash = "";
    fireEvent.click(within(row).getByText("every 5m"));
    await waitFor(() => expect(location.hash).toBe("#/jobs/job-1"));
    location.hash = "";
    fireEvent.click(within(row).getByRole("button", { name: /Actions for/ }));
    expect(location.hash).toBe("");
  });

  it("keeps filter fields and result status in semantic reading order", async () => {
    render(<Jobs />);
    await screen.findAllByText("nightly-backup-with-a-very-long-operator-visible-name");
    const toolbar = document.querySelector<HTMLElement>('search[aria-label="Filter jobs"]')!;
    expect(toolbar).toBeTruthy();
    expect(toolbar.classList.contains("jobs-filter-grid")).toBe(true);
    const fields = toolbar.querySelectorAll(":scope > .field");
    expect(fields).toHaveLength(2);
    expect(fields[0]?.querySelector("label")?.textContent).toBe("Search jobs");
    expect(fields[0]?.querySelector(".field-help")?.textContent).toBe("Match a name, description, or tag.");
    expect(fields[1]?.querySelector("label")?.textContent).toBe("State filter");
    expect(fields[1]?.querySelector(".field-help")).toBeNull();
    const search = screen.getByRole("searchbox", { name: "Search jobs" });
    expect(search.getAttribute("aria-describedby")).toBe(fields[0]?.querySelector(".field-help")?.id);
    expect(screen.getByRole("combobox", { name: "State filter" })).toBeTruthy();
    const status = screen.getByRole("status");
    expect(status.parentElement).toBe(toolbar);
    expect(toolbar.children[0]).toBe(fields[0]);
    expect(toolbar.children[1]).toBe(fields[1]);
    expect(toolbar.children[2]).toBe(status);
  });

  it("keeps the table and mobile list frame for a first-use empty dataset", async () => {
    get.mockImplementation((path) => Promise.resolve(path === "/api/v1/jobs?all=1" ? { data: [], warnings: [] } : { data: { runs: [] }, warnings: [] }) as never);
    render(<Jobs />);
    await waitFor(() => expect(screen.getAllByText("No jobs yet")).toHaveLength(2));
    const table = screen.getByRole("table");
    expect(within(table).getAllByRole("columnheader")).toHaveLength(5);
    const emptyCell = within(table).getByRole("cell");
    expect(emptyCell.getAttribute("colspan")).toBe("5");
    const status = screen.getByRole("status");
    expect(status.textContent).toBe("0 results.");
    expect(table.getAttribute("aria-describedby")).toBe(status.id);
    expect(document.querySelector(".mobile-data article")).toBeNull();
    expect(document.querySelector(".empty-object-list .data-empty")).toBeTruthy();
    expect(screen.getAllByRole("link", { name: "Create job" })).toHaveLength(2);
  });

  it("clears a filtered zero, focuses search, and restores the result announcement", async () => {
    render(<Jobs />);
    await waitFor(() => expect(screen.getAllByText("nightly-backup-with-a-very-long-operator-visible-name")).toHaveLength(2));
    const input = screen.getByRole("searchbox", { name: "Search jobs" });
    fireEvent.change(input, { target: { value: "missing" } });
    expect(screen.getAllByText("No jobs match these filters")).toHaveLength(2);
    expect(screen.getByRole("status").textContent).toBe("0 results.");
    const clear = screen.getAllByRole("button", { name: "Clear filters" });
    expect(clear).toHaveLength(2);
    expect(clear[0]?.hasAttribute("disabled")).toBe(false);
    fireEvent.click(clear[0]!);
    expect(document.activeElement).toBe(input);
    await waitFor(() => expect(screen.getByRole("status").textContent).toBe("1 result."));
    expect((input as HTMLInputElement).value).toBe("");
  });

  it("keeps loading and request error distinct from successful empty data", async () => {
    const request = deferred<{ data: Job[]; warnings: never[] }>();
    get.mockImplementation((path) => path === "/api/v1/jobs?all=1" ? request.promise as never : Promise.resolve({ data: { runs: [] }, warnings: [] }) as never);
    render(<Jobs />);
    expect(screen.getByText("Loading jobs…", { selector: ".loading-state" })).toBeTruthy();
    expect(screen.queryByRole("table")).toBeNull();
    expect(screen.queryByText("No jobs yet")).toBeNull();
    await act(async () => { request.reject(new Error("jobs unavailable")); await Promise.resolve(); });
    expect(screen.getByText("jobs unavailable")).toBeTruthy();
    expect(screen.queryByText("Loading jobs…", { selector: ".loading-state" })).toBeNull();
    expect(screen.queryByRole("table")).toBeNull();
    expect(screen.queryByText("No jobs yet")).toBeNull();
  });
});

describe("complete enabled and disabled Jobs collection", () => {
  const definition = JSON.stringify({ schedule: { kind: "every", interval: 300_000_000 } });
  const enabledJob: Job = { id: "job-enabled", name: "nightly-backup", description: "Archive the primary volume", tags: ["production", "filesystem"], enabled: true, definition_json: definition };
  const disabledJob: Job = { id: "job-disabled", name: "api-heartbeat", description: "Pulse monitor", tags: ["service", "monitoring"], enabled: false, definition_json: definition };

  beforeEach(() => {
    location.hash = "#/jobs";
    post.mockReset();
    post.mockResolvedValue({ data: {}, warnings: [] });
    Element.prototype.hasPointerCapture = vi.fn(() => false);
    Element.prototype.setPointerCapture = vi.fn();
    Element.prototype.releasePointerCapture = vi.fn();
    Element.prototype.scrollIntoView = vi.fn();
  });

  function completeCollection(jobs = [enabledJob, disabledJob]) {
    get.mockReset();
    get.mockImplementation((path) => {
      if (path === "/api/v1/jobs?all=1") return Promise.resolve({ data: jobs, warnings: [] }) as never;
      if (String(path).includes("/preview")) return Promise.resolve({ data: { occurrences: ["2026-08-26T00:00:00Z"] }, warnings: [] }) as never;
      if (String(path).startsWith("/api/v1/runs?job=")) return Promise.resolve({ data: { runs: [{ state: "succeeded", requested_at_us: 1_800_000_000_000_000 }] }, warnings: [] }) as never;
      return Promise.reject(new Error(`unexpected path ${String(path)}`)) as never;
    });
  }

  it("lists both states, filters partial text fields, and never previews a disabled job", async () => {
    completeCollection();
    render(<Jobs />);
    await waitFor(() => expect(screen.getByRole("status").textContent).toBe("2 results."));
    expect(screen.getAllByText("nightly-backup")).toHaveLength(2);
    expect(screen.getAllByText("api-heartbeat")).toHaveLength(2);
    expect(screen.getAllByRole("link", { name: /api-heartbeat.*view job details/ })).toHaveLength(2);
    expect(screen.getAllByText("disabled — not scheduled")).toHaveLength(2);
    expect(screen.getAllByText(/succeeded ·/)).toHaveLength(4);
    expect(get.mock.calls.some(([path]) => path === "/api/v1/jobs/job-disabled/preview?count=1")).toBe(false);
    expect(get.mock.calls.some(([path]) => path === "/api/v1/jobs/job-enabled/preview?count=1")).toBe(true);

    const search = screen.getByRole("searchbox", { name: "Search jobs" });
    fireEvent.change(search, { target: { value: "pulse" } });
    expect(screen.getByRole("status").textContent).toBe("1 result.");
    expect(screen.queryByText("nightly-backup")).toBeNull();
    expect(screen.getAllByText("api-heartbeat")).toHaveLength(2);
    fireEvent.change(search, { target: { value: "file" } });
    expect(screen.getAllByText("nightly-backup")).toHaveLength(2);
    expect(screen.queryByText("api-heartbeat")).toBeNull();
    fireEvent.change(search, { target: { value: "heart" } });
    expect(screen.getAllByText("api-heartbeat")).toHaveLength(2);
  });

  it("applies All, Enabled, and Disabled to the complete local collection", async () => {
    completeCollection();
    render(<Jobs />);
    await waitFor(() => expect(screen.getByRole("status").textContent).toBe("2 results."));
    const chooseState = async (name: "All states" | "Enabled" | "Disabled") => {
      const trigger = screen.getByRole("combobox", { name: "State filter" });
      fireEvent.keyDown(trigger, { key: "ArrowDown" });
      const option = await screen.findByRole("option", { name });
      fireEvent.click(option);
    };
    await chooseState("Enabled");
    expect(screen.getByRole("status").textContent).toBe("1 result.");
    expect(screen.getAllByText("nightly-backup")).toHaveLength(2);
    expect(screen.queryByText("api-heartbeat")).toBeNull();
    await chooseState("Disabled");
    expect(screen.getAllByText("api-heartbeat")).toHaveLength(2);
    expect(screen.queryByText("nightly-backup")).toBeNull();
    await chooseState("All states");
    expect(screen.getByRole("status").textContent).toBe("2 results.");
  });

  it("disables and enables from the row menu without navigation or filter reset", async () => {
    let disabled = false;
    get.mockReset();
    get.mockImplementation((path) => {
      if (path === "/api/v1/jobs?all=1") return Promise.resolve({ data: [{ ...enabledJob, enabled: !disabled }], warnings: [] }) as never;
      if (String(path).includes("/preview")) return Promise.resolve({ data: { occurrences: ["2026-08-26T00:00:00Z"] }, warnings: [] }) as never;
      if (String(path).startsWith("/api/v1/runs?job=")) return Promise.resolve({ data: { runs: [] }, warnings: [] }) as never;
      return Promise.reject(new Error(`unexpected path ${String(path)}`)) as never;
    });
    post.mockImplementation((path) => { disabled = String(path).endsWith("/disable"); return Promise.resolve({ data: {}, warnings: [] }) as never; });
    render(<Jobs />);
    const search = await screen.findByRole("searchbox", { name: "Search jobs" });
    fireEvent.change(search, { target: { value: "night" } });
    const chooseState = async (name: "Enabled" | "Disabled") => {
      const trigger = screen.getByRole("combobox", { name: "State filter" });
      fireEvent.keyDown(trigger, { key: "ArrowDown" });
      fireEvent.click(await screen.findByRole("option", { name }));
    };
    const openAction = async (action: "Disable" | "Enable") => {
      const trigger = screen.getAllByRole("button", { name: "Actions for nightly-backup" })[0]!;
      fireEvent.pointerDown(trigger, { button: 0, ctrlKey: false });
      fireEvent.click(trigger);
      const item = await screen.findByRole("menuitem", { name: action });
      fireEvent.click(item);
    };
    await chooseState("Enabled");
    await openAction("Disable");
    await waitFor(() => expect(post).toHaveBeenCalledWith("/api/v1/jobs/job-enabled/disable"));
    await waitFor(() => expect(screen.getAllByText("No jobs match these filters")).toHaveLength(2));
    expect(location.hash).toBe("#/jobs");
    expect((search as HTMLInputElement).value).toBe("night");
    expect(screen.getByRole("combobox", { name: "State filter" }).textContent).toContain("Enabled");
    await chooseState("Disabled");
    await waitFor(() => expect(screen.getAllByText("disabled")).toHaveLength(2));
    await openAction("Enable");
    await waitFor(() => expect(post).toHaveBeenCalledWith("/api/v1/jobs/job-enabled/enable"));
    await waitFor(() => expect(screen.getAllByText("No jobs match these filters")).toHaveLength(2));
    expect(location.hash).toBe("#/jobs");
    expect((search as HTMLInputElement).value).toBe("night");
    expect(screen.getByRole("combobox", { name: "State filter" }).textContent).toContain("Disabled");
  });
});

describe("job detail recent runs", () => {
  const runId = "run-12345678-90ab-cdef-1234-567890abcdef";
  const detail: Job = {
    id: "job-1",
    name: "nightly-backup",
    enabled: true,
    definition_json: JSON.stringify({
      schedule: { kind: "every", interval: 300_000_000, anchor: 1_800_000_000_000_000 },
      target: { kind: "shell", command: "backup", shell: "/bin/sh" },
      cwd: "/tmp",
      environment: { values: {} },
      policy: { overlap: "skip", missed_run: "skip", start_deadline: null, catch_up_limit: 1, retries: 0, retry_delay: 1_000_000, retry_cap: 1_000_000, backoff: "fixed", retry_timeout: false, timeout: null, termination_grace: 1_000_000, per_job_concurrency: 1 },
    }),
  };

  beforeEach(() => {
    location.hash = "";
    get.mockImplementation((path) => {
      if (path === "/api/v1/jobs/job-1") return Promise.resolve({ data: detail, warnings: [] }) as never;
      if (path === "/api/v1/jobs/job-1/why") return Promise.resolve({ data: { explanation: "ready", overlap: "skip", daemon_running: true }, warnings: [] }) as never;
      if (path === "/api/v1/runs?job=job-1&limit=5") return Promise.resolve({ data: { runs: [{ id: runId, state: "succeeded", requested_at_us: 1_800_000_000_000_000 }] }, warnings: [] }) as never;
      return Promise.reject(new Error(`unexpected path ${String(path)}`)) as never;
    });
  });

  it("uses the shared whole-row contract while keeping the full run ID accessible", async () => {
    render(<JobDetail reference="job-1" />);
    const link = await screen.findByRole("link", { name: new RegExp(`view full run ${runId} details`) });
    expect(link.textContent).toContain(runId);
    const row = link.closest("tr")!;
    expect(link.getAttribute("data-row-link")).not.toBeNull();
    expect(row.getAttribute("role")).toBeNull();
    expect(row.getAttribute("tabindex")).toBeNull();
    fireEvent.click(within(row).getByText("succeeded"));
    await waitFor(() => expect(location.hash).toBe(`#/runs/${runId}`));
    location.hash = "";
    fireEvent.click(within(row).getByText("succeeded"), { metaKey: true });
    expect(location.hash).toBe("");
  });

  it("preserves the existing empty recent-runs state", async () => {
    get.mockImplementation((path) => {
      if (path === "/api/v1/jobs/job-1") return Promise.resolve({ data: detail, warnings: [] }) as never;
      if (path === "/api/v1/jobs/job-1/why") return Promise.resolve({ data: { explanation: "ready", overlap: "skip", daemon_running: true }, warnings: [] }) as never;
      if (path === "/api/v1/runs?job=job-1&limit=5") return Promise.resolve({ data: { runs: [] }, warnings: [] }) as never;
      return Promise.reject(new Error(`unexpected path ${String(path)}`)) as never;
    });
    render(<JobDetail reference="job-1" />);
    await screen.findByRole("heading", { name: "Recent runs" });
    expect(screen.getByText("No runs yet.")).toBeTruthy();
  });
});
