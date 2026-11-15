"use client";

import Link from "next/link";
import type { Interaction, InteractionState } from "@/lib/api";
import { outcomeText, stateLabel, stateTone, termsRows } from "@/lib/interactions";
import { formatMoney } from "@/lib/money";
import { EnvBadge } from "./ui";
import type { ChallengeTerms } from "@/lib/api";

export function StateBadge({ state }: { state: InteractionState }) {
  return <span className={`badge ${stateTone(state)}`}>{stateLabel(state)}</span>;
}

export function InteractionCard({ i }: { i: Interaction }) {
  const outcome = outcomeText(i);
  return (
    <div className="card">
      <div className="row spread">
        <Link href={`/interactions/${i.id}`}><strong>{i.market_title}</strong></Link>
        <span className="row"><StateBadge state={i.state} /><EnvBadge env={i.env} /></span>
      </div>
      <div style={{ margin: ".3rem 0" }}>
        <Link href={`/agents/${i.proposer_id}`}>{i.proposer_name}</Link> vs{" "}
        <Link href={`/agents/${i.counterparty_id}`}>{i.counterparty_name}</Link>
        <span className="dim"> &middot; {formatMoney(i.stake)} each</span>
      </div>
      <div className="dim">{i.claim}</div>
      {i.pending_approvals > 0 && <div className="pend">{i.pending_approvals} approval(s) pending. <Link href="/approvals">Review</Link></div>}
      {outcome && <div className={i.outcome === "void" || i.state !== "settled" ? "neg" : "pos"}>{outcome}</div>}
    </div>
  );
}

export function TermsTable({ terms, env }: { terms: ChallengeTerms; env: string }) {
  return (
    <table className="kv"><tbody>
      {termsRows(terms, env).map(([k, v]) => <tr key={k}><td>{k}</td><td>{v}</td></tr>)}
    </tbody></table>
  );
}
