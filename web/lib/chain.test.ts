import { describe, expect, it } from "vitest";
import { ContractFunctionRevertedError, UserRejectedRequestError } from "viem";
import type { EnabledChain } from "./api/types";
import {
  AGENT_VAULT_ABI,
  ARC_MIN_FEE_PER_GAS,
  NoWalletError,
  WrongChainError,
  addChainParams,
  agentRef,
  allocateCall,
  allocationAmount,
  applyFeeFloor,
  calldata,
  chainFromConfig,
  chainIdHex,
  chainProblem,
  createVaultCall,
  explorerAddressUrl,
  explorerTxUrl,
  maxAllocatable,
  parsePercentBps,
  parseUsdc,
  percentOf,
  sameAddress,
  setLimitsCall,
  setPausedCall,
  vaultLimits,
  walletError,
  withdrawAmount,
  withdrawCall,
} from "./chain";

const CFG: EnabledChain = {
  enabled: true,
  chain_id: 5042002,
  chain_name: "Arc Testnet",
  rpc_url: "https://rpc.testnet.arc.io",
  explorer_url: "https://explorer.testnet.arc.io",
  usdc_address: "0x3600000000000000000000000000000000000000",
  usdc_decimals: 6,
  escrow_address: "0x4179f2e314410D10C2906B1914bAAe8DD93b5698",
  vault_factory_address: "0x4EA6ac6A9F7b8fcf01ba04b177F0A098d9664C45",
  operator_address: "0x84dDb2795697e6EE319fe9bb55b5D5c1c3B90eca",
};
const AGENT = "1b4e28ba-2fa1-416f-9a3e-5a0b2c9d7e11";
const VAULT = "0x1111111111111111111111111111111111111111";
const OWNER = "0x1F0eb73C714Ae0BD1a97c1c8e0F34513073393Fb";

describe("agentRef", () => {
  it("left-pads the 16 UUID bytes to 32", () => {
    expect(agentRef(AGENT)).toBe("0x000000000000000000000000000000001b4e28ba2fa1416f9a3e5a0b2c9d7e11");
    expect(agentRef(AGENT.toUpperCase())).toBe(agentRef(AGENT));
    expect(agentRef(AGENT)).toHaveLength(66);
  });
  it("rejects non-UUIDs", () => {
    expect(() => agentRef("not-a-uuid")).toThrow();
    expect(() => agentRef("1b4e28ba2fa1416f9a3e5a0b2c9d7e11")).toThrow();
  });
});

describe("calldata (cross-checked with `cast calldata`)", () => {
  it("createVault", () => {
    const call = createVaultCall(CFG, AGENT, BigInt(5_000_000), BigInt(20_000_000));
    expect(call.address).toBe(CFG.vault_factory_address);
    expect(calldata(call)).toBe(
      "0xdeb6433e00000000000000000000000084ddb2795697e6ee319fe9bb55b5d5c1c3b90eca00000000000000000000000000000000000000000000000000000000004c4b400000000000000000000000000000000000000000000000000000000001312d00000000000000000000000000000000001b4e28ba2fa1416f9a3e5a0b2c9d7e11",
    );
  });
  it("allocate is a USDC transfer to the vault", () => {
    const call = allocateCall(CFG, VAULT, BigInt(1_500_000));
    expect(call.address).toBe(CFG.usdc_address);
    expect(calldata(call)).toBe(`0xa9059cbb${VAULT.slice(2).padStart(64, "0")}${(1_500_000).toString(16).padStart(64, "0")}`);
  });
  it("withdraw, setPaused, setLimits target the vault", () => {
    expect(calldata(withdrawCall(VAULT, OWNER, BigInt(1)))).toBe(
      `0xf3fef3a3${OWNER.slice(2).toLowerCase().padStart(64, "0")}${"1".padStart(64, "0")}`,
    );
    expect(calldata(setPausedCall(VAULT, true))).toBe(`0x16c38b3c${"1".padStart(64, "0")}`);
    expect(calldata(setPausedCall(VAULT, false))).toBe(`0x16c38b3c${"0".padStart(64, "0")}`);
    expect(calldata(setLimitsCall(VAULT, BigInt(2), BigInt(3)))).toBe(`0xc4590d3f${"2".padStart(64, "0")}${"3".padStart(64, "0")}`);
    expect(withdrawCall(VAULT, OWNER, BigInt(1)).address).toBe(VAULT);
  });
  it("rejects a bad address", () => {
    expect(() => withdrawCall("0x123", OWNER, BigInt(1))).toThrow();
  });
});

describe("chain config", () => {
  it("builds add-chain params with Arc's 18-decimal native USDC", () => {
    expect(chainIdHex(5042002)).toBe("0x4cef52");
    expect(addChainParams(CFG)).toEqual({
      chainId: "0x4cef52",
      chainName: "Arc Testnet",
      nativeCurrency: { name: "USDC", symbol: "USDC", decimals: 18 },
      rpcUrls: ["https://rpc.testnet.arc.io"],
      blockExplorerUrls: ["https://explorer.testnet.arc.io"],
    });
    const c = chainFromConfig(CFG);
    expect(c.id).toBe(5042002);
    expect(c.rpcUrls.default.http[0]).toBe(CFG.rpc_url);
  });
  it("flags a misconfigured backend", () => {
    expect(chainProblem(CFG)).toBeNull();
    expect(chainProblem({ ...CFG, usdc_decimals: 18 })).toMatch(/decimals/);
    expect(chainProblem({ ...CFG, vault_factory_address: "0xnope" })).toMatch(/vault_factory_address/);
    expect(chainProblem({ ...CFG, rpc_url: "ftp://x" })).toMatch(/RPC/);
  });
  it("explorer links and address compare", () => {
    expect(explorerTxUrl("https://explorer.testnet.arc.io/", "0xab")).toBe("https://explorer.testnet.arc.io/tx/0xab");
    expect(explorerAddressUrl(CFG.explorer_url, VAULT)).toBe(`https://explorer.testnet.arc.io/address/${VAULT}`);
    expect(sameAddress(OWNER, OWNER.toLowerCase())).toBe(true);
    expect(sameAddress(OWNER, VAULT)).toBe(false);
    expect(sameAddress(null, OWNER)).toBe(false);
    expect(sameAddress("junk", "junk")).toBe(false);
  });
  it("applies the 20 gwei fee floor on Arc only", () => {
    expect(applyFeeFloor(5042002, { maxFeePerGas: BigInt(1), maxPriorityFeePerGas: BigInt(1) })).toEqual({ maxFeePerGas: ARC_MIN_FEE_PER_GAS, maxPriorityFeePerGas: BigInt(1) });
    const high = { maxFeePerGas: ARC_MIN_FEE_PER_GAS * BigInt(2), maxPriorityFeePerGas: BigInt(0) };
    expect(applyFeeFloor(5042002, high)).toEqual(high);
    expect(applyFeeFloor(31337, { maxFeePerGas: BigInt(1), maxPriorityFeePerGas: BigInt(1) }).maxFeePerGas).toBe(BigInt(1));
  });
});

describe("amounts", () => {
  it("parses USDC to bigint minor units", () => {
    expect(parseUsdc("1.5")).toBe(BigInt(1_500_000));
    expect(parseUsdc("0.0000001")).toBeNull();
    expect(parseUsdc("-1")).toBeNull();
  });
  it("parses percentages to bps", () => {
    expect(parsePercentBps("25")).toBe(2500);
    expect(parsePercentBps("12.5%")).toBe(1250);
    expect(parsePercentBps("100")).toBe(10000);
    expect(parsePercentBps("0.01")).toBe(1);
    expect(parsePercentBps("0")).toBeNull();
    expect(parsePercentBps("100.01")).toBeNull();
    expect(parsePercentBps("1.234")).toBeNull();
    expect(parsePercentBps("abc")).toBeNull();
  });
  it("computes percent of balance with exact bigint floor", () => {
    expect(percentOf(BigInt(10_000_000), 2500)).toBe(BigInt(2_500_000));
    expect(percentOf(BigInt(3), 5000)).toBe(BigInt(1));
    const big = BigInt("123456789012345678901");
    expect(percentOf(big, 3333)).toBe((big * BigInt(3333)) / BigInt(10000));
  });
  it("keeps a gas reserve when allocating", () => {
    expect(maxAllocatable(BigInt(1_000_000))).toBe(BigInt(990_000));
    expect(maxAllocatable(BigInt(5))).toBe(BigInt(0));
    expect(allocationAmount("amount", "0.5", BigInt(1_000_000))).toEqual({ amount: BigInt(500_000) });
    expect(allocationAmount("percent", "50", BigInt(1_000_000))).toEqual({ amount: BigInt(500_000) });
    expect(allocationAmount("percent", "100", BigInt(1_000_000))).toHaveProperty("error");
    expect(allocationAmount("amount", "1", BigInt(1_000_000))).toHaveProperty("error");
    expect(allocationAmount("amount", "0", BigInt(1_000_000))).toHaveProperty("error");
    expect(allocationAmount("percent", "50", null)).toHaveProperty("error");
    expect(allocationAmount("amount", "2", null)).toEqual({ amount: BigInt(2_000_000) });
  });
  it("bounds withdrawals by the vault balance", () => {
    expect(withdrawAmount("1", BigInt(1_000_000))).toEqual({ amount: BigInt(1_000_000) });
    expect(withdrawAmount("1.000001", BigInt(1_000_000))).toHaveProperty("error");
    expect(withdrawAmount("", BigInt(1_000_000))).toHaveProperty("error");
  });
  it("validates vault limits", () => {
    expect(vaultLimits("5", "20")).toEqual({ maxPerChallenge: BigInt(5_000_000), maxExposure: BigInt(20_000_000), warning: null });
    expect(vaultLimits("30", "20")).toMatchObject({ warning: expect.stringMatching(/effective/) });
    expect(vaultLimits("0", "20")).toMatchObject({ warning: expect.stringMatching(/zero/) });
    expect(vaultLimits("x", "20")).toEqual({ error: expect.any(String), field: "max_per_challenge" });
    expect(vaultLimits("1", "")).toEqual({ error: expect.any(String), field: "max_exposure" });
  });
});

describe("walletError", () => {
  it("maps user rejection (raw EIP-1193 and viem)", () => {
    expect(walletError({ code: 4001, message: "User rejected the request." }).kind).toBe("user_rejected");
    expect(walletError(new UserRejectedRequestError(new Error("x"))).kind).toBe("user_rejected");
    expect(walletError({ message: "wrapped", cause: { code: 4001 } }).kind).toBe("user_rejected");
  });
  it("maps chain problems", () => {
    expect(walletError({ code: 4902, message: "Unrecognized chain ID" }).kind).toBe("unknown_chain");
    expect(walletError(new WrongChainError(5042002, 1)).kind).toBe("wrong_chain");
    expect(walletError({ name: "ChainMismatchError", message: "x" }).kind).toBe("wrong_chain");
  });
  it("maps pending request, no wallet, funds, timeout", () => {
    expect(walletError({ code: -32002, message: "already pending" }).kind).toBe("request_pending");
    expect(walletError(new NoWalletError()).kind).toBe("no_wallet");
    expect(walletError({ name: "InsufficientFundsError", message: "x" }).kind).toBe("insufficient_funds");
    expect(walletError({ name: "WaitForTransactionReceiptTimeoutError", message: "x" }).kind).toBe("timeout");
  });
  it("decodes vault reverts into plain language", () => {
    const err = new ContractFunctionRevertedError({ abi: AGENT_VAULT_ABI, functionName: "withdraw", data: "0x30cd7471" });
    const info = walletError({ name: "ContractFunctionExecutionError", message: "failed", cause: err });
    expect(info.kind).toBe("reverted");
    expect(info.message).toMatch(/owner/);
  });
  it("falls back to the first line of the message", () => {
    expect(walletError(new Error("boom\nstack"))).toEqual({ kind: "unknown", message: "boom" });
  });
});
