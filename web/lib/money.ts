// Money is a string of integer USDC minor units (6 decimals). Only BigInt is used; never floats.

export const DECIMALS = 6;
const SCALE = BigInt(10) ** BigInt(DECIMALS);

/** Format minor units as a decimal string, e.g. "1500000" -> "1.50". Shows at least `minFrac` decimals, trimming trailing zeros beyond that. */
export function formatMoney(minor: string, opts: { signed?: boolean; minFrac?: number; unit?: boolean } = {}): string {
  const { signed = false, minFrac = 2, unit = true } = opts;
  let v: bigint;
  try {
    v = BigInt(minor);
  } catch {
    return "n/a";
  }
  const neg = v < BigInt(0);
  const abs = neg ? -v : v;
  const whole = abs / SCALE;
  let frac = (abs % SCALE).toString().padStart(DECIMALS, "0");
  while (frac.length > minFrac && frac.endsWith("0")) frac = frac.slice(0, -1);
  const wholeStr = whole.toString().replace(/\B(?=(\d{3})+(?!\d))/g, ",");
  const body = frac.length ? `${wholeStr}.${frac}` : wholeStr;
  const sign = neg ? "-" : signed && v > BigInt(0) ? "+" : "";
  return `${sign}${body}${unit ? " USDC" : ""}`;
}

/**
 * Parse user input like "1.5" into minor units ("1500000"). Returns null when invalid:
 * empty, negative, non-numeric, or more than 6 decimals. Commas and surrounding spaces are tolerated.
 */
export function parseMoney(input: string): string | null {
  const s = input.trim().replace(/,/g, "");
  const m = /^(\d+)(?:\.(\d{0,6}))?$/.exec(s);
  if (!m) return null;
  const whole = BigInt(m[1]);
  const frac = BigInt((m[2] ?? "").padEnd(DECIMALS, "0") || "0");
  return (whole * SCALE + frac).toString();
}

/** Sum a list of minor-unit strings. Invalid items are ignored. */
export function sumMoney(values: string[]): string {
  let t = BigInt(0);
  for (const v of values) {
    try {
      t += BigInt(v);
    } catch {
      /* ignore */
    }
  }
  return t.toString();
}

/** -1, 0 or 1 by sign of a minor-unit string (0 for invalid). */
export function signOf(minor: string): -1 | 0 | 1 {
  try {
    const v = BigInt(minor);
    return v < BigInt(0) ? -1 : v > BigInt(0) ? 1 : 0;
  } catch {
    return 0;
  }
}
