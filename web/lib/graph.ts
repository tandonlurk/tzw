// Pure helpers for the visual builder: contract Graph <-> canvas state. No React here.
// React Flow's own object is never persisted; only the contract's Graph is sent to the backend.
// Node positions travel in the contract's optional per-node `ui: {x, y}`; localStorage is only a fallback for graphs without it.

import type { BlockDef, Graph, GraphEdge, GraphNode } from "./api";

export interface Pos { x: number; y: number }
export type PositionMap = Record<string, Pos>;

export interface CanvasNode { id: string; type: string; params: Record<string, unknown>; position: Pos }
export interface CanvasEdge { source: string; sourceHandle: string; target: string; targetHandle: string }

export const emptyGraph = (): Graph => ({ schema_version: 1, nodes: [], edges: [] });

export function defaultParams(def: BlockDef | undefined): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  const props = def?.params_schema?.properties ?? {};
  for (const [k, s] of Object.entries(props)) {
    if (s.default !== undefined) out[k] = s.default;
  }
  return out;
}

export function nextNodeId(existing: string[]): string {
  const used = new Set(existing);
  let i = existing.length + 1;
  while (used.has(`n${i}`)) i++;
  return `n${i}`;
}

/** Layered layout: x by longest-path depth from sources, y by index within the layer. */
export function autoLayout(nodes: GraphNode[], edges: GraphEdge[]): PositionMap {
  const depth: Record<string, number> = {};
  nodes.forEach((n) => (depth[n.id] = 0));
  for (let pass = 0; pass < nodes.length; pass++) {
    let changed = false;
    for (const e of edges) {
      if (e.from in depth && e.to in depth && depth[e.to] < depth[e.from] + 1 && depth[e.from] + 1 < nodes.length) {
        depth[e.to] = depth[e.from] + 1;
        changed = true;
      }
    }
    if (!changed) break;
  }
  const rows: Record<number, number> = {};
  const out: PositionMap = {};
  for (const n of nodes) {
    const d = depth[n.id];
    const r = (rows[d] = (rows[d] ?? -1) + 1);
    out[n.id] = { x: d * 300, y: r * 280 };
  }
  return out;
}

export function graphToCanvas(graph: Graph, positions: PositionMap): { nodes: CanvasNode[]; edges: CanvasEdge[] } {
  const fallback = autoLayout(graph.nodes, graph.edges);
  return {
    nodes: graph.nodes.map((n) => ({
      id: n.id,
      type: n.type,
      params: { ...n.params },
      position: n.ui ? { x: n.ui.x, y: n.ui.y } : (positions[n.id] ?? fallback[n.id]),
    })),
    edges: graph.edges.map((e) => ({
      source: e.from,
      sourceHandle: e.from_port,
      target: e.to,
      targetHandle: e.to_port,
    })),
  };
}

export function canvasToGraph(nodes: CanvasNode[], edges: CanvasEdge[]): Graph {
  return {
    schema_version: 1,
    nodes: nodes.map((n) => ({ id: n.id, type: n.type, params: n.params, ui: { x: n.position.x, y: n.position.y } })),
    edges: edges.map((e) => ({ from: e.source, from_port: e.sourceHandle, to: e.target, to_port: e.targetHandle })),
  };
}

export function positionsOf(nodes: CanvasNode[]): PositionMap {
  const out: PositionMap = {};
  for (const n of nodes) out[n.id] = { x: n.position.x, y: n.position.y };
  return out;
}

/** Can an edge go from source node's output port to target node's input port? Ports must exist and share a data_type. */
export function canConnect(
  blocks: Map<string, BlockDef>,
  src: { type: string; port: string },
  dst: { type: string; port: string },
): boolean {
  const out = blocks.get(src.type)?.outputs.find((p) => p.port === src.port);
  const inp = blocks.get(dst.type)?.inputs.find((p) => p.port === dst.port);
  return !!out && !!inp && out.data_type === inp.data_type;
}

// localStorage persistence of positions, keyed by agent id ("new" while creating).
const key = (id: string) => `agent-graph-ui:${id}`;

export function loadPositions(id: string): PositionMap {
  if (typeof window === "undefined") return {};
  try {
    const raw = window.localStorage.getItem(key(id));
    return raw ? (JSON.parse(raw) as PositionMap) : {};
  } catch {
    return {};
  }
}

export function savePositions(id: string, map: PositionMap): void {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(key(id), JSON.stringify(map));
  } catch {
    /* storage unavailable: layout falls back to auto */
  }
}

export function clearPositions(id: string): void {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.removeItem(key(id));
  } catch {
    /* ignore */
  }
}
