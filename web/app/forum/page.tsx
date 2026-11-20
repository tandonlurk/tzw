"use client";

import Link from "next/link";
import { useState, type FormEvent } from "react";
import { ApiError, api, isApiError, type Agent } from "@/lib/api";
import { FORUM_CATEGORIES, type ForumCategory, type ForumPost, type ForumReply } from "@/lib/forum";
import { useAsync } from "@/lib/useAsync";
import { Async, Empty, ErrorBanner, PageHead, formatTs } from "@/components/ui";

export default function ForumPage() {
  const posts = useAsync(api.listForumPosts, "forum-posts");
  const agents = useAsync(api.listAgents, "forum-agents");
  const [filter, setFilter] = useState<ForumCategory | "all">("all");
  const [composing, setComposing] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);

  return (
    <div className="stack">
      <PageHead title="Public Forum">
        <button className="outline sm" onClick={posts.reload} disabled={posts.loading}>Refresh</button>
        <button className="primary" onClick={() => setComposing((v) => !v)}>{composing ? "Close composer" : "Start a discussion"}</button>
      </PageHead>
      <div className="row spread">
        <div className="row">
          <button className={`chip${filter === "all" ? " on" : ""}`} onClick={() => setFilter("all")}>All threads</button>
          {FORUM_CATEGORIES.map((c) => <button key={c.value} className={`chip${filter === c.value ? " on" : ""}`} onClick={() => setFilter(c.value)}>{c.label}</button>)}
        </div>
      </div>
      {notice && <div className="notice info" role="status">{notice}</div>}
      {composing && <Composer agents={agents.data ?? []} agentsError={agents.error} onSaved={() => { posts.reload(); setComposing(false); setNotice("Your thread is saved."); }} />}
      <ErrorBanner error={posts.data ? posts.error : null} />
      <Async state={posts.data ? { ...posts, error: null } : posts} what="forum threads" stale>{(list) => {
        const shown = list.filter((p) => filter === "all" || p.category === filter);
        return shown.length === 0 ? <Empty>{list.length === 0 ? "No discussions yet. Put the first thesis on the table." : "No threads in this category yet."}</Empty> : <div className="stack">{shown.map((post) => <Thread key={post.id} post={post} agents={agents.data ?? []} onChanged={posts.reload} />)}</div>;
      }}</Async>
    </div>
  );
}

function Identity({ value, onChange, agents, id }: { value: string; onChange: (v: string) => void; agents: Agent[]; id: string }) {
  return <div className="field"><label htmlFor={id}>Post as</label><select id={id} value={value} onChange={(e) => onChange(e.target.value)}><option value="">You · owner</option>{agents.map((a) => <option key={a.id} value={a.id}>You on behalf of {a.name}</option>)}</select></div>;
}

function Composer({ agents, agentsError, onSaved }: { agents: Agent[]; agentsError: ApiError | null; onSaved: () => void }) {
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");
  const [category, setCategory] = useState<ForumCategory>("discussion");
  const [identity, setIdentity] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault(); setBusy(true); setError(null);
    try { await api.createForumPost({ title: title.trim(), body: body.trim(), category, agent_id: identity || null }); onSaved(); }
    catch (e) { setError(asError(e)); }
    finally { setBusy(false); }
  }

  return <form className="card stack" onSubmit={submit}>
    <div className="card-head"><h3>Start a discussion</h3><span className="badge neutral">Owner post</span></div>
    <div className="field"><label htmlFor="forum-title">Title</label><input id="forum-title" value={title} onChange={(e) => setTitle(e.target.value)} maxLength={160} required placeholder="What should the network explore?" /></div>
    <div className="grid-2"><Identity id="forum-identity" value={identity} onChange={setIdentity} agents={agents} /><div className="field"><label htmlFor="forum-category">Category</label><select id="forum-category" value={category} onChange={(e) => setCategory(e.target.value as ForumCategory)}>{FORUM_CATEGORIES.map((c) => <option key={c.value} value={c.value}>{c.label}</option>)}</select></div></div>
    {agentsError && <div className="dim small">Agent identities could not load. You can still post as the owner.</div>}
    <div className="field"><label htmlFor="forum-body">Your post</label><textarea id="forum-body" value={body} onChange={(e) => setBody(e.target.value)} maxLength={4000} rows={5} required placeholder="Explain the claim, share your evidence, or describe the collaborator you need." /></div>
    <ErrorBanner error={error} /><div><button type="submit" className="primary" disabled={busy || !title.trim() || !body.trim()}>{busy ? "Publishing…" : "Publish thread"}</button></div>
  </form>;
}

function Attribution({ item }: { item: ForumReply }) {
  return <div className="row small dim"><span>{item.agent_name ? `${item.is_demo ? "Sample agent" : "Owner on behalf of"} · ${item.agent_name}` : item.is_demo ? "Sample participant" : "You · owner"}</span><span>· {formatTs(item.created_at)}</span></div>;
}

function Thread({ post, agents, onChanged }: { post: ForumPost; agents: Agent[]; onChanged: () => void }) {
  const [reply, setReply] = useState("");
  const [identity, setIdentity] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  const category = FORUM_CATEGORIES.find((c) => c.value === post.category)?.label ?? "Discussion";

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault(); setBusy(true); setError(null);
    try { await api.replyForumPost(post.id, { body: reply.trim(), agent_id: identity || null }); setReply(""); onChanged(); }
    catch (e) { setError(asError(e)); }
    finally { setBusy(false); }
  }

  return <article className="card stack">
    <div className="row spread"><div className="row"><span className="badge info">{category}</span>{post.is_demo && <span className="badge neutral">Sample</span>}</div><span className="small dim">{post.replies.length} {post.replies.length === 1 ? "reply" : "replies"}</span></div>
    <div><h2 style={{ fontSize: "1.2rem", marginBottom: 8, overflowWrap: "anywhere" }}>{post.title}</h2><Attribution item={post} /></div>
    <p style={{ whiteSpace: "pre-wrap", overflowWrap: "anywhere", margin: 0 }}>{post.body}</p>
    <details><summary>{post.replies.length ? "Read discussion and reply" : "Join the discussion"}</summary><div className="stack" style={{ marginTop: 16 }}>
      {post.replies.map((r) => <div key={r.id} style={{ borderLeft: "2px solid var(--border)", paddingLeft: 16 }}><Attribution item={r} />{r.is_demo && <span className="badge neutral small">Sample</span>}<p style={{ whiteSpace: "pre-wrap", overflowWrap: "anywhere" }}>{r.body}</p></div>)}
      <form className="stack" onSubmit={submit}><Identity id={`reply-identity-${post.id}`} value={identity} onChange={setIdentity} agents={agents} /><div className="field"><label htmlFor={`reply-${post.id}`}>Your reply</label><textarea id={`reply-${post.id}`} value={reply} onChange={(e) => setReply(e.target.value)} maxLength={2000} rows={3} required /></div><ErrorBanner error={error} /><div><button type="submit" className="outline sm" disabled={busy || !reply.trim()}>{busy ? "Saving…" : "Post reply"}</button></div></form>
    </div></details>
  </article>;
}

function asError(e: unknown): ApiError {
  return isApiError(e) ? e : new ApiError("client_error", e instanceof Error ? e.message : "Could not save your post", 0);
}
