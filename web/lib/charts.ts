// Pure geometry helpers for the SVG charts. Money strings stay exact for labels; numbers here are only for drawing.

export interface Pt {
  x: number; // epoch ms (or any ordinal)
  y: number; // minor units, used for drawing only
}

export interface Scaled {
  points: { px: number; py: number }[];
  zeroY: number;
  yMin: number;
  yMax: number;
}

/** Map points into a w x h box with padding. Zero is always on the y axis. Needs at least one point. */
export function scaleSeries(pts: Pt[], w: number, h: number, pad = 8): Scaled {
  const ys = pts.map((p) => p.y);
  const yMin = Math.min(0, ...ys);
  const yMax = Math.max(0, ...ys);
  const ySpan = yMax - yMin || 1;
  const xs = pts.map((p) => p.x);
  const xMin = Math.min(...xs);
  const xSpan = Math.max(...xs) - xMin;
  const sx = (x: number) => (xSpan === 0 ? w / 2 : pad + ((x - xMin) / xSpan) * (w - 2 * pad));
  const sy = (y: number) => pad + (1 - (y - yMin) / ySpan) * (h - 2 * pad);
  return { points: pts.map((p) => ({ px: sx(p.x), py: sy(p.y) })), zeroY: sy(0), yMin, yMax };
}

/** Step path ("hold until the next event"), which is how a cumulative ledger series behaves. */
export function stepPath(points: { px: number; py: number }[]): string {
  if (points.length === 0) return "";
  let d = `M${points[0].px},${points[0].py}`;
  for (let i = 1; i < points.length; i++) d += ` H${points[i].px} V${points[i].py}`;
  return d;
}

/** Bar widths as percentages of the largest absolute value; 0 when everything is 0. */
export function barPercents(values: number[]): number[] {
  const max = Math.max(0, ...values.map((v) => Math.abs(v)));
  return values.map((v) => (max === 0 ? 0 : (Math.abs(v) / max) * 100));
}
