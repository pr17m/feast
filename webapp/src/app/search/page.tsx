"use client";

import { Suspense, useState } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import { Search, SlidersHorizontal, X } from "lucide-react";
import { useCity } from "@/components/providers";
import { RestaurantRow } from "@/components/restaurant-card";
import { Chip, Empty, ErrorState, Skeleton, Sheet, Button } from "@/components/ui";
import { useQuery } from "@/lib/hooks";
import type { Facet, OfferScope, PriceBand, SearchFacets, SearchQuery, SearchService } from "@/lib/api/types";
import { queryToHref } from "@/components/home-sections";

const SORTS: { id: NonNullable<SearchQuery["sort"]>; label: string }[] = [
  { id: "relevance", label: "Best deals" },
  { id: "rating", label: "Top rated" },
  { id: "distance", label: "Nearest" },
  { id: "costLow", label: "Cost: low to high" },
  { id: "costHigh", label: "Cost: high to low" },
];

function readQuery(sp: URLSearchParams): SearchQuery {
  const str = (k: string) => sp.get(k) || undefined;
  const band = Number(sp.get("priceBand"));
  return {
    q: str("q"),
    cuisine: str("cuisine"),
    tag: str("tag"),
    area: str("area"),
    priceBand: band >= 1 && band <= 4 ? (band as PriceBand) : undefined,
    offer: str("offer") as OfferScope | undefined,
    service: str("service") as SearchService | undefined,
    pureVeg: sp.get("pureVeg") === "true" || undefined,
    minRating: sp.get("minRating") ? Number(sp.get("minRating")) : undefined,
    sort: (sp.get("sort") as SearchQuery["sort"]) ?? undefined,
  };
}

/** Filter groups in the sheet. Options come from the server's facets (i.e. from what restaurants list). */
const GROUPS: { key: "cuisine" | "tag" | "area" | "priceBand" | "offer" | "service"; facet: Exclude<keyof SearchFacets, "pureVeg">; title: string }[] = [
  { key: "cuisine", facet: "cuisines", title: "Cuisine" },
  { key: "tag", facet: "tags", title: "Experience" },
  { key: "offer", facet: "offers", title: "Offers" },
  { key: "service", facet: "services", title: "On Feast" },
  { key: "priceBand", facet: "priceBands", title: "Cost for two" },
  { key: "area", facet: "areas", title: "Area" },
];

type GroupKey = (typeof GROUPS)[number]["key"];
const FILTER_KEYS: (GroupKey | "pureVeg" | "minRating")[] = ["cuisine", "tag", "area", "priceBand", "offer", "service", "pureVeg", "minRating"];

function FacetChips({ options, selected, onPick }: { options: Facet[]; selected?: string; onPick: (v: string | undefined) => void }) {
  const shown = options.filter((o) => o.count > 0 || o.value === selected);
  if (!shown.length) return <p className="text-sm text-muted">Nothing to filter by here yet.</p>;
  return (
    <div className="flex flex-wrap gap-2">
      {shown.map((o) => (
        <Chip key={o.value} active={selected === o.value} onClick={() => onPick(selected === o.value ? undefined : o.value)}>
          {o.label}
          <span className={selected === o.value ? "text-white/70" : "text-faint"}>{o.count}</span>
        </Chip>
      ))}
    </div>
  );
}

function SearchBox({ initial, onSearch }: { initial: string; onSearch: (q: string | undefined) => void }) {
  const [text, setText] = useState(initial);
  return (
    <form
      onSubmit={(e) => {
        e.preventDefault();
        onSearch(text.trim() || undefined);
      }}
      role="search"
    >
      <label className="flex h-12 items-center gap-2 rounded-xl bg-surface px-4 ring-1 ring-line focus-within:ring-2 focus-within:ring-brand">
        <Search className="h-5 w-5 text-muted" aria-hidden />
        <input
          autoFocus={!initial}
          value={text}
          onChange={(e) => setText(e.target.value)}
          placeholder="Search restaurants, cuisines, dishes"
          className="w-full bg-transparent outline-none"
          aria-label="Search"
          enterKeyHint="search"
        />
        {text && (
          <button
            type="button"
            onClick={() => {
              setText("");
              onSearch(undefined);
            }}
            aria-label="Clear search"
          >
            <X className="h-4 w-4 text-muted" />
          </button>
        )}
      </label>
    </form>
  );
}

function SearchInner() {
  const sp = useSearchParams();
  const router = useRouter();
  const { cityId, ready } = useCity();
  const query = readQuery(sp);
  const [filtersOpen, setFiltersOpen] = useState(false);

  const key = ready ? `search:${cityId}:${sp.toString()}` : null;
  const res = useQuery(key, (api) => api.search(cityId, query));

  const update = (patch: Partial<SearchQuery>) => router.replace(queryToHref({ ...query, ...patch }), { scroll: false });
  const activeFilters = FILTER_KEYS.filter((k) => query[k] !== undefined).length;
  const facets = res.data?.facets;
  /** Label for an active filter pill, from the facet that produced it. */
  const pillLabel = (k: GroupKey) => {
    const g = GROUPS.find((x) => x.key === k)!;
    const v = String(query[k]);
    return facets?.[g.facet].find((o) => o.value === v)?.label ?? v;
  };
  const activeGroups = GROUPS.filter((g) => query[g.key] !== undefined);

  return (
    <div className="space-y-4">
      <SearchBox key={query.q ?? ""} initial={query.q ?? ""} onSearch={(q) => update({ q })} />

      <div className="no-scrollbar -mx-4 flex gap-2 overflow-x-auto px-4 py-1">
        <Chip active={activeFilters > 0} onClick={() => setFiltersOpen(true)}>
          <SlidersHorizontal className="h-4 w-4" aria-hidden />
          Filters{activeFilters > 0 && ` (${activeFilters})`}
        </Chip>
        {(query.pureVeg || (facets?.pureVeg ?? 0) > 0) && (
          <Chip active={query.pureVeg} onClick={() => update({ pureVeg: query.pureVeg ? undefined : true })}>
            Pure veg
          </Chip>
        )}
        {facets?.offers
          .filter((o) => o.value === "PREBOOK" && (o.count > 0 || query.offer === o.value))
          .map((o) => (
            <Chip key={o.value} active={query.offer === o.value} onClick={() => update({ offer: query.offer === o.value ? undefined : (o.value as OfferScope) })}>
              {o.label}
            </Chip>
          ))}
        <Chip active={query.minRating === 4.5} onClick={() => update({ minRating: query.minRating === 4.5 ? undefined : 4.5 })}>
          Rating 4.5+
        </Chip>
        {SORTS.map((s) => (
          <Chip key={s.id} active={(query.sort ?? "relevance") === s.id} onClick={() => update({ sort: s.id })}>
            {s.label}
          </Chip>
        ))}
      </div>

      {activeGroups.length > 0 && (
        <div className="flex flex-wrap gap-2">
          {activeGroups.map((g) => (
            <button
              key={g.key}
              onClick={() => update({ [g.key]: undefined })}
              className="inline-flex items-center gap-1 rounded-full bg-ink px-3 py-1 text-sm font-medium text-white"
            >
              {pillLabel(g.key)} <X className="h-3.5 w-3.5" aria-label={`Remove ${g.title.toLowerCase()} filter`} />
            </button>
          ))}
        </div>
      )}

      {res.error && <ErrorState error={res.error} onRetry={res.reload} />}
      {res.loading && !res.data && (
        <div className="grid gap-3 md:grid-cols-2">
          {Array.from({ length: 4 }, (_, i) => (
            <Skeleton key={i} className="h-36" />
          ))}
        </div>
      )}
      {res.data && (
        <>
          <p className="text-sm text-muted" aria-live="polite">
            {res.data.total} {res.data.total === 1 ? "restaurant" : "restaurants"}
            {query.q ? ` for “${query.q}”` : ""}
          </p>
          {res.data.total === 0 ? (
            <Empty
              title="No restaurants match"
              body="Try a different dish or cuisine, or clear some filters."
              action={<Button variant="secondary" onClick={() => router.replace("/search")}>Clear all</Button>}
            />
          ) : (
            <div className={`grid gap-3 md:grid-cols-2 ${res.loading ? "opacity-60" : ""}`}>
              {res.data.restaurants.map((r) => (
                <RestaurantRow key={r.id} r={r} />
              ))}
            </div>
          )}

          <Sheet
            open={filtersOpen}
            onClose={() => setFiltersOpen(false)}
            title="Filters"
            footer={
              <div className="flex gap-3">
                <Button
                  variant="secondary"
                  className="flex-1"
                  onClick={() => update(Object.fromEntries(FILTER_KEYS.map((k) => [k, undefined])) as Partial<SearchQuery>)}
                >
                  Clear
                </Button>
                <Button className="flex-1" onClick={() => setFiltersOpen(false)}>
                  Show {res.data.total} results
                </Button>
              </div>
            }
          >
            <p className="mb-4 text-sm text-muted">Options come from what restaurants in your city list. Numbers show how many places match.</p>
            <div className="space-y-5">
              {GROUPS.map((g) => (
                <section key={g.key} aria-label={g.title}>
                  <h3 className="mb-2 font-semibold">{g.title}</h3>
                  <FacetChips
                    options={res.data!.facets[g.facet]}
                    selected={query[g.key] !== undefined ? String(query[g.key]) : undefined}
                    onPick={(v) => update({ [g.key]: g.key === "priceBand" && v ? Number(v) : v } as Partial<SearchQuery>)}
                  />
                </section>
              ))}
            </div>
          </Sheet>
        </>
      )}
    </div>
  );
}

export default function SearchPage() {
  return (
    <Suspense fallback={<Skeleton className="h-12 w-full" />}>
      <SearchInner />
    </Suspense>
  );
}
