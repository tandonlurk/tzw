"use client";

import Link from "next/link";
import type { Interaction, InteractionState } from "@/lib/api";
import { escrowRows, outcomeText, stateLabel, stateTone, termsRows } from "@/lib/interactions";
import { explorerTxUrl } from "@/lib/chain";
import { useChainConfig } from "@/lib/useChainConfig";
import { formatMoney } from "@/lib/money";
import { EnvBadge } from "./ui";
import type { ChallengeTerms, EscrowTxs } from "@/lib/api";

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

export function TermsTable({ terms, env, marketSource }: { terms: ChallengeTerms; env: string; marketSource?: Interaction["market_source"] }) {
  return (
    <table className="kv"><tbody>
      {termsRows(terms, env, marketSource).map(([k, v]) => <tr key={k}><td>{k}</td><td>{v}</td></tr>)}
    </tbody></table>
  );
}

/** Arc testnet escrow transactions with explorer links; steps not reached yet read "not yet". */
export function EscrowTxTable({ escrow, proposer, counterparty }: { escrow: EscrowTxs; proposer: string; counterparty: string }) {
  const chain = useChainConfig();
  const explorer = chain.data?.enabled ? chain.data.explorer_url : null;
  return (
    <table className="kv"><tbody>
      {escrowRows(escrow, proposer, counterparty).map(([k, hash]) => (
        <tr key={k}><td>{k}</td><td className="mono">
          {hash && explorer ? <a href={explorerTxUrl(explorer, hash)} target="_blank" rel="noreferrer" title={hash}>{hash.slice(0, 10)}...{hash.slice(-6)}</a>
            : hash ? <span title={hash}>{hash.slice(0, 10)}...{hash.slice(-6)}</span>
            : <span className="dim">not yet</span>}
        </td></tr>
      ))}
    </tbody></table>
  );
}
