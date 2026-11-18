// SVG geometry for the wheels. Angles are radians, clockwise from 12 o'clock. No React here.

export interface Pt { x: number; y: number }

export function polar(cx: number, cy: number, r: number, a: number): Pt {
  return { x: cx + r * Math.sin(a), y: cy - r * Math.cos(a) };
}

const f = (n: number) => Number(n.toFixed(2));

/** Annular sector between radii r0 < r1 and angles a0 < a1. A full turn is drawn as a ring. */
export function arcPath(cx: number, cy: number, r0: number, r1: number, a0: number, a1: number): string {
  const span = a1 - a0;
  if (span >= Math.PI * 2 - 1e-6) {
    // two half-arcs per circle; evenodd fill leaves the hole
    const ring = (r: number) => `M ${f(cx)} ${f(cy - r)} A ${r} ${r} 0 1 1 ${f(cx)} ${f(cy + r)} A ${r} ${r} 0 1 1 ${f(cx)} ${f(cy - r)} Z`;
    return r0 > 0 ? `${ring(r1)} ${ring(r0)}` : ring(r1);
  }
  const large = span > Math.PI ? 1 : 0;
  const p0 = polar(cx, cy, r1, a0);
  const p1 = polar(cx, cy, r1, a1);
  const p2 = polar(cx, cy, r0, a1);
  const p3 = polar(cx, cy, r0, a0);
  return [
    `M ${f(p0.x)} ${f(p0.y)}`,
    `A ${r1} ${r1} 0 ${large} 1 ${f(p1.x)} ${f(p1.y)}`,
    `L ${f(p2.x)} ${f(p2.y)}`,
    r0 > 0 ? `A ${r0} ${r0} 0 ${large} 0 ${f(p3.x)} ${f(p3.y)}` : "",
    "Z",
  ].filter(Boolean).join(" ");
}

export interface Segment {
  key: string;
  label: string;
  /** Category of a subcategory segment. */
  parent: string | null;
  a0: number;
  a1: number;
}

/**
 * Sunburst: categories share the full turn equally (inner ring); each category's subcategories
 * share its wedge equally (outer ring). Categories without subcategories leave the outer ring empty.
 */
export function sunburstLayout(categories: string[], subcategories: Record<string, string[]>): { inner: Segment[]; outer: Segment[] } {
  const inner: Segment[] = [];
  const outer: Segment[] = [];
  const n = categories.length;
  if (n === 0) return { inner, outer };
  const step = (Math.PI * 2) / n;
  categories.forEach((c, i) => {
    const a0 = i * step;
    inner.push({ key: `c:${c}`, label: c, parent: null, a0, a1: a0 + step });
    const subs = subcategoriesOf(subcategories, c);
    const sub = subs.length ? step / subs.length : 0;
    subs.forEach((s, j) => outer.push({ key: `s:${c}:${s}`, label: s, parent: c, a0: a0 + j * sub, a1: a0 + (j + 1) * sub }));
  });
  return { inner, outer };
}

/** Subcategories for a category; keys are matched case-insensitively. */
export function subcategoriesOf(subcategories: Record<string, string[]>, category: string): string[] {
  const k = Object.keys(subcategories).find((x) => x.toLowerCase() === category.toLowerCase());
  return k ? subcategories[k] : [];
}

/** Middle of a segment at radius r, for labels. */
export function segmentCentre(cx: number, cy: number, r: number, s: Segment): Pt {
  return polar(cx, cy, r, (s.a0 + s.a1) / 2);
}

/** Shorten a label to fit `max` characters. */
export function fitLabel(label: string, max: number): string {
  return label.length <= max ? label : `${label.slice(0, Math.max(1, max - 1))}…`;
}
