"use client";

import { useMemo, useState } from "react";
import clsx from "clsx";
import { Globe2, MapPin, Plus, Save, Trash2 } from "lucide-react";
import { PageHeader, Toggle } from "@/components/portal-shell";
import { useMarkets, useToast } from "@/components/providers";
import { Button, Card, Chip, ErrorState, Sheet, Skeleton } from "@/components/ui";
import { useMutation, useQuery } from "@/lib/hooks";
import { money } from "@/lib/format";
import type { City, CreateCityRequest, DocumentRequirement, Market, MarketInput, PaymentMethod, PlanId } from "@/lib/api/types";

const field = "h-10 w-full rounded-lg bg-canvas px-3 text-sm ring-1 ring-line outline-none focus:ring-2 focus:ring-brand";
const ALL_METHODS: PaymentMethod[] = ["UPI", "CARD", "NETBANKING", "WALLET", "APPLE_PAY", "GOOGLE_PAY"];
const PLANS: PlanId[] = ["MONTHLY", "QUARTERLY", "YEARLY"];

function F({ label, children, hint }: { label: string; children: React.ReactNode; hint?: string }) {
  return (
    <label className="block">
      <span className="mb-1 block text-xs font-semibold text-muted">{label}</span>
      {children}
      {hint && <span className="mt-1 block text-xs text-faint">{hint}</span>}
    </label>
  );
}

/** Major-unit text field bound to a minor-unit number. */
function MoneyInput({ value, onChange, currency, decimals }: { value: number; onChange: (v: number) => void; currency: string; decimals: number }) {
  const [text, setText] = useState(String(value / 10 ** decimals));
  return (
    <div className="flex items-center gap-1">
      <span className="text-xs text-muted">{currency}</span>
      <input
        value={text}
        inputMode="decimal"
        onChange={(e) => {
          const t = e.target.value.replace(/[^\d.]/g, "");
          setText(t);
          onChange(Math.round(Number(t || 0) * 10 ** decimals));
        }}
        className={field}
      />
    </div>
  );
}

function DocumentsEditor({ docs, onChange }: { docs: DocumentRequirement[]; onChange: (d: DocumentRequirement[]) => void }) {
  const set = (i: number, patch: Partial<DocumentRequirement>) => onChange(docs.map((d, j) => (j === i ? { ...d, ...patch } : d)));
  return (
    <div className="space-y-3">
      {docs.map((d, i) => (
        <div key={i} className="space-y-2 rounded-xl bg-canvas p-3">
          <div className="grid gap-2 md:grid-cols-[1fr_2fr_auto]">
            <F label="Key">
              <input value={d.id} onChange={(e) => set(i, { id: e.target.value.toLowerCase().replace(/[^a-z0-9_]/g, "_") })} className={field} />
            </F>
            <F label="Name">
              <input value={d.name} onChange={(e) => set(i, { name: e.target.value })} className={field} />
            </F>
            <Button size="sm" variant="ghost" className="self-end text-danger hover:bg-red-50" onClick={() => onChange(docs.filter((_, j) => j !== i))} aria-label={`Remove ${d.name || "document"}`}>
              <Trash2 className="h-4 w-4" aria-hidden />
            </Button>
          </div>
          <div className="grid gap-2 md:grid-cols-2">
            <F label="Description">
              <input value={d.description} onChange={(e) => set(i, { description: e.target.value })} className={field} />
            </F>
            <F label="Issued by">
              <input value={d.issuer ?? ""} onChange={(e) => set(i, { issuer: e.target.value || undefined })} className={field} />
            </F>
            <F label="Number label (empty = no number)">
              <input value={d.numberLabel ?? ""} onChange={(e) => set(i, { numberLabel: e.target.value || undefined })} className={field} />
            </F>
            <div className="flex items-end gap-4 pb-2 text-sm">
              <label className="flex items-center gap-2">
                <input type="checkbox" checked={d.required} onChange={(e) => set(i, { required: e.target.checked })} className="accent-[var(--color-brand)]" /> Required
              </label>
              <label className="flex items-center gap-2">
                <input type="checkbox" checked={d.hasExpiry} onChange={(e) => set(i, { hasExpiry: e.target.checked })} className="accent-[var(--color-brand)]" /> Has expiry
              </label>
            </div>
          </div>
        </div>
      ))}
      <Button size="sm" variant="secondary" onClick={() => onChange([...docs, { id: `doc_${docs.length + 1}`, name: "", description: "", required: true, hasExpiry: false }])}>
        <Plus className="h-4 w-4" aria-hidden /> Add document
      </Button>
    </div>
  );
}

function CitySheet({ open, countryCode, onClose, onDone }: { open: boolean; countryCode: string; onClose: () => void; onDone: () => void }) {
  const [f, setF] = useState({ name: "", timezone: "", areas: "", lat: "", lng: "", tax: "" });
  const create = useMutation((api, req: CreateCityRequest) => api.createCity(req));
  return (
    <Sheet
      open={open}
      onClose={onClose}
      title="Add a city"
      footer={
        <Button
          className="w-full"
          loading={create.pending}
          disabled={!f.name || !f.timezone}
          onClick={async () => {
            const ok = await create.run({
              name: f.name,
              countryCode,
              timezone: f.timezone.trim(),
              areas: f.areas.split(",").map((a) => a.trim()).filter(Boolean),
              lat: Number(f.lat) || 0,
              lng: Number(f.lng) || 0,
              foodTaxPercent: f.tax ? Number(f.tax) : undefined,
            });
            if (ok) onDone();
          }}
        >
          Add city
        </Button>
      }
    >
      <div className="space-y-3">
        <F label="City name">
          <input value={f.name} onChange={(e) => setF({ ...f, name: e.target.value })} className={field} />
        </F>
        <F label="Time zone (IANA)" hint="e.g. Australia/Sydney, Pacific/Auckland, America/Toronto">
          <input value={f.timezone} onChange={(e) => setF({ ...f, timezone: e.target.value })} className={field} />
        </F>
        <F label="Areas (comma separated)">
          <input value={f.areas} onChange={(e) => setF({ ...f, areas: e.target.value })} className={field} />
        </F>
        <div className="grid grid-cols-3 gap-2">
          <F label="Latitude">
            <input value={f.lat} onChange={(e) => setF({ ...f, lat: e.target.value })} inputMode="decimal" className={field} />
          </F>
          <F label="Longitude">
            <input value={f.lng} onChange={(e) => setF({ ...f, lng: e.target.value })} inputMode="decimal" className={field} />
          </F>
          <F label="Food tax override %" hint="Optional (provinces/states)">
            <input value={f.tax} onChange={(e) => setF({ ...f, tax: e.target.value })} inputMode="decimal" className={field} />
          </F>
        </div>
        {create.error && <p className="text-sm text-danger">{create.error.message}</p>}
      </div>
    </Sheet>
  );
}

function MarketEditor({ initial, cities, onSaved }: { initial: MarketInput; cities: City[]; onSaved: (m: Market) => void }) {
  const toast = useToast();
  const [m, setM] = useState<MarketInput>(initial);
  const [addingCity, setAddingCity] = useState(false);
  const providers = useQuery("paymentProviders", (api) => api.paymentProviders());
  const save = useMutation((api, x: MarketInput) => api.saveMarket(x));
  const set = (patch: Partial<MarketInput>) => setM({ ...m, ...patch });
  const dec = m.currencyDecimals;
  const provider = providers.data?.find((p) => p.id === m.payments.provider);
  const preview = useMemo(() => {
    try {
      return m.currency.length === 3 ? money(123456 * 10 ** Math.max(0, dec - 2), m.currency) : "—";
    } catch {
      return "—";
    }
  }, [m.currency, dec]);

  return (
    <div className="space-y-4">
      <Card className="grid gap-3 md:grid-cols-3">
        <F label="Country code (ISO)" hint="Fixed for this deployment">
          <input value={m.countryCode} disabled className={field} />
        </F>
        <F label="Country name">
          <input value={m.name} onChange={(e) => set({ name: e.target.value })} className={field} />
        </F>
        <F label="Status">
          <select value={m.status} onChange={(e) => set({ status: e.target.value as Market["status"] })} className={field}>
            <option value="COMING_SOON">Coming soon (hidden)</option>
            <option value="LIVE">Live</option>
          </select>
        </F>
        <F label="Currency (ISO 4217)" hint={`Set in the country file · e.g. ${preview}`}>
          <input value={m.currency} disabled className={field} />
        </F>
        <F label="Decimals">
          <input value={m.currencyDecimals} disabled className={field} />
        </F>
        <F label="Locale" hint="Number formatting, e.g. en-AU">
          <input value={m.locale} onChange={(e) => set({ locale: e.target.value })} className={field} />
        </F>
      </Card>

      <Card className="space-y-3">
        <h3 className="font-bold">Taxes</h3>
        <div className="grid gap-3 md:grid-cols-3">
          <F label="Tax name">
            <input value={m.taxName} onChange={(e) => set({ taxName: e.target.value })} className={field} />
          </F>
          <F label="Tax on restaurant bills %">
            <input value={m.foodTaxPercent} onChange={(e) => set({ foodTaxPercent: Number(e.target.value) || 0 })} inputMode="decimal" className={field} />
          </F>
          <F label="Tax on Feast subscriptions %">
            <input value={m.platformTaxPercent} onChange={(e) => set({ platformTaxPercent: Number(e.target.value) || 0 })} inputMode="decimal" className={field} />
          </F>
        </div>
        <Toggle checked={m.pricesIncludeTax} onChange={(v) => set({ pricesIncludeTax: v })} label="Menu prices include tax" description="On for AU/NZ/UK (bills show “includes GST/VAT”); off for IN/CA (tax added to the bill)." />
      </Card>

      <Card className="space-y-3">
        <h3 className="font-bold">Prices</h3>
        <div className="grid gap-3 md:grid-cols-3">
          {m.priceBands.map((b, i) => (
            <F key={i} label={`Band ${i + 1}→${i + 2} threshold (cost for two)`}>
              <MoneyInput value={b} decimals={dec} currency={m.currency} onChange={(v) => set({ priceBands: m.priceBands.map((x, j) => (j === i ? v : x)) as Market["priceBands"] })} />
            </F>
          ))}
          <F label="Cover charge per 2 guests (band 4)">
            <MoneyInput value={m.coverChargePerPair} decimals={dec} currency={m.currency} onChange={(v) => set({ coverChargePerPair: v })} />
          </F>
          {PLANS.map((p) => (
            <F key={p} label={`${p.toLowerCase()} plan (before tax)`}>
              <MoneyInput value={m.planPrices[p]} decimals={dec} currency={m.currency} onChange={(v) => set({ planPrices: { ...m.planPrices, [p]: v } })} />
            </F>
          ))}
          <F label="Tip buttons (comma separated, major units)">
            <input
              defaultValue={m.tipPresets.map((t) => t / 10 ** dec).join(", ")}
              onChange={(e) => set({ tipPresets: [0, ...e.target.value.split(",").map((x) => Math.round(Number(x.trim()) * 10 ** dec)).filter((x) => x > 0)] })}
              className={field}
            />
          </F>
        </div>
      </Card>

      <Card className="space-y-3">
        <h3 className="font-bold">Payments</h3>
        <div className="grid gap-3 md:grid-cols-2">
          <F label="Provider" hint={provider && !provider.configured ? "Add this provider's API keys in server config first." : "API keys live in server config, never here."}>
            <select value={m.payments.provider} onChange={(e) => set({ payments: { ...m.payments, provider: e.target.value } })} className={field}>
              {providers.data?.map((p) => (
                <option key={p.id} value={p.id} disabled={!p.configured}>
                  {p.name}
                  {p.configured ? "" : " (not configured)"}
                </option>
              ))}
            </select>
          </F>
          <F label="Methods offered">
            <div className="flex flex-wrap gap-2 pt-1">
              {ALL_METHODS.filter((x) => !provider || provider.methods.includes(x)).map((x) => (
                <Chip
                  key={x}
                  active={m.payments.methods.includes(x)}
                  onClick={() => set({ payments: { ...m.payments, methods: m.payments.methods.includes(x) ? m.payments.methods.filter((y) => y !== x) : [...m.payments.methods, x] } })}
                >
                  {x.replace("_", " ").toLowerCase()}
                </Chip>
              ))}
            </div>
          </F>
        </div>
      </Card>

      <Card className="grid gap-3 md:grid-cols-3">
        <h3 className="font-bold md:col-span-3">Phone sign-in</h3>
        <F label="Dial code">
          <input value={m.phone.dialCode} onChange={(e) => set({ phone: { ...m.phone, dialCode: e.target.value } })} className={field} />
        </F>
        <F label="National number pattern (regex)">
          <input value={m.phone.pattern} onChange={(e) => set({ phone: { ...m.phone, pattern: e.target.value } })} className={clsx(field, "font-mono")} />
        </F>
        <F label="Example">
          <input value={m.phone.example} onChange={(e) => set({ phone: { ...m.phone, example: e.target.value } })} className={field} />
        </F>
      </Card>

      <Card className="space-y-3">
        <h3 className="font-bold">Onboarding documents</h3>
        <p className="text-sm text-muted">What a restaurant must upload to list here. Required documents must be approved before it can go live.</p>
        <DocumentsEditor docs={m.documents} onChange={(documents) => set({ documents })} />
      </Card>

      {(
        <Card className="space-y-3">
          <div className="flex items-center justify-between">
            <h3 className="font-bold">Cities</h3>
            <Button size="sm" variant="secondary" onClick={() => setAddingCity(true)}>
              <Plus className="h-4 w-4" aria-hidden /> Add city
            </Button>
          </div>
          {cities.length === 0 ? (
            <p className="text-sm text-muted">No cities yet — add one before going live.</p>
          ) : (
            <ul className="grid gap-2 md:grid-cols-2">
              {cities.map((c) => (
                <li key={c.id} className="flex items-center gap-2 rounded-xl bg-canvas p-3 text-sm">
                  <MapPin className="h-4 w-4 text-brand" aria-hidden />
                  <span className="flex-1">
                    <b>{c.name}</b> · {c.timezone}
                    {c.foodTaxPercent !== undefined && ` · tax ${c.foodTaxPercent}%`}
                  </span>
                </li>
              ))}
            </ul>
          )}
          <CitySheet
            key={String(addingCity)}
            open={addingCity}
            countryCode={m.countryCode}
            onClose={() => setAddingCity(false)}
            onDone={() => {
              setAddingCity(false);
              toast("City added");
              onSaved(m as Market);
            }}
          />
        </Card>
      )}

      {save.error && <p className="text-sm text-danger">{save.error.message}</p>}
      <div className="sticky bottom-3 flex justify-end">
        <Button
          size="lg"
          loading={save.pending}
          onClick={async () => {
            const res = await save.run(m);
            if (res) {
              toast(`${res.name} saved`);
              onSaved(res);
            }
          }}
        >
          <Save className="h-4 w-4" aria-hidden /> Save {m.name || "market"}
        </Button>
      </div>
    </div>
  );
}

/**
 * This deployment's country. Every country runs its own deployment (same code),
 * selected with FEAST_COUNTRY; its defaults come from config/countries/<cc>.yaml.
 */
export default function AdminMarkets() {
  const markets = useQuery("adminMarkets", (api) => api.listMarkets());
  const cities = useQuery("adminCities", (api) => api.listCities());
  const { reload } = useMarkets();
  const [nonce, setNonce] = useState(0);
  if (markets.error) return <ErrorState error={markets.error} onRetry={markets.reload} />;
  if (!markets.data || !cities.data) return <Skeleton className="h-96 w-full" />;
  const current = markets.data[0];
  if (!current) return <ErrorState error={new Error("This deployment has no market configured.")} onRetry={markets.reload} />;

  const saved = () => {
    markets.reload();
    cities.reload();
    reload();
    setNonce((n) => n + 1);
  };

  return (
    <div className="space-y-5">
      <PageHeader
        title={`Country · ${current.name}`}
        subtitle="Currency, taxes, prices, payment provider, phone rules and onboarding documents for this country's deployment."
        actions={
          <span className={clsx("inline-flex items-center gap-1.5 rounded-full px-3 py-1 text-sm font-bold", current.status === "LIVE" ? "bg-green-50 text-veg" : "bg-stone-100 text-stone-700")}>
            <Globe2 className="h-4 w-4" aria-hidden /> {current.status === "LIVE" ? "Live" : "Coming soon"}
          </span>
        }
      />
      <MarketEditor key={`${current.countryCode}:${nonce}`} initial={current} cities={cities.data.filter((c) => c.countryCode === current.countryCode)} onSaved={saved} />
    </div>
  );
}
