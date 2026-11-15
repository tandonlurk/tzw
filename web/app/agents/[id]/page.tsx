"use client";

import { use, useState } from "react";
import Link from "next/link";
import { api, isApiError, type Agent, type ApiError } from "@/lib/api";
import { formatMoney, parseMoney } from "@/lib/money";
import { useAsync } from "@/lib/useAsync";
import { Async, Empty, ErrorBanner, EnvBadge, Money, StatusBadge, Tile } from "@/components/ui";
import { LedgerTable } from "@/components/LedgerTable";
import { InteractionCard } from "@/components/Interactions";
import { isOpen } from "@/lib/interactions";

export default function AgentDetailPage({ params }: { params: Promise<{ id: string }> }) {
  const { id } = use(params);
  const agent = useAsync(() => api.getAgent(id), `agent:${id}`);
  const ledger = useAsync(() => api.getAgentLedger(id), `ledger:${id}`);

  const interactions = useAsync(() => api.listInteractions({ agent_id: id }), `interactions:${id}`);

  function refresh() {
    interactions.reload();
    agent.reload();
    ledger.reload();
  }

  return (
    <div>
      <Async state={agent} what="agent">
        {(a) => <Detail agent={a} onChanged={refresh} />}
      </Async>
      <h2>Challenges</h2>
      <Async state={interactions} what="challenges">
        {(list) => {
          const open = list.filter((i) => isOpen(i.state));
          const past = list.filter((i) => !isOpen(i.state));
          return list.length === 0 ? <Empty>This agent has no challenges yet.</Empty> : (
            <div className="stack">
              {open.length > 0 && <><h3>Active and in progress</h3>{open.map((i) => <InteractionCard key={i.id} i={i} />)}</>}
              {past.length > 0 && <><h3>History</h3>{past.map((i) => <InteractionCard key={i.id} i={i} />)}</>}
            </div>
          );
        }}
      </Async>
      <h2>Ledger</h2>
      <Async state={ledger} what="ledger">{(entries) => <LedgerTable entries={entries} />}</Async>
    </div>
  );
}

function Detail({ agent: a, onChanged }: { agent: Agent; onChanged: () => void }) {
  const [amount, setAmount] = useState("10");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  const [localErr, setLocalErr] = useState<string | null>(null);

  async function run(fn: () => Promise<unknown>) {
    setBusy(true);
    setError(null);
    setLocalErr(null);
    try {
      await fn();
      onChanged();
    } catch (e) {
      if (isApiError(e)) setError(e);
    } finally {
      setBusy(false);
    }
  }

  function move(kind: "allocate" | "deallocate") {
    const minor = parseMoney(amount);
    if (!minor || minor === "0") return setLocalErr("Enter a positive amount with at most 6 decimals.");
    run(() => (kind === "allocate" ? api.allocate(a.id, minor) : api.deallocate(a.id, minor)));
  }

  const winRate = a.stats.settled > 0 ? `${a.stats.wins} of ${a.stats.settled} won` : "No settled interactions";

  return (
    <div className="stack">
      <div className="page-head">
        <div>
          <h1>{a.name}</h1>
          <div className="row"><StatusBadge status={a.status} /><EnvBadge env={a.env} /><span className="dim">version {a.version}</span></div>
          {a.description && <div className="dim" style={{ marginTop: ".4rem" }}>{a.description}</div>}
        </div>
        <div className="row">
          <button disabled={busy} onClick={() => run(() => api.setAgentStatus(a.id, a.status === "active" ? "paused" : "active"))}>
            {a.status === "active" ? "Pause" : "Resume"}
          </button>
          <Link className="btn primary" href={`/agents/${a.id}/edit`}>Edit</Link>
        </div>
      </div>
      <ErrorBanner error={error} />

      <div className="grid">
        <Tile label="Available"><Money value={a.balances.available} /></Tile>
        <Tile label="Committed"><Money value={a.balances.committed} /></Tile>
        <Tile label="Realized P&L"><Money value={a.stats.realized_pnl} signed tone /></Tile>
        <Tile label="Settled" note={winRate}>{a.stats.settled}</Tile>
      </div>
      {a.wallet_address && <div className="dim">Wallet: <span className="mono">{a.wallet_address}</span></div>}

      <div className="card">
        <h3 style={{ marginTop: 0 }}>Funding</h3>
        {a.env === "simulated" ? (
          <>
            <div className="dim" style={{ marginBottom: ".5rem" }}>Moves simulated funds between your unallocated balance and this agent. Not real funds.</div>
            <div className="row">
              <input style={{ maxWidth: 160 }} value={amount} onChange={(e) => setAmount(e.target.value)} aria-label="Amount in USDC" inputMode="decimal" />
              <span className="dim">USDC (simulated)</span>
              <button disabled={busy} onClick={() => move("allocate")}>Allocate</button>
              <button disabled={busy} onClick={() => move("deallocate")}>Deallocate</button>
            </div>
            {localErr && <div className="field-error">{localErr}</div>}
          </>
        ) : (
          <div className="dim">Allocation for arc_testnet agents is not available in this milestone.</div>
        )}
      </div>

      <div className="grid-2">
        <div className="card">
          <h3 style={{ marginTop: 0 }}>Config</h3>
          <table><tbody>
            <tr><td className="dim">Goals</td><td>{a.config.goals || "-"}</td></tr>
            <tr><td className="dim">Strategy</td><td>{a.config.strategy || "-"}</td></tr>
            <tr><td className="dim">Categories</td><td>{a.config.categories.join(", ") || "-"}</td></tr>
            <tr><td className="dim">Horizon</td><td>{a.config.horizon_secs}s</td></tr>
            <tr><td className="dim">Schedule</td><td>every {a.config.schedule_secs}s</td></tr>
            <tr><td className="dim">Risk tolerance</td><td>{a.config.risk_tolerance}</td></tr>
          </tbody></table>
        </div>
        <div className="card">
          <h3 style={{ marginTop: 0 }}>Policy (enforced)</h3>
          <table><tbody>
            <tr><td className="dim">Max per interaction</td><td>{formatMoney(a.policy.max_per_interaction)}</td></tr>
            <tr><td className="dim">Max aggregate exposure</td><td>{formatMoney(a.policy.max_aggregate_exposure)}</td></tr>
            <tr><td className="dim">Approval above</td><td>{formatMoney(a.policy.approval_threshold)}</td></tr>
            <tr><td className="dim">Allowed types</td><td>{a.policy.allowed_interaction_types.join(", ") || "none"}</td></tr>
            <tr><td className="dim">Min conviction</td><td>{a.policy.min_conviction_bps} bps</td></tr>
            <tr><td className="dim">Stop loss</td><td>{a.policy.stop_loss === null ? "none" : formatMoney(a.policy.stop_loss)}</td></tr>
          </tbody></table>
        </div>
      </div>

      <div className="card">
        <h3 style={{ marginTop: 0 }}>Behavior graph</h3>
        {a.graph.nodes.length === 0 ? (
          <Empty>No blocks configured.</Empty>
        ) : (
          <>
            <div className="dim">{a.graph.nodes.length} block(s), {a.graph.edges.length} connection(s). Open Edit to view or change the canvas.</div>
            <details><summary>Raw graph</summary><pre>{JSON.stringify(a.graph, null, 2)}</pre></details>
          </>
        )}
      </div>
    </div>
  );
}
