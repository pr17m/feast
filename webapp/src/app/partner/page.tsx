"use client";

import { InstallAppCard } from "@/components/pwa";
import Link from "next/link";
import clsx from "clsx";
import { ArrowRight, Bell, PauseCircle, PlayCircle } from "lucide-react";
import { PageHeader, Stat } from "@/components/portal-shell";
import { useToast } from "@/components/providers";
import { Button, ErrorState, Skeleton } from "@/components/ui";
import { useMutation, useQuery, useTick } from "@/lib/hooks";
import { compactMoney, fromISODate, money } from "@/lib/format";
import { usePartner } from "./partner-context";

export default function PartnerDashboard() {
  const { restaurant, reloadRestaurants } = usePartner();
  const toast = useToast();
  const tick = useTick(8000);
  const dash = useQuery(`dash:${restaurant.id}:${tick}`, (api) => api.partnerDashboard(restaurant.id));
  const setStatus = useMutation((api, s: "LIVE" | "PAUSED") => api.setRestaurantStatus(restaurant.id, s));

  if (dash.error && !dash.data) return <ErrorState error={dash.error} onRetry={dash.reload} />;
  if (!dash.data) return <Skeleton className="h-96 w-full" />;
  const d = dash.data;
  const max = Math.max(1, ...d.last7Days.map((x) => x.revenue));
  const status = d.restaurant.status;

  const toggle = async () => {
    const next = status === "LIVE" ? "PAUSED" : "LIVE";
    const res = await setStatus.run(next);
    if (res) {
      toast(next === "PAUSED" ? "Paused — diners can't book or pay on Feast" : "You're live on Feast again");
      dash.reload();
      reloadRestaurants();
    } else if (setStatus.error) toast(setStatus.error.message, "error");
  };

  return (
    <div className="space-y-6">
      <PageHeader
        title={restaurant.name}
        subtitle={
          <span className="inline-flex items-center gap-2">
            <span className={clsx("h-2 w-2 rounded-full", status === "LIVE" ? "bg-veg" : status === "PAUSED" ? "bg-amber-500" : "bg-danger")} />
            {status === "LIVE" ? "Live on Feast" : status.replace("_", " ").toLowerCase()} · {restaurant.area}
          </span>
        }
        actions={
          (status === "LIVE" || status === "PAUSED") && (
            <Button variant={status === "LIVE" ? "secondary" : "primary"} onClick={toggle} loading={setStatus.pending}>
              {status === "LIVE" ? <PauseCircle className="h-4 w-4" aria-hidden /> : <PlayCircle className="h-4 w-4" aria-hidden />}
              {status === "LIVE" ? "Pause on Feast" : "Go live"}
            </Button>
          )
        }
      />

      {status === "PENDING_APPROVAL" && (
        <p className="rounded-xl bg-amber-50 p-3 text-sm text-amber-900">Your documents are with Feast for review. Set up your menu and offers meanwhile.</p>
      )}
      {status === "SUSPENDED" && <p className="rounded-xl bg-red-50 p-3 text-sm text-danger">Your listing is suspended. Contact your Feast account manager.</p>}

      <InstallAppCard app="partner" />

      <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        <Stat label="Bookings today" value={d.bookingsToday} hint={`${d.coversToday} covers`} />
        <Stat label="Arrived today" value={d.arrivedToday} hint={`${d.seatedNow} seated now`} />
        <Stat label="Paid via Feast today" value={money(d.revenueToday)} hint={`${money(d.savingsGivenToday)} in diner savings`} tone="brand" />
        <Stat
          label="Counter payments to confirm"
          value={d.pendingCounterPayments}
          hint="Diners paying cash or card at your counter"
          tone={d.pendingCounterPayments ? "deal" : undefined}
        />
      </div>

      {d.pendingCounterPayments > 0 && (
        <Link href="/partner/payments" className="flex items-center gap-3 rounded-2xl bg-deal-soft p-4 ring-1 ring-deal/30 hover:shadow-[var(--shadow-card)]">
          <Bell className="h-6 w-6 text-deal" aria-hidden />
          <span className="flex-1 font-semibold">
            {d.pendingCounterPayments} counter {d.pendingCounterPayments === 1 ? "payment needs" : "payments need"} your confirmation
          </span>
          <ArrowRight className="h-5 w-5 text-deal" aria-hidden />
        </Link>
      )}

      <section className="rounded-2xl bg-surface p-4 ring-1 ring-line md:p-6">
        <h2 className="mb-4 font-bold">Last 7 days</h2>
        <div className="flex h-48 items-end gap-2 md:gap-4" role="img" aria-label="Revenue paid via Feast over the last 7 days">
          {d.last7Days.map((x) => (
            <div key={x.date} className="flex flex-1 flex-col items-center gap-1">
              <span className="text-[11px] font-semibold text-muted">{x.revenue ? compactMoney(x.revenue) : ""}</span>
              <div className="w-full max-w-12 rounded-t-lg bg-brand/85" style={{ height: `${Math.max(2, (x.revenue / max) * 140)}px` }} title={`${money(x.revenue)} · ${x.bookings} bookings`} />
              <span className="text-xs text-muted">{fromISODate(x.date).toLocaleDateString(undefined, { weekday: "short" })}</span>
              <span className="text-[11px] text-faint">{x.bookings} bk</span>
            </div>
          ))}
        </div>
      </section>

      <div className="grid gap-3 md:grid-cols-3">
        {[
          { href: "/partner/bookings", title: "Today's bookings", body: "Seat guests and mark no-shows" },
          { href: "/partner/menu", title: "Menu availability", body: "86 an item in one tap" },
          { href: "/partner/offers", title: "Offers & deals", body: "Tune pre-book and bill discounts" },
        ].map((c) => (
          <Link key={c.href} href={c.href} className="rounded-2xl bg-surface p-4 ring-1 ring-line hover:shadow-[var(--shadow-card)]">
            <p className="font-bold">{c.title}</p>
            <p className="text-sm text-muted">{c.body}</p>
          </Link>
        ))}
      </div>
    </div>
  );
}
