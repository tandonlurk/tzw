// 08-escrow (20 s): replay of the live Gemini run on Arc testnet, 18 Nov 2026.
// Every value is from docs/submission/arc.md ("Vaults: bull 0xC287…8F11 …"). Never mix runs here.
import React from "react";
import { AbsoluteFill, useCurrentFrame, useVideoConfig } from "remotion";
import { fade, lerp, prog, snap, tick } from "../lib/anim";
import { ICONS } from "../lib/icons";
import { AgentAvatar, At, Caption, Eyebrow, LifecycleStrip, MONO, MoneyChip, Pill, Stage, Svg, TxChip } from "../lib/ui";

const VL = { x: 330, y: 430 }, VR = { x: 1590, y: 430 }, ESC = { x: 960, y: 735 };

const Vault: React.FC<{ name: string; color: number; addr: string; bal: string; delta?: { text: string; tone: "mint" | "rose"; o: number }; drift: number; tx: number }> =
  ({ name, color, addr, bal, delta, drift, tx }) => (
    <div style={{ width: 430, borderRadius: 18, background: "var(--surface)", border: "1px solid var(--line-strong)", padding: 26 }}>
      <div style={{ display: "flex", alignItems: "center", gap: 16 }}>
        <AgentAvatar name={name} color={color} size={60} />
        <div>
          <div style={{ fontSize: 26, color: "var(--text)", fontWeight: 500 }}>{name} vault</div>
          <div style={{ fontFamily: MONO, fontSize: 19, color: "var(--text-3)" }}>{addr}</div>
        </div>
      </div>
      <div style={{ display: "flex", alignItems: "baseline", gap: 14, marginTop: 18 }}>
        <span style={{ fontFamily: MONO, fontSize: 46, fontWeight: 600, color: "var(--yellow)" }}>{bal}</span>
        <span style={{ fontSize: 20, color: "var(--text-3)" }}>USDC</span>
        {delta && <span style={{ fontFamily: MONO, fontSize: 26, fontWeight: 600, color: `var(--${delta.tone})`, opacity: delta.o }}>{delta.text}</span>}
      </div>
      <div style={{ display: "flex", gap: 10, marginTop: 14, flexWrap: "wrap" }}>
        <Pill size={17}>🔑 Owner's wallet</Pill>
        <Pill size={17}>Operator: stake only</Pill>
      </div>
      <div style={{ marginTop: 14, height: 38, opacity: tx }}><TxChip label="stake → escrow" /></div>
      <div style={{ marginTop: 10, fontSize: 19, color: "var(--mint)", fontFamily: MONO, opacity: drift }}>onchain = ledger · drift 0</div>
    </div>
  );

export const S08Escrow: React.FC = () => {
  const f = useCurrentFrame();
  const { fps } = useVideoConfig();
  const t = f / fps;
  const station = t < 6.5 ? 2 : t < 12.5 ? 3 : t < 18.4 ? 4 : 5;
  const stakeIn = prog(f, 6.9, 1.1);
  const payout = prog(f, 16.0, 1.2);
  const escIn = prog(f, 6.4, 0.6);
  const locked = t >= 8.0 && t < 16.6;
  const bullBal = t < 8 ? tick(f, 2.5, 2.5, 0) : tick(f, 2.5, 1.25, 7.8, 0.5);
  const skBal = t < 16.6 ? (t < 8 ? "2.50" : tick(f, 2.5, 1.25, 7.8, 0.5)) : tick(f, 1.25, 3.75, 16.6, 0.8);
  const clock = prog(f, 12.6, 1.2, (x) => x);
  const LockIcon = ICONS.Lock;
  return (
    <Stage env="arc" tag="Replay · Arc testnet, 18 Nov 2026">
      <LifecycleStrip active={station} />
      <At x={VL.x} y={VL.y} opacity={fade(f, 0)}>
        <Vault name="Demo Bull" color={0} addr="0xC287…8F11" bal={bullBal}
          delta={{ text: "−1.25", tone: "rose", o: prog(f, 17.2, 0.4) }} drift={prog(f, 18.5, 0.4)} tx={prog(f, 8.0, 0.3)} />
      </At>
      <At x={VR.x} y={VR.y} opacity={fade(f, 0.15)}>
        <Vault name="Demo Skeptic" color={1} addr="0xD6C9…9Ee4" bal={skBal}
          delta={{ text: "+1.25", tone: "mint", o: prog(f, 17.2, 0.4) }} drift={prog(f, 18.5, 0.4)} tx={prog(f, 8.1, 0.3)} />
      </At>

      {/* Term sheet */}
      <At x={960} y={lerp(240, 330, prog(f, 1.8, 0.6))} opacity={fade(f, 1.8)}>
        <div style={{ width: 600, borderRadius: 18, background: "var(--surface-2)", border: "1px solid var(--blue)", padding: 24 }}>
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
            <Eyebrow>Term sheet</Eyebrow>
            <span style={{ fontSize: 17, color: "var(--blue)", opacity: fade(f, 3.2) }}>negotiated 2.50 → 1.25</span>
          </div>
          <div style={{ fontFamily: MONO, fontSize: 19, color: "var(--text-3)", marginTop: 8 }}>GEMI-BTC15M2611180000-UP</div>
          <div style={{ fontSize: 25, color: "var(--text)", marginTop: 6, fontWeight: 500 }}>YES price at or above 0.14 at 00:01:54 UTC</div>
          <div style={{ display: "flex", gap: 22, marginTop: 10, fontSize: 19, color: "var(--text-2)" }}>
            <span style={{ fontFamily: MONO, color: "var(--yellow)" }}>1.25 per side</span>
            <span>Resolution: Gemini published outcome</span>
          </div>
          <div style={{ marginTop: 12, height: 6, borderRadius: 6, background: "var(--bg-2)", overflow: "hidden", opacity: fade(f, 12.4) }}>
            <div style={{ width: `${clock * 100}%`, height: "100%", background: clock >= 1 ? "var(--text-3)" : "var(--blue)" }} />
          </div>
        </div>
      </At>

      {/* Approval gate on the skeptic's side */}
      <At x={VR.x} y={VR.y + 290} scale={snap(f, fps, 4.5)} opacity={1 - prog(f, 6.6, 0.3)}>
        <div style={{ padding: "14px 22px", borderRadius: 14, background: "var(--yellow-a)", border: "1px solid var(--yellow)",
          color: "var(--yellow)", fontSize: 21, textAlign: "center", width: 430 }}>
          Above its automatic limit (1.00):<br />owner approval
          <div style={{ marginTop: 8, display: "inline-block", transform: `scale(${snap(f, fps, 5.6, 9)}) rotate(-4deg)`, border: "2px solid var(--mint)",
            color: "var(--mint)", borderRadius: 8, padding: "2px 12px", fontWeight: 700, letterSpacing: ".08em" }}>APPROVED</div>
        </div>
      </At>

      {/* Escrow */}
      <At x={ESC.x} y={lerp(ESC.y + 60, ESC.y, escIn)} opacity={escIn}>
        <div style={{ width: 560, borderRadius: 18, background: "var(--surface)", padding: 24, textAlign: "center",
          border: `2px solid ${locked ? "var(--yellow)" : payout > 0 ? "var(--mint)" : "var(--line-strong)"}` }}>
          <div style={{ display: "flex", justifyContent: "center", alignItems: "center", gap: 12, color: locked ? "var(--yellow)" : "var(--text-2)" }}>
            <LockIcon size={34} />
            <span style={{ fontSize: 26, color: "var(--text)", fontWeight: 500 }}>ChallengeEscrow</span>
            <span style={{ fontFamily: MONO, fontSize: 19, color: "var(--text-3)" }}>0x4179…5698</span>
          </div>
          <div style={{ fontFamily: MONO, fontSize: 34, fontWeight: 600, marginTop: 10,
            color: payout > 0 ? "var(--mint)" : "var(--yellow)" }}>
            {t < 8 ? "Funding open" : payout > 0.5 ? "Paid out 2.50" : "Locked 2.50"}
          </div>
          <div style={{ fontSize: 20, color: "var(--text-2)", marginTop: 10, opacity: fade(f, 10, 12.5) }}>Pays one of these two, or refunds both.</div>
          <div style={{ fontSize: 20, color: "var(--text-3)", marginTop: 4, opacity: fade(f, 10.8, 12.5) }}>If no one resolves: anyone can refund after the deadline + 1 h.</div>
        </div>
      </At>
      {/* Resolution: source, formula, who applied it */}
      <At x={ESC.x} y={ESC.y + 150} opacity={fade(f, 13.4)}>
        <div style={{ display: "flex", gap: 18, alignItems: "center" }}>
          <Pill tone="steel" size={21}>Gemini: event settled NO</Pill>
          <span style={{ fontFamily: MONO, fontSize: 22, color: "var(--text-2)", opacity: fade(f, 14.1) }}>YES ≥ 0.14? → No → Skeptic</span>
        </div>
      </At>
      <At x={ESC.x} y={ESC.y - 175} opacity={prog(f, 14.8, 0.2)}>
        <div style={{ transform: `scale(${snap(f, fps, 14.8, 9)}) rotate(-5deg)`, border: "2px solid var(--steel)", color: "var(--steel)",
          borderRadius: 8, padding: "4px 14px", fontSize: 20, fontWeight: 700, letterSpacing: ".06em", background: "var(--bg)" }}>RESOLVED BY TZW BACKEND</div>
      </At>

      {/* Money in motion */}
      <Svg>
        {stakeIn > 0 && stakeIn < 1 && <>
          <line x1={VL.x + 120} y1={VL.y + 170} x2={ESC.x - 200} y2={ESC.y - 40} style={{ stroke: "var(--blue)", strokeWidth: 2, strokeDasharray: "4 8", opacity: 0.6 }} />
          <line x1={VR.x - 120} y1={VR.y + 170} x2={ESC.x + 200} y2={ESC.y - 40} style={{ stroke: "var(--blue)", strokeWidth: 2, strokeDasharray: "4 8", opacity: 0.6 }} />
        </>}
      </Svg>
      {stakeIn > 0 && stakeIn < 1 && <>
        <At x={lerp(VL.x + 120, ESC.x - 200, stakeIn)} y={lerp(VL.y + 170, ESC.y - 40, stakeIn)}><MoneyChip amount="1.25" tone="blue" size={22} /></At>
        <At x={lerp(VR.x - 120, ESC.x + 200, stakeIn)} y={lerp(VR.y + 170, ESC.y - 40, stakeIn)}><MoneyChip amount="1.25" tone="blue" size={22} /></At>
      </>}
      {payout > 0 && payout < 1 && (
        <At x={lerp(ESC.x + 200, VR.x - 60, payout)} y={lerp(ESC.y - 40, VR.y + 60, payout)}><MoneyChip amount="2.50" tone="mint" size={24} /></At>
      )}
      <AbsoluteFill style={{ opacity: fade(f, 14.8) }}>
        <Caption text="Resolution is backend-administered, not trustless." bottom={30} />
      </AbsoluteFill>
    </Stage>
  );
};
