"use client";

import Link from "next/link";
import { Suspense } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import clsx from "clsx";
import { ChevronRight } from "lucide-react";
import { Art } from "@/components/art";
import { Empty, ErrorState, LinkButton, Skeleton } from "@/components/ui";
import { useQuery } from "@/lib/hooks";
import { longDate, money, time12 } from "@/lib/format";
import { STATUS_STYLE } from "@/lib/status";

const TABS = [
  { id: "upcoming", label: "Upcoming" },
  { id: "past", label: "Past" },
  { id: "payments", label: "Bill payments" },
] as const;

type TabId = (typeof TABS)[number]["id"];

function BookingsInner() {
  // Tab lives in the URL so links like /bookings?tab=payments open the right view.
  const sp = useSearchParams();
  const router = useRouter();
  const tab: TabId = TABS.some((t) => t.id === sp.get("tab")) ? (sp.get("tab") as TabId) : "upcoming";
  const setTab = (t: TabId) => router.replace(t === "upcoming" ? "/bookings" : `/bookings?tab=${t}`, { scroll: false });
  const bookings = useQuery("bookings", (api) => api.listBookings());
  const payments = useQuery(tab === "payments" ? "billPayments" : null, (api) => api.listBillPayments());

  const upcoming = bookings.data?.filter((b) => b.status === "CONFIRMED") ?? [];
  const past = (bookings.data?.filter((b) => b.status !== "CONFIRMED") ?? []).reverse();
  const list = tab === "upcoming" ? upcoming : past;

  return (
    <div className="mx-auto max-w-2xl space-y-4">
      <h1 className="text-2xl font-black tracking-tight">Your dining</h1>
      <div className="flex gap-2" role="tablist">
        {TABS.map((t) => (
          <button
            key={t.id}
            role="tab"
            aria-selected={tab === t.id}
            onClick={() => setTab(t.id)}
            className={clsx("rounded-full px-4 py-1.5 text-sm font-semibold ring-1", tab === t.id ? "bg-ink text-white ring-ink" : "bg-surface ring-line")}
          >
            {t.label}
            {t.id === "upcoming" && upcoming.length > 0 && ` (${upcoming.length})`}
          </button>
        ))}
      </div>

      {tab !== "payments" && (
        <>
          {bookings.error && <ErrorState error={bookings.error} onRetry={bookings.reload} />}
          {!bookings.data && !bookings.error && <Skeleton className="h-28 w-full" />}
          {bookings.data && list.length === 0 && (
            <Empty
              title={tab === "upcoming" ? "No upcoming bookings" : "No past bookings yet"}
              body="Pre-book a table to lock in deals of up to 40% off your bill."
              action={<LinkButton href="/">Find a restaurant</LinkButton>}
            />
          )}
          <ul className="space-y-3">
            {list.map((b) => (
              <li key={b.id}>
                <Link href={`/bookings/${b.id}`} className="flex items-center gap-3 rounded-2xl bg-surface p-3 ring-1 ring-line hover:shadow-[var(--shadow-card)]">
                  <Art hue={b.restaurant.art.hue} motif={b.restaurant.art.motif} image={b.restaurant.art.image} credit={b.restaurant.art.imageCredit} className="h-16 w-16 shrink-0 rounded-xl" iconClassName="h-10 w-10" />
                  <div className="min-w-0 flex-1">
                    <p className="truncate font-bold">{b.restaurant.name}</p>
                    <p className="text-sm text-muted">
                      {longDate(b.date)} · {time12(b.time)} · {b.guests} {b.guests === 1 ? "guest" : "guests"}
                    </p>
                    <div className="mt-1 flex items-center gap-2">
                      <span className={clsx("rounded-md px-1.5 py-0.5 text-xs font-bold", STATUS_STYLE[b.status])}>{b.status.replace("_", " ")}</span>
                      {b.deal && b.status === "CONFIRMED" && <span className="text-xs font-semibold text-deal">{b.deal.title}</span>}
                    </div>
                  </div>
                  <ChevronRight className="h-5 w-5 text-faint" aria-hidden />
                </Link>
              </li>
            ))}
          </ul>
        </>
      )}

      {tab === "payments" && (
        <>
          {payments.error && <ErrorState error={payments.error} onRetry={payments.reload} />}
          {!payments.data && !payments.error && <Skeleton className="h-28 w-full" />}
          {payments.data && payments.data.length === 0 && (
            <Empty title="No bill payments yet" body="Book a table on Feast, then pay your bill in the app on the day to get your deal." action={<LinkButton href="/">Find a table</LinkButton>} />
          )}
          <ul className="space-y-3">
            {payments.data?.map((p) => (
              <li key={p.id}>
                <Link href={`/payments/${p.id}`} className="flex items-center gap-3 rounded-2xl bg-surface p-3 ring-1 ring-line hover:shadow-[var(--shadow-card)]">
                  <Art hue={p.restaurant.art.hue} motif={p.restaurant.art.motif} image={p.restaurant.art.image} credit={p.restaurant.art.imageCredit} className="h-16 w-16 shrink-0 rounded-xl" iconClassName="h-10 w-10" />
                  <div className="min-w-0 flex-1">
                    <p className="truncate font-bold">{p.restaurant.name}</p>
                    <p className="text-sm text-muted">{new Date(p.payment.paidAt).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" })}</p>
                    {p.bill.savings > 0 && <p className="text-xs font-semibold text-veg">Saved {money(p.bill.savings, p.bill.currency)}</p>}
                  </div>
                  <span className="font-bold">{money(p.payment.amount, p.payment.currency)}</span>
                </Link>
              </li>
            ))}
          </ul>
        </>
      )}
    </div>
  );
}

export default function BookingsPage() {
  return (
    <Suspense fallback={<Skeleton className="mx-auto h-64 max-w-2xl" />}>
      <BookingsInner />
    </Suspense>
  );
}
