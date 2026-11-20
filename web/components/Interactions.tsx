"use client";

import Link from "next/link";
import type { Interaction, InteractionState } from "@/lib/api";
import { escrowRows, outcomeText, stateLabel, stateTone, termsRows } from "@/lib/interactions";
import { explorerTxUrl } from "@/lib/chain";
import { useChainConfig } from "@/lib/useChainConfig";
import { formatMoney } from "@/lib/money";
import { EnvBadge } from "./ui";
import { shortName } from "@/lib/agentColors";
import type { ChallengeTerms, EscrowTxs, StreamEvent } from "@/lib/api";
import { eventsForTx } from "@/lib/streams";
import { StreamTxBadge } from "./StreamBadge";

export function StateBadge({ state }: { state: InteractionState }) {
  return <span className={`badge ${stateTone(state)}`}>{stateLabel(state)}</span>;
}

export function InteractionCard({ i }: { i: Interaction }) {
  const outcome = outcomeText(i);
  return (
    <div className="card ic">
      <div className="ic-top">
        <Link href={`/interactions/${i.id}`} className="ic-title">{i.market_title}</Link>
        <span className="row" style={{ gap: 6 }}><StateBadge state={i.state} /><EnvBadge env={i.env} /></span>
      </div>
      <div className="ic-parties">
        <Link href={`/agents/${i.proposer_id}`}>{shortName(i.proposer_name)}</Link>
        <span className="vs">vs</span>
        <Link href={`/agents/${i.counterparty_id}`}>{shortName(i.counterparty_name)}</Link>
        <span className="ic-stake"><span className="num">{formatMoney(i.stake)}</span> each</span>
      </div>
      <div className="dim small">{i.claim}</div>
      {i.pending_approvals > 0 && <div className="pend small" style={{ marginTop: 6 }}>{i.pending_approvals} approval{i.pending_approvals === 1 ? "" : "s"} pending · <Link href="/approvals">Review</Link></div>}
      {outcome && <div className={`small ${i.outcome === "void" || i.state !== "settled" ? "neg" : "pos"}`} style={{ marginTop: 6 }}>{outcome}</div>}
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
export function EscrowTxTable({ escrow, proposer, counterparty, streams }: { escrow: EscrowTxs; proposer: string; counterparty: string; streams?: StreamEvent[] }) {
  const chain = useChainConfig();
  const explorer = chain.data?.enabled ? chain.data.explorer_url : null;
  return (
    <table className="kv"><tbody>
      {escrowRows(escrow, proposer, counterparty).map(([k, hash]) => (
        <tr key={k}><td>{k}</td><td className="mono">
          {hash && explorer ? <a href={explorerTxUrl(explorer, hash)} target="_blank" rel="noreferrer" title={hash}>{hash.slice(0, 10)}...{hash.slice(-6)}</a>
            : hash ? <span title={hash}>{hash.slice(0, 10)}...{hash.slice(-6)}</span>
            : <span className="dim">not yet</span>}
        </td>
        {streams && <td>{hash && <StreamTxBadge events={eventsForTx(streams, hash)} />}</td>}
        </tr>
      ))}
    </tbody></table>
  );
}
