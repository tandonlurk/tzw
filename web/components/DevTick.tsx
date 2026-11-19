"use client";

import { useEffect, useState } from "react";
import { api, isApiError, type TickReport } from "@/lib/api";

/** Dev-only control. Calls POST /dev/tick (one lifecycle step). Hidden when the endpoint is not mounted (404). */
export function DevTick({ onDone }: { onDone?: () => void }) {
  const [busy, setBusy] = useState(false);
  const [report, setReport] = useState<TickReport | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [hidden, setHidden] = useState(false);
  const [off, setOff] = useState(false);
  // Settings > Developer can hide this control (browser-only preference).
  // eslint-disable-next-line react-hooks/set-state-in-effect
  useEffect(() => setOff(window.localStorage.getItem("tzw.devControls") === "0"), []);
  if (hidden || off) return null;

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
    <div className="dev-box" title="Runs one lifecycle step now. Development only.">
      <span className="badge pend plain">Dev</span>
      <button className="xs ghost" disabled={busy} onClick={run} aria-label="Dev: run tick">{busy ? "Running..." : "Run tick"}</button>
      {summary && <span className="info xs">{summary}</span>}
      {report && report.errors.length > 0 && <span className="neg">{report.errors.join("; ")}</span>}
      {msg && <span className="neg">{msg}</span>}
    </div>
  );
}
