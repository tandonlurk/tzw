import { describe, expect, it } from "vitest";
import type { Agent } from "./api";
import { families, paramLabel, paramValue, parseCount, splitBudget } from "./multiply";

describe("parseCount", () => {
  it("accepts 1..100 integers only", () => {
    expect(parseCount("1")).toBe(1);
    expect(parseCount(" 100 ")).toBe(100);
    expect(parseCount("0")).toBeNull();
    expect(parseCount("101")).toBeNull();
    expect(parseCount("2.5")).toBeNull();
    expect(parseCount("")).toBeNull();
  });
});

describe("splitBudget", () => {
  it("splits evenly, rounds down and keeps the remainder (same as the server)", () => {
    expect(splitBudget("100", 3, "1000")).toEqual({ perClone: "33", total: "99", remainder: "1", after: "901", fundsOk: true });
    expect(splitBudget("55000000", 10, null)).toEqual({ perClone: "5500000", total: "55000000", remainder: "0", after: null, fundsOk: null });
  });
  it("flags a shortfall", () => {
    const p = splitBudget("20000010", 10, "20000000")!;
    expect(p.total).toBe("20000010");
    expect(p.fundsOk).toBe(false);
    expect(p.after).toBe("-10");
  });
  it("refuses budgets too small for every clone", () => {
    expect(splitBudget("9", 10, "100")).toBeNull();
    expect(splitBudget("0", 1, "100")).toBeNull();
    expect(splitBudget("x", 1, "100")).toBeNull();
    expect(splitBudget("100", 0, "100")).toBeNull();
  });
});

describe("param labels", () => {
  it("names and formats varied parameters", () => {
    expect(paramLabel("graph.nodes.trigger.params.threshold_bps")).toBe("Move threshold");
    expect(paramLabel("graph.nodes.approach.params.stance.ambition_bps")).toBe("Ambition");
    expect(paramLabel("x.unknown")).toBe("unknown");
    expect(paramValue("policy.min_conviction_bps", 6214)).toBe("62.14%");
    expect(paramValue("x.threshold_bps", 200)).toBe("2%");
    expect(paramValue("config.horizon_secs", 3600)).toBe("1h");
    expect(paramValue("config.horizon_secs", 3459)).toBe("58m");
    expect(paramValue("x.every_secs", 45)).toBe("45s");
  });
});

function agent(id: string, family: string | null, pnl: string, index?: number): Agent {
  return {
    id, name: id, description: "", status: "active", env: "simulated", wallet_address: null, version: 1,
    config: {} as Agent["config"], policy: {} as Agent["policy"], graph: { schema_version: 1, nodes: [], edges: [] },
    balances: { available: "1000000", committed: "500000" },
    stats: { realized_pnl: pnl, settled: 2, wins: 1 },
    family_id: family,
    clone: index === undefined ? null : { index, multiplication_id: "m", source_agent_id: "r", source_version: 1, variance_bps: 0, seed: 1, params: [] },
    created_at: "", updated_at: "",
  };
}

describe("families", () => {
  it("groups root and clones and sums their own numbers", () => {
    const list = [agent("r", "r", "2500000"), agent("c2", "r", "-1000000", 2), agent("c1", "r", "-500000", 1), agent("solo", null, "7")];
    const fs = families(list);
    expect(fs).toHaveLength(1);
    expect(fs[0].root?.id).toBe("r");
    expect(fs[0].clones.map((a) => a.id)).toEqual(["c1", "c2"]);
    expect(fs[0].realizedPnl).toBe("1000000");
    expect(fs[0].available).toBe("3000000");
    expect(fs[0].committed).toBe("1500000");
    expect(fs[0].settled).toBe(6);
    expect(fs[0].wins).toBe(3);
  });
  it("ignores agents from older backends without family_id", () => {
    const a = agent("x", null, "0");
    delete a.family_id;
    expect(families([a])).toEqual([]);
  });
});
