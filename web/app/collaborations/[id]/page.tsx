"use client";

import { use, useState } from "react";
import Link from "next/link";
import { api, isApiError, type ApiError, type CollabAction, type CollabParty, type Collaboration } from "@/lib/api";
import { approvalRequiredParties, collabActionLabel, formatBps } from "@/lib/collaborations";
import { formatMoney } from "@/lib/money";
import { useAsync } from "@/lib/useAsync";
import { Async, Empty, EnvBadge, ErrorBanner, Money, Tile, formatTs } from "@/components/ui";
import { CollabStateBadge } from "@/components/CollabCard";
import { LedgerTable } from "@/components/LedgerTable";

export default function CollaborationPage({ params }: { params: Promise<{ id: string }> }) {
  const { id } = use(params);
  const detail = useAsync(() => api.getCollaboration(id), `collab:${id}`);
  return (
    <Async state={detail} what="collaboration">
      {(d) => (
        <div className="stack">
          <Header c={d.collaboration} onChanged={detail.reload} />
          <h2>Ledger</h2>
          <LedgerTable entries={d.ledger} />
          <h2>Activity</h2>
          {d.activity.length === 0 ? <Empty>No activity yet.</Empty> : (
            <div className="stack">
              {d.activity.map((it) => (
                <div key={it.id} className="card">
                  <div className="row spread"><span>{it.summary}</span><span className="dim">{formatTs(it.ts)}</span></div>
                  {it.detail && <details><summary>Details</summary><pre>{JSON.stringify(it.detail, null, 2)}</pre></details>}
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </Async>
  );
}

function PartyCard({ role, p, done }: { role: string; p: CollabParty; done: boolean }) {
  return (
    <div className="card">
      <h3 style={{ marginTop: 0 }}>{role}: <Link href={`/agents/${p.agent_id}`}>{p.name}</Link></h3>
      <table><tbody>
        <tr><td className="dim">Contribution</td><td>{formatMoney(p.contribution)}</td></tr>
        <tr><td className="dim">Share of pool</td><td>{formatBps(p.share_bps)} ({p.share_bps} bps)</td></tr>
        <tr><td className="dim">Expected payout</td><td>{formatMoney(p.expected_payout)}</td></tr>
        {done && p.payout !== null && <tr><td className="dim">Payout</td><td>{formatMoney(p.payout)}</td></tr>}
        {done && p.net !== null && <tr><td className="dim">Net</td><td><Money value={p.net} signed tone /></td></tr>}
      </tbody></table>
    </div>
  );
}

function Header({ c, onChanged }: { c: Collaboration; onChanged: () => void }) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  const [needApproval, setNeedApproval] = useState<("proposer" | "partner")[] | null>(null);
  const [reason, setReason] = useState("");
  const done = c.state === "completed";

  async function run(fn: () => Promise<unknown>) {
    setBusy(true);
    setError(null);
    try {
      await fn();
      setNeedApproval(null);
      onChanged();
    } catch (e) {
      if (isApiError(e)) {
        const parties = approvalRequiredParties(e);
        if (parties) setNeedApproval(parties);
        else setError(e);
      }
    } finally {
      setBusy(false);
    }
  }

  function act(a: CollabAction) {
    if (a === "fund") return run(() => api.fundCollaboration(c.id));
    if (a === "cancel") return run(() => api.cancelCollaboration(c.id, reason.trim() || undefined));
    return run(() => api.collabAction(c.id, a));
  }

  return (
    <>
      <div className="page-head">
        <div>
          <h1>{c.objective}</h1>
          <div className="row"><CollabStateBadge state={c.state} /><EnvBadge env={c.env} /><span className="dim">Completion is declared by the owner, not trustless.</span></div>
          {c.status_reason && <div className="neg">{c.status_reason}</div>}
        </div>
        <Link className="btn" href="/collaborations">All collaborations</Link>
      </div>
      <div className="grid">
        <Tile label="Pool"><Money value={c.pool} /></Tile>
        <Tile label="Created">{formatTs(c.created_at)}</Tile>
        {c.funded_at && <Tile label="Funded">{formatTs(c.funded_at)}</Tile>}
        {c.closed_at && <Tile label="Closed">{formatTs(c.closed_at)}</Tile>}
      </div>
      <div className="grid-2">
        <PartyCard role="Proposer" p={c.proposer} done={done} />
        <PartyCard role="Partner" p={c.partner} done={done} />
      </div>

      <ErrorBanner error={error} />
      {needApproval && (
        <div className="notice" role="alert">
          <strong>Owner approval required.</strong> The contribution of {needApproval.map((r) => c[r].name).join(" and ")} is above that agent&apos;s approval threshold, so nothing was funded.
          Approving commits {formatMoney(c.proposer.contribution)} (proposer) and {formatMoney(c.partner.contribution)} (partner), simulated funds, from the agents&apos; available balances.
          <div className="row" style={{ marginTop: ".5rem" }}>
            <button className="primary" disabled={busy} onClick={() => run(() => api.fundCollaboration(c.id, true))}>I approve: fund now</button>
            <button disabled={busy} onClick={() => setNeedApproval(null)}>Not now</button>
          </div>
        </div>
      )}
      {c.actions.length === 0 ? <div className="dim">No further actions: this collaboration is final.</div> : (
        <div className="card">
          <h3 style={{ marginTop: 0 }}>Actions</h3>
          <div className="row">
            {c.actions.map((a) => (
              <button key={a} disabled={busy} className={a === "complete" || a === "fund" ? "primary" : undefined} onClick={() => act(a)}>{collabActionLabel(a)}</button>
            ))}
            {c.actions.includes("cancel") && (
              <input style={{ maxWidth: 260 }} value={reason} maxLength={200} onChange={(e) => setReason(e.target.value)} placeholder="Cancel reason (optional)" aria-label="Cancel reason" />
            )}
          </div>
          {c.state === "funded" && <div className="dim" style={{ marginTop: ".4rem" }}>Cancelling a funded collaboration refunds both contributions.</div>}
        </div>
      )}
    </>
  );
}
