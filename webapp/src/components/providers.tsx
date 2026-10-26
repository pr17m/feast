"use client";

import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { CheckCircle2, AlertCircle } from "lucide-react";
import { LiveProvider } from "./live";
import { useLocalState } from "@/lib/hooks";
import { getApi } from "@/lib/api/client";
import { AUTH_EVENT, setToken } from "@/lib/api/token";
import { registerMarkets, setActiveCurrency } from "@/lib/format";
import type { AuthSession, City, GeoResult, Market, Role, User } from "@/lib/api/types";

// ---------- Markets (countries) & cities ----------

interface MarketsState {
  markets: Market[];
  /** Cities of live markets. */
  cities: City[];
  ready: boolean;
  marketOf: (countryCode: string | undefined) => Market | undefined;
  marketForCity: (cityId: string | undefined) => Market | undefined;
  reload: () => void;
}

const MarketsCtx = createContext<MarketsState>({ markets: [], cities: [], ready: false, marketOf: () => undefined, marketForCity: () => undefined, reload: () => {} });

/** Country config (currency, taxes, payment methods, phone rules, documents). */
export function useMarkets() {
  return useContext(MarketsCtx);
}

// ---------- City ----------

interface CityState {
  cityId: string;
  setCityId: (id: string) => void;
  /** City resolved (saved choice, detected, or default) and markets loaded. */
  ready: boolean;
  /** The visitor appears to be somewhere this deployment doesn't serve. */
  unsupportedCountry?: string;
  /** …and the Feast site that does serve it, if any. */
  countryRedirect?: string;
  /** Pick the nearest city using the browser's location (asks permission). */
  locateMe: () => Promise<boolean>;
}

const CityCtx = createContext<CityState>({ cityId: "", setCityId: () => {}, ready: false, locateMe: async () => false });

export function useCity() {
  return useContext(CityCtx);
}

function distanceKm(a: { lat: number; lng: number }, b: { lat: number; lng: number }): number {
  const rad = (x: number) => (x * Math.PI) / 180;
  const dLat = rad(b.lat - a.lat);
  const dLng = rad(b.lng - a.lng);
  const h = Math.sin(dLat / 2) ** 2 + Math.cos(rad(a.lat)) * Math.cos(rad(b.lat)) * Math.sin(dLng / 2) ** 2;
  return 12742 * Math.asin(Math.sqrt(h));
}

// ---------- Auth ----------

interface AuthState {
  user: User | null;
  /** False until we've asked the backend who the token belongs to. */
  ready: boolean;
  signIn: (s: AuthSession) => void;
  signOut: () => Promise<void>;
  refresh: () => void;
}

const AuthCtx = createContext<AuthState>({ user: null, ready: false, signIn: () => {}, signOut: async () => {}, refresh: () => {} });

export function useAuth() {
  return useContext(AuthCtx);
}

export function homeFor(role: Role): string {
  return role === "ADMIN" ? "/admin" : role === "OWNER" ? "/partner" : "/";
}

// ---------- Toasts ----------

type Toast = { id: number; text: string; tone: "ok" | "error" };
const ToastCtx = createContext<(text: string, tone?: Toast["tone"]) => void>(() => {});

export function useToast() {
  return useContext(ToastCtx);
}

export function Providers({ children }: { children: ReactNode }) {
  // "" = the visitor hasn't chosen; detect from location/IP below.
  const [savedCity, setCityId, storageReady] = useLocalState("feast.city", "");
  const [markets, setMarkets] = useState<Market[]>([]);
  const [cities, setCities] = useState<City[]>([]);
  const [marketsReady, setMarketsReady] = useState(false);
  const [marketsNonce, setMarketsNonce] = useState(0);
  const [geo, setGeo] = useState<GeoResult | null>(null);

  useEffect(() => {
    let alive = true;
    getApi()
      .then((api) => Promise.all([api.listMarkets(), api.listCities()]))
      .then(([m, c]) => {
        if (!alive) return;
        registerMarkets(m);
        setMarkets(m);
        setCities(c);
        setMarketsReady(true);
      })
      .catch(() => alive && setMarketsReady(true));
    return () => {
      alive = false;
    };
  }, [marketsNonce]);

  // First visit: country from the IP / CDN header (server).
  useEffect(() => {
    if (!storageReady || savedCity || geo) return;
    let alive = true;
    getApi()
      .then((api) => api.locate())
      .then((g) => alive && setGeo(g))
      .catch(() => alive && setGeo({ supported: true, source: "default" }));
    return () => {
      alive = false;
    };
  }, [storageReady, savedCity, geo]);

  const known = (id: string) => cities.some((c) => c.id === id);
  const detected = geo?.cityId && known(geo.cityId) ? geo.cityId : cities[0]?.id ?? "";
  const cityId = savedCity && (!marketsReady || known(savedCity) || !cities.length) ? savedCity : detected;
  const ready = storageReady && marketsReady && (!!savedCity || geo !== null) && !!cityId;

  const marketOf = useCallback((code: string | undefined) => markets.find((m) => m.countryCode === code), [markets]);
  const marketForCity = useCallback((id: string | undefined) => marketOf(cities.find((c) => c.id === id)?.countryCode), [cities, marketOf]);
  // Default currency for customer screens = the browsed city's market.
  setActiveCurrency(marketForCity(cityId)?.currency);

  const locateMe = useCallback(async () => {
    if (!("geolocation" in navigator) || !cities.length) return false;
    const pos = await new Promise<GeolocationPosition | null>((resolve) =>
      navigator.geolocation.getCurrentPosition(resolve, () => resolve(null), { timeout: 8000, maximumAge: 600_000 }),
    );
    if (!pos) return false;
    const here = { lat: pos.coords.latitude, lng: pos.coords.longitude };
    const nearest = [...cities].sort((a, b) => distanceKm(here, a) - distanceKm(here, b))[0];
    setCityId(nearest.id);
    return true;
  }, [cities, setCityId]);

  const marketsValue = useMemo(
    () => ({ markets, cities, ready: marketsReady, marketOf, marketForCity, reload: () => setMarketsNonce((n) => n + 1) }),
    [markets, cities, marketsReady, marketOf, marketForCity],
  );
  const unsupportedCountry = !savedCity && geo && !geo.supported ? geo.countryCode : undefined;
  const countryRedirect = unsupportedCountry ? geo?.redirectUrl : undefined;
  const [toasts, setToasts] = useState<Toast[]>([]);
  const [auth, setAuth] = useState<{ user: User | null; ready: boolean }>({ user: null, ready: false });
  const [authNonce, setAuthNonce] = useState(0);

  // Resolve the current user whenever this tab's token changes (sign in/out).
  useEffect(() => {
    let alive = true;
    getApi()
      .then((api) => api.me())
      .then((user) => alive && setAuth({ user, ready: true }))
      .catch(() => alive && setAuth({ user: null, ready: true }));
    return () => {
      alive = false;
    };
  }, [authNonce]);

  useEffect(() => {
    // Identity is per tab (see lib/api/token.ts): other tabs signing in/out don't affect this one.
    const bump = () => setAuthNonce((n) => n + 1);
    window.addEventListener(AUTH_EVENT, bump);
    return () => window.removeEventListener(AUTH_EVENT, bump);
  }, []);

  const signIn = useCallback((s: AuthSession) => {
    setAuth({ user: s.user, ready: true });
    setToken(s.token);
  }, []);

  const signOut = useCallback(async () => {
    try {
      await (await getApi()).logout();
    } finally {
      setAuth({ user: null, ready: true });
      setToken(null);
    }
  }, []);

  const refresh = useCallback(() => setAuthNonce((n) => n + 1), []);

  const toast = useCallback((text: string, tone: Toast["tone"] = "ok") => {
    const id = Date.now() + Math.random();
    setToasts((t) => [...t, { id, text, tone }]);
    setTimeout(() => setToasts((t) => t.filter((x) => x.id !== id)), 3200);
  }, []);

  return (
    <AuthCtx.Provider value={{ ...auth, signIn, signOut, refresh }}>
      <MarketsCtx.Provider value={marketsValue}>
      <CityCtx.Provider value={{ cityId, setCityId, ready, unsupportedCountry, countryRedirect, locateMe }}>
        <ToastCtx.Provider value={toast}>
          <LiveProvider>{children}</LiveProvider>
          <div
            aria-live="polite"
            className="pointer-events-none fixed inset-x-0 bottom-24 z-[60] flex flex-col items-center gap-2 px-4 md:bottom-8"
          >
            {toasts.map((t) => (
              <div
                key={t.id}
                className="animate-sheet-in flex max-w-sm items-center gap-2 rounded-full bg-ink px-4 py-2.5 text-sm text-white shadow-lg"
              >
                {t.tone === "ok" ? (
                  <CheckCircle2 className="h-4 w-4 shrink-0 text-emerald-300" aria-hidden />
                ) : (
                  <AlertCircle className="h-4 w-4 shrink-0 text-red-300" aria-hidden />
                )}
                {t.text}
              </div>
            ))}
          </div>
        </ToastCtx.Provider>
      </CityCtx.Provider>
      </MarketsCtx.Provider>
    </AuthCtx.Provider>
  );
}
