import { useSyncExternalStore } from "react";

/**
 * Where the web app is running. The "Feast" and "Feast Partner" store apps are
 * Capacitor shells that load this same site; inside them `window.Capacitor`
 * reports a native platform.
 *
 * Store rules this drives: restaurant subscriptions are bought on the website,
 * never inside the iOS/Android apps (no purchase buttons, prices to buy or
 * links to buy there).
 */

type CapacitorGlobal = { isNativePlatform?: () => boolean; getPlatform?: () => string };

function capacitor(): CapacitorGlobal | undefined {
  if (typeof window === "undefined") return undefined;
  return (window as unknown as { Capacitor?: CapacitorGlobal }).Capacitor;
}

/** True inside the iOS or Android app (false on the server and in browsers). */
export function isNativeApp(): boolean {
  try {
    return capacitor()?.isNativePlatform?.() === true;
  } catch {
    return false;
  }
}

/** "ios" | "android" inside the apps, else "web". */
export function nativePlatform(): "ios" | "android" | "web" {
  if (!isNativeApp()) return "web";
  const p = capacitor()?.getPlatform?.();
  return p === "ios" || p === "android" ? p : "web";
}

/**
 * `isNativeApp()` for components: false during server rendering and the first
 * client render (so hydration matches), then the real value.
 */
export function useIsNativeApp(): boolean {
  return useSyncExternalStore(noSubscribe, isNativeApp, () => false);
}

const noSubscribe = () => () => {};
