import type { AgentConfig, AgentPolicy, AgentVersion, Graph, Interaction } from "./api";
import { formatMoney } from "./money";
import { isOpen } from "./interactions";

/** Counts shown on the agent detail page. Display only: derived from the interactions list the API returned. */
export function interactionCounts(list: Interaction[]): { active: number; past: number; total: number } {
  const active = list.filter((i) => isOpen(i.state)).length;
  return { active, past: list.length - active, total: list.length };
}

export function winRateText(wins: number, settled: number): string {
  if (settled <= 0) return "No settled interactions";
  return `${wins} of ${settled} won (${((wins * 100) / settled).toFixed(0)}%)`;
}

const CONFIG_LABELS: Record<keyof AgentConfig, string> = {
  goals: "Goals",
  strategy: "Strategy",
  categories: "Categories",
  horizon_secs: "Horizon",
  risk_tolerance: "Risk tolerance",
  schedule_secs: "Schedule",
};

const POLICY_LABELS: Record<keyof AgentPolicy, string> = {
  max_per_interaction: "Max per interaction",
  max_aggregate_exposure: "Max aggregate exposure",
  approval_threshold: "Approval above",
  allowed_interaction_types: "Allowed types",
  min_conviction_bps: "Min conviction",
  stop_loss: "Stop loss",
};

const MONEY_KEYS = new Set<string>(["max_per_interaction", "max_aggregate_exposure", "approval_threshold", "stop_loss"]);

function show(key: string, v: unknown): string {
  if (v === null || v === undefined) return "none";
  if (MONEY_KEYS.has(key) && typeof v === "string") return formatMoney(v);
  if (Array.isArray(v)) return v.length ? v.join(", ") : "none";
  if (typeof v === "string") return v === "" ? "(empty)" : v;
  return String(v);
}

function graphSig(g: Graph): string {
  return JSON.stringify(g);
}

/** Human-readable list of what changed between two versions. */
export function diffVersions(prev: AgentVersion | null, next: AgentVersion): string[] {
  if (!prev) return [];
  const out: string[] = [];
  for (const k of Object.keys(CONFIG_LABELS) as (keyof AgentConfig)[]) {
    if (JSON.stringify(prev.config[k]) !== JSON.stringify(next.config[k])) {
      out.push(`${CONFIG_LABELS[k]}: ${show(k, prev.config[k])} -> ${show(k, next.config[k])}`);
    }
  }
  for (const k of Object.keys(POLICY_LABELS) as (keyof AgentPolicy)[]) {
    if (JSON.stringify(prev.policy[k]) !== JSON.stringify(next.policy[k])) {
      out.push(`${POLICY_LABELS[k]}: ${show(k, prev.policy[k])} -> ${show(k, next.policy[k])}`);
    }
  }
  if (graphSig(prev.graph) !== graphSig(next.graph)) out.push("Behavior graph changed");
  return out;
}

/** Versions newest first, each with its diff against the one before it. */
export function versionHistory(versions: AgentVersion[]): { v: AgentVersion; changes: string[] }[] {
  const asc = [...versions].sort((a, b) => a.version - b.version);
  return asc.map((v, i) => ({ v, changes: diffVersions(i > 0 ? asc[i - 1] : null, v) })).reverse();
}
