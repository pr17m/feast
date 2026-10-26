"use client";

import { useMemo, useState } from "react";
import clsx from "clsx";
import { Flag, Phone } from "lucide-react";
import { useRouter } from "next/navigation";
import { ReportProblemSheet } from "@/components/trust";
import { PageHeader } from "@/components/portal-shell";
import { useToast } from "@/components/providers";
import { Button, Chip, Empty, ErrorState, Skeleton } from "@/components/ui";
import { useMutation, useQuery, useTick } from "@/lib/hooks";
import { addDays, dayLabel, longDate, time12, toISODate } from "@/lib/format";
import { STATUS_STYLE } from "@/lib/status";
import type { Booking } from "@/lib/api/types";
import { usePartner } from "../partner-context";

type Action = "SEATED" | "NO_SHOW" | "COMPLETED" | "CANCELLED";

export default function PartnerBookings() {
  const { restaurant, canEdit, isStaff, href } = usePartner();
  const toast = useToast();
  const router = useRouter();
  const [reporting, setReporting] = useState<Booking | null>(null);
  const tick = useTick(10000);
  const today = useMemo(() => new Date(), []);
  const days = useMemo(() => Array.from({ length: 9 }, (_, i) => toISODate(addDays(today, i - 1))), [today]);
  const [date, setDate] = useState(toISODate(today));
  const list = useQuery(`pBookings:${restaurant.id}:${date}:${tick}`, (api) => api.partnerBookings(restaurant.id, date));
  const update = useMutation((api, id: string, s: Action) => api.updateBookingStatus(id, s));

  const act = async (b: Booking, s: Action) => {
    const res = await update.run(b.id, s);
    if (res) {
      toast(`${b.contact.name}: ${s.replace("_", " ").toLowerCase()}`);
      list.reload();
    } else toast("Couldn't update this booking", "error");
  };

  const rows = list.data ?? [];
  const active = rows.filter((b) => b.status !== "CANCELLED");
  const covers = active.reduce((t, b) => t + b.guests, 0);

  const todayISO = toISODate(today);
  const actions = (b: Booking) => (
    <div className="flex flex-wrap gap-2">
      {canEdit && b.status === "CONFIRMED" && (
        <>
          <Button size="sm" onClick={() => act(b, "SEATED")}>
            Seat
          </Button>
          <Button size="sm" variant="secondary" onClick={() => act(b, "NO_SHOW")}>
            No-show
          </Button>
          <Button size="sm" variant="ghost" className="text-danger hover:bg-red-50" onClick={() => act(b, "CANCELLED")}>
            Cancel
          </Button>
        </>
      )}
      {canEdit && b.status === "SEATED" && (
        <Button size="sm" variant="secondary" onClick={() => act(b, "COMPLETED")}>
          Complete
        </Button>
      )}
      {!isStaff && b.date <= todayISO && b.status !== "CANCELLED" && (
        <Button size="sm" variant="ghost" onClick={() => setReporting(b)} aria-label={`Report ${b.contact.name} to Feast`}>
          <Flag className="h-4 w-4" aria-hidden /> Report
        </Button>
      )}
    </div>
  );

  return (
    <div className="space-y-5">
      <PageHeader title="Bookings" subtitle={`${longDate(date)} · ${active.length} bookings · ${covers} covers`} />

      <div className="no-scrollbar -mx-4 flex gap-2 overflow-x-auto px-4 py-1 md:mx-0 md:px-0">
        {days.map((d) => (
          <Chip key={d} active={d === date} onClick={() => setDate(d)}>
            {d === toISODate(addDays(today, -1)) ? "Yesterday" : dayLabel(d, today)} {Number(d.slice(8))}
          </Chip>
        ))}
      </div>

      {list.error && !list.data && <ErrorState error={list.error} onRetry={list.reload} />}
      {!list.data && !list.error && <Skeleton className="h-64 w-full" />}
      {list.data && rows.length === 0 && <Empty title="No bookings for this day" body="Bookings made on Feast show up here instantly." />}

      {rows.length > 0 && (
        <>
          {/* Desktop table */}
          <div className="hidden overflow-hidden rounded-2xl bg-surface ring-1 ring-line md:block">
            <table className="w-full text-left text-sm">
              <thead className="bg-canvas text-xs uppercase tracking-wide text-muted">
                <tr>
                  <th className="px-4 py-3">Time</th>
                  <th className="px-4 py-3">Guest</th>
                  <th className="px-4 py-3">Party</th>
                  <th className="px-4 py-3">Deal</th>
                  <th className="px-4 py-3">Status</th>
                  <th className="px-4 py-3">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-line">
                {rows.map((b) => (
                  <tr key={b.id} className={clsx(b.status === "CANCELLED" && "text-faint")}>
                    <td className="px-4 py-3 font-bold">{time12(b.time)}</td>
                    <td className="px-4 py-3">
                      <p className="font-semibold">{b.contact.name}</p>
                      <p className="text-xs text-muted">
                        +91 {b.contact.phone} · {b.code}
                      </p>
                      {b.specialRequest && <p className="mt-1 text-xs text-deal">“{b.specialRequest}”</p>}
                    </td>
                    <td className="px-4 py-3">{b.guests}</td>
                    <td className="px-4 py-3 text-deal">{b.deal?.title ?? "—"}</td>
                    <td className="px-4 py-3">
                      <span className={clsx("rounded-md px-2 py-0.5 text-xs font-bold", STATUS_STYLE[b.status])}>{b.status.replace("_", " ")}</span>
                      {b.checkedInAt && b.status === "CONFIRMED" && <span className="ml-1 rounded-md bg-green-50 px-2 py-0.5 text-xs font-bold text-veg">ARRIVED</span>}
                    </td>
                    <td className="px-4 py-3">{actions(b)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>

          {/* Mobile cards */}
          <ul className="space-y-3 md:hidden">
            {rows.map((b) => (
              <li key={b.id} className={clsx("rounded-2xl bg-surface p-4 ring-1 ring-line", b.status === "CANCELLED" && "opacity-60")}>
                <div className="mb-2 flex items-start justify-between gap-2">
                  <div>
                    <p className="text-lg font-black">{time12(b.time)}</p>
                    <p className="font-semibold">
                      {b.contact.name} · {b.guests} guests
                    </p>
                    <p className="text-xs text-muted">
                      {b.code}
                      {b.deal ? ` · ${b.deal.title}` : ""}
                    </p>
                  </div>
                  <span className="flex flex-col items-end gap-1">
                    <span className={clsx("rounded-md px-2 py-0.5 text-xs font-bold", STATUS_STYLE[b.status])}>{b.status.replace("_", " ")}</span>
                    {b.checkedInAt && b.status === "CONFIRMED" && <span className="rounded-md bg-green-50 px-2 py-0.5 text-xs font-bold text-veg">ARRIVED</span>}
                  </span>
                </div>
                {b.specialRequest && <p className="mb-2 text-sm text-deal">“{b.specialRequest}”</p>}
                <div className="flex items-center justify-between gap-2">
                  {actions(b)}
                  <a
                    href={`tel:+91${b.contact.phone}`}
                    className="grid h-9 w-9 place-items-center rounded-lg ring-1 ring-line"
                    aria-label={`Call ${b.contact.name}`}
                  >
                    <Phone className="h-4 w-4" />
                  </a>
                </div>
              </li>
            ))}
          </ul>
        </>
      )}
      {toISODate(today) !== date && (
        <p className="text-xs text-muted">
          {dayLabel(date, today)} · {longDate(date)}
        </p>
      )}
      {reporting && (
        <ReportProblemSheet
          open
          onClose={() => setReporting(null)}
          kind="REPORT"
          bookingId={reporting.id}
          subject={`${reporting.contact.name} · ${reporting.guests} guests · ${longDate(reporting.date)} ${time12(reporting.time)} · ${reporting.code}`}
          onOpened={(c) => {
            setReporting(null);
            toast("Sent to Feast");
            router.push(href(`/partner/cases/${c.id}`));
          }}
        />
      )}
    </div>
  );
}
