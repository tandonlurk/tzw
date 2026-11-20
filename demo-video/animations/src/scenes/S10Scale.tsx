// 10-scale (15 s): split one wallet into strategy agents built from blocks, multiply one agent
// with variance (D-73: 1–100 per request, money limits copied unchanged), and the family rule.
import React from "react";
import { AbsoluteFill, useCurrentFrame, useVideoConfig } from "remotion";
import { fade, lerp, prog, snap } from "../lib/anim";
import { AgentAvatar, At, BlockGlyph, BlockKind, Card, Eyebrow, MONO, Stage, Svg, Title, agentColor, MoneyChip, useMinimal } from "../lib/ui";

const AGENTS = [
  { name: "Momentum", color: 0, amt: "30.00", w: 0.3, blocks: ["trigger", "condition", "action"] as BlockKind[] },
  { name: "Skeptic", color: 1, amt: "30.00", w: 0.3, blocks: ["trigger", "condition", "condition", "action"] as BlockKind[] },
  { name: "Cautious", color: 2, amt: "40.00", w: 0.4, blocks: ["approach", "trigger", "action"] as BlockKind[] },
];
const AX = [480, 960, 1440];

// Deterministic jitter per clone (stands in for the seeded variance).
const jit = (i: number, k: number) => {
  const x = Math.sin(i * 12.9898 + k * 78.233) * 43758.5453;
  return x - Math.floor(x) - 0.5;
};

export const S10Scale: React.FC = () => {
  const f = useCurrentFrame();
  const { fps } = useVideoConfig();
  const t = f / fps;
  const split = prog(f, 1.6, 0.8);
  const partB = prog(f, 5.2, 0.5);
  const partC = prog(f, 10.8, 0.5);
  const minimal = useMinimal();
  const n = t < 6.4 ? 1 : t < 7.6 ? 10 : 100;
  return (
    <Stage env="sim" tag="Example">
      {/* Part A: wallet splits into strategy agents */}
      <AbsoluteFill style={{ opacity: 1 - partB }}>
        <At x={960} y={lerp(470, 300, split)} opacity={fade(f, 0)}>
          <Card style={{ width: 1200, padding: 26 }}>
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline" }}>
              <Eyebrow>Your wallet</Eyebrow>
              <span style={{ fontFamily: MONO, fontSize: 34, color: "var(--text)", fontWeight: 600 }}>100.00 {!minimal && <span style={{ fontSize: 20, color: "var(--text-3)" }}>(simulated)</span>}</span>
            </div>
            <div style={{ display: "flex", gap: lerp(0, 8, split), marginTop: 16, height: 22 }}>
              {AGENTS.map((a) => <div key={a.name} style={{ flex: a.w, borderRadius: 6, background: lerp(0, 1, split) > 0.5 ? agentColor(a.color) : "var(--yellow)" }} />)}
            </div>
          </Card>
        </At>
        {AGENTS.map((a, i) => (
          <At key={a.name} x={AX[i]} y={lerp(470, 600, split)} opacity={prog(f, 1.9 + i * 0.1, 0.4)}>
            <Card style={{ width: 400, display: "flex", flexDirection: "column", alignItems: "center", gap: 14 }}>
              <AgentAvatar name={a.name} color={a.color} size={78} label={a.name} />
              <div style={{ fontFamily: MONO, fontSize: 26, color: "var(--yellow)" }}>{a.amt}</div>
              <div style={{ display: "flex", gap: 10, height: 56 }}>
                {a.blocks.map((b, j) => {
                  const p = snap(f, fps, 3.0 + i * 0.25 + j * 0.15, 11);
                  return <div key={j} style={{ transform: `translateY(${(1 - Math.min(1, p)) * -40}px) scale(${p})` }}><BlockGlyph kind={b} size={50} /></div>;
                })}
              </div>
              {!minimal && <div style={{ fontSize: 19, color: "var(--text-3)" }}>its own strategy, built from blocks</div>}
            </Card>
          </At>
        ))}
        <Title at={3.4} out={5.1} y={880} size={44}>Split one wallet into many agents. One strategy each.</Title>
      </AbsoluteFill>

      {/* Part B + C: multiply one agent, then the family rule */}
      <AbsoluteFill style={{ opacity: partB }}>
        <At x={300} y={460}>
          <div style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 18 }}>
            <AgentAvatar name="Momentum" color={0} size={110} label="Momentum" ring="blue" />
            <div style={{ fontFamily: MONO, fontSize: 84, fontWeight: 600, color: "var(--blue)" }}>{n}×</div>
          </div>
        </At>
        {/* Clone grid: 10 x 10, each a little off the original */}
        <div style={{ position: "absolute", left: 620, top: 230, width: 620, height: 520 }}>
          {Array.from({ length: 100 }, (_, i) => {
            const shown = i < n;
            const c = i % 10, r = Math.floor(i / 10);
            const appear = i === 0 ? 1 : i < 10 ? prog(f, 6.4 + i * 0.03, 0.3) : prog(f, 7.6 + (i % 37) * 0.012, 0.3);
            const x = c * 62 + jit(i, 1) * 10 * (i > 0 ? 1 : 0);
            const y = r * 52 + jit(i, 2) * 8 * (i > 0 ? 1 : 0);
            const size = 34 + jit(i, 3) * 6;
            return shown ? <div key={i} style={{ position: "absolute", left: x, top: y, width: size, height: size, borderRadius: size,
              background: "var(--agent-0)", opacity: appear * (0.7 + jit(i, 4) * 0.5), transform: `scale(${appear})`,
              filter: `hue-rotate(${jit(i, 5) * 30}deg)` }} /> : null;
          })}
          {/* family ring */}
          <svg width={720} height={620} style={{ position: "absolute", left: -50, top: -50, overflow: "visible" }}>
            <rect x={10} y={10} width={700} height={600} rx={40}
              style={{ fill: "none", stroke: "var(--blue)", strokeWidth: 2.5, strokeDasharray: "10 10", opacity: partC }} />
          </svg>
          <div style={{ position: "absolute", left: 0, top: -88, fontSize: 22, color: "var(--blue)", opacity: minimal ? 0 : partC }}>Family · one owner</div>
        </div>
        {/* Readouts */}
        {minimal && <At x={1560} y={420} opacity={fade(f, 8.2) * (1 - partC)}>
          <div style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 18 }}>
            <MoneyChip amount="30.00" size={30} />
            <span style={{ fontSize: 34, color: "var(--text-3)" }}>↓</span>
            <MoneyChip amount="0.30" suffix=" × 100" size={30} />
          </div>
        </At>}
        {!minimal && <At x={1600} y={380} opacity={fade(f, 8.2) * (1 - partC)}>
          <div style={{ width: 460, display: "flex", flexDirection: "column", gap: 22 }}>
            <div>
              <Eyebrow>Capital</Eyebrow>
              <div style={{ fontFamily: MONO, fontSize: 26, color: "var(--text)", marginTop: 6 }}>30.00 ÷ 100 = <span style={{ color: "var(--yellow)" }}>0.30 each</span></div>
            </div>
            <div>
              <Eyebrow>Varies, up to ±25 %</Eyebrow>
              <div style={{ fontSize: 22, color: "var(--text-2)", marginTop: 6 }}>horizon, move threshold, schedule, conviction, ambition</div>
            </div>
            <div>
              <Eyebrow>Copied unchanged</Eyebrow>
              <div style={{ fontSize: 22, color: "var(--text-2)", marginTop: 6 }}>every money limit</div>
            </div>
          </div>
        </At>}
        {/* Family rule: siblings never face each other; outside agents can */}
        <Svg>
          <line x1={700} y1={300} x2={1010} y2={600} style={{ stroke: "var(--text-3)", strokeWidth: 3, strokeDasharray: "8 10",
            opacity: prog(f, 11.2, 0.4) * (1 - prog(f, 12.6, 0.4)) }} />
          <line x1={1220} y1={420} x2={lerp(1220, 1600, prog(f, 12.4, 0.6))} y2={420} style={{ stroke: "var(--blue)", strokeWidth: 4, opacity: partC }} />
        </Svg>
        <At x={855} y={450} opacity={fade(f, 11.8) * (1 - prog(f, 13.2, 0.4))}>
          <span style={{ fontFamily: MONO, fontSize: 44, color: "var(--text-3)" }}>×</span>
        </At>
        <At x={1690} y={420} opacity={fade(f, 12.6)}>
          <AgentAvatar name="Other" color={4} size={84} label="Another owner's agent" />
        </At>
        <At x={1650} y={640} opacity={minimal ? 0 : fade(f, 11.4)}>
          <div style={{ fontSize: 22, color: "var(--text-2)", width: 420, textAlign: "center" }}>Clones never challenge each other, so money can't just move between your own agents.</div>
        </At>
        <Title at={8.4} out={10.8} y={880} size={44}>Multiply an agent 10× or 100×, each a little different.</Title>
        <Title at={13.2} y={880} size={44}>Same money, smaller stakes, more chances.</Title>
      </AbsoluteFill>
    </Stage>
  );
};
