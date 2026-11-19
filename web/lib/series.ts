// Time-series helpers for the interactive charts. Pure. Numbers here are for drawing only;
// labels are formatted from the exact money strings by the caller where it matters.

import type { Portfolio } from "./api";

export interface SeriesPoint { t: number; v: number }

export interface Series {
  id: string;
  name: string;
  points: SeriesPoint[]; // sorted by t; a step series ("holds until the next event")
}

/** Value of a step series at time t: the last point at or before t, else `before` (0 for cumulative P&L). */
export function valueAt(points: SeriesPoint[], t: number, before = 0): number {
  let lo = 0;
  let hi = points.length - 1;
  let best = -1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (points[mid].t <= t) {
      best = mid;
      lo = mid + 1;
    } else hi = mid - 1;
  }
  return best < 0 ? before : points[best].v;
}

/** Step polyline vertices for a series clipped to [t0, t1], starting from its value at t0. */
export function stepVertices(points: SeriesPoint[], t0: number, t1: number, before = 0): SeriesPoint[] {
  if (t1 < t0) return [];
  const out: SeriesPoint[] = [{ t: t0, v: valueAt(points, t0, before) }];
  for (const p of points) {
    if (p.t <= t0) continue;
    if (p.t > t1) break;
    out.push({ t: p.t, v: out[out.length - 1].v });
    out.push({ t: p.t, v: p.v });
  }
  out.push({ t: t1, v: out[out.length - 1].v });
  return out;
}

/** Cumulative realized P&L per agent plus the portfolio total, from /stats/portfolio. Nothing is interpolated. */
export function portfolioSeries(p: Portfolio): { agents: Series[]; total: Series } {
  const names = new Map(p.agents.map((a) => [a.agent_id, a.name]));
  const by = new Map<string, Series>();
  const total: Series = { id: "__total", name: "Portfolio", points: [] };
  for (const e of p.pnl_over_time) {
    const t = new Date(e.ts).getTime();
    if (Number.isNaN(t)) continue;
    let s = by.get(e.agent_id);
    if (!s) by.set(e.agent_id, (s = { id: e.agent_id, name: names.get(e.agent_id) ?? e.agent_id, points: [] }));
    s.points.push({ t, v: Number(e.agent_cumulative) });
    total.points.push({ t, v: Number(e.portfolio_cumulative) });
  }
  // Stable sort, then keep the last value per timestamp: two sides of one settlement share a time,
  // and the intermediate running total between them is not a real state.
  const sort = (s: Series) => {
    s.points.sort((a, b) => a.t - b.t);
    s.points = s.points.filter((p, i, arr) => i === arr.length - 1 || arr[i + 1].t !== p.t);
  };
  by.forEach(sort);
  sort(total);
  return { agents: [...by.values()], total };
}

export type RangeKey = "1h" | "6h" | "24h" | "7d" | "all";
export const RANGES: { key: RangeKey; label: string; ms: number | null }[] = [
  { key: "1h", label: "1H", ms: 3_600_000 },
  { key: "6h", label: "6H", ms: 6 * 3_600_000 },
  { key: "24h", label: "1D", ms: 86_400_000 },
  { key: "7d", label: "1W", ms: 7 * 86_400_000 },
  { key: "all", label: "All", ms: null },
];

/** Time domain for a range. "all" spans the data with a little padding; others end at max(now, last event)
 *  (the simulated clock can put events in the future). */
export function domainFor(series: Series[], range: RangeKey, now: number): [number, number] | null {
  const ts = series.flatMap((s) => s.points.map((p) => p.t));
  if (ts.length === 0) return null;
  const first = Math.min(...ts);
  const last = Math.max(...ts);
  const end = Math.max(now, last);
  const r = RANGES.find((x) => x.key === range);
  if (r?.ms) return [end - r.ms, end];
  const span = Math.max(last - first, 60_000);
  return [first - span * 0.06, last + span * 0.06];
}

/** "Nice" round tick values covering [min, max]. Always includes 0 when the range crosses it. */
export function niceTicks(min: number, max: number, count = 5): number[] {
  if (!Number.isFinite(min) || !Number.isFinite(max)) return [];
  if (min === max) {
    const pad = Math.abs(min) || 1;
    min -= pad;
    max += pad;
  }
  const raw = (max - min) / Math.max(1, count);
  const mag = 10 ** Math.floor(Math.log10(raw));
  const step = [1, 2, 2.5, 5, 10].map((m) => m * mag).find((s) => s >= raw) ?? 10 * mag;
  const start = Math.floor(min / step) * step;
  const out: number[] = [];
  for (let v = start; v <= max + step * 0.5; v += step) out.push(Math.round(v / step) * step);
  return out;
}

const MIN = 60_000;
const HOUR = 60 * MIN;
const DAY = 24 * HOUR;
const TIME_STEPS = [MIN, 5 * MIN, 15 * MIN, 30 * MIN, HOUR, 3 * HOUR, 6 * HOUR, 12 * HOUR, DAY, 2 * DAY, 7 * DAY, 30 * DAY];

/** Time ticks on round boundaries (local time) for a domain, about `count` of them. */
export function timeTicks(t0: number, t1: number, count = 6): { ticks: number[]; step: number } {
  const span = t1 - t0;
  if (span <= 0) return { ticks: [t0], step: MIN };
  const step = TIME_STEPS.find((s) => span / s <= count) ?? TIME_STEPS[TIME_STEPS.length - 1];
  const tz = new Date(t0).getTimezoneOffset() * MIN;
  const first = Math.ceil((t0 - tz) / step) * step + tz;
  const ticks: number[] = [];
  for (let t = first; t <= t1; t += step) ticks.push(t);
  return { ticks, step };
}

export function formatTick(t: number, step: number): string {
  const d = new Date(t);
  if (step >= DAY) return d.toLocaleDateString(undefined, { month: "short", day: "numeric" });
  return d.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
}

/** Compact money label for axis ticks from minor units (6 decimals), e.g. 2500000 -> "2.5". */
export function compactUsdc(minor: number): string {
  const v = minor / 1e6;
  const a = Math.abs(v);
  const s = a >= 1e6 ? `${(v / 1e6).toFixed(1)}M` : a >= 1e3 ? `${(v / 1e3).toFixed(1)}k` : a >= 100 ? v.toFixed(0) : a >= 1 ? v.toFixed(2).replace(/\.?0+$/, "") : v.toFixed(2);
  return s === "-0" ? "0" : s;
}
