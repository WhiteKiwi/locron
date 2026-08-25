import { fireEvent, render, screen, within } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, it } from "vitest";
import { DurationInput, ThemeControl } from "./components";

function DurationHarness({ initial = 60_000_000, nullable = false }: { initial?: number | null; nullable?: boolean }) {
  const [value, setValue] = useState<number | null>(initial);
  return <DurationInput label="Start deadline" value={value} nullable={nullable} nullableLabel="Off" description="Off accepts an occurrence regardless of age." onChange={setValue} />;
}

describe("duration input", () => {
  it("normalizes a pasted suffix into magnitude and unit on blur", () => {
    render(<DurationHarness />);
    const magnitude = screen.getByLabelText("Start deadline") as HTMLInputElement;
    fireEvent.change(magnitude, { target: { value: "1m" } });
    fireEvent.blur(magnitude);
    expect(magnitude.value).toBe("1");
    expect(screen.getByRole("combobox", { name: "Start deadline unit" }).textContent).toContain("minutes");
  });

  it("uses field-specific Off semantics instead of retention language", () => {
    render(<DurationHarness initial={null} nullable />);
    expect((screen.getByRole("checkbox", { name: "Off" }) as HTMLInputElement).checked).toBe(true);
    expect(screen.queryByText(/No age limit/)).toBeNull();
    expect(screen.getByText(/Off accepts an occurrence/)).toBeTruthy();
  });
});

describe("described control groups", () => {
  it("associates browser-local theme help with the complete radio group", () => {
    render(<ThemeControl name="test-theme" />);
    const group = screen.getByRole("group", { name: "Color theme" });
    const help = document.getElementById(group.getAttribute("aria-describedby") ?? "");
    expect(help?.textContent).toBe("Browser-local only; this does not change daemon settings.");
    expect(group.contains(help)).toBe(true);
    expect(within(group).getAllByRole("radio")).toHaveLength(3);
    expect(help?.previousElementSibling?.classList.contains("theme-options")).toBe(true);
  });
});
