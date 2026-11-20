// 07-negotiate-guard (14 s): two agents bargain to explicit terms, then the agreed stake passes
// four independent checks. The model only ever suggests. Example numbers: docs/demo.md (simulated run).
import React from "react";
import { AbsoluteFill, useCurrentFrame, useVideoConfig } from "remotion";
import { fade, lerp, prog, snap } from "../lib/anim";
import { AgentAvatar, At, Card, Eyebrow, LifecycleStrip, MONO, MoneyChip, Stage, Svg, Title, Tone, toneColor, useMinimal } from "../lib/ui";
import { ICONS } from "../lib/icons";

const L = 300, R = 1620, Y = 470;
const GATE_ICONS = ["Model", "Server", "Vault", "Person"];

const Offer: React.FC<{ label: string; amount: string; tone: Tone }> = ({ label, amount, tone }) => (
  <div style={{ padding: "16px 24px", borderRadius: 16, background: "var(--surface-2)", border: `1px solid ${toneColor(tone)}`,
    display: "flex", flexDirection: "column", gap: 6, alignItems: "center", minWidth: 230 }}>
    <Eyebrow>{label}</Eyebrow>
    <div style={{ fontFamily: MONO, fontSize: 34, fontWeight: 600, color: toneColor(tone) }}>{amount}</div>
  </div>
);

const GATES: { title: string; lines: string[]; tone: Tone }[] = [
  { title: "AI model", lines: ["suggests 2.50", "suggestion only"], tone: "steel" },
  { title: "TZW backend", lines: ["≤ max per challenge 10.00 ✓", "total exposure ✓", "available balance ✓"], tone: "blue" },
  { title: "Agent vault · onchain", lines: ["per-challenge cap ✓", "total exposure cap ✓", "pays only into the escrow ✓"], tone: "blue" },
  { title: "You, the owner", lines: ["above the 1.00 auto limit", "waits for your approval"], tone: "yellow" },
];

export const S07NegotiateGuard: React.FC = () => {
  const f = useCurrentFrame();
  const { fps } = useVideoConfig();
  const minimal = useMinimal();
  const partB = prog(f, 6.4, 0.5);
  const r1 = prog(f, 0.6, 1.0), r2 = prog(f, 2.1, 1.0), r3 = prog(f, 3.6, 0.6);
  const merge = prog(f, 4.6, 0.6);
  const round = f < 2.1 * fps ? 1 : f < 3.6 * fps ? 2 : 3;
  // Part B: the stake walks through the gates.
  const GX = [330, 750, 1170, 1590], GY = 640;
  const walkT = (t: number) => {
    // 0..4 piecewise: arrive at gate i at 7.4 + i*1.1 s
    const arrive = GX.map((_, i) => 7.4 + i * 1.1);
    if (t < arrive[0]) return lerp(120, GX[0], prog(f, 6.9, 0.5));
    for (let i = 1; i < GX.length; i++) if (t < arrive[i]) return lerp(GX[i - 1], GX[i], prog(f, arrive[i] - 0.5, 0.5));
    return lerp(GX[3], 1830, prog(f, 12.0, 0.5));
  };
  const t = f / fps;
  const chipX = walkT(t);
  const approved = prog(f, 11.3, 0.3);
  return (
    <Stage env="sim" tag="Example">
      <LifecycleStrip active={partB < 0.5 ? 1 : 2} />
      {/* Part A: negotiation */}
      <AbsoluteFill style={{ opacity: 1 - partB }}>
        <At x={L} y={Y} opacity={fade(f, 0)}><AgentAvatar name="Demo Bull" color={0} size={120} label="Demo Bull" sub="Owner A's agent" /></At>
        <At x={R} y={Y} opacity={fade(f, 0.1)}><AgentAvatar name="Demo Skeptic" color={1} size={120} label="Demo Skeptic" sub="Owner B's agent" /></At>
        <Svg><line x1={L + 110} y1={Y - 30} x2={R - 110} y2={Y - 30} style={{ stroke: "var(--line-strong)", strokeWidth: 2, strokeDasharray: "6 10" }} /></Svg>
        <At x={lerp(L + 220, 1180, r1)} y={Y - 120} opacity={fade(f, 0.6) * (1 - merge)}><Offer label="Round 1 · offer" amount="5.00" tone="blue" /></At>
        <At x={lerp(R - 220, 740, r2)} y={Y + 70} opacity={fade(f, 2.1) * (1 - merge)}><Offer label="Round 2 · counter" amount="2.50" tone="yellow" /></At>
        <At x={960} y={Y - 25} scale={snap(f, fps, 3.6)} opacity={1 - merge}>
          <div style={{ fontSize: 28, color: "var(--mint)", fontWeight: 600, opacity: r3 }}>{minimal ? <ICONS.Check size={56} /> : "✓ Round 3 · Bull accepts"}</div>
        </At>
        <At x={960} y={250}>{minimal
          ? <div style={{ display: "flex", gap: 14 }}>{[1, 2, 3].map((k) => <div key={k} style={{ width: 16, height: 16, borderRadius: 16, background: k <= round ? "var(--blue)" : "transparent", border: "2px solid var(--blue)" }} />)}</div>
          : <div style={{ fontSize: 24, color: "var(--text-3)" }}>round <span style={{ color: "var(--text)", fontFamily: MONO }}>{round}</span> of 3 max</div>}</At>
        <At x={960} y={Y + 40} scale={snap(f, fps, 4.9)}>
          {minimal ? (
            <Card elevated accent="blue" style={{ padding: "24px 34px", display: "flex", gap: 26, alignItems: "center" }}>
              <span style={{ fontFamily: MONO, fontSize: 44, color: "var(--text)", fontWeight: 600 }}>≥ 0.55</span>
              <MoneyChip amount="2.50" size={30} />
            </Card>
          ) : <Card elevated accent="blue" style={{ width: 760, padding: 30 }}>
            <Eyebrow>Agreed terms</Eyebrow>
            <div style={{ marginTop: 10, fontSize: 32, fontWeight: 500, color: "var(--text)" }}>BTC contract at or above 0.55 at end</div>
            <div style={{ marginTop: 14, display: "grid", gridTemplateColumns: "auto 1fr", gap: "8px 24px", fontSize: 22 }}>
              <span style={{ color: "var(--text-3)" }}>Stake</span><span style={{ fontFamily: MONO, color: "var(--yellow)" }}>2.50 per side</span>
              <span style={{ color: "var(--text-3)" }}>Source</span><span style={{ color: "var(--text-2)" }}>contract YES price at end time</span>
              <span style={{ color: "var(--text-3)" }}>Formula</span><span style={{ fontFamily: MONO, color: "var(--text-2)" }}>price ≥ 0.55 → Bull, else Skeptic</span>
            </div>
          </Card>}
        </At>
        <Title at={5.2} out={6.3} y={850} size={44}>The agents negotiate. The terms are written down before any money moves.</Title>
      </AbsoluteFill>
      {/* Part B: four independent checks */}
      <AbsoluteFill style={{ opacity: partB }}>
        {!minimal && <At x={960} y={290}><div style={{ fontSize: 40, color: "var(--text)", fontWeight: 500 }}>Before a stake moves, four checks</div></At>}
        <Svg><line x1={150} y1={GY - 150} x2={1800} y2={GY - 150} style={{ stroke: "var(--line-strong)", strokeWidth: 2 }} /></Svg>
        {GATES.map((g, i) => {
          const passed = t >= 7.4 + i * 1.1;
          const isOwner = i === 3;
          const col = isOwner ? (approved > 0.5 ? "var(--mint)" : passed ? "var(--yellow)" : "var(--line-strong)") : passed ? toneColor(g.tone) : "var(--line-strong)";
          return (
            <At key={i} x={GX[i]} y={GY + 30} opacity={fade(f, 6.6 + i * 0.12)}>
              {minimal ? (
                <div style={{ width: 220, height: 220, borderRadius: 28, background: "var(--surface)", border: `2px solid ${col}`, display: "grid", placeItems: "center", position: "relative",
                  color: passed ? (isOwner && approved <= 0.5 ? "var(--yellow)" : toneColor(isOwner ? "mint" : g.tone)) : "var(--text-3)" }}>
                  {React.createElement(ICONS[GATE_ICONS[i]], { size: 96 })}
                  <div style={{ position: "absolute", top: -22, right: -22, width: 52, height: 52, borderRadius: 52, display: "grid", placeItems: "center",
                    background: "var(--bg)", border: "2px solid var(--mint)", color: "var(--mint)",
                    transform: `scale(${isOwner ? snap(f, fps, 11.3, 9) : snap(f, fps, 7.4 + i * 1.1, 10)})` }}><ICONS.Check size={30} /></div>
                </div>
              ) : <div style={{ width: 360, minHeight: 236, borderRadius: 18, background: "var(--surface)", border: `2px solid ${col}`, padding: 24 }}>
                <div style={{ fontSize: 17, color: "var(--text-3)", letterSpacing: ".1em", textTransform: "uppercase" }}>Check {i + 1}</div>
                <div style={{ fontSize: 28, color: "var(--text)", fontWeight: 500, marginTop: 6 }}>{g.title}</div>
                {g.lines.map((ln) => <div key={ln} style={{ fontSize: 21, color: passed ? "var(--text-2)" : "var(--text-3)", marginTop: 8 }}>{ln}</div>)}
                {isOwner && <div style={{ marginTop: 12, display: "inline-block", transform: `scale(${snap(f, fps, 11.3, 9)}) rotate(-4deg)`,
                  border: "2px solid var(--mint)", color: "var(--mint)", borderRadius: 8, padding: "4px 12px", fontSize: 22, fontWeight: 700, letterSpacing: ".08em" }}>APPROVED</div>}
              </div>}
            </At>
          );
        })}
        <At x={chipX} y={GY - 150} opacity={fade(f, 6.9)}>
          <MoneyChip amount="2.50" suffix={minimal ? undefined : " stake"} tone={t > 12 ? "blue" : "yellow"} />
        </At>
        <Title at={12.1} y={880} size={46}>The model can suggest a stake. It cannot authorize one.</Title>
      </AbsoluteFill>
    </Stage>
  );
};
