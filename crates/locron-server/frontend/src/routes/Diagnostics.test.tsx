import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Diagnostics } from "./Diagnostics";

const fetchMock = vi.fn<typeof fetch>();
const diagnostics = {
  daemon_running: true,
  state_dir: "C:\\Users\\operator\\AppData\\Local\\locron",
  database: "C:\\Users\\operator\\AppData\\Local\\locron\\locron.db",
  execution_path: "C:\\Windows\\System32;C:\\Windows",
  checks: ["Database integrity verified"],
};

function respond(facts: Record<string, unknown>) {
  fetchMock.mockResolvedValue(new Response(JSON.stringify({
    schema: "locron.api/v1", ok: true, data: { ...diagnostics, ...facts }, warnings: [],
  }), { status: 200 }));
}

function fact(label: string) {
  const value = screen.getByText(label, { selector: "dt" }).nextElementSibling;
  expect(value?.tagName).toBe("DD");
  return value as HTMLElement;
}

function expectOneRead() {
  expect(fetchMock).toHaveBeenCalledTimes(1);
  const [path, init] = fetchMock.mock.calls[0]!;
  expect(path).toBe("/api/v1/diagnostics");
  expect(init?.method).toBe("GET");
  expect(init?.body).toBeUndefined();
  expect(init?.signal).toBeInstanceOf(AbortSignal);
}

describe("passive diagnostics wake facts", () => {
  beforeEach(() => { fetchMock.mockReset(); vi.stubGlobal("fetch", fetchMock); });
  afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

  it("renders Windows named-pipe facts from the API envelope without claiming socket absence", async () => {
    respond({ wake_socket: null, wake: { transport: "named_pipe", socket_present: null, availability: "unprobed" } });
    render(<Diagnostics />);
    await screen.findByRole("heading", { name: "Health & exposure" });
    expect(fact("Wake transport").textContent).toBe("Named pipe");
    expect(fact("Wake availability").textContent).toBe("Not probed");
    expect(screen.queryByText("Wake socket", { selector: "dt" })).toBeNull();
    expect(fact("Daemon").textContent).toBe("running");
    expect(fact("State directory").textContent).toBe(diagnostics.state_dir);
    expect(fact("Database").textContent).toBe(diagnostics.database);
    expect(fact("Execution path").textContent).toBe(diagnostics.execution_path);
    expect(screen.getByText(diagnostics.checks[0]!)).toBeTruthy();
    expectOneRead();
  });

  it.each([{ wake_socket: true, status: "present" }, { wake_socket: false, status: "absent" }])(
    "preserves Unix socket $status without additive wake facts",
    async ({ wake_socket, status }) => {
      respond({ wake_socket });
      render(<Diagnostics />);
      await screen.findByRole("heading", { name: "Health & exposure" });
      expect(fact("Wake socket").textContent).toBe(status);
      expect(screen.queryByText("Wake availability", { selector: "dt" })).toBeNull();
      expectOneRead();
    },
  );

  it.each([
    { label: "null legacy", facts: { wake_socket: null } },
    { label: "missing", facts: {} },
    { label: "null additive", facts: { wake_socket: null, wake: null } },
    { label: "unsupported transport", facts: { wake_socket: null, wake: { transport: "unsupported", socket_present: false, availability: "present" } } },
    { label: "missing transport", facts: { wake_socket: null, wake: { socket_present: false, availability: "unprobed" } } },
    { label: "null transport", facts: { wake_socket: null, wake: { transport: null, availability: "unprobed" } } },
  ])("renders $label facts as unknown", async ({ facts }) => {
    respond(facts);
    render(<Diagnostics />);
    await screen.findByRole("heading", { name: "Health & exposure" });
    expect(fact("Wake socket").textContent).toBe("unknown");
    expect(screen.queryByText("Wake availability", { selector: "dt" })).toBeNull();
    expectOneRead();
  });

  it.each([undefined, "present"])("keeps named-pipe availability unknown for unsupported fact %s", async (availability) => {
    respond({ wake_socket: null, wake: { transport: "named_pipe", availability } });
    render(<Diagnostics />);
    await screen.findByRole("heading", { name: "Health & exposure" });
    expect(fact("Wake transport").textContent).toBe("Named pipe");
    expect(fact("Wake availability").textContent).toBe("unknown");
    expectOneRead();
  });

  it("preserves the loading state while the single read is pending", () => {
    fetchMock.mockReturnValue(new Promise<Response>(() => {}));
    render(<Diagnostics />);
    expect(screen.getByText("Loading diagnostics…").getAttribute("aria-busy")).toBe("true");
    expect(screen.queryByRole("heading", { name: "Health & exposure" })).toBeNull();
    expectOneRead();
  });

  it("preserves API errors without rendering health facts or retrying", async () => {
    fetchMock.mockResolvedValue(new Response(JSON.stringify({
      schema: "locron.api/v1", ok: false, error: { code: "unavailable", message: "Diagnostics unavailable" },
    }), { status: 503 }));
    render(<Diagnostics />);
    expect((await screen.findByRole("alert")).textContent).toBe("Diagnostics unavailable");
    expect(screen.queryByText("Loading diagnostics…")).toBeNull();
    expect(screen.queryByRole("heading", { name: "Health & exposure" })).toBeNull();
    expectOneRead();
  });
});
