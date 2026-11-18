import Link from "next/link";
import type { ActivityItem } from "@/lib/api";
import { customCodeFailure } from "@/lib/customCode";
import { activityAmount, activityFirmId, activityKindLabel, activityKindTone } from "@/lib/firms";
import { formatMoney } from "@/lib/money";

/** Readable badge for an activity kind (firm and approach kinds get plain names and semantic tones). */
export function ActivityKindBadge({ kind }: { kind: string }) {
  return <span className={`badge ${activityKindTone(kind)}`}>{activityKindLabel(kind)}</span>;
}

/** Link to the firm an activity belongs to, if any. */
export function ActivityFirmLink({ it }: { it: ActivityItem }) {
  const id = activityFirmId(it);
  return id ? <Link href={`/firms/${id}`}>firm</Link> : null;
}

/** Plain-language extra for activity kinds whose summary alone is not enough. Raw detail stays in the expandable JSON. */
export function ActivityExtra({ it }: { it: ActivityItem }) {
  if (it.kind === "custom_code_failed") {
    const f = customCodeFailure(it.detail);
    if (!f) return null;
    return (
      <div className="neg" style={{ marginTop: ".3rem" }}>
        Your custom code failed, so this agent did nothing this run. No funds moved.
        <div className="mono" style={{ fontSize: ".8rem" }}>{f.error}</div>
        {f.logs.length > 0 && <details><summary>Logs ({f.logs.length})</summary><pre>{f.logs.join("\n")}</pre></details>}
      </div>
    );
  }
  const amount = activityAmount(it);
  switch (it.kind) {
    case "firm_task_fee":
      return <div className="dim small">Guaranteed task fee from the lead to the member{amount ? `: ${formatMoney(amount)} (simulated)` : " (simulated)"}. A transfer, not profit for the lead.</div>;
    case "firm_payout":
      return <div className="dim small">Member share of a settled firm challenge&apos;s profit{amount ? `: ${formatMoney(amount)} (simulated)` : " (simulated)"}. Counted as firm income, separate from the member&apos;s own challenge P&amp;L.</div>;
    case "firm_fee_denied":
      return <div className="neg small">The lead&apos;s limits refused the task fee, so the member was not paid for this contribution. No funds moved.</div>;
    case "approach_filtered":
      return <div className="dim small">Markets outside this agent&apos;s approach were skipped. Change them in the agent&apos;s Approach block.</div>;
    default:
      return null;
  }
}
