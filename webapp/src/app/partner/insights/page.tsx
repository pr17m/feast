"use client";

import Link from "next/link";
import { useState } from "react";
import { ArrowRight, TrendingUp } from "lucide-react";
import { PageHeader, Stat } from "@/components/portal-shell";
import { Columns, Meter, Panel, PeriodPicker, hourLabel, monthLabel, pct } from "@/components/insights";
import { Empty, ErrorState, Skeleton } from "@/components/ui";
import { useQuery } from "@/lib/hooks";
import { compactMoney, fromISODate, money } from "@/lib/format";
import { usePartner } from "../partner-context";

type Days = 7 | 30 | 90;
const PERIODS: { value: Days; label: string }[] = [
  { value: 7, label: "7 days" },
  { value: 30, label: "30 days" },
  { value: 90, label: "90 days" },
];

const fmtDate = (d: string) => fromISODate(d).toLocaleDateString(undefined, { day: "numeric", month: "short" });

export default function PartnerInsights() {
  const { restaurant } = usePartner();
  const [days, setDays] = useState<Days>(30);
  const q = useQuery(`pInsights:${restaurant.id}:${days}`, (api) => api.partnerInsights(restaurant.id, days));
  const d = q.data;

  return (
    <div className="max-w-5xl space-y-5">
      <PageHeader
        title="Insights"
        subtitle={d?.period.from ? `Did Feast pay for itself? · ${fmtDate(d.period.from)} – ${fmtDate(d.period.to)}` : "Did Feast pay for itself?"}
        actions={<PeriodPicker value={days} options={PERIODS} onChange={setDays} />}
      />
      {q.error && !d ? (
        <ErrorState error={q.error} onRetry={q.reload} />
      ) : !d ? (
        <Skeleton className="h-96 w-full" />
      ) : (
        <>
          <Scorecard revenue={d.revenue} offline={d.offlineRevenue ?? 0} cost={d.feastCost} ratio={d.returnOnCost} currency={d.currency} />

          <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
            <Stat label="Feast diners" value={d.feastDiners} hint={`${d.newDiners} new · ${d.repeatDiners} returning`} tone="brand" />
            <Stat label="Covers from bookings" value={d.covers} hint={`${d.visits} visits`} />
            <Stat label="Bills paid on Feast" value={d.billsPaid} hint={d.averageBill ? `avg ${money(d.averageBill, d.currency)}` : "no bills yet"} />
            <Stat
              label="No-show rate"
              value={pct(d.noShowRate)}
              hint={`${d.bookings.honoured} of ${d.bookings.booked} came · ${d.bookings.cancelled} cancelled`}
              tone={d.noShowRate && d.noShowRate > 0.1 ? "deal" : undefined}
            />
          </div>

          <div className="grid gap-4 lg:grid-cols-2">
            <Panel title="Deals and your budget" subtitle="Feast deals only apply to bills paid on Feast.">
              <div className="space-y-4">
                <div className="grid grid-cols-2 gap-3 text-sm">
                  <p>
                    <span className="block text-2xl font-black">{d.deals.dealsUsed}</span>
                    discounted bills
                  </p>
                  <p>
                    <span className="block text-2xl font-black">{money(d.deals.discountsGiven, d.currency)}</span>
                    given to Feast diners
                  </p>
                </div>
                {d.deals.dealsAvailable !== undefined && d.deals.budgetUsed !== undefined ? (
                  <Meter
                    value={d.deals.budgetUsed}
                    label="Deal budget used"
                    hint={`${d.deals.dealsUsed} of ${d.deals.dealsAvailable} deals (${d.deals.dailyDealLimit} a day)`}
                  />
                ) : (
                  <p className="text-sm text-muted">
                    No daily deal limit set.{" "}
                    <Link href={`/partner/offers?r=${restaurant.id}`} className="font-semibold text-brand">
                      Set one in Offers
                    </Link>{" "}
                    to keep discount costs predictable.
                  </p>
                )}
                {d.deals.maxDiscountPerBill ? <p className="text-sm text-muted">Capped at {money(d.deals.maxDiscountPerBill, d.currency)} off any one bill.</p> : null}
                {d.revenue > 0 && (
                  <p className="text-sm text-muted">
                    Discounts were {pct(d.deals.discountsGiven / (d.revenue + d.deals.discountsGiven))} of the bills before the deal.
                  </p>
                )}
              </div>
            </Panel>

            <Panel title="Reputation" subtitle={`${d.rating.toFixed(1)}★ from ${d.ratingCount.toLocaleString()} ratings`}>
              {d.ratingTrend.some((m) => m.reviews > 0) ? (
                <Columns
                  label="Average Feast review rating by month"
                  data={d.ratingTrend.map((m) => ({ label: monthLabel(m.month), value: m.average }))}
                  format={(v) => v.toFixed(1)}
                  highlight="none"
                />
              ) : (
                <p className="text-sm text-muted">No Feast reviews in the last 6 months. Diners can review after a kept booking or a bill paid on Feast.</p>
              )}
              <Link href={`/partner/cases?r=${restaurant.id}`} className="mt-4 flex items-center gap-2 rounded-xl bg-canvas p-3 text-sm hover:ring-1 hover:ring-line">
                <span className="flex-1">
                  <b>{d.complaintsOpened}</b> {d.complaintsOpened === 1 ? "complaint" : "complaints"} this period · <b>{d.openCases}</b> open now
                </span>
                <ArrowRight className="h-4 w-4 text-muted" aria-hidden />
              </Link>
            </Panel>
          </div>

          <div className="grid gap-4 lg:grid-cols-2">
            <Panel title="Busiest days" subtitle="Feast bookings by day of the week">
              {d.byWeekday.some((w) => w.bookings > 0) ? (
                <Columns label="Feast bookings by weekday" data={d.byWeekday.map((w) => ({ label: w.day, value: w.bookings }))} format={(v) => String(v)} />
              ) : (
                <p className="text-sm text-muted">No bookings in this period.</p>
              )}
            </Panel>
            <Panel title="Busiest times" subtitle="Covers by booked slot">
              {d.byHour.length ? (
                <Columns label="Covers by booked hour" data={d.byHour.map((h) => ({ label: hourLabel(h.hour), value: h.covers }))} format={(v) => String(v)} />
              ) : (
                <p className="text-sm text-muted">No bookings in this period.</p>
              )}
            </Panel>
          </div>

          <Panel title="Revenue by weekday" subtitle="Bills paid on Feast">
            {d.revenue > 0 ? (
              <Columns label="Revenue paid on Feast by weekday" data={d.byWeekday.map((w) => ({ label: w.day, value: w.revenue }))} format={(v) => compactMoney(v, d.currency)} />
            ) : (
              <p className="text-sm text-muted">No bills paid on Feast in this period.</p>
            )}
          </Panel>

          {d.visits === 0 && d.bookings.booked === 0 && (
            <Empty title="Nothing to show yet" body="Insights fill in as Feast diners book and pay their bills." />
          )}
        </>
      )}
    </div>
  );
}

function Scorecard({ revenue, offline, cost, ratio, currency }: { revenue: number; offline: number; cost: number; ratio?: number; currency: string }) {
  return (
    <section className="flex flex-col gap-4 rounded-2xl bg-brand-soft p-5 ring-1 ring-brand/20 md:flex-row md:items-center">
      <TrendingUp className="h-10 w-10 shrink-0 text-brand" aria-hidden />
      <div className="flex-1">
        <p className="text-sm font-semibold uppercase tracking-wide text-brand-strong">Feast scorecard</p>
        <p className="mt-1 text-lg">
          Diners paid <b>{money(revenue, currency)}</b> through Feast
          {offline > 0 && (
            <>
              {" "}
              (plus <b>{money(offline, currency)}</b> Feast diners paid at your counter)
            </>
          )}
          {cost > 0 ? (
            <>
              {" "}
              for <b>{money(cost, currency)}</b> of subscription.
            </>
          ) : (
            <>. Your subscription cost nothing for this period (free trial or not billed yet).</>
          )}
        </p>
      </div>
      {ratio !== undefined && (
        <p className="text-right">
          <span className="block text-4xl font-black text-brand">{ratio >= 10 ? Math.round(ratio) : ratio.toFixed(1)}×</span>
          <span className="text-sm text-muted">what Feast cost</span>
        </p>
      )}
    </section>
  );
}
