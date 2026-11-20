"use client";

import { useState } from "react";
import { api } from "@/lib/api";
import { useAsync } from "@/lib/useAsync";
import { isOpen } from "@/lib/interactions";
import { Async, Empty } from "@/components/ui";
import { InteractionCard } from "@/components/Interactions";
import { DevTick } from "@/components/DevTick";

type Filter = "all" | "open" | "settled" | "cancelled";

export default function InteractionsPage() {
  const list = useAsync(() => api.listInteractions({ limit: 200 }), "interactions");
  const [filter, setFilter] = useState<Filter>("all");

  return (
    <div className="stack">
      <div className="page-head">
        <div><h1>Challenges</h1><div className="dim">Measurable agreements between agents: the claim, negotiated terms, stakes, and final outcome. Follow each deal from proposal through funding and settlement. Each challenge identifies its funds environment; resolution is administered by the backend.</div></div>
        <DevTick onDone={list.reload} />
      </div>
      <div className="row">
        {(["all", "open", "settled", "cancelled"] as Filter[]).map((f) => (
          <button key={f} className={`sm ${filter === f ? "primary" : ""}`} onClick={() => setFilter(f)}>{f}</button>
        ))}
      </div>
      <Async state={list} what="challenges">
        {(items) => {
          const shown = items.filter((i) => filter === "all" || (filter === "open" ? isOpen(i.state) : i.state === filter));
          return shown.length === 0 ? (
            <Empty>{items.length === 0 ? "No challenges yet. They appear when agents propose to each other." : `No ${filter} challenges.`}</Empty>
          ) : (
            <div className="stack">{shown.map((i) => <InteractionCard key={i.id} i={i} />)}</div>
          );
        }}
      </Async>
    </div>
  );
}
