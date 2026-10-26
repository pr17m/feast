"use client";

import { useState } from "react";
import { Eye, Plus, X } from "lucide-react";
import Link from "next/link";
import { PageHeader, Toggle } from "@/components/portal-shell";
import { useToast } from "@/components/providers";
import { Button, Card, Chip } from "@/components/ui";
import { useMutation, useQuery } from "@/lib/hooks";
import { LISTING_LIMITS } from "@/lib/api/listing";
import { costLimits, priceBandFor } from "@/lib/api/market";
import { currencySymbol, money, priceBand, toMajor, toMinor } from "@/lib/format";
import { useMarkets } from "@/components/providers";
import type { Facet, ListingPatch } from "@/lib/api/types";
import { usePartner } from "../partner-context";

const field = "h-11 w-full rounded-xl bg-canvas px-3 ring-1 ring-line outline-none focus:ring-2 focus:ring-brand";

/** Chip list with add/remove, plus suggestions other restaurants already use (keeps filters consistent). */
function LabelEditor({
  label,
  hint,
  values,
  max,
  suggestions,
  onChange,
}: {
  label: string;
  hint: string;
  values: string[];
  max: number;
  suggestions: Facet[];
  onChange: (v: string[]) => void;
}) {
  const [text, setText] = useState("");
  const has = (v: string) => values.some((x) => x.toLowerCase() === v.toLowerCase());
  const add = (v: string) => {
    const t = v.replace(/\s+/g, " ").trim();
    if (t && !has(t) && values.length < max) onChange([...values, t]);
    setText("");
  };
  const ideas = suggestions.filter((s) => !has(s.value)).slice(0, 12);
  const full = values.length >= max;
  return (
    <div>
      <div className="mb-1 flex items-baseline justify-between gap-3">
        <h3 className="font-semibold">{label}</h3>
        <span className="text-xs text-muted">
          {values.length}/{max}
        </span>
      </div>
      <p className="mb-3 text-sm text-muted">{hint}</p>
      <ul className="mb-3 flex flex-wrap gap-2" aria-label={label}>
        {values.map((v) => (
          <li key={v}>
            <span className="inline-flex items-center gap-1 rounded-full bg-ink py-1 pl-3 pr-1.5 text-sm font-medium text-white">
              {v}
              <button
                type="button"
                onClick={() => onChange(values.filter((x) => x !== v))}
                className="grid h-5 w-5 place-items-center rounded-full hover:bg-white/20"
                aria-label={`Remove ${v}`}
              >
                <X className="h-3.5 w-3.5" aria-hidden />
              </button>
            </span>
          </li>
        ))}
        {!values.length && <li className="text-sm text-faint">None yet</li>}
      </ul>
      <form
        className="flex gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          add(text);
        }}
      >
        <input
          value={text}
          onChange={(e) => setText(e.target.value)}
          maxLength={LISTING_LIMITS.label}
          disabled={full}
          placeholder={full ? `Up to ${max}` : `Add ${label.toLowerCase()}`}
          className={field}
          aria-label={`Add ${label.toLowerCase()}`}
        />
        <Button type="submit" variant="secondary" disabled={full || !text.trim()} aria-label={`Add ${label.toLowerCase()}`}>
          <Plus className="h-4 w-4" aria-hidden />
        </Button>
      </form>
      {ideas.length > 0 && !full && (
        <div className="mt-3">
          <p className="mb-1.5 text-xs font-semibold uppercase tracking-wide text-faint">Used by other restaurants</p>
          <div className="flex flex-wrap gap-1.5">
            {ideas.map((s) => (
              <Chip key={s.value} onClick={() => add(s.value)} className="!py-1 text-xs">
                <Plus className="h-3 w-3" aria-hidden /> {s.label}
              </Chip>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

export default function PartnerListing() {
  const { restaurant: r, reloadRestaurants } = usePartner();
  const toast = useToast();
  const [form, setForm] = useState({
    cuisines: r.cuisines,
    tags: r.tags,
    isPureVeg: r.isPureVeg,
    cost: toMajor(r.costForTwo, r.currency),
    about: r.about,
  });
  // City-wide options so owners reuse the same spellings diners filter by.
  const facets = useQuery(`facets:${r.cityId}`, (api) => api.search(r.cityId, {}));
  const save = useMutation((api, patch: ListingPatch) => api.updateListing(r.id, patch));

  const market = useMarkets().marketOf(r.countryCode);
  const limits = market ? costLimits(market) : { min: 1, max: Number.MAX_SAFE_INTEGER };
  const costPaise = toMinor(form.cost || 0, r.currency);
  const costOk = Number.isFinite(costPaise) && costPaise >= limits.min && costPaise <= limits.max;
  const dirty =
    form.cuisines.join("|") !== r.cuisines.join("|") ||
    form.tags.join("|") !== r.tags.join("|") ||
    form.isPureVeg !== r.isPureVeg ||
    costPaise !== r.costForTwo ||
    form.about !== r.about;

  const submit = async () => {
    const res = await save.run({ cuisines: form.cuisines, tags: form.tags, isPureVeg: form.isPureVeg, costForTwo: costPaise, about: form.about });
    if (res) {
      setForm({ cuisines: res.cuisines, tags: res.tags, isPureVeg: res.isPureVeg, cost: toMajor(res.costForTwo, res.currency), about: res.about });
      reloadRestaurants();
      toast("Listing updated — diners see it now");
    }
  };

  return (
    <div className="max-w-2xl space-y-5 pb-24">
      <PageHeader
        title="Listing"
        subtitle="How diners find you. Search filters on Feast are built from what restaurants list here."
        actions={
          <Link href={`/restaurants/${r.slug}`} className="inline-flex h-10 items-center gap-2 rounded-xl px-3 text-sm font-semibold text-muted ring-1 ring-line hover:text-ink">
            <Eye className="h-4 w-4" aria-hidden /> View as customer
          </Link>
        }
      />

      <Card className="space-y-6">
        <LabelEditor
          label="Cuisines"
          hint="Diners filter by cuisine. List what you're known for, most important first."
          values={form.cuisines}
          max={LISTING_LIMITS.cuisines}
          suggestions={facets.data?.facets.cuisines ?? []}
          onChange={(cuisines) => setForm({ ...form, cuisines })}
        />
        <LabelEditor
          label="Experiences"
          hint="Shown as the “Experience” filter — e.g. Rooftop, Live music, Pet friendly."
          values={form.tags}
          max={LISTING_LIMITS.tags}
          suggestions={facets.data?.facets.tags ?? []}
          onChange={(tags) => setForm({ ...form, tags })}
        />
      </Card>

      <Card className="space-y-4">
        <label className="block">
          <span className="mb-1 block font-semibold">Average cost for two ({currencySymbol(r.currency)})</span>
          <span className="mb-2 block text-sm text-muted">Sets your price band in the “Cost for two” filter.</span>
          <input inputMode="numeric" value={form.cost} onChange={(e) => setForm({ ...form, cost: e.target.value.replace(/[^\d.]/g, "") })} className={field} aria-invalid={!costOk} />
          {costOk ? (
            <span className="mt-1.5 block text-sm text-muted">
              Price band <b className="text-ink">{market ? priceBand(priceBandFor(costPaise, market), r.currency) : "—"}</b> · {money(costPaise, r.currency)} for two
            </span>
          ) : (
            <span className="mt-1.5 block text-sm text-danger">
              Enter an amount between {money(limits.min, r.currency)} and {money(limits.max, r.currency)}
            </span>
          )}
        </label>
        <Toggle checked={form.isPureVeg} onChange={(isPureVeg) => setForm({ ...form, isPureVeg })} label="Pure vegetarian" description="Appear under the Pure veg filter." />
        <label className="block">
          <span className="mb-2 block font-semibold">About</span>
          <textarea
            value={form.about}
            maxLength={LISTING_LIMITS.about}
            onChange={(e) => setForm({ ...form, about: e.target.value })}
            rows={4}
            className="w-full rounded-xl bg-canvas p-3 ring-1 ring-line outline-none focus:ring-2 focus:ring-brand"
          />
        </label>
      </Card>

      <p className="text-sm text-muted">
        Offers and services also appear as filters — manage them under <Link href="/partner/offers" className="font-semibold text-brand">Offers</Link> and{" "}
        <Link href="/partner/settings" className="font-semibold text-brand">Settings</Link>.
      </p>

      {save.error && <p className="text-sm text-danger">{save.error.message}</p>}
      <div className="fixed inset-x-0 bottom-0 z-30 border-t border-line bg-surface/95 px-4 py-3 backdrop-blur md:left-64 md:px-8">
        <div className="flex max-w-2xl items-center justify-end gap-3">
          {dirty && <span className="mr-auto text-sm text-muted">Unsaved changes</span>}
          <Button disabled={!dirty || !costOk || !form.cuisines.length} loading={save.pending} onClick={submit}>
            Save listing
          </Button>
        </div>
      </div>
    </div>
  );
}
