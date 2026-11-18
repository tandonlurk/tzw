"use client";

import { useContext } from "react";
import type { NodeProps } from "@xyflow/react";
import { normalizeApproach } from "@/lib/approach";
import { ApproachSummary } from "@/components/approach/ApproachSummary";
import { BuilderContext, type FlowNode } from "./BuilderNode";

/** Pinned approach block on the canvas: no ports, cannot be deleted. Selecting it opens the editor panel. */
export function ApproachNode({ id, data, selected }: NodeProps<FlowNode>) {
  const ctx = useContext(BuilderContext);
  const errs = ctx?.errors[id] ?? [];
  return (
    <div className={`bnode approach-node${errs.length ? " has-error" : ""}${selected ? " selected" : ""}`}>
      <div className="head">
        <strong className="k-approach">Approach</strong>
        <span className="badge dim" title="Every agent has exactly one approach block. Reset it instead of deleting it.">pinned</span>
      </div>
      <div className="body">
        <ApproachSummary p={normalizeApproach(data.params)} />
        <button type="button" className="sm nodrag" style={{ marginTop: ".3rem" }} onClick={() => ctx?.openApproach?.()}>Edit approach</button>
      </div>
      {errs.length > 0 && <div className="node-errors">{errs.map((e, i) => <div key={i}>{e}</div>)}</div>}
    </div>
  );
}
