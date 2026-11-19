"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import { api, isApiError, type AgentInput, type ApiError } from "@/lib/api";
import { AgentForm } from "@/components/AgentForm";
import { useWorkspace } from "./AgentWorkspace";

/** Configuration tab (both modes). Saving creates a new version; the graph is kept as-is (edit it in Logic). */
export function ConfigTab({ doneHref }: { doneHref: (version: number) => string }) {
  const { agent, refresh } = useWorkspace();
  const a = agent.data!;
  const [error, setError] = useState<ApiError | null>(null);
  const router = useRouter();

  async function save(input: AgentInput) {
    setError(null);
    try {
      const updated = await api.updateAgent(a.id, input);
      refresh();
      router.push(doneHref(updated.version));
    } catch (e) {
      if (isApiError(e)) setError(e);
    }
  }

  return (
    <AgentForm
      key={a.version}
      variant="sections"
      initial={{ name: a.name, description: a.description, config: a.config, policy: a.policy, graph: a.graph }}
      submitLabel={`Save as version ${a.version + 1}`}
      onSubmit={save}
      serverError={error}
    />
  );
}
