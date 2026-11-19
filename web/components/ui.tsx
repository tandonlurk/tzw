"use client";

import type { ReactNode } from "react";
import type { ApiError, Env } from "@/lib/api";
import type { AsyncState } from "@/lib/useAsync";
import { formatMoney, signOf } from "@/lib/money";
import { API_URL } from "@/lib/api";
import { unreachableHint } from "@/lib/errors";

export function Loading({ what = "data" }: { what?: string }) {
  return (
    <div className="loading" role="status" aria-label={`Loading ${what}`}>
      <div className="skeleton" style={{ height: 14, width: "38%" }} />
      <div className="skeleton" style={{ height: 14, width: "72%" }} />
      <div className="skeleton" style={{ height: 14, width: "56%" }} />
      <span className="sr-only">Loading {what}...</span>
    </div>
  );
}

export function Empty({ children }: { children: ReactNode }) {
  return <div className="state">{children}</div>;
}

export function ErrorState({ error, onRetry }: { error: ApiError; onRetry?: () => void }) {
  if (error.unreachable) {
    return (
      <div className="state unreachable" role="alert">
        <strong>Backend unreachable.</strong>
        <div>{unreachableHint(typeof window === "undefined" ? null : window.location.origin, API_URL)} Also check NEXT_PUBLIC_API_URL.</div>
        {onRetry && <button className="sm" onClick={onRetry} style={{ marginTop: ".5rem" }}>Retry</button>}
      </div>
    );
  }
  return (
    <div className="state error" role="alert">
      <strong>{error.code}</strong>: {error.message}
      {onRetry && (
        <div><button className="sm" onClick={onRetry} style={{ marginTop: ".5rem" }}>Retry</button></div>
      )}
    </div>
  );
}

/** Renders loading / error / content for an AsyncState. Empty handling is up to the caller. */
export function Async<T>({
  state,
  what,
  children,
}: {
  state: AsyncState<T>;
  what?: string;
  children: (data: T) => ReactNode;
}) {
  if (state.error) return <ErrorState error={state.error} onRetry={state.reload} />;
  if (state.loading || state.data === null) return <Loading what={what} />;
  return <>{children(state.data)}</>;
}

export function EnvBadge({ env }: { env: Env }) {
  return env === "simulated" ? (
    <span className="badge sim plain" title="Simulated balance. Not real funds.">Simulated</span>
  ) : (
    <span className="badge chain plain" title="Arc testnet USDC: onchain testnet tokens, not real funds.">Arc testnet</span>
  );
}

export function StatusBadge({ status }: { status: "active" | "paused" }) {
  return <span className={`badge ${status}`}>{status === "active" ? "Active" : "Paused"}</span>;
}

/** Money with optional semantic sign coloring (mint positive, rose negative). */
export function Money({ value, signed = false, tone = false }: { value: string; signed?: boolean; tone?: boolean }) {
  const s = signOf(value);
  const cls = tone ? (s > 0 ? "pos" : s < 0 ? "neg" : "") : "";
  return <span className={cls}>{formatMoney(value, { signed })}</span>;
}

export function Tile({ label, children, note }: { label: string; children: ReactNode; note?: ReactNode }) {
  return (
    <div className="card tile">
      <div className="label">{label}</div>
      <div className="value">{children}</div>
      {note && <div className="dim small" style={{ marginTop: 2 }}>{note}</div>}
    </div>
  );
}

/** Page title row with optional subtitle and actions on the right (primary action last, Fitts). */
export function PageHead({ title, sub, eyebrow, children }: { title: ReactNode; sub?: ReactNode; eyebrow?: ReactNode; children?: ReactNode }) {
  return (
    <div className="page-head">
      <div className="grow">
        {eyebrow && <div className="eyebrow" style={{ marginBottom: 6 }}>{eyebrow}</div>}
        <h1>{title}</h1>
        {sub && <div className="sub">{sub}</div>}
      </div>
      {children && <div className="row">{children}</div>}
    </div>
  );
}

/** Agent monogram in its identity colour. */
export function Avatar({ name, color, size = 32 }: { name: string; color: string; size?: number }) {
  const words = name.replace(/\(.*?\)/g, "").trim().split(/\s+/).filter(Boolean);
  const text = words.length > 1 ? (words[0][0] + words[1][0]).toUpperCase() : (words[0] ?? "?").slice(0, 2).toUpperCase();
  return (
    <span className="avatar" style={{ width: size, height: size, fontSize: size * 0.36, ["--c" as string]: color }} aria-hidden>
      {text}
    </span>
  );
}

/** Compact metric: label over value, optional delta line. */
export function Kpi({ label, children, note, tone }: { label: ReactNode; children: ReactNode; note?: ReactNode; tone?: "pos" | "neg" | "pend" | "info" }) {
  return (
    <div className="kpi">
      <div className="kpi-label">{label}</div>
      <div className={`kpi-value ${tone ?? ""}`}>{children}</div>
      {note && <div className="kpi-note">{note}</div>}
    </div>
  );
}

export interface SegPart { key: string; label: string; value: bigint; tone: "free" | "alloc" | "commit" | "other"; display: string }

/** Proportional stacked bar for money states (available / allocated / committed). Exact amounts are in the legend. */
export function MoneyBar({ parts, label }: { parts: SegPart[]; label: string }) {
  const total = parts.reduce((t, p) => t + (p.value > BigInt(0) ? p.value : BigInt(0)), BigInt(0));
  return (
    <div>
      <div className="moneybar" role="img" aria-label={label}>
        {total === BigInt(0) ? <div className="seg-empty" /> : parts.map((p) => {
          const pct = Number((p.value * BigInt(10000)) / total) / 100;
          return pct > 0 ? <div key={p.key} className={`mb-${p.tone}`} style={{ width: `${pct}%` }} title={`${p.label}: ${p.display}`} /> : null;
        })}
      </div>
      <div className="moneybar-legend">
        {parts.map((p) => (
          <div key={p.key} className="mbl">
            <span className={`mbl-sw mb-${p.tone}`} />
            <span className="dim">{p.label}</span>
            <span className="num text">{p.display}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

export function formatTs(ts: string): string {
  const d = new Date(ts);
  return Number.isNaN(d.getTime()) ? ts : d.toLocaleString();
}

/** Server-side error banner (non-field-specific) with details listed. */
export function ErrorBanner({ error }: { error: ApiError | null }) {
  if (!error) return null;
  if (error.unreachable) {
    return <div className="banner-error" role="alert"><strong>Backend unreachable.</strong> {error.message}</div>;
  }
  return (
    <div className="banner-error" role="alert">
      <strong>{error.code}</strong>: {error.message}
      {error.details.length > 0 && (
        <ul>{error.details.map((d, i) => <li key={i}>{detailText(d)}</li>)}</ul>
      )}
    </div>
  );
}

export function detailText(d: unknown): string {
  if (typeof d === "string") return d;
  if (d && typeof d === "object") {
    const o = d as Record<string, unknown>;
    const field = typeof o.field === "string" ? o.field : typeof o.path === "string" ? o.path : null;
    const msg = typeof o.message === "string" ? o.message : null;
    if (msg) return field ? `${field}: ${msg}` : msg;
    return JSON.stringify(d);
  }
  return String(d);
}
