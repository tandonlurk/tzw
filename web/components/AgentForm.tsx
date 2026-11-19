"use client";

import { useEffect, useMemo, useState, type ReactNode } from "react";
import type {
  AgentConfig, AgentInput, AgentPolicy, ApiError, GraphError, Graph, InteractionType, RiskTolerance, Template,
} from "@/lib/api";
import { formatMoney, parseMoney } from "@/lib/money";
import { ErrorBanner, detailText } from "./ui";
import { Icon } from "./shell/icons";

const m = (v: string) => formatMoney(v, { unit: false, minFrac: 0 });

interface FormState {
  name: string;
  description: string;
  goals: string;
  strategy: string;
  categories: string[];
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
    categories: [...i.config.categories],
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

/** Convert form text to an AgentInput. Returns local (convenience) errors keyed by field path. The backend re-validates. */
function toInput(s: FormState, graph: Graph): { input?: AgentInput; errors: Record<string, string> } {
  const errors: Record<string, string> = {};
  if (!s.name.trim()) errors["name"] = "Give the agent a name";
  const horizon = int(s.horizon_secs);
  if (horizon === null || horizon === 0) errors["config.horizon_secs"] = "Whole number of seconds above zero";
  const sched = int(s.schedule_secs);
  if (sched === null || sched === 0) errors["config.schedule_secs"] = "Whole number of seconds above zero";
  const bps = int(s.min_conviction_bps);
  if (bps === null || bps > 10000) errors["policy.min_conviction_bps"] = "Between 0% and 100%";
  const moneyField = (key: keyof FormState, path: string) => {
    const v = parseMoney(String(s[key]));
    if (v === null) errors[path] = "Amount in USDC, up to 6 decimals";
    return v;
  };
  const mpi = moneyField("max_per_interaction", "policy.max_per_interaction");
  const mae = moneyField("max_aggregate_exposure", "policy.max_aggregate_exposure");
  const at = moneyField("approval_threshold", "policy.approval_threshold");
  if (mpi && mae && BigInt(mpi) > BigInt(mae)) errors["policy.max_per_interaction"] = "Cannot be above the total exposure limit";
  let stop: string | null = null;
  if (s.stop_loss.trim() !== "") {
    stop = parseMoney(s.stop_loss);
    if (stop === null) errors["policy.stop_loss"] = "Amount in USDC, up to 6 decimals, or leave empty";
  }
  if (Object.keys(errors).length) return { errors };
  const config: AgentConfig = {
    goals: s.goals,
    strategy: s.strategy,
    categories: s.categories.map((c) => c.trim()).filter(Boolean),
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

function detailPath(d: unknown): string | null {
  if (!d || typeof d !== "object") return null;
  const o = d as Record<string, unknown>;
  const f = typeof o.path === "string" ? o.path : typeof o.field === "string" ? o.field : null;
  return f ? f.replace(/^\$?\.?/, "") : null;
}

function detailMessage(d: unknown): string {
  const o = d as Record<string, unknown>;
  const code = typeof o.code === "string" ? `${o.code}: ` : "";
  return typeof o.message === "string" ? code + o.message : detailText(d);
}

/** Map server 422 details (path like "policy.max_per_interaction") to form fields. graph.* details go to the logic canvas. */
function serverFieldErrors(err: ApiError | null): Record<string, string> {
  const acc: Record<string, string[]> = {};
  for (const d of err?.details ?? []) {
    const f = detailPath(d);
    if (f && !f.startsWith("graph")) (acc[f] ??= []).push(detailMessage(d));
  }
  return Object.fromEntries(Object.entries(acc).map(([k, v]) => [k, v.join("; ")]));
}

/** graph.nodes.<id> -> node error; any other graph path -> graph-level error. */
export function serverGraphErrors(err: ApiError | null): GraphError[] {
  const out: GraphError[] = [];
  for (const d of err?.details ?? []) {
    const f = detailPath(d);
    if (!f || !f.startsWith("graph")) continue;
    const mm = /^graph\.nodes\.([^.]+)/.exec(f);
    const o = d as Record<string, unknown>;
    out.push({ code: String(o.code ?? "invalid"), message: String(o.message ?? ""), node_id: mm ? mm[1] : null });
  }
  return out;
}

// ---- Sections

type SectionId = "identity" | "goals" | "risk" | "permissions" | "automation";
const SECTIONS: { id: SectionId; label: string; lede: string; fields: string[] }[] = [
  { id: "identity", label: "Identity", lede: "Who this agent is. The name shows everywhere its activity appears.", fields: ["name", "description"] },
  { id: "goals", label: "Goals and strategy", lede: "Guidance the agent reasons with. It shapes what it looks for, but it is not a limit: limits are in Risk and Permissions.", fields: ["config.goals", "config.strategy", "config.categories", "config.horizon_secs"] },
  { id: "risk", label: "Risk", lede: "How much the agent may put at stake. These limits are enforced by the server on every action, whatever the agent decides.", fields: ["config.risk_tolerance", "policy.max_per_interaction", "policy.max_aggregate_exposure", "policy.stop_loss"] },
  { id: "permissions", label: "Permissions", lede: "What the agent may do on its own, and when it must ask you first.", fields: ["policy.allowed_interaction_types", "policy.approval_threshold"] },
  { id: "automation", label: "Automation", lede: "When the agent looks for opportunities and how sure it must be before acting.", fields: ["config.schedule_secs", "policy.min_conviction_bps"] },
];
const sectionOf = (path: string): SectionId => SECTIONS.find((s) => s.fields.includes(path))?.id ?? "identity";

const HORIZONS = [{ v: "900", l: "15 min" }, { v: "3600", l: "1 hour" }, { v: "86400", l: "1 day" }, { v: "604800", l: "1 week" }];
const SCHEDULES = [{ v: "60", l: "Every minute" }, { v: "300", l: "Every 5 min" }, { v: "900", l: "Every 15 min" }, { v: "3600", l: "Hourly" }];
const RISK: { v: RiskTolerance; t: string; d: string }[] = [
  { v: "low", t: "Careful", d: "Prefers small, high-conviction stakes. Passes on most disagreements." },
  { v: "medium", t: "Balanced", d: "Takes a fair share of opportunities at moderate size." },
  { v: "high", t: "Bold", d: "Seeks out disagreements and stakes closer to its limits." },
];
const TYPES: { v: InteractionType; t: string; d: string }[] = [
  { v: "challenge", t: "Challenges", d: "Stake against another agent on a measurable outcome, e.g. whether BTC rises 2% in an hour." },
  { v: "collaboration", t: "Collaborations", d: "Pool funds or work with other agents and share the result." },
];

export function humanSecs(s: string): string {
  const n = int(s);
  if (n === null) return s;
  if (n % 604800 === 0) return `${n / 604800} week${n === 604800 ? "" : "s"}`;
  if (n % 86400 === 0) return `${n / 86400} day${n === 86400 ? "" : "s"}`;
  if (n % 3600 === 0) return `${n / 3600} hour${n === 3600 ? "" : "s"}`;
  if (n % 60 === 0) return `${n / 60} min`;
  return `${n} s`;
}

interface Props {
  initial: AgentInput;
  submitLabel: string;
  onSubmit: (input: AgentInput) => Promise<void>;
  serverError: ApiError | null;
  /** "wizard": guided steps for creation. "sections": one page with a section index, for editing. */
  variant?: "wizard" | "sections";
  /** Wizard only: starting templates shown as the first step. */
  templates?: Template[];
  onCancel?: () => void;
}

export function AgentForm({ initial, submitLabel, onSubmit, serverError, variant = "sections", templates, onCancel }: Props) {
  const [s, setS] = useState<FormState>(() => toState(initial));
  const [graph, setGraph] = useState<Graph>(initial.graph);
  const [template, setTemplate] = useState<string | null>(null);
  const [local, setLocal] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState(false);
  const fieldErrs = { ...serverFieldErrors(serverError), ...local };
  const set = <K extends keyof FormState>(k: K, v: FormState[K]) => {
    setS((p) => ({ ...p, [k]: v }));
    setLocal((l) => { const n = { ...l }; for (const key of Object.keys(n)) if (key.endsWith(k as string)) delete n[key]; return n; });
  };
  const graphErrors = serverGraphErrors(serverError);
  const dirty = useMemo(() => JSON.stringify(s) !== JSON.stringify(toState(initial)), [s, initial]);

  async function submit() {
    const r = toInput(s, graph);
    setLocal(r.errors);
    if (!r.input) return Object.keys(r.errors)[0];
    setBusy(true);
    try {
      await onSubmit(r.input);
    } finally {
      setBusy(false);
    }
    return null;
  }

  // ---- field renderers
  const err = (path: string) => fieldErrs[path] && <div className="field-error" role="alert">{fieldErrs[path]}</div>;
  const help = (t: string) => <span className="help" title={t} aria-label={t}>?</span>;
  const money = (label: string, k: keyof FormState, path: string, hint: ReactNode, tip?: string, placeholder?: string) => (
    <div className={`field${fieldErrs[path] ? " has-error" : ""}`}>
      <label htmlFor={path}>{label}{tip && help(tip)}</label>
      <div className="input-affix">
        <input id={path} value={String(s[k])} onChange={(e) => set(k, e.target.value as never)} inputMode="decimal" placeholder={placeholder} aria-invalid={!!fieldErrs[path]} />
        <span className="affix">USDC</span>
      </div>
      <div className="hint">{hint}</div>
      {err(path)}
    </div>
  );
  const presets = (label: string, k: "horizon_secs" | "schedule_secs", path: string, opts: { v: string; l: string }[], hint: ReactNode) => {
    const custom = !opts.some((o) => o.v === s[k]);
    return (
      <div className={`field${fieldErrs[path] ? " has-error" : ""}`}>
        <label>{label}</label>
        <div className="row" style={{ gap: 6 }}>
          {opts.map((o) => <button key={o.v} type="button" className={`chip${s[k] === o.v ? " on" : ""}`} aria-pressed={s[k] === o.v} onClick={() => set(k, o.v)}>{o.l}</button>)}
          <span className="row nowrap" style={{ gap: 6 }}>
            <input aria-label={`${label} in seconds`} style={{ width: 110, height: 28 }} value={s[k]} onChange={(e) => set(k, e.target.value)} inputMode="numeric" />
            <span className="dim xs">{custom ? `seconds · ${humanSecs(s[k])}` : "seconds"}</span>
          </span>
        </div>
        <div className="hint">{hint}</div>
        {err(path)}
      </div>
    );
  };

  const body: Record<SectionId, ReactNode> = {
    identity: (
      <>
        <div className={`field${fieldErrs.name ? " has-error" : ""}`}>
          <label htmlFor="name">Name</label>
          <input id="name" value={s.name} onChange={(e) => set("name", e.target.value)} placeholder="e.g. Momentum Scout" autoFocus={variant === "wizard"} aria-invalid={!!fieldErrs.name} />
          {err("name")}
        </div>
        <div className="field">
          <label htmlFor="description">Description</label>
          <textarea id="description" value={s.description} onChange={(e) => set("description", e.target.value)} placeholder="One or two sentences on what this agent is for." />
          <div className="hint">Shown on its card in My Agents.</div>
          {err("description")}
        </div>
      </>
    ),
    goals: (
      <>
        <div className="field">
          <label htmlFor="config.goals">What should it try to achieve?</label>
          <textarea id="config.goals" value={s.goals} onChange={(e) => set("goals", e.target.value)} placeholder="e.g. Find short-term BTC moves other agents misjudge." />
          {err("config.goals")}
        </div>
        <div className="field">
          <label htmlFor="config.strategy">Strategy and personality</label>
          <textarea id="config.strategy" value={s.strategy} onChange={(e) => set("strategy", e.target.value)} placeholder="e.g. Contrarian. Takes the other side of large moves." />
          <div className="hint">Plain language. The agent reads this; the server never treats it as permission.</div>
          {err("config.strategy")}
        </div>
        <ChipsField label="Opportunity categories" values={s.categories} onChange={(v) => set("categories", v)} error={fieldErrs["config.categories"]}
          hint="Markets it should pay attention to, e.g. crypto, macro. Press Enter to add." />
        {presets("Time horizon", "horizon_secs", "config.horizon_secs", HORIZONS, "How long its challenges usually run before they resolve.")}
      </>
    ),
    risk: (
      <>
        <div className="field">
          <label>Risk tolerance</label>
          <div className="choice-grid">
            {RISK.map((r) => (
              <button key={r.v} type="button" className={`choice${s.risk_tolerance === r.v ? " on" : ""}`} aria-pressed={s.risk_tolerance === r.v} onClick={() => set("risk_tolerance", r.v)}>
                <span className="ct">{r.t}</span><span className="cd">{r.d}</span>
              </button>
            ))}
          </div>
          <div className="hint">Guidance for its reasoning. The amounts below are the hard limits.</div>
        </div>
        <div className="form-grid">
          {money("Max per challenge", "max_per_interaction", "policy.max_per_interaction", "The most it can stake on any single challenge.", "Per-interaction commitment limit")}
          {money("Max total exposure", "max_aggregate_exposure", "policy.max_aggregate_exposure", "The most it can have locked across all open challenges.", "Aggregate exposure limit: committed funds summed over every open interaction")}
        </div>
        {money("Stop loss", "stop_loss", "policy.stop_loss", "Optional. Once realized losses reach this, the agent stops taking new challenges.", "Stop condition on realized losses", "No stop loss")}
      </>
    ),
    permissions: (
      <>
        <div className={`field${fieldErrs["policy.allowed_interaction_types"] ? " has-error" : ""}`}>
          <label>Allowed to</label>
          <div className="choice-grid">
            {TYPES.map((t) => {
              const on = s.allowed.includes(t.v);
              return (
                <button key={t.v} type="button" className={`choice${on ? " on" : ""}`} aria-pressed={on}
                  onClick={() => set("allowed", on ? s.allowed.filter((x) => x !== t.v) : [...s.allowed, t.v])}>
                  <span className="ct"><Icon name={on ? "check" : "plus"} size={14} />{t.t}</span><span className="cd">{t.d}</span>
                </button>
              );
            })}
          </div>
          {s.allowed.length === 0 && <div className="hint pend">With nothing allowed the agent can only watch.</div>}
          {err("policy.allowed_interaction_types")}
        </div>
        {money("Ask me before staking more than", "approval_threshold", "policy.approval_threshold", "Stakes above this wait in Approvals until you accept or reject them. Use 0 to approve everything yourself.", "Approval requirement")}
      </>
    ),
    automation: (
      <>
        {presets("Check for opportunities", "schedule_secs", "config.schedule_secs", SCHEDULES, "How often it evaluates markets and incoming challenges.")}
        <div className={`field${fieldErrs["policy.min_conviction_bps"] ? " has-error" : ""}`}>
          <label htmlFor="conv">Act only when at least this sure{help("Minimum conviction: the agent's own confidence score. It is not a probability, so treat it as a filter.")}</label>
          <div className="slider-row">
            <input id="conv" type="range" min={0} max={10000} step={100} value={int(s.min_conviction_bps) ?? 0} onChange={(e) => set("min_conviction_bps", e.target.value)} />
            <div className="input-affix">
              <input aria-label="Minimum conviction in percent" value={String(Math.round((int(s.min_conviction_bps) ?? 0) / 100))} inputMode="numeric"
                onChange={(e) => { const n = int(e.target.value); set("min_conviction_bps", n === null ? e.target.value : String(Math.min(100, n) * 100)); }} />
              <span className="affix">%</span>
            </div>
          </div>
          <div className="hint">Below this, it skips the opportunity. Approval rules still apply above it.</div>
          {err("policy.min_conviction_bps")}
        </div>
        <div className="notice dim small">The step-by-step behaviour (which markets trigger it, what it checks, when it proposes) lives in the agent&apos;s <strong>Logic</strong> canvas. It starts from the template&apos;s logic and you can change it after saving.</div>
      </>
    ),
  };

  if (variant === "wizard") {
    return (
      <Wizard
        sections={SECTIONS}
        body={body}
        templates={templates}
        template={template}
        onTemplate={(t) => { setTemplate(t.id); setS((p) => ({ ...toState({ name: p.name, description: t.description, config: t.config, policy: t.policy, graph: t.graph }) })); setGraph(t.graph); }}
        errors={fieldErrs}
        serverError={serverError}
        graphErrors={graphErrors}
        validateSection={(id) => {
          const r = toInput(s, graph);
          const mine = Object.fromEntries(Object.entries(r.errors).filter(([k]) => sectionOf(k) === id));
          setLocal(mine);
          return Object.keys(mine).length === 0;
        }}
        review={<Review s={s} template={templates?.find((t) => t.id === template)?.name ?? null} graph={graph} />}
        submit={submit}
        busy={busy}
        submitLabel={submitLabel}
        onCancel={onCancel}
      />
    );
  }

  return (
    <SectionsForm sections={SECTIONS} body={body} errors={fieldErrs} serverError={serverError} graphErrors={graphErrors}
      submit={submit} busy={busy} submitLabel={submitLabel} dirty={dirty} onCancel={onCancel} onReset={() => { setS(toState(initial)); setLocal({}); }} />
  );
}

function ChipsField({ label, values, onChange, hint, error }: { label: string; values: string[]; onChange: (v: string[]) => void; hint: string; error?: string }) {
  const [text, setText] = useState("");
  const add = () => {
    const parts = text.split(",").map((t) => t.trim()).filter(Boolean).filter((t) => !values.includes(t));
    if (parts.length) onChange([...values, ...parts]);
    setText("");
  };
  return (
    <div className="field">
      <label htmlFor="config.categories">{label}</label>
      <div className="row" style={{ gap: 6, padding: 6, border: "1px solid var(--line-strong)", borderRadius: "var(--r)", background: "var(--bg-2)" }}>
        {values.map((v) => (
          <span key={v} className="chip on">{v}<button type="button" aria-label={`Remove ${v}`} onClick={() => onChange(values.filter((x) => x !== v))}><Icon name="close" size={11} /></button></span>
        ))}
        <input id="config.categories" value={text} onChange={(e) => setText(e.target.value)} onBlur={add}
          onKeyDown={(e) => { if (e.key === "Enter" || e.key === ",") { e.preventDefault(); add(); } else if (e.key === "Backspace" && !text && values.length) onChange(values.slice(0, -1)); }}
          placeholder={values.length ? "" : "crypto, macro"} style={{ flex: 1, minWidth: 120, border: 0, height: 26, background: "transparent", boxShadow: "none" }} />
      </div>
      <div className="hint">{hint}</div>
      {error && <div className="field-error">{error}</div>}
    </div>
  );
}

function GraphErrorList({ errors }: { errors: GraphError[] }) {
  if (!errors.length) return null;
  return (
    <div className="banner-error">
      The agent&apos;s logic was rejected. Open the Logic tab to fix it:
      <ul>{errors.map((e, i) => <li key={i}>{e.node_id ? `Block ${e.node_id}: ` : ""}{e.code}: {e.message}</li>)}</ul>
    </div>
  );
}

function Review({ s, template, graph }: { s: FormState; template: string | null; graph: Graph }) {
  const row = (k: string, v: ReactNode) => <tr><td>{k}</td><td className="text">{v}</td></tr>;
  const usdc = (v: string) => (v.trim() ? `${v} USDC` : "None");
  return (
    <div className="card flat">
      <table className="kv"><tbody>
        {row("Name", s.name || <span className="neg">Missing</span>)}
        {template && row("Template", template)}
        {row("Risk tolerance", RISK.find((r) => r.v === s.risk_tolerance)?.t)}
        {row("Max per challenge", usdc(s.max_per_interaction))}
        {row("Max total exposure", usdc(s.max_aggregate_exposure))}
        {row("Stop loss", usdc(s.stop_loss))}
        {row("Allowed to", s.allowed.length ? s.allowed.map((a) => TYPES.find((t) => t.v === a)?.t).join(", ") : "Nothing (watch only)")}
        {row("Asks you above", usdc(s.approval_threshold))}
        {row("Checks", `every ${humanSecs(s.schedule_secs)}`)}
        {row("Horizon", humanSecs(s.horizon_secs))}
        {row("Min conviction", `${Math.round((int(s.min_conviction_bps) ?? 0) / 100)}%`)}
        {row("Logic", `${graph.nodes.length} blocks from the template`)}
      </tbody></table>
      <div className="notice dim small" style={{ marginTop: 12 }}>The agent starts with simulated funds only after you allocate some to it. Nothing real is at stake.</div>
    </div>
  );
}

interface WizardProps {
  sections: typeof SECTIONS;
  body: Record<SectionId, ReactNode>;
  templates?: Template[];
  template: string | null;
  onTemplate: (t: Template) => void;
  errors: Record<string, string>;
  serverError: ApiError | null;
  graphErrors: GraphError[];
  validateSection: (id: SectionId) => boolean;
  review: ReactNode;
  submit: () => Promise<string | null | undefined>;
  busy: boolean;
  submitLabel: string;
  onCancel?: () => void;
}

function Wizard({ sections, body, templates, template, onTemplate, errors, serverError, graphErrors, validateSection, review, submit, busy, submitLabel, onCancel }: WizardProps) {
  const hasTemplates = !!templates && templates.length > 0;
  const steps = [...(hasTemplates ? [{ id: "template", label: "Template" }] : []), ...sections.map((x) => ({ id: x.id, label: x.label })), { id: "review", label: "Review" }];
  const [i, setI] = useState(0);
  const [seen, setSeen] = useState(0);
  const cur = steps[i];
  const sec = sections.find((x) => x.id === cur.id);
  const errSections = new Set(Object.keys(errors).map(sectionOf));

  // A server rejection jumps to the first step with a field error.
  useEffect(() => {
    if (!serverError) return;
    const first = Object.keys(serverFieldErrors(serverError))[0];
    if (first) {
      const idx = steps.findIndex((st) => st.id === sectionOf(first));
      // eslint-disable-next-line react-hooks/set-state-in-effect
      if (idx >= 0) setI(idx);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [serverError]);

  const go = (n: number) => { setI(n); setSeen((v) => Math.max(v, n)); };
  function next() {
    if (sec && !validateSection(sec.id)) return;
    go(i + 1);
  }
  async function create() {
    const firstErr = await submit();
    if (firstErr) {
      const idx = steps.findIndex((st) => st.id === sectionOf(firstErr));
      if (idx >= 0) setI(idx);
    }
  }
  const needTemplate = cur.id === "template" && !template;

  return (
    <div className="wizard">
      <nav className="steps" aria-label="Steps">
        {steps.map((st, k) => (
          <button key={st.id} type="button" className={`step${k === i ? " on" : ""}${k < i || k <= seen && k !== i ? " done" : ""}${errSections.has(st.id as SectionId) && st.id !== "template" && st.id !== "review" ? " err" : ""}`}
            onClick={() => (k <= seen || (k === i + 1 && !needTemplate)) && go(k)} disabled={k > seen && k !== i + 1} aria-current={k === i ? "step" : undefined}>
            <span className="n">{k < i ? <Icon name="check" size={12} /> : k + 1}</span><span className="lbl">{st.label}</span>
          </button>
        ))}
      </nav>
      <div className="wizard-pane" key={cur.id}>
        {cur.id === "template" && templates && (
          <>
            <h2>Start from a template</h2>
            <p className="lede">Templates set sensible limits and logic. You can change everything in the next steps.</p>
            <div className="choice-grid">
              {templates.map((t) => (
                <button key={t.id} type="button" className={`choice${template === t.id ? " on" : ""}`} aria-pressed={template === t.id} onClick={() => onTemplate(t)}>
                  <span className="ct">{t.name}</span>
                  <span className="cd">{t.description}</span>
                  <span className="tags">
                    <span className="badge plain">{t.config.risk_tolerance} risk</span>
                    <span className="badge plain">≤ {formatMoney(t.policy.max_per_interaction)}</span>
                  </span>
                </button>
              ))}
            </div>
          </>
        )}
        {sec && (
          <>
            <h2>{sec.label}</h2>
            <p className="lede">{sec.lede}</p>
            {body[sec.id]}
          </>
        )}
        {cur.id === "review" && (
          <>
            <h2>Review and create</h2>
            <p className="lede">Check the limits. They are what the server enforces; everything else guides the agent.</p>
            <ErrorBanner error={serverError && Object.keys(serverFieldErrors(serverError)).length === 0 && graphErrors.length === 0 ? serverError : null} />
            <GraphErrorList errors={graphErrors} />
            {review}
          </>
        )}
        <div className="wizard-nav">
          <div>{i > 0 ? <button type="button" className="ghost" onClick={() => go(i - 1)}>Back</button> : onCancel && <button type="button" className="ghost" onClick={onCancel}>Cancel</button>}</div>
          {cur.id === "review" ? (
            <button type="button" className="primary" disabled={busy} onClick={create}>{busy ? "Creating..." : submitLabel}</button>
          ) : (
            <button type="button" className="primary" disabled={needTemplate} onClick={next}>Continue<Icon name="chevron-right" size={14} /></button>
          )}
        </div>
      </div>
    </div>
  );
}

interface SectionsProps {
  sections: typeof SECTIONS;
  body: Record<SectionId, ReactNode>;
  errors: Record<string, string>;
  serverError: ApiError | null;
  graphErrors: GraphError[];
  submit: () => Promise<string | null | undefined>;
  busy: boolean;
  submitLabel: string;
  dirty: boolean;
  onCancel?: () => void;
  onReset: () => void;
}

function SectionsForm({ sections, body, errors, serverError, graphErrors, submit, busy, submitLabel, dirty, onCancel, onReset }: SectionsProps) {
  const [active, setActive] = useState<SectionId>("identity");

  const errSections = new Set(Object.keys(errors).map(sectionOf));

  useEffect(() => {
    const els = sections.map((x) => document.getElementById(`sec-${x.id}`)).filter(Boolean) as HTMLElement[];
    const io = new IntersectionObserver((entries) => {
      const vis = entries.filter((e) => e.isIntersecting).sort((a, b) => a.boundingClientRect.top - b.boundingClientRect.top)[0];
      if (vis) setActive(vis.target.id.slice(4) as SectionId);
    }, { rootMargin: "-10% 0px -70% 0px" });
    els.forEach((e) => io.observe(e));
    return () => io.disconnect();
  }, [sections]);

  async function save(e: React.FormEvent) {
    e.preventDefault();
    const first = await submit();
    if (first) document.getElementById(`sec-${sectionOf(first)}`)?.scrollIntoView({ behavior: "smooth", block: "start" });
  }

  return (
    <form onSubmit={save} className="cfg">
      <nav className="section-nav" aria-label="Sections">
        {sections.map((x) => (
          <a key={x.id} href={`#sec-${x.id}`} className={`step${active === x.id ? " on" : ""}${errSections.has(x.id) ? " err" : ""}`}
            onClick={(e) => { e.preventDefault(); document.getElementById(`sec-${x.id}`)?.scrollIntoView({ behavior: "smooth", block: "start" }); }}>
            <span className="lbl">{x.label}</span>
            {errSections.has(x.id) && <Icon name="alert" size={13} style={{ marginLeft: "auto", color: "var(--rose)" }} />}
          </a>
        ))}
      </nav>
      <div style={{ minWidth: 0, maxWidth: 760 }}>
        <ErrorBanner error={serverError && Object.keys(serverFieldErrors(serverError)).length === 0 && graphErrors.length === 0 ? serverError : null} />
        <GraphErrorList errors={graphErrors} />
        {sections.map((x) => (
          <section key={x.id} id={`sec-${x.id}`} className="cfg-section">
            <h2>{x.label}</h2>
            <p className="lede">{x.lede}</p>
            {body[x.id]}
          </section>
        ))}
        <div className="cfg-bar">
          <span className="small dim">{Object.keys(errors).length > 0 ? <span className="neg">Fix the highlighted fields.</span> : dirty ? "Unsaved changes. Saving creates a new version; history is kept." : "No changes."}</span>
          <span className="row nowrap">
            {dirty && <button type="button" className="ghost sm" onClick={onReset}>Discard</button>}
            {onCancel && <button type="button" className="ghost sm" onClick={onCancel}>Cancel</button>}
            <button className="primary sm" type="submit" disabled={busy || !dirty}>{busy ? "Saving..." : submitLabel}</button>
          </span>
        </div>
      </div>
    </form>
  );
}
