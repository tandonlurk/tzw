"use client";

import { barPercents } from "@/lib/charts";

export interface BarItem {
  label: string;
  value: number; // drawing only
  display: string; // exact text
  tone?: "pos" | "neg" | "pend" | "info";
}

/** Horizontal bars scaled to the largest absolute value. */
export function BarList({ items, ariaLabel }: { items: BarItem[]; ariaLabel: string }) {
  const pct = barPercents(items.map((i) => i.value));
  return (
    <div role="list" aria-label={ariaLabel}>
      {items.map((it, i) => (
        <div key={it.label} className="bar-row" role="listitem">
          <span title={it.label} style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{it.label}</span>
          <div className="bar-track"><div className={`bar-fill ${it.tone ?? ""}`} style={{ width: `${pct[i]}%` }} /></div>
          <span className={it.tone}>{it.display}</span>
        </div>
      ))}
    </div>
  );
}
