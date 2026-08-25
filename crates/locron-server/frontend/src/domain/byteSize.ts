import { formatExact, parseExact } from "./exact";
export const byteMultipliers = { B: 1n, KiB: 1_024n, MiB: 1_048_576n, GiB: 1_073_741_824n } as const;
export type ByteUnit = keyof typeof byteMultipliers;
export const parseByteSize = (raw: string, unit: ByteUnit) => parseExact(raw, unit, byteMultipliers, "Size");
export const formatByteSize = (value: number | string | bigint) => formatExact(value, ["GiB", "MiB", "KiB", "B"], byteMultipliers);
export const byteEquivalent = (value: number) => `${new Intl.NumberFormat("en-US").format(value)} bytes`;
