"use client";

import Link from "next/link";
import { useState } from "react";
import { api, isApiError, type ActivityItem, type Agent, type ApiError, type Interaction } from "@/lib/api";
import { formatMoney, parseMoney } from "@/lib/money";
import { isOpen } from "@/lib/interactions";
import { versionHistory } from "@/lib/agentView";
import { categoryOf } from "@/lib/activityKinds";
import { useAsync } from "@/lib/useAsync";
import { Async, Empty, ErrorBanner, formatTs } from "@/components/ui";
import { InteractionCard } from "@/components/Interactions";
import { FeedItem } from "@/components/activity/Feed";
import { TESTNET_LABEL } from "@/components/wallet/WalletStatus";

/** Committed funds against the owner's aggregate exposure limit, plus the other enforced limits. */
export function RiskCard({ a }: { a: Agent }) {
  const committed = BigInt(a.balances.committed);
  const max = BigInt(a.policy.max_aggregate_exposure);
  const pct = max > BigInt(0) ? Math.min(100, Number((committed * BigInt(1000)) / max) / 10) : 0;
  return (
    <div className="card">
      <div className="card-head"><h3>Risk and exposure</h3><span className="badge dim plain" title="Checked by the backend on every action, whatever the agent's logic says">Enforced</span></div>
      <div className="row spread small"><span className="dim">Committed of max exposure</span><span className="num text">{formatMoney(a.balances.committed)} / {formatMoney(a.policy.max_aggregate_exposure)}</span></div>
      <div className="bar-track" style={{ margin: "8px 0 4px", height: 6 }} role="meter" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(pct)} aria-label="Exposure used">
        <div className={`bar-fill ${pct >= 80 ? "neg" : pct > 0 ? "pend" : ""}`} style={{ width: `${pct}%` }} />
      </div>
      <div className="dim xs" style={{ marginBottom: 10 }}>{pct.toFixed(0)}% of the exposure limit is in use.</div>
      <table className="kv"><tbody>
        <tr><td>Max per challenge</td><td className="num">{formatMoney(a.policy.max_per_interaction)}</td></tr>
        <tr><td>Asks you above</td><td className="num">{formatMoney(a.policy.approval_threshold)}</td></tr>
        <tr><td>Stop loss</td><td className="num">{a.policy.stop_loss === null ? "None" : formatMoney(a.policy.stop_loss)}</td></tr>
        <tr><td>Min conviction</td><td className="num">{(a.policy.min_conviction_bps / 100).toFixed(0)}%</td></tr>
        <tr><td>Allowed</td><td>{a.policy.allowed_interaction_types.join(", ") || "Nothing"}</td></tr>
      </tbody></table>
    </div>
  );
}

/** Simulated allocate / deallocate between the owner's unallocated balance and the agent. */
export function FundingCard({ a, onChanged }: { a: Agent; onChanged: () => void }) {
  const [amount, setAmount] = useState("10");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  const [localErr, setLocalErr] = useState<string | null>(null);

  async function move(kind: "allocate" | "deallocate") {
    const minor = parseMoney(amount);
    if (!minor || minor === "0") return setLocalErr("Enter a positive amount with at most 6 decimals.");
    setBusy(true); setError(null); setLocalErr(null);
    try {
      await (kind === "allocate" ? api.allocate(a.id, minor) : api.deallocate(a.id, minor));
      onChanged();
    } catch (e) {
      if (isApiError(e)) setError(e);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="card">
      <div className="card-head"><h3>Funding</h3></div>
      {a.env === "simulated" ? (
        <>
          <div className="dim small" style={{ marginBottom: 12 }}>Move simulated funds between your unallocated balance and this agent. Not real funds.</div>
          <ErrorBanner error={error} />
          <div className="input-affix" style={{ marginBottom: 10 }}>
            <input value={amount} onChange={(e) => setAmount(e.target.value)} aria-label="Amount in USDC" inputMode="decimal" />
            <span className="affix">USDC</span>
          </div>
          <div className="row nowrap">
            <button type="button" className="grow" disabled={busy} onClick={() => move("allocate")}>Allocate</button>
            <button type="button" className="grow outline" disabled={busy} onClick={() => move("deallocate")}>Withdraw</button>
          </div>
          {localErr && <div className="field-error">{localErr}</div>}
        </>
      ) : (
        <div className="dim small">This agent uses {TESTNET_LABEL} held in its vault. Allocate and withdraw in the vault section below.</div>
      )}
    </div>
  );
}

export function OpenChallenges({ list }: { list: Interaction[] }) {
  const open = list.filter((i) => isOpen(i.state));
  return open.length === 0 ? <Empty>No open positions right now.</Empty> : <div className="stack">{open.map((i) => <InteractionCard key={i.id} i={i} />)}</div>;
}

export function PastChallenges({ list }: { list: Interaction[] }) {
  const past = list.filter((i) => !isOpen(i.state));
  return past.length === 0 ? <Empty>No finished challenges yet.</Empty> : <div className="stack">{past.map((i) => <InteractionCard key={i.id} i={i} />)}</div>;
}

const DECISION = new Set(["discovery", "challenge", "negotiation", "approval"]);

/** The agent's own choices (proposals, counters, accepts, approval requests), newest first. */
export function RecentDecisions({ items, limit = 6 }: { items: ActivityItem[]; limit?: number }) {
  const picked = items.filter((it) => DECISION.has(categoryOf(it.kind)) || it.kind === "custom_code_failed").slice(0, limit);
  return picked.length === 0 ? <Empty>No decisions yet. They appear when the agent evaluates a market.</Empty> : (
    <div className="feed">{picked.map((it) => <FeedItem key={it.id} it={it} />)}</div>
  );
}

export function VersionHistory({ agentId, bump }: { agentId: string; bump: number }) {
  const versions = useAsync(() => api.listAgentVersions(agentId), `versions:${agentId}:${bump}`);
  return (
    <Async state={versions} what="versions">
      {(list) => (
        <div className="list">
          {versionHistory(list).map(({ v, changes }) => (
            <div key={v.version} className="list-row" style={{ alignItems: "flex-start" }}>
              <span className="badge plain" style={{ marginTop: 2 }}>v{v.version}</span>
              <div className="grow">
                {changes.length > 0 ? <ul style={{ margin: 0, paddingLeft: 18 }}>{changes.map((c, k) => <li key={k} className="small">{c}</li>)}</ul>
                  : <div className="dim small">{v.version === 1 ? "Initial version." : "No config, policy or graph change."}</div>}
              </div>
              <span className="dim xs">{formatTs(v.created_at)}</span>
            </div>
          ))}
        </div>
      )}
    </Async>
  );
}

export function SavedNotice({ saved }: { saved: string | null }) {
  if (!saved || !/^\d+$/.test(saved)) return null;
  return (
    <div className="notice info" role="status" style={{ marginBottom: "var(--s-5)" }}>
      Saved as version {saved}. Earlier versions, past challenges and the ledger were not changed.
    </div>
  );
}

export function LinkToLogic({ a, href }: { a: Agent; href: string }) {
  return (
    <div className="card">
      <div className="card-head"><h3>Logic</h3><Link href={href} className="small">Open canvas</Link></div>
      <div className="dim small">{a.graph.nodes.length} blocks and {a.graph.edges.length} connections decide when this agent acts. Your limits above still apply whatever the logic says.</div>
    </div>
  );
}
