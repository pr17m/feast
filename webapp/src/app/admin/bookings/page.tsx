"use client";

import { useState } from "react";
import clsx from "clsx";
import { Search } from "lucide-react";
import { PageHeader } from "@/components/portal-shell";
import { Chip, Empty, ErrorState, Skeleton } from "@/components/ui";
import { useQuery } from "@/lib/hooks";
import { longDate, time12 } from "@/lib/format";
import { STATUS_STYLE } from "@/lib/status";
import type { BookingStatus } from "@/lib/api/types";

const FILTERS: (BookingStatus | "ALL")[] = ["ALL", "CONFIRMED", "SEATED", "COMPLETED", "CANCELLED", "NO_SHOW"];

export default function AdminBookings() {
  const [status, setStatus] = useState<BookingStatus | "ALL">("ALL");
  const [q, setQ] = useState("");
  const list = useQuery("adminBookings", (api) => api.adminBookings());

  if (list.error) return <ErrorState error={list.error} onRetry={list.reload} />;
  if (!list.data) return <Skeleton className="h-96 w-full" />;
  const term = q.trim().toLowerCase();
  const rows = list.data.filter(
    (b) => (status === "ALL" || b.status === status) && (!term || `${b.restaurant.name} ${b.contact.name} ${b.code} ${b.contact.phone}`.toLowerCase().includes(term)),
  );

  return (
    <div className="space-y-5">
      <PageHeader title="All bookings" subtitle={`${list.data.length} bookings across every restaurant`} />
      <div className="flex flex-col gap-3 md:flex-row md:items-center">
        <label className="flex h-10 items-center gap-2 rounded-xl bg-surface px-3 ring-1 ring-line focus-within:ring-2 focus-within:ring-brand md:w-80">
          <Search className="h-4 w-4 text-muted" aria-hidden />
          <input value={q} onChange={(e) => setQ(e.target.value)} placeholder="Restaurant, guest, phone or code" className="w-full bg-transparent text-sm outline-none" aria-label="Search bookings" />
        </label>
        <div className="no-scrollbar -mx-4 flex gap-2 overflow-x-auto px-4 py-1 md:mx-0 md:px-0">
          {FILTERS.map((f) => (
            <Chip key={f} active={status === f} onClick={() => setStatus(f)}>
              {f === "ALL" ? "All" : f.replace("_", " ").toLowerCase()}
            </Chip>
          ))}
        </div>
      </div>
      {rows.length === 0 && <Empty title="No bookings match" />}
      {rows.length > 0 && (
        <>
          <div className="hidden overflow-x-auto rounded-2xl bg-surface ring-1 ring-line md:block">
            <table className="w-full text-left text-sm">
              <thead className="bg-canvas text-xs uppercase tracking-wide text-muted">
                <tr>
                  <th className="px-4 py-3">When</th>
                  <th className="px-4 py-3">Restaurant</th>
                  <th className="px-4 py-3">Guest</th>
                  <th className="px-4 py-3">Party</th>
                  <th className="px-4 py-3">Deal</th>
                  <th className="px-4 py-3">Status</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-line">
                {rows.slice(0, 200).map((b) => (
                  <tr key={b.id}>
                    <td className="whitespace-nowrap px-4 py-3">
                      {longDate(b.date)} · {time12(b.time)}
                    </td>
                    <td className="px-4 py-3 font-semibold">{b.restaurant.name}</td>
                    <td className="px-4 py-3">
                      {b.contact.name}
                      <span className="block text-xs text-muted">{b.code}</span>
                    </td>
                    <td className="px-4 py-3">{b.guests}</td>
                    <td className="px-4 py-3 text-deal">{b.deal?.title ?? "—"}</td>
                    <td className="px-4 py-3">
                      <span className={clsx("rounded-md px-2 py-0.5 text-xs font-bold", STATUS_STYLE[b.status])}>{b.status.replace("_", " ")}</span>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <ul className="space-y-2 md:hidden">
            {rows.slice(0, 100).map((b) => (
              <li key={b.id} className="rounded-2xl bg-surface p-3 ring-1 ring-line">
                <div className="flex items-start justify-between gap-2">
                  <p className="font-semibold">{b.restaurant.name}</p>
                  <span className={clsx("shrink-0 rounded-md px-2 py-0.5 text-xs font-bold", STATUS_STYLE[b.status])}>{b.status.replace("_", " ")}</span>
                </div>
                <p className="text-sm text-muted">
                  {longDate(b.date)} · {time12(b.time)} · {b.guests} guests · {b.contact.name}
                </p>
              </li>
            ))}
          </ul>
          {rows.length > 200 && <p className="text-xs text-muted">Showing the latest 200. Refine your search to see more.</p>}
        </>
      )}
    </div>
  );
}
