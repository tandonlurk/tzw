// Browser wallet (EIP-1193) plumbing for the owner's own wallet (D-57). The backend never sees keys or signatures;
// it only receives tx hashes afterwards. Reads go through the backend-provided public RPC (not the wallet), so
// balances still load while the wallet is on another network.

import { createPublicClient, createWalletClient, custom, getAddress, http, type Hex, type PublicClient, type TransactionReceipt } from "viem";
import type { EnabledChain } from "./api/types";
import { ERC20_ABI, NoWalletError, WrongChainError, addChainParams, applyFeeFloor, chainFromConfig, chainIdHex, type ContractCall } from "./chain";

export interface Eip1193Provider {
  request(args: { method: string; params?: unknown }): Promise<unknown>;
  on?(event: string, listener: (...args: unknown[]) => void): void;
  removeListener?(event: string, listener: (...args: unknown[]) => void): void;
}

/** The injected provider (MetaMask and compatible), or null when none is installed or outside the browser. */
export function injectedProvider(): Eip1193Provider | null {
  if (typeof window === "undefined") return null;
  const eth = (window as unknown as { ethereum?: Eip1193Provider }).ethereum;
  return eth && typeof eth.request === "function" ? eth : null;
}

function firstAccount(v: unknown): Hex | null {
  return Array.isArray(v) && typeof v[0] === "string" ? getAddress(v[0]) : null;
}

/** Accounts already authorized for this site (no prompt). */
export async function readAccount(p: Eip1193Provider): Promise<Hex | null> {
  return firstAccount(await p.request({ method: "eth_accounts" }));
}

/** Ask the wallet to connect (prompts). */
export async function requestAccount(p: Eip1193Provider): Promise<Hex> {
  const a = firstAccount(await p.request({ method: "eth_requestAccounts" }));
  if (!a) throw new Error("The wallet returned no account.");
  return a;
}

export async function readChainId(p: Eip1193Provider): Promise<number> {
  const v = await p.request({ method: "eth_chainId" });
  return typeof v === "string" ? Number.parseInt(v, 16) : Number(v);
}

function codeOf(e: unknown): unknown {
  let cur: unknown = e;
  for (let i = 0; i < 5 && cur && typeof cur === "object"; i++) {
    const c = (cur as { code?: unknown }).code;
    if (c !== undefined) return c;
    const data = (cur as { data?: { originalError?: { code?: unknown } } }).data;
    if (data?.originalError?.code !== undefined) return data.originalError.code;
    cur = (cur as { cause?: unknown }).cause;
  }
  return undefined;
}

/**
 * Make sure the wallet is on the backend's chain: switch, and if the wallet does not know the chain (4902),
 * add it from the backend config and switch again. Throws WrongChainError if the wallet still reports another chain.
 */
export async function ensureChain(p: Eip1193Provider, cfg: EnabledChain): Promise<void> {
  if ((await readChainId(p)) === cfg.chain_id) return;
  const chainId = chainIdHex(cfg.chain_id);
  try {
    await p.request({ method: "wallet_switchEthereumChain", params: [{ chainId }] });
  } catch (e) {
    if (codeOf(e) !== 4902) throw e;
    await p.request({ method: "wallet_addEthereumChain", params: [addChainParams(cfg)] });
    if ((await readChainId(p)) !== cfg.chain_id) {
      await p.request({ method: "wallet_switchEthereumChain", params: [{ chainId }] });
    }
  }
  const now = await readChainId(p);
  if (now !== cfg.chain_id) throw new WrongChainError(cfg.chain_id, now);
}

const publicClients = new Map<string, PublicClient>();

export function publicClientFor(cfg: EnabledChain): PublicClient {
  const key = `${cfg.chain_id}|${cfg.rpc_url}`;
  let c = publicClients.get(key);
  if (!c) {
    c = createPublicClient({ chain: chainFromConfig(cfg), transport: http(cfg.rpc_url) }) as PublicClient;
    publicClients.set(key, c);
  }
  return c;
}

/** ERC-20 USDC balance (6 decimals) of `address`, via the public RPC. */
export async function readUsdcBalance(cfg: EnabledChain, address: string): Promise<bigint> {
  return publicClientFor(cfg).readContract({
    address: getAddress(cfg.usdc_address),
    abi: ERC20_ABI,
    functionName: "balanceOf",
    args: [getAddress(address)],
  });
}

/**
 * Simulate `call` from `account` (so a revert is reported with its reason before the owner is asked to sign),
 * then ask the wallet to sign and send it. Returns the tx hash once the wallet has broadcast it.
 */
export async function sendContractCall(p: Eip1193Provider | null, cfg: EnabledChain, account: Hex, call: ContractCall): Promise<Hex> {
  if (!p) throw new NoWalletError();
  await ensureChain(p, cfg);
  const chain = chainFromConfig(cfg);
  const pub = publicClientFor(cfg);
  const req = { address: call.address, abi: call.abi, functionName: call.functionName, args: call.args, account } as Parameters<PublicClient["simulateContract"]>[0];
  await pub.simulateContract(req);
  let fees: { maxFeePerGas: bigint; maxPriorityFeePerGas: bigint } | Record<string, never> = {};
  try {
    fees = applyFeeFloor(cfg.chain_id, await pub.estimateFeesPerGas());
  } catch {
    fees = {}; // let the wallet choose
  }
  const wallet = createWalletClient({ account, chain, transport: custom(p) });
  return wallet.writeContract({ ...(req as object), ...fees, chain, account } as Parameters<typeof wallet.writeContract>[0]);
}

/** Wait for the receipt via the public RPC. Arc has sub-second deterministic finality, so one confirmation is final. */
export async function waitForReceipt(cfg: EnabledChain, hash: Hex, timeoutMs = 90_000): Promise<TransactionReceipt> {
  return publicClientFor(cfg).waitForTransactionReceipt({ hash, timeout: timeoutMs });
}
