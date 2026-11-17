"use client";

import { useState } from "react";
import { api, isApiError, type ApiError } from "@/lib/api";
import { parseMoney } from "@/lib/money";
import { useAsync } from "@/lib/useAsync";
import { Async, ErrorBanner, EnvBadge, Money, Tile } from "@/components/ui";
import { LedgerTable } from "@/components/LedgerTable";
import { TestnetWallet } from "@/components/wallet/TestnetWallet";

export default function WalletPage() {
  const wallet = useAsync(api.getWallet, "wallet");
  const [amount, setAmount] = useState("100");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  const [localErr, setLocalErr] = useState<string | null>(null);

  async function deposit() {
    const minor = parseMoney(amount);
    if (!minor || minor === "0") return setLocalErr("Enter a positive amount with at most 6 decimals.");
    setLocalErr(null);
    setBusy(true);
    setError(null);
    try {
      await api.simulatedDeposit(minor);
      wallet.reload();
    } catch (e) {
      if (isApiError(e)) setError(e);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div>
      <div className="page-head"><div><h1>Wallet</h1></div></div>
      <Async state={wallet} what="wallet">
        {(w) => (
          <div className="stack">
            <div className="grid">
              <Tile label="Unallocated" note={<EnvBadge env={w.env} />}><Money value={w.unallocated} /></Tile>
              <Tile label="Allocated to agents"><Money value={w.allocated} /></Tile>
              <Tile label="Committed"><Money value={w.committed} /></Tile>
            </div>
            {w.env === "simulated" && <div className="notice">These balances are simulated. They are not real funds.</div>}

            <div className="card">
              <h3 style={{ marginTop: 0 }}>Add simulated funds</h3>
              <div className="dim" style={{ marginBottom: ".5rem" }}>Credits play money to your unallocated balance. No real assets move.</div>
              <ErrorBanner error={error} />
              <div className="row">
                <input style={{ maxWidth: 160 }} value={amount} onChange={(e) => setAmount(e.target.value)} aria-label="Amount in USDC" inputMode="decimal" />
                <span className="dim">USDC (simulated)</span>
                <button className="primary" onClick={deposit} disabled={busy}>{busy ? "Adding..." : "Add simulated funds"}</button>
              </div>
              {localErr && <div className="field-error">{localErr}</div>}
            </div>

            <h2>Transactions</h2>
            <LedgerTable entries={w.entries} />
          </div>
        )}
      </Async>
      <TestnetWallet />
    </div>
  );
}
