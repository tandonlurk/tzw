import { describe, expect, it } from "vitest";
import type { ChallengeTerms, Interaction } from "./api";
import { formatThreshold, isOpen, outcomeText, stateTone, termsRows } from "./interactions";
import { barPercents, scaleSeries, stepPath } from "./charts";

const terms: ChallengeTerms = {
  market: { kind: "gemini_prediction_contract", event_ticker: "EV1", instrument_symbol: "GEMI-EV1-YES" },
  direction: "above",
  threshold: 5500,
  start_ts: 1_700_000_000,
  end_ts: 1_700_003_600,
  stake: "2500000",
  resolution_source: "gemini_prediction_markets",
  formula: "pm_yes_price_bps_v1",
};

describe("interactions helpers", () => {
  it("formats thresholds by formula", () => {
    expect(formatThreshold(5500, "pm_yes_price_bps_v1")).toBe("0.55 YES price");
    expect(formatThreshold(65_000_000_000, "spot_last_price_micros_v1")).toContain("65,000");
  });
  it("labels resolution as backend-administered and env as simulated", () => {
    const rows = Object.fromEntries(termsRows(terms, "simulated"));
    expect(rows["Resolved by"]).toContain("backend-administered");
    expect(rows["Environment"]).toContain("simulated");
    expect(rows["Stake (each side)"]).toBe("2.50 USDC");
  });
  it("maps state tones and openness", () => {
    expect(stateTone("awaiting_approval")).toBe("pend");
    expect(stateTone("settled")).toBe("pos");
    expect(stateTone("cancelled")).toBe("neg");
    expect(isOpen("active")).toBe(true);
    expect(isOpen("settled")).toBe(false);
  });
  it("describes outcomes", () => {
    const base = { proposer_name: "Bull", counterparty_name: "Skeptic", stake: "2500000", observed_display: "0.70", status_reason: null } as unknown as Interaction;
    expect(outcomeText({ ...base, state: "settled", outcome: "a" })).toBe("Bull won 2.50 USDC from Skeptic (observed 0.70)");
    expect(outcomeText({ ...base, state: "cancelled", outcome: "void" })).toContain("refunded");
    expect(outcomeText({ ...base, state: "cancelled", outcome: null, status_reason: "limit" })).toBe("Cancelled: limit");
    expect(outcomeText({ ...base, state: "active", outcome: null })).toBeNull();
  });
});

describe("chart helpers", () => {
  it("scales with zero included and builds a step path", () => {
    const s = scaleSeries([{ x: 0, y: 10 }, { x: 10, y: -10 }], 100, 50, 0);
    expect(s.yMax).toBe(10);
    expect(s.yMin).toBe(-10);
    expect(s.points[0]).toEqual({ px: 0, py: 0 });
    expect(s.zeroY).toBe(25);
    expect(stepPath(s.points)).toBe("M0,0 H100 V50");
  });
  it("handles a single point and empty bars", () => {
    expect(scaleSeries([{ x: 5, y: 0 }], 100, 50).points[0].px).toBe(50);
    expect(barPercents([0, 0])).toEqual([0, 0]);
    expect(barPercents([5, -10])).toEqual([50, 100]);
  });
});
