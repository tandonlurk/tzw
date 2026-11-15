"use client";

import { scaleSeries, stepPath, type Pt } from "@/lib/charts";

/** Step line chart over a series. Caller provides real points; this component never invents data. */
export function LineChart({
  points,
  ariaLabel,
  formatY,
  width = 640,
  height = 200,
}: {
  points: (Pt & { label?: string })[];
  ariaLabel: string;
  formatY: (y: number) => string;
  width?: number;
  height?: number;
}) {
  if (points.length === 0) return null;
  const s = scaleSeries(points, width, height, 12);
  return (
    <div className="chart">
      <svg viewBox={`0 0 ${width} ${height}`} role="img" aria-label={ariaLabel}>
        <line className="axis" x1={0} x2={width} y1={s.zeroY} y2={s.zeroY} />
        <path className="line" d={stepPath(s.points)} />
        {s.points.map((p, i) => (
          <circle key={i} className="dot" cx={p.px} cy={p.py} r={3}>
            <title>{points[i].label ?? formatY(points[i].y)}</title>
          </circle>
        ))}
      </svg>
      <div className="row spread dim" style={{ fontSize: ".8rem" }}>
        <span>min {formatY(s.yMin)}</span>
        <span>zero line shown; max {formatY(s.yMax)}</span>
      </div>
    </div>
  );
}
