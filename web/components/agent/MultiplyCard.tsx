"use client";

import Link from "next/link";
import { useState } from "react";
import { api, isApiError, type Agent, type ApiError, type MultiplyReport } from "@/lib/api";
import { formatMoney, parseMoney } from "@/lib/money";
import { useAsync } from "@/lib/useAsync";
import { MAX_CLONES, MAX_VARIANCE_BPS, PRESETS, paramLabel, paramValue, parseCount, splitBudget } from "@/lib/multiply";
import { ErrorBanner } from "@/components/ui";

type Choice = (typeof PRESETS)[number] | "custom";

/**
 * Multiply an agent (D-73): N clones of its current version, each with its own simulated allocation from
 * the owner's unallocated balance and seeded variance on strategy parameters. Review = server dry run;
 * confirm sends the same seed, so what is created is exactly what was previewed.
 */
export function MultiplyCard({ a, onChanged }: { a: Agent; onChanged: () => void }) {
  const wallet = useAsync(api.getWallet, "wallet");
  const [choice, setChoice] = useState<Choice>(10);
  const [custom, setCustom] = useState("5");
  const [variance, setVariance] = useState(1000);
  const [budget, setBudget] = useState("50");
  const [plan, setPlan] = useState<MultiplyReport | null>(null);
  const [done, setDone] = useState<MultiplyReport | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);

  const count = choice === "custom" ? parseCount(custom) : choice;
  const budgetMinor = parseMoney(budget);
  const unallocated = wallet.data?.unallocated ?? null;
  const split = count && budgetMinor ? splitBudget(budgetMinor, count, unallocated) : null;
  const problem = !count ? `Enter a number of clones from 1 to ${MAX_CLONES}.`
    : !budgetMinor || budgetMinor === "0" ? "Enter a budget in USDC (at most 6 decimals)."
    : !split ? "The budget is too small to give every clone something."
    : split.fundsOk === false ? `Not enough unallocated simulated funds: ${formatMoney(split.total)} needed, ${formatMoney(unallocated!)} available. Add simulated funds on the Wallet page.`
    : null;

  if (a.env !== "simulated") {
    return (
      <div className="card" id="multiply">
        <div className="card-head"><h3>Multiply</h3></div>
        <div className="dim small">Only agents on simulated funds can be multiplied.</div>
      </div>
    );
  }

  function reset() { setPlan(null); setDone(null); setError(null); }

  async function run(dryRun: boolean) {
    if (problem || !count || !budgetMinor) return;
    setBusy(true); setError(null);
    try {
      const out = await api.multiplyAgent(a.id, {
        count, variance_bps: variance, budget: budgetMinor, dry_run: dryRun,
        ...(plan && !dryRun ? { seed: plan.seed } : {}),
      });
      if (dryRun) setPlan(out);
      else { setDone(out); setPlan(null); wallet.reload(); onChanged(); }
    } catch (e) {
      if (isApiError(e)) setError(e);
    } finally {
      setBusy(false);
    }
  }

  if (done) {
    return (
      <div className="card" id="multiply">
        <div className="card-head"><h3>Multiply</h3><span className="badge sim">Simulated</span></div>
        <div className="notice info" role="status">
          Created {done.count} {done.count === 1 ? "clone" : "clones"} with {formatMoney(done.per_clone)} (simulated) each, {formatMoney(done.total)} in total. Seed {done.seed}.
        </div>
        <div className="row" style={{ marginTop: 12 }}>
          <Link className="btn outline" href="/agents">See the family</Link>
          <button type="button" className="ghost" onClick={reset}>Multiply again</button>
        </div>
      </div>
    );
  }

  return (
    <div className="card" id="multiply">
      <div className="card-head"><h3>Multiply</h3><span className="badge sim">Simulated</span></div>
      <div className="dim small" style={{ marginBottom: 12 }}>
        Run copies of this strategy, each slightly different, so you can keep up with more opportunities. Each clone gets its own simulated funds from your unallocated balance and keeps this agent&apos;s limits.
      </div>
      <ErrorBanner error={error} />

      <div className="field">
        <label id="mult-count">Copies</label>
        <div className="row">
          <div className="seg" role="group" aria-labelledby="mult-count">
            {PRESETS.map((n) => (
              <button key={n} type="button" className={choice === n ? "on" : ""} aria-pressed={choice === n} onClick={() => { setChoice(n); setPlan(null); }}>{n}x</button>
            ))}
            <button type="button" className={choice === "custom" ? "on" : ""} aria-pressed={choice === "custom"} onClick={() => { setChoice("custom"); setPlan(null); }}>Custom</button>
          </div>
          {choice === "custom" && (
            <input style={{ width: 80 }} value={custom} inputMode="numeric" aria-label={`Number of clones, 1 to ${MAX_CLONES}`} onChange={(e) => { setCustom(e.target.value); setPlan(null); }} />
          )}
        </div>
      </div>

      <div className="field">
        <label htmlFor="mult-variance">Variance: up to ±{variance / 100}%</label>
        <input id="mult-variance" type="range" min={0} max={MAX_VARIANCE_BPS} step={100} value={variance} onChange={(e) => { setVariance(Number(e.target.value)); setPlan(null); }} />
        <div className="hint">Nudges thresholds, windows, conviction gates and ambition per clone. Money limits never change; the policy&apos;s minimum conviction can only rise.</div>
      </div>

      <div className="field">
        <label htmlFor="mult-budget">Total budget</label>
        <div className="input-affix">
          <input id="mult-budget" value={budget} inputMode="decimal" onChange={(e) => { setBudget(e.target.value); setPlan(null); }} />
          <span className="affix">USDC</span>
        </div>
        <div className="hint">Unallocated: {unallocated === null ? "…" : `${formatMoney(unallocated)} (simulated)`}</div>
      </div>

      {split && count && (
        <div className="notice dim" style={{ marginBottom: 12 }}>
          <div>{count} × {formatMoney(split.perClone)} = <strong>{formatMoney(split.total)}</strong> (simulated)</div>
          {split.remainder !== "0" && <div>{formatMoney(split.remainder)} rounding remainder stays unallocated</div>}
          {split.after !== null && split.fundsOk && <div>Unallocated after: {formatMoney(split.after)}</div>}
          <div>Per clone: max {formatMoney(a.policy.max_per_interaction)} per challenge, approval above {formatMoney(a.policy.approval_threshold)}</div>
        </div>
      )}
      {problem && <div className="field-error" style={{ marginBottom: 12 }}>{problem}</div>}

      {plan ? (
        <>
          <ClonePreview plan={plan} />
          <div className="row nowrap" style={{ marginTop: 12 }}>
            <button type="button" className="grow" disabled={busy || !!problem} onClick={() => run(false)}>
              Create {plan.count} {plan.count === 1 ? "clone" : "clones"} · {formatMoney(plan.total)}
            </button>
            <button type="button" className="outline" disabled={busy} onClick={() => setPlan(null)}>Back</button>
          </div>
        </>
      ) : (
        <button type="button" className="outline" style={{ width: "100%" }} disabled={busy || !!problem} onClick={() => run(true)}>Review clones</button>
      )}
    </div>
  );
}

function ClonePreview({ plan }: { plan: MultiplyReport }) {
  const shown = plan.clones.slice(0, 5);
  const paths = shown[0]?.params.map((p) => p.path) ?? [];
  return (
    <div>
      <div className="dim xs" style={{ marginBottom: 6 }}>Preview, seed {plan.seed}{plan.clones.length > shown.length ? ` · first ${shown.length} of ${plan.clones.length}` : ""}</div>
      <div className="table-wrap">
        <table className="dense">
          <thead>
            <tr><th>Clone</th>{paths.map((p) => <th key={p}>{paramLabel(p)}</th>)}</tr>
          </thead>
          <tbody>
            {shown.map((c) => (
              <tr key={c.index}>
                <td>#{c.index}</td>
                {c.params.map((p) => <td key={p.path} className="num" title={`from ${paramValue(p.path, p.from)}`}>{paramValue(p.path, p.to)}</td>)}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
