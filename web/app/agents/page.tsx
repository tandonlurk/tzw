"use client";

import Link from "next/link";
import { useMemo, useState } from "react";
import { api, type ActivityItem, type Agent } from "@/lib/api";
import { formatMoney, signOf } from "@/lib/money";
import { useAsync } from "@/lib/useAsync";
import { agentStyles, shortName } from "@/lib/agentColors";
import { portfolioSeries, type Series } from "@/lib/series";
import { winRateText } from "@/lib/agentView";
import { Async, Avatar, EnvBadge, PageHead } from "@/components/ui";
import { TimeChart } from "@/components/charts/TimeChart";
import { Icon } from "@/components/shell/icons";
import { timeOf } from "@/components/activity/Feed";

type Filter = "all" | "active" | "paused";

export default function AgentsPage() {
  const agents = useAsync(api.listAgents, "agents");
  const portfolio = useAsync(() => api.getPortfolio("simulated"), "portfolio");
  const activity = useAsync(() => api.listActivity({ limit: 200 }), "activity-200");
  const [filter, setFilter] = useState<Filter>("all");

  const series = useMemo(() => new Map((portfolio.data ? portfolioSeries(portfolio.data).agents : []).map((s) => [s.id, s])), [portfolio.data]);
  const latest = useMemo(() => {
    const m = new Map<string, ActivityItem>();
    for (const it of activity.data ?? []) if (it.agent_id && !m.has(it.agent_id)) m.set(it.agent_id, it);
    return m;
  }, [activity.data]);

  return (
    <div>
      <PageHead title="My Agents" sub="Each agent works for you inside the limits you set. Open one to see what it is doing.">
        <Link className="btn primary" href="/agents/new"><Icon name="plus" size={15} />New agent</Link>
      </PageHead>
      <Async state={agents} what="agents">
        {(list) => {
          const styles = agentStyles(list.map((a) => a.id));
          const shown = list.filter((a) => filter === "all" || a.status === filter);
          return (
            <>
              {list.length > 0 && (
                <div className="row spread" style={{ marginBottom: "var(--s-4)" }}>
                  <div className="seg" role="group" aria-label="Filter agents">
                    {(["all", "active", "paused"] as Filter[]).map((f) => (
                      <button key={f} type="button" className={filter === f ? "on" : ""} aria-pressed={filter === f} onClick={() => setFilter(f)}>
                        {f === "all" ? `All ${list.length}` : f === "active" ? `Active ${list.filter((a) => a.status === "active").length}` : `Paused ${list.filter((a) => a.status === "paused").length}`}
                      </button>
                    ))}
                  </div>
                </div>
              )}
              <div className="agent-grid">
                {shown.map((a) => (
                  <AgentCard key={a.id} a={a} color={styles.get(a.id)!.color} series={series.get(a.id)} last={latest.get(a.id)} />
                ))}
                <Link href="/agents/new" className="agent-card new">
                  <span className="plus"><Icon name="plus" size={20} /></span>
                  <span style={{ color: "var(--text)", fontWeight: 500 }}>Create an agent</span>
                  <span className="small" style={{ textAlign: "center", maxWidth: 220 }}>Start from a template, set its limits, and let it look for opportunities.</span>
                </Link>
              </div>
            </>
          );
        }}
      </Async>
    </div>
  );
}

function AgentCard({ a, color, series, last }: { a: Agent; color: string; series?: Series; last?: ActivityItem }) {
  const s = signOf(a.stats.realized_pnl);
  return (
    <Link href={`/agents/${a.id}`} className="agent-card">
      <div className="top">
        <Avatar name={a.name} color={color} size={40} />
        <div className="grow">
          <div className="row" style={{ gap: 8 }}>
            <span className="name">{shortName(a.name)}</span>
            <span className={`status-dot ${a.status}`} title={a.status} />
          </div>
          <div className="row" style={{ gap: 6, marginTop: 4 }}><EnvBadge env={a.env} /><span className="dim xs">v{a.version}</span></div>
        </div>
        <Icon name="arrow-up-right" size={16} className="dim" />
      </div>
      <div className="purpose">{a.description || a.config.goals || "No description yet."}</div>
      <div className="figs">
        <div className="kpi"><div className="kpi-label">Available</div><div className="kpi-value" style={{ fontSize: "1rem" }}>{formatMoney(a.balances.available, { unit: false })}</div></div>
        <div className="kpi"><div className="kpi-label">Committed</div><div className={`kpi-value${BigInt(a.balances.committed) > BigInt(0) ? " pend" : ""}`} style={{ fontSize: "1rem" }}>{formatMoney(a.balances.committed, { unit: false })}</div></div>
        <div className="kpi"><div className="kpi-label">P&amp;L</div><div className={`kpi-value ${s > 0 ? "pos" : s < 0 ? "neg" : ""}`} style={{ fontSize: "1rem" }}>{formatMoney(a.stats.realized_pnl, { signed: true, unit: false })}</div></div>
      </div>
      <div className="spark">
        <TimeChart compact controls={false} area height={44} ariaLabel={`${a.name} cumulative P&L`} formatValue={(v) => String(v)}
          lines={series ? [{ id: a.id, name: a.name, color, points: series.points }] : []} emptyText="No settled challenges yet" />
      </div>
      <div className="foot">
        <span className="last">{last ? last.summary : "No activity yet"}</span>
        <span className="num" style={{ flexShrink: 0 }}>{last ? timeOf(last.ts) : winRateText(a.stats.wins, a.stats.settled)}</span>
      </div>
    </Link>
  );
}
