import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../api";
import { Settings, type SettingsData } from "./Settings";

vi.mock("../api", () => ({ api: { get: vi.fn(), put: vi.fn(), delete: vi.fn() } }));
const get = vi.mocked(api.get), put = vi.mocked(api.put);
const saved: SettingsData = { global_concurrency: 4, execution_path: "/bin:/usr/bin", run_retention_count: 100, run_retention_age_us: null, output_limit_bytes: 1024, per_run_output_limit_bytes: 512, environment: {} };
const response = (data: SettingsData) => ({ data, warnings: [] });

describe("aggregate settings review", () => {
  beforeEach(() => {
    get.mockReset(); put.mockReset();
    get.mockResolvedValue(response(saved));
    put.mockResolvedValue({ data: {}, warnings: [] });
  });

  it("dry-runs several changes in order, opens one dialog, and applies once", async () => {
    get.mockResolvedValueOnce(response(saved)).mockResolvedValueOnce(response({ ...saved, global_concurrency: 6, output_limit_bytes: 2048 }));
    render(<Settings />);
    const concurrency = await screen.findByLabelText("Global concurrency");
    const output = screen.getByLabelText("Total retained output");
    const review = screen.getByRole("button", { name: "Review changes" });
    expect(review.hasAttribute("disabled")).toBe(true);
    expect(screen.queryByRole("button", { name: "Review total output" })).toBeNull();
    fireEvent.change(concurrency, { target: { value: "6" } });
    fireEvent.change(output, { target: { value: "2" } });
    expect(screen.getByText("2 unsaved durable changes")).toBeTruthy();
    const unload = new Event("beforeunload", { cancelable: true });
    window.dispatchEvent(unload);
    expect(unload.defaultPrevented).toBe(true);
    fireEvent.click(review);
    await screen.findByRole("heading", { name: "Review durable changes" });
    expect(screen.getByText("Global concurrency: 4 → 6")).toBeTruthy();
    expect(screen.getByText(/Total retained output: 1,024 bytes.*2,048 bytes/)).toBeTruthy();
    expect(put.mock.calls.map(([path, body]) => [path, body])).toEqual([
      ["/api/v1/settings/global_concurrency", { value: "6", dry_run: "1" }],
      ["/api/v1/settings/output_limit_bytes", { value: "2048", dry_run: "1" }],
    ]);
    fireEvent.click(screen.getByRole("button", { name: "Apply 2 changes" }));
    await waitFor(() => expect(screen.getByText("Saved 2 durable changes.")).toBeTruthy());
    expect(put.mock.calls.slice(2).map(([path, body]) => [path, body])).toEqual([
      ["/api/v1/settings/global_concurrency", { value: "6" }],
      ["/api/v1/settings/output_limit_bytes", { value: "2048" }],
    ]);
    expect(get).toHaveBeenCalledTimes(2);
    expect(screen.getByRole("button", { name: "Review changes" }).hasAttribute("disabled")).toBe(true);
  });

  it("keeps appearance immediate, focuses invalid fields, and discards durable edits", async () => {
    render(<Settings />);
    const concurrency = await screen.findByLabelText("Global concurrency");
    const review = screen.getByRole("button", { name: "Review changes" });
    fireEvent.click(screen.getByRole("radio", { name: "Dark" }));
    expect(review.hasAttribute("disabled")).toBe(true);
    fireEvent.change(concurrency, { target: { value: "0" } });
    fireEvent.click(review);
    expect(document.activeElement).toBe(concurrency);
    expect(put).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Discard changes" }));
    expect((concurrency as HTMLInputElement).value).toBe("4");
    expect(review.hasAttribute("disabled")).toBe(true);
  });

  it("retains a failed staged environment change after canonical refetch", async () => {
    get.mockResolvedValueOnce(response(saved)).mockResolvedValueOnce(response({ ...saved, global_concurrency: 6 }));
    put.mockImplementation((path, body) => !(body as { dry_run?: string } | undefined)?.dry_run && String(path).includes("environment.NEW_SECRET") ? Promise.reject(new Error("permission denied")) as never : Promise.resolve({ data: {}, warnings: [] }) as never);
    render(<Settings />);
    fireEvent.change(await screen.findByLabelText("Global concurrency"), { target: { value: "6" } });
    fireEvent.change(screen.getByLabelText("Variable name"), { target: { value: "NEW_SECRET" } });
    fireEvent.change(screen.getByLabelText("Variable value"), { target: { value: "sensitive-value" } });
    fireEvent.click(screen.getByRole("button", { name: "Review changes" }));
    await screen.findByRole("heading", { name: "Review durable changes" });
    expect(screen.getByText(/Environment NEW_SECRET.*value remains redacted/)).toBeTruthy();
    expect(screen.getByRole("dialog").textContent).not.toContain("sensitive-value");
    fireEvent.click(screen.getByRole("button", { name: "Apply 2 changes" }));
    await waitFor(() => expect(screen.getByText(/Saved 1 of 2 changes.*environment.NEW_SECRET.*permission denied/)).toBeTruthy());
    expect((screen.getByLabelText("Global concurrency") as HTMLInputElement).value).toBe("6");
    expect((screen.getByLabelText("Variable name") as HTMLInputElement).value).toBe("NEW_SECRET");
    expect((screen.getByLabelText("Variable value") as HTMLInputElement).value).toBe("sensitive-value");
    expect(screen.getByText("1 unsaved durable change")).toBeTruthy();
    expect(get).toHaveBeenCalledTimes(2);
  });

  it("blocks and focuses an invalid staged environment name before dry-run", async () => {
    render(<Settings />);
    const name = await screen.findByLabelText("Variable name");
    fireEvent.change(name, { target: { value: "LOCRON_SECRET" } });
    fireEvent.change(screen.getByLabelText("Variable value"), { target: { value: "sensitive-value" } });
    fireEvent.click(screen.getByRole("button", { name: "Review changes" }));
    await waitFor(() => expect(document.activeElement).toBe(name));
    expect(screen.getByText(/cannot start LOCRON_/)).toBeTruthy();
    expect(put).not.toHaveBeenCalled();
  });
});
