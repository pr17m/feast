"use client";

import { InstallAppCard } from "@/components/pwa";
import Link from "next/link";
import clsx from "clsx";
import { useMemo } from "react";
import { CalendarCheck, CheckCircle2, ChevronRight, MapPin, QrCode, Receipt } from "lucide-react";
import { Art } from "@/components/art";
import { ScanQrButton } from "@/components/native-app";
import { useAuth, useToast } from "@/components/providers";
import { Button, Card, Skeleton } from "@/components/ui";
import { useMutation, useQuery } from "@/lib/hooks";
import { toISODate } from "@/lib/format";
import type { Booking } from "@/lib/api/types";

/**
 * At the restaurant: today's Feast bookings, with "I'm here" (tells the
 * restaurant you've arrived) and "Pay bill & get your deal". Feast deals are
 * for diners who booked on Feast and pay through Feast; walk-ins pay the
 * restaurant as usual. The restaurant's QR code opens its page on Feast.
 */
export default function AtTheRestaurantPage() {
  const { user, ready } = useAuth();
  const bookings = useQuery(ready && user ? "bookings" : null, (api) => api.listBookings());
  const today = useMemo(() => {
    const date = toISODate(new Date());
    return (bookings.data ?? [])
      .filter((b) => b.date === date && (b.status === "CONFIRMED" || b.status === "SEATED" || b.status === "COMPLETED"))
      .sort((a, b) => a.time.localeCompare(b.time));
  }, [bookings.data]);

  return (
    <div className="mx-auto max-w-2xl space-y-5">
      <div>
        <h1 className="text-2xl font-black tracking-tight">At the restaurant</h1>
        <p className="text-muted">Tell the restaurant you&apos;ve arrived, then pay the bill for your Feast booking to get your deal.</p>
      </div>

      <section aria-labelledby="today-heading" className="space-y-2">
        <h2 id="today-heading" className="flex items-center gap-2 font-bold">
          <CalendarCheck className="h-5 w-5 text-brand" aria-hidden /> Your Feast booking today
        </h2>
        {ready && user && !bookings.data && !bookings.error && <Skeleton className="h-28 w-full" />}
        {bookings.error && <p className="text-sm text-danger">{bookings.error.message}</p>}
        {today.map((b) => (
          <TodayBooking key={b.id} booking={b} onChanged={bookings.reload} />
        ))}
        {(ready && !user) || (bookings.data && today.length === 0) ? (
          <Card className="space-y-2">
            <p className="text-sm text-muted">
              {user ? "No Feast booking for today." : "Sign in to see today's booking."} Feast deals are for diners who book a table on Feast and pay their bill
              through Feast — the discount isn&apos;t available at the till.
            </p>
            <Link href={user ? "/" : "/login?next=/dine-in"} className="inline-flex items-center gap-1 text-sm font-semibold text-brand">
              {user ? "Find a restaurant and book" : "Sign in"} <ChevronRight className="h-4 w-4" aria-hidden />
            </Link>
          </Card>
        ) : null}
      </section>

      <section aria-labelledby="qr-heading" className="space-y-2">
        <h2 id="qr-heading" className="flex items-center gap-2 font-bold">
          <QrCode className="h-5 w-5 text-brand" aria-hidden /> Scan the restaurant&apos;s QR code
        </h2>
        <Card>
          <p className="text-sm text-muted">
            Restaurants on Feast display a QR code on the tables or at the counter. Point your phone camera at it to open the restaurant on Feast: pay the bill
            for today&apos;s booking, or book your next visit.
          </p>
          <ScanQrButton className="mt-3 w-full" />
        </Card>
      </section>

      <InstallAppCard app="diner" />
    </div>
  );
}

function TodayBooking({ booking: b, onChanged }: { booking: Booking; onChanged: () => void }) {
  const toast = useToast();
  const checkIn = useMutation((api) => api.checkInBooking(b.id));
  return (
    <Card className="space-y-3">
      <div className="flex items-center gap-3">
        <Art
          hue={b.restaurant.art.hue}
          motif={b.restaurant.art.motif}
          image={b.restaurant.art.image}
          credit={b.restaurant.art.imageCredit}
          className="h-12 w-12 shrink-0 rounded-xl"
          iconClassName="h-8 w-8"
        />
        <div className="min-w-0 flex-1">
          <p className="truncate font-semibold">{b.restaurant.name}</p>
          <p className="text-sm text-muted">
            {b.time} · {b.guests} guests · {b.code}
          </p>
          {b.deal && <p className="text-sm font-semibold text-deal">{b.deal.title}</p>}
        </div>
      </div>
      {b.checkedInAt && (
        <p className="flex items-center gap-1.5 text-sm font-semibold text-veg">
          <CheckCircle2 className="h-4 w-4 shrink-0" aria-hidden /> You&apos;re checked in
        </p>
      )}
      <div className="grid gap-2 sm:grid-cols-2">
        {!b.checkedInAt && b.status !== "COMPLETED" && (
          <Button
            variant="secondary"
            loading={checkIn.pending}
            onClick={async () => {
              const res = await checkIn.run();
              if (res) {
                toast(`${b.restaurant.name} knows you're here`);
                onChanged();
              }
            }}
          >
            <MapPin className="h-4 w-4" aria-hidden /> I&apos;m here
          </Button>
        )}
        <Link
          href={`/pay/${b.restaurant.slug}?booking=${encodeURIComponent(b.id)}`}
          className={clsx(
            "inline-flex h-11 items-center justify-center gap-1.5 rounded-xl bg-brand px-4 text-sm font-semibold text-white hover:opacity-90",
            (b.checkedInAt || b.status === "COMPLETED") && "sm:col-span-2",
          )}
        >
          <Receipt className="h-4 w-4" aria-hidden /> Pay bill &amp; get your deal
        </Link>
      </div>
      {checkIn.error && (
        <p role="alert" className="text-sm text-danger">
          {checkIn.error.message}
        </p>
      )}
    </Card>
  );
}
