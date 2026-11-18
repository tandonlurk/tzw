// Display helpers for the vault view. The numbers (balances, drift) come from the backend; nothing is recomputed here.

import type { ApiError, LedgerEntry } from "./api";
import { formatMoney, signOf, sumMoney } from "./money";

/** Sentence for a `/vault/sync` result. */
export function syncNote(credited: LedgerEntry[]): string {
  if (credited.length === 0) return "Ledger already up to date (nothing new in this transaction).";
  const total = sumMoney(credited.map((e) => e.delta));
  const verb = signOf(total) < 0 ? "Debited" : "Credited";
  return `${verb} ${formatMoney(total.replace(/^-/, ""))} on the agent ledger (${credited.length} entr${credited.length === 1 ? "y" : "ies"}).`;
}

/** Null when chain and ledger agree, otherwise a short explanation. `drift` is signed, as reported by the backend. */
export function driftNote(drift: string): string | null {
  const s = signOf(drift);
  if (s === 0) return null;
  const amt = formatMoney(drift.replace(/^-/, ""));
  return s > 0
    ? `The vault holds ${amt} more than the ledger shows. A deposit may not be synced yet: sync its transaction hash below.`
    : `The ledger shows ${amt} more than the vault holds. A withdrawal may not be synced yet: sync its transaction hash below.`;
}

/** Plain-language text for the vault endpoints' error codes (docs: reports/handoffs/2026-11-17-wallet-ui.md). */
export function vaultErrorText(e: ApiError): string {
  switch (e.code) {
    case "vault_not_linked":
      return "This agent has no vault yet.";
    case "chain_unavailable":
      return "The backend cannot reach Arc testnet right now. Onchain balances are unavailable; try again shortly.";
    case "invalid_vault_tx":
      return `The backend did not accept this transaction: ${e.message}`;
    case "agent_has_open_interactions":
      return "This agent has open challenges or collaborations. Wait until they settle (or cancel them), then link the vault.";
    case "vault_already_linked":
      return "This agent already has a vault linked.";
    default:
      return `${e.code}: ${e.message}`;
  }
}
