// Agent multiplier (D-73, docs/api.md "Agent multiplier"). Pure helpers for the
// Multiply card and the family view on My Agents. The server re-checks everything.

import type { Agent, Money } from "./api";

export const MAX_CLONES = 100;
export const MAX_VARIANCE_BPS = 2500;
export const PRESETS = [1, 10, 100] as const;

export interface SplitPreview {
  perClone: Money;
  total: Money;
  remainder: Money;
  /** Unallocated balance once the clones are funded (null when unknown). */
  after: Money | null;
  fundsOk: boolean | null;
}

/** Valid clone count, or null. */
export function parseCount(input: string): number | null {
  if (!/^\d+$/.test(input.trim())) return null;
  const n = Number(input.trim());
  return n >= 1 && n <= MAX_CLONES ? n : null;
}

/**
 * Same split as the server: the budget divided evenly, rounded down to the minor unit; the remainder stays
 * unallocated. Null when the budget cannot give every clone at least one minor unit.
 */
export function splitBudget(budget: Money, count: number, unallocated: Money | null): SplitPreview | null {
  if (!Number.isInteger(count) || count < 1) return null;
  let b: bigint;
  try {
    b = BigInt(budget);
  } catch {
    return null;
  }
  const n = BigInt(count);
  const per = b / n;
  if (per <= BigInt(0)) return null;
  const total = per * n;
  let after: Money | null = null;
  let fundsOk: boolean | null = null;
  if (unallocated !== null) {
    const u = BigInt(unallocated);
    fundsOk = u >= total;
    after = (u - total).toString();
  }
  return { perClone: per.toString(), total: total.toString(), remainder: (b - total).toString(), after, fundsOk };
}

/** "graph.nodes.trigger.params.threshold_bps" -> "Move threshold" (falls back to the last path segment). */
export function paramLabel(path: string): string {
  const last = path.split(".").pop() ?? path;
  const labels: Record<string, string> = {
    horizon_secs: "Horizon",
    min_conviction_bps: "Min conviction (policy)",
    threshold_bps: "Move threshold",
    window_secs: "Move window",
    every_secs: "Schedule",
    min_bps: "Conviction gate",
    ambition_bps: "Ambition",
  };
  return labels[last] ?? last;
}

/** Human value for a varied parameter, by its unit. */
export function paramValue(path: string, value: number): string {
  if (path.endsWith("_bps")) return `${(value / 100).toFixed(value % 100 === 0 ? 0 : 2)}%`;
  if (path.endsWith("_secs")) {
    if (value % 3600 === 0) return `${value / 3600}h`;
    if (value >= 600) return `${Math.round(value / 60)}m`;
    return `${value}s`;
  }
  return String(value);
}

export interface Family {
  id: string;
  root: Agent | null;
  clones: Agent[];
  realizedPnl: Money;
  available: Money;
  committed: Money;
  settled: number;
  wins: number;
}

/**
 * Families from the agent list: root first, clones by index. Totals cover the root and every clone (each
 * agent's own ledger-derived numbers summed; transfers between them never count as P&L).
 */
export function families(agents: Agent[]): Family[] {
  const map = new Map<string, Family>();
  for (const a of agents) {
    if (!a.family_id) continue;
    let f = map.get(a.family_id);
    if (!f) {
      f = { id: a.family_id, root: null, clones: [], realizedPnl: "0", available: "0", committed: "0", settled: 0, wins: 0 };
      map.set(a.family_id, f);
    }
    if (a.id === a.family_id) f.root = a;
    else f.clones.push(a);
    f.realizedPnl = (BigInt(f.realizedPnl) + BigInt(a.stats.realized_pnl)).toString();
    f.available = (BigInt(f.available) + BigInt(a.balances.available)).toString();
    f.committed = (BigInt(f.committed) + BigInt(a.balances.committed)).toString();
    f.settled += a.stats.settled;
    f.wins += a.stats.wins;
  }
  for (const f of map.values()) f.clones.sort((x, y) => (x.clone?.index ?? 0) - (y.clone?.index ?? 0));
  return [...map.values()];
}
