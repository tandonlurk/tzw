import { describe, expect, it } from "vitest";
import { perAgentSeries, scaleMulti } from "./charts";

describe("perAgentSeries", () => {
  const ev = [
    { ts: "2026-01-01T00:00:00Z", agent_id: "a", agent_cumulative: "15000000" },
    { ts: "2026-01-01T00:00:00Z", agent_id: "b", agent_cumulative: "-15000000" },
    { ts: "2026-01-02T00:00:00Z", agent_id: "a", agent_cumulative: "10000000" },
  ];
  it("groups events by agent, keeps order, uses names", () => {
    const s = perAgentSeries(ev, [{ agent_id: "a", name: "Bull" }, { agent_id: "b", name: "Skeptic" }]);
    expect(s.map((x) => x.name)).toEqual(["Bull", "Skeptic"]);
    expect(s[0].points.map((p) => p.y)).toEqual([15000000, 10000000]);
    expect(s[0].finalMinor).toBe(10000000);
    expect(s[1].finalMinor).toBe(-15000000);
  });
  it("is empty with no events and never invents points", () => {
    expect(perAgentSeries([], [{ agent_id: "a", name: "A" }])).toEqual([]);
    expect(scaleMulti([], 100, 100)).toBeNull();
  });
  it("scales on a shared axis including zero", () => {
    const m = scaleMulti([[{ x: 0, y: 15 }], [{ x: 0, y: -15 }]], 100, 100)!;
    expect(m.yMax).toBe(15);
    expect(m.yMin).toBe(-15);
    expect(m.lines[0][0].py).toBeLessThan(m.lines[1][0].py);
  });
});
