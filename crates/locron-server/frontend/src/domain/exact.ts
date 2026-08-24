export const MAX_SAFE = BigInt(Number.MAX_SAFE_INTEGER);
const MAX_I64 = 9_223_372_036_854_775_807n;

export type ExactResult<U extends string> = { value: number; unit: U; exact: bigint };

export function parseExact<U extends string>(raw: string, selected: U, multipliers: Record<U, bigint>, label: string, allowZero = true): ExactResult<U> {
  const match = /^(\d+)(?:\.(\d{1,6}))?\s*([A-Za-z]+)?$/.exec(raw.trim());
  if (!match) throw new Error(`${label} must be one decimal amount with an optional unit`);
  const suffix = match[3];
  const unit = (suffix ? Object.keys(multipliers).find((item) => item.toLowerCase() === suffix.toLowerCase()) : selected) as U | undefined;
  if (!unit) throw new Error(`${label} has an invalid unit`);
  const fraction = match[2] ?? "";
  const scale = 10n ** BigInt(fraction.length);
  const numerator = BigInt(`${match[1]}${fraction}`) * multipliers[unit];
  if (numerator % scale !== 0n) throw new Error(`${label} is more precise than the stored unit`);
  const exact = numerator / scale;
  if (!allowZero && exact === 0n) throw new Error(`${label} must be greater than zero`);
  if (exact > MAX_I64) throw new Error(`${label} overflows signed 64-bit storage`);
  if (exact > MAX_SAFE) throw new Error(`${label} is not a safe JSON integer`);
  return { value: Number(exact), unit, exact };
}

export function formatExact<U extends string>(value: number | string | bigint, units: readonly U[], multipliers: Record<U, bigint>): { magnitude: string; unit: U } {
  const exact = BigInt(value);
  if (exact < 0n || exact > MAX_SAFE) throw new Error("value is outside the browser-safe range");
  for (const unit of units) {
    const multiplier = multipliers[unit];
    const whole = exact / multiplier;
    const remainder = exact % multiplier;
    if (exact < multiplier) continue;
    if (remainder === 0n) return { magnitude: whole.toString(), unit };
    const scaled = remainder * 1_000_000n;
    if (scaled % multiplier === 0n) {
      const fraction = (scaled / multiplier).toString().padStart(6, "0").replace(/0+$/, "");
      return { magnitude: `${whole}.${fraction}`, unit };
    }
  }
  const unit = units[units.length - 1];
  if (!unit) throw new Error("at least one unit is required");
  const multiplier = multipliers[unit];
  const scaled = (exact % multiplier) * 1_000_000n / multiplier;
  return { magnitude: `${exact / multiplier}.${scaled.toString().padStart(6, "0")}`.replace(/\.?0+$/, ""), unit };
}
