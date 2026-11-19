"use client";

import Link from "next/link";
import { usePathname, useRouter } from "next/navigation";
import { createContext, useCallback, useContext, useEffect, useState, type ReactNode } from "react";
import { api } from "@/lib/api";
import { ADV, MODE_COOKIE, agentIdOfPath, counterpartPath, modeOfPath, type Mode } from "@/lib/mode";
import { Icon, type IconName } from "./icons";
import { Crumbs } from "./Crumbs";

interface ModeCtx { mode: Mode; switchMode: () => void; lastAgentId: string | null }
const ModeContext = createContext<ModeCtx>({ mode: "basic", switchMode: () => {}, lastAgentId: null });
export const useMode = () => useContext(ModeContext);

const LAST_AGENT_KEY = "tzw.lastAgent";

interface Item { href: string; label: string; icon: IconName; match?: (p: string) => boolean; count?: boolean }
interface Group { label: string; items: Item[] }

const starts = (...pre: string[]) => (p: string) => pre.some((x) => p === x || p.startsWith(`${x}/`));
const advSub = (sub: string) => (p: string) => p === `${ADV}/${sub}` || new RegExp(`^${ADV}/agents/[^/]+/${sub}$`).test(p);

const BASIC: Group[] = [
  {
    label: "Workspace",
    items: [
      { href: "/", label: "Overview", icon: "overview", match: (p) => p === "/" },
      { href: "/agents", label: "My Agents", icon: "agents", match: starts("/agents") },
      { href: "/wallet", label: "Wallet", icon: "wallet", match: starts("/wallet") },
      { href: "/activity", label: "Activity", icon: "activity", match: starts("/activity") },
    ],
  },
  {
    label: "Network",
    items: [
      { href: "/interactions", label: "Challenges", icon: "challenge", match: starts("/interactions") },
      { href: "/firms", label: "Firms", icon: "firm", match: starts("/firms", "/collaborations") },
    ],
  },
  {
    label: "Assist",
    items: [
      { href: "/approvals", label: "Approvals", icon: "approval", match: starts("/approvals"), count: true },
      { href: "/cocaptain", label: "Co-captain", icon: "cocaptain", match: starts("/cocaptain") },
    ],
  },
];

const ADVANCED: Group[] = [
  {
    label: "Workspace",
    items: [
      { href: ADV, label: "Terminal", icon: "terminal", match: (p) => p === ADV },
      { href: `${ADV}/agents`, label: "Agents", icon: "agents", match: (p) => starts(`${ADV}/agents`)(p) && !advSub("logic")(p) && !advSub("code")(p) },
      { href: `${ADV}/logic`, label: "Logic", icon: "logic", match: advSub("logic") },
      { href: `${ADV}/code`, label: "Code", icon: "code", match: advSub("code") },
      { href: `${ADV}/activity`, label: "History", icon: "history", match: starts(`${ADV}/activity`) },
    ],
  },
  {
    label: "Network",
    items: [
      { href: "/interactions", label: "Deals", icon: "challenge", match: starts("/interactions") },
      { href: "/firms", label: "Firms", icon: "firm", match: starts("/firms", "/collaborations") },
      { href: "/approvals", label: "Approve", icon: "approval", match: starts("/approvals"), count: true },
    ],
  },
  {
    label: "Account",
    items: [
      { href: "/wallet", label: "Wallet", icon: "wallet", match: starts("/wallet") },
      { href: "/cocaptain", label: "Co-pilot", icon: "cocaptain", match: starts("/cocaptain") },
    ],
  },
];

function writeModeCookie(mode: Mode) {
  document.cookie = `${MODE_COOKIE}=${mode}; path=/; max-age=31536000; samesite=lax`;
}

/** Pending approvals for the sidebar count. Polls gently; failures just hide the count. */
function usePendingApprovals(path: string): number {
  const [n, setN] = useState(0);
  useEffect(() => {
    let alive = true;
    const load = () => api.listApprovals("pending").then((a) => alive && setN(a.length)).catch(() => alive && setN(0));
    load();
    const t = window.setInterval(load, 15_000);
    return () => { alive = false; window.clearInterval(t); };
  }, [path]);
  return n;
}

export function AppShell({ initialMode, children }: { initialMode: Mode; children: ReactNode }) {
  const path = usePathname() ?? "/";
  const router = useRouter();
  const forced = modeOfPath(path);
  const [pref, setPref] = useState<Mode>(initialMode);
  // A mode-specific page becomes the preference, so shared pages (wallet, firms...) keep the last mode.
  if (forced && forced !== pref) setPref(forced);
  const mode = forced ?? pref;

  const pathAgent = agentIdOfPath(path);
  const [storedAgent, setStoredAgent] = useState<string | null>(null);
  useEffect(() => {
    if (pathAgent) window.localStorage.setItem(LAST_AGENT_KEY, pathAgent);
    // Reading browser storage after mount is the point of this effect.
    // eslint-disable-next-line react-hooks/set-state-in-effect
    else setStoredAgent(window.localStorage.getItem(LAST_AGENT_KEY));
  }, [pathAgent]);
  const lastAgentId = pathAgent ?? storedAgent;

  useEffect(() => { writeModeCookie(mode); document.documentElement.dataset.mode = mode; }, [mode]);
  useEffect(() => {
    document.documentElement.dataset.reduceMotion = window.localStorage.getItem("tzw.reduceMotion") === "1" ? "true" : "false";
  }, []);

  const switchMode = useCallback(() => {
    const target: Mode = mode === "basic" ? "advanced" : "basic";
    setPref(target);
    writeModeCookie(target);
    const next = counterpartPath(path, target);
    if (next !== path) router.push(next);
  }, [mode, path, router]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === "\\") { e.preventDefault(); switchMode(); }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [switchMode]);

  const pending = usePendingApprovals(path);
  const groups = mode === "basic" ? BASIC : ADVANCED;
  const fullBleed = /\/(logic|code)$/.test(path);

  return (
    <ModeContext.Provider value={{ mode, switchMode, lastAgentId }}>
      <div className="app" data-mode={mode}>
        <aside className="sidebar" aria-label="Primary">
          <div className="sidebar-top">
            <Link href={mode === "basic" ? "/" : ADV} className="wordmark" aria-label="TZW home">
              <span>TZW</span><span className="tick" aria-hidden />
            </Link>
            {mode === "basic" && <span className="mode-tag" style={{ marginLeft: "auto" }}>Basic</span>}
          </div>
          <nav className="sidebar-nav" aria-label="Main">
            {groups.map((g) => (
              <div className="nav-group" key={g.label}>
                <div className="nav-group-label">{g.label}</div>
                {g.items.map((i) => {
                  const active = i.match ? i.match(path) : path === i.href;
                  return (
                    <Link key={i.href} href={i.href} className={`nav-item${active ? " active" : ""}`} aria-current={active ? "page" : undefined} data-tip={i.label}>
                      <Icon name={i.icon} size={mode === "advanced" ? 18 : 17} />
                      <span className="label">{i.label}</span>
                      {i.count && pending > 0 && <span className="count" aria-label={`${pending} pending`}>{pending}</span>}
                    </Link>
                  );
                })}
              </div>
            ))}
          </nav>
          <div className="sidebar-foot">
            <Link href="/settings" className={`nav-item${path.startsWith("/settings") ? " active" : ""}`} data-tip="Settings">
              <Icon name="settings" size={mode === "advanced" ? 18 : 17} />
              <span className="label">Settings</span>
            </Link>
            <div className="env-note" title="Balances are simulated or Arc testnet tokens. No real funds.">
              <span className="dotc" aria-hidden />
              <span>Simulated and Arc testnet funds only. Nothing here is real money.</span>
            </div>
          </div>
        </aside>
        <div className="app-main">
          <header className="topbar">
            <Crumbs path={path} />
            <div className="topbar-actions">
              {pending > 0 && !path.startsWith("/approvals") && (
                <Link href="/approvals" className="attn"><span className="pulse" aria-hidden />{pending} pending approval{pending === 1 ? "" : "s"}</Link>
              )}
              <button
                type="button"
                className="mode-switch"
                onClick={switchMode}
                aria-label={mode === "basic" ? "Switch to Advanced mode" : "Switch to Basic mode"}
                data-tip={mode === "basic" ? "Advanced mode  ⌘\\" : "Basic mode  ⌘\\"}
              >
                <span className={`ico ${mode === "basic" ? "on" : "off"}`}><Icon name="advanced-mode" size={17} /></span>
                <span className={`ico ${mode === "advanced" ? "on" : "off"}`}><Icon name="basic-mode" size={17} /></span>
              </button>
            </div>
          </header>
          <div className="app-scroll">
            <main key={mode} className={`app-content mode-enter${fullBleed ? " full" : ""}`}>{children}</main>
          </div>
        </div>
      </div>
    </ModeContext.Provider>
  );
}
