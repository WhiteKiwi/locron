import { describe, expect, it } from "vitest";
import { buildJobPayload, defaultDefinition, parseSuccessStatuses } from "./JobForm";

describe("job payload grammar", () => {
  it("keeps an empty success status field empty rather than serializing zero", () => {
    expect(parseSuccessStatuses("   ")).toEqual([]);
    const definition = defaultDefinition();
    definition.cwd = "/tmp";
    definition.schedule = { kind: "every", interval: 1_000_000, anchor: 1_787_650_200_000_000 };
    definition.target = { kind: "http", method: "POST", url: "https://example.test", success_statuses: [0], follow_redirects: true, headers: {}, body: Array.from(new TextEncoder().encode("안녕")) };
    const result = buildJobPayload({ name: "unicode-http", description: "", tags: "", enabled: true, definition }, "  ");
    expect(result.errors).toEqual([]);
    expect(result.payload.definition.target).toMatchObject({ success_statuses: [], body: [236, 149, 136, 235, 133, 149] });
  });

  it("validates statuses, tags, blank arguments, and dependent concurrency", () => {
    expect(() => parseSuccessStatuses("99, 200")).toThrow("100 through 599");
    const definition = defaultDefinition();
    definition.cwd = "/tmp";
    definition.schedule = { kind: "every", interval: 1_000_000, anchor: 1_787_650_200_000_000 };
    definition.target = { kind: "process", executable: "/bin/echo", args: ["okay", ""] };
    definition.policy.overlap = "allow";
    definition.policy.per_job_concurrency = 0;
    const result = buildJobPayload({ name: "job", description: "", tags: "one,,two", enabled: true, definition }, "");
    expect(result.errors.map((error) => error.field)).toEqual(expect.arrayContaining(["job-tags", "process-arguments", "job-concurrency"]));
  });

  it("never sends redacted secret placeholders back as real values", () => {
    const definition = defaultDefinition();
    definition.cwd = "/tmp";
    definition.schedule = { kind: "every", interval: 1_000_000, anchor: 1_787_650_200_000_000 };
    definition.environment.values = { TOKEN: "<redacted>" };
    definition.target = { kind: "http", method: "POST", url: "https://example.test", success_statuses: [], follow_redirects: true, headers: { Authorization: { source: "inline", value: "<redacted>" } }, body: "<redacted>" };
    const result = buildJobPayload({ name: "secret-http", description: "", tags: "", enabled: true, definition }, "");
    expect(result.errors.map((error) => error.field)).toEqual(expect.arrayContaining(["http-headers", "http-body", "environment-values"]));
  });

  it("rejects invalid schedule, environment, path, and conflicting body grammar", () => {
    const definition = defaultDefinition();
    definition.cwd = "/tmp";
    definition.schedule = { kind: "every", interval: 0, anchor: 1_787_650_200_000_000 };
    definition.environment.path = "/bin::/usr/bin";
    definition.environment.values = { "BAD-NAME": "value" };
    definition.target = { kind: "http", method: "POST", url: "https://example.test", success_statuses: [], follow_redirects: true, headers: {}, body: [1], body_file: "/tmp/body" };
    const result = buildJobPayload({ name: "invalid", description: "", tags: "", enabled: true, definition }, "200");
    expect(result.errors.map((error) => error.field)).toEqual(expect.arrayContaining(["schedule-interval", "environment-path", "environment-values", "http-body"]));
  });
});
