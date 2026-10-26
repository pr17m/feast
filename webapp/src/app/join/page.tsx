"use client";

import Link from "next/link";
import { useState } from "react";
import { useRouter } from "next/navigation";
import { CheckCircle2, FileCheck2, Store, Wallet } from "lucide-react";
import { useAuth, useCity, useMarkets } from "@/components/providers";
import { PhoneField, phoneE164, type PhoneValue } from "@/components/phone-field";
import { Button, Card } from "@/components/ui";
import { useMutation } from "@/lib/hooks";
import { costLimits, plansFor } from "@/lib/api/market";
import { currencySymbol, money, toMinor } from "@/lib/format";
import type { PartnerSignupRequest } from "@/lib/api/types";

const field = "h-11 w-full rounded-xl bg-canvas px-3 ring-1 ring-line outline-none focus:ring-2 focus:ring-brand";

/** Public self-serve onboarding: owner account + draft restaurant, then documents in the partner portal. */
export default function JoinPage() {
  const router = useRouter();
  const { signIn } = useAuth();
  const { cities, marketForCity, markets } = useMarkets();
  const { cityId: browsing } = useCity();
  const [f, setF] = useState({ name: "", email: "", password: "", restaurant: "", cityId: "", area: "", cuisines: "", cost: "", veg: false });
  const [phone, setPhone] = useState<PhoneValue>({ country: "", national: "" });
  const signup = useMutation((api, req: PartnerSignupRequest) => api.partnerSignup(req));

  const cityId = f.cityId || browsing || cities[0]?.id || "";
  const city = cities.find((c) => c.id === cityId);
  const market = marketForCity(cityId);
  const currency = market?.currency ?? "INR";
  const cost = f.cost ? toMinor(f.cost, currency) : 0;
  const limits = market ? costLimits(market) : null;
  const phoneValue = phone.country ? phone : { ...phone, country: market?.countryCode ?? "IN" };
  const e164 = phoneE164(markets, phoneValue);
  const monthly = market ? plansFor(market)[0] : null;
  const ok = f.name.trim() && f.email.includes("@") && f.password.length >= 8 && f.restaurant.trim() && city && f.area && f.cuisines.trim() && limits && cost >= limits.min && cost <= limits.max && (!phone.national || e164);

  return (
    <div className="mx-auto grid max-w-5xl gap-6 md:grid-cols-[1fr_1.2fr] md:py-6">
      <div className="space-y-4 rounded-3xl bg-brand p-6 text-white md:p-8">
        <Store className="h-8 w-8" aria-hidden />
        <h1 className="text-3xl font-black leading-tight">List your restaurant on Feast</h1>
        <ul className="space-y-3 text-white/90">
          <li className="flex gap-2">
            <FileCheck2 className="mt-0.5 h-5 w-5 shrink-0" aria-hidden /> Create your account, then upload your licences ({market?.documents.filter((d) => d.required).length ?? "a few"} required in {market?.name ?? "your country"}).
          </li>
          <li className="flex gap-2">
            <CheckCircle2 className="mt-0.5 h-5 w-5 shrink-0" aria-hidden /> Feast verifies them — you go live once approved.
          </li>
          <li className="flex gap-2">
            <Wallet className="mt-0.5 h-5 w-5 shrink-0" aria-hidden /> First 30 days free{monthly ? `, then from ${money(monthly.price, monthly.currency)}/month + ${monthly.taxName}` : ""}.
          </li>
        </ul>
        <p className="text-sm text-white/75">
          Already on Feast?{" "}
          <Link href="/login?as=partner" className="font-semibold text-white underline">
            Sign in
          </Link>
        </p>
      </div>

      <Card className="space-y-4 p-5 md:p-8">
        <section className="space-y-3">
          <h2 className="font-bold">You</h2>
          <input value={f.name} onChange={(e) => setF({ ...f, name: e.target.value })} placeholder="Your full name" aria-label="Your full name" autoComplete="name" className={field} />
          <input type="email" value={f.email} onChange={(e) => setF({ ...f, email: e.target.value })} placeholder="Work email" aria-label="Work email" autoComplete="email" className={field} />
          <input type="password" value={f.password} onChange={(e) => setF({ ...f, password: e.target.value })} placeholder="Password (8+ characters)" aria-label="Password" autoComplete="new-password" className={field} />
          <PhoneField value={phoneValue} onChange={setPhone} label="Mobile (optional)" />
        </section>
        <section className="space-y-3">
          <h2 className="font-bold">Your restaurant</h2>
          <input value={f.restaurant} onChange={(e) => setF({ ...f, restaurant: e.target.value })} placeholder="Restaurant name" aria-label="Restaurant name" className={field} />
          <div className="grid grid-cols-2 gap-3">
            <select value={cityId} onChange={(e) => setF({ ...f, cityId: e.target.value, area: "" })} aria-label="City" className={field}>
              {cities.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.name}
                </option>
              ))}
            </select>
            <select value={f.area} onChange={(e) => setF({ ...f, area: e.target.value })} aria-label="Area" className={field}>
              <option value="">Area…</option>
              {city?.areas.map((a) => (
                <option key={a}>{a}</option>
              ))}
            </select>
          </div>
          <input value={f.cuisines} onChange={(e) => setF({ ...f, cuisines: e.target.value })} placeholder="Cuisines, comma separated" aria-label="Cuisines" className={field} />
          <label className="block">
            <span className="mb-1 block text-sm font-medium">Average cost for two ({currencySymbol(currency)})</span>
            <input value={f.cost} onChange={(e) => setF({ ...f, cost: e.target.value.replace(/[^\d.]/g, "") })} inputMode="decimal" className={field} />
          </label>
          <label className="flex items-center gap-2 text-sm">
            <input type="checkbox" checked={f.veg} onChange={(e) => setF({ ...f, veg: e.target.checked })} className="accent-[var(--color-brand)]" /> Pure vegetarian
          </label>
        </section>
        {signup.error && <p className="rounded-xl bg-red-50 p-3 text-sm text-danger">{signup.error.message}</p>}
        <Button
          size="lg"
          className="w-full"
          disabled={!ok}
          loading={signup.pending}
          onClick={async () => {
            const s = await signup.run({
              owner: { name: f.name, email: f.email, password: f.password, phone: e164 ?? undefined },
              restaurant: { name: f.restaurant, cityId, area: f.area, cuisines: f.cuisines.split(",").map((x) => x.trim()).filter(Boolean), costForTwo: cost, isPureVeg: f.veg },
            });
            if (s) {
              signIn(s);
              router.replace(`/partner/documents?r=${s.user.restaurantIds[0]}`);
            }
          }}
        >
          Create account & continue
        </Button>
        <p className="text-xs text-muted">
          By continuing you agree to Feast&apos;s{" "}
          <Link href="/legal/terms#restaurants" className="underline">
            terms
          </Link>{" "}
          and{" "}
          <Link href="/legal/privacy" className="underline">
            privacy policy
          </Link>
          . Your restaurant stays hidden until your documents are approved.
        </p>
      </Card>
    </div>
  );
}
