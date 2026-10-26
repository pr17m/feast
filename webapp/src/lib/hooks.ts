"use client";

import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from "react";
import { getApi, type FeastApi } from "./api/client";

/** Window event fired when the server reports a change relevant to this user (see components/live.tsx). */
export const LIVE_CHANGE_EVENT = "feast:change";

export interface Query<T> {
  data: T | undefined;
  error: Error | undefined;
  loading: boolean;
  reload: () => void;
}

/**
 * Minimal data hook: runs `fn(api)` whenever `key` changes (null = don't run yet).
 * Keeps the previous data while a new key loads, so lists don't flash.
 * Swap for TanStack Query when caching/retries matter; call sites keep the same shape.
 */
export function useQuery<T>(key: string | null, fn: (api: FeastApi) => Promise<T>): Query<T> {
  const [nonce, setNonce] = useState(0);
  const [state, setState] = useState<{ key: string | null; data?: T; error?: Error }>({ key: null });
  const fnRef = useRef(fn);
  useEffect(() => {
    fnRef.current = fn;
  });

  const reqKey = key === null ? null : `${key}#${nonce}`;

  useEffect(() => {
    if (reqKey === null) return;
    let alive = true;
    getApi()
      .then((api) => fnRef.current(api))
      .then((data) => alive && setState({ key: reqKey, data }))
      .catch((error: Error) => alive && setState({ key: reqKey, error }));
    return () => {
      alive = false;
    };
  }, [reqKey]);

  const settled = state.key === reqKey;
  const reload = useCallback(() => setNonce((n) => n + 1), []);

  // Something changed on the server for this user (a booking came in, an order
  // moved, a document was reviewed…): refetch so open screens stay current.
  useEffect(() => {
    if (key === null) return;
    window.addEventListener(LIVE_CHANGE_EVENT, reload);
    return () => window.removeEventListener(LIVE_CHANGE_EVENT, reload);
  }, [key, reload]);
  return {
    data: state.data,
    error: settled ? state.error : undefined,
    loading: reqKey !== null && !settled,
    reload,
  };
}

/** Runs a mutation with pending/error state. */
export function useMutation<A extends unknown[], T>(fn: (api: FeastApi, ...args: A) => Promise<T>) {
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<Error>();
  const fnRef = useRef(fn);
  useEffect(() => {
    fnRef.current = fn;
  });
  const run = useCallback(async (...args: A): Promise<T | undefined> => {
    setPending(true);
    setError(undefined);
    try {
      const api = await getApi();
      return await fnRef.current(api, ...args);
    } catch (e) {
      setError(e as Error);
      return undefined;
    } finally {
      setPending(false);
    }
  }, []);
  const clearError = useCallback(() => setError(undefined), []);
  return { run, pending, error, clearError };
}

// ---------- localStorage-backed state (per-device preferences) ----------

const LOCAL_EVENT = "feast-local-state";

function subscribeLocal(cb: () => void) {
  window.addEventListener("storage", cb);
  window.addEventListener(LOCAL_EVENT, cb);
  return () => {
    window.removeEventListener("storage", cb);
    window.removeEventListener(LOCAL_EVENT, cb);
  };
}

const noopSubscribe = () => () => {};

/** True after hydration on the client. */
export function useHydrated(): boolean {
  return useSyncExternalStore(noopSubscribe, () => true, () => false);
}

export function useLocalState<T>(key: string, initial: T): [T, (v: T) => void, boolean] {
  const raw = useSyncExternalStore(
    subscribeLocal,
    () => {
      try {
        return window.localStorage.getItem(key);
      } catch {
        return null;
      }
    },
    () => null,
  );
  const ready = useHydrated();
  let value = initial;
  if (raw !== null) {
    try {
      value = JSON.parse(raw) as T;
    } catch {
      value = initial;
    }
  }
  const set = useCallback(
    (v: T) => {
      try {
        window.localStorage.setItem(key, JSON.stringify(v));
      } catch {
        // storage unavailable — preference won't persist
      }
      window.dispatchEvent(new Event(LOCAL_EVENT));
    },
    [key],
  );
  return [value, set, ready];
}

/** Increments every `ms` milliseconds while mounted and the tab is visible — use as part of a query key for live views. */
export function useTick(ms: number): number {
  const [tick, setTick] = useState(0);
  useEffect(() => {
    const t = setInterval(() => {
      if (document.visibilityState === "visible") setTick((n) => n + 1);
    }, ms);
    return () => clearInterval(t);
  }, [ms]);
  return tick;
}
