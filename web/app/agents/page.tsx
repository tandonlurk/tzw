"use client";

import Link from "next/link";
import { api } from "@/lib/api";
import { useAsync } from "@/lib/useAsync";
import { Async, Empty, EnvBadge, Money, StatusBadge } from "@/components/ui";

export default function AgentsPage() {
  const agents = useAsync(api.listAgents, "agents");
  return (
    <div>
      <div className="page-head">
        <div><h1>My Agents</h1></div>
        <Link className="btn primary" href="/agents/new">Create agent</Link>
      </div>
      <Async state={agents} what="agents">
        {(list) =>
          list.length === 0 ? (
            <Empty>No agents yet. <Link href="/agents/new">Create one</Link> from a template.</Empty>
          ) : (
            <div className="grid">
              {list.map((a) => (
                <div key={a.id} className="card stack">
                  <div className="row spread">
                    <strong><Link href={`/agents/${a.id}`}>{a.name}</Link></strong>
                    <StatusBadge status={a.status} />
                  </div>
                  <div className="row"><EnvBadge env={a.env} /><span className="dim">v{a.version}</span></div>
                  <table>
                    <tbody>
                      <tr><td className="dim">Available</td><td className="num"><Money value={a.balances.available} /></td></tr>
                      <tr><td className="dim">Committed</td><td className="num"><Money value={a.balances.committed} /></td></tr>
                      <tr><td className="dim">Realized P&L</td><td className="num"><Money value={a.stats.realized_pnl} signed tone /></td></tr>
                      <tr><td className="dim">Settled / wins</td><td className="num">{a.stats.settled} / {a.stats.wins}</td></tr>
                    </tbody>
                  </table>
                  <div className="row">
                    <Link className="btn sm" href={`/agents/${a.id}`}>Details</Link>
                    <Link className="btn sm" href={`/agents/${a.id}/edit`}>Edit</Link>
                  </div>
                </div>
              ))}
            </div>
          )
        }
      </Async>
    </div>
  );
}
