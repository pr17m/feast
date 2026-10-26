/**
 * Bridge to the Feast iOS and Android apps (Capacitor shells in /mobile).
 *
 * The apps open this web app from the live country site, and Capacitor's
 * native bridge is available on the page as `window.Capacitor` (see
 * mobile/README.md). This module talks to the native plugins through that
 * bridge directly, so the web build needs no Capacitor packages and every
 * function is a harmless no-op in a normal browser and during SSR.
 *
 * Native features used:
 * - FirebaseMessaging (@capacitor-firebase/messaging): push notifications on
 *   iOS and Android through Firebase Cloud Messaging; the FCM token is
 *   registered with the API (POST /v1/me/push-tokens) after sign-in and
 *   removed on sign-out. A tap on a notification opens `data.path`.
 * - BarcodeScanner (@capacitor-mlkit/barcode-scanning): scan a restaurant's
 *   Feast QR code (opens /r/{code} in the app).
 * - App (@capacitor/app): Feast links (universal/app links, feast:// and
 *   feastpartner://) opened while the app runs.
 */
import { isNativeApp } from "./platform";
import { apiBaseUrl } from "./api/client";
import { AUTH_EVENT, getToken } from "./api/token";

export type NativePlatform = "ios" | "android";
export type NativeAppKind = "diner" | "partner";

type ListenerHandle = { remove: () => Promise<void> | void };
interface CapacitorBridge {
  isNativePlatform?: () => boolean;
  getPlatform?: () => string;
  isPluginAvailable?: (name: string) => boolean;
  getServerUrl?: () => string;
  nativePromise?: <T>(plugin: string, method: string, options?: object) => Promise<T>;
  addListener?: (plugin: string, event: string, callback: (data: unknown) => void) => ListenerHandle;
}

function bridge(): CapacitorBridge | null {
  if (!isNativeApp()) return null;
  return (window as unknown as { Capacitor?: CapacitorBridge }).Capacitor ?? null;
}

export function nativePlatform(): NativePlatform | null {
  const p = bridge()?.getPlatform?.();
  return p === "ios" || p === "android" ? p : null;
}

/** Which store app this is (from the user agent the shells append: "FeastApp/diner"). */
export function nativeAppKind(): NativeAppKind | null {
  if (!isNativeApp()) return null;
  const m = /FeastApp\/(diner|partner)/.exec(navigator.userAgent);
  return m ? (m[1] as NativeAppKind) : null;
}

export function hasNativePlugin(name: string): boolean {
  const b = bridge();
  return !!b?.nativePromise && !!b.isPluginAvailable?.(name);
}

function call<T>(plugin: string, method: string, options: object = {}): Promise<T> {
  const b = bridge();
  if (!b?.nativePromise || !b.isPluginAvailable?.(plugin)) return Promise.reject(new Error(`${plugin} isn't available in this app`));
  return b.nativePromise<T>(plugin, method, options);
}

function listen(plugin: string, event: string, cb: (data: unknown) => void): () => void {
  const b = bridge();
  if (!b?.addListener || !b.isPluginAvailable?.(plugin)) return () => {};
  const handle = b.addListener(plugin, event, cb);
  return () => {
    void handle?.remove();
  };
}

// ---------------------------------------------------------------- navigation

type Navigate = (path: string) => void;

/**
 * Opens a Feast link inside the app. Same-site links use the in-app router;
 * other https links are handed to the shell (another country's Feast site
 * stays in the app, anything else opens in the browser); custom-scheme links
 * (feast://r/abc) map to the path. Returns false for anything else.
 */
export function openFeastLink(raw: string, navigate: Navigate): boolean {
  let url: URL;
  try {
    url = new URL(raw, window.location.href);
  } catch {
    return false;
  }
  if (url.protocol === "https:" || url.protocol === "http:") {
    if (url.host === window.location.host) {
      navigate(url.pathname + url.search + url.hash);
      return true;
    }
    // Another country's Feast site stays in the app (the shell allows those
    // hosts); any other site opens in the system browser.
    window.location.assign(url.toString());
    return true;
  }
  if (url.protocol === "feast:" || url.protocol === "feastpartner:") {
    // feast://r/abc → host "r", path "/abc"
    const path = "/" + `${url.host}${url.pathname}`.replace(/^\/+/, "") + url.search;
    navigate(path);
    return true;
  }
  return false;
}

/** Back to the app's bundled country picker (it forgets the saved country). */
export function changeCountry(): void {
  const local = bridge()?.getServerUrl?.();
  if (!local) return;
  // The picker is a page bundled in the app (capacitor://localhost or https://localhost), not a Next.js route.
  // eslint-disable-next-line @next/next/no-location-assign-relative-destination
  window.location.assign(`${local.replace(/\/$/, "")}/index.html?change=1`);
}

// ---------------------------------------------------------------- QR scanning

export type ScanResult = { ok: true; value: string } | { ok: false; reason: "cancelled" | "unavailable" | "preparing" | "error"; message?: string };

const INSTALL_COMPLETED = 4;
const INSTALL_FAILED = 5;
const INSTALL_CANCELED = 3;

/** Opens the native QR scanner and returns the scanned text. */
export async function scanQrCode(): Promise<ScanResult> {
  if (!hasNativePlugin("BarcodeScanner")) return { ok: false, reason: "unavailable" };
  try {
    if (nativePlatform() === "android") {
      // Android uses Google's code scanner, a Play services module that may need a one-off download.
      const { available } = await call<{ available: boolean }>("BarcodeScanner", "isGoogleBarcodeScannerModuleAvailable");
      if (!available) {
        const ready = await new Promise<boolean>((resolve) => {
          const stop = listen("BarcodeScanner", "googleBarcodeScannerModuleInstallProgress", (e) => {
            const state = (e as { state?: number } | null)?.state;
            if (state === INSTALL_COMPLETED || state === INSTALL_FAILED || state === INSTALL_CANCELED) {
              stop();
              resolve(state === INSTALL_COMPLETED);
            }
          });
          call("BarcodeScanner", "installGoogleBarcodeScannerModule").catch(() => {
            stop();
            resolve(false);
          });
          setTimeout(() => {
            stop();
            resolve(false);
          }, 60_000);
        });
        if (!ready) return { ok: false, reason: "preparing", message: "The scanner is still downloading. Try again in a moment." };
      }
    }
    const { barcodes } = await call<{ barcodes?: { rawValue?: string; displayValue?: string }[] }>("BarcodeScanner", "scan", {
      formats: ["QR_CODE"],
      autoZoom: true,
    });
    const value = barcodes?.[0]?.rawValue ?? barcodes?.[0]?.displayValue;
    return value ? { ok: true, value } : { ok: false, reason: "cancelled" };
  } catch (e) {
    const message = e instanceof Error ? e.message : String(e);
    if (/cancel/i.test(message)) return { ok: false, reason: "cancelled" };
    return { ok: false, reason: "error", message };
  }
}

/** Whether a scanned value is a Feast restaurant code we can open. */
export function feastQrPath(value: string): string | null {
  try {
    const u = new URL(value, window.location.href);
    if (/^\/(r|t)\/[A-Za-z0-9_-]+\/?$/.test(u.pathname)) return u.pathname.replace(/\/$/, "");
  } catch {
    /* not a URL */
  }
  return null;
}

// ---------------------------------------------------------------- push notifications

const PUSH_KEY = "feast.push.registered"; // `${fcmToken}|${authTokenTail}` last sent to the API

async function pushApi(method: "POST" | "DELETE", authToken: string, fcmToken: string): Promise<boolean> {
  const base = apiBaseUrl();
  const platform = nativePlatform();
  const app = nativeAppKind() ?? "diner";
  const res = await fetch(method === "POST" ? `${base}/v1/me/push-tokens` : `${base}/v1/me/push-tokens/${encodeURIComponent(fcmToken)}`, {
    method,
    headers: { authorization: `Bearer ${authToken}`, ...(method === "POST" ? { "content-type": "application/json" } : {}) },
    body: method === "POST" ? JSON.stringify({ platform, token: fcmToken, app }) : undefined,
  });
  return res.ok;
}

function rememberRegistration(value: string | null) {
  try {
    if (value) window.localStorage.setItem(PUSH_KEY, value);
    else window.localStorage.removeItem(PUSH_KEY);
  } catch {
    /* storage unavailable */
  }
}
function registration(): string | null {
  try {
    return window.localStorage.getItem(PUSH_KEY);
  } catch {
    return null;
  }
}

/** After sign-in: ask for notification permission and register this device. */
export async function registerPush(): Promise<void> {
  const auth = getToken();
  if (!auth || !hasNativePlugin("FirebaseMessaging")) return;
  try {
    const perm = await call<{ receive?: string }>("FirebaseMessaging", "checkPermissions");
    let state = perm.receive;
    if (state === "prompt" || state === "prompt-with-rationale") state = (await call<{ receive?: string }>("FirebaseMessaging", "requestPermissions")).receive;
    if (state !== "granted") return;
    const { token } = await call<{ token?: string }>("FirebaseMessaging", "getToken");
    if (!token) return;
    const key = `${token}|${auth.slice(-16)}`;
    if (registration() === key) return;
    if (await pushApi("POST", auth, token)) rememberRegistration(key);
  } catch {
    // Firebase not configured in this build, or the user declined: the app works without push.
  }
}

/** On sign-out: stop sending this device the signed-out user's notifications. */
export async function unregisterPush(authToken: string | null): Promise<void> {
  const reg = registration();
  rememberRegistration(null);
  if (!authToken || !reg || !hasNativePlugin("FirebaseMessaging")) return;
  const fcmToken = reg.split("|")[0];
  try {
    await pushApi("DELETE", authToken, fcmToken);
  } catch {
    /* best effort; the server also drops tokens FCM reports as unregistered */
  }
}

// ---------------------------------------------------------------- start-up

/**
 * Wires the app shell to the web app: push registration that follows sign-in
 * and sign-out, notification taps and Feast links routed in-app. Returns a
 * cleanup function. No-op outside the apps.
 */
export function initNativeApp(navigate: Navigate): () => void {
  if (!bridge()) return () => {};
  const stops: (() => void)[] = [];

  const onAuth = () => {
    if (getToken()) void registerPush();
  };
  onAuth();
  window.addEventListener(AUTH_EVENT, onAuth);
  stops.push(() => window.removeEventListener(AUTH_EVENT, onAuth));

  // FCM rotates tokens now and then: register the new one.
  stops.push(
    listen("FirebaseMessaging", "tokenReceived", () => {
      rememberRegistration(null);
      void registerPush();
    }),
  );
  // Tapping a notification: open the page it points at (data.path, e.g. "/bookings/bk_123").
  stops.push(
    listen("FirebaseMessaging", "notificationActionPerformed", (e) => {
      const data = (e as { notification?: { data?: Record<string, unknown>; link?: string } } | null)?.notification;
      const target = (typeof data?.data?.path === "string" && data.data.path) || (typeof data?.data?.link === "string" && data.data.link) || data?.link;
      if (typeof target === "string" && target) openFeastLink(target, navigate);
    }),
  );
  // Feast links opened while the app is running.
  stops.push(
    listen("App", "appUrlOpen", (e) => {
      const url = (e as { url?: string } | null)?.url;
      if (url) openFeastLink(url, navigate);
    }),
  );
  return () => stops.forEach((s) => s());
}
