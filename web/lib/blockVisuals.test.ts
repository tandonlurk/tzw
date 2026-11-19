import { describe, expect, it } from "vitest";
import { FAMILIES, familyOf, paramSummary, portLabel, reachableFromTriggers, shapePath } from "./blockVisuals";

describe("familyOf", () => {
  it("maps the backend catalog to distinct families", () => {
    expect(familyOf("trigger.market_move", "trigger")).toBe("trigger");
    expect(familyOf("reason.evaluate_opportunity", "reasoning")).toBe("reasoning");
    expect(familyOf("cond.conviction_gte", "condition")).toBe("condition");
    expect(familyOf("action.propose_challenge", "action")).toBe("action");
    expect(familyOf("action.request_approval", "action")).toBe("financial");
    expect(familyOf("custom.code", "action")).toBe("code");
    expect(familyOf("approach.profile", "approach")).toBe("data");
  });
  it("gives every family a different shape", () => {
    const shapes = Object.values(FAMILIES).map((f) => f.shape);
    expect(new Set(shapes).size).toBe(shapes.length);
    for (const s of shapes) expect(shapePath(s)).toMatch(/^M/);
  });
});

describe("paramSummary", () => {
  it("formats bps, money and seconds", () => {
    expect(paramSummary("cond.conviction_gte", { min_bps: 6000 })).toBe("≥ 60%");
    expect(paramSummary("cond.stake_lte", { max_stake: "2000000" })).toBe("2.00 USDC");
    expect(paramSummary("trigger.schedule", { every_secs: 300 })).toBe("5m");
    expect(paramSummary("custom.code", { source: "a\n\nb\n" })).toBe("2 lines of JS");
  });
});

describe("reachableFromTriggers", () => {
  it("follows edges from triggers only", () => {
    const nodes = [{ id: "t", type: "trigger.schedule" }, { id: "a", type: "reason.x" }, { id: "b", type: "action.y" }, { id: "o", type: "action.z" }];
    const r = reachableFromTriggers(nodes, [{ source: "t", target: "a" }, { source: "a", target: "b" }]);
    expect([...r].sort()).toEqual(["a", "b", "t"]);
  });
});

it("portLabel", () => {
  expect(portLabel("true")).toBe("yes");
  expect(portLabel("out")).toBe("");
});
