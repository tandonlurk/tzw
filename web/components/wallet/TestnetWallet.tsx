"use client";

import Link from "next/link";
import { api, isApiError, type Agent, type EnabledChain, type VaultView } from "@/lib/api";
import { formatMoney, signOf, sumMoney } from "@/lib/money";
import { useAsync } from "@/lib/useAsync";
import { vaultErrorText } from "@/lib/vault";
import { Async, Empty, Money, Tile } from "@/components/ui";
import { ChainGate, TESTNET_LABEL, WalletStatus } from "./WalletStatus";

type Row = { agent: Agent; vault: VaultView | null; error: string | null };

async function loadRows(): Promise<Row[]> {
  const agents = (await api.listAgents()).filter((a) => a.env === "arc_testnet");
  return Promise.all(
    agents.map(async (agent) => {
      try {
        return { agent, vault: await api.getVault(agent.id), error: null };
      } catch (e) {
        return { agent, vault: null, error: isApiError(e) ? vaultErrorText(e) : String(e) };
      }
    }),
  );
}

/** Wallet page section: connected-wallet USDC and per-agent vault balances, all Arc testnet. Never summed with simulated funds. */
export function TestnetWallet() {
  return (
    <div className="stack">
      <h2>Arc testnet</h2>
      <div className="dim">{TESTNET_LABEL}. Separate from the simulated balances above; the two are never added together.</div>
      <ChainGate>{(cfg) => <Inner cfg={cfg} />}</ChainGate>
    </div>
  );
}

function Inner({ cfg }: { cfg: EnabledChain }) {
  const rows = useAsync(loadRows, "testnet-vaults");
  return (
    <div className="stack">
      <WalletStatus cfg={cfg} />
      <Async state={rows} what="agent vaults">
        {(list) => {
          if (list.length === 0) {
            return <Empty>No agent has a vault yet. Open an agent under <Link href="/agents">My Agents</Link> and create one in its &quot;Arc testnet vault&quot; section.</Empty>;
          }
          const ok = list.filter((r) => r.vault);
          const inVaults = sumMoney(ok.map((r) => r.vault!.onchain_balance));
          const staked = sumMoney(ok.map((r) => r.vault!.onchain_exposure));
          return (
            <>
              <div className="grid">
                <Tile label="In agent vaults (onchain)" note={TESTNET_LABEL}><Money value={inVaults} /></Tile>
                <Tile label="Staked in challenges (onchain)" note="Held by the escrow contract"><Money value={staked} /></Tile>
              </div>
              <div className="row spread"><span /><button className="sm" onClick={rows.reload}>Refresh</button></div>
              <div className="table-wrap">
                <table>
                  <thead>
                    <tr><th>Agent</th><th>Vault</th><th className="num">In vault</th><th className="num">Staked</th><th className="num">Ledger available</th><th className="num">Drift</th><th>Status</th></tr>
                  </thead>
                  <tbody>
                    {list.map(({ agent, vault, error }) => (
                      <tr key={agent.id}>
                        <td><Link href={`/agents/${agent.id}`}>{agent.name}</Link></td>
                        {vault ? (
                          <>
                            <td><a className="mono" href={vault.explorer_url} target="_blank" rel="noreferrer" title={vault.address}>{vault.address.slice(0, 10)}...</a></td>
                            <td className="num"><Money value={vault.onchain_balance} /></td>
                            <td className="num"><Money value={vault.onchain_exposure} /></td>
                            <td className="num"><Money value={vault.ledger_available} /></td>
                            <td className={`num ${signOf(vault.drift) === 0 ? "" : "pend"}`}>{formatMoney(vault.drift, { signed: true })}</td>
                            <td>{vault.paused ? <span className="badge pend">paused</span> : <span className="badge pos">active</span>}</td>
                          </>
                        ) : (
                          <td colSpan={6} className="neg">{error}</td>
                        )}
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </>
          );
        }}
      </Async>
    </div>
  );
}
