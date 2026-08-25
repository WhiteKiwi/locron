import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { JobForm } from "./JobForm";

describe("job form progressive controls", () => {
  it("reveals only controls owned by the selected policies and target", () => {
    render(<JobForm />);
    expect(screen.queryByLabelText("Per-job concurrency")).toBeNull();
    fireEvent.click(screen.getByRole("radio", { name: /Allow/ }));
    expect(screen.getByLabelText("Per-job concurrency")).toBeTruthy();
    fireEvent.change(screen.getByLabelText("Per-job concurrency"), { target: { value: "3" } });
    fireEvent.click(within(screen.getByRole("group", { name: "Overlap" })).getByRole("radio", { name: /Skip/ }));
    fireEvent.click(within(screen.getByRole("group", { name: "Overlap" })).getByRole("radio", { name: /Allow/ }));
    expect((screen.getByLabelText("Per-job concurrency") as HTMLInputElement).value).toBe("3");
    expect(screen.queryByLabelText("Catch-up limit")).toBeNull();
    fireEvent.click(within(screen.getByRole("group", { name: "Missed runs" })).getByRole("radio", { name: /All/ }));
    expect(screen.getByLabelText("Catch-up limit")).toBeTruthy();
    expect(screen.queryByLabelText("Retry delay")).toBeNull();
    fireEvent.change(screen.getByLabelText("Retries"), { target: { value: "2" } });
    expect(screen.getByLabelText("Retry delay")).toBeTruthy();
    fireEvent.click(screen.getByRole("radio", { name: /HTTP/ }));
    expect(screen.getByLabelText("HTTP method")).toBeTruthy();
    fireEvent.change(screen.getByLabelText("Inline body"), { target: { value: "kept text" } });
    fireEvent.click(screen.getByRole("radio", { name: /Absolute file path/ }));
    expect(screen.getByLabelText("Body file")).toBeTruthy();
    expect(screen.queryByLabelText("Inline body")).toBeNull();
    fireEvent.click(screen.getByRole("radio", { name: /Inline UTF-8 text/ }));
    expect((screen.getByLabelText("Inline body") as HTMLTextAreaElement).value).toBe("kept text");
    fireEvent.click(screen.getByRole("button", { name: "Add path" }));
    expect(screen.getByLabelText("Job PATH override path 1")).toBeTruthy();
    expect(screen.getByText("Advanced colon-delimited value")).toBeTruthy();
  });

  it("summarizes validation and focuses the first invalid field", async () => {
    vi.useFakeTimers();
    render(<JobForm />);
    fireEvent.click(screen.getByRole("button", { name: "Save job" }));
    expect(document.getElementById("validation-summary")?.textContent).toContain("Name is required");
    await act(async () => { await vi.runAllTimersAsync(); });
    expect(document.activeElement).toBe(screen.getByLabelText("Name"));
    vi.useRealTimers();
  });
});
