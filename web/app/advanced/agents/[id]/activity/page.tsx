"use client";

import { useMemo, useState } from "react";
import { api } from "@/lib/api";
import { useAsync } from "@/lib/useAsync";
import { Async, Empty } from "@/components/ui";
import { useWorkspace } from "@/components/agent/AgentWorkspace";
import { HistoryTable } from "@/components/activity/HistoryTable";
import { LedgerTable } from "@/components/LedgerTable";
import { VersionHistory } from "@/components/agent/sections";

export default function AdvancedAgentActivity() {
  const { agent, bump } = useWorkspace();
  const a = agent.data!;
  const [view, setView] = useState<"events" | "ledger" | "versions">("events");
  const activity = useAsync(() => api.listActivity({ agent_id: a.id, limit: 300 }), `activity:${a.id}:300:${bump}`);
  const ledger = useAsync(() => api.getAgentLedger(a.id), `ledger:${a.id}:${bump}`);
  const names = useMemo(() => new Map([[a.id, a.name]]), [a.id, a.name]);
  return (
    <div>
      <div className="seg" role="tablist" style={{ marginBottom: 12 }}>
        {(["events", "ledger", "versions"] as const).map((k) => (
          <button key={k} type="button" role="tab" aria-selected={view === k} className={view === k ? "on" : ""} onClick={() => setView(k)}>
            {k === "events" ? "Events" : k === "ledger" ? "Ledger" : "Versions"}
          </button>
        ))}
      </div>
      {view === "events" && <Async state={activity} what="events">{(items) => items.length === 0 ? <Empty>No events yet.</Empty> : <HistoryTable items={items} names={names} showAgent={false} />}</Async>}
      {view === "ledger" && <div className="card pad-0"><Async state={ledger} what="ledger">{(e) => <LedgerTable entries={e} />}</Async></div>}
      {view === "versions" && <div className="card"><VersionHistory agentId={a.id} bump={bump} /></div>}
    </div>
  );
}
