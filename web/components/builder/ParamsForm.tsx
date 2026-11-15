"use client";

import { useState } from "react";
import type { JsonSchema } from "@/lib/api";

type Params = Record<string, unknown>;

/** Small form generated from a block's params_schema. string/number/integer/boolean/enum, else JSON textarea. */
export function ParamsForm({ schema, params, onChange }: { schema: JsonSchema; params: Params; onChange: (p: Params) => void }) {
  const props = schema?.properties ?? {};
  const keys = Object.keys(props);
  if (keys.length === 0) return <div className="dim">No parameters</div>;

  const set = (k: string, v: unknown) => {
    const next = { ...params };
    if (v === undefined) delete next[k];
    else next[k] = v;
    onChange(next);
  };

  return (
    <div>
      {keys.map((k) => (
        <ParamField
          key={k}
          name={k}
          schema={props[k]}
          required={schema.required?.includes(k) ?? false}
          value={params[k]}
          onChange={(v) => set(k, v)}
        />
      ))}
    </div>
  );
}

function ParamField({ name, schema, required, value, onChange }: {
  name: string; schema: JsonSchema; required: boolean; value: unknown; onChange: (v: unknown) => void;
}) {
  const label = `${schema.title ?? name}${required ? " *" : ""}`;
  const type = schema.type;

  if (Array.isArray(schema.enum)) {
    return (
      <div className="field nodrag">
        <label>{label}</label>
        <select value={value === undefined ? "" : String(value)} onChange={(e) => {
          const picked = schema.enum!.find((o) => String(o) === e.target.value);
          onChange(picked);
        }}>
          {value === undefined && <option value="">(unset)</option>}
          {schema.enum.map((o) => <option key={String(o)} value={String(o)}>{String(o)}</option>)}
        </select>
      </div>
    );
  }
  if (type === "boolean") {
    return (
      <div className="field nodrag">
        <label><input type="checkbox" checked={value === true} onChange={(e) => onChange(e.target.checked)} /> {label}</label>
      </div>
    );
  }
  if (type === "string") {
    return (
      <div className="field nodrag">
        <label>{label}</label>
        <input value={typeof value === "string" ? value : ""} onChange={(e) => onChange(e.target.value === "" && !required ? undefined : e.target.value)} />
      </div>
    );
  }
  if (type === "number" || type === "integer") {
    return <NumberField label={label} integer={type === "integer"} value={value} onChange={onChange} />;
  }
  return <JsonField label={label} value={value} onChange={onChange} />;
}

function NumberField({ label, integer, value, onChange }: { label: string; integer: boolean; value: unknown; onChange: (v: unknown) => void }) {
  const [text, setText] = useState(typeof value === "number" ? String(value) : "");
  const [err, setErr] = useState<string | null>(null);
  return (
    <div className="field nodrag">
      <label>{label}</label>
      <input
        value={text}
        inputMode="decimal"
        onChange={(e) => {
          const t = e.target.value;
          setText(t);
          if (t.trim() === "") { setErr(null); return onChange(undefined); }
          const n = Number(t);
          if (!Number.isFinite(n)) return setErr("Not a number");
          if (integer && !Number.isSafeInteger(n)) return setErr("Must be a whole number within safe range");
          setErr(null);
          onChange(n);
        }}
      />
      {err && <div className="field-error">{err}</div>}
    </div>
  );
}

function JsonField({ label, value, onChange }: { label: string; value: unknown; onChange: (v: unknown) => void }) {
  const [text, setText] = useState(value === undefined ? "" : JSON.stringify(value));
  const [err, setErr] = useState<string | null>(null);
  return (
    <div className="field nodrag nowheel">
      <label>{label} (JSON)</label>
      <textarea
        value={text}
        onChange={(e) => {
          const t = e.target.value;
          setText(t);
          if (t.trim() === "") { setErr(null); return onChange(undefined); }
          try { onChange(JSON.parse(t)); setErr(null); } catch { setErr("Invalid JSON"); }
        }}
      />
      {err && <div className="field-error">{err}</div>}
    </div>
  );
}
