"use client";

import Link from "next/link";
import { useState } from "react";
import type { ActivityItem } from "@/lib/api";
import { categoryOf, groupByDay, type ActivityCategory } from "@/lib/activityKinds";
import { shortName } from "@/lib/agentColors";
import { ActivityExtra, ActivityFirmLink, ActivityKindBadge } from "@/components/ActivityDetail";
import { Icon, type IconName } from "@/components/shell/icons";
import { StreamActivityBadge } from "@/components/StreamBadge";

export const CATEGORY_ICON: Record<ActivityCategory, IconName> = {
  discovery: "search",
  challenge: "challenge",
  negotiation: "branch",
  collaboration: "firm",
  approval: "approval",
  transaction: "wallet",
  settlement: "check",
  agent: "agents",
};

export function timeOf(ts: string): string {
  const d = new Date(ts);
  return Number.isNaN(d.getTime()) ? ts : d.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
}

/** One readable event: icon by category, summary, agent and links, details on demand. */
export function FeedItem({ it, agentName }: { it: ActivityItem; agentName?: string }) {
  const [open, setOpen] = useState(false);
  const cat = categoryOf(it.kind);
  return (
    <div className="feed-item">
      <span className={`feed-ico c-${cat}`}><Icon name={CATEGORY_ICON[cat]} size={15} /></span>
      <div className="feed-main">
        <div className="t">{it.summary}</div>
        <div className="m">
          <ActivityKindBadge kind={it.kind} />
          <StreamActivityBadge it={it} />
          {it.agent_id && <Link href={`/agents/${it.agent_id}`}>{agentName ? shortName(agentName) : "agent"}</Link>}
          <ActivityFirmLink it={it} />
          {it.collaboration_id && <Link href={`/collaborations/${it.collaboration_id}`}>collaboration</Link>}
          {it.interaction_id && <Link href={`/interactions/${it.interaction_id}`}>challenge</Link>}
          {it.detail && (
            <button type="button" className="xs ghost" style={{ height: 20, padding: "0 6px" }} aria-expanded={open} onClick={() => setOpen((o) => !o)}>
              {open ? "Hide details" : "Details"}
            </button>
          )}
        </div>
        <ActivityExtra it={it} />
        {open && it.detail && <pre style={{ marginTop: 8 }}>{JSON.stringify(it.detail, null, 2)}</pre>}
      </div>
      <time className="feed-time" dateTime={it.ts} title={new Date(it.ts).toLocaleString()}>{timeOf(it.ts)}</time>
    </div>
  );
}

/** Day-grouped feed. */
export function Feed({ items, names }: { items: ActivityItem[]; names: Map<string, string> }) {
  return (
    <div>
      {groupByDay(items).map((g) => (
        <section key={g.label + g.items[0].id} className="feed-day">
          <div className="eyebrow">{g.label}</div>
          <div className="feed">
            {g.items.map((it) => <FeedItem key={it.id} it={it} agentName={it.agent_id ? names.get(it.agent_id) : undefined} />)}
          </div>
        </section>
      ))}
    </div>
  );
}
