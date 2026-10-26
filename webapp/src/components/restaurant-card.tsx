import Link from "next/link";
import clsx from "clsx";
import { MapPin } from "lucide-react";
import type { RestaurantSummary } from "@/lib/api/types";
import { money } from "@/lib/format";
import { Art } from "./art";
import { OfferTag, Rating } from "./ui";

/** Tall card for carousels. */
export function RestaurantCard({ r, className }: { r: RestaurantSummary; className?: string }) {
  return (
    <Link
      href={`/restaurants/${r.slug}`}
      className={clsx(
        "group relative block overflow-hidden rounded-[var(--radius-card)] bg-surface ring-1 ring-line transition-shadow hover:shadow-[var(--shadow-card)] focus-visible:outline-2 focus-visible:outline-brand",
        className,
      )}
    >
      <div className="relative">
        <Art hue={r.art.hue} motif={r.art.motif} image={r.art.image} credit={r.art.imageCredit} className="aspect-[4/3] w-full" />
        {r.headlineOffer && (
          <span className="absolute bottom-2 left-2 rounded-lg bg-deal px-2 py-1 text-xs font-bold text-white shadow">
            {r.headlineOffer.replace(" on pre-booking", "")}
          </span>
        )}
      </div>
      <div className="space-y-1 p-3">
        <div className="flex items-start justify-between gap-2">
          <h3 className="line-clamp-1 font-bold group-hover:text-brand">{r.name}</h3>
          <Rating value={r.rating} />
        </div>
        <p className="line-clamp-1 text-sm text-muted">{r.cuisines.join(", ")}</p>
        <p className="flex items-center gap-1 text-sm text-muted">
          <MapPin className="h-3.5 w-3.5" aria-hidden />
          {r.area} · {r.distanceKm} km · {money(r.costForTwo, r.currency)} for two
        </p>
      </div>
    </Link>
  );
}

/** Wide row for lists and search results. */
export function RestaurantRow({ r }: { r: RestaurantSummary }) {
  return (
    <Link
      href={`/restaurants/${r.slug}`}
      className="group relative flex gap-4 rounded-[var(--radius-card)] bg-surface p-3 ring-1 ring-line transition-shadow hover:shadow-[var(--shadow-card)] focus-visible:outline-2 focus-visible:outline-brand"
    >
      <Art hue={r.art.hue} motif={r.art.motif} image={r.art.image} credit={r.art.imageCredit} className="h-28 w-28 shrink-0 rounded-xl md:h-32 md:w-40" iconClassName="h-16 w-16" />
      <div className="min-w-0 flex-1 space-y-1 py-0.5">
        <div className="flex items-start justify-between gap-2">
          <h3 className="line-clamp-1 text-base font-bold group-hover:text-brand">{r.name}</h3>
          <Rating value={r.rating} count={r.ratingCount} className="shrink-0" />
        </div>
        <p className="line-clamp-1 text-sm text-muted">{r.cuisines.join(", ")}</p>
        <p className="text-sm text-muted">
          {r.area} · {r.distanceKm} km · {money(r.costForTwo, r.currency)} for two
        </p>
        <div className="flex flex-wrap gap-1.5 pt-0.5">
          {r.isPureVeg && <span className="rounded-md bg-green-50 px-1.5 py-0.5 text-xs font-medium text-veg">Pure veg</span>}
          {r.tags
            .filter((t) => t !== "Pure veg")
            .slice(0, 2)
            .map((t) => (
              <span key={t} className="rounded-md bg-canvas px-1.5 py-0.5 text-xs font-medium text-muted">
                {t}
              </span>
            ))}
        </div>
        {r.headlineOffer && <OfferTag className="pt-1">{r.headlineOffer}</OfferTag>}
      </div>
    </Link>
  );
}
