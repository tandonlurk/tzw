"use client";

import { use, useState } from "react";
import Link from "next/link";
import { api, isApiError, type Agent, type ApiError, type Firm, type FirmRoleName } from "@/lib/api";
import { approachOf, marketsSummary } from "@/lib/approach";
import { formatBps } from "@/lib/collaborations";
import { ROLE_TASK, firmActionLabel, openRoles, slotsFilled } from "@/lib/firms";
import { formatMoney } from "@/lib/money";
import { useAsync } from "@/lib/useAsync";
import { Async, Empty, ErrorBanner, Money, Tile, formatTs } from "@/components/ui";
import { FirmStateBadge, SplitBar } from "@/components/firms/FirmParts";
import { InteractionCard } from "@/components/Interactions";
import { EnvBadge } from "@/components/ui";

export default function FirmPage({ params }: { params: Promise<{ id: string }> }) {
  const { id } = use(params);
  const firm = useAsync(() => api.getFirm(id), `firm:${id}`);
  const contribs = useAsync(() => api.listFirmContributions(id), `firm-contribs:${id}`);
  const agents = useAsync(api.listAgents, "agents");
  const leadId = firm.data?.lead_agent_id ?? "";
  const challenges = useAsync(() => (leadId ? api.listInteractions({ agent_id: leadId }) : Promise.resolve([])), `firm-ix:${leadId}`);

  function refresh() {
    firm.reload();
    contribs.reload();
    challenges.reload();
  }

  return (
    <Async state={firm} what="firm">
      {(f) => (
        <div className="stack">
          <Header f={f} agents={agents.data ?? []} onChanged={refresh} />
          <h2>Members</h2>
          {f.members.length === 0 ? <Empty>No members yet. Agents set to &quot;Join firms&quot; join on their own when a slot is free and markets overlap, or you can add one above.</Empty> : (
            <div className="table-wrap"><table>
              <thead><tr><th>Agent</th><th>Role</th><th>Joined</th><th className="num">Contributions</th><th className="num">Used</th><th className="num">Earned (simulated)</th></tr></thead>
              <tbody>
                {f.members.map((m) => (
                  <tr key={m.agent_id}>
                    <td><Link href={`/agents/${m.agent_id}`}>{m.name}</Link></td>
                    <td>{m.role}</td>
                    <td className="dim">{formatTs(m.joined_at)}</td>
                    <td className="num">{m.contributions}</td>
                    <td className="num">{m.used}</td>
                    <td className="num"><Money value={m.earned} tone /></td>
                  </tr>
                ))}
              </tbody>
            </table></div>
          )}

          <h2>Contributions</h2>
          <div className="dim small">A contribution is used when it fed a challenge the lead actually proposed. Fee = guaranteed task fee; payout = share of that challenge&apos;s profit.</div>
          <Async state={contribs} what="contributions">
            {(list) => list.length === 0 ? <Empty>No contributions yet. Members submit them on each tick.</Empty> : (
              <div className="table-wrap"><table>
                <thead><tr><th>When</th><th>Member</th><th>Task</th><th>Market</th><th>Side</th><th className="num">Conviction</th><th>Used</th><th className="num">Fee</th><th className="num">Payout</th></tr></thead>
                <tbody>
                  {list.map((c) => (
                    <tr key={c.id}>
                      <td className="dim">{formatTs(c.created_at)}</td>
                      <td><Link href={`/agents/${c.agent_id}`}>{c.agent_name}</Link> <span className="dim">({c.role})</span></td>
                      <td>{c.kind === "scout_candidate" ? "candidate" : "signal"}<div className="dim small">{c.summary}</div></td>
                      <td className="mono">{c.market_id}</td>
                      <td>{c.side ?? "-"}</td>
                      <td className="num">{formatBps(c.conviction_bps)}</td>
                      <td>{c.used ? (c.interaction_id ? <Link href={`/interactions/${c.interaction_id}`} className="pos">used</Link> : <span className="pos">used</span>) : <span className="dim">not used</span>}</td>
                      <td className="num">{c.fee_paid === "0" ? <span className="dim">0</span> : formatMoney(c.fee_paid)}</td>
                      <td className="num">{c.payout === "0" ? <span className="dim">0</span> : <Money value={c.payout} tone />}</td>
                    </tr>
                  ))}
                </tbody>
              </table></div>
            )}
          </Async>

          <h2>Firm challenges</h2>
          <Async state={challenges} what="challenges">
            {(list) => {
              const mine = list.filter((i) => i.firm_id === f.id);
              return mine.length === 0 ? <Empty>No challenges proposed for this firm yet.</Empty> : <div className="stack">{mine.map((i) => <InteractionCard key={i.id} i={i} />)}</div>;
            }}
          </Async>
        </div>
      )}
    </Async>
  );
}

function Header({ f, agents, onChanged }: { f: Firm; agents: Agent[]; onChanged: () => void }) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  const free = openRoles(f);
  const lead = agents.find((a) => a.id === f.lead_agent_id);

  async function run(fn: () => Promise<unknown>) {
    setBusy(true);
    setError(null);
    try {
      await fn();
      onChanged();
    } catch (e) {
      if (isApiError(e)) setError(e);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="stack">
      <div className="page-head">
        <div>
          <div className="dim small"><Link href="/firms">Firms</Link> ›</div>
          <h1>{f.name}</h1>
          <div className="row"><FirmStateBadge state={f.state} /><EnvBadge env={f.env} /><span className="dim">opened {formatTs(f.created_at)}</span></div>
          <div style={{ marginTop: ".3rem" }}>Led by <Link href={`/agents/${f.lead_agent_id}`}>{f.lead_name}</Link></div>
          {f.mandate && <div className="dim" style={{ marginTop: ".3rem" }}>{f.mandate}</div>}
        </div>
        {f.actions.includes("close") && (
          <button disabled={busy} onClick={() => { if (window.confirm("Close this firm? No new contributions; pending payouts still settle.")) run(() => api.closeFirm(f.id)); }}>
            {firmActionLabel("close")}
          </button>
        )}
      </div>
      <ErrorBanner error={error} />
      {f.state === "closed" && <div className="notice info">Closed: no new contributions. Payouts for challenges already proposed still settle.</div>}

      <div className="grid">
        <Tile label="Lead realized P&L" note="Net of member payouts (simulated)"><Money value={f.stats.lead_realized_pnl} signed tone /></Tile>
        <Tile label="Paid to members" note="Task fees + profit shares (simulated)"><Money value={f.stats.paid_to_members} /></Tile>
        <Tile label="Challenges">{f.stats.challenges}</Tile>
        <Tile label="Settled">{f.stats.settled}</Tile>
        <Tile label="Task fee" note="Per used contribution"><Money value={f.task_fee} /></Tile>
      </div>

      <div className="card">
        <h3 style={{ marginTop: 0 }}>Split</h3>
        <SplitBar leadShareBps={f.lead_share_bps} roles={f.roles} />
      </div>

      <div className="grid-2">
        <div className="card">
          <h3 style={{ marginTop: 0 }}>Roles and slots</h3>
          <table><thead><tr><th>Role</th><th className="num">Filled</th><th className="num">Weight</th></tr></thead>
            <tbody>
              {f.roles.map((r) => (
                <tr key={r.role}>
                  <td>{r.role}<div className="dim small">{ROLE_TASK[r.role] ?? ""}</div></td>
                  <td className="num">{slotsFilled(f, r.role)}/{r.slots}</td>
                  <td className="num">{formatBps(r.weight_bps)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        <div className="card">
          <h3 style={{ marginTop: 0 }}>Markets</h3>
          {f.markets ? <div>{marketsSummary(f.markets)}</div> : (
            <div>Inherits the lead&apos;s approach{lead ? <>: <span className="dim">{marketsSummary(approachOf(lead.graph).markets)}</span></> : "."}</div>
          )}
        </div>
      </div>

      {f.actions.includes("join") && free.length > 0 && <JoinForm key={`${agents.length}:${f.members.length}:${free.join()}`} f={f} agents={agents} free={free} busy={busy} run={run} />}
      {f.actions.includes("leave") && f.members.length > 0 && (
        <div className="card">
          <h3 style={{ marginTop: 0 }}>Remove a member</h3>
          <div className="row">
            {f.members.map((m) => (
              <button key={m.agent_id} className="sm" disabled={busy} onClick={() => run(() => api.leaveFirm(f.id, m.agent_id))}>{firmActionLabel("leave")}: {m.name}</button>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

function JoinForm({ f, agents, free, busy, run }: { f: Firm; agents: Agent[]; free: FirmRoleName[]; busy: boolean; run: (fn: () => Promise<unknown>) => void }) {
  const candidates = agents.filter((a) => a.id !== f.lead_agent_id && a.env === "simulated" && !f.members.some((m) => m.agent_id === a.id));
  const [agent, setAgent] = useState(candidates[0]?.id ?? "");
  const [role, setRole] = useState<FirmRoleName>(free[0]);
  const picked = candidates.find((a) => a.id === agent);
  const pickedRole = picked ? approachOf(picked.graph).firm_role : null;
  return (
    <div className="card">
      <h3 style={{ marginTop: 0 }}>Add a member</h3>
      {candidates.length === 0 ? <div className="dim">No other simulated agent to add.</div> : (
        <>
          <div className="row">
            <select style={{ maxWidth: 240 }} value={agent} aria-label="Agent" onChange={(e) => setAgent(e.target.value)}>
              {candidates.map((a) => <option key={a.id} value={a.id}>{a.name}</option>)}
            </select>
            <select style={{ maxWidth: 160 }} value={role} aria-label="Role" onChange={(e) => setRole(e.target.value as FirmRoleName)}>
              {free.map((r) => <option key={r} value={r}>{r}</option>)}
            </select>
            <button className="primary" disabled={busy || !agent} onClick={() => run(() => api.joinFirm(f.id, agent, role))}>{firmActionLabel("join")}</button>
          </div>
          {pickedRole !== null && pickedRole !== "member" && (
            <div className="pend small" style={{ marginTop: ".3rem" }}>This agent&apos;s approach is not &quot;Join firms&quot;; it will not join firms on its own. The backend decides whether an owner-added member is allowed.</div>
          )}
        </>
      )}
    </div>
  );
}
