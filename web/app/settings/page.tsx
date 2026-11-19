"use client";

import { useEffect, useState } from "react";
import { api, API_URL } from "@/lib/api";
import { clearTerminal } from "@/lib/layouts";
import { useAsync } from "@/lib/useAsync";
import { PageHead } from "@/components/ui";
import { useMode } from "@/components/shell/AppShell";
import { Icon } from "@/components/shell/icons";

function useLocalFlag(key: string, fallback: boolean): [boolean, (v: boolean) => void] {
  const [v, setV] = useState(fallback);
  // Browser-only preference, read after mount.
  // eslint-disable-next-line react-hooks/set-state-in-effect
  useEffect(() => { const s = window.localStorage.getItem(key); if (s !== null) setV(s === "1"); }, [key]);
  return [v, (n: boolean) => { setV(n); window.localStorage.setItem(key, n ? "1" : "0"); }];
}

function Switch({ on, onChange, label }: { on: boolean; onChange: (v: boolean) => void; label: string }) {
  return <button type="button" role="switch" aria-checked={on} aria-label={label} className="switch" onClick={() => onChange(!on)} />;
}

export default function SettingsPage() {
  const { mode, switchMode } = useMode();
  const [reduce, setReduce] = useLocalFlag("tzw.reduceMotion", false);
  const [dev, setDev] = useLocalFlag("tzw.devControls", true);
  const [cleared, setCleared] = useState(false);
  const health = useAsync(api.health, "health");
  const chain = useAsync(api.getChain, "chain");

  useEffect(() => { document.documentElement.dataset.reduceMotion = reduce ? "true" : "false"; }, [reduce]);

  return (
    <div className="settings">
      <PageHead title="Settings" sub="Preferences for this browser. Agent limits and permissions live on each agent's Configuration tab." />

      <h2>Interface</h2>
      <div className="card" style={{ paddingTop: 4, paddingBottom: 4 }}>
        <div className="set-row">
          <div className="l"><div className="t">Mode</div><div className="d">Basic is a calm workspace for managing agents. Advanced is a terminal with charts, logic and code. Toggle any time with the icon in the top right or <kbd>⌘</kbd><kbd>\</kbd>.</div></div>
          <div className="seg" role="group" aria-label="Mode">
            <button type="button" className={mode === "basic" ? "on" : ""} onClick={() => mode !== "basic" && switchMode()}>Basic</button>
            <button type="button" className={mode === "advanced" ? "on" : ""} onClick={() => mode !== "advanced" && switchMode()}>Advanced</button>
          </div>
        </div>
        <div className="set-row">
          <div className="l"><div className="t">Reduce motion</div><div className="d">Turns off transitions and animations. Your system setting is respected either way.</div></div>
          <Switch on={reduce} onChange={setReduce} label="Reduce motion" />
        </div>
        <div className="set-row">
          <div className="l"><div className="t">Saved terminal layouts</div><div className="d">Chart layouts you arranged or saved in Advanced mode. Stored in this browser only.</div></div>
          <button type="button" className="sm outline" onClick={() => { clearTerminal(); setCleared(true); }} disabled={cleared}>{cleared ? <><Icon name="check" size={13} />Reset</> : "Reset layouts"}</button>
        </div>
      </div>

      <h2>Funds</h2>
      <div className="card" style={{ paddingTop: 4, paddingBottom: 4 }}>
        <div className="set-row">
          <div className="l"><div className="t">Environments</div><div className="d">Every balance is labelled. <strong>Simulated</strong> is play money kept in the ledger. <strong>Arc testnet</strong> is testnet USDC held in per-agent vaults. Neither is real money, and the two are never added together.</div></div>
        </div>
        <div className="set-row">
          <div className="l"><div className="t">Arc testnet</div><div className="d">{chain.error ? `Unavailable (${chain.error.code})` : !chain.data ? "Checking…" : chain.data.enabled ? `Enabled · chain ${chain.data.chain_id}` : "Not configured on this backend"}</div></div>
          <span className={`badge ${chain.data?.enabled ? "pos" : "dim"}`}>{chain.data?.enabled ? "On" : "Off"}</span>
        </div>
      </div>

      <h2>Connection</h2>
      <div className="card" style={{ paddingTop: 4, paddingBottom: 4 }}>
        <div className="set-row">
          <div className="l"><div className="t">Backend</div><div className="d mono">{API_URL}</div></div>
          <span className={`badge ${health.data ? "pos" : health.error ? "neg" : "dim"}`}>{health.data ? "Connected" : health.error ? (health.error.unreachable ? "Unreachable" : health.error.code) : "Checking"}</span>
        </div>
        <div className="set-row">
          <div className="l"><div className="t">Developer controls</div><div className="d">Shows the “Run tick” button that advances the agent lifecycle one step. Only works when the backend exposes its dev endpoints.</div></div>
          <Switch on={dev} onChange={setDev} label="Developer controls" />
        </div>
      </div>
    </div>
  );
}
