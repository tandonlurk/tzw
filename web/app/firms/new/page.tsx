"use client";

import { useState } from "react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { api, isApiError, type Agent, type ApiError, type ApproachMarkets, type FirmRoleName, type FirmRoleSlot } from "@/lib/api";
import { approachOf, marketsSummary } from "@/lib/approach";
import { formatBps, parseSharePercent } from "@/lib/collaborations";
import {
  FIRM_ROLE_NAMES,
  LEAD_SHARE_DEFAULT_BPS,
  LEAD_SHARE_MAX_BPS,
  LEAD_SHARE_MIN_BPS,
  MANDATE_MAX,
  MAX_SLOTS,
  ROLE_TASK,
  defaultRoles,
  validateRoles,
} from "@/lib/firms";
import { parseMoney } from "@/lib/money";
import { useAsync } from "@/lib/useAsync";
import { Async, Empty, ErrorBanner } from "@/components/ui";
import { SplitBar } from "@/components/firms/FirmParts";
import { MarketEditor } from "@/components/approach/MarketEditor";

export default function NewFirmPage() {
  const agents = useAsync(api.listAgents, "agents");
  return (
    <div className="stack">
      <div className="page-head">
        <div><h1>Open a firm</h1><div className="dim">Simulated funds only. The lead pays task fees and shares profit; members never put up capital.</div></div>
        <Link className="btn" href="/firms">Cancel</Link>
      </div>
      <Async state={agents} what="agents">
        {(list) => {
          const leads = list.filter((a) => approachOf(a.graph).firm_role === "lead" && a.env === "simulated");
          return leads.length === 0 ? (
            <Empty>
              None of your simulated agents can lead a firm. Open an agent&apos;s <strong>Edit</strong> page, and in its Approach block pick <strong>Lead a firm</strong> on the outer ring
              (collaboration at least 30%).
              {list.length > 0 && <div style={{ marginTop: ".5rem" }}>{list.map((a) => <Link key={a.id} href={`/agents/${a.id}/edit`} style={{ marginRight: ".75rem" }}>{a.name}</Link>)}</div>}
            </Empty>
          ) : (
            <FirmForm leads={leads} />
          );
        }}
      </Async>
    </div>
  );
}

interface RoleRow { role: FirmRoleName; slots: string; weight: string }

function FirmForm({ leads }: { leads: Agent[] }) {
  const router = useRouter();
  const [lead, setLead] = useState(leads[0].id);
  const [name, setName] = useState("");
  const [mandate, setMandate] = useState("");
  const [share, setShare] = useState(LEAD_SHARE_DEFAULT_BPS);
  const [fee, setFee] = useState("0");
  const [roles, setRoles] = useState<RoleRow[]>(defaultRoles().map((r) => ({ role: r.role, slots: String(r.slots), weight: String(r.weight_bps / 100) })));
  const [inherit, setInherit] = useState(true);
  const [markets, setMarkets] = useState<ApproachMarkets>(() => approachOf(leads[0].graph).markets);
  const [local, setLocal] = useState<Record<string, string>>({});
  const [error, setError] = useState<ApiError | null>(null);
  const [busy, setBusy] = useState(false);

  const leadAgent = leads.find((a) => a.id === lead) ?? leads[0];
  const parsedRoles: FirmRoleSlot[] = roles.map((r) => ({
    role: r.role,
    slots: /^\d+$/.test(r.slots.trim()) ? Number(r.slots) : NaN,
    weight_bps: parseSharePercent(r.weight) ?? NaN,
  }));
  const roleErrs = validateRoles(parsedRoles);
  const weightSum = parsedRoles.reduce((s, r) => s + (Number.isFinite(r.weight_bps) ? r.weight_bps : 0), 0);
  const unusedRoles = FIRM_ROLE_NAMES.filter((n) => !roles.some((r) => r.role === n));
  const setRole = (i: number, patch: Partial<RoleRow>) => setRoles((rs) => rs.map((r, j) => (j === i ? { ...r, ...patch } : r)));

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    setError(null);
    const errs: Record<string, string> = {};
    if (!name.trim()) errs.name = "Required";
    if (mandate.length > MANDATE_MAX) errs.mandate = `At most ${MANDATE_MAX} characters`;
    const feeMinor = parseMoney(fee);
    if (feeMinor === null) errs.task_fee = "USDC amount, up to 6 decimals (0 for none)";
    if (roleErrs.length) errs.roles = roleErrs.join(" ");
    setLocal(errs);
    if (Object.keys(errs).length) return;
    setBusy(true);
    try {
      const f = await api.createFirm({
        lead_agent_id: lead,
        name: name.trim(),
        mandate: mandate.trim(),
        lead_share_bps: share,
        task_fee: feeMinor!,
        roles: parsedRoles,
        markets: inherit ? null : markets,
      });
      router.push(`/firms/${f.id}`);
    } catch (err) {
      if (isApiError(err)) setError(err);
    } finally {
      setBusy(false);
    }
  }

  const fieldErr = (k: string) => {
    const server = (error?.details ?? []).filter((d) => (d as { path?: string; field?: string })?.path === k || (d as { field?: string })?.field === k)
      .map((d) => (d as { message?: string }).message).filter(Boolean).join("; ");
    // roles: once a submit flagged them, show the live check so fixing the weights clears it
    const msg = k === "roles" && local.roles ? roleErrs.join(" ") : (local[k] ?? server);
    return msg ? <div className="field-error">{msg}</div> : null;
  };

  return (
    <form className="stack" onSubmit={submit}>
      <ErrorBanner error={error} />
      <fieldset>
        <legend>Firm</legend>
        <div className="form-grid">
          <div className="field">
            <label htmlFor="lead">Lead agent</label>
            <select id="lead" value={lead} onChange={(e) => { setLead(e.target.value); setMarkets(approachOf(leads.find((a) => a.id === e.target.value)!.graph).markets); }}>
              {leads.map((a) => <option key={a.id} value={a.id}>{a.name}</option>)}
            </select>
            <div className="dim small">Only agents whose approach is &quot;Lead a firm&quot; are listed. An agent can lead one open firm.</div>
            {fieldErr("lead_agent_id")}
          </div>
          <div className="field">
            <label htmlFor="name">Name</label>
            <input id="name" value={name} onChange={(e) => setName(e.target.value)} placeholder="Energy desk" />
            {fieldErr("name")}
          </div>
        </div>
        <div className="field">
          <label htmlFor="mandate">Mandate <span className="dim">({mandate.length}/{MANDATE_MAX})</span></label>
          <textarea id="mandate" value={mandate} maxLength={MANDATE_MAX} onChange={(e) => setMandate(e.target.value)} placeholder="Trade energy prediction markets after a >3% move." />
          {fieldErr("mandate")}
        </div>
      </fieldset>

      <fieldset>
        <legend>Profit split and fees (simulated)</legend>
        <div className="field">
          <label htmlFor="share">Lead keeps <span className="info">{formatBps(share)}</span>, members share <span className="info">{formatBps(10000 - share)}</span></label>
          <input id="share" type="range" min={LEAD_SHARE_MIN_BPS} max={LEAD_SHARE_MAX_BPS} step={100} value={share} onChange={(e) => setShare(Number(e.target.value))}
            aria-valuetext={`Lead ${formatBps(share)}`} />
          <div className="row spread dim small"><span>50% lead</span><span>100% lead</span></div>
          {fieldErr("lead_share_bps")}
        </div>
        <SplitBar leadShareBps={share} roles={parsedRoles.map((r) => ({ ...r, weight_bps: Number.isFinite(r.weight_bps) ? r.weight_bps : 0 }))} />
        <div className="field" style={{ marginTop: ".75rem", maxWidth: 260 }}>
          <label htmlFor="fee">Task fee per used contribution (USDC, simulated)</label>
          <input id="fee" value={fee} inputMode="decimal" onChange={(e) => setFee(e.target.value)} />
          <div className="dim small">Guaranteed: paid by the lead whenever a member&apos;s contribution feeds a challenge, win or lose. Goes through the lead&apos;s limits.</div>
          {fieldErr("task_fee")}
        </div>
      </fieldset>

      <fieldset>
        <legend>Roles</legend>
        <table>
          <thead><tr><th>Role</th><th>Task</th><th className="num">Slots</th><th className="num">Weight %</th><th /></tr></thead>
          <tbody>
            {roles.map((r, i) => (
              <tr key={i}>
                <td>
                  <select value={r.role} aria-label="Role" onChange={(e) => setRole(i, { role: e.target.value as FirmRoleName })}>
                    {FIRM_ROLE_NAMES.map((n) => <option key={n} value={n} disabled={n !== r.role && roles.some((x) => x.role === n)}>{n}</option>)}
                  </select>
                </td>
                <td className="dim small">{ROLE_TASK[r.role]}</td>
                <td className="num"><input style={{ width: 70 }} type="number" min={1} max={MAX_SLOTS} value={r.slots} aria-label={`${r.role} slots`} onChange={(e) => setRole(i, { slots: e.target.value })} /></td>
                <td className="num"><input style={{ width: 80 }} value={r.weight} inputMode="decimal" aria-label={`${r.role} weight percent`} onChange={(e) => setRole(i, { weight: e.target.value })} /></td>
                <td><button type="button" className="sm" disabled={roles.length <= 1} onClick={() => setRoles((rs) => rs.filter((_, j) => j !== i))}>Remove</button></td>
              </tr>
            ))}
          </tbody>
        </table>
        <div className="row" style={{ marginTop: ".4rem" }}>
          {unusedRoles.length > 0 && (
            <button type="button" className="sm" onClick={() => setRoles((rs) => [...rs, { role: unusedRoles[0], slots: "1", weight: "0" }])}>Add {unusedRoles[0]}</button>
          )}
          <span className={weightSum === 10000 ? "dim" : "neg"}>Weights total {formatBps(weightSum)} (must be 100%)</span>
        </div>
        {fieldErr("roles")}
      </fieldset>

      <fieldset>
        <legend>Markets</legend>
        <label className="inline"><input type="radio" checked={inherit} onChange={() => setInherit(true)} /> Inherit the lead&apos;s approach</label>
        <label className="inline"><input type="radio" checked={!inherit} onChange={() => setInherit(false)} /> Customise</label>
        {inherit ? (
          <div className="dim" style={{ marginTop: ".4rem" }}>{leadAgent.name}: {marketsSummary(approachOf(leadAgent.graph).markets)}. Changes to the lead&apos;s approach apply to the firm too.</div>
        ) : (
          <div style={{ marginTop: ".5rem" }}>
            <MarketEditor value={markets} onChange={setMarkets} agentId={lead} />
          </div>
        )}
        {fieldErr("markets")}
      </fieldset>

      <div className="row">
        <button className="primary" type="submit" disabled={busy}>{busy ? "Opening..." : "Open firm"}</button>
        {Object.keys(local).length > 0 && <span className="neg">Fix the highlighted fields.</span>}
      </div>
    </form>
  );
}
