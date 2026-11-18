import type { ApproachParams } from "@/lib/api";
import { ambitionLabel, firmRoleLabel, marketsSummary, stanceZone } from "@/lib/approach";
import { formatBps } from "@/lib/collaborations";

const ZONE_LABEL = { competitor: "Competitor", both: "Competes and collaborates", collaborator: "Collaborator" };

/** Compact read-only lines for an approach (canvas node, agent page). */
export function ApproachSummary({ p }: { p: ApproachParams }) {
  const zone = stanceZone(p.stance.collaboration_bps);
  return (
    <div className="approach-summary">
      <div><span className={`zone-dot zone-${zone}`} /> {ZONE_LABEL[zone]} <span className="dim">({formatBps(p.stance.collaboration_bps)} collaborative)</span></div>
      <div>{ambitionLabel(p.stance.ambition_bps)} <span className="dim">({formatBps(p.stance.ambition_bps)} ambition)</span></div>
      <div>{firmRoleLabel(p.firm_role)}</div>
      <div className="dim">Markets: {marketsSummary(p.markets)}</div>
    </div>
  );
}
