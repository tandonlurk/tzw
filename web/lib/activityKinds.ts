// Groups backend activity kinds into the event categories the owner sees (brief 4 Activity / 10).
// Unknown kinds fall into "agent" so new backend kinds still show up.

export type ActivityCategory =
  | "discovery"
  | "challenge"
  | "negotiation"
  | "collaboration"
  | "approval"
  | "transaction"
  | "settlement"
  | "agent";

export const CATEGORIES: { id: ActivityCategory; label: string }[] = [
  { id: "discovery", label: "Discoveries" },
  { id: "challenge", label: "Challenges" },
  { id: "negotiation", label: "Negotiations" },
  { id: "collaboration", label: "Collaborations" },
  { id: "approval", label: "Approvals" },
  { id: "transaction", label: "Transactions" },
  { id: "settlement", label: "Settlements" },
  { id: "agent", label: "Agent changes" },
];

const EXACT: Record<string, ActivityCategory> = {
  approach_filtered: "discovery",
  market_data_error: "discovery",
  market_data_unavailable: "discovery",
  agent_reasoning: "discovery",
  proposal_denied: "discovery",
  challenge_countered: "negotiation",
  negotiation_cap_reached: "negotiation",
  challenge_settled: "settlement",
  challenge_voided: "settlement",
  collab_payout: "settlement",
  collab_refund: "settlement",
  firm_payout: "settlement",
  vault_released: "settlement",
  collab_commit: "transaction",
  escrow_created: "transaction",
  simulated_deposit: "transaction",
  firm_task_fee: "transaction",
  custom_code_failed: "agent",
};

const PREFIX: [string, ActivityCategory][] = [
  ["challenge_", "challenge"],
  ["collaboration_", "collaboration"],
  ["firm_", "collaboration"],
  ["approval_", "approval"],
  ["suggestion_", "approval"],
  ["funds_", "transaction"],
  ["stake_", "transaction"],
  ["vault_", "transaction"],
  ["agent_", "agent"],
];

export function categoryOf(kind: string): ActivityCategory {
  if (kind in EXACT) return EXACT[kind];
  for (const [p, c] of PREFIX) if (kind.startsWith(p)) return c;
  return "agent";
}

/** Day bucket label for a timeline ("Today", "Yesterday", or a date), relative to `now`. */
export function dayLabel(ts: string, now: Date = new Date()): string {
  const d = new Date(ts);
  if (Number.isNaN(d.getTime())) return "Unknown date";
  const startOf = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const diff = Math.round((startOf(now) - startOf(d)) / 86_400_000);
  if (diff === 0) return "Today";
  if (diff === 1) return "Yesterday";
  if (diff === -1) return "Tomorrow (simulated clock)";
  return d.toLocaleDateString(undefined, { weekday: "short", month: "short", day: "numeric", year: d.getFullYear() === now.getFullYear() ? undefined : "numeric" });
}

/** Group items (already newest first) into consecutive day buckets. */
export function groupByDay<T extends { ts: string }>(items: T[], now: Date = new Date()): { label: string; items: T[] }[] {
  const out: { label: string; items: T[] }[] = [];
  for (const it of items) {
    const label = dayLabel(it.ts, now);
    const last = out[out.length - 1];
    if (last && last.label === label) last.items.push(it);
    else out.push({ label, items: [it] });
  }
  return out;
}
