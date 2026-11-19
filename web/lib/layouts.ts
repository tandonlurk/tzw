// Terminal (Advanced mode) panel layouts: model, presets, edits and browser persistence. Pure except load/save.

export type PanelMetric =
  | "pnl" // cumulative realized P&L, many agents + portfolio on one chart
  | "agent_pnl" // one agent's cumulative realized P&L
  | "pnl_by_agent" // realized P&L per agent (bars)
  | "allocation" // capital allocation per agent (available vs committed)
  | "outcomes" // challenge outcomes
  | "record" // wins / losses per agent
  | "agents" // agent table
  | "activity"; // recent activity

export const METRICS: { id: PanelMetric; label: string; hint: string }[] = [
  { id: "pnl", label: "P&L over time", hint: "Cumulative realized P&L, agents and portfolio on one chart" },
  { id: "agent_pnl", label: "Single agent P&L", hint: "One agent's cumulative realized P&L" },
  { id: "pnl_by_agent", label: "P&L by agent", hint: "Realized P&L per agent" },
  { id: "allocation", label: "Capital allocation", hint: "Available and committed funds per agent" },
  { id: "outcomes", label: "Outcomes", hint: "Settled, voided, cancelled and open challenges" },
  { id: "record", label: "Win / loss record", hint: "Settled wins and losses per agent" },
  { id: "agents", label: "Agents table", hint: "Balances, P&L and status per agent" },
  { id: "activity", label: "Recent activity", hint: "Latest events across agents" },
];

export const GRID_COLS = 4;
export const MAX_ROWS = 3;

export interface Panel {
  id: string;
  metric: PanelMetric;
  w: number; // 1..GRID_COLS columns
  h: number; // 1..MAX_ROWS row units
  /** agent_pnl: the agent shown. null = the n-th agent by |P&L| (n = slot), so presets work for any agent set. */
  agentId?: string | null;
  slot?: number;
}

export interface Layout { id: string; name: string; panels: Panel[]; builtin?: boolean }

let seq = 0;
export const panelId = () => `p${Date.now().toString(36)}${(seq++).toString(36)}`;

const P = (id: string, metric: PanelMetric, w: number, h: number, extra: Partial<Panel> = {}): Panel => ({ id, metric, w, h, ...extra });

export const PRESETS: Layout[] = [
  {
    id: "preset-focus",
    name: "Focus",
    builtin: true,
    panels: [P("f1", "pnl", 4, 2), P("f2", "agents", 4, 1)],
  },
  {
    id: "preset-overview",
    name: "Overview",
    builtin: true,
    panels: [P("o1", "pnl", 3, 2), P("o2", "allocation", 1, 1), P("o3", "outcomes", 1, 1), P("o4", "pnl_by_agent", 2, 1), P("o5", "activity", 2, 1)],
  },
  {
    id: "preset-grid",
    name: "Compare 2×2",
    builtin: true,
    panels: [0, 1, 2, 3].map((n) => P(`g${n}`, "agent_pnl", 2, 1, { agentId: null, slot: n })),
  },
  {
    id: "preset-risk",
    name: "Risk",
    builtin: true,
    panels: [P("r1", "allocation", 2, 1), P("r2", "record", 2, 1), P("r3", "pnl_by_agent", 2, 1), P("r4", "outcomes", 2, 1)],
  },
];

const clamp = (v: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, Math.round(v)));

export function normalizePanel(p: Panel): Panel {
  return { ...p, w: clamp(p.w, 1, GRID_COLS), h: clamp(p.h, 1, MAX_ROWS) };
}

export function addPanel(panels: Panel[], metric: PanelMetric): Panel[] {
  const big = metric === "pnl" || metric === "agents";
  return [...panels, { id: panelId(), metric, w: big ? 4 : 2, h: 1, ...(metric === "agent_pnl" ? { agentId: null, slot: 0 } : {}) }];
}

export function removePanel(panels: Panel[], id: string): Panel[] {
  return panels.filter((p) => p.id !== id);
}

export function updatePanel(panels: Panel[], id: string, patch: Partial<Panel>): Panel[] {
  return panels.map((p) => (p.id === id ? normalizePanel({ ...p, ...patch, id: p.id }) : p));
}

/** Move panel `id` to the position currently held by `beforeId` (drag-and-drop reorder). */
export function movePanel(panels: Panel[], id: string, beforeId: string): Panel[] {
  if (id === beforeId) return panels;
  const from = panels.findIndex((p) => p.id === id);
  const to = panels.findIndex((p) => p.id === beforeId);
  if (from < 0 || to < 0) return panels;
  const next = [...panels];
  const [moved] = next.splice(from, 1);
  next.splice(to, 0, moved);
  return next;
}

/** Validate untrusted JSON from storage into panels; drops anything malformed. */
export function parsePanels(raw: unknown): Panel[] | null {
  if (!Array.isArray(raw)) return null;
  const ids = new Set(METRICS.map((m) => m.id));
  const out: Panel[] = [];
  for (const r of raw) {
    if (!r || typeof r !== "object") continue;
    const o = r as Record<string, unknown>;
    if (typeof o.id !== "string" || typeof o.metric !== "string" || !ids.has(o.metric as PanelMetric)) continue;
    out.push(normalizePanel({
      id: o.id,
      metric: o.metric as PanelMetric,
      w: typeof o.w === "number" ? o.w : 2,
      h: typeof o.h === "number" ? o.h : 1,
      agentId: typeof o.agentId === "string" ? o.agentId : null,
      slot: typeof o.slot === "number" ? o.slot : undefined,
    }));
  }
  return out;
}

// ---- Persistence (browser only). Layouts are UI preferences, never financial data.

const KEY = "tzw.terminal.v1";

export interface TerminalState {
  current: Panel[];
  activeId: string | null; // preset or saved layout the current panels came from
  saved: Layout[];
}

export function defaultTerminalState(): TerminalState {
  return { current: PRESETS[1].panels.map((p) => ({ ...p })), activeId: PRESETS[1].id, saved: [] };
}

export function loadTerminal(): TerminalState {
  if (typeof window === "undefined") return defaultTerminalState();
  try {
    const raw = JSON.parse(window.localStorage.getItem(KEY) ?? "null") as Record<string, unknown> | null;
    if (!raw) return defaultTerminalState();
    const current = parsePanels(raw.current) ?? defaultTerminalState().current;
    const saved = Array.isArray(raw.saved)
      ? (raw.saved as Record<string, unknown>[]).flatMap((l) => {
          const panels = parsePanels(l?.panels);
          return panels && typeof l.id === "string" && typeof l.name === "string" ? [{ id: l.id, name: l.name, panels }] : [];
        })
      : [];
    return { current, activeId: typeof raw.activeId === "string" ? raw.activeId : null, saved };
  } catch {
    return defaultTerminalState();
  }
}

export function saveTerminal(s: TerminalState): void {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(KEY, JSON.stringify(s));
  } catch {
    /* storage full or blocked: layout just is not remembered */
  }
}

export function clearTerminal(): void {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.removeItem(KEY);
  } catch {
    /* ignore */
  }
}
