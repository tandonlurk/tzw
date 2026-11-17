"use client";

import { useCallback, useReducer, useRef } from "react";
import { isApiError, type EnabledChain } from "@/lib/api";
import { walletError, type ContractCall } from "@/lib/chain";
import { txBusy, txReducer } from "@/lib/txlog";
import { useAsync } from "@/lib/useAsync";
import { vaultErrorText } from "@/lib/vault";
import { injectedProvider, readUsdcBalance, sendContractCall, waitForReceipt } from "@/lib/wallet";
import { useWallet } from "./WalletProvider";

/** ERC-20 USDC balance of the connected wallet, re-read after every confirmed tx. Null data when not connected. */
export function useUsdcBalance(cfg: EnabledChain) {
  const w = useWallet();
  const account = w.account;
  return useAsync<bigint | null>(
    () => (account ? readUsdcBalance(cfg, account) : Promise.resolve(null)),
    `usdc:${cfg.chain_id}:${account ?? "-"}:${w.version}`,
  );
}

/** Runs after a tx is confirmed (e.g. POST the hash to the backend); returns a note to show. */
export type AfterConfirm = (hash: string) => Promise<string>;

/**
 * Sign-and-send flow with a visible log: simulate, wallet signature, broadcast, receipt, then optionally record the
 * hash in the backend. A failed backend step can be retried without signing again.
 */
export function useTxRunner(cfg: EnabledChain) {
  const w = useWallet();
  const [list, dispatch] = useReducer(txReducer, []);
  const nextId = useRef(1);
  const afters = useRef(new Map<number, { hash: string; fn: AfterConfirm }>());

  const record = useCallback(async (id: number, hash: string, fn: AfterConfirm) => {
    dispatch({ type: "backend_start", id });
    try {
      dispatch({ type: "backend_done", id, message: await fn(hash) });
    } catch (e) {
      dispatch({ type: "backend_error", id, message: isApiError(e) ? vaultErrorText(e) : e instanceof Error ? e.message : String(e) });
    }
  }, []);

  const run = useCallback(
    async (label: string, call: ContractCall, after?: AfterConfirm): Promise<boolean> => {
      const id = nextId.current++;
      dispatch({ type: "start", id, label });
      if (!w.account) {
        dispatch({ type: "wallet_error", id, rejected: false, message: "Connect your wallet first." });
        return false;
      }
      let hash: string;
      try {
        hash = await sendContractCall(w.provider ?? injectedProvider(), cfg, w.account, call);
      } catch (e) {
        const info = walletError(e);
        dispatch({ type: "wallet_error", id, rejected: info.kind === "user_rejected", message: info.message });
        return false;
      }
      dispatch({ type: "sent", id, hash });
      try {
        const receipt = await waitForReceipt(cfg, hash as `0x${string}`);
        const ok = receipt.status === "success";
        dispatch({ type: "mined", id, ok });
        w.bump();
        if (!ok) return false;
      } catch (e) {
        dispatch({ type: "wallet_error", id, rejected: false, message: walletError(e).message });
        return false;
      }
      if (after) {
        afters.current.set(id, { hash, fn: after });
        await record(id, hash, after);
      }
      return true;
    },
    [cfg, w, record],
  );

  const retry = useCallback(
    (id: number) => {
      const a = afters.current.get(id);
      if (a) void record(id, a.hash, a.fn);
    },
    [record],
  );

  return { list, run, retry, busy: txBusy(list) };
}
