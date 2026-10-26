"use client";

import { PageHeader, Stat } from "@/components/portal-shell";
import { Empty, ErrorState, Skeleton } from "@/components/ui";
import { useQuery } from "@/lib/hooks";
import { money } from "@/lib/format";

export default function AdminPayments() {
  const list = useQuery("adminPayments", (api) => api.adminPayments());
  if (list.error) return <ErrorState error={list.error} onRetry={list.reload} />;
  if (!list.data) return <Skeleton className="h-96 w-full" />;
  const rows = list.data;
  const gmv = rows.reduce((t, p) => t + p.payment.amount, 0);
  const savings = rows.reduce((t, p) => t + p.bill.savings, 0);
  const byMethod = rows.reduce<Record<string, number>>((m, p) => ({ ...m, [p.payment.method]: (m[p.payment.method] ?? 0) + 1 }), {});

  return (
    <div className="space-y-5">
      <PageHeader title="All payments" subtitle="Diner payments processed through Feast." />
      <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        <Stat label="GMV" value={money(gmv)} tone="brand" />
        <Stat label="Payments" value={rows.length} />
        <Stat label="Diner savings" value={money(savings)} />
        <Stat label="Top method" value={Object.entries(byMethod).sort((a, b) => b[1] - a[1])[0]?.[0] ?? "—"} />
      </div>
      {rows.length === 0 && <Empty title="No payments yet" />}
      {rows.length > 0 && (
        <>
          <div className="hidden overflow-x-auto rounded-2xl bg-surface ring-1 ring-line md:block">
            <table className="w-full text-left text-sm">
              <thead className="bg-canvas text-xs uppercase tracking-wide text-muted">
                <tr>
                  <th className="px-4 py-3">Paid at</th>
                  <th className="px-4 py-3">Restaurant</th>
                  <th className="px-4 py-3 text-right">Bill</th>
                  <th className="px-4 py-3 text-right">Discount</th>
                  <th className="px-4 py-3">Method</th>
                  <th className="px-4 py-3 text-right">Amount</th>
                  <th className="px-4 py-3">Reference</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-line">
                {rows.slice(0, 200).map((p) => (
                  <tr key={p.id}>
                    <td className="whitespace-nowrap px-4 py-3">{new Date(p.payment.paidAt).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" })}</td>
                    <td className="px-4 py-3 font-semibold">{p.restaurant.name}</td>
                    <td className="px-4 py-3 text-right">{money(p.bill.billAmount)}</td>
                    <td className="px-4 py-3 text-right text-veg">{p.bill.savings ? money(-p.bill.savings) : "—"}</td>
                    <td className="px-4 py-3">{p.payment.method}</td>
                    <td className="px-4 py-3 text-right font-bold">{money(p.payment.amount)}</td>
                    <td className="px-4 py-3 font-mono text-xs text-muted">{p.payment.intentId}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <ul className="space-y-2 md:hidden">
            {rows.slice(0, 100).map((p) => (
              <li key={p.id} className="flex items-center justify-between gap-3 rounded-2xl bg-surface p-3 ring-1 ring-line">
                <span className="min-w-0">
                  <span className="block truncate font-semibold">{p.restaurant.name}</span>
                  <span className="text-xs text-muted">
                    {new Date(p.payment.paidAt).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" })} · {p.payment.method}
                  </span>
                </span>
                <span className="font-bold">{money(p.payment.amount)}</span>
              </li>
            ))}
          </ul>
        </>
      )}
    </div>
  );
}
