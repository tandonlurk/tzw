"use client";

import Link from "next/link";
import { api, type Agent } from "@/lib/api";
import { formatMoney, signOf } from "@/lib/money";
import { useAsync } from "@/lib/useAsync";
import { families, paramLabel, paramValue } from "@/lib/multiply";
import { winRateText } from "@/lib/agentView";

/** The agent's family (D-73): for a clone, where it came from and its exact variance; for any member, family totals. */
export function FamilyNote({ a }: { a: Agent }) {
  const all = useAsync(api.listAgents, "agents");
  if (!a.family_id) return null;
  const fam = families(all.data ?? []).find((f) => f.id === a.family_id);
  const pnl = fam ? signOf(fam.realizedPnl) : 0;
  return (
    <div className="card">
      <div className="card-head"><h3>Family</h3><span className="badge sim">Simulated</span></div>
      {a.clone && (
        <div className="small" style={{ marginBottom: 10 }}>
          Clone #{a.clone.index} of{" "}
          <Link href={`/agents/${a.clone.source_agent_id}`}>{all.data?.find((x) => x.id === a.clone!.source_agent_id)?.name ?? "its source"}</Link>{" "}
          (version {a.clone.source_version}), up to ±{a.clone.variance_bps / 100}% variance, seed {a.clone.seed}.
        </div>
      )}
      {fam && (
        <table className="kv"><tbody>
          <tr><td className="dim">Members</td><td>{fam.clones.length + (fam.root ? 1 : 0)} agents ({fam.clones.length} clones)</td></tr>
          <tr><td className="dim">Family realized P&amp;L</td><td className={pnl > 0 ? "pos" : pnl < 0 ? "neg" : ""}>{formatMoney(fam.realizedPnl, { signed: true })}</td></tr>
          <tr><td className="dim">Available / committed</td><td>{formatMoney(fam.available, { unit: false })} / {formatMoney(fam.committed)}</td></tr>
          <tr><td className="dim">Record</td><td>{winRateText(fam.wins, fam.settled)}</td></tr>
        </tbody></table>
      )}
      {a.clone && a.clone.params.length > 0 && (
        <details style={{ marginTop: 10 }}>
          <summary className="small">Parameters changed by variance</summary>
          <table className="kv" style={{ marginTop: 6 }}><tbody>
            {a.clone.params.map((p) => (
              <tr key={p.path} title={p.path}>
                <td className="dim">{paramLabel(p.path)}</td>
                <td className="num">{paramValue(p.path, p.from)} → {paramValue(p.path, p.to)}</td>
              </tr>
            ))}
          </tbody></table>
        </details>
      )}
      <div className="dim xs" style={{ marginTop: 10 }}>Family members never challenge each other, so the owner&apos;s money is not moved between its own agents.</div>
    </div>
  );
}
