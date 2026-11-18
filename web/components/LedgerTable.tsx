"use client";

import type { LedgerEntry } from "@/lib/api";
import { explorerTxUrl } from "@/lib/chain";
import { useChainConfig } from "@/lib/useChainConfig";
import { Empty, EnvBadge, Money, formatTs } from "./ui";

export function LedgerTable({ entries }: { entries: LedgerEntry[] }) {
  const chain = useChainConfig();
  const explorer = chain.data?.enabled ? chain.data.explorer_url : null;
  if (entries.length === 0) return <Empty>No ledger entries yet.</Empty>;
  return (
    <div className="table-wrap">
      <table>
        <thead>
          <tr><th>Time</th><th>Env</th><th>Account</th><th>Reason</th><th className="num">Change</th><th>Tx</th></tr>
        </thead>
        <tbody>
          {entries.map((e) => (
            <tr key={e.id}>
              <td>{formatTs(e.ts)}</td>
              <td><EnvBadge env={e.env} /></td>
              <td className="mono">{e.account}</td>
              <td>{e.reason}</td>
              <td className="num"><Money value={e.delta} signed tone /></td>
              <td className="mono">
                {e.tx_hash && explorer && e.env === "arc_testnet" ? (
                  <a href={explorerTxUrl(explorer, e.tx_hash)} target="_blank" rel="noreferrer" title={e.tx_hash}>{e.tx_hash.slice(0, 10)}...</a>
                ) : e.tx_hash ? (
                  `${e.tx_hash.slice(0, 10)}...`
                ) : (
                  ""
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
