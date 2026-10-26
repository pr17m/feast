"use client";

import Link from "next/link";
import { Suspense, useState } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import clsx from "clsx";
import { ExternalLink, Pencil, Plus, Search, Settings2 } from "lucide-react";
import { Art } from "@/components/art";
import { PageHeader } from "@/components/portal-shell";
import { useMarkets, useToast } from "@/components/providers";
import { Button, Chip, Empty, ErrorState, Rating, Sheet, Skeleton } from "@/components/ui";
import { useMutation, useQuery } from "@/lib/hooks";
import { currencySymbol, money, priceBand, toMajor, toMinor } from "@/lib/format";
import { priceBandFor } from "@/lib/api/market";
import type { CreateRestaurantRequest, RestaurantDetail, RestaurantStatus } from "@/lib/api/types";

const STATUS: RestaurantStatus[] = ["ONBOARDING", "PENDING_APPROVAL", "LIVE", "PAUSED", "SUSPENDED"];
const STATUS_STYLE: Record<RestaurantStatus, string> = {
  ONBOARDING: "bg-stone-100 text-stone-700",
  LIVE: "bg-green-50 text-veg",
  PAUSED: "bg-amber-50 text-amber-800",
  PENDING_APPROVAL: "bg-sky-50 text-sky-800",
  SUSPENDED: "bg-red-50 text-danger",
};
const field = "h-10 w-full rounded-lg bg-canvas px-3 text-sm ring-1 ring-line outline-none focus:ring-2 focus:ring-brand";

function StatusActions({ r, onSet, busy }: { r: RestaurantDetail; onSet: (s: RestaurantStatus) => void; busy: boolean }) {
  const btn = (s: RestaurantStatus, label: string, danger?: boolean) => (
    <Button key={s} size="sm" variant={danger ? "ghost" : "secondary"} className={danger ? "text-danger hover:bg-red-50" : ""} disabled={busy} onClick={() => onSet(s)}>
      {label}
    </Button>
  );
  if (r.status === "PENDING_APPROVAL" || r.status === "ONBOARDING")
    return (
      <Link href={`/admin/approvals?r=${r.id}`} className="inline-flex h-8 items-center rounded-lg px-3 text-sm font-semibold text-brand ring-1 ring-line hover:bg-canvas">
        Review documents
      </Link>
    );
  if (r.status === "SUSPENDED") return btn("LIVE", "Reinstate");
  if (r.status === "PAUSED") return <>{[btn("LIVE", "Set live"), btn("SUSPENDED", "Suspend", true)]}</>;
  return <>{[btn("PAUSED", "Pause"), btn("SUSPENDED", "Suspend", true)]}</>;
}

function EditSheet({ r, onClose, onSaved }: { r: RestaurantDetail | null; onClose: () => void; onSaved: () => void }) {
  const [form, setForm] = useState(() =>
    r ? { name: r.name, area: r.area, cuisines: r.cuisines.join(", "), tags: r.tags.join(", "), costForTwo: toMajor(r.costForTwo, r.currency), about: r.about, phone: r.phone } : null,
  );
  const save = useMutation((api, id: string, patch: Parameters<typeof api.updateRestaurant>[1]) => api.updateRestaurant(id, patch));
  if (!r || !form) return null;
  const set = (k: keyof typeof form, v: string) => setForm({ ...form, [k]: v });
  return (
    <Sheet
      open
      onClose={onClose}
      title={`Edit ${r.name}`}
      footer={
        <Button
          className="w-full"
          loading={save.pending}
          onClick={async () => {
            const res = await save.run(r.id, {
              name: form.name,
              area: form.area,
              cuisines: form.cuisines.split(",").map((x) => x.trim()).filter(Boolean),
              tags: form.tags.split(",").map((x) => x.trim()).filter(Boolean),
              costForTwo: toMinor(form.costForTwo, r.currency),
              about: form.about,
              phone: form.phone,
            });
            if (res) onSaved();
          }}
        >
          Save
        </Button>
      }
    >
      <div className="space-y-3">
        {(
          [
            ["name", "Name"],
            ["area", "Area"],
            ["cuisines", "Cuisines (comma separated)"],
            ["tags", "Tags (comma separated)"],
            ["costForTwo", `Cost for two (${currencySymbol(r.currency)})`],
            ["phone", "Phone"],
          ] as const
        ).map(([k, label]) => (
          <label key={k} className="block">
            <span className="mb-1 block text-xs font-semibold text-muted">{label}</span>
            <input value={form[k]} onChange={(e) => set(k, e.target.value)} className={field} />
          </label>
        ))}
        <label className="block">
          <span className="mb-1 block text-xs font-semibold text-muted">About</span>
          <textarea value={form.about} onChange={(e) => set("about", e.target.value)} rows={3} className="w-full rounded-lg bg-canvas p-3 text-sm ring-1 ring-line outline-none focus:ring-2 focus:ring-brand" />
        </label>
        {save.error && <p className="text-sm text-danger">{save.error.message}</p>}
      </div>
    </Sheet>
  );
}

function CreateSheet({ open, onClose, onCreated }: { open: boolean; onClose: () => void; onCreated: () => void }) {
  const cities = useQuery("cities", (api) => api.listCities());
  const { marketOf } = useMarkets();
  const users = useQuery(open ? "adminUsers" : null, (api) => api.adminUsers());
  const [f, setF] = useState({ name: "", cityId: "", area: "", cuisines: "", costForTwo: "", isPureVeg: false, tags: "", about: "", ownerUserId: "" });
  const create = useMutation((api, req: CreateRestaurantRequest) => api.createRestaurant(req));
  const cityId = f.cityId || cities.data?.[0]?.id || "";
  const city = cities.data?.find((c) => c.id === cityId);
  const market = marketOf(city?.countryCode);
  const currency = market?.currency ?? "INR";
  const cost = f.costForTwo ? toMinor(f.costForTwo, currency) : 0;
  return (
    <Sheet
      open={open}
      onClose={onClose}
      title="Onboard a restaurant"
      footer={
        <Button
          className="w-full"
          loading={create.pending}
          onClick={async () => {
            const res = await create.run({
              name: f.name,
              cityId,
              area: f.area,
              cuisines: f.cuisines.split(",").map((x) => x.trim()).filter(Boolean),
              costForTwo: cost,
              isPureVeg: f.isPureVeg,
              tags: f.tags.split(",").map((x) => x.trim()).filter(Boolean),
              about: f.about,
              ownerUserId: f.ownerUserId || undefined,
            });
            if (res) onCreated();
          }}
        >
          Create (owner uploads documents next)
        </Button>
      }
    >
      <div className="space-y-3">
        <label className="block">
          <span className="mb-1 block text-xs font-semibold text-muted">Restaurant name</span>
          <input value={f.name} onChange={(e) => setF({ ...f, name: e.target.value })} className={field} autoFocus />
        </label>
        <div className="grid grid-cols-2 gap-3">
          <label className="block">
            <span className="mb-1 block text-xs font-semibold text-muted">City</span>
            <select value={cityId} onChange={(e) => setF({ ...f, cityId: e.target.value, area: "" })} className={field}>
              {cities.data?.map((c) => <option key={c.id} value={c.id}>{c.name} · {c.countryCode}</option>)}
            </select>
          </label>
          <label className="block">
            <span className="mb-1 block text-xs font-semibold text-muted">Area</span>
            <select value={f.area} onChange={(e) => setF({ ...f, area: e.target.value })} className={field}>
              <option value="">Select…</option>
              {city?.areas.map((a) => <option key={a}>{a}</option>)}
            </select>
          </label>
        </div>
        <label className="block">
          <span className="mb-1 block text-xs font-semibold text-muted">Cuisines (comma separated)</span>
          <input value={f.cuisines} onChange={(e) => setF({ ...f, cuisines: e.target.value })} placeholder="North Indian, Kebabs" className={field} />
        </label>
        <label className="block">
          <span className="mb-1 block text-xs font-semibold text-muted">Cost for two ({currencySymbol(currency)})</span>
          <input value={f.costForTwo} onChange={(e) => setF({ ...f, costForTwo: e.target.value.replace(/[^\d.]/g, "") })} inputMode="decimal" className={field} />
          {market && cost > 0 && (
            <span className="mt-1 block text-xs text-muted">
              Price band {priceBand(priceBandFor(cost, market), currency)} · {money(cost, currency)} for two
              {priceBandFor(cost, market) === 4 && market.coverChargePerPair > 0 ? ` · cover charge ${money(market.coverChargePerPair, currency)} per 2 guests` : ""}
            </span>
          )}
        </label>
        <label className="block">
          <span className="mb-1 block text-xs font-semibold text-muted">Tags (comma separated)</span>
          <input value={f.tags} onChange={(e) => setF({ ...f, tags: e.target.value })} placeholder="Rooftop, Date night" className={field} />
        </label>
        <label className="flex items-center gap-2 text-sm">
          <input type="checkbox" checked={f.isPureVeg} onChange={(e) => setF({ ...f, isPureVeg: e.target.checked })} className="accent-[var(--color-brand)]" /> Pure veg
        </label>
        <label className="block">
          <span className="mb-1 block text-xs font-semibold text-muted">Owner account (optional)</span>
          <select value={f.ownerUserId} onChange={(e) => setF({ ...f, ownerUserId: e.target.value })} className={field}>
            <option value="">Assign later</option>
            {users.data?.filter((u) => u.role !== "ADMIN" && u.email).map((u) => (
              <option key={u.id} value={u.id}>{u.name} · {u.email}</option>
            ))}
          </select>
        </label>
        <label className="block">
          <span className="mb-1 block text-xs font-semibold text-muted">About</span>
          <textarea value={f.about} onChange={(e) => setF({ ...f, about: e.target.value })} rows={2} className="w-full rounded-lg bg-canvas p-3 text-sm ring-1 ring-line outline-none focus:ring-2 focus:ring-brand" />
        </label>
        {create.error && <p role="alert" className="text-sm text-danger">{create.error.message}</p>}
      </div>
    </Sheet>
  );
}

function AdminRestaurantsInner() {
  const sp = useSearchParams();
  const router = useRouter();
  const toast = useToast();
  const filter = (sp.get("status") as RestaurantStatus | null) ?? null;
  const [q, setQ] = useState("");
  const [editing, setEditing] = useState<RestaurantDetail | null>(null);
  const [creating, setCreating] = useState(false);
  const list = useQuery("adminRestaurants", (api) => api.adminRestaurants());
  const setStatus = useMutation((api, id: string, s: RestaurantStatus) => api.setRestaurantStatus(id, s));
  const cities = useQuery("cities", (api) => api.listCities());
  const cityName = (id: string) => cities.data?.find((c) => c.id === id)?.name ?? "";

  const onSet = async (r: RestaurantDetail, s: RestaurantStatus) => {
    const res = await setStatus.run(r.id, s);
    if (res) {
      toast(`${r.name} is now ${s.replace("_", " ").toLowerCase()}`);
      list.reload();
    } else toast("Couldn't change status", "error");
  };

  if (list.error) return <ErrorState error={list.error} onRetry={list.reload} />;
  if (!list.data) return <Skeleton className="h-96 w-full" />;
  const term = q.trim().toLowerCase();
  const rows = list.data.filter((r) => (!filter || r.status === filter) && (!term || `${r.name} ${r.area} ${r.cuisines.join(" ")}`.toLowerCase().includes(term)));

  return (
    <div className="space-y-5">
      <PageHeader
        title="Restaurants"
        subtitle={`${list.data.length} on the platform`}
        actions={
          <Button onClick={() => setCreating(true)}>
            <Plus className="h-4 w-4" aria-hidden /> Onboard restaurant
          </Button>
        }
      />
      <div className="flex flex-col gap-3 md:flex-row md:items-center">
        <label className="flex h-10 items-center gap-2 rounded-xl bg-surface px-3 ring-1 ring-line focus-within:ring-2 focus-within:ring-brand md:w-72">
          <Search className="h-4 w-4 text-muted" aria-hidden />
          <input value={q} onChange={(e) => setQ(e.target.value)} placeholder="Search name, area, cuisine" className="w-full bg-transparent text-sm outline-none" aria-label="Search restaurants" />
        </label>
        <div className="no-scrollbar -mx-4 flex gap-2 overflow-x-auto px-4 py-1 md:mx-0 md:px-0">
          <Chip active={!filter} onClick={() => router.replace("/admin/restaurants")}>All</Chip>
          {STATUS.map((s) => (
            <Chip key={s} active={filter === s} onClick={() => router.replace(`/admin/restaurants?status=${s}`)}>
              {s.replace("_", " ").toLowerCase()} ({list.data!.filter((r) => r.status === s).length})
            </Chip>
          ))}
        </div>
      </div>

      {rows.length === 0 && <Empty title="No restaurants match" />}

      {rows.length > 0 && (
        <>
          <div className="hidden overflow-x-auto rounded-2xl bg-surface ring-1 ring-line lg:block">
            <table className="w-full text-left text-sm">
              <thead className="bg-canvas text-xs uppercase tracking-wide text-muted">
                <tr>
                  <th className="px-4 py-3">Restaurant</th>
                  <th className="px-4 py-3">City</th>
                  <th className="px-4 py-3">Rating</th>
                  <th className="whitespace-nowrap px-4 py-3">Cost for two</th>
                  <th className="hidden px-4 py-3 2xl:table-cell">Services</th>
                  <th className="px-4 py-3">Status</th>
                  <th className="px-4 py-3">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-line">
                {rows.map((r) => (
                  <tr key={r.id}>
                    <td className="px-4 py-3">
                      <div className="flex items-center gap-3">
                        <Art hue={r.art.hue} motif={r.art.motif} image={r.art.image} credit={r.art.imageCredit} className="h-10 w-10 shrink-0 rounded-lg" iconClassName="h-7 w-7" />
                        <div>
                          <p className="font-semibold">{r.name}</p>
                          <p className="text-xs text-muted">{r.cuisines.join(", ")}</p>
                        </div>
                      </div>
                    </td>
                    <td className="px-4 py-3">
                      {r.area}
                      <span className="block text-xs text-muted">{cityName(r.cityId)}</span>
                    </td>
                    <td className="px-4 py-3">{r.ratingCount ? <Rating value={r.rating} /> : <span className="text-muted">New</span>}</td>
                    <td className="px-4 py-3">{money(r.costForTwo, r.currency)}</td>
                    <td className="hidden whitespace-nowrap px-4 py-3 text-xs text-muted 2xl:table-cell">
                      {[r.settings.bookingsEnabled && "Book", r.settings.billPaymentEnabled && "Pay"].filter(Boolean).join(" · ") || "—"}
                    </td>
                    <td className="px-4 py-3">
                      <span className={clsx("rounded-md px-2 py-0.5 text-xs font-bold", STATUS_STYLE[r.status])}>{r.status.replace("_", " ")}</span>
                    </td>
                    <td className="px-4 py-3">
                      <div className="flex items-center gap-1 whitespace-nowrap">
                        <StatusActions r={r} busy={setStatus.pending} onSet={(s) => onSet(r, s)} />
                        <button onClick={() => setEditing(r)} className="grid h-9 w-9 place-items-center rounded-lg text-muted hover:bg-canvas" aria-label={`Edit ${r.name}`}>
                          <Pencil className="h-4 w-4" />
                        </button>
                        <Link href={`/partner?r=${r.id}`} className="grid h-9 w-9 place-items-center rounded-lg text-muted hover:bg-canvas" aria-label={`Manage ${r.name} as owner`} title="Open partner portal">
                          <Settings2 className="h-4 w-4" />
                        </Link>
                        {(r.status === "LIVE" || r.status === "PAUSED") && (
                          <Link href={`/restaurants/${r.slug}`} className="grid h-9 w-9 place-items-center rounded-lg text-muted hover:bg-canvas" aria-label={`View ${r.name} as diner`} title="View public page">
                            <ExternalLink className="h-4 w-4" />
                          </Link>
                        )}
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>

          <ul className="space-y-3 lg:hidden">
            {rows.map((r) => (
              <li key={r.id} className="rounded-2xl bg-surface p-4 ring-1 ring-line">
                <div className="mb-3 flex items-start gap-3">
                  <Art hue={r.art.hue} motif={r.art.motif} image={r.art.image} credit={r.art.imageCredit} className="h-12 w-12 shrink-0 rounded-lg" iconClassName="h-8 w-8" />
                  <div className="min-w-0 flex-1">
                    <p className="font-bold">{r.name}</p>
                    <p className="text-xs text-muted">{cityName(r.cityId)} · {r.area} · {money(r.costForTwo, r.currency)} for two</p>
                  </div>
                  <span className={clsx("shrink-0 rounded-md px-2 py-0.5 text-xs font-bold", STATUS_STYLE[r.status])}>{r.status.replace("_", " ")}</span>
                </div>
                <div className="flex flex-wrap items-center gap-2">
                  <StatusActions r={r} busy={setStatus.pending} onSet={(s) => onSet(r, s)} />
                  <Button size="sm" variant="secondary" onClick={() => setEditing(r)}>Edit</Button>
                  <Link href={`/partner?r=${r.id}`} className="inline-flex h-9 items-center rounded-xl px-3 text-sm font-semibold text-brand hover:bg-brand-soft">Manage</Link>
                </div>
              </li>
            ))}
          </ul>
        </>
      )}

      <EditSheet
        key={editing?.id ?? "none"}
        r={editing}
        onClose={() => setEditing(null)}
        onSaved={() => {
          setEditing(null);
          toast("Restaurant updated");
          list.reload();
        }}
      />
      <CreateSheet
        open={creating}
        onClose={() => setCreating(false)}
        onCreated={() => {
          setCreating(false);
          toast("Restaurant created — approve it to go live");
          router.replace("/admin/restaurants?status=PENDING_APPROVAL");
          list.reload();
        }}
      />
    </div>
  );
}

export default function AdminRestaurants() {
  return (
    <Suspense fallback={<Skeleton className="h-96 w-full" />}>
      <AdminRestaurantsInner />
    </Suspense>
  );
}
