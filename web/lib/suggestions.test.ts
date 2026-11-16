import { describe, expect, it } from "vitest";
import { ApiError } from "./api";
import { changeRows, decisionError, evidenceRows, sourceLabel, statusTone } from "./suggestions";

describe("suggestions helpers", () => {
  it("formats a proposed change as before -> after using money formatting", () => {
    const rows = changeRows(
      { policy: { max_per_interaction: "2500000", min_conviction_bps: 6500 }, config: { risk_tolerance: "low" } },
      { policy: { max_per_interaction: "5000000", min_conviction_bps: 5000 }, risk_tolerance: "medium" },
    );
    expect(rows).toEqual([
      { field: "policy.max_per_interaction", label: "Max per challenge", before: "5.00 USDC", after: "2.50 USDC" },
      { field: "policy.min_conviction_bps", label: "Minimum conviction", before: "50%", after: "65%" },
      { field: "config.risk_tolerance", label: "Risk tolerance", before: "medium", after: "low" },
    ]);
  });
  it("has unknown before when the agent is not loaded, and no rows for text-only", () => {
    expect(changeRows({ policy: { stop_loss: "1000000" } })[0].before).toBeNull();
    expect(changeRows(null)).toEqual([]);
  });
  it("formats evidence money and leaves other values alone", () => {
    const rows = Object.fromEntries(evidenceRows({ available: "50000000", seconds_left: 90, recent_settle_nets: ["-6000000", "1000000"], reason: "x_y" }));
    expect(rows["available"]).toBe("50.00 USDC");
    expect(rows["seconds left"]).toBe("90");
    expect(rows["recent settle nets"]).toBe("-6.00 USDC, 1.00 USDC");
    expect(rows["reason"]).toBe("x_y");
  });
  it("words 422 as still pending and 409 as stale", () => {
    const e422 = decisionError(new ApiError("validation_failed", "bad limit", 422));
    expect(e422.tone).toBe("bad");
    expect(e422.text).toContain("stays pending");
    expect(e422.text).toContain("bad limit");
    expect(decisionError(new ApiError("suggestion_not_pending", "x", 409)).stale).toBe(true);
    expect(decisionError(new ApiError("unreachable", "x", 0, [], true)).text).toContain("Nothing was changed");
  });
  it("maps tones and labels", () => {
    expect(statusTone("accepted")).toBe("pos");
    expect(statusTone("pending")).toBe("pend");
    expect(statusTone("expired")).toBe("dim");
    expect(sourceLabel("grok")).toBe("Grok");
    expect(sourceLabel("rules")).toBe("Rules");
  });
});
