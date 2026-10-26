"use client";

import { InstallAppCard } from "@/components/pwa";
import Link from "next/link";
import { Suspense, useState } from "react";
import { useParams, useSearchParams } from "next/navigation";
import clsx from "clsx";
import { CalendarPlus, CheckCircle2, MapPin, Navigation, QrCode, Users, Clock, CalendarDays } from "lucide-react";
import { Art } from "@/components/art";
import { useToast } from "@/components/providers";
import { Button, Card, ErrorState, LinkButton, Sheet, Skeleton } from "@/components/ui";
import { useMutation, useQuery } from "@/lib/hooks";
import { longDate, money, time12 } from "@/lib/format";
import { STATUS_STYLE } from "@/lib/status";
import { VisitActions } from "@/components/visit-actions";
import { HistoryCard } from "@/components/audit";
import { RefundsCard } from "@/components/refunds";

function icsFor(b: { restaurant: { name: string; area: string }; date: string; time: string; guests: number; code: string }) {
  const start = `${b.date.replace(/-/g, "")}T${b.time.replace(":", "")}00`;
  const [h, m] = b.time.split(":").map(Number);
  const endH = String((h + 2) % 24).padStart(2, "0");
  const end = `${b.date.replace(/-/g, "")}T${endH}${String(m).padStart(2, "0")}00`;
  return [
    "BEGIN:VCALENDAR",
    "VERSION:2.0",
    "PRODID:-//Feast//Booking//EN",
    "BEGIN:VEVENT",
    `UID:${b.code}@feast`,
    `DTSTART:${start}`,
    `DTEND:${end}`,
    `SUMMARY:Table at ${b.restaurant.name}`,
    `LOCATION:${b.restaurant.area}`,
    `DESCRIPTION:Booking ${b.code} for ${b.guests}`,
    "END:VEVENT",
    "END:VCALENDAR",
  ].join("\r\n");
}

function BookingInner() {
  const { id } = useParams<{ id: string }>();
  const isNew = useSearchParams().get("new") === "1";
  const toast = useToast();
  const [confirmCancel, setConfirmCancel] = useState(false);
  const b = useQuery(`booking:${id}`, (api) => api.getBooking(id));
  const cancel = useMutation((api) => api.cancelBooking(id));
  const checkIn = useMutation((api) => api.checkInBooking(id));

  if (b.error) return <ErrorState error={b.error} onRetry={b.reload} />;
  if (!b.data) return <Skeleton className="mx-auto h-96 max-w-xl" />;
  const bk = b.data;
  const confirmed = bk.status === "CONFIRMED";

  return (
    <div className="mx-auto max-w-xl space-y-4">
      {isNew && confirmed && (
        <div className="flex flex-col items-center gap-2 rounded-3xl bg-brand px-6 py-8 text-center text-white">
          <CheckCircle2 className="h-12 w-12" aria-hidden />
          <h1 className="text-2xl font-black">Table booked!</h1>
          <p className="text-white/85">
            We&apos;ve sent the details to {bk.contact.phone.startsWith("+") ? bk.contact.phone : `your phone (${bk.contact.phone})`}.
          </p>
        </div>
      )}

      <Card className="space-y-4">
        <div className="flex items-center gap-3">
          <Art hue={bk.restaurant.art.hue} motif={bk.restaurant.art.motif} image={bk.restaurant.art.image} credit={bk.restaurant.art.imageCredit} className="h-14 w-14 shrink-0 rounded-xl" iconClassName="h-10 w-10" />
          <div className="min-w-0 flex-1">
            <Link href={`/restaurants/${bk.restaurant.slug}`} className="block truncate text-lg font-bold hover:text-brand">
              {bk.restaurant.name}
            </Link>
            <p className="text-sm text-muted">{bk.restaurant.area}</p>
          </div>
          <span className={clsx("rounded-md px-2 py-1 text-xs font-bold", STATUS_STYLE[bk.status])}>{bk.status.replace("_", " ")}</span>
        </div>

        <dl className="grid grid-cols-3 gap-2 rounded-2xl bg-canvas p-3 text-center">
          <div>
            <dt className="flex items-center justify-center gap-1 text-xs text-muted">
              <CalendarDays className="h-3.5 w-3.5" aria-hidden />
              Date
            </dt>
            <dd className="font-bold">{longDate(bk.date)}</dd>
          </div>
          <div>
            <dt className="flex items-center justify-center gap-1 text-xs text-muted">
              <Clock className="h-3.5 w-3.5" aria-hidden />
              Time
            </dt>
            <dd className="font-bold">{time12(bk.time)}</dd>
          </div>
          <div>
            <dt className="flex items-center justify-center gap-1 text-xs text-muted">
              <Users className="h-3.5 w-3.5" aria-hidden />
              Guests
            </dt>
            <dd className="font-bold">{bk.guests}</dd>
          </div>
        </dl>

        <div className="flex items-center justify-between rounded-2xl border border-dashed border-line p-3">
          <div>
            <p className="text-xs text-muted">Show this code at the restaurant</p>
            <p className="font-mono text-2xl font-black tracking-widest">{bk.code}</p>
          </div>
          <QrCode className="h-10 w-10 text-ink" aria-hidden />
        </div>

        {bk.deal && (
          <div className="rounded-xl bg-deal-soft p-3">
            <p className="font-bold text-deal">{bk.deal.title} on your total bill</p>
            <p className="text-sm text-muted">Pay your bill on Feast to get your deal; the discount isn&apos;t available at the till.</p>
          </div>
        )}
        {bk.specialRequest && (
          <p className="text-sm">
            <span className="font-semibold">Special request:</span> <span className="text-muted">{bk.specialRequest}</span>
          </p>
        )}
        {bk.payment && (
          <p className="text-sm text-muted">
            Cover charge of {money(bk.payment.amount, bk.payment.currency)} paid via {bk.payment.method}. It will be adjusted in your bill.
          </p>
        )}
      </Card>

      <RefundsCard payment={bk.payment} version={bk.status} />

      {confirmed && (
        <div className="grid grid-cols-2 gap-3">
          <a
            href={`data:text/calendar;charset=utf-8,${encodeURIComponent(icsFor(bk))}`}
            download={`feast-${bk.code}.ics`}
            className="inline-flex h-11 items-center justify-center gap-2 rounded-xl bg-surface text-sm font-semibold ring-1 ring-line hover:bg-canvas"
          >
            <CalendarPlus className="h-4 w-4" aria-hidden /> Add to calendar
          </a>
          <a
            href={`https://www.google.com/maps/search/?api=1&query=${encodeURIComponent(`${bk.restaurant.name} ${bk.restaurant.area}`)}`}
            target="_blank"
            rel="noreferrer"
            className="inline-flex h-11 items-center justify-center gap-2 rounded-xl bg-surface text-sm font-semibold ring-1 ring-line hover:bg-canvas"
          >
            <Navigation className="h-4 w-4" aria-hidden /> Directions
          </a>
          {bk.date === new Date().toLocaleDateString("en-CA") &&
            (bk.checkedInAt ? (
              <p className="col-span-2 flex items-center gap-2 rounded-xl bg-green-50 p-3 text-sm text-veg">
                <CheckCircle2 className="h-5 w-5 shrink-0" aria-hidden /> You&apos;re checked in. When you&apos;re done, pay the bill here to get your deal.
              </p>
            ) : (
              <div className="col-span-2 space-y-1">
                <Button
                  className="w-full"
                  loading={checkIn.pending}
                  onClick={async () => {
                    const res = await checkIn.run();
                    if (res) {
                      toast(`${bk.restaurant.name} knows you're here`);
                      b.reload();
                    }
                  }}
                >
                  <MapPin className="h-4 w-4" aria-hidden /> I&apos;m here
                </Button>
                {checkIn.error && <p role="alert" className="text-sm text-danger">{checkIn.error.message}</p>}
                <p className="text-xs text-muted">Lets the restaurant know you&apos;ve arrived. Opens 30 minutes before your booking.</p>
              </div>
            ))}
          <LinkButton href={`/pay/${bk.restaurant.slug}?booking=${encodeURIComponent(bk.id)}`} className="col-span-2">
            At the restaurant? Pay bill &amp; get your deal
          </LinkButton>
          <Button variant="ghost" className="col-span-2 text-danger hover:bg-red-50" onClick={() => setConfirmCancel(true)}>
            Cancel booking
          </Button>
        </div>
      )}

      {bk.status !== "PENDING_PAYMENT" && (bk.status !== "CONFIRMED" || bk.date <= new Date().toISOString().slice(0, 10)) && (
        <VisitActions
          bookingId={bk.id}
          restaurantName={bk.restaurant.name}
          visitDate={bk.date}
          canReview={bk.status === "COMPLETED" || (bk.status === "SEATED" && bk.date <= new Date().toISOString().slice(0, 10))}
        />
      )}

      <HistoryCard title="Booking history" scopeKey={`booking:${bk.id}`} load={(api) => api.bookingHistory(bk.id)} />

      <Link href="/bookings" className="block text-center text-sm font-semibold text-brand">
        View all bookings
      </Link>

      <Sheet
        open={confirmCancel}
        onClose={() => setConfirmCancel(false)}
        title="Cancel this booking?"
        footer={
          <div className="flex gap-3">
            <Button variant="secondary" className="flex-1" onClick={() => setConfirmCancel(false)}>
              Keep it
            </Button>
            <Button
              className="flex-1 bg-danger hover:bg-red-800"
              loading={cancel.pending}
              onClick={async () => {
                const res = await cancel.run();
                if (res) {
                  setConfirmCancel(false);
                  toast("Booking cancelled");
                  b.reload();
                }
              }}
            >
              Cancel booking
            </Button>
          </div>
        }
      >
        <p className="text-muted">
          Your table at {bk.restaurant.name} on {longDate(bk.date)}, {time12(bk.time)} will be released
          {bk.deal ? ` and you'll lose the ${bk.deal.title.toLowerCase()} deal` : ""}.
        </p>
        {bk.payment && bk.payment.settlement !== "OFFLINE" && (
          <p className="mt-2 text-sm text-muted">
            The upfront charge of {money(bk.payment.amount, bk.payment.currency)} is refunded in full if you cancel at least 2 hours before your booking.
            Later than that, the restaurant keeps it.{" "}
            <Link href="/legal/refunds" className="font-semibold text-brand hover:underline">
              Refunds policy
            </Link>
          </p>
        )}
        {cancel.error && <p className="mt-2 text-sm text-danger">{cancel.error.message}</p>}
      </Sheet>

      {/* After a booking: offer the home-screen app (web only; hidden once installed). */}
      {confirmed && <InstallAppCard app="diner" />}
    </div>
  );
}

export default function BookingPage() {
  return (
    <Suspense fallback={<Skeleton className="mx-auto h-96 max-w-xl" />}>
      <BookingInner />
    </Suspense>
  );
}
