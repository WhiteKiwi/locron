export type ThemePreference = "system" | "light" | "dark";
export type ResolvedTheme = "light" | "dark";
const key = "locron.theme";
const media = typeof window.matchMedia === "function"
  ? window.matchMedia("(prefers-color-scheme: dark)")
  : { matches: false, addEventListener: () => undefined };
let preference: ThemePreference = window.__LOCRON_THEME__?.preference ?? "system";
const listeners = new Set<() => void>();
export const resolveTheme = (value = preference): ResolvedTheme => value === "system" ? (media.matches ? "dark" : "light") : value;
export const getTheme = () => preference;
export function setTheme(value: ThemePreference, persist = true) {
  preference = value;
  const resolved = resolveTheme();
  document.documentElement.dataset.theme = resolved;
  document.documentElement.style.colorScheme = resolved;
  document.querySelector<HTMLMetaElement>('meta[name="theme-color"]')?.setAttribute("content", resolved === "dark" ? "#11141B" : "#F4F2EC");
  if (persist) { try { localStorage.setItem(key, value); } catch { /* private storage */ } }
  listeners.forEach((listener) => listener());
}
media.addEventListener("change", () => { if (preference === "system") setTheme("system", false); });
export function subscribeTheme(listener: () => void) { listeners.add(listener); return () => listeners.delete(listener); }

declare global { interface Window { __LOCRON_THEME__?: { preference: ThemePreference } } }
