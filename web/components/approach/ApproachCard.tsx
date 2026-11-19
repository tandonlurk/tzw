"use client";

import Link from "next/link";
import { api, type Agent } from "@/lib/api";
import { approachNodeOf, approachOf } from "@/lib/approach";
import { useAsync } from "@/lib/useAsync";
import { ApproachSummary } from "./ApproachSummary";
import { Derived } from "./ApproachEditor";

/** Agent page: the agent's approach (defaults when its graph has none) and its firm, if any. */
export function ApproachCard({ agent: a }: { agent: Agent }) {
  const p = approachOf(a.graph);
  const firms = useAsync(api.listFirms, "firms");
  const leads = (firms.data ?? []).filter((f) => f.lead_agent_id === a.id);
  const memberOf = (firms.data ?? []).filter((f) => f.members.some((m) => m.agent_id === a.id));
  return (
    <div className="card">
      <div className="row spread">
        <h3 style={{ margin: 0 }}>Approach</h3>
        <Link className="btn sm outline" href={`/agents/${a.id}/logic?node=${approachNodeOf(a.graph)?.id ?? ""}`}>Edit in logic</Link>
      </div>
      {!approachNodeOf(a.graph) && <div className="dim small">No approach block saved yet: the defaults below apply.</div>}
      <div className="grid-2" style={{ marginTop: ".5rem" }}>
        <ApproachSummary p={p} />
        <Derived c={p.stance.collaboration_bps} a={p.stance.ambition_bps} role={p.firm_role} maxPerInteraction={a.policy.max_per_interaction} />
      </div>
      {firms.error ? (
        <div className="dim small">Firms unavailable ({firms.error.code}).</div>
      ) : (leads.length > 0 || memberOf.length > 0) && (
        <div style={{ marginTop: ".4rem" }}>
          {leads.map((f) => <div key={f.id}>Leads <Link href={`/firms/${f.id}`}>{f.name}</Link> <span className="dim">({f.state})</span></div>)}
          {memberOf.map((f) => <div key={f.id}>Member of <Link href={`/firms/${f.id}`}>{f.name}</Link> as {f.members.find((m) => m.agent_id === a.id)?.role}</div>)}
        </div>
      )}
    </div>
  );
}
