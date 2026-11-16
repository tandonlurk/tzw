"use client";

import { useState } from "react";
import type { Portfolio } from "@/lib/api";
import { perAgentSeries } from "@/lib/charts";
import { MultiLineChart } from "./charts/MultiLineChart";
import { formatMoney, signOf } from "@/lib/money";
import { LineChart } from "./charts/LineChart";
import { BarList, type BarItem } from "./charts/BarList";
import { Empty, EnvBadge, Tile, formatTs } from "./ui";

/** Presentational: renders only what /stats/portfolio returned. */
export function PortfolioCharts({ p }: { p: Portfolio }) {
  const [view, setView] = useState<"agents" | "total">("agents");
  const series = perAgentSeries(
    p.pnl_over_time,
    p.agents,
    (v) => formatMoney(v, { signed: true }),
    formatTs,
  );
  const toneOf = (finalMinor: number) => (finalMinor > 0 ? "pos" : finalMinor < 0 ? "neg" : "info");
  const o = p.outcomes;
  const pnlPoints = p.pnl_over_time.map((e) => ({
    x: new Date(e.ts).getTime(),
    y: Number(e.portfolio_cumulative),
    label: `${formatTs(e.ts)}: ${formatMoney(e.portfolio_cumulative, { signed: true })}`,
  }));
  const alloc: BarItem[] = p.allocation.map((a) => ({
    label: a.name,
    value: Number(a.available) + Number(a.committed),
    display: `${formatMoney(a.available)} free, ${formatMoney(a.committed)} committed`,
    tone: "info",
  }));
  const perAgent: BarItem[] = p.agents.map((a) => ({
    label: a.name,
    value: Number(a.realized_pnl),
    display: `${formatMoney(a.realized_pnl, { signed: true })} (${a.wins}W/${a.losses}L)`,
    tone: signOf(a.realized_pnl) > 0 ? "pos" : signOf(a.realized_pnl) < 0 ? "neg" : "info",
  }));
  const outcomeBars: BarItem[] = [
    { label: "Settled", value: o.settled, display: String(o.settled), tone: "pos" },
    { label: "Voided (refunded)", value: o.voided, display: String(o.voided), tone: "info" },
    { label: "Cancelled", value: o.cancelled, display: String(o.cancelled), tone: "neg" },
    { label: "Failed", value: o.failed, display: String(o.failed), tone: "neg" },
  ];
  const totalInteractions = o.open + o.settled + o.cancelled + o.failed;

  return (
    <div className="stack">
      <div className="row"><EnvBadge env={p.env} /><span className="dim">As of {formatTs(p.as_of)}. All figures come from the ledger; formulas in the footnote.</span></div>
      <div className="grid">
        <Tile label="Realized P&L"><span className={signOf(p.totals.realized_pnl) > 0 ? "pos" : signOf(p.totals.realized_pnl) < 0 ? "neg" : ""}>{formatMoney(p.totals.realized_pnl, { signed: true })}</span></Tile>
        <Tile label="At risk (committed)">{formatMoney(p.totals.at_risk)}</Tile>
        <Tile label="Allocated to agents">{formatMoney(p.totals.allocated)}</Tile>
        <Tile label="Active / open" note={`${o.active} active of ${o.open} open`}>{o.open}</Tile>
      </div>

      <div className="grid-2">
        <div className="card">
          <h3 style={{ marginTop: 0 }}>Realized P&amp;L over time</h3>
          {pnlPoints.length === 0 ? <Empty>No settled challenges yet.</Empty> : (
            <>
              <div className="seg" role="group" aria-label="P&L view">
                <button className={view === "agents" ? "on" : ""} aria-pressed={view === "agents"} onClick={() => setView("agents")}>By agent</button>
                <button className={view === "total" ? "on" : ""} aria-pressed={view === "total"} onClick={() => setView("total")}>Portfolio total</button>
              </div>
              {view === "agents" ? (
                <>
                  <MultiLineChart series={series} tone={(s) => toneOf(s.finalMinor)} ariaLabel="Cumulative realized P&L per agent" formatY={(y) => formatMoney(String(Math.round(y)), { signed: true })} />
                  <div className="legend">
                    {series.map((s) => <span key={s.agentId} className={toneOf(s.finalMinor)}><span className="sw" />{s.name}: {formatMoney(String(s.finalMinor), { signed: true })}</span>)}
                  </div>
                </>
              ) : (
                <>
                  <LineChart points={pnlPoints} ariaLabel="Cumulative portfolio realized P&L" formatY={(y) => formatMoney(String(Math.round(y)), { signed: true })} />
                  {signOf(p.totals.realized_pnl) === 0 && (
                    <div className="dim" style={{ fontSize: ".8rem" }}>Nets to zero when one owner is on both sides of every challenge; see the by-agent view.</div>
                  )}
                </>
              )}
            </>
          )}
        </div>
        <div className="card">
          <h3 style={{ marginTop: 0 }}>Realized P&amp;L by agent</h3>
          {perAgent.length === 0 ? <Empty>No agents.</Empty> : p.agents.every((a) => a.settled === 0) ? <Empty>No agent has a settled challenge yet.</Empty> : <BarList items={perAgent} ariaLabel="Realized P&L by agent" />}
        </div>
        <div className="card">
          <h3 style={{ marginTop: 0 }}>Capital allocation</h3>
          {alloc.length === 0 || alloc.every((a) => a.value === 0) ? <Empty>No capital allocated to agents yet.</Empty> : <BarList items={alloc} ariaLabel="Capital allocation by agent" />}
        </div>
        <div className="card">
          <h3 style={{ marginTop: 0 }}>Outcomes</h3>
          {totalInteractions === 0 ? <Empty>No challenges yet.</Empty> : (
            <>
              <BarList items={outcomeBars} ariaLabel="Challenge outcomes" />
              <div className="dim" style={{ marginTop: ".4rem" }}>Active vs settled: {o.active} active, {o.open - o.active} other open, {o.settled} settled.</div>
            </>
          )}
        </div>
      </div>

      <details><summary>Metric formulas</summary>
        <table className="kv"><tbody>{Object.entries(p.formulas).map(([k, v]) => <tr key={k}><td>{k}</td><td>{v}</td></tr>)}</tbody></table>
      </details>
    </div>
  );
}
