"use client";

import { useState } from "react";
import { CalendarPlus } from "lucide-react";
import { PageHeader } from "@/components/portal-shell";
import { useToast } from "@/components/providers";
import { SUB_STATUS, SubscriptionBadge, daysLabel, shortDate } from "@/components/subscription-badge";
import { Button, Chip, Empty, ErrorState, Sheet, Skeleton } from "@/components/ui";
import { useMutation, useQuery } from "@/lib/hooks";
import { money } from "@/lib/format";
import type { AdminSubscriptionRow, SubscriptionStatus } from "@/lib/api/types";

const ORDER: SubscriptionStatus[] = ["PAST_DUE", "EXPIRED", "TRIAL", "ACTIVE", "NOT_STARTED"];

function ExtendSheet({ row, onClose, onDone }: { row: AdminSubscriptionRow | null; onClose: () => void; onDone: () => void }) {
  const [days, setDays] = useState(14);
  const [note, setNote] = useState("");
  const extend = useMutation((api, id: string, d: number, n: string) => api.extendSubscription(id, d, n));
  return (
    <Sheet
      open={!!row}
      onClose={onClose}
      title={row ? `Free days for ${row.restaurant.name}` : ""}
      footer={
        <Button className="w-full" loading={extend.pending} onClick={async () => row && (await extend.run(row.restaurant.id, days, note)) && onDone()}>
          <CalendarPlus className="h-4 w-4" aria-hidden /> Add {days} days
        </Button>
      }
    >
      <p className="mb-3 text-sm text-muted">Extends the trial (if they never paid) or the paid period. Recorded as a zero-value entry in platform payments.</p>
      <div className="mb-3 flex gap-2">
        {[7, 14, 30, 60].map((d) => (
          <Chip key={d} active={days === d} onClick={() => setDays(d)}>
            {d} days
          </Chip>
        ))}
      </div>
      <input value={note} onChange={(e) => setNote(e.target.value)} placeholder="Reason (e.g. launch promo)" aria-label="Reason" className="h-11 w-full rounded-xl bg-canvas px-3 ring-1 ring-line outline-none focus:ring-2 focus:ring-brand" />
      {extend.error && <p className="mt-2 text-sm text-danger">{extend.error.message}</p>}
    </Sheet>
  );
}

export default function AdminSubscriptions() {
  const toast = useToast();
  const q = useQuery("adminSubscriptions", (api) => api.adminSubscriptions());
  const [filter, setFilter] = useState<SubscriptionStatus | "ALL">("ALL");
  const [country, setCountry] = useState<string>("ALL");
  const [extending, setExtending] = useState<AdminSubscriptionRow | null>(null);
  if (q.error) return <ErrorState error={q.error} onRetry={q.reload} />;
  if (!q.data) return <Skeleton className="h-96 w-full" />;
  const countries = [...new Set(q.data.map((r) => r.restaurant.countryCode))];
  const inCountry = q.data.filter((r) => country === "ALL" || r.restaurant.countryCode === country);
  const rows = inCountry
    .filter((r) => filter === "ALL" || r.subscription.status === filter)
    .sort((a, b) => ORDER.indexOf(a.subscription.status) - ORDER.indexOf(b.subscription.status) || (a.subscription.daysLeft ?? 0) - (b.subscription.daysLeft ?? 0));
  const count = (s: SubscriptionStatus) => inCountry.filter((r) => r.subscription.status === s).length;

  return (
    <div className="space-y-5">
      <PageHeader title="Subscriptions" subtitle="Every restaurant's listing subscription. Past-due listings are still visible during the grace period; expired ones are hidden." />
      <div className="flex flex-wrap gap-2">
        <Chip active={country === "ALL"} onClick={() => setCountry("ALL")}>
          All countries
        </Chip>
        {countries.map((c) => (
          <Chip key={c} active={country === c} onClick={() => setCountry(c)}>
            {c}
          </Chip>
        ))}
        <span className="mx-1 w-px bg-line" />
        <Chip active={filter === "ALL"} onClick={() => setFilter("ALL")}>
          All {inCountry.length}
        </Chip>
        {ORDER.map((s) => (
          <Chip key={s} active={filter === s} onClick={() => setFilter(s)}>
            {SUB_STATUS[s].label} {count(s)}
          </Chip>
        ))}
      </div>
      {rows.length === 0 ? (
        <Empty title="No restaurants here" />
      ) : (
        <div className="overflow-x-auto rounded-2xl bg-surface ring-1 ring-line">
          <table className="w-full min-w-[820px] text-left text-sm">
            <thead className="bg-canvas text-xs uppercase tracking-wide text-muted">
              <tr>
                <th className="px-4 py-3">Restaurant</th>
                <th className="px-4 py-3">Owner</th>
                <th className="px-4 py-3">Status</th>
                <th className="px-4 py-3">Plan</th>
                <th className="px-4 py-3">Ends</th>
                <th className="px-4 py-3 text-right">Total paid</th>
                <th className="px-4 py-3">Last payment</th>
                <th className="px-4 py-3" />
              </tr>
            </thead>
            <tbody className="divide-y divide-line">
              {rows.map((r) => (
                <tr key={r.restaurant.id}>
                  <td className="px-4 py-3">
                    <span className="block font-semibold">{r.restaurant.name}</span>
                    <span className="text-xs text-muted">
                      {r.restaurant.area} · {r.restaurant.countryCode}
                    </span>
                  </td>
                  <td className="px-4 py-3">{r.owners.map((o) => o.name).join(", ") || "—"}</td>
                  <td className="px-4 py-3">
                    <SubscriptionBadge s={r.subscription} />
                  </td>
                  <td className="px-4 py-3">{r.subscription.planId ? r.subscription.planId.toLowerCase() : r.subscription.status === "TRIAL" ? "trial" : "—"}</td>
                  <td className="whitespace-nowrap px-4 py-3">
                    {shortDate(r.subscription.endsAt)}
                    {r.subscription.endsAt && <span className="block text-xs text-muted">{daysLabel(r.subscription)}</span>}
                  </td>
                  <td className="px-4 py-3 text-right font-semibold">{money(r.totalPaid, r.restaurant.currency)}</td>
                  <td className="px-4 py-3">{shortDate(r.lastPaymentAt)}</td>
                  <td className="px-4 py-3 text-right">
                    {r.subscription.status !== "NOT_STARTED" && (
                      <Button size="sm" variant="secondary" onClick={() => setExtending(r)}>
                        Extend
                      </Button>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      <ExtendSheet
        key={extending?.restaurant.id ?? "none"}
        row={extending}
        onClose={() => setExtending(null)}
        onDone={() => {
          toast("Free days added");
          setExtending(null);
          q.reload();
        }}
      />
    </div>
  );
}
