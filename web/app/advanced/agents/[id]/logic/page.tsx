"use client";

import { LogicTab } from "@/components/agent/LogicTab";

export default function AdvancedLogicPage() {
  return <LogicTab codeHref={(id, node) => `/advanced/agents/${id}/code?node=${node}`} />;
}
