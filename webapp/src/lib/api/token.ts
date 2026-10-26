/**
 * Where the client keeps its auth token. The HTTP client sends it as a Bearer token;
 * the backend authenticates every call with it. In production prefer an
 * httpOnly cookie set by the backend and drop this file.
 *
 * Each browser tab has its own signed-in user (sessionStorage), so you can run the
 * customer app in one tab and the owner portal in another side by side. A new tab
 * starts as the most recent sign-in (localStorage) and then keeps its own identity:
 * signing in elsewhere never switches an open tab to a different user.
 */
const TAB_KEY = "feast.auth.token";
const LAST_KEY = "feast.auth.last";
export const AUTH_EVENT = "feast-auth-changed";

export function getToken(): string | null {
  try {
    const tab = window.sessionStorage.getItem(TAB_KEY);
    if (tab !== null) return tab || null; // "" = this tab signed out
    const last = window.localStorage.getItem(LAST_KEY);
    if (last) window.sessionStorage.setItem(TAB_KEY, last);
    return last;
  } catch {
    return null;
  }
}

export function setToken(token: string | null): void {
  try {
    const previous = window.sessionStorage.getItem(TAB_KEY);
    window.sessionStorage.setItem(TAB_KEY, token ?? "");
    if (token) window.localStorage.setItem(LAST_KEY, token);
    // Signing out: new tabs shouldn't come up as the user who just left.
    else if (previous && window.localStorage.getItem(LAST_KEY) === previous) window.localStorage.removeItem(LAST_KEY);
  } catch {
    // storage unavailable — session lasts until reload
  }
  window.dispatchEvent(new Event(AUTH_EVENT));
}
