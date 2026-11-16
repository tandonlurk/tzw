"use client";

import Link from "next/link";
import { useState } from "react";
import { api, isApiError, type Agent, type Suggestion } from "@/lib/api";
import { useAsync } from "@/lib/useAsync";
import { Async, Empty, ErrorBanner, formatTs } from "@/components/ui";
import { changeRows, decisionError, evidenceRows, sourceLabel, statusTone, type DecisionNotice } from "@/lib/suggestions";
import type { ApiError } from "@/lib/api";

export default function CoCaptainPage() {
  const pending = useAsync(() => api.listSuggestions({ status: "pending" }), "suggestions:pending");
  const decided = useAsync(async () => {
    const all = await api.listSuggestions();
    return all.filter((s) => s.status !== "pending");
  }, "suggestions:decided");
  const agents = useAsync(() => api.listAgents(), "suggestions:agents");
  const [checking, setChecking] = useState(false);
  const [checkError, setCheckError] = useState<ApiError | null>(null);
  const [checkMsg, setCheckMsg] = useState<string | null>(null);
  const [notice, setNotice] = useState<DecisionNotice | null>(null);

  function reloadAll() {
    pending.reload();
    decided.reload();
    agents.reload();
  }

  async function checkNow() {
    setChecking(true);
    setCheckError(null);
    setCheckMsg(null);
    try {
      const r = await api.generateSuggestions();
      setCheckMsg(
        r.created.length === 0
          ? `Checked (${sourceLabel(r.source)}). Nothing new to suggest.`
          : `Checked (${sourceLabel(r.source)}). ${r.created.length} new suggestion${r.created.length === 1 ? "" : "s"}.`,
      );
      reloadAll();
    } catch (e) {
      if (isApiError(e)) setCheckError(e);
    } finally {
      setChecking(false);
    }
  }

  const agentById = new Map<string, Agent>((agents.data ?? []).map((a) => [a.id, a]));

  return (
    <div className="stack">
      <div className="page-head">
        <div>
          <h1>Co-captain</h1>
          <div className="dim">
            Suggestions from your co-captain. They are advisory only: nothing here moves funds or approves a challenge.
            Accepting a suggestion that proposes a change only edits that agent&apos;s configuration, as a new version, with the same checks as editing it yourself.
          </div>
        </div>
        <button className="primary" disabled={checking} onClick={checkNow}>{checking ? "Checking..." : "Check now"}</button>
      </div>

      {checkMsg && <div className="notice info" role="status">{checkMsg}</div>}
      <ErrorBanner error={checkError} />
      {notice && <div className={`notice ${notice.tone}`} role={notice.tone === "bad" ? "alert" : "status"}>{notice.text}</div>}

      <h2>Pending</h2>
      <Async state={pending} what="suggestions">
        {(list) =>
          list.length === 0 ? (
            <Empty>No pending suggestions. Use &quot;Check now&quot; to have the co-captain look at the latest records.</Empty>
          ) : (
            <div className="stack">
              {list.map((s) => (
                <SuggestionCard
                  key={s.id}
                  s={s}
                  agent={s.agent_id ? agentById.get(s.agent_id) : undefined}
                  onNotice={setNotice}
                  onChanged={reloadAll}
                />
              ))}
            </div>
          )
        }
      </Async>

      <h2>Decided and expired</h2>
      <Async state={decided} what="past suggestions">
        {(list) =>
          list.length === 0 ? (
            <Empty>Nothing decided yet.</Empty>
          ) : (
            <div className="stack">
              {list.map((s) => (
                <div className="card" key={s.id}>
                  <div className="row spread">
                    <strong>{s.title}</strong>
                    <span className={`badge ${statusTone(s.status)}`}>{s.status}</span>
                  </div>
                  <div className="dim" style={{ fontSize: ".8rem" }}>
                    {s.agent_name ?? "All agents"} &middot; {sourceLabel(s.source)} &middot; created {formatTs(s.created_at)}
                    {s.decided_at ? ` · decided ${formatTs(s.decided_at)}` : ""}
                  </div>
                </div>
              ))}
            </div>
          )
        }
      </Async>
    </div>
  );
}

function SuggestionCard({ s, agent, onNotice, onChanged }: {
  s: Suggestion;
  agent?: Agent;
  onNotice: (n: DecisionNotice | null) => void;
  onChanged: () => void;
}) {
  const [busy, setBusy] = useState(false);
  const rows = changeRows(s.proposed_change, agent ? { policy: agent.policy, risk_tolerance: agent.config.risk_tolerance } : null);
  const evidence = evidenceRows(s.evidence);

  async function decide(kind: "accept" | "dismiss") {
    setBusy(true);
    onNotice(null);
    try {
      await (kind === "accept" ? api.acceptSuggestion(s.id) : api.dismissSuggestion(s.id));
      onNotice({
        tone: "info",
        stale: false,
        text:
          kind === "dismiss"
            ? `Dismissed "${s.title}".`
            : rows.length > 0
              ? `Accepted "${s.title}". ${s.agent_name ?? "The agent"} now has a new configuration version.`
              : `Accepted "${s.title}". It was text only, so nothing was changed.`,
      });
      onChanged();
    } catch (e) {
      if (isApiError(e)) {
        const n = decisionError(e);
        onNotice(n);
        if (n.stale) onChanged();
      }
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="card">
      <div className="row spread">
        <strong>{s.title}</strong>
        <span className="badge neutral">{sourceLabel(s.source)}</span>
      </div>
      <div className="dim" style={{ fontSize: ".8rem" }}>
        {s.agent_id ? <Link href={`/agents/${s.agent_id}`}>{s.agent_name ?? "Agent"}</Link> : "All agents"} &middot; {formatTs(s.created_at)}
      </div>
      <p style={{ margin: ".5rem 0" }}>{s.body}</p>

      {rows.length > 0 && (
        <div style={{ margin: ".5rem 0" }}>
          <div className="dim" style={{ fontSize: ".8rem" }}>If accepted, this agent&apos;s configuration changes:</div>
          <table>
            <thead><tr><th>Setting</th><th>Now</th><th>After</th></tr></thead>
            <tbody>
              {rows.map((r) => (
                <tr key={r.field}>
                  <td>{r.label}</td>
                  <td>{r.before ?? "unknown"}</td>
                  <td>{r.after}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {s.loosens_control && (
        <div className="notice" role="alert" style={{ margin: ".5rem 0" }}>
          <strong>Warning:</strong> accepting this loosens a control. It raises a limit, the approval threshold or the stop-loss, so this agent could commit more with less oversight. Review the numbers before accepting.
        </div>
      )}

      {evidence.length > 0 && (
        <details>
          <summary>Evidence</summary>
          <table>
            <tbody>{evidence.map(([k, v]) => <tr key={k}><td>{k}</td><td>{v}</td></tr>)}</tbody>
          </table>
        </details>
      )}

      <div className="row" style={{ marginTop: ".6rem" }}>
        <button className="primary" disabled={busy} onClick={() => decide("accept")}>Accept</button>
        <button disabled={busy} onClick={() => decide("dismiss")}>Dismiss</button>
      </div>
    </div>
  );
}
