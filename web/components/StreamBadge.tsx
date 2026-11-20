import type { ActivityItem, StreamEvent } from "@/lib/api";
import { latencyText, overallStatus, streamInfo, streamStatusHint, streamStatusLabel, streamStatusTone } from "@/lib/streams";

/** "via QuickNode Streams · block N · 1.1 s" for activity that came from a stream delivery. */
export function StreamActivityBadge({ it }: { it: ActivityItem }) {
  const s = streamInfo(it.detail);
  if (!s) return null;
  const tone = s.status ? streamStatusTone(s.status) : "dim";
  const latency = latencyText(s.latencyMs);
  const title = [
    "Delivered by a QuickNode Stream on Arc testnet.",
    s.status ? streamStatusHint(s.status) : "Recorded only.",
    latency ? `Latency from the block's timestamp to delivery: ${latency} (block timestamps are whole seconds).` : "",
  ].filter(Boolean).join(" ");
  return (
    <span className={`badge ${tone}`} title={title}>
      via QuickNode Streams{s.block !== null && <> &middot; block {s.block}</>}{latency && <> &middot; {latency}</>}
      {s.status === "mismatch" && <> &middot; mismatch</>}
    </span>
  );
}

/** Compact Streams status for the escrow transactions table. */
export function StreamTxBadge({ events }: { events: StreamEvent[] }) {
  const status = overallStatus(events);
  if (!status) return <span className="dim small">not streamed</span>;
  const first = events[0];
  const latency = latencyText(first.latency_ms);
  const title = `${events.map((e) => e.event).join(" + ")}: ${streamStatusHint(status)}${events.map((e) => (e.note ? ` (${e.note})` : "")).join("")}`;
  return (
    <span className={`badge ${streamStatusTone(status)}`} title={title}>
      {streamStatusLabel(status)} &middot; block {first.block_number}{latency && <> &middot; {latency}</>}
    </span>
  );
}
