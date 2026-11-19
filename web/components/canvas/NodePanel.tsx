"use client";

import Link from "next/link";
import { useState } from "react";
import type { BlockDef, Money } from "@/lib/api";
import { FAMILIES, familyOf, guardsFunds } from "@/lib/blockVisuals";
import { CODE_BLOCK_TYPE } from "@/lib/customCode";
import { APPROACH_BLOCK_TYPE, approachToParams, defaultApproach, normalizeApproach } from "@/lib/approach";
import { ParamsForm } from "@/components/builder/ParamsForm";
import { CodeEditor } from "@/components/builder/CodeEditor";
import { ApproachEditor } from "@/components/approach/ApproachEditor";
import { Icon } from "@/components/shell/icons";
import { ShapeGlyph, iconFor } from "./ShapeNode";

interface Props {
  id: string;
  blockType: string;
  params: Record<string, unknown>;
  def: BlockDef | undefined;
  errors: string[];
  orphan: boolean;
  agentId: string;
  maxPerInteraction: Money | null;
  codeHref: string | null;
  onChange: (params: Record<string, unknown>) => void;
  onDuplicate: () => void;
  onDelete: () => void;
  onClose: () => void;
}

/** Right-side contextual panel for the selected block. Edits apply to the canvas draft immediately; Revert restores this block. */
export function NodePanel({ id, blockType, params, def, errors, orphan, agentId, maxPerInteraction, codeHref, onChange, onDuplicate, onDelete, onClose }: Props) {
  const [original] = useState(() => JSON.stringify(params));
  const [rev, setRev] = useState(0);
  const changed = JSON.stringify(params) !== original;
  const fam = familyOf(blockType, def?.kind);
  const f = FAMILIES[fam];
  const approach = blockType === APPROACH_BLOCK_TYPE;

  return (
    <aside className={`lc-panel${approach ? " wide" : ""}`} aria-label="Block settings">
      <div className="lp-head">
        <ShapeGlyph family={fam} size={34} icon={iconFor(blockType, fam)} />
        <div className="grow">
          <div className="lp-title">{def?.label ?? blockType}</div>
          <div className="lp-type"><span style={{ color: f.color }}>{f.label}</span> · <span className="mono">{blockType}</span> · {id}</div>
        </div>
        <button type="button" className="icon-btn sm ghost" onClick={onClose} aria-label="Close panel" data-tip="Close (Esc)"><Icon name="close" size={15} /></button>
      </div>
      <div className="lp-body">
        {def?.description && <p className="lp-desc">{def.description}</p>}
        {guardsFunds(blockType) && <div className="notice small" style={{ marginBottom: 12 }}>This block guards funds. Your policy limits are enforced by the server either way; this adds a stricter check inside the logic.</div>}
        {orphan && <div className="notice dim small" style={{ marginBottom: 12 }}>Nothing leads into this block yet, so it never runs. Connect it from a trigger or another block.</div>}
        {errors.length > 0 && (
          <div className="banner-error small" role="alert"><strong>Needs fixing</strong><ul>{errors.map((e, i) => <li key={i}>{e}</li>)}</ul></div>
        )}
        {def && (def.inputs.length > 0 || def.outputs.length > 0) && (
          <div className="lp-ports">
            {def.inputs.map((p) => <span key={`i${p.port}`} className="chip"><Icon name="chevron-right" size={11} />in · {p.data_type}</span>)}
            {def.outputs.map((p) => <span key={`o${p.port}`} className="chip">{p.port === "true" ? "yes" : p.port === "false" ? "no" : "out"} · {p.data_type}<Icon name="chevron-right" size={11} /></span>)}
          </div>
        )}
        <div className="lp-form">
          {!def ? (
            <div className="neg small">Unknown block type. The backend catalog does not list “{blockType}”; delete it or the save will be rejected.</div>
          ) : approach ? (
            <>
              <div className="row spread" style={{ marginBottom: 8 }}>
                <span className="dim small">Every agent has exactly one approach block. It sets what it watches and how it plays.</span>
                <button type="button" className="xs ghost" onClick={() => onChange(approachToParams(defaultApproach()))}>Reset to defaults</button>
              </div>
              <ApproachEditor value={normalizeApproach(params)} onChange={(p) => onChange(approachToParams(p))} maxPerInteraction={maxPerInteraction} agentId={agentId} />
            </>
          ) : blockType === CODE_BLOCK_TYPE ? (
            <>
              {codeHref && <Link href={codeHref} className="btn sm outline" style={{ marginBottom: 10 }}><Icon name="code" size={14} />Open in Code workspace</Link>}
              <CodeEditor source={typeof params.source === "string" ? params.source : ""} onChange={(src) => onChange({ ...params, source: src })} agentId={agentId} />
            </>
          ) : (
            <ParamsForm key={`${id}:${rev}`} schema={def.params_schema} params={params} onChange={onChange} />
          )}
        </div>
      </div>
      <div className="lp-foot">
        <span className="row nowrap">
          {!approach && <button type="button" className="sm ghost" onClick={onDuplicate} title="Duplicate (⌘D)"><Icon name="copy" size={14} />Duplicate</button>}
          {!approach && <button type="button" className="sm danger" onClick={onDelete} title="Delete (Backspace)"><Icon name="trash" size={14} />Delete</button>}
        </span>
        <span className="row nowrap">
          {changed && <button type="button" className="sm ghost" onClick={() => { onChange(JSON.parse(original)); setRev((r) => r + 1); }}>Revert</button>}
          <button type="button" className="sm primary" onClick={onClose}>Done</button>
        </span>
      </div>
    </aside>
  );
}
