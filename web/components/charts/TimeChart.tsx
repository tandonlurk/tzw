"use client";

import { useEffect, useMemo, useRef, useState } from "react";
import {
  RANGES, compactUsdc, domainFor, formatTick, niceTicks, stepVertices, timeTicks, valueAt,
  type RangeKey, type SeriesPoint,
} from "@/lib/series";

export interface ChartLine {
  id: string;
  name: string;
  color: string;
  dash?: string;
  points: SeriesPoint[];
  /** Thicker, drawn last (e.g. the portfolio total). */
  emphasis?: boolean;
}

interface Props {
  lines: ChartLine[];
  /** Exact label for a value at hover (minor units in, text out). */
  formatValue: (v: number) => string;
  ariaLabel: string;
  /** Pixel height, or "fill" to take the parent's height. */
  height?: number | "fill";
  /** Show range buttons and zoom controls. */
  controls?: boolean;
  /** Show the toggleable legend (defaults to `controls`). */
  legend?: boolean;
  /** Minimal sparkline-like rendering: no axes, no hover. */
  compact?: boolean;
  /** Area fill under single lines. */
  area?: boolean;
  initialRange?: RangeKey;
  initialHidden?: string[];
  emptyText?: string;
}

const M = { top: 14, right: 56, bottom: 26, left: 8 };
const MC = { top: 4, right: 4, bottom: 4, left: 4 };

/** Interactive step-line chart: multi-series, legend toggles, ranges, hover crosshair, drag-to-zoom. Only draws given points. */
export function TimeChart({
  lines, formatValue, ariaLabel, height = 280, controls = true, legend, compact = false, area = false,
  initialRange = "all", initialHidden = [], emptyText = "No data yet.",
}: Props) {
  const wrap = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ w: 600, h: typeof height === "number" ? height : 280 });
  const [range, setRange] = useState<RangeKey>(initialRange);
  const [zoom, setZoom] = useState<[number, number] | null>(null);
  const [hidden, setHidden] = useState<Set<string>>(() => new Set(initialHidden));
  const [hoverX, setHoverX] = useState<number | null>(null);
  const [brush, setBrush] = useState<{ a: number; b: number } | null>(null);
  const [now] = useState(() => Date.now());

  useEffect(() => {
    const el = wrap.current;
    if (!el) return;
    const ro = new ResizeObserver(([e]) => {
      const w = Math.max(120, Math.floor(e.contentRect.width));
      const h = height === "fill" ? Math.max(120, Math.floor(e.contentRect.height)) : height;
      setSize((s) => (s.w === w && s.h === h ? s : { w, h }));
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, [height]);

  const visible = useMemo(() => lines.filter((l) => !hidden.has(l.id)), [lines, hidden]);
  const base = useMemo(() => domainFor(lines, range, now), [lines, range, now]);
  const dom = zoom ?? base;
  const m = compact ? MC : M;
  const W = size.w;
  const H = size.h;
  const iw = Math.max(10, W - m.left - m.right);
  const ih = Math.max(10, H - m.top - m.bottom);

  const geo = useMemo(() => {
    if (!dom) return null;
    const [t0, t1] = dom;
    const verts = visible.map((l) => stepVertices(l.points, t0, t1));
    const vals = verts.flat().map((p) => p.v);
    let lo = Math.min(0, ...vals);
    let hi = Math.max(0, ...vals);
    if (lo === hi) { lo -= 1_000_000; hi += 1_000_000; }
    const pad = (hi - lo) * 0.1;
    const yt = compact ? [] : niceTicks(lo - pad, hi + pad, Math.max(2, Math.round(ih / 56)));
    const y0 = compact ? lo - pad : Math.min(lo - pad, yt[0] ?? lo);
    const y1 = compact ? hi + pad : Math.max(hi + pad, yt[yt.length - 1] ?? hi);
    const x = (t: number) => m.left + ((t - t0) / Math.max(1, t1 - t0)) * iw;
    const y = (v: number) => m.top + (1 - (v - y0) / Math.max(1, y1 - y0)) * ih;
    const tx = compact ? { ticks: [], step: 60_000 } : timeTicks(t0, t1, Math.max(2, Math.round(iw / 110)));
    return { t0, t1, verts, x, y, yt: yt.filter((v) => v >= y0 && v <= y1), tx };
  }, [dom, visible, iw, ih, m.left, m.top, compact]);

  const hasData = lines.some((l) => l.points.length > 0);
  const tAt = (px: number) => (geo ? geo.t0 + ((px - m.left) / iw) * (geo.t1 - geo.t0) : 0);

  function localX(e: React.PointerEvent) {
    const r = (e.currentTarget as SVGElement).getBoundingClientRect();
    return Math.max(m.left, Math.min(m.left + iw, e.clientX - r.left));
  }

  const hoverT = hoverX !== null && geo ? tAt(hoverX) : null;
  const rows = hoverT !== null
    ? visible.map((l) => ({ l, v: valueAt(l.points, hoverT) })).sort((a, b) => b.v - a.v)
    : [];

  function toggle(id: string, solo: boolean) {
    setHidden((h) => {
      if (solo) {
        const others = lines.filter((l) => l.id !== id).map((l) => l.id);
        const alreadySolo = !h.has(id) && others.every((o) => h.has(o));
        return alreadySolo ? new Set() : new Set(others);
      }
      const n = new Set(h);
      if (n.has(id)) n.delete(id); else n.add(id);
      return n;
    });
  }

  return (
    <div className={`tchart${compact ? " compact" : ""}${height === "fill" ? " fill" : ""}`}>
      {controls && !compact && (
        <div className="tchart-bar">
          <div className="seg" role="group" aria-label="Time range">
            {RANGES.map((r) => (
              <button key={r.key} type="button" className={range === r.key && !zoom ? "on" : ""} aria-pressed={range === r.key && !zoom}
                onClick={() => { setRange(r.key); setZoom(null); }}>{r.label}</button>
            ))}
          </div>
          {zoom ? (
            <button type="button" className="xs ghost" onClick={() => setZoom(null)}>Reset zoom</button>
          ) : hasData ? (
            <span className="dim xs hide-sm">Drag across the chart to zoom</span>
          ) : null}
        </div>
      )}
      <div className="tchart-plot" ref={wrap} style={height === "fill" ? undefined : { height }}>
        {!hasData || !geo ? (
          <div className="tchart-empty">{emptyText}</div>
        ) : (
          <svg width={W} height={H} role="img" aria-label={ariaLabel}>
            <defs>
              {area && visible.map((l) => (
                <linearGradient key={l.id} id={`g-${l.id}`} x1="0" x2="0" y1="0" y2="1">
                  <stop offset="0%" stopColor={l.color} stopOpacity={0.22} />
                  <stop offset="100%" stopColor={l.color} stopOpacity={0} />
                </linearGradient>
              ))}
              <clipPath id={`clip-${ariaLabel.replace(/\W/g, "")}`}><rect x={m.left} y={0} width={iw} height={H} /></clipPath>
            </defs>
            {!compact && geo.yt.map((v) => (
              <g key={v}>
                <line className={v === 0 ? "grid zero" : "grid"} x1={m.left} x2={m.left + iw} y1={geo.y(v)} y2={geo.y(v)} />
                <text className="ylab" x={m.left + iw + 8} y={geo.y(v)} dy="0.32em">{compactUsdc(v)}</text>
              </g>
            ))}
            {compact && <line className="grid zero" x1={m.left} x2={m.left + iw} y1={geo.y(0)} y2={geo.y(0)} />}
            {!compact && geo.tx.ticks.map((t) => (
              <text key={t} className="xlab" x={geo.x(t)} y={H - 6} textAnchor="middle">{formatTick(t, geo.tx.step)}</text>
            ))}
            <g clipPath={`url(#clip-${ariaLabel.replace(/\W/g, "")})`}>
              {visible.map((l, i) => {
                const vs = geo.verts[i];
                if (!vs.length) return null;
                const d = vs.map((p, k) => `${k ? "L" : "M"}${geo.x(p.t).toFixed(1)},${geo.y(p.v).toFixed(1)}`).join("");
                return (
                  <g key={l.id} className="tline">
                    {area && <path d={`${d}L${geo.x(vs[vs.length - 1].t)},${geo.y(0)}L${geo.x(vs[0].t)},${geo.y(0)}Z`} fill={`url(#g-${l.id})`} />}
                    <path d={d} fill="none" stroke={l.color} strokeWidth={l.emphasis ? 2.25 : 1.6} strokeDasharray={l.dash || undefined} strokeLinejoin="round" />
                    {!compact && l.points.filter((p) => p.t >= geo.t0 && p.t <= geo.t1).map((p, k) => (
                      <circle key={k} cx={geo.x(p.t)} cy={geo.y(p.v)} r={2.4} fill="var(--bg)" stroke={l.color} strokeWidth={1.4} />
                    ))}
                  </g>
                );
              })}
            </g>
            {brush && (
              <rect className="brush" x={Math.min(brush.a, brush.b)} y={m.top} width={Math.abs(brush.b - brush.a)} height={ih} />
            )}
            {hoverX !== null && !compact && (
              <g className="hover" pointerEvents="none">
                <line x1={hoverX} x2={hoverX} y1={m.top} y2={m.top + ih} />
                {rows.map(({ l, v }) => <circle key={l.id} cx={hoverX} cy={geo.y(v)} r={3.5} fill={l.color} stroke="var(--bg)" strokeWidth={1.5} />)}
              </g>
            )}
            {!compact && (
              <rect
                className="overlay"
                x={m.left} y={m.top} width={iw} height={ih}
                onPointerMove={(e) => { const x = localX(e); setHoverX(x); if (brush) setBrush({ ...brush, b: x }); }}
                onPointerLeave={() => { setHoverX(null); }}
                onPointerDown={(e) => { (e.target as Element).setPointerCapture(e.pointerId); const x = localX(e); setBrush({ a: x, b: x }); }}
                onPointerUp={() => {
                  if (brush && Math.abs(brush.b - brush.a) > 8) {
                    const a = tAt(Math.min(brush.a, brush.b));
                    const b = tAt(Math.max(brush.a, brush.b));
                    if (b - a > 1000) setZoom([a, b]);
                  }
                  setBrush(null);
                }}
                onDoubleClick={() => setZoom(null)}
              />
            )}
          </svg>
        )}
        {hoverT !== null && rows.length > 0 && hoverX !== null && !compact && (
          <div className="tchart-tip" style={hoverX > W * 0.6 ? { right: W - hoverX + 14 } : { left: hoverX + 14 }}>
            <div className="t">{new Date(hoverT).toLocaleString(undefined, { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit", second: "2-digit" })}</div>
            {rows.map(({ l, v }) => (
              <div key={l.id} className="r">
                <span className="sw" style={{ background: l.color }} />
                <span className="n">{l.name}</span>
                <span className={`v ${v > 0 ? "pos" : v < 0 ? "neg" : ""}`}>{formatValue(v)}</span>
              </div>
            ))}
          </div>
        )}
      </div>
      {(legend ?? controls) && !compact && lines.length > 1 && (
        <div className="tlegend" role="group" aria-label="Series">
          {lines.map((l) => {
            const off = hidden.has(l.id);
            const last = l.points.length ? l.points[l.points.length - 1].v : 0;
            return (
              <button key={l.id} type="button" className={`tl-item${off ? " off" : ""}`} aria-pressed={!off}
                title="Click to show or hide. Alt-click to show only this one."
                onClick={(e) => toggle(l.id, e.altKey || e.shiftKey)}>
                <span className="sw" style={{ background: l.color, ...(l.dash ? { backgroundImage: "none", opacity: .8 } : {}) }} />
                <span className="n">{l.name}</span>
                <span className={`v ${last > 0 ? "pos" : last < 0 ? "neg" : "dim"}`}>{formatValue(last)}</span>
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}
