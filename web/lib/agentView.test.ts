import { describe, expect, it } from "vitest";
import type { AgentVersion, Interaction } from "./api";
import { diffVersions, interactionCounts, versionHistory, winRateText } from "./agentView";

const base: AgentVersion = {
  version: 1,
  created_at: "2026-11-15T00:00:00Z",
  config: { goals: "g", strategy: "s", categories: ["crypto"], horizon_secs: 3600, risk_tolerance: "low", schedule_secs: 60 },
  policy: {
    max_per_interaction: "5000000",
    max_aggregate_exposure: "20000000",
    approval_threshold: "2000000",
    allowed_interaction_types: ["challenge"],
    min_conviction_bps: 5000,
    stop_loss: null,
  },
  graph: { schema_version: 1, nodes: [], edges: [] },
};

describe("agentView", () => {
  it("diffs config, policy and graph", () => {
    const next: AgentVersion = {
      ...base,
      version: 2,
      config: { ...base.config, strategy: "new", risk_tolerance: "high" },
      policy: { ...base.policy, max_per_interaction: "7500000", stop_loss: "1000000" },
      graph: { ...base.graph, nodes: [{ id: "a", type: "t", params: {} }] },
    };
    const d = diffVersions(base, next);
    expect(d).toContain("Strategy: s -> new");
    expect(d).toContain("Risk tolerance: low -> high");
    expect(d.some((x) => x.startsWith("Max per interaction: 5") && x.includes("7.5"))).toBe(true);
    expect(d.some((x) => x.startsWith("Stop loss: none -> "))).toBe(true);
    expect(d).toContain("Behavior graph changed");
    expect(diffVersions(base, base)).toEqual([]);
    expect(diffVersions(null, base)).toEqual([]);
  });

  it("orders versions newest first with diffs", () => {
    const v2 = { ...base, version: 2, config: { ...base.config, goals: "x" } };
    const h = versionHistory([base, v2]);
    expect(h.map((x) => x.v.version)).toEqual([2, 1]);
    expect(h[0].changes).toEqual(["Goals: g -> x"]);
    expect(h[1].changes).toEqual([]);
  });

  it("win rate and counts", () => {
    expect(winRateText(0, 0)).toBe("No settled interactions");
    expect(winRateText(1, 4)).toBe("1 of 4 won (25%)");
    const mk = (state: string) => ({ state }) as Interaction;
    expect(interactionCounts([mk("active"), mk("settled"), mk("cancelled"), mk("negotiating")])).toEqual({ active: 2, past: 2, total: 4 });
  });
});
