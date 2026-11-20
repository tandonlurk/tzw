// 05-lifecycle-map (8 s): the six stations every challenge passes through, drawn once,
// then collapsed into the strip that sits on top of every later explainer.
import React from "react";
import { AbsoluteFill, useCurrentFrame, useVideoConfig } from "remotion";
import { fade, lerp, prog, snap } from "../lib/anim";
import { ICONS } from "../lib/icons";
import { LifecycleStrip, MoneyChip, STATIONS, Stage, Svg, Title } from "../lib/ui";

const X0 = 210, DX = 300, Y = 470;

export const S05LifecycleMap: React.FC = () => {
  const f = useCurrentFrame();
  const { fps } = useVideoConfig();
  const collapse = prog(f, 6.6, 0.8);
  // A stake token walks the path once all stations are drawn.
  const walk = prog(f, 3.6, 2.4, (t) => t);
  const tokenX = lerp(X0, X0 + DX * 5, walk);
  const at = Math.min(5, Math.round(walk * 5));
  return (
    <Stage>
      <AbsoluteFill style={{ opacity: 1 - collapse, transform: `translateY(${-collapse * 260}px) scale(${1 - collapse * 0.45})` }}>
        <Svg>
          <line x1={X0} y1={Y} x2={X0 + DX * 5} y2={Y}
            style={{ stroke: "var(--line-strong)", strokeWidth: 3, strokeDasharray: 1600, strokeDashoffset: (1 - prog(f, 0.3, 2.8)) * 1600 }} />
          <line x1={X0} y1={Y} x2={tokenX} y2={Y} style={{ stroke: "var(--blue)", strokeWidth: 3, opacity: walk > 0 ? 1 : 0 }} />
        </Svg>
        {STATIONS.map((st, i) => {
          const Icon = ICONS[st.name];
          const p = snap(f, fps, 0.3 + i * 0.45);
          const lit = walk > 0 && at >= i;
          return (
            <div key={st.name} style={{ position: "absolute", left: X0 + DX * i, top: Y, transform: `translate(-50%, -50%) scale(${p})` }}>
              <div style={{ width: 132, height: 132, borderRadius: 32, display: "grid", placeItems: "center",
                background: lit ? "linear-gradient(var(--blue-a), var(--blue-a)), var(--surface)" : "var(--surface)", border: `2px solid ${lit ? "var(--blue)" : "var(--line-strong)"}`,
                color: lit ? "var(--blue)" : "var(--text-2)" }}>
                <Icon size={60} />
              </div>
              <div style={{ position: "absolute", top: 156, left: "50%", transform: "translateX(-50%)", textAlign: "center", whiteSpace: "nowrap" }}>
                <div style={{ fontSize: 32, fontWeight: 500, color: "var(--text)" }}>{st.name}</div>
                <div style={{ fontSize: 23, color: "var(--text-3)", marginTop: 4, opacity: fade(f, 0.6 + i * 0.45) }}>{st.sub}</div>
              </div>
              <div style={{ position: "absolute", top: -112, left: "50%", transform: "translateX(-50%)", fontSize: 22,
                color: "var(--text-3)", fontFamily: "inherit" }}>{i + 1}</div>
            </div>
          );
        })}
        <div style={{ position: "absolute", left: tokenX, top: Y - 128, transform: "translate(-50%, -50%)", opacity: fade(f, 3.5) * (1 - prog(f, 6.1, 0.3)) }}>
          <MoneyChip amount="stake" size={22} />
        </div>
        <Title at={1.4} y={150} size={50}>Every challenge takes the same six steps.</Title>
      </AbsoluteFill>
      <AbsoluteFill style={{ opacity: prog(f, 7.0, 0.5) }}>
        <LifecycleStrip active={[]} />
      </AbsoluteFill>
    </Stage>
  );
};
