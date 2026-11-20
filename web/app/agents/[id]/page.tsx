"use client";

import Link from "next/link";
import { useSearchParams } from "next/navigation";
import { api } from "@/lib/api";
import { formatMoney, signOf } from "@/lib/money";
import { useAsync } from "@/lib/useAsync";
import { interactionCounts, winRateText } from "@/lib/agentView";
import { Async, Kpi } from "@/components/ui";
import { useWorkspace } from "@/components/agent/AgentWorkspace";
import { FundingCard, LinkToLogic, OpenChallenges, RecentDecisions, RiskCard, SavedNotice } from "@/components/agent/sections";
import { ApproachCard } from "@/components/approach/ApproachCard";
import { MultiplyCard } from "@/components/agent/MultiplyCard";
import { FamilyNote } from "@/components/agent/FamilyNote";
import { VaultPanel } from "@/components/wallet/VaultPanel";

export default function AgentOverviewPage() {
  const { agent, refresh, bump } = useWorkspace();
  const a = agent.data!;
  const interactions = useAsync(() => api.listInteractions({ agent_id: a.id }), `interactions:${a.id}:${bump}`);
  const activity = useAsync(() => api.listActivity({ agent_id: a.id, limit: 40 }), `activity:${a.id}:${bump}`);
  const saved = useSearchParams().get("saved");
  const pnl = signOf(a.stats.realized_pnl);
  const active = interactions.data ? interactionCounts(interactions.data).active : null;

  return (
    <div>
      <SavedNotice saved={saved} />
      <div className="card">
        <div className="kpi-row">
          <Kpi label="Available">{formatMoney(a.balances.available)}</Kpi>
          <Kpi label="Committed" tone={BigInt(a.balances.committed) > BigInt(0) ? "pend" : undefined} note="Locked in open challenges">{formatMoney(a.balances.committed)}</Kpi>
          <Kpi label="Realized P&L" tone={pnl > 0 ? "pos" : pnl < 0 ? "neg" : undefined} note="Unrealized: not computed">{formatMoney(a.stats.realized_pnl, { signed: true })}</Kpi>
          <Kpi label="Results" note={winRateText(a.stats.wins, a.stats.settled)}>{a.stats.settled} settled</Kpi>
          <Kpi label="Open challenges">{active === null ? "…" : active}</Kpi>
        </div>
      </div>

      <div className="grid-3" style={{ marginTop: "var(--gap)", alignItems: "start" }}>
        <div style={{ gridColumn: "span 2", minWidth: 0 }} className="stack">
          <div className="card">
            <div className="card-head"><h3>Current activity</h3><Link href="/interactions" className="small">All challenges</Link></div>
            <Async state={interactions} what="challenges">{(list) => <OpenChallenges list={list} />}</Async>
          </div>
          <div className="card">
            <div className="card-head"><h3>Recent decisions</h3><Link href={`/agents/${a.id}/activity`} className="small">Full history</Link></div>
            <Async state={activity} what="activity">{(items) => <RecentDecisions items={items} />}</Async>
          </div>
        </div>
        <div className="stack" style={{ minWidth: 0 }}>
          <RiskCard a={a} />
          <FundingCard a={a} onChanged={refresh} />
          {a.status === "active" ? <MultiplyCard a={a} onChanged={refresh} /> : <div className="card" id="multiply"><div className="card-head"><h3>Multiply</h3></div><div className="dim small">Resume this agent to multiply it.</div></div>}
          <FamilyNote a={a} />
          <LinkToLogic a={a} href={`/agents/${a.id}/logic`} />
        </div>
      </div>

      {a.wallet_address && <div className="dim small" style={{ marginTop: "var(--gap)" }}>Agent wallet <span className="mono">{a.wallet_address}</span></div>}
      <div style={{ marginTop: "var(--gap)" }}><VaultPanel agent={a} onChanged={refresh} /></div>
      <div style={{ marginTop: "var(--gap)" }}><ApproachCard agent={a} /></div>
    </div>
  );
}
