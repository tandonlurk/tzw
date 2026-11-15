"use client";

import { use, useState } from "react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { api, isApiError, type AgentInput, type ApiError } from "@/lib/api";
import { useAsync } from "@/lib/useAsync";
import { Async } from "@/components/ui";
import { AgentForm } from "@/components/AgentForm";

export default function EditAgentPage({ params }: { params: Promise<{ id: string }> }) {
  const { id } = use(params);
  const agent = useAsync(() => api.getAgent(id), `agent:${id}`);
  const blocks = useAsync(api.listBlocks, "blocks");
  const [error, setError] = useState<ApiError | null>(null);
  const router = useRouter();

  async function save(input: AgentInput) {
    setError(null);
    try {
      await api.updateAgent(id, input);
      router.push(`/agents/${id}`);
    } catch (e) {
      if (isApiError(e)) setError(e);
    }
  }

  return (
    <div>
      <div className="page-head">
        <div>
          <h1>Edit agent</h1>
          <div className="dim">Saving creates a new version. Past activity and ledger history are unchanged.</div>
        </div>
        <Link className="btn" href={`/agents/${id}`}>Cancel</Link>
      </div>
      <Async state={agent} what="agent">
        {(a) => (
          <Async state={blocks} what="block catalog">
            {(defs) => (
              <AgentForm
                blocks={defs}
                initial={{ name: a.name, description: a.description, config: a.config, policy: a.policy, graph: a.graph }}
                storageId={id}
                submitLabel={`Save as version ${a.version + 1}`}
                onSubmit={save}
                serverError={error}
              />
            )}
          </Async>
        )}
      </Async>
    </div>
  );
}
