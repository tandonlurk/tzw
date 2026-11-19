"use client";

import { use } from "react";
import { AgentWorkspace } from "@/components/agent/AgentWorkspace";

export default function AdvancedAgentLayout({ params, children }: { params: Promise<{ id: string }>; children: React.ReactNode }) {
  const { id } = use(params);
  return <AgentWorkspace id={id} advanced>{children}</AgentWorkspace>;
}
