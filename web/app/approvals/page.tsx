"use client";

import Link from "next/link";
import { useState } from "react";
import { api, isApiError, type Approval, type ApprovalDecision } from "@/lib/api";
import { useAsync } from "@/lib/useAsync";
import { Async, Empty, EnvBadge, ErrorBanner, Money, PageHead, formatTs } from "@/components/ui";
import { reasonSuffix } from "@/lib/interactions";
import { DevTick } from "@/components/DevTick";
import type { ApiError } from "@/lib/api";
import { shortName } from "@/lib/agentColors";
import { Icon } from "@/components/shell/icons";

type Result = { approval: Approval; decision: "approve" | "reject"; result: ApprovalDecision };

export default function ApprovalsPage() {
  const pending = useAsync(() => api.listApprovals("pending"), "approvals:pending");
  const [results, setResults] = useState<Result[]>([]);

  return (
    <div className="stack">
      <PageHead title="Approvals" sub="Stakes above an agent's approval limit wait here. Nothing is committed until you approve.">
        <DevTick onDone={pending.reload} />
      </PageHead>

      {results.map((r) => <ResultNotice key={r.approval.id} r={r} />)}

      <Async state={pending} what="approvals">
        {(list) =>
          list.length === 0 ? (
            <Empty>Nothing waiting for approval. Approvals appear when an agent wants to stake more than its approval threshold.</Empty>
          ) : (
            <div className="stack">
              {list.map((a) => (
                <ApprovalCard
                  key={a.id}
                  a={a}
                  onDecided={(decision, result) => {
                    setResults((rs) => [{ approval: a, decision, result }, ...rs]);
                    pending.reload();
                  }}
                  onStale={pending.reload}
                />
              ))}
            </div>
          )
        }
      </Async>
    </div>
  );
}

function ResultNotice({ r }: { r: Result }) {
  const i = r.result.interaction;
  const denied = i.state === "cancelled";
  const link = <Link href={`/interactions/${i.id}`}>View challenge</Link>;
  if (r.decision === "reject") {
    return <div className="notice info" role="status">Rejected {r.approval.agent_name}&apos;s stake on &quot;{i.market_title}&quot;. The challenge is cancelled and no funds moved. {link}</div>;
  }
  if (denied) {
    return (
      <div className="notice bad" role="alert">
        Approved, but the challenge was cancelled: authorization was denied when re-checked against current balances and limits.
        {reasonSuffix(i.status_reason)} No funds moved. {link}
      </div>
    );
  }
  const msg = i.state === "active" ? "Both sides are funded and the challenge is now active." : i.state === "awaiting_approval" ? "Waiting for the other agent's approval." : `Challenge is now ${i.state}.`;
  return <div className="notice info" role="status">Approved. {msg} {link}</div>;
}

function ApprovalCard({ a, onDecided, onStale }: {
  a: Approval;
  onDecided: (d: "approve" | "reject", r: ApprovalDecision) => void;
  onStale: () => void;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);

  async function decide(kind: "approve" | "reject") {
    setBusy(true);
    setError(null);
    try {
      onDecided(kind, await (kind === "approve" ? api.approve(a.id) : api.reject(a.id)));
    } catch (e) {
      if (isApiError(e)) {
        setError(e);
        if (e.status === 409) onStale();
      }
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="card attention approval-card">
      <div className="ap-main">
        <div className="row" style={{ gap: 8, marginBottom: 6 }}><span className="eyebrow" style={{ color: "var(--yellow)" }}>Needs your decision</span><EnvBadge env={a.env} /></div>
        <div className="ap-title"><strong>{shortName(a.agent_name)}</strong> wants to stake</div>
        <div className="ap-amount"><Money value={a.stake} /></div>
        <div className="small" style={{ marginTop: 6 }}>On &quot;{a.market_title}&quot; · <Link href={`/interactions/${a.interaction_id}`}>Open challenge</Link></div>
        <div className="dim small" style={{ marginTop: 4 }}>{a.reason}</div>
        <div className="dim xs" style={{ marginTop: 4 }}>Requested {formatTs(a.requested_at)} · expires {formatTs(a.expires_at)}</div>
        <div style={{ marginTop: ".5rem" }}><ErrorBanner error={error} /></div>
      </div>
      <div className="ap-actions">
        <button className="primary" disabled={busy} onClick={() => decide("approve")}><Icon name="check" size={15} />Approve</button>
        <button className="outline" disabled={busy} onClick={() => decide("reject")}><Icon name="close" size={15} />Reject</button>
      </div>
    </div>
  );
}
