"use client";

import Link from "next/link";
import { api } from "@/lib/api";
import { useAsync } from "@/lib/useAsync";
import { ActivityExtra } from "@/components/ActivityDetail";
import { Async, Empty, formatTs } from "@/components/ui";

export default function ActivityPage() {
  const activity = useAsync(() => api.listActivity({ limit: 200 }), "activity");
  const agents = useAsync(api.listAgents, "agents");
  const names = new Map((agents.data ?? []).map((a) => [a.id, a.name]));

  return (
    <div>
      <div className="page-head"><div><h1>Activity</h1><div className="dim">Newest first. Open Details for the raw record.</div></div>
        <button className="sm" onClick={() => activity.reload()}>Refresh</button></div>
      <Async state={activity} what="activity">
        {(items) =>
          items.length === 0 ? (
            <Empty>No activity yet. Events appear here as agents are created and act.</Empty>
          ) : (
            <div className="stack">
              {items.map((it) => (
                <div key={it.id} className="card">
                  <div className="row spread">
                    <span>{it.summary}</span>
                    <span className="dim">{formatTs(it.ts)}</span>
                  </div>
                  <div className="row dim" style={{ fontSize: ".8rem" }}>
                    <span className="badge neutral">{it.kind.replace(/_/g, " ")}</span>
                    {it.agent_id && <Link href={`/agents/${it.agent_id}`}>{names.get(it.agent_id) ?? it.agent_id}</Link>}
                    {it.collaboration_id && <Link href={`/collaborations/${it.collaboration_id}`}>collaboration</Link>}
                    {it.interaction_id && <Link href={`/interactions/${it.interaction_id}`}>challenge</Link>}
                  </div>
                  <ActivityExtra it={it} />
                  {it.detail && (
                    <details style={{ marginTop: ".4rem" }}>
                      <summary>Details</summary>
                      <pre>{JSON.stringify(it.detail, null, 2)}</pre>
                    </details>
                  )}
                </div>
              ))}
            </div>
          )
        }
      </Async>
    </div>
  );
}
