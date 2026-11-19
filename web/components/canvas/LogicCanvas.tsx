"use client";

import "@xyflow/react/dist/style.css";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useSearchParams } from "next/navigation";
import {
  Background, BackgroundVariant, MarkerType, MiniMap, ReactFlow, ReactFlowProvider, addEdge,
  useEdgesState, useNodesState, useReactFlow,
  type Connection, type Edge, type FinalConnectionState,
} from "@xyflow/react";
import { api, isApiError, type Agent, type ApiError, type BlockDef, type Graph, type GraphError } from "@/lib/api";
import { canConnect, canvasToGraph, defaultParams, graphToCanvas, loadPositions, nextNodeId, positionsOf, savePositions } from "@/lib/graph";
import { APPROACH_BLOCK_TYPE, approachNodeOf, ensureApproachNode } from "@/lib/approach";
import { CODE_BLOCK_TYPE, STARTER_SOURCE } from "@/lib/customCode";
import { FAMILIES, familyOf, reachableFromTriggers, type Family } from "@/lib/blockVisuals";
import { serverGraphErrors } from "@/components/AgentForm";
import { ErrorBanner } from "@/components/ui";
import { Icon } from "@/components/shell/icons";
import { BlockMenu, DRAG_MIME } from "./BlockMenu";
import { NodePanel } from "./NodePanel";
import { CanvasContext, ShapeGlyph, ShapeNode, type CanvasFlowNode } from "./ShapeNode";

const nodeTypes = { shape: ShapeNode };
const EDGE_DEFAULTS = { type: "default", markerEnd: { type: MarkerType.ArrowClosed, width: 16, height: 16, color: "oklch(0.62 0 0)" } };

interface Props {
  agent: Agent;
  blocks: BlockDef[];
  /** Called after a successful save with the new agent. */
  onSaved: (a: Agent) => void;
  /** Link to the code workspace for a code node, or null to hide it. */
  codeHref?: (nodeId: string) => string;
}

export function LogicCanvas(props: Props) {
  return (
    <ReactFlowProvider>
      <Inner {...props} />
    </ReactFlowProvider>
  );
}

const toGraph = (nodes: CanvasFlowNode[], edges: Edge[]): Graph => canvasToGraph(
  nodes.map((n) => ({ id: n.id, type: n.data.blockType, params: n.data.params, position: { x: Math.round(n.position.x), y: Math.round(n.position.y) } })),
  edges.map((e) => ({ source: e.source, sourceHandle: e.sourceHandle ?? "", target: e.target, targetHandle: e.targetHandle ?? "" })),
);

function Inner({ agent, blocks, onSaved, codeHref }: Props) {
  const blockMap = useMemo(() => new Map(blocks.map((b) => [b.type, b])), [blocks]);
  const rf = useReactFlow<CanvasFlowNode, Edge>();
  const wrap = useRef<HTMLDivElement>(null);
  const focus = useSearchParams().get("node");

  const [init] = useState(() => {
    const g = ensureApproachNode(agent.graph);
    const c = graphToCanvas(g, loadPositions(agent.id));
    const nodes: CanvasFlowNode[] = c.nodes.map((n) => ({
      id: n.id, type: "shape", position: n.position, data: { blockType: n.type, params: n.params },
      ...(n.type === APPROACH_BLOCK_TYPE ? { deletable: false } : {}),
    }));
    const edges: Edge[] = c.edges.map((e, i) => ({ id: `e${i}-${e.source}-${e.sourceHandle}-${e.target}`, ...e, ...EDGE_DEFAULTS }));
    return { nodes, edges, addedApproach: !approachNodeOf(agent.graph), baseline: JSON.stringify(toGraph(nodes, edges)) };
  });
  const [nodes, setNodes, onNodesChange] = useNodesState<CanvasFlowNode>(init.nodes);
  const [edges, setEdges, onEdgesChange] = useEdgesState<Edge>(init.edges);
  const [baseline, setBaseline] = useState(init.addedApproach ? "" : init.baseline);
  const [selected, setSelected] = useState<string | null>(focus && init.nodes.some((n) => n.id === focus) ? focus : null);
  const [menu, setMenu] = useState<null | { at?: { x: number; y: number }; from?: { node: string; handle: string } }>(null);
  const [minimap, setMinimap] = useState(true);
  const [legend, setLegend] = useState(false);
  const [problems, setProblems] = useState(false);
  const [errors, setErrors] = useState<GraphError[]>([]);
  const [validation, setValidation] = useState<"idle" | "busy" | "valid" | "invalid" | "failed">("idle");
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<ApiError | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const [snap, setSnap] = useState({ nodes: init.nodes, edges: init.edges });

  const graph = useMemo(() => toGraph(nodes, edges), [nodes, edges]);
  const graphJson = useMemo(() => JSON.stringify(graph), [graph]);
  const dirty = graphJson !== baseline;
  const reachable = useMemo(() => reachableFromTriggers(nodes.map((n) => ({ id: n.id, type: n.data.blockType })), edges), [nodes, edges]);

  useEffect(() => { savePositions(agent.id, positionsOf(nodes.map((n) => ({ id: n.id, type: n.data.blockType, params: n.data.params, position: n.position })))); }, [nodes, agent.id]);

  // Live validation, debounced (Doherty: feedback without a button press).
  useEffect(() => {
    let alive = true;
    const t = window.setTimeout(async () => {
      setValidation("busy");
      try {
        const r = await api.validateGraph(JSON.parse(graphJson));
        if (!alive) return;
        setErrors(r.errors);
        setValidation(r.valid ? "valid" : "invalid");
      } catch {
        if (alive) setValidation("failed");
      }
    }, 500);
    return () => { alive = false; window.clearTimeout(t); };
  }, [graphJson]);

  useEffect(() => {
    if (!dirty) return;
    const warn = (e: BeforeUnloadEvent) => { e.preventDefault(); };
    window.addEventListener("beforeunload", warn);
    return () => window.removeEventListener("beforeunload", warn);
  }, [dirty]);

  useEffect(() => { if (!toast) return; const t = window.setTimeout(() => setToast(null), 2600); return () => window.clearTimeout(t); }, [toast]);

  const shownErrors = useMemo(() => [...errors, ...serverGraphErrors(saveError)], [errors, saveError]);
  const errorsByNode = useMemo(() => {
    const m: Record<string, string[]> = {};
    for (const e of shownErrors) if (e.node_id) (m[e.node_id] ??= []).push(e.message || e.code);
    return m;
  }, [shownErrors]);

  const setParams = useCallback((id: string, params: Record<string, unknown>) =>
    setNodes((ns) => ns.map((n) => (n.id === id ? { ...n, data: { ...n.data, params } } : n))), [setNodes]);

  const viewportCenter = useCallback(() => {
    const r = wrap.current?.getBoundingClientRect();
    return r ? rf.screenToFlowPosition({ x: r.left + r.width / 2 - 32, y: r.top + r.height / 2 - 32 }) : { x: 0, y: 0 };
  }, [rf]);

  const addBlock = useCallback((type: string, at?: { x: number; y: number }, from?: { node: string; handle: string }) => {
    const def = blockMap.get(type);
    const id = nextNodeId(rf.getNodes().map((n) => n.id));
    // Nudge a menu-added block off any block already at the drop point.
    const position = { ...(at ?? viewportCenter()) };
    const taken = rf.getNodes();
    for (let k = 0; k < 12 && taken.some((n) => Math.abs(n.position.x - position.x) < 90 && Math.abs(n.position.y - position.y) < 110); k++) position.y += 121;
    const params = type === CODE_BLOCK_TYPE ? { ...defaultParams(def), source: STARTER_SOURCE } : defaultParams(def);
    setNodes((ns) => [...ns.map((n) => ({ ...n, selected: false })), { id, type: "shape", position, data: { blockType: type, params }, selected: true }]);
    if (from && def) {
      const src = rf.getNode(from.node);
      const port = def.inputs.find((p) => src && canConnect(blockMap, { type: src.data.blockType, port: from.handle }, { type, port: p.port }));
      if (port) setEdges((es) => addEdge({ source: from.node, sourceHandle: from.handle, target: id, targetHandle: port.port, ...EDGE_DEFAULTS, className: "fresh" }, es));
    }
    setSelected(id);
    setMenu(null);
  }, [blockMap, rf, setNodes, setEdges, viewportCenter]);

  const duplicate = useCallback((id: string) => {
    const n = rf.getNode(id);
    if (!n || n.data.blockType === APPROACH_BLOCK_TYPE) return;
    const nid = nextNodeId(rf.getNodes().map((x) => x.id));
    setNodes((ns) => [...ns.map((x) => ({ ...x, selected: false })), { id: nid, type: "shape", position: { x: n.position.x + 40, y: n.position.y + 120 }, data: { blockType: n.data.blockType, params: structuredClone(n.data.params) }, selected: true }]);
    setSelected(nid);
  }, [rf, setNodes]);

  const remove = useCallback((id: string) => {
    setNodes((ns) => ns.filter((n) => n.id !== id || n.data.blockType === APPROACH_BLOCK_TYPE));
    setEdges((es) => es.filter((e) => e.source !== id && e.target !== id));
    setSelected(null);
  }, [setNodes, setEdges]);

  const isValidConnection = useCallback((c: Connection | Edge) => {
    if (!c.source || !c.target || c.source === c.target || !c.sourceHandle || !c.targetHandle) return false;
    const s = rf.getNode(c.source);
    const t = rf.getNode(c.target);
    if (!s || !t) return false;
    if (rf.getEdges().some((e) => e.source === c.source && e.sourceHandle === c.sourceHandle && e.target === c.target && e.targetHandle === c.targetHandle)) return false;
    return canConnect(blockMap, { type: s.data.blockType, port: c.sourceHandle }, { type: t.data.blockType, port: c.targetHandle });
  }, [rf, blockMap]);

  const onConnect = useCallback((c: Connection) => setEdges((es) => addEdge({ ...c, ...EDGE_DEFAULTS, className: "fresh" }, es)), [setEdges]);

  // Dropping a connection on empty canvas opens the block menu, filtered to compatible blocks (n8n-style).
  const onConnectEnd = useCallback((ev: MouseEvent | TouchEvent, state: FinalConnectionState) => {
    if (state.isValid || !state.fromNode || !state.fromHandle || state.fromHandle.type !== "source") return;
    const target = ev.target as Element | null;
    if (!target?.classList.contains("react-flow__pane")) return;
    const pt = "changedTouches" in ev ? ev.changedTouches[0] : ev;
    setMenu({ at: rf.screenToFlowPosition({ x: pt.clientX, y: pt.clientY - 32 }), from: { node: state.fromNode.id, handle: state.fromHandle.id ?? "" } });
  }, [rf]);

  const menuFilter = useMemo(() => {
    if (!menu?.from) return undefined;
    const src = rf.getNode(menu.from.node);
    const handle = menu.from.handle;
    return (b: BlockDef) => !!src && b.inputs.some((p) => canConnect(blockMap, { type: src.data.blockType, port: handle }, { type: b.type, port: p.port }));
  }, [menu, rf, blockMap]);

  async function save() {
    setSaving(true);
    setSaveError(null);
    try {
      const updated = await api.updateAgent(agent.id, { name: agent.name, description: agent.description, config: agent.config, policy: agent.policy, graph });
      setBaseline(graphJson);
      setSnap({ nodes, edges });
      setToast(`Saved as version ${updated.version}`);
      onSaved(updated);
    } catch (e) {
      if (isApiError(e)) { setSaveError(e); setProblems(true); }
    } finally {
      setSaving(false);
    }
  }

  function discard() {
    setNodes(snap.nodes);
    setEdges(snap.edges);
    setSelected(null);
    setSaveError(null);
  }

  // Keyboard: / opens the block menu, ⌘D duplicates, ⌘S saves, Esc closes panels.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const el = e.target as HTMLElement;
      const typing = el.closest("input, textarea, select, [contenteditable=true]");
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "s") { e.preventDefault(); if (dirty && !saving) save(); return; }
      if (typing) return;
      if (e.key === "/") { e.preventDefault(); setMenu({}); }
      else if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "d" && selected) { e.preventDefault(); duplicate(selected); }
      else if (e.key === "Escape") { if (menu) setMenu(null); else setSelected(null); }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  const selNode = selected ? nodes.find((n) => n.id === selected) : undefined;
  const ctx = useMemo(() => ({ blocks: blockMap, errors: errorsByNode, reachable }), [blockMap, errorsByNode, reachable]);
  const usedFamilies = useMemo(() => [...new Set(nodes.map((n) => familyOf(n.data.blockType, blockMap.get(n.data.blockType)?.kind)))] as Family[], [nodes, blockMap]);
  const triggers = nodes.filter((n) => familyOf(n.data.blockType) === "trigger");
  const problemCount = shownErrors.length;
  const orphanCount = nodes.filter((n) => { const f = familyOf(n.data.blockType); return f !== "trigger" && f !== "data" && !reachable.has(n.id); }).length;

  return (
    <CanvasContext.Provider value={ctx}>
      <div className="lc">
        <div className="lc-main" ref={wrap}
          onDragOver={(e) => { e.preventDefault(); e.dataTransfer.dropEffect = "move"; }}
          onDrop={(e) => { e.preventDefault(); const t = e.dataTransfer.getData(DRAG_MIME); if (t) addBlock(t, rf.screenToFlowPosition({ x: e.clientX - 32, y: e.clientY - 32 })); }}>
          <ReactFlow
            nodes={nodes}
            edges={edges}
            nodeTypes={nodeTypes}
            onNodesChange={onNodesChange}
            onEdgesChange={onEdgesChange}
            onConnect={onConnect}
            onConnectEnd={onConnectEnd}
            isValidConnection={isValidConnection}
            onNodeClick={(_, n) => setSelected(n.id)}
            onPaneClick={() => { setSelected(null); setMenu(null); }}
            onNodesDelete={(ns) => { if (ns.some((n) => n.id === selected)) setSelected(null); }}
            defaultEdgeOptions={EDGE_DEFAULTS}
            colorMode="dark"
            fitView
            fitViewOptions={{ padding: 0.25, maxZoom: 1.1 }}
            minZoom={0.2}
            maxZoom={2}
            snapToGrid
            snapGrid={[11, 11]}
            deleteKeyCode={["Backspace", "Delete"]}
            proOptions={{ hideAttribution: true }}
          >
            <Background variant={BackgroundVariant.Dots} gap={22} size={1.3} color="oklch(1 0 0 / 0.16)" />
            {minimap && (
              <MiniMap pannable zoomable className="lc-minimap" maskColor="oklch(0.205 0 0 / 0.7)" bgColor="oklch(0.231 0 0)"
                nodeColor={(n) => FAMILIES[familyOf((n.data as CanvasFlowNode["data"]).blockType)].color} nodeBorderRadius={6} nodeStrokeWidth={0} />
            )}
          </ReactFlow>

          <div className="lc-bar lc-tl">
            <div className="lc-ctx"><span className="dim">Logic for</span> <strong>{agent.name.replace(/\s*\((simulated|arc[_ ]testnet)\)$/i, "")}</strong></div>
            <button type="button" className="sm primary" onClick={() => setMenu({})} aria-keyshortcuts="/"><Icon name="plus" size={14} />Add block<kbd style={{ marginLeft: 4, color: "inherit", borderColor: "currentColor", opacity: .6 }}>/</kbd></button>
          </div>

          <div className="lc-bar lc-tr">
            {dirty ? <span className="lc-dirty" title={init.addedApproach ? "This logic had no Approach block; saving adds one with defaults." : undefined}><span className="d" />{init.addedApproach && baseline === "" ? "Approach block added" : "Unsaved changes"}</span> : <span className="dim xs">Saved · version {agent.version}</span>}
            {dirty && <button type="button" className="sm ghost" onClick={discard}>Discard</button>}
            <button type="button" className="sm primary" disabled={!dirty || saving} onClick={save} aria-keyshortcuts="Meta+S">
              <Icon name="save" size={14} />{saving ? "Saving..." : `Save as v${agent.version + 1}`}
            </button>
          </div>

          <div className="lc-bar lc-bl">
            <button type="button" className={`lc-pill ${validation}`} onClick={() => setProblems((p) => !p)} aria-expanded={problems}>
              {validation === "busy" ? <><span className="spin" />Checking</> : problemCount > 0 ? <><Icon name="alert" size={13} />{problemCount} problem{problemCount === 1 ? "" : "s"}</>
                : validation === "failed" ? <><Icon name="alert" size={13} />Can&apos;t validate</> : <><Icon name="check" size={13} />Valid</>}
            </button>
            <span className="dim xs">{nodes.length} blocks · {edges.length} links · starts at {triggers.length ? triggers.map((t) => blockMap.get(t.data.blockType)?.label ?? t.id).join(", ") : <span className="pend">no trigger</span>}{orphanCount > 0 && <span className="pend"> · {orphanCount} unconnected</span>}</span>
          </div>

          <div className="lc-bar lc-br">
            <button type="button" className={`icon-btn sm ghost${legend ? " on" : ""}`} onClick={() => setLegend((l) => !l)} aria-pressed={legend} data-tip="Shapes legend"><Icon name="layout" size={15} /></button>
            <button type="button" className={`icon-btn sm ghost${minimap ? " on" : ""}`} onClick={() => setMinimap((m) => !m)} aria-pressed={minimap} data-tip="Minimap"><Icon name="minimap" size={15} /></button>
            <span className="sep" />
            <button type="button" className="icon-btn sm ghost" onClick={() => rf.zoomOut({ duration: 200 })} aria-label="Zoom out">−</button>
            <button type="button" className="icon-btn sm ghost" onClick={() => rf.zoomIn({ duration: 200 })} aria-label="Zoom in">+</button>
            <button type="button" className="icon-btn sm ghost" onClick={() => rf.fitView({ padding: 0.25, duration: 300, maxZoom: 1.1 })} data-tip="Fit to view"><Icon name="fit" size={15} /></button>
          </div>

          {legend && (
            <div className="lc-legend">
              <div className="eyebrow" style={{ marginBottom: 6 }}>Block shapes</div>
              {(Object.keys(FAMILIES) as Family[]).map((f) => (
                <div key={f} className={`lg-row${usedFamilies.includes(f) ? "" : " unused"}`}><ShapeGlyph family={f} size={20} /><span>{FAMILIES[f].label}</span><span className="dim">{FAMILIES[f].blurb}</span></div>
              ))}
            </div>
          )}

          {problems && (
            <div className="lc-problems">
              <div className="row spread" style={{ marginBottom: 6 }}><span className="eyebrow">Problems</span><button type="button" className="icon-btn xs ghost" onClick={() => setProblems(false)} aria-label="Close"><Icon name="close" size={13} /></button></div>
              <ErrorBanner error={saveError && serverGraphErrors(saveError).length === 0 ? saveError : null} />
              {problemCount === 0 && orphanCount === 0 && <div className="dim small">No problems. The server accepts this logic.</div>}
              {shownErrors.map((e, i) => (
                <button key={i} type="button" className="lc-prob" onClick={() => { if (e.node_id) { setSelected(e.node_id); const n = rf.getNode(e.node_id); if (n) rf.setCenter(n.position.x + 32, n.position.y + 32, { zoom: 1.1, duration: 300 }); } }}>
                  <Icon name="alert" size={13} /><span className="grow">{e.node_id ? <strong>{e.node_id}: </strong> : null}{e.message || e.code}</span><span className="mono dim xs">{e.code}</span>
                </button>
              ))}
              {orphanCount > 0 && <div className="dim small" style={{ marginTop: 6 }}>{orphanCount} block{orphanCount === 1 ? " is" : "s are"} not reachable from a trigger and will never run.</div>}
            </div>
          )}

          {menu && (
            <div className="lc-menu-wrap" onClick={(e) => { if (e.target === e.currentTarget) setMenu(null); }}>
              <BlockMenu blocks={blocks} filter={menuFilter} title={menu.from ? "Connect to…" : "Add a block"}
                onPick={(t) => addBlock(t, menu.at, menu.from)} onClose={() => setMenu(null)} />
            </div>
          )}
          {nodes.length <= 1 && !menu && (
            <div className="lc-empty">
              <div className="t">Start with a trigger</div>
              <div className="dim small">A trigger decides when this agent wakes up. Press <kbd>/</kbd> or Add block.</div>
            </div>
          )}
          {toast && <div className="toast pos" role="status"><Icon name="check" size={15} />{toast}</div>}
        </div>

        {selNode && (
          <NodePanel
            key={selNode.id}
            id={selNode.id}
            blockType={selNode.data.blockType}
            params={selNode.data.params}
            def={blockMap.get(selNode.data.blockType)}
            errors={errorsByNode[selNode.id] ?? []}
            orphan={!reachable.has(selNode.id) && !["trigger", "data"].includes(familyOf(selNode.data.blockType))}
            agentId={agent.id}
            maxPerInteraction={agent.policy.max_per_interaction}
            codeHref={codeHref ? codeHref(selNode.id) : null}
            onChange={(p) => setParams(selNode.id, p)}
            onDuplicate={() => duplicate(selNode.id)}
            onDelete={() => remove(selNode.id)}
            onClose={() => setSelected(null)}
          />
        )}
      </div>
    </CanvasContext.Provider>
  );
}
