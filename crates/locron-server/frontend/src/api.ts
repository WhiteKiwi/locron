export class ApiError extends Error { constructor(readonly code: string, message: string, readonly status: number) { super(message); } }
type Envelope<T> = { schema: "locron.api/v1"; ok: boolean; data: T; warnings: string[]; error?: { code: string; message: string } };
function cookie(name: string) { return document.cookie.split(";").map((part) => part.trim()).find((part) => part.startsWith(`${name}=`))?.slice(name.length + 1); }
async function request<T>(method: string, path: string, body?: unknown, init: RequestInit = {}) {
  const headers = new Headers(init.headers);
  if (body !== undefined) headers.set("Content-Type", "application/json");
  if (method !== "GET" && method !== "HEAD") { const csrf = cookie("csrf_token"); if (csrf) headers.set("X-CSRF-Token", decodeURIComponent(csrf)); }
  const response = await fetch(path, { ...init, method, headers, ...(body === undefined ? {} : { body: JSON.stringify(body) }) });
  const text = await response.text(); let payload: Envelope<T> | null = null;
  try { payload = JSON.parse(text) as Envelope<T>; } catch { /* handled below */ }
  if (payload?.schema === "locron.api/v1") { if (payload.ok) return { data: payload.data, warnings: payload.warnings ?? [] }; if (response.status === 401) window.dispatchEvent(new Event("session-expired")); throw new ApiError(payload.error?.code ?? "api_error", payload.error?.message ?? "Request failed", response.status); }
  if (!response.ok) throw new ApiError("http_error", text || response.statusText, response.status);
  return { data: payload as T, warnings: [] as string[] };
}
export const api = { get: <T>(path: string, init?: RequestInit) => request<T>("GET", path, undefined, init), post: <T>(path: string, body: unknown = {}) => request<T>("POST", path, body), put: <T>(path: string, body: unknown = {}) => request<T>("PUT", path, body), delete: <T>(path: string) => request<T>("DELETE", path) };
