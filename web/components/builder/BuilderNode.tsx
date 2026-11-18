"use client";

import { createContext, useContext } from "react";
import { Handle, Position, type Node, type NodeProps } from "@xyflow/react";
import type { BlockDef } from "@/lib/api";
import { ParamsForm } from "./ParamsForm";
import { CodeEditor } from "./CodeEditor";
import { CODE_BLOCK_TYPE } from "@/lib/customCode";

export interface BuilderCtx {
  blocks: Map<string, BlockDef>;
  errors: Record<string, string[]>;
  setParams: (id: string, params: Record<string, unknown>) => void;
  remove: (id: string) => void;
  /** Saved agent id (enables code Test run); null while creating. */
  agentId: string | null;
  /** Opens the approach editor panel (pinned approach block). */
  openApproach?: () => void;
}

export const BuilderContext = createContext<BuilderCtx | null>(null);

export type FlowNodeData = { blockType: string; params: Record<string, unknown> };
export type FlowNode = Node<FlowNodeData, "block" | "approach">;

export function BuilderNode({ id, data }: NodeProps<FlowNode>) {
  const ctx = useContext(BuilderContext);
  if (!ctx) return null;
  const def = ctx.blocks.get(data.blockType);
  const errs = ctx.errors[id] ?? [];
  const rows = def ? Math.max(def.inputs.length, def.outputs.length) : 0;

  return (
    <div className={`bnode${errs.length ? " has-error" : ""}${data.blockType === CODE_BLOCK_TYPE ? " code-node" : ""}`}>
      <div className="head">
        <strong className={def ? `k-${def.kind}` : "neg"}>{def?.label ?? data.blockType}</strong>
        <button className="sm nodrag" onClick={() => ctx.remove(id)} aria-label={`Remove ${def?.label ?? id}`}>x</button>
      </div>
      {def ? (
        <>
          {Array.from({ length: rows }).map((_, i) => {
            const inp = def.inputs[i];
            const out = def.outputs[i];
            return (
              <div className="ports" key={i}>
                <span>{inp ? `${inp.port}: ${inp.data_type}` : ""}</span>
                <span>{out ? `${out.port}: ${out.data_type}` : ""}</span>
                {inp && <Handle type="target" position={Position.Left} id={inp.port} />}
                {out && <Handle type="source" position={Position.Right} id={out.port} />}
              </div>
            );
          })}
          <div className="body">
            {data.blockType === CODE_BLOCK_TYPE ? (
              <CodeEditor
                source={typeof data.params.source === "string" ? data.params.source : ""}
                onChange={(src) => ctx.setParams(id, { ...data.params, source: src })}
                agentId={ctx.agentId}
              />
            ) : (
              <ParamsForm schema={def.params_schema} params={data.params} onChange={(p) => ctx.setParams(id, p)} />
            )}
          </div>
        </>
      ) : (
        <div className="body neg">Unknown block type &quot;{data.blockType}&quot;</div>
      )}
      {errs.length > 0 && <div className="node-errors">{errs.map((e, i) => <div key={i}>{e}</div>)}</div>}
    </div>
  );
}
