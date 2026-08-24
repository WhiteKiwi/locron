import { describe, expect, it } from "vitest";
import { parseRoute, titleForRoute } from "./App";

describe("safe route titles", () => {
  it("distinguishes the new job task from generic job detail", () => {
    expect(titleForRoute(parseRoute("#/jobs/new"))).toBe("New job · Locron");
    expect(titleForRoute(parseRoute("#/jobs/job-id"))).toBe("Job · Locron");
    expect(titleForRoute(parseRoute("#/jobs/job-id/edit"))).toBe("Edit job · Locron");
  });
});
