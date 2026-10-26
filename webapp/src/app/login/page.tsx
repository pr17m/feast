"use client";

import Link from "next/link";
import { Suspense, useState } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import clsx from "clsx";
import { Building2, Smartphone } from "lucide-react";
import { homeFor, useAuth, useCity, useMarkets } from "@/components/providers";
import { Logo } from "@/components/app-shell";
import { PhoneField, phoneE164, type PhoneValue } from "@/components/phone-field";
import { Button, Sheet, Skeleton } from "@/components/ui";
import { useMutation, useQuery } from "@/lib/hooks";
import { PROVIDER_LABEL, demoToken, signInWith } from "@/lib/social";
import type { AuthProvider, AuthProviderInfo, AuthSession, OAuthLoginRequest } from "@/lib/api/types";

function ProviderIcon({ p }: { p: AuthProvider }) {
  if (p === "GOOGLE")
    return (
      <svg viewBox="0 0 24 24" className="h-5 w-5" aria-hidden>
        <path fill="#4285F4" d="M23.5 12.3c0-.8-.1-1.6-.2-2.3H12v4.4h6.5c-.3 1.5-1.1 2.7-2.4 3.6v3h3.9c2.2-2.1 3.5-5.1 3.5-8.7z" />
        <path fill="#34A853" d="M12 24c3.2 0 6-1.1 7.9-2.9l-3.9-3c-1.1.7-2.4 1.2-4 1.2-3.1 0-5.7-2.1-6.6-4.9h-4v3.1C3.4 21.4 7.4 24 12 24z" />
        <path fill="#FBBC05" d="M5.4 14.4c-.2-.7-.4-1.5-.4-2.4s.1-1.6.4-2.4V6.5h-4C.5 8.2 0 10 0 12s.5 3.8 1.4 5.5l4-3.1z" />
        <path fill="#EA4335" d="M12 4.8c1.8 0 3.3.6 4.6 1.8l3.4-3.4C17.9 1.2 15.2 0 12 0 7.4 0 3.4 2.6 1.4 6.5l4 3.1C6.3 6.9 8.9 4.8 12 4.8z" />
      </svg>
    );
  if (p === "APPLE")
    return (
      <svg viewBox="0 0 24 24" className="h-5 w-5" aria-hidden>
        <path fill="currentColor" d="M16.4 12.6c0-2.5 2.1-3.7 2.2-3.8-1.2-1.7-3-2-3.7-2-1.6-.2-3 .9-3.8.9-.8 0-2-.9-3.3-.9-1.7 0-3.3 1-4.2 2.5-1.8 3.1-.5 7.7 1.3 10.2.9 1.2 1.9 2.6 3.2 2.6 1.3-.1 1.8-.8 3.3-.8s2 .8 3.3.8c1.4 0 2.3-1.3 3.1-2.5 1-1.4 1.4-2.8 1.4-2.9-.1 0-2.8-1.1-2.8-4.1zM13.9 5.2c.7-.8 1.2-2 1-3.2-1 .1-2.2.7-2.9 1.5-.6.7-1.2 1.9-1 3 1.1.1 2.2-.5 2.9-1.3z" />
      </svg>
    );
  return (
    <svg viewBox="0 0 24 24" className="h-5 w-5" aria-hidden>
      <path fill="#1877F2" d="M24 12a12 12 0 1 0-13.9 11.9v-8.4H7.1V12h3V9.4c0-3 1.8-4.7 4.5-4.7 1.3 0 2.7.2 2.7.2v3h-1.5c-1.5 0-2 .9-2 1.9V12h3.4l-.5 3.5h-2.9v8.4A12 12 0 0 0 24 12z" />
    </svg>
  );
}

/**
 * Test-mode sign-in, shown only when the server's social login runs in test
 * mode (development: no real Google/Apple/Facebook app configured). Production
 * servers refuse test mode, so this never appears there.
 */
function TestProviderSheet({ provider, onClose, onPick }: { provider: AuthProvider | null; onClose: () => void; onPick: (req: OAuthLoginRequest) => void }) {
  const [email, setEmail] = useState("");
  const [name, setName] = useState("");
  const label = provider ? PROVIDER_LABEL[provider] : "";
  return (
    <Sheet open={!!provider} onClose={onClose} title={`Continue with ${label} (test mode)`}>
      <p className="mb-3 text-sm text-muted">
        This server has no {label} app configured, so sign-in runs in test mode: enter a name and email to continue as that diner.
      </p>
      <form
        className="space-y-2"
        onSubmit={(e) => {
          e.preventDefault();
          onPick({ idToken: demoToken(email, name), accessToken: demoToken(email, name) });
        }}
      >
        <input value={name} onChange={(e) => setName(e.target.value)} placeholder="Name" aria-label="Name" className={input} />
        <input type="email" value={email} onChange={(e) => setEmail(e.target.value)} placeholder="Email" aria-label="Email" className={input} required />
        <Button type="submit" className="w-full" disabled={!email.includes("@")}>
          Continue
        </Button>
      </form>
    </Sheet>
  );
}

const input =
  "h-12 w-full rounded-xl bg-canvas px-3 ring-1 ring-line outline-none focus:ring-2 focus:ring-brand";

function LoginInner() {
  const sp = useSearchParams();
  const router = useRouter();
  const { signIn } = useAuth();
  const next = sp.get("next");
  const [tab, setTab] = useState<"otp" | "email">(sp.get("as") === "partner" ? "email" : "otp");
  const { markets, marketForCity } = useMarkets();
  const { cityId } = useCity();
  const [phone, setPhone] = useState<PhoneValue>({ country: "", national: "" });
  // Default the dial code to the detected/browsed country.
  const defaultCountry = marketForCity(cityId)?.countryCode ?? markets[0]?.countryCode ?? "";
  const phoneValue = phone.country ? phone : { ...phone, country: defaultCountry };
  const e164 = phoneE164(markets, phoneValue);
  const providers = useQuery("authProviders", (api) => api.authProviders());
  const [demoProvider, setDemoProvider] = useState<AuthProvider | null>(null);
  const [socialError, setSocialError] = useState<string | null>(null);
  const [otp, setOtp] = useState("");
  const [name, setName] = useState("");
  const [otpSent, setOtpSent] = useState<string | null>(null);
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");

  const sendOtp = useMutation((api, p: string) => api.requestOtp(p));
  const verify = useMutation((api, p: string, o: string, n?: string) => api.verifyOtp(p, o, n));
  const login = useMutation((api, e: string, pw: string) => api.login(e, pw));
  const oauth = useMutation((api, p: AuthProvider, req: OAuthLoginRequest) => api.oauthLogin(p, req));

  const social = async (info: AuthProviderInfo) => {
    setSocialError(null);
    if (info.demo) return setDemoProvider(info.provider);
    try {
      done(await oauth.run(info.provider, await signInWith(info)));
    } catch (e) {
      setSocialError((e as Error).message);
    }
  };

  const done = (s: AuthSession | undefined) => {
    if (!s) return;
    signIn(s);
    // Only follow `next` if it's an internal path; otherwise land on the role's home.
    const safeNext = next && next.startsWith("/") && !next.startsWith("//") ? next : null;
    router.replace(safeNext ?? homeFor(s.user.role));
  };

  const error = tab === "otp" ? sendOtp.error ?? verify.error ?? oauth.error : login.error;

  return (
    <div className="mx-auto grid max-w-4xl gap-6 md:grid-cols-2 md:items-start md:py-6">
      <div className="hidden rounded-3xl bg-brand p-8 text-white md:block">
        <Logo className="text-white [&>span]:text-orange-300" />
        <h1 className="mt-6 text-3xl font-black leading-tight">Dine out better.</h1>
        <p className="mt-2 text-white/85">Pre-book tables with deals and pay your bill in seconds.</p>
        <ul className="mt-6 space-y-2 text-sm text-white/85">
          <li>• Diners sign in with their mobile number, Google, Apple or Facebook</li>
          <li>• Restaurant partners and Feast admins use email</li>
        </ul>
      </div>

      <div className="space-y-5 rounded-3xl bg-surface p-5 ring-1 ring-line md:p-8">
        <div>
          <h2 className="text-2xl font-black tracking-tight">Sign in</h2>
          <p className="text-sm text-muted">{next ? "Sign in to continue." : "Welcome to Feast."}</p>
        </div>

        <div className="grid grid-cols-2 gap-1 rounded-xl bg-canvas p-1" role="tablist">
          {(
            [
              ["otp", "Diner", Smartphone],
              ["email", "Partner / Admin", Building2],
            ] as const
          ).map(([id, label, Icon]) => (
            <button
              key={id}
              role="tab"
              aria-selected={tab === id}
              onClick={() => setTab(id)}
              className={clsx("flex h-10 items-center justify-center gap-1.5 rounded-lg text-sm font-semibold", tab === id ? "bg-surface shadow-sm" : "text-muted")}
            >
              <Icon className="h-4 w-4" aria-hidden /> {label}
            </button>
          ))}
        </div>

        {tab === "otp" ? (
          <form
            className="space-y-3"
            onSubmit={async (e) => {
              e.preventDefault();
              if (!otpSent) {
                if (!e164) return;
                const res = await sendOtp.run(e164);
                if (res) setOtpSent(res.hint ?? "OTP sent");
              } else if (e164) done(await verify.run(e164, otp, name || undefined));
            }}
          >
            <PhoneField
              value={phoneValue}
              onChange={(v) => {
                setPhone(v);
                setOtpSent(null);
              }}
              tall
              autoFocus
            />
            {otpSent && (
              <>
                <label className="block">
                  <span className="mb-1 block text-sm font-medium">OTP</span>
                  <input
                    value={otp}
                    onChange={(e) => setOtp(e.target.value.replace(/\D/g, "").slice(0, 6))}
                    inputMode="numeric"
                    autoComplete="one-time-code"
                    className={clsx(input, "tracking-[0.5em]")}
                    autoFocus
                  />
                  <span className="mt-1 block text-xs font-semibold text-deal">{otpSent}</span>
                </label>
                <label className="block">
                  <span className="mb-1 block text-sm font-medium">Your name (new users)</span>
                  <input value={name} onChange={(e) => setName(e.target.value)} autoComplete="name" className={input} />
                </label>
              </>
            )}
            <Button type="submit" size="lg" className="w-full" loading={sendOtp.pending || verify.pending} disabled={!e164 || (!!otpSent && otp.length !== 6)}>
              {otpSent ? "Verify & sign in" : "Get OTP"}
            </Button>
            {!!providers.data?.length && (
              <div className="space-y-2 pt-1">
                <p className="flex items-center gap-3 text-xs font-semibold uppercase tracking-wide text-faint">
                  <span className="h-px flex-1 bg-line" /> or <span className="h-px flex-1 bg-line" />
                </p>
                {providers.data.map((p) => (
                  <Button key={p.provider} type="button" variant="secondary" size="lg" className="w-full" disabled={oauth.pending} onClick={() => social(p)}>
                    <ProviderIcon p={p.provider} /> Continue with {PROVIDER_LABEL[p.provider]}
                  </Button>
                ))}
                {socialError && <p className="text-sm text-danger">{socialError}</p>}
              </div>
            )}
            <p className="text-xs text-muted">
              By continuing you agree to Feast&apos;s{" "}
              <Link href="/legal/terms" className="underline">
                terms
              </Link>{" "}
              and{" "}
              <Link href="/legal/privacy" className="underline">
                privacy policy
              </Link>
              .
            </p>
          </form>
        ) : (
          <form
            className="space-y-3"
            onSubmit={async (e) => {
              e.preventDefault();
              done(await login.run(email, password));
            }}
          >
            <label className="block">
              <span className="mb-1 block text-sm font-medium">Work email</span>
              <input type="email" value={email} onChange={(e) => setEmail(e.target.value)} autoComplete="email" className={input} autoFocus />
            </label>
            <label className="block">
              <span className="mb-1 block text-sm font-medium">Password</span>
              <input type="password" value={password} onChange={(e) => setPassword(e.target.value)} autoComplete="current-password" className={input} />
            </label>
            <Button type="submit" size="lg" className="w-full" loading={login.pending} disabled={!email || !password}>
              Sign in
            </Button>
          </form>
        )}

        {error && (
          <p role="alert" className="rounded-xl bg-red-50 p-3 text-sm text-danger">
            {error.message}
          </p>
        )}

        <p className="text-center text-sm text-muted">
          Own a restaurant?{" "}
          <Link href="/join" className="font-semibold text-brand">
            List it on Feast
          </Link>
        </p>
        <TestProviderSheet
          provider={demoProvider}
          onClose={() => setDemoProvider(null)}
          onPick={async (req) => {
            const p = demoProvider!;
            setDemoProvider(null);
            done(await oauth.run(p, req));
          }}
        />
      </div>
    </div>
  );
}

export default function LoginPage() {
  return (
    <Suspense fallback={<Skeleton className="mx-auto h-96 max-w-md" />}>
      <LoginInner />
    </Suspense>
  );
}
