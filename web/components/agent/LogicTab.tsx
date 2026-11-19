"use client";

import { api } from "@/lib/api";
import { useAsync } from "@/lib/useAsync";
import { Async } from "@/components/ui";
import { LogicCanvas } from "@/components/canvas/LogicCanvas";
import { useWorkspace } from "./AgentWorkspace";

/** The agent's own logic canvas (one canvas per agent). */
export function LogicTab({ codeHref }: { codeHref?: (agentId: string, nodeId: string) => string }) {
  const { agent, refresh } = useWorkspace();
  const a = agent.data!;
  const blocks = useAsync(api.listBlocks, "blocks");
  return (
    <div className="logic-host">
      <Async state={blocks} what="block catalog">
        {(defs) => <LogicCanvas key={a.id} agent={a} blocks={defs} onSaved={() => refresh()} codeHref={codeHref ? (n) => codeHref(a.id, n) : undefined} />}
      </Async>
    </div>
  );
}
