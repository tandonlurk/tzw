"use client";

import { ConfigTab } from "@/components/agent/ConfigTab";
import { useWorkspace } from "@/components/agent/AgentWorkspace";

export default function AdvancedSettingsPage() {
  const { agent } = useWorkspace();
  return <ConfigTab doneHref={(v) => `/advanced/agents/${agent.data!.id}?saved=${v}`} />;
}
