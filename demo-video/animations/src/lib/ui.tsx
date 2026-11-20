// Shared visual vocabulary for every animation (animation-plan.md section 3).
// Colours come from the app's own tokens (web/app/globals.css, imported in Root.tsx); never hex here.
import React from "react";
import { AbsoluteFill, useCurrentFrame, useVideoConfig } from "remotion";
import { loadFont as loadInter } from "@remotion/google-fonts/Inter";
import { loadFont as loadMono } from "@remotion/google-fonts/JetBrainsMono";
import { fade, prog, snap } from "./anim";

const inter = loadInter("normal", { weights: ["400", "500", "600", "700"], subsets: ["latin"] });
const mono = loadMono("normal", { weights: ["400", "500", "600"], subsets: ["latin"] });

export const SANS = inter.fontFamily;
export const MONO = mono.fontFamily;

export type Tone = "blue" | "yellow" | "mint" | "rose" | "steel" | "text" | "dim";
export const toneColor = (t: Tone) =>
  t === "text" ? "var(--text)" : t === "dim" ? "var(--text-3)" : `var(--${t})`;
export const toneBg = (t: Tone) =>
  t === "blue" || t === "yellow" || t === "mint" || t === "rose" ? `var(--${t}-a)` : "var(--surface-2)";
export const agentColor = (i: number) => `var(--agent-${i % 8})`;

// ---------- Stage: canvas + always-on chrome ----------

export type Env = "arc" | "sim";

export const EnvBadge: React.FC<{ env: Env }> = ({ env }) => (
  <div style={{ position: "absolute", top: 40, right: 48, display: "flex", alignItems: "center", gap: 12,
    fontSize: 22, color: "var(--text-3)", padding: "10px 18px", border: "1px solid var(--line-strong)",
    borderRadius: 999, background: "var(--bg-2)" }}>
    <span style={{ width: 9, height: 9, borderRadius: 9, background: "var(--yellow)" }} />
    {env === "arc" ? "Arc testnet · no real value" : "Simulated funds"}
  </div>
);

export const Tag: React.FC<{ text: string }> = ({ text }) => (
  <div style={{ position: "absolute", top: 40, left: 48, fontSize: 18, letterSpacing: ".1em",
    textTransform: "uppercase", color: "var(--text-3)", border: "1px solid var(--line-strong)",
    borderRadius: 999, padding: "8px 16px" }}>{text}</div>
);

/** Small fixed honesty caption, bottom centre. */
export const Caption: React.FC<{ text: string; at?: number; bottom?: number }> = ({ text, at = 0, bottom = 44 }) => {
  const f = useCurrentFrame();
  return (
    <div style={{ position: "absolute", bottom, left: 0, right: 0, textAlign: "center", fontSize: 22,
      color: "var(--text-3)", opacity: fade(f, at) }}>{text}</div>
  );
};

export const Stage: React.FC<{
  env?: Env; tag?: string; children: React.ReactNode; fadeIn?: boolean;
}> = ({ env, tag, children, fadeIn = false }) => {
  const f = useCurrentFrame();
  return (
    <AbsoluteFill style={{ background: "var(--bg)", fontFamily: SANS, color: "var(--text-2)",
      fontFeatureSettings: '"cv11","ss01","ss03"', fontVariantNumeric: "tabular-nums",
      ["--font-inter" as string]: SANS, ["--font-jbmono" as string]: MONO } as React.CSSProperties}>
      <AbsoluteFill style={{ opacity: fadeIn ? fade(f, 0, undefined, 0.6) : 1 }}>
        {children}
        {tag && <Tag text={tag} />}
        {env && <EnvBadge env={env} />}
      </AbsoluteFill>
    </AbsoluteFill>
  );
};

// ---------- Text ----------

/** Large statement caption; fades + rises in at `at` seconds. */
export const Title: React.FC<{ children: React.ReactNode; at: number; out?: number; y?: number; size?: number; sub?: string; subAt?: number }> =
  ({ children, at, out, y = 860, size = 52, sub, subAt }) => {
    const f = useCurrentFrame();
    const p = prog(f, at, 0.5);
    return (
      <div style={{ position: "absolute", top: y, left: 120, right: 120, textAlign: "center",
        opacity: fade(f, at, out, 0.45), transform: `translateY(${(1 - p) * 18}px)` }}>
        <div style={{ fontSize: size, fontWeight: 500, color: "var(--text)", letterSpacing: "-0.02em", lineHeight: 1.15 }}>{children}</div>
        {sub && <div style={{ marginTop: 14, fontSize: 28, color: "var(--text-3)", opacity: fade(f, subAt ?? at + 0.6) }}>{sub}</div>}
      </div>
    );
  };

// ---------- Brand ----------

export const Wordmark: React.FC<{ at?: number; size?: number }> = ({ at = 0, size = 120 }) => {
  const f = useCurrentFrame();
  const { fps } = useVideoConfig();
  const letters = ["T", "Z", "W"];
  return (
    <div style={{ display: "inline-flex", alignItems: "center", gap: size * 0.3, fontFamily: MONO,
      fontWeight: 600, fontSize: size, letterSpacing: ".22em", color: "var(--text)" }}>
      <span>
        {letters.map((l, i) => {
          const p = prog(f, at + i * 0.06, 0.35);
          return <span key={l} style={{ display: "inline-block", opacity: p, transform: `translateY(${(1 - p) * 20}px)` }}>{l}</span>;
        })}
      </span>
      <span style={{ width: size * 0.2, height: size * 0.2, borderRadius: size * 0.03, background: "var(--yellow)",
        transform: `scale(${snap(f, fps, at + 0.3, 10)})`, marginLeft: -size * 0.12 }} />
    </div>
  );
};

// ---------- Agents and money ----------

export const initials = (name: string) => {
  const w = name.replace(/\(.*?\)/g, "").trim().split(/\s+/).filter(Boolean);
  return ((w[0]?.[0] ?? "?") + (w[1]?.[0] ?? "")).toUpperCase();
};

export const AgentAvatar: React.FC<{
  name: string; color: number; size?: number; label?: string; sub?: string; ring?: Tone; style?: React.CSSProperties; dim?: boolean;
}> = ({ name, color, size = 96, label, sub, ring, style, dim }) => (
  <div style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 12, ...style }}>
    <div style={{ width: size, height: size, borderRadius: size, display: "grid", placeItems: "center",
      background: agentColor(color), color: "var(--on-accent)", fontWeight: 600, fontSize: size * 0.36,
      boxShadow: ring ? `0 0 0 ${size * 0.06}px var(--bg), 0 0 0 ${size * 0.1}px ${toneColor(ring)}` : undefined,
      opacity: dim ? 0.45 : 1 }}>{initials(name)}</div>
    {label && <div style={{ fontSize: Math.max(20, size * 0.26), color: "var(--text)", fontWeight: 500, whiteSpace: "nowrap" }}>{label}</div>}
    {sub && <div style={{ fontSize: Math.max(17, size * 0.2), color: "var(--text-3)", whiteSpace: "nowrap" }}>{sub}</div>}
  </div>
);

export const MoneyChip: React.FC<{ amount: string; tone?: Tone; size?: number; style?: React.CSSProperties; suffix?: string }> =
  ({ amount, tone = "yellow", size = 26, style, suffix }) => (
    <div style={{ display: "inline-flex", alignItems: "center", gap: 8, padding: `${size * 0.3}px ${size * 0.6}px`,
      borderRadius: 999, background: toneBg(tone), color: toneColor(tone), border: `1px solid ${toneColor(tone)}`,
      fontFamily: MONO, fontWeight: 600, fontSize: size, whiteSpace: "nowrap", ...style }}>
      {amount}{suffix && <span style={{ fontWeight: 400, opacity: 0.8, fontSize: size * 0.75 }}>{suffix}</span>}
    </div>
  );

export const Pill: React.FC<{ children: React.ReactNode; tone?: Tone; size?: number; style?: React.CSSProperties; mono?: boolean }> =
  ({ children, tone = "steel", size = 22, style, mono }) => (
    <div style={{ display: "inline-flex", alignItems: "center", gap: 10, padding: `${size * 0.35}px ${size * 0.75}px`,
      borderRadius: 999, background: toneBg(tone), color: toneColor(tone), border: "1px solid var(--line-strong)",
      fontSize: size, fontFamily: mono ? MONO : SANS, fontWeight: 500, whiteSpace: "nowrap", ...style }}>{children}</div>
  );

export const Card: React.FC<{ children: React.ReactNode; style?: React.CSSProperties; elevated?: boolean; accent?: Tone }> =
  ({ children, style, elevated, accent }) => (
    <div style={{ background: elevated ? "var(--surface-2)" : "var(--surface)", borderRadius: 18,
      border: `1px solid ${accent ? toneColor(accent) : "var(--line-strong)"}`, padding: 28, ...style }}>{children}</div>
  );

export const Eyebrow: React.FC<{ children: React.ReactNode; style?: React.CSSProperties }> = ({ children, style }) => (
  <div style={{ fontSize: 17, letterSpacing: ".1em", textTransform: "uppercase", color: "var(--text-3)", fontWeight: 500, ...style }}>{children}</div>
);

export const TxChip: React.FC<{ label: string; style?: React.CSSProperties }> = ({ label, style }) => (
  <div style={{ display: "inline-flex", alignItems: "center", gap: 10, padding: "8px 14px", borderRadius: 10,
    background: "var(--surface-2)", border: "1px solid var(--line-strong)", fontFamily: MONO, fontSize: 19,
    color: "var(--text-2)", whiteSpace: "nowrap", ...style }}>
    <span style={{ color: "var(--mint)" }}>✓</span>{label}
  </div>
);

// ---------- Logic-block glyphs (docs/design/ui.md canvas shapes) ----------

export type BlockKind = "trigger" | "condition" | "action" | "approach";
export const BlockGlyph: React.FC<{ kind: BlockKind; size?: number; lit?: number; style?: React.CSSProperties }> =
  ({ kind, size = 56, lit = 1, style }) => {
    const color = kind === "trigger" ? "var(--blue)" : kind === "condition" ? "var(--yellow)" : kind === "action" ? "var(--mint)" : "var(--steel)";
    const h = size / 2;
    const shape =
      kind === "trigger" ? <circle cx={h} cy={h} r={h - 4} /> :
      kind === "condition" ? <polygon points={`${h},4 ${size - 4},${h} ${h},${size - 4} 4,${h}`} /> :
      kind === "action" ? <polygon points={`${size * 0.27},5 ${size * 0.73},5 ${size - 4},${h} ${size * 0.73},${size - 5} ${size * 0.27},${size - 5} 4,${h}`} /> :
      <polygon points={`${size * 0.25},8 ${size - 4},8 ${size * 0.75},${size - 8} 4,${size - 8}`} />;
    return (
      <svg width={size} height={size} style={{ overflow: "visible", ...style }}>
        <g style={{ fill: color, fillOpacity: 0.12 + 0.7 * lit, stroke: color, strokeWidth: 3 }}>{shape}</g>
      </svg>
    );
  };

// ---------- Lifecycle map (story-outline.md "recurring device") ----------

export const STATIONS = [
  { name: "Discover", sub: "market data" },
  { name: "Negotiate", sub: "agent to agent" },
  { name: "Check limits", sub: "outside the AI" },
  { name: "Lock", sub: "escrow on Arc" },
  { name: "Resolve", sub: "named source" },
  { name: "Record", sub: "ledger + chain" },
] as const;

/** Thin strip at the top of an explainer with the current station lit. */
export const LifecycleStrip: React.FC<{ active: number | number[]; at?: number }> = ({ active, at = 0 }) => {
  const f = useCurrentFrame();
  const act = Array.isArray(active) ? active : [active];
  return (
    <div style={{ position: "absolute", top: 118, left: 0, right: 0, display: "flex", justifyContent: "center",
      alignItems: "center", gap: 14, opacity: fade(f, at) }}>
      {STATIONS.map((st, i) => {
        const on = act.includes(i);
        return (
          <React.Fragment key={st.name}>
            {i > 0 && <div style={{ width: 34, height: 2, background: "var(--line-strong)" }} />}
            <div style={{ fontSize: 20, padding: "7px 16px", borderRadius: 999,
              color: on ? "var(--on-accent)" : "var(--text-3)", background: on ? "var(--blue)" : "transparent",
              border: `1px solid ${on ? "var(--blue)" : "var(--line-strong)"}`, fontWeight: on ? 600 : 400 }}>{st.name}</div>
          </React.Fragment>
        );
      })}
    </div>
  );
};

// ---------- Lines between points (SVG overlay) ----------

/** A line that draws from a to b as t goes 0→1. */
export const DrawLine: React.FC<{ x1: number; y1: number; x2: number; y2: number; t: number; color?: string; width?: number; dash?: string; opacity?: number }> =
  ({ x1, y1, x2, y2, t, color = "var(--blue)", width = 3, dash, opacity = 1 }) => (
    <line x1={x1} y1={y1} x2={x1 + (x2 - x1) * t} y2={y1 + (y2 - y1) * t}
      style={{ stroke: color, strokeWidth: width, strokeDasharray: dash, opacity, strokeLinecap: "round" }} />
  );

export const Svg: React.FC<{ children: React.ReactNode }> = ({ children }) => (
  <svg width={1920} height={1080} style={{ position: "absolute", inset: 0, overflow: "visible" }}>{children}</svg>
);

/** Position a child centred on (x, y). */
export const At: React.FC<{ x: number; y: number; children: React.ReactNode; scale?: number; opacity?: number; style?: React.CSSProperties }> =
  ({ x, y, children, scale = 1, opacity = 1, style }) => (
    <div style={{ position: "absolute", left: x, top: y, transform: `translate(-50%, -50%) scale(${scale})`, opacity, ...style }}>{children}</div>
  );
