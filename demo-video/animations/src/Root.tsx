// One composition per animated scene in ../story-outline.md. Composition ids = file prefixes,
// so `npm run render` writes renders/NN-slug.mp4 that the assembly step pairs by prefix.
import "../../../web/app/globals.css";
import React from "react";
import { Composition } from "remotion";
import { FPS, s } from "./lib/anim";
import { MinimalProvider } from "./lib/ui";
import { CONCEPTS } from "./logos/Logos";
import { S01ColdOpen } from "./scenes/S01ColdOpen";
import { S02Problem } from "./scenes/S02Problem";
import { S03AgentToAgent } from "./scenes/S03AgentToAgent";
import { S05LifecycleMap } from "./scenes/S05LifecycleMap";
import { S06Discover } from "./scenes/S06Discover";
import { S07NegotiateGuard } from "./scenes/S07NegotiateGuard";
import { S08Escrow } from "./scenes/S08Escrow";
import { S10Scale } from "./scenes/S10Scale";
import { S12Firms } from "./scenes/S12Firms";
import { S14Close } from "./scenes/S14Close";

export const SCENES: { id: string; component: React.FC; sec: number }[] = [
  { id: "01-cold-open", component: S01ColdOpen, sec: 4 },
  { id: "02-problem", component: S02Problem, sec: 21 },
  { id: "03-agent-to-agent", component: S03AgentToAgent, sec: 10 },
  { id: "05-lifecycle-map", component: S05LifecycleMap, sec: 8 },
  { id: "06-discover", component: S06Discover, sec: 10 },
  { id: "07-negotiate-guard", component: S07NegotiateGuard, sec: 14 },
  { id: "08-escrow", component: S08Escrow, sec: 20 },
  { id: "10-scale", component: S10Scale, sec: 15 },
  { id: "12-firms", component: S12Firms, sec: 18 },
  { id: "14-close", component: S14Close, sec: 6 },
];

// Every scene also renders as `<id>-min`: same timing, no on-screen words (the voiceover carries them).
// (Props must be serializable in Remotion, so wrap each scene once instead of passing it as a prop.)
const MIN = SCENES.map((sc) => {
  const C = sc.component;
  const W: React.FC = () => <MinimalProvider value={true}><C /></MinimalProvider>;
  return W;
});

export const Root: React.FC = () => (
  <>
    {SCENES.map((sc, i) => (
      <React.Fragment key={sc.id}>
        <Composition id={sc.id} component={sc.component} durationInFrames={s(sc.sec)} fps={FPS} width={1920} height={1080} />
        <Composition id={`${sc.id}-min`} component={MIN[i]}
          durationInFrames={s(sc.sec)} fps={FPS} width={1920} height={1080} />
      </React.Fragment>
    ))}
    {/* Logo concepts: 3:2 for the Devpost thumbnail, square for avatars. Render with `remotion still`. */}
    {CONCEPTS.map((c) => (
      <React.Fragment key={c.id}>
        <Composition id={`logo-${c.id}`} component={LOGO_WIDE[c.id]} durationInFrames={1} fps={FPS} width={1200} height={800} />
        <Composition id={`logo-${c.id}-sq`} component={LOGO_SQ[c.id]} durationInFrames={1} fps={FPS} width={1024} height={1024} />
      </React.Fragment>
    ))}
  </>
);

const LOGO_WIDE: Record<string, React.FC> = Object.fromEntries(CONCEPTS.map((c) => [c.id, () => <>{c.render(false)}</>]));
const LOGO_SQ: Record<string, React.FC> = Object.fromEntries(CONCEPTS.map((c) => [c.id, () => <>{c.render(true)}</>]));
