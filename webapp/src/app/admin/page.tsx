"use client";

import Link from "next/link";
import { PageHeader, Stat } from "@/components/portal-shell";
import { ErrorState, Skeleton } from "@/components/ui";
import { useQuery } from "@/lib/hooks";
import { money } from "@/lib/format";

export default function AdminOverview() {
  const m = useQuery("adminMetrics", (api) => api.adminMetrics());

  if (m.error) return <ErrorState error={m.error} onRetry={m.reload} />;
  if (!m.data) return <Skeleton className="h-96 w-full" />;
  const d = m.data;
  const pending = d.restaurants.PENDING_APPROVAL;
  const subs = d.subscriptions;

  return (
    <div className="space-y-6">
      <PageHeader
        title="Platform overview"
        subtitle="Everything on Feast in this country, by city."
      />

      {pending > 0 && (
        <Link href="/admin/approvals" className="block rounded-2xl bg-amber-50 p-4 font-semibold text-amber-900 ring-1 ring-amber-200">
          {pending} restaurant{pending === 1 ? " is" : "s are"} waiting for document review →
        </Link>
      )}

      <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        {d.payments.byCurrency.map((c) => (
          <Stat key={c.currency} label={`GMV · ${c.currency}`} value={money(c.gmv, c.currency)} hint={`Diners saved ${money(c.savings, c.currency)}`} tone="brand" />
        ))}
        <Stat label="Bookings" value={d.bookings.total} hint={`${d.bookings.upcoming} upcoming · ${d.bookings.cancelled} cancelled`} />
      </div>

      <section className="rounded-2xl bg-surface p-4 ring-1 ring-line">
        <div className="mb-3 flex flex-wrap items-baseline justify-between gap-2">
          <h2 className="font-bold">Restaurant subscriptions</h2>
          <span className="flex gap-3 text-sm">
            <Link href="/admin/subscriptions" className="font-semibold text-brand">
              All subscriptions →
            </Link>
            <Link href="/admin/platform-payments" className="font-semibold text-brand">
              Platform payments →
            </Link>
          </span>
        </div>
        <div className="grid grid-cols-2 gap-3 lg:grid-cols-5">
          <Stat label="Paying" value={subs.active} />
          <Stat label="In free trial" value={subs.trial} hint={subs.trialsEndingSoon ? `${subs.trialsEndingSoon} ending within 7 days` : undefined} />
          <Stat label="Past due" value={subs.pastDue} tone={subs.pastDue ? "deal" : undefined} hint="In grace period" />
          <Stat label="Expired" value={subs.expired} hint="Hidden from diners" />
          {subs.revenue.map((r) => (
            <Stat key={r.currency} label={`MRR · ${r.currency}`} value={money(r.mrr, r.currency)} hint={`${money(r.thisMonth, r.currency)} collected this month`} tone="brand" />
          ))}
        </div>
      </section>

      <div className="grid gap-4 lg:grid-cols-2">
        <section className="rounded-2xl bg-surface p-4 ring-1 ring-line">
          <h2 className="mb-3 font-bold">Restaurants by status</h2>
          <dl className="grid grid-cols-2 gap-3">
            {Object.entries(d.restaurants).map(([k, v]) => (
              <Link key={k} href={`/admin/restaurants?status=${k}`} className="rounded-xl bg-canvas p-3 hover:ring-1 hover:ring-brand">
                <dt className="text-xs font-semibold uppercase tracking-wide text-muted">{k.replace("_", " ").toLowerCase()}</dt>
                <dd className="text-xl font-black">{v}</dd>
              </Link>
            ))}
          </dl>
        </section>
        <section className="rounded-2xl bg-surface p-4 ring-1 ring-line">
          <h2 className="mb-3 font-bold">Users by role</h2>
          <dl className="grid grid-cols-3 gap-3">
            {Object.entries(d.users).map(([k, v]) => (
              <Link key={k} href={`/admin/users?role=${k}`} className="rounded-xl bg-canvas p-3 hover:ring-1 hover:ring-brand">
                <dt className="text-xs font-semibold uppercase tracking-wide text-muted">{k.toLowerCase()}s</dt>
                <dd className="text-xl font-black">{v}</dd>
              </Link>
            ))}
          </dl>
        </section>
      </div>

      <section className="overflow-hidden rounded-2xl bg-surface ring-1 ring-line">
        <h2 className="px-4 pt-4 font-bold">By city</h2>
        <div className="overflow-x-auto">
          <table className="w-full min-w-[480px] text-left text-sm">
            <thead className="text-xs uppercase tracking-wide text-muted">
              <tr>
                <th className="px-4 py-3">City</th>
                <th className="px-4 py-3 text-right">Restaurants</th>
                <th className="px-4 py-3 text-right">Bookings</th>
                <th className="px-4 py-3 text-right">GMV</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-line">
              {d.byCity.map((c) => (
                <tr key={c.cityId}>
                  <td className="px-4 py-3 font-semibold">{c.cityName}</td>
                  <td className="px-4 py-3 text-right">{c.restaurants}</td>
                  <td className="px-4 py-3 text-right">{c.bookings}</td>
                  <td className="px-4 py-3 text-right font-semibold">{money(c.gmv, c.currency)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>

    </div>
  );
}
