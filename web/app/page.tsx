"use client";

import Link from "next/link";
import { useMemo } from "react";
import { api, type Agent, type Env, type Portfolio, type Wallet } from "@/lib/api";
import { formatMoney, signOf, sumMoney } from "@/lib/money";
import { useAsync } from "@/lib/useAsync";
import { agentStyles, shortName } from "@/lib/agentColors";
import { portfolioSeries } from "@/lib/series";
import { Async, Avatar, Empty, EnvBadge, Kpi, MoneyBar, PageHead, formatTs } from "@/components/ui";
import { DevTick } from "@/components/DevTick";
import { TimeChart, type ChartLine } from "@/components/charts/TimeChart";
import { FeedItem } from "@/components/activity/Feed";
import { Icon } from "@/components/shell/icons";

const usd = (v: number) => formatMoney(String(Math.round(v)), { signed: true });

function greeting(d = new Date()) {
  const h = d.getHours();
  return h < 5 ? "Working late" : h < 12 ? "Good morning" : h < 18 ? "Good afternoon" : "Good evening";
}

export default function Overview() {
  const agents = useAsync(api.listAgents, "agents");
  const wallet = useAsync(api.getWallet, "wallet");
  const portfolio = useAsync(() => api.getPortfolio("simulated"), "portfolio");
  const hasArc = agents.data?.some((a) => a.env === "arc_testnet") ?? false;
  const arc = useAsync(() => (hasArc ? api.getPortfolio("arc_testnet") : Promise.resolve(null)), `portfolio-arc:${hasArc}`);
  const approvals = useAsync(() => api.listApprovals("pending"), "approvals");
  const suggestions = useAsync(() => api.listSuggestions({ status: "pending" }), "pending-suggestions");
  const activity = useAsync(() => api.listActivity({ limit: 7 }), "activity-recent");
  const reloadAll = () => { agents.reload(); wallet.reload(); portfolio.reload(); arc.reload(); approvals.reload(); suggestions.reload(); activity.reload(); };
  const names = useMemo(() => new Map((agents.data ?? []).map((a) => [a.id, a.name])), [agents.data]);
  const styles = useMemo(() => agentStyles((agents.data ?? []).map((a) => a.id)), [agents.data]);

  return (
    <div>
      <PageHead
        eyebrow={new Date().toLocaleDateString(undefined, { weekday: "long", month: "long", day: "numeric" })}
        title={greeting()}
        sub="What your agents are doing, and what needs you."
      >
        <DevTick onDone={reloadAll} />
        <Link className="btn primary" href="/agents/new"><Icon name="plus" size={15} />New agent</Link>
      </PageHead>

      <Async state={wallet} what="wallet">
        {(w) => <Hero wallet={w} portfolio={portfolio.data} styles={styles} />}
      </Async>

      {hasArc && arc.data && <ArcStrip p={arc.data} />}

      <div className="grid-2" style={{ marginTop: "var(--gap)" }}>
        <div className={`card${(approvals.data?.length ?? 0) > 0 ? " attention" : ""}`}>
          <div className="card-head">
            <h3>Needs your attention</h3>
            <Link href="/approvals" className="small">All approvals</Link>
          </div>
          <Async state={approvals} what="approvals">
            {(list) => (
              <div className="list">
                {list.length === 0 && (suggestions.data?.length ?? 0) === 0 && <div className="dim small" style={{ padding: "8px 0" }}>Nothing waiting. Agents act on their own within your limits.</div>}
                {list.slice(0, 3).map((a) => (
                  <Link key={a.id} href="/approvals" className="list-row">
                    <span className="feed-ico c-approval"><Icon name="approval" size={15} /></span>
                    <div className="grow">
                      <div className="t">{shortName(a.agent_name)} wants to stake {formatMoney(a.stake)}</div>
                      <div className="s">{a.market_title}</div>
                    </div>
                    <EnvBadge env={a.env} />
                    <Icon name="chevron-right" size={14} className="dim" />
                  </Link>
                ))}
                {list.length > 3 && <div className="dim small" style={{ paddingTop: 8 }}>+{list.length - 3} more</div>}
                {(suggestions.data?.length ?? 0) > 0 && (
                  <Link href="/cocaptain" className="list-row">
                    <span className="feed-ico c-discovery"><Icon name="cocaptain" size={15} /></span>
                    <div className="grow">
                      <div className="t">Co-captain has {suggestions.data!.length} suggestion{suggestions.data!.length === 1 ? "" : "s"}</div>
                      <div className="s">{suggestions.data![0].title ?? "Review the suggested changes"}</div>
                    </div>
                    <Icon name="chevron-right" size={14} className="dim" />
                  </Link>
                )}
              </div>
            )}
          </Async>
        </div>

        <div className="card">
          <div className="card-head">
            <h3>Your agents</h3>
            <Link href="/agents" className="small">Manage</Link>
          </div>
          <Async state={agents} what="agents">
            {(list) => list.length === 0 ? (
              <Empty>No agents yet. <Link href="/agents/new">Create your first agent</Link>.</Empty>
            ) : (
              <div className="list">
                <div className="dim small" style={{ paddingBottom: 4 }}>{list.filter((a) => a.status === "active").length} of {list.length} active</div>
                {[...list].sort((a, b) => Number(BigInt(b.stats.realized_pnl) - BigInt(a.stats.realized_pnl))).slice(0, 5).map((a) => (
                  <AgentRow key={a.id} a={a} color={styles.get(a.id)?.color ?? "var(--blue)"} />
                ))}
              </div>
            )}
          </Async>
        </div>
      </div>

      <div className="card" style={{ marginTop: "var(--gap)" }}>
        <div className="card-head">
          <h3>Recent activity</h3>
          <Link href="/activity" className="small">View all</Link>
        </div>
        <Async state={activity} what="activity">
          {(items) => items.length === 0 ? <Empty>No activity yet. Events appear here as agents act.</Empty> : (
            <div className="feed">{items.map((it) => <FeedItem key={it.id} it={it} agentName={it.agent_id ? names.get(it.agent_id) : undefined} />)}</div>
          )}
        </Async>
      </div>
    </div>
  );
}

function AgentRow({ a, color }: { a: Agent; color: string }) {
  const s = signOf(a.stats.realized_pnl);
  return (
    <Link href={`/agents/${a.id}`} className="list-row">
      <Avatar name={a.name} color={color} size={30} />
      <div className="grow">
        <div className="t">{shortName(a.name)}</div>
        <div className="s"><span className={`status-dot ${a.status}`} style={{ marginRight: 6 }} />{a.status === "active" ? "Active" : "Paused"} · {a.stats.settled} settled</div>
      </div>
      <span className={`num small ${s > 0 ? "pos" : s < 0 ? "neg" : "dim"}`}>{formatMoney(a.stats.realized_pnl, { signed: true })}</span>
    </Link>
  );
}

function Hero({ wallet: w, portfolio: p, styles }: { wallet: Wallet; portfolio: Portfolio | null; styles: Map<string, { color: string; dash: string }> }) {
  const total = sumMoney([w.unallocated, w.allocated]);
  const agentAvailable = (BigInt(w.allocated) - BigInt(w.committed)).toString();
  const pnl = p?.totals.realized_pnl ?? "0";
  const lines: ChartLine[] = useMemo(() => {
    if (!p) return [];
    const s = portfolioSeries(p);
    return s.agents.map((a) => ({ id: a.id, name: shortName(a.name), color: styles.get(a.id)?.color ?? "var(--blue)", dash: styles.get(a.id)?.dash, points: a.points }));
  }, [p, styles]);

  return (
    <div className="card" style={{ padding: "var(--s-6)" }}>
      <div className="hero">
        <div className="stack" style={{ display: "flex", flexDirection: "column" }}>
          <div className="row" style={{ marginTop: 0 }}><span className="eyebrow">Portfolio value</span><EnvBadge env={w.env} /></div>
          <div className="big">{formatMoney(total, { unit: false })}<span className="unit">USDC</span></div>
          <div className="row small">
            <span className="dim">Realized P&amp;L</span>
            <span className={`num ${signOf(pnl) > 0 ? "pos" : signOf(pnl) < 0 ? "neg" : "text"}`}>{formatMoney(pnl, { signed: true })}</span>
            {p && <span className="dim">· {p.outcomes.settled} settled, {p.outcomes.open} open</span>}
          </div>
          <div style={{ marginTop: "auto", paddingTop: "var(--s-6)" }}>
            <MoneyBar
              label="Funds by state"
              parts={[
                { key: "u", label: "Unallocated", value: BigInt(w.unallocated), tone: "free", display: formatMoney(w.unallocated) },
                { key: "a", label: "With agents, free", value: BigInt(agentAvailable), tone: "alloc", display: formatMoney(agentAvailable) },
                { key: "c", label: "Committed to challenges", value: BigInt(w.committed), tone: "commit", display: formatMoney(w.committed) },
              ]}
            />
          </div>
        </div>
        <div style={{ minWidth: 0 }}>
          <div className="row spread" style={{ marginBottom: 6 }}>
            <span className="eyebrow">Realized P&amp;L by agent</span>
            {p && <span className="dim xs">as of {formatTs(p.as_of)}</span>}
          </div>
          <TimeChart lines={lines} formatValue={usd} ariaLabel="Cumulative realized P&L per agent" height={190} controls={false} legend emptyText="No settled challenges yet. P&L appears here after the first settlement." />
        </div>
      </div>
    </div>
  );
}

function ArcStrip({ p }: { p: Portfolio }) {
  return (
    <div className="card flat" style={{ marginTop: "var(--gap)" }}>
      <div className="row spread" style={{ marginBottom: 10 }}>
        <span className="row"><span className="eyebrow">Onchain agents</span><EnvBadge env={p.env as Env} /></span>
        <Link href="/wallet" className="small">Vaults and wallet</Link>
      </div>
      <div className="kpi-row">
        <Kpi label="Allocated">{formatMoney(p.totals.allocated)}</Kpi>
        <Kpi label="Committed" tone="pend">{formatMoney(p.totals.committed)}</Kpi>
        <Kpi label="Realized P&L" tone={signOf(p.totals.realized_pnl) > 0 ? "pos" : signOf(p.totals.realized_pnl) < 0 ? "neg" : undefined}>{formatMoney(p.totals.realized_pnl, { signed: true })}</Kpi>
        <Kpi label="Open challenges">{p.outcomes.open}</Kpi>
      </div>
    </div>
  );
}
