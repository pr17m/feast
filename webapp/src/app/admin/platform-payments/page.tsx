"use client";

import { useState } from "react";
import { PageHeader, Stat } from "@/components/portal-shell";
import { shortDate } from "@/components/subscription-badge";
import { Chip, Empty, ErrorState, Skeleton } from "@/components/ui";
import { useQuery } from "@/lib/hooks";
import { money } from "@/lib/format";

export default function AdminPlatformPayments() {
  const q = useQuery("platformPayments", (api) => api.platformPayments());
  const [currency, setCurrency] = useState<string | null>(null);
  if (q.error) return <ErrorState error={q.error} onRetry={q.reload} />;
  if (!q.data) return <Skeleton className="h-96 w-full" />;
  const currencies = [...new Set(q.data.map((p) => p.currency))];
  const cur = currency && currencies.includes(currency) ? currency : currencies[0];
  const rows = q.data.filter((p) => p.currency === cur);
  const paid = rows.filter((p) => p.status === "PAID" && p.planId !== "COMP");
  const month = new Date().toISOString().slice(0, 7);
  const sum = (xs: typeof rows, k: "amount" | "tax" | "total") => xs.reduce((t, p) => t + p[k], 0);

  return (
    <div className="space-y-5">
      <PageHeader title="Platform payments" subtitle="Subscription payments from restaurants to Feast, with tax. One currency at a time — amounts are never mixed." />
      {currencies.length > 1 && (
        <div className="flex gap-2">
          {currencies.map((c) => (
            <Chip key={c} active={c === cur} onClick={() => setCurrency(c)}>
              {c}
            </Chip>
          ))}
        </div>
      )}
      {cur && (
        <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
          <Stat label="Collected (incl. tax)" value={money(sum(paid, "total"), cur)} tone="brand" />
          <Stat label="Net revenue" value={money(sum(paid, "amount"), cur)} />
          <Stat label="Tax collected" value={money(sum(paid, "tax"), cur)} hint={paid[0]?.taxName} />
          <Stat label="This month" value={money(sum(paid.filter((p) => p.createdAt.startsWith(month)), "total"), cur)} />
        </div>
      )}
      {rows.length === 0 ? (
        <Empty title="No platform payments yet" />
      ) : (
        <div className="overflow-x-auto rounded-2xl bg-surface ring-1 ring-line">
          <table className="w-full min-w-[900px] text-left text-sm">
            <thead className="bg-canvas text-xs uppercase tracking-wide text-muted">
              <tr>
                <th className="px-4 py-3">Date</th>
                <th className="px-4 py-3">Invoice</th>
                <th className="px-4 py-3">Restaurant</th>
                <th className="px-4 py-3">Plan · period</th>
                <th className="px-4 py-3 text-right">Amount</th>
                <th className="px-4 py-3 text-right">Tax</th>
                <th className="px-4 py-3 text-right">Total</th>
                <th className="px-4 py-3">Method</th>
                <th className="px-4 py-3">Status</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-line">
              {rows.map((p) => (
                <tr key={p.id}>
                  <td className="whitespace-nowrap px-4 py-3">{shortDate(p.createdAt)}</td>
                  <td className="px-4 py-3 font-mono text-xs">{p.invoiceNumber}</td>
                  <td className="px-4 py-3 font-semibold">{p.restaurant.name}</td>
                  <td className="px-4 py-3">
                    {p.planId === "COMP" ? <span title={p.note}>Free days{p.note ? ` · ${p.note}` : ""}</span> : p.planId.toLowerCase()}
                    <span className="block text-xs text-muted">
                      {shortDate(p.periodStart)} – {shortDate(p.periodEnd)}
                    </span>
                  </td>
                  <td className="px-4 py-3 text-right">{money(p.amount, p.currency)}</td>
                  <td className="px-4 py-3 text-right">
                    {money(p.tax, p.currency)} <span className="text-xs text-muted">{p.taxName}</span>
                  </td>
                  <td className="px-4 py-3 text-right font-bold">{money(p.total, p.currency)}</td>
                  <td className="px-4 py-3">
                    {p.method ?? "—"}
                    {p.provider && <span className="block text-xs text-muted">{p.provider}</span>}
                  </td>
                  <td className="px-4 py-3">{p.status === "PAID" ? <span className="text-veg">Paid</span> : <span className="text-danger">Failed</span>}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
