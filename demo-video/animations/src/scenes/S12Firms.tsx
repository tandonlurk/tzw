// 12-firms (18 s): replay of the simulated firm demo (backend/README.md "Firm demo").
// Lead keeps 80 %, 20 % pool split 60/40 by weight, 0.10 task fee per used contribution (D-65).
import React from "react";
import { AbsoluteFill, useCurrentFrame, useVideoConfig } from "remotion";
import { fade, lerp, prog, snap, tick } from "../lib/anim";
import { ICONS } from "../lib/icons";
import { AgentAvatar, At, Caption, Card, Eyebrow, MONO, MoneyChip, Pill, Stage, Svg, Title } from "../lib/ui";

const LEAD = { x: 900, y: 470 }, SCOUT = { x: 400, y: 330 }, ANALYST = { x: 400, y: 640 }, RIVAL = { x: 1530, y: 470 };

const Delta: React.FC<{ v: string; tone: "mint" | "rose" | "yellow"; o: number }> = ({ v, tone, o }) => (
  <div style={{ fontFamily: MONO, fontSize: 30, fontWeight: 600, color: `var(--${tone})`, opacity: o, textAlign: "center" }}>{v}</div>
);

export const S12Firms: React.FC = () => {
  const f = useCurrentFrame();
  const { fps } = useVideoConfig();
  const t = f / fps;
  const join = prog(f, 2.6, 0.9);
  const contrib = prog(f, 3.8, 0.8);
  const fee = prog(f, 4.9, 0.7);
  const duel = prog(f, 6.6, 0.7);
  const split = prog(f, 9.6, 0.6);
  const pour = prog(f, 10.6, 0.9);
  const loss = prog(f, 15.2, 0.5);
  const LockIcon = ICONS.Lock;
  const scoutEarned = t < 5.6 ? null : t < 11.5 ? "+0.10" : "+" + tick(f, 0.1, 0.7, 11.5, 0.5);
  const analystEarned = t < 5.6 ? null : t < 11.5 ? "+0.10" : "+" + tick(f, 0.1, 0.5, 11.5, 0.5);
  const leadVal = t < 8.4 ? null : t < 12.4 ? "+5.00" : "+" + tick(f, 5.0, 3.8, 12.4, 0.6);
  return (
    <Stage env="sim" tag="Replay · simulated firm demo">
      <AbsoluteFill style={{ opacity: 1 - loss * 0.75 }}>
        {/* Firm card */}
        <At x={LEAD.x} y={190} opacity={fade(f, 0.6)}>
          <Card style={{ padding: "18px 28px", display: "flex", gap: 28, alignItems: "center" }}>
            <span style={{ fontSize: 28, color: "var(--text)", fontWeight: 500 }}>Firm demo desk</span>
            <span style={{ fontSize: 22, color: "var(--text-2)" }}>lead keeps <b style={{ color: "var(--text)" }}>80 %</b></span>
            <span style={{ fontSize: 22, color: "var(--text-2)" }}>members share <b style={{ color: "var(--text)" }}>20 %</b></span>
            <span style={{ fontSize: 22, color: "var(--text-2)" }}>task fee <b style={{ fontFamily: MONO, color: "var(--yellow)" }}>0.10</b></span>
          </Card>
        </At>
        {/* Membership lines */}
        <Svg>
          {[SCOUT, ANALYST].map((m, i) => (
            <line key={i} x1={m.x} y1={m.y} x2={lerp(m.x, LEAD.x, join)} y2={lerp(m.y, LEAD.y, join)}
              style={{ stroke: "var(--line-strong)", strokeWidth: 3 }} />
          ))}
          <line x1={LEAD.x} y1={LEAD.y} x2={lerp(LEAD.x, RIVAL.x, duel)} y2={LEAD.y} style={{ stroke: "var(--blue)", strokeWidth: 4 }} />
        </Svg>
        <At x={LEAD.x} y={LEAD.y} scale={snap(f, fps, 0.1)}>
          <AgentAvatar name="Firm Lead" color={0} size={150} label="Firm Lead" sub="runs the firm" ring="blue" />
        </At>
        <At x={LEAD.x} y={LEAD.y + 190}><Delta v={leadVal ?? ""} tone="mint" o={leadVal ? 1 : 0} /></At>
        {[{ m: SCOUT, name: "Firm Scout", role: "scout · finds markets", color: 2, earned: scoutEarned, d: 0 },
          { m: ANALYST, name: "Firm Analyst", role: "analyst · reads signals", color: 3, earned: analystEarned, d: 0.15 }].map((x) => (
          <React.Fragment key={x.name}>
            <At x={x.m.x} y={x.m.y} opacity={fade(f, 2.0 + x.d)} scale={lerp(0.6, 1, prog(f, 2.0 + x.d, 0.6))}>
              <AgentAvatar name={x.name} color={x.color} size={100} label={x.name} sub={x.role} ring="steel" />
            </At>
            <At x={x.m.x + 170} y={x.m.y - 20}><Delta v={x.earned ?? ""} tone="mint" o={x.earned ? 1 : 0} /></At>
          </React.Fragment>
        ))}
        {/* Contributions in, task fees out */}
        {contrib > 0 && contrib < 1 && <>
          <At x={lerp(SCOUT.x, LEAD.x, contrib)} y={lerp(SCOUT.y, LEAD.y, contrib)}><Pill tone="blue" size={19}>market candidate</Pill></At>
          <At x={lerp(ANALYST.x, LEAD.x, contrib)} y={lerp(ANALYST.y, LEAD.y, contrib)}><Pill tone="blue" size={19}>signal</Pill></At>
        </>}
        {fee > 0 && fee < 1 && <>
          <At x={lerp(LEAD.x, SCOUT.x, fee)} y={lerp(LEAD.y, SCOUT.y, fee)}><MoneyChip amount="0.10" size={20} /></At>
          <At x={lerp(LEAD.x, ANALYST.x, fee)} y={lerp(LEAD.y, ANALYST.y, fee)}><MoneyChip amount="0.10" size={20} /></At>
        </>}
        <At x={(LEAD.x + 220 + 260) / 2 + 100} y={LEAD.y + 220} opacity={fade(f, 4.9, 6.4)}>
          <div style={{ fontSize: 21, color: "var(--text-3)", width: 380, textAlign: "center" }}>fee paid as each contribution is used, before any outcome</div>
        </At>
        {/* The lead's challenge */}
        <At x={RIVAL.x} y={RIVAL.y} opacity={fade(f, 6.4)}>
          <AgentAvatar name="Firm Rival" color={4} size={110} label="Firm Rival" sub="another owner" />
        </At>
        <At x={RIVAL.x} y={RIVAL.y + 180}><Delta v="−5.00" tone="rose" o={prog(f, 8.4, 0.4)} /></At>
        <At x={(LEAD.x + RIVAL.x) / 2} y={LEAD.y - 50} opacity={fade(f, 7.0)}>
          <div style={{ display: "flex", alignItems: "center", gap: 12, color: "var(--yellow)" }}>
            <LockIcon size={34} /><span style={{ fontFamily: MONO, fontSize: 24 }}>5.00 per side</span>
          </div>
        </At>
        <At x={(LEAD.x + RIVAL.x) / 2} y={LEAD.y + 50} scale={snap(f, fps, 8.2, 10)}>
          <Pill tone="mint" size={22}>Lead wins</Pill>
        </At>
        {/* 80 / 20 split and the pool pouring to members by weight */}
        <At x={LEAD.x} y={LEAD.y + 275} opacity={fade(f, 9.4) * (1 - prog(f, 13.6, 0.4))}>
          <div style={{ width: 520 }}>
            <div style={{ display: "flex", gap: lerp(0, 10, split), height: 40 }}>
              <div style={{ flex: 80, borderRadius: 8, background: "var(--mint-a)", border: "1px solid var(--mint)", display: "grid", placeItems: "center",
                fontFamily: MONO, fontSize: 20, color: "var(--mint)" }}>lead 4.00</div>
              <div style={{ flex: 20, borderRadius: 8, background: "var(--yellow-a)", border: "1px solid var(--yellow)", display: "grid", placeItems: "center",
                fontFamily: MONO, fontSize: 20, color: "var(--yellow)", opacity: 1 - pour * 0.6 }}>1.00</div>
            </div>
            <Eyebrow style={{ marginTop: 10, textAlign: "center" }}>+5.00 → 80 % lead · 20 % member pool</Eyebrow>
          </div>
        </At>
        {pour > 0 && pour < 1 && <>
          <At x={lerp(LEAD.x + 210, SCOUT.x + 170, pour)} y={lerp(LEAD.y + 275, SCOUT.y - 20, pour)}><MoneyChip amount="0.60" tone="mint" size={20} /></At>
          <At x={lerp(LEAD.x + 210, ANALYST.x + 170, pour)} y={lerp(LEAD.y + 275, ANALYST.y - 20, pour)}><MoneyChip amount="0.40" tone="mint" size={20} /></At>
        </>}
        <At x={LEAD.x} y={LEAD.y + 290} opacity={fade(f, 13.8)}>
          <div style={{ fontFamily: MONO, fontSize: 21, color: "var(--text-3)", textAlign: "center" }}>5.00 − 1.00 pool − 0.20 fees = 3.80</div>
        </At>
      </AbsoluteFill>
      {/* Loss variant */}
      <At x={960} y={500} opacity={loss} scale={lerp(0.96, 1, loss)}>
        <Card elevated style={{ width: 820, padding: 34, textAlign: "center" }}>
          <Eyebrow>If the lead loses instead</Eyebrow>
          <div style={{ marginTop: 14, fontSize: 32, color: "var(--text)", fontWeight: 500 }}>The lead takes the loss.</div>
          <div style={{ marginTop: 8, fontSize: 26, color: "var(--text-2)" }}>Members keep their task fees and pay nothing toward it.</div>
        </Card>
      </At>
      <Title at={15.8} y={850} size={44}>Members are paid from the lead's profit. They never cover its losses.</Title>
      <Caption text="Firms run on simulated funds." at={0.5} bottom={30} />
    </Stage>
  );
};
