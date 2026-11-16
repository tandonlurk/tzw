"use client";

import Link from "next/link";
import { api, type Agent, type Env } from "@/lib/api";
import { sumMoney } from "@/lib/money";
import { useAsync } from "@/lib/useAsync";
import { Async, Empty, EnvBadge, Money, Tile } from "@/components/ui";
import { PortfolioCharts } from "@/components/PortfolioCharts";
import { DevTick } from "@/components/DevTick";

function byEnv(agents: Agent[]) {
  const envs = Array.from(new Set(agents.map((a) => a.env))) as Env[];
  return envs.map((env) => {
    const list = agents.filter((a) => a.env === env);
    return {
      env,
      count: list.length,
      available: sumMoney(list.map((a) => a.balances.available)),
      committed: sumMoney(list.map((a) => a.balances.committed)),
      pnl: sumMoney(list.map((a) => a.stats.realized_pnl)),
      settled: list.reduce((n, a) => n + a.stats.settled, 0),
      wins: list.reduce((n, a) => n + a.stats.wins, 0),
    };
  });
}

export default function Dashboard() {
  const agents = useAsync(api.listAgents, "agents");
  const wallet = useAsync(api.getWallet, "wallet");
  const portfolio = useAsync(api.getPortfolio, "portfolio");
  const pending = useAsync(() => api.listSuggestions({ status: "pending" }), "pending-suggestions");
  const reloadAll = () => { agents.reload(); wallet.reload(); portfolio.reload(); pending.reload(); };

  return (
    <div>
      <div className="page-head"><div><h1>Dashboard</h1><div className="dim">Totals computed from your agents and wallet.</div></div><DevTick onDone={reloadAll} /></div>

      {pending.error ? (
        <div className="dim">Co-captain suggestions unavailable: {pending.error.message}</div>
      ) : pending.data && (
        <Link href="/cocaptain" className={`notice${pending.data.length ? "" : " dim"}`} style={{ display: "block", marginBottom: "1rem" }}>
          {pending.data.length === 0
            ? "Co-captain: no pending suggestions."
            : `Co-captain: ${pending.data.length} pending suggestion${pending.data.length === 1 ? "" : "s"} to review.`}
        </Link>
      )}

      <h2>Challenges and performance</h2>
      <Async state={portfolio} what="portfolio">{(p) => <PortfolioCharts p={p} />}</Async>

      <h2>Wallet</h2>
      <Async state={wallet} what="wallet">
        {(w) => (
          <div className="grid">
            <Tile label="Unallocated" note={<EnvBadge env={w.env} />}><Money value={w.unallocated} /></Tile>
            <Tile label="Allocated to agents"><Money value={w.allocated} /></Tile>
            <Tile label="Committed"><Money value={w.committed} /></Tile>
          </div>
        )}
      </Async>

      <h2>Agents</h2>
      <Async state={agents} what="agents">
        {(list) =>
          list.length === 0 ? (
            <Empty>No agents yet. <Link href="/agents/new">Create your first agent</Link>.</Empty>
          ) : (
            <div className="stack">
              <div className="grid">
                <Tile label="Agents" note={`${list.filter((a) => a.status === "active").length} active`}>{list.length}</Tile>
              </div>
              {byEnv(list).map((g) => (
                <div key={g.env}>
                  <div className="row" style={{ marginBottom: ".4rem" }}><EnvBadge env={g.env} /><span className="dim">{g.count} agent(s)</span></div>
                  <div className="grid">
                    <Tile label="Agent available"><Money value={g.available} /></Tile>
                    <Tile label="Agent committed"><Money value={g.committed} /></Tile>
                    <Tile label="Realized P&L"><Money value={g.pnl} signed tone /></Tile>
                    <Tile label="Agent results: settled / wins" note="Each challenge counts once per side, so 3 challenges show as 6">{g.settled} / {g.wins}</Tile>
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
