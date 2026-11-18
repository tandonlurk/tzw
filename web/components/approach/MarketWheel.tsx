"use client";

import { useState } from "react";
import { includesCI } from "@/lib/approach";
import { arcPath, fitLabel, segmentCentre, subcategoriesOf, sunburstLayout, type Segment } from "@/lib/wheel";

const W = 320;
const C = W / 2;
const R_CENTRE = 44;
const R_IN0 = 48;
const R_IN1 = 100;
const R_OUT0 = 103;
const R_OUT1 = 152;

interface Props {
  categories: string[];
  subcategories: Record<string, string[]>;
  selectedCategories: string[];
  selectedSubcategories: string[];
  /** True when no market filter is set at all. */
  isAny: boolean;
  onToggleCategory: (c: string) => void;
  onToggleSubcategory: (s: string) => void;
  onAny: () => void;
}

/**
 * Sunburst: centre = any market, inner ring = categories, outer ring = subcategories. Click or Enter/Space toggles.
 * Categories and subcategories combine with OR (a market matches its category or its subcategory).
 */
export function MarketWheel(p: Props) {
  const [hover, setHover] = useState<string | null>(null);
  const { inner, outer } = sunburstLayout(p.categories, p.subcategories);

  const seg = (s: Segment, r0: number, r1: number, on: boolean, covered: boolean, toggle: () => void, label: string) => {
    const span = s.a1 - s.a0;
    const mid = segmentCentre(C, C, (r0 + r1) / 2, s);
    const maxChars = Math.max(0, Math.floor((span * (r0 + r1)) / 2 / 7));
    const showLabel = maxChars >= 3;
    return (
      <g key={s.key}>
        <path
          d={arcPath(C, C, r0, r1, s.a0 + 0.006, s.a1 - 0.006)}
          className={`sb-seg${on ? " on" : covered ? " covered" : ""}`}
          role="checkbox"
          aria-checked={on}
          aria-label={label}
          tabIndex={0}
          onClick={toggle}
          onKeyDown={(e) => {
            if (e.key === "Enter" || e.key === " ") {
              e.preventDefault();
              toggle();
            }
          }}
          onMouseEnter={() => setHover(label)}
          onMouseLeave={() => setHover(null)}
          onFocus={() => setHover(label)}
          onBlur={() => setHover(null)}
        >
          <title>{label}</title>
        </path>
        {showLabel && (
          <text x={mid.x} y={mid.y} className="sb-label" textAnchor="middle" dominantBaseline="middle" pointerEvents="none">
            {fitLabel(s.label, Math.min(maxChars, 12))}
          </text>
        )}
      </g>
    );
  };

  return (
    <svg viewBox={`0 0 ${W} ${W}`} className="wheel market-wheel" role="group" aria-label="Market wheel">
      {inner.map((s) => seg(s, R_IN0, R_IN1, includesCI(p.selectedCategories, s.label), false, () => p.onToggleCategory(s.label), s.label))}
      {outer.map((s) =>
        seg(
          s,
          R_OUT0,
          R_OUT1,
          includesCI(p.selectedSubcategories, s.label),
          includesCI(p.selectedCategories, s.parent ?? ""),
          () => p.onToggleSubcategory(s.label),
          `${s.parent} › ${s.label}`,
        ),
      )}
      <circle
        cx={C}
        cy={C}
        r={R_CENTRE}
        className={`sb-centre${p.isAny ? " on" : ""}`}
        role="button"
        aria-pressed={p.isAny}
        aria-label="Any market (clears every market filter)"
        tabIndex={0}
        onClick={p.onAny}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            p.onAny();
          }
        }}
      />
      <text x={C} y={C - 6} className="sb-centre-label" textAnchor="middle" pointerEvents="none">{hover ? fitLabel(hover.split(" › ").pop() ?? hover, 11) : "Any"}</text>
      <text x={C} y={C + 10} className="sb-centre-label dim" textAnchor="middle" pointerEvents="none">{hover ? "" : "market"}</text>
    </svg>
  );
}

/** Accessible fallback: the same selection as nested checkboxes. */
export function MarketList(p: Omit<Props, "isAny" | "onAny">) {
  return (
    <div className="market-list">
      {p.categories.map((c) => (
        <div key={c}>
          <label className="inline">
            <input type="checkbox" checked={includesCI(p.selectedCategories, c)} onChange={() => p.onToggleCategory(c)} /> {c} (all)
          </label>
          <div style={{ paddingLeft: "1.2rem" }}>
            {subcategoriesOf(p.subcategories, c).map((s) => (
              <label key={s} className="inline">
                <input type="checkbox" checked={includesCI(p.selectedSubcategories, s)} onChange={() => p.onToggleSubcategory(s)} /> {s}
              </label>
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}
