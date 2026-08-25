import { Activity, BriefcaseBusiness, History, Settings } from "lucide-react";
import { type ReactNode } from "react";
import { StatusBadge, Tooltip } from "./ui";

const items = [
  { key: "jobs", label: "Jobs", icon: BriefcaseBusiness },
  { key: "runs", label: "Run history", icon: History },
  { key: "diagnostics", label: "Diagnostics", icon: Activity },
  { key: "settings", label: "Settings", icon: Settings },
] as const;

function LabelledNavItems({ current }: { current: string }) {
  return <>{items.map(({ key, label, icon: Icon }) => <a key={key} href={`#/${key}`} aria-current={current === key ? "page" : undefined}><Icon size={19} strokeWidth={1.8} aria-hidden="true" /><span>{label}</span></a>)}</>;
}
function CompactNavItems({ current }: { current: string }) {
  return <>{items.map(({ key, label, icon: Icon }) => <Tooltip key={key} label={label}><a href={`#/${key}`} aria-label={label} aria-current={current === key ? "page" : undefined}><Icon size={19} strokeWidth={1.8} aria-hidden="true" /></a></Tooltip>)}</>;
}

export function AppShell({ current, daemon, children }: { current: string; daemon: boolean | null; children: ReactNode }) {
  const daemonLabel = daemon ? "running" : daemon === false ? "not running" : "unknown";
  return <div className="app-shell">
    <a className="skip-link" href="#main-content">Skip to dashboard content</a>
    <aside className="side-rail">
      <a className="rail-brand" href="#/jobs"><span className="brand-mark" aria-hidden="true">L</span><span className="brand-copy"><strong>locron</strong><small>local scheduler</small></span></a>
      <nav className="rail-labelled" aria-label="Dashboard"><LabelledNavItems current={current} /></nav>
      <nav className="rail-compact" aria-label="Dashboard"><CompactNavItems current={current} /></nav>
      <div className="rail-status"><span className="rail-status-label">Daemon</span><StatusBadge status={daemonLabel} compact={daemon ? "On" : daemon === false ? "Off" : "?"} /></div>
    </aside>
    <header className="mobile-topbar"><a className="rail-brand" href="#/jobs"><span className="brand-mark" aria-hidden="true">L</span><span className="brand-copy"><strong>locron</strong></span></a><StatusBadge status={daemonLabel} /></header>
    <div className="shell-workbench">
      {daemon === false && <div className="banner offline" role="status"><strong>Daemon not running.</strong> Runs stay durably queued.</div>}
      <main id="main-content" tabIndex={-1}>{children}</main>
    </div>
    <nav className="mobile-nav" aria-label="Dashboard"><LabelledNavItems current={current} /></nav>
  </div>;
}
