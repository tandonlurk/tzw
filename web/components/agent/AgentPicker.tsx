"use client";

import Link from "next/link";
import { useRouter } from "next/navigation";
import { useEffect } from "react";
import { api } from "@/lib/api";
import { useAsync } from "@/lib/useAsync";
import { agentStyles, shortName } from "@/lib/agentColors";
import { ADV } from "@/lib/mode";
import { Async, Avatar, Empty, EnvBadge } from "@/components/ui";
import { useMode } from "@/components/shell/AppShell";

/** /advanced/logic and /advanced/code: open the last agent's canvas or editor, else let the owner pick one. */
export function AgentPicker({ sub, title, blurb }: { sub: "logic" | "code"; title: string; blurb: string }) {
  const { lastAgentId } = useMode();
  const router = useRouter();
  const agents = useAsync(api.listAgents, "agents");
  const known = lastAgentId && agents.data?.some((a) => a.id === lastAgentId);
  useEffect(() => { if (known) router.replace(`${ADV}/agents/${lastAgentId}/${sub}`); }, [known, lastAgentId, router, sub]);
  if (known) return null;
  return (
    <div>
      <div className="t-head"><h1 style={{ margin: 0 }}>{title}</h1></div>
      <p className="dim" style={{ marginTop: -6, marginBottom: 16, maxWidth: 640 }}>{blurb}</p>
      <Async state={agents} what="agents">
        {(list) => {
          const st = agentStyles(list.map((a) => a.id));
          return list.length === 0 ? <Empty>No agents yet. <Link href="/agents/new">Create one</Link>.</Empty> : (
            <div className="agent-grid">
              {list.map((a) => (
                <Link key={a.id} href={`${ADV}/agents/${a.id}/${sub}`} className="agent-card" style={{ gap: 10, padding: 16 }}>
                  <div className="top">
                    <Avatar name={a.name} color={st.get(a.id)!.color} size={32} />
                    <div className="grow"><div className="name">{shortName(a.name)}</div><div className="dim xs">{a.graph.nodes.length} blocks · {a.graph.nodes.filter((n) => n.type === "custom.code").length} code</div></div>
                    <EnvBadge env={a.env} />
                  </div>
                </Link>
              ))}
            </div>
          );
        }}
      </Async>
    </div>
  );
}
