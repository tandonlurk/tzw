import { describe, expect, it } from "vitest";
import type { BlockDef, Graph } from "./api";
import { autoLayout, canConnect, canvasToGraph, defaultParams, graphToCanvas, nextNodeId } from "./graph";

const graph: Graph = {
  schema_version: 1,
  nodes: [
    { id: "n1", type: "trigger.schedule", params: { every_secs: 60 } },
    { id: "n2", type: "reason.evaluate_opportunity", params: {} },
    { id: "n3", type: "action.propose_challenge", params: { a: 1 } },
  ],
  edges: [
    { from: "n1", from_port: "out", to: "n2", to_port: "in" },
    { from: "n2", from_port: "out", to: "n3", to_port: "in" },
  ],
};

describe("graph conversion", () => {
  it("round-trips contract graph through canvas form, adding ui positions", () => {
    const c = graphToCanvas(graph, {});
    const back = canvasToGraph(c.nodes, c.edges);
    expect(back.edges).toEqual(graph.edges);
    expect(back.nodes.map(({ ui, ...n }) => n)).toEqual(graph.nodes);
    expect(back.nodes.every((n) => n.ui)).toBe(true);
  });
  it("prefers node ui, then stored positions, then auto layout", () => {
    const g = { ...graph, nodes: graph.nodes.map((n, i) => (i === 1 ? { ...n, ui: { x: 9, y: 8 } } : n)) };
    const c = graphToCanvas(g, { n1: { x: 5, y: 6 }, n2: { x: 1, y: 1 } });
    expect(c.nodes[0].position).toEqual({ x: 5, y: 6 });
    expect(c.nodes[1].position).toEqual({ x: 9, y: 8 });
    expect(c.nodes[2].position).toEqual({ x: 600, y: 0 });
  });
  it("serialized graph uses ui, not react-flow position", () => {
    const c = graphToCanvas(graph, { n1: { x: 5, y: 6 } });
    const s = JSON.stringify(canvasToGraph(c.nodes, c.edges));
    expect(s).not.toContain("position");
    expect(canvasToGraph(c.nodes, c.edges).nodes[0].ui).toEqual({ x: 5, y: 6 });
  });
});

describe("autoLayout", () => {
  it("terminates on cycles", () => {
    const pos = autoLayout(
      [{ id: "a", type: "x", params: {} }, { id: "b", type: "x", params: {} }],
      [{ from: "a", from_port: "o", to: "b", to_port: "i" }, { from: "b", from_port: "o", to: "a", to_port: "i" }],
    );
    expect(Object.keys(pos)).toHaveLength(2);
  });
});

describe("helpers", () => {
  const defs = new Map<string, BlockDef>([
    ["a", { type: "a", kind: "trigger", label: "A", description: "", inputs: [], outputs: [{ port: "out", data_type: "event" }], params_schema: { properties: { n: { type: "integer", default: 3 } } } }],
    ["b", { type: "b", kind: "action", label: "B", description: "", inputs: [{ port: "in", data_type: "event" }, { port: "x", data_type: "num" }], outputs: [], params_schema: {} }],
  ]);
  it("checks port data types", () => {
    expect(canConnect(defs, { type: "a", port: "out" }, { type: "b", port: "in" })).toBe(true);
    expect(canConnect(defs, { type: "a", port: "out" }, { type: "b", port: "x" })).toBe(false);
    expect(canConnect(defs, { type: "a", port: "nope" }, { type: "b", port: "in" })).toBe(false);
    expect(canConnect(defs, { type: "zzz", port: "out" }, { type: "b", port: "in" })).toBe(false);
  });
  it("defaults and ids", () => {
    expect(defaultParams(defs.get("a"))).toEqual({ n: 3 });
    expect(nextNodeId(["n1", "n3"])).toBe("n4");
  });
});
