"use client";

import Link from "next/link";
import { useState } from "react";
import { PageHeader, Stat } from "@/components/portal-shell";
import { BarList, Meter, Panel, PeriodPicker, pct } from "@/components/insights";
import { ErrorState, Skeleton } from "@/components/ui";
import { useQuery } from "@/lib/hooks";
import { compactMoney, money } from "@/lib/format";

type Days = 7 | 30 | 90;
const PERIODS: { value: Days; label: string }[] = [
  { value: 7, label: "7 days" },
  { value: 30, label: "30 days" },
  { value: 90, label: "90 days" },
];

export default function AdminInsightsPage() {
  const [days, setDays] = useState<Days>(30);
  const q = useQuery(`adminInsights:${days}`, (api) => api.adminInsights(days));
  const d = q.data;

  return (
    <div className="space-y-5">
      <PageHeader
        title="Marketplace insights"
        subtitle={d?.period.from ? `Health of Feast in this country · ${d.period.from} to ${d.period.to} (UTC)` : "Health of Feast in this country"}
        actions={<PeriodPicker value={days} options={PERIODS} onChange={setDays} />}
      />
      {q.error && !d ? (
        <ErrorState error={q.error} onRetry={q.reload} />
      ) : !d ? (
        <Skeleton className="h-96 w-full" />
      ) : (
        <>
          <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
            {d.money.map((m) => (
              <Stat
                key={`gmv-${m.currency}`}
                label={`GMV · ${m.currency}`}
                value={money(m.gmv, m.currency)}
                hint={`${m.billsPaid} bills through Feast${m.averageBill ? ` · avg ${money(m.averageBill, m.currency)}` : ""}${m.offlineSettled ? ` · + ${money(m.offlineSettled, m.currency)} settled offline at counters (${m.offlineBills ?? 0} bills, not through Feast)` : ""}`}
                tone="brand"
              />
            ))}
            {d.money.map((m) => (
              <Stat
                key={`rev-${m.currency}`}
                label={`Feast revenue · ${m.currency}`}
                value={money(m.subscriptionRevenue, m.currency)}
                hint={`subscriptions paid · MRR ${money(m.mrr, m.currency)}`}
              />
            ))}
            <Stat label="Active diners" value={d.activeDiners} hint={`${d.visits} visits · ${d.newDiners} new sign-ups`} />
            <Stat
              label="No-show rate"
              value={pct(d.noShowRate)}
              hint={`${d.bookings.honoured} of ${d.bookings.booked} came · ${d.bookings.cancelled} cancelled`}
              tone={d.noShowRate && d.noShowRate > 0.1 ? "deal" : undefined}
            />
          </div>

          <div className="grid gap-4 lg:grid-cols-3">
            <Panel title="Restaurants">
              <dl className="grid grid-cols-2 gap-3 text-sm">
                <Fig label="Live" value={d.restaurants.live} />
                <Fig label="Waiting for approval" value={d.restaurants.pendingApproval} href={d.restaurants.pendingApproval ? "/admin/approvals" : undefined} />
                <Fig label="Onboarding" value={d.restaurants.onboarding} />
                <Fig label="Paused or suspended" value={d.restaurants.pausedOrSuspended} />
              </dl>
            </Panel>
            <Panel title="Subscriptions">
              <dl className="grid grid-cols-2 gap-3 text-sm">
                <Fig label="Paying" value={d.subscriptions.active} />
                <Fig label="On free trial" value={d.subscriptions.trial} hint={d.subscriptions.trialsEndingSoon ? `${d.subscriptions.trialsEndingSoon} end within 7 days` : undefined} />
                <Fig label="Overdue" value={d.subscriptions.pastDue} href={d.subscriptions.pastDue ? "/admin/subscriptions" : undefined} />
                <Fig label="Expired" value={d.subscriptions.expired} />
              </dl>
            </Panel>
            <Panel title="Deals funded by restaurants">
              <ul className="space-y-3 text-sm">
                {d.money.map((m) => (
                  <li key={m.currency}>
                    <span className="block text-2xl font-black">{money(m.discounts, m.currency)}</span>
                    {m.gmv + m.discounts > 0 ? `${pct(m.discounts / (m.gmv + m.discounts))} of bills before the deal` : "no deals yet"}
                    {m.coverCharges > 0 && ` · ${money(m.coverCharges, m.currency)} cover charges`}
                  </li>
                ))}
              </ul>
            </Panel>
          </div>

          <div className="grid gap-4 lg:grid-cols-2">
            <Panel title="Trust" subtitle="Complaints, reports and reviews in this period">
              <div className="space-y-4">
                <dl className="grid grid-cols-3 gap-3 text-sm">
                  <Fig label="Opened" value={d.disputes.opened} />
                  <Fig label="Per 100 visits" value={d.disputes.per100Visits ?? "—"} />
                  <Fig label="With Feast now" value={d.disputes.withFeastNow} href={d.disputes.withFeastNow ? "/admin/cases" : undefined} />
                </dl>
                {d.disputes.decidedOnTime !== undefined ? (
                  <Meter value={d.disputes.decidedOnTime} label="Decided within 72 hours" hint={`${d.disputes.decidedByFeast} decided by Feast`} />
                ) : (
                  <p className="text-sm text-muted">Feast hasn&apos;t had to decide any case in this period.</p>
                )}
                <p className="text-sm">
                  <b>{d.reviews.posted}</b> reviews{d.reviews.average !== undefined && <> · average {d.reviews.average.toFixed(1)}★</>}
                  {d.reviews.hidden > 0 && (
                    <>
                      {" "}
                      · <Link href="/admin/reviews" className="font-semibold text-brand">{d.reviews.hidden} hidden</Link>
                    </>
                  )}
                </p>
              </div>
            </Panel>
            <Panel title="Cities" subtitle="Feast bookings in this period">
              <BarList
                empty="No cities set up."
                rows={d.cities.map((c) => ({
                  key: c.cityId,
                  label: `${c.cityName} · ${c.liveRestaurants} live`,
                  value: c.bookings,
                  display: `${c.bookings} · ${compactMoney(c.gmv, c.currency)}`,
                }))}
              />
            </Panel>
          </div>

          <Panel title="Top restaurants" subtitle="By Feast bookings, then bills paid on Feast">
            {d.topRestaurants.length ? (
              <div className="overflow-x-auto">
                <table className="w-full min-w-[32rem] text-sm">
                  <thead className="text-left text-xs uppercase tracking-wide text-muted">
                    <tr>
                      <th className="py-2 font-semibold">Restaurant</th>
                      <th className="py-2 font-semibold">City</th>
                      <th className="py-2 text-right font-semibold">Bookings</th>
                      <th className="py-2 text-right font-semibold">GMV</th>
                      <th className="py-2 text-right font-semibold">Rating</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-line">
                    {d.topRestaurants.map((t) => (
                      <tr key={t.restaurant.id}>
                        <td className="py-2 font-semibold">
                          {t.restaurant.name}
                        </td>
                        <td className="py-2 text-muted">{t.cityName}</td>
                        <td className="py-2 text-right tabular-nums">{t.bookings}</td>
                        <td className="py-2 text-right tabular-nums">{money(t.gmv, t.currency)}</td>
                        <td className="py-2 text-right tabular-nums">{t.rating.toFixed(1)}★</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            ) : (
              <p className="text-sm text-muted">No bookings or bills in this period.</p>
            )}
          </Panel>
        </>
      )}
    </div>
  );
}

function Fig({ label, value, hint, href }: { label: string; value: number | string; hint?: string; href?: string }) {
  return (
    <div className="p-1">
      <dt className="text-muted">{label}</dt>
      <dd className="text-2xl font-black">
        {href ? (
          <Link href={href} className="text-brand hover:underline">
            {value}
          </Link>
        ) : (
          value
        )}
      </dd>
      {hint && <dd className="text-xs text-muted">{hint}</dd>}
    </div>
  );
}
