import { describe, expect, it } from "vitest";
import { ApiError, type LedgerEntry } from "./api";
import { driftNote, syncNote, vaultErrorText } from "./vault";

const e = (delta: string): LedgerEntry => ({ id: delta, ts: "", env: "arc_testnet", account: "a", delta, reason: "vault_deposit", interaction_id: null, tx_hash: null });

describe("vault display helpers", () => {
  it("describes sync results", () => {
    expect(syncNote([])).toMatch(/up to date/);
    expect(syncNote([e("5000000")])).toBe("Credited 5.00 USDC on the agent ledger (1 entry).");
    expect(syncNote([e("-1500000")])).toBe("Debited 1.50 USDC on the agent ledger (1 entry).");
  });
  it("describes drift by sign", () => {
    expect(driftNote("0")).toBeNull();
    expect(driftNote("2000000")).toMatch(/vault holds 2.00 USDC more/);
    expect(driftNote("-2000000")).toMatch(/ledger shows 2.00 USDC more/);
  });
  it("maps vault error codes", () => {
    expect(vaultErrorText(new ApiError("agent_has_open_interactions", "x", 409))).toMatch(/open challenges/);
    expect(vaultErrorText(new ApiError("chain_unavailable", "x", 503))).toMatch(/cannot reach Arc/);
    expect(vaultErrorText(new ApiError("weird", "msg", 500))).toBe("weird: msg");
  });
});
