"use client";

import Link from "next/link";
import { Suspense, useMemo, useState } from "react";
import { useParams, useRouter, useSearchParams } from "next/navigation";
import clsx from "clsx";
import { ArrowLeft, Clock, MapPin, Navigation, Phone, Share2, Sparkles, BadgePercent, CreditCard, CalendarDays } from "lucide-react";
import { Art } from "@/components/art";
import { MenuList } from "@/components/menu-list";
import { useAuth, useToast } from "@/components/providers";
import { Card, Chip, ErrorState, LinkButton, Rating, Sheet, Skeleton, VegMark } from "@/components/ui";
import { ReviewsSection } from "@/components/trust";
import { useQuery } from "@/lib/hooks";
import { priceBand, money, setActiveCurrency, toISODate } from "@/lib/format";
import type { Offer } from "@/lib/api/types";

const TABS = ["overview", "menu", "photos"] as const;
type Tab = (typeof TABS)[number];

const SCOPE_META: Record<Offer["scope"], { label: string; icon: typeof BadgePercent; tone: string }> = {
  PREBOOK: {
    label: "Pre-book offer",
    icon: CalendarDays,
    tone: "bg-deal-soft text-deal",
  },
  BILL: {
    label: "Bill payment offer",
    icon: BadgePercent,
    tone: "bg-brand-soft text-brand-strong",
  },
  BANK: {
    label: "Bank offer",
    icon: CreditCard,
    tone: "bg-sky-50 text-sky-800",
  },
};

function RestaurantInner() {
  const { slug } = useParams<{ slug: string }>();
  const sp = useSearchParams();
  const router = useRouter();
  const toast = useToast();
  const tab = (TABS.includes(sp.get("tab") as Tab) ? sp.get("tab") : "overview") as Tab;
  const [offer, setOffer] = useState<Offer | null>(null);
  const [vegOnly, setVegOnly] = useState(false);

  const r = useQuery(`restaurant:${slug}`, (api) => api.getRestaurant(slug));
  const { user, ready } = useAuth();
  const bookings = useQuery(ready && user ? "bookings" : null, (api) => api.listBookings());
  // Feast deals (and paying here) are for diners with a Feast booking today.
  const restaurantId = r.data?.id;
  const todaysBooking = useMemo(() => {
    const today = toISODate(new Date());
    return bookings.data?.find(
      (b) => b.restaurant.id === restaurantId && b.date === today && (b.status === "CONFIRMED" || b.status === "SEATED" || b.status === "COMPLETED"),
    );
  }, [bookings.data, restaurantId]);
  // Prices on this page are in the restaurant's currency (it may differ from the browsed city's).
  if (r.data) setActiveCurrency(r.data.currency);
  const menu = useQuery(r.data && tab === "menu" ? `menu:${r.data.id}` : null, (api) => api.getMenu(r.data!.id));

  if (r.error) return <ErrorState error={r.error} onRetry={r.reload} />;
  if (!r.data)
    return (
      <div className="space-y-4" aria-busy="true">
        <Skeleton className="h-56 w-full rounded-3xl" />
        <Skeleton className="h-8 w-2/3" />
        <Skeleton className="h-24 w-full" />
      </div>
    );

  const d = r.data;
  const prebook = d.offers.find((o) => o.scope === "PREBOOK");
  const canBook = d.acceptsBookings;
  const canPay = d.acceptsBillPayment && !!todaysBooking;
  const payHref = todaysBooking ? `/pay/${d.slug}?booking=${encodeURIComponent(todaysBooking.id)}` : `/pay/${d.slug}`;
  const bestPct = Math.max(0, ...d.offers.filter((o) => o.scope !== "BANK").map((o) => o.percentOff ?? 0));
  const bookBtn = (cls: string) =>
    canBook ? (
      <LinkButton href={`/restaurants/${d.slug}/book`} className={cls}>
        {bestPct > 0 && !todaysBooking ? `Book a table to get ${bestPct}% off` : "Book a table"}
      </LinkButton>
    ) : (
      <span className={clsx("inline-flex h-11 items-center justify-center rounded-xl bg-canvas px-4 text-sm font-semibold text-faint ring-1 ring-line", cls)}>
        Bookings paused
      </span>
    );
  const payBtn = (cls: string) =>
    canPay ? (
      <LinkButton href={payHref} variant="secondary" className={cls}>
        Pay bill &amp; get your deal
      </LinkButton>
    ) : null;
  const setTab = (t: Tab) => router.replace(`/restaurants/${slug}${t === "overview" ? "" : `?tab=${t}`}`, { scroll: false });

  const share = async () => {
    const url = window.location.href;
    try {
      if (navigator.share) await navigator.share({ title: d.name, url });
      else {
        await navigator.clipboard.writeText(url);
        toast("Link copied");
      }
    } catch {
      // user dismissed share sheet
    }
  };

  return (
    <div className="pb-24 md:pb-0">
      <button onClick={() => router.back()} className="mb-3 inline-flex items-center gap-1 text-sm font-semibold text-muted hover:text-ink">
        <ArrowLeft className="h-4 w-4" aria-hidden /> Back
      </button>

      <div className="grid gap-6 md:grid-cols-[1fr_340px]">
        <div className="min-w-0 space-y-5">
          <Art hue={d.art.hue} motif={d.art.motif} image={d.art.image} credit={d.art.imageCredit} className="h-48 rounded-3xl md:h-64" iconClassName="h-40 w-40" label={`${d.name} cover`} />
          {d.status === "PAUSED" && (
            <p className="rounded-xl bg-amber-50 p-3 text-sm font-medium text-amber-900" role="status">
              {d.name} has paused Feast bookings and payments for now. You can still browse the menu.
            </p>
          )}

          <div className="space-y-2">
            <div className="flex items-start justify-between gap-3">
              <h1 className="text-2xl font-black tracking-tight md:text-3xl">{d.name}</h1>
              <a href="#reviews" className="mt-1 shrink-0" aria-label="See reviews">
                <Rating value={d.rating} count={d.ratingCount} />
              </a>
            </div>
            <p className="text-muted">{d.cuisines.join(" · ")}</p>
            <p className="flex flex-wrap items-center gap-x-2 text-sm text-muted">
              <span className="inline-flex items-center gap-1">
                <MapPin className="h-4 w-4" aria-hidden />
                {d.area}, {d.distanceKm} km
              </span>
              <span aria-hidden>·</span>
              <span>
                {money(d.costForTwo)} for two <span className="text-faint">({priceBand(d.priceBand, d.currency)})</span>
              </span>
              <span aria-hidden>·</span>
              <span className="inline-flex items-center gap-1">
                <Clock className="h-4 w-4" aria-hidden />
                {d.hours[0].open}–{d.hours[0].close}
              </span>
            </p>
            <div className="flex flex-wrap gap-1.5 pt-1">
              {d.isPureVeg && <span className="rounded-md bg-green-50 px-2 py-1 text-xs font-semibold text-veg">Pure veg</span>}
              {d.tags
                .filter((t) => t !== "Pure veg")
                .map((t) => (
                  <span key={t} className="rounded-md bg-surface px-2 py-1 text-xs font-medium ring-1 ring-line">
                    {t}
                  </span>
                ))}
            </div>
            <div className="flex gap-2 pt-2">
              <a
                href={`tel:${d.phone.replace(/\s/g, "")}`}
                className="inline-flex h-9 items-center gap-1.5 rounded-lg bg-surface px-3 text-sm font-semibold ring-1 ring-line hover:bg-canvas"
              >
                <Phone className="h-4 w-4" aria-hidden /> Call
              </a>
              <a
                href={`https://www.google.com/maps/search/?api=1&query=${encodeURIComponent(d.address)}`}
                target="_blank"
                rel="noreferrer"
                className="inline-flex h-9 items-center gap-1.5 rounded-lg bg-surface px-3 text-sm font-semibold ring-1 ring-line hover:bg-canvas"
              >
                <Navigation className="h-4 w-4" aria-hidden /> Directions
              </a>
              <button
                onClick={share}
                className="inline-flex h-9 items-center gap-1.5 rounded-lg bg-surface px-3 text-sm font-semibold ring-1 ring-line hover:bg-canvas"
              >
                <Share2 className="h-4 w-4" aria-hidden /> Share
              </button>
            </div>
          </div>

          <section aria-label="Offers">
            <h2 className="mb-2 text-lg font-bold">Offers</h2>
            <div className="no-scrollbar -mx-4 flex gap-3 overflow-x-auto px-4 py-1 md:mx-0 md:grid md:grid-cols-2 md:px-0">
              {d.offers.map((o) => {
                const meta = SCOPE_META[o.scope];
                return (
                  <button
                    key={o.id}
                    onClick={() => setOffer(o)}
                    className="flex w-64 shrink-0 items-start gap-3 rounded-2xl bg-surface p-3 text-left ring-1 ring-line hover:ring-brand md:w-auto"
                  >
                    <span className={clsx("grid h-10 w-10 shrink-0 place-items-center rounded-xl", meta.tone)}>
                      <meta.icon className="h-5 w-5" aria-hidden />
                    </span>
                    <span>
                      <span className="block text-xs font-semibold uppercase tracking-wide text-muted">{meta.label}</span>
                      <span className="block font-bold">{o.title}</span>
                      <span className="block text-sm text-muted">{o.subtitle}</span>
                    </span>
                  </button>
                );
              })}
            </div>
          </section>

          <div className="sticky top-14 z-30 -mx-4 border-b border-line bg-canvas/95 px-4 backdrop-blur" role="tablist">
            <div className="flex gap-6">
              {TABS.map((t) => (
                <button
                  key={t}
                  role="tab"
                  aria-selected={tab === t}
                  onClick={() => setTab(t)}
                  className={clsx(
                    "border-b-2 py-3 text-sm font-bold capitalize",
                    tab === t ? "border-brand text-brand" : "border-transparent text-muted hover:text-ink",
                  )}
                >
                  {t}
                </button>
              ))}
            </div>
          </div>

          {tab === "overview" && (
            <div className="space-y-5">
              <Card>
                <h2 className="mb-1 font-bold">About</h2>
                <p className="text-muted">{d.about}</p>
                <ul className="mt-3 grid gap-2 sm:grid-cols-2">
                  {d.highlights.map((h) => (
                    <li key={h} className="flex items-center gap-2 text-sm">
                      <Sparkles className="h-4 w-4 shrink-0 text-brand" aria-hidden />
                      {h}
                    </li>
                  ))}
                </ul>
              </Card>
              <Card>
                <div className="mb-1 flex items-center justify-between">
                  <h2 className="font-bold">Popular dishes</h2>
                  <button onClick={() => setTab("menu")} className="text-sm font-semibold text-brand">
                    See menu
                  </button>
                </div>
                <ul className="divide-y divide-line">
                  {d.menuPreview.map((i) => (
                    <li key={i.id} className="flex items-center justify-between gap-3 py-2.5">
                      <span className="flex items-center gap-2">
                        <VegMark veg={i.isVeg} />
                        {i.name}
                      </span>
                      <span className="text-sm font-semibold">{money(i.price)}</span>
                    </li>
                  ))}
                </ul>
              </Card>
              <Card>
                <h2 className="mb-1 font-bold">Address</h2>
                <p className="text-muted">{d.address}</p>
                <p className="mt-2 text-sm text-muted">
                  Open every day, {d.hours[0].open}–{d.hours[0].close}
                </p>
              </Card>
            </div>
          )}

          {tab === "menu" && (
            <div>
              <div className="mb-2 flex items-center justify-between">
                <p className="text-sm text-muted">Menu prices as listed by the restaurant. Order with your server; book on Feast and pay here to get the deal.</p>
                {!d.isPureVeg && (
                  <Chip active={vegOnly} onClick={() => setVegOnly((v) => !v)}>
                    <VegMark veg /> Veg only
                  </Chip>
                )}
              </div>
              {menu.error && <ErrorState error={menu.error} onRetry={menu.reload} />}
              {!menu.data && !menu.error && <Skeleton className="h-64 w-full" />}
              {menu.data && <MenuList categories={menu.data.categories} vegOnly={vegOnly} stickyOffset={104} />}
            </div>
          )}

          {tab === "photos" && (
            <div className="grid grid-cols-2 gap-3 md:grid-cols-3">
              {d.photos.map((p) => (
                <figure key={p.id} className="overflow-hidden rounded-2xl bg-surface ring-1 ring-line">
                  <Art hue={p.hue} motif={d.art.motif} image={p.url} credit={p.credit} className="aspect-square" iconClassName="h-20 w-20" label={p.caption} />
                  <figcaption className="px-3 py-2 text-sm font-medium">{p.caption}</figcaption>
                </figure>
              ))}
            </div>
          )}
          <ReviewsSection slug={d.slug} />
        </div>

        {/* Desktop action card */}
        <aside className="hidden md:block">
          <div className="sticky top-20 space-y-3">
            <Card className="space-y-3">
              {prebook && canBook && (
                <div className="rounded-xl bg-deal-soft p-3">
                  <p className="font-bold text-deal">{prebook.title} on pre-booking</p>
                  <p className="text-sm text-muted">Pick an off-peak slot for an even bigger deal.</p>
                </div>
              )}
              {bookBtn("w-full")}
              {payBtn("w-full")}
            </Card>
          </div>
        </aside>
      </div>

      {/* Mobile sticky footer */}
      <div className="fixed inset-x-0 bottom-[calc(57px+env(safe-area-inset-bottom))] z-30 border-t border-line bg-surface px-4 py-3 md:hidden">
        {prebook && canBook && <p className="mb-2 text-center text-xs font-semibold text-deal">{prebook.title} when you pre-book</p>}
        <div className="flex gap-3">
          {payBtn("flex-1")}
          {bookBtn("flex-[2]")}
        </div>
      </div>

      <Sheet open={!!offer} onClose={() => setOffer(null)} title={offer?.title ?? ""}>
        {offer && (
          <div className="space-y-3">
            <p className="text-muted">{offer.subtitle}</p>
            <h3 className="font-semibold">Terms</h3>
            <ul className="list-disc space-y-1 pl-5 text-sm text-muted">
              {offer.terms.map((t) => (
                <li key={t}>{t}</li>
              ))}
            </ul>
            <p className="pt-1 text-sm text-muted">Feast deals are for diners who book on Feast and pay their bill in the app; the discount isn&apos;t available at the till.</p>
            {offer.scope !== "BANK" &&
              (todaysBooking && canPay ? (
                <Link href={payHref} className="inline-block pt-2 font-semibold text-brand">
                  Pay bill to use this →
                </Link>
              ) : (
                <Link href={`/restaurants/${d.slug}/book`} className="inline-block pt-2 font-semibold text-brand">
                  Book a table to use this →
                </Link>
              ))}
          </div>
        )}
      </Sheet>
    </div>
  );
}

export default function RestaurantPage() {
  return (
    <Suspense fallback={<Skeleton className="h-56 w-full rounded-3xl" />}>
      <RestaurantInner />
    </Suspense>
  );
}
