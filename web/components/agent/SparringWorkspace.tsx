"use client";

import { useEffect, useState, type FormEvent } from "react";
import { ApiError, api, isApiError, type Agent } from "@/lib/api";
import { useAsync } from "@/lib/useAsync";
import { safeEvidenceUrl, sparringPrompt, type SparringRound, type SparringSession } from "@/lib/sparring";
import { Async, Empty, ErrorBanner, formatTs } from "@/components/ui";
import { useWorkspace } from "./AgentWorkspace";

export default function SparringWorkspace() {
  const { agent } = useWorkspace();
  const a = agent.data!;
  return <Practice key={a.id} a={a} />;
}

function Practice({ a }: { a: Agent }) {
  const sessions = useAsync(() => api.listSparring(a.id), `sparring:${a.id}`);
  const { reload } = sessions;
  const [selected, setSelected] = useState<string | null>(null);
  const [thesis, setThesis] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  const session = sessions.data?.find((s) => s.id === selected) ?? sessions.data?.[0];

  useEffect(() => {
    const timer = window.setInterval(() => {
      if (document.visibilityState === "visible") reload();
    }, 10000);
    return () => window.clearInterval(timer);
  }, [reload]);

  async function create(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const created = await api.createSparring(a.id, thesis.trim());
      setSelected(created.id);
      setThesis("");
      sessions.reload();
    } catch (e) {
      setError(asError(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="stack">
      <div className="row spread">
        <div><h2>Meet your adversary</h2><p className="dim">Let Grok Bot find the weak point before you commit.</p></div>
        <span className="badge info">Practice · no funds</span>
      </div>
      <div className="notice info">A separate Grok Bot researches counterarguments and submits critiques here. Replies and revisions stay in this practice record; they do not change your agent or any live challenge. The connection identifies where a reply came from, not which model produced it.</div>
      <form className="card" onSubmit={create}>
        <div className="card-head"><h3>Put a thesis on the table</h3><span className="dim small">Agent version {a.version}</span></div>
        <div className="field">
          <label htmlFor="sparring-thesis">What should the adversary challenge?</label>
          <textarea id="sparring-thesis" value={thesis} onChange={(e) => setThesis(e.target.value)} maxLength={4000} required rows={4} placeholder="State your claim, evidence, time horizon, and what would prove you wrong." />
        </div>
        <ErrorBanner error={error} />
        <button className="primary" type="submit" disabled={busy || !thesis.trim()}>{busy ? "Creating…" : "Start a practice session"}</button>
      </form>
      <div className="row spread"><h3>Practice sessions</h3><button type="button" className="sm outline" onClick={sessions.reload} disabled={sessions.loading}>{sessions.loading ? "Refreshing…" : "Refresh replies"}</button></div>
      {sessions.data && <ErrorBanner error={sessions.error} />}
      <Async state={sessions.data ? { ...sessions, error: null } : sessions} what="practice sessions" stale>{(list) => list.length === 0 ? <Empty>No sessions yet. Start with a thesis you want to pressure-test.</Empty> : (
        <>
          <div className="row" aria-label="Practice sessions">
            {list.map((s, index) => <button key={s.id} type="button" className={session?.id === s.id ? "sm primary" : "sm outline"} aria-pressed={session?.id === s.id} onClick={() => setSelected(s.id)} title={s.thesis}>Session {list.length - index} · {s.rounds.length} rounds</button>)}
          </div>
          {session && <Session key={session.id} session={session} currentVersion={a.version} onChanged={sessions.reload} />}
        </>
      )}</Async>
    </div>
  );
}

function Session({ session, currentVersion, onChanged }: { session: SparringSession; currentVersion: number; onChanged: () => void }) {
  const [reply, setReply] = useState("");
  const [revision, setRevision] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const prompt = sparringPrompt(session.id);

  async function copyPrompt() {
    try {
      await navigator.clipboard.writeText(prompt);
      setNotice("Prompt copied. Paste it into your configured Grok Bot conversation.");
    } catch {
      setNotice("Clipboard unavailable. Expand the prompt below and copy it manually.");
    }
  }

  async function respond(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await api.replySparring(session.id, reply.trim(), revision.trim() || null);
      setReply("");
      setRevision("");
      setNotice("Your response is saved in the practice record. Ask Grok Bot to review the latest round.");
      onChanged();
    } catch (e) {
      setError(asError(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="stack">
      <div className="card">
        <div className="card-head"><h3>The thesis</h3><span className="dim small">Version {session.agent_version} · {formatTs(session.created_at)}</span></div>
        <p style={{ whiteSpace: "pre-wrap", overflowWrap: "anywhere" }}>{session.thesis}</p>
        <details style={{ marginBottom: 12 }}><summary>Strategy at the start of this session</summary><p style={{ whiteSpace: "pre-wrap", overflowWrap: "anywhere" }}>{session.agent_snapshot.strategy}</p><div className="dim small">Goal: {session.agent_snapshot.goals} · Horizon: {session.agent_snapshot.horizon_secs} seconds</div></details>
        {currentVersion !== session.agent_version && <p className="small pend">This session uses version {session.agent_version}; the agent is now version {currentVersion}.</p>}
        <div className="row"><button type="button" className="outline" onClick={copyPrompt}>Copy Grok Bot prompt</button><span className="dim small">Requires Grok Bot endpoint setup</span></div>
        <details style={{ marginTop: 12 }}><summary>Prompt and session ID</summary><p className="mono small" style={{ whiteSpace: "pre-wrap", overflowWrap: "anywhere" }}>{prompt}</p></details>
      </div>
      {notice && <div className="notice info" role="status">{notice}</div>}
      {session.rounds.length === 0 ? <Empty>Waiting for the adversary. Send the copied prompt to Grok Bot after configuring its TZW endpoint. Replies refresh every 10 seconds while this page is visible.</Empty> : (
        <div className="stack" aria-label="Practice debate">{session.rounds.map((round, index) => <Round key={round.id} round={round} number={index + 1} />)}</div>
      )}
      {session.rounds.length >= 12 ? <div className="notice info">This session reached its 12-round limit. Start a new session to explore a revised thesis.</div> : <form className="card" onSubmit={respond}>
        <div className="card-head"><h3>Your response</h3><span className="badge neutral">Owner</span></div>
        <div className="field"><label htmlFor="sparring-reply">Defend, concede, or clarify</label><textarea id="sparring-reply" value={reply} onChange={(e) => setReply(e.target.value)} maxLength={6000} required rows={3} /></div>
        <div className="field"><label htmlFor="sparring-revision">Revised thesis or terms <span className="dim">(optional)</span></label><textarea id="sparring-revision" value={revision} onChange={(e) => setRevision(e.target.value)} maxLength={4000} rows={3} /><div className="hint">Saved for discussion. Your agent configuration stays as-is.</div></div>
        <ErrorBanner error={error} />
        <button type="submit" className="primary" disabled={busy || !reply.trim()}>{busy ? "Saving…" : "Save response"}</button>
      </form>}
    </div>
  );
}

function Round({ round, number }: { round: SparringRound; number: number }) {
  return (
    <article className="card">
      <div className="card-head"><h3>{round.source === "grok_bot" ? "Grok Bot connection · adversary" : "You · owner"}</h3><span className="dim small">Round {number} · {formatTs(round.created_at)}</span></div>
      <p style={{ whiteSpace: "pre-wrap", overflowWrap: "anywhere" }}>{round.argument}</p>
      {round.proposed_revision && <div className="notice info"><strong>Proposed revision</strong><p style={{ whiteSpace: "pre-wrap", overflowWrap: "anywhere", marginBottom: 0 }}>{round.proposed_revision}</p></div>}
      {round.evidence_urls.length > 0 && <div style={{ marginTop: 12 }}><div className="dim small">Cited sources · not independently verified</div><ul>{round.evidence_urls.map((value, index) => { const url = safeEvidenceUrl(value); return <li key={`${index}:${value}`} style={{ overflowWrap: "anywhere" }}>{url ? <a href={url} target="_blank" rel="noopener noreferrer">{value}</a> : <span className="dim">Invalid source URL</span>}</li>; })}</ul></div>}
    </article>
  );
}

function asError(error: unknown): ApiError {
  return isApiError(error) ? error : new ApiError("client_error", error instanceof Error ? error.message : "Something went wrong", 0);
}
