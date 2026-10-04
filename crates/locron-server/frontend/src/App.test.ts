import { describe, expect, it } from "vitest";
import { parseRoute, titleForRoute } from "./App";

describe("safe route titles", () => {
  it("distinguishes the new job task from generic job detail", () => {
    expect(titleForRoute(parseRoute("#/jobs/new"))).toBe("New job · Locron");
    expect(titleForRoute(parseRoute("#/jobs/job-id"))).toBe("Job · Locron");
    expect(titleForRoute(parseRoute("#/jobs/job-id/edit"))).toBe("Edit job · Locron");
  });
});

import { afterAll, vi } from "vitest";
import { createElement } from "react";
import { fireEvent, render, screen, waitFor, type RenderResult } from "@testing-library/react";
import { App } from "./App";
import { api } from "./api";

const COOKIE_TOKEN = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const COOKIE_CSRF = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
const CLIENT_KEYS = ["CL01", "CL02", "CL03", "CL04", "CL05", "CL06", "CL07", "CL08", "CL09", "CL10", "CL11", "CL12", "CL13", "CL14"] as const;
type ClientKey = typeof CLIENT_KEYS[number];
type FetchObservation = { path: string; method: string; headers: Headers; body: BodyInit | null | undefined; signal: AbortSignal | null | undefined };

function clientEnvelope(data: unknown, status = 200) {
  return new Response(JSON.stringify({ schema: "locron.api/v1", ok: true, data, warnings: [] }), { status, headers: { "Content-Type": "application/json" } });
}
function clientRefusal(status: number, message: string) {
  return new Response(JSON.stringify({ schema: "locron.api/v1", ok: false, error: { code: status === 401 ? "unauthenticated" : "refused", message } }), { status, headers: { "Content-Type": "application/json" } });
}
function visibleCookies() {
  return new Map(document.cookie.split(";").map((pair) => pair.trim()).filter(Boolean).map((pair) => {
    const separator = pair.indexOf("=");
    return [pair.slice(0, separator), pair.slice(separator + 1)] as const;
  }));
}
function documentAttributes(element: HTMLElement) {
  return new Map(Array.from(element.attributes, (attribute) => [attribute.name, attribute.value]));
}
function restoreAttributes(element: HTMLElement, original: Map<string, string>) {
  for (const attribute of Array.from(element.attributes)) if (!original.has(attribute.name)) element.removeAttribute(attribute.name);
  for (const [name, value] of original) element.setAttribute(name, value);
}
function storageInventory() {
  return new Map(Array.from({ length: localStorage.length }, (_, index) => localStorage.key(index)!).map((key) => [key, localStorage.getItem(key)!]));
}
function sameInventory(left: Map<string, string>, right: Map<string, string>) {
  return left.size === right.size && Array.from(left).every(([key, value]) => right.get(key) === value);
}

class CookieDomFixture {
  readonly started = performance.now();
  readonly cookieInventory = visibleCookies();
  readonly hash = location.hash;
  readonly title = document.title;
  readonly html = document.documentElement;
  readonly body = document.body;
  readonly htmlAttributes = documentAttributes(this.html);
  readonly bodyAttributes = documentAttributes(this.body);
  readonly storage = storageInventory();
  readonly bodyChildren = new Set(this.body.childNodes);
  readonly originalFetch = globalThis.fetch;
  readonly requests: FetchObservation[] = [];
  readonly signals = new Set<AbortSignal>();
  readonly container = document.createElement("div");
  readonly expiration = () => { this.expired += 1; };
  expired = 0;
  mounted: RenderResult | undefined;
  restoreFetch: (() => void) | undefined;

  constructor() {
    this.body.appendChild(this.container);
    window.addEventListener("session-expired", this.expiration);
  }
  remaining() {
    const remaining = 10_000 - (performance.now() - this.started);
    expect(remaining > 0, "real client case retains its original 10s bound").toBe(true);
    return remaining;
  }
  async assertSoon(assertion: () => void) {
    const started = performance.now();
    const limit = Math.min(2000, this.remaining());
    await waitFor(assertion, { timeout: limit });
    expect(performance.now() - started < limit, "real DOM observation completes within its original bound").toBe(true);
    this.remaining();
  }
  seed(cookie: string) {
    document.cookie = "csrf_token=; Max-Age=0; Path=/";
    document.cookie = "other=; Max-Age=0; Path=/";
    document.cookie = `${cookie}; Path=/`;
    const separator = cookie.indexOf("=");
    expect(visibleCookies().get(cookie.slice(0, separator)) === cookie.slice(separator + 1), "actual document.cookie stores the selected raw bytes").toBe(true);
  }
  intercept(handler: (request: FetchObservation) => Response) {
    const spy = vi.spyOn(globalThis, "fetch").mockImplementation((input, init) => {
      const request: FetchObservation = {
        path: typeof input === "string" ? input : input instanceof URL ? input.toString() : input.url,
        method: init?.method ?? "GET", headers: new Headers(init?.headers), body: init?.body, signal: init?.signal,
      };
      this.requests.push(request);
      if (request.signal) this.signals.add(request.signal);
      return Promise.resolve(handler(request));
    });
    this.restoreFetch = () => { spy.mockRestore(); };
  }
  mountApp() {
    location.hash = "#/jobs";
    this.mounted = render(createElement(App), { container: this.container });
  }
  finish() {
    expect(document.documentElement === this.html && document.body === this.body, "owned DOM roots retain actual identity").toBe(true);
    this.mounted?.unmount();
    for (const signal of this.signals) expect(signal.aborted, "real App/route effect cleanup aborts its actual request signal").toBe(true);
    window.removeEventListener("session-expired", this.expiration);
    expect(this.container.parentNode).toBe(this.body);
    this.container.remove();
    const beforeChildren = [...this.bodyChildren];
    const afterChildren = Array.from(this.body.childNodes);
    expect(afterChildren.length === beforeChildren.length && afterChildren.every((node, index) => node === beforeChildren[index]), "actual original child inventory is exact; no unknown portal or owned DOM remains").toBe(true);
    this.restoreFetch?.();
    expect(globalThis.fetch).toBe(this.originalFetch);
    for (const name of ["csrf_token", "other"]) {
      document.cookie = `${name}=; Max-Age=0; Path=/`;
      const original = this.cookieInventory.get(name);
      if (original !== undefined) document.cookie = `${name}=${original}; Path=/`;
    }
    expect(sameInventory(visibleCookies(), this.cookieInventory), "actual original visible-cookie inventory is exact").toBe(true);
    const oldTheme = this.storage.get("locron.theme");
    if (oldTheme === undefined) localStorage.removeItem("locron.theme"); else localStorage.setItem("locron.theme", oldTheme);
    expect(sameInventory(storageInventory(), this.storage), "actual original storage inventory is exact").toBe(true);
    restoreAttributes(this.html, this.htmlAttributes);
    restoreAttributes(this.body, this.bodyAttributes);
    expect(documentAttributes(this.html)).toEqual(this.htmlAttributes);
    expect(documentAttributes(this.body)).toEqual(this.bodyAttributes);
    location.hash = this.hash;
    document.title = this.title;
    expect(location.hash).toBe(this.hash);
    expect(document.title).toBe(this.title);
    this.remaining();
  }
}

describe("selected dashboard cookie recovery — actual api/App with fetch boundary only", () => {
  const completed = new Set<ClientKey>();
  async function checked(key: ClientKey, assertion: (fixture: CookieDomFixture) => Promise<void>) {
    expect(CLIENT_KEYS.includes(key) && !completed.has(key), "literal client key is selected once").toBe(true);
    const fixture = new CookieDomFixture();
    let asserted = false;
    try { await assertion(fixture); asserted = true; }
    finally { fixture.finish(); }
    expect(asserted).toBe(true);
    completed.add(key);
    console.info("PR145_CLIENT_LEDGER", JSON.stringify({ selected: [key], completed: [key], elapsed_ms: performance.now() - fixture.started, cleanup: "checked" }));
  }
  afterAll(() => {
    expect(Array.from(completed).sort()).toEqual([...CLIENT_KEYS]);
    expect(completed.size).toBe(14);
  });
  async function paste(fixture: CookieDomFixture, cookie: string, echo: string | null, body?: unknown) {
    fixture.seed(cookie);
    fixture.intercept(() => clientEnvelope({ authenticated: true }));
    if (body === undefined) await api.post("/api/v1/session"); else await api.post("/api/v1/session", body);
    expect(fixture.requests.length === 1, "actual literal paste fetch count is one").toBe(true);
    const request = fixture.requests[0]!;
    expect(request.path).toBe("/api/v1/session");
    expect(request.method).toBe("POST");
    expect(request.headers.get("X-CSRF-Token") === echo, "actual echo has exact selected bytes or is genuinely absent").toBe(true);
    expect(request.headers.get("Content-Type")).toBe("application/json");
    expect(request.body === JSON.stringify(body ?? {}), "actual literal paste JSON bytes are exact").toBe(true);
    expect(fixture.expired).toBe(0);
  }

  it("CL01 omits the literal malformed percent cookie and reaches fetch with exact secret JSON", () => checked("CL01", (fixture) => paste(fixture, "csrf_token=%recoverable", null, { token: COOKIE_TOKEN })), 10_000);
  it("CL02 omits a literal 63-byte CSRF cookie", () => checked("CL02", (fixture) => paste(fixture, `csrf_token=${COOKIE_CSRF.slice(0, 63)}`, null)), 10_000);
  it("CL03 omits a literal 64-byte nonhex CSRF cookie", () => checked("CL03", (fixture) => paste(fixture, `csrf_token=${"g".repeat(64)}`, null)), 10_000);
  it("CL04 omits percent-encoded hex without decoding the raw cookie", () => checked("CL04", (fixture) => paste(fixture, `csrf_token=%61${COOKIE_CSRF.slice(1)}`, null)), 10_000);
  it("CL05 retains actual readable empty-cookie storage with no echo", () => checked("CL05", (fixture) => paste(fixture, "csrf_token=", null)), 10_000);
  it("CL06 retains genuine cookie absence with an unrelated cookie", () => checked("CL06", (fixture) => paste(fixture, "other=OTHER", null)), 10_000);
  it("CL07 preserves valid lowercase echo bytes", () => checked("CL07", (fixture) => paste(fixture, `csrf_token=${COOKIE_CSRF}`, COOKIE_CSRF)), 10_000);
  it("CL08 preserves valid uppercase echo bytes", () => checked("CL08", (fixture) => paste(fixture, `csrf_token=${COOKIE_CSRF.toUpperCase()}`, COOKIE_CSRF.toUpperCase())), 10_000);
  it("CL09 preserves old URIError and fetch0 for a query-bearing paste path", () => checked("CL09", async (fixture) => {
    fixture.seed("csrf_token=%recoverable");
    fixture.intercept(() => { throw new Error("unexpected client fetch"); });
    await expect(api.post("/api/v1/session?x=QUERYCANARY")).rejects.toBeInstanceOf(URIError);
    expect(fixture.requests.length === 0, "query-bearing paste never reaches fetch").toBe(true);
  }), 10_000);
  it("CL10 preserves old URIError and fetch0 for ordinary PUT", () => checked("CL10", async (fixture) => {
    fixture.seed("csrf_token=%recoverable");
    fixture.intercept(() => { throw new Error("unexpected client fetch"); });
    await expect(api.put("/api/v1/pr145-probe")).rejects.toBeInstanceOf(URIError);
    expect(fixture.requests.length === 0, "ordinary malformed-cookie PUT never reaches fetch").toBe(true);
  }), 10_000);
  it("CL11 preserves ordinary POST JSON/header bytes", () => checked("CL11", async (fixture) => {
    fixture.seed(`csrf_token=${COOKIE_CSRF}`);
    fixture.intercept(() => clientEnvelope({}));
    await api.post("/api/v1/pr145-probe", { canary: "BODYCANARY" });
    expect(fixture.requests.length === 1, "ordinary POST actual fetch count is one").toBe(true);
    const request = fixture.requests[0]!;
    expect(request.path).toBe("/api/v1/pr145-probe");
    expect(request.method).toBe("POST");
    expect(request.headers.get("X-CSRF-Token") === COOKIE_CSRF, "ordinary POST echo bytes are exact").toBe(true);
    expect(request.headers.get("Content-Type")).toBe("application/json");
    expect(request.body === '{"canary":"BODYCANARY"}', "ordinary POST body bytes are exact").toBe(true);
  }), 10_000);
  it("CL12 drives actual bootstrap401 into the password Entry without protected route calls", () => checked("CL12", async (fixture) => {
    fixture.seed("csrf_token=%recoverable");
    fixture.intercept((request) => {
      expect(request.path).toBe("/api/v1/session");
      expect(request.method).toBe("GET");
      return clientRefusal(401, "a valid access token or session cookie is required");
    });
    fixture.mountApp();
    await fixture.assertSoon(() => expect(screen.getByLabelText("Access token").getAttribute("type")).toBe("password"));
    expect(fixture.requests.length === 1, "bootstrap401 makes only the actual status request").toBe(true);
    expect(fixture.signals.size).toBe(1);
    expect(fixture.requests.filter((request) => request.path === "/api/v1/diagnostics").length === 0, "bootstrap401 has diagnostics0").toBe(true);
    expect(document.querySelector(".app-shell")).toBeNull();
    for (const canary of [COOKIE_TOKEN, COOKIE_CSRF, "HEADERCANARY", "QUERYCANARY", "BODYCANARY"]) expect(document.body.textContent?.includes(canary)).toBe(false);
  }), 10_000);
  it("CL13 drives real trimmed Entry submission into actual Jobs/diagnostics through api", () => checked("CL13", async (fixture) => {
    fixture.seed("csrf_token=%recoverable");
    fixture.intercept((request) => {
      if (request.path === "/api/v1/session" && request.method === "GET") return clientRefusal(401, "a valid access token or session cookie is required");
      if (request.path === "/api/v1/session" && request.method === "POST") return clientEnvelope({ authenticated: true });
      if (request.path === "/api/v1/jobs?all=1") return clientEnvelope([]);
      if (request.path === "/api/v1/diagnostics") return clientEnvelope({ daemon_running: false });
      throw new Error("unexpected actual App recovery request");
    });
    fixture.mountApp();
    await fixture.assertSoon(() => expect(screen.getByLabelText("Access token")).toBeTruthy());
    const password = screen.getByLabelText("Access token") as HTMLInputElement;
    expect(password.type).toBe("password");
    fireEvent.change(password, { target: { value: `  ${COOKIE_TOKEN}  ` } });
    fireEvent.submit(password.form!);
    await fixture.assertSoon(() => expect(screen.getAllByText("No jobs yet")).toHaveLength(2));
    const pastes = fixture.requests.filter((request) => request.path === "/api/v1/session" && request.method === "POST");
    expect(pastes.length === 1, "actual Entry submission reaches paste once").toBe(true);
    expect(pastes[0]!.headers.get("X-CSRF-Token")).toBeNull();
    expect(pastes[0]!.body === JSON.stringify({ token: COOKIE_TOKEN }), "actual Entry sends the exact trimmed secret JSON").toBe(true);
    expect(screen.queryByLabelText("Access token")).toBeNull();
    expect(!password.isConnected || password.value === "").toBe(true);
    expect(document.querySelector(".app-shell")).toBeTruthy();
    expect(fixture.requests.filter((request) => request.path === "/api/v1/jobs?all=1").length === 1, "actual recovery reaches Jobs once").toBe(true);
    await fixture.assertSoon(() => expect(fixture.requests.filter((request) => request.path === "/api/v1/diagnostics").length === 1, "actual recovery reaches diagnostics once").toBe(true));
    expect(fixture.signals.size).toBe(3);
    // Fetch stubs do not deliver cookies or prove the backend issuer; paired backend rows do.
    expect(visibleCookies().get("csrf_token")).toBe("%recoverable");
    expect(visibleCookies().has("locron_session")).toBe(false);
  }), 10_000);
  it("CL14 retains actual authenticated Jobs and Feedback after an ordinary Run now403", () => checked("CL14", async (fixture) => {
    fixture.seed(`csrf_token=${COOKIE_CSRF}`);
    const job = { id: "pr145-owned-job", name: "owned-error-boundary", enabled: false, tags: [], definition_json: '{"schedule":{"kind":"every","interval":1000000,"anchor":0}}' };
    fixture.intercept((request) => {
      if (request.path === "/api/v1/session") return clientEnvelope({ authenticated: true });
      if (request.path === "/api/v1/jobs?all=1") return clientEnvelope([job]);
      if (request.path === "/api/v1/diagnostics") return clientEnvelope({ daemon_running: false });
      if (request.path === "/api/v1/runs?job=owned-error-boundary&limit=1") return clientEnvelope({ runs: [] });
      if (request.path === "/api/v1/jobs/pr145-owned-job/run") return clientRefusal(403, "ordinary request refused");
      throw new Error("unexpected actual App ordinary request");
    });
    fixture.mountApp();
    await fixture.assertSoon(() => expect(screen.getAllByText("owned-error-boundary")).toHaveLength(2));
    fireEvent.click(screen.getByRole("button", { name: "Run now" }));
    await fixture.assertSoon(() => expect(screen.getByText("ordinary request refused", { selector: "p.error" }).getAttribute("role")).toBe("status"));
    const mutations = fixture.requests.filter((request) => request.path === "/api/v1/jobs/pr145-owned-job/run");
    expect(mutations.length === 1, "actual ordinary Run now sends one mutation").toBe(true);
    expect(mutations[0]!.method).toBe("POST");
    expect(mutations[0]!.headers.get("X-CSRF-Token") === COOKIE_CSRF, "actual ordinary Run now echo bytes are exact").toBe(true);
    expect(fixture.expired).toBe(0);
    expect(screen.queryByLabelText("Access token")).toBeNull();
    expect(document.querySelector(".app-shell")).toBeTruthy();
    expect(fixture.requests.filter((request) => request.path === "/api/v1/session").length === 1, "ordinary403 does not initiate a new bootstrap").toBe(true);
    expect(fixture.requests.some((request) => request.path.includes("/preview"))).toBe(false);
    expect(fixture.signals.size).toBe(4);
  }), 10_000);
});
