import { describe, expect, it } from "vitest";
import type { EnabledChain } from "./api/types";
import { NoWalletError, WrongChainError, setPausedCall, walletError } from "./chain";
import { ensureChain, readAccount, readChainId, requestAccount, sendContractCall, type Eip1193Provider } from "./wallet";

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

/** Minimal fake wallet: tracks the current chain and which chains it knows; records every call. */
function fakeWallet(opts: { chain: number; known?: number[]; rejectSwitch?: boolean; ignoreSwitch?: boolean }) {
  let chain = opts.chain;
  const known = new Set(opts.known ?? [opts.chain]);
  const calls: string[] = [];
  const p: Eip1193Provider = {
    async request({ method, params }) {
      calls.push(method);
      const arg = Array.isArray(params) ? (params[0] as { chainId?: string }) : undefined;
      switch (method) {
        case "eth_chainId":
          return `0x${chain.toString(16)}`;
        case "eth_accounts":
          return ["0x1f0eb73c714ae0bd1a97c1c8e0f34513073393fb"];
        case "eth_requestAccounts":
          return [];
        case "wallet_switchEthereumChain": {
          if (opts.rejectSwitch) throw { code: 4001, message: "User rejected the request." };
          const id = Number.parseInt(arg!.chainId!, 16);
          if (!known.has(id)) throw { code: 4902, message: "Unrecognized chain ID" };
          if (!opts.ignoreSwitch) chain = id;
          return null;
        }
        case "wallet_addEthereumChain": {
          const id = Number.parseInt(arg!.chainId!, 16);
          known.add(id);
          chain = id; // MetaMask switches after adding
          return null;
        }
      }
      throw new Error(`unexpected ${method}`);
    },
  };
  return { p, calls };
}

describe("ensureChain", () => {
  it("does nothing when already on the chain", async () => {
    const w = fakeWallet({ chain: 5042002 });
    await ensureChain(w.p, CFG);
    expect(w.calls).toEqual(["eth_chainId"]);
  });
  it("switches to a known chain", async () => {
    const w = fakeWallet({ chain: 1, known: [1, 5042002] });
    await ensureChain(w.p, CFG);
    expect(w.calls).toContain("wallet_switchEthereumChain");
    expect(w.calls).not.toContain("wallet_addEthereumChain");
    expect(await readChainId(w.p)).toBe(5042002);
  });
  it("adds the chain when the wallet does not know it (4902)", async () => {
    const w = fakeWallet({ chain: 1 });
    await ensureChain(w.p, CFG);
    expect(w.calls).toContain("wallet_addEthereumChain");
    expect(await readChainId(w.p)).toBe(5042002);
  });
  it("surfaces a rejected switch as user_rejected", async () => {
    const w = fakeWallet({ chain: 1, known: [1, 5042002], rejectSwitch: true });
    const err = await ensureChain(w.p, CFG).catch((e) => e);
    expect(walletError(err).kind).toBe("user_rejected");
  });
  it("throws WrongChainError if the wallet stays on another chain", async () => {
    const w = fakeWallet({ chain: 1, known: [1, 5042002], ignoreSwitch: true });
    await expect(ensureChain(w.p, CFG)).rejects.toBeInstanceOf(WrongChainError);
  });
});

describe("accounts", () => {
  it("reads a checksummed account and rejects an empty connect", async () => {
    const w = fakeWallet({ chain: 5042002 });
    expect(await readAccount(w.p)).toBe("0x1F0eb73C714Ae0BD1a97c1c8e0F34513073393Fb");
    await expect(requestAccount(w.p)).rejects.toThrow(/no account/);
  });
});

describe("sendContractCall", () => {
  it("refuses without a wallet before touching the network", async () => {
    const err = await sendContractCall(null, CFG, "0x1F0eb73C714Ae0BD1a97c1c8e0F34513073393Fb", setPausedCall("0x1111111111111111111111111111111111111111", true)).catch((e) => e);
    expect(err).toBeInstanceOf(NoWalletError);
  });
  it("stops when the user rejects the network switch", async () => {
    const w = fakeWallet({ chain: 1, known: [1, 5042002], rejectSwitch: true });
    const err = await sendContractCall(w.p, CFG, "0x1F0eb73C714Ae0BD1a97c1c8e0F34513073393Fb", setPausedCall("0x1111111111111111111111111111111111111111", true)).catch((e) => e);
    expect(walletError(err).kind).toBe("user_rejected");
    expect(w.calls).not.toContain("eth_sendTransaction");
  });
});
