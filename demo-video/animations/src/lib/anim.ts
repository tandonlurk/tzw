// Timing helpers. 30 fps everywhere; scene specs are written in seconds (animation-plan.md 4).
import { Easing, interpolate, spring } from "remotion";

export const FPS = 30;
export const s = (sec: number) => Math.round(sec * FPS);

// The app's eases (web/app/globals.css --ease-out / --ease-in / --ease-spring)
export const easeOut = Easing.bezier(0.22, 1, 0.36, 1);
export const easeIn = Easing.bezier(0.55, 0, 1, 0.45);
export const easeSpring = Easing.bezier(0.34, 1.36, 0.64, 1);

const clamp = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const;

/** 0→1 between two times (seconds), eased. */
export function prog(frame: number, startSec: number, durSec = 0.35, easing = easeOut) {
  return interpolate(frame, [s(startSec), s(startSec) + Math.max(1, s(durSec))], [0, 1], { ...clamp, easing });
}

/** Fade in at `inSec`, optionally out at `outSec`. */
export function fade(frame: number, inSec: number, outSec?: number, dur = 0.3) {
  const a = prog(frame, inSec, dur);
  const b = outSec === undefined ? 0 : prog(frame, outSec, dur, easeIn);
  return a * (1 - b);
}

/** Spring snap (0→~1.05→1) starting at a time. */
export function snap(frame: number, fps: number, startSec: number, damping = 14) {
  return spring({ frame: frame - s(startSec), fps, config: { damping, stiffness: 180, mass: 0.7 } });
}

export const lerp = (a: number, b: number, t: number) => a + (b - a) * t;

/** Count a number up/down between two times, formatted with fixed decimals. */
export function tick(frame: number, from: number, to: number, startSec: number, durSec = 0.6, dp = 2) {
  return lerp(from, to, prog(frame, startSec, durSec)).toFixed(dp);
}

/** Typewriter: how many characters of `text` show. */
export function typed(frame: number, text: string, startSec: number, cps = 40) {
  const n = Math.max(0, Math.floor(((frame - s(startSec)) / FPS) * cps));
  return text.slice(0, n);
}
