"use client";

import "@xyflow/react/dist/style.css";
import { useCallback, useEffect, useMemo, useState } from "react";
import {
  Background,
  Controls,
  ReactFlow,
  ReactFlowProvider,
  addEdge,
  useEdgesState,
  useNodesState,
  useReactFlow,
  type Connection,
  type Edge,
} from "@xyflow/react";
import { CODE_BLOCK_TYPE, STARTER_SOURCE } from "@/lib/customCode";
import { api, isApiError, type BlockDef, type Graph, type GraphError, type Money } from "@/lib/api";
import { APPROACH_BLOCK_TYPE, approachNodeOf, approachToParams, defaultApproach, ensureApproachNode, normalizeApproach } from "@/lib/approach";
import { ApproachEditor } from "@/components/approach/ApproachEditor";
import { ApproachNode } from "./ApproachNode";
import {
  canConnect,
  canvasToGraph,
  defaultParams,
  graphToCanvas,
  loadPositions,
  nextNodeId,
  positionsOf,
  savePositions,
} from "@/lib/graph";
import { BuilderContext, BuilderNode, type FlowNode } from "./BuilderNode";

const nodeTypes = { block: BuilderNode, approach: ApproachNode };
const isApproach = (n: FlowNode) => n.data.blockType === APPROACH_BLOCK_TYPE;
const DRAG_MIME = "application/x-block-type";

interface Props {
  blocks: BlockDef[];
  /** Graph to load. Remount (change `key`) to load a different graph, e.g. a template. */
  initialGraph: Graph;
  /** localStorage key for UI-only node positions: the agent id, or "new". */
  storageId: string;
  onChange: (graph: Graph) => void;
  /** Errors from a rejected save (422), shown until the next Validate. */
  serverErrors?: GraphError[];
  /** Agent's max_per_interaction (minor units) for the approach editor's stake target; null if unknown. */
  maxPerInteraction?: Money | null;
}

export function GraphBuilder(props: Props) {
  return (
    <ReactFlowProvider>
      <Inner {...props} />
    </ReactFlowProvider>
  );
}

function Inner({ blocks, initialGraph, storageId, onChange, serverErrors, maxPerInteraction = null }: Props) {
  const blockMap = useMemo(() => new Map(blocks.map((b) => [b.type, b])), [blocks]);
  const rf = useReactFlow<FlowNode, Edge>();

  // Every graph gets exactly one pinned approach node; a graph without one shows defaults and gains it on save.
  const [init] = useState(() => ({
    ...graphToCanvas(ensureApproachNode(initialGraph), loadPositions(storageId)),
    addedApproach: !approachNodeOf(initialGraph),
  }));
  const [nodes, setNodes, onNodesChange] = useNodesState<FlowNode>(
    init.nodes.map((n) =>
      n.type === APPROACH_BLOCK_TYPE
        ? { id: n.id, type: "approach", position: n.position, deletable: false, connectable: false, data: { blockType: n.type, params: n.params } }
        : { id: n.id, type: "block", position: n.position, data: { blockType: n.type, params: n.params } },
    ),
  );
  const [panelOpen, setPanelOpen] = useState(true);
  const [edges, setEdges, onEdgesChange] = useEdgesState<Edge>(
    init.edges.map((e, i) => ({ id: `e${i}-${e.source}-${e.target}`, ...e })),
  );

  const [errors, setErrors] = useState<GraphError[]>([]);
  const [validation, setValidation] = useState<"idle" | "busy" | "valid" | "invalid" | "failed">("idle");
  const [validateMsg, setValidateMsg] = useState<string | null>(null);

  // Sync canvas -> contract Graph for the parent, and persist UI-only positions.
  useEffect(() => {
    const cn = nodes.map((n) => ({ id: n.id, type: n.data.blockType, params: n.data.params, position: n.position }));
    const ce = edges.map((e) => ({
      source: e.source,
      sourceHandle: e.sourceHandle ?? "",
      target: e.target,
      targetHandle: e.targetHandle ?? "",
    }));
    savePositions(storageId, positionsOf(cn));
    onChange(canvasToGraph(cn, ce));
    // onChange is expected to be a stable setter
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [nodes, edges, storageId]);

  const setParams = useCallback(
    (id: string, params: Record<string, unknown>) =>
      setNodes((ns) => ns.map((n) => (n.id === id ? { ...n, data: { ...n.data, params } } : n))),
    [setNodes],
  );
  const remove = useCallback(
    (id: string) => {
      setNodes((ns) => ns.filter((n) => n.id !== id || isApproach(n)));
      setEdges((es) => es.filter((e) => e.source !== id && e.target !== id));
    },
    [setNodes, setEdges],
  );

  const addBlock = useCallback(
    (type: string, at?: { x: number; y: number }) => {
      setNodes((ns) => {
        const id = nextNodeId(ns.map((n) => n.id));
        const position = at ?? { x: 40 + (ns.length % 4) * 40, y: 40 + ns.length * 30 };
        return [...ns, { id, type: "block", position, data: { blockType: type, params: type === CODE_BLOCK_TYPE ? { ...defaultParams(blockMap.get(type)), source: STARTER_SOURCE } : defaultParams(blockMap.get(type)) } }];
      });
    },
    [setNodes, blockMap],
  );

  const isValidConnection = useCallback(
    (c: Connection | Edge) => {
      if (!c.source || !c.target || c.source === c.target || !c.sourceHandle || !c.targetHandle) return false;
      const s = rf.getNode(c.source);
      const t = rf.getNode(c.target);
      if (!s || !t) return false;
      const dup = rf.getEdges().some(
        (e) => e.source === c.source && e.sourceHandle === c.sourceHandle && e.target === c.target && e.targetHandle === c.targetHandle,
      );
      if (dup) return false;
      return canConnect(
        blockMap,
        { type: (s.data as FlowNode["data"]).blockType, port: c.sourceHandle },
        { type: (t.data as FlowNode["data"]).blockType, port: c.targetHandle },
      );
    },
    [rf, blockMap],
  );

  const onConnect = useCallback((c: Connection) => setEdges((es) => addEdge(c, es)), [setEdges]);

  const onDrop = useCallback(
    (ev: React.DragEvent) => {
      ev.preventDefault();
      const type = ev.dataTransfer.getData(DRAG_MIME);
      if (!type) return;
      addBlock(type, rf.screenToFlowPosition({ x: ev.clientX, y: ev.clientY }));
    },
    [addBlock, rf],
  );

  async function validate() {
    setValidation("busy");
    setValidateMsg(null);
    const graph = canvasToGraph(
      nodes.map((n) => ({ id: n.id, type: n.data.blockType, params: n.data.params, position: n.position })),
      edges.map((e) => ({ source: e.source, sourceHandle: e.sourceHandle ?? "", target: e.target, targetHandle: e.targetHandle ?? "" })),
    );
    try {
      const r = await api.validateGraph(graph);
      setErrors(r.errors);
      setValidation(r.valid ? "valid" : "invalid");
    } catch (e) {
      setErrors([]);
      setValidation("failed");
      setValidateMsg(isApiError(e) ? (e.unreachable ? "Backend unreachable" : `${e.code}: ${e.message}`) : "Validation failed");
    }
  }

  const shownErrors = useMemo(() => (validation === "idle" ? (serverErrors ?? []) : errors), [validation, serverErrors, errors]);
  const errorsByNode = useMemo(() => {
    const m: Record<string, string[]> = {};
    for (const e of shownErrors) if (e.node_id) (m[e.node_id] ??= []).push(`${e.code}: ${e.message}`);
    return m;
  }, [shownErrors]);
  const graphErrors = shownErrors.filter((e) => !e.node_id);

  const openApproach = useCallback(() => setPanelOpen(true), []);
  const ctx = useMemo(
    () => ({ blocks: blockMap, errors: errorsByNode, setParams, remove, agentId: storageId === "new" ? null : storageId, openApproach }),
    [blockMap, errorsByNode, setParams, remove, storageId, openApproach],
  );
  const approachNode = nodes.find(isApproach);

  const kinds = ["trigger", "reasoning", "condition", "action"] as const;

  return (
    <BuilderContext.Provider value={ctx}>
      <div className={`builder${nodes.some((n) => n.data.blockType === CODE_BLOCK_TYPE) ? " has-code" : ""}`}>
        <div className="palette card">
          <div className="dim" style={{ fontSize: ".8rem" }}>Drag a block onto the canvas, or click to add.</div>
          {blocks.length === 0 && <div className="dim">The backend returned no blocks.</div>}
          {kinds.map((k) => {
            const list = blocks.filter((b) => b.kind === k);
            if (!list.length) return null;
            return (
              <div key={k}>
                <div className="kind">{k}</div>
                {list.map((b) => (
                  <div
                    key={b.type}
                    className="item"
                    draggable
                    title={b.description}
                    onDragStart={(e) => { e.dataTransfer.setData(DRAG_MIME, b.type); e.dataTransfer.effectAllowed = "move"; }}
                    onClick={() => addBlock(b.type)}
                  >
                    {b.label}
                  </div>
                ))}
              </div>
            );
          })}
        </div>
        <div className="canvas" onDragOver={(e) => { e.preventDefault(); e.dataTransfer.dropEffect = "move"; }} onDrop={onDrop}>
          <ReactFlow
            nodes={nodes}
            edges={edges}
            nodeTypes={nodeTypes}
            onNodesChange={onNodesChange}
            onEdgesChange={onEdgesChange}
            onConnect={onConnect}
            onNodeClick={(_, n) => { if (isApproach(n)) setPanelOpen(true); }}
            isValidConnection={isValidConnection}
            colorMode="dark"
            fitView
            deleteKeyCode={["Backspace", "Delete"]}
          >
            <Background />
            <Controls />
          </ReactFlow>
        </div>
      </div>
      {approachNode && (
        <div className="card approach-panel" style={{ marginTop: ".75rem" }}>
          <div className="row spread">
            <h3 style={{ margin: 0 }}>Approach <span className="badge dim">pinned block</span></h3>
            <span className="row">
              {panelOpen && (
                <button type="button" className="sm" onClick={() => { if (window.confirm("Reset the approach to the defaults?")) setParams(approachNode.id, approachToParams(defaultApproach())); }}>
                  Reset to defaults
                </button>
              )}
              <button type="button" className="sm" aria-expanded={panelOpen} onClick={() => setPanelOpen((o) => !o)}>{panelOpen ? "Hide" : "Edit approach"}</button>
            </span>
          </div>
          {init.addedApproach && <div className="notice info" style={{ margin: ".5rem 0" }}>This graph had no approach block, so the defaults below apply. Saving adds the block.</div>}
          {!blockMap.has(APPROACH_BLOCK_TYPE) && <div className="notice" style={{ margin: ".5rem 0" }}>The backend&apos;s block catalog does not list approach.profile yet; it may reject the save.</div>}
          {(errorsByNode[approachNode.id] ?? []).length > 0 && (
            <div className="banner-error" style={{ margin: ".5rem 0" }}><ul>{errorsByNode[approachNode.id].map((e, i) => <li key={i}>{e}</li>)}</ul></div>
          )}
          {panelOpen ? (
            <ApproachEditor
              value={normalizeApproach(approachNode.data.params)}
              onChange={(p) => setParams(approachNode.id, approachToParams(p))}
              maxPerInteraction={maxPerInteraction}
              agentId={storageId === "new" ? null : storageId}
            />
          ) : (
            <div className="dim small">Select the Approach block on the canvas or press Edit approach.</div>
          )}
        </div>
      )}
      <div className="row" style={{ marginTop: ".5rem" }}>
        <button type="button" onClick={validate} disabled={validation === "busy"}>{validation === "busy" ? "Validating..." : "Validate"}</button>
        {validation === "valid" && <span className="pos">Graph is valid.</span>}
        {validation === "invalid" && <span className="neg">{errors.length} problem(s) found. Offending nodes are outlined.</span>}
        {validation === "failed" && <span className="pend">{validateMsg}</span>}
      </div>
      {graphErrors.length > 0 && (
        <div className="banner-error" style={{ marginTop: ".5rem" }}>
          <ul>{graphErrors.map((e, i) => <li key={i}>{e.code}: {e.message}</li>)}</ul>
        </div>
      )}
    </BuilderContext.Provider>
  );
}
