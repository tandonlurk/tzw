// Paper-cut, Renaissance-fresco logo scenes for TZW: two reaching hands (after Michelangelo's
// "Creation of Adam", public domain) with a TZW object in the gap. Pure SVG; each layer is a cut
// sheet of textured paper with a soft drop shadow. Designed on a 1200x800 canvas (Devpost 3:2);
// the square version crops the centre 800x800.
import React from "react";
import { AbsoluteFill } from "remotion";
import { loadFont as loadCinzel } from "@remotion/google-fonts/Cinzel";

export const SERIF = loadCinzel("normal", { weights: ["600", "700"], subsets: ["latin"] }).fontFamily;

export type Palette = {
  bg: string; bg2: string; ring: string[]; skin: [string, string]; skinShade: string; crease: string;
  cloak: string; cloak2: string; ground: string; ink: string; gold: string; goldDark: string; agent: [string, string];
};

// Fresco: plaster, ochre, sienna, lapis; gold is TZW's yellow.
export const FRESCO: Palette = {
  bg: "#EDE3CF", bg2: "#E2D3B6", ring: ["#D9C39D", "#CBAF83", "#B9976A"],
  skin: ["#F0CFA9", "#C98F63"], skinShade: "#A86E48", crease: "#8A5534",
  cloak: "#A4483A", cloak2: "#7E3229", ground: "#7C8B5C", ink: "#3B2A1E",
  gold: "#E7BD58", goldDark: "#B88A2A", agent: ["#BCD3EA", "#6F8FB0"],
};
// Night: the app's charcoal with the same paper hands (fits the product UI and dark Devpost cards).
export const NIGHT: Palette = {
  bg: "#1D1D1D", bg2: "#242424", ring: ["#2B2B2B", "#333333", "#3B3B3B"],
  skin: ["#EBCBA6", "#B9825A"], skinShade: "#946140", crease: "#7A4A2D",
  cloak: "#3E5A7A", cloak2: "#2C4460", ground: "#3B4636", ink: "#F1F1F1",
  gold: "#E7BD58", goldDark: "#B88A2A", agent: ["#A9CFF5", "#5E7FA3"],
};

/** Shared defs: paper grain + cut shadow. */
export const PaperDefs: React.FC<{ p: Palette }> = ({ p }) => (
  <defs>
    <filter id="paper" x="-20%" y="-20%" width="140%" height="140%">
      <feTurbulence type="fractalNoise" baseFrequency="0.85" numOctaves={2} seed={7} result="noise" />
      <feColorMatrix in="noise" type="saturate" values="0" result="mono" />
      <feComponentTransfer in="mono" result="grain">
        <feFuncR type="linear" slope={0.16} intercept={0.86} />
        <feFuncG type="linear" slope={0.16} intercept={0.86} />
        <feFuncB type="linear" slope={0.16} intercept={0.86} />
      </feComponentTransfer>
      <feBlend in="SourceGraphic" in2="grain" mode="multiply" result="tex" />
      <feComposite in="tex" in2="SourceAlpha" operator="in" result="cut" />
      <feDropShadow in="cut" dx={0} dy={5} stdDeviation={6} floodColor="#1E120A" floodOpacity={0.42} />
    </filter>
    <linearGradient id="skinL" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stopColor={p.skin[0]} /><stop offset="1" stopColor={p.skin[1]} />
    </linearGradient>
    <linearGradient id="agentSkin" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stopColor={p.agent[0]} /><stop offset="1" stopColor={p.agent[1]} />
    </linearGradient>
    <radialGradient id="glow" cx="0.5" cy="0.5" r="0.5">
      <stop offset="0" stopColor={p.gold} stopOpacity={0.55} /><stop offset="1" stopColor={p.gold} stopOpacity={0} />
    </radialGradient>
    <linearGradient id="goldL" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stopColor="#F6D98C" /><stop offset="1" stopColor={p.goldDark} />
    </linearGradient>
  </defs>
);

// ---------- Hand: pointing right, fingertip at (474, 128) ----------

const HAND = "M0,118 C80,112 180,108 255,112 C285,113 305,104 330,100 C350,97 365,103 372,110 L455,116 C470,117 476,124 474,131 C472,138 464,140 452,139 L378,136 C392,140 396,150 388,156 C396,160 398,170 388,176 C394,181 392,190 380,192 C350,198 320,196 300,192 C270,196 250,200 230,202 C150,208 80,210 0,214 Z";
const THUMB = "M296,126 C318,121 342,126 354,137 C360,143 354,150 343,148 C328,146 312,142 298,140 Z";
const SHADE = "M0,182 C90,180 190,176 250,178 C285,180 320,186 360,186 L380,192 C350,198 320,196 300,192 C270,196 250,200 230,202 C150,208 80,210 0,214 Z";
const CREASES = ["M384,146 L396,148", "M384,166 L396,168", "M386,184 L392,186", "M418,121 L420,133", "M440,123 L441,135"];
export const TIP = { x: 474, y: 128 };

export const Hand: React.FC<{ p: Palette; agent?: boolean; sleeve?: boolean }> = ({ p, agent, sleeve }) => (
  <g>
    {sleeve && <path d="M-40,92 C40,80 120,86 190,96 C205,120 205,200 190,226 C120,236 40,240 -40,236 Z" style={{ fill: p.cloak, filter: "url(#paper)" }} />}
    {sleeve && <path d="M-40,104 C40,96 110,100 168,108 C176,140 176,190 168,214 C110,222 40,226 -40,224 Z" style={{ fill: p.cloak2, opacity: 0.55 }} />}
    <path d={HAND} style={{ fill: agent ? "url(#agentSkin)" : "url(#skinL)", filter: "url(#paper)" }} />
    <path d={SHADE} style={{ fill: agent ? p.agent[1] : p.skinShade, opacity: 0.35 }} />
    <path d={THUMB} style={{ fill: agent ? p.agent[0] : p.skin[0], filter: "url(#paper)" }} />
    {CREASES.map((d) => <path key={d} d={d} style={{ stroke: agent ? p.agent[1] : p.crease, strokeWidth: 2, strokeLinecap: "round", opacity: 0.6 }} />)}
    {agent && [[120, 160], [200, 150], [280, 140], [330, 125]].map(([x, y], i) => (
      <g key={i}>
        {i > 0 && <line x1={[120, 200, 280][i - 1]} y1={[160, 150, 140][i - 1]} x2={x} y2={y} style={{ stroke: p.agent[1], strokeWidth: 2.5 }} />}
        <circle cx={x} cy={y} r={7} style={{ fill: p.agent[1] }} />
      </g>
    ))}
  </g>
);

/** Place a hand so its fingertip sits at (x, y). `mirror` = coming from the right. */
export const PlacedHand: React.FC<{ p: Palette; x: number; y: number; mirror?: boolean; rot?: number; scale?: number; agent?: boolean; sleeve?: boolean }> =
  ({ p, x, y, mirror, rot = 0, scale = 1, agent, sleeve }) => (
    <g transform={`translate(${x} ${y}) rotate(${rot}) scale(${mirror ? -scale : scale} ${scale}) translate(${-TIP.x} ${-TIP.y})`}>
      <Hand p={p} agent={agent} sleeve={sleeve} />
    </g>
  );

// ---------- Backdrop ----------

export const Backdrop: React.FC<{ p: Palette; frame?: "rings" | "roundel" | "cartouche" }> = ({ p, frame = "rings" }) => (
  <g>
    <rect x={0} y={0} width={1200} height={800} style={{ fill: p.bg }} />
    {frame === "rings" && p.ring.map((c, i) => (
      <ellipse key={i} cx={600} cy={400} rx={560 - i * 120} ry={360 - i * 80} style={{ fill: c, filter: "url(#paper)", opacity: 0.9 }} />
    ))}
    {frame === "roundel" && [0, 1, 2].map((i) => (
      <circle key={i} cx={600} cy={400} r={370 - i * 34} style={{ fill: p.ring[i], filter: "url(#paper)" }} />
    ))}
    {frame === "cartouche" && <>
      <path d="M200,170 Q600,90 1000,170 Q1060,400 1000,630 Q600,710 200,630 Q140,400 200,170 Z" style={{ fill: p.ring[0], filter: "url(#paper)" }} />
      <path d="M250,215 Q600,145 950,215 Q995,400 950,585 Q600,655 250,585 Q205,400 250,215 Z" style={{ fill: p.bg2, filter: "url(#paper)" }} />
    </>}
    {/* God's cloud-mantle (right) and Adam's hill (left), as cut paper */}
    <path d="M1200,120 C1090,130 1010,190 990,260 C960,250 920,270 915,310 C880,320 870,370 905,390 L1200,420 Z" style={{ fill: p.cloak, filter: "url(#paper)" }} />
    <path d="M1200,170 C1110,180 1050,230 1035,280 C1005,285 985,310 990,340 L1200,360 Z" style={{ fill: p.cloak2, filter: "url(#paper)" }} />
    <path d="M0,800 L0,560 C120,520 260,540 330,600 C380,640 400,700 420,800 Z" style={{ fill: p.ground, filter: "url(#paper)" }} />
  </g>
);

export const Glow: React.FC<{ x?: number; y?: number; r?: number }> = ({ x = 600, y = 400, r = 120 }) => (
  <circle cx={x} cy={y} r={r} style={{ fill: "url(#glow)" }} />
);

/** Full logo scene: backdrop, two hands with fingertips `gap` apart, object in the middle. */
export const Scene: React.FC<{
  p: Palette; square?: boolean; frame?: "rings" | "roundel" | "cartouche"; gap?: number; cy?: number;
  agentRight?: boolean; children?: React.ReactNode; below?: React.ReactNode; glow?: boolean;
}> = ({ p, square, frame, gap = 150, cy = 400, agentRight, children, below, glow = true }) => (
  <AbsoluteFill style={{ background: p.bg }}>
    <svg width="100%" height="100%" viewBox={square ? "200 0 800 800" : "0 0 1200 800"} preserveAspectRatio="xMidYMid slice">
      <PaperDefs p={p} />
      <Backdrop p={p} frame={frame} />
      {glow && <Glow y={cy} />}
      <PlacedHand p={p} x={600 - gap / 2} y={cy + 6} rot={10} scale={1.05} />
      <PlacedHand p={p} x={600 + gap / 2} y={cy} mirror rot={-4} scale={1.05} sleeve={!agentRight} agent={agentRight} />
      <g transform={`translate(600 ${cy})`}>{children}</g>
      {below}
    </svg>
  </AbsoluteFill>
);
