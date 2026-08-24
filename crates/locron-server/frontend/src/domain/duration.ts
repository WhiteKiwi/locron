import { formatExact, parseExact } from "./exact";
export const durationMultipliers = { s: 1_000_000n, m: 60_000_000n, h: 3_600_000_000n, d: 86_400_000_000n } as const;
export type DurationUnit = keyof typeof durationMultipliers;
export const parseDuration = (raw: string, unit: DurationUnit, allowZero = true) => parseExact(raw, unit, durationMultipliers, "Duration", allowZero);
export const formatDuration = (value: number | string | bigint) => formatExact(value, ["d", "h", "m", "s"], durationMultipliers);
export function humanDuration(value: number | string | bigint) {
  const formatted = formatDuration(value);
  return `${formatted.magnitude}${formatted.unit}`;
}
