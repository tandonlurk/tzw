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
import { families, type Family } from "@/lib/multiply";

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
      <PageHead title="My Agents">
        <Link className="btn primary" href="/agents/new"><Icon name="plus" size={15} />New agent</Link>
      </PageHead>
      <Async state={agents} what="agents">
        {(list) => {
          const styles = agentStyles(list.map((a) => a.id));
          const fams = families(list);
          const cloneCount = new Map(fams.map((f) => [f.id, f.clones.length]));
          // Clones are listed under their family, not as cards (a 100x family would bury everything else).
          const shown = list.filter((a) => !a.clone && (filter === "all" || a.status === filter));
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
                  <AgentCard key={a.id} a={a} color={styles.get(a.id)!.color} series={series.get(a.id)} last={latest.get(a.id)} clones={cloneCount.get(a.id) ?? 0} />
                ))}
                <Link href="/agents/new" className="agent-card new">
                  <span className="plus"><Icon name="plus" size={20} /></span>
                  <span style={{ color: "var(--text)", fontWeight: 500 }}>Create an agent</span>
                  <span className="small" style={{ textAlign: "center", maxWidth: 220 }}>Start from a template, set its limits, and let it look for opportunities.</span>
                </Link>
              </div>
              {fams.length > 0 && (
                <section style={{ marginTop: "var(--s-6)" }} aria-labelledby="families-h">
                  <div className="card-head"><h3 id="families-h">Families</h3><span className="badge sim">Simulated</span></div>
                  <div className="stack">
                    {fams.map((f) => <FamilyCard key={f.id} f={f} filter={filter} />)}
                  </div>
                </section>
              )}
            </>
          );
        }}
      </Async>
    </div>
  );
}

function FamilyCard({ f, filter }: { f: Family; filter: Filter }) {
  const [open, setOpen] = useState(false);
  const pnl = signOf(f.realizedPnl);
  const clones = f.clones.filter((a) => filter === "all" || a.status === filter);
  const name = f.root ? shortName(f.root.name) : "Family";
  return (
    <div className="card">
      <div className="row spread">
        <div className="row" style={{ gap: 10 }}>
          <strong>{name}</strong>
          <span className="badge info plain">{f.clones.length} {f.clones.length === 1 ? "clone" : "clones"}</span>
          <span className="badge sim">Simulated</span>
        </div>
        <div className="row" style={{ gap: "var(--s-6)" }}>
          <div className="kpi"><div className="kpi-label">Family P&amp;L</div><div className={`kpi-value ${pnl > 0 ? "pos" : pnl < 0 ? "neg" : ""}`} style={{ fontSize: "1rem" }}>{formatMoney(f.realizedPnl, { signed: true, unit: false })}</div></div>
          <div className="kpi"><div className="kpi-label">Available</div><div className="kpi-value" style={{ fontSize: "1rem" }}>{formatMoney(f.available, { unit: false })}</div></div>
          <div className="kpi"><div className="kpi-label">Committed</div><div className={`kpi-value${BigInt(f.committed) > BigInt(0) ? " pend" : ""}`} style={{ fontSize: "1rem" }}>{formatMoney(f.committed, { unit: false })}</div></div>
          <div className="kpi"><div className="kpi-label">Record</div><div className="kpi-value" style={{ fontSize: "1rem" }}>{f.wins}W / {f.settled - f.wins}L</div></div>
          <button type="button" className="sm outline" aria-expanded={open} onClick={() => setOpen(!open)}>{open ? "Hide clones" : "Show clones"}</button>
        </div>
      </div>
      {open && (
        <div className="table-wrap" style={{ marginTop: "var(--s-3)" }}>
          <table className="dense">
            <thead><tr><th>Clone</th><th>Status</th><th className="num">Available</th><th className="num">Committed</th><th className="num">P&amp;L</th><th>Record</th></tr></thead>
            <tbody>
              {clones.map((a) => {
                const s = signOf(a.stats.realized_pnl);
                return (
                  <tr key={a.id}>
                    <td><Link href={`/agents/${a.id}`}>{shortName(a.name)}</Link></td>
                    <td><span className={`status-dot ${a.status}`} title={a.status} /> {a.status}</td>
                    <td className="num">{formatMoney(a.balances.available, { unit: false })}</td>
                    <td className="num">{formatMoney(a.balances.committed, { unit: false })}</td>
                    <td className={`num ${s > 0 ? "pos" : s < 0 ? "neg" : ""}`}>{formatMoney(a.stats.realized_pnl, { signed: true, unit: false })}</td>
                    <td>{a.stats.wins}W / {a.stats.settled - a.stats.wins}L</td>
                  </tr>
                );
              })}
            </tbody>
          </table>
          {clones.length === 0 && <div className="dim small" style={{ padding: 8 }}>No clones match this filter.</div>}
        </div>
      )}
    </div>
  );
}

function AgentCard({ a, color, series, last, clones = 0 }: { a: Agent; color: string; series?: Series; last?: ActivityItem; clones?: number }) {
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
          <div className="row" style={{ gap: 6, marginTop: 4 }}><EnvBadge env={a.env} /><span className="dim xs">v{a.version}</span>{clones > 0 && <span className="badge info plain" title="This agent was multiplied; see Families below">+{clones} clones</span>}</div>
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
          lines={series ? [{ id: a.id, name: a.name, color, points: series.points }] : []} emptyText="No settled positions yet" />
      </div>
      <div className="foot">
        <span className="last">{last ? last.summary : "No activity yet"}</span>
        <span className="num" style={{ flexShrink: 0 }}>{last ? timeOf(last.ts) : winRateText(a.stats.wins, a.stats.settled)}</span>
      </div>
    </Link>
  );
}
