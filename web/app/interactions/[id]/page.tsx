"use client";

import { use } from "react";
import Link from "next/link";
import { api, type InteractionDetail, type Run } from "@/lib/api";
import { useAsync } from "@/lib/useAsync";
import { outcomeText } from "@/lib/interactions";
import { Async, Empty, EnvBadge, Money, Tile, formatTs } from "@/components/ui";
import { StateBadge, TermsTable } from "@/components/Interactions";
import { LedgerTable } from "@/components/LedgerTable";

export default function InteractionPage({ params }: { params: Promise<{ id: string }> }) {
  const { id } = use(params);
  const d = useAsync(() => api.getInteraction(id), `interaction:${id}`);
  return <Async state={d} what="challenge">{(i) => <Detail i={i} />}</Async>;
}

function Detail({ i }: { i: InteractionDetail }) {
  const names = new Map([[i.proposer_id, i.proposer_name], [i.counterparty_id, i.counterparty_name]]);
  const outcome = outcomeText(i);
  return (
    <div className="stack">
      <div className="page-head">
        <div>
          <h1>{i.market_title}</h1>
          <div className="row"><StateBadge state={i.state} /><EnvBadge env={i.env} />
            {i.market_source === "fake" && <span className="badge sim" title="Prices come from the offline fake market">fake market data</span>}
            <span className="dim">round {i.rounds} of {i.max_rounds}</span></div>
          <div style={{ marginTop: ".4rem" }}>
            <Link href={`/agents/${i.proposer_id}`}>{i.proposer_name}</Link> (proposer) vs{" "}
            <Link href={`/agents/${i.counterparty_id}`}>{i.counterparty_name}</Link>
          </div>
          <div className="dim">{i.claim}</div>
        </div>
      </div>

      {i.state === "awaiting_approval" && <div className="notice">Waiting for owner approval. <Link href="/approvals">Open approvals</Link></div>}
      {outcome && <div className={`notice ${i.state === "settled" ? "info" : "bad"}`}>{outcome}{i.state === "settled" && i.resolution && " (resolution is backend-administered, not trustless)"}</div>}

      <div className="grid">
        <Tile label="Stake per side"><Money value={i.stake} /></Tile>
        <Tile label="Observed at resolution">{i.observed_display ?? "not yet"}</Tile>
        <Tile label="Ends">{formatTs(i.end_at)}</Tile>
      </div>

      <div className="card"><h3 style={{ marginTop: 0 }}>Terms</h3><TermsTable terms={i.terms} env={i.env} /></div>

      <div className="card">
        <h3 style={{ marginTop: 0 }}>Negotiation</h3>
        {i.negotiation.length === 0 ? <Empty>No negotiation rounds recorded.</Empty> : (
          <div className="timeline">
            {i.negotiation.map((n) => (
              <div key={n.seq}>
                <div className="row"><span className="badge neutral">{n.action}</span><strong>{names.get(n.agent_id) ?? n.agent_id}</strong><span className="dim">{formatTs(n.ts)}</span></div>
                <div>{n.summary}</div>
                {n.terms && <div className="dim">Stake <Money value={n.terms.stake} />, proposer takes {n.terms.direction}</div>}
              </div>
            ))}
          </div>
        )}
      </div>

      {i.approvals.length > 0 && (
        <div className="card">
          <h3 style={{ marginTop: 0 }}>Approvals</h3>
          <table><tbody>
            {i.approvals.map((a) => (
              <tr key={a.id}><td>{a.agent_name}</td><td><Money value={a.stake} /></td>
                <td><span className={`badge ${a.status === "pending" ? "pend" : a.status === "approved" ? "pos" : "neg"}`}>{a.status}</span></td>
                <td className="dim">{a.decided_at ? formatTs(a.decided_at) : `expires ${formatTs(a.expires_at)}`}</td></tr>
            ))}
          </tbody></table>
        </div>
      )}

      <div className="card">
        <h3 style={{ marginTop: 0 }}>Decision trace</h3>
        <div className="dim" style={{ marginBottom: ".4rem" }}>Concise summaries of what each agent&apos;s graph decided. Not model reasoning.</div>
        {i.runs.length === 0 ? <Empty>No agent runs linked to this challenge.</Empty> : i.runs.map((r) => <RunView key={r.id} r={r} name={names.get(r.agent_id) ?? r.agent_id} />)}
      </div>

      <div className="card">
        <h3 style={{ marginTop: 0 }}>Timeline</h3>
        {i.activity.length === 0 ? <Empty>No events.</Empty> : (
          <div className="timeline">
            {i.activity.map((a) => (
              <div key={a.id}><span className="dim">{formatTs(a.ts)}</span> {a.summary}
                {a.detail && <details><summary>Details</summary><pre>{JSON.stringify(a.detail, null, 2)}</pre></details>}</div>
            ))}
          </div>
        )}
      </div>

      <div className="card"><h3 style={{ marginTop: 0 }}>Ledger entries</h3><LedgerTable entries={i.ledger} /></div>
    </div>
  );
}

function RunView({ r, name }: { r: Run; name: string }) {
  return (
    <details style={{ marginBottom: ".4rem" }}>
      <summary>
        {name} (config v{r.agent_version}) &middot; {formatTs(r.ts)} &middot; trigger {r.trigger} &middot; <span className={r.status === "error" ? "neg" : ""}>{r.status}</span> &middot; {r.reasoner}
      </summary>
      {r.error && <div className="neg">{r.error}</div>}
      <ul>{r.trace.map((t, k) => <li key={k}><span className="mono dim">{t.node_type}</span> {t.summary}</li>)}</ul>
      {r.intents.map((it, k) => <div key={k} className="dim">Intent {it.intent.type}: {it.summary}{it.ask_owner ? " (asks owner)" : ""}</div>)}
    </details>
  );
}
