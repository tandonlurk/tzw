// 02-problem (21 s): three 7 s beats. Trust · speed gap · isolation.
import React from "react";
import { AbsoluteFill, Sequence, useCurrentFrame } from "remotion";
import { fade, lerp, prog, s, typed } from "../lib/anim";
import { RING, RING_C, RING_NAMES } from "../lib/ring";
import { AgentAvatar, At, DrawLine, Eyebrow, MoneyChip, Stage, Svg, Title, agentColor, MONO } from "../lib/ui";

const BEAT = 7;

const Trust: React.FC = () => {
  const f = useCurrentFrame();
  const meet = prog(f, 1.0, 1.2);
  const apart = prog(f, 4.6, 1.2);
  const ring = prog(f, 2.4, 0.8);
  const ringFail = prog(f, 4.3, 0.5);
  const out = 1 - prog(f, BEAT - 0.35, 0.35);
  const lx = lerp(lerp(470, 880, meet), 640, apart);
  const rx = lerp(lerp(1450, 1040, meet), 1280, apart);
  return (
    <AbsoluteFill style={{ opacity: out }}>
      <At x={360} y={470} opacity={fade(f, 0)} style={{ transform: `translate(-50%,-50%) translateX(${(1 - prog(f, 0, 0.6)) * -80}px)` }}>
        <AgentAvatar name="A" color={0} size={130} label="Agent A" sub="Owner A" />
      </At>
      <At x={1560} y={470} opacity={fade(f, 0.15)} style={{ transform: `translate(-50%,-50%) translateX(${(1 - prog(f, 0.15, 0.6)) * 80}px)` }}>
        <AgentAvatar name="B" color={1} size={130} label="Agent B" sub="Owner B" />
      </At>
      <At x={lx} y={450} opacity={fade(f, 0.9)}><MoneyChip amount="5.00" /></At>
      <At x={rx} y={450} opacity={fade(f, 1.0)}><MoneyChip amount="5.00" /></At>
      <Svg>
        <ellipse cx={960} cy={450} rx={260} ry={120}
          style={{ fill: "none", stroke: "var(--text-3)", strokeWidth: 3, strokeDasharray: "10 12",
            strokeDashoffset: (1 - ring) * 1200, opacity: ring * (1 - ringFail) }} />
      </Svg>
      <At x={960} y={250} opacity={fade(f, 2.6) * (1 - prog(f, 4.6, 0.4))}>
        <div style={{ display: "flex", flexDirection: "column", gap: 14, alignItems: "center", fontSize: 38, color: "var(--text)" }}>
          <div>{typed(f, "Who holds the money?", 2.6, 30)}</div>
          <div>{typed(f, "Who decides who won?", 3.4, 30)}</div>
        </div>
      </At>
      <Title at={4.9} size={84} y={700}>Is it fair?</Title>
    </AbsoluteFill>
  );
};

const SpeedGap: React.FC = () => {
  const f = useCurrentFrame();
  const out = 1 - prog(f, BEAT - 0.35, 0.35);
  const cols = 6, rows = 4;
  // Dense ticks for the fleet, one per 2 s for the person (no figures: we have no sourced number).
  const fleetTicks = Math.floor(Math.max(0, f - s(1.2)) / 2);
  const youTicks = Math.floor(Math.max(0, f - s(1.2)) / s(2)) + (f > s(1.2) ? 1 : 0);
  const strip = (n: number, max: number, color: string) => (
    <div style={{ width: 640, height: 44, borderRadius: 10, background: "var(--bg-2)", border: "1px solid var(--line)", position: "relative", overflow: "hidden" }}>
      {Array.from({ length: Math.min(n, max) }, (_, i) => (
        <div key={i} style={{ position: "absolute", left: 8 + i * (624 / max), top: 10, width: 3, height: 24, borderRadius: 2, background: color }} />
      ))}
    </div>
  );
  return (
    <AbsoluteFill style={{ opacity: out * fade(f, 0, undefined, 0.35) }}>
      <div style={{ position: "absolute", left: 960, top: 180, bottom: 300, width: 1, background: "var(--line-strong)" }} />
      <At x={480} y={200}><Eyebrow style={{ fontSize: 26 }}>Firms</Eyebrow></At>
      <At x={1440} y={200}><Eyebrow style={{ fontSize: 26 }}>You</Eyebrow></At>
      <At x={480} y={440}>
        <div style={{ display: "grid", gridTemplateColumns: `repeat(${cols}, 60px)`, gap: 26 }}>
          {Array.from({ length: cols * rows }, (_, i) => {
            const phase = (f + i * 7) % 20;
            const pulse = f > s(1.2) && phase < 5 ? 1 : 0;
            return <div key={i} style={{ width: 60, height: 60, borderRadius: 60, background: agentColor(i),
              opacity: 0.55 + 0.45 * pulse, boxShadow: pulse ? "0 0 0 8px var(--blue-a)" : undefined,
              transform: `scale(${prog(f, 0.1 + i * 0.02, 0.3)})` }} />;
          })}
        </div>
      </At>
      <At x={1440} y={440}>
        {(() => {
          const phase = Math.max(0, f - s(1.2)) % s(2);
          const pulse = f > s(1.2) && phase < 8 ? 1 : 0;
          return <div style={{ boxShadow: pulse ? "0 0 0 14px var(--blue-a)" : undefined, borderRadius: 999 }}>
            <AgentAvatar name="You" color={2} size={130} />
          </div>;
        })()}
      </At>
      <At x={480} y={680}>{strip(fleetTicks, 90, "var(--blue)")}</At>
      <At x={1440} y={680}>{strip(youTicks, 90, "var(--blue)")}</At>
      <Title at={3.4} y={810} size={50}>Firms run fleets of agents. Individuals can't keep up.</Title>
    </AbsoluteFill>
  );
};

export const Isolation: React.FC<{ hold?: boolean }> = ({ hold }) => {
  const f = useCurrentFrame();
  const tries: [number, number][] = [[0, 2], [1, 3], [4, 1], [3, 0]];
  return (
    <AbsoluteFill style={{ opacity: hold ? 1 : fade(f, 0, undefined, 0.35) }}>
      <Svg>
        {RING.map((p, i) => (
          <DrawLine key={i} x1={p.x} y1={p.y} x2={RING_C.x} y2={RING_C.y} t={prog(f, 0.6 + i * 0.12, 0.5)} color="var(--steel)" width={3} />
        ))}
        {tries.map(([a, b], i) => {
          const st = 2.0 + i * 0.45;
          const t = prog(f, st, 0.6) * 0.5;
          const gone = prog(f, st + 0.9, 0.4);
          return <DrawLine key={i} x1={RING[a].x} y1={RING[a].y} x2={RING[b].x} y2={RING[b].y} t={t} color="var(--text-3)" dash="8 10" opacity={1 - gone} />;
        })}
      </Svg>
      {tries.map(([a, b], i) => {
        const st = 2.0 + i * 0.45;
        const mx = (RING[a].x + RING[b].x) / 2, my = (RING[a].y + RING[b].y) / 2;
        return <At key={i} x={mx} y={my} opacity={fade(f, st + 0.6) * (1 - prog(f, st + 1.6, 0.5))}>
          <span style={{ fontFamily: MONO, fontSize: 34, color: "var(--text-3)" }}>×</span>
        </At>;
      })}
      <At x={RING_C.x} y={RING_C.y} scale={0.9 + 0.1 * prog(f, 0, 0.5)}>
        <div style={{ padding: "26px 44px", borderRadius: 18, background: "var(--surface-2)", border: "1px solid var(--steel)",
          color: "var(--steel)", fontSize: 36, fontWeight: 600 }}>Market</div>
      </At>
      {RING.map((p, i) => (
        <At key={i} x={p.x} y={p.y} opacity={fade(f, 0.1 + i * 0.08)}>
          <AgentAvatar name={RING_NAMES[i]} color={p.color} size={84} />
        </At>
      ))}
      <Title at={4.2} y={900} size={48}>Agents can trade with the market. Not with each other.</Title>
    </AbsoluteFill>
  );
};

export const S02Problem: React.FC = () => (
  <Stage fadeIn>
    <Sequence durationInFrames={s(BEAT)}><Trust /></Sequence>
    <Sequence from={s(BEAT)} durationInFrames={s(BEAT)}><SpeedGap /></Sequence>
    <Sequence from={s(BEAT * 2)}><Isolation /></Sequence>
  </Stage>
);
