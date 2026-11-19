"use client";

import { useEffect, useMemo, useRef, useState } from "react";
import type { BlockDef } from "@/lib/api";
import { FAMILIES, familyOf, type Family } from "@/lib/blockVisuals";
import { Icon } from "@/components/shell/icons";
import { ShapeGlyph, iconFor } from "./ShapeNode";

export const DRAG_MIME = "application/x-block-type";
const ORDER: Family[] = ["trigger", "data", "reasoning", "condition", "action", "financial", "code", "end"];

/** Searchable block picker (keyboard: type, arrows, Enter; Esc closes). Blocks can also be dragged onto the canvas. */
export function BlockMenu({ blocks, onPick, onClose, filter, title = "Add a block" }: {
  blocks: BlockDef[];
  onPick: (type: string) => void;
  onClose: () => void;
  /** Only show blocks this returns true for (e.g. compatible with a dragged connection). */
  filter?: (b: BlockDef) => boolean;
  title?: string;
}) {
  const [q, setQ] = useState("");
  const [cursor, setCursor] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  const list = useRef<HTMLDivElement>(null);

  const groups = useMemo(() => {
    const needle = q.trim().toLowerCase();
    const pool = blocks.filter((b) => b.type !== "approach.profile" && (!filter || filter(b)))
      .filter((b) => !needle || `${b.label} ${b.type} ${b.description} ${FAMILIES[familyOf(b.type, b.kind)].label}`.toLowerCase().includes(needle));
    return ORDER.map((fam) => ({ fam, items: pool.filter((b) => familyOf(b.type, b.kind) === fam) })).filter((g) => g.items.length);
  }, [blocks, q, filter]);
  const flat = groups.flatMap((g) => g.items);
  const cur = Math.min(cursor, Math.max(0, flat.length - 1));

  useEffect(() => { input.current?.focus(); }, []);
  useEffect(() => {
    list.current?.querySelector<HTMLElement>(`[data-i="${cur}"]`)?.scrollIntoView({ block: "nearest" });
  }, [cur]);

  function onKey(e: React.KeyboardEvent) {
    if (e.key === "Escape") { e.preventDefault(); onClose(); }
    else if (e.key === "ArrowDown") { e.preventDefault(); setCursor(Math.min(flat.length - 1, cur + 1)); }
    else if (e.key === "ArrowUp") { e.preventDefault(); setCursor(Math.max(0, cur - 1)); }
    else if (e.key === "Enter" && flat[cur]) { e.preventDefault(); onPick(flat[cur].type); }
  }

  let i = -1;
  return (
    <div className="bmenu" role="dialog" aria-label={title} onKeyDown={onKey}>
      <div className="bmenu-search">
        <Icon name="search" size={15} />
        <input ref={input} value={q} onChange={(e) => { setQ(e.target.value); setCursor(0); }} placeholder={`${title}…`} aria-label="Search blocks"
          role="combobox" aria-expanded aria-controls="bmenu-list" aria-activedescendant={flat[cur] ? `bm-${flat[cur].type}` : undefined} />
        <kbd>esc</kbd>
      </div>
      <div className="bmenu-list" id="bmenu-list" role="listbox" ref={list}>
        {flat.length === 0 && <div className="dim small" style={{ padding: 16 }}>No block matches “{q}”.</div>}
        {groups.map((g) => (
          <div key={g.fam}>
            <div className="bmenu-group">{FAMILIES[g.fam].label}<span className="dim"> · {FAMILIES[g.fam].blurb}</span></div>
            {g.items.map((b) => {
              i++;
              const idx = i;
              return (
                <div key={b.type} id={`bm-${b.type}`} data-i={idx} role="option" aria-selected={idx === cur}
                  className={`bmenu-item${idx === cur ? " on" : ""}`} draggable
                  onDragStart={(e) => { e.dataTransfer.setData(DRAG_MIME, b.type); e.dataTransfer.effectAllowed = "move"; }}
                  onMouseEnter={() => setCursor(idx)} onClick={() => onPick(b.type)}>
                  <ShapeGlyph family={g.fam} size={28} icon={iconFor(b.type, g.fam)} />
                  <div className="grow">
                    <div className="t">{b.label}</div>
                    <div className="d">{b.description}</div>
                  </div>
                </div>
              );
            })}
          </div>
        ))}
      </div>
      <div className="bmenu-foot"><span><kbd>↑</kbd> <kbd>↓</kbd> to move, <kbd>↵</kbd> to add</span><span>or drag onto the canvas</span></div>
    </div>
  );
}
