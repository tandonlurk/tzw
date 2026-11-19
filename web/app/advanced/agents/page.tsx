"use client";

import Link from "next/link";
import { useRouter } from "next/navigation";
import { useMemo, useState } from "react";
import { api, type Agent } from "@/lib/api";
import { formatMoney, signOf } from "@/lib/money";
import { useAsync } from "@/lib/useAsync";
import { agentStyles, shortName } from "@/lib/agentColors";
import { portfolioSeries } from "@/lib/series";
import { ADV } from "@/lib/mode";
import { Async, EnvBadge } from "@/components/ui";
import { TimeChart } from "@/components/charts/TimeChart";
import { Icon } from "@/components/shell/icons";

type Key = "name" | "available" | "committed" | "exposure" | "pnl" | "winrate" | "settled";

const big = (s: string) => { try { return BigInt(s); } catch { return BigInt(0); } };
function exposurePct(a: Agent): number {
  const max = big(a.policy.max_aggregate_exposure);
  return max > BigInt(0) ? Number((big(a.balances.committed) * BigInt(1000)) / max) / 10 : 0;
}

export default function AdvancedAgents() {
  const agents = useAsync(api.listAgents, "agents");
  const portfolio = useAsync(() => api.getPortfolio("simulated"), "portfolio");
  const [q, setQ] = useState("");
  const [sort, setSort] = useState<{ k: Key; dir: 1 | -1 }>({ k: "pnl", dir: -1 });
  const router = useRouter();
  const series = useMemo(() => new Map((portfolio.data ? portfolioSeries(portfolio.data).agents : []).map((s) => [s.id, s])), [portfolio.data]);

  const cmp = (a: Agent, b: Agent): number => {
    const v = (x: Agent): number | string => {
      switch (sort.k) {
        case "name": return x.name.toLowerCase();
        case "available": return Number(x.balances.available);
        case "committed": return Number(x.balances.committed);
        case "exposure": return exposurePct(x);
        case "pnl": return Number(x.stats.realized_pnl);
        case "winrate": return x.stats.settled ? x.stats.wins / x.stats.settled : -1;
        case "settled": return x.stats.settled;
      }
    };
    const A = v(a); const B = v(b);
    return (A < B ? -1 : A > B ? 1 : 0) * sort.dir;
  };
  const th = (k: Key, label: string, num = true) => (
    <th className={num ? "num" : undefined} aria-sort={sort.k === k ? (sort.dir === 1 ? "ascending" : "descending") : "none"}>
      <button type="button" className="th-sort" onClick={() => setSort((s) => ({ k, dir: s.k === k ? (s.dir === 1 ? -1 : 1) : -1 }))}>
        {label}{sort.k === k && <Icon name="chevron-down" size={11} style={{ transform: sort.dir === 1 ? "rotate(180deg)" : undefined }} />}
      </button>
    </th>
  );

  return (
    <div>
      <div className="t-head">
        <h1 style={{ margin: 0 }}>Agents</h1>
        <div className="row">
          <input type="search" placeholder="Filter agents" value={q} onChange={(e) => setQ(e.target.value)} aria-label="Filter agents" style={{ width: 220, height: 30 }} />
          <Link href="/agents/new" className="btn sm primary"><Icon name="plus" size={14} />New agent</Link>
        </div>
      </div>
      <Async state={agents} what="agents">
        {(list) => {
          const styles = agentStyles(list.map((a) => a.id));
          const rows = list.filter((a) => !q || `${a.name} ${a.description}`.toLowerCase().includes(q.toLowerCase())).sort(cmp);
          return (
            <div className="card pad-0 table-wrap">
              <table className="dense adv-table">
                <thead>
                  <tr>
                    {th("name", "Agent", false)}
                    <th>Status</th><th>Env</th>
                    {th("available", "Available")}
                    {th("committed", "Committed")}
                    {th("exposure", "Exposure used")}
                    {th("pnl", "Realized P&L")}
                    <th style={{ width: 140 }}>Trend</th>
                    {th("settled", "Settled")}
                    {th("winrate", "Win rate")}
                    <th />
                  </tr>
                </thead>
                <tbody>
                  {rows.map((a) => {
                    const st = styles.get(a.id)!;
                    const s = series.get(a.id);
                    const ex = exposurePct(a);
                    return (
                      <tr key={a.id} onClick={() => router.push(`${ADV}/agents/${a.id}`)} tabIndex={0} onKeyDown={(e) => e.key === "Enter" && router.push(`${ADV}/agents/${a.id}`)}>
                        <td><span className="row nowrap" style={{ gap: 8 }}><span className="sw" style={{ background: st.color }} /><span className="text">{shortName(a.name)}</span><span className="dim xs">v{a.version}</span></span></td>
                        <td><span className={`status-dot ${a.status}`} /> <span className="dim xs">{a.status}</span></td>
                        <td><EnvBadge env={a.env} /></td>
                        <td className="num">{formatMoney(a.balances.available, { unit: false })}</td>
                        <td className={`num${big(a.balances.committed) > BigInt(0) ? " pend" : ""}`}>{formatMoney(a.balances.committed, { unit: false })}</td>
                        <td className="num"><span className="row nowrap" style={{ justifyContent: "flex-end", gap: 8 }}><span className="bar-track" style={{ width: 56, height: 5 }}><span className={`bar-fill ${ex >= 80 ? "neg" : ex > 0 ? "pend" : ""}`} style={{ width: `${ex}%`, display: "block" }} /></span>{ex.toFixed(0)}%</span></td>
                        <td className={`num ${signOf(a.stats.realized_pnl) > 0 ? "pos" : signOf(a.stats.realized_pnl) < 0 ? "neg" : ""}`}>{formatMoney(a.stats.realized_pnl, { signed: true, unit: false })}</td>
                        <td style={{ padding: "2px 8px" }}><div style={{ height: 26 }}><TimeChart compact controls={false} height={26} ariaLabel={`${a.name} trend`} formatValue={String} lines={s ? [{ id: a.id, name: a.name, color: st.color, points: s.points }] : []} emptyText="—" /></div></td>
                        <td className="num">{a.stats.settled}</td>
                        <td className="num">{a.stats.settled ? `${((a.stats.wins * 100) / a.stats.settled).toFixed(0)}%` : "–"}</td>
                        <td><Icon name="chevron-right" size={13} className="dim" /></td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
              {rows.length === 0 && <div className="dim small" style={{ padding: 16 }}>No agents match “{q}”.</div>}
            </div>
          );
        }}
      </Async>
      <p className="dim xs" style={{ marginTop: 10 }}>Amounts in USDC. Exposure used = committed ÷ the agent&apos;s max total exposure. Trend = cumulative realized P&amp;L (simulated agents).</p>
    </div>
  );
}
