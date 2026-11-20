// Ten logo concepts. Each is a <Scene> with a different TZW object in the gap between the hands.
import React from "react";
import { FRESCO, NIGHT, Palette, SERIF, Scene } from "./paper";

const PF = { filter: "url(#paper)" } as const;

/** TZW wordmark in Roman capitals with the yellow tick, placed below the hands. */
const Word: React.FC<{ p: Palette; y?: number; size?: number }> = ({ p, y = 650, size = 96 }) => (
  <g>
    <text x={600} y={y} textAnchor="middle" style={{ fontFamily: SERIF, fontWeight: 700, fontSize: size, letterSpacing: "0.22em", fill: p.ink, ...PF }}>TZW</text>
    <rect x={600 + size * 1.08} y={y - size * 0.17} width={size * 0.16} height={size * 0.16} rx={2} style={{ fill: p.gold, ...PF }} />
  </g>
);

export type Concept = { id: string; name: string; idea: string; render: (square: boolean) => React.ReactNode };

export const CONCEPTS: Concept[] = [
  {
    id: "01-coin", name: "The Coin", idea: "A gold coin passes between the hands: value exchanged between owner and agent.",
    render: (sq) => (
      <Scene p={FRESCO} square={sq}>
        <circle r={46} style={{ fill: "url(#goldL)", ...PF }} />
        <circle r={36} style={{ fill: "none", stroke: "#8E6A1E", strokeWidth: 2.5 }} />
        <text y={14} textAnchor="middle" style={{ fontFamily: SERIF, fontWeight: 700, fontSize: 40, fill: "#6E4F12" }}>Z</text>
      </Scene>
    ),
  },
  {
    id: "02-escrow-lock", name: "Escrow Lock", idea: "A padlock between the fingertips: the stake both sides lock before a challenge.",
    render: (sq) => (
      <Scene p={FRESCO} square={sq}>
        <path d="M-24,-6 V-26 A24,24 0 0 1 24,-26 V-6" style={{ fill: "none", stroke: "#8E6A1E", strokeWidth: 9, strokeLinecap: "round", ...PF }} />
        <rect x={-38} y={-10} width={76} height={60} rx={10} style={{ fill: "url(#goldL)", ...PF }} />
        <circle cy={16} r={7} style={{ fill: "#6E4F12" }} /><rect x={-3} y={18} width={6} height={16} rx={3} style={{ fill: "#6E4F12" }} />
      </Scene>
    ),
  },
  {
    id: "03-term-scroll", name: "The Terms", idea: "A small unrolled scroll: every challenge is written down before money moves.",
    render: (sq) => (
      <Scene p={FRESCO} square={sq}>
        <rect x={-34} y={-44} width={68} height={88} rx={4} style={{ fill: "#F4EBD8", ...PF }} />
        <rect x={-42} y={-52} width={84} height={14} rx={7} style={{ fill: "url(#goldL)", ...PF }} />
        <rect x={-42} y={38} width={84} height={14} rx={7} style={{ fill: "url(#goldL)", ...PF }} />
        {[-22, -8, 6, 20].map((y, i) => <line key={y} x1={-22} x2={i === 3 ? 6 : 22} y1={y} y2={y} style={{ stroke: "#7A6247", strokeWidth: 3, strokeLinecap: "round" }} />)}
      </Scene>
    ),
  },
  {
    id: "04-scales", name: "Fair Scales", idea: "A balance in the gap: a fair, pre-agreed way to decide who won.",
    render: (sq) => (
      <Scene p={FRESCO} square={sq} gap={170}>
        <rect x={-3} y={-50} width={6} height={92} style={{ fill: "#8E6A1E", ...PF }} />
        <rect x={-58} y={-46} width={116} height={6} rx={3} style={{ fill: "url(#goldL)", ...PF }} />
        {[-50, 50].map((x) => <g key={x}>
          <line x1={x} y1={-42} x2={x - 16} y2={4} style={{ stroke: "#8E6A1E", strokeWidth: 2 }} />
          <line x1={x} y1={-42} x2={x + 16} y2={4} style={{ stroke: "#8E6A1E", strokeWidth: 2 }} />
          <path d={`M${x - 22},4 Q${x},26 ${x + 22},4 Z`} style={{ fill: "url(#goldL)", ...PF }} />
        </g>)}
        <rect x={-22} y={40} width={44} height={8} rx={4} style={{ fill: "url(#goldL)", ...PF }} />
      </Scene>
    ),
  },
  {
    id: "05-tick-wordmark", name: "The Spark (wordmark)", idea: "The app's yellow tick is the spark between the hands; TZW in Roman capitals below.",
    render: (sq) => (
      <Scene p={FRESCO} square={sq} cy={360} gap={120} below={<Word p={FRESCO} y={sq ? 640 : 650} />}>
        <rect x={-15} y={-15} width={30} height={30} rx={4} transform="rotate(45)" style={{ fill: FRESCO.gold, ...PF }} />
      </Scene>
    ),
  },
  {
    id: "06-z-spark", name: "Z Spark (roundel)", idea: "The letter Z is the lightning between the fingers, inside a fresco roundel.",
    render: (sq) => (
      <Scene p={FRESCO} square={sq} frame="roundel" gap={130}>
        <path d="M-30,-38 H30 V-26 L-10,26 H30 V38 H-30 V26 L10,-26 H-30 Z" style={{ fill: FRESCO.gold, stroke: FRESCO.goldDark, strokeWidth: 2, ...PF }} />
      </Scene>
    ),
  },
  {
    id: "07-owner-agent", name: "Owner and Agent", idea: "A human hand meets a paper 'agent' hand traced with circuit nodes; the tick sparks between them.",
    render: (sq) => (
      <Scene p={NIGHT} square={sq} agentRight gap={120}>
        <rect x={-13} y={-13} width={26} height={26} rx={4} transform="rotate(45)" style={{ fill: NIGHT.gold, ...PF }} />
      </Scene>
    ),
  },
  {
    id: "08-agent-network", name: "Agent Network", idea: "Three linked nodes in the gap: many owners' agents meeting each other.",
    render: (sq) => (
      <Scene p={FRESCO} square={sq} gap={170}>
        {[[0, -36], [-36, 28], [36, 28]].map(([x, y], i, a) => <line key={i} x1={x} y1={y} x2={a[(i + 1) % 3][0]} y2={a[(i + 1) % 3][1]} style={{ stroke: "#8E6A1E", strokeWidth: 4 }} />)}
        {[[0, -36], [-36, 28], [36, 28]].map(([x, y], i) => <circle key={i} cx={x} cy={y} r={15} style={{ fill: i === 0 ? FRESCO.gold : "#F4EBD8", stroke: "#8E6A1E", strokeWidth: 3, ...PF }} />)}
      </Scene>
    ),
  },
  {
    id: "09-chain-link", name: "The Link", idea: "Two interlocked links: the agreement recorded onchain, binding both sides.",
    render: (sq) => (
      <Scene p={FRESCO} square={sq} gap={170}>
        <rect x={-50} y={-20} width={62} height={40} rx={20} transform="rotate(-20)" style={{ fill: "none", stroke: "url(#goldL)", strokeWidth: 11, ...PF }} />
        <rect x={-12} y={-20} width={62} height={40} rx={20} transform="rotate(-20)" style={{ fill: "none", stroke: FRESCO.gold, strokeWidth: 11, ...PF }} />
      </Scene>
    ),
  },
  {
    id: "10-cartouche", name: "Cartouche (letter logo)", idea: "TZW monogram on a Renaissance cartouche, the hands touching above it.",
    render: (sq) => (
      <Scene p={FRESCO} square={sq} frame="cartouche" cy={330} gap={110}
        below={<Word p={FRESCO} y={560} size={110} />}>
        <circle r={10} style={{ fill: FRESCO.gold, ...PF }} />
      </Scene>
    ),
  },
];
