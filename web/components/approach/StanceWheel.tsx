"use client";

import { useRef, useState } from "react";
import type { FirmRole } from "@/lib/api";
import {
  BPS,
  INITIATE_MAX_BPS,
  JOIN_MIN_BPS,
  firmRoleLabel,
  nudgeStance,
  pointToStance,
  stanceToPoint,
  stanceZone,
} from "@/lib/approach";
import { arcPath, polar } from "@/lib/wheel";
import { formatBps as bpsPct } from "@/lib/collaborations";

type Stance = { collaboration_bps: number; ambition_bps: number };

const W = 340;
const CX = W / 2;
const CY = 182;
const R = 128; // stance disc
const RING0 = 136;
const RING1 = 160; // firm role ring

/** Clock angle (radians, clockwise from 12) for a collaboration value: 0 → 9 o'clock, 10000 → 3 o'clock. */
const clockOf = (c: number) => -Math.PI / 2 + (Math.PI * c) / BPS;

const RING: { role: FirmRole; c0: number; c1: number }[] = [
  { role: "none", c0: 0, c1: 5000 },
  { role: "member", c0: 5000, c1: 7500 },
  { role: "lead", c0: 7500, c1: 10000 },
];

const ZONE_TEXT = {
  competitor: "Competitor: proposes challenges, stays out of firms",
  both: "Both: proposes challenges and works in firms",
  collaborator: "Collaborator: works in firms, does not start challenges",
};

/**
 * Half-disc wheel. Angle: competitive (left) to collaborative (right). Radius: cautious (centre) to ambitious (edge).
 * Outer ring picks the firm role. Drag, click or use the arrow keys; numeric inputs are rendered by the caller.
 */
export function StanceWheel({ stance, role, onStance, onRole }: {
  stance: Stance;
  role: FirmRole;
  onStance: (s: Stance) => void;
  onRole: (r: FirmRole) => void;
}) {
  const svgRef = useRef<SVGSVGElement>(null);
  const [drag, setDrag] = useState(false);
  const p = stanceToPoint(stance.collaboration_bps, stance.ambition_bps, R);
  const zone = stanceZone(stance.collaboration_bps);

  function fromEvent(e: React.PointerEvent) {
    const svg = svgRef.current;
    const ctm = svg?.getScreenCTM();
    if (!svg || !ctm) return;
    const pt = new DOMPoint(e.clientX, e.clientY).matrixTransform(ctm.inverse());
    onStance(pointToStance(pt.x - CX, CY - pt.y, R, stance.collaboration_bps));
  }

  function onKey(e: React.KeyboardEvent) {
    const next = nudgeStance(stance, e.key, e.shiftKey);
    if (next) {
      e.preventDefault();
      onStance(next);
    }
  }

  const line = (c: number) => {
    const o = polar(CX, CY, R, clockOf(c));
    return <line x1={CX} y1={CY} x2={o.x} y2={o.y} className="wheel-guide" />;
  };

  return (
    <svg ref={svgRef} viewBox={`0 0 ${W} ${CY + 30}`} className="wheel stance-wheel" role="group" aria-label="Stance wheel">
      {/* zones */}
      <path d={arcPath(CX, CY, 0, R, clockOf(0), clockOf(JOIN_MIN_BPS))} className="zone zone-neg" />
      <path d={arcPath(CX, CY, 0, R, clockOf(JOIN_MIN_BPS), clockOf(INITIATE_MAX_BPS))} className="zone zone-info" />
      <path d={arcPath(CX, CY, 0, R, clockOf(INITIATE_MAX_BPS), clockOf(BPS))} className="zone zone-pos" />
      {[0.33, 0.66].map((f) => (
        <path key={f} d={arcPath(CX, CY, R * f, R * f, clockOf(0), clockOf(BPS))} className="wheel-guide" fill="none" />
      ))}
      {line(JOIN_MIN_BPS)}
      {line(INITIATE_MAX_BPS)}

      {/* interactive disc */}
      <path
        d={arcPath(CX, CY, 0, R, clockOf(0), clockOf(BPS))}
        className={`disc${drag ? " dragging" : ""}`}
        tabIndex={0}
        role="slider"
        aria-roledescription="two-dimensional stance control"
        aria-label="Stance: left and right arrows change collaboration, up and down change ambition"
        aria-valuemin={0}
        aria-valuemax={BPS}
        aria-valuenow={stance.collaboration_bps}
        aria-valuetext={`Collaboration ${bpsPct(stance.collaboration_bps)}, ambition ${bpsPct(stance.ambition_bps)}`}
        onKeyDown={onKey}
        onPointerDown={(e) => {
          (e.target as Element).setPointerCapture(e.pointerId);
          setDrag(true);
          fromEvent(e);
        }}
        onPointerMove={(e) => drag && fromEvent(e)}
        onPointerUp={() => setDrag(false)}
        onPointerCancel={() => setDrag(false)}
      />

      {/* firm role ring */}
      <g role="radiogroup" aria-label="Firm role">
        {RING.map((s) => {
          const on = role === s.role;
          const mid = polar(CX, CY, (RING0 + RING1) / 2, clockOf((s.c0 + s.c1) / 2));
          return (
            <g key={s.role}>
              <path
                d={arcPath(CX, CY, RING0, RING1, clockOf(s.c0) + 0.01, clockOf(s.c1) - 0.01)}
                className={`ring-seg${on ? " on" : ""}`}
                role="radio"
                aria-checked={on}
                aria-label={firmRoleLabel(s.role)}
                tabIndex={0}
                onClick={() => onRole(s.role)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    onRole(s.role);
                  }
                }}
              >
                <title>{firmRoleLabel(s.role)}</title>
              </path>
              <text x={mid.x} y={mid.y} className="ring-label" textAnchor="middle" dominantBaseline="middle" pointerEvents="none">
                {s.role === "none" ? "No firm" : s.role === "member" ? "Join firms" : "Lead a firm"}
              </text>
            </g>
          );
        })}
      </g>

      {/* marker */}
      <circle cx={CX + p.x} cy={CY - p.y} r={8} className={`marker marker-${zone}`} pointerEvents="none" />

      <text x={CX - R} y={CY + 18} className="axis-label" textAnchor="start">Competitive</text>
      <text x={CX + R} y={CY + 18} className="axis-label" textAnchor="end">Collaborative</text>
      <text x={CX} y={CY + 18} className="axis-label" textAnchor="middle">cautious ⟶ edge: ambitious</text>
    </svg>
  );
}

export { ZONE_TEXT };
