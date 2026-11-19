"use client";

import { useMemo, useState } from "react";
import { api } from "@/lib/api";
import { useAsync } from "@/lib/useAsync";
import { CATEGORIES, categoryOf, type ActivityCategory } from "@/lib/activityKinds";
import { shortName } from "@/lib/agentColors";
import { Async, Empty, PageHead } from "@/components/ui";
import { Feed } from "@/components/activity/Feed";
import { Icon } from "@/components/shell/icons";

const PAGE = 60;

export default function ActivityPage() {
  const activity = useAsync(() => api.listActivity({ limit: 300 }), "activity-300");
  const agents = useAsync(api.listAgents, "agents");
  const names = useMemo(() => new Map((agents.data ?? []).map((a) => [a.id, a.name])), [agents.data]);
  const [cat, setCat] = useState<ActivityCategory | "all">("all");
  const [agent, setAgent] = useState<string>("");
  const [shown, setShown] = useState(PAGE);

  return (
    <div>
      <PageHead title="Activity" sub="Everything your agents did, newest first. Open Details on an event for the full record.">
        <button type="button" className="ghost" onClick={() => activity.reload()}><Icon name="refresh" size={15} />Refresh</button>
      </PageHead>
      <Async state={activity} what="activity" stale>
        {(items) => {
          const present = new Set(items.map((it) => categoryOf(it.kind)));
          const filtered = items.filter((it) => (cat === "all" || categoryOf(it.kind) === cat) && (!agent || it.agent_id === agent));
          return (
            <>
              <div className="row spread" style={{ marginBottom: "var(--s-5)", alignItems: "flex-start" }}>
                <div className="filters" role="group" aria-label="Event type" style={{ marginBottom: 0 }}>
                  <button type="button" className={`chip${cat === "all" ? " on" : ""}`} onClick={() => setCat("all")}>All</button>
                  {CATEGORIES.filter((c) => present.has(c.id)).map((c) => (
                    <button key={c.id} type="button" className={`chip${cat === c.id ? " on" : ""}`} aria-pressed={cat === c.id} onClick={() => setCat(c.id)}>{c.label}</button>
                  ))}
                </div>
                <select value={agent} onChange={(e) => setAgent(e.target.value)} aria-label="Filter by agent" style={{ width: 200, height: 30 }}>
                  <option value="">All agents</option>
                  {(agents.data ?? []).map((a) => <option key={a.id} value={a.id}>{shortName(a.name)}</option>)}
                </select>
              </div>
              {filtered.length === 0 ? (
                <Empty>{items.length === 0 ? "No activity yet. Events appear here as agents are created and act." : "No events match these filters."}</Empty>
              ) : (
                <div className="card" style={{ paddingTop: 8 }}>
                  <Feed items={filtered.slice(0, shown)} names={names} />
                  {filtered.length > shown && (
                    <div className="feed-more"><button type="button" className="ghost sm" onClick={() => setShown((n) => n + PAGE)}>Show {Math.min(PAGE, filtered.length - shown)} more</button></div>
                  )}
                </div>
              )}
            </>
          );
        }}
      </Async>
    </div>
  );
}
