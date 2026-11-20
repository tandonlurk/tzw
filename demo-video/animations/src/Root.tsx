// One composition per animated scene in ../story-outline.md. Composition ids = file prefixes,
// so `npm run render` writes renders/NN-slug.mp4 that the assembly step pairs by prefix.
import "../../../web/app/globals.css";
import React from "react";
import { Composition } from "remotion";
import { FPS, s } from "./lib/anim";
import { S01ColdOpen } from "./scenes/S01ColdOpen";
import { S14Close } from "./scenes/S14Close";

export const SCENES: { id: string; component: React.FC; sec: number }[] = [
  { id: "01-cold-open", component: S01ColdOpen, sec: 4 },
  { id: "14-close", component: S14Close, sec: 6 },
];

export const Root: React.FC = () => (
  <>
    {SCENES.map((sc) => (
      <Composition key={sc.id} id={sc.id} component={sc.component} durationInFrames={s(sc.sec)} fps={FPS} width={1920} height={1080} />
    ))}
  </>
);
