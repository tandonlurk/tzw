"use client";

import { useState } from "react";
import { api, isApiError, type Agent, type ApiError, type EnabledChain, type VaultView } from "@/lib/api";
import {
  allocateCall,
  allocationAmount,
  createVaultCall,
  isTxHash,
  maxAllocatable,
  sameAddress,
  setLimitsCall,
  setPausedCall,
  vaultLimits,
  withdrawAmount,
  withdrawCall,
} from "@/lib/chain";
import { formatMoney, signOf } from "@/lib/money";
import { useAsync } from "@/lib/useAsync";
import { driftNote, syncNote, vaultErrorText } from "@/lib/vault";
import { Loading, Money, Tile } from "@/components/ui";
import { useTxRunner, useUsdcBalance } from "./hooks";
import { AddressLink, ChainGate, TESTNET_LABEL, TxLog, WalletStatus } from "./WalletStatus";
import { useWallet } from "./WalletProvider";

/** Plain decimal for prefilling inputs, e.g. "5000000" -> "5". */
function plain(minor: string): string {
  return formatMoney(minor, { unit: false, minFrac: 0 }).replace(/,/g, "");
}

/** Arc testnet vault for one agent: create + link, live onchain view, allocate / withdraw / pause / caps. */
export function VaultPanel({ agent, onChanged }: { agent: Agent; onChanged: () => void }) {
  return (
    <div className="card">
      <h3 style={{ marginTop: 0 }}>Arc testnet vault</h3>
      <ChainGate quiet={agent.env === "simulated"}>{(cfg) => <VaultPanelInner cfg={cfg} agent={agent} onChanged={onChanged} />}</ChainGate>
    </div>
  );
}

function VaultPanelInner({ cfg, agent, onChanged }: { cfg: EnabledChain; agent: Agent; onChanged: () => void }) {
  const [rev, setRev] = useState(0);
  const vault = useAsync(() => api.getVault(agent.id), `vault:${agent.id}:${rev}`);
  const tx = useTxRunner(cfg);
  const refresh = () => {
    setRev((r) => r + 1);
    onChanged();
  };

  let content;
  if (vault.error && vault.error.code === "vault_not_linked") {
    content = <CreateVault cfg={cfg} agent={agent} tx={tx} onLinked={refresh} />;
  } else if (vault.error) {
    content = (
      <div className={vault.error.unreachable ? "state unreachable" : "state error"} role="alert">
        {vault.error.unreachable ? "Backend unreachable." : vaultErrorText(vault.error)}
        <div><button className="sm" onClick={vault.reload} style={{ marginTop: ".5rem" }}>Retry</button></div>
      </div>
    );
  } else if (!vault.data) {
    content = <Loading what="vault" />;
  } else {
    // Keep showing the last vault view while a refresh is in flight so forms are not reset.
    content = <VaultDetails cfg={cfg} v={vault.data} agent={agent} tx={tx} onRefresh={refresh} refreshing={vault.loading} />;
  }

  return (
    <div className="stack">
      <WalletStatus cfg={cfg} compact />
      {content}
      <TxLog cfg={cfg} list={tx.list} onRetry={tx.retry} />
    </div>
  );
}

type Tx = ReturnType<typeof useTxRunner>;

function CreateVault({ cfg, agent, tx, onLinked }: { cfg: EnabledChain; agent: Agent; tx: Tx; onLinked: () => void }) {
  const w = useWallet();
  const [maxPer, setMaxPer] = useState(plain(agent.policy.max_per_interaction));
  const [maxExp, setMaxExp] = useState(plain(agent.policy.max_aggregate_exposure));
  const [err, setErr] = useState<{ field: string; msg: string } | null>(null);
  const limits = vaultLimits(maxPer, maxExp);

  async function create() {
    if ("error" in limits) return setErr({ field: limits.field, msg: limits.error });
    setErr(null);
    await tx.run(
      `Create vault (caps ${formatMoney(limits.maxPerChallenge.toString())} / ${formatMoney(limits.maxExposure.toString())})`,
      createVaultCall(cfg, agent.id, limits.maxPerChallenge, limits.maxExposure),
      async (hash) => {
        await api.linkVault(agent.id, hash);
        onLinked();
        return "Vault linked to this agent.";
      },
    );
  }

  return (
    <div className="stack">
      <div className="dim">
        No vault linked. Creating one deploys an <code>AgentVault</code> owned by your connected wallet. Only you can withdraw, pause it or change its caps.
        The agent runner (operator <AddressLink cfg={cfg} address={cfg.operator_address} />) can only stake vault funds into challenges, within the caps below. Payouts return to the vault.
      </div>
      {agent.env === "simulated" && (
        <div className="notice">
          Linking a vault switches this agent from simulated funds to {TESTNET_LABEL}. The agent must have no open challenges or collaborations.
        </div>
      )}
      <div className="form-grid">
        <div className="field">
          <label htmlFor="v-maxper">Max per challenge (USDC)</label>
          <input id="v-maxper" value={maxPer} onChange={(e) => setMaxPer(e.target.value)} inputMode="decimal" />
          {err?.field === "max_per_challenge" && <div className="field-error">{err.msg}</div>}
        </div>
        <div className="field">
          <label htmlFor="v-maxexp">Max total exposure (USDC)</label>
          <input id="v-maxexp" value={maxExp} onChange={(e) => setMaxExp(e.target.value)} inputMode="decimal" />
          {err?.field === "max_exposure" && <div className="field-error">{err.msg}</div>}
        </div>
      </div>
      <div className="dim" style={{ fontSize: ".85rem" }}>Prefilled from the agent&apos;s policy. These caps are enforced by the vault contract onchain, in addition to the backend policy.</div>
      {"warning" in limits && limits.warning && <div className="notice">{limits.warning}</div>}
      <div className="row">
        <button className="primary" disabled={!w.account || tx.busy} onClick={() => void create()}>Create vault (sign in wallet)</button>
        {!w.account && <span className="dim">Connect your wallet first.</span>}
      </div>
      <LinkByHash agentId={agent.id} onLinked={onLinked} />
    </div>
  );
}

function LinkByHash({ agentId, onLinked }: { agentId: string; onLinked: () => void }) {
  const [hash, setHash] = useState("");
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  async function link() {
    if (!isTxHash(hash)) return setMsg({ ok: false, text: "Enter a transaction hash: 0x followed by 64 hex characters." });
    setBusy(true);
    setMsg(null);
    try {
      await api.linkVault(agentId, hash.trim());
      setMsg({ ok: true, text: "Vault linked." });
      onLinked();
    } catch (e) {
      setMsg({ ok: false, text: isApiError(e) ? vaultErrorText(e) : String(e) });
    } finally {
      setBusy(false);
    }
  }
  return (
    <details>
      <summary>Already created a vault? Link it by its creation transaction hash</summary>
      <div className="row" style={{ marginTop: ".5rem" }}>
        <input style={{ maxWidth: 520 }} className="mono" placeholder="0x..." value={hash} onChange={(e) => setHash(e.target.value)} aria-label="Vault creation transaction hash" />
        <button disabled={busy} onClick={() => void link()}>{busy ? "Linking..." : "Link vault"}</button>
      </div>
      {msg && <div className={msg.ok ? "pos" : "field-error"} role={msg.ok ? "status" : "alert"}>{msg.text}</div>}
    </details>
  );
}

function VaultDetails({ cfg, v, agent, tx, onRefresh, refreshing }: { cfg: EnabledChain; v: VaultView; agent: Agent; tx: Tx; onRefresh: () => void; refreshing: boolean }) {
  const w = useWallet();
  const isOwner = sameAddress(w.account, v.owner);
  const drift = driftNote(v.drift);

  return (
    <div className="stack">
      <div className="row spread">
        <div className="row">
          <span>Vault <a className="mono" href={v.explorer_url} target="_blank" rel="noreferrer" title={v.address}>{v.address}</a></span>
          {v.paused ? <span className="badge pend">paused</span> : <span className="badge pos">accepting stakes</span>}
        </div>
        <button className="sm" disabled={refreshing} onClick={onRefresh}>{refreshing ? "Refreshing..." : "Refresh"}</button>
      </div>
      <div className="dim" style={{ fontSize: ".85rem" }}>
        {TESTNET_LABEL}. Onchain values are read from the chain by the backend; ledger values are the backend&apos;s books for this agent.
      </div>
      <div className="grid">
        <Tile label="In vault (onchain)"><Money value={v.onchain_balance} /></Tile>
        <Tile label="Staked in challenges (onchain)" note="Held by the escrow contract until settled"><Money value={v.onchain_exposure} /></Tile>
        <Tile label="Ledger available"><Money value={v.ledger_available} /></Tile>
        <Tile label="Ledger committed"><Money value={v.ledger_committed} /></Tile>
        <Tile label="Drift (chain vs ledger)" note={signOf(v.drift) === 0 ? "In agreement" : "Needs a sync"}>
          <span className={signOf(v.drift) === 0 ? "pos" : "pend"}>{formatMoney(v.drift, { signed: true })}</span>
        </Tile>
      </div>
      {drift && <div className="notice">{drift}</div>}
      <table className="kv"><tbody>
        <tr><td>Owner</td><td><AddressLink cfg={cfg} address={v.owner} you={isOwner} /></td></tr>
        <tr><td>Operator</td><td><AddressLink cfg={cfg} address={v.operator} /> <span className="dim">(agent runner; can only stake within caps)</span></td></tr>
        <tr><td>Max per challenge</td><td>{formatMoney(v.max_per_challenge)} <span className="dim">onchain cap</span></td></tr>
        <tr><td>Max exposure</td><td>{formatMoney(v.max_exposure)} <span className="dim">onchain cap</span></td></tr>
      </tbody></table>

      {!w.account ? (
        <div className="dim">Connect your wallet to fund or manage this vault.</div>
      ) : !isOwner ? (
        <div className="notice">The connected wallet is not this vault&apos;s owner. Withdraw, pause and caps need the owner wallet ({v.owner}). You can still send USDC to the vault.</div>
      ) : null}

      <div className="grid-2">
        <Allocate cfg={cfg} v={v} agentId={agent.id} tx={tx} onDone={onRefresh} />
        {isOwner && <Withdraw v={v} agentId={agent.id} tx={tx} onDone={onRefresh} />}
        {isOwner && <PauseCard v={v} tx={tx} onDone={onRefresh} />}
        {isOwner && <LimitsCard v={v} tx={tx} onDone={onRefresh} />}
      </div>
      <SyncByHash agentId={agent.id} onDone={onRefresh} />
    </div>
  );
}

function syncAfter(agentId: string): (hash: string) => Promise<string> {
  return async (hash) => syncNote((await api.syncVault(agentId, hash)).credited);
}

function Allocate({ cfg, v, agentId, tx, onDone }: { cfg: EnabledChain; v: VaultView; agentId: string; tx: Tx; onDone: () => void }) {
  const w = useWallet();
  const bal = useUsdcBalance(cfg);
  const [mode, setMode] = useState<"amount" | "percent">("amount");
  const [input, setInput] = useState("");
  const [err, setErr] = useState<string | null>(null);
  const balance = bal.data ?? null;
  const preview = input ? allocationAmount(mode, input, balance) : null;

  async function submit() {
    const r = allocationAmount(mode, input, balance);
    if ("error" in r) return setErr(r.error);
    setErr(null);
    const ok = await tx.run(`Allocate ${formatMoney(r.amount.toString())} to vault`, allocateCall(cfg, v.address, r.amount), syncAfter(agentId));
    if (ok) setInput("");
    onDone();
  }

  return (
    <div className="card">
      <h3 style={{ marginTop: 0 }}>Allocate</h3>
      <div className="dim" style={{ marginBottom: ".5rem", fontSize: ".85rem" }}>Sends USDC from your wallet to the vault, then records it on the agent ledger.</div>
      <div className="row" role="radiogroup" aria-label="Amount type">
        <label className="row" style={{ margin: 0 }}><input type="radio" name="alloc-mode" checked={mode === "amount"} onChange={() => setMode("amount")} /> USDC amount</label>
        <label className="row" style={{ margin: 0 }}><input type="radio" name="alloc-mode" checked={mode === "percent"} onChange={() => setMode("percent")} /> % of wallet</label>
      </div>
      <div className="row" style={{ marginTop: ".4rem" }}>
        <input style={{ maxWidth: 160 }} value={input} onChange={(e) => setInput(e.target.value)} inputMode="decimal" aria-label={mode === "amount" ? "Amount in USDC" : "Percent of wallet balance"} placeholder={mode === "amount" ? "10" : "25"} />
        <span className="dim">{mode === "amount" ? "USDC" : "%"}</span>
        {[25, 50].map((p) => (
          <button key={p} className="sm" disabled={balance === null} onClick={() => { setMode("percent"); setInput(String(p)); }}>{p}%</button>
        ))}
        <button className="sm" disabled={balance === null} title="Wallet balance minus 0.01 USDC kept for gas" onClick={() => { setMode("amount"); setInput(plain(maxAllocatable(balance ?? BigInt(0)).toString())); }}>Max</button>
      </div>
      {preview && "amount" in preview && mode === "percent" && <div className="dim">= {formatMoney(preview.amount.toString())}</div>}
      {err && <div className="field-error">{err}</div>}
      <button className="primary" style={{ marginTop: ".5rem" }} disabled={!w.account || tx.busy} onClick={() => void submit()}>Allocate (sign in wallet)</button>
    </div>
  );
}

function Withdraw({ v, agentId, tx, onDone }: { v: VaultView; agentId: string; tx: Tx; onDone: () => void }) {
  const w = useWallet();
  const [input, setInput] = useState("");
  const [err, setErr] = useState<string | null>(null);

  async function submit() {
    if (!w.account) return;
    const r = withdrawAmount(input, BigInt(v.onchain_balance));
    if ("error" in r) return setErr(r.error);
    setErr(null);
    const ok = await tx.run(`Withdraw ${formatMoney(r.amount.toString())} to your wallet`, withdrawCall(v.address, w.account, r.amount), syncAfter(agentId));
    if (ok) setInput("");
    onDone();
  }

  return (
    <div className="card">
      <h3 style={{ marginTop: 0 }}>Withdraw</h3>
      <div className="dim" style={{ marginBottom: ".5rem", fontSize: ".85rem" }}>Sends USDC from the vault back to your wallet. Stakes in open challenges stay in escrow until settled.</div>
      <div className="row">
        <input style={{ maxWidth: 160 }} value={input} onChange={(e) => setInput(e.target.value)} inputMode="decimal" aria-label="Withdraw amount in USDC" placeholder="10" />
        <span className="dim">USDC</span>
        <button className="sm" onClick={() => setInput(plain(v.onchain_balance))}>Max</button>
      </div>
      {err && <div className="field-error">{err}</div>}
      <button className="primary" style={{ marginTop: ".5rem" }} disabled={tx.busy} onClick={() => void submit()}>Withdraw (sign in wallet)</button>
    </div>
  );
}

function PauseCard({ v, tx, onDone }: { v: VaultView; tx: Tx; onDone: () => void }) {
  async function toggle() {
    await tx.run(v.paused ? "Unpause vault" : "Pause vault", setPausedCall(v.address, !v.paused));
    onDone();
  }
  return (
    <div className="card">
      <h3 style={{ marginTop: 0 }}>{v.paused ? "Paused" : "Pause"}</h3>
      <div className="dim" style={{ marginBottom: ".5rem", fontSize: ".85rem" }}>
        {v.paused
          ? "The agent cannot stake from this vault. Open challenges still settle and pay out to the vault."
          : "Stops the agent from staking vault funds in new challenges. Open challenges still settle. You can still withdraw."}
      </div>
      <button className={v.paused ? "primary" : ""} disabled={tx.busy} onClick={() => void toggle()}>{v.paused ? "Unpause vault (sign in wallet)" : "Pause vault (sign in wallet)"}</button>
    </div>
  );
}

function LimitsCard({ v, tx, onDone }: { v: VaultView; tx: Tx; onDone: () => void }) {
  const [maxPer, setMaxPer] = useState(plain(v.max_per_challenge));
  const [maxExp, setMaxExp] = useState(plain(v.max_exposure));
  const [err, setErr] = useState<{ field: string; msg: string } | null>(null);
  const limits = vaultLimits(maxPer, maxExp);

  async function submit() {
    if ("error" in limits) return setErr({ field: limits.field, msg: limits.error });
    setErr(null);
    await tx.run(
      `Set caps ${formatMoney(limits.maxPerChallenge.toString())} / ${formatMoney(limits.maxExposure.toString())}`,
      setLimitsCall(v.address, limits.maxPerChallenge, limits.maxExposure),
    );
    onDone();
  }

  return (
    <div className="card">
      <h3 style={{ marginTop: 0 }}>Onchain caps</h3>
      <div className="form-grid">
        <div className="field">
          <label htmlFor="l-maxper">Max per challenge (USDC)</label>
          <input id="l-maxper" value={maxPer} onChange={(e) => setMaxPer(e.target.value)} inputMode="decimal" />
          {err?.field === "max_per_challenge" && <div className="field-error">{err.msg}</div>}
        </div>
        <div className="field">
          <label htmlFor="l-maxexp">Max exposure (USDC)</label>
          <input id="l-maxexp" value={maxExp} onChange={(e) => setMaxExp(e.target.value)} inputMode="decimal" />
          {err?.field === "max_exposure" && <div className="field-error">{err.msg}</div>}
        </div>
      </div>
      {"warning" in limits && limits.warning && <div className="notice" style={{ marginBottom: ".5rem" }}>{limits.warning}</div>}
      <div className="dim" style={{ marginBottom: ".5rem", fontSize: ".85rem" }}>Applies to new stakes only; existing stakes are not touched.</div>
      <button disabled={tx.busy} onClick={() => void submit()}>Save caps (sign in wallet)</button>
    </div>
  );
}

function SyncByHash({ agentId, onDone }: { agentId: string; onDone: () => void }) {
  const [hash, setHash] = useState("");
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  async function sync() {
    if (!isTxHash(hash)) return setMsg({ ok: false, text: "Enter a transaction hash: 0x followed by 64 hex characters." });
    setBusy(true);
    setMsg(null);
    try {
      const r = await api.syncVault(agentId, hash.trim());
      setMsg({ ok: true, text: syncNote(r.credited) });
      onDone();
    } catch (e) {
      setMsg({ ok: false, text: isApiError(e) ? vaultErrorText(e as ApiError) : String(e) });
    } finally {
      setBusy(false);
    }
  }
  return (
    <details>
      <summary>Record a deposit or withdrawal made outside this page</summary>
      <div className="dim" style={{ fontSize: ".85rem", marginTop: ".4rem" }}>Paste the hash of a USDC transfer into or out of this vault. Recording the same transaction twice has no effect.</div>
      <div className="row" style={{ marginTop: ".4rem" }}>
        <input style={{ maxWidth: 520 }} className="mono" placeholder="0x..." value={hash} onChange={(e) => setHash(e.target.value)} aria-label="Transaction hash to sync" />
        <button disabled={busy} onClick={() => void sync()}>{busy ? "Recording..." : "Sync transaction"}</button>
      </div>
      {msg && <div className={msg.ok ? "pos" : "field-error"} role={msg.ok ? "status" : "alert"}>{msg.text}</div>}
    </details>
  );
}
