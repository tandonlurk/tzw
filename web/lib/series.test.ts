import { describe, expect, it } from "vitest";
import type { Portfolio } from "./api";
import { compactUsdc, domainFor, niceTicks, portfolioSeries, stepVertices, timeTicks, valueAt } from "./series";

const pts = [{ t: 10, v: 5 }, { t: 20, v: -3 }, { t: 30, v: 7 }];

describe("valueAt", () => {
  it("holds the last value and uses `before` ahead of the first point", () => {
    expect(valueAt(pts, 5)).toBe(0);
    expect(valueAt(pts, 10)).toBe(5);
    expect(valueAt(pts, 25)).toBe(-3);
    expect(valueAt(pts, 99)).toBe(7);
  });
});

describe("stepVertices", () => {
  it("starts at the value at t0 and holds to t1", () => {
    expect(stepVertices(pts, 15, 25)).toEqual([
      { t: 15, v: 5 }, { t: 20, v: 5 }, { t: 20, v: -3 }, { t: 25, v: -3 },
    ]);
  });
});

describe("portfolioSeries", () => {
  it("groups events by agent and keeps the total", () => {
    const p = {
      agents: [{ agent_id: "a", name: "A" }, { agent_id: "b", name: "B" }],
      pnl_over_time: [
        { ts: "2026-01-01T00:00:00Z", agent_id: "a", agent_cumulative: "100", portfolio_cumulative: "100" },
        { ts: "2026-01-01T00:00:00Z", agent_id: "b", agent_cumulative: "-100", portfolio_cumulative: "0" },
        { ts: "2026-01-01T01:00:00Z", agent_id: "a", agent_cumulative: "300", portfolio_cumulative: "200" },
      ],
    } as unknown as Portfolio;
    const s = portfolioSeries(p);
    expect(s.agents.map((a) => [a.name, a.points.map((x) => x.v)])).toEqual([["A", [100, 300]], ["B", [-100]]]);
    expect(s.total.points.map((x) => x.v)).toEqual([0, 200]); // same-time points collapse to the last value
  });
});

describe("axes", () => {
  it("niceTicks covers the range with round steps", () => {
    const t = niceTicks(-2_500_000, 7_500_000, 4);
    expect(t[0]).toBeLessThanOrEqual(-2_500_000);
    expect(t[t.length - 1]).toBeGreaterThanOrEqual(7_500_000);
    expect(t).toContain(0);
  });
  it("timeTicks picks a step that gives a handful of ticks", () => {
    const { ticks } = timeTicks(0, 3_600_000, 6);
    expect(ticks.length).toBeGreaterThan(1);
    expect(ticks.length).toBeLessThanOrEqual(7);
  });
  it("domainFor ranges end at max(now, last)", () => {
    expect(domainFor([{ id: "x", name: "x", points: [{ t: 1000, v: 1 }] }], "1h", 500)).toEqual([1000 - 3_600_000, 1000]);
    expect(domainFor([], "all", 0)).toBe(null);
  });
  it("compactUsdc", () => {
    expect(compactUsdc(2_500_000)).toBe("2.5");
    expect(compactUsdc(-1_500_000_000)).toBe("-1.5k");
    expect(compactUsdc(0)).toBe("0.00");
  });
});
