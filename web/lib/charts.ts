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

export interface AgentSeries {
  agentId: string;
  name: string;
  /** Cumulative realized P&L per settled event, from the ledger series; money minor units, drawing only. */
  points: (Pt & { label: string })[];
  finalMinor: number;
}

interface PnlEvent { ts: string; agent_id: string; agent_cumulative: string }

/** Group the API's pnl_over_time events into one series per agent (events keep API order). Nothing is interpolated or invented. */
export function perAgentSeries(
  events: PnlEvent[],
  agents: { agent_id: string; name: string }[],
  fmt: (v: string) => string = (v) => v,
  fmtTs: (ts: string) => string = (t) => t,
): AgentSeries[] {
  const names = new Map(agents.map((a) => [a.agent_id, a.name]));
  const out = new Map<string, AgentSeries>();
  for (const e of events) {
    let s = out.get(e.agent_id);
    if (!s) {
      s = { agentId: e.agent_id, name: names.get(e.agent_id) ?? e.agent_id, points: [], finalMinor: 0 };
      out.set(e.agent_id, s);
    }
    const y = Number(e.agent_cumulative);
    s.points.push({ x: new Date(e.ts).getTime(), y, label: `${s.name} ${fmtTs(e.ts)}: ${fmt(e.agent_cumulative)}` });
    s.finalMinor = y;
  }
  return [...out.values()];
}

/** Scale several series onto one shared axis (zero included). */
export function scaleMulti(series: Pt[][], w: number, h: number, pad = 8) {
  const all = series.flat();
  if (all.length === 0) return null;
  const shared = scaleSeries(all, w, h, pad);
  let i = 0;
  const lines = series.map((s) => {
    const pts = shared.points.slice(i, i + s.length);
    i += s.length;
    return pts;
  });
  return { lines, zeroY: shared.zeroY, yMin: shared.yMin, yMax: shared.yMax };
}
