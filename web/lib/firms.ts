// Pure helpers for firms (docs/design/approach-and-firms.md section 2). Display math only; the backend pays out.

import type { ActivityItem, Firm, FirmRoleName, FirmRoleSlot, FirmState, Money } from "./api";
import type { Tone } from "./interactions";

export const FIRM_ROLE_NAMES: FirmRoleName[] = ["scout", "analyst"];
export const LEAD_SHARE_MIN_BPS = 5000;
export const LEAD_SHARE_MAX_BPS = 10000;
export const LEAD_SHARE_DEFAULT_BPS = 8000;
export const MAX_SLOTS = 5;
export const MANDATE_MAX = 500;

export const ROLE_TASK: Record<FirmRoleName, string> = {
  scout: "Scans markets each tick and submits at most one candidate to the lead.",
  analyst: "Evaluates the lead's drafted proposal and returns a signal for or against.",
};

export function defaultRoles(): FirmRoleSlot[] {
  return [
    { role: "scout", slots: 2, weight_bps: 5000 },
    { role: "analyst", slots: 1, weight_bps: 5000 },
  ];
}

export function memberPoolBps(leadShareBps: number): number {
  return 10000 - leadShareBps;
}

/** Local checks for the roles list (the backend validates too). Empty array = valid. */
export function validateRoles(roles: FirmRoleSlot[]): string[] {
  const errs: string[] = [];
  if (roles.length === 0) errs.push("Add at least one role.");
  const seen = new Set<string>();
  for (const r of roles) {
    if (!FIRM_ROLE_NAMES.includes(r.role)) errs.push(`Unknown role "${r.role}".`);
    if (seen.has(r.role)) errs.push(`Role "${r.role}" is listed twice.`);
    seen.add(r.role);
    if (!Number.isInteger(r.slots) || r.slots < 1 || r.slots > MAX_SLOTS) errs.push(`${r.role}: slots must be 1 to ${MAX_SLOTS}.`);
    if (!Number.isInteger(r.weight_bps) || r.weight_bps < 0 || r.weight_bps > 10000) errs.push(`${r.role}: weight must be 0% to 100%.`);
  }
  const sum = roles.reduce((s, r) => s + (Number.isFinite(r.weight_bps) ? r.weight_bps : 0), 0);
  if (roles.length > 0 && sum !== 10000) errs.push(`Role weights must add up to 100% (now ${sum / 100}%).`);
  return errs;
}

export interface SplitResult {
  /** What the lead keeps (profit minus member payouts); the whole amount when profit ≤ 0. */
  lead: Money;
  pool: Money;
  /** One per used contribution, in input order. */
  payouts: Money[];
}

/**
 * Profit split for one settled firm challenge (decided 2026-11-17, same as the backend):
 * pool = p × member_pool_bps / 10000 (floor). The pool goes only to roles with at least one used
 * contribution, by weight_bps renormalised over those roles, equal within a role, each payout floored.
 * Remainders stay with the lead. Losses (p ≤ 0) are the lead's alone.
 */
export function splitProfit(profit: Money, leadShareBps: number, used: { role: string; weight_bps: number }[]): SplitResult {
  const p = BigInt(profit);
  const zero = used.map(() => "0");
  if (p <= BigInt(0)) return { lead: p.toString(), pool: "0", payouts: zero };
  const pool = (p * BigInt(memberPoolBps(leadShareBps))) / BigInt(10000);
  const count = new Map<string, number>();
  const weight = new Map<string, number>();
  for (const u of used) {
    count.set(u.role, (count.get(u.role) ?? 0) + 1);
    weight.set(u.role, u.weight_bps);
  }
  const W = [...weight.values()].reduce((s, w) => s + w, 0);
  if (W <= 0 || pool === BigInt(0)) return { lead: p.toString(), pool: pool.toString(), payouts: zero };
  const payouts = used.map((u) => (pool * BigInt(weight.get(u.role)!)) / (BigInt(W) * BigInt(count.get(u.role)!)));
  const paid = payouts.reduce((s, x) => s + x, BigInt(0));
  return { lead: (p - paid).toString(), pool: pool.toString(), payouts: payouts.map(String) };
}

/** Share of the whole profit each role would get if every role contributed, in bps (for the split bar). */
export function roleShareBps(leadShareBps: number, roles: FirmRoleSlot[]): { role: FirmRoleName; bps: number }[] {
  const pool = memberPoolBps(leadShareBps);
  return roles.map((r) => ({ role: r.role, bps: Math.floor((pool * r.weight_bps) / 10000) }));
}

export function slotsFilled(firm: Firm, role: FirmRoleName): number {
  return firm.members.filter((m) => m.role === role).length;
}

export function totalSlots(firm: Firm): number {
  return firm.roles.reduce((s, r) => s + r.slots, 0);
}

/** Roles that still have a free slot. */
export function openRoles(firm: Firm): FirmRoleName[] {
  return firm.roles.filter((r) => slotsFilled(firm, r.role) < r.slots).map((r) => r.role);
}

export function firmStateTone(s: FirmState): Tone {
  return s === "open" ? "info" : "dim";
}

const ACTION_LABEL: Record<string, string> = { join: "Join", leave: "Leave", close: "Close firm" };
export function firmActionLabel(a: string): string {
  return ACTION_LABEL[a] ?? a;
}

// ---- Activity kinds added with approach + firms ----

const KIND: Record<string, { label: string; tone: Tone }> = {
  firm_created: { label: "firm opened", tone: "info" },
  firm_joined: { label: "joined firm", tone: "info" },
  firm_left: { label: "left firm", tone: "dim" },
  firm_closed: { label: "firm closed", tone: "dim" },
  firm_contribution: { label: "firm contribution", tone: "info" },
  firm_task_fee: { label: "task fee paid", tone: "pos" },
  firm_fee_denied: { label: "task fee denied", tone: "neg" },
  firm_payout: { label: "firm payout", tone: "pos" },
  approach_filtered: { label: "outside approach", tone: "dim" },
  // QuickNode Streams (D-74)
  stream_confirmed: { label: "confirmed onchain", tone: "pos" },
  stream_mismatch: { label: "stream mismatch", tone: "neg" },
  stream_observed: { label: "escrow event", tone: "info" },
  chain_event: { label: "chain event", tone: "dim" },
  chain_event_removed: { label: "reorged out", tone: "pend" },
};

export function activityKindLabel(kind: string): string {
  return KIND[kind]?.label ?? kind.replace(/_/g, " ");
}

export function activityKindTone(kind: string): Tone {
  return KIND[kind]?.tone ?? "info";
}

export function isFirmActivity(kind: string): boolean {
  return kind.startsWith("firm_");
}

/** Firm id of an activity item: a top-level `firm_id` if the backend adds one, else `detail.firm_id`. */
export function activityFirmId(it: ActivityItem): string | null {
  const top = (it as ActivityItem & { firm_id?: unknown }).firm_id;
  if (typeof top === "string" && top) return top;
  const d = it.detail?.firm_id;
  return typeof d === "string" && d ? d : null;
}

/** Money amount carried in a firm activity's detail (`amount`, `fee` or `payout`), if any. */
export function activityAmount(it: ActivityItem): Money | null {
  for (const k of ["amount", "fee", "payout"]) {
    const v = it.detail?.[k];
    if (typeof v === "string" && /^-?\d+$/.test(v)) return v;
  }
  return null;
}
