"use client";

import { useState } from "react";
import { PageHeader, Stat } from "@/components/portal-shell";
import { Button, Card, Empty, ErrorState, Skeleton } from "@/components/ui";
import { useMutation, useQuery } from "@/lib/hooks";
import { money } from "@/lib/format";
import { useToast } from "@/components/providers";
import { RefundRow } from "@/components/refunds";

const field = "h-10 w-full rounded-lg bg-canvas px-3 text-sm ring-1 ring-line outline-none focus:ring-2 focus:ring-brand";

/** Refund (part of) an online payment by hand: never more than what's left, always with a
 *  note (it is in the audit trail). Counter payments are refused: the restaurant refunds those. */
function Refunds() {
  const toast = useToast();
  const list = useQuery("adminRefunds", (api) => api.adminRefunds());
  const [intentId, setIntentId] = useState("");
  const [amount, setAmount] = useState("");
  const [note, setNote] = useState("");
  const refund = useMutation((api) =>
    api.adminRefund({ intentId: intentId.trim(), note: note.trim(), amount: amount.trim() ? Math.round(Number(amount) * 100) : undefined }),
  );
  const failed = (list.data ?? []).filter((r) => r.status === "FAILED").length;
  return (
    <Card className="space-y-4">
      <div>
        <h2 className="font-bold">Refunds</h2>
        <p className="text-sm text-muted">
          Cancellations, late or double payments and complaint decisions are refunded automatically. Refund by hand here; the amount can&apos;t exceed what&apos;s
          left of the payment.{failed > 0 && <span className="font-semibold text-danger"> {failed} failed — follow up with the diner.</span>}
        </p>
      </div>
      <form
        className="grid gap-2 md:grid-cols-[1.4fr_0.6fr_2fr_auto]"
        onSubmit={async (e) => {
          e.preventDefault();
          const r = await refund.run();
          if (r) {
            toast(`Refund of ${money(r.amount, r.currency)} ${r.status === "FAILED" ? "failed" : "started"}`);
            setIntentId("");
            setAmount("");
            setNote("");
            list.reload();
          }
        }}
      >
        <input className={field} placeholder="Payment reference (pi_…)" value={intentId} onChange={(e) => setIntentId(e.target.value)} aria-label="Payment reference" required />
        <input className={field} placeholder="Amount (all)" inputMode="decimal" value={amount} onChange={(e) => setAmount(e.target.value)} aria-label="Amount" />
        <input className={field} placeholder="Why (shown in the audit trail)" value={note} onChange={(e) => setNote(e.target.value)} aria-label="Note" minLength={10} required />
        <Button type="submit" loading={refund.pending}>
          Refund
        </Button>
      </form>
      {refund.error && <p role="alert" className="text-sm text-danger">{refund.error.message}</p>}
      <div className="space-y-3">
        {(list.data ?? []).slice(0, 20).map((r) => (
          <div key={r.id} className="border-t border-line pt-3">
            <RefundRow r={r} />
            <p className="font-mono text-xs text-muted">
              {r.intentId} · {r.purpose.toLowerCase()} {r.referenceId}
              {r.note ? ` · ${r.note}` : ""}
            </p>
          </div>
        ))}
        {list.data && list.data.length === 0 && <p className="text-sm text-muted">No refunds yet.</p>}
      </div>
    </Card>
  );
}

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
      <Refunds />
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
