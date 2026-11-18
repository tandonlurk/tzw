"use client";

import Link from "next/link";
import type { Firm, FirmRoleSlot } from "@/lib/api";
import { formatBps } from "@/lib/collaborations";
import { memberPoolBps, roleShareBps, slotsFilled, totalSlots } from "@/lib/firms";
import { formatMoney } from "@/lib/money";
import { marketsSummary } from "@/lib/approach";
import { EnvBadge, Money } from "@/components/ui";

/** Lead share vs member pool, the pool divided by role weight (as if every role contributed). */
export function SplitBar({ leadShareBps, roles }: { leadShareBps: number; roles: FirmRoleSlot[] }) {
  const shares = roleShareBps(leadShareBps, roles);
  const pool = memberPoolBps(leadShareBps);
  return (
    <div>
      <div className="split-bar" role="img" aria-label={`Lead keeps ${formatBps(leadShareBps)}; members share ${formatBps(pool)}`}>
        <div className="lead" style={{ width: `${leadShareBps / 100}%` }}>{leadShareBps >= 1500 ? `lead ${formatBps(leadShareBps)}` : ""}</div>
        {shares.map((s) => (
          <div key={s.role} className={`r-${s.role}`} style={{ width: `${s.bps / 100}%` }} title={`${s.role}: ${formatBps(s.bps)} of profit`}>
            {s.bps >= 800 ? `${s.role} ${formatBps(s.bps)}` : ""}
          </div>
        ))}
      </div>
      <div className="legend">
        <span className="info"><span className="sw" />Lead {formatBps(leadShareBps)}</span>
        {shares.map((s) => (
          <span key={s.role} className={s.role === "scout" ? "pos" : "pend"}><span className="sw" />{s.role} {formatBps(s.bps)}</span>
        ))}
      </div>
      <div className="dim small">
        Of each profitable firm challenge&apos;s realized profit. Only roles that actually contributed share the {formatBps(pool)} pool; rounding remainders and any unused share stay with the lead. Losses are the lead&apos;s alone.
      </div>
    </div>
  );
}

export function FirmStateBadge({ state }: { state: Firm["state"] }) {
  return <span className={`badge ${state === "open" ? "info" : "dim"}`}>{state}</span>;
}

export function FirmCard({ f }: { f: Firm }) {
  return (
    <div className="card stack">
      <div className="row spread">
        <Link href={`/firms/${f.id}`}><strong>{f.name}</strong></Link>
        <span className="row"><FirmStateBadge state={f.state} /><EnvBadge env={f.env} /></span>
      </div>
      <div>
        Led by <Link href={`/agents/${f.lead_agent_id}`}>{f.lead_name}</Link>
        <span className="dim"> · members {f.members.length}/{totalSlots(f)} · {f.roles.map((r) => `${r.role} ${slotsFilled(f, r.role)}/${r.slots}`).join(", ")}</span>
      </div>
      {f.mandate && <div className="dim">{f.mandate}</div>}
      <SplitBar leadShareBps={f.lead_share_bps} roles={f.roles} />
      <div className="row dim small">
        <span>Markets: {f.markets ? marketsSummary(f.markets) : "the lead's approach"}</span>
        <span>· Task fee {formatMoney(f.task_fee)} (simulated)</span>
      </div>
      <div className="row">
        <span>Lead P&amp;L <Money value={f.stats.lead_realized_pnl} signed tone /></span>
        <span className="dim">·</span>
        <span>Paid to members <Money value={f.stats.paid_to_members} /></span>
        <span className="dim">· {f.stats.challenges} challenge(s), {f.stats.settled} settled</span>
      </div>
    </div>
  );
}
