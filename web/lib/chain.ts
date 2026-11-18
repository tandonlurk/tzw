// Pure helpers for the Arc testnet wallet UI (D-57: owner-held AgentVaults). No browser or network access here,
// so everything is unit-tested. Amounts are bigint USDC minor units (6 decimals, ERC-20 interface); the backend
// contract carries them as decimal strings. Addresses always come from `GET /chain`, never from constants.

import { defineChain, encodeFunctionData, getAddress, isAddress, isAddressEqual, isHash, toHex, type Chain, type Hex } from "viem";
import type { EnabledChain } from "./api/types";
import { DECIMALS, parseMoney } from "./money";

// ------------------------------------------------------------------ ABIs (subset of contracts/src/AgentVault.sol + ERC-20)

const VAULT_ERRORS = [
  { type: "error", name: "ZeroAddress", inputs: [] },
  { type: "error", name: "NotOwner", inputs: [] },
  { type: "error", name: "NotOperator", inputs: [] },
  { type: "error", name: "IsPaused", inputs: [] },
  { type: "error", name: "TransferFailed", inputs: [] },
] as const;

export const ERC20_ABI = [
  { type: "function", name: "balanceOf", stateMutability: "view", inputs: [{ name: "account", type: "address" }], outputs: [{ type: "uint256" }] },
  { type: "function", name: "transfer", stateMutability: "nonpayable", inputs: [{ name: "to", type: "address" }, { name: "amount", type: "uint256" }], outputs: [{ type: "bool" }] },
] as const;

export const VAULT_FACTORY_ABI = [
  {
    type: "function", name: "createVault", stateMutability: "nonpayable",
    inputs: [
      { name: "operator", type: "address" },
      { name: "maxPerChallenge", type: "uint256" },
      { name: "maxExposure", type: "uint256" },
      { name: "agentRef", type: "bytes32" },
    ],
    outputs: [{ name: "vault", type: "address" }],
  },
  { type: "error", name: "ZeroAddress", inputs: [] },
] as const;

export const AGENT_VAULT_ABI = [
  { type: "function", name: "withdraw", stateMutability: "nonpayable", inputs: [{ name: "to", type: "address" }, { name: "amount", type: "uint256" }], outputs: [] },
  { type: "function", name: "setLimits", stateMutability: "nonpayable", inputs: [{ name: "maxPerChallenge_", type: "uint256" }, { name: "maxExposure_", type: "uint256" }], outputs: [] },
  { type: "function", name: "setPaused", stateMutability: "nonpayable", inputs: [{ name: "paused_", type: "bool" }], outputs: [] },
  ...VAULT_ERRORS,
] as const;

// ------------------------------------------------------------------ chain config

/**
 * Arc rejects transactions whose maxFeePerGas is below 20 gwei without returning a receipt
 * (docs/research/sponsors.md §1, docs.arc.io gas-and-fees). Applied only on Arc testnet.
 */
export const ARC_TESTNET_ID = 5042002;
export const ARC_MIN_FEE_PER_GAS = BigInt(20_000_000_000);

/** USDC left in the wallet when allocating: on Arc the same USDC balance pays gas. */
export const GAS_RESERVE = BigInt(10_000); // 0.01 USDC

/** Why this config cannot drive the wallet UI, or null when it can. Guards against a misconfigured backend. */
export function chainProblem(cfg: EnabledChain): string | null {
  if (!Number.isSafeInteger(cfg.chain_id) || cfg.chain_id <= 0) return "The backend reported an invalid chain id.";
  if (cfg.usdc_decimals !== DECIMALS) return `USDC has ${cfg.usdc_decimals} decimals on this chain; this app only handles ${DECIMALS}.`;
  for (const [k, v] of [["usdc_address", cfg.usdc_address], ["vault_factory_address", cfg.vault_factory_address], ["operator_address", cfg.operator_address]] as const) {
    if (!isAddress(v, { strict: false })) return `The backend reported an invalid ${k}.`;
  }
  if (!/^https?:\/\//.test(cfg.rpc_url)) return "The backend reported an invalid RPC URL.";
  return null;
}

export function chainIdHex(id: number): Hex {
  return toHex(id);
}

/** Params for EIP-3085 `wallet_addEthereumChain`. Arc's native gas token is USDC with 18 decimals (the ERC-20 view has 6). */
export function addChainParams(cfg: EnabledChain) {
  return {
    chainId: chainIdHex(cfg.chain_id),
    chainName: cfg.chain_name,
    nativeCurrency: { name: "USDC", symbol: "USDC", decimals: 18 },
    rpcUrls: [cfg.rpc_url],
    blockExplorerUrls: [cfg.explorer_url],
  };
}

/** viem chain object built from the backend config (so nothing chain-specific is hardcoded in components). */
export function chainFromConfig(cfg: EnabledChain): Chain {
  return defineChain({
    id: cfg.chain_id,
    name: cfg.chain_name,
    nativeCurrency: { name: "USDC", symbol: "USDC", decimals: 18 },
    rpcUrls: { default: { http: [cfg.rpc_url] } },
    blockExplorers: { default: { name: "Explorer", url: cfg.explorer_url } },
    testnet: true,
  });
}

export function explorerTxUrl(explorer: string, hash: string): string {
  return `${explorer.replace(/\/+$/, "")}/tx/${hash}`;
}

export function explorerAddressUrl(explorer: string, address: string): string {
  return `${explorer.replace(/\/+$/, "")}/address/${address}`;
}

export function shortAddress(a: string): string {
  return a.length > 12 ? `${a.slice(0, 6)}...${a.slice(-4)}` : a;
}

export function sameAddress(a: string | null | undefined, b: string | null | undefined): boolean {
  if (!a || !b || !isAddress(a, { strict: false }) || !isAddress(b, { strict: false })) return false;
  return isAddressEqual(getAddress(a), getAddress(b));
}

export function isTxHash(s: string): boolean {
  return isHash(s.trim());
}

/** Raise the wallet's suggested fees to Arc's 20 gwei floor (other chains unchanged). */
export function applyFeeFloor(
  chainId: number,
  fees: { maxFeePerGas: bigint; maxPriorityFeePerGas: bigint },
): { maxFeePerGas: bigint; maxPriorityFeePerGas: bigint } {
  if (chainId !== ARC_TESTNET_ID) return fees;
  const maxFeePerGas = fees.maxFeePerGas < ARC_MIN_FEE_PER_GAS ? ARC_MIN_FEE_PER_GAS : fees.maxFeePerGas;
  const maxPriorityFeePerGas = fees.maxPriorityFeePerGas > maxFeePerGas ? maxFeePerGas : fees.maxPriorityFeePerGas;
  return { maxFeePerGas, maxPriorityFeePerGas };
}

// ------------------------------------------------------------------ agentRef

const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

/** The agent's UUID as the factory's `bytes32 agentRef`: 16 UUID bytes left-padded with zeros. Throws on a non-UUID. */
export function agentRef(agentId: string): Hex {
  if (!UUID_RE.test(agentId)) throw new Error(`Not a UUID: ${agentId}`);
  return `0x${"0".repeat(32)}${agentId.replace(/-/g, "").toLowerCase()}`;
}

// ------------------------------------------------------------------ amounts

/** "1.5" -> 1500000n; null for empty, negative, non-numeric or more than 6 decimals. */
export function parseUsdc(input: string): bigint | null {
  const minor = parseMoney(input);
  return minor === null ? null : BigInt(minor);
}

/** Percentage input ("25", "12.5", "100") -> basis points, or null outside 0 < p <= 100 / more than 2 decimals. */
export function parsePercentBps(input: string): number | null {
  const m = /^(\d{1,3})(?:\.(\d{1,2}))?$/.exec(input.trim().replace(/%$/, "").trim());
  if (!m) return null;
  const bps = Number(m[1]) * 100 + Number((m[2] ?? "").padEnd(2, "0") || "0");
  return bps > 0 && bps <= 10_000 ? bps : null;
}

/** floor(balance * bps / 10000), exact in bigint. */
export function percentOf(balance: bigint, bps: number): bigint {
  return (balance * BigInt(bps)) / BigInt(10_000);
}

/** Largest allocation that still leaves GAS_RESERVE for fees, never negative. */
export function maxAllocatable(walletBalance: bigint, reserve: bigint = GAS_RESERVE): bigint {
  return walletBalance > reserve ? walletBalance - reserve : BigInt(0);
}

/** Resolve the allocate form ("amount" or "percent" of wallet USDC) to minor units, or an error to show. */
export function allocationAmount(
  mode: "amount" | "percent",
  input: string,
  walletBalance: bigint | null,
): { amount: bigint } | { error: string } {
  let amount: bigint | null;
  if (mode === "percent") {
    if (walletBalance === null) return { error: "Wallet balance not loaded yet." };
    const bps = parsePercentBps(input);
    if (bps === null) return { error: "Enter a percentage between 0 and 100 (at most 2 decimals)." };
    amount = percentOf(walletBalance, bps);
  } else {
    amount = parseUsdc(input);
    if (amount === null) return { error: "Enter a positive amount with at most 6 decimals." };
  }
  if (amount <= BigInt(0)) return { error: "The amount must be greater than zero." };
  if (walletBalance !== null && amount > maxAllocatable(walletBalance)) {
    return { error: "That leaves too little USDC for gas. On Arc, fees are paid from the same USDC balance; keep at least 0.01 USDC." };
  }
  return { amount };
}

/** Withdraw form: positive, at most 6 decimals, within the vault's current onchain balance. */
export function withdrawAmount(input: string, vaultBalance: bigint): { amount: bigint } | { error: string } {
  const amount = parseUsdc(input);
  if (amount === null) return { error: "Enter a positive amount with at most 6 decimals." };
  if (amount <= BigInt(0)) return { error: "The amount must be greater than zero." };
  if (amount > vaultBalance) return { error: "More than the vault holds. Funds staked in open challenges are in escrow, not in the vault." };
  return { amount };
}

/** Vault caps form. A per-challenge cap above the total cap is allowed onchain but has no effect, so it only warns. */
export function vaultLimits(
  maxPerInput: string,
  maxExpInput: string,
): { maxPerChallenge: bigint; maxExposure: bigint; warning: string | null } | { error: string; field: "max_per_challenge" | "max_exposure" } {
  const maxPerChallenge = parseUsdc(maxPerInput);
  if (maxPerChallenge === null) return { error: "Enter an amount with at most 6 decimals.", field: "max_per_challenge" };
  const maxExposure = parseUsdc(maxExpInput);
  if (maxExposure === null) return { error: "Enter an amount with at most 6 decimals.", field: "max_exposure" };
  const warning =
    maxPerChallenge === BigInt(0) || maxExposure === BigInt(0)
      ? "A zero cap means the agent cannot stake anything from this vault."
      : maxPerChallenge > maxExposure
        ? "Max per challenge is above max exposure, so max exposure is the effective limit."
        : null;
  return { maxPerChallenge, maxExposure, warning };
}

// ------------------------------------------------------------------ transactions

/** A contract write the owner signs: a viem `writeContract` request minus account/chain. */
export interface ContractCall {
  address: Hex;
  abi: readonly unknown[];
  functionName: string;
  args: readonly unknown[];
}

export function createVaultCall(cfg: EnabledChain, agentId: string, maxPerChallenge: bigint, maxExposure: bigint): ContractCall {
  return {
    address: getAddress(cfg.vault_factory_address),
    abi: VAULT_FACTORY_ABI,
    functionName: "createVault",
    args: [getAddress(cfg.operator_address), maxPerChallenge, maxExposure, agentRef(agentId)],
  };
}

export function allocateCall(cfg: EnabledChain, vault: string, amount: bigint): ContractCall {
  return { address: getAddress(cfg.usdc_address), abi: ERC20_ABI, functionName: "transfer", args: [getAddress(vault), amount] };
}

export function withdrawCall(vault: string, to: string, amount: bigint): ContractCall {
  return { address: getAddress(vault), abi: AGENT_VAULT_ABI, functionName: "withdraw", args: [getAddress(to), amount] };
}

export function setPausedCall(vault: string, paused: boolean): ContractCall {
  return { address: getAddress(vault), abi: AGENT_VAULT_ABI, functionName: "setPaused", args: [paused] };
}

export function setLimitsCall(vault: string, maxPerChallenge: bigint, maxExposure: bigint): ContractCall {
  return { address: getAddress(vault), abi: AGENT_VAULT_ABI, functionName: "setLimits", args: [maxPerChallenge, maxExposure] };
}

/** Calldata for a call (used by tests and for display in the technical details). */
export function calldata(call: ContractCall): Hex {
  return encodeFunctionData({ abi: call.abi, functionName: call.functionName, args: call.args } as Parameters<typeof encodeFunctionData>[0]);
}

// ------------------------------------------------------------------ errors

export type WalletErrorKind =
  | "no_wallet"
  | "user_rejected"
  | "request_pending"
  | "wrong_chain"
  | "unknown_chain"
  | "reverted"
  | "insufficient_funds"
  | "timeout"
  | "unknown";

export interface WalletErrorInfo {
  kind: WalletErrorKind;
  message: string;
}

const REVERT_TEXT: Record<string, string> = {
  NotOwner: "Only the vault's owner wallet can do this. Connect the wallet that created the vault.",
  ZeroAddress: "The contract refused a zero address.",
  TransferFailed: "The USDC transfer failed. The amount may be more than the vault holds.",
  IsPaused: "The vault is paused.",
  NotOperator: "Only the vault operator can do this.",
};

/** Thrown by our own code when the wallet ends up on another chain than the one the backend uses. */
export class WrongChainError extends Error {
  constructor(public readonly expected: number, public readonly actual: number | null) {
    super(`Wallet is on chain ${actual ?? "unknown"}, expected ${expected}.`);
    this.name = "WrongChainError";
  }
}

export class NoWalletError extends Error {
  constructor() {
    super("No browser wallet found.");
    this.name = "NoWalletError";
  }
}

interface ErrLike {
  name?: unknown;
  code?: unknown;
  message?: unknown;
  shortMessage?: unknown;
  cause?: unknown;
  data?: unknown;
}

function chainOf(e: unknown): ErrLike[] {
  const out: ErrLike[] = [];
  let cur: unknown = e;
  for (let i = 0; i < 10 && cur && typeof cur === "object"; i++) {
    out.push(cur as ErrLike);
    cur = (cur as ErrLike).cause;
  }
  return out;
}

/**
 * Map an EIP-1193 / viem / own error to a kind and a plain-language message. Walks the `cause` chain because
 * viem wraps provider errors (e.g. ContractFunctionExecutionError -> ContractFunctionRevertedError).
 */
export function walletError(e: unknown): WalletErrorInfo {
  const chain = chainOf(e);
  const has = (pred: (x: ErrLike) => boolean) => chain.some(pred);
  const msgs = chain.map((x) => `${typeof x.message === "string" ? x.message : ""} ${typeof x.shortMessage === "string" ? x.shortMessage : ""}`).join(" ");

  if (has((x) => x.name === "NoWalletError")) {
    return { kind: "no_wallet", message: "No browser wallet found. Install a wallet such as MetaMask, then reload this page." };
  }
  if (has((x) => x.code === 4001 || x.name === "UserRejectedRequestError") || /user (rejected|denied)/i.test(msgs)) {
    return { kind: "user_rejected", message: "You rejected the request in your wallet. Nothing was sent." };
  }
  if (has((x) => x.code === -32002)) {
    return { kind: "request_pending", message: "Your wallet already has a request waiting. Open the wallet and finish or dismiss it." };
  }
  if (has((x) => x.code === 4902) || /unrecognized chain/i.test(msgs)) {
    return { kind: "unknown_chain", message: "Your wallet does not know Arc Testnet yet. Approve adding the network, then try again." };
  }
  if (has((x) => x.name === "WrongChainError" || x.name === "ChainMismatchError")) {
    return { kind: "wrong_chain", message: "Your wallet is on a different network. Switch to Arc Testnet and try again." };
  }
  const reverted = chain.find((x) => x.name === "ContractFunctionRevertedError");
  if (reverted) {
    const data = reverted.data as { errorName?: string } | undefined;
    const name = data?.errorName;
    const text = name && REVERT_TEXT[name];
    const reason = (reverted as { reason?: unknown }).reason;
    return { kind: "reverted", message: text ?? `The contract refused the transaction${name ? ` (${name})` : typeof reason === "string" ? ` (${reason})` : ""}.` };
  }
  if (has((x) => x.name === "InsufficientFundsError") || /insufficient funds/i.test(msgs)) {
    return { kind: "insufficient_funds", message: "Not enough USDC in your wallet to pay for gas (Arc pays gas in USDC). Get testnet USDC from the faucet." };
  }
  if (has((x) => x.name === "WaitForTransactionReceiptTimeoutError")) {
    return { kind: "timeout", message: "No confirmation yet. The transaction may still land; check the explorer link, then sync it by hash." };
  }
  const first = chain[0];
  const short = first && typeof first.shortMessage === "string" ? first.shortMessage : first && typeof first.message === "string" ? first.message : "Unknown wallet error.";
  return { kind: "unknown", message: short.split("\n")[0] };
}
