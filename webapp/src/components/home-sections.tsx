"use client";

import Link from "next/link";
import { ArrowRight } from "lucide-react";
import type { HomeSection, SearchQuery } from "@/lib/api/types";
import { Art } from "./art";
import { RestaurantCard, RestaurantRow } from "./restaurant-card";
import { SectionTitle } from "./ui";

export function queryToHref(q: SearchQuery): string {
  const p = new URLSearchParams();
  for (const [k, v] of Object.entries(q)) if (v !== undefined && v !== "") p.set(k, String(v));
  return `/search?${p.toString()}`;
}

/**
 * Renders server-provided home sections. Unknown section types are skipped,
 * so the server can ship new types before every client supports them.
 */
export function HomeSections({ sections }: { sections: HomeSection[] }) {
  return (
    <div className="space-y-8 md:space-y-10">
      {sections.map((s) => {
        switch (s.type) {
          case "HERO_BANNER":
            return (
              <section key={s.id} className="relative overflow-hidden rounded-3xl">
                <Art hue={s.hue} className="absolute inset-0" />
                <div className="relative max-w-xl px-6 py-8 text-white md:px-10 md:py-12">
                  <p className="mb-2 text-sm font-semibold uppercase tracking-wider text-white/80">Dine out, save more</p>
                  <h1 className="text-2xl font-black leading-tight md:text-4xl">{s.title}</h1>
                  <p className="mt-2 text-white/85 md:text-lg">{s.subtitle}</p>
                  <Link
                    href={s.cta.href}
                    className="mt-5 inline-flex h-11 items-center gap-2 rounded-xl bg-white px-4 text-sm font-bold text-brand-strong hover:bg-brand-soft"
                  >
                    {s.cta.label}
                    <ArrowRight className="h-4 w-4" aria-hidden />
                  </Link>
                </div>
              </section>
            );
          case "COLLECTIONS":
            return (
              <section key={s.id}>
                <SectionTitle title={s.title} />
                <div className="no-scrollbar -mx-4 flex gap-3 overflow-x-auto px-4 pb-1 md:mx-0 md:grid md:grid-cols-4 md:overflow-visible md:px-0 lg:grid-cols-8">
                  {s.items.map((c) => (
                    <Link
                      key={c.id}
                      href={queryToHref(c.query)}
                      className="group relative block h-28 w-32 shrink-0 overflow-hidden rounded-2xl md:w-auto"
                    >
                      <Art hue={c.hue} className="absolute inset-0 transition-transform group-hover:scale-105" />
                      <div className="absolute inset-0 bg-gradient-to-t from-black/60 to-transparent" />
                      <div className="absolute inset-x-0 bottom-0 p-2.5 text-white">
                        <p className="text-sm font-bold leading-tight">{c.title}</p>
                        <p className="text-xs text-white/80">{c.count} {c.count === 1 ? "place" : "places"}</p>
                      </div>
                    </Link>
                  ))}
                </div>
              </section>
            );
          case "RESTAURANT_CAROUSEL":
            return (
              <section key={s.id}>
                <SectionTitle title={s.title} subtitle={s.subtitle} />
                <div className="no-scrollbar -mx-4 flex snap-x gap-3 overflow-x-auto px-4 pb-1 md:mx-0 md:grid md:grid-cols-3 md:overflow-visible md:px-0 lg:grid-cols-4">
                  {s.restaurants.map((r) => (
                    <RestaurantCard key={r.id} r={r} className="w-64 shrink-0 snap-start md:w-auto" />
                  ))}
                </div>
              </section>
            );
          case "RESTAURANT_LIST":
            return (
              <section key={s.id}>
                <SectionTitle title={s.title} subtitle={`${s.restaurants.length} places`} />
                <div className="grid gap-3 md:grid-cols-2">
                  {s.restaurants.map((r) => (
                    <RestaurantRow key={r.id} r={r} />
                  ))}
                </div>
              </section>
            );
          default:
            return null;
        }
      })}
    </div>
  );
}
