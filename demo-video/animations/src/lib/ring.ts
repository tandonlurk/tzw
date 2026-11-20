// Five personal agents in a ring: the closing frame of 02-problem and the opening frame of 03.
export const RING_C = { x: 960, y: 500 };
export const RING = Array.from({ length: 5 }, (_, i) => {
  const a = -Math.PI / 2 + (i * 2 * Math.PI) / 5;
  return { x: RING_C.x + Math.cos(a) * 380, y: RING_C.y + Math.sin(a) * 290, color: i + 2 };
});
export const RING_NAMES = ["Ava", "Ben", "Chen", "Dee", "Eli"];
