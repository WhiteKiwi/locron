import * as DialogPrimitive from "@radix-ui/react-dialog";
import * as MenuPrimitive from "@radix-ui/react-dropdown-menu";
import * as SelectPrimitive from "@radix-ui/react-select";
import * as TooltipPrimitive from "@radix-ui/react-tooltip";
import { Check, ChevronDown, Ellipsis, X } from "lucide-react";
import { type ReactNode } from "react";

function portalContainer() {
  return typeof document === "undefined" ? undefined : document.getElementById("portal-root") ?? undefined;
}

export type SelectOption = { value: string; label: string };
export function LocronSelect({ id, label, value, options, onChange }: { id?: string; label: string; value: string; options: SelectOption[]; onChange: (value: string) => void }) {
  return <span className="select-shell"><SelectPrimitive.Root value={value} onValueChange={onChange}>
    <SelectPrimitive.Trigger id={id} className="select-trigger" aria-label={label}>
      <SelectPrimitive.Value />
      <SelectPrimitive.Icon><ChevronDown size={16} aria-hidden="true" /></SelectPrimitive.Icon>
    </SelectPrimitive.Trigger>
    <SelectPrimitive.Portal container={portalContainer()}>
      <SelectPrimitive.Content className="select-content" position="popper" sideOffset={6}>
        <SelectPrimitive.Viewport>{options.map((option) => <SelectPrimitive.Item className="select-item" key={option.value} value={option.value}>
          <SelectPrimitive.ItemText>{option.label}</SelectPrimitive.ItemText><SelectPrimitive.ItemIndicator><Check size={14} /></SelectPrimitive.ItemIndicator>
        </SelectPrimitive.Item>)}</SelectPrimitive.Viewport>
      </SelectPrimitive.Content>
    </SelectPrimitive.Portal>
  </SelectPrimitive.Root></span>;
}

export function ActionMenu({ label = "Actions", children }: { label?: string; children: ReactNode }) {
  return <MenuPrimitive.Root><MenuPrimitive.Trigger className="icon-button" aria-label={label}><Ellipsis size={18} aria-hidden="true" /></MenuPrimitive.Trigger>
    <MenuPrimitive.Portal container={portalContainer()}><MenuPrimitive.Content className="menu-content" align="end" sideOffset={6}>{children}</MenuPrimitive.Content></MenuPrimitive.Portal>
  </MenuPrimitive.Root>;
}
export function ActionMenuItem({ children, onSelect, href, danger = false }: { children: ReactNode; onSelect?: () => void; href?: string; danger?: boolean }) {
  if (href) return <MenuPrimitive.Item asChild className={`menu-item${danger ? " danger" : ""}`}><a href={href}>{children}</a></MenuPrimitive.Item>;
  return <MenuPrimitive.Item className={`menu-item${danger ? " danger" : ""}`} onSelect={() => onSelect?.()}>{children}</MenuPrimitive.Item>;
}

export function Dialog({ open, onOpenChange, title, description, children }: { open: boolean; onOpenChange: (open: boolean) => void; title: string; description?: string; children: ReactNode }) {
  return <DialogPrimitive.Root open={open} onOpenChange={onOpenChange}><DialogPrimitive.Portal container={portalContainer()}>
    <DialogPrimitive.Overlay className="dialog-overlay" />
    <DialogPrimitive.Content className="dialog-content" onOpenAutoFocus={(event) => { const target = document.querySelector<HTMLElement>(".dialog-content [data-dialog-cancel]"); if (target) { event.preventDefault(); target.focus(); } }}>
      <div className="dialog-heading"><div><DialogPrimitive.Title>{title}</DialogPrimitive.Title>{description && <DialogPrimitive.Description>{description}</DialogPrimitive.Description>}</div><DialogPrimitive.Close className="icon-button" aria-label="Close dialog"><X size={18} /></DialogPrimitive.Close></div>
      {children}
    </DialogPrimitive.Content>
  </DialogPrimitive.Portal></DialogPrimitive.Root>;
}

export function Tooltip({ label, children }: { label: string; children: ReactNode }) {
  return <TooltipPrimitive.Root><TooltipPrimitive.Trigger asChild>{children}</TooltipPrimitive.Trigger><TooltipPrimitive.Portal container={portalContainer()}><TooltipPrimitive.Content className="tooltip-content" sideOffset={6}>{label}<TooltipPrimitive.Arrow className="tooltip-arrow" /></TooltipPrimitive.Content></TooltipPrimitive.Portal></TooltipPrimitive.Root>;
}

export function TooltipProvider({ children }: { children: ReactNode }) {
  return <TooltipPrimitive.Provider delayDuration={600} skipDelayDuration={300}>{children}</TooltipPrimitive.Provider>;
}

export function StatusBadge({ status, compact }: { status: string; compact?: string }) {
  const normalized = status.toLocaleLowerCase().replaceAll(" ", "_");
  const kind = ["running", "enabled", "succeeded", "healthy", "present"].includes(normalized) ? "positive" : ["failed", "stopped", "disabled", "absent", "cancelled"].includes(normalized) ? "negative" : ["queued", "admitted", "retry_wait", "unknown"].includes(normalized) ? "pending" : "neutral";
  return <span className={`status-badge ${kind}`} aria-label={status.replaceAll("_", " ")}><span aria-hidden="true" /><span className="status-text">{status.replaceAll("_", " ")}</span>{compact && <span className="status-compact" aria-hidden="true">{compact}</span>}</span>;
}

export function RouteHeader({ title, description, actions }: { title: string; description: string; actions?: ReactNode }) {
  return <div className="route-header"><div><p className="route-kicker">Local operations</p><h1>{title}</h1><p className="page-intro">{description}</p></div>{actions && <div className="route-actions">{actions}</div>}</div>;
}

export function ResponsiveData({ desktop, mobile, busy }: { desktop: ReactNode; mobile: ReactNode; busy?: boolean }) {
  return <div aria-busy={busy}><div className="desktop-data">{desktop}</div><div className="mobile-data">{mobile}</div></div>;
}

function DataEmpty({ title, description, actions }: { title: string; description: string; actions: ReactNode }) {
  return <div className="data-empty"><h2>{title}</h2><p>{description}</p><div className="empty-actions">{actions}</div></div>;
}

export function EmptyTableRow({ columns, title, description, actions }: { columns: number; title: string; description: string; actions: ReactNode }) {
  return <tr className="empty-table-row"><td colSpan={columns}><DataEmpty title={title} description={description} actions={actions} /></td></tr>;
}

export function EmptyObjectList({ title, description, actions }: { title: string; description: string; actions: ReactNode }) {
  return <div className="object-list empty-object-list"><DataEmpty title={title} description={description} actions={actions} /></div>;
}
