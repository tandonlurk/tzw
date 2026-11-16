import type { ApiError } from "./api";
import type { AgentPolicy, ProposedChange, Suggestion, SuggestionStatus } from "./api";
import { formatMoney } from "./money";

/** Display-only: human labels for the fields a ProposedChange may name. */
const POLICY_LABELS: Record<string, string> = {
  max_per_interaction: "Max per challenge",
  max_aggregate_exposure: "Max total exposure",
  approval_threshold: "Approval threshold",
  stop_loss: "Stop-loss",
  min_conviction_bps: "Minimum conviction",
};
const MONEY_POLICY = new Set(["max_per_interaction", "max_aggregate_exposure", "approval_threshold", "stop_loss"]);

export interface ChangeRow {
  field: string;
  label: string;
  before: string | null; // null when the current value is unknown or not loaded
  after: string;
}

function fmtPolicy(key: string, value: unknown): string {
  if (value === null || value === undefined) return "none";
  if (MONEY_POLICY.has(key)) return formatMoney(String(value));
  if (key === "min_conviction_bps") return `${Number(value) / 100}%`;
  return String(value);
}

/**
 * Rows for a readable before -> after view. `current` is the agent's current policy/risk
 * (display only; the server merges and validates the change against its own current version).
 */
export function changeRows(
  change: ProposedChange | null,
  current?: { policy?: Partial<AgentPolicy>; risk_tolerance?: string } | null,
): ChangeRow[] {
  if (!change) return [];
  const rows: ChangeRow[] = [];
  for (const [k, v] of Object.entries(change.policy ?? {})) {
    if (v === undefined) continue;
    const cur = current?.policy ? (current.policy as Record<string, unknown>)[k] : undefined;
    rows.push({
      field: `policy.${k}`,
      label: POLICY_LABELS[k] ?? k,
      before: current?.policy ? fmtPolicy(k, cur) : null,
      after: fmtPolicy(k, v),
    });
  }
  if (change.config?.risk_tolerance) {
    rows.push({
      field: "config.risk_tolerance",
      label: "Risk tolerance",
      before: current?.risk_tolerance ?? null,
      after: change.config.risk_tolerance,
    });
  }
  return rows;
}

/** Evidence values that are money (minor-unit strings), by key; everything else is shown as-is. */
const MONEY_EVIDENCE = /^(available|unallocated|stake|realized_pnl|net_allocated|approval_threshold|max_denied_stake|stop_loss|max_per_interaction|max_aggregate_exposure|balance)$/;

export function evidenceValue(key: string, value: unknown): string {
  if (MONEY_EVIDENCE.test(key) && typeof value === "string") return formatMoney(value);
  if (Array.isArray(value)) return value.map((v) => evidenceValue(key === "recent_settle_nets" ? "realized_pnl" : key, v)).join(", ");
  if (value === null || value === undefined) return "none";
  if (typeof value === "object") return JSON.stringify(value);
  return String(value);
}

export function evidenceRows(evidence: Record<string, unknown>): [string, string][] {
  return Object.entries(evidence ?? {}).map(([k, v]) => [k.replace(/_/g, " "), evidenceValue(k, v)]);
}

export const isPending = (s: Suggestion) => s.status === "pending";

export function statusTone(s: SuggestionStatus): "pos" | "neg" | "pend" | "dim" {
  return s === "accepted" ? "pos" : s === "pending" ? "pend" : "dim";
}

export function sourceLabel(src: Suggestion["source"]): string {
  return src === "grok" ? "Grok" : "Rules";
}

export type DecisionNotice = { tone: "bad" | "info"; text: string; stale: boolean };

/**
 * Honest wording for a failed accept/dismiss. 422: the change no longer validates, the
 * suggestion stays pending. 409: already decided or expired, so the list is stale.
 */
export function decisionError(e: ApiError): DecisionNotice {
  if (e.unreachable) return { tone: "bad", text: "Cannot reach the backend. Nothing was changed.", stale: false };
  if (e.status === 422) {
    return { tone: "bad", text: `Not applied. The change no longer passes validation, so the suggestion stays pending: ${e.message}`, stale: false };
  }
  if (e.status === 409) {
    return { tone: "info", text: "This suggestion was already decided or has expired, so nothing was changed. The list has been refreshed.", stale: true };
  }
  return { tone: "bad", text: `${e.code}: ${e.message}`, stale: false };
}
