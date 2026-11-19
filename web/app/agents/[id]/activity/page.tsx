"use client";

import Link from "next/link";
import { useMemo, useState } from "react";
import { api } from "@/lib/api";
import { useAsync } from "@/lib/useAsync";
import { CATEGORIES, categoryOf, type ActivityCategory } from "@/lib/activityKinds";
import { Async, Empty } from "@/components/ui";
import { useWorkspace } from "@/components/agent/AgentWorkspace";
import { Feed } from "@/components/activity/Feed";
import { PastChallenges, VersionHistory } from "@/components/agent/sections";
import { LedgerTable } from "@/components/LedgerTable";
import { CollabCard } from "@/components/CollabCard";

type View = "events" | "challenges" | "ledger" | "versions";

export default function AgentActivityPage() {
  const { agent, bump } = useWorkspace();
  const a = agent.data!;
  const [view, setView] = useState<View>("events");
  const [cat, setCat] = useState<ActivityCategory | "all">("all");
  const activity = useAsync(() => api.listActivity({ agent_id: a.id, limit: 200 }), `activity:${a.id}:200:${bump}`);
  const interactions = useAsync(() => api.listInteractions({ agent_id: a.id }), `interactions:${a.id}:${bump}`);
  const collabs = useAsync(() => api.listCollaborations({ agent_id: a.id }), `collabs:${a.id}:${bump}`);
  const ledger = useAsync(() => api.getAgentLedger(a.id), `ledger:${a.id}:${bump}`);
  const names = useMemo(() => new Map([[a.id, a.name]]), [a.id, a.name]);
  const present = useMemo(() => new Set((activity.data ?? []).map((it) => categoryOf(it.kind))), [activity.data]);

  return (
    <div>
      <div className="row spread" style={{ marginBottom: "var(--s-5)" }}>
        <div className="seg" role="tablist" aria-label="History view">
          {([["events", "Events"], ["challenges", "Challenges"], ["ledger", "Transactions"], ["versions", "Versions"]] as [View, string][]).map(([k, l]) => (
            <button key={k} type="button" role="tab" aria-selected={view === k} className={view === k ? "on" : ""} onClick={() => setView(k)}>{l}</button>
          ))}
        </div>
        <button type="button" className="sm ghost" onClick={() => { activity.reload(); interactions.reload(); ledger.reload(); collabs.reload(); }}>Refresh</button>
      </div>

      {view === "events" && (
        <>
          <div className="filters" role="group" aria-label="Event type">
            <button type="button" className={`chip${cat === "all" ? " on" : ""}`} onClick={() => setCat("all")}>All</button>
            {CATEGORIES.filter((c) => present.has(c.id)).map((c) => (
              <button key={c.id} type="button" className={`chip${cat === c.id ? " on" : ""}`} aria-pressed={cat === c.id} onClick={() => setCat(c.id)}>{c.label}</button>
            ))}
          </div>
          <Async state={activity} what="activity">
            {(items) => {
              const shown = cat === "all" ? items : items.filter((it) => categoryOf(it.kind) === cat);
              return shown.length === 0 ? <Empty>No events for this agent{cat === "all" ? " yet" : " in this category"}.</Empty> : <Feed items={shown} names={names} />;
            }}
          </Async>
        </>
      )}

      {view === "challenges" && (
        <div className="stack">
          <Async state={interactions} what="challenges">{(list) => <PastChallenges list={list} />}</Async>
          <h2>Collaborations</h2>
          <Async state={collabs} what="collaborations">
            {(list) => list.length === 0 ? <Empty>No collaborations yet. <Link href="/collaborations">Propose one</Link>.</Empty> : <div className="stack">{list.map((c) => <CollabCard key={c.id} c={c} />)}</div>}
          </Async>
        </div>
      )}

      {view === "ledger" && (
        <div className="card pad-0"><Async state={ledger} what="ledger">{(entries) => <LedgerTable entries={entries} />}</Async></div>
      )}

      {view === "versions" && (
        <div className="card">
          <div className="dim small" style={{ marginBottom: 8 }}>Each save creates a new version. Past decisions keep the version they ran under.</div>
          <VersionHistory agentId={a.id} bump={bump} />
        </div>
      )}
    </div>
  );
}
