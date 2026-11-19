"use client";

import { AgentPicker } from "@/components/agent/AgentPicker";

export default function CodeIndex() {
  return <AgentPicker sub="code" title="Code" blurb="Custom JavaScript blocks, per agent. Pick an agent to open its code workspace." />;
}
