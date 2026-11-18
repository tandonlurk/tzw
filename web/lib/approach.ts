// Pure helpers for the `approach.profile` block (docs/design/approach-and-firms.md section 1). No React here.
// The formulas mirror the backend's for display only: the backend computes and enforces the real numbers.

import type {
  ApproachMarkets,
  ApproachParams,
  ApproachRanges,
  Bound,
  FirmRole,
  Graph,
  GraphNode,
  MarketScope,
  Money,
} from "./api";

export const APPROACH_BLOCK_TYPE = "approach.profile";
export const BPS = 10000;

/** Section 1 thresholds on collaboration_bps. */
export const JOIN_MIN_BPS = 3000;
export const INITIATE_MAX_BPS = 7000;

export const FIRM_ROLES: FirmRole[] = ["none", "member", "lead"];
export const STRUCTURES = ["binary_up_down", "price_ladder", "categorical"] as const;

export function defaultRanges(): ApproachRanges {
  return {
    move_bps: { min: null, max: null },
    yes_price_bps: { min: null, max: null },
    expiry_secs: { min: null, max: null },
    volume_delta_24h_bps: { min: null, max: null },
  };
}

export function defaultMarkets(): ApproachMarkets {
  return { scope: "any", structures: [], categories: [], subcategories: [], keywords: [], symbols: [], ranges: defaultRanges() };
}

export function defaultApproach(): ApproachParams {
  return { stance: { collaboration_bps: 3000, ambition_bps: 5000 }, firm_role: "none", markets: defaultMarkets() };
}

// ---- Deterministic effects (section 1 table) ----

const clampBps = (n: number) => Math.min(BPS, Math.max(0, Math.trunc(n)));

/** stake target = max_per_interaction × (2000 + 8 × a / 10) / 10000, integer division throughout. */
export function stakeTarget(maxPerInteraction: Money, ambitionBps: number): Money | null {
  let max: bigint;
  try {
    max = BigInt(maxPerInteraction);
  } catch {
    return null;
  }
  const a = BigInt(clampBps(ambitionBps));
  return ((max * (BigInt(2000) + (BigInt(8) * a) / BigInt(10))) / BigInt(BPS)).toString();
}

/** Stake target as a share of max_per_interaction, in bps (20 % cautious → 100 % ambitious). */
export function stakeTargetBps(ambitionBps: number): number {
  return 2000 + Math.floor((8 * clampBps(ambitionBps)) / 10);
}

/** Default minimum conviction = 7500 − a / 4 bps (75 % → 50 %). */
export function minConvictionBps(ambitionBps: number): number {
  return 7500 - Math.floor(clampBps(ambitionBps) / 4);
}

/** Max open interactions = 1 + 4 × a / 10000 (1 → 5). */
export function maxOpenInteractions(ambitionBps: number): number {
  return 1 + Math.floor((4 * clampBps(ambitionBps)) / BPS);
}

export interface StanceEffects {
  initiatesChallenges: boolean;
  joinsFirms: boolean;
  canLead: boolean;
  /** A firm role is picked but collaboration is too low for it to take effect. */
  roleInactive: boolean;
}

export function stanceEffects(collaborationBps: number, role: FirmRole): StanceEffects {
  const c = clampBps(collaborationBps);
  const collaborative = c >= JOIN_MIN_BPS;
  return {
    initiatesChallenges: c <= INITIATE_MAX_BPS,
    joinsFirms: collaborative && role === "member",
    canLead: collaborative && role === "lead",
    roleInactive: role !== "none" && !collaborative,
  };
}

export function stanceZone(collaborationBps: number): "competitor" | "both" | "collaborator" {
  const c = clampBps(collaborationBps);
  if (c < JOIN_MIN_BPS) return "competitor";
  if (c > INITIATE_MAX_BPS) return "collaborator";
  return "both";
}

export function ambitionLabel(ambitionBps: number): string {
  const a = clampBps(ambitionBps);
  return a < 3400 ? "Cautious" : a <= 6600 ? "Measured" : "Ambitious";
}

export function firmRoleLabel(r: FirmRole): string {
  return r === "member" ? "Join firms" : r === "lead" ? "Lead a firm" : "No firm role";
}

// ---- Stance wheel geometry: upper half-disc. angle: left = competitive (0), right = collaborative (10000); radius: centre = cautious (0), edge = ambitious (10000). ----

export const STANCE_STEP = 100;

const snap = (n: number, step: number) => clampBps(Math.round(n / step) * step);

/** Point for a stance, relative to the wheel centre, y pointing UP, for radius `R`. */
export function stanceToPoint(c: number, a: number, R: number): { x: number; y: number } {
  const t = Math.PI * (1 - clampBps(c) / BPS);
  const r = (clampBps(a) / BPS) * R;
  return { x: r * Math.cos(t), y: r * Math.sin(t) };
}

/**
 * Stance for a pointer position relative to the centre (y UP). Points below the baseline clamp onto it.
 * Near the centre the angle is meaningless, so `prevC` is kept. Values snap to `step` bps.
 */
export function pointToStance(x: number, y: number, R: number, prevC: number, step = STANCE_STEP): { collaboration_bps: number; ambition_bps: number } {
  const yy = Math.max(0, y);
  const dist = Math.hypot(x, yy);
  const a = snap((Math.min(dist, R) / R) * BPS, step);
  if (dist < R * 0.04) return { collaboration_bps: clampBps(prevC), ambition_bps: a };
  const t = Math.atan2(yy, x); // 0 (right) .. π (left)
  return { collaboration_bps: snap((1 - t / Math.PI) * BPS, step), ambition_bps: a };
}

/** Keyboard control: ←/→ collaboration, ↑/↓ ambition, 500 bps per press (100 with Shift), Home/End jump to the ends. */
export function nudgeStance(
  s: { collaboration_bps: number; ambition_bps: number },
  key: string,
  shift = false,
): { collaboration_bps: number; ambition_bps: number } | null {
  const d = shift ? 100 : 500;
  const c = s.collaboration_bps;
  const a = s.ambition_bps;
  switch (key) {
    case "ArrowLeft": return { collaboration_bps: clampBps(c - d), ambition_bps: a };
    case "ArrowRight": return { collaboration_bps: clampBps(c + d), ambition_bps: a };
    case "ArrowUp": return { collaboration_bps: c, ambition_bps: clampBps(a + d) };
    case "ArrowDown": return { collaboration_bps: c, ambition_bps: clampBps(a - d) };
    case "Home": return { collaboration_bps: 0, ambition_bps: a };
    case "End": return { collaboration_bps: BPS, ambition_bps: a };
    default: return null;
  }
}

// ---- Market filters ----

const uniqCI = (xs: string[]): string[] => {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const raw of xs) {
    const x = raw.trim();
    if (!x || seen.has(x.toLowerCase())) continue;
    seen.add(x.toLowerCase());
    out.push(x);
  }
  return out;
};

export function hasRangeBounds(r: ApproachRanges): boolean {
  return Object.values(r).some((b: Bound) => b.min !== null || b.max !== null);
}

/** UI hint (the filters are what count): named markets → specific; only structure/ranges → structure; nothing → any. */
export function deriveScope(m: Omit<ApproachMarkets, "scope">): MarketScope {
  if (m.categories.length || m.subcategories.length || m.keywords.length || m.symbols.length) return "specific";
  if (m.structures.length || hasRangeBounds(m.ranges)) return "structure";
  return "any";
}

/** Canonical form sent to the backend: trimmed, de-duplicated (case-insensitive), symbols upper-case, scope derived. */
export function serializeMarkets(m: ApproachMarkets): ApproachMarkets {
  const out = {
    structures: uniqCI(m.structures),
    categories: uniqCI(m.categories),
    subcategories: uniqCI(m.subcategories),
    keywords: uniqCI(m.keywords.map((k) => k.toLowerCase())),
    symbols: uniqCI(m.symbols.map((s) => s.toUpperCase())),
    ranges: {
      move_bps: { ...m.ranges.move_bps },
      yes_price_bps: { ...m.ranges.yes_price_bps },
      expiry_secs: { ...m.ranges.expiry_secs },
      volume_delta_24h_bps: { ...m.ranges.volume_delta_24h_bps },
    },
  };
  return { scope: deriveScope(out), ...out };
}

/** Toggle `v` in `list`, case-insensitively. */
export function toggleIn(list: string[], v: string): string[] {
  const has = list.some((x) => x.toLowerCase() === v.toLowerCase());
  return has ? list.filter((x) => x.toLowerCase() !== v.toLowerCase()) : [...list, v];
}

export const includesCI = (list: string[], v: string) => list.some((x) => x.toLowerCase() === v.toLowerCase());

// ---- Ranges: display units and validation ----

export type RangeKey = keyof ApproachRanges;

export interface RangeSpec {
  key: RangeKey;
  label: string;
  hint: string;
  unit: "%" | "h";
  /** Slider bounds in stored units (bps or seconds). Typed values may go past them. */
  slider: { min: number; max: number; step: number };
  /** Hard limits in stored units; null = none. */
  limit: { min: number | null; max: number | null };
}

export const RANGE_SPECS: RangeSpec[] = [
  { key: "move_bps", label: "Move (delta)", hint: "Size of the price move over the agent's window, either direction", unit: "%",
    slider: { min: 0, max: 2000, step: 25 }, limit: { min: 0, max: null } },
  { key: "yes_price_bps", label: "YES price", hint: "Prediction contract YES price (50% = a coin flip)", unit: "%",
    slider: { min: 0, max: 10000, step: 100 }, limit: { min: 0, max: BPS } },
  { key: "expiry_secs", label: "Time to expiry", hint: "From now, in hours", unit: "h",
    slider: { min: 0, max: 30 * 24 * 3600, step: 3600 }, limit: { min: 0, max: null } },
  { key: "volume_delta_24h_bps", label: "Volume change 24h", hint: "Gemini 24h volume change; negative = falling volume", unit: "%",
    slider: { min: -10000, max: 50000, step: 500 }, limit: { min: null, max: null } },
];

/** bps → percent text, e.g. 350 → "3.5", -125 → "-1.25". */
export function bpsToPercentText(bps: number): string {
  const neg = bps < 0;
  const abs = Math.abs(Math.trunc(bps));
  const whole = Math.floor(abs / 100);
  const frac = String(abs % 100).padStart(2, "0").replace(/0+$/, "");
  return `${neg ? "-" : ""}${whole}${frac ? `.${frac}` : ""}`;
}

/** Percent text (up to 2 decimals, optional sign) → integer bps without float error, or null. */
export function percentTextToBps(text: string): number | null {
  const t = text.trim();
  const m = /^(-)?(\d{1,7})(?:\.(\d{1,2}))?$/.exec(t);
  if (!m) return null;
  const v = Number(m[2]) * 100 + Number((m[3] ?? "").padEnd(2, "0"));
  return m[1] && v !== 0 ? -v : v;
}

export function secsToHoursText(secs: number): string {
  const h = secs / 3600;
  return Number.isInteger(h) ? String(h) : h.toFixed(2).replace(/0+$/, "").replace(/\.$/, "");
}

/** Hours text (up to 2 decimals) → whole seconds, or null. */
export function hoursTextToSecs(text: string): number | null {
  const m = /^(\d{1,6})(?:\.(\d{1,2}))?$/.exec(text.trim());
  if (!m) return null;
  return Number(m[1]) * 3600 + Math.round((Number((m[2] ?? "").padEnd(2, "0")) * 3600) / 100);
}

export function toDisplay(spec: RangeSpec, v: number | null): string {
  if (v === null) return "";
  return spec.unit === "h" ? secsToHoursText(v) : bpsToPercentText(v);
}

export function fromDisplay(spec: RangeSpec, text: string): number | null {
  return spec.unit === "h" ? hoursTextToSecs(text) : percentTextToBps(text);
}

export function formatRangeValue(spec: RangeSpec, v: number): string {
  return `${toDisplay(spec, v)}${spec.unit === "h" ? " h" : "%"}`;
}

/** Local checks (convenience only; the backend validates): integers, within limits, min ≤ max. Keyed by "ranges.<key>". */
export function rangeErrors(r: ApproachRanges): Record<string, string> {
  const out: Record<string, string> = {};
  for (const spec of RANGE_SPECS) {
    const b = r[spec.key];
    const msgs: string[] = [];
    for (const v of [b.min, b.max]) {
      if (v === null) continue;
      if (!Number.isSafeInteger(v)) msgs.push("must be a whole number");
      else if (spec.limit.min !== null && v < spec.limit.min) msgs.push(`must be at least ${formatRangeValue(spec, spec.limit.min)}`);
      else if (spec.limit.max !== null && v > spec.limit.max) msgs.push(`must be at most ${formatRangeValue(spec, spec.limit.max)}`);
    }
    if (b.min !== null && b.max !== null && b.min > b.max) msgs.push("min must not exceed max");
    if (msgs.length) out[`ranges.${spec.key}`] = [...new Set(msgs)].join("; ");
  }
  return out;
}

export function rangeText(spec: RangeSpec, b: Bound): string | null {
  if (b.min === null && b.max === null) return null;
  if (b.min !== null && b.max !== null) return `${spec.label} ${formatRangeValue(spec, b.min)} to ${formatRangeValue(spec, b.max)}`;
  return b.min !== null ? `${spec.label} ≥ ${formatRangeValue(spec, b.min)}` : `${spec.label} ≤ ${formatRangeValue(spec, b.max!)}`;
}

/** One-line description of a market filter for cards and summaries. */
export function marketsSummary(m: ApproachMarkets): string {
  const s = serializeMarkets(m);
  if (s.scope === "any") return "Any market";
  const parts: string[] = [];
  if (s.categories.length) parts.push(s.categories.join(", "));
  if (s.subcategories.length) parts.push(`sub: ${s.subcategories.join(", ")}`);
  if (s.keywords.length) parts.push(`keywords: ${s.keywords.join(", ")}`);
  if (s.symbols.length) parts.push(`symbols: ${s.symbols.join(", ")}`);
  if (s.structures.length) parts.push(`structure: ${s.structures.map(structureLabel).join(", ")}`);
  for (const spec of RANGE_SPECS) {
    const t = rangeText(spec, s.ranges[spec.key]);
    if (t) parts.push(t);
  }
  return parts.join("; ");
}

export function structureLabel(s: string): string {
  return s === "binary_up_down" ? "Up / down" : s === "price_ladder" ? "Price ladder" : s === "categorical" ? "Categorical" : s.replace(/_/g, " ");
}

// ---- Parsing loose params (old graphs, hand-edited JSON) into the exact schema ----

const intOrNull = (v: unknown): number | null => (typeof v === "number" && Number.isSafeInteger(v) ? v : null);
const strList = (v: unknown): string[] => (Array.isArray(v) ? v.filter((x): x is string => typeof x === "string") : []);
const obj = (v: unknown): Record<string, unknown> => (v && typeof v === "object" && !Array.isArray(v) ? (v as Record<string, unknown>) : {});

export function normalizeMarkets(raw: unknown): ApproachMarkets {
  const m = obj(raw);
  const r = obj(m.ranges);
  const bound = (v: unknown): Bound => ({ min: intOrNull(obj(v).min), max: intOrNull(obj(v).max) });
  const scope = m.scope === "structure" || m.scope === "specific" ? m.scope : "any";
  return {
    scope,
    structures: strList(m.structures),
    categories: strList(m.categories),
    subcategories: strList(m.subcategories),
    keywords: strList(m.keywords),
    symbols: strList(m.symbols),
    ranges: {
      move_bps: bound(r.move_bps),
      yes_price_bps: bound(r.yes_price_bps),
      expiry_secs: bound(r.expiry_secs),
      volume_delta_24h_bps: bound(r.volume_delta_24h_bps),
    },
  };
}

export function normalizeApproach(raw: unknown): ApproachParams {
  const p = obj(raw);
  const st = obj(p.stance);
  const d = defaultApproach();
  const c = intOrNull(st.collaboration_bps);
  const a = intOrNull(st.ambition_bps);
  return {
    stance: { collaboration_bps: c === null ? d.stance.collaboration_bps : clampBps(c), ambition_bps: a === null ? d.stance.ambition_bps : clampBps(a) },
    firm_role: p.firm_role === "member" || p.firm_role === "lead" ? p.firm_role : "none",
    markets: p.markets === undefined ? d.markets : normalizeMarkets(p.markets),
  };
}

/** Exact section 1 shape for the graph node's params. */
export function approachToParams(p: ApproachParams): Record<string, unknown> {
  return {
    stance: { collaboration_bps: clampBps(p.stance.collaboration_bps), ambition_bps: clampBps(p.stance.ambition_bps) },
    firm_role: p.firm_role,
    markets: serializeMarkets(p.markets),
  };
}

// ---- Graph helpers ----

export function approachNodeOf(graph: Graph): GraphNode | undefined {
  return graph.nodes.find((n) => n.type === APPROACH_BLOCK_TYPE);
}

/** The agent's approach; defaults when the graph has no approach node (section 1: absent = defaults). */
export function approachOf(graph: Graph): ApproachParams {
  const n = approachNodeOf(graph);
  return n ? normalizeApproach(n.params) : defaultApproach();
}

export function firmRoleOf(graph: Graph): FirmRole {
  return approachOf(graph).firm_role;
}

/** Return `graph` with exactly one approach node: the existing one kept, else a default one added. */
export function ensureApproachNode(graph: Graph): Graph {
  if (approachNodeOf(graph)) return graph;
  const ids = new Set(graph.nodes.map((n) => n.id));
  let id = "approach";
  for (let i = 2; ids.has(id); i++) id = `approach_${i}`;
  const minY = graph.nodes.reduce((y, n) => Math.min(y, n.ui?.y ?? 0), 0);
  return {
    ...graph,
    nodes: [{ id, type: APPROACH_BLOCK_TYPE, params: approachToParams(defaultApproach()), ui: { x: 0, y: minY - 200 } }, ...graph.nodes],
  };
}
