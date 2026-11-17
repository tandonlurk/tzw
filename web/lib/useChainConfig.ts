"use client";

import { api, isApiError, type ChainConfig } from "./api";
import { useAsync, type AsyncState } from "./useAsync";

// One `GET /chain` per page load, shared by every component that needs it.
let cached: Promise<ChainConfig> | null = null;

function loadChain(): Promise<ChainConfig> {
  if (!cached) {
    cached = api.getChain().catch((e) => {
      cached = null;
      // A backend without the chain endpoints behaves like one with the chain switched off.
      if (isApiError(e) && e.status === 404) return { enabled: false } as ChainConfig;
      throw e;
    });
  }
  return cached;
}

export function useChainConfig(): AsyncState<ChainConfig> {
  const s = useAsync(loadChain, "chain-config");
  return {
    ...s,
    reload: () => {
      cached = null;
      s.reload();
    },
  };
}
