"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import { api, isApiError, type ApiError, type AgentInput, type Template } from "@/lib/api";
import { clearPositions, loadPositions, savePositions } from "@/lib/graph";
import { useAsync } from "@/lib/useAsync";
import { Async, Empty } from "@/components/ui";
import { AgentForm } from "@/components/AgentForm";

export default function NewAgentPage() {
  const templates = useAsync(api.listTemplates, "templates");
  const blocks = useAsync(api.listBlocks, "blocks");
  const [picked, setPicked] = useState<Template | null>(null);
  const [error, setError] = useState<ApiError | null>(null);
  const router = useRouter();

  function pick(t: Template | null) {
    clearPositions("new"); // a different starting graph gets a fresh auto layout
    setError(null);
    setPicked(t);
  }

  async function create(input: AgentInput) {
    setError(null);
    try {
      const agent = await api.createAgent(input);
      savePositions(agent.id, loadPositions("new"));
      clearPositions("new");
      router.push(`/agents/${agent.id}`);
    } catch (e) {
      if (isApiError(e)) setError(e);
    }
  }

  return (
    <div>
      <div className="page-head"><div><h1>Create agent</h1></div></div>
      <Async state={templates} what="templates">
        {(tpls) =>
          tpls.length === 0 ? (
            <Empty>The backend has no templates to start from.</Empty>
          ) : !picked ? (
            <div>
              <div className="dim" style={{ marginBottom: ".75rem" }}>Pick a template. You can change everything afterwards.</div>
              <div className="grid">
                {tpls.map((t) => (
                  <div key={t.id} className="card stack">
                    <strong>{t.name}</strong>
                    <div className="dim">{t.description}</div>
                    <button className="primary" onClick={() => pick(t)}>Use this template</button>
                  </div>
                ))}
              </div>
            </div>
          ) : (
            <Async state={blocks} what="block catalog">
              {(defs) => (
                <div>
                  <div className="row" style={{ marginBottom: ".75rem" }}>
                    <span>Template: <strong>{picked.name}</strong></span>
                    <button className="sm" onClick={() => pick(null)}>Change template</button>
                  </div>
                  <AgentForm
                    key={picked.id}
                    blocks={defs}
                    initial={{ name: "", description: "", config: picked.config, policy: picked.policy, graph: picked.graph }}
                    storageId="new"
                    submitLabel="Create agent"
                    onSubmit={create}
                    serverError={error}
                  />
                </div>
              )}
            </Async>
          )
        }
      </Async>
    </div>
  );
}
