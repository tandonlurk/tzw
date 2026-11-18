"use client";

import { useEffect, useState } from "react";
import { api, ApiError, isApiError, type ApproachMarkets, type Bound, type MarketPreview } from "@/lib/api";
import {
  RANGE_SPECS,
  STRUCTURES,
  defaultMarkets,
  formatRangeValue,
  fromDisplay,
  includesCI,
  rangeErrors,
  serializeMarkets,
  structureLabel,
  toDisplay,
  toggleIn,
  type RangeSpec,
} from "@/lib/approach";
import { bpsToPercentText, secsToHoursText } from "@/lib/approach";
import { subcategoriesOf } from "@/lib/wheel";
import { useAsync } from "@/lib/useAsync";
import { detailText, formatTs } from "@/components/ui";
import { MarketList, MarketWheel } from "./MarketWheel";

interface Props {
  value: ApproachMarkets;
  onChange: (m: ApproachMarkets) => void;
  /** Agent the preview is for (lets the backend apply its window); omitted while creating. */
  agentId?: string | null;
}

/** Market scope editor: sunburst + chips + ranges + live preview. Writes the section 1 `markets` object. */
export function MarketEditor({ value, onChange, agentId }: Props) {
  const cats = useAsync(api.getMarketCategories, "market-categories");
  const set = (patch: Partial<ApproachMarkets>) => onChange(serializeMarkets({ ...value, ...patch }));
  const s = serializeMarkets(value);
  const localErrs = rangeErrors(value.ranges);
  const catList = cats.data?.categories ?? [];
  const subMap = cats.data?.subcategories ?? {};
  const parentOf = (sub: string) => catList.find((c) => includesCI(subcategoriesOf(subMap, c), sub)) ?? null;

  return (
    <div className="market-editor">
      <div className="market-top">
        <div className="wheel-box">
          {cats.error ? (
            <div className="state error" role="alert">
              Market categories unavailable: <strong>{cats.error.code}</strong> {cats.error.message}
              <div className="dim">Keywords, symbols and ranges below still work.</div>
              <button type="button" className="sm" onClick={cats.reload}>Retry</button>
            </div>
          ) : cats.loading && !cats.data ? (
            <div className="state" role="status">Loading market categories...</div>
          ) : catList.length === 0 ? (
            <div className="state">The backend returned no market categories.</div>
          ) : (
            <>
              <MarketWheel
                categories={catList}
                subcategories={subMap}
                selectedCategories={s.categories}
                selectedSubcategories={s.subcategories}
                isAny={s.scope === "any"}
                onToggleCategory={(c) => set({ categories: toggleIn(s.categories, c) })}
                onToggleSubcategory={(x) => set({ subcategories: toggleIn(s.subcategories, x) })}
                onAny={() => onChange(defaultMarkets())}
              />
              <div className="dim small">
                Categories: {cats.data?.source === "gemini" ? "live from Gemini" : "fallback list (Gemini unavailable or mocked)"}
              </div>
              <details>
                <summary>List view</summary>
                <MarketList
                  categories={catList}
                  subcategories={subMap}
                  selectedCategories={s.categories}
                  selectedSubcategories={s.subcategories}
                  onToggleCategory={(c) => set({ categories: toggleIn(s.categories, c) })}
                  onToggleSubcategory={(x) => set({ subcategories: toggleIn(s.subcategories, x) })}
                />
              </details>
            </>
          )}
        </div>

        <div className="market-side">
          <div className="crumbs" aria-label="Selected markets">
            <button type="button" className={`chip${s.scope === "any" ? " on" : ""}`} onClick={() => onChange(defaultMarkets())} title="Clear every market filter">Any market</button>
            {s.categories.map((c) => (
              <span key={`c${c}`} className="chip on">› {c} (all) <button type="button" aria-label={`Remove ${c}`} onClick={() => set({ categories: toggleIn(s.categories, c) })}>×</button></span>
            ))}
            {s.subcategories.map((x) => {
              const parent = parentOf(x);
              return (
                <span key={`s${x}`} className="chip on">› {parent ? `${parent} › ` : ""}{x} <button type="button" aria-label={`Remove ${x}`} onClick={() => set({ subcategories: toggleIn(s.subcategories, x) })}>×</button></span>
              );
            })}
          </div>
          {(s.categories.length > 0 || s.subcategories.length > 0) && (
            <div className="dim small">A market matches if its category OR its subcategory is picked. Every other filter below must also match.</div>
          )}

          <div className="field">
            <label>Structure</label>
            <div className="row">
              {(cats.data?.structures?.length ? cats.data.structures : [...STRUCTURES]).map((x) => (
                <button key={x} type="button" className={`chip${includesCI(s.structures, x) ? " on" : ""}`} aria-pressed={includesCI(s.structures, x)}
                  onClick={() => set({ structures: toggleIn(s.structures, x) })}>{structureLabel(x)}</button>
              ))}
            </div>
            <div className="dim small">None picked = every structure.</div>
          </div>
          <ChipInput label="Keywords" hint='Matched in titles, tickers and tags, e.g. "energy", "oil"' values={s.keywords} onChange={(keywords) => set({ keywords })} />
          <ChipInput label="Symbols" hint="Base assets, e.g. BTC" values={s.symbols} onChange={(symbols) => set({ symbols })} />
        </div>
      </div>

      <div className="ranges">
        {RANGE_SPECS.map((spec) => (
          <RangeRow key={spec.key} spec={spec} value={value.ranges[spec.key]} error={localErrs[`ranges.${spec.key}`]}
            onChange={(b) => set({ ranges: { ...value.ranges, [spec.key]: b } })} />
        ))}
      </div>

      <PreviewPanel markets={s} agentId={agentId ?? null} blocked={Object.keys(localErrs).length > 0} />
    </div>
  );
}

function ChipInput({ label, hint, values, onChange }: { label: string; hint: string; values: string[]; onChange: (v: string[]) => void }) {
  const [text, setText] = useState("");
  const add = () => {
    const parts = text.split(",").map((t) => t.trim()).filter(Boolean);
    if (parts.length) onChange(parts.reduce((acc, p) => (includesCI(acc, p) ? acc : [...acc, p]), values));
    setText("");
  };
  return (
    <div className="field">
      <label>{label}</label>
      <div className="row">
        {values.map((v) => (
          <span key={v} className="chip on">{v} <button type="button" aria-label={`Remove ${v}`} onClick={() => onChange(values.filter((x) => x !== v))}>×</button></span>
        ))}
        <input
          style={{ maxWidth: 180 }}
          value={text}
          aria-label={`Add ${label.toLowerCase()}`}
          placeholder="type, then Enter"
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" || e.key === ",") {
              e.preventDefault();
              add();
            }
          }}
          onBlur={add}
        />
      </div>
      <div className="dim small">{hint}. None = any.</div>
    </div>
  );
}

function RangeRow({ spec, value, error, onChange }: { spec: RangeSpec; value: Bound; error?: string; onChange: (b: Bound) => void }) {
  const summary =
    value.min === null && value.max === null ? "any" : `${value.min === null ? "no min" : formatRangeValue(spec, value.min)} to ${value.max === null ? "no max" : formatRangeValue(spec, value.max)}`;
  return (
    <div className={`range-row${error ? " has-error" : ""}`}>
      <div className="row spread">
        <strong>{spec.label}</strong>
        <span className={value.min === null && value.max === null ? "dim" : "info"}>{summary}</span>
      </div>
      <div className="dim small">{spec.hint}</div>
      <div className="range-bounds">
        <BoundInput spec={spec} which="min" v={value.min} onChange={(min) => onChange({ ...value, min })} />
        <BoundInput spec={spec} which="max" v={value.max} onChange={(max) => onChange({ ...value, max })} />
      </div>
      {error && <div className="field-error">{error}</div>}
    </div>
  );
}

function BoundInput({ spec, which, v, onChange }: { spec: RangeSpec; which: "min" | "max"; v: number | null; onChange: (v: number | null) => void }) {
  const [text, setText] = useState(toDisplay(spec, v));
  const [last, setLast] = useState(v);
  const [bad, setBad] = useState(false);
  if (v !== last) {
    // external change (slider, reset): resync the text unless it already says the same number
    setLast(v);
    const parsed = text.trim() === "" ? null : fromDisplay(spec, text);
    if (parsed !== v) {
      setText(toDisplay(spec, v));
      setBad(false);
    }
  }
  const sliderVal = v ?? (which === "min" ? spec.slider.min : spec.slider.max);
  const unitText = spec.unit === "h" ? "hours" : "%";
  return (
    <div className="bound">
      <label className="inline small">
        {which}
        <input
          value={text}
          inputMode="decimal"
          placeholder={`no ${which}`}
          aria-label={`${spec.label} ${which} (${unitText})`}
          onChange={(e) => {
            const t = e.target.value;
            setText(t);
            if (t.trim() === "") {
              setBad(false);
              return onChange(null);
            }
            const n = fromDisplay(spec, t);
            setBad(n === null);
            if (n !== null) onChange(n);
          }}
        />
        <span className="dim">{spec.unit}</span>
        {v !== null && <button type="button" className="sm" aria-label={`Clear ${spec.label} ${which}`} onClick={() => onChange(null)}>×</button>}
      </label>
      <input
        type="range"
        className={v === null ? "unset" : undefined}
        min={spec.slider.min}
        max={spec.slider.max}
        step={spec.slider.step}
        value={Math.min(spec.slider.max, Math.max(spec.slider.min, sliderVal))}
        aria-label={`${spec.label} ${which} slider`}
        aria-valuetext={v === null ? `no ${which}` : formatRangeValue(spec, v)}
        onChange={(e) => onChange(Number(e.target.value))}
      />
      {bad && <div className="field-error">{spec.unit === "h" ? "Hours, up to 2 decimals" : "Percent, up to 2 decimals"}</div>}
    </div>
  );
}

interface PreviewState {
  key: string;
  data: MarketPreview | null;
  error: ApiError | null;
}

/** Debounced POST /markets/preview for the current filter. */
function useMarketPreview(markets: ApproachMarkets, agentId: string | null, blocked: boolean) {
  const key = JSON.stringify(markets);
  const reqKey = `${key}|${agentId ?? ""}`;
  const [st, setSt] = useState<PreviewState | null>(null);
  const [tick, setTick] = useState(0);
  useEffect(() => {
    if (blocked) return;
    let cancelled = false;
    const t = setTimeout(() => {
      api
        .previewMarkets(JSON.parse(key) as ApproachMarkets, agentId ?? undefined)
        .then((data) => !cancelled && setSt({ key: reqKey, data, error: null }))
        .catch((e) => !cancelled && setSt({ key: reqKey, data: null, error: isApiError(e) ? e : new ApiError("client_error", String(e), 0) }));
    }, 400);
    return () => {
      cancelled = true;
      clearTimeout(t);
    };
  }, [key, reqKey, agentId, blocked, tick]);
  const current = st && st.key === reqKey ? st : null;
  return { current, last: st, loading: !blocked && current === null, retry: () => setTick((n) => n + 1) };
}

function sourceText(src: string): string {
  if (src === "gemini") return "live Gemini data";
  if (src === "fake") return "fake market data (simulated)";
  return src;
}

function PreviewPanel({ markets, agentId, blocked }: { markets: ApproachMarkets; agentId: string | null; blocked: boolean }) {
  const { current, last, loading, retry } = useMarketPreview(markets, agentId, blocked);
  const shown = current ?? last;
  return (
    <div className="preview card" aria-live="polite">
      {blocked ? (
        <div className="pend">Fix the range errors above to preview matching markets.</div>
      ) : shown?.error && current ? (
        <div className="neg" role="alert">
          Preview failed: <strong>{shown.error.unreachable ? "backend unreachable" : shown.error.code}</strong> {shown.error.unreachable ? "" : shown.error.message}
          {shown.error.details.length > 0 && <ul>{shown.error.details.map((d, i) => <li key={i}>{detailText(d)}</li>)}</ul>}
          <div><button type="button" className="sm" onClick={retry}>Retry</button></div>
        </div>
      ) : shown?.data ? (
        <div className={loading ? "stale" : undefined}>
          <div className="row spread">
            <span><strong className="big">{shown.data.count}</strong> market{shown.data.count === 1 ? "" : "s"} match now{loading ? " (updating...)" : ""}</span>
            <span className="dim small">{sourceText(shown.data.source)} · as of {formatTs(shown.data.as_of)}</span>
          </div>
          {shown.data.count === 0 ? (
            <div className="dim">Nothing matches right now. The agent will sit out until a market fits, or loosen the filters.</div>
          ) : (
            <ul className="sample">
              {shown.data.sample.map((m, i) => (
                <li key={i}>
                  {m.title}
                  <span className="dim small">
                    {[m.category && (m.subcategory ? `${m.category} › ${m.subcategory}` : m.category), m.structure && structureLabel(m.structure),
                      typeof m.move_bps === "number" && `move ${bpsToPercentText(m.move_bps)}%`,
                      typeof m.yes_price_bps === "number" && `YES ${bpsToPercentText(m.yes_price_bps)}%`,
                      typeof m.expiry_secs === "number" && `expires in ${secsToHoursText(m.expiry_secs)} h`]
                      .filter(Boolean).map((t) => ` · ${t}`).join("")}
                  </span>
                </li>
              ))}
              {shown.data.count > shown.data.sample.length && <li className="dim">and {shown.data.count - shown.data.sample.length} more</li>}
            </ul>
          )}
        </div>
      ) : (
        <div className="dim" role="status">Checking which markets match...</div>
      )}
    </div>
  );
}
