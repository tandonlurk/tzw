"use client";

import { ConfigTab } from "@/components/agent/ConfigTab";
import { useWorkspace } from "@/components/agent/AgentWorkspace";

export default function EditAgentPage() {
  const { agent } = useWorkspace();
  return <ConfigTab doneHref={(v) => `/agents/${agent.data!.id}?saved=${v}`} />;
}
