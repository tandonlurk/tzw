"use client";

import Link from "next/link";
import type { Collaboration } from "@/lib/api";
import { collabStateLabel, collabStateTone, formatBps } from "@/lib/collaborations";
import { formatMoney } from "@/lib/money";
import { EnvBadge, Money } from "./ui";

export function CollabStateBadge({ state }: { state: Collaboration["state"] }) {
  return <span className={`badge ${collabStateTone(state)}`}>{collabStateLabel(state)}</span>;
}

export function CollabCard({ c }: { c: Collaboration }) {
  return (
    <div className="card">
      <div className="row spread">
        <Link href={`/collaborations/${c.id}`}><strong>{c.objective}</strong></Link>
        <span className="row"><CollabStateBadge state={c.state} /><EnvBadge env={c.env} /></span>
      </div>
      <div style={{ margin: ".3rem 0" }}>
        <Link href={`/agents/${c.proposer.agent_id}`}>{c.proposer.name}</Link> ({formatMoney(c.proposer.contribution)}, {formatBps(c.proposer.share_bps)}) with{" "}
        <Link href={`/agents/${c.partner.agent_id}`}>{c.partner.name}</Link> ({formatMoney(c.partner.contribution)}, {formatBps(c.partner.share_bps)})
      </div>
      <div className="dim">Pool {formatMoney(c.pool)}. Completion is declared by the owner, not trustless.</div>
      {c.state === "completed" && c.proposer.net !== null && c.partner.net !== null && (
        <div>Net: {c.proposer.name} <Money value={c.proposer.net} signed tone />, {c.partner.name} <Money value={c.partner.net} signed tone /></div>
      )}
      {c.status_reason && <div className="neg">{c.status_reason}</div>}
    </div>
  );
}
