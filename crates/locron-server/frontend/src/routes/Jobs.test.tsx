import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../api";
import type { Job } from "../types";
import { Jobs } from "./Jobs";

vi.mock("../api", () => ({ api: { get: vi.fn(), post: vi.fn() } }));
const get = vi.mocked(api.get);
function deferred<T>() { let resolve!: (value: T) => void; let reject!: (reason: unknown) => void; const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; }

describe("responsive jobs data", () => {
  beforeEach(() => {
    get.mockImplementation((path) => {
      if (path === "/api/v1/jobs") return Promise.resolve({ data: [{ id: "job-1", name: "nightly-backup-with-a-very-long-operator-visible-name", description: "archive", tags: ["production", "filesystem"], enabled: true, definition_json: JSON.stringify({ schedule: { kind: "every", interval: 300_000_000 } }) }], warnings: [] }) as never;
      if (String(path).includes("/preview")) return Promise.resolve({ data: { occurrences: ["2026-08-26T00:00:00Z"] }, warnings: [] }) as never;
      return Promise.resolve({ data: { runs: [] }, warnings: [] }) as never;
    });
  });

  it("renders matching core facts and named actions in table and mobile object variants", async () => {
    render(<Jobs />);
    await waitFor(() => expect(screen.getAllByText("nightly-backup-with-a-very-long-operator-visible-name")).toHaveLength(2));
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

  it("keeps the table and mobile list frame for a first-use empty dataset", async () => {
    get.mockImplementation((path) => Promise.resolve(path === "/api/v1/jobs" ? { data: [], warnings: [] } : { data: { runs: [] }, warnings: [] }) as never);
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
    get.mockImplementation((path) => path === "/api/v1/jobs" ? request.promise as never : Promise.resolve({ data: { runs: [] }, warnings: [] }) as never);
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
