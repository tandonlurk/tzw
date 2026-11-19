// How blocks look on the logic canvas (UI brief 8): family -> shape + colour, readable param summaries,
// and which blocks are reachable from a trigger. Pure.

export type Family = "trigger" | "reasoning" | "condition" | "action" | "financial" | "data" | "code" | "end";
export type Shape = "circle" | "rounded" | "diamond" | "hexagon" | "octagon" | "parallelogram" | "capsule" | "square";

export const FAMILIES: Record<Family, { label: string; shape: Shape; color: string; blurb: string }> = {
  trigger: { label: "Trigger", shape: "circle", color: "var(--blue)", blurb: "Starts the workflow" },
  reasoning: { label: "AI reasoning", shape: "rounded", color: "var(--lavender)", blurb: "The agent thinks" },
  condition: { label: "Condition", shape: "diamond", color: "var(--yellow)", blurb: "Yes / no decision" },
  action: { label: "Action", shape: "hexagon", color: "var(--mint)", blurb: "Does something" },
  financial: { label: "Financial", shape: "octagon", color: "var(--rose)", blurb: "Touches funds or approvals" },
  data: { label: "Market & approach", shape: "parallelogram", color: "var(--steel)", blurb: "What it watches" },
  code: { label: "Custom code", shape: "square", color: "var(--offwhite)", blurb: "Your JavaScript" },
  end: { label: "End", shape: "capsule", color: "var(--offwhite)", blurb: "Stops here" },
};

const FINANCIAL = new Set(["action.request_approval", "cond.stake_lte", "cond.exposure_within"]);

export function familyOf(type: string, kind?: string): Family {
  if (type === "custom.code") return "code";
  if (type === "approach.profile" || kind === "approach") return "data";
  if (type === "action.request_approval") return "financial";
  if (type === "action.reject") return "end";
  if (kind === "trigger" || type.startsWith("trigger.")) return "trigger";
  if (kind === "reasoning" || type.startsWith("reason.")) return "reasoning";
  if (kind === "condition" || type.startsWith("cond.")) return "condition";
  if (kind === "action" || type.startsWith("action.")) return "action";
  return "action";
}

/** Financial checks are conditions, but they guard money: flagged so the panel can say so. */
export const guardsFunds = (type: string) => FINANCIAL.has(type);

/** SVG path for a shape inside a 64x64 box (with a 2px inset for the stroke). */
export function shapePath(shape: Shape, w = 64, h = 64): string {
  const p = 2;
  const W = w - p;
  const H = h - p;
  const cx = w / 2;
  const cy = h / 2;
  switch (shape) {
    case "circle": {
      const r = Math.min(w, h) / 2 - p;
      return `M${cx - r},${cy}a${r},${r} 0 1,0 ${2 * r},0a${r},${r} 0 1,0 ${-2 * r},0`;
    }
    case "diamond":
      return `M${cx},${p}L${W},${cy}L${cx},${H}L${p},${cy}Z`;
    case "hexagon":
      return `M${p + 14},${p + 4}L${W - 14},${p + 4}L${W},${cy}L${W - 14},${H - 4}L${p + 14},${H - 4}L${p},${cy}Z`;
    case "octagon": {
      const c = 17;
      return `M${p + c},${p}L${W - c},${p}L${W},${p + c}L${W},${H - c}L${W - c},${H}L${p + c},${H}L${p},${H - c}L${p},${p + c}Z`;
    }
    case "parallelogram":
      return `M${p + 12},${p + 8}L${W},${p + 8}L${W - 12},${H - 8}L${p},${H - 8}Z`;
    case "capsule":
      return `M${p + 14},${p + 12}L${W - 14},${p + 12}A14,14 0 0 1 ${W - 14},${H - 12}L${p + 14},${H - 12}A14,14 0 0 1 ${p + 14},${p + 12}Z`;
    case "square":
      return `M${p + 6},${p}L${W - 6},${p}Q${W},${p} ${W},${p + 6}L${W},${H - 6}Q${W},${H} ${W - 6},${H}L${p + 6},${H}Q${p},${H} ${p},${H - 6}L${p},${p + 6}Q${p},${p} ${p + 6},${p}Z`;
    case "rounded":
    default:
      return `M${p + 16},${p}L${W - 16},${p}Q${W},${p} ${W},${p + 16}L${W},${H - 16}Q${W},${H} ${W - 16},${H}L${p + 16},${H}Q${p},${H} ${p},${H - 16}L${p},${p + 16}Q${p},${p} ${p + 16},${p}Z`;
  }
}

const isMinorMoney = (k: string, v: unknown) => typeof v === "string" && /^\d{4,}$/.test(v) && /(stake|amount|max|min|limit|threshold)/i.test(k);

/** One short line describing a block's params, e.g. "≥ 60%" or "BTC · 2.00%". Empty when nothing useful. */
export function paramSummary(type: string, params: Record<string, unknown>): string {
  if (type === "custom.code") {
    const src = typeof params.source === "string" ? params.source : "";
    const lines = src.split("\n").filter((l) => l.trim()).length;
    return lines ? `${lines} lines of JS` : "empty";
  }
  if (type === "approach.profile") return "stance, markets, ranges";
  const parts: string[] = [];
  for (const [k, v] of Object.entries(params)) {
    if (v === undefined || v === null || v === "") continue;
    let text: string;
    if (k.endsWith("_bps") && typeof v === "number") text = `${k.startsWith("min") ? "≥ " : ""}${(v / 100).toFixed(v % 100 ? 2 : 0)}%`;
    else if (isMinorMoney(k, v)) text = `${(Number(v) / 1e6).toFixed(2)} USDC`;
    else if (typeof v === "number") text = k.endsWith("_secs") ? `${v >= 3600 ? `${v / 3600}h` : v >= 60 ? `${v / 60}m` : `${v}s`}` : String(v);
    else if (typeof v === "boolean") text = v ? k.replace(/_/g, " ") : "";
    else if (typeof v === "string") text = v;
    else if (Array.isArray(v)) text = v.slice(0, 2).join(", ") + (v.length > 2 ? "…" : "");
    else continue;
    if (text) parts.push(text);
    if (parts.join(" · ").length > 26) break;
  }
  const s = parts.join(" · ");
  return s.length > 30 ? `${s.slice(0, 29)}…` : s;
}

/** Ids of nodes reachable from any trigger-family node by following edges. */
export function reachableFromTriggers(nodes: { id: string; type: string }[], edges: { source: string; target: string }[]): Set<string> {
  const out = new Map<string, string[]>();
  for (const e of edges) (out.get(e.source) ?? out.set(e.source, []).get(e.source)!).push(e.target);
  const seen = new Set<string>();
  const stack = nodes.filter((n) => familyOf(n.type) === "trigger").map((n) => n.id);
  while (stack.length) {
    const id = stack.pop()!;
    if (seen.has(id)) continue;
    seen.add(id);
    for (const t of out.get(id) ?? []) stack.push(t);
  }
  return seen;
}

/** Human port label: condition outputs "true"/"false" read as yes/no. */
export function portLabel(port: string): string {
  if (port === "true") return "yes";
  if (port === "false") return "no";
  return port === "in" || port === "out" ? "" : port.replace(/_/g, " ");
}
