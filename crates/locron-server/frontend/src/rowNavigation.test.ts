import { describe, expect, it, vi } from "vitest";
import { shouldNavigateRow } from "./rowNavigation";

function event(target: Element, currentTarget: Element, change: Partial<MouseEvent> = {}) {
  return { defaultPrevented: false, button: 0, metaKey: false, ctrlKey: false, shiftKey: false, altKey: false, target, currentTarget, ...change } as MouseEvent;
}

describe("safe whole-row pointer navigation", () => {
  it("accepts an ordinary click on noninteractive row space", () => {
    const row = document.createElement("tr"), cell = document.createElement("td"); row.append(cell);
    expect(shouldNavigateRow(event(cell, row), "")).toBe(true);
  });

  it("ignores selection, prevented, modified, and non-primary events", () => {
    const row = document.createElement("article"), surface = document.createElement("div"); row.append(surface);
    expect(shouldNavigateRow(event(surface, row), "selected")).toBe(false);
    for (const change of [{ defaultPrevented: true }, { button: 1 }, { metaKey: true }, { ctrlKey: true }, { shiftKey: true }, { altKey: true }]) expect(shouldNavigateRow(event(surface, row, change), "")).toBe(false);
  });

  it("ignores native links, controls, and menu items", () => {
    const row = document.createElement("article");
    for (const item of [document.createElement("a"), document.createElement("button"), document.createElement("input"), document.createElement("select"), document.createElement("textarea")]) { row.replaceChildren(item); expect(shouldNavigateRow(event(item, row), "")).toBe(false); }
    const menu = document.createElement("div"); menu.setAttribute("role", "menuitem"); row.replaceChildren(menu); expect(shouldNavigateRow(event(menu, row), "")).toBe(false);
  });

  it("does not treat an interactive ancestor outside the row as an embedded control", () => {
    const outside = document.createElement("button"), row = document.createElement("span"); outside.append(row);
    expect(shouldNavigateRow(event(row, row), "")).toBe(true);
    vi.restoreAllMocks();
  });
});
