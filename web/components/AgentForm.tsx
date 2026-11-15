"use client";

import { useState } from "react";
import type {
  AgentConfig,
  AgentInput,
  AgentPolicy,
  ApiError,
  BlockDef,
  Graph,
  InteractionType,
  RiskTolerance,
} from "@/lib/api";
import { formatMoney, parseMoney } from "@/lib/money";
import { GraphBuilder } from "./builder/GraphBuilder";
import { ErrorBanner, detailText } from "./ui";

const m = (v: string) => formatMoney(v, { unit: false, minFrac: 0 });

interface FormState {
  name: string;
  description: string;
  goals: string;
  strategy: string;
  categories: string;
  horizon_secs: string;
  risk_tolerance: RiskTolerance;
  schedule_secs: string;
  max_per_interaction: string;
  max_aggregate_exposure: string;
  approval_threshold: string;
  allowed: InteractionType[];
  min_conviction_bps: string;
  stop_loss: string;
}

function toState(i: AgentInput): FormState {
  return {
    name: i.name,
    description: i.description,
    goals: i.config.goals,
    strategy: i.config.strategy,
    categories: i.config.categories.join(", "),
    horizon_secs: String(i.config.horizon_secs),
    risk_tolerance: i.config.risk_tolerance,
    schedule_secs: String(i.config.schedule_secs),
    max_per_interaction: m(i.policy.max_per_interaction),
    max_aggregate_exposure: m(i.policy.max_aggregate_exposure),
    approval_threshold: m(i.policy.approval_threshold),
    allowed: i.policy.allowed_interaction_types,
    min_conviction_bps: String(i.policy.min_conviction_bps),
    stop_loss: i.policy.stop_loss === null ? "" : m(i.policy.stop_loss),
  };
}

const int = (s: string) => (/^\d+$/.test(s.trim()) ? Number(s.trim()) : null);

/** Convert form text to an AgentInput. Returns local (convenience) errors keyed by field path. */
function toInput(s: FormState, graph: Graph): { input?: AgentInput; errors: Record<string, string> } {
  const errors: Record<string, string> = {};
  if (!s.name.trim()) errors["name"] = "Name is required";
  const horizon = int(s.horizon_secs);
  if (horizon === null) errors["config.horizon_secs"] = "Whole number of seconds";
  const sched = int(s.schedule_secs);
  if (sched === null) errors["config.schedule_secs"] = "Whole number of seconds";
  const bps = int(s.min_conviction_bps);
  if (bps === null || bps > 10000) errors["policy.min_conviction_bps"] = "Whole number 0 to 10000";
  const moneyField = (key: keyof FormState, path: string) => {
    const v = parseMoney(String(s[key]));
    if (v === null) errors[path] = "Amount in USDC, up to 6 decimals";
    return v;
  };
  const mpi = moneyField("max_per_interaction", "policy.max_per_interaction");
  const mae = moneyField("max_aggregate_exposure", "policy.max_aggregate_exposure");
  const at = moneyField("approval_threshold", "policy.approval_threshold");
  let stop: string | null = null;
  if (s.stop_loss.trim() !== "") {
    stop = parseMoney(s.stop_loss);
    if (stop === null) errors["policy.stop_loss"] = "Amount in USDC, up to 6 decimals, or leave empty";
  }
  if (Object.keys(errors).length) return { errors };
  const config: AgentConfig = {
    goals: s.goals,
    strategy: s.strategy,
    categories: s.categories.split(",").map((c) => c.trim()).filter(Boolean),
    horizon_secs: horizon!,
    risk_tolerance: s.risk_tolerance,
    schedule_secs: sched!,
  };
  const policy: AgentPolicy = {
    max_per_interaction: mpi!,
    max_aggregate_exposure: mae!,
    approval_threshold: at!,
    allowed_interaction_types: s.allowed,
    min_conviction_bps: bps!,
    stop_loss: stop,
  };
  return { input: { name: s.name.trim(), description: s.description, config, policy, graph }, errors };
}

/** Map server 422 details to field paths. Unmatched details stay in the top banner. */
function serverFieldErrors(err: ApiError | null): Record<string, string> {
  const out: Record<string, string> = {};
  for (const d of err?.details ?? []) {
    if (d && typeof d === "object") {
      const o = d as Record<string, unknown>;
      const f = typeof o.field === "string" ? o.field : typeof o.path === "string" ? o.path : null;
      if (f) out[f.replace(/^\$?\.?/, "")] = detailText(d).replace(/^[^:]*: /, "");
    }
  }
  return out;
}

interface Props {
  blocks: BlockDef[];
  initial: AgentInput;
  /** "new" while creating, else the agent id. Used for UI-only node position storage. */
  storageId: string;
  submitLabel: string;
  onSubmit: (input: AgentInput) => Promise<void>;
  serverError: ApiError | null;
}

export function AgentForm({ blocks, initial, storageId, submitLabel, onSubmit, serverError }: Props) {
  const [s, setS] = useState<FormState>(() => toState(initial));
  const [graph, setGraph] = useState<Graph>(initial.graph);
  const [local, setLocal] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState(false);

  const fieldErrs = { ...serverFieldErrors(serverError), ...local };
  const set = <K extends keyof FormState>(k: K, v: FormState[K]) => setS((p) => ({ ...p, [k]: v }));

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    const r = toInput(s, graph);
    setLocal(r.errors);
    if (!r.input) return;
    setBusy(true);
    try {
      await onSubmit(r.input);
    } finally {
      setBusy(false);
    }
  }

  const text = (label: string, k: keyof FormState, path: string, hint?: string) => (
    <div className="field">
      <label htmlFor={path}>{label}</label>
      <input id={path} value={String(s[k])} onChange={(e) => set(k, e.target.value as never)} />
      {hint && <div className="dim" style={{ fontSize: ".75rem" }}>{hint}</div>}
      {fieldErrs[path] && <div className="field-error">{fieldErrs[path]}</div>}
    </div>
  );
  const area = (label: string, k: "description" | "goals" | "strategy", path: string) => (
    <div className="field">
      <label htmlFor={path}>{label}</label>
      <textarea id={path} value={s[k]} onChange={(e) => set(k, e.target.value)} />
      {fieldErrs[path] && <div className="field-error">{fieldErrs[path]}</div>}
    </div>
  );

  return (
    <form onSubmit={submit}>
      <ErrorBanner error={serverError} />
      <fieldset>
        <legend>Identity</legend>
        {text("Name", "name", "name")}
        {area("Description", "description", "description")}
      </fieldset>

      <fieldset>
        <legend>Config (guidance for the agent, not enforced limits)</legend>
        {area("Goals", "goals", "config.goals")}
        {area("Strategy", "strategy", "config.strategy")}
        <div className="form-grid">
          {text("Categories", "categories", "config.categories", "Comma separated")}
          {text("Horizon (seconds)", "horizon_secs", "config.horizon_secs")}
          {text("Schedule (seconds)", "schedule_secs", "config.schedule_secs")}
          <div className="field">
            <label htmlFor="config.risk_tolerance">Risk tolerance</label>
            <select id="config.risk_tolerance" value={s.risk_tolerance} onChange={(e) => set("risk_tolerance", e.target.value as RiskTolerance)}>
              <option value="low">low</option><option value="medium">medium</option><option value="high">high</option>
            </select>
            {fieldErrs["config.risk_tolerance"] && <div className="field-error">{fieldErrs["config.risk_tolerance"]}</div>}
          </div>
        </div>
      </fieldset>

      <fieldset>
        <legend>Policy (enforced by the backend, amounts in USDC)</legend>
        <div className="form-grid">
          {text("Max per interaction", "max_per_interaction", "policy.max_per_interaction")}
          {text("Max aggregate exposure", "max_aggregate_exposure", "policy.max_aggregate_exposure")}
          {text("Approval threshold", "approval_threshold", "policy.approval_threshold", "Above this, the owner must approve")}
          {text("Min conviction (basis points)", "min_conviction_bps", "policy.min_conviction_bps", "0 to 10000")}
          {text("Stop loss", "stop_loss", "policy.stop_loss", "Optional")}
        </div>
        <div className="field">
          <label>Allowed interaction types</label>
          <div className="row">
            {(["challenge", "collaboration"] as InteractionType[]).map((t) => (
              <label key={t} style={{ display: "inline-flex", gap: ".3rem", alignItems: "center" }}>
                <input
                  type="checkbox"
                  checked={s.allowed.includes(t)}
                  onChange={(e) => set("allowed", e.target.checked ? [...s.allowed, t] : s.allowed.filter((x) => x !== t))}
                />
                {t}
              </label>
            ))}
          </div>
          {fieldErrs["policy.allowed_interaction_types"] && <div className="field-error">{fieldErrs["policy.allowed_interaction_types"]}</div>}
        </div>
      </fieldset>

      <fieldset>
        <legend>Behavior (visual logic)</legend>
        <div className="dim" style={{ marginBottom: ".5rem" }}>
          Blocks express your preferences. They are not security: the policy above is enforced regardless of this graph.
        </div>
        <GraphBuilder blocks={blocks} initialGraph={initial.graph} storageId={storageId} onChange={setGraph} />
        {fieldErrs["graph"] && <div className="field-error">{fieldErrs["graph"]}</div>}
      </fieldset>

      <div className="row">
        <button className="primary" type="submit" disabled={busy}>{busy ? "Saving..." : submitLabel}</button>
        {Object.keys(local).length > 0 && <span className="neg">Fix the highlighted fields.</span>}
      </div>
    </form>
  );
}
