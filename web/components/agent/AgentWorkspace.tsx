"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { createContext, useContext, useMemo, useState, type ReactNode } from "react";
import { api, isApiError, type Agent, type ApiError } from "@/lib/api";
import { useAsync, type AsyncState } from "@/lib/useAsync";
import { formatMoney, signOf } from "@/lib/money";
import { agentStyles, shortName } from "@/lib/agentColors";
import { usesCustomCode } from "@/lib/customCode";
import { winRateText } from "@/lib/agentView";
import { ADV } from "@/lib/mode";
import { Avatar, EnvBadge, ErrorBanner, ErrorState, Loading, StatusBadge } from "@/components/ui";
import { Icon, type IconName } from "@/components/shell/icons";

interface WsCtx { agent: AsyncState<Agent>; color: string; refresh: () => void; bump: number }
const Ctx = createContext<WsCtx | null>(null);

/** The agent being viewed, shared by all workspace tabs (one fetch, one reload). */
export function useWorkspace(): WsCtx {
  const c = useContext(Ctx);
  if (!c) throw new Error("useWorkspace outside AgentWorkspace");
  return c;
}

const BASIC_TABS: { sub: string; label: string; icon: IconName }[] = [
  { sub: "", label: "Overview", icon: "overview" },
  { sub: "/logic", label: "Logic", icon: "logic" },
  { sub: "/activity", label: "Activity", icon: "activity" },
  { sub: "/edit", label: "Configuration", icon: "settings" },
];
const ADV_TABS: { sub: string; label: string; icon: IconName }[] = [
  { sub: "", label: "Overview", icon: "overview" },
  { sub: "/performance", label: "Performance", icon: "terminal" },
  { sub: "/logic", label: "Logic", icon: "logic" },
  { sub: "/code", label: "Code", icon: "code" },
  { sub: "/activity", label: "Activity", icon: "history" },
  { sub: "/settings", label: "Settings", icon: "settings" },
];

export function AgentWorkspace({ id, advanced, children }: { id: string; advanced: boolean; children: ReactNode }) {
  const [bump, setBump] = useState(0);
  const agent = useAsync(() => api.getAgent(id), `agent:${id}`);
  const all = useAsync(api.listAgents, "agents");
  const path = usePathname() ?? "";
  const base = advanced ? `${ADV}/agents/${id}` : `/agents/${id}`;
  const tabs = advanced ? ADV_TABS : BASIC_TABS;
  const sub = path.slice(base.length);
  const full = sub === "/logic" || sub === "/code";
  const color = useMemo(() => agentStyles((all.data ?? []).map((a) => a.id).concat(id)).get(id)?.color ?? "var(--blue)", [all.data, id]);
  const ctx = useMemo(() => ({ agent, color, bump, refresh: () => { agent.reload(); setBump((b) => b + 1); } }), [agent, color, bump]);

  const head = agent.error && !agent.data ? <ErrorState error={agent.error} onRetry={agent.reload} /> : !agent.data ? <Loading what="agent" /> : (
    <Header a={agent.data} color={color} advanced={advanced} compact={full} onChanged={ctx.refresh} />
  );
  const nav = (
    <nav className="tabs" aria-label="Agent sections">
      {tabs.map((t) => {
        const active = sub === t.sub;
        return (
          <Link key={t.sub} href={`${base}${t.sub}`} className={`tab${active ? " active" : ""}`} aria-current={active ? "page" : undefined}>
            <Icon name={t.icon} size={14} />{t.label}
          </Link>
        );
      })}
    </nav>
  );

  return (
    <Ctx.Provider value={ctx}>
      {full ? (
        <div className="ws-full">
          <div className="ws-top">{head}{nav}</div>
          <div className="ws-fill">{agent.data ? children : null}</div>
        </div>
      ) : (
        <div>
          {head}
          {nav}
          <div className="ws-body">{agent.data ? children : null}</div>
        </div>
      )}
    </Ctx.Provider>
  );
}

function Header({ a, color, advanced, compact, onChanged }: { a: Agent; color: string; advanced: boolean; compact: boolean; onChanged: () => void }) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  async function toggle() {
    setBusy(true);
    setError(null);
    try {
      await api.setAgentStatus(a.id, a.status === "active" ? "paused" : "active");
      onChanged();
    } catch (e) {
      if (isApiError(e)) setError(e);
    } finally {
      setBusy(false);
    }
  }
  const pnl = signOf(a.stats.realized_pnl);
  const badges = (
    <>
      <StatusBadge status={a.status} />
      <EnvBadge env={a.env} />
      <span className="dim">Version {a.version}</span>
      {usesCustomCode(a.graph) && <span className="badge info plain" title="This agent's logic contains a custom code block.">Custom code</span>}
    </>
  );
  const pause = (
    <button type="button" className={advanced ? "sm outline" : "outline"} disabled={busy} onClick={toggle}>
      <Icon name={a.status === "active" ? "pause" : "play"} size={14} />{a.status === "active" ? "Pause" : "Resume"}
    </button>
  );

  if (advanced || compact) {
    return (
      <>
        <div className="ws-head compact">
          <div className="who">
            <Avatar name={a.name} color={color} size={34} />
            <div style={{ minWidth: 0 }}>
              <h1>{shortName(a.name)}</h1>
              <div className="meta" style={{ marginTop: 2 }}>{badges}</div>
            </div>
          </div>
          <div className="row" style={{ gap: "var(--s-6)" }}>
            <div className="kpi"><div className="kpi-label">Available</div><div className="kpi-value">{formatMoney(a.balances.available, { unit: false })}</div></div>
            <div className="kpi"><div className="kpi-label">Committed</div><div className={`kpi-value${BigInt(a.balances.committed) > BigInt(0) ? " pend" : ""}`}>{formatMoney(a.balances.committed, { unit: false })}</div></div>
            <div className="kpi"><div className="kpi-label">Realized P&amp;L</div><div className={`kpi-value ${pnl > 0 ? "pos" : pnl < 0 ? "neg" : ""}`}>{formatMoney(a.stats.realized_pnl, { signed: true, unit: false })}</div></div>
            <div className="kpi"><div className="kpi-label">Record</div><div className="kpi-value">{a.stats.wins}W / {a.stats.settled - a.stats.wins}L</div></div>
            {pause}
            {!advanced && <Link className="btn sm ghost" href={`${ADV}/agents/${a.id}/logic`} title="Open this agent in the Advanced workspace"><Icon name="advanced-mode" size={14} />Advanced</Link>}
          </div>
        </div>
        <ErrorBanner error={error} />
      </>
    );
  }

  return (
    <>
      <div className="ws-head">
        <div className="who">
          <Avatar name={a.name} color={color} size={52} />
          <div style={{ minWidth: 0 }}>
            <h1>{shortName(a.name)}</h1>
            <div className="meta">{badges}<span className="dim">· {winRateText(a.stats.wins, a.stats.settled)}</span></div>
          </div>
        </div>
        <div className="row">
          {pause}
          <Link className="btn ghost" href={`${ADV}/agents/${a.id}`} title="Open this agent in the Advanced workspace"><Icon name="advanced-mode" size={15} />Advanced</Link>
        </div>
      </div>
      {a.description && <p className="ws-desc">{a.description}</p>}
      <ErrorBanner error={error} />
    </>
  );
}
