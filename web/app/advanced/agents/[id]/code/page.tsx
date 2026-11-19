"use client";

import { useWorkspace } from "@/components/agent/AgentWorkspace";
import { CodeWorkspace } from "@/components/code/CodeWorkspace";

export default function AgentCodePage() {
  const { agent, refresh } = useWorkspace();
  return (
    <div className="logic-host">
      <CodeWorkspace key={agent.data!.id} agent={agent.data!} onSaved={() => refresh()} />
    </div>
  );
}
