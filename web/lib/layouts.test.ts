import { describe, expect, it } from "vitest";
import { PRESETS, addPanel, movePanel, parsePanels, removePanel, updatePanel, type Panel } from "./layouts";

const base: Panel[] = [
  { id: "a", metric: "pnl", w: 4, h: 2 },
  { id: "b", metric: "outcomes", w: 1, h: 1 },
  { id: "c", metric: "allocation", w: 1, h: 1 },
];

describe("layout edits", () => {
  it("adds, removes and resizes with clamping", () => {
    const added = addPanel(base, "record");
    expect(added).toHaveLength(4);
    expect(added[3]).toMatchObject({ metric: "record", w: 2, h: 1 });
    expect(removePanel(base, "b").map((p) => p.id)).toEqual(["a", "c"]);
    expect(updatePanel(base, "b", { w: 9, h: 0 })[1]).toMatchObject({ w: 4, h: 1 });
  });
  it("reorders by drop target", () => {
    expect(movePanel(base, "c", "a").map((p) => p.id)).toEqual(["c", "a", "b"]);
    expect(movePanel(base, "a", "c").map((p) => p.id)).toEqual(["b", "c", "a"]);
    expect(movePanel(base, "a", "zz")).toBe(base);
  });
  it("adds agent slot defaults for single-agent panels", () => {
    expect(addPanel([], "agent_pnl")[0]).toMatchObject({ agentId: null, slot: 0 });
  });
});

describe("parsePanels", () => {
  it("drops malformed entries and unknown metrics", () => {
    expect(parsePanels([{ id: "x", metric: "pnl", w: 2, h: 1 }, { id: 3 }, { id: "y", metric: "nope" }, null])).toEqual([
      { id: "x", metric: "pnl", w: 2, h: 1, agentId: null, slot: undefined },
    ]);
    expect(parsePanels("junk")).toBe(null);
  });
  it("accepts every preset", () => {
    for (const l of PRESETS) expect(parsePanels(JSON.parse(JSON.stringify(l.panels)))).toHaveLength(l.panels.length);
  });
});
