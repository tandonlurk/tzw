"use client";

import { useState } from "react";
import { api, isApiError, type TestRun } from "@/lib/api";
import { MAX_SOURCE_BYTES, STARTER_SOURCE, byteCounterText, reasonLabel, sourceBytes, sourceProblem, testRunSummary, verdictLabel, verdictTone } from "@/lib/customCode";
import { formatMoney } from "@/lib/money";

/** Plain textarea editor for a `custom.code` block's `source` param, with a dry-run button. */
export function CodeEditor({ source, onChange, agentId }: { source: string; onChange: (s: string) => void; agentId: string | null }) {
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<TestRun | null>(null);
  const [error, setError] = useState<string | null>(null);
  const bytes = sourceBytes(source);
  const problem = sourceProblem(source);

  async function testRun() {
    if (!agentId) return;
    setBusy(true);
    setError(null);
    setResult(null);
    try {
      setResult(await api.testRunCode(agentId, source));
    } catch (e) {
      setError(isApiError(e) ? (e.unreachable ? e.message : `${e.code}: ${e.message}${e.details.length ? ` (${e.details.map((d) => (d as { message?: string }).message ?? JSON.stringify(d)).join("; ")})` : ""}`) : "Test run failed");
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="code-editor nodrag nowheel">
      <div className="dim">JavaScript defining <span className="mono">function decide(ctx)</span>. Runs in a sandbox with no network, files or keys. Every returned intent still goes through the server&apos;s limit checks.</div>
      <textarea
        className="code mono"
        spellCheck={false}
        value={source}
        onChange={(e) => onChange(e.target.value)}
        aria-label="Custom code source"
      />
      <div className="row spread">
        <span className={bytes > MAX_SOURCE_BYTES ? "neg" : "dim"}>{byteCounterText(bytes)}</span>
        <span className="row">
          <button type="button" className="sm" onClick={() => { if (source.trim() === "" || window.confirm("Replace the current code with the starter template?")) onChange(STARTER_SOURCE); }}>Insert starter template</button>
          <button type="button" className="sm" disabled={busy || !agentId || problem !== null} onClick={testRun}>{busy ? "Running..." : "Test run (dry run)"}</button>
        </span>
      </div>
      {problem && <div className="field-error">{problem}</div>}
      {!agentId && <div className="dim">Save the agent once to enable Test run; it needs the agent&apos;s real balances.</div>}
      {error && <div className="field-error" role="alert">{error}</div>}
      {result && <TestRunView r={result} />}
    </div>
  );
}

export function TestRunView({ r }: { r: TestRun }) {
  return (
    <div className="card" style={{ marginTop: ".4rem" }} role="status">
      <div className="notice" style={{ marginBottom: ".4rem" }}>Dry run, simulated: nothing was written. No ledger entry, run, activity, challenge or approval.</div>
      <strong className={r.ok ? "" : "neg"}>{testRunSummary(r)}</strong>
      {r.error && <div className="neg">{r.error.message} <span className="mono dim">({r.error.code})</span></div>}
      {r.intents.length > 0 && (
        <ul>
          {r.intents.map((it, i) => (
            <li key={i}>
              <span className="mono">{it.intent.type}</span>{it.intent.stake ? ` stake ${formatMoney(it.intent.stake)}` : ""}:{" "}
              <span className={verdictTone(it.decision.result)}>{verdictLabel(it.decision.result)}</span>
              {it.decision.reasons.length > 0 && <> ({it.decision.reasons.map(reasonLabel).join("; ")})</>}
            </li>
          ))}
        </ul>
      )}
      <div className="dim">Fuel used: {r.fuel_used === null ? "n/a" : r.fuel_used.toLocaleString("en-US")} of 50,000,000</div>
      {r.logs.length > 0 && <details open><summary>Logs ({r.logs.length})</summary><pre>{r.logs.join("\n")}</pre></details>}
      <details><summary>Input the code saw (ctx)</summary><pre>{JSON.stringify(r.ctx, null, 2)}</pre></details>
    </div>
  );
}
