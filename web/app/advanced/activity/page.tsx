"use client";

import { useMemo } from "react";
import { api } from "@/lib/api";
import { useAsync } from "@/lib/useAsync";
import { Async, Empty } from "@/components/ui";
import { HistoryTable } from "@/components/activity/HistoryTable";
import { Icon } from "@/components/shell/icons";

export default function AdvancedHistory() {
  const activity = useAsync(() => api.listActivity({ limit: 500 }), "activity-500");
  const agents = useAsync(api.listAgents, "agents");
  const names = useMemo(() => new Map((agents.data ?? []).map((a) => [a.id, a.name])), [agents.data]);
  return (
    <div>
      <div className="t-head">
        <h1 style={{ margin: 0 }}>History</h1>
        <button type="button" className="sm ghost" onClick={() => activity.reload()}><Icon name="refresh" size={14} />Refresh</button>
      </div>
      <Async state={activity} what="history" stale>
        {(items) => items.length === 0 ? <Empty>No activity yet.</Empty> : <HistoryTable items={items} names={names} />}
      </Async>
    </div>
  );
}
