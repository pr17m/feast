/**
 * Social sign-in on the web. Each provider's own SDK shows its consent UI and
 * returns a credential that the backend verifies (POST /v1/auth/oauth/{provider}).
 * In demo mode (no client ids configured) the caller shows a fake account picker
 * and builds a `demo:` token instead — see `demoToken`.
 */
import type { AuthProvider, AuthProviderInfo, OAuthLoginRequest } from "./api/types";

const scripts = new Map<string, Promise<void>>();

function loadScript(src: string): Promise<void> {
  let p = scripts.get(src);
  if (!p) {
    p = new Promise<void>((resolve, reject) => {
      const el = document.createElement("script");
      el.src = src;
      el.async = true;
      el.onload = () => resolve();
      el.onerror = () => reject(new Error(`Couldn't load ${new URL(src).host}`));
      document.head.appendChild(el);
    });
    scripts.set(src, p);
  }
  return p;
}

/** A random nonce, bound into the ID token to stop replays. */
function nonce(): string {
  return Array.from(crypto.getRandomValues(new Uint8Array(16)), (b) => b.toString(16).padStart(2, "0")).join("");
}

type GoogleId = {
  accounts: { id: { initialize: (o: object) => void; prompt: (cb?: (n: { isNotDisplayed: () => boolean; isSkippedMoment: () => boolean }) => void) => void } };
};
type AppleId = { auth: { init: (o: object) => void; signIn: () => Promise<{ authorization: { id_token: string }; user?: { name?: { firstName?: string; lastName?: string } } }> } };
type FacebookSdk = { init: (o: object) => void; login: (cb: (r: { authResponse?: { accessToken: string } }) => void, o: object) => void };

/** Run the provider's real sign-in and return what the backend needs. */
export async function signInWith(info: AuthProviderInfo): Promise<OAuthLoginRequest> {
  if (!info.clientId) throw new Error("This sign-in option isn't configured");
  const w = window as unknown as { google?: GoogleId; AppleID?: AppleId; FB?: FacebookSdk };
  switch (info.provider) {
    case "GOOGLE": {
      await loadScript("https://accounts.google.com/gsi/client");
      const n = nonce();
      return new Promise((resolve, reject) => {
        w.google!.accounts.id.initialize({
          client_id: info.clientId,
          nonce: n,
          callback: (r: { credential?: string }) => (r.credential ? resolve({ idToken: r.credential, nonce: n }) : reject(new Error("Google sign-in was cancelled"))),
        });
        w.google!.accounts.id.prompt((note) => {
          if (note.isNotDisplayed() || note.isSkippedMoment()) reject(new Error("Google sign-in was closed. Allow third-party sign-in prompts and try again."));
        });
      });
    }
    case "APPLE": {
      await loadScript("https://appleid.cdn-apple.com/appleauth/static/jsapi/appleid/1/en_US/appleid.auth.js");
      const n = nonce();
      w.AppleID!.auth.init({ clientId: info.clientId, scope: "name email", redirectURI: info.redirectUri ?? window.location.origin, usePopup: true, nonce: n });
      const r = await w.AppleID!.auth.signIn();
      const name = [r.user?.name?.firstName, r.user?.name?.lastName].filter(Boolean).join(" ") || undefined;
      return { idToken: r.authorization.id_token, nonce: n, name };
    }
    case "FACEBOOK": {
      await loadScript("https://connect.facebook.net/en_US/sdk.js");
      w.FB!.init({ appId: info.clientId, version: "v19.0", cookie: false, xfbml: false });
      return new Promise((resolve, reject) =>
        w.FB!.login((r) => (r.authResponse ? resolve({ accessToken: r.authResponse.accessToken }) : reject(new Error("Facebook sign-in was cancelled"))), { scope: "public_profile,email" }),
      );
    }
  }
}

/** Demo-mode credential (accepted only by servers with `auth.oauth.<p>.mode = "demo"`). */
export function demoToken(email: string, name: string): string {
  const json = JSON.stringify({ email, name });
  const b64 = btoa(unescape(encodeURIComponent(json))).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
  return `demo:${b64}`;
}

export const PROVIDER_LABEL: Record<AuthProvider, string> = { GOOGLE: "Google", APPLE: "Apple", FACEBOOK: "Facebook" };
