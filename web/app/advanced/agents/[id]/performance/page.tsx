"use client";

import Link from "next/link";
import { useState } from "react";
import { api } from "@/lib/api";
import { formatMoney, signOf, sumMoney } from "@/lib/money";
import { useAsync } from "@/lib/useAsync";
import { agentStyles, shortName } from "@/lib/agentColors";
import { portfolioSeries } from "@/lib/series";
import { Async, Kpi, formatTs } from "@/components/ui";
import { useWorkspace } from "@/components/agent/AgentWorkspace";
import { TimeChart, type ChartLine } from "@/components/charts/TimeChart";

const usd = (v: number) => formatMoney(String(Math.round(v)), { signed: true });
const tone = (m: string) => (signOf(m) > 0 ? "pos" : signOf(m) < 0 ? "neg" : undefined);

export default function PerformancePage() {
  const { agent, color, bump } = useWorkspace();
  const a = agent.data!;
  const portfolio = useAsync(() => api.getPortfolio(a.env), `portfolio:${a.env}:${bump}`);
  const [compare, setCompare] = useState<string[]>([]);

  return (
    <Async state={portfolio} what="performance">
      {(p) => {
        const all = portfolioSeries(p);
        const styles = agentStyles(p.agents.map((x) => x.agent_id).concat(a.id));
        const mine = all.agents.find((s) => s.id === a.id);
        const events = p.pnl_over_time.filter((e) => e.agent_id === a.id);
        const deltas = events.map((e) => e.delta);
        const wins = deltas.filter((d) => signOf(d) > 0);
        const losses = deltas.filter((d) => signOf(d) < 0);
        const best = deltas.reduce<string | null>((m, d) => (m === null || BigInt(d) > BigInt(m) ? d : m), null);
        const worst = deltas.reduce<string | null>((m, d) => (m === null || BigInt(d) < BigInt(m) ? d : m), null);
        const avg = deltas.length ? (BigInt(sumMoney(deltas)) / BigInt(deltas.length)).toString() : null;
        const stats = p.agents.find((x) => x.agent_id === a.id);
        const lines: ChartLine[] = [
          ...(mine ? [{ id: a.id, name: shortName(a.name), color, points: mine.points, emphasis: true }] : []),
          ...all.agents.filter((s) => compare.includes(s.id)).map((s) => ({ id: s.id, name: shortName(s.name), color: styles.get(s.id)!.color, dash: "5 4", points: s.points })),
        ];
        let cum = BigInt(0);
        const rows = events.map((e) => { cum += BigInt(e.delta); return { ...e, cum: cum.toString() }; }).reverse();

        return (
          <div className="stack">
            <div className="card">
              <div className="kpi-row">
                <Kpi label="Realized P&L" tone={tone(a.stats.realized_pnl)}>{formatMoney(a.stats.realized_pnl, { signed: true })}</Kpi>
                <Kpi label="Settled">{stats?.settled ?? 0}</Kpi>
                <Kpi label="Wins / losses">{stats ? `${stats.wins} / ${stats.losses}` : "0 / 0"}</Kpi>
                <Kpi label="Win rate" note="voided excluded">{stats?.win_rate_bps != null ? `${(stats.win_rate_bps / 100).toFixed(0)}%` : "–"}</Kpi>
                <Kpi label="Avg per challenge" tone={avg ? tone(avg) : undefined}>{avg ? formatMoney(avg, { signed: true }) : "–"}</Kpi>
                <Kpi label="Best" tone="pos">{best && signOf(best) > 0 ? formatMoney(best, { signed: true }) : "–"}</Kpi>
                <Kpi label="Worst" tone="neg">{worst && signOf(worst) < 0 ? formatMoney(worst, { signed: true }) : "–"}</Kpi>
              </div>
            </div>
            <div className="card">
              <div className="card-head">
                <h3>Cumulative realized P&amp;L</h3>
                <div className="row" style={{ gap: 4 }}>
                  <span className="dim xs">Compare with</span>
                  {all.agents.filter((s) => s.id !== a.id).map((s) => {
                    const on = compare.includes(s.id);
                    return (
                      <button key={s.id} type="button" className={`chip${on ? " on" : ""}`} style={{ height: 24 }} aria-pressed={on}
                        onClick={() => setCompare((c) => (on ? c.filter((x) => x !== s.id) : [...c, s.id]))}>
                        <span className="sw" style={{ background: styles.get(s.id)!.color }} />{shortName(s.name)}
                      </button>
                    );
                  })}
                  {all.agents.filter((s) => s.id !== a.id).length === 0 && <span className="dim xs">no other agents with results</span>}
                </div>
              </div>
              <TimeChart lines={lines} formatValue={usd} ariaLabel="Agent P&L compared" height={340} area={compare.length === 0} emptyText="No settled challenges yet." />
            </div>
            <div className="grid-2">
              <div className="card pad-0">
                <div className="card-head" style={{ padding: "12px 14px 0" }}><h3>Settled challenges</h3><span className="dim xs">{rows.length}</span></div>
                {rows.length === 0 ? <div className="dim small" style={{ padding: 14 }}>None yet.</div> : (
                  <div className="table-wrap" style={{ maxHeight: 360 }}>
                    <table className="dense">
                      <thead><tr><th>Settled</th><th>Challenge</th><th className="num">Result</th><th className="num">Cumulative</th></tr></thead>
                      <tbody>
                        {rows.map((r) => (
                          <tr key={`${r.interaction_id}-${r.ts}`}>
                            <td className="dim num">{formatTs(r.ts)}</td>
                            <td><Link href={`/interactions/${r.interaction_id}`} className="mono xs">{r.interaction_id.slice(0, 8)}</Link></td>
                            <td className={`num ${tone(r.delta) ?? ""}`}>{formatMoney(r.delta, { signed: true, unit: false })}</td>
                            <td className={`num ${tone(r.cum) ?? ""}`}>{formatMoney(r.cum, { signed: true, unit: false })}</td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                )}
              </div>
              <div className="card">
                <div className="card-head"><h3>Result distribution</h3><span className="dim xs">per settled challenge, USDC</span></div>
                {deltas.length === 0 ? <div className="dim small">None yet.</div> : (
                  <div className="dbars">
                    {[["Wins", wins, "pos"], ["Losses", losses, "neg"]].map(([label, list, c]) => {
                      const l = list as string[];
                      const total = sumMoney(l);
                      return (
                        <div key={label as string} className="dbar">
                          <span className="n">{label as string} · {l.length}</span>
                          <span className="track"><span className={`fill ${c}`} style={{ left: 0, width: `${(l.length / deltas.length) * 100}%` }} /></span>
                          <span className={`v num ${c}`}>{formatMoney(total, { signed: true, unit: false })}</span>
                        </div>
                      );
                    })}
                    <div className="dim xs" style={{ marginTop: 6 }}>Realized only: open challenges have no fair mark, so unrealized P&amp;L is not computed. {a.stats.firm_income ? <>Firm income ({formatMoney(a.stats.firm_income)}) is tracked separately.</> : null}</div>
                  </div>
                )}
              </div>
            </div>
          </div>
        );
      }}
    </Async>
  );
}
