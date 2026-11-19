"use client";

import { useEffect, useMemo, useRef, useState } from "react";
import { api, type Env } from "@/lib/api";
import { formatMoney, signOf, sumMoney } from "@/lib/money";
import { useAsync } from "@/lib/useAsync";
import { agentStyles, shortName } from "@/lib/agentColors";
import {
  GRID_COLS, MAX_ROWS, METRICS, PRESETS, addPanel, defaultTerminalState, loadTerminal, movePanel, removePanel, saveTerminal, updatePanel,
  type Layout, type Panel, type PanelMetric, type TerminalState,
} from "@/lib/layouts";
import { Async, EnvBadge } from "@/components/ui";
import { DevTick } from "@/components/DevTick";
import { Icon } from "@/components/shell/icons";
import { PanelBody, panelAgentId, type TerminalData } from "./panels";

const ROW_H = 230;

function useCols(ref: React.RefObject<HTMLDivElement | null>) {
  const [w, setW] = useState(1200);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(([e]) => setW(e.contentRect.width));
    ro.observe(el);
    return () => ro.disconnect();
  }, [ref]);
  return { width: w, cols: w > 1080 ? GRID_COLS : w > 640 ? 2 : 1 };
}

export function Terminal() {
  const [env, setEnv] = useState<Env>("simulated");
  const [tick, setTick] = useState(0);
  const [auto, setAuto] = useState(true);
  const portfolio = useAsync(() => api.getPortfolio(env), `t-portfolio:${env}:${tick}`);
  const agents = useAsync(api.listAgents, `t-agents:${tick}`);
  const wallet = useAsync(api.getWallet, `t-wallet:${tick}`);
  const activity = useAsync(() => api.listActivity({ limit: 60 }), `t-activity:${tick}`);
  const [state, setState] = useState<TerminalState>(defaultTerminalState);
  const [loaded, setLoaded] = useState(false);
  const [naming, setNaming] = useState<string | null>(null);
  const [adding, setAdding] = useState(false);

  // Layout lives in the browser; read it after mount (server render uses the default).
  // eslint-disable-next-line react-hooks/set-state-in-effect
  useEffect(() => { setState(loadTerminal()); setLoaded(true); }, []);
  useEffect(() => { if (loaded) saveTerminal(state); }, [state, loaded]);
  useEffect(() => {
    if (!auto) return;
    const t = window.setInterval(() => setTick((n) => n + 1), 15_000);
    return () => window.clearInterval(t);
  }, [auto]);

  const hasArc = agents.data?.some((a) => a.env === "arc_testnet") ?? false;
  const styles = useMemo(() => agentStyles((agents.data ?? []).map((a) => a.id)), [agents.data]);
  const setPanels = (fn: (p: Panel[]) => Panel[]) => setState((s) => ({ ...s, current: fn(s.current), activeId: null }));
  const allLayouts: Layout[] = [...PRESETS, ...state.saved];
  const active = allLayouts.find((l) => l.id === state.activeId);

  function applyLayout(l: Layout) {
    setState((s) => ({ ...s, current: l.panels.map((p) => ({ ...p })), activeId: l.id }));
  }
  function saveAs(name: string) {
    const id = `saved-${Date.now().toString(36)}`;
    setState((s) => ({ ...s, saved: [...s.saved, { id, name, panels: s.current.map((p) => ({ ...p })) }], activeId: id }));
    setNaming(null);
  }
  function deleteSaved(id: string) {
    setState((s) => ({ ...s, saved: s.saved.filter((l) => l.id !== id), activeId: s.activeId === id ? null : s.activeId }));
  }

  return (
    <div className="terminal">
      <div className="t-head">
        <div className="row" style={{ gap: 12 }}>
          <h1 style={{ margin: 0 }}>Terminal</h1>
          <div className="seg" role="group" aria-label="Funds environment">
            <button type="button" className={env === "simulated" ? "on" : ""} aria-pressed={env === "simulated"} onClick={() => setEnv("simulated")}>Simulated</button>
            <button type="button" className={env === "arc_testnet" ? "on" : ""} aria-pressed={env === "arc_testnet"} onClick={() => setEnv("arc_testnet")} disabled={!hasArc} title={hasArc ? undefined : "No agent has an Arc testnet vault yet"}>Arc testnet</button>
          </div>
        </div>
        <div className="row">
          <DevTick onDone={() => setTick((n) => n + 1)} />
          <button type="button" className={`sm ghost${auto ? " live" : ""}`} onClick={() => setAuto((a) => !a)} aria-pressed={auto} title="Refresh every 15 seconds">
            <span className={`live-dot${auto ? " on" : ""}`} />{auto ? "Live" : "Paused"}
          </button>
          <button type="button" className="icon-btn sm ghost" onClick={() => setTick((n) => n + 1)} aria-label="Refresh now" data-tip="Refresh now"><Icon name="refresh" size={15} /></button>
        </div>
      </div>

      <Async state={portfolio} what="portfolio" stale>
        {(p) => (
          <div className="ticker">
            <TickerItem label="Portfolio value" value={wallet.data && env === "simulated" ? formatMoney(sumMoney([wallet.data.unallocated, wallet.data.allocated])) : formatMoney(sumMoney([p.totals.unallocated, p.totals.allocated]))} note={<EnvBadge env={p.env} />} />
            <TickerItem label="Realized P&L" value={formatMoney(p.totals.realized_pnl, { signed: true })} tone={signOf(p.totals.realized_pnl)} note={p.formulas.portfolio_note && signOf(p.totals.realized_pnl) === 0 ? "nets to zero, see agents" : undefined} />
            <TickerItem label="At risk" value={formatMoney(p.totals.at_risk)} warn={signOf(p.totals.at_risk) > 0} note="committed stakes" />
            <TickerItem label="Free with agents" value={formatMoney(p.totals.available)} />
            <TickerItem label="Open / active" value={`${p.outcomes.open} / ${p.outcomes.active}`} note={`${p.outcomes.settled} settled`} />
            <TickerItem label="Unrealized" value="n/a" note="no fair mark before resolution" />
          </div>
        )}
      </Async>

      <div className="t-toolbar">
        <div className="row" style={{ gap: 6 }}>
          <Icon name="layout" size={15} className="dim" />
          <div className="seg" role="group" aria-label="Layouts">
            {PRESETS.map((l) => <button key={l.id} type="button" className={state.activeId === l.id ? "on" : ""} aria-pressed={state.activeId === l.id} onClick={() => applyLayout(l)}>{l.name}</button>)}
          </div>
          {state.saved.length > 0 && (
            <div className="seg" role="group" aria-label="Saved layouts">
              {state.saved.map((l) => (
                <span key={l.id} className="row nowrap" style={{ gap: 0 }}>
                  <button type="button" className={state.activeId === l.id ? "on" : ""} onClick={() => applyLayout(l)}>{l.name}</button>
                  <button type="button" className="xs" aria-label={`Delete layout ${l.name}`} onClick={() => deleteSaved(l.id)} style={{ padding: "0 4px" }}><Icon name="close" size={11} /></button>
                </span>
              ))}
            </div>
          )}
          {!active && <span className="dim xs">Custom (unsaved)</span>}
        </div>
        <div className="row" style={{ gap: 6 }}>
          {naming !== null ? (
            <form className="row nowrap" style={{ gap: 6 }} onSubmit={(e) => { e.preventDefault(); if (naming.trim()) saveAs(naming.trim()); }}>
              <input autoFocus value={naming} onChange={(e) => setNaming(e.target.value)} placeholder="Layout name" aria-label="Layout name" style={{ height: 28, width: 160 }} onKeyDown={(e) => e.key === "Escape" && setNaming(null)} />
              <button type="submit" className="sm primary" disabled={!naming.trim()}>Save</button>
              <button type="button" className="sm ghost" onClick={() => setNaming(null)}>Cancel</button>
            </form>
          ) : (
            <button type="button" className="sm ghost" onClick={() => setNaming("")}><Icon name="save" size={14} />Save layout</button>
          )}
          <div className="popwrap">
            <button type="button" className="sm outline" onClick={() => setAdding((a) => !a)} aria-expanded={adding}><Icon name="plus" size={14} />Add panel</button>
            {adding && (
              <div className="pop" role="menu" onMouseLeave={() => setAdding(false)}>
                {METRICS.map((m) => (
                  <button key={m.id} type="button" role="menuitem" className="pop-item" onClick={() => { setPanels((p) => addPanel(p, m.id)); setAdding(false); }}>
                    <span className="t">{m.label}</span><span className="d">{m.hint}</span>
                  </button>
                ))}
              </div>
            )}
          </div>
        </div>
      </div>

      <Async state={portfolio} what="panels" stale>
        {(p) => (
          <PanelGrid
            panels={state.current}
            d={{ portfolio: p, agents: agents.data ?? [], activity: activity.data, styles }}
            onChange={setPanels}
          />
        )}
      </Async>
      <p className="dim xs" style={{ marginTop: 12 }}>All figures come from the ledger via <span className="mono">/stats/portfolio</span>. Layouts are saved in this browser only. Drag a panel by its grip to move it; drag its corner to resize.</p>
    </div>
  );
}

function TickerItem({ label, value, note, tone, warn }: { label: string; value: string; note?: React.ReactNode; tone?: number; warn?: boolean }) {
  return (
    <div className="tk">
      <div className="tk-l">{label}</div>
      <div className={`tk-v num${tone && tone > 0 ? " pos" : tone && tone < 0 ? " neg" : ""}${warn ? " pend" : ""}`}>{value}</div>
      {note && <div className="tk-n">{note}</div>}
    </div>
  );
}

function PanelGrid({ panels, d, onChange }: { panels: Panel[]; d: TerminalData; onChange: (fn: (p: Panel[]) => Panel[]) => void }) {
  const grid = useRef<HTMLDivElement>(null);
  const { width, cols } = useCols(grid);
  const [dragId, setDragId] = useState<string | null>(null);
  const [overId, setOverId] = useState<string | null>(null);
  const [resize, setResize] = useState<{ id: string; w: number; h: number } | null>(null);
  const gap = 12;
  const colW = (width - gap * (cols - 1)) / cols;

  function startResize(e: React.PointerEvent, p: Panel) {
    e.preventDefault();
    const sx = e.clientX;
    const sy = e.clientY;
    const startW = Math.min(p.w, cols);
    let cur = { id: p.id, w: startW, h: p.h };
    setResize(cur);
    const move = (ev: PointerEvent) => {
      const w = Math.max(1, Math.min(cols, Math.round(startW + (ev.clientX - sx) / (colW + gap))));
      const h = Math.max(1, Math.min(MAX_ROWS, Math.round(p.h + (ev.clientY - sy) / (ROW_H + gap))));
      if (w !== cur.w || h !== cur.h) { cur = { id: p.id, w, h }; setResize(cur); }
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      onChange((ps) => updatePanel(ps, cur.id, { w: cols < GRID_COLS && cur.w === cols ? Math.max(p.w, cols) : cur.w, h: cur.h }));
      setResize(null);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  }

  if (panels.length === 0) {
    return (
      <div className="state" style={{ padding: 48 }}>
        <div className="text" style={{ marginBottom: 6 }}>No panels</div>
        Pick a layout above or use Add panel.
      </div>
    );
  }

  return (
    <div className="tgrid" ref={grid} style={{ gridTemplateColumns: `repeat(${cols}, minmax(0, 1fr))`, gridAutoRows: ROW_H, gap }}>
      {panels.map((p) => {
        const live = resize?.id === p.id ? resize : null;
        const w = Math.min(live?.w ?? p.w, cols);
        const h = live?.h ?? p.h;
        const meta = METRICS.find((m) => m.id === p.metric)!;
        const agentId = p.metric === "agent_pnl" ? panelAgentId(p, d) : null;
        const agentName = agentId ? d.portfolio.agents.find((a) => a.agent_id === agentId)?.name : null;
        return (
          <section
            key={p.id}
            className={`tp${dragId === p.id ? " dragging" : ""}${overId === p.id && dragId !== p.id ? " over" : ""}${live ? " resizing" : ""}`}
            style={{ gridColumn: `span ${w}`, gridRow: `span ${h}` }}
            onDragOver={(e) => { if (dragId) { e.preventDefault(); setOverId(p.id); } }}
            onDragLeave={() => setOverId((o) => (o === p.id ? null : o))}
            onDrop={(e) => { e.preventDefault(); if (dragId) onChange((ps) => movePanel(ps, dragId, p.id)); setDragId(null); setOverId(null); }}
            aria-label={meta.label}
          >
            <header className="tp-head">
              <span className="tp-grip" draggable
                onDragStart={(e) => { setDragId(p.id); e.dataTransfer.effectAllowed = "move"; e.dataTransfer.setData("text/plain", p.id); const el = (e.currentTarget as HTMLElement).closest("section"); if (el) e.dataTransfer.setDragImage(el, 24, 16); }}
                onDragEnd={() => { setDragId(null); setOverId(null); }}
                title="Drag to move" aria-label="Drag to move panel"><Icon name="grip" size={14} /></span>
              <select className="tp-metric" value={p.metric} aria-label="Panel metric" onChange={(e) => onChange((ps) => updatePanel(ps, p.id, { metric: e.target.value as PanelMetric, ...(e.target.value === "agent_pnl" ? { agentId: null, slot: 0 } : {}) }))}>
                {METRICS.map((m) => <option key={m.id} value={m.id}>{m.label}</option>)}
              </select>
              {p.metric === "agent_pnl" && (
                <select className="tp-metric sub" value={p.agentId ?? ""} aria-label="Agent" onChange={(e) => onChange((ps) => updatePanel(ps, p.id, { agentId: e.target.value || null }))}>
                  <option value="">{agentName ? `Auto · ${shortName(agentName)}` : "Auto"}</option>
                  {d.portfolio.agents.map((a) => <option key={a.agent_id} value={a.agent_id}>{shortName(a.name)}</option>)}
                </select>
              )}
              <span className="grow" />
              <span className="tp-size dim xs">{w}×{h}</span>
              <button type="button" className="icon-btn xs ghost tp-ctl" aria-label="Wider" title="Wider" onClick={() => onChange((ps) => updatePanel(ps, p.id, { w: p.w >= GRID_COLS ? 1 : p.w + 1 }))}><Icon name="expand" size={12} /></button>
              <button type="button" className="icon-btn xs ghost tp-ctl" aria-label="Remove panel" title="Remove" onClick={() => onChange((ps) => removePanel(ps, p.id))}><Icon name="close" size={12} /></button>
            </header>
            <div className="tp-body"><PanelBody panel={p} d={d} /></div>
            <span className="tp-resize" onPointerDown={(e) => startResize(e, p)} title="Drag to resize" aria-hidden />
          </section>
        );
      })}
    </div>
  );
}
