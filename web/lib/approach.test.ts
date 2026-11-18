import { describe, expect, it } from "vitest";
import type { Graph } from "./api";
import {
  APPROACH_BLOCK_TYPE,
  approachOf,
  approachToParams,
  bpsToPercentText,
  defaultApproach,
  defaultMarkets,
  deriveScope,
  ensureApproachNode,
  hoursTextToSecs,
  marketsSummary,
  maxOpenInteractions,
  minConvictionBps,
  normalizeApproach,
  nudgeStance,
  percentTextToBps,
  pointToStance,
  rangeErrors,
  secsToHoursText,
  serializeMarkets,
  stakeTarget,
  stakeTargetBps,
  stanceEffects,
  stanceToPoint,
  stanceZone,
  toggleIn,
} from "./approach";

describe("approach formulas (section 1)", () => {
  it("stake target = max × (2000 + 8a/10) / 10000 with integer division", () => {
    expect(stakeTarget("5000000", 0)).toBe("1000000");
    expect(stakeTarget("5000000", 3000)).toBe("2200000");
    expect(stakeTarget("5000000", 5000)).toBe("3000000");
    expect(stakeTarget("5000000", 7000)).toBe("3800000");
    expect(stakeTarget("5000000", 10000)).toBe("5000000");
    expect(stakeTarget("5000000", 1)).toBe("1000000"); // 8*1/10 floors to 0
    expect(stakeTarget("3", 5000)).toBe("1"); // 3*6000/10000 = 1.8 floors
    expect(stakeTarget("not money", 5000)).toBeNull();
    expect(stakeTargetBps(0)).toBe(2000);
    expect(stakeTargetBps(10000)).toBe(10000);
  });

  it("min conviction = 7500 − a/4, max open = 1 + 4a/10000", () => {
    expect([0, 3000, 7000, 10000].map(minConvictionBps)).toEqual([7500, 6750, 5750, 5000]);
    expect(minConvictionBps(3)).toBe(7500);
    expect(minConvictionBps(4)).toBe(7499);
    expect([0, 2499, 2500, 3000, 7000, 7500, 10000].map(maxOpenInteractions)).toEqual([1, 1, 2, 2, 3, 4, 5]);
  });

  it("collaboration thresholds at 3000 and 7000", () => {
    expect(stanceEffects(0, "none")).toEqual({ initiatesChallenges: true, joinsFirms: false, canLead: false, roleInactive: false });
    expect(stanceEffects(2999, "member")).toMatchObject({ joinsFirms: false, roleInactive: true, initiatesChallenges: true });
    expect(stanceEffects(3000, "member")).toMatchObject({ joinsFirms: true, initiatesChallenges: true, canLead: false });
    expect(stanceEffects(7000, "lead")).toMatchObject({ canLead: true, initiatesChallenges: true, joinsFirms: false });
    expect(stanceEffects(7001, "member").initiatesChallenges).toBe(false);
    expect(stanceEffects(10000, "lead")).toEqual({ initiatesChallenges: false, joinsFirms: false, canLead: true, roleInactive: false });
    expect(stanceEffects(0, "lead")).toMatchObject({ canLead: false, roleInactive: true });
    expect([0, 2999, 3000, 7000, 7001, 10000].map(stanceZone)).toEqual(["competitor", "competitor", "both", "both", "collaborator", "collaborator"]);
  });

  it("clamps out-of-range inputs", () => {
    expect(minConvictionBps(20000)).toBe(5000);
    expect(maxOpenInteractions(-5)).toBe(1);
  });
});

describe("stance wheel mapping", () => {
  const R = 100;
  it("maps angle to collaboration (left 0, right 10000) and radius to ambition", () => {
    const left = stanceToPoint(0, 10000, R);
    expect(left.x).toBeCloseTo(-100);
    expect(left.y).toBeCloseTo(0);
    const right = stanceToPoint(10000, 10000, R);
    expect(right.x).toBeCloseTo(100);
    const top = stanceToPoint(5000, 5000, R);
    expect(top.x).toBeCloseTo(0);
    expect(top.y).toBeCloseTo(50);
    const centre = stanceToPoint(3000, 0, R);
    expect(Math.abs(centre.x) + Math.abs(centre.y)).toBe(0);
  });

  it("round-trips the boundaries 0/3000/7000/10000", () => {
    for (const c of [0, 3000, 7000, 10000]) {
      for (const a of [0 + 1000, 3000, 7000, 10000]) {
        const p = stanceToPoint(c, a, R);
        expect(pointToStance(p.x, p.y, R, 5000)).toEqual({ collaboration_bps: c, ambition_bps: a });
      }
    }
  });

  it("clamps below the baseline and outside the rim; keeps angle at the centre", () => {
    expect(pointToStance(-80, -30, R, 5000)).toEqual({ collaboration_bps: 0, ambition_bps: 8000 });
    expect(pointToStance(500, 500, R, 5000)).toEqual({ collaboration_bps: 7500, ambition_bps: 10000 });
    expect(pointToStance(0, 0, R, 4200)).toEqual({ collaboration_bps: 4200, ambition_bps: 0 });
  });

  it("keyboard nudges", () => {
    const s = { collaboration_bps: 3000, ambition_bps: 9800 };
    expect(nudgeStance(s, "ArrowLeft")).toEqual({ collaboration_bps: 2500, ambition_bps: 9800 });
    expect(nudgeStance(s, "ArrowRight", true)).toEqual({ collaboration_bps: 3100, ambition_bps: 9800 });
    expect(nudgeStance(s, "ArrowUp")).toEqual({ collaboration_bps: 3000, ambition_bps: 10000 });
    expect(nudgeStance({ collaboration_bps: 200, ambition_bps: 0 }, "ArrowLeft")).toEqual({ collaboration_bps: 0, ambition_bps: 0 });
    expect(nudgeStance(s, "End")).toEqual({ collaboration_bps: 10000, ambition_bps: 9800 });
    expect(nudgeStance(s, "a")).toBeNull();
  });
});

describe("market filters", () => {
  it("derives scope from the filters", () => {
    const m = defaultMarkets();
    expect(deriveScope(m)).toBe("any");
    expect(deriveScope({ ...m, structures: ["binary_up_down"] })).toBe("structure");
    expect(deriveScope({ ...m, ranges: { ...m.ranges, move_bps: { min: 300, max: null } } })).toBe("structure");
    expect(deriveScope({ ...m, subcategories: ["energy"] })).toBe("specific");
    expect(deriveScope({ ...m, keywords: ["oil"], structures: ["categorical"] })).toBe("specific");
  });

  it("serialises to the exact schema: trimmed, de-duplicated, scope derived", () => {
    const s = serializeMarkets({
      ...defaultMarkets(),
      scope: "any",
      categories: [" Commodities", "commodities", ""],
      keywords: ["Energy", "energy ", "OIL"],
      symbols: ["btc", "BTC"],
    });
    expect(s).toEqual({
      scope: "specific",
      structures: [],
      categories: ["Commodities"],
      subcategories: [],
      keywords: ["energy", "oil"],
      symbols: ["BTC"],
      ranges: {
        move_bps: { min: null, max: null },
        yes_price_bps: { min: null, max: null },
        expiry_secs: { min: null, max: null },
        volume_delta_24h_bps: { min: null, max: null },
      },
    });
    expect(Object.keys(s)).toEqual(["scope", "structures", "categories", "subcategories", "keywords", "symbols", "ranges"]);
    expect(JSON.parse(JSON.stringify(s))).toEqual(s); // nulls survive JSON
  });

  it("toggles case-insensitively", () => {
    expect(toggleIn(["Crypto"], "crypto")).toEqual([]);
    expect(toggleIn(["Crypto"], "Energy")).toEqual(["Crypto", "Energy"]);
  });

  it("summarises", () => {
    expect(marketsSummary(defaultMarkets())).toBe("Any market");
    const m = { ...defaultMarkets(), subcategories: ["Energy"], ranges: { ...defaultMarkets().ranges, move_bps: { min: 300, max: null } } };
    expect(marketsSummary(m)).toBe("sub: Energy; Move (delta) ≥ 3%");
  });

  it("converts display units exactly", () => {
    expect(percentTextToBps("3.5")).toBe(350);
    expect(percentTextToBps("-1.25")).toBe(-125);
    expect(percentTextToBps("0.07")).toBe(7);
    expect(percentTextToBps("1.234")).toBeNull();
    expect(percentTextToBps("abc")).toBeNull();
    expect(bpsToPercentText(350)).toBe("3.5");
    expect(bpsToPercentText(-125)).toBe("-1.25");
    expect(bpsToPercentText(7)).toBe("0.07");
    expect(bpsToPercentText(10000)).toBe("100");
    expect(hoursTextToSecs("1.5")).toBe(5400);
    expect(hoursTextToSecs("24")).toBe(86400);
    expect(hoursTextToSecs("-1")).toBeNull();
    expect(secsToHoursText(5400)).toBe("1.5");
    expect(secsToHoursText(86400)).toBe("24");
  });

  it("validates ranges locally", () => {
    const r = defaultMarkets().ranges;
    expect(rangeErrors(r)).toEqual({});
    expect(rangeErrors({ ...r, move_bps: { min: 500, max: 100 } })["ranges.move_bps"]).toMatch(/min must not exceed max/);
    expect(rangeErrors({ ...r, yes_price_bps: { min: null, max: 10001 } })["ranges.yes_price_bps"]).toMatch(/at most 100%/);
    expect(rangeErrors({ ...r, expiry_secs: { min: -1, max: null } })["ranges.expiry_secs"]).toMatch(/at least/);
    expect(rangeErrors({ ...r, volume_delta_24h_bps: { min: -5000, max: 20000 } })).toEqual({});
  });
});

describe("approach params and graphs", () => {
  const base: Graph = { schema_version: 1, nodes: [{ id: "n1", type: "trigger.schedule", params: {}, ui: { x: 0, y: 40 } }], edges: [] };

  it("defaults when the graph has no approach node, and adds one with exact params", () => {
    expect(approachOf(base)).toEqual(defaultApproach());
    const g = ensureApproachNode(base);
    expect(g.nodes).toHaveLength(2);
    expect(g.nodes[0]).toMatchObject({ id: "approach", type: APPROACH_BLOCK_TYPE });
    expect(g.nodes[0].params).toEqual({
      stance: { collaboration_bps: 3000, ambition_bps: 5000 },
      firm_role: "none",
      markets: approachToParams(defaultApproach()).markets,
    });
    expect(ensureApproachNode(g)).toBe(g); // at most one
    expect(g.edges).toEqual([]);
  });

  it("picks a free id when 'approach' is taken", () => {
    const g = ensureApproachNode({ ...base, nodes: [...base.nodes, { id: "approach", type: "trigger.schedule", params: {} }] });
    expect(g.nodes[0].id).toBe("approach_2");
  });

  it("normalises loose params", () => {
    const p = normalizeApproach({ stance: { collaboration_bps: 12000, ambition_bps: "x" }, firm_role: "boss", markets: { keywords: ["oil", 3], ranges: { move_bps: { min: 300 } } } });
    expect(p.stance).toEqual({ collaboration_bps: 10000, ambition_bps: 5000 });
    expect(p.firm_role).toBe("none");
    expect(p.markets.keywords).toEqual(["oil"]);
    expect(p.markets.ranges.move_bps).toEqual({ min: 300, max: null });
  });
});
