"use client";

import Link from "next/link";
import { useState } from "react";
import { ArrowLeft, PiggyBank } from "lucide-react";
import { Art } from "@/components/art";
import { BarList, Columns, Panel, PeriodPicker, monthLabel } from "@/components/insights";
import { useAuth } from "@/components/providers";
import { Empty, ErrorState, LinkButton, Skeleton } from "@/components/ui";
import { useQuery } from "@/lib/hooks";
import { compactMoney, money } from "@/lib/format";

type Days = 30 | 90 | 365 | 0;
const PERIODS: { value: Days; label: string }[] = [
  { value: 0, label: "All time" },
  { value: 365, label: "12 months" },
  { value: 90, label: "90 days" },
  { value: 30, label: "30 days" },
];

export default function MyInsightsPage() {
  const { user, ready } = useAuth();
  const [days, setDays] = useState<Days>(0);
  const q = useQuery(user?.role === "CUSTOMER" ? `myInsights:${days}` : null, (api) => api.myInsights(days));

  if (!ready) return <Skeleton className="mx-auto h-64 max-w-2xl" />;
  if (!user)
    return (
      <div className="mx-auto max-w-md text-center">
        <LinkButton href="/login?next=/account/insights">Sign in to see what you&apos;ve saved</LinkButton>
      </div>
    );
  const d = q.data;

  return (
    <div className="mx-auto max-w-3xl space-y-5">
      <Link href="/account" className="inline-flex items-center gap-1 text-sm font-semibold text-brand">
        <ArrowLeft className="h-4 w-4" aria-hidden /> Account
      </Link>
      <div className="space-y-3">
        <h1 className="text-2xl font-black">Your Feast savings</h1>
        <PeriodPicker value={days} options={PERIODS} onChange={setDays} />
      </div>

      {q.error && !d ? (
        <ErrorState error={q.error} onRetry={q.reload} />
      ) : !d ? (
        <Skeleton className="h-80 w-full" />
      ) : d.visits === 0 && d.billsPaid === 0 && d.bookings.upcoming === 0 ? (
        <Empty
          title="No Feast visits yet"
          body="Book a table or pay your bill on Feast and your savings show up here."
          action={<LinkButton href="/">Find a restaurant</LinkButton>}
        />
      ) : (
        <>
          <section className="flex items-center gap-4 rounded-2xl bg-deal-soft p-5 ring-1 ring-deal/30">
            <PiggyBank className="h-10 w-10 shrink-0 text-deal" aria-hidden />
            <div>
              <p className="text-3xl font-black text-deal">{money(d.totalSaved, d.currency)}</p>
              <p className="text-sm">
                saved on {d.billsPaid} {d.billsPaid === 1 ? "bill" : "bills"} paid on Feast · {money(d.totalSpent, d.currency)} paid in total
              </p>
            </div>
          </section>

          <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
            <Tile label="Visits" value={d.visits} hint={`${d.restaurantsVisited} ${d.restaurantsVisited === 1 ? "restaurant" : "restaurants"}`} />
            <Tile label="Bookings kept" value={d.bookings.honoured} hint={`${d.bookings.upcoming} coming up`} />
            <Tile label="Cancelled" value={d.bookings.cancelled} hint={d.bookings.noShow ? `${d.bookings.noShow} missed` : "none missed"} />
            <Tile label="Reviews written" value={d.reviewsWritten} hint="verified visits only" />
          </div>

          {d.favouriteRestaurants.length > 0 && (
            <Panel title="Your favourites">
              <ul className="space-y-3">
                {d.favouriteRestaurants.map((f) => (
                  <li key={f.restaurant.id}>
                    <Link href={`/restaurants/${f.restaurant.slug}`} className="flex items-center gap-3 rounded-xl p-1 hover:bg-canvas">
                      <Art
                        hue={f.restaurant.art.hue}
                        motif={f.restaurant.art.motif}
                        image={f.restaurant.art.image}
                        credit={f.restaurant.art.imageCredit}
                        className="h-12 w-12 shrink-0 rounded-xl"
                        iconClassName="h-8 w-8"
                      />
                      <span className="min-w-0 flex-1">
                        <span className="block truncate font-semibold">{f.restaurant.name}</span>
                        <span className="text-sm text-muted">
                          {f.visits} {f.visits === 1 ? "visit" : "visits"} · {f.restaurant.area}
                        </span>
                      </span>
                      {f.saved > 0 && <span className="text-sm font-semibold text-deal">saved {money(f.saved, d.currency)}</span>}
                    </Link>
                  </li>
                ))}
              </ul>
            </Panel>
          )}

          <div className="grid gap-4 md:grid-cols-2">
            <Panel title="Savings by month">
              {d.byMonth.some((m) => m.saved > 0) ? (
                <Columns label="Savings by month" data={d.byMonth.slice(-6).map((m) => ({ label: monthLabel(m.month), value: m.saved }))} format={(v) => compactMoney(v, d.currency)} />
              ) : (
                <p className="text-sm text-muted">Pay your bill on Feast to get the restaurant&apos;s deal.</p>
              )}
            </Panel>
            <Panel title="Cuisines you love">
              <BarList rows={d.favouriteCuisines.map((c) => ({ key: c.name, label: c.name, value: c.count, display: `${c.count} ${c.count === 1 ? "visit" : "visits"}` }))} />
            </Panel>
          </div>
        </>
      )}
    </div>
  );
}

function Tile({ label, value, hint }: { label: string; value: number; hint: string }) {
  return (
    <div className="rounded-2xl bg-surface p-4 ring-1 ring-line">
      <p className="text-xs font-semibold uppercase tracking-wide text-muted">{label}</p>
      <p className="mt-1 text-2xl font-black">{value}</p>
      <p className="text-xs text-muted">{hint}</p>
    </div>
  );
}
