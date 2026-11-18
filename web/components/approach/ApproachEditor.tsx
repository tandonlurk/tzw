"use client";

import type { ApproachParams, FirmRole, Money } from "@/lib/api";
import {
  FIRM_ROLES,
  ambitionLabel,
  firmRoleLabel,
  maxOpenInteractions,
  minConvictionBps,
  stakeTarget,
  stakeTargetBps,
  stanceEffects,
  stanceZone,
} from "@/lib/approach";
import { formatBps } from "@/lib/collaborations";
import { formatMoney } from "@/lib/money";
import { MarketEditor } from "./MarketEditor";
import { StanceWheel, ZONE_TEXT } from "./StanceWheel";

interface Props {
  value: ApproachParams;
  onChange: (p: ApproachParams) => void;
  /** Agent's policy max_per_interaction (minor units) for the stake target, or null if not set/invalid. */
  maxPerInteraction: Money | null;
  agentId?: string | null;
}

/** The two-wheel approach editor. Pure presentation over ApproachParams; numbers are display-only. */
export function ApproachEditor({ value, onChange, maxPerInteraction, agentId }: Props) {
  const { collaboration_bps: c, ambition_bps: a } = value.stance;
  const setStance = (s: ApproachParams["stance"]) => onChange({ ...value, stance: s });
  const setRole = (firm_role: FirmRole) => onChange({ ...value, firm_role });

  return (
    <div className="approach-editor">
      <section>
        <h3>Stance</h3>
        <div className="stance-grid">
          <div>
            <StanceWheel stance={value.stance} role={value.firm_role} onStance={setStance} onRole={setRole} />
            <div className="dim small">Drag or click the half-wheel, or focus it and use the arrow keys (Shift for fine steps). Click the outer ring to pick a firm role.</div>
          </div>
          <div>
            <NumberSlider label="Collaboration" left="competitive" right="collaborative" value={c} onChange={(v) => setStance({ ...value.stance, collaboration_bps: v })} />
            <NumberSlider label="Ambition" left="cautious" right="ambitious" value={a} onChange={(v) => setStance({ ...value.stance, ambition_bps: v })} />
            <fieldset className="tight">
              <legend>Firm role</legend>
              <div className="row">
                {FIRM_ROLES.map((r) => (
                  <label key={r} className="inline">
                    <input type="radio" name="firm_role" checked={value.firm_role === r} onChange={() => setRole(r)} /> {firmRoleLabel(r)}
                  </label>
                ))}
              </div>
            </fieldset>
            <Derived c={c} a={a} role={value.firm_role} maxPerInteraction={maxPerInteraction} />
          </div>
        </div>
      </section>
      <section>
        <h3>Markets</h3>
        <div className="dim small" style={{ marginBottom: ".5rem" }}>
          From any market down to a specific corner. Pick wedges on the wheel (centre clears everything), then narrow with structure, keywords, symbols and ranges.
        </div>
        <MarketEditor value={value.markets} onChange={(markets) => onChange({ ...value, markets })} agentId={agentId} />
      </section>
    </div>
  );
}

function NumberSlider({ label, left, right, value, onChange }: { label: string; left: string; right: string; value: number; onChange: (v: number) => void }) {
  const id = `ns-${label.toLowerCase()}`;
  return (
    <div className="field">
      <div className="row spread">
        <label htmlFor={id}>{label} <span className="info">{formatBps(value)}</span></label>
        <input
          type="number"
          min={0}
          max={10000}
          step={100}
          value={value}
          style={{ width: 100 }}
          aria-label={`${label} in basis points`}
          onChange={(e) => {
            const n = Number(e.target.value);
            if (e.target.value !== "" && Number.isInteger(n)) onChange(Math.min(10000, Math.max(0, n)));
          }}
        />
      </div>
      <input id={id} type="range" min={0} max={10000} step={100} value={value} onChange={(e) => onChange(Number(e.target.value))} />
      <div className="row spread dim small"><span>{left}</span><span>bps</span><span>{right}</span></div>
    </div>
  );
}

function YesNo({ on }: { on: boolean }) {
  return <span className={on ? "pos" : "dim"}>{on ? "yes" : "no"}</span>;
}

export function Derived({ c, a, role, maxPerInteraction }: { c: number; a: number; role: FirmRole; maxPerInteraction: Money | null }) {
  const fx = stanceEffects(c, role);
  const stake = maxPerInteraction === null ? null : stakeTarget(maxPerInteraction, a);
  return (
    <div className="derived">
      <div className={`zone-text zone-${stanceZone(c)}`}>{ZONE_TEXT[stanceZone(c)]}. {ambitionLabel(a)}.</div>
      <table className="kv"><tbody>
        <tr><td>Stake target</td><td>{stake === null ? <span className="dim">set a valid max per interaction</span> : <>{formatMoney(stake)} <span className="dim">({formatBps(stakeTargetBps(a))} of max {formatMoney(maxPerInteraction!)})</span></>}</td></tr>
        <tr><td>Default min conviction</td><td>{formatBps(minConvictionBps(a))}</td></tr>
        <tr><td>Max open deals</td><td>{maxOpenInteractions(a)}</td></tr>
        <tr><td>Starts challenges</td><td><YesNo on={fx.initiatesChallenges} /> <span className="dim">(collaboration ≤ 70%)</span></td></tr>
        <tr><td>Joins firms</td><td><YesNo on={fx.joinsFirms} /> <span className="dim">(Join firms and ≥ 30%)</span></td></tr>
        <tr><td>Can lead a firm</td><td><YesNo on={fx.canLead} /> <span className="dim">(Lead a firm and ≥ 30%)</span></td></tr>
      </tbody></table>
      {fx.roleInactive && <div className="pend small">{firmRoleLabel(role)} has no effect below 30% collaboration.</div>}
      <div className="dim small">Display only: the backend computes and enforces these. Your policy limits and any condition blocks still apply on top.</div>
    </div>
  );
}
