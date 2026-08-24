import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../api";
import { Settings } from "./Settings";

vi.mock("../api", () => ({ api: { get: vi.fn(), put: vi.fn(), delete: vi.fn() } }));
const get = vi.mocked(api.get), put = vi.mocked(api.put);

describe("settings review", () => {
  beforeEach(() => {
    get.mockResolvedValue({ data: { global_concurrency: 4, execution_path: "/bin:/usr/bin", run_retention_count: 100, run_retention_age_us: null, output_limit_bytes: 1024, per_run_output_limit_bytes: 512, environment: {} }, warnings: [] });
    put.mockResolvedValue({ data: {}, warnings: [] });
  });

  it("dry-runs a human-readable pruning change before explicit apply", async () => {
    render(<Settings />);
    const input = await screen.findByLabelText("Total retained output");
    fireEvent.change(input, { target: { value: "2" } });
    fireEvent.click(screen.getByRole("button", { name: "Review total output" }));
    await screen.findByRole("heading", { name: "Review durable change" });
    await waitFor(() => expect(document.activeElement).toBe(screen.getByRole("button", { name: "Cancel" })));
    expect(screen.getByText(/1,024 bytes.*2,048 bytes/)).toBeTruthy();
    expect(put).toHaveBeenCalledTimes(1);
    expect(put.mock.calls[0]?.[1]).toMatchObject({ dry_run: "1" });
    fireEvent.click(screen.getByRole("button", { name: "Apply change" }));
    await waitFor(() => expect(put).toHaveBeenCalledTimes(2));
    expect(put.mock.calls[1]?.[1]).toEqual({ value: "2048" });
  });
});
