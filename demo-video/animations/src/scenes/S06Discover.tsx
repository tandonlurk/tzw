// 06-discover (10 s): how an agent finds an idea. Market data in, the agent's own filters and
// logic blocks decide, a proposal with explicit terms comes out. Example numbers from docs/demo.md.
import React from "react";
import { AbsoluteFill, useCurrentFrame, useVideoConfig } from "remotion";
import { fade, lerp, prog, snap } from "../lib/anim";
import { AgentAvatar, At, BlockGlyph, BlockKind, Caption, Card, Eyebrow, LifecycleStrip, MONO, Pill, Stage, Svg, Title, MoneyChip, useMinimal } from "../lib/ui";

const TILES = [
  { t: "ETH  −0.4 %", hit: false },
  { t: "SOL  +0.8 %", hit: false },
  { t: "Rate-cut contract  0.38", hit: false },
  { t: "BTC  +3.0 % · contract 0.55", hit: true },
  { t: "DOGE  +1.1 %", hit: false },
  { t: "ETH  +0.9 %", hit: false },
  { t: "XRP  −1.6 %", hit: false },
];
const HIT = 3;

const BLOCKS: { kind: BlockKind; label: string }[] = [
  { kind: "trigger", label: "Market moved > 3 %" },
  { kind: "condition", label: "Conviction ≥ 60 %" },
  { kind: "action", label: "Propose a challenge" },
];

export const S06Discover: React.FC = () => {
  const f = useCurrentFrame();
  const { fps } = useVideoConfig();
  const minimal = useMinimal();
  // Tiles scroll upward through the filter window; the hit tile stops in the window.
  const scroll = prog(f, 0.8, 3.2, (t) => 1 - Math.pow(1 - t, 3));
  const rowH = 92;
  const offset = lerp(560, -HIT * rowH, scroll);
  const judged = prog(f, 3.6, 0.4);
  const fly = prog(f, 4.3, 0.7);
  return (
    <Stage env="sim" tag="Example">
      <LifecycleStrip active={0} />
      {/* Agent with its filters */}
      <At x={300} y={520} opacity={fade(f, 0.1)}>
        <Card style={{ width: 400, display: "flex", flexDirection: "column", alignItems: "center", gap: 20 }}>
          <AgentAvatar name="Demo Bull" color={0} size={104} label="Demo Bull" sub="your agent" />
          <Eyebrow>Watching</Eyebrow>
          <div style={{ display: "flex", flexWrap: "wrap", gap: 10, justifyContent: "center" }}>
            {!minimal && <>
            <Pill tone="blue" size={21}>Crypto</Pill>
            <Pill tone="blue" size={21}>Move &gt; 3 %</Pill>
            <Pill tone="blue" size={21}>Horizon 60 s</Pill>
            </>}
          </div>
        </Card>
      </At>
      {/* Market stream through a filter window */}
      <At x={850} y={300}><Eyebrow style={{ fontSize: 20 }}>Gemini prediction markets · data feed</Eyebrow></At>
      <div style={{ position: "absolute", left: 610, top: 340, width: 480, height: 380, overflow: "hidden",
        maskImage: "linear-gradient(to bottom, transparent 0, #000 22%, #000 78%, transparent 100%)" }}>
        {TILES.map((tile, i) => {
          const y = 150 + offset + i * rowH;
          const dim = judged * (tile.hit ? 0 : 0.75);
          return (
            <div key={i} style={{ position: "absolute", left: 10, right: 10, top: y, height: 72, borderRadius: 14,
              background: "var(--surface)", border: `1px solid ${tile.hit && judged > 0.5 ? "var(--blue)" : "var(--line-strong)"}`,
              display: "flex", alignItems: "center", padding: "0 22px", fontFamily: MONO, fontSize: 22, whiteSpace: "nowrap",
              color: tile.hit && judged > 0.5 ? "var(--text)" : "var(--text-2)", opacity: (1 - dim) * (tile.hit ? 1 - fly : 1) }}>
              {minimal ? tile.t.split(" · ")[0].replace("Rate-cut contract", "CUT") : tile.t}
            </div>
          );
        })}
      </div>
      <div style={{ position: "absolute", left: 606, top: 482, width: 488, height: 96, borderRadius: 18,
        border: "2px dashed var(--blue)", opacity: fade(f, 0.6) * 0.8 }} />
      {/* The hit tile flies to the logic chain */}
      <At x={lerp(850, 1450, fly)} y={lerp(530, 280, fly)} opacity={fly > 0 && fly < 1 ? 1 : fly >= 1 ? 1 : 0} scale={lerp(1, 0.9, fly)}>
        <div style={{ padding: "18px 24px", borderRadius: 14, background: "var(--blue-a)", border: "1px solid var(--blue)",
          fontFamily: MONO, fontSize: 24, color: "var(--text)", whiteSpace: "nowrap" }}>{minimal ? "BTC  +3.0 %" : "BTC +3.0 % · contract 0.55"}</div>
      </At>
      {/* Logic chain */}
      <Svg>
        {[0, 1].map((i) => (
          <line key={i} x1={1450} y1={360 + i * 100 + 34} x2={1450} y2={360 + (i + 1) * 100 - 34}
            style={{ stroke: "var(--line-strong)", strokeWidth: 3 }} />
        ))}
      </Svg>
      {BLOCKS.map((b, i) => {
        const lit = prog(f, 5.1 + i * 0.55, 0.3);
        return (
          <div key={i} style={{ position: "absolute", left: 1450 - 34, top: 360 + i * 100 - 34, display: "flex", alignItems: "center", gap: 22,
            opacity: fade(f, 0.4 + i * 0.1) }}>
            <BlockGlyph kind={b.kind} size={68} lit={lit} />
            {!minimal && <span style={{ fontSize: 26, color: lit > 0.5 ? "var(--text)" : "var(--text-3)", whiteSpace: "nowrap" }}>{b.label}</span>}
          </div>
        );
      })}
      {/* Proposal with explicit terms */}
      <At x={1520} y={700} scale={snap(f, fps, 7.0)}>
        {minimal ? (
          <Card elevated accent="blue" style={{ padding: "22px 30px", display: "flex", gap: 22, alignItems: "center" }}>
            <span style={{ fontFamily: MONO, fontSize: 40, color: "var(--text)", fontWeight: 600 }}>≥ 0.55</span>
            <MoneyChip amount="5.00" size={28} />
          </Card>
        ) : <Card elevated accent="blue" style={{ width: 560, padding: 24 }}>
          <Eyebrow>Proposal · explicit terms</Eyebrow>
          <div style={{ marginTop: 10, fontSize: 26, color: "var(--text)", fontWeight: 500 }}>BTC contract at or above 0.55 at end</div>
          <div style={{ marginTop: 10, display: "flex", gap: 18, fontSize: 21, color: "var(--text-2)", fontFamily: MONO }}>
            <span>side YES</span><span>stake 5.00</span><span>ends +60 s</span>
          </div>
        </Card>}
      </At>
      <AbsoluteFill style={{ opacity: 1 - prog(f, 6.6, 0.3) }}>
        <Title at={0.5} y={880} size={44}>Your agent watches the markets through its own filters.</Title>
      </AbsoluteFill>
      <Title at={7.6} y={880} size={44}>Data finds the idea. Its rules decide whether to propose.</Title>
      <Caption text="Gemini prices are market data, not an oracle." at={1} bottom={30} />
    </Stage>
  );
};
