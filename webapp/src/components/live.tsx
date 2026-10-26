"use client";

import { createContext, useCallback, useContext, useEffect, useRef, useState, type ReactNode } from "react";
import { apiBaseUrl } from "@/lib/api/client";
import { getToken } from "@/lib/api/token";
import type { Notification } from "@/lib/api/types";
import { LIVE_CHANGE_EVENT } from "@/lib/hooks";
import { useAuth } from "./providers";

interface LiveState {
  /** Unread notifications for the signed-in user. */
  unread: number;
  setUnread: (n: number) => void;
  /** Newest notification received while the app is open (for a toast). */
  latest: Notification | null;
}

const LiveCtx = createContext<LiveState>({ unread: 0, setUnread: () => {}, latest: null });

export function useLive() {
  return useContext(LiveCtx);
}

/**
 * Live updates from the server over Server-Sent Events (`GET /v1/me/events`):
 * - `ready`: unread count on connect
 * - `notification`: a new notification for this user
 * - `change`: something this user sees changed → open screens refetch
 * EventSource reconnects by itself; we back off when the server is unreachable.
 */
export function LiveProvider({ children }: { children: ReactNode }) {
  const { user } = useAuth();
  const [unread, setUnread] = useState(0);
  const [latest, setLatest] = useState<Notification | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Coalesce bursts of changes into one refetch.
  const changed = useCallback(() => {
    if (timer.current) return;
    timer.current = setTimeout(() => {
      timer.current = null;
      window.dispatchEvent(new Event(LIVE_CHANGE_EVENT));
    }, 300);
  }, []);

  useEffect(() => {
    const token = getToken();
    if (!user || !token) return;
    let es: EventSource | null = null;
    let retry: ReturnType<typeof setTimeout> | null = null;
    let attempts = 0;
    let stopped = false;

    const open = () => {
      const url = new URL("/v1/me/events", apiBaseUrl());
      url.searchParams.set("access_token", token);
      es = new EventSource(url.toString());
      es.addEventListener("ready", (e) => {
        attempts = 0;
        setUnread(JSON.parse((e as MessageEvent).data).unread ?? 0);
      });
      es.addEventListener("notification", (e) => {
        const n = JSON.parse((e as MessageEvent).data) as Notification;
        setLatest(n);
        setUnread((u) => u + 1);
        changed();
      });
      es.addEventListener("change", changed);
      es.onerror = () => {
        // Closed by the server or offline: retry with backoff (max 30s).
        if (es?.readyState === EventSource.CLOSED && !stopped) {
          es.close();
          attempts += 1;
          retry = setTimeout(open, Math.min(30_000, 1000 * 2 ** attempts));
        }
      };
    };
    open();
    return () => {
      stopped = true;
      es?.close();
      if (retry) clearTimeout(retry);
    };
  }, [user, changed]);

  return <LiveCtx.Provider value={{ unread: user ? unread : 0, setUnread, latest: user ? latest : null }}>{children}</LiveCtx.Provider>;
}
