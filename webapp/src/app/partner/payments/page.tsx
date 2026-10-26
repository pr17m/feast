"use client";

import { useState } from "react";
import { Banknote, Check, CreditCard, X } from "lucide-react";
import { PageHeader, Stat } from "@/components/portal-shell";
import { OFFLINE_METHOD_LABEL } from "@/components/audit";
import { Button, Card, Empty, ErrorState, Sheet, Skeleton } from "@/components/ui";
import { useMutation, useQuery } from "@/lib/hooks";
import { money } from "@/lib/format";
import type { OfflinePayment } from "@/lib/api/types";
import { usePartner } from "../partner-context";

/** Counter payments diners say they've made: confirm once the money is in the till. */
function CounterQueue() {
  const { restaurant, canEdit } = usePartner();
  const q = useQuery(`counterPending:${restaurant.id}`, (api) => api.partnerOfflinePayments(restaurant.id, "PENDING_APPROVAL"));
  const approve = useMutation((api, id: string) => api.approveOfflinePayment(id));
  const reject = useMutation((api, id: string, reason: string) => api.rejectOfflinePayment(id, reason));
  const [rejecting, setRejecting] = useState<OfflinePayment | null>(null);
  const [reason, setReason] = useState("");
  const list = q.data ?? [];
  if (q.error) return <ErrorState error={q.error} onRetry={q.reload} />;
  if (list.length === 0) return null;
  return (
    <Card className="space-y-3 ring-2 ring-deal/40">
      <div>
        <h2 className="font-bold">Confirm counter payments ({list.length})</h2>
        <p className="text-sm text-muted">These diners are paying at your counter. Confirm once you have the money — Feast doesn&apos;t collect it. Unconfirmed payments lapse after 2 hours.</p>
      </div>
      <ul className="divide-y divide-line">
        {list.map((p) => (
          <li key={p.id} className="flex flex-wrap items-center gap-3 py-3">
            <span className="grid h-10 w-10 place-items-center rounded-xl bg-canvas text-muted ring-1 ring-line">
              {p.method === "CASH" ? <Banknote className="h-5 w-5" aria-hidden /> : <CreditCard className="h-5 w-5" aria-hidden />}
            </span>
            <span className="min-w-0 flex-1">
              <span className="block font-semibold">
                {p.dinerName} · {money(p.amount, p.currency)}
              </span>
              <span className="text-sm text-muted">
                {OFFLINE_METHOD_LABEL[p.method]}
                {p.savings > 0 ? ` · includes Feast deal −${money(p.savings, p.currency)}` : ""} · {new Date(p.submittedAt).toLocaleTimeString(undefined, { timeStyle: "short" })}
              </span>
            </span>
            {canEdit ? (
              <span className="flex gap-2">
                <Button
                  size="sm"
                  loading={approve.pending}
                  onClick={async () => {
                    if (await approve.run(p.id)) q.reload();
                  }}
                >
                  <Check className="h-4 w-4" aria-hidden /> Received {money(p.amount, p.currency)}
                </Button>
                <Button size="sm" variant="secondary" onClick={() => setRejecting(p)}>
                  <X className="h-4 w-4" aria-hidden /> Not paid
                </Button>
              </span>
            ) : (
              <span className="text-xs text-muted">Owner confirms</span>
            )}
          </li>
        ))}
      </ul>
      {approve.error && <p role="alert" className="text-sm text-danger">{approve.error.message}</p>}
      <Sheet open={!!rejecting} onClose={() => setRejecting(null)} title="Payment not received">
        <form
          className="space-y-3"
          onSubmit={async (e) => {
            e.preventDefault();
            if (!rejecting) return;
            if (await reject.run(rejecting.id, reason)) {
              setRejecting(null);
              setReason("");
              q.reload();
            }
          }}
        >
          <label className="block text-sm font-semibold" htmlFor="reject-reason">
            Tell {rejecting?.dinerName ?? "the diner"} why
          </label>
          <textarea
            id="reject-reason"
            required
            minLength={5}
            value={reason}
            onChange={(e) => setReason(e.target.value)}
            placeholder="e.g. We didn't receive the cash — please pay at the counter"
            className="h-24 w-full rounded-xl bg-canvas p-3 text-sm ring-1 ring-line focus:ring-brand"
          />
          {reject.error && <p role="alert" className="text-sm text-danger">{reject.error.message}</p>}
          <Button type="submit" className="w-full" loading={reject.pending} disabled={reason.trim().length < 5}>
            Mark as not paid
          </Button>
        </form>
      </Sheet>
    </Card>
  );
}

export default function PartnerPayments() {
  const { restaurant } = usePartner();
  const pays = useQuery(`pPays:${restaurant.id}`, (api) => api.partnerPayments(restaurant.id));
  const offline = (p: { payment: { settlement?: string } }) => p.payment.settlement === "OFFLINE";

  if (pays.error) return <ErrorState error={pays.error} onRetry={pays.reload} />;
  if (!pays.data) return <Skeleton className="h-96 w-full" />;
  const rows = pays.data;
  const total = rows.filter((p) => !offline(p)).reduce((t, p) => t + p.payment.amount, 0);
  const atCounter = rows.filter(offline).reduce((t, p) => t + p.payment.amount, 0);
  const savings = rows.reduce((t, p) => t + p.bill.savings, 0);
  const tips = rows.reduce((t, p) => t + p.bill.lines.filter((l) => l.kind === "TIP").reduce((x, l) => x + l.amount, 0), 0);

  return (
    <div className="space-y-5">
      <PageHeader
        title="Payments"
        subtitle="Bills paid through Feast are settled to your bank account T+1 (in production). Counter payments you confirmed are recorded here too; you already hold that money."
      />
      <CounterQueue />
      <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        <Stat label="Through Feast" value={money(total)} tone="brand" hint={atCounter ? `+ ${money(atCounter)} paid at your counter` : undefined} />
        <Stat label="Payments" value={rows.length} />
        <Stat label="Diner savings" value={money(savings)} hint="Discounts you funded" />
        <Stat label="Tips for staff" value={money(tips)} />
      </div>
      {rows.length === 0 && <Empty title="No payments yet" />}
      {rows.length > 0 && (
        <>
          <div className="hidden overflow-hidden rounded-2xl bg-surface ring-1 ring-line md:block">
            <table className="w-full text-left text-sm">
              <thead className="bg-canvas text-xs uppercase tracking-wide text-muted">
                <tr>
                  <th className="px-4 py-3">Paid at</th>
                  <th className="px-4 py-3">Booking</th>
                  <th className="px-4 py-3 text-right">Bill</th>
                  <th className="px-4 py-3 text-right">Discount</th>
                  <th className="px-4 py-3">Method</th>
                  <th className="px-4 py-3 text-right">Received</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-line">
                {rows.map((p) => (
                  <tr key={p.id}>
                    <td className="px-4 py-3">{new Date(p.payment.paidAt).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" })}</td>
                    <td className="px-4 py-3 text-muted">{p.bill.bookingId ? "Feast booking" : "—"}</td>
                    <td className="px-4 py-3 text-right">{money(p.bill.billAmount)}</td>
                    <td className="px-4 py-3 text-right text-veg">{p.bill.savings ? money(-p.bill.savings) : "—"}</td>
                    <td className="px-4 py-3">
                      {offline(p) ? <span className="rounded-full bg-canvas px-2 py-0.5 text-xs font-semibold ring-1 ring-line">At counter · {OFFLINE_METHOD_LABEL[p.payment.method as keyof typeof OFFLINE_METHOD_LABEL]}</span> : p.payment.method}
                    </td>
                    <td className="px-4 py-3 text-right font-bold">{money(p.payment.amount)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <ul className="space-y-2 md:hidden">
            {rows.map((p) => (
              <li key={p.id} className="flex items-center justify-between rounded-2xl bg-surface p-3 ring-1 ring-line">
                <span>
                  <span className="block text-sm font-semibold">{new Date(p.payment.paidAt).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" })}</span>
                  <span className="text-xs text-muted">
                    {offline(p) ? "At counter" : p.payment.method} · bill {money(p.bill.billAmount)}
                    {p.bill.savings ? ` · −${money(p.bill.savings)}` : ""}
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
