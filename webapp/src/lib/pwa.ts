/**
 * Installable web app ("Add to Home Screen") on the web — not used inside the
 * iOS/Android store apps, which are native shells around this same site.
 *
 * - Service worker: registered in production browsers only (see /sw.js).
 * - Install prompt: Chrome/Edge/Android fire `beforeinstallprompt`; an inline
 *   script in the root layout keeps that event (it can fire before React
 *   hydrates) on `window.__feastInstallPrompt`, and this module exposes it.
 * - iOS has no prompt API: we show "Share → Add to Home Screen" steps instead.
 */
import { isNativeApp } from "@/lib/platform";
export { INSTALL_PROMPT_CAPTURE } from "@/lib/pwa-capture";

export type InstallApp = "diner" | "partner";

interface BeforeInstallPromptEvent extends Event {
  prompt(): Promise<void>;
  userChoice: Promise<{ outcome: "accepted" | "dismissed" }>;
}

declare global {
  interface Window {
    __feastInstallPrompt?: BeforeInstallPromptEvent | null;
  }
}

export function subscribeInstall(cb: () => void): () => void {
  if (typeof window === "undefined") return () => {};
  window.addEventListener("feast-install-change", cb);
  const mq = window.matchMedia?.("(display-mode: standalone)");
  mq?.addEventListener?.("change", cb);
  return () => {
    window.removeEventListener("feast-install-change", cb);
    mq?.removeEventListener?.("change", cb);
  };
}

/** Already opened from the home screen / as an installed app. */
export function isStandalone(): boolean {
  if (typeof window === "undefined") return false;
  const nav = navigator as Navigator & { standalone?: boolean };
  return nav.standalone === true || window.matchMedia?.("(display-mode: standalone)").matches === true;
}

/** iPhone/iPad (incl. iPadOS, which reports itself as a Mac with touch). */
export function isIos(): boolean {
  if (typeof navigator === "undefined") return false;
  return /iPhone|iPad|iPod/.test(navigator.userAgent) || (navigator.platform === "MacIntel" && navigator.maxTouchPoints > 1);
}

export type InstallMode = "prompt" | "ios" | null;

/** How we can offer installation right now (null = we can't, or it's already installed). */
export function installMode(): InstallMode {
  if (typeof window === "undefined" || isNativeApp() || isStandalone()) return null;
  if (window.__feastInstallPrompt) return "prompt";
  if (isIos()) return "ios";
  return null;
}

/** Show the browser's own install dialog. Resolves true if the user installed. */
export async function promptInstall(): Promise<boolean> {
  const e = window.__feastInstallPrompt;
  if (!e) return false;
  window.__feastInstallPrompt = null;
  window.dispatchEvent(new Event("feast-install-change"));
  try {
    await e.prompt();
    const choice = await e.userChoice;
    return choice.outcome === "accepted";
  } catch {
    return false;
  }
}

const DISMISS_DAYS = 60;
const dismissKey = (app: InstallApp) => `feast.install.dismissed.${app}`;

export function installDismissed(app: InstallApp): boolean {
  try {
    const at = Number(localStorage.getItem(dismissKey(app)) || 0);
    return at > 0 && Date.now() - at < DISMISS_DAYS * 86_400_000;
  } catch {
    return false;
  }
}

export function dismissInstall(app: InstallApp): void {
  try {
    localStorage.setItem(dismissKey(app), String(Date.now()));
  } catch {
    /* private mode: just hide for this visit */
  }
  window.dispatchEvent(new Event("feast-install-change"));
}

/**
 * Register the service worker (production web only). `onUpdate` is called when
 * a new version is installed and waiting; call `applyUpdate()` to switch.
 */
export function registerServiceWorker(onUpdate: (apply: () => void) => void): void {
  if (typeof window === "undefined" || !("serviceWorker" in navigator) || isNativeApp()) return;
  if (process.env.NODE_ENV !== "production") return;
  const sw = navigator.serviceWorker;
  let reloading = false;
  sw.addEventListener("controllerchange", () => {
    if (reloading) return;
    reloading = true;
    window.location.reload();
  });
  sw.register("/sw.js", { scope: "/" })
    .then((reg) => {
      const offer = () => {
        const waiting = reg.waiting;
        if (waiting && sw.controller) onUpdate(() => waiting.postMessage({ type: "SKIP_WAITING" }));
      };
      offer();
      reg.addEventListener("updatefound", () => {
        const next = reg.installing;
        next?.addEventListener("statechange", () => {
          if (next.state === "installed") offer();
        });
      });
      // Check for a new version when the app comes back to the foreground.
      document.addEventListener("visibilitychange", () => {
        if (document.visibilityState === "visible") reg.update().catch(() => {});
      });
    })
    .catch(() => {
      /* unsupported or blocked: the site works without it */
    });
}
