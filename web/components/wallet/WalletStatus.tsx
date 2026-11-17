"use client";

import type { ReactNode } from "react";
import type { EnabledChain } from "@/lib/api";
import { chainProblem, explorerAddressUrl, explorerTxUrl, shortAddress } from "@/lib/chain";
import { formatMoney } from "@/lib/money";
import { txPhaseText, type TxRecord } from "@/lib/txlog";
import { useChainConfig } from "@/lib/useChainConfig";
import { ErrorState, Loading } from "@/components/ui";
import { useUsdcBalance } from "./hooks";
import { useWallet } from "./WalletProvider";

/** Shown next to every onchain amount (AGENTS.md 3.2: never imply testnet funds are real). */
export const TESTNET_LABEL = "Arc testnet USDC (not real funds)";

/** Renders children with the backend's chain config, or explains why the wallet UI is off. */
export function ChainGate({ children, quiet = false }: { children: (cfg: EnabledChain) => ReactNode; quiet?: boolean }) {
  const chain = useChainConfig();
  if (chain.error) return <ErrorState error={chain.error} onRetry={chain.reload} />;
  if (chain.loading || !chain.data) return <Loading what="chain settings" />;
  const cfg = chain.data;
  if (!cfg.enabled) {
    return (
      <div className={quiet ? "dim" : "notice info"}>
        Arc testnet is not enabled on this backend, so browser-wallet vaults and onchain balances are unavailable.
      </div>
    );
  }
  const problem = chainProblem(cfg);
  if (problem) return <div className="notice bad" role="alert">Wallet features are off: {problem}</div>;
  return <>{children(cfg)}</>;
}

export function AddressLink({ cfg, address, you }: { cfg: EnabledChain; address: string; you?: boolean }) {
  return (
    <span>
      <a className="mono" href={explorerAddressUrl(cfg.explorer_url, address)} target="_blank" rel="noreferrer" title={address}>
        {shortAddress(address)}
      </a>
      {you && <span className="badge info" style={{ marginLeft: ".4rem" }}>you</span>}
    </span>
  );
}

export function TxLink({ cfg, hash }: { cfg: EnabledChain; hash: string }) {
  return (
    <a className="mono" href={explorerTxUrl(cfg.explorer_url, hash)} target="_blank" rel="noreferrer" title={hash}>
      {hash.slice(0, 10)}...
    </a>
  );
}

/** Connected wallet: address, network, USDC balance, with connect / switch actions and every failure state. */
export function WalletStatus({ cfg, compact = false }: { cfg: EnabledChain; compact?: boolean }) {
  const w = useWallet();
  const bal = useUsdcBalance(cfg);
  const onChain = w.chainId === cfg.chain_id;

  let body: ReactNode;
  if (!w.ready) {
    body = <span className="dim">Checking for a browser wallet...</span>;
  } else if (!w.provider) {
    body = (
      <div className="row">
        <span className="pend">No browser wallet detected.</span>
        <span className="dim">Install MetaMask (or another EIP-1193 wallet) to create and fund agent vaults, then reload.</span>
        <button className="sm" onClick={() => void w.connect()}>Check again</button>
      </div>
    );
  } else if (!w.account) {
    body = (
      <div className="row">
        <button className="primary" disabled={w.busy} onClick={() => void w.connect()}>{w.busy ? "Waiting for wallet..." : "Connect wallet"}</button>
        <span className="dim">Shares your address with this page. Nothing is signed.</span>
      </div>
    );
  } else {
    body = (
      <div className="row" style={{ gap: "1rem" }}>
        <span>Wallet <AddressLink cfg={cfg} address={w.account} /></span>
        {onChain ? (
          <span className="badge pos">{cfg.chain_name}</span>
        ) : (
          <span className="row">
            <span className="badge pend">Wrong network{w.chainId !== null ? ` (chain ${w.chainId})` : ""}</span>
            <button className="sm primary" disabled={w.busy} onClick={() => void w.switchTo(cfg)}>{w.busy ? "Waiting for wallet..." : `Switch to ${cfg.chain_name}`}</button>
          </span>
        )}
        <span>
          {bal.error ? (
            <span className="neg" title={bal.error.message}>USDC balance unavailable</span>
          ) : bal.data === null || bal.loading ? (
            <span className="dim">USDC ...</span>
          ) : (
            <strong>{formatMoney(bal.data.toString())}</strong>
          )}{" "}
          <span className="dim">{TESTNET_LABEL}</span>
          <button className="sm" style={{ marginLeft: ".4rem" }} onClick={bal.reload}>Refresh</button>
        </span>
      </div>
    );
  }

  const zero = bal.data !== null && bal.data !== undefined && bal.data === BigInt(0);
  return (
    <div className={compact ? "" : "card"}>
      {!compact && <h3 style={{ marginTop: 0 }}>Browser wallet</h3>}
      {body}
      {w.account && !onChain && <div className="dim" style={{ marginTop: ".4rem", fontSize: ".85rem" }}>Switching adds {cfg.chain_name} to your wallet if it is not there yet (chain id {cfg.chain_id}, gas paid in USDC).</div>}
      {w.account && zero && (
        <div className="dim" style={{ marginTop: ".4rem", fontSize: ".85rem" }}>
          No testnet USDC yet. Request some at <a href="https://faucet.circle.com" target="_blank" rel="noreferrer">faucet.circle.com</a> (network Arc Testnet). It also pays gas.
        </div>
      )}
      {w.error && !(w.error.kind === "no_wallet" && !w.provider) && <div className="notice bad" role="alert" style={{ marginTop: ".5rem" }}>{w.error.message}</div>}
    </div>
  );
}

/** Session log of wallet transactions with explorer links and a retry for a failed backend step. */
export function TxLog({ cfg, list, onRetry }: { cfg: EnabledChain; list: TxRecord[]; onRetry: (id: number) => void }) {
  if (list.length === 0) return null;
  return (
    <div className="table-wrap">
      <table>
        <thead><tr><th>Transaction</th><th>Status</th><th>Tx</th><th>Details</th></tr></thead>
        <tbody>
          {list.map((r) => {
            const s = txPhaseText(r);
            return (
              <tr key={r.id}>
                <td>{r.label}</td>
                <td><span className={`badge ${s.tone}`}>{s.text}</span></td>
                <td>{r.hash ? <TxLink cfg={cfg} hash={r.hash} /> : <span className="dim">-</span>}</td>
                <td>
                  {r.message && <div className={r.phase === "rejected" ? "dim" : "neg"}>{r.message}</div>}
                  {r.backendMessage && <div className={r.backend === "error" ? "neg" : "dim"}>{r.backendMessage}</div>}
                  {r.backend === "error" && <button className="sm" onClick={() => onRetry(r.id)}>Retry recording</button>}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
