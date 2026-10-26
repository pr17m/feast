"use client";

import { useState } from "react";
import { CalendarDays, CreditCard, Gauge, Lock, Plus, Trash2, BadgePercent } from "lucide-react";
import { PageHeader } from "@/components/portal-shell";
import { useAuth, useToast } from "@/components/providers";
import { Button, Card } from "@/components/ui";
import { useMutation, useQuery } from "@/lib/hooks";
import { money, currencySymbol, toMajor, toMinor } from "@/lib/format";
import type { Offer } from "@/lib/api/types";
import { usePartner } from "../partner-context";
import { ModerateButton, ModerationNotice } from "@/components/trust";

const field = "h-10 w-full rounded-lg bg-canvas px-3 text-sm ring-1 ring-line outline-none focus:ring-2 focus:ring-brand";

/** Keeps titles/subtitles in sync with the numbers so diners always see accurate copy. */
function describe(o: Offer): Offer {
  if (o.scope === "PREBOOK")
    return {
      ...o,
      title: `Flat ${o.percentOff ?? 0}% off`,
      subtitle: "on pre-booking a table",
    };
  if (o.scope !== "BILL") return o;
  if (o.flatOff)
    return {
      ...o,
      percentOff: undefined,
      maxDiscount: undefined,
      title: `Flat ${money(o.flatOff)} off`,
      subtitle: o.minBill ? `on bills above ${money(o.minBill)}` : "on your bill",
    };
  return {
    ...o,
    flatOff: undefined,
    title: `${o.percentOff ?? 0}% off`,
    subtitle: `on bill payment${o.maxDiscount ? `, up to ${money(o.maxDiscount)}` : ""}`,
  };
}

function Num({
  label,
  value,
  onChange,
  suffix,
  prefix,
}: {
  label: string;
  value: number | undefined;
  onChange: (v: number | undefined) => void;
  suffix?: string;
  prefix?: string;
}) {
  return (
    <label className="block">
      <span className="mb-1 block text-xs font-semibold text-muted">{label}</span>
      <span className="flex items-center gap-1">
        {prefix && <span className="text-sm text-muted">{prefix}</span>}
        <input
          inputMode="numeric"
          value={value ?? ""}
          onChange={(e) => {
            const v = e.target.value.replace(/\D/g, "");
            onChange(v === "" ? undefined : Number(v));
          }}
          className={field}
        />
        {suffix && <span className="text-sm text-muted">{suffix}</span>}
      </span>
    </label>
  );
}

/** Feast's moderation state of one offer: notice for the owner, hide/restore for staff. */
function OfferModeration({ o }: { o: Offer }) {
  const { restaurant, reloadRestaurants, isStaff } = usePartner();
  if (!o.id || o.id.startsWith("new_")) return null;
  return (
    <div className="mt-2 flex flex-wrap items-start gap-2">
      <div className="min-w-0 flex-1">
        {o.moderation && (
          <ModerationNotice m={o.moderation} restaurantId={restaurant.id} kind="OFFER" id={o.id} canAppeal={!isStaff} onDone={reloadRestaurants} />
        )}
      </div>
      {isStaff && <ModerateButton restaurantId={restaurant.id} kind="OFFER" id={o.id} title={o.title} hidden={!!o.moderation} onDone={reloadRestaurants} />}
    </div>
  );
}

/**
 * Restaurant-wide fairness controls: a per-bill cap on any Feast deal and a
 * daily limit on discounted bills, so the discount cost is predictable.
 * Saved with the restaurant settings, separately from the offers list.
 */
function DealBudgetCard() {
  const { restaurant, reloadRestaurants, canEdit } = usePartner();
  const toast = useToast();
  const budget = useQuery(`deal-budget:${restaurant.id}`, (api) => api.dealBudget(restaurant.id));
  const initialCap = restaurant.settings.maxDiscountPerBill;
  const initialLimit = restaurant.settings.dailyDealLimit;
  const [cap, setCap] = useState<number | undefined>(initialCap === undefined ? undefined : Number(toMajor(initialCap)));
  const [limit, setLimit] = useState<number | undefined>(initialLimit);
  const save = useMutation((api, c: number | undefined, l: number | undefined) =>
    api.updateSettings(restaurant.id, { maxDiscountPerBill: c === undefined ? null : toMinor(c), dailyDealLimit: l ?? null }),
  );
  const dirty = (cap === undefined ? undefined : toMinor(cap)) !== initialCap || limit !== initialLimit;
  const b = budget.data;

  return (
    <Card className="space-y-3">
      <h2 className="flex items-center gap-2 font-bold">
        <Gauge className="h-5 w-5 text-brand" aria-hidden /> Deal budget
      </h2>
      <p className="text-sm text-muted">
        Keep your discount cost predictable. Menu prices stay the same for every diner; Feast diners get the deal on their bill, within these limits. Leave a
        field empty for no limit.
      </p>
      <fieldset disabled={!canEdit} className="contents">
        <div className="flex flex-wrap items-end gap-4">
          <div className="w-48">
            <Num label="Most off any one bill" prefix={currencySymbol()} value={cap} onChange={setCap} />
          </div>
          <div className="w-48">
            <Num label="Discounted bills per day" value={limit} onChange={setLimit} suffix="bills" />
          </div>
          {canEdit && (
            <Button
              size="sm"
              disabled={!dirty}
              loading={save.pending}
              onClick={async () => {
                const res = await save.run(cap, limit);
                if (res) {
                  reloadRestaurants();
                  budget.reload();
                  toast("Deal budget saved");
                }
              }}
            >
              Save budget
            </Button>
          )}
        </div>
      </fieldset>
      {save.error && (
        <p role="alert" className="rounded-xl bg-red-50 p-3 text-sm text-danger">
          {save.error.message}
        </p>
      )}
      {b && (
        <p className="rounded-xl bg-canvas p-3 text-sm" aria-live="polite">
          {b.dailyDealLimit !== undefined ? (
            <>
              <b>
                {b.used} of {b.dailyDealLimit}
              </b>{" "}
              deals used today
              {b.remaining === 0 ? " — Feast diners can still pay here today, just without the deal." : "."}
            </>
          ) : (
            <>
              <b>{b.used}</b> {b.used === 1 ? "deal" : "deals"} given today (no daily limit).
            </>
          )}
          {b.maxDiscountPerBill !== undefined && <> No bill gets more than {money(b.maxDiscountPerBill)} off.</>}
        </p>
      )}
    </Card>
  );
}

export default function PartnerOffers() {
  const { restaurant } = usePartner();
  // Remount the editor when Feast's moderation of an offer changes, so it shows the latest state.
  const key = restaurant.offers.map((o) => `${o.id}:${o.moderation?.reviewRequestedAt ?? o.moderation?.hiddenAt ?? ""}`).join("|");
  return <OffersEditor key={key} />;
}

function OffersEditor() {
  const { restaurant, reloadRestaurants, canEdit } = usePartner();
  const { user } = useAuth();
  const toast = useToast();
  const [offers, setOffers] = useState<Offer[]>(restaurant.offers);
  const save = useMutation((api, list: Offer[]) => api.updateOffers(restaurant.id, list));
  const isAdmin = user?.role === "ADMIN";

  const prebook = offers.find((o) => o.scope === "PREBOOK");
  const bill = offers.filter((o) => o.scope === "BILL");
  const bank = offers.filter((o) => o.scope === "BANK");
  const update = (id: string, patch: Partial<Offer>) => setOffers((list) => list.map((o) => (o.id === id ? describe({ ...o, ...patch }) : o)));
  const dirty = JSON.stringify(offers) !== JSON.stringify(restaurant.offers);

  return (
    <div className="max-w-3xl space-y-5">
      <PageHeader
        title="Offers & deals"
        subtitle="Feast deals apply only to diners who booked on Feast and pay on Feast — don't apply them at your till."
        actions={
          canEdit && (
            <>
              <Button variant="secondary" disabled={!dirty || save.pending} onClick={() => setOffers(restaurant.offers)}>
                Discard
              </Button>
              <Button
                disabled={!dirty}
                loading={save.pending}
                onClick={async () => {
                  const res = await save.run(offers.map(describe));
                  if (res) {
                    setOffers(res);
                    reloadRestaurants();
                    toast("Offers saved");
                  }
                }}
              >
                Save changes
              </Button>
            </>
          )
        }
      />
      {save.error && (
        <p role="alert" className="rounded-xl bg-red-50 p-3 text-sm text-danger">
          {save.error.message}
        </p>
      )}

      <DealBudgetCard />

      <Card className="space-y-3">
        <h2 className="flex items-center gap-2 font-bold">
          <CalendarDays className="h-5 w-5 text-deal" aria-hidden /> Pre-booking deal
        </h2>
        <p className="text-sm text-muted">
          Base discount for diners who book a table. Feast adds +10% on your off-peak slots and takes −10% at peak hours automatically.
        </p>
        <fieldset disabled={!canEdit} className="contents">
          {prebook ? (
            <div className="flex flex-wrap items-end gap-4">
              <div className="w-40">
                <Num
                  label="Discount on total bill"
                  value={prebook.percentOff}
                  onChange={(v) => update(prebook.id, { percentOff: Math.min(60, v ?? 0) })}
                  suffix="%"
                />
              </div>
              <input
                type="range"
                min={0}
                max={50}
                step={5}
                value={prebook.percentOff ?? 0}
                onChange={(e) => update(prebook.id, { percentOff: Number(e.target.value) })}
                className="flex-1 accent-[var(--color-deal)]"
                aria-label="Pre-booking discount"
              />
              <Button variant="ghost" size="sm" onClick={() => setOffers((l) => l.filter((o) => o.id !== prebook.id))}>
                Remove
              </Button>
            </div>
          ) : (
            <Button
              variant="secondary"
              size="sm"
              onClick={() =>
                setOffers((l) => [
                  describe({
                    id: "",
                    scope: "PREBOOK",
                    title: "",
                    percentOff: 20,
                    terms: ["Valid on total bill for the booked slot"],
                  }),
                  ...l,
                ])
              }
            >
              <Plus className="h-4 w-4" aria-hidden /> Add pre-booking deal
            </Button>
          )}
        </fieldset>
        {prebook && <OfferModeration o={prebook} />}
      </Card>

      <Card className="space-y-3">
        <h2 className="flex items-center gap-2 font-bold">
          <BadgePercent className="h-5 w-5 text-brand" aria-hidden /> Bill payment offers
        </h2>
        <p className="text-sm text-muted">For diners who pay their bill on Feast. The best eligible offer is applied automatically.</p>
        <ul className="space-y-3">
          {bill.map((o) => (
            <li key={o.id} className="rounded-xl bg-canvas p-3 ring-1 ring-line">
              <fieldset disabled={!canEdit} className="contents">
                <div className="mb-2 flex items-center justify-between gap-2">
                  <p className="font-semibold">{o.title}</p>
                  <div className="flex items-center gap-1">
                    <select
                      value={o.flatOff ? "flat" : "percent"}
                      onChange={(e) =>
                        update(
                          o.id,
                          e.target.value === "flat"
                            ? {
                                flatOff: 15000,
                                minBill: 100000,
                                percentOff: undefined,
                                maxDiscount: undefined,
                              }
                            : {
                                percentOff: 10,
                                maxDiscount: 50000,
                                flatOff: undefined,
                                minBill: undefined,
                              },
                        )
                      }
                      className="h-8 rounded-lg bg-surface px-2 text-sm ring-1 ring-line"
                      aria-label="Offer type"
                    >
                      <option value="percent">Percentage</option>
                      <option value="flat">Flat amount</option>
                    </select>
                    <button
                      onClick={() => setOffers((l) => l.filter((x) => x.id !== o.id))}
                      className="grid h-8 w-8 place-items-center rounded-lg text-muted hover:bg-red-50 hover:text-danger"
                      aria-label={`Remove ${o.title}`}
                    >
                      <Trash2 className="h-4 w-4" />
                    </button>
                  </div>
                </div>
                <div className="grid grid-cols-2 gap-3">
                  {o.flatOff !== undefined ? (
                    <>
                      <Num
                        label="Flat discount"
                        prefix={currencySymbol()}
                        value={Number(toMajor(o.flatOff))}
                        onChange={(v) => update(o.id, { flatOff: toMinor(v ?? 0) })}
                      />
                      <Num
                        label="Minimum bill"
                        prefix={currencySymbol()}
                        value={o.minBill ? Number(toMajor(o.minBill)) : undefined}
                        onChange={(v) => update(o.id, { minBill: v ? toMinor(v) : undefined })}
                      />
                    </>
                  ) : (
                    <>
                      <Num label="Discount" suffix="%" value={o.percentOff} onChange={(v) => update(o.id, { percentOff: Math.min(60, v ?? 0) })} />
                      <Num
                        label="Maximum discount"
                        prefix={currencySymbol()}
                        value={o.maxDiscount ? Number(toMajor(o.maxDiscount)) : undefined}
                        onChange={(v) =>
                          update(o.id, {
                            maxDiscount: v ? toMinor(v) : undefined,
                          })
                        }
                      />
                    </>
                  )}
                </div>
              </fieldset>
              <OfferModeration o={o} />
            </li>
          ))}
        </ul>
        <Button
          variant="secondary"
          size="sm"
          disabled={!canEdit}
          onClick={() =>
            setOffers((l) => [
              ...l,
              describe({
                id: `new_${Date.now()}`,
                scope: "BILL",
                title: "",
                percentOff: 10,
                maxDiscount: 50000,
                terms: ["For diners who booked on Feast: applied when you pay your bill on Feast for your booking"],
              }),
            ])
          }
        >
          <Plus className="h-4 w-4" aria-hidden /> Add bill offer
        </Button>
      </Card>

      <Card className="space-y-2">
        <h2 className="flex items-center gap-2 font-bold">
          <CreditCard className="h-5 w-5 text-sky-700" aria-hidden /> Bank offers
          {!isAdmin && <Lock className="h-4 w-4 text-muted" aria-label="Managed by Feast" />}
        </h2>
        <p className="text-sm text-muted">
          Funded by Feast and partner banks
          {isAdmin ? "." : " — contact your account manager to change these."}
        </p>
        <ul className="space-y-1">
          {bank.map((o) => (
            <li key={o.id} className="text-sm">
              <b>{o.title}</b> <span className="text-muted">{o.subtitle}</span>
            </li>
          ))}
          {bank.length === 0 && <li className="text-sm text-muted">None right now.</li>}
        </ul>
      </Card>
    </div>
  );
}
