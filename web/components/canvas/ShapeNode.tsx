"use client";

import { createContext, useContext } from "react";
import { Handle, Position, type Node, type NodeProps } from "@xyflow/react";
import type { BlockDef } from "@/lib/api";
import { FAMILIES, familyOf, paramSummary, portLabel, shapePath, type Family } from "@/lib/blockVisuals";
import { Icon, type IconName } from "@/components/shell/icons";

export type CanvasNodeData = { blockType: string; params: Record<string, unknown> };
export type CanvasFlowNode = Node<CanvasNodeData, "shape">;

export interface CanvasCtx {
  blocks: Map<string, BlockDef>;
  errors: Record<string, string[]>;
  reachable: Set<string>;
}
export const CanvasContext = createContext<CanvasCtx | null>(null);

const TYPE_ICON: Record<string, IconName> = {
  "trigger.schedule": "history",
  "trigger.market_move": "activity",
  "trigger.challenge_received": "challenge",
  "action.propose_challenge": "arrow-up-right",
  "action.accept": "check",
  "action.counter": "branch",
  "action.reject": "close",
  "action.request_approval": "approval",
  "custom.code": "code",
  "approach.profile": "search",
};
const FAMILY_ICON: Record<Family, IconName> = {
  trigger: "bolt", reasoning: "cocaptain", condition: "filter", action: "play", financial: "wallet", data: "search", code: "code", end: "close",
};
export const iconFor = (type: string, fam: Family): IconName => TYPE_ICON[type] ?? FAMILY_ICON[fam];

/** Small static glyph of a family's shape (block menu, legend, panel header). */
export function ShapeGlyph({ family, size = 22, icon }: { family: Family; size?: number; icon?: IconName }) {
  const f = FAMILIES[family];
  return (
    <span className="glyph" style={{ width: size, height: size, ["--c" as string]: f.color }} aria-hidden>
      <svg viewBox="0 0 64 64" width={size} height={size}><path d={shapePath(f.shape)} /></svg>
      {icon && <Icon name={icon} size={Math.round(size * 0.5)} />}
    </span>
  );
}

/** A block on the canvas: a compact shape (by family) with its label underneath. Configuration lives in the side panel. */
export function ShapeNode({ id, data, selected }: NodeProps<CanvasFlowNode>) {
  const ctx = useContext(CanvasContext);
  const def = ctx?.blocks.get(data.blockType);
  const fam = familyOf(data.blockType, def?.kind);
  const f = FAMILIES[fam];
  const errs = ctx?.errors[id] ?? [];
  const orphan = ctx ? fam !== "trigger" && fam !== "data" && !ctx.reachable.has(id) : false;
  const inputs = def?.inputs ?? [];
  const outputs = def?.outputs ?? [];
  const isDiamond = f.shape === "diamond";
  const summary = paramSummary(data.blockType, data.params);

  return (
    <div className={`sn fam-${fam}${selected ? " selected" : ""}${errs.length ? " has-error" : ""}${orphan ? " orphan" : ""}${!def ? " unknown" : ""}`} style={{ ["--c" as string]: f.color }}>
      {fam === "trigger" && <span className="sn-start">Start</span>}
      <svg className="sn-shape" viewBox="0 0 64 64" width={64} height={64} aria-hidden>
        <path d={shapePath(f.shape)} />
      </svg>
      <span className="sn-icon"><Icon name={iconFor(data.blockType, fam)} size={20} /></span>
      {errs.length > 0 && <span className="sn-err" title={errs.join("\n")}>!</span>}
      {inputs.map((p, i) => (
        <Handle key={`in-${p.port}`} type="target" position={Position.Left} id={p.port}
          style={{ top: `${((i + 1) / (inputs.length + 1)) * 100}%` }} title={`${p.port}: ${p.data_type}`} />
      ))}
      {outputs.map((p, i) => {
        // Diamonds: "yes" on the right point, "no" on the bottom point.
        const bottom = isDiamond && p.port === "false";
        const rightCount = isDiamond ? outputs.filter((o) => o.port !== "false").length : outputs.length;
        const ri = isDiamond ? outputs.filter((o) => o.port !== "false").indexOf(p) : i;
        const lbl = portLabel(p.port);
        return (
          <Handle key={`out-${p.port}`} type="source" position={bottom ? Position.Bottom : Position.Right} id={p.port}
            style={bottom ? { left: "50%" } : { top: `${((ri + 1) / (rightCount + 1)) * 100}%` }} title={`${p.port}: ${p.data_type}`}>
            {lbl && <span className={`sn-port ${bottom ? "b" : "r"}`}>{lbl}</span>}
          </Handle>
        );
      })}
      <div className="sn-label">
        <div className="sn-title">{def?.label ?? data.blockType}</div>
        {summary && <div className="sn-sub">{summary}</div>}
        {orphan && <div className="sn-sub warn">Not connected to a trigger</div>}
      </div>
    </div>
  );
}
