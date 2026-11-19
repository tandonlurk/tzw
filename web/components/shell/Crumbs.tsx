"use client";

import Link from "next/link";
import { useEffect, useState } from "react";
import { api } from "@/lib/api";
import { shortName } from "@/lib/agentColors";
import { ADV, agentIdOfPath } from "@/lib/mode";
import { Icon } from "./icons";

const LABEL: Record<string, string> = {
  agents: "Agents", wallet: "Wallet", activity: "Activity", interactions: "Challenges", firms: "Firms",
  collaborations: "Collaborations", approvals: "Approvals", cocaptain: "Co-captain", settings: "Settings",
  new: "New", edit: "Configuration", logic: "Logic", code: "Code", performance: "Performance", logic_: "Logic",
};

const names = new Map<string, string>();

/** Agent name for the breadcrumb; cached per page load so tab switches do not refetch. */
function useAgentName(id: string | null): string | null {
  const [, force] = useState(0);
  useEffect(() => {
    if (!id || names.has(id)) return;
    let alive = true;
    api.getAgent(id).then((a) => { names.set(id, shortName(a.name)); if (alive) force((n) => n + 1); }).catch(() => {});
    return () => { alive = false; };
  }, [id]);
  return id ? names.get(id) ?? null : null;
}

export function Crumbs({ path }: { path: string }) {
  const adv = path === ADV || path.startsWith(`${ADV}/`);
  const rest = adv ? path.slice(ADV.length) : path;
  const segs = rest.split("/").filter(Boolean);
  const agentId = agentIdOfPath(path);
  const agentName = useAgentName(agentId);

  const parts: { href: string; label: string }[] = [];
  let acc = adv ? ADV : "";
  segs.forEach((s, i) => {
    acc += `/${s}`;
    let label = LABEL[s] ?? s;
    if (segs[0] === "agents" && i === 1 && s !== "new") label = agentName ?? "Agent";
    else if (i === 1 && /^[0-9a-f-]{16,}$/i.test(s)) label = "Details";
    else if (adv && segs[0] === "activity") label = "History";
    else if (adv && segs[0] === "settings" && i === 2) label = "Settings";
    parts.push({ href: acc, label });
  });
  if (adv && segs[0] === "agents" && segs.length === 3 && segs[2] === "settings") parts[2].label = "Settings";

  const home = adv ? { href: ADV, label: "Terminal" } : { href: "/", label: "Overview" };
  const all = parts.length ? [home, ...parts] : [home];

  return (
    <nav className="crumb" aria-label="Breadcrumb">
      {all.map((p, i) => (
        <span key={p.href} className="row nowrap" style={{ gap: 6 }}>
          {i > 0 && <Icon name="chevron-right" size={12} />}
          {i === all.length - 1 ? <span className="here" aria-current="page">{p.label}</span> : <Link href={p.href}>{p.label}</Link>}
        </span>
      ))}
    </nav>
  );
}
