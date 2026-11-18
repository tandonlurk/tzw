"use client";

import { useState } from "react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { api, isApiError, type ApiError } from "@/lib/api";
import { parseMoney } from "@/lib/money";
import { parseSharePercent } from "@/lib/collaborations";
import { useAsync } from "@/lib/useAsync";
import { Async, Empty, ErrorBanner } from "@/components/ui";
import { CollabCard } from "@/components/CollabCard";

export default function CollaborationsPage() {
  const list = useAsync(() => api.listCollaborations(), "collaborations");
  const agents = useAsync(api.listAgents, "agents");
  return (
    <div className="stack">
      <div className="page-head"><div><div className="dim small"><Link href="/firms">Firms</Link> ›</div><h1>Simple agreements</h1>
        <div className="dim">Two of your agents pool funds toward an objective and split the pool by an agreed share. Simulated funds; completion is declared by you, not trustless.</div></div>
        <button className="sm" onClick={() => list.reload()}>Refresh</button></div>
      <Async state={agents} what="agents">
        {(a) => a.length < 2 ? <Empty>You need at least two agents to propose a collaboration.</Empty> : (
          <ProposeForm agents={a.filter((x) => x.env === "simulated").map((x) => ({ id: x.id, name: x.name }))} />
        )}
      </Async>
      <h2>All collaborations</h2>
      <Async state={list} what="collaborations">
        {(items) => items.length === 0 ? <Empty>No collaborations yet.</Empty> : <div className="stack">{items.map((c) => <CollabCard key={c.id} c={c} />)}</div>}
      </Async>
    </div>
  );
}

function ProposeForm({ agents }: { agents: { id: string; name: string }[] }) {
  const router = useRouter();
  const [proposer, setProposer] = useState(agents[0]?.id ?? "");
  const [partner, setPartner] = useState(agents[1]?.id ?? "");
  const [objective, setObjective] = useState("");
  const [share, setShare] = useState("50");
  const [pc, setPc] = useState("");
  const [qc, setQc] = useState("");
  const [local, setLocal] = useState<Record<string, string>>({});
  const [error, setError] = useState<ApiError | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    setError(null);
    const errs: Record<string, string> = {};
    const bps = parseSharePercent(share);
    const a = parseMoney(pc);
    const b = parseMoney(qc);
    if (!objective.trim()) errs.objective = "Required (up to 500 characters)";
    if (bps === null) errs.share = "Percent from 0 to 100, up to 2 decimals";
    if (!a || a === "0") errs.pc = "Positive USDC amount, up to 6 decimals";
    if (!b || b === "0") errs.qc = "Positive USDC amount, up to 6 decimals";
    if (proposer === partner) errs.partner = "Pick two different agents";
    setLocal(errs);
    if (Object.keys(errs).length) return;
    setBusy(true);
    try {
      const c = await api.createCollaboration({
        proposer_id: proposer, partner_id: partner, objective: objective.trim(),
        proposer_share_bps: bps!, proposer_contribution: a!, partner_contribution: b!,
      });
      router.push(`/collaborations/${c.id}`);
    } catch (err) {
      if (isApiError(err)) setError(err);
    } finally {
      setBusy(false);
    }
  }

  const err = (k: string) => local[k] && <div className="field-error">{local[k]}</div>;
  return (
    <form className="card" onSubmit={submit}>
      <h3 style={{ marginTop: 0 }}>Propose a collaboration (simulated funds)</h3>
      <ErrorBanner error={error} />
      <div className="form-grid">
        <div className="field"><label>Proposer agent
          <select value={proposer} onChange={(e) => setProposer(e.target.value)}>{agents.map((a) => <option key={a.id} value={a.id}>{a.name}</option>)}</select></label></div>
        <div className="field"><label>Partner agent
          <select value={partner} onChange={(e) => setPartner(e.target.value)}>{agents.map((a) => <option key={a.id} value={a.id}>{a.name}</option>)}</select></label>{err("partner")}</div>
      </div>
      <div className="field"><label>Objective
        <input value={objective} maxLength={500} onChange={(e) => setObjective(e.target.value)} placeholder="Research ETH funding rates and share the findings" /></label>{err("objective")}</div>
      <div className="form-grid">
        <div className="field"><label>Proposer contribution (USDC, simulated)
          <input value={pc} inputMode="decimal" onChange={(e) => setPc(e.target.value)} /></label>{err("pc")}</div>
        <div className="field"><label>Partner contribution (USDC, simulated)
          <input value={qc} inputMode="decimal" onChange={(e) => setQc(e.target.value)} /></label>{err("qc")}</div>
        <div className="field"><label>Proposer share of the pool (%)
          <input value={share} inputMode="decimal" onChange={(e) => setShare(e.target.value)} /></label>{err("share")}
          <div className="dim" style={{ fontSize: ".8rem" }}>The partner gets the rest. The server computes exact payouts.</div></div>
      </div>
      <button className="primary" type="submit" disabled={busy}>{busy ? "Proposing..." : "Propose"}</button>
    </form>
  );
}
