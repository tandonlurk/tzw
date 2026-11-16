import type { ApiError, CollabAction, CollabState } from "./api";
import type { Tone } from "./interactions";

const STATE_LABEL: Record<CollabState, string> = {
  proposed: "Proposed",
  accepted: "Accepted, not funded",
  rejected: "Rejected",
  funded: "Funded",
  completed: "Completed",
  cancelled: "Cancelled",
};

export function collabStateLabel(s: CollabState): string {
  return STATE_LABEL[s] ?? s;
}

export function collabStateTone(s: CollabState): Tone {
  switch (s) {
    case "proposed":
    case "accepted":
      return "pend";
    case "completed":
      return "pos";
    case "rejected":
    case "cancelled":
      return "neg";
    default:
      return "info";
  }
}

export function isCollabOpen(s: CollabState): boolean {
  return s === "proposed" || s === "accepted" || s === "funded";
}

const ACTION_LABEL: Record<CollabAction, string> = {
  accept: "Accept (partner)",
  reject: "Reject (partner)",
  fund: "Fund",
  complete: "Declare complete and pay out",
  cancel: "Cancel",
};

export function collabActionLabel(a: CollabAction): string {
  return ACTION_LABEL[a];
}

/** Basis points as a percentage, e.g. 6000 -> "60%", 3333 -> "33.33%". */
export function formatBps(bps: number): string {
  const pct = bps / 100;
  return `${Number.isInteger(pct) ? pct : pct.toFixed(2).replace(/0+$/, "")}%`;
}

/** Percent text (e.g. "60" or "33.33") to integer basis points, or null if invalid (0 to 100, at most 2 decimals). */
export function parseSharePercent(input: string): number | null {
  const t = input.trim();
  if (!/^\d{1,3}(\.\d{1,2})?$/.test(t)) return null;
  const [w, f = ""] = t.split(".");
  const bps = Number(w) * 100 + Number(f.padEnd(2, "0"));
  return bps > 10000 ? null : bps;
}

/** For a 409 approval_required: which parties' contributions exceed their approval threshold. */
export function approvalRequiredParties(e: unknown): ("proposer" | "partner")[] | null {
  const err = e as Partial<ApiError> | null;
  if (!err || err.code !== "approval_required") return null;
  const out: ("proposer" | "partner")[] = [];
  for (const d of err.details ?? []) {
    const p = (d as { path?: unknown })?.path;
    if ((p === "proposer" || p === "partner") && !out.includes(p)) out.push(p);
  }
  return out;
}
