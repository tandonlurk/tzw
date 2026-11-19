"use client";

import Link from "next/link";
import { useSearchParams } from "next/navigation";
import { useEffect, useMemo, useRef, useState } from "react";
import { api, isApiError, type Agent, type ApiError, type Graph, type GraphError, type TestRun } from "@/lib/api";
import { CODE_BLOCK_TYPE, MAX_SOURCE_BYTES, STARTER_SOURCE, byteCounterText, sourceBytes, sourceProblem } from "@/lib/customCode";
import { nextNodeId } from "@/lib/graph";
import { highlightJs } from "@/lib/jsHighlight";
import { useAsync } from "@/lib/useAsync";
import { ADV } from "@/lib/mode";
import { formatTs } from "@/components/ui";
import { TestRunView } from "@/components/builder/CodeEditor";
import { Icon } from "@/components/shell/icons";

type Files = Record<string, string>; // node id -> source

const codeNodes = (g: Graph) => g.nodes.filter((n) => n.type === CODE_BLOCK_TYPE);
const filesOf = (g: Graph): Files => Object.fromEntries(codeNodes(g).map((n) => [n.id, typeof n.params.source === "string" ? n.params.source : ""]));

/** Code workspace: custom.code blocks of one agent as files, an editor, versions, and a dry-run output panel. */
export function CodeWorkspace({ agent, onSaved }: { agent: Agent; onSaved: (a: Agent) => void }) {
  const focus = useSearchParams().get("node");
  const [graph, setGraph] = useState<Graph>(agent.graph);
  const [saved, setSaved] = useState<Files>(() => filesOf(agent.graph));
  const files = useMemo(() => filesOf(graph), [graph]);
  const ids = Object.keys(files);
  const [current, setCurrent] = useState<string | null>(focus && focus in files ? focus : ids[0] ?? null);
  const [viewing, setViewing] = useState<{ version: number; source: string } | null>(null);
  const [tab, setTab] = useState<"output" | "problems">("output");
  const [run, setRun] = useState<TestRun | null>(null);
  const [runErr, setRunErr] = useState<string | null>(null);
  const [running, setRunning] = useState(false);
  const [saving, setSaving] = useState(false);
  const [saveErr, setSaveErr] = useState<ApiError | null>(null);
  const [graphErrors, setGraphErrors] = useState<GraphError[]>([]);
  const [cursor, setCursor] = useState({ line: 1, col: 1 });
  const versions = useAsync(() => api.listAgentVersions(agent.id), `versions:${agent.id}:${agent.version}`);

  const src = viewing ? viewing.source : current ? files[current] ?? "" : "";
  const dirtyIds = ids.filter((id) => files[id] !== saved[id]);
  const added = ids.filter((id) => !(id in saved));
  const dirty = dirtyIds.length > 0 || added.length > 0;
  const problem = current ? sourceProblem(files[current] ?? "") : null;
  const nodeErrors = graphErrors.filter((e) => !e.node_id || e.node_id === current);
  const problemCount = (problem ? 1 : 0) + nodeErrors.length;

  // Server validation of the whole graph after edits settle.
  const gJson = JSON.stringify(graph);
  useEffect(() => {
    let alive = true;
    const t = window.setTimeout(() => {
      api.validateGraph(JSON.parse(gJson)).then((r) => alive && setGraphErrors(r.errors)).catch(() => {});
    }, 600);
    return () => { alive = false; window.clearTimeout(t); };
  }, [gJson]);

  function setSource(s: string) {
    if (!current || viewing) return;
    setGraph((g) => ({ ...g, nodes: g.nodes.map((n) => (n.id === current ? { ...n, params: { ...n.params, source: s } } : n)) }));
  }

  function addFile() {
    const id = nextNodeId(graph.nodes.map((n) => n.id));
    const ys = graph.nodes.map((n) => n.ui?.y ?? 0);
    setGraph((g) => ({ ...g, nodes: [...g.nodes, { id, type: CODE_BLOCK_TYPE, params: { source: STARTER_SOURCE }, ui: { x: 0, y: (ys.length ? Math.max(...ys) : 0) + 160 } }] }));
    setCurrent(id);
    setViewing(null);
  }

  async function save() {
    setSaving(true);
    setSaveErr(null);
    try {
      const updated = await api.updateAgent(agent.id, { name: agent.name, description: agent.description, config: agent.config, policy: agent.policy, graph });
      setSaved(filesOf(graph));
      onSaved(updated);
    } catch (e) {
      if (isApiError(e)) { setSaveErr(e); setTab("problems"); }
    } finally {
      setSaving(false);
    }
  }

  async function testRun() {
    if (!current || problem) return;
    setRunning(true);
    setRunErr(null);
    setTab("output");
    try {
      setRun(await api.testRunCode(agent.id, files[current]));
    } catch (e) {
      setRun(null);
      setRunErr(isApiError(e) ? (e.unreachable ? e.message : `${e.code}: ${e.message}`) : "Test run failed");
    } finally {
      setRunning(false);
    }
  }

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "s") { e.preventDefault(); if (dirty && !saving) save(); }
      if ((e.metaKey || e.ctrlKey) && e.key === "Enter") { e.preventDefault(); testRun(); }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  if (ids.length === 0) {
    return (
      <div className="cw-empty">
        <div className="glyph-big"><Icon name="code" size={26} /></div>
        <h2 style={{ margin: "12px 0 4px" }}>No custom code yet</h2>
        <p className="dim" style={{ maxWidth: 460, textAlign: "center" }}>
          This agent runs on visual logic only. A code block lets you write <span className="mono">decide(ctx)</span> in JavaScript. It runs in a sandbox with no network or keys, and every stake it returns still goes through your limits.
        </p>
        <div className="row" style={{ marginTop: 12 }}>
          <button type="button" className="primary" onClick={addFile}><Icon name="plus" size={15} />Add a code block</button>
          <Link href={`${ADV}/agents/${agent.id}/logic`} className="btn ghost"><Icon name="logic" size={15} />Back to logic</Link>
        </div>
      </div>
    );
  }

  return (
    <div className="cw">
      <aside className="cw-side">
        <div className="cw-sec">
          <div className="cw-sec-h"><span>Files</span><button type="button" className="icon-btn xs ghost" onClick={addFile} aria-label="New code block" data-tip="New code block"><Icon name="plus" size={13} /></button></div>
          {ids.map((id) => (
            <button key={id} type="button" className={`cw-file${current === id && !viewing ? " on" : ""}`} onClick={() => { setCurrent(id); setViewing(null); }}>
              <Icon name="file" size={14} /><span className="grow">decide.js</span><span className="dim xs mono">{id}</span>
              {(dirtyIds.includes(id) || added.includes(id)) && <span className="dot-dirty" aria-label="unsaved" />}
            </button>
          ))}
          {added.length > 0 && <div className="dim xs" style={{ padding: "6px 10px" }}>New blocks are not connected yet. Wire them up in <Link href={`${ADV}/agents/${agent.id}/logic`}>Logic</Link> after saving.</div>}
        </div>
        <div className="cw-sec grow-sec">
          <div className="cw-sec-h"><span>Versions</span></div>
          {(versions.data ?? []).slice().reverse().map((v) => {
            const s = current ? v.graph.nodes.find((n) => n.id === current)?.params.source : undefined;
            const has = typeof s === "string";
            return (
              <button key={v.version} type="button" disabled={!has} className={`cw-file${viewing?.version === v.version ? " on" : ""}`}
                onClick={() => has && setViewing({ version: v.version, source: s as string })} title={has ? "View this block's code at this version" : "This block did not exist in this version"}>
                <Icon name="history" size={14} /><span className="grow">v{v.version}{v.version === agent.version ? " · current" : ""}</span><span className="dim xs">{formatTs(v.created_at).split(",")[0]}</span>
              </button>
            );
          })}
        </div>
        <div className="cw-note">Sandboxed: no network, files or keys. 64 KiB max. Limits are enforced by the server, not by this code.</div>
      </aside>

      <section className="cw-main">
        <div className="cw-tabs">
          <div className="cw-tab on">
            <Icon name="file" size={13} />{viewing ? `decide.js @ v${viewing.version}` : "decide.js"}
            {current && <span className="dim xs mono">{current}</span>}
            {current && !viewing && (dirtyIds.includes(current) || added.includes(current)) && <span className="dot-dirty" />}
            {viewing && <span className="badge info plain">read-only</span>}
          </div>
          <span className="grow" />
          {viewing ? (
            <span className="row nowrap">
              <button type="button" className="xs ghost" onClick={() => setViewing(null)}>Back to draft</button>
              <button type="button" className="xs outline" onClick={() => { const s = viewing.source; setViewing(null); setSource(s); }}>Restore into draft</button>
            </span>
          ) : (
            <span className="row nowrap">
              {current && <Link href={`${ADV}/agents/${agent.id}/logic?node=${current}`} className="btn xs ghost"><Icon name="logic" size={13} />Show in logic</Link>}
              <button type="button" className="xs outline" onClick={testRun} disabled={running || !!problem} title="Dry run against this agent's real balances (⌘↵)"><Icon name="play" size={12} />{running ? "Running…" : "Test run"}</button>
              {dirty && <button type="button" className="xs ghost" onClick={() => { setGraph((g) => ({ ...g, nodes: g.nodes.filter((n) => !added.includes(n.id)).map((n) => (n.id in saved ? { ...n, params: { ...n.params, source: saved[n.id] } } : n)) })); if (current && added.includes(current)) setCurrent(Object.keys(saved)[0] ?? null); }}>Discard</button>}
              <button type="button" className="xs primary" onClick={save} disabled={!dirty || saving} title="⌘S"><Icon name="save" size={12} />{saving ? "Saving…" : `Save as v${agent.version + 1}`}</button>
            </span>
          )}
        </div>
        <Editor value={src} onChange={setSource} readOnly={!!viewing} onCursor={setCursor} />
        <div className="cw-status">
          <span className={sourceBytes(src) > MAX_SOURCE_BYTES ? "neg" : ""}>{byteCounterText(sourceBytes(src))}</span>
          <span>Ln {cursor.line}, Col {cursor.col}</span>
          <span>JavaScript</span>
          <span className="grow" />
          <button type="button" className={`cw-st-btn${problemCount ? " neg" : " pos"}`} onClick={() => setTab("problems")}>
            <Icon name={problemCount ? "alert" : "check"} size={12} />{problemCount ? `${problemCount} problem${problemCount === 1 ? "" : "s"}` : "No problems"}
          </button>
          <span>{dirty ? <span className="pend">Unsaved</span> : `Saved · v${agent.version}`}</span>
        </div>
      </section>

      <section className="cw-out">
        <div className="cw-out-tabs">
          <button type="button" className={tab === "output" ? "on" : ""} onClick={() => setTab("output")}><Icon name="terminal-window" size={13} />Output</button>
          <button type="button" className={tab === "problems" ? "on" : ""} onClick={() => setTab("problems")}>Problems{problemCount > 0 && <span className="cnt">{problemCount}</span>}</button>
        </div>
        <div className="cw-out-body">
          {tab === "output" && (
            runErr ? <div className="neg small mono">{runErr}</div>
              : run ? <TestRunView r={run} />
              : <div className="dim small">Press <kbd>Test run</kbd> or <kbd>⌘</kbd><kbd>↵</kbd> to dry-run this code against the agent&apos;s current balances. Nothing is written: no ledger entry, challenge or approval.</div>
          )}
          {tab === "problems" && (
            <div>
              {saveErr && <div className="neg small" style={{ marginBottom: 6 }}>Save rejected: {saveErr.code}: {saveErr.message}</div>}
              {problem && <div className="cw-prob"><Icon name="alert" size={13} />{problem}</div>}
              {nodeErrors.map((e, i) => <div key={i} className="cw-prob"><Icon name="alert" size={13} />{e.node_id ? <span className="mono">{e.node_id}</span> : null} {e.message || e.code}</div>)}
              {problemCount === 0 && !saveErr && <div className="dim small">No problems found. The server accepts this logic.</div>}
            </div>
          )}
        </div>
      </section>
    </div>
  );
}

/** Textarea with line-number gutter and a highlighted overlay. Tab inserts two spaces. */
function Editor({ value, onChange, readOnly, onCursor }: { value: string; onChange: (s: string) => void; readOnly: boolean; onCursor: (c: { line: number; col: number }) => void }) {
  const ta = useRef<HTMLTextAreaElement>(null);
  const hl = useRef<HTMLPreElement>(null);
  const gutter = useRef<HTMLDivElement>(null);
  const lines = value.split("\n").length;
  const html = useMemo(() => highlightJs(value), [value]);

  const sync = () => {
    const t = ta.current;
    if (!t) return;
    if (hl.current) { hl.current.scrollTop = t.scrollTop; hl.current.scrollLeft = t.scrollLeft; }
    if (gutter.current) gutter.current.scrollTop = t.scrollTop;
  };
  const report = () => {
    const t = ta.current;
    if (!t) return;
    const before = t.value.slice(0, t.selectionStart).split("\n");
    onCursor({ line: before.length, col: before[before.length - 1].length + 1 });
  };

  return (
    <div className={`cw-editor${readOnly ? " ro" : ""}`}>
      <div className="cw-gutter" ref={gutter} aria-hidden>{Array.from({ length: lines }, (_, i) => <div key={i}>{i + 1}</div>)}</div>
      <div className="cw-code">
        <pre className="cw-hl" ref={hl} aria-hidden dangerouslySetInnerHTML={{ __html: html }} />
        <textarea
          ref={ta}
          value={value}
          readOnly={readOnly}
          spellCheck={false}
          autoCapitalize="off"
          autoCorrect="off"
          aria-label="Custom code source"
          onChange={(e) => onChange(e.target.value)}
          onScroll={sync}
          onKeyUp={report}
          onClick={report}
          onKeyDown={(e) => {
            if (e.key === "Tab" && !readOnly) {
              e.preventDefault();
              const t = e.currentTarget;
              const s = t.selectionStart;
              const next = `${t.value.slice(0, s)}  ${t.value.slice(t.selectionEnd)}`;
              onChange(next);
              requestAnimationFrame(() => { t.selectionStart = t.selectionEnd = s + 2; });
            }
          }}
        />
      </div>
    </div>
  );
}
