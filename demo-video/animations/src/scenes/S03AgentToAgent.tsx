// 03-agent-to-agent (10 s): continues 02's ring. The market shrinks to a data source;
// agents connect to each other with measurable challenges and locked stakes.
import React from "react";
import { AbsoluteFill, useCurrentFrame, useVideoConfig } from "remotion";
import { fade, lerp, prog, snap } from "../lib/anim";
import { RING, RING_C, RING_NAMES } from "../lib/ring";
import { ICONS } from "../lib/icons";
import { AgentAvatar, At, DrawLine, MoneyChip, Pill, Stage, Svg, Title, useMinimal } from "../lib/ui";

const PAIRS: { a: number; b: number; text: string }[] = [
  { a: 0, b: 1, text: "BTC contract ≥ 0.55 at end?" },
  { a: 2, b: 3, text: "ETH up 5 % by Friday?" },
  { a: 3, b: 4, text: "Rate-cut contract ≥ 0.40?" },
];

const Lock: React.FC<{ size?: number }> = ({ size = 44 }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" style={{ overflow: "visible" }}>
    <rect x="4" y="10.5" width="16" height="11" rx="2.5" style={{ fill: "var(--yellow-a)", stroke: "var(--yellow)", strokeWidth: 1.8 }} />
    <path d="M8 10.5V7.5a4 4 0 0 1 8 0v3" style={{ fill: "none", stroke: "var(--yellow)", strokeWidth: 1.8 }} />
  </svg>
);

export const S03AgentToAgent: React.FC = () => {
  const f = useCurrentFrame();
  const { fps } = useVideoConfig();
  const minimal = useMinimal();
  const shrink = prog(f, 0.3, 1.2);
  const mx = RING_C.x, my = lerp(RING_C.y, 118, shrink);
  return (
    <Stage tag="Example">
      <Svg>
        {RING.map((p, i) => (
          <DrawLine key={i} x1={p.x} y1={p.y} x2={mx} y2={my} t={1} color="var(--steel)" width={3} opacity={lerp(1, 0.18, shrink)} />
        ))}
        {PAIRS.map((pr, i) => (
          <DrawLine key={i} x1={RING[pr.a].x} y1={RING[pr.a].y} x2={RING[pr.b].x} y2={RING[pr.b].y}
            t={prog(f, 2 + i * 0.5, 0.7)} color="var(--blue)" width={4} />
        ))}
      </Svg>
      <At x={mx} y={my}>
        <div style={{ padding: `${lerp(26, 12, shrink)}px ${lerp(44, 22, shrink)}px`, borderRadius: lerp(18, 999, shrink),
          background: "var(--surface-2)", border: "1px solid var(--steel)", color: "var(--steel)",
          fontSize: lerp(36, 24, shrink), fontWeight: 600, whiteSpace: "nowrap" }}>
          {minimal ? <ICONS.Market size={lerp(64, 40, shrink)} /> : shrink < 0.5 ? "Market" : "Gemini prediction markets · data"}
        </div>
      </At>
      {PAIRS.map((pr, i) => {
        const A = RING[pr.a], B = RING[pr.b];
        const cx = (A.x + B.x) / 2, cy = (A.y + B.y) / 2;
        // pill sits just outside the edge, away from the ring centre
        const dx = cx - RING_C.x, dy = cy - RING_C.y, dl = Math.hypot(dx, dy) || 1;
        const px = cx + (dx / dl) * 220, py = cy + (dy / dl) * 60;
        const flow = prog(f, 6 + i * 0.2, 1.0);
        const locked = f > (6.9 + i * 0.2) * fps;
        return (
          <React.Fragment key={i}>
            <At x={px} y={py} scale={snap(f, fps, 2.5 + i * 0.5)}>
              {!minimal && <Pill tone="blue" size={21}>{pr.text}</Pill>}
            </At>
            {!locked && <>
              <At x={lerp(A.x, cx, flow)} y={lerp(A.y, cy, flow)} opacity={fade(f, 6 + i * 0.2, undefined, 0.15)}><MoneyChip amount="●" size={14} /></At>
              <At x={lerp(B.x, cx, flow)} y={lerp(B.y, cy, flow)} opacity={fade(f, 6 + i * 0.2, undefined, 0.15)}><MoneyChip amount="●" size={14} /></At>
            </>}
            <At x={cx} y={cy + 6} scale={snap(f, fps, 6.9 + i * 0.2, 10)}><Lock /></At>
          </React.Fragment>
        );
      })}
      {RING.map((p, i) => (
        <At key={i} x={p.x} y={p.y}>
          <AgentAvatar name={RING_NAMES[i]} color={p.color} size={84} />
        </At>
      ))}
      {/* 02's last caption, carried over the cut and faded */}
      <AbsoluteFill style={{ opacity: 1 - prog(f, 0, 0.4) }}>
        <Title at={-1} y={900} size={48}>Agents can trade with the market. Not with each other.</Title>
      </AbsoluteFill>
      <Title at={8} y={880} size={46} sub="inside limits you set" subAt={8.6}>
        Agents you own, making measurable challenges with each other.
      </Title>
    </Stage>
  );
};
