"use client";

import { useState } from "react";
import { History, LifeBuoy, PauseCircle, PlayCircle } from "lucide-react";
import { PageHeader, Toggle } from "@/components/portal-shell";
import { useToast } from "@/components/providers";
import { Button, Card, Stepper } from "@/components/ui";
import { useMutation, useQuery } from "@/lib/hooks";
import { timeAgo } from "@/lib/format";
import type { RestaurantSettings } from "@/lib/api/types";
import { usePartner } from "../partner-context";

export default function PartnerSettings() {
  const { restaurant, reloadRestaurants, canEdit } = usePartner();
  const toast = useToast();
  const [s, setS] = useState<RestaurantSettings>(restaurant.settings);
  const save = useMutation((api, patch: Partial<RestaurantSettings>) => api.updateSettings(restaurant.id, patch));
  const status = useMutation((api, next: "LIVE" | "PAUSED") => api.setRestaurantStatus(restaurant.id, next));

  const change = async (patch: Partial<RestaurantSettings>) => {
    const prev = s;
    setS({ ...s, ...patch }); // optimistic
    const res = await save.run(patch);
    if (res) {
      setS(res);
      reloadRestaurants();
      toast("Saved");
    } else {
      setS(prev);
      toast("Couldn't save that change", "error");
    }
  };

  const live = restaurant.status === "LIVE";
  const canToggleStatus = restaurant.status === "LIVE" || restaurant.status === "PAUSED";

  return (
    <div className="max-w-2xl space-y-5">
      <PageHeader title="Settings" subtitle="Changes apply to diners immediately." />

      <Card>
        <h2 className="mb-1 font-bold">Availability on Feast</h2>
        <div className="flex items-center justify-between gap-4 py-2">
          <p className="text-sm text-muted">
            {live
              ? "You're live. Pause if you're closed unexpectedly or overwhelmed — your page stays visible but bookings, payments and table ordering stop."
              : restaurant.status === "PAUSED"
                ? "You're paused. Diners can see your page but can't book or pay."
                : `Status: ${restaurant.status.replace("_", " ").toLowerCase()}. Only Feast admins can change this.`}
          </p>
          {canToggleStatus && (
            <Button
              variant={live ? "secondary" : "primary"}
              loading={status.pending}
              onClick={async () => {
                if (await status.run(live ? "PAUSED" : "LIVE")) {
                  reloadRestaurants();
                  toast(live ? "Paused on Feast" : "You're live again");
                }
              }}
            >
              {live ? <PauseCircle className="h-4 w-4" aria-hidden /> : <PlayCircle className="h-4 w-4" aria-hidden />}
              {live ? "Pause" : "Go live"}
            </Button>
          )}
        </div>
      </Card>

      <fieldset disabled={!canEdit} className="contents">
        <Card className="divide-y divide-line">
          <h2 className="pb-2 font-bold">Services</h2>
          <Toggle
            checked={s.bookingsEnabled}
            onChange={(v) => change({ bookingsEnabled: v })}
            label="Table bookings"
            description="Diners can pre-book slots with your pre-booking deal."
          />
          <Toggle
            checked={s.billPaymentEnabled}
            onChange={(v) => change({ billPaymentEnabled: v })}
            label="Pay bill on Feast"
            description="Diners with a Feast booking pay their bill in the app (or at your counter) and get your Feast deal."
          />
          <div className="flex items-center justify-between gap-4 py-3">
            <span>
              <span className="block font-semibold">Tables per 30-minute slot</span>
              <span className="block text-sm text-muted">How many tables Feast can book in each slot. Set 0 to stop new bookings.</span>
            </span>
            <Stepper value={s.tablesPerSlot} min={0} max={50} onChange={(v) => change({ tablesPerSlot: v })} label="tables per slot" />
          </div>
        </Card>

      </fieldset>

      <SupportAccessCard />

    </div>
  );
}

/**
 * Restaurants own their content. The owner can let Feast support edit it for a
 * limited time (e.g. to set up the menu); every staff action is listed here.
 */
function SupportAccessCard() {
  const { restaurant, isStaff, reloadAccess } = usePartner();
  const toast = useToast();
  const q = useQuery(`supportAccess:${restaurant.id}`, (api) => api.supportAccess(restaurant.id));
  const set = useMutation((api, hours: number) => api.setSupportAccess(restaurant.id, hours));
  const run = async (hours: number) => {
    if (await set.run(hours)) {
      toast(hours ? `Feast support can edit for ${hours} hours` : "Support access turned off");
      q.reload();
      reloadAccess();
    }
  };
  const until = q.data?.activeUntil;
  return (
    <Card className="space-y-3">
      <h2 className="flex items-center gap-2 font-bold">
        <LifeBuoy className="h-5 w-5 text-brand" aria-hidden /> Feast support access
      </h2>
      <p className="text-sm text-muted">
        Your menu, listing and offers are yours — Feast staff can only view them. If you&apos;d like Feast to help (for example, setting up your menu), give
        support access for a limited time. Every change they make is recorded below.
      </p>
      {until ? (
        <p className="rounded-xl bg-amber-50 p-3 text-sm text-amber-900">Feast support can edit until {new Date(until).toLocaleString()}.</p>
      ) : (
        <p className="rounded-xl bg-canvas p-3 text-sm text-muted">Support access is off.</p>
      )}
      {!isStaff && (
        <div className="flex flex-wrap gap-2">
          {until ? (
            <Button variant="secondary" size="sm" loading={set.pending} onClick={() => run(0)}>
              Turn off now
            </Button>
          ) : (
            <>
              <Button variant="secondary" size="sm" loading={set.pending} onClick={() => run(24)}>
                Allow for 24 hours
              </Button>
              <Button variant="secondary" size="sm" loading={set.pending} onClick={() => run(72)}>
                Allow for 3 days
              </Button>
            </>
          )}
        </div>
      )}
      {set.error && (
        <p role="alert" className="text-sm text-danger">
          {set.error.message}
        </p>
      )}
      <div>
        <h3 className="mb-2 flex items-center gap-2 text-sm font-semibold">
          <History className="h-4 w-4" aria-hidden /> Activity by Feast
        </h3>
        {q.data && q.data.audit.length === 0 && <p className="text-sm text-muted">Nothing yet.</p>}
        <ul className="divide-y divide-line text-sm">
          {q.data?.audit.slice(0, 20).map((a) => (
            <li key={a.id} className="py-2">
              <p>{a.summary}</p>
              <p className="text-xs text-muted">
                {a.actorName} · {a.actorRole === "ADMIN" ? "Feast" : "Owner"} · {timeAgo(a.at)}
              </p>
            </li>
          ))}
        </ul>
      </div>
    </Card>
  );
}
