"use client";

import Link from "next/link";
import type { ActivityItem, Agent, Portfolio } from "@/lib/api";
import { formatMoney, signOf } from "@/lib/money";
import { shortName, type AgentStyle } from "@/lib/agentColors";
import { portfolioSeries } from "@/lib/series";
import type { Panel } from "@/lib/layouts";
import { TimeChart, type ChartLine } from "@/components/charts/TimeChart";
import { ActivityKindBadge } from "@/components/ActivityDetail";
import { timeOf } from "@/components/activity/Feed";
import { ADV } from "@/lib/mode";

export interface TerminalData {
  portfolio: Portfolio;
  agents: Agent[];
  activity: ActivityItem[] | null;
  styles: Map<string, AgentStyle>;
}

const usd = (v: number) => formatMoney(String(Math.round(v)), { signed: true });
const tone = (m: string) => (signOf(m) > 0 ? "pos" : signOf(m) < 0 ? "neg" : "");

/** Agents ordered by |realized P&L| (the "slot" order used by preset single-agent panels). */
export function rankedAgentIds(p: Portfolio): string[] {
  const abs = (m: string) => { const v = BigInt(m); return v < BigInt(0) ? -v : v; };
  return [...p.agents].sort((a, b) => {
    const d = abs(b.realized_pnl) - abs(a.realized_pnl);
    return d > BigInt(0) ? 1 : d < BigInt(0) ? -1 : b.settled - a.settled;
  }).map((a) => a.agent_id);
}

export function panelAgentId(panel: Panel, d: TerminalData): string | null {
  if (panel.agentId) return panel.agentId;
  return rankedAgentIds(d.portfolio)[panel.slot ?? 0] ?? null;
}

export function PanelBody({ panel, d }: { panel: Panel; d: TerminalData }) {
  switch (panel.metric) {
    case "pnl": return <PnlPanel d={d} />;
    case "agent_pnl": return <AgentPnlPanel id={panelAgentId(panel, d)} d={d} />;
    case "pnl_by_agent": return <PnlBars d={d} />;
    case "allocation": return <Allocation d={d} />;
    case "outcomes": return <Outcomes p={d.portfolio} />;
    case "record": return <Record d={d} />;
    case "agents": return <AgentsTable d={d} />;
    case "activity": return <ActivityList d={d} />;
  }
}

function PnlPanel({ d }: { d: TerminalData }) {
  const s = portfolioSeries(d.portfolio);
  const lines: ChartLine[] = [
    ...s.agents.map((a) => ({ id: a.id, name: shortName(a.name), color: d.styles.get(a.id)?.color ?? "var(--blue)", dash: d.styles.get(a.id)?.dash, points: a.points })),
    ...(s.total.points.length ? [{ id: s.total.id, name: "Portfolio", color: "var(--offwhite)", dash: "5 4", points: s.total.points, emphasis: true }] : []),
  ];
  return <TimeChart lines={lines} formatValue={usd} ariaLabel="Cumulative realized P&L, agents and portfolio" height="fill" emptyText="No settled challenges yet." />;
}

function AgentPnlPanel({ id, d }: { id: string | null; d: TerminalData }) {
  const s = portfolioSeries(d.portfolio).agents.find((a) => a.id === id);
  const a = d.portfolio.agents.find((x) => x.agent_id === id);
  if (!id || !a) return <div className="tchart-empty">No agent in this slot yet.</div>;
  const color = d.styles.get(id)?.color ?? "var(--blue)";
  return (
    <div className="tp-agent">
      <div className="tp-agent-head">
        <span className={`num ${tone(a.realized_pnl)}`} style={{ fontSize: "1.1rem" }}>{formatMoney(a.realized_pnl, { signed: true })}</span>
        <span className="dim xs">{a.wins}W / {a.losses}L{a.win_rate_bps !== null ? ` · ${(a.win_rate_bps / 100).toFixed(0)}%` : ""}</span>
      </div>
      <TimeChart lines={s ? [{ id, name: shortName(a.name), color, points: s.points }] : []} formatValue={usd} ariaLabel={`${a.name} cumulative P&L`}
        height="fill" area legend={false} emptyText="No settled challenges yet." />
    </div>
  );
}

function PnlBars({ d }: { d: TerminalData }) {
  const rows = d.portfolio.agents;
  const max = Math.max(1, ...rows.map((a) => Math.abs(Number(a.realized_pnl))));
  if (rows.every((a) => a.settled === 0)) return <div className="tchart-empty">No settled challenges yet.</div>;
  return (
    <div className="dbars" role="list" aria-label="Realized P&L by agent">
      {rows.map((a) => {
        const v = Number(a.realized_pnl);
        const pct = (Math.abs(v) / max) * 50;
        return (
          <div key={a.agent_id} className="dbar" role="listitem">
            <span className="n" title={a.name}><span className="sw" style={{ background: d.styles.get(a.agent_id)?.color }} />{shortName(a.name)}</span>
            <span className="track"><span className="mid" /><span className={`fill ${v >= 0 ? "pos" : "neg"}`} style={v >= 0 ? { left: "50%", width: `${pct}%` } : { right: "50%", width: `${pct}%` }} /></span>
            <span className={`v num ${tone(a.realized_pnl)}`}>{formatMoney(a.realized_pnl, { signed: true, unit: false })}</span>
          </div>
        );
      })}
    </div>
  );
}

function Allocation({ d }: { d: TerminalData }) {
  const rows = d.portfolio.allocation;
  const totals = rows.map((r) => BigInt(r.available) + BigInt(r.committed));
  const max = totals.reduce((m, t) => (t > m ? t : m), BigInt(0));
  if (max === BigInt(0)) return <div className="tchart-empty">No capital allocated to agents yet.</div>;
  const pct = (v: string) => Number((BigInt(v) * BigInt(10000)) / max) / 100;
  return (
    <div className="dbars" role="list" aria-label="Capital allocation">
      {rows.map((r) => (
        <div key={r.agent_id} className="dbar alloc" role="listitem" title={`${formatMoney(r.available)} free, ${formatMoney(r.committed)} committed`}>
          <span className="n"><span className="sw" style={{ background: d.styles.get(r.agent_id)?.color }} />{shortName(r.name)}</span>
          <span className="track"><span className="fill info" style={{ left: 0, width: `${pct(r.available)}%` }} /><span className="fill pend" style={{ left: `${pct(r.available)}%`, width: `${pct(r.committed)}%` }} /></span>
          <span className="v num">{formatMoney((BigInt(r.available) + BigInt(r.committed)).toString(), { unit: false })}</span>
        </div>
      ))}
      <div className="dlegend"><span><i className="info" />Free</span><span><i className="pend" />Committed</span><span className="dim">USDC</span></div>
    </div>
  );
}

function Outcomes({ p }: { p: Portfolio }) {
  const o = p.outcomes;
  const items = [
    { k: "Settled", v: o.settled, c: "pos" },
    { k: "Open", v: o.open, c: "pend", note: `${o.active} active` },
    { k: "Voided", v: o.voided, c: "info", note: "refunded" },
    { k: "Cancelled", v: o.cancelled, c: "dim" },
    { k: "Failed", v: o.failed, c: "neg" },
  ];
  const total = items.reduce((t, i) => t + i.v, 0);
  if (total === 0) return <div className="tchart-empty">No challenges yet.</div>;
  return (
    <div className="outcomes">
      <div className="ostack" role="img" aria-label="Challenge outcomes">
        {items.filter((i) => i.v > 0).map((i) => <span key={i.k} className={`os ${i.c}`} style={{ flex: i.v }} title={`${i.k}: ${i.v}`} />)}
      </div>
      <div className="ogrid">
        {items.map((i) => (
          <div key={i.k} className="oi"><span className={`dotc ${i.c}`} /><span className="dim">{i.k}</span><span className="num text">{i.v}</span>{i.note && <span className="dim xs">{i.note}</span>}</div>
        ))}
      </div>
    </div>
  );
}

function Record({ d }: { d: TerminalData }) {
  const rows = d.portfolio.agents.filter((a) => a.settled > 0);
  if (rows.length === 0) return <div className="tchart-empty">No settled challenges yet.</div>;
  const max = Math.max(...rows.map((a) => a.wins + a.losses));
  return (
    <div className="dbars" role="list" aria-label="Wins and losses per agent">
      {rows.map((a) => (
        <div key={a.agent_id} className="dbar" role="listitem">
          <span className="n"><span className="sw" style={{ background: d.styles.get(a.agent_id)?.color }} />{shortName(a.name)}</span>
          <span className="track"><span className="fill pos" style={{ left: 0, width: `${(a.wins / max) * 100}%` }} /><span className="fill neg" style={{ left: `${(a.wins / max) * 100}%`, width: `${(a.losses / max) * 100}%` }} /></span>
          <span className="v num">{a.wins}–{a.losses}{a.win_rate_bps !== null && <span className="dim"> {(a.win_rate_bps / 100).toFixed(0)}%</span>}</span>
        </div>
      ))}
    </div>
  );
}

function AgentsTable({ d }: { d: TerminalData }) {
  const byId = new Map(d.portfolio.agents.map((a) => [a.agent_id, a]));
  const rows = d.agents.filter((a) => a.env === d.portfolio.env);
  if (!rows.length) return <div className="tchart-empty">No agents in this environment.</div>;
  return (
    <div className="table-wrap tp-scroll">
      <table className="dense">
        <thead><tr><th>Agent</th><th>Status</th><th className="num">Available</th><th className="num">Committed</th><th className="num">Realized P&amp;L</th><th className="num">W / L</th></tr></thead>
        <tbody>
          {rows.map((a) => {
            const s = byId.get(a.id);
            return (
              <tr key={a.id}>
                <td><Link href={`${ADV}/agents/${a.id}`} className="row nowrap" style={{ gap: 8, color: "var(--text)" }}><span className="sw" style={{ background: d.styles.get(a.id)?.color }} />{shortName(a.name)}</Link></td>
                <td><span className={`status-dot ${a.status}`} /> <span className="dim xs">{a.status}</span></td>
                <td className="num">{formatMoney(a.balances.available, { unit: false })}</td>
                <td className={`num${BigInt(a.balances.committed) > BigInt(0) ? " pend" : ""}`}>{formatMoney(a.balances.committed, { unit: false })}</td>
                <td className={`num ${tone(a.stats.realized_pnl)}`}>{formatMoney(a.stats.realized_pnl, { signed: true, unit: false })}</td>
                <td className="num">{s ? `${s.wins} / ${s.losses}` : "–"}</td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

function ActivityList({ d }: { d: TerminalData }) {
  const names = new Map(d.agents.map((a) => [a.id, shortName(a.name)]));
  if (!d.activity) return <div className="tchart-empty">Loading activity…</div>;
  if (!d.activity.length) return <div className="tchart-empty">No activity yet.</div>;
  return (
    <div className="tp-scroll">
      {d.activity.slice(0, 30).map((it) => (
        <div key={it.id} className="act-row">
          <span className="num dim xs">{timeOf(it.ts)}</span>
          <ActivityKindBadge kind={it.kind} />
          <span className="grow s">{it.summary}</span>
          {it.agent_id && <span className="dim xs">{names.get(it.agent_id)}</span>}
        </div>
      ))}
    </div>
  );
}
