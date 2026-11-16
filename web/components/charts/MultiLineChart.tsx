"use client";

import { scaleMulti, stepPath, type AgentSeries } from "@/lib/charts";

/** A cumulative ledger value holds until the next event, so extend each line to the right edge. */
function holdToEdge(pts: { px: number; py: number }[], edge: number) {
  const last = pts[pts.length - 1];
  return last && last.px < edge ? [...pts, { px: edge, py: last.py }] : pts;
}

export type SeriesTone = "pos" | "neg" | "info";

/** One step line per series on a shared axis. Caller supplies real points only. Tone drives colour via CSS classes. */
export function MultiLineChart({
  series,
  tone,
  ariaLabel,
  formatY,
  width = 640,
  height = 200,
}: {
  series: AgentSeries[];
  tone: (s: AgentSeries) => SeriesTone;
  ariaLabel: string;
  formatY: (y: number) => string;
  width?: number;
  height?: number;
}) {
  const m = scaleMulti(series.map((s) => s.points), width, height, 12);
  if (!m) return null;
  return (
    <div className="chart">
      <svg viewBox={`0 0 ${width} ${height}`} role="img" aria-label={ariaLabel}>
        <line className="axis" x1={0} x2={width} y1={m.zeroY} y2={m.zeroY} />
        {series.map((s, si) => (
          <g key={s.agentId} className={`series ${tone(s)}`}>
            <path className="line" d={stepPath(holdToEdge(m.lines[si], width - 12))} />
            {m.lines[si].map((p, i) => (
              <circle key={i} className="dot" cx={p.px} cy={p.py} r={3}>
                <title>{s.points[i].label}</title>
              </circle>
            ))}
          </g>
        ))}
      </svg>
      <div className="row spread dim" style={{ fontSize: ".8rem" }}>
        <span>min {formatY(m.yMin)}</span>
        <span>zero line shown; max {formatY(m.yMax)}</span>
      </div>
    </div>
  );
}
