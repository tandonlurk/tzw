"use client";

import Link from "next/link";
import { useSearchParams } from "next/navigation";
import { useMemo } from "react";
import { api } from "@/lib/api";
import { formatMoney } from "@/lib/money";
import { useAsync } from "@/lib/useAsync";
import { isOpen, stateLabel, stateTone } from "@/lib/interactions";
import { portfolioSeries } from "@/lib/series";
import { categoryOf } from "@/lib/activityKinds";
import { humanSecs } from "@/components/AgentForm";
import { Async, Empty, formatTs } from "@/components/ui";
import { useWorkspace } from "@/components/agent/AgentWorkspace";
import { FundingCard, RiskCard, SavedNotice } from "@/components/agent/sections";
import { TimeChart } from "@/components/charts/TimeChart";
import { ActivityKindBadge } from "@/components/ActivityDetail";
import { VaultPanel } from "@/components/wallet/VaultPanel";
import { timeOf } from "@/components/activity/Feed";
import { ADV } from "@/lib/mode";

const usd = (v: number) => formatMoney(String(Math.round(v)), { signed: true });

export default function AdvancedAgentOverview() {
  const { agent, color, refresh, bump } = useWorkspace();
  const a = agent.data!;
  const portfolio = useAsync(() => api.getPortfolio(a.env), `portfolio:${a.env}:${bump}`);
  const interactions = useAsync(() => api.listInteractions({ agent_id: a.id }), `interactions:${a.id}:${bump}`);
  const activity = useAsync(() => api.listActivity({ agent_id: a.id, limit: 40 }), `activity:${a.id}:${bump}`);
  const saved = useSearchParams().get("saved");
  const series = useMemo(() => (portfolio.data ? portfolioSeries(portfolio.data).agents.find((s) => s.id === a.id) : undefined), [portfolio.data, a.id]);
  const lastDecision = activity.data?.find((it) => ["discovery", "challenge", "negotiation"].includes(categoryOf(it.kind)));

  return (
    <div>
      <SavedNotice saved={saved} />
      <div className="grid-3" style={{ alignItems: "start" }}>
        <div className="stack" style={{ gridColumn: "span 2", minWidth: 0 }}>
          <div className="card">
            <div className="card-head"><h3>Cumulative realized P&amp;L</h3><Link href={`${ADV}/agents/${a.id}/performance`} className="small">Performance</Link></div>
            <TimeChart lines={series ? [{ id: a.id, name: a.name, color, points: series.points }] : []} formatValue={usd} ariaLabel={`${a.name} cumulative P&L`} height={260} area legend={false} emptyText="No settled challenges yet." />
          </div>
          <div className="card pad-0">
            <div className="card-head" style={{ padding: "12px 14px 0" }}><h3>Current commitments</h3><span className="dim xs">stakes locked in open challenges</span></div>
            <Async state={interactions} what="challenges">
              {(list) => {
                const open = list.filter((i) => isOpen(i.state));
                return open.length === 0 ? <div className="dim small" style={{ padding: 14 }}>No open challenges.</div> : (
                  <div className="table-wrap">
                    <table className="dense">
                      <thead><tr><th>Market</th><th>Against</th><th>State</th><th className="num">Stake</th><th>Ends</th></tr></thead>
                      <tbody>
                        {open.map((i) => (
                          <tr key={i.id}>
                            <td><Link href={`/interactions/${i.id}`}>{i.market_title}</Link></td>
                            <td>{i.proposer_id === a.id ? i.counterparty_name : i.proposer_name}</td>
                            <td><span className={`badge ${stateTone(i.state)}`}>{stateLabel(i.state)}</span></td>
                            <td className="num">{formatMoney(i.stake, { unit: false })}</td>
                            <td className="dim num">{formatTs(i.end_at)}</td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                );
              }}
            </Async>
          </div>
          <div className="card pad-0">
            <div className="card-head" style={{ padding: "12px 14px 0" }}><h3>Recent actions</h3><Link href={`${ADV}/agents/${a.id}/activity`} className="small">All history</Link></div>
            <Async state={activity} what="activity">
              {(items) => items.length === 0 ? <div style={{ padding: 14 }}><Empty>No actions yet.</Empty></div> : (
                <div style={{ padding: "4px 14px 10px" }}>
                  {items.slice(0, 10).map((it) => (
                    <div key={it.id} className="act-row">
                      <span className="num dim xs" style={{ width: 64 }}>{timeOf(it.ts)}</span>
                      <ActivityKindBadge kind={it.kind} />
                      <span className="grow s">{it.summary}</span>
                      {it.interaction_id && <Link href={`/interactions/${it.interaction_id}`} className="xs">open</Link>}
                    </div>
                  ))}
                </div>
              )}
            </Async>
          </div>
        </div>
        <div className="stack" style={{ minWidth: 0 }}>
          <div className="card">
            <div className="card-head"><h3>Status</h3></div>
            <table className="kv"><tbody>
              <tr><td>State</td><td className="text">{a.status === "active" ? "Running" : "Paused"}</td></tr>
              <tr><td>Checks</td><td>every {humanSecs(String(a.config.schedule_secs))}</td></tr>
              <tr><td>Horizon</td><td>{humanSecs(String(a.config.horizon_secs))}</td></tr>
              <tr><td>Risk</td><td>{a.config.risk_tolerance}</td></tr>
              <tr><td>Last decision</td><td>{lastDecision ? formatTs(lastDecision.ts) : "–"}</td></tr>
              <tr><td>Logic</td><td><Link href={`${ADV}/agents/${a.id}/logic`}>{a.graph.nodes.length} blocks</Link></td></tr>
            </tbody></table>
          </div>
          <RiskCard a={a} />
          <FundingCard a={a} onChanged={refresh} />
        </div>
      </div>
      <div style={{ marginTop: "var(--gap)" }}><VaultPanel agent={a} onChanged={refresh} /></div>
    </div>
  );
}
