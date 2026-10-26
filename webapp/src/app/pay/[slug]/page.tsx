"use client";

import { Suspense, useEffect, useMemo, useState } from "react";
import { useParams, useRouter, useSearchParams } from "next/navigation";
import clsx from "clsx";
import Link from "next/link";
import { ArrowLeft, BadgePercent, Banknote, CalendarCheck, CheckCircle2, Clock, CreditCard, Info, Sparkles, XCircle } from "lucide-react";
import { Art } from "@/components/art";
import { Checkout } from "@/components/checkout";
import { Button, Card, Chip, ErrorState, Sheet, Skeleton } from "@/components/ui";
import { getApi } from "@/lib/api/client";
import { useLocalState, useMutation, useQuery, useTick } from "@/lib/hooks";
import { currencySymbol, money, setActiveCurrency, toISODate, toMinor } from "@/lib/format";
import { useMarkets } from "@/components/providers";
import { offerDiscount } from "@/lib/pricing";
import { ApiError, type Bill, type OfflineMethod, type OfflinePayment, type Offer, type PaymentIntent } from "@/lib/api/types";
import { OFFLINE_METHOD_LABEL } from "@/components/audit";

function PayInner() {
  const { slug } = useParams<{ slug: string }>();
  const params = useSearchParams();
  const bookingParam = params.get("booking") ?? undefined;
  const router = useRouter();

  const r = useQuery(`restaurant:${slug}`, (api) => api.getRestaurant(slug));
  const bookings = useQuery("bookings", (api) => api.listBookings());

  const [amountText, setAmountText] = useState("");
  const [tip, setTip] = useState(0);
  const [offerId, setOfferId] = useState<string | undefined>(undefined);
  const [quote, setQuote] = useState<{ key: string; bill?: Bill; error?: Error } | null>(null);
  const [intent, setIntent] = useState<PaymentIntent | null>(null);
  // Bumped when the server says today's deals just ran out, to fetch the new total.
  const [requote, setRequote] = useState(0);
  // A counter payment waiting for the restaurant survives reloads (its bill id).
  const [counterBill, setCounterBill, counterReady] = useLocalState<string>(`feast.counter.${slug}`, "");
  const [counterOpen, setCounterOpen] = useState(false);
  const submitCounter = useMutation(async (api, billId: string, method: OfflineMethod) => {
    try {
      return await api.submitOfflinePayment(billId, method);
    } catch (e) {
      if (e instanceof ApiError && (e.code === "DEALS_USED_UP" || e.code === "BOOKING_DEAL_USED")) setRequote((n) => n + 1);
      throw e;
    }
  });
  const createIntent = useMutation(async (api, billId: string) => {
    try {
      return await api.createPaymentIntent("BILL", billId);
    } catch (e) {
      // The deal went between the quote and payment (daily limit reached, or this
      // booking's deal used on another bill): re-price.
      if (e instanceof ApiError && (e.code === "DEALS_USED_UP" || e.code === "BOOKING_DEAL_USED")) setRequote((n) => n + 1);
      throw e;
    }
  });

  const d = r.data;
  const market = useMarkets().marketOf(d?.countryCode);
  if (d) setActiveCurrency(d.currency);
  const unit = 10 ** (market?.currencyDecimals ?? 2);
  const tips = market?.tipPresets ?? [0];
  // Feast deals are for diners who booked on Feast: today's booking here (the
  // server checks it again and has the final say).
  const todaysBooking = useMemo(() => {
    const today = toISODate(new Date());
    const mine = bookings.data?.filter(
      (b) => d && b.restaurant.id === d.id && b.date === today && (b.status === "CONFIRMED" || b.status === "SEATED" || b.status === "COMPLETED"),
    );
    return mine?.find((b) => b.id === bookingParam) ?? mine?.[0];
  }, [bookings.data, d, bookingParam]);
  const bookingsKnown = !!bookings.data || !!bookings.error;
  const amount = d ? toMinor(amountText || 0, d.currency) : 0;

  const offers: Offer[] = useMemo(() => {
    if (!d) return [];
    // No booking, no Feast deal.
    if (!todaysBooking) return [];
    const list = d.offers.filter((o) => o.scope === "BILL");
    if (todaysBooking?.deal)
      list.unshift({ ...todaysBooking.deal, id: `${todaysBooking.id}_deal`, scope: "BILL", subtitle: `Pre-booking deal · ${todaysBooking.code}` });
    return list;
  }, [d, todaysBooking]);

  // Server-side quote (debounced). The server is the source of truth for discounts.
  const quoteKey = d && amount >= unit ? `${d.id}:${amount}:${tip}:${offerId ?? "best"}:${todaysBooking?.id ?? ""}:${requote}` : null;
  useEffect(() => {
    if (!quoteKey || !d) return;
    let alive = true;
    const t = setTimeout(async () => {
      try {
        const api = await getApi();
        const bill = await api.quoteBill({ restaurantId: d.id, billAmount: amount, tip, offerId, bookingId: todaysBooking?.id });
        if (alive) setQuote({ key: quoteKey, bill });
      } catch (error) {
        if (alive) setQuote({ key: quoteKey, error: error as Error });
      }
    }, 350);
    return () => {
      alive = false;
      clearTimeout(t);
    };
  }, [quoteKey, d, amount, tip, offerId, todaysBooking]);

  const bill = quote?.key === quoteKey ? quote.bill : undefined;
  const quoteError = quote?.key === quoteKey ? quote.error : undefined;
  const quoting = !!quoteKey && quote?.key !== quoteKey;

  if (r.error) return <ErrorState error={r.error} onRetry={r.reload} />;
  if (!d) return <Skeleton className="mx-auto h-96 max-w-xl" />;
  if (counterReady && counterBill)
    return (
      <CounterStatus
        billId={counterBill}
        restaurantName={d.name}
        onPaid={(bpId) => {
          setCounterBill("");
          router.replace(`/payments/${bpId}`);
        }}
        onPayAgain={() => {
          setCounterBill("");
          setRequote((n) => n + 1);
        }}
      />
    );
  const bookingRequired = quoteError instanceof ApiError && quoteError.code === "BOOKING_REQUIRED";
  // Paying on Feast is for diners who booked a table here on Feast today.
  if ((bookingsKnown && !todaysBooking) || bookingRequired) return <BookFirst slug={d.slug} name={d.name} headline={d.headlineOffer} />;

  const pay = async () => {
    if (!bill) return;
    const i = await createIntent.run(bill.id);
    if (i) setIntent(i);
  };
  /** Client preview of an offer's saving, within the restaurant's per-bill cap (the server decides). */
  const previewSaving = (o: Offer, amt: number, m: NonNullable<typeof market>) => {
    const s = offerDiscount(o, amt, m);
    const cap = d?.settings?.maxDiscountPerBill;
    return cap && cap > 0 ? Math.min(s, cap) : s;
  };

  return (
    <div className="mx-auto max-w-xl space-y-4 pb-36 md:pb-10">
      <button onClick={() => router.back()} className="inline-flex items-center gap-1 text-sm font-semibold text-muted hover:text-ink">
        <ArrowLeft className="h-4 w-4" aria-hidden /> Back
      </button>

      <div className="flex items-center gap-3">
        <Art hue={d.art.hue} motif={d.art.motif} image={d.art.image} credit={d.art.imageCredit} className="h-14 w-14 shrink-0 rounded-xl" iconClassName="h-10 w-10" />
        <div>
          <h1 className="text-xl font-black tracking-tight">Pay bill</h1>
          <p className="text-sm text-muted">
            {d.name} · {d.area}
          </p>
        </div>
      </div>

      <Card>
          <label htmlFor="amount" className="text-sm font-semibold">
            Enter the total on your restaurant bill
          </label>
          <div className="mt-2 flex items-center gap-1 border-b-2 border-brand pb-1">
            <span className="text-3xl font-black text-muted">{currencySymbol(d.currency)}</span>
            <input
              id="amount"
              autoFocus
              inputMode="decimal"
              placeholder="0"
              value={amountText}
              onChange={(e) => {
                const v = e.target.value.replace(/[^\d.]/g, "");
                if (/^\d{0,6}(\.\d{0,2})?$/.test(v)) setAmountText(v);
              }}
              className="w-full bg-transparent text-4xl font-black outline-none"
            />
          </div>
          <p className="mt-2 text-xs text-muted">Including taxes and service charge, as printed on the bill. The restaurant sees what you paid.</p>
        </Card>

      {todaysBooking && (
        <div className="flex items-center gap-2 rounded-xl bg-deal-soft p-3 text-sm">
          <Sparkles className="h-4 w-4 shrink-0 text-deal" aria-hidden />
          <span>
            You booked a table here today ({todaysBooking.code}) — your Feast deal is applied when you pay here
            {todaysBooking.deal ? (
              <>
                : <b>{todaysBooking.deal.title}</b>
              </>
            ) : null}
            .
          </span>
        </div>
      )}

      {offers.length > 0 && (
        <Card>
          <h2 className="mb-2 font-bold">Offers</h2>
          <div className="space-y-2" role="radiogroup" aria-label="Offer">
            {[{ id: undefined, title: "Best offer", subtitle: "We'll apply the biggest saving automatically" } as Partial<Offer>, ...offers].map((o) => {
              const selected = offerId === o.id;
              const saving = o.id && amount && market ? previewSaving(o as Offer, amount, market) : undefined;
              return (
                <button
                  key={o.id ?? "best"}
                  role="radio"
                  aria-checked={selected}
                  onClick={() => setOfferId(o.id)}
                  className={clsx("flex w-full items-center gap-3 rounded-xl p-3 text-left ring-1", selected ? "bg-brand-soft ring-brand" : "ring-line hover:bg-canvas")}
                >
                  <BadgePercent className={clsx("h-5 w-5 shrink-0", o.id ? "text-deal" : "text-brand")} aria-hidden />
                  <span className="flex-1">
                    <span className="block font-semibold">{o.title}</span>
                    <span className="text-sm text-muted">{o.subtitle}</span>
                  </span>
                  {saving !== undefined && amount > 0 && (
                    <span className={clsx("text-sm font-semibold", saving > 0 ? "text-veg" : "text-faint")}>
                      {saving > 0 ? `−${money(saving)}` : "Not eligible"}
                    </span>
                  )}
                </button>
              );
            })}
          </div>
        </Card>
      )}

      <Card>
        <h2 className="mb-1 font-bold">Add a tip for the staff</h2>
        <p className="mb-3 text-sm text-muted">100% goes to the restaurant team.</p>
        <div className="flex flex-wrap gap-2">
          {tips.map((t) => (
            <Chip key={t} active={tip === t} onClick={() => setTip(t)}>
              {t === 0 ? "No tip" : money(t)}
            </Chip>
          ))}
        </div>
      </Card>

      {quoteError && !bookingRequired && <p role="alert" className="rounded-xl bg-red-50 p-3 text-sm text-danger">{quoteError.message}</p>}

      {bill && (
        <Card className={clsx("space-y-1.5", quoting && "opacity-60")}>
          <h2 className="mb-1 font-bold">Summary</h2>
          <div className="flex justify-between text-sm">
            <span className="text-muted">Bill amount</span>
            <span>{money(bill.billAmount)}</span>
          </div>
          {bill.lines
            .filter((l) => l.kind === "DISCOUNT" || l.kind === "TIP")
            .map((l) => (
              <div key={l.label} className={clsx("flex justify-between text-sm", l.kind === "DISCOUNT" && "text-veg")}>
                <span>{l.label}</span>
                <span>{money(l.amount)}</span>
              </div>
            ))}
          <div className="flex justify-between border-t border-line pt-2 text-lg font-black">
            <span>To pay</span>
            <span>{money(bill.payable)}</span>
          </div>
          {bill.savings > 0 && (
            <p className="flex items-center gap-1.5 rounded-lg bg-green-50 px-3 py-2 text-sm font-semibold text-veg">
              <CheckCircle2 className="h-4 w-4" aria-hidden /> You save {money(bill.savings)} on this bill
            </p>
          )}
          {bill.dealNotice && (
            <p className="flex items-start gap-1.5 rounded-lg bg-amber-50 px-3 py-2 text-sm text-amber-900" role="status">
              <Info className="mt-0.5 h-4 w-4 shrink-0" aria-hidden /> {bill.dealNotice}
            </p>
          )}
        </Card>
      )}

      <div className="fixed inset-x-0 bottom-[calc(57px+env(safe-area-inset-bottom))] z-30 border-t border-line bg-surface px-4 py-3 md:static md:border-0 md:bg-transparent md:p-0">
        <div className="mx-auto max-w-xl">
          <Button size="lg" className="w-full" disabled={!bill || quoting} loading={createIntent.pending} onClick={pay}>
            {bill ? `Pay ${money(bill.payable)}` : amount >= unit ? "Calculating…" : "Enter bill amount"}
          </Button>
          {createIntent.error && <p className="mt-2 text-sm text-danger">{createIntent.error.message}</p>}
          <button
            type="button"
            disabled={!bill || quoting}
            onClick={() => setCounterOpen(true)}
            className="mt-2 w-full rounded-xl py-2 text-sm font-semibold text-brand hover:bg-brand-soft disabled:text-faint disabled:hover:bg-transparent"
          >
            Pay at the counter instead (cash or card)
          </button>
        </div>
      </div>

      <Sheet open={counterOpen} onClose={() => setCounterOpen(false)} title="Pay at the counter">
        <div className="space-y-3">
          <p className="text-sm text-muted">
            Pay {bill ? <b className="text-ink">{money(bill.payable)}</b> : "your bill"} to the staff. {d.name} confirms it in their app, and your receipt
            {bill && bill.savings > 0 ? " — with your Feast deal —" : ""} appears here. Feast doesn&apos;t handle this money.
          </p>
          {(["CASH", "CARD_AT_COUNTER"] as OfflineMethod[]).map((m) => (
            <Button
              key={m}
              variant="secondary"
              size="lg"
              className="w-full justify-start"
              loading={submitCounter.pending}
              disabled={!bill}
              onClick={async () => {
                if (!bill) return;
                const op = await submitCounter.run(bill.id, m);
                if (op) {
                  setCounterOpen(false);
                  setCounterBill(op.billId);
                }
              }}
            >
              {m === "CASH" ? <Banknote className="h-5 w-5" aria-hidden /> : <CreditCard className="h-5 w-5" aria-hidden />} {OFFLINE_METHOD_LABEL[m]}
            </Button>
          ))}
          {submitCounter.error && (
            <p role="alert" className="text-sm text-danger">
              {submitCounter.error.message}
            </p>
          )}
        </div>
      </Sheet>

      <Checkout
        open={!!intent}
        intent={intent}
        title={`Bill at ${d.name}`}
        onClose={() => setIntent(null)}
        onSuccess={(i) => router.replace(`/payments/${i.resultRef}`)}
      />
    </div>
  );
}

/** Waiting for the restaurant to confirm a counter payment (polls; the bell also notifies). */
function CounterStatus({ billId, restaurantName, onPaid, onPayAgain }: { billId: string; restaurantName: string; onPaid: (billPaymentId: string) => void; onPayAgain: () => void }) {
  const tick = useTick(4000);
  const op = useQuery(`counter:${billId}:${tick}`, (api) => api.getOfflinePayment(billId));
  const p: OfflinePayment | null | undefined = op.data;
  useEffect(() => {
    if (p?.status === "APPROVED" && p.billPaymentId) onPaid(p.billPaymentId);
  }, [p, onPaid]);
  if (op.error && !p) return <ErrorState error={op.error} onRetry={op.reload} />;
  if (p === undefined) return <Skeleton className="mx-auto h-64 max-w-xl" />;
  const closed = !p || p.status === "REJECTED" || p.status === "EXPIRED";
  return (
    <div className="mx-auto max-w-xl space-y-4">
      <Card className="space-y-3 text-center" aria-live="polite">
        <span className={clsx("mx-auto grid h-14 w-14 place-items-center rounded-2xl", closed ? "bg-red-50 text-danger" : "bg-brand-soft text-brand")}>
          {closed ? <XCircle className="h-7 w-7" aria-hidden /> : <Clock className="h-7 w-7" aria-hidden />}
        </span>
        {p && !closed && (
          <>
            <h1 className="text-xl font-black tracking-tight">Waiting for {restaurantName} to confirm</h1>
            <p className="text-sm text-muted">
              Pay <b className="text-ink">{money(p.amount, p.currency)}</b> {p.method === "CASH" ? "in cash" : "by card"} at the counter. Staff confirm it in their
              app and your receipt opens here{p.savings > 0 ? `, with your Feast deal (you save ${money(p.savings, p.currency)})` : ""}.
            </p>
            <p className="text-xs text-faint">If nobody confirms it by {new Date(p.expiresAt).toLocaleTimeString(undefined, { timeStyle: "short" })}, it lapses and you can pay again.</p>
          </>
        )}
        {closed && (
          <>
            <h1 className="text-xl font-black tracking-tight">Counter payment not confirmed</h1>
            <p className="text-sm text-muted">
              {p?.status === "EXPIRED" ? "Nobody confirmed it in time." : `${restaurantName} didn't confirm it${p?.reason ? `: “${p.reason}”` : "."}`} Your bill is still open.
            </p>
            <Button size="lg" className="w-full" onClick={onPayAgain}>
              Pay again
            </Button>
          </>
        )}
      </Card>
    </div>
  );
}

/** A walk-in without a Feast booking: deals (and paying a typed-in bill) need one. */
function BookFirst({ slug, name, headline }: { slug: string; name: string; headline?: string }) {
  return (
    <div className="mx-auto max-w-xl space-y-4">
      <Card className="space-y-3 text-center">
        <span className="mx-auto grid h-14 w-14 place-items-center rounded-2xl bg-deal-soft text-deal">
          <CalendarCheck className="h-7 w-7" aria-hidden />
        </span>
        <h1 className="text-xl font-black tracking-tight">Book on Feast to pay here and save</h1>
        <p className="text-sm text-muted">
          Feast deals at {name} are for diners who book a table on Feast and pay their bill through Feast. Book your table, then pay here on the day
          {headline ? ` to get ${headline.toLowerCase()}` : " to get your deal"}.
        </p>
        <div className="flex flex-col gap-2 sm:flex-row sm:justify-center">
          <Link href={`/restaurants/${slug}/book`} className="inline-flex h-11 items-center justify-center rounded-xl bg-brand px-5 font-semibold text-white hover:opacity-90">
            Book a table
          </Link>
          <Link href={`/restaurants/${slug}`} className="inline-flex h-11 items-center justify-center rounded-xl px-5 font-semibold ring-1 ring-line hover:bg-canvas">
            See the restaurant
          </Link>
        </div>
        <p className="text-xs text-muted">Already here without a booking? Pay at the counter as usual.</p>
      </Card>
    </div>
  );
}

export default function PayPage() {
  return (
    <Suspense fallback={<Skeleton className="mx-auto h-96 max-w-xl" />}>
      <PayInner />
    </Suspense>
  );
}
