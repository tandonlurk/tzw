// QuickNode Streams confirmations (D-74): how a stream-delivered escrow event compares with the
// transactions the backend confirmed from its own receipts. Display helpers only; the backend
// decides the status and nothing here moves state or money.

import type { StreamEvent, StreamStatus } from "./api";
import type { Tone } from "./interactions";

const STATUS: Record<StreamStatus, { label: string; tone: Tone; hint: string }> = {
  confirmed: { label: "confirmed", tone: "pos", hint: "Same transaction and values as the backend's verified receipt." },
  pending: { label: "waiting for backend", tone: "pend", hint: "Streams delivered it before the backend confirmed the same step; rechecked every tick." },
  mismatch: { label: "mismatch", tone: "neg", hint: "Disagrees with what the backend confirmed. Flagged only: nothing was changed." },
  observed: { label: "observed", tone: "info", hint: "A real escrow event the backend did not send (e.g. a timeout refund)." },
  unmatched: { label: "unknown id", tone: "dim", hint: "No challenge or agent on this backend has this id." },
};

function isStatus(s: unknown): s is StreamStatus {
  return typeof s === "string" && s in STATUS;
}

export function streamStatusLabel(s: StreamStatus): string {
  return STATUS[s]?.label ?? s;
}

export function streamStatusTone(s: StreamStatus): Tone {
  return STATUS[s]?.tone ?? "dim";
}

export function streamStatusHint(s: StreamStatus): string {
  return STATUS[s]?.hint ?? "";
}

/** Block-to-delivery latency, e.g. `1.1 s`. Block timestamps are whole seconds, so this is approximate. */
export function latencyText(ms: number | null | undefined): string | null {
  if (typeof ms !== "number" || !Number.isFinite(ms) || ms < 0) return null;
  if (ms >= 120_000) return `${Math.round(ms / 60_000)} min`;
  return `${(ms / 1000).toFixed(1)} s`;
}

export interface StreamInfo {
  status: StreamStatus | null;
  event: string | null;
  block: number | null;
  latencyMs: number | null;
  explorerUrl: string | null;
}

/** Streams facts carried by an activity item's `detail`, or null when it did not come from QuickNode Streams. */
export function streamInfo(detail: Record<string, unknown> | null | undefined): StreamInfo | null {
  if (!detail || detail.source !== "quicknode_streams") return null;
  const num = (v: unknown) => (typeof v === "number" && Number.isFinite(v) ? v : null);
  const str = (v: unknown) => (typeof v === "string" && v ? v : null);
  return {
    status: isStatus(detail.status) ? detail.status : null,
    event: str(detail.event),
    block: num(detail.block_number),
    latencyMs: num(detail.latency_ms),
    explorerUrl: str(detail.explorer_url),
  };
}

/** Stream events for one transaction (not reorged out), in log order. */
export function eventsForTx(events: StreamEvent[] | undefined, hash: string | null): StreamEvent[] {
  if (!events || !hash) return [];
  const h = hash.toLowerCase();
  return events.filter((e) => !e.removed && e.tx_hash === h).sort((a, b) => a.log_index - b.log_index);
}

/** The status to show for a set of events: the worst one wins (mismatch > pending > observed > confirmed). */
export function overallStatus(events: StreamEvent[]): StreamStatus | null {
  const order: StreamStatus[] = ["mismatch", "pending", "observed", "unmatched", "confirmed"];
  for (const s of order) if (events.some((e) => e.status === s)) return s;
  return null;
}
