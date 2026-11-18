// State of the owner's wallet transactions in this browser session: signing -> pending -> confirmed / failed,
// then (for ledger-relevant txs) recording the hash in the backend. Pure reducer so the transitions are tested.

export type TxPhase = "signing" | "pending" | "confirmed" | "failed" | "rejected" | "error";
export type BackendPhase = "none" | "recording" | "recorded" | "error";

export interface TxRecord {
  id: number;
  label: string;
  phase: TxPhase;
  hash: string | null;
  /** Wallet / chain message for rejected, failed or error. */
  message: string | null;
  backend: BackendPhase;
  backendMessage: string | null;
}

export type TxAction =
  | { type: "start"; id: number; label: string }
  | { type: "sent"; id: number; hash: string }
  | { type: "mined"; id: number; ok: boolean }
  | { type: "wallet_error"; id: number; rejected: boolean; message: string }
  | { type: "backend_start"; id: number }
  | { type: "backend_done"; id: number; message: string }
  | { type: "backend_error"; id: number; message: string };

export function txReducer(list: TxRecord[], a: TxAction): TxRecord[] {
  if (a.type === "start") {
    const rec: TxRecord = { id: a.id, label: a.label, phase: "signing", hash: null, message: null, backend: "none", backendMessage: null };
    return [rec, ...list].slice(0, 20);
  }
  return list.map((r) => {
    if (r.id !== a.id) return r;
    switch (a.type) {
      case "sent":
        return { ...r, phase: "pending", hash: a.hash };
      case "mined":
        return { ...r, phase: a.ok ? "confirmed" : "failed", message: a.ok ? null : "The transaction reverted onchain. Nothing changed except the gas fee." };
      case "wallet_error":
        // An error after the hash is known (e.g. receipt timeout) leaves the tx possibly pending onchain.
        return { ...r, phase: a.rejected ? "rejected" : "error", message: a.message };
      case "backend_start":
        return { ...r, backend: "recording", backendMessage: null };
      case "backend_done":
        return { ...r, backend: "recorded", backendMessage: a.message };
      case "backend_error":
        return { ...r, backend: "error", backendMessage: a.message };
    }
  });
}

export function txPhaseText(r: TxRecord): { text: string; tone: "pend" | "pos" | "neg" | "dim" } {
  switch (r.phase) {
    case "signing":
      return { text: "Confirm in your wallet", tone: "pend" };
    case "pending":
      return { text: "Pending onchain", tone: "pend" };
    case "confirmed":
      return r.backend === "recording"
        ? { text: "Confirmed, recording in ledger", tone: "pend" }
        : r.backend === "error"
          ? { text: "Confirmed, not recorded in ledger", tone: "neg" }
          : { text: "Confirmed", tone: "pos" };
    case "failed":
      return { text: "Failed onchain", tone: "neg" };
    case "rejected":
      return { text: "Rejected in wallet", tone: "dim" };
    case "error":
      return { text: r.hash ? "Unknown, check explorer" : "Not sent", tone: "neg" };
  }
}

/** True while a tx is waiting on the owner or the chain, so the UI can block a second one. */
export function txBusy(list: TxRecord[]): boolean {
  return list.some((r) => r.phase === "signing" || r.phase === "pending" || r.backend === "recording");
}
