"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import { api, isApiError, type ApiError, type AgentInput } from "@/lib/api";
import { clearPositions, emptyGraph } from "@/lib/graph";
import { useAsync } from "@/lib/useAsync";
import { Async, PageHead } from "@/components/ui";
import { AgentForm } from "@/components/AgentForm";

const BLANK: AgentInput = {
  name: "",
  description: "",
  config: { goals: "", strategy: "", categories: [], horizon_secs: 3600, risk_tolerance: "medium", schedule_secs: 300 },
  policy: { max_per_interaction: "5000000", max_aggregate_exposure: "20000000", approval_threshold: "2000000", allowed_interaction_types: ["challenge"], min_conviction_bps: 6000, stop_loss: null },
  graph: emptyGraph(),
};

export default function NewAgentPage() {
  const templates = useAsync(api.listTemplates, "templates");
  const [error, setError] = useState<ApiError | null>(null);
  const router = useRouter();

  async function create(input: AgentInput) {
    setError(null);
    try {
      const agent = await api.createAgent(input);
      clearPositions("new");
      router.push(`/agents/${agent.id}`);
    } catch (e) {
      if (isApiError(e)) setError(e);
    }
  }

  return (
    <div>
      <PageHead eyebrow="New agent" title="Create an agent" sub="A few short steps. Limits you set here are enforced by the server, not by the agent." />
      <Async state={templates} what="templates">
        {(tpls) => (
          <AgentForm
            variant="wizard"
            templates={tpls}
            initial={tpls[0] ? { ...BLANK, description: tpls[0].description, config: tpls[0].config, policy: tpls[0].policy, graph: tpls[0].graph } : BLANK}
            submitLabel="Create agent"
            onSubmit={create}
            serverError={error}
            onCancel={() => router.push("/agents")}
          />
        )}
      </Async>
    </div>
  );
}
