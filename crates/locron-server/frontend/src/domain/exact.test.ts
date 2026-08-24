import { describe, expect, it } from "vitest";
import { formatDuration, humanDuration, parseDuration } from "./duration";
import { formatByteSize, parseByteSize } from "./byteSize";
import { parseInstant } from "./instant";

describe("exact operator quantities", () => {
  it("round trips duration without floating point", () => {
    expect(parseDuration("1.5m", "s").value).toBe(90_000_000);
    expect(formatDuration(60_000_001)).toEqual({ magnitude: "60.000001", unit: "s" });
    const compact = formatDuration(90_000_000);
    expect(parseDuration(compact.magnitude, compact.unit).value).toBe(90_000_000);
    expect(humanDuration(300_000_000)).toBe("5m");
    expect(humanDuration(3_600_000_000)).toBe("1h");
  });
  it("round trips binary byte sizes and keeps zero semantic", () => {
    expect(parseByteSize("1.5MiB", "B").value).toBe(1_572_864);
    expect(formatByteSize(268_435_456)).toEqual({ magnitude: "256", unit: "MiB" });
    expect(parseByteSize("0", "GiB").value).toBe(0);
  });
  it.each(["1e3", "1m 2s", "0.0000001s", "9007199255s"])("rejects invalid duration %s", (raw) => {
    expect(() => parseDuration(raw, "s")).toThrow();
  });
  it.each(["1MB", "1e3", "0.0000001KiB", "9007199254740992B"])("rejects invalid size %s", (raw) => {
    expect(() => parseByteSize(raw, "B")).toThrow();
  });
  it("converts a timezone-qualified instant without epoch input", () => {
    expect(parseInstant("2026-08-25T09:30:00", "UTC")).toBe(1_787_650_200_000_000);
    expect(() => parseInstant("not-an-instant", "local")).toThrow("complete local date and time");
    expect(() => parseInstant("2026-08-25T09:30:00", "Not/A-Timezone")).toThrow("valid IANA timezone");
  });
});
