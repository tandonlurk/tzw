"use client";

import { useState } from "react";
import { api, isApiError, type TickReport } from "@/lib/api";

/** Dev-only control. Calls POST /dev/tick (one lifecycle step). Hidden when the endpoint is not mounted (404). */
export function DevTick({ onDone }: { onDone?: () => void }) {
  const [busy, setBusy] = useState(false);
  const [report, setReport] = useState<TickReport | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [hidden, setHidden] = useState(false);
  if (hidden) return null;

  async function run() {
    setBusy(true);
    setMsg(null);
    try {
      setReport(await api.devTick());
      onDone?.();
    } catch (e) {
      if (isApiError(e) && e.status === 404) setHidden(true);
      else setMsg(isApiError(e) ? e.message : "Tick failed");
    } finally {
      setBusy(false);
    }
  }

  const summary = report
    ? Object.entries(report).filter(([k, v]) => typeof v === "number" && v > 0 && k !== "now").map(([k, v]) => `${k}: ${v}`).join(", ") || "nothing happened"
    : null;

  return (
    <div className="dev-box row">
      <span className="badge pend">Dev</span>
      <button className="sm" disabled={busy} onClick={run}>Dev: run tick</button>
      <span className="dim" style={{ fontSize: ".85rem" }}>Runs one lifecycle step now. Development only.</span>
      {summary && <span className="info">{summary}</span>}
      {report && report.errors.length > 0 && <span className="neg">{report.errors.join("; ")}</span>}
      {msg && <span className="neg">{msg}</span>}
    </div>
  );
}
