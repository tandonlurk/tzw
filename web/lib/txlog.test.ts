import { describe, expect, it } from "vitest";
import { txBusy, txPhaseText, txReducer, type TxRecord } from "./txlog";

const H = `0x${"ab".repeat(32)}`;

describe("txReducer", () => {
  it("walks signing -> pending -> confirmed -> recorded", () => {
    let s: TxRecord[] = txReducer([], { type: "start", id: 1, label: "Allocate 5 USDC" });
    expect(s[0].phase).toBe("signing");
    expect(txBusy(s)).toBe(true);
    s = txReducer(s, { type: "sent", id: 1, hash: H });
    expect(s[0]).toMatchObject({ phase: "pending", hash: H });
    expect(txPhaseText(s[0]).text).toBe("Pending onchain");
    s = txReducer(s, { type: "mined", id: 1, ok: true });
    s = txReducer(s, { type: "backend_start", id: 1 });
    expect(txPhaseText(s[0]).text).toMatch(/recording/);
    expect(txBusy(s)).toBe(true);
    s = txReducer(s, { type: "backend_done", id: 1, message: "Credited 5.00 USDC" });
    expect(s[0]).toMatchObject({ phase: "confirmed", backend: "recorded", backendMessage: "Credited 5.00 USDC" });
    expect(txPhaseText(s[0])).toEqual({ text: "Confirmed", tone: "pos" });
    expect(txBusy(s)).toBe(false);
  });
  it("records rejection and failure", () => {
    let s = txReducer([], { type: "start", id: 1, label: "x" });
    s = txReducer(s, { type: "wallet_error", id: 1, rejected: true, message: "You rejected" });
    expect(txPhaseText(s[0]).text).toBe("Rejected in wallet");
    s = txReducer(s, { type: "start", id: 2, label: "y" });
    s = txReducer(s, { type: "sent", id: 2, hash: H });
    s = txReducer(s, { type: "mined", id: 2, ok: false });
    expect(s[0]).toMatchObject({ id: 2, phase: "failed" });
    expect(s[1]).toMatchObject({ id: 1, phase: "rejected" });
  });
  it("flags a confirmed tx the backend could not record", () => {
    let s = txReducer([], { type: "start", id: 1, label: "x" });
    s = txReducer(s, { type: "sent", id: 1, hash: H });
    s = txReducer(s, { type: "mined", id: 1, ok: true });
    s = txReducer(s, { type: "backend_error", id: 1, message: "invalid_vault_tx" });
    expect(txPhaseText(s[0])).toEqual({ text: "Confirmed, not recorded in ledger", tone: "neg" });
  });
  it("distinguishes not-sent from unknown-after-send", () => {
    let s = txReducer([], { type: "start", id: 1, label: "x" });
    s = txReducer(s, { type: "wallet_error", id: 1, rejected: false, message: "boom" });
    expect(txPhaseText(s[0]).text).toBe("Not sent");
    s = txReducer(s, { type: "start", id: 2, label: "y" });
    s = txReducer(s, { type: "sent", id: 2, hash: H });
    s = txReducer(s, { type: "wallet_error", id: 2, rejected: false, message: "timeout" });
    expect(txPhaseText(s[0]).text).toMatch(/explorer/);
  });
  it("keeps at most 20 records", () => {
    let s: TxRecord[] = [];
    for (let i = 0; i < 25; i++) s = txReducer(s, { type: "start", id: i, label: String(i) });
    expect(s).toHaveLength(20);
    expect(s[0].id).toBe(24);
  });
});
