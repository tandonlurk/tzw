// 14-close (6 s): wordmark, three-line promise, funds footer.
import React from "react";
import { AbsoluteFill, useCurrentFrame } from "remotion";
import { fade, prog } from "../lib/anim";
import { Stage, Wordmark, useMinimal } from "../lib/ui";

const LINES = ["Agents you own.", "Limits you set.", "Every stake on the record."];

export const S14Close: React.FC = () => {
  const f = useCurrentFrame();
  const minimal = useMinimal();
  return (
    <Stage fadeIn>
      <AbsoluteFill style={{ justifyContent: "center", alignItems: "center", flexDirection: "column", gap: 56 }}>
        <Wordmark at={0} size={minimal ? 140 : 96} />
        {!minimal && <><div style={{ display: "flex", gap: 48 }}>
          {LINES.map((l, i) => (
            <div key={l} style={{ fontSize: 46, fontWeight: 500, color: "var(--text)", letterSpacing: "-0.02em",
              opacity: fade(f, 1 + i * 0.8), transform: `translateY(${(1 - prog(f, 1 + i * 0.8, 0.5)) * 14}px)` }}>{l}</div>
          ))}
        </div>
        <div style={{ fontSize: 26, color: "var(--text-3)", opacity: fade(f, 3.8) }}>
          Escrow on Arc testnet · market data from Gemini prediction markets · confirmed by QuickNode Streams
        </div></>}
      </AbsoluteFill>
      <div style={{ position: "absolute", bottom: 56, left: 0, right: 0, display: "flex", justifyContent: "center",
        alignItems: "center", gap: 14, fontSize: minimal ? 18 : 24, color: "var(--text-3)", opacity: fade(f, minimal ? 1.5 : 4.2) }}>
        <span style={{ width: 10, height: 10, borderRadius: 10, background: "var(--yellow)" }} />
        {minimal ? "Simulated and testnet funds only" : "Simulated and Arc testnet funds only. Nothing here is real money."}
      </div>
    </Stage>
  );
};
