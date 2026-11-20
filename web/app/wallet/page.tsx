"use client";

import Link from "next/link";
import { useState } from "react";
import { api, isApiError, type ApiError } from "@/lib/api";
import { formatMoney, parseMoney, sumMoney } from "@/lib/money";
import { useAsync } from "@/lib/useAsync";
import { agentStyles, shortName } from "@/lib/agentColors";
import { Async, Avatar, EnvBadge, ErrorBanner, Kpi, MoneyBar, PageHead } from "@/components/ui";
import { LedgerTable } from "@/components/LedgerTable";
import { TestnetWallet } from "@/components/wallet/TestnetWallet";
import { Icon } from "@/components/shell/icons";

type TxFilter = "all" | "deposit" | "allocate" | "stake" | "settle";
const MATCH: Record<Exclude<TxFilter, "all">, RegExp> = {
  deposit: /deposit/i,
  allocate: /alloc/i,
  stake: /stake|commit|refund|release/i,
  settle: /settle|payout|fee/i,
};

export default function WalletPage() {
  const wallet = useAsync(api.getWallet, "wallet");
  const agents = useAsync(api.listAgents, "agents");
  const [amount, setAmount] = useState("100");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  const [localErr, setLocalErr] = useState<string | null>(null);
  const [done, setDone] = useState<string | null>(null);
  const [filter, setFilter] = useState<TxFilter>("all");

  async function deposit() {
    const minor = parseMoney(amount);
    if (!minor || minor === "0") return setLocalErr("Enter a positive amount with at most 6 decimals.");
    setLocalErr(null); setBusy(true); setError(null); setDone(null);
    try {
      await api.simulatedDeposit(minor);
      wallet.reload();
      setDone(`Added ${formatMoney(minor)} (simulated)`);
    } catch (e) {
      if (isApiError(e)) setError(e);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div>
      <PageHead title="Wallet" />
      <Async state={wallet} what="wallet" stale>
        {(w) => {
          const free = (BigInt(w.allocated) - BigInt(w.committed)).toString();
          const sim = (agents.data ?? []).filter((a) => a.env === w.env);
          const styles = agentStyles((agents.data ?? []).map((a) => a.id));
          const entries = filter === "all" ? w.entries : w.entries.filter((e) => MATCH[filter].test(e.reason));
          return (
            <div className="stack">
              <div className="card" style={{ padding: "var(--s-6)" }}>
                <div className="row" style={{ marginBottom: 6 }}><span className="eyebrow">Total balance</span><EnvBadge env={w.env} /></div>
                <div className="hero" style={{ gridTemplateColumns: "minmax(0,1fr) minmax(0,1.3fr)", alignItems: "end" }}>
                  <div className="big">{formatMoney(sumMoney([w.unallocated, w.allocated]), { unit: false })}<span className="unit">USDC</span></div>
                  <div className="kpi-row">
                    <Kpi label="Available to you" note="unallocated">{formatMoney(w.unallocated)}</Kpi>
                    <Kpi label="Allocated to agents" note="free + committed">{formatMoney(w.allocated)}</Kpi>
                    <Kpi label="Committed" tone={BigInt(w.committed) > BigInt(0) ? "pend" : undefined}>{formatMoney(w.committed)}</Kpi>
                  </div>
                </div>
                <div style={{ marginTop: "var(--s-6)" }}>
                  <MoneyBar label="Funds by state" parts={[
                    { key: "u", label: "Unallocated", value: BigInt(w.unallocated), tone: "free", display: formatMoney(w.unallocated) },
                    { key: "a", label: "With agents, free", value: BigInt(free), tone: "alloc", display: formatMoney(free) },
                    { key: "c", label: "Committed", value: BigInt(w.committed), tone: "commit", display: formatMoney(w.committed) },
                  ]} />
                </div>
                {w.env === "simulated" && <div className="dim xs" style={{ marginTop: "var(--s-3)" }}>Simulated funds</div>}
              </div>

              <div className="grid-3" style={{ alignItems: "start" }}>
                <div className="card">
                  <div className="card-head"><h3>Add funds</h3><span className="badge sim">Simulated</span></div>
                  <div className="dim small" style={{ marginBottom: 12 }}>Add to your unallocated balance.</div>
                  <ErrorBanner error={error} />
                  <div className="input-affix" style={{ marginBottom: 10 }}>
                    <input value={amount} onChange={(e) => setAmount(e.target.value)} aria-label="Amount in USDC" inputMode="decimal" />
                    <span className="affix">USDC</span>
                  </div>
                  <div className="row" style={{ gap: 6, marginBottom: 10 }}>
                    {["50", "100", "500"].map((v) => <button key={v} type="button" className={`chip${amount === v ? " on" : ""}`} onClick={() => setAmount(v)}>{v}</button>)}
                  </div>
                  <button className="primary" style={{ width: "100%" }} onClick={deposit} disabled={busy}>{busy ? "Adding..." : "Add funds"}</button>
                  {localErr && <div className="field-error">{localErr}</div>}
                  {done && <div className="pos small" style={{ marginTop: 8 }} role="status"><Icon name="check" size={13} /> {done}</div>}
                </div>
                <div className="card" style={{ gridColumn: "span 2" }}>
                  <div className="card-head"><h3>By agent</h3><Link href="/agents" className="small">Manage agents</Link></div>
                  {sim.length === 0 ? <div className="dim small">No agents in this environment yet.</div> : (
                    <div className="table-wrap">
                      <table>
                        <thead><tr><th>Agent</th><th className="num">Free</th><th className="num">Committed</th><th className="num">Total</th></tr></thead>
                        <tbody>
                          {sim.map((a) => (
                            <tr key={a.id}>
                              <td><Link href={`/agents/${a.id}`} className="row nowrap" style={{ gap: 10, color: "var(--text)" }}><Avatar name={a.name} color={styles.get(a.id)!.color} size={24} />{shortName(a.name)}</Link></td>
                              <td className="num">{formatMoney(a.balances.available)}</td>
                              <td className={`num${BigInt(a.balances.committed) > BigInt(0) ? " pend" : ""}`}>{formatMoney(a.balances.committed)}</td>
                              <td className="num text">{formatMoney(sumMoney([a.balances.available, a.balances.committed]))}</td>
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    </div>
                  )}
                </div>
              </div>

              <div className="card pad-0">
                <div className="card-head" style={{ padding: "16px 20px 4px" }}>
                  <h3>Transactions</h3>
                  <div className="seg" role="group" aria-label="Transaction type">
                    {([["all", "All"], ["deposit", "Deposits"], ["allocate", "Allocations"], ["stake", "Stakes"], ["settle", "Settlements"]] as [TxFilter, string][]).filter(([k]) => k === "all" || w.entries.some((e) => MATCH[k as Exclude<TxFilter, "all">].test(e.reason))).map(([k, l]) => (
                      <button key={k} type="button" className={filter === k ? "on" : ""} aria-pressed={filter === k} onClick={() => setFilter(k)}>{l}</button>
                    ))}
                  </div>
                </div>
                <LedgerTable entries={entries} />
              </div>
            </div>
          );
        }}
      </Async>
      <div className="card" style={{ marginTop: "var(--gap)" }}>
        <TestnetWallet />
      </div>
    </div>
  );
}
