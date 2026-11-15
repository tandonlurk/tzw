"use client";

import { useCallback, useEffect, useState } from "react";
import { ApiError, isApiError } from "./api";

export interface AsyncState<T> {
  data: T | null;
  error: ApiError | null;
  loading: boolean;
  reload: () => void;
}

function toApiError(e: unknown): ApiError {
  if (isApiError(e)) return e;
  return new ApiError("client_error", e instanceof Error ? e.message : "Unknown error", 0);
}

/**
 * Run an async loader on mount and whenever `key` changes. `loader` is read when the effect runs,
 * so callers identify its inputs through `key` (e.g. the agent id).
 */
export function useAsync<T>(loader: () => Promise<T>, key: string): AsyncState<T> {
  const [tick, setTick] = useState(0);
  const reqKey = `${key}#${tick}`;
  const [done, setDone] = useState<{ key: string; data: T | null; error: ApiError | null } | null>(null);

  useEffect(() => {
    let cancelled = false;
    loader()
      .then((data) => !cancelled && setDone({ key: reqKey, data, error: null }))
      .catch((e) => !cancelled && setDone({ key: reqKey, data: null, error: toApiError(e) }));
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [reqKey]);

  const reload = useCallback(() => setTick((t) => t + 1), []);
  const current = done && done.key === reqKey ? done : null;
  return {
    data: current ? current.data : (done?.data ?? null),
    error: current ? current.error : null,
    loading: current === null,
    reload,
  };
}
