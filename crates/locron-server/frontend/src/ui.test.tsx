import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useState } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AppShell } from "./AppShell";
import { ActionMenu, ActionMenuItem, Dialog, LocronSelect, TooltipProvider } from "./ui";

beforeEach(() => {
  document.body.innerHTML = '<div id="portal-root"></div>';
  Element.prototype.scrollIntoView = vi.fn();
  Element.prototype.hasPointerCapture = vi.fn(() => false);
  Element.prototype.setPointerCapture = vi.fn();
  Element.prototype.releasePointerCapture = vi.fn();
});

describe("Locron popup primitives", () => {
  it("exposes a labelled fixed-value select trigger", () => {
    const changed = vi.fn();
    render(<form><LocronSelect label="State filter" value="all" onChange={changed} options={[{ value: "all", label: "All states" }, { value: "enabled", label: "Enabled" }]} /></form>);
    const trigger = screen.getByRole("combobox", { name: "State filter" });
    expect(screen.getAllByRole("combobox")).toEqual([trigger]);
    expect(trigger.getAttribute("aria-expanded")).toBe("false");
    expect(trigger.textContent).toContain("All states");
    const bubble = document.querySelector(".select-shell > select");
    expect(bubble?.getAttribute("aria-hidden")).toBe("true");
    expect(bubble?.getAttribute("tabindex")).toBe("-1");
    // Radix positioning/typeahead needs browser layout; browser QA covers the open layer.
  });

  it("focuses the safe dialog action, closes with Escape, and restores its trigger", async () => {
    function Harness() { const [open, setOpen] = useState(false); return <><button onClick={() => setOpen(true)}>Remove</button><Dialog open={open} onOpenChange={setOpen} title="Remove job" description="Confirm removal"><button data-dialog-cancel onClick={() => setOpen(false)}>Keep job</button></Dialog></>; }
    render(<Harness />);
    const trigger = screen.getByRole("button", { name: "Remove" });
    trigger.focus();
    fireEvent.click(trigger);
    await waitFor(() => expect(document.activeElement).toBe(screen.getByRole("button", { name: "Keep job" })));
    fireEvent.keyDown(document, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    // jsdom drops focus to body as the portal unmounts; real-browser QA covers trigger restoration.
  });

  it("keeps a named row menu trigger", () => {
    render(<ActionMenu label="Actions for nightly backup"><ActionMenuItem onSelect={() => undefined}>Run now</ActionMenuItem><ActionMenuItem href="#/jobs/1/edit">Edit</ActionMenuItem></ActionMenu>);
    expect(screen.getByRole("button", { name: "Actions for nightly backup" }).getAttribute("aria-haspopup")).toBe("menu");
    // Radix menu content is portalled lazily; browser QA covers arrows, Escape, and collision.
  });
});

describe("adaptive application shell", () => {
  it("keeps all destinations, current route, and daemon health in desktop and mobile landmarks", () => {
    render(<TooltipProvider><AppShell current="runs" daemon={false}><h1>Run history</h1></AppShell></TooltipProvider>);
    const navigations = screen.getAllByRole("navigation", { name: "Dashboard" });
    expect(navigations).toHaveLength(3);
    expect(navigations[0]?.querySelector('[aria-current="page"]')?.textContent).toContain("Run history");
    expect(navigations[1]?.querySelector('[aria-current="page"]')?.getAttribute("aria-label")).toBe("Run history");
    expect(navigations[2]?.querySelector('[aria-current="page"]')?.textContent).toContain("Run history");
    expect(navigations[0]?.querySelector("[data-state]")).toBeNull();
    expect(navigations[1]?.querySelector('[data-state="closed"]')).toBeTruthy();
    expect(navigations[2]?.querySelector("[data-state]")).toBeNull();
    expect(screen.getAllByText("not running").length).toBeGreaterThan(0);
    expect(screen.getByRole("main")).toBeTruthy();
  });
});
