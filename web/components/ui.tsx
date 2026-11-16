"use client";

import type { ReactNode } from "react";
import type { ApiError, Env } from "@/lib/api";
import type { AsyncState } from "@/lib/useAsync";
import { formatMoney, signOf } from "@/lib/money";
import { API_URL } from "@/lib/api";
import { unreachableHint } from "@/lib/errors";

export function Loading({ what = "data" }: { what?: string }) {
  return <div className="state" role="status">Loading {what}...</div>;
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
    <span className="badge sim" title="Simulated balance. Not real funds.">simulated</span>
  ) : (
    <span className="badge chain" title="Real testnet USDC on Arc testnet.">arc_testnet</span>
  );
}

export function StatusBadge({ status }: { status: "active" | "paused" }) {
  return <span className={`badge ${status}`}>{status}</span>;
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
      {note && <div className="dim" style={{ fontSize: ".8rem" }}>{note}</div>}
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
