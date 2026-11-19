"use client";

import Link from "next/link";
import { useEffect, useMemo, useRef, useState } from "react";
import type { ActivityItem } from "@/lib/api";
import { CATEGORIES, categoryOf, type ActivityCategory } from "@/lib/activityKinds";
import { shortName } from "@/lib/agentColors";
import { ActivityExtra, ActivityFirmLink, ActivityKindBadge } from "@/components/ActivityDetail";
import { Icon } from "@/components/shell/icons";
import { CATEGORY_ICON } from "./Feed";

/** Dense, filterable event table with an inline detail drawer (Advanced mode). */
export function HistoryTable({ items, names, showAgent = true }: { items: ActivityItem[]; names: Map<string, string>; showAgent?: boolean }) {
  const [cat, setCat] = useState<ActivityCategory | "all">("all");
  const [q, setQ] = useState("");
  const [open, setOpen] = useState<ActivityItem | null>(null);
  const closeBtn = useRef<HTMLButtonElement>(null);
  const opener = useRef<HTMLElement | null>(null);
  const show = (it: ActivityItem) => { opener.current = document.activeElement as HTMLElement; setOpen(it); };
  useEffect(() => {
    if (!open) return;
    closeBtn.current?.focus();
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") setOpen(null); };
    window.addEventListener("keydown", onKey);
    return () => { window.removeEventListener("keydown", onKey); opener.current?.focus(); };
  }, [open]);
  const counts = useMemo(() => {
    const m = new Map<ActivityCategory, number>();
    for (const it of items) { const c = categoryOf(it.kind); m.set(c, (m.get(c) ?? 0) + 1); }
    return m;
  }, [items]);
  const rows = items.filter((it) => (cat === "all" || categoryOf(it.kind) === cat) && (!q || `${it.summary} ${it.kind} ${names.get(it.agent_id ?? "") ?? ""}`.toLowerCase().includes(q.toLowerCase())));

  return (
    <div>
      <div className="toolbar">
        <div className="seg" role="group" aria-label="Event type">
          <button type="button" className={cat === "all" ? "on" : ""} onClick={() => setCat("all")}>All {items.length}</button>
          {CATEGORIES.filter((c) => counts.has(c.id)).map((c) => (
            <button key={c.id} type="button" className={cat === c.id ? "on" : ""} aria-pressed={cat === c.id} onClick={() => setCat(c.id)}>{c.label} <span className="dim">{counts.get(c.id)}</span></button>
          ))}
        </div>
        <input type="search" placeholder="Search events" value={q} onChange={(e) => setQ(e.target.value)} aria-label="Search events" />
      </div>
      <div className="card pad-0 table-wrap">
        <table className="dense adv-table">
          <thead><tr><th style={{ width: 150 }}>Time</th><th style={{ width: 28 }} /><th>Event</th>{showAgent && <th>Agent</th>}<th>Summary</th><th>Links</th></tr></thead>
          <tbody>
            {rows.map((it) => {
              const c = categoryOf(it.kind);
              return (
                <tr key={it.id} className={open?.id === it.id ? "selected" : undefined} onClick={() => show(it)} tabIndex={0} aria-label={`${it.summary}. Open details`} onKeyDown={(e) => (e.key === "Enter" || e.key === " ") && (e.preventDefault(), show(it))}>
                  <td className="dim num">{new Date(it.ts).toLocaleString(undefined, { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit", second: "2-digit" })}</td>
                  <td><span className={`feed-ico c-${c}`} style={{ width: 22, height: 22, borderRadius: 6 }}><Icon name={CATEGORY_ICON[c]} size={12} /></span></td>
                  <td><ActivityKindBadge kind={it.kind} /></td>
                  {showAgent && <td>{it.agent_id ? shortName(names.get(it.agent_id) ?? it.agent_id.slice(0, 8)) : <span className="dim">–</span>}</td>}
                  <td className="wrap" style={{ maxWidth: 520, color: "var(--text-2)" }}>{it.summary}</td>
                  <td onClick={(e) => e.stopPropagation()}>
                    <span className="row nowrap" style={{ gap: 8 }}>
                      {it.interaction_id && <Link href={`/interactions/${it.interaction_id}`} className="xs">challenge</Link>}
                      {it.collaboration_id && <Link href={`/collaborations/${it.collaboration_id}`} className="xs">collab</Link>}
                      <ActivityFirmLink it={it} />
                    </span>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
        {rows.length === 0 && <div className="dim small" style={{ padding: 16 }}>No events match.</div>}
      </div>
      {open && (
        <>
          <div className="drawer-backdrop" onClick={() => setOpen(null)} />
          <aside className="drawer" role="dialog" aria-modal="true" aria-label="Event details">
            <div className="drawer-head">
              <div className="row"><ActivityKindBadge kind={open.kind} /><span className="dim small">{new Date(open.ts).toLocaleString()}</span></div>
              <button ref={closeBtn} type="button" className="icon-btn sm ghost" onClick={() => setOpen(null)} aria-label="Close details (Esc)"><Icon name="close" size={15} /></button>
            </div>
            <div className="drawer-body">
              <div className="text" style={{ fontSize: "1rem", marginBottom: 12 }}>{open.summary}</div>
              <table className="kv"><tbody>
                <tr><td>Kind</td><td className="mono">{open.kind}</td></tr>
                <tr><td>Category</td><td>{CATEGORIES.find((c) => c.id === categoryOf(open.kind))?.label}</td></tr>
                {open.agent_id && <tr><td>Agent</td><td><Link href={`/advanced/agents/${open.agent_id}`}>{names.get(open.agent_id) ?? open.agent_id}</Link></td></tr>}
                {open.interaction_id && <tr><td>Challenge</td><td><Link href={`/interactions/${open.interaction_id}`} className="mono">{open.interaction_id}</Link></td></tr>}
                {open.collaboration_id && <tr><td>Collaboration</td><td><Link href={`/collaborations/${open.collaboration_id}`} className="mono">{open.collaboration_id}</Link></td></tr>}
                <tr><td>Event id</td><td className="mono xs">{open.id}</td></tr>
              </tbody></table>
              <div style={{ marginTop: 12 }}><ActivityExtra it={open} /></div>
              {open.detail && <><div className="eyebrow" style={{ margin: "16px 0 6px" }}>Raw record</div><pre>{JSON.stringify(open.detail, null, 2)}</pre></>}
            </div>
          </aside>
        </>
      )}
    </div>
  );
}
