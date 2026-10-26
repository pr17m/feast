"use client";

import { useMemo, useRef, useState } from "react";
import { useParams, useRouter } from "next/navigation";
import clsx from "clsx";
import { ArrowLeft, Info, Users, CalendarDays, Clock } from "lucide-react";
import { Art } from "@/components/art";
import { Checkout } from "@/components/checkout";
import { Button, Card, ErrorState, Skeleton, Stepper } from "@/components/ui";
import { useLocalState, useMutation, useQuery } from "@/lib/hooks";
import { addDays, dayLabel, fromISODate, longDate, money, setActiveCurrency, time12, toISODate } from "@/lib/format";
import { PhoneField, phoneE164, phoneFromE164, type PhoneValue } from "@/components/phone-field";
import { useAuth, useMarkets } from "@/components/providers";
import type { MealPeriod, PaymentIntent, Slot } from "@/lib/api/types";
import { coverChargeFor } from "@/lib/pricing";

const PERIOD_LABEL: Record<MealPeriod, string> = { BREAKFAST: "Breakfast", LUNCH: "Lunch", DINNER: "Dinner" };
const DAYS = 14;

export default function BookPage() {
  const { slug } = useParams<{ slug: string }>();
  const router = useRouter();
  const r = useQuery(`restaurant:${slug}`, (api) => api.getRestaurant(slug));

  const today = useMemo(() => new Date(), []);
  const dates = useMemo(() => Array.from({ length: DAYS }, (_, i) => toISODate(addDays(today, i))), [today]);
  const [date, setDate] = useState(dates[0]);
  const [guests, setGuests] = useState(2);
  const [period, setPeriod] = useState<MealPeriod | null>(null);
  const [slot, setSlot] = useState<Slot | null>(null);
  const [step, setStep] = useState<"pick" | "details">("pick");
  const [profile, setProfile] = useLocalState("feast.profile", { name: "", phone: "" });
  const [name, setName] = useState<string | null>(null);
  const [phone, setPhone] = useState<PhoneValue | null>(null);
  const { markets, marketOf } = useMarkets();
  const { user } = useAuth();
  const [note, setNote] = useState("");
  const [intent, setIntent] = useState<PaymentIntent | null>(null);
  const idemKey = useRef<string | null>(null);

  const slots = useQuery(r.data ? `slots:${r.data.id}:${date}:${guests}` : null, (api) => api.getSlots(r.data!.id, date, guests));
  const book = useMutation((api, req: Parameters<typeof api.createBooking>[0]) => api.createBooking(req));

  // Never show slots fetched for a different date/party size (avoids booking a stale slot while the new ones load).
  const avail = slots.data && slots.data.date === date && slots.data.guests === guests ? slots.data : undefined;

  const periods = useMemo(() => {
    const ps = new Set(avail?.slots.map((s) => s.period));
    return (["BREAKFAST", "LUNCH", "DINNER"] as MealPeriod[]).filter((p) => ps.has(p));
  }, [avail]);
  const activePeriod = period && periods.includes(period) ? period : (periods.find((p) => avail?.slots.some((s) => s.period === p && s.available)) ?? periods[0]);
  const shown = avail?.slots.filter((s) => s.period === activePeriod) ?? [];

  if (r.error) return <ErrorState error={r.error} onRetry={r.reload} />;
  if (!r.data) return <Skeleton className="h-96 w-full" />;
  const d = r.data;
  setActiveCurrency(d.currency);
  const market = marketOf(d.countryCode);

  const contactName = name ?? (profile.name || user?.name || "");
  // Prefill from the saved profile or the account's phone; default to the restaurant's country.
  const contactPhoneValue = phone ?? phoneFromE164(markets, profile.phone || user?.phone, d.countryCode);
  const contactPhone = phoneE164(markets, contactPhoneValue);
  const cover = market ? coverChargeFor(d.priceBand, guests, market) : 0;
  const phoneOk = !!contactPhone;

  const choose = (s: Slot) => {
    setSlot(s);
    idemKey.current = null; // new selection = new booking attempt
  };

  const confirm = async () => {
    if (!slot) return;
    idemKey.current ??= crypto.randomUUID();
    setProfile({ name: contactName.trim(), phone: contactPhone ?? "" });
    const res = await book.run({
      restaurantId: d.id,
      date,
      slotId: slot.id,
      guests,
      dealId: slot.deal?.id,
      specialRequest: note,
      contact: { name: contactName, phone: contactPhone ?? "" },
      idempotencyKey: idemKey.current,
    });
    if (!res) {
      slots.reload();
      return;
    }
    if (res.paymentIntent) setIntent(res.paymentIntent);
    else router.push(`/bookings/${res.booking.id}?new=1`);
  };

  return (
    <div className="mx-auto max-w-2xl pb-36 md:pb-10">
      <button
        onClick={() => (step === "details" ? setStep("pick") : router.back())}
        className="mb-3 inline-flex items-center gap-1 text-sm font-semibold text-muted hover:text-ink"
      >
        <ArrowLeft className="h-4 w-4" aria-hidden /> {step === "details" ? "Change slot" : "Back"}
      </button>

      <div className="mb-5 flex items-center gap-3">
        <Art hue={d.art.hue} motif={d.art.motif} image={d.art.image} credit={d.art.imageCredit} className="h-14 w-14 shrink-0 rounded-xl" iconClassName="h-10 w-10" />
        <div>
          <h1 className="text-xl font-black tracking-tight">Book a table</h1>
          <p className="text-sm text-muted">
            {d.name} · {d.area}
          </p>
        </div>
      </div>

      {step === "pick" && (
        <div className="space-y-5">
          <Card className="flex items-center justify-between">
            <span className="flex items-center gap-2 font-semibold">
              <Users className="h-5 w-5 text-brand" aria-hidden /> Guests
            </span>
            <Stepper value={guests} min={1} max={20} onChange={(g) => { setGuests(g); setSlot(null); }} label="guests" />
          </Card>

          <section>
            <h2 className="mb-2 flex items-center gap-2 font-bold">
              <CalendarDays className="h-5 w-5 text-brand" aria-hidden /> Date
            </h2>
            <div className="no-scrollbar -mx-4 flex gap-2 overflow-x-auto px-4 py-1" role="radiogroup" aria-label="Date">
              {dates.map((iso) => {
                const dt = fromISODate(iso);
                const active = iso === date;
                return (
                  <button
                    key={iso}
                    role="radio"
                    aria-checked={active}
                    onClick={() => { setDate(iso); setSlot(null); }}
                    className={clsx(
                      "flex w-16 shrink-0 flex-col items-center rounded-2xl py-2 ring-1 transition-colors",
                      active ? "bg-brand text-white ring-brand" : "bg-surface ring-line hover:ring-faint",
                    )}
                  >
                    <span className={clsx("text-xs font-semibold", active ? "text-white/80" : "text-muted")}>{dayLabel(iso, today)}</span>
                    <span className="text-lg font-black">{dt.getDate()}</span>
                    <span className={clsx("text-xs", active ? "text-white/80" : "text-muted")}>
                      {dt.toLocaleDateString(undefined, { month: "short" })}
                    </span>
                  </button>
                );
              })}
            </div>
          </section>

          <section>
            <h2 className="mb-2 flex items-center gap-2 font-bold">
              <Clock className="h-5 w-5 text-brand" aria-hidden /> Time
            </h2>
            {slots.error && <ErrorState error={slots.error} onRetry={slots.reload} />}
            {!avail && !slots.error && <Skeleton className="h-40 w-full" />}
            {avail && avail.slots.length === 0 && (
              <p className="rounded-xl bg-surface p-4 text-sm text-muted ring-1 ring-line">{d.name} isn&apos;t taking bookings on Feast right now.</p>
            )}
            {avail && avail.slots.length > 0 && (
              <>
                <div className="mb-3 flex gap-2" role="tablist">
                  {periods.map((p) => (
                    <button
                      key={p}
                      role="tab"
                      aria-selected={activePeriod === p}
                      onClick={() => setPeriod(p)}
                      className={clsx(
                        "rounded-full px-4 py-1.5 text-sm font-semibold ring-1",
                        activePeriod === p ? "bg-ink text-white ring-ink" : "bg-surface ring-line",
                      )}
                    >
                      {PERIOD_LABEL[p]}
                    </button>
                  ))}
                </div>
                {shown.every((s) => !s.available) ? (
                  <p className="rounded-xl bg-surface p-4 text-sm text-muted ring-1 ring-line">
                    No {activePeriod ? PERIOD_LABEL[activePeriod].toLowerCase() : ""} tables left for {guests} on {longDate(date)}. Try another
                    time or date.
                  </p>
                ) : (
                  <div className="grid grid-cols-3 gap-2 sm:grid-cols-4" role="radiogroup" aria-label="Time slot">
                    {shown.map((s) => {
                      const active = slot?.id === s.id;
                      return (
                        <button
                          key={s.id}
                          role="radio"
                          aria-checked={active}
                          disabled={!s.available}
                          onClick={() => choose(s)}
                          className={clsx(
                            "flex flex-col items-center rounded-xl px-2 py-2.5 ring-1 transition-colors disabled:cursor-not-allowed",
                            active
                              ? "bg-brand-soft ring-2 ring-brand"
                              : s.available
                                ? "bg-surface ring-line hover:ring-faint"
                                : "bg-canvas text-faint ring-line",
                          )}
                        >
                          <span className="font-bold">{time12(s.time)}</span>
                          <span className={clsx("text-xs font-semibold", s.available ? "text-deal" : "text-faint")}>
                            {s.available ? (s.deal ? `${s.deal.percentOff}% off` : "No deal") : "Full"}
                          </span>
                        </button>
                      );
                    })}
                  </div>
                )}
              </>
            )}
          </section>
        </div>
      )}

      {step === "details" && slot && (
        <div className="space-y-4">
          <Card className="space-y-2">
            <p className="text-lg font-bold">
              {longDate(date)} · {time12(slot.time)}
            </p>
            <p className="text-muted">
              {guests} {guests === 1 ? "guest" : "guests"}
            </p>
            {slot.deal && (
              <div className="rounded-xl bg-deal-soft p-3">
                <p className="font-bold text-deal">{slot.deal.title} on your total bill</p>
                <ul className="mt-1 list-disc pl-5 text-sm text-muted">
                  {slot.deal.terms.map((t) => (
                    <li key={t}>{t}</li>
                  ))}
                </ul>
              </div>
            )}
          </Card>

          <Card className="space-y-3">
            <h2 className="font-bold">Your details</h2>
            <label className="block">
              <span className="mb-1 block text-sm font-medium">Name</span>
              <input
                value={contactName}
                onChange={(e) => setName(e.target.value)}
                autoComplete="name"
                className="h-11 w-full rounded-xl bg-canvas px-3 ring-1 ring-line outline-none focus:ring-2 focus:ring-brand"
              />
            </label>
            <PhoneField value={contactPhoneValue} onChange={setPhone} />
            <label className="block">
              <span className="mb-1 block text-sm font-medium">Special request (optional)</span>
              <textarea
                value={note}
                onChange={(e) => setNote(e.target.value.slice(0, 200))}
                rows={2}
                placeholder="Birthday, window seat, high chair…"
                className="w-full rounded-xl bg-canvas px-3 py-2 ring-1 ring-line outline-none focus:ring-2 focus:ring-brand"
              />
            </label>
          </Card>

          {cover > 0 && (
            <Card className="flex gap-3">
              <Info className="mt-0.5 h-5 w-5 shrink-0 text-brand" aria-hidden />
              <p className="text-sm text-muted">
                This venue collects a <b className="text-ink">{money(cover)}</b> cover charge to hold your table. It&apos;s adjusted in your final bill.
              </p>
            </Card>
          )}

          {book.error && (
            <p role="alert" className="rounded-xl bg-red-50 p-3 text-sm text-danger">
              {book.error.message}
            </p>
          )}
          <p className="text-xs text-muted">Free cancellation up to 1 hour before your slot. Please arrive within 15 minutes of your booked time.</p>
        </div>
      )}

      <div className="fixed inset-x-0 bottom-[calc(57px+env(safe-area-inset-bottom))] z-30 border-t border-line bg-surface px-4 py-3 md:static md:mt-6 md:border-0 md:bg-transparent md:p-0">
        <div className="mx-auto max-w-2xl">
          {step === "pick" ? (
            <Button size="lg" className="w-full" disabled={!slot} onClick={() => setStep("details")}>
              {slot ? `Continue · ${time12(slot.time)}${slot.deal ? ` · ${slot.deal.percentOff}% off` : ""}` : "Select a time slot"}
            </Button>
          ) : (
            <Button size="lg" className="w-full" loading={book.pending} disabled={!contactName.trim() || !phoneOk} onClick={confirm}>
              {cover > 0 ? `Pay ${money(cover)} & confirm` : "Confirm booking"}
            </Button>
          )}
        </div>
      </div>

      <Checkout
        open={!!intent}
        intent={intent}
        title={`Cover charge · ${d.name}`}
        onClose={() => setIntent(null)}
        onSuccess={(i) => router.push(`/bookings/${i.resultRef}?new=1`)}
      />
    </div>
  );
}
