import type { MouseEvent as ReactMouseEvent } from "react";

const interactiveSelector = "a,button,input,select,textarea,summary,[role='menuitem'],[data-row-interactive]";

type RowEvent = { defaultPrevented: boolean; button: number; metaKey: boolean; ctrlKey: boolean; shiftKey: boolean; altKey: boolean; target: EventTarget | null; currentTarget: EventTarget | null };
export function shouldNavigateRow(event: RowEvent, selection = globalThis.getSelection?.()?.toString() ?? "") {
  if (event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey || selection) return false;
  if (!(event.target instanceof Element) || !(event.currentTarget instanceof Element)) return false;
  const interactive = event.target.closest(interactiveSelector);
  return !interactive || !event.currentTarget.contains(interactive);
}

export function navigateRow(event: ReactMouseEvent<HTMLElement>) {
  if (!shouldNavigateRow(event)) return;
  event.currentTarget.querySelector<HTMLAnchorElement>("[data-row-link]")?.click();
}
