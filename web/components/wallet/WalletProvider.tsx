"use client";

import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { getAddress, isAddress, type Hex } from "viem";
import type { EnabledChain } from "@/lib/api";
import { NoWalletError, walletError, type WalletErrorInfo } from "@/lib/chain";
import { ensureChain, injectedProvider, readAccount, readChainId, requestAccount, type Eip1193Provider } from "@/lib/wallet";

export interface WalletCtx {
  /** False until the first check for an injected wallet has run (avoids flashing "no wallet"). */
  ready: boolean;
  provider: Eip1193Provider | null;
  account: Hex | null;
  chainId: number | null;
  busy: boolean;
  error: WalletErrorInfo | null;
  connect: () => Promise<void>;
  switchTo: (cfg: EnabledChain) => Promise<void>;
  /** Bumped after every confirmed tx so balance readers refresh. */
  version: number;
  bump: () => void;
}

const Ctx = createContext<WalletCtx | null>(null);

export function useWallet(): WalletCtx {
  const c = useContext(Ctx);
  if (!c) throw new Error("useWallet outside WalletProvider");
  return c;
}

/** Connection state of the owner's injected (EIP-1193) wallet. Never stores keys; only the public address. */
export function WalletProvider({ children }: { children: ReactNode }) {
  const [ready, setReady] = useState(false);
  const [provider, setProvider] = useState<Eip1193Provider | null>(null);
  const [account, setAccount] = useState<Hex | null>(null);
  const [chainId, setChainId] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<WalletErrorInfo | null>(null);
  const [version, setVersion] = useState(0);

  useEffect(() => {
    let p: Eip1193Provider | null = null;
    let cancelled = false;
    const onAccounts = (...args: unknown[]) => {
      const list = args[0];
      setAccount(Array.isArray(list) && typeof list[0] === "string" && isAddress(list[0]) ? getAddress(list[0]) : null);
    };
    const onChain = (...args: unknown[]) => {
      const id = args[0];
      setChainId(typeof id === "string" ? Number.parseInt(id, 16) : typeof id === "number" ? id : null);
    };
    async function detect() {
      p = injectedProvider();
      if (cancelled) return;
      setProvider(p);
      if (p) {
        p.on?.("accountsChanged", onAccounts);
        p.on?.("chainChanged", onChain);
        try {
          const [a, c] = await Promise.all([readAccount(p), readChainId(p)]);
          if (!cancelled) {
            setAccount(a);
            setChainId(c);
          }
        } catch {
          /* wallet locked or unavailable: stay disconnected */
        }
      }
      if (!cancelled) setReady(true);
    }
    // Some wallets inject after page load and announce it with this event.
    const onInit = () => void detect();
    window.addEventListener("ethereum#initialized", onInit, { once: true });
    void detect();
    return () => {
      cancelled = true;
      window.removeEventListener("ethereum#initialized", onInit);
      p?.removeListener?.("accountsChanged", onAccounts);
      p?.removeListener?.("chainChanged", onChain);
    };
  }, []);

  const connect = useCallback(async () => {
    // Re-read: a wallet may have been injected after mount.
    const p = provider ?? injectedProvider();
    setError(null);
    if (!p) {
      setError(walletError(new NoWalletError()));
      return;
    }
    setProvider(p);
    setBusy(true);
    try {
      setAccount(await requestAccount(p));
      setChainId(await readChainId(p));
    } catch (e) {
      setError(walletError(e));
    } finally {
      setBusy(false);
    }
  }, [provider]);

  const switchTo = useCallback(
    async (cfg: EnabledChain) => {
      const p = provider ?? injectedProvider();
      setError(null);
      if (!p) {
        setError(walletError(new NoWalletError()));
        return;
      }
      setBusy(true);
      try {
        await ensureChain(p, cfg);
        setChainId(await readChainId(p));
      } catch (e) {
        setError(walletError(e));
      } finally {
        setBusy(false);
      }
    },
    [provider],
  );

  const bump = useCallback(() => setVersion((v) => v + 1), []);

  const value = useMemo<WalletCtx>(
    () => ({ ready, provider, account, chainId, busy, error, connect, switchTo, version, bump }),
    [ready, provider, account, chainId, busy, error, connect, switchTo, version, bump],
  );
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}
