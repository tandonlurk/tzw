import { describe, expect, it } from "vitest";
import type { StreamEvent } from "./api";
import { eventsForTx, latencyText, overallStatus, streamInfo, streamStatusTone } from "./streams";

const TX = "0x792b93c846b895f583082956e78853cb0ec71c444907e9a3e1e9e713c83f840f";

function ev(over: Partial<StreamEvent>): StreamEvent {
  return {
    id: "e", event: "Deposited", status: "confirmed", note: null, tx_hash: TX, log_index: 0,
    block_number: 73112834, block_timestamp: null, received_at: "2026-11-20T01:52:45Z",
    latency_ms: 841, removed: false, explorer_url: null, ...over,
  };
}

describe("streams", () => {
  it("formats latency and refuses nonsense", () => {
    expect(latencyText(841)).toBe("0.8 s");
    expect(latencyText(16932)).toBe("16.9 s");
    expect(latencyText(670_400)).toBe("11 min");
    expect(latencyText(null)).toBeNull();
    expect(latencyText(-5)).toBeNull();
  });

  it("reads stream facts only from QuickNode activity", () => {
    expect(streamInfo({ source: "lifecycle" })).toBeNull();
    expect(streamInfo(null)).toBeNull();
    expect(streamInfo({ source: "quicknode_streams", status: "confirmed", event: "Resolved", block_number: 7, latency_ms: 1200, explorer_url: "x" }))
      .toEqual({ status: "confirmed", event: "Resolved", block: 7, latencyMs: 1200, explorerUrl: "x" });
    // Raw chain_event rows have no status.
    expect(streamInfo({ source: "quicknode_streams", block_number: 7 })?.status).toBeNull();
  });

  it("matches events to a transaction case-insensitively and skips reorged ones", () => {
    const evs = [ev({ log_index: 14, event: "Activated" }), ev({ log_index: 13 }), ev({ log_index: 15, removed: true }), ev({ tx_hash: "0xother" })];
    expect(eventsForTx(evs, TX.toUpperCase().replace("0X", "0x")).map((e) => e.log_index)).toEqual([13, 14]);
    expect(eventsForTx(evs, null)).toEqual([]);
  });

  it("shows the worst status and maps tones", () => {
    expect(overallStatus([ev({}), ev({ status: "pending" })])).toBe("pending");
    expect(overallStatus([ev({}), ev({ status: "mismatch" })])).toBe("mismatch");
    expect(overallStatus([])).toBeNull();
    expect(streamStatusTone("mismatch")).toBe("neg");
    expect(streamStatusTone("confirmed")).toBe("pos");
  });
});
