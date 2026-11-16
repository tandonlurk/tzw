"use client";

import { use, useState } from "react";
import Link from "next/link";
import { useSearchParams } from "next/navigation";
import { api, isApiError, type Agent, type ApiError } from "@/lib/api";
import { formatMoney, parseMoney } from "@/lib/money";
import { useAsync } from "@/lib/useAsync";
import { ActivityExtra } from "@/components/ActivityDetail";
import { Async, Empty, ErrorBanner, EnvBadge, Money, StatusBadge, Tile, formatTs } from "@/components/ui";
import { LedgerTable } from "@/components/LedgerTable";
import { CollabCard } from "@/components/CollabCard";
import { InteractionCard } from "@/components/Interactions";
import { isOpen } from "@/lib/interactions";
import { usesCustomCode } from "@/lib/customCode";
import { interactionCounts, versionHistory, winRateText } from "@/lib/agentView";

export default function AgentDetailPage({ params }: { params: Promise<{ id: string }> }) {
  const { id } = use(params);
  const agent = useAsync(() => api.getAgent(id), `agent:${id}`);
  const ledger = useAsync(() => api.getAgentLedger(id), `ledger:${id}`);

  const interactions = useAsync(() => api.listInteractions({ agent_id: id }), `interactions:${id}`);
  const collabs = useAsync(() => api.listCollaborations({ agent_id: id }), `collabs:${id}`);
  const versions = useAsync(() => api.listAgentVersions(id), `versions:${id}`);
  const activity = useAsync(() => api.listActivity({ agent_id: id, limit: 20 }), `activity:${id}`);
  const saved = useSearchParams().get("saved");

  function refresh() {
    interactions.reload();
    versions.reload();
    collabs.reload();
    activity.reload();
    agent.reload();
    ledger.reload();
  }

  return (
    <div>
      {saved && /^\d+$/.test(saved) && (
        <div className="notice" role="status" style={{ marginBottom: "1rem" }}>
          Saved as version {saved}. Earlier versions, past challenges and the ledger were not changed.
        </div>
      )}
      <Async state={agent} what="agent">
        {(a) => <Detail agent={a} onChanged={refresh} active={interactions.data ? interactionCounts(interactions.data).active : null} />}
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
      <h2>Collaborations</h2>
      <Async state={collabs} what="collaborations">
        {(list) => list.length === 0 ? <Empty>This agent has no collaborations yet. <Link href="/collaborations">Propose one</Link>.</Empty> : (
          <div className="stack">{list.map((c) => <CollabCard key={c.id} c={c} />)}</div>
        )}
      </Async>
      <h2>Wallet transactions</h2>
      <Async state={ledger} what="ledger">{(entries) => <LedgerTable entries={entries} />}</Async>
      <h2>Recent activity</h2>
      <Async state={activity} what="activity">
        {(items) => items.length === 0 ? <Empty>No activity for this agent yet.</Empty> : (
          <div className="stack">
            {items.map((it) => (
              <div key={it.id} className="card">
                <div className="row spread"><span>{it.summary}</span><span className="dim">{formatTs(it.ts)}</span></div>
                <ActivityExtra it={it} />
                {it.interaction_id && <Link href={`/interactions/${it.interaction_id}`}>Open challenge</Link>}
              </div>
            ))}
          </div>
        )}
      </Async>
      <h2>Version history</h2>
      <div className="dim" style={{ marginBottom: ".5rem" }}>Each save creates a new version. Past decisions keep the version they ran under (see Decision trace on a challenge).</div>
      <Async state={versions} what="versions">
        {(list) => (
          <div className="stack">
            {versionHistory(list).map(({ v, changes }) => (
              <div key={v.version} className="card">
                <div className="row spread"><strong>Version {v.version}</strong><span className="dim">{formatTs(v.created_at)}</span></div>
                {changes.length > 0 ? <ul>{changes.map((c, k) => <li key={k}>{c}</li>)}</ul> : <div className="dim">{v.version === 1 ? "Initial version." : "No config, policy or graph change."}</div>}
              </div>
            ))}
          </div>
        )}
      </Async>
    </div>
  );
}

function Detail({ agent: a, onChanged, active }: { agent: Agent; onChanged: () => void; active: number | null }) {
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

  const winRate = winRateText(a.stats.wins, a.stats.settled);

  return (
    <div className="stack">
      <div className="page-head">
        <div>
          <h1>{a.name}</h1>
          <div className="row"><StatusBadge status={a.status} /><EnvBadge env={a.env} /><span className="dim">version {a.version}</span>{usesCustomCode(a.graph) && <span className="badge info" title="This agent's behavior graph contains a custom code block.">runs custom code</span>}</div>
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
        <Tile label="Unrealized P&L" note="Not computed: no fair mark for an unresolved challenge">n/a</Tile>
        <Tile label="Settled" note={winRate}>{a.stats.settled}</Tile>
        <Tile label="Active challenges">{active === null ? "..." : active}</Tile>
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
            {usesCustomCode(a.graph) && <div>Runs custom JavaScript in a sandbox. Its stakes still go through your limits. Failed runs appear in Recent activity as &quot;custom code failed&quot;.</div>}
            <details><summary>Raw graph</summary><pre>{JSON.stringify(a.graph, null, 2)}</pre></details>
          </>
        )}
      </div>
    </div>
  );
}
