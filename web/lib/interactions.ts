import type { ChallengeTerms, EscrowTxs, Interaction, InteractionState, MarketRef } from "./api";
import { formatMoney } from "./money";

export type Tone = "pos" | "neg" | "pend" | "info" | "dim";

const STATE_LABEL: Record<InteractionState, string> = {
  proposed: "Proposed",
  negotiating: "Negotiating",
  accepted: "Accepted",
  awaiting_approval: "Awaiting approval",
  funded: "Funded",
  active: "Active",
  resolving: "Resolving",
  settled: "Settled",
  cancelled: "Cancelled",
  disputed: "Disputed",
  failed: "Failed",
};

export function stateLabel(s: InteractionState): string {
  return STATE_LABEL[s] ?? s;
}

/** Semantic tone: amber needs attention, mint is a good end, rose adverse, blue neutral/in flight. */
export function stateTone(s: InteractionState): Tone {
  switch (s) {
    case "awaiting_approval":
    case "disputed":
      return "pend";
    case "settled":
      return "pos";
    case "cancelled":
    case "failed":
      return "neg";
    default:
      return "info";
  }
}

/** States that are still in flight (not terminal). */
export function isOpen(s: InteractionState): boolean {
  return !["settled", "cancelled", "failed"].includes(s);
}

export function marketLabel(m: MarketRef): string {
  return m.kind === "gemini_prediction_contract" ? `${m.instrument_symbol} (${m.event_ticker})` : `${m.symbol} spot`;
}

/** Threshold in the unit of the formula, as people read it. */
export function formatThreshold(value: number, formula: ChallengeTerms["formula"]): string {
  if (formula === "pm_yes_price_bps_v1") return `${(value / 10000).toFixed(2)} YES price`;
  return `$${(value / 1_000_000).toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 6 })}`;
}

export function formatUnix(ts: number): string {
  return new Date(ts * 1000).toLocaleString();
}

export const RESOLUTION_LABEL: Record<ChallengeTerms["resolution_source"], string> = {
  gemini_prediction_markets: "Gemini prediction markets data",
  gemini_spot_ticker: "Gemini spot ticker",
};

/** Plain-language rows for the terms block. Resolution is always backend-administered (not trustless).
 *  On the offline fake market the terms still name the Gemini source, but the fake feed resolves it. */
export function termsRows(t: ChallengeTerms, env: string, marketSource?: Interaction["market_source"]): [string, string][] {
  const source = marketSource === "fake" ? `Offline fake market (stands in for ${RESOLUTION_LABEL[t.resolution_source]})` : RESOLUTION_LABEL[t.resolution_source];
  return [
    ["Market", marketLabel(t.market)],
    [
      "Proposer's side",
      `${t.direction === "above" ? "At or above" : "Below"} ${formatThreshold(t.threshold, t.formula)} at the end time`,
    ],
    ["Stake (each side)", formatMoney(t.stake)],
    ["Funding deadline", formatUnix(t.start_ts)],
    ["Ends", formatUnix(t.end_ts)],
    ["Resolved by", `${source}, backend-administered (not trustless)`],
    ["Environment", env === "simulated" ? "simulated funds, not real money" : env === "arc_testnet" ? "Arc testnet USDC, not real funds" : env],
  ];
}

/** One-line outcome for a terminal interaction, or null while still open. */
export function outcomeText(i: Interaction): string | null {
  if (i.state === "settled" && i.outcome && i.outcome !== "void") {
    const winner = i.outcome === "a" ? i.proposer_name : i.counterparty_name;
    const loser = i.outcome === "a" ? i.counterparty_name : i.proposer_name;
    return `${winner} won ${formatMoney(i.stake)} from ${loser}` + (i.observed_display ? ` (observed ${i.observed_display})` : "");
  }
  if (i.state === "cancelled" && i.outcome === "void") return "Voided: both stakes refunded";
  if (i.state === "cancelled") return `Cancelled${i.status_reason ? `: ${i.status_reason}` : ""}`;
  if (i.state === "failed") return `Failed${i.status_reason ? `: ${i.status_reason}` : ""}`;
  return null;
}

/** Reason text for a notice: trimmed, without a trailing period so callers can add punctuation. */
export function reasonSuffix(reason: string | null | undefined): string {
  const r = (reason ?? "").trim().replace(/\.+$/, "");
  return r ? ` Reason: ${r}.` : "";
}

/** Escrow transactions of an Arc testnet challenge in lifecycle order; null hashes are steps not reached (or not needed). */
export function escrowRows(e: EscrowTxs, proposer: string, counterparty: string): [string, string | null][] {
  return [
    ["Escrow created", e.create_tx],
    [`${proposer} stake`, e.proposer_stake_tx],
    [`${counterparty} stake`, e.counterparty_stake_tx],
    ["Resolved", e.settle_tx],
    [`Released to ${proposer}'s vault`, e.proposer_release_tx],
    [`Released to ${counterparty}'s vault`, e.counterparty_release_tx],
  ];
}
